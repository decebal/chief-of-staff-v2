//! The dashboard server supervisor, against real child processes. The test
//! binary doubles as the fake server: `<self> serve <addr>`,
//! `<self> serve-with-member <addr>`, `<self> serve-handling-term <addr>`,
//! `<self> serve-handling-term-with-member <addr>`, `<self> serve-ignoring-term <addr>`,
//! `<self> idle-blocking-term <addr>` and `<self> idle <addr>` run a mode instead
//! of the tests, and `<self> leftover <mode> <addr>` starts a mode through an
//! intermediate process that exits, as an earlier app's server is left after a
//! crash. macOS only: elsewhere every spawn is refused, so `main` returns.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, SocketAddrV4, TcpListener, TcpStream};
use std::os::fd::AsRawFd;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use cos::business::{Dirs, Sources};
use cos::dashboards::{Dashboard, Dashboards, Probe, ProbeResult, Server};
use cos::proc;
use cos_desktop::Start;
use cos_desktop::servers::{
    ExitNotice, Ownership, Refusal, Running, SpawnResult, StoreError, Supervisor,
    binary_admissible, real_connect, real_spawn, spawn_result_admissible,
};
use serde_json::{Value, json};
use tauri::test::{mock_builder, mock_context, noop_assets};
use tauri::{Listener, Manager};

const READY: &str = r#""status":"ok""#;
const MARKER_A: &str = "<title>Fake dashboard A";
const MARKER_B: &str = r#"<script src="/fake-b.js""#;
/// The path `idle-blocking-term` waits for before it unblocks SIGTERM.
const PIDFILE_ENV: &str = "COS_FAKE_WAIT_FOR";

const TESTS: &[(&str, fn())] = &[
    ("new_creates_servers_dirs", new_creates_servers_dirs),
    ("contracts_serialise_by_kind", contracts_serialise_by_kind),
    ("setup_manages_the_supervisor", setup_manages_the_supervisor),
    ("setup_fails_when_new_fails", setup_fails_when_new_fails),
    ("exited_event_only_for_owned", exited_event_only_for_owned),
    (
        "real_connect_reports_a_listener",
        real_connect_reports_a_listener,
    ),
    ("markers_pass_the_parse_rules", markers_pass_the_parse_rules),
    ("serve_answers_each_marker", serve_answers_each_marker),
    (
        "serve_with_member_outlives_term",
        serve_with_member_outlives_term,
    ),
    ("idle_binds_nothing", idle_binds_nothing),
    ("leftover_is_not_our_child", leftover_is_not_our_child),
    ("identity_retry_rule", identity_retry_rule),
    ("unmanaged_not_running", unmanaged_not_running),
    (
        "waiter_on_reserved_gets_identity",
        waiter_on_reserved_gets_identity,
    ),
    (
        "waiter_on_reserved_embeds_external",
        waiter_on_reserved_embeds_external,
    ),
    (
        "pidfile_written_from_spawn_result",
        pidfile_written_from_spawn_result,
    ),
    ("pidfile_removal_is_gated", pidfile_removal_is_gated),
    (
        "binary_admissible_refuses_before_spawn",
        binary_admissible_refuses_before_spawn,
    ),
    ("held_port_lock_refuses", held_port_lock_refuses),
    (
        "answering_port_is_never_spawned_on",
        answering_port_is_never_spawned_on,
    ),
    (
        "concurrent_first_opens_spawn_once",
        concurrent_first_opens_spawn_once,
    ),
    (
        "readiness_timeout_stops_the_group",
        readiness_timeout_stops_the_group,
    ),
    ("open_during_epilogue_waits", open_during_epilogue_waits),
    ("child_exits_at_once", child_exits_at_once),
    (
        "leader_target_signals_the_leader_alone",
        leader_target_signals_the_leader_alone,
    ),
    (
        "spawn_result_admissible_names_the_field",
        spawn_result_admissible_names_the_field,
    ),
    (
        "foreign_listener_before_the_child_binds",
        foreign_listener_before_the_child_binds,
    ),
    (
        "spawned_page_without_the_marker_is_stopped",
        spawned_page_without_the_marker_is_stopped,
    ),
    (
        "spawned_page_with_the_marker_is_owned",
        spawned_page_with_the_marker_is_owned,
    ),
    ("owned_child_exit_is_reported", owned_child_exit_is_reported),
    (
        "server_log_truncated_before_spawn",
        server_log_truncated_before_spawn,
    ),
    ("port_lock_open_is_gated", port_lock_open_is_gated),
    ("port_lock_not_inherited", port_lock_not_inherited),
    ("port_lock_keeps_its_inode", port_lock_keeps_its_inode),
    (
        "second_supervisor_locks_after_the_reap",
        second_supervisor_locks_after_the_reap,
    ),
    (
        "control_room_then_salesroom_share_one_server",
        control_room_then_salesroom_share_one_server,
    ),
    (
        "owned_slot_refuses_a_marker_it_never_serves",
        owned_slot_refuses_a_marker_it_never_serves,
    ),
    (
        "success_wakes_a_waiter_with_its_own_probe",
        success_wakes_a_waiter_with_its_own_probe,
    ),
    (
        "reopen_after_owned_exit_waits_for_the_reaper",
        reopen_after_owned_exit_waits_for_the_reaper,
    ),
    (
        "reopen_after_owned_exit_refuses_at_the_deadline",
        reopen_after_owned_exit_refuses_at_the_deadline,
    ),
    ("stop_owned_frees_the_port", stop_owned_frees_the_port),
    (
        "quit_during_readiness_stops_the_group",
        quit_during_readiness_stops_the_group,
    ),
    (
        "quit_before_the_port_lock_is_taken",
        quit_before_the_port_lock_is_taken,
    ),
    (
        "open_after_quit_refuses_at_reserve",
        open_after_quit_refuses_at_reserve,
    ),
    (
        "open_on_owned_during_quit_refuses",
        open_on_owned_during_quit_refuses,
    ),
    (
        "quit_signals_owned_before_waiting_on_a_probe",
        quit_signals_owned_before_waiting_on_a_probe,
    ),
    (
        "quit_kills_a_member_and_spares_another_instance",
        quit_kills_a_member_and_spares_another_instance,
    ),
    (
        "quit_waits_for_a_spawn_to_be_recorded",
        quit_waits_for_a_spawn_to_be_recorded,
    ),
    (
        "spawn_recorded_after_quit_returned",
        spawn_recorded_after_quit_returned,
    ),
    ("quit_while_a_reaper_finishes", quit_while_a_reaper_finishes),
    (
        "quit_signals_a_leader_target_alone",
        quit_signals_a_leader_target_alone,
    ),
    ("quit_kills_a_stopped_server", quit_kills_a_stopped_server),
    (
        "cleanup_removes_an_undeclared_leftover",
        cleanup_removes_an_undeclared_leftover,
    ),
    ("cleanup_leaves_a_held_lock", cleanup_leaves_a_held_lock),
    (
        "forged_pidfile_is_overwritten_by_the_spawn",
        forged_pidfile_is_overwritten_by_the_spawn,
    ),
    (
        "cleanup_ignores_non_canonical_names",
        cleanup_ignores_non_canonical_names,
    ),
    ("cleanup_keeps_the_lock_inode", cleanup_keeps_the_lock_inode),
    (
        "cleanup_removes_a_record_for_another_port",
        cleanup_removes_a_record_for_another_port,
    ),
    ("cleanup_removes_a_dead_pid", cleanup_removes_a_dead_pid),
    (
        "cleanup_removes_a_mismatching_leftover",
        cleanup_removes_a_mismatching_leftover,
    ),
    (
        "cleanup_removes_a_leftover_of_another_binary",
        cleanup_removes_a_leftover_of_another_binary,
    ),
    (
        "cleanup_stops_a_matching_leftover",
        cleanup_stops_a_matching_leftover,
    ),
    (
        "cleanup_kills_a_leftover_member",
        cleanup_kills_a_leftover_member,
    ),
];

