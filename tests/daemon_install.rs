#![cfg(feature = "stand-in")]
//! `cos daemon install` / `uninstall` against a temp `LaunchAgents`
//! (`COS_LAUNCH_AGENTS`) and the stand-in as `launchctl` (`COS_LAUNCHCTL`,
//! a symlink named `launchctl`), which records each call's argv. Nothing
//! here reaches the real launchd or `~/Library`.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use cos::proc::{self, Deadline};
use cos::runs::{self, State};
use serde_json::{Value, json};

const COS: &str = env!("CARGO_BIN_EXE_cos");
const STAND_IN: &str = env!("CARGO_BIN_EXE_cos-stand-in");
const TASKFILE: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/Taskfile.yml"));
const BOUND: Duration = Duration::from_secs(60);

struct Env {
    root: PathBuf,
    registry: PathBuf,
    home: PathBuf,
}

/// Short names: the socket path under `root` must fit `sun_path`.
fn env(name: &str) -> Env {
    let root = std::env::temp_dir().join(format!("cosi-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("bin")).unwrap();
    let root = root.canonicalize().unwrap();
    std::os::unix::fs::symlink(STAND_IN, root.join("bin/launchctl")).unwrap();
    let home = root.join("acme");
    std::fs::create_dir_all(home.join("projects")).unwrap();
    let registry = root.join("businesses.toml");
    std::fs::write(
        &registry,
        format!(
            "[business.acme]\nhome = {home:?}\nworkspace = {home:?}\nprojects = {:?}\n",
            home.join("projects")
        ),
    )
    .unwrap();
    Env {
        root,
        registry,
        home,
    }
}

impl Env {
    fn agents(&self) -> PathBuf {
        self.root.join("LaunchAgents")
    }

    fn plist(&self) -> PathBuf {
        self.agents().join("cos.daemon.plist")
    }

    fn log(&self) -> String {
        std::fs::read_to_string(self.root.join("launchctl.log")).unwrap_or_default()
    }

    fn cmd(&self, args: &[&str]) -> Command {
        let mut cmd = Command::new(COS);
        cmd.args(args)
            .env("HOME", &self.root)
            .env("COS_LAUNCH_AGENTS", self.agents())
            .env("COS_LAUNCHCTL", self.root.join("bin/launchctl"))
            .env("COS_STAND_IN_LOG", self.root.join("launchctl.log"))
            .env("COS_BUSINESSES", &self.registry)
            .env("COS_DAEMON_DIR", self.root.join("d"))
            .env_remove("COS_STAND_IN_FAIL")
            .env_remove("COS_BUSINESS")
            .env_remove("COS_HOME")
            .env_remove("COS_WORKSPACE")
            .env_remove("COS_PROJECTS");
        cmd
    }

    fn run(&self, args: &[&str], extra: &[(&str, &str)]) -> proc::Output {
        let mut cmd = self.cmd(args);
        for (k, v) in extra {
            cmd.env(k, v);
        }
        proc::run(&mut cmd, BOUND).unwrap()
    }
}

fn uid() -> libc::uid_t {
    // SAFETY: geteuid has no preconditions.
    unsafe { libc::geteuid() }
}

/// The Taskfile's `TOOL_PATH`, for `home`.
fn tool_path(home: &Path) -> String {
    let line = TASKFILE
        .lines()
        .find_map(|l| l.trim().strip_prefix("TOOL_PATH:"))
        .expect("TOOL_PATH in Taskfile.yml");
    line.trim()
        .trim_matches('"')
        .replace("{{.HOME}}", home.to_str().unwrap())
}

fn dict(v: &plist::Value) -> &plist::Dictionary {
    v.as_dictionary().expect("a dict")
}

#[test]
fn install_writes_the_plist_then_boots_out_and_bootstraps() {
    let e = env("ok");
    let out = e.run(&["daemon", "install"], &[]);
    assert!(out.status.success(), "{}", out.stderr);

    let plist = plist::Value::from_file(e.plist()).unwrap();
    let job = dict(&plist);
    assert_eq!(job["Label"].as_string(), Some("cos.daemon"));
    let argv: Vec<&str> = job["ProgramArguments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_string().unwrap())
        .collect();
    assert_eq!(argv, [COS, "daemon"]);
    assert_eq!(job["RunAtLoad"].as_boolean(), Some(true));
    let keep = dict(&job["KeepAlive"]);
    assert_eq!(keep.len(), 1, "{keep:?}");
    assert_eq!(keep["SuccessfulExit"].as_boolean(), Some(false));
    let vars = dict(&job["EnvironmentVariables"]);
    assert_eq!(vars["PATH"].as_string(), Some(tool_path(&e.root).as_str()));
    assert_eq!(
        vars["COS_BUSINESSES"].as_string(),
        Some(e.registry.to_str().unwrap())
    );
    assert!(!e.agents().join("cos.daemon.plist.new").exists());

    let uid = uid();
    assert_eq!(
        e.log(),
        format!(
            "bootout gui/{uid}/cos.daemon\nbootstrap gui/{uid} {}\n",
            e.plist().display()
        )
    );
}

#[test]
fn install_refuses_a_bin_holding_markup_or_a_newline() {
    let e = env("markup");
    for bin in ["/opt/a<b/cos", "/opt/a&b/cos", "/opt/a\nb/cos"] {
        let out = e.run(&["daemon", "install", "--bin", bin], &[]);
        assert!(!out.status.success(), "{bin:?} accepted");
        assert!(out.stderr.contains("bin"), "{bin:?}: {}", out.stderr);
    }
    let out = e.run(&["daemon", "install", "--bin", "relative/cos"], &[]);
    assert!(!out.status.success(), "a relative bin accepted");
    assert!(!e.plist().exists());
    assert!(!e.agents().join("cos.daemon.plist.new").exists());
    assert_eq!(e.log(), "", "launchctl called for a refused install");
}

#[test]
fn install_refuses_a_path_holding_markup() {
    let e = env("home");
    let out = e.run(&["daemon", "install"], &[("HOME", "/Users/a&b")]);
    assert!(!out.status.success());
    assert!(out.stderr.contains("PATH"), "{}", out.stderr);
    assert!(!e.plist().exists());
    assert_eq!(e.log(), "");
}

#[test]
fn install_reports_a_failed_bootstrap() {
    let e = env("fail");
    let out = e.run(
        &["daemon", "install"],
        &[("COS_STAND_IN_FAIL", "bootstrap")],
    );
    assert!(!out.status.success());
    assert!(out.stderr.contains("bootstrap"), "{}", out.stderr);
}

#[test]
fn uninstall_boots_the_job_out_and_removes_the_plist() {
    let e = env("un");
    let out = e.run(&["daemon", "install"], &[]);
    assert!(out.status.success(), "{}", out.stderr);
    let out = e.run(&["daemon", "uninstall"], &[]);
    assert!(out.status.success(), "{}", out.stderr);
    assert!(!e.plist().exists());
    let uid = uid();
    assert!(
        e.log()
            .ends_with(&format!("\nbootout gui/{uid}/cos.daemon\n")),
        "{}",
        e.log()
    );
    let out = e.run(&["daemon", "uninstall"], &[]);
    assert!(out.status.success(), "a second uninstall: {}", out.stderr);
}

/// A daemon this test owns: SIGTERM, then SIGKILL, then reaped on drop.
struct Daemon(Option<Child>);

impl Daemon {
    fn pid(&self) -> libc::pid_t {
        self.0.as_ref().map_or(0, |c| c.id() as libc::pid_t)
    }

    /// Whether it exited, reaping it, within `within`.
    fn gone(&mut self, within: Duration) -> bool {
        let Some(mut child) = self.0.take() else {
            return true;
        };
        if proc::wait_until(&mut child, &Deadline::after(within)).is_ok() {
            return true;
        }
        self.0 = Some(child);
        false
    }

    fn kill(&mut self) {
        if let Some(mut child) = self.0.take() {
            proc::kill_and_reap(&mut child);
        }
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            proc::signal_pid(child.id() as libc::pid_t, libc::SIGTERM);
            if proc::wait_until(&mut child, &Deadline::after(Duration::from_secs(30))).is_err() {
                proc::kill_and_reap(&mut child);
            }
        }
    }
}

/// Kills a run group a test saw, while its leader is still the recorded one.
struct Reaper(proc::Group);

impl Drop for Reaper {
    fn drop(&mut self) {
        proc::kill_recorded(&self.0);
    }
}

fn call(sock: &Path, req: &Value) -> Option<Value> {
    use std::io::{BufRead, BufReader, Write};
    let mut s = std::os::unix::net::UnixStream::connect(sock).ok()?;
    s.set_read_timeout(Some(Duration::from_secs(30))).ok()?;
    s.write_all(format!("{req}\n").as_bytes()).ok()?;
    let mut line = String::new();
    BufReader::new(s).read_line(&mut line).ok()?;
    serde_json::from_str(&line).ok()
}

fn eventually<T>(what: &str, mut probe: impl FnMut() -> Option<T>) -> T {
    let until = Instant::now() + BOUND;
    loop {
        if let Some(v) = probe() {
            return v;
        }
        assert!(Instant::now() < until, "never: {what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// `cos daemon` on the env's socket directory, its runs the stand-in sleeping
/// 60 s, answering `hello`.
fn daemon(e: &Env) -> Daemon {
    daemon_with(e, &[])
}

fn daemon_with(e: &Env, extra: &[(&str, &str)]) -> Daemon {
    let log = std::fs::File::create(e.root.join("daemon.out")).unwrap();
    let mut cmd = e.cmd(&["daemon"]);
    cmd.env("COS_RUN_BIN", STAND_IN)
        .env("COS_SLEEP_MS", "60000")
        .env("COS_HC", e.root.join("no-hc"))
        .env_remove("COS_OWNER")
        .env_remove("COS_RUN_ID")
        .env_remove("COS_DAEMON_UID")
        .env_remove("COS_DAEMON_DEADLINE_SECS")
        .stdout(Stdio::from(log.try_clone().unwrap()))
        .stderr(Stdio::from(log));
    for (k, v) in extra {
        cmd.env(k, v);
    }
    let d = Daemon(Some(proc::spawn_grouped(&mut cmd).unwrap()));
    let sock = e.root.join("d/daemon.sock");
    eventually("the daemon answers hello", || {
        call(&sock, &json!({"op": "hello"}))
    });
    d
}

fn records(home: &Path) -> Vec<runs::Record> {
    let Ok(entries) = std::fs::read_dir(home.join(runs::DIR)) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|f| {
            let id = f.file_name().to_str()?.strip_suffix(".json")?.to_owned();
            runs::read(home, &id).ok().flatten()
        })
        .collect()
}

/// The stand-in `launchctl` as launchd: on `bootout` it asks the daemon for a
/// brief of `acme`, logs the reply, then ends the daemon.
const BOOTOUT: (&str, &str) = ("COS_STAND_IN_BOOTOUT", "acme");

fn start_at_bootout(log: &str) -> Value {
    let line = log
        .lines()
        .find_map(|l| l.strip_prefix("start acme: "))
        .unwrap_or_else(|| panic!("no start at bootout in {log}"));
    serde_json::from_str(line).unwrap()
}

#[test]
fn install_drains_so_a_start_landing_before_bootout_is_refused() {
    for (name, args) in [
        ("drain", &["daemon", "install"][..]),
        ("drainf", &["daemon", "install", "--force"]),
    ] {
        let e = env(name);
        let mut d = daemon(&e);
        let out = e.run(args, &[BOOTOUT]);
        assert!(out.status.success(), "{args:?}: {}", out.stderr);
        let log = e.log();
        let refused = start_at_bootout(&log);
        assert!(
            refused["error"]
                .as_str()
                .unwrap_or_default()
                .contains("draining"),
            "{args:?}: a start between drain and bootout was accepted: {refused}"
        );
        assert!(records(&e.home).is_empty(), "{:?}", records(&e.home));
        assert!(e.plist().exists());
        assert!(log.contains("\nbootstrap "), "{log}");
        assert!(d.gone(Duration::from_secs(10)), "bootout left the daemon");
    }
}

#[test]
fn install_while_a_start_is_still_spawning_refuses_and_leaves_the_run_live() {
    let e = env("inflight");
    let _d = daemon_with(
        &e,
        &[("COS_GRANDCHILD_SECS", "60"), ("COS_GATE_HOLD_MS", "5000")],
    );
    let sock = e.root.join("d/daemon.sock");
    let reply = call(
        &sock,
        &json!({"op": "start", "kind": "brief", "business": "acme", "client_req": "00000000000000d1"}),
    )
    .unwrap();
    let id = reply["run"].as_str().expect("run id").to_owned();
    let gate = e.home.join("gate.pid");
    let pgid: libc::pid_t = eventually("the run holds its spawn gate", || {
        std::fs::read_to_string(&gate).ok()?.trim().parse().ok()
    });
    let _grandchild = Reaper(proc::Group {
        pgid,
        start: proc::start_of(pgid).unwrap_or_default(),
    });

    let out = e.run(&["daemon", "install"], &[BOOTOUT]);
    assert_eq!(out.status.code(), Some(75), "{}", out.stderr);
    assert!(out.stderr.contains(&id), "{}", out.stderr);
    assert!(!e.plist().exists());
    assert_eq!(e.log(), "");
    let r = runs::read(&e.home, &id).unwrap().unwrap();
    assert_eq!(r.state, State::Running, "{r:?}");
    assert!(!runs::process_gone(&r), "install ended the run: {r:?}");
}

#[test]
fn install_over_a_daemon_bootout_leaves_running_fails_and_undrains_it() {
    let e = env("shell");
    let d = daemon(&e);
    let out = e.run(&["daemon", "install"], &[]);
    assert!(
        !out.status.success(),
        "installed over a daemon bootout left"
    );
    assert_ne!(out.status.code(), Some(75), "{}", out.stderr);
    assert!(out.stderr.contains(&d.pid().to_string()), "{}", out.stderr);
    assert!(out.stderr.contains("bootout"), "{}", out.stderr);
    assert!(!e.log().contains("bootstrap"), "{}", e.log());
    let sock = e.root.join("d/daemon.sock");
    let reply = call(
        &sock,
        &json!({"op": "start", "kind": "brief", "business": "acme", "client_req": "00000000000000b1"}),
    )
    .unwrap();
    assert!(reply["run"].is_string(), "left drained: {reply}");
}

#[test]
fn install_with_no_daemon_answering_refuses_a_live_daemon_record_even_with_force() {
    let e = env("silent");
    let mut d = daemon_with(&e, &[("COS_STAND_IN_NO_WATCHDOG", "1")]);
    let sock = e.root.join("d/daemon.sock");
    let reply = call(
        &sock,
        &json!({"op": "start", "kind": "brief", "business": "acme", "client_req": "00000000000000c1"}),
    )
    .unwrap();
    let id = reply["run"].as_str().expect("run id").to_owned();
    let r = eventually("the run is running", || {
        runs::read(&e.home, &id)
            .unwrap()
            .filter(|r| r.state == State::Running)
    });
    let _run = Reaper(proc::Group {
        pgid: r.pid.unwrap(),
        start: r.start.clone().unwrap(),
    });
    d.kill();
    assert!(sock.exists(), "precondition: a socket nothing answers on");

    for args in [
        &["daemon", "install"][..],
        &["daemon", "install", "--force"],
    ] {
        let out = e.run(args, &[]);
        assert_eq!(out.status.code(), Some(75), "{args:?}: {}", out.stderr);
        assert!(out.stderr.contains(&id), "{}", out.stderr);
    }
    assert!(!e.plist().exists());
    assert_eq!(e.log(), "");
}

/// A socket in the env's daemon directory answering each line with `reply`,
/// or holding the connection with no answer when it is `None`.
fn fake_daemon(e: &Env, reply: Option<Value>) {
    fake_daemon_seq(e, vec![reply]);
}

/// As `fake_daemon`, the n-th connection answered with `replies[n]`, the last
/// one from then on.
fn fake_daemon_seq(e: &Env, replies: Vec<Option<Value>>) {
    use std::os::unix::fs::PermissionsExt;
    let dir = e.root.join("d");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    let listener = std::os::unix::net::UnixListener::bind(dir.join("daemon.sock")).unwrap();
    std::thread::spawn(move || {
        use std::io::{BufRead, BufReader, Write};
        let mut held = Vec::new();
        for (n, s) in listener.incoming().enumerate() {
            let Ok(mut s) = s else { return };
            let mut line = String::new();
            let _ = BufReader::new(&s).read_line(&mut line);
            match &replies[n.min(replies.len() - 1)] {
                Some(r) => {
                    let _ = s.write_all(format!("{r}\n").as_bytes());
                }
                None => held.push(s),
            }
        }
    });
}

#[test]
fn install_against_a_daemon_without_drain_falls_back_to_the_records() {
    let e = env("old");
    fake_daemon(&e, Some(json!({"error": "unknown op \"drain\""})));
    let out = e.run(&["daemon", "install"], &[]);
    assert!(out.status.success(), "{}", out.stderr);
    assert!(e.plist().exists());
}

#[test]
fn install_drains_again_before_bootout_and_refuses_a_run_that_appeared_since() {
    let e = env("again");
    const RUN: &str = "r-20261127-090000-ab01";
    fake_daemon_seq(
        &e,
        vec![
            Some(json!({"error": "unknown op \"drain\""})),
            Some(json!({"error": format!("runs are live: {RUN} (acme)"),
                        "live": [{"run": RUN, "business": "acme"}]})),
        ],
    );
    let out = e.run(&["daemon", "install"], &[]);
    assert_eq!(out.status.code(), Some(75), "{}", out.stderr);
    assert!(out.stderr.contains(RUN), "{}", out.stderr);
    assert!(!e.plist().exists());
    assert_eq!(e.log(), "");
}

#[test]
fn install_against_a_daemon_that_does_not_answer_fails_and_calls_no_launchctl() {
    let e = env("mute");
    fake_daemon(&e, None);
    let out = e.run(&["daemon", "install", "--force"], &[]);
    assert!(!out.status.success(), "installed past a mute daemon");
    assert_ne!(out.status.code(), Some(75), "{}", out.stderr);
    assert!(out.stderr.contains("drain"), "{}", out.stderr);
    assert!(!e.plist().exists());
    assert_eq!(e.log(), "");
}

#[test]
fn install_refuses_while_the_daemon_owns_a_live_run_and_force_stops_it_first() {
    let e = env("live");
    let mut d = daemon(&e);
    let sock = e.root.join("d/daemon.sock");
    let reply = call(
        &sock,
        &json!({"op": "start", "kind": "brief", "business": "acme", "client_req": "00000000000000a1"}),
    )
    .unwrap();
    let id = reply["run"].as_str().expect("run id").to_owned();
    eventually("the run is running", || {
        runs::read(&e.home, &id)
            .unwrap()
            .filter(|r| r.state == State::Running)
    });

    let out = e.run(&["daemon", "install"], &[]);
    assert_eq!(out.status.code(), Some(75), "{}", out.stderr);
    assert!(out.stderr.contains(&id), "{}", out.stderr);
    assert!(out.stderr.contains("--force"), "{}", out.stderr);
    assert!(!e.plist().exists());
    assert_eq!(e.log(), "");
    assert_eq!(
        runs::read(&e.home, &id).unwrap().unwrap().state,
        State::Running
    );

    let out = e.run(&["daemon", "install", "--force"], &[BOOTOUT]);
    assert!(out.status.success(), "{}", out.stderr);
    assert_eq!(
        runs::read(&e.home, &id).unwrap().unwrap().state,
        State::Stopped
    );
    assert!(e.plist().exists());
    let log = e.log();
    assert!(log.starts_with("bootout "), "{log}");
    let refused = start_at_bootout(&log);
    assert!(
        refused["error"]
            .as_str()
            .unwrap_or_default()
            .contains("draining"),
        "--force left the daemon taking starts: {refused}"
    );
    assert_eq!(records(&e.home).len(), 1, "{:?}", records(&e.home));
    assert!(d.gone(Duration::from_secs(10)), "bootout left the daemon");
}

/// The `>-` folded cmd holding `{{.BIN}} daemon install`, its lines joined
/// with spaces as YAML folds them.
fn daemon_install_cmd(install: &str) -> String {
    let lines: Vec<&str> = install.lines().collect();
    let at = lines
        .iter()
        .position(|l| l.trim_start().starts_with("{{.BIN}} daemon install"))
        .expect("daemon install cmd");
    assert_eq!(
        lines[at - 1].trim(),
        "- >-",
        "daemon install is a folded cmd"
    );
    let indent = lines[at].len() - lines[at].trim_start().len();
    lines[at..]
        .iter()
        .take_while(|l| l.len() - l.trim_start().len() == indent && !l.trim().is_empty())
        .map(|l| l.trim())
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn install_refuses_a_relative_registry() {
    let e = env("relreg");
    let mut cmd = e.cmd(&["daemon", "install"]);
    cmd.env("COS_BUSINESSES", "businesses.toml")
        .current_dir(&e.root);
    let out = proc::run(&mut cmd, BOUND).unwrap();
    assert!(!out.status.success(), "a relative registry accepted");
    assert!(out.stderr.contains("COS_BUSINESSES"), "{}", out.stderr);
    assert!(!e.plist().exists());
    assert_eq!(e.log(), "");
}

#[test]
fn task_install_runs_daemon_install_after_the_binary_copy_and_reports_a_refusal() {
    let install = TASKFILE
        .split("\n  install:\n")
        .nth(1)
        .and_then(|rest| rest.split("\n  install-mcp:\n").next())
        .expect("the install task");
    let mv = install
        .find("mv -f {{.BIN}}.new {{.BIN}}")
        .expect("the copy");
    let daemon = install
        .find("{{.BIN}} daemon install")
        .expect("daemon install");
    assert!(mv < daemon, "daemon install runs before the copy");

    let cmd = daemon_install_cmd(install);
    for (rc, ok) in [(0, true), (75, true), (1, false), (2, false)] {
        let script = cmd.replace("{{.BIN}}", &format!("/bin/sh -c 'exit {rc}'"));
        let out = proc::run(Command::new("/bin/sh").args(["-c", &script]), BOUND).unwrap();
        assert_eq!(out.status.success(), ok, "exit {rc}: {script}");
        assert_eq!(
            out.stdout.contains("refused"),
            rc == 75,
            "exit {rc}: {}",
            out.stdout
        );
    }

    let setup = TASKFILE
        .split("\n  setup:\n")
        .nth(1)
        .and_then(|rest| rest.split("\n  build:\n").next())
        .expect("the setup task");
    assert!(setup.contains("task: install"), "setup runs install");
}
