//! The dashboard server supervisor, against real child processes. The test
//! binary doubles as the fake server: `<self> serve <addr>`,
//! `<self> serve-with-member <addr>` and `<self> idle <addr>` run a mode instead
//! of the tests, and `<self> leftover <mode> <addr>` starts a mode through an
//! intermediate process that exits, as an earlier app's server is left after a
//! crash. macOS only: elsewhere every spawn is refused, so `main` returns.

use std::collections::BTreeSet;
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
use cos::dashboards::{Dashboard, Probe, ProbeResult, Server};
use cos::proc;
use cos_desktop::Start;
use cos_desktop::servers::{
    ExitNotice, Ownership, Refusal, Running, SpawnResult, StoreError, Supervisor,
    binary_admissible, real_connect, real_spawn,
};
use serde_json::{Value, json};
use tauri::test::{mock_builder, mock_context, noop_assets};
use tauri::{Listener, Manager};

const READY: &str = r#""status":"ok""#;
const MARKER_A: &str = "<title>Fake dashboard A";
const MARKER_B: &str = r#"<script src="/fake-b.js""#;

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
        "server_log_truncated_before_spawn",
        server_log_truncated_before_spawn,
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
                    fork_member();
                    serve(addr)
                }
                "idle" => idle(),
                _ => {}
            }
        }
        [leftover, mode, addr] if leftover == "leftover" => detach(mode, addr),
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

/// Forks one process that stays in this process's group and ignores SIGTERM. It runs
/// before `bind`, so it holds no listener.
fn fork_member() {
    // SAFETY: the mode runs before any thread is started, so the child of `fork` is a
    // single-threaded copy; it only changes a signal disposition and sleeps.
    let pid = unsafe { libc::fork() };
    if pid == 0 {
        // SAFETY: setting a disposition to SIG_IGN touches no memory.
        unsafe {
            libc::signal(libc::SIGTERM, libc::SIG_IGN);
        }
        idle();
    }
}

fn idle() -> ! {
    loop {
        std::thread::sleep(Duration::from_secs(3600));
    }
}

/// The intermediate of a leftover: starts `<self> <mode> <addr>` as the leader of its own
/// group, prints its pid and exits without waiting, so the leader is reparented away.
fn detach(mode: &str, addr: &str) -> ! {
    let child = Command::new(std::env::current_exe().unwrap())
        .args([mode, addr])
        .process_group(0)
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

fn refused(r: Result<(), StoreError>) -> &'static str {
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