fn main() {
    if !cfg!(target_os = "macos") {
        return;
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    fake(&args);
    let filters: Vec<&str> = args
        .iter()
        .filter(|a| !a.starts_with('-'))
        .map(String::as_str)
        .collect();
    let mut failed = Vec::new();
    let mut ran = 0;
    for (name, test) in TESTS {
        if !filters.is_empty() && !filters.iter().any(|f| name.contains(f)) {
            continue;
        }
        ran += 1;
        println!("test {name} ...");
        match catch_unwind(test) {
            Ok(()) => println!("test {name} ... ok"),
            Err(_) => {
                println!("test {name} ... FAILED");
                failed.push(*name);
            }
        }
    }
    println!(
        "\ntest result: {}. {} passed; {} failed",
        if failed.is_empty() { "ok" } else { "FAILED" },
        ran - failed.len(),
        failed.len()
    );
    if !failed.is_empty() {
        println!("failures: {failed:?}");
        std::process::exit(1);
    }
}

/// Runs a fake-server mode when the arguments name one; otherwise returns.
fn fake(args: &[String]) {
    match args {
        [mode, addr] => {
            let Ok(addr) = addr.parse::<SocketAddrV4>() else {
                return;
            };
            match mode.as_str() {
                "serve" => serve(addr),
                "serve-with-member" => {
                    fork_member(true);
                    serve(addr)
                }
                "idle" => idle(),
                "idle-blocking-term" => {
                    if let Some(pidfile) = std::env::var_os(PIDFILE_ENV).map(PathBuf::from) {
                        while !pidfile.exists() {
                            std::thread::sleep(Duration::from_millis(10));
                        }
                    }
                    set_term_blocked(false);
                    idle()
                }
                "serve-handling-term" => {
                    catch_term();
                    serve(addr)
                }
                "serve-handling-term-with-member" => {
                    println!("member {}", fork_member(false));
                    catch_term();
                    serve(addr)
                }
                "serve-ignoring-term" => {
                    // SAFETY: setting a disposition to SIG_IGN touches no memory.
                    unsafe {
                        libc::signal(libc::SIGTERM, libc::SIG_IGN);
                    }
                    serve(addr)
                }
                _ => {}
            }
        }
        [leftover, mode, addr] if leftover == "leftover" => detach(mode, addr, true),
        [leftover, mode, addr] if leftover == "leftover-member" => detach(mode, addr, false),
        _ => {}
    }
}

fn serve(addr: SocketAddrV4) -> ! {
    let listener = TcpListener::bind(addr).expect("bind the fake server");
    for mut s in listener.incoming().flatten() {
        let _ = answer(&mut s);
    }
    unreachable!("incoming() never ends")
}

fn answer(s: &mut TcpStream) -> io::Result<()> {
    s.set_read_timeout(Some(Duration::from_secs(2)))?;
    let mut req = Vec::new();
    let mut chunk = [0u8; 1024];
    while !req.windows(4).any(|w| w == b"\r\n\r\n") && req.len() < 16 * 1024 {
        let n = s.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        req.extend_from_slice(&chunk[..n]);
    }
    let req = String::from_utf8_lossy(&req);
    // Under `real_spawn` stdout is the server log, so a test can count the requests.
    println!("{}", req.lines().next().unwrap_or(""));
    let (status, body) = match req.split_whitespace().nth(1).unwrap_or("") {
        "/health" => ("200 OK", format!("{{{READY}}}")),
        "/" => (
            "200 OK",
            format!("<html><head>{MARKER_A}</title></head></html>"),
        ),
        "/sales" => ("200 OK", format!("<html>{MARKER_B}></script></html>")),
        _ => ("404 Not Found", "not found".to_owned()),
    };
    write!(
        s,
        "HTTP/1.0 {status}\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )?;
    s.flush()
}

/// Forks one process that stays in this process's group, ignoring SIGTERM when
/// `ignore_term`, and returns its pid. It runs before `bind`, so it holds no listener.
fn fork_member(ignore_term: bool) -> libc::pid_t {
    // SAFETY: the mode runs before any thread is started, so the child of `fork` is a
    // single-threaded copy; it only changes a signal disposition and sleeps.
    let pid = unsafe { libc::fork() };
    if pid == 0 {
        if ignore_term {
            // SAFETY: setting a disposition to SIG_IGN touches no memory.
            unsafe {
                libc::signal(libc::SIGTERM, libc::SIG_IGN);
            }
        }
        idle();
    }
    pid
}

fn catch_term() {
    // SAFETY: `exit_on_term` only calls `_exit`, which is async-signal-safe.
    unsafe {
        libc::signal(
            libc::SIGTERM,
            exit_on_term as *const () as libc::sighandler_t,
        );
    }
}

/// Blocks or unblocks SIGTERM for the calling thread. A blocked SIGTERM stays pending
/// and the mask survives exec, so a child blocked between fork and exec dies only
/// once it unblocks.
fn set_term_blocked(blocked: bool) {
    // SAFETY: the set is initialised by `sigemptyset` before use; `sigprocmask` only
    // changes this thread's mask and is async-signal-safe.
    unsafe {
        let mut set: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut set);
        libc::sigaddset(&mut set, libc::SIGTERM);
        let how = if blocked {
            libc::SIG_BLOCK
        } else {
            libc::SIG_UNBLOCK
        };
        libc::sigprocmask(how, &set, std::ptr::null_mut());
    }
}

/// A caught SIGTERM, unlike a default one, stays pending while the process is stopped:
/// Darwin ends a stopped process on a signal whose default action terminates it.
extern "C" fn exit_on_term(_: libc::c_int) {
    // SAFETY: `_exit` is async-signal-safe and touches no Rust state.
    unsafe { libc::_exit(0) }
}

fn idle() -> ! {
    loop {
        std::thread::sleep(Duration::from_secs(3600));
    }
}

/// The intermediate of a leftover: starts `<self> <mode> <addr>` as the leader of its own
/// group, prints its pid and exits without waiting, so the leader is reparented away.
/// Without `own_group` the child stays in the intermediate's group.
fn detach(mode: &str, addr: &str, own_group: bool) -> ! {
    let mut cmd = Command::new(std::env::current_exe().unwrap());
    if own_group {
        cmd.process_group(0);
    }
    let child = cmd
        .args([mode, addr])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn the leftover");
    println!("{}", child.id());
    std::process::exit(0)
}

/// A fresh directory for one test, named by the test.
fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("cos-servers-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d.canonicalize().unwrap()
}

fn free_addr() -> SocketAddrV4 {
    let l = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    SocketAddrV4::new(Ipv4Addr::LOCALHOST, l.local_addr().unwrap().port())
}

fn until(timeout: Duration, mut done: impl FnMut() -> bool) -> bool {
    let end = Instant::now() + timeout;
    loop {
        if done() {
            return true;
        }
        if Instant::now() >= end {
            return false;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn get(addr: SocketAddrV4, path: &str, contains: &str) -> ProbeResult {
    let probe = Probe {
        path: path.to_owned(),
        contains: contains.to_owned(),
    };
    cos::dashboards::probe(addr, &probe, Duration::from_secs(3))
}

fn matched(r: &ProbeResult) -> bool {
    matches!(r, ProbeResult::Response { matched: true, .. })
}

/// A fake server this test spawned as a group leader; dropping it kills the group and
/// reaps the leader.
struct Kid(Child);

impl Kid {
    fn pid(&self) -> libc::pid_t {
        self.0.id() as libc::pid_t
    }
}

impl Drop for Kid {
    fn drop(&mut self) {
        proc::kill_group(self.pid());
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn spawn_mode(mode: &str, addr: SocketAddrV4) -> Kid {
    let child = Command::new(std::env::current_exe().unwrap())
        .args([mode, &addr.to_string()])
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    Kid(child)
}

/// A leftover's group; it is not this process's child, so dropping it kills the group
/// and launchd reaps the leader.
struct Leftover(libc::pid_t);

impl Drop for Leftover {
    fn drop(&mut self) {
        proc::kill_group(self.0);
    }
}

fn leftover(mode: &str, addr: SocketAddrV4) -> Leftover {
    let out = Command::new(std::env::current_exe().unwrap())
        .args(["leftover", mode, &addr.to_string()])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(out.status.success(), "leftover intermediate: {out:?}");
    Leftover(
        String::from_utf8(out.stdout)
            .unwrap()
            .trim()
            .parse()
            .unwrap(),
    )
}

/// A leftover that is a member of a group it does not lead: the intermediate leads its
/// own group, and the leftover stays in it after the intermediate exits.
struct Member(libc::pid_t);

impl Drop for Member {
    fn drop(&mut self) {
        proc::signal_pid(self.0, libc::SIGKILL);
    }
}

fn leftover_member(mode: &str, addr: SocketAddrV4) -> Member {
    let out = Command::new(std::env::current_exe().unwrap())
        .args(["leftover-member", mode, &addr.to_string()])
        .process_group(0)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(out.status.success(), "leftover intermediate: {out:?}");
    Member(
        String::from_utf8(out.stdout)
            .unwrap()
            .trim()
            .parse()
            .unwrap(),
    )
}

fn supervisor(data: &Path, logs: &Path) -> io::Result<Supervisor> {
    Supervisor::new(
        data,
        logs,
        |_: ExitNotice| {},
        |_: &Server, _: &Path| -> anyhow::Result<Child> { anyhow::bail!("no spawn in this case") },
        |_: SocketAddrV4| false,
    )
}

fn start(root: &Path) -> Start {
    let install = root.join("install");
    std::fs::create_dir_all(&install).unwrap();
    Start {
        install,
        user_home: Some(root.join("user")),
        registry: root.join("businesses.toml"),
        settings_file: root.join("settings.json"),
        data_dir: root.join("data"),
        log_dir: root.join("logs"),
        ui_state_file: root.join("ui-state.json"),
        dashboards_file: None,
        sources: Sources {
            business: None,
            dirs: Dirs {
                home: None,
                workspace: None,
                projects: None,
            },
        },
        cos_bin: None,
        slots: None,
    }
}

fn notice(status: &str, owned: bool) -> ExitNotice {
    ExitNotice {
        addr: "127.0.0.1:4777".parse().unwrap(),
        status: status.to_owned(),
        log_tail: "last line\n".to_owned(),
        owned,
    }
}

fn new_creates_servers_dirs() {
    let root = scratch("new");
    let (data, logs) = (root.join("data"), root.join("logs"));
    supervisor(&data, &logs).expect("Supervisor::new on a fresh tempdir");
    assert!(data.join("servers").is_dir(), "data servers/ missing");
    assert!(logs.join("servers").is_dir(), "log servers/ missing");

    let file = root.join("file");
    std::fs::write(&file, "not a directory").unwrap();
    assert!(supervisor(&file, &logs).is_err(), "data dir is a file");
    assert!(supervisor(&data, &file).is_err(), "log dir is a file");
}

fn kind(r: &Refusal) -> &'static str {
    match r {
        Refusal::Identity { .. } => "Identity",
        Refusal::BinaryMissing { .. } => "BinaryMissing",
        Refusal::NotAdmissible { .. } => "NotAdmissible",
        Refusal::FailedToStart { .. } => "FailedToStart",
        Refusal::NotRunning { .. } => "NotRunning",
        Refusal::StaleBusiness { .. } => "StaleBusiness",
        Refusal::UnknownDashboard { .. } => "UnknownDashboard",
        Refusal::InvalidConfig { .. } => "InvalidConfig",
        Refusal::Internal { .. } => "Internal",
    }
}

fn contracts_serialise_by_kind() {
    let cases = [
        (
            Refusal::Identity {
                status: "HTTP/1.1 200 OK".into(),
                head: "<html>".into(),
                owned: false,
                spawned: true,
            },
            json!({"kind": "Identity", "status": "HTTP/1.1 200 OK", "head": "<html>",
                   "owned": false, "spawned": true}),
        ),
        (
            Refusal::BinaryMissing {
                path: "/opt/cr/bin".into(),
            },
            json!({"kind": "BinaryMissing", "path": "/opt/cr/bin"}),
        ),
        (
            Refusal::NotAdmissible {
                rule: "platform".into(),
                path: "/opt/cr/bin".into(),
            },
            json!({"kind": "NotAdmissible", "rule": "platform", "path": "/opt/cr/bin"}),
        ),
        (
            Refusal::FailedToStart {
                log_tail: "bind: address in use\n".into(),
            },
            json!({"kind": "FailedToStart", "log_tail": "bind: address in use\n"}),
        ),
        (
            Refusal::NotRunning {
                hint: "the app is quitting".into(),
            },
            json!({"kind": "NotRunning", "hint": "the app is quitting"}),
        ),
        (
            Refusal::StaleBusiness {
                message: "stale".into(),
            },
            json!({"kind": "StaleBusiness", "message": "stale"}),
        ),
        (
            Refusal::UnknownDashboard {
                name: "Salesroom".into(),
            },
            json!({"kind": "UnknownDashboard", "name": "Salesroom"}),
        ),
        (
            Refusal::InvalidConfig {
                message: "business.x[0].url: bad".into(),
            },
            json!({"kind": "InvalidConfig", "message": "business.x[0].url: bad"}),
        ),
        (
            Refusal::Internal {
                message: "boom".into(),
            },
            json!({"kind": "Internal", "message": "boom"}),
        ),
    ];
    let mut kinds = BTreeSet::new();
    for (refusal, want) in &cases {
        assert_eq!(serde_json::to_value(refusal).unwrap(), *want);
        kinds.insert(kind(refusal));
    }
    assert_eq!(kinds.len(), 9, "every Refusal kind has a case");

    let running = Running {
        url: "http://127.0.0.1:4777/".into(),
        owned: true,
    };
    assert_eq!(
        serde_json::to_value(&running).unwrap(),
        json!({"url": "http://127.0.0.1:4777/", "owned": true})
    );
}

fn setup_manages_the_supervisor() {
    let root = scratch("setup-ok");
    let s = start(&root);
    let (data, logs) = (s.data_dir.clone(), s.log_dir.clone());
    let mut app = cos_desktop::builder(mock_builder(), Some(s))
        .build(mock_context(noop_assets()))
        .unwrap();
    // `build` does not run `setup`; on the mock runtime one iteration runs it.
    #[allow(deprecated)]
    app.run_iteration(|_, _| {});
    assert!(
        app.try_state::<Supervisor>().is_some(),
        "supervisor managed"
    );
    assert!(data.join("servers").is_dir());
    assert!(logs.join("servers").is_dir());
}

fn setup_fails_when_new_fails() {
    let root = scratch("setup-err");
    let s = start(&root);
    std::fs::write(&s.data_dir, "not a directory").unwrap();
    let mut app = cos_desktop::builder(mock_builder(), Some(s))
        .build(mock_context(noop_assets()))
        .unwrap();
    // The mock runtime panics with the error `setup` returned.
    #[allow(deprecated)]
    let r = catch_unwind(AssertUnwindSafe(|| app.run_iteration(|_, _| {})));
    let msg = r.expect_err("setup must fail");
    let msg = msg.downcast_ref::<String>().cloned().unwrap_or_default();
    assert!(msg.contains("Failed to setup app"), "{msg}");
    assert!(msg.contains("servers"), "{msg}");
    std::mem::forget(app);
}

fn exited_event_only_for_owned() {
    let app = mock_builder().build(mock_context(noop_assets())).unwrap();
    let (tx, rx) = mpsc::channel();
    app.listen_any("dashboard-exited", move |e| {
        let _ = tx.send(e.payload().to_owned());
    });
    cos_desktop::emit_exited(app.handle(), &notice("exit 1", false));
    cos_desktop::emit_exited(app.handle(), &notice("signal 9", true));
    let got = rx.recv_timeout(Duration::from_secs(2)).expect("one event");
    assert_eq!(
        serde_json::from_str::<Value>(&got).unwrap(),
        json!({"addr": "127.0.0.1:4777", "status": "signal 9", "log_tail": "last line\n"})
    );
    assert!(
        rx.recv_timeout(Duration::from_millis(300)).is_err(),
        "owned: false emitted"
    );
}

fn real_connect_reports_a_listener() {
    let l = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let addr = SocketAddrV4::new(Ipv4Addr::LOCALHOST, l.local_addr().unwrap().port());
    assert!(real_connect(addr));
    drop(l);
    assert!(!real_connect(addr));
}

fn markers_pass_the_parse_rules() {
    let root = scratch("markers");
    let addr = free_addr();
    let file = root.join("dashboards.toml");
    let toml = format!(
        r#"[server.fake]
binary = {bin:?}
addr = "{addr}"
ready = {{ path = "/health", contains = {READY:?} }}

[[business.test]]
name = "A"
url = "http://{addr}/"
server = "fake"
probe = {{ path = "/", contains = {MARKER_A:?} }}

[[business.test]]
name = "B"
url = "http://{addr}/sales"
server = "fake"
probe = {{ path = "/sales", contains = {MARKER_B:?} }}
"#,
        bin = std::env::current_exe().unwrap().display().to_string(),
    );
    std::fs::write(&file, toml).unwrap();
    let d = cos::dashboards::load(&file).expect("the fake server's markers parse");
    assert_eq!(d.for_business("test").len(), 2);
}

fn serve_answers_each_marker() {
    let addr = free_addr();
    let _kid = spawn_mode("serve", addr);
    assert!(
        until(Duration::from_secs(5), || matched(&get(
            addr, "/health", READY
        ))),
        "serve never became ready"
    );
    assert!(matched(&get(addr, "/", MARKER_A)));
    assert!(matched(&get(addr, "/sales", MARKER_B)));
    assert!(!matched(&get(addr, "/", MARKER_B)));
}

fn serve_with_member_outlives_term() {
    let addr = free_addr();
    let mut kid = spawn_mode("serve-with-member", addr);
    let pid = kid.pid();
    assert!(
        until(Duration::from_secs(5), || matched(&get(
            addr, "/health", READY
        ))),
        "serve-with-member never became ready"
    );
    proc::signal_group(pid, libc::SIGTERM);
    kid.0.wait().unwrap();
    assert!(proc::group_alive(pid), "the member exited on SIGTERM");
    assert_eq!(get(addr, "/health", READY), ProbeResult::NoListener);
    proc::kill_group(pid);
    assert!(
        until(Duration::from_secs(2), || !proc::group_alive(pid)),
        "the member survived SIGKILL"
    );
}

fn idle_binds_nothing() {
    let addr = free_addr();
    let mut kid = spawn_mode("idle", addr);
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(get(addr, "/", MARKER_A), ProbeResult::NoListener);
    assert!(kid.0.try_wait().unwrap().is_none(), "idle exited by itself");
    proc::signal_pid(kid.pid(), libc::SIGTERM);
    let status = kid.0.wait().unwrap();
    assert!(!status.success());
}

fn leftover_is_not_our_child() {
    let addr = free_addr();
    let left = leftover("idle", addr);
    let pid = left.0;
    assert!(proc::alive(pid));
    assert_eq!(proc::pgid(pid), Some(pid), "a leftover leads its own group");
    // SAFETY: WNOHANG never blocks; a pid that is not our child gives ECHILD.
    let r = unsafe { libc::waitpid(pid, std::ptr::null_mut(), libc::WNOHANG) };
    assert_eq!(r, -1, "the leftover is still this process's child");
    assert!(proc::start_time(pid).is_some());
    let exe = proc::exe(pid).unwrap().canonicalize().unwrap();
    assert_eq!(
        exe,
        std::env::current_exe().unwrap().canonicalize().unwrap()
    );
    proc::kill_group(pid);
    assert!(
        until(Duration::from_secs(2), || !proc::alive(pid)),
        "the leftover survived SIGKILL"
    );
}

/// A supervisor whose spawn records each call and starts nothing.
fn counting(
    root: &Path,
    connect: impl Fn(SocketAddrV4) -> bool + Send + Sync + 'static,
) -> (Supervisor, Arc<AtomicUsize>) {
    let spawns = Arc::new(AtomicUsize::new(0));
    let counted = spawns.clone();
    let sup = Supervisor::new(
        &root.join("data"),
        &root.join("logs"),
        |_: ExitNotice| {},
        move |_: &Server, _: &Path| -> anyhow::Result<Child> {
            counted.fetch_add(1, Ordering::SeqCst);
            anyhow::bail!("no spawn in this case")
        },
        connect,
    )
    .unwrap();
    (sup, spawns)
}

fn unmanaged(addr: SocketAddrV4) -> Dashboard {
    Dashboard {
        name: "Gazette".into(),
        url: format!("http://{addr}/"),
        port: addr.port(),
        server: None,
        probe: Probe {
            path: "/".into(),
            contains: MARKER_A.into(),
        },
    }
}

/// What a test listener does with an accepted connection.
#[derive(Clone, Copy)]
enum Act {
    Close,
    Reply(&'static str),
    Silent,
}

/// A loopback listener that counts accepted connections and acts on the `n`th (from 0)
/// by `act(n)`.
struct Fixture {
    addr: SocketAddrV4,
    accepts: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Fixture {
    fn new(act: fn(usize) -> Act) -> Self {
        let l = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let addr = SocketAddrV4::new(Ipv4Addr::LOCALHOST, l.local_addr().unwrap().port());
        l.set_nonblocking(true).unwrap();
        let accepts = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let (n, halt) = (accepts.clone(), stop.clone());
        let thread = std::thread::spawn(move || {
            let mut held = Vec::new();
            while !halt.load(Ordering::SeqCst) {
                match l.accept() {
                    Ok((mut s, _)) => {
                        s.set_nonblocking(false).unwrap();
                        match act(n.fetch_add(1, Ordering::SeqCst)) {
                            Act::Close => drop(s),
                            Act::Reply(body) => {
                                let _ = reply(&mut s, body);
                            }
                            Act::Silent => held.push(s),
                        }
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(10)),
                }
            }
        });
        Self {
            addr,
            accepts,
            stop,
            thread: Some(thread),
        }
    }

    fn accepts(&self) -> usize {
        self.accepts.load(Ordering::SeqCst)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn reply(s: &mut TcpStream, body: &str) -> io::Result<()> {
    s.set_read_timeout(Some(Duration::from_secs(2)))?;
    let mut req = Vec::new();
    let mut chunk = [0u8; 1024];
    while !req.windows(4).any(|w| w == b"\r\n\r\n") {
        let n = s.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        req.extend_from_slice(&chunk[..n]);
    }
    write!(
        s,
        "HTTP/1.0 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )?;
    s.flush()
}

const WITH_MARKER: &str = "<html><head><title>Fake dashboard A</title></head></html>";
const WITHOUT_MARKER: &str = "<html><head><title>Someone else</title></head></html>";

fn identity_retry_rule() {
    let root = scratch("identity-retry");
    let (sup, spawns) = counting(&root, real_connect);

    let late = Fixture::new(|n| {
        if n <= 1 {
            Act::Close
        } else {
            Act::Reply(WITH_MARKER)
        }
    });
    let d = unmanaged(late.addr);
    assert_eq!(
        sup.ensure(None, &d),
        Ok(Running {
            url: d.url.clone(),
            owned: false
        })
    );
    // `connect` is accepted first; the identity probe's GETs follow it.
    assert_eq!(
        late.accepts(),
        3,
        "connect, a closed GET, a GET with the marker"
    );

    let foreign = Fixture::new(|_| Act::Reply(WITHOUT_MARKER));
    match sup.ensure(None, &unmanaged(foreign.addr)) {
        Err(Refusal::Identity {
            status,
            head,
            owned: false,
            spawned: false,
        }) => {
            assert_eq!(status, "HTTP/1.0 200 OK");
            assert!(head.starts_with("HTTP/1.0 200 OK"), "{head}");
        }
        other => panic!("foreign listener: {other:?}"),
    }
    assert_eq!(foreign.accepts(), 2, "connect and exactly one GET");

    let silent = Fixture::new(|_| Act::Silent);
    assert_eq!(
        sup.ensure(None, &unmanaged(silent.addr)),
        Err(Refusal::Identity {
            status: "no response".into(),
            head: String::new(),
            owned: false,
            spawned: false,
        })
    );
    assert_eq!(silent.accepts(), 3, "connect and two GETs");
    assert_eq!(spawns.load(Ordering::SeqCst), 0);
}

fn unmanaged_not_running() {
    let root = scratch("unmanaged");
    let (sup, spawns) = counting(&root, real_connect);
    let d = unmanaged(free_addr());
    let want = Err(Refusal::NotRunning {
        hint: "its owner (for example launchd) is not running it".into(),
    });
    assert_eq!(sup.ensure(None, &d), want);
    // The next open drops the Refused slot and probes again.
    assert_eq!(sup.ensure(None, &d), want);
    assert_eq!(spawns.load(Ordering::SeqCst), 0, "nothing spawned");
}

/// Opens `d` twice, the second while the first's `connect` is blocked. Returns both
/// results, the number of `connect` calls and the number of spawns.
fn two_opens(root: &Path, d: &Dashboard) -> ([Result<Running, Refusal>; 2], usize, usize) {
    let (gate_tx, gate_rx) = mpsc::channel::<()>();
    let gate = Mutex::new(gate_rx);
    let connects = Arc::new(AtomicUsize::new(0));
    let counted = connects.clone();
    let (sup, spawns) = counting(root, move |addr| {
        if counted.fetch_add(1, Ordering::SeqCst) == 0 {
            let _ = gate.lock().unwrap().recv();
        }
        real_connect(addr)
    });
    let sup = Arc::new(sup);
    let (tx, rx) = mpsc::channel();
    let mut threads = Vec::new();
    for i in 0..2 {
        if i == 1 {
            assert!(
                until(Duration::from_secs(2), || connects.load(Ordering::SeqCst)
                    == 1),
                "the first open never reached connect"
            );
        }
        let (sup, d, tx) = (sup.clone(), d.clone(), tx.clone());
        threads.push(std::thread::spawn(move || {
            let _ = tx.send((i, sup.ensure(None, &d)));
        }));
    }
    assert!(
        rx.recv_timeout(Duration::from_millis(300)).is_err(),
        "an open returned while the first probe was blocked"
    );
    assert_eq!(
        connects.load(Ordering::SeqCst),
        1,
        "the waiter probed a Reserved slot"
    );
    gate_tx.send(()).unwrap();
    let mut got: [Option<Result<Running, Refusal>>; 2] = [None, None];
    for _ in 0..2 {
        let (i, r) = rx
            .recv_timeout(Duration::from_secs(15))
            .expect("an open hung");
        got[i] = Some(r);
    }
    for t in threads {
        t.join().unwrap();
    }
    let [a, b] = got;
    (
        [a.unwrap(), b.unwrap()],
        connects.load(Ordering::SeqCst),
        spawns.load(Ordering::SeqCst),
    )
}

fn waiter_on_reserved_gets_identity() {
    let root = scratch("waiter-identity");
    let foreign = Fixture::new(|_| Act::Reply(WITHOUT_MARKER));
    let d = unmanaged(foreign.addr);
    let ([first, second], connects, spawns) = two_opens(&root, &d);
    assert!(
        matches!(
            first,
            Err(Refusal::Identity {
                owned: false,
                spawned: false,
                ..
            })
        ),
        "{first:?}"
    );
    assert_eq!(second, first, "the waiter gets a copy of the refusal");
    assert_eq!(connects, 1, "the waiter never probed");
    assert_eq!(foreign.accepts(), 2, "one connect and one GET in all");
    assert_eq!(spawns, 0);
}

fn waiter_on_reserved_embeds_external() {
    let root = scratch("waiter-external");
    let addr = free_addr();
    let mut kid = spawn_mode("serve", addr);
    assert!(
        until(Duration::from_secs(5), || matched(&get(
            addr, "/health", READY
        ))),
        "serve never became ready"
    );
    let d = unmanaged(addr);
    let ([first, second], connects, spawns) = two_opens(&root, &d);
    let want = Ok(Running {
        url: d.url.clone(),
        owned: false,
    });
    assert_eq!(first, want);
    assert_eq!(second, want);
    assert_eq!(
        connects, 2,
        "the waiter reruns ensure once the slot is removed"
    );
    assert_eq!(spawns, 0);
    assert!(
        kid.0.try_wait().unwrap().is_none(),
        "the External listener was signalled"
    );
    assert!(
        matched(&get(addr, "/", MARKER_A)),
        "the External listener stopped answering"
    );
}

fn refused<T: std::fmt::Debug>(r: Result<T, StoreError>) -> &'static str {
    match r {
        Err(StoreError::Refused { rule }) => rule,
        other => panic!("not refused: {other:?}"),
    }
}

fn pidfile_written_from_spawn_result() {
    let root = scratch("pidfile-write");
    let sup = supervisor(&root.join("data"), &root.join("logs")).unwrap();
    let addr = free_addr();
    let kid = spawn_mode("idle", addr);
    let pid = kid.pid();
    let file = root
        .join("data")
        .join("servers")
        .join(format!("{}.json", addr.port()));
    std::fs::write(&file, r#"{"pid":1,"pgid":1,"start_time":[0,0],"exe":"/x"}"#).unwrap();

    sup.write_pidfile(&SpawnResult::read(&kid.0), addr)
        .expect("write the pidfile");
    let got: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    let (secs, micros) = proc::start_time(pid).expect("start time");
    assert_eq!(
        got,
        json!({
            "pid": pid,
            "pgid": proc::pgid(pid).expect("pgid"),
            "start_time": [secs, micros],
            "exe": proc::exe(pid).expect("exe"),
            "addr": addr.to_string(),
        })
    );

    let zero = SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0);
    assert_eq!(
        refused(sup.write_pidfile(&SpawnResult::read(&kid.0), zero)),
        "pidfile_path"
    );
    assert!(!root.join("data").join("servers").join("0.json").exists());
}

fn pidfile_removal_is_gated() {
    let root = scratch("pidfile-remove");
    let sup = supervisor(&root.join("data"), &root.join("logs")).unwrap();
    let servers = root.join("data").join("servers");
    let planted = |path: &Path| {
        std::fs::write(path, "{}").unwrap();
        path.to_owned()
    };

    for path in [
        servers.join("04777.json"),
        servers.join("70000.json"),
        servers.join("abc.json"),
        servers.join("4777.lock"),
        root.join("4777.json"),
    ] {
        let path = planted(&path);
        assert_eq!(
            refused(sup.remove_pidfile(&path, &Ownership::Stale)),
            "pidfile_path",
            "{}",
            path.display()
        );
        assert!(path.exists(), "{} removed", path.display());
    }

    let valid = planted(&servers.join("4777.json"));
    for ownership in [
        Ownership::Owned { exited: false },
        Ownership::Starting { exited: false },
        Ownership::Leftover { stopped: false },
        Ownership::Held,
    ] {
        assert_eq!(
            refused(sup.remove_pidfile(&valid, &ownership)),
            "ownership",
            "{ownership:?}"
        );
        assert!(valid.exists(), "{ownership:?} removed the pidfile");
    }

    for ownership in [
        Ownership::Owned { exited: true },
        Ownership::Starting { exited: true },
        Ownership::Leftover { stopped: true },
        Ownership::Stale,
    ] {
        let valid = planted(&valid);
        sup.remove_pidfile(&valid, &ownership)
            .unwrap_or_else(|e| panic!("{ownership:?}: {e:?}"));
        assert!(!valid.exists(), "{ownership:?} kept the pidfile");
    }
    sup.remove_pidfile(&valid, &Ownership::Stale)
        .expect("a pidfile already gone");
}

/// A server running `binary` on `addr`, and its dashboard probing `/` for `marker`.
fn managed(addr: SocketAddrV4, binary: PathBuf, marker: &str) -> (Server, Dashboard) {
    let server = Server {
        binary,
        addr,
        ready: Probe {
            path: "/health".into(),
            contains: READY.into(),
        },
    };
    let dashboard = Dashboard {
        name: "Control room".into(),
        url: format!("http://{addr}/"),
        port: addr.port(),
        server: Some("fake".into()),
        probe: Probe {
            path: "/".into(),
            contains: marker.into(),
        },
    };
    (server, dashboard)
}

/// Takes `<root>/data/servers/<port>.lock` as another app instance would; `None` while
/// someone else holds it.
fn take_lock(root: &Path, port: u16) -> Option<File> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(
            root.join("data")
                .join("servers")
                .join(format!("{port}.lock")),
        )
        .unwrap();
    // SAFETY: flock on a descriptor this process owns; no memory is touched.
    let taken = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0;
    taken.then_some(file)
}

fn binary_admissible_refuses_before_spawn() {
    let root = scratch("admissible");
    let (sup, spawns) = counting(&root, real_connect);

    let addr = free_addr();
    let missing = root.join("bin").join("control-room");
    let (server, d) = managed(addr, missing.clone(), MARKER_A);
    assert_eq!(
        sup.ensure(Some(&server), &d),
        Err(Refusal::BinaryMissing { path: missing })
    );
    assert!(
        take_lock(&root, addr.port()).is_some(),
        "the port lock was left held"
    );

    let plain = root.join("plain");
    std::fs::write(&plain, "#!/bin/sh\n").unwrap();
    std::fs::set_permissions(&plain, std::fs::Permissions::from_mode(0o644)).unwrap();
    let (server, d) = managed(free_addr(), plain.clone(), MARKER_A);
    assert_eq!(
        sup.ensure(Some(&server), &d),
        Err(Refusal::NotAdmissible {
            rule: "executable".into(),
            path: plain,
        })
    );

    let dir = root.join("bin");
    std::fs::create_dir_all(&dir).unwrap();
    let (server, _) = managed(addr, dir.clone(), MARKER_A);
    assert_eq!(
        binary_admissible(&server, true),
        Err(Refusal::NotAdmissible {
            rule: "regular_file".into(),
            path: dir,
        })
    );
    let (server, _) = managed(addr, PathBuf::from("bin/control-room"), MARKER_A);
    assert_eq!(
        binary_admissible(&server, true),
        Err(Refusal::NotAdmissible {
            rule: "absolute".into(),
            path: "bin/control-room".into(),
        })
    );

    let exe = std::env::current_exe().unwrap();
    let (server, _) = managed(addr, exe.clone(), MARKER_A);
    assert_eq!(
        binary_admissible(&server, false),
        Err(Refusal::NotAdmissible {
            rule: "platform".into(),
            path: exe,
        })
    );
    assert_eq!(binary_admissible(&server, true), Ok(()));
    assert_eq!(spawns.load(Ordering::SeqCst), 0, "nothing spawned");
}

fn spawn_result_admissible_names_the_field() {
    let exe = Path::new("/usr/local/bin/control-room");
    let started = Some((1_700_000_000, 0));

    assert_eq!(
        spawn_result_admissible(0, Some(0), started, Some(exe)),
        Err("pid")
    );
    assert_eq!(
        spawn_result_admissible(-1, Some(-1), started, Some(exe)),
        Err("pid")
    );
    assert_eq!(
        spawn_result_admissible(4242, Some(4241), started, Some(exe)),
        Err("pgid")
    );
    assert_eq!(
        spawn_result_admissible(4242, None, started, Some(exe)),
        Err("pgid")
    );
    assert_eq!(
        spawn_result_admissible(4242, Some(4242), started, None),
        Err("exe")
    );
    assert_eq!(
        spawn_result_admissible(4242, Some(4242), started, Some(Path::new(""))),
        Err("exe")
    );
    assert_eq!(
        spawn_result_admissible(4242, Some(4242), None, Some(exe)),
        Err("start_time")
    );
    assert_eq!(
        spawn_result_admissible(4242, Some(4242), started, Some(exe)),
        Ok(())
    );
}

fn held_port_lock_refuses() {
    let root = scratch("lock-held");
    let (sup, spawns) = counting(&root, real_connect);
    let addr = free_addr();
    let other = take_lock(&root, addr.port()).expect("the other instance takes the lock");
    let (server, d) = managed(addr, std::env::current_exe().unwrap(), MARKER_A);
    let want = Err(Refusal::NotRunning {
        hint: "another app instance is starting it; open the dashboard again".into(),
    });
    assert_eq!(sup.ensure(Some(&server), &d), want);
    // The next open drops the Refused slot and meets the same lock.
    assert_eq!(sup.ensure(Some(&server), &d), want);
    assert_eq!(spawns.load(Ordering::SeqCst), 0, "nothing spawned");
    drop(other);
}

fn answering_port_is_never_spawned_on() {
    let root = scratch("answering");
    let (sup, spawns) = counting(&root, real_connect);
    let addr = free_addr();
    let mut kid = spawn_mode("serve", addr);
    assert!(
        until(Duration::from_secs(5), || matched(&get(
            addr, "/health", READY
        ))),
        "serve never became ready"
    );
    let exe = std::env::current_exe().unwrap();

    let (server, d) = managed(addr, exe.clone(), MARKER_A);
    assert_eq!(
        sup.ensure(Some(&server), &d),
        Ok(Running {
            url: d.url.clone(),
            owned: false
        })
    );
    let (server, d) = managed(addr, exe, MARKER_B);
    assert!(
        matches!(
            sup.ensure(Some(&server), &d),
            Err(Refusal::Identity {
                owned: false,
                spawned: false,
                ..
            })
        ),
        "a listener without the marker"
    );
    assert_eq!(
        spawns.load(Ordering::SeqCst),
        0,
        "spawned on a port that answers"
    );
    assert!(
        take_lock(&root, addr.port()).is_some(),
        "the port lock was taken for an answering port"
    );
    assert!(
        kid.0.try_wait().unwrap().is_none(),
        "the listener was signalled"
    );
}

fn concurrent_first_opens_spawn_once() {
    let root = scratch("first-opens");
    let addr = free_addr();
    let (server, d) = managed(addr, std::env::current_exe().unwrap(), MARKER_A);
    let (gate_tx, gate_rx) = mpsc::channel::<()>();
    let gate = Mutex::new(gate_rx);
    let connects = Arc::new(AtomicUsize::new(0));
    let spawned = Arc::new(Mutex::new(Vec::<libc::pid_t>::new()));
    let (exit_tx, exit_rx) = mpsc::channel::<ExitNotice>();
    let exit_tx = Mutex::new(exit_tx);
    let (counted, recorded) = (connects.clone(), spawned.clone());
    let sup = Arc::new(
        Supervisor::new(
            &root.join("data"),
            &root.join("logs"),
            move |n: ExitNotice| {
                let _ = exit_tx.lock().unwrap().send(n);
            },
            move |server: &Server, log: &Path| -> anyhow::Result<Child> {
                let child = real_spawn(server, log)?;
                recorded.lock().unwrap().push(child.id() as libc::pid_t);
                Ok(child)
            },
            move |a: SocketAddrV4| {
                if counted.fetch_add(1, Ordering::SeqCst) == 0 {
                    let _ = gate.lock().unwrap().recv();
                }
                real_connect(a)
            },
        )
        .unwrap(),
    );

    let (tx, rx) = mpsc::channel();
    let mut threads = Vec::new();
    for i in 0..2 {
        if i == 1 {
            assert!(
                until(Duration::from_secs(2), || connects.load(Ordering::SeqCst)
                    == 1),
                "the first open never reached connect"
            );
        }
        let (sup, server, d, tx) = (sup.clone(), server.clone(), d.clone(), tx.clone());
        threads.push(std::thread::spawn(move || {
            let _ = tx.send((i, sup.ensure(Some(&server), &d)));
        }));
    }
    assert!(
        rx.recv_timeout(Duration::from_millis(300)).is_err(),
        "an open returned while the first probe was blocked"
    );
    gate_tx.send(()).unwrap();
    let mut got: [Option<Result<Running, Refusal>>; 2] = [None, None];
    for _ in 0..2 {
        let (i, r) = rx
            .recv_timeout(Duration::from_secs(20))
            .expect("an open hung");
        got[i] = Some(r);
    }
    for t in threads {
        t.join().unwrap();
    }
    let pids = spawned.lock().unwrap().clone();
    let group = pids.first().map(|&pid| Leftover(pid));
    let want = Ok(Running {
        url: d.url.clone(),
        owned: true,
    });
    assert_eq!(got[0].as_ref(), Some(&want), "first open");
    assert_eq!(got[1].as_ref(), Some(&want), "second open");
    assert_eq!(pids.len(), 1, "one spawn");
    assert_eq!(
        connects.load(Ordering::SeqCst),
        1,
        "the waiter connected instead of reusing the slot"
    );
    assert!(
        take_lock(&root, addr.port()).is_none(),
        "the port lock is not held"
    );

    let pid = pids[0];
    let pidfile = root
        .join("data")
        .join("servers")
        .join(format!("{}.json", addr.port()));
    let got: Value = serde_json::from_slice(&std::fs::read(&pidfile).unwrap()).unwrap();
    let (secs, micros) = proc::start_time(pid).expect("start time");
    assert_eq!(
        got,
        json!({
            "pid": pid,
            "pgid": pid,
            "start_time": [secs, micros],
            "exe": proc::exe(pid).expect("exe"),
            "addr": addr.to_string(),
        })
    );
    let log = root
        .join("logs")
        .join("servers")
        .join(format!("{}.log", addr.port()));
    let text = std::fs::read_to_string(&log).expect("the server log");
    assert!(text.contains("GET /health HTTP/1.0"), "{text}");
    assert_eq!(
        text.lines().filter(|l| *l == "GET / HTTP/1.0").count(),
        2,
        "one identity probe per open: {text}"
    );

    proc::kill_group(pid);
    let n = exit_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("the reaper never reported the exit");
    assert_eq!(n.addr, addr);
    assert_eq!(n.status, "signal 9");
    assert!(n.owned, "an Owned server's exit");
    assert!(n.log_tail.contains("GET / HTTP/1.0"), "{}", n.log_tail);
    assert!(
        until(Duration::from_secs(5), || take_lock(&root, addr.port())
            .is_some()),
        "the reaper never released the port lock"
    );
    assert!(!pidfile.exists(), "the reaper left the pidfile");
    // SAFETY: WNOHANG never blocks; a reaped pid is no longer our child.
    let r = unsafe { libc::waitpid(pid, std::ptr::null_mut(), libc::WNOHANG) };
    assert_eq!(r, -1, "the reaper never reaped the child");
    // The group id is free again once reaped; killing it now could reach a stranger.
    std::mem::forget(group);
}

fn pidfile_of(root: &Path, addr: SocketAddrV4) -> PathBuf {
    root.join("data")
        .join("servers")
        .join(format!("{}.json", addr.port()))
}

/// A supervisor with `real_connect`, so ensure: probe finds the free port and spawns.
fn spawning(
    root: &Path,
    emit: impl Fn(ExitNotice) + Send + Sync + 'static,
    spawn: impl Fn(&Server, &Path) -> anyhow::Result<Child> + Send + Sync + 'static,
) -> Arc<Supervisor> {
    Arc::new(
        Supervisor::new(
            &root.join("data"),
            &root.join("logs"),
            emit,
            spawn,
            real_connect,
        )
        .unwrap(),
    )
}

/// Spawns `<self> <mode> <addr>` into group `group` (0: a group of its own), with stdout
/// and stderr appended to the log after a `fake: spawning <mode>` line, and records its pid.
fn fake_spawn(
    mode: &'static str,
    group: libc::pid_t,
    pids: Arc<Mutex<Vec<libc::pid_t>>>,
) -> impl Fn(&Server, &Path) -> anyhow::Result<Child> + Send + Sync + 'static {
    move |server: &Server, log: &Path| {
        let mut out = OpenOptions::new().create(true).append(true).open(log)?;
        writeln!(out, "fake: spawning {mode}")?;
        let err = out.try_clone()?;
        let child = Command::new(std::env::current_exe()?)
            .args([mode, &server.addr.to_string()])
            .process_group(group)
            .stdin(Stdio::null())
            .stdout(out)
            .stderr(err)
            .spawn()?;
        pids.lock().unwrap().push(child.id() as libc::pid_t);
        Ok(child)
    }
}

/// An `emit` that forwards every notice to the returned receiver.
fn recording() -> (
    impl Fn(ExitNotice) + Send + Sync + 'static,
    mpsc::Receiver<ExitNotice>,
) {
    let (tx, rx) = mpsc::channel();
    let tx = Mutex::new(tx);
    (
        move |n: ExitNotice| {
            let _ = tx.lock().unwrap().send(n);
        },
        rx,
    )
}

type Opened = (usize, Result<Running, Refusal>);

/// Runs `ensure` on its own thread and sends `(i, result)` to `tx`.
fn open_on_thread(
    sup: &Arc<Supervisor>,
    server: &Server,
    d: &Dashboard,
    i: usize,
    tx: &mpsc::Sender<Opened>,
) -> JoinHandle<()> {
    let (sup, server, d, tx) = (sup.clone(), server.clone(), d.clone(), tx.clone());
    std::thread::spawn(move || {
        let _ = tx.send((i, sup.ensure(Some(&server), &d)));
    })
}

fn collect(
    rx: &mpsc::Receiver<Opened>,
    threads: Vec<JoinHandle<()>>,
    within: Duration,
) -> Vec<Result<Running, Refusal>> {
    let mut got: Vec<Option<Result<Running, Refusal>>> = vec![None; threads.len()];
    for _ in 0..threads.len() {
        let (i, r) = rx.recv_timeout(within).expect("an open hung");
        got[i] = Some(r);
    }
    for t in threads {
        t.join().unwrap();
    }
    got.into_iter().map(Option::unwrap).collect()
}

fn log_tail_has(r: &Result<Running, Refusal>, line: &str) {
    match r {
        Err(Refusal::FailedToStart { log_tail }) => {
            assert!(log_tail.contains(line), "{line:?} not in {log_tail:?}")
        }
        other => panic!("not FailedToStart: {other:?}"),
    }
}

fn readiness_timeout_stops_the_group() {
    let root = scratch("ready-timeout");
    let addr = free_addr();
    let (server, d) = managed(addr, std::env::current_exe().unwrap(), MARKER_A);
    let pids = Arc::new(Mutex::new(Vec::new()));
    let (emit, exits) = recording();
    let sup = spawning(&root, emit, fake_spawn("idle", 0, pids.clone()));
    let pidfile = pidfile_of(&root, addr);

    let (tx, rx) = mpsc::channel();
    let mut threads = vec![open_on_thread(&sup, &server, &d, 0, &tx)];
    assert!(
        until(Duration::from_secs(5), || pidfile.exists()),
        "the spawn never wrote its pidfile"
    );
    threads.push(open_on_thread(&sup, &server, &d, 1, &tx));
    let got = collect(&rx, threads, Duration::from_secs(25));

    let pids = pids.lock().unwrap().clone();
    assert_eq!(pids.len(), 1, "one spawn");
    log_tail_has(&got[0], "fake: spawning idle");
    assert_eq!(got[1], got[0], "the waiter gets the same refusal");
    let n = exits
        .recv_timeout(Duration::from_secs(5))
        .expect("the reaper never reported the exit");
    assert_eq!(n.status, "signal 15", "the group was sent SIGTERM");
    assert!(!n.owned);
    assert!(
        !proc::group_alive(pids[0]),
        "the group outlived the epilogue"
    );
    assert!(!pidfile.exists(), "the pidfile was kept");
    assert!(
        take_lock(&root, addr.port()).is_some(),
        "the port lock is still held"
    );
}

fn open_during_epilogue_waits() {
    let root = scratch("epilogue-open");
    let addr = free_addr();
    let (server, d) = managed(addr, std::env::current_exe().unwrap(), MARKER_A);
    let pids = Arc::new(Mutex::new(Vec::new()));
    let (exit_tx, exits) = mpsc::channel::<ExitNotice>();
    let exit_tx = Mutex::new(exit_tx);
    let sup = spawning(
        &root,
        move |n: ExitNotice| {
            let owned = n.owned;
            let _ = exit_tx.lock().unwrap().send(n);
            if !owned {
                std::thread::sleep(Duration::from_millis(200));
            }
        },
        fake_spawn("idle", 0, pids.clone()),
    );
    let pidfile = pidfile_of(&root, addr);

    let (tx, rx) = mpsc::channel();
    let mut threads = vec![open_on_thread(&sup, &server, &d, 0, &tx)];
    assert!(
        until(Duration::from_secs(5), || pidfile.exists()),
        "the spawn never wrote its pidfile"
    );
    let pid = pids.lock().unwrap()[0];
    // SAFETY: `pid` is this process's unreaped child; its reaper has not run `waitpid`.
    unsafe {
        libc::kill(pid, libc::SIGKILL);
    }
    let n = exits
        .recv_timeout(Duration::from_secs(5))
        .expect("the reaper never reported the exit");
    assert!(!n.owned);
    threads.push(open_on_thread(&sup, &server, &d, 1, &tx));
    let got = collect(&rx, threads, Duration::from_secs(15));

    assert_eq!(pids.lock().unwrap().len(), 1, "a second spawn");
    log_tail_has(&got[0], "fake: spawning idle");
    assert_eq!(
        got[1], got[0],
        "the open during the epilogue got another result"
    );
    assert!(
        exits.try_iter().all(|n| !n.owned),
        "dashboard-exited for a Starting slot"
    );
    assert!(!pidfile.exists(), "the pidfile was kept");
    assert!(
        take_lock(&root, addr.port()).is_some(),
        "the port lock is still held"
    );
}

fn child_exits_at_once() {
    for round in 0..3 {
        let root = scratch(&format!("exits-at-once-{round}"));
        let addr = free_addr();
        let (server, d) = managed(addr, std::env::current_exe().unwrap(), MARKER_A);
        let spawns = Arc::new(AtomicUsize::new(0));
        let counted = spawns.clone();
        let (emit, exits) = recording();
        let sup = spawning(&root, emit, move |_: &Server, log: &Path| {
            let mut out = OpenOptions::new().create(true).append(true).open(log)?;
            writeln!(out, "fake: spawning sh")?;
            let err = out.try_clone()?;
            let child = Command::new("/bin/sh")
                .args(["-c", "exit 3"])
                .process_group(0)
                .stdin(Stdio::null())
                .stdout(out)
                .stderr(err)
                .spawn()?;
            counted.fetch_add(1, Ordering::SeqCst);
            Ok(child)
        });

        let r = sup.ensure(Some(&server), &d);
        log_tail_has(&r, "fake: spawning sh");
        assert_eq!(spawns.load(Ordering::SeqCst), 1);
        assert!(!pidfile_of(&root, addr).exists(), "a pidfile remains");
        assert!(
            take_lock(&root, addr.port()).is_some(),
            "the port lock is still held after the epilogue"
        );
        let n = exits
            .recv_timeout(Duration::from_secs(5))
            .expect("the reaper never reported the exit");
        assert!(!n.owned);
    }
}

fn leader_target_signals_the_leader_alone() {
    let root = scratch("leader-target");
    let mut group = spawn_mode("idle", free_addr());
    let addr = free_addr();
    let (server, d) = managed(addr, std::env::current_exe().unwrap(), MARKER_A);
    let pids = Arc::new(Mutex::new(Vec::new()));
    let (emit, exits) = recording();
    let sup = spawning(&root, emit, fake_spawn("idle", group.pid(), pids.clone()));

    let started = Instant::now();
    let r = sup.ensure(Some(&server), &d);
    log_tail_has(&r, "fake: spawning idle");
    log_tail_has(&r, "cos: spawn result refused: pgid");
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "the leader needed SIGKILL"
    );
    let pids = pids.lock().unwrap().clone();
    assert_eq!(pids.len(), 1);
    assert!(
        group.0.try_wait().unwrap().is_none(),
        "the group the child joined was signalled"
    );
    let n = exits
        .recv_timeout(Duration::from_secs(5))
        .expect("the reaper never reported the exit");
    assert_eq!(n.status, "signal 15", "the leader was sent SIGTERM");
    assert!(!n.owned);
    // SAFETY: WNOHANG never blocks; a reaped pid is no longer our child.
    let reaped = unsafe { libc::waitpid(pids[0], std::ptr::null_mut(), libc::WNOHANG) };
    assert_eq!(reaped, -1, "the reaper never reaped the leader");
    assert!(!pidfile_of(&root, addr).exists(), "a pidfile was written");
    assert!(
        take_lock(&root, addr.port()).is_some(),
        "the port lock is still held"
    );
}

/// Asserts an `identity` refusal from the probe after readiness of a spawned server.
fn refused_as_spawned(r: &Result<Running, Refusal>) {
    match r {
        Err(Refusal::Identity {
            status,
            owned: false,
            spawned: true,
            ..
        }) => assert!(status.starts_with("HTTP/1.0 200"), "{status}"),
        other => panic!("not Identity marked spawned: {other:?}"),
    }
}

/// Asserts that `pid`'s reaper reported a non-owned exit, reaped it and left its group
/// empty.
fn stopped(pid: libc::pid_t, exits: &mpsc::Receiver<ExitNotice>) {
    let n = exits
        .recv_timeout(Duration::from_secs(5))
        .expect("the reaper never reported the exit");
    assert_eq!(n.status, "signal 15", "the child was sent SIGTERM");
    assert!(!n.owned, "dashboard-exited for a Starting slot");
    // SAFETY: WNOHANG never blocks; a reaped pid is no longer our child.
    let reaped = unsafe { libc::waitpid(pid, std::ptr::null_mut(), libc::WNOHANG) };
    assert_eq!(reaped, -1, "the reaper never reaped the child");
    assert!(!proc::group_alive(pid), "the child's group outlived it");
}

const READY_ONLY: &str = r#"{"status":"ok"}"#;

fn foreign_listener_before_the_child_binds() {
    let root = scratch("foreign-before-bind");
    let foreign = Fixture::new(|_| Act::Reply(READY_ONLY));
    let addr = foreign.addr;
    let (server, d) = managed(addr, std::env::current_exe().unwrap(), MARKER_A);
    let pids = Arc::new(Mutex::new(Vec::new()));
    let (emit, exits) = recording();
    let sup = Supervisor::new(
        &root.join("data"),
        &root.join("logs"),
        emit,
        fake_spawn("idle", 0, pids.clone()),
        |_: SocketAddrV4| false,
    )
    .unwrap();

    let r = sup.ensure(Some(&server), &d);
    refused_as_spawned(&r);
    let pids = pids.lock().unwrap().clone();
    assert_eq!(pids.len(), 1, "the free port was not spawned on");
    assert!(
        foreign.accepts() >= 3,
        "readiness and identity never reached the listener: {}",
        foreign.accepts()
    );
    stopped(pids[0], &exits);
    assert!(!pidfile_of(&root, addr).exists(), "a pidfile remains");
    assert!(
        take_lock(&root, addr.port()).is_some(),
        "the port lock is still held"
    );
    assert!(
        matched(&get(addr, "/health", READY)),
        "the foreign listener stopped answering"
    );
}

fn spawned_page_without_the_marker_is_stopped() {
    let root = scratch("spawned-no-marker");
    let addr = free_addr();
    let (server, d) = managed(
        addr,
        std::env::current_exe().unwrap(),
        "<title>Never served",
    );
    let pids = Arc::new(Mutex::new(Vec::new()));
    let (emit, exits) = recording();
    let sup = spawning(&root, emit, fake_spawn("serve", 0, pids.clone()));

    let r = sup.ensure(Some(&server), &d);
    refused_as_spawned(&r);
    let pids = pids.lock().unwrap().clone();
    assert_eq!(pids.len(), 1);
    stopped(pids[0], &exits);
    assert!(!pidfile_of(&root, addr).exists(), "a pidfile remains");
    assert!(
        TcpListener::bind(addr).is_ok(),
        "the port is still bound after the epilogue"
    );
    assert!(
        take_lock(&root, addr.port()).is_some(),
        "the port lock is still held"
    );
}

fn spawned_page_with_the_marker_is_owned() {
    let root = scratch("spawned-marker");
    let addr = free_addr();
    let (server, d) = managed(addr, std::env::current_exe().unwrap(), MARKER_A);
    let pids = Arc::new(Mutex::new(Vec::new()));
    let (emit, exits) = recording();
    let sup = spawning(&root, emit, fake_spawn("serve", 0, pids.clone()));
    let want = Ok(Running {
        url: d.url.clone(),
        owned: true,
    });

    assert_eq!(sup.ensure(Some(&server), &d), want);
    let pid = pids.lock().unwrap()[0];
    assert!(pidfile_of(&root, addr).exists(), "the pidfile was not kept");
    assert!(
        exits.recv_timeout(Duration::from_millis(300)).is_err(),
        "the owned server exited"
    );
    assert!(proc::alive(pid), "the owned server was signalled");
    assert!(
        take_lock(&root, addr.port()).is_none(),
        "the Owned slot gave up its port lock"
    );
    assert_eq!(sup.ensure(Some(&server), &d), want, "the Owned slot reused");
    assert_eq!(pids.lock().unwrap().len(), 1, "a second spawn");
    assert!(pidfile_of(&root, addr).exists(), "the pidfile was removed");
    kill_and_reap(&root, addr, pid, &exits);
}

fn owned_child_exit_is_reported() {
    let root = scratch("owned-exit");
    let addr = free_addr();
    let (server, d) = managed(addr, std::env::current_exe().unwrap(), MARKER_A);
    let pids = Arc::new(Mutex::new(Vec::new()));
    let (emit, exits) = recording();
    let sup = spawning(&root, emit, fake_spawn("serve", 0, pids.clone()));
    let pidfile = pidfile_of(&root, addr);
    let want = Ok(Running {
        url: d.url.clone(),
        owned: true,
    });

    assert_eq!(sup.ensure(Some(&server), &d), want);
    let pid = pids.lock().unwrap()[0];
    let record: Value = serde_json::from_slice(&std::fs::read(&pidfile).unwrap()).unwrap();
    assert_eq!(record["pgid"], json!(pid), "the stop target is not a group");
    proc::signal_pid(pid, libc::SIGKILL);

    let n = exits
        .recv_timeout(Duration::from_secs(5))
        .expect("the reaper never reported the exit");
    assert!(n.owned, "an Owned server's exit");
    assert_eq!(n.addr, addr);
    assert_eq!(n.status, "signal 9");
    assert!(
        n.log_tail.contains("fake: spawning serve") && n.log_tail.contains("GET / HTTP/1.0"),
        "{}",
        n.log_tail
    );
    assert!(!pidfile.exists(), "the reaper left the pidfile");
    assert!(
        until(Duration::from_secs(5), || take_lock(&root, addr.port())
            .is_some()),
        "the reaper never released the port lock"
    );
    // SAFETY: WNOHANG never blocks; a reaped pid is no longer our child.
    let reaped = unsafe { libc::waitpid(pid, std::ptr::null_mut(), libc::WNOHANG) };
    assert_eq!(reaped, -1, "the reaper never reaped the child");
    assert!(
        !proc::group_alive(pid),
        "a member of the reaped leader's group is left"
    );
    assert!(
        exits.recv_timeout(Duration::from_millis(300)).is_err(),
        "a second exit notice"
    );

    assert_eq!(
        sup.ensure(Some(&server), &d),
        want,
        "the reopen after the Exited slot"
    );
    let pids = pids.lock().unwrap().clone();
    assert_eq!(pids.len(), 2, "the Exited slot was reused, not replaced");
    kill_and_reap(&root, addr, pids[1], &exits);
}

fn server_log_truncated_before_spawn() {
    const MIB: usize = 1024 * 1024;
    let root = scratch("server-log");
    let addr = free_addr();
    let (server, d) = managed(addr, std::env::current_exe().unwrap(), MARKER_A);
    let want = root
        .join("logs")
        .join("servers")
        .join(format!("{}.log", addr.port()));
    let mut old = vec![b'x'; 4096];
    old.extend(std::iter::repeat_n(b'y', MIB - "last line\n".len()));
    old.extend_from_slice(b"last line\n");
    std::fs::create_dir_all(want.parent().unwrap()).unwrap();
    std::fs::write(&want, &old).unwrap();

    let seen = Arc::new(Mutex::new(None::<(PathBuf, Vec<u8>)>));
    let saw = seen.clone();
    let (emit, exits) = recording();
    let sup = spawning(&root, emit, move |_: &Server, log: &Path| {
        *saw.lock().unwrap() = Some((log.to_owned(), std::fs::read(log)?));
        let mut out = OpenOptions::new().create(true).append(true).open(log)?;
        writeln!(out, "fake: spawning sh")?;
        let err = out.try_clone()?;
        let child = Command::new("/bin/sh")
            .args(["-c", "echo child output; exit 3"])
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(out)
            .stderr(err)
            .spawn()?;
        Ok(child)
    });

    let r = sup.ensure(Some(&server), &d);
    let n = exits
        .recv_timeout(Duration::from_secs(5))
        .expect("the reaper never reported the exit");
    assert!(!n.owned);
    log_tail_has(&r, "fake: spawning sh");

    let (path, before) = seen.lock().unwrap().take().expect("the spawn never ran");
    assert_eq!(path, want, "the log path");
    assert_eq!(before.len(), MIB, "the log before the spawn");
    assert!(before[..] == old[old.len() - MIB..], "not the last 1 MiB");
    let after = std::fs::read(&want).unwrap();
    assert!(after[..MIB] == before[..], "the spawn did not append");
    let appended = String::from_utf8_lossy(&after[MIB..]);
    assert!(appended.starts_with("fake: spawning sh\n"), "{appended}");
    assert!(appended.contains("child output\n"), "{appended}");
}

fn port_lock_open_is_gated() {
    let root = scratch("lock-open");
    let sup = supervisor(&root.join("data"), &root.join("logs")).unwrap();
    let servers = root.join("data").join("servers");
    let mut kid = spawn_mode("idle", free_addr());

    for path in [
        servers.join("04777.lock"),
        servers.join("70000.lock"),
        servers.join("0.lock"),
        servers.join("abc.lock"),
        servers.join("4777.json"),
        root.join("4777.lock"),
    ] {
        assert_eq!(
            refused(sup.open_port_lock(&path)),
            "pidfile_path",
            "{}",
            path.display()
        );
        assert!(!path.exists(), "{} created", path.display());
    }
    assert_eq!(
        std::fs::read_dir(&servers).unwrap().count(),
        0,
        "a refused open created a file under servers/"
    );
    assert!(
        kid.0.try_wait().unwrap().is_none(),
        "a refused open signalled"
    );

    let valid = servers.join("4777.lock");
    let lock = sup
        .open_port_lock(&valid)
        .expect("open 4777.lock")
        .expect("4777.lock is free");
    assert!(take_lock(&root, 4777).is_none(), "the lock is not held");
    assert!(
        sup.open_port_lock(&valid)
            .expect("reopen 4777.lock")
            .is_none(),
        "a second open took a held lock"
    );
    drop(lock);
    assert!(valid.exists(), "the lock file was unlinked");
    assert!(take_lock(&root, 4777).is_some(), "dropping kept the lock");
}

/// A supervisor whose spawn is `real_spawn`; the returned list records each child's pid.
fn real_spawning(
    root: &Path,
    emit: impl Fn(ExitNotice) + Send + Sync + 'static,
) -> (Arc<Supervisor>, Arc<Mutex<Vec<libc::pid_t>>>) {
    let pids = Arc::new(Mutex::new(Vec::new()));
    let recorded = pids.clone();
    let sup = spawning(root, emit, move |server: &Server, log: &Path| {
        let child = real_spawn(server, log)?;
        recorded.lock().unwrap().push(child.id() as libc::pid_t);
        Ok(child)
    });
    (sup, pids)
}

fn lock_path(root: &Path, addr: SocketAddrV4) -> PathBuf {
    root.join("data")
        .join("servers")
        .join(format!("{}.lock", addr.port()))
}

fn lsof(pid: u32) -> String {
    let out = Command::new("lsof")
        .args(["-n", "-P", "-p", &pid.to_string()])
        .stdin(Stdio::null())
        .output()
        .expect("run lsof");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Kills an owned child's group and waits until its reaper has released the port lock.
fn kill_and_reap(
    root: &Path,
    addr: SocketAddrV4,
    pid: libc::pid_t,
    exits: &mpsc::Receiver<ExitNotice>,
) {
    proc::kill_group(pid);
    let n = exits
        .recv_timeout(Duration::from_secs(5))
        .expect("the reaper never reported the exit");
    assert!(n.owned, "an Owned server's exit");
    assert!(
        until(Duration::from_secs(5), || take_lock(root, addr.port())
            .is_some()),
        "the reaper never released the port lock"
    );
}

fn quitting() -> Result<Running, Refusal> {
    Err(Refusal::NotRunning {
        hint: "the app is quitting".into(),
    })
}

fn stop_owned_frees_the_port() {
    let root = scratch("stop-owned");
    let addr = free_addr();
    let (server, d) = managed(addr, std::env::current_exe().unwrap(), MARKER_A);
    let pids = Arc::new(Mutex::new(Vec::new()));
    let (emit, exits) = recording();
    let sup = spawning(&root, emit, fake_spawn("serve", 0, pids.clone()));
    assert_eq!(
        sup.ensure(Some(&server), &d),
        Ok(Running {
            url: d.url.clone(),
            owned: true,
        })
    );
    let pid = pids.lock().unwrap()[0];

    let started = Instant::now();
    sup.stop_owned();
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "the server needed the deadline SIGKILL"
    );
    let n = exits
        .recv_timeout(Duration::from_secs(1))
        .expect("the reaper never reported the exit");
    assert_eq!(n.status, "signal 15", "the group was sent SIGTERM");
    // SAFETY: WNOHANG never blocks; a reaped pid is no longer our child.
    let reaped = unsafe { libc::waitpid(pid, std::ptr::null_mut(), libc::WNOHANG) };
    assert_eq!(reaped, -1, "stop_owned returned before the reap");
    assert!(!proc::group_alive(pid), "the group outlived stop_owned");
    assert!(TcpListener::bind(addr).is_ok(), "the port is still bound");
    assert!(!pidfile_of(&root, addr).exists(), "a pidfile remains");
    assert!(
        take_lock(&root, addr.port()).is_some(),
        "the port lock is still held"
    );

    let again = Instant::now();
    sup.stop_owned();
    assert!(
        again.elapsed() < Duration::from_millis(100),
        "the second stop_owned waited"
    );
    assert!(
        exits.recv_timeout(Duration::from_millis(300)).is_err(),
        "the second stop_owned stopped something"
    );
    assert_eq!(pids.lock().unwrap().len(), 1);
}

fn quit_during_readiness_stops_the_group() {
    let root = scratch("quit-readiness");
    let addr = free_addr();
    let (server, d) = managed(addr, std::env::current_exe().unwrap(), MARKER_A);
    let pids = Arc::new(Mutex::new(Vec::new()));
    let (emit, exits) = recording();
    let sup = spawning(&root, emit, fake_spawn("idle", 0, pids.clone()));
    let pidfile = pidfile_of(&root, addr);

    let (tx, rx) = mpsc::channel();
    let threads = vec![open_on_thread(&sup, &server, &d, 0, &tx)];
    assert!(
        until(Duration::from_secs(5), || pidfile.exists()),
        "the spawn never wrote its pidfile"
    );
    let pid = pids.lock().unwrap()[0];
    let started = Instant::now();
    sup.stop_owned();
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "the group needed the deadline SIGKILL"
    );
    assert!(!proc::group_alive(pid), "the group outlived stop_owned");
    let n = exits
        .recv_timeout(Duration::from_secs(1))
        .expect("the reaper never reported the exit");
    assert_eq!(n.status, "signal 15", "the group was sent SIGTERM");
    assert!(!n.owned, "dashboard-exited for a Starting slot");

    let got = collect(&rx, threads, Duration::from_secs(5));
    assert!(got[0].is_err(), "a stopped start returned {:?}", got[0]);
    assert_eq!(pids.lock().unwrap().len(), 1);
    assert!(!pidfile.exists(), "a pidfile remains");
    assert!(
        take_lock(&root, addr.port()).is_some(),
        "the port lock is still held"
    );
}

fn quit_before_the_port_lock_is_taken() {
    let root = scratch("quit-reserved");
    let addr = free_addr();
    let (server, d) = managed(addr, std::env::current_exe().unwrap(), MARKER_A);
    let (gate_tx, gate_rx) = mpsc::channel::<()>();
    let gate = Mutex::new(gate_rx);
    let connects = Arc::new(AtomicUsize::new(0));
    let counted = connects.clone();
    let (sup, spawns) = counting(&root, move |_| {
        counted.fetch_add(1, Ordering::SeqCst);
        let _ = gate.lock().unwrap().recv();
        false
    });
    let sup = Arc::new(sup);

    let (tx, rx) = mpsc::channel();
    let threads = vec![open_on_thread(&sup, &server, &d, 0, &tx)];
    assert!(
        until(Duration::from_secs(2), || connects.load(Ordering::SeqCst)
            == 1),
        "the open never reached connect"
    );
    let started = Instant::now();
    sup.stop_owned();
    let took = started.elapsed();
    assert!(
        took >= Duration::from_millis(4900) && took < Duration::from_secs(6),
        "stop_owned returned after {took:?}, not at its 5 s deadline"
    );
    gate_tx.send(()).unwrap();

    let got = collect(&rx, threads, Duration::from_secs(5));
    assert_eq!(got[0], quitting());
    assert_eq!(spawns.load(Ordering::SeqCst), 0, "spawned during quit");
    assert!(!pidfile_of(&root, addr).exists(), "a pidfile was written");
    assert!(
        take_lock(&root, addr.port()).is_some(),
        "the port lock is held"
    );
}

fn open_after_quit_refuses_at_reserve() {
    let root = scratch("quit-reserve");
    let connects = Arc::new(AtomicUsize::new(0));
    let counted = connects.clone();
    let (sup, spawns) = counting(&root, move |_| {
        counted.fetch_add(1, Ordering::SeqCst);
        false
    });
    let addr = free_addr();
    let (server, d) = managed(addr, std::env::current_exe().unwrap(), MARKER_A);
    sup.stop_owned();

    assert_eq!(sup.ensure(Some(&server), &d), quitting());
    assert_eq!(sup.ensure(None, &unmanaged(free_addr())), quitting());
    assert_eq!(sup.ensure(Some(&server), &d), quitting());
    assert_eq!(connects.load(Ordering::SeqCst), 0, "probed during quit");
    assert_eq!(spawns.load(Ordering::SeqCst), 0, "spawned during quit");
    assert!(!pidfile_of(&root, addr).exists(), "a pidfile was written");
    assert!(
        take_lock(&root, addr.port()).is_some(),
        "the port lock is held"
    );
}

/// No unprivileged fake survives SIGKILL, so the Owned slot after quit began is one whose
/// server ignores SIGTERM, opened within the TERM grace.
fn open_on_owned_during_quit_refuses() {
    let root = scratch("quit-owned-open");
    let addr = free_addr();
    let (server, d) = managed(addr, std::env::current_exe().unwrap(), MARKER_A);
    let pids = Arc::new(Mutex::new(Vec::new()));
    let (emit, exits) = recording();
    let sup = spawning(
        &root,
        emit,
        fake_spawn("serve-ignoring-term", 0, pids.clone()),
    );
    assert_eq!(
        sup.ensure(Some(&server), &d),
        Ok(Running {
            url: d.url.clone(),
            owned: true,
        })
    );
    let pid = pids.lock().unwrap()[0];
    let probes = requests(&root, addr, "GET / HTTP/1.0");

    let started = Instant::now();
    let stopper = {
        let sup = sup.clone();
        std::thread::spawn(move || sup.stop_owned())
    };
    assert!(
        until(Duration::from_secs(2), || sup
            .ensure(None, &unmanaged(free_addr()))
            == quitting()),
        "quit never began"
    );
    assert!(proc::alive(pid), "the server died on SIGTERM");
    assert_eq!(sup.ensure(Some(&server), &d), quitting());
    assert!(
        started.elapsed() < Duration::from_millis(4500),
        "the open ran past the TERM grace: {:?}",
        started.elapsed()
    );
    assert_eq!(
        requests(&root, addr, "GET / HTTP/1.0"),
        probes,
        "an identity probe ran during quit"
    );

    stopper.join().unwrap();
    let n = exits
        .recv_timeout(Duration::from_secs(1))
        .expect("the reaper never reported the exit");
    assert_eq!(n.status, "signal 9", "the server died from the SIGKILL");
    assert!(n.owned, "the Owned server's exit");
    assert!(!proc::group_alive(pid), "the group outlived stop_owned");
    assert!(TcpListener::bind(addr).is_ok(), "the port is still bound");
    assert!(!pidfile_of(&root, addr).exists(), "a pidfile remains");
    assert!(
        take_lock(&root, addr.port()).is_some(),
        "the port lock is still held"
    );
    assert_eq!(sup.ensure(Some(&server), &d), quitting());
    assert_eq!(pids.lock().unwrap().len(), 1, "spawned during quit");
}

fn quit_signals_owned_before_waiting_on_a_probe() {
    let root = scratch("quit-owned-and-probing");
    let (owned_addr, probing_addr) = (free_addr(), free_addr());
    let (owned_server, owned_d) = managed(owned_addr, std::env::current_exe().unwrap(), MARKER_A);
    let (probing_server, probing_d) =
        managed(probing_addr, std::env::current_exe().unwrap(), MARKER_A);
    let (gate_tx, gate_rx) = mpsc::channel::<()>();
    let gate = Mutex::new(gate_rx);
    let connects = Arc::new(AtomicUsize::new(0));
    let counted = connects.clone();
    let pids = Arc::new(Mutex::new(Vec::new()));
    let (emit, exits) = recording();
    let sup = Arc::new(
        Supervisor::new(
            &root.join("data"),
            &root.join("logs"),
            emit,
            fake_spawn("serve", 0, pids.clone()),
            move |addr: SocketAddrV4| {
                if addr != probing_addr {
                    return real_connect(addr);
                }
                counted.fetch_add(1, Ordering::SeqCst);
                let _ = gate.lock().unwrap().recv();
                false
            },
        )
        .unwrap(),
    );
    assert_eq!(
        sup.ensure(Some(&owned_server), &owned_d),
        Ok(Running {
            url: owned_d.url.clone(),
            owned: true,
        })
    );
    let pid = pids.lock().unwrap()[0];

    let (tx, rx) = mpsc::channel();
    let threads = vec![open_on_thread(&sup, &probing_server, &probing_d, 0, &tx)];
    assert!(
        until(Duration::from_secs(2), || connects.load(Ordering::SeqCst)
            == 1),
        "the second open never reached connect"
    );

    let stopping = Arc::new(AtomicBool::new(true));
    let started = Instant::now();
    let stopper = {
        let (sup, stopping) = (sup.clone(), stopping.clone());
        std::thread::spawn(move || {
            sup.stop_owned();
            stopping.store(false, Ordering::SeqCst);
        })
    };
    let n = exits
        .recv_timeout(Duration::from_secs(2))
        .expect("the Owned server was not stopped while the probe blocked");
    assert!(
        stopping.load(Ordering::SeqCst),
        "stop_owned returned before the Owned server exited"
    );
    assert!(n.owned, "the Owned server's exit");
    assert_eq!(n.status, "signal 15", "the group was sent SIGTERM");
    assert!(
        until(Duration::from_secs(1), || !proc::group_alive(pid)),
        "the Owned group outlived its SIGTERM"
    );
    assert!(
        started.elapsed() < Duration::from_secs(4),
        "the Owned server stopped only after {:?}",
        started.elapsed()
    );

    stopper.join().unwrap();
    let took = started.elapsed();
    assert!(
        took >= Duration::from_millis(4900) && took < Duration::from_secs(7),
        "stop_owned returned after {took:?}, not at its deadline"
    );
    assert!(
        exits.recv_timeout(Duration::from_millis(200)).is_err(),
        "a second exit was reported"
    );
    assert!(
        TcpListener::bind(owned_addr).is_ok(),
        "the Owned port is still bound"
    );
    assert!(
        !pidfile_of(&root, owned_addr).exists(),
        "the Owned pidfile remains"
    );

    gate_tx.send(()).unwrap();
    let got = collect(&rx, threads, Duration::from_secs(5));
    assert_eq!(got[0], quitting());
    assert_eq!(pids.lock().unwrap().len(), 1, "spawned during quit");
    assert!(!pidfile_of(&root, probing_addr).exists());
}

fn quit_kills_a_member_and_spares_another_instance() {
    let root = scratch("quit-member");
    let addr = free_addr();
    let (server, d) = managed(addr, std::env::current_exe().unwrap(), MARKER_A);
    let pids = Arc::new(Mutex::new(Vec::new()));
    let (emit, exits) = recording();
    let first = spawning(
        &root,
        emit,
        fake_spawn("serve-with-member", 0, pids.clone()),
    );
    assert_eq!(
        first.ensure(Some(&server), &d),
        Ok(Running {
            url: d.url.clone(),
            owned: true,
        })
    );
    let pid = pids.lock().unwrap()[0];
    let pidfile = pidfile_of(&root, addr);

    let (second_emit, second_exits) = recording();
    let second_spawns = Arc::new(AtomicUsize::new(0));
    let counted = second_spawns.clone();
    let second = Supervisor::new(
        &root.join("data"),
        &root.join("logs"),
        second_emit,
        move |_: &Server, _: &Path| -> anyhow::Result<Child> {
            counted.fetch_add(1, Ordering::SeqCst);
            anyhow::bail!("no spawn in the second instance")
        },
        real_connect,
    )
    .unwrap();
    assert_eq!(
        second.ensure(Some(&server), &d),
        Ok(Running {
            url: d.url.clone(),
            owned: false,
        }),
        "the second instance did not embed the server"
    );
    second.stop_owned();
    assert!(
        exits.recv_timeout(Duration::from_millis(300)).is_err(),
        "the second instance's stop_owned stopped the server"
    );
    assert!(second_exits.try_recv().is_err());
    assert_eq!(second_spawns.load(Ordering::SeqCst), 0);
    assert!(proc::alive(pid), "the leader died");
    assert!(
        matched(&get(addr, "/", MARKER_A)),
        "the server stopped answering"
    );
    assert!(pidfile.exists(), "the first instance's pidfile is gone");

    let started = Instant::now();
    first.stop_owned();
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "stop_owned waited for its deadline: {:?}",
        started.elapsed()
    );
    let n = exits
        .recv_timeout(Duration::from_secs(1))
        .expect("the reaper never reported the exit");
    assert_eq!(n.status, "signal 15", "the leader exited on SIGTERM");
    assert!(
        !proc::group_alive(pid),
        "the SIGTERM-ignoring member outlived stop_owned"
    );
    assert!(TcpListener::bind(addr).is_ok(), "the port is still bound");
    assert!(!pidfile.exists(), "a pidfile remains");
    assert!(
        take_lock(&root, addr.port()).is_some(),
        "the port lock is still held"
    );
}

/// A spawn of `inner` that, once its child has started, reports the child's pid on the
/// returned receiver and waits for the returned sender before it returns.
fn gated_spawn(
    inner: impl Fn(&Server, &Path) -> anyhow::Result<Child> + Send + Sync + 'static,
) -> (
    impl Fn(&Server, &Path) -> anyhow::Result<Child> + Send + Sync + 'static,
    mpsc::Receiver<libc::pid_t>,
    mpsc::Sender<()>,
) {
    let (spawned_tx, spawned_rx) = mpsc::channel();
    let (gate_tx, gate_rx) = mpsc::channel::<()>();
    let (spawned_tx, gate_rx) = (Mutex::new(spawned_tx), Mutex::new(gate_rx));
    let spawn = move |server: &Server, log: &Path| {
        let child = inner(server, log)?;
        let _ = spawned_tx.lock().unwrap().send(child.id() as libc::pid_t);
        let _ = gate_rx.lock().unwrap().recv();
        Ok(child)
    };
    (spawn, spawned_rx, gate_tx)
}

/// Spawns `idle-blocking-term` into a group of its own with SIGTERM blocked from before
/// exec, so the child holds every SIGTERM pending until `pidfile` exists.
fn term_blocked_until(
    pidfile: PathBuf,
) -> impl Fn(&Server, &Path) -> anyhow::Result<Child> + Send + Sync + 'static {
    move |server: &Server, log: &Path| {
        let out = OpenOptions::new().create(true).append(true).open(log)?;
        let err = out.try_clone()?;
        let mut cmd = Command::new(std::env::current_exe()?);
        // SAFETY: the closure runs between fork and exec and only calls `sigprocmask`,
        // which is async-signal-safe.
        unsafe {
            cmd.pre_exec(|| {
                set_term_blocked(true);
                Ok(())
            });
        }
        Ok(cmd
            .args(["idle-blocking-term", &server.addr.to_string()])
            .env(PIDFILE_ENV, &pidfile)
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(out)
            .stderr(err)
            .spawn()?)
    }
}

fn quit_waits_for_a_spawn_to_be_recorded() {
    let root = scratch("quit-unrecorded");
    let addr = free_addr();
    let (server, d) = managed(addr, std::env::current_exe().unwrap(), MARKER_A);
    let (spawn, spawned, gate) = gated_spawn(fake_spawn("idle", 0, Arc::default()));
    let (emit, exits) = recording();
    let sup = spawning(&root, emit, spawn);

    let (tx, rx) = mpsc::channel();
    let threads = vec![open_on_thread(&sup, &server, &d, 0, &tx)];
    let pid = spawned
        .recv_timeout(Duration::from_secs(5))
        .expect("the spawn never started its child");
    let started = Instant::now();
    let stopper = {
        let sup = sup.clone();
        std::thread::spawn(move || sup.stop_owned())
    };
    std::thread::sleep(Duration::from_millis(500));
    assert!(
        !stopper.is_finished(),
        "stop_owned left an unrecorded spawn"
    );
    assert!(proc::alive(pid), "stop_owned signalled an unrecorded spawn");
    assert!(exits.try_recv().is_err(), "an exit before the record");
    gate.send(()).unwrap();

    stopper.join().unwrap();
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "the group needed the deadline SIGKILL"
    );
    let n = exits
        .recv_timeout(Duration::from_secs(1))
        .expect("the reaper never reported the exit");
    assert_eq!(n.status, "signal 15", "the record sent SIGTERM");
    assert!(!n.owned, "dashboard-exited for a Starting slot");
    assert!(!proc::group_alive(pid), "the group outlived stop_owned");
    let got = collect(&rx, threads, Duration::from_secs(5));
    assert!(got[0].is_err(), "a stopped start returned {:?}", got[0]);
    assert!(!pidfile_of(&root, addr).exists(), "a pidfile was kept");
    assert!(
        take_lock(&root, addr.port()).is_some(),
        "the port lock is still held"
    );
}

fn spawn_recorded_after_quit_returned() {
    let root = scratch("quit-then-record");
    let addr = free_addr();
    let (server, d) = managed(addr, std::env::current_exe().unwrap(), MARKER_A);
    // A child that died before spawn step 4 would leave the slot Exited there, and an
    // Exited slot fails to start; the child dies only once the pidfile exists.
    let (spawn, spawned, gate) = gated_spawn(term_blocked_until(pidfile_of(&root, addr)));
    let (emit, exits) = recording();
    let sup = spawning(&root, emit, spawn);

    let (tx, rx) = mpsc::channel();
    let threads = vec![open_on_thread(&sup, &server, &d, 0, &tx)];
    let pid = spawned
        .recv_timeout(Duration::from_secs(5))
        .expect("the spawn never started its child");
    sup.stop_owned();
    assert!(proc::alive(pid), "stop_owned signalled an unrecorded spawn");
    assert!(exits.try_recv().is_err(), "an exit before the record");
    gate.send(()).unwrap();

    let got = collect(&rx, threads, Duration::from_secs(10));
    assert_eq!(got[0], quitting());
    let n = exits
        .recv_timeout(Duration::from_secs(1))
        .expect("the reaper never reported the exit");
    assert_eq!(n.status, "signal 15", "the record sent SIGTERM");
    assert!(!n.owned, "dashboard-exited for a Starting slot");
    // SAFETY: WNOHANG never blocks; a reaped pid is no longer our child.
    let reaped = unsafe { libc::waitpid(pid, std::ptr::null_mut(), libc::WNOHANG) };
    assert_eq!(reaped, -1, "the epilogue returned before the reap");
    assert!(!proc::group_alive(pid), "the group outlived the epilogue");
    assert!(!pidfile_of(&root, addr).exists(), "a pidfile was kept");
    assert!(
        take_lock(&root, addr.port()).is_some(),
        "the port lock is still held"
    );
}

fn quit_while_a_reaper_finishes() {
    let root = scratch("quit-reaper");
    let addr = free_addr();
    let (server, control) = managed(addr, std::env::current_exe().unwrap(), MARKER_A);
    let (emit, exits) = delaying(Duration::from_millis(1000));
    let (sup, pids) = real_spawning(&root, emit);
    let pid = open_then_exit(&root, &server, &control, &sup, &pids, &exits);

    let started = Instant::now();
    sup.stop_owned();
    let took = started.elapsed();
    assert!(
        took < Duration::from_secs(5),
        "stop_owned waited for its deadline: {took:?}"
    );
    // SAFETY: WNOHANG never blocks; a reaped pid is no longer our child.
    let reaped = unsafe { libc::waitpid(pid, std::ptr::null_mut(), libc::WNOHANG) };
    assert_eq!(reaped, -1, "stop_owned returned before the reap");
    assert!(!proc::group_alive(pid), "the group outlived stop_owned");
    assert!(TcpListener::bind(addr).is_ok(), "the port is still bound");
    assert!(
        take_lock(&root, addr.port()).is_some(),
        "the port lock is still held"
    );
    assert!(!pidfile_of(&root, addr).exists(), "a pidfile remains");
    assert!(exits.try_recv().is_err(), "a second exit was reported");
    assert_eq!(pids.lock().unwrap().len(), 1);
}

fn quit_signals_a_leader_target_alone() {
    let root = scratch("quit-leader-target");
    let mut group = spawn_mode("idle", free_addr());
    let addr = free_addr();
    let (server, d) = managed(addr, std::env::current_exe().unwrap(), MARKER_A);
    let pids = Arc::new(Mutex::new(Vec::new()));
    let (emit, exits) = recording();
    let (recorded, joined) = (pids.clone(), group.pid());
    let sup = spawning(&root, emit, move |server: &Server, log: &Path| {
        let out = OpenOptions::new().create(true).append(true).open(log)?;
        let err = out.try_clone()?;
        let mut cmd = Command::new(std::env::current_exe()?);
        // SAFETY: the closure runs between fork and exec and only calls `signal`, which
        // is async-signal-safe. An ignored disposition survives exec, so the leader
        // ignores SIGTERM from its first instruction.
        unsafe {
            cmd.pre_exec(|| {
                libc::signal(libc::SIGTERM, libc::SIG_IGN);
                Ok(())
            });
        }
        let child = cmd
            .args(["idle", &server.addr.to_string()])
            .process_group(joined)
            .stdin(Stdio::null())
            .stdout(out)
            .stderr(err)
            .spawn()?;
        recorded.lock().unwrap().push(child.id() as libc::pid_t);
        Ok(child)
    });

    let (tx, rx) = mpsc::channel();
    let threads = vec![open_on_thread(&sup, &server, &d, 0, &tx)];
    assert!(
        until(Duration::from_secs(5), || !pids.lock().unwrap().is_empty()),
        "the spawn never started its child"
    );
    let opened = Instant::now();
    let pid = pids.lock().unwrap()[0];
    // The leader ignores the epilogue's SIGTERM, so its slot stays Starting with a
    // recorded spawn until the epilogue's SIGKILL at 5 s.
    std::thread::sleep(Duration::from_millis(500));
    assert!(proc::alive(pid), "the leader died before quit");
    assert!(exits.try_recv().is_err(), "an exit before quit");

    let started = Instant::now();
    sup.stop_owned();
    let took = started.elapsed();
    let n = exits
        .recv_timeout(Duration::from_secs(1))
        .expect("the reaper never reported the exit");
    assert_eq!(n.status, "signal 9", "the leader ignores SIGTERM");
    assert!(!n.owned, "dashboard-exited for a Starting slot");
    // SAFETY: WNOHANG never blocks; a reaped pid is no longer our child.
    let reaped = unsafe { libc::waitpid(pid, std::ptr::null_mut(), libc::WNOHANG) };
    assert_eq!(reaped, -1, "stop_owned returned before the reap");
    assert!(
        opened.elapsed() < Duration::from_millis(5500),
        "stop_owned returned {:?} after the open, past the leader's reap at 5 s",
        opened.elapsed()
    );
    assert!(
        took < Duration::from_millis(4900),
        "stop_owned spent its budget on the leader's group: {took:?}"
    );
    assert!(
        group.0.try_wait().unwrap().is_none(),
        "the group the leader joined was signalled"
    );
    let got = collect(&rx, threads, Duration::from_secs(5));
    log_tail_has(&got[0], "cos: spawn result refused: pgid");
    assert!(!pidfile_of(&root, addr).exists(), "a pidfile was written");
    assert!(
        take_lock(&root, addr.port()).is_some(),
        "the port lock is still held"
    );
}

/// The pid `serve-handling-term-with-member` prints to its server log.
fn member_of(root: &Path, addr: SocketAddrV4) -> libc::pid_t {
    let log = std::fs::read_to_string(log_of(root, addr)).unwrap();
    log.lines()
        .find_map(|l| l.strip_prefix("member ")?.parse().ok())
        .unwrap_or_else(|| panic!("no member pid in {log:?}"))
}

/// Whether `pid` has exited: gone, or a zombie whose stopped parent cannot reap it.
fn exited(pid: libc::pid_t) -> bool {
    const SZOMB: u32 = 5; // <sys/proc.h>
    // SAFETY: `proc_bsdinfo` is plain data; all-zero is a valid value.
    let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<libc::proc_bsdinfo>() as libc::c_int;
    // SAFETY: the buffer is a `proc_bsdinfo` of the size passed.
    let n = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTBSDINFO,
            0,
            (&mut info as *mut libc::proc_bsdinfo).cast(),
            size,
        )
    };
    n != size || info.pbi_status == SZOMB
}

fn quit_kills_a_stopped_server() {
    let root = scratch("quit-stopped");
    let addr = free_addr();
    let (server, d) = managed(addr, std::env::current_exe().unwrap(), MARKER_A);
    let pids = Arc::new(Mutex::new(Vec::new()));
    let (emit, exits) = recording();
    let sup = spawning(
        &root,
        emit,
        fake_spawn("serve-handling-term-with-member", 0, pids.clone()),
    );
    assert_eq!(
        sup.ensure(Some(&server), &d),
        Ok(Running {
            url: d.url.clone(),
            owned: true,
        })
    );
    let pid = pids.lock().unwrap()[0];
    let member = member_of(&root, addr);
    let pidfile = pidfile_of(&root, addr);
    proc::signal_pid(pid, libc::SIGSTOP);
    assert!(
        exits.recv_timeout(Duration::from_millis(500)).is_err(),
        "the reaper took the stop for an exit"
    );
    assert!(!exited(member), "the member died before quit");
    assert!(pidfile.exists(), "the pidfile of a stopped server is gone");
    assert!(
        take_lock(&root, addr.port()).is_none(),
        "the slot of a stopped server released its lock"
    );

    let started = Instant::now();
    let stopper = {
        let sup = sup.clone();
        std::thread::spawn(move || sup.stop_owned())
    };
    // The member is not stopped and keeps SIGTERM's default action, so its death shows
    // the group SIGTERM that the stopped leader holds pending.
    assert!(
        until(Duration::from_secs(2), || exited(member)),
        "stop_owned did not signal the stopped server's group"
    );
    std::thread::sleep(Duration::from_millis(4500).saturating_sub(started.elapsed()));
    assert!(
        proc::alive(pid),
        "the stopped leader acted on SIGTERM: {:?} after {:?}",
        exits.try_recv(),
        started.elapsed()
    );
    assert!(exits.try_recv().is_err(), "an exit before the deadline");
    assert!(pidfile.exists(), "the pidfile went before the reap");

    stopper.join().unwrap();
    let took = started.elapsed();
    assert!(
        took >= Duration::from_millis(4900) && took < Duration::from_secs(6),
        "stop_owned returned after {took:?}, not after the deadline SIGKILL"
    );
    let n = exits
        .recv_timeout(Duration::from_secs(1))
        .expect("the reaper never reported the exit");
    assert_eq!(
        n.status, "signal 9",
        "the leader died from the deadline SIGKILL"
    );
    assert!(n.owned, "the Owned server's exit");
    assert!(!proc::group_alive(pid), "the group outlived stop_owned");
    assert!(TcpListener::bind(addr).is_ok(), "the port is still bound");
    assert!(!pidfile.exists(), "a pidfile remains");
    assert!(
        take_lock(&root, addr.port()).is_some(),
        "the port lock is still held"
    );
}

fn port_lock_not_inherited() {
    let root = scratch("lock-cloexec");
    let addr = free_addr();
    let (server, d) = managed(addr, std::env::current_exe().unwrap(), MARKER_A);
    let (emit, exits) = recording();
    let (sup, pids) = real_spawning(&root, emit);

    assert_eq!(
        sup.ensure(Some(&server), &d),
        Ok(Running {
            url: d.url.clone(),
            owned: true
        })
    );
    let pid = pids.lock().unwrap()[0];
    let lock = lock_path(&root, addr);
    let lock_name = format!("{}.lock", addr.port());
    let log = root
        .join("logs")
        .join("servers")
        .join(format!("{}.log", addr.port()));

    let ours = lsof(std::process::id());
    assert!(
        ours.contains(lock.to_str().unwrap()),
        "lsof does not list the lock this process holds:\n{ours}"
    );
    let theirs = lsof(pid as u32);
    assert!(
        theirs.contains(log.to_str().unwrap()),
        "lsof did not list the child's log:\n{theirs}"
    );
    assert!(
        !theirs.contains(&lock_name),
        "the child inherited {lock_name}:\n{theirs}"
    );

    kill_and_reap(&root, addr, pid, &exits);
}

fn port_lock_keeps_its_inode() {
    use std::os::unix::fs::MetadataExt;

    let root = scratch("lock-inode");
    let addr = free_addr();
    let (server, d) = managed(addr, std::env::current_exe().unwrap(), MARKER_A);
    let (emit, exits) = recording();
    let (sup, pids) = real_spawning(&root, emit);
    let lock = lock_path(&root, addr);
    let ino = || std::fs::metadata(&lock).expect("the lock file").ino();

    drop(
        sup.open_port_lock(&lock)
            .expect("open the lock")
            .expect("the lock is free"),
    );
    let first = ino();
    let want = Ok(Running {
        url: d.url.clone(),
        owned: true,
    });

    for round in 0..2 {
        assert_eq!(sup.ensure(Some(&server), &d), want, "spawn {round}");
        assert_eq!(ino(), first, "inode changed by spawn {round}");
        let pid = pids.lock().unwrap()[round];
        kill_and_reap(&root, addr, pid, &exits);
        assert_eq!(ino(), first, "inode changed by reap {round}");
    }
    assert_eq!(pids.lock().unwrap().len(), 2);
}

fn second_supervisor_locks_after_the_reap() {
    let root = scratch("lock-second");
    let addr = free_addr();
    let (server, d) = managed(addr, std::env::current_exe().unwrap(), MARKER_A);
    let (exit_tx, exits) = mpsc::channel::<ExitNotice>();
    let exit_tx = Mutex::new(exit_tx);
    let (first, first_pids) = real_spawning(&root, move |n: ExitNotice| {
        let _ = exit_tx.lock().unwrap().send(n);
        std::thread::sleep(Duration::from_millis(500));
    });
    let (emit, second_exits) = recording();
    let (second, second_pids) = real_spawning(&root, emit);
    let lock = lock_path(&root, addr);
    let want = Ok(Running {
        url: d.url.clone(),
        owned: true,
    });

    assert_eq!(first.ensure(Some(&server), &d), want);
    let pid = first_pids.lock().unwrap()[0];
    assert!(
        second.open_port_lock(&lock).expect("open").is_none(),
        "the second supervisor took the lock of a live owned child"
    );

    proc::kill_group(pid);
    let n = exits
        .recv_timeout(Duration::from_secs(5))
        .expect("the reaper never reported the exit");
    assert!(n.owned);
    // The reaper sleeps in `emit` before `waitpid`: the slot is Exited, the child unreaped.
    assert!(
        second.open_port_lock(&lock).expect("open").is_none(),
        "the second supervisor took the lock before the reap"
    );
    assert_eq!(
        second.ensure(Some(&server), &d),
        Err(Refusal::NotRunning {
            hint: "another app instance is starting it; open the dashboard again".into(),
        })
    );
    assert!(
        second_pids.lock().unwrap().is_empty(),
        "spawned before the reap"
    );

    assert!(
        until(Duration::from_secs(5), || second
            .open_port_lock(&lock)
            .expect("open")
            .is_some()),
        "the reaper never released the port lock"
    );
    // SAFETY: WNOHANG never blocks; a reaped pid is no longer our child.
    let reaped = unsafe { libc::waitpid(pid, std::ptr::null_mut(), libc::WNOHANG) };
    assert_eq!(reaped, -1, "the lock was free before the reap");

    assert_eq!(second.ensure(Some(&server), &d), want);
    let pids = second_pids.lock().unwrap().clone();
    assert_eq!(pids.len(), 1, "the second supervisor started one server");
    assert!(matched(&get(addr, "/", MARKER_A)));
    assert!(
        take_lock(&root, addr.port()).is_none(),
        "the second supervisor holds no lock"
    );
    kill_and_reap(&root, addr, pids[0], &second_exits);
}

/// The second dashboard on `addr`'s server: `/sales`, probed for marker B.
fn salesroom(addr: SocketAddrV4) -> Dashboard {
    Dashboard {
        name: "Salesroom".into(),
        url: format!("http://{addr}/sales"),
        port: addr.port(),
        server: Some("fake".into()),
        probe: Probe {
            path: "/sales".into(),
            contains: MARKER_B.into(),
        },
    }
}

fn log_of(root: &Path, addr: SocketAddrV4) -> PathBuf {
    root.join("logs")
        .join("servers")
        .join(format!("{}.log", addr.port()))
}

/// How many times the server log records `request` as a request line.
fn requests(root: &Path, addr: SocketAddrV4, request: &str) -> usize {
    std::fs::read_to_string(log_of(root, addr))
        .unwrap_or_default()
        .lines()
        .filter(|l| *l == request)
        .count()
}

fn control_room_then_salesroom_share_one_server() {
    let root = scratch("owned-reuse");
    let addr = free_addr();
    let (server, control) = managed(addr, std::env::current_exe().unwrap(), MARKER_A);
    let sales = salesroom(addr);
    let (emit, exits) = recording();
    let (sup, pids) = real_spawning(&root, emit);

    assert_eq!(
        sup.ensure(Some(&server), &control),
        Ok(Running {
            url: control.url.clone(),
            owned: true
        })
    );
    assert_eq!(requests(&root, addr, "GET /sales HTTP/1.0"), 0);
    assert_eq!(
        sup.ensure(Some(&server), &sales),
        Ok(Running {
            url: sales.url.clone(),
            owned: true
        })
    );
    let pids = pids.lock().unwrap().clone();
    assert_eq!(pids.len(), 1, "one process for two dashboards");
    assert_eq!(
        requests(&root, addr, "GET /sales HTTP/1.0"),
        1,
        "the Salesroom ran its own identity probe"
    );
    kill_and_reap(&root, addr, pids[0], &exits);
}

fn owned_slot_refuses_a_marker_it_never_serves() {
    let root = scratch("owned-foreign-marker");
    let addr = free_addr();
    let exe = std::env::current_exe().unwrap();
    let (server, control) = managed(addr, exe.clone(), MARKER_A);
    let (_, never) = managed(addr, exe, "<title>Never served");
    let (emit, exits) = recording();
    let (sup, pids) = real_spawning(&root, emit);
    let want = Ok(Running {
        url: control.url.clone(),
        owned: true,
    });

    assert_eq!(sup.ensure(Some(&server), &control), want);
    match sup.ensure(Some(&server), &never) {
        Err(Refusal::Identity {
            status,
            head,
            owned: true,
            spawned: false,
        }) => {
            assert_eq!(status, "HTTP/1.0 200 OK");
            assert!(head.starts_with("HTTP/1.0 200 OK"), "{head}");
        }
        other => panic!("a marker the server never serves: {other:?}"),
    }
    let pid = pids.lock().unwrap()[0];
    assert!(
        exits.recv_timeout(Duration::from_millis(300)).is_err(),
        "the Owned server exited"
    );
    assert!(proc::alive(pid), "the Owned server was signalled");
    assert!(
        take_lock(&root, addr.port()).is_none(),
        "the Owned slot gave up its port lock"
    );
    assert!(pidfile_of(&root, addr).exists(), "the pidfile was removed");
    assert_eq!(
        sup.ensure(Some(&server), &control),
        want,
        "the first dashboard"
    );
    assert_eq!(pids.lock().unwrap().len(), 1, "a second spawn");
    kill_and_reap(&root, addr, pid, &exits);
}

fn success_wakes_a_waiter_with_its_own_probe() {
    let root = scratch("owned-waiter");
    let addr = free_addr();
    let (server, control) = managed(addr, std::env::current_exe().unwrap(), MARKER_A);
    let sales = salesroom(addr);
    let (gate_tx, gate_rx) = mpsc::channel::<()>();
    let gate = Mutex::new(gate_rx);
    let calls = Arc::new(AtomicUsize::new(0));
    let pids = Arc::new(Mutex::new(Vec::<libc::pid_t>::new()));
    let (counted, recorded) = (calls.clone(), pids.clone());
    let (emit, exits) = recording();
    let sup = spawning(&root, emit, move |server: &Server, log: &Path| {
        counted.fetch_add(1, Ordering::SeqCst);
        let _ = gate.lock().unwrap().recv();
        let child = real_spawn(server, log)?;
        recorded.lock().unwrap().push(child.id() as libc::pid_t);
        Ok(child)
    });

    let (tx, rx) = mpsc::channel();
    let mut threads = vec![open_on_thread(&sup, &server, &control, 0, &tx)];
    assert!(
        until(Duration::from_secs(5), || calls.load(Ordering::SeqCst) == 1),
        "the first open never reached the spawn"
    );
    threads.push(open_on_thread(&sup, &server, &sales, 1, &tx));
    assert!(
        rx.recv_timeout(Duration::from_millis(300)).is_err(),
        "an open returned while the slot was Starting"
    );
    gate_tx.send(()).unwrap();
    let got = collect(&rx, threads, Duration::from_secs(20));

    assert_eq!(
        got[0],
        Ok(Running {
            url: control.url.clone(),
            owned: true
        })
    );
    assert_eq!(
        got[1],
        Ok(Running {
            url: sales.url.clone(),
            owned: true
        }),
        "the waiter's own URL"
    );
    let pids = pids.lock().unwrap().clone();
    assert_eq!(pids.len(), 1, "the waiter spawned");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(requests(&root, addr, "GET / HTTP/1.0"), 1);
    assert_eq!(
        requests(&root, addr, "GET /sales HTTP/1.0"),
        1,
        "the waiter ran its own identity probe"
    );
    kill_and_reap(&root, addr, pids[0], &exits);
}

/// An `emit` that forwards every notice, then sleeps `delay` after the first owned one.
fn delaying(
    delay: Duration,
) -> (
    impl Fn(ExitNotice) + Send + Sync + 'static,
    mpsc::Receiver<ExitNotice>,
) {
    let (tx, rx) = mpsc::channel();
    let tx = Mutex::new(tx);
    let delayed = AtomicBool::new(false);
    (
        move |n: ExitNotice| {
            let owned = n.owned;
            let _ = tx.lock().unwrap().send(n);
            if owned && !delayed.swap(true, Ordering::SeqCst) {
                std::thread::sleep(delay);
            }
        },
        rx,
    )
}

/// Opens the Control room, kills its group and returns once the reaper has emitted the
/// exit, while `emit` still delays the reap.
fn open_then_exit(
    root: &Path,
    server: &Server,
    control: &Dashboard,
    sup: &Supervisor,
    pids: &Mutex<Vec<libc::pid_t>>,
    exits: &mpsc::Receiver<ExitNotice>,
) -> libc::pid_t {
    assert_eq!(
        sup.ensure(Some(server), control),
        Ok(Running {
            url: control.url.clone(),
            owned: true
        })
    );
    let pid = pids.lock().unwrap()[0];
    proc::kill_group(pid);
    let n = exits
        .recv_timeout(Duration::from_secs(5))
        .expect("the reaper never reported the exit");
    assert!(n.owned, "an Owned server's exit");
    assert!(
        take_lock(root, server.addr.port()).is_none(),
        "the reaper released the lock before the delayed reap"
    );
    pid
}

fn reopen_after_owned_exit_waits_for_the_reaper() {
    let root = scratch("owned-reopen");
    let addr = free_addr();
    let (server, control) = managed(addr, std::env::current_exe().unwrap(), MARKER_A);
    let (emit, exits) = delaying(Duration::from_millis(200));
    let (sup, pids) = real_spawning(&root, emit);
    let first = open_then_exit(&root, &server, &control, &sup, &pids, &exits);

    assert_eq!(
        sup.ensure(Some(&server), &control),
        Ok(Running {
            url: control.url.clone(),
            owned: true
        }),
        "the reopen while the reaper still holds the lock"
    );
    let pids = pids.lock().unwrap().clone();
    assert_eq!(pids.len(), 2, "one fresh server");
    // SAFETY: WNOHANG never blocks; a reaped pid is no longer our child.
    let reaped = unsafe { libc::waitpid(first, std::ptr::null_mut(), libc::WNOHANG) };
    assert_eq!(reaped, -1, "the reaper never reaped the first server");
    kill_and_reap(&root, addr, pids[1], &exits);
}

fn reopen_after_owned_exit_refuses_at_the_deadline() {
    let root = scratch("owned-reopen-deadline");
    let addr = free_addr();
    let (server, control) = managed(addr, std::env::current_exe().unwrap(), MARKER_A);
    let (emit, exits) = delaying(Duration::from_millis(6500));
    let (sup, pids) = real_spawning(&root, emit);
    open_then_exit(&root, &server, &control, &sup, &pids, &exits);

    let started = Instant::now();
    let r = sup.ensure(Some(&server), &control);
    let waited = started.elapsed();
    assert_eq!(
        r,
        Err(Refusal::NotRunning {
            hint: "the previous server on this port is still exiting; open the dashboard again"
                .into(),
        })
    );
    assert!(
        waited >= Duration::from_millis(4900) && waited < Duration::from_millis(6000),
        "refused after {waited:?}, not at the 5 s deadline"
    );
    assert_eq!(pids.lock().unwrap().len(), 1, "spawned at the deadline");
    assert!(
        take_lock(&root, addr.port()).is_none(),
        "the reaper released the lock before the delayed reap"
    );

    // A dropped slot would reach lock-take while the reaper still holds the lock and
    // refuse as "another app instance"; the kept Exited slot is waited on instead.
    assert_eq!(
        sup.ensure(Some(&server), &control),
        Ok(Running {
            url: control.url.clone(),
            owned: true
        }),
        "the reopen after the reap"
    );
    let pids = pids.lock().unwrap().clone();
    assert_eq!(pids.len(), 2, "one fresh server");
    kill_and_reap(&root, addr, pids[1], &exits);
}

/// `dashboards.toml` declaring one server, running `binary` on `addr`.
fn declaring(addr: SocketAddrV4, binary: PathBuf) -> Dashboards {
    let (server, _) = managed(addr, binary, MARKER_A);
    Dashboards {
        servers: BTreeMap::from([("fake".to_owned(), server)]),
        businesses: BTreeMap::new(),
    }
}

/// Writes `<root>/data/servers/<name>` as `pid`'s record, read from the kernel, so it
/// passes the four kernel checks; `addr` is written as given.
fn plant_record(root: &Path, name: &str, pid: libc::pid_t, addr: SocketAddrV4) -> PathBuf {
    let file = root.join("data").join("servers").join(name);
    let (secs, micros) = proc::start_time(pid).expect("start time");
    let record = json!({
        "pid": pid,
        "pgid": proc::pgid(pid).expect("pgid"),
        "start_time": [secs, micros],
        "exe": proc::exe(pid).expect("exe"),
        "addr": addr.to_string(),
    });
    std::fs::write(&file, serde_json::to_vec(&record).unwrap()).unwrap();
    file
}

/// An idle leftover dies on SIGTERM or SIGKILL and launchd reaps it, so a signal shows
/// as the pid going away.
fn assert_unsignalled(pid: libc::pid_t, case: &str) {
    assert!(
        !until(Duration::from_millis(300), || !proc::alive(pid)),
        "{case}: the leftover was signalled"
    );
}

fn cleanup_removes_an_undeclared_leftover() {
    let root = scratch("cleanup-undeclared");
    let sup = supervisor(&root.join("data"), &root.join("logs")).unwrap();
    let addr = free_addr();
    let other = SocketAddrV4::new(Ipv4Addr::LOCALHOST, addr.port() % 60000 + 1);
    let left = leftover("idle", addr);
    let name = format!("{}.json", addr.port());

    for (case, dashboards) in [
        (
            "no declared server has its port",
            declaring(other, std::env::current_exe().unwrap()),
        ),
        ("empty value", Dashboards::default()),
    ] {
        let file = plant_record(&root, &name, left.0, addr);
        sup.launch_cleanup(&dashboards);
        assert!(!file.exists(), "{case}: the pidfile was kept");
        assert_unsignalled(left.0, case);
    }
    assert!(
        take_lock(&root, addr.port()).is_some(),
        "cleanup kept the port lock"
    );
}

fn cleanup_leaves_a_held_lock() {
    let root = scratch("cleanup-held");
    let sup = supervisor(&root.join("data"), &root.join("logs")).unwrap();
    let addr = free_addr();
    let left = leftover("idle", addr);
    let held = take_lock(&root, addr.port()).expect("the lock is free");
    let file = plant_record(&root, &format!("{}.json", addr.port()), left.0, addr);
    let before = std::fs::read(&file).unwrap();

    for (case, dashboards) in [
        (
            "declared",
            declaring(addr, std::env::current_exe().unwrap()),
        ),
        ("empty value", Dashboards::default()),
    ] {
        sup.launch_cleanup(&dashboards);
        assert_eq!(
            std::fs::read(&file).ok(),
            Some(before.clone()),
            "{case}: the pidfile of a held lock changed"
        );
        assert_unsignalled(left.0, case);
    }
    drop(held);
}

fn forged_pidfile_is_overwritten_by_the_spawn() {
    let root = scratch("forged-pidfile");
    let addr = free_addr();
    let exe = std::env::current_exe().unwrap();
    let (server, control) = managed(addr, exe.clone(), MARKER_A);
    let (emit, exits) = recording();
    let (sup, pids) = real_spawning(&root, emit);
    sup.launch_cleanup(&declaring(addr, exe));

    let left = leftover("idle", addr);
    let forged = SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 2), addr.port());
    let file = plant_record(&root, &format!("{}.json", addr.port()), left.0, forged);

    assert_eq!(
        sup.ensure(Some(&server), &control),
        Ok(Running {
            url: control.url.clone(),
            owned: true
        })
    );
    let pid = pids.lock().unwrap()[0];
    let got: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    let (secs, micros) = proc::start_time(pid).expect("start time");
    assert_eq!(
        got,
        json!({
            "pid": pid,
            "pgid": pid,
            "start_time": [secs, micros],
            "exe": proc::exe(pid).expect("exe"),
            "addr": addr.to_string(),
        })
    );
    assert_unsignalled(left.0, "the forged pidfile's process");
    kill_and_reap(&root, addr, pid, &exits);
}

fn cleanup_ignores_non_canonical_names() {
    let root = scratch("cleanup-names");
    let sup = supervisor(&root.join("data"), &root.join("logs")).unwrap();
    let servers = root.join("data").join("servers");
    let left = leftover("idle", free_addr());

    for (name, port) in [("04777.json", 4777), ("0.json", 0)] {
        let addr = SocketAddrV4::new(Ipv4Addr::LOCALHOST, port);
        let file = plant_record(&root, name, left.0, addr);
        sup.launch_cleanup(&Dashboards::default());
        assert!(file.exists(), "{name} was removed");
        for lock in [format!("{port}.lock"), name.replace(".json", ".lock")] {
            assert!(
                !servers.join(&lock).exists(),
                "{name}: cleanup opened {lock}"
            );
        }
        assert_unsignalled(left.0, name);
    }
}

fn cleanup_keeps_the_lock_inode() {
    use std::os::unix::fs::MetadataExt;

    let root = scratch("cleanup-inode");
    let sup = supervisor(&root.join("data"), &root.join("logs")).unwrap();
    let servers = root.join("data").join("servers");
    let left = leftover("idle", free_addr());
    let addr: SocketAddrV4 = "127.0.0.1:4777".parse().unwrap();
    let lock = servers.join("4777.lock");
    drop(
        sup.open_port_lock(&lock)
            .expect("open the lock")
            .expect("the lock is free"),
    );
    let ino = || std::fs::metadata(&lock).expect("the lock file").ino();
    let first = ino();
    let live = plant_record(&root, "4777.json", left.0, addr);
    let ignored: Vec<PathBuf> = ["abc.json", "70000.json"]
        .iter()
        .map(|n| {
            let p = servers.join(n);
            std::fs::copy(&live, &p).unwrap();
            p
        })
        .collect();

    sup.launch_cleanup(&Dashboards::default());
    assert!(!live.exists(), "the Stale 4777.json was kept");
    assert_eq!(ino(), first, "launch cleanup changed the lock inode");
    for p in &ignored {
        assert!(p.exists(), "{} was handled", p.display());
    }
    for name in ["abc.lock", "70000.lock"] {
        assert!(!servers.join(name).exists(), "cleanup created {name}");
    }
    assert_unsignalled(left.0, "abc.json and 70000.json");
    assert!(take_lock(&root, 4777).is_some(), "cleanup kept 4777.lock");
}

fn cleanup_removes_a_record_for_another_port() {
    let root = scratch("cleanup-addr");
    let sup = supervisor(&root.join("data"), &root.join("logs")).unwrap();
    let left = leftover("idle", free_addr());
    let addr: SocketAddrV4 = "127.0.0.1:4777".parse().unwrap();
    let declared = declaring(addr, std::env::current_exe().unwrap());

    let file = plant_record(
        &root,
        "4777.json",
        left.0,
        "127.0.0.1:4778".parse().unwrap(),
    );
    sup.launch_cleanup(&declared);
    assert!(
        !file.exists(),
        "a record for port 4778 in 4777.json was kept"
    );
    assert_unsignalled(left.0, "addr port 4778");

    std::fs::write(&file, "not a record").unwrap();
    sup.launch_cleanup(&declared);
    assert!(!file.exists(), "an unparseable record was kept");
}

fn cleanup_removes_a_dead_pid() {
    let root = scratch("cleanup-dead");
    let sup = supervisor(&root.join("data"), &root.join("logs")).unwrap();
    let addr = free_addr();
    let left = leftover("idle", addr);
    let pid = left.0;
    let file = plant_record(&root, &format!("{}.json", addr.port()), pid, addr);
    drop(left);
    assert!(
        until(Duration::from_secs(2), || !proc::alive(pid)),
        "the leftover survived SIGKILL"
    );

    sup.launch_cleanup(&declaring(addr, std::env::current_exe().unwrap()));
    assert!(!file.exists(), "a dead pid's pidfile was kept");
    assert!(
        take_lock(&root, addr.port()).is_some(),
        "cleanup kept the port lock"
    );
}

/// Each case differs from a record passing all five checks in one field, so only the
/// check under test can remove it.
fn cleanup_removes_a_mismatching_leftover() {
    let root = scratch("cleanup-mismatch");
    let sup = supervisor(&root.join("data"), &root.join("logs")).unwrap();
    let addr = free_addr();
    let declared = declaring(addr, std::env::current_exe().unwrap());
    let name = format!("{}.json", addr.port());
    let left = leftover("idle", addr);

    for case in ["start time", "executable", "record pgid"] {
        let file = plant_record(&root, &name, left.0, addr);
        let mut record: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
        match case {
            "start time" => {
                record["start_time"][0] = json!(record["start_time"][0].as_i64().unwrap() - 1)
            }
            "executable" => record["exe"] = json!("/bin/sleep"),
            _ => record["pgid"] = json!(left.0 + 1),
        }
        std::fs::write(&file, serde_json::to_vec(&record).unwrap()).unwrap();
        sup.launch_cleanup(&declared);
        assert!(!file.exists(), "{case}: the pidfile was kept");
        assert_unsignalled(left.0, case);
    }

    let member = leftover_member("idle", addr);
    assert_ne!(
        proc::pgid(member.0),
        Some(member.0),
        "the member leads a group"
    );
    let file = plant_record(&root, &name, member.0, addr);
    let mut record: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    record["pgid"] = json!(member.0);
    std::fs::write(&file, serde_json::to_vec(&record).unwrap()).unwrap();
    sup.launch_cleanup(&declared);
    assert!(!file.exists(), "process group: the pidfile was kept");
    assert_unsignalled(member.0, "process group");
    assert_unsignalled(left.0, "process group");
}

fn cleanup_removes_a_leftover_of_another_binary() {
    let root = scratch("cleanup-binary");
    let sup = supervisor(&root.join("data"), &root.join("logs")).unwrap();
    let addr = free_addr();
    let other = root.join("other-server");
    std::fs::write(&other, "#!/bin/sh\n").unwrap();
    std::fs::set_permissions(&other, std::fs::Permissions::from_mode(0o755)).unwrap();
    let left = leftover("idle", addr);

    let file = plant_record(&root, &format!("{}.json", addr.port()), left.0, addr);
    sup.launch_cleanup(&declaring(addr, other));
    assert!(!file.exists(), "the pidfile of another binary was kept");
    assert_unsignalled(left.0, "declared binary is another file");
}

/// The leftover runs the test binary, declared for its port, and dies on SIGTERM, so
/// cleanup returning well before the 5 s deadline means the SIGTERM reached it.
fn cleanup_stops_a_matching_leftover() {
    let root = scratch("cleanup-match");
    let sup = supervisor(&root.join("data"), &root.join("logs")).unwrap();
    let addr = free_addr();
    let left = leftover("idle", addr);
    let pid = left.0;
    assert_eq!(proc::pgid(pid), Some(pid), "the leftover leads its group");
    let file = plant_record(&root, &format!("{}.json", addr.port()), pid, addr);

    let began = Instant::now();
    sup.launch_cleanup(&declaring(addr, std::env::current_exe().unwrap()));
    assert!(
        began.elapsed() < Duration::from_secs(4),
        "cleanup waited for the deadline: {:?}",
        began.elapsed()
    );
    assert!(!file.exists(), "the pidfile of a stopped leftover was kept");
    assert!(
        until(Duration::from_secs(1), || !proc::group_alive(pid)),
        "the leftover's group survived cleanup"
    );
    assert!(
        take_lock(&root, addr.port()).is_some(),
        "cleanup kept the port lock"
    );
}

/// The member ignores SIGTERM, so only the group SIGKILL after the leader's exit can
/// stop it; cleanup does not wait for the group, so the test polls for it.
fn cleanup_kills_a_leftover_member() {
    let root = scratch("cleanup-member");
    let sup = supervisor(&root.join("data"), &root.join("logs")).unwrap();
    let addr = free_addr();
    let left = leftover("serve-with-member", addr);
    let pid = left.0;
    assert!(
        until(Duration::from_secs(5), || matched(&get(
            addr, "/health", READY
        ))),
        "the leftover never became ready"
    );
    let file = plant_record(&root, &format!("{}.json", addr.port()), pid, addr);

    let began = Instant::now();
    sup.launch_cleanup(&declaring(addr, std::env::current_exe().unwrap()));
    assert!(
        began.elapsed() < Duration::from_secs(5),
        "cleanup reached the deadline: {:?}",
        began.elapsed()
    );
    assert!(!file.exists(), "the pidfile of a stopped leftover was kept");
    assert!(
        until(Duration::from_secs(1), || !proc::group_alive(pid)
            && get(addr, "/health", READY) == ProbeResult::NoListener),
        "the group or the listener outlived cleanup"
    );
}
