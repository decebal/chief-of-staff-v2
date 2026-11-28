#![cfg(feature = "stand-in")]
//! `cos daemon` on a temp socket directory (`COS_DAEMON_DIR`) and a temp
//! registry (`COS_BUSINESSES`), with the stand-in as its run program
//! (`COS_RUN_BIN`). Every daemon, run and group a test starts is ended and
//! reaped before the test returns, pass or fail.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use cos::proc::{self, Deadline, group_alive};
use cos::runs::{self, Kind, Owner, Record, State};
use serde_json::{Value, json};

const COS: &str = env!("CARGO_BIN_EXE_cos");
const STAND_IN: &str = env!("CARGO_BIN_EXE_cos-stand-in");
const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");
const BOUND: Duration = Duration::from_secs(60);

struct Env {
    root: PathBuf,
    dir: PathBuf,
    registry: PathBuf,
    homes: BTreeMap<String, PathBuf>,
    daemons: std::cell::Cell<usize>,
}

/// A temp root holding the socket directory, the registry and one home per
/// business. Names stay short: the socket path must fit `sun_path`.
fn env(name: &str, businesses: &[&str]) -> Env {
    let root = std::env::temp_dir().join(format!("cosd-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let root = root.canonicalize().unwrap();
    let mut toml = String::new();
    let mut homes = BTreeMap::new();
    for b in businesses {
        let home = root.join(b);
        std::fs::create_dir_all(home.join("projects")).unwrap();
        toml.push_str(&format!(
            "[business.{b}]\nhome = {:?}\nworkspace = {:?}\nprojects = {:?}\n\n",
            home,
            home,
            home.join("projects")
        ));
        homes.insert((*b).to_owned(), home);
    }
    let registry = root.join("businesses.toml");
    std::fs::write(&registry, toml).unwrap();
    Env {
        dir: root.join("d"),
        root,
        registry,
        homes,
        daemons: std::cell::Cell::new(0),
    }
}

/// A daemon this test owns: SIGTERM, then SIGKILL, then reaped on drop.
struct Daemon {
    child: Option<Child>,
    /// Its stdout, and its stderr until it reopens it.
    log: PathBuf,
    /// `<daemon dir>/daemon.log`, where it writes its stderr from then on.
    own_log: PathBuf,
}

impl Daemon {
    fn pid(&self) -> libc::pid_t {
        self.child.as_ref().map_or(0, |c| c.id() as libc::pid_t)
    }

    fn log(&self) -> String {
        let read = |p: &Path| std::fs::read_to_string(p).unwrap_or_default();
        read(&self.log) + &read(&self.own_log)
    }

    fn signal(&self, sig: libc::c_int) {
        proc::signal_pid(self.pid(), sig);
    }

    fn wait(&mut self, within: Duration) -> ExitStatus {
        let mut child = self.child.take().expect("daemon already reaped");
        let status = proc::wait_until(&mut child, &Deadline::after(within));
        status.unwrap_or_else(|e| panic!("daemon did not exit: {e:#}\n{}", self.log()))
    }

    fn running(&mut self) -> bool {
        self.child
            .as_mut()
            .is_some_and(|c| c.try_wait().unwrap().is_none())
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            proc::signal_pid(child.id() as libc::pid_t, libc::SIGTERM);
            if proc::wait_until(&mut child, &Deadline::after(Duration::from_secs(30))).is_err() {
                proc::kill_and_reap(&mut child);
            }
        }
    }
}

/// Kills a group a test saw, so a failing test leaks nothing; only while its
/// leader is still the recorded one.
struct Reaper(proc::Group);

impl Reaper {
    fn pgid(&self) -> libc::pid_t {
        self.0.pgid
    }
}

impl Drop for Reaper {
    fn drop(&mut self) {
        proc::kill_recorded(&self.0);
    }
}

impl Env {
    fn sock(&self) -> PathBuf {
        self.dir.join("daemon.sock")
    }

    fn home(&self, business: &str) -> &Path {
        &self.homes[business]
    }

    fn daemon(&self, extra: &[(&str, &str)]) -> Daemon {
        self.daemon_from(Path::new(COS), extra, true)
    }

    fn daemon_from(&self, program: &Path, extra: &[(&str, &str)], ready: bool) -> Daemon {
        let n = self.daemons.get();
        self.daemons.set(n + 1);
        let log = self.root.join(format!("daemon-{n}.log"));
        let out = std::fs::File::create(&log).unwrap();
        let mut cmd = Command::new(program);
        cmd.arg("daemon")
            .env("COS_DAEMON_DIR", &self.dir)
            .env("COS_BUSINESSES", &self.registry)
            .env("COS_RUN_BIN", STAND_IN)
            .env("COS_HC", self.root.join("no-hc"))
            .env("HOME", &self.root)
            .env_remove("COS_BUSINESS")
            .env_remove("COS_HOME")
            .env_remove("COS_WORKSPACE")
            .env_remove("COS_PROJECTS")
            .env_remove("COS_OWNER")
            .env_remove("COS_RUN_ID")
            .env_remove("COS_RUN_RECORD")
            .env_remove("COS_DAEMON_UID")
            .env_remove("COS_DAEMON_DEADLINE_SECS")
            .stdout(Stdio::from(out.try_clone().unwrap()))
            .stderr(Stdio::from(out));
        for (k, v) in extra {
            cmd.env(k, v);
        }
        let d = Daemon {
            child: Some(proc::spawn_grouped(&mut cmd).unwrap()),
            log,
            own_log: self.dir.join("daemon.log"),
        };
        if ready {
            let pid = d.pid();
            eventually("the daemon answers hello", BOUND, || {
                try_call(&self.sock(), &json!({"op": "hello"}).to_string())
                    .ok()
                    .filter(|v| v["pid"] == json!(pid))
            });
        }
        d
    }

    fn call(&self, req: Value) -> Value {
        try_call(&self.sock(), &req.to_string()).unwrap()
    }

    fn start(&self, kind: &str, business: &str, client_req: &str) -> Value {
        let mut req =
            json!({"op": "start", "kind": kind, "business": business, "client_req": client_req});
        if kind == "ask" {
            req["question"] = json!("what changed this week?");
        }
        self.call(req)
    }

    fn started(&self, kind: &str, business: &str, client_req: &str) -> String {
        let reply = self.start(kind, business, client_req);
        assert_eq!(reply["existing"], json!(false), "{reply}");
        reply["run"].as_str().expect("run id").to_owned()
    }
}

fn try_call(sock: &Path, line: &str) -> Result<Value, String> {
    let mut s = UnixStream::connect(sock).map_err(|e| e.to_string())?;
    s.set_read_timeout(Some(Duration::from_secs(30))).unwrap();
    s.write_all(format!("{line}\n").as_bytes())
        .map_err(|e| e.to_string())?;
    let mut reply = String::new();
    BufReader::new(s)
        .read_line(&mut reply)
        .map_err(|e| e.to_string())?;
    serde_json::from_str(&reply).map_err(|e| format!("{e}: {reply:?}"))
}

fn eventually<T>(what: &str, within: Duration, mut probe: impl FnMut() -> Option<T>) -> T {
    let until = Instant::now() + within;
    loop {
        if let Some(v) = probe() {
            return v;
        }
        assert!(Instant::now() < until, "never: {what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn record(home: &Path, id: &str) -> Record {
    runs::read(home, id).unwrap().expect("record")
}

fn records(home: &Path) -> Vec<Record> {
    let Ok(entries) = std::fs::read_dir(home.join(runs::DIR)) else {
        return vec![];
    };
    entries
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .map(|e| Record::parse(&std::fs::read_to_string(e.path()).unwrap()).unwrap())
        .collect()
}

fn final_record(home: &Path, id: &str, within: Duration) -> Record {
    eventually(&format!("{id} final"), within, || {
        Some(record(home, id)).filter(|r| r.state.is_final())
    })
}

/// The run's record once it is running with its grandchild group recorded,
/// and a reaper for each of its groups.
fn running_with_groups(home: &Path, id: &str) -> (Record, Vec<Reaper>) {
    let r = eventually("a running record with groups", BOUND, || {
        Some(record(home, id)).filter(|r| r.state == State::Running && !r.groups.is_empty())
    });
    let mut reapers: Vec<Reaper> = r.groups.iter().cloned().map(Reaper).collect();
    reapers.push(Reaper(proc::Group {
        pgid: r.pid.unwrap(),
        start: r.start.clone().expect("a running record has a start"),
    }));
    (r, reapers)
}

fn gone_within(pgid: libc::pid_t, within: Duration) -> bool {
    let until = Instant::now() + within;
    while group_alive(pgid) {
        if Instant::now() > until {
            return false;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    true
}

fn all_gone(r: &Record) {
    for g in &r.groups {
        assert!(
            gone_within(g.pgid, Duration::from_secs(5)),
            "group {g:?} lives on"
        );
    }
    let pid = r.pid.unwrap();
    assert!(
        gone_within(pid, Duration::from_secs(5)),
        "the run's own group {pid} lives on"
    );
}

const LONG: &[(&str, &str)] = &[("COS_SLEEP_MS", "60000"), ("COS_GRANDCHILD_SECS", "60")];

#[test]
fn hello_answers_on_a_user_only_socket() {
    let e = env("hello", &["acme"]);
    std::fs::create_dir_all(&e.dir).unwrap();
    std::fs::set_permissions(&e.dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    let d = e.daemon(&[]);
    let reply = e.call(json!({"op": "hello"}));
    assert_eq!(
        reply,
        json!({"version": env!("CARGO_PKG_VERSION"), "pid": d.pid()})
    );
    let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&e.dir), 0o700, "socket directory");
    assert_eq!(mode(&e.sock()), 0o600, "socket");
}

#[test]
fn a_started_brief_runs_queued_to_done_and_a_subscriber_sees_it() {
    let e = env("sub", &["acme"]);
    let d = e.daemon(&[("COS_SLEEP_MS", "1500")]);
    let mut sub = UnixStream::connect(e.sock()).unwrap();
    sub.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    sub.write_all(b"{\"op\":\"subscribe\"}\n").unwrap();
    let mut lines = BufReader::new(sub.try_clone().unwrap()).lines();
    let ack: Value = serde_json::from_str(&lines.next().unwrap().unwrap()).unwrap();
    assert_eq!(ack, json!({"subscribed": true}), "the baseline is taken");
    sub.set_read_timeout(Some(BOUND)).unwrap();

    let id = e.started("brief", "acme", "00000000000000a1");
    let mut seen = Vec::new();
    for line in lines {
        let v: Value = serde_json::from_str(&line.unwrap()).unwrap();
        if v["run"] != json!(id) {
            continue;
        }
        assert_eq!(
            (&v["business"], &v["kind"]),
            (&json!("acme"), &json!("brief"))
        );
        let state = v["state"].as_str().unwrap().to_owned();
        let done = state == "done";
        seen.push(state);
        if done {
            break;
        }
    }
    assert!(seen.iter().any(|s| s == "running"), "{seen:?}");
    let r = record(e.home("acme"), &id);
    assert_eq!(
        (r.state, r.kind, r.owner),
        (State::Done, Kind::Brief, Owner::Daemon)
    );
    assert_eq!(r.owner_pid, Some(d.pid()));
    assert_eq!(r.owner_start, proc::start_of(d.pid()));
    assert_eq!(r.client_req.as_deref(), Some("00000000000000a1"));
}

#[test]
fn a_second_brief_returns_the_first_and_a_retry_returns_the_first_reply() {
    let e = env("existing", &["acme", "beta"]);
    let _d = e.daemon(LONG);
    let first = e.start("brief", "acme", "00000000000000b1");
    let id = first["run"].as_str().unwrap().to_owned();
    assert_eq!(first, json!({"run": id, "existing": false}));
    let (_, _reap) = running_with_groups(e.home("acme"), &id);

    let second = e.start("brief", "acme", "00000000000000b2");
    assert_eq!(second, json!({"run": id, "existing": true}));
    let retry = e.start("brief", "acme", "00000000000000b1");
    assert_eq!(retry, first, "a repeated client_req gets the first reply");
    assert_eq!(records(e.home("acme")).len(), 1, "nothing else was spent");

    let holder = "r-20261126-120000-abcd";
    let runs::Taken::Held(lock) = runs::take_brief_lock(e.home("beta"), holder).unwrap() else {
        panic!("lock busy in a fresh home");
    };
    let held = e.start("brief", "beta", "00000000000000b3");
    assert_eq!(held, json!({"run": holder, "existing": true}));
    assert!(records(e.home("beta")).is_empty());
    drop(lock);

    let stopped = e.call(json!({"op": "stop", "run": id, "business": "acme"}));
    assert_eq!(stopped, json!({"state": "stopped"}));
}

#[test]
fn stop_ends_every_group_and_writes_stopped() {
    let e = env("stop", &["acme"]);
    let _d = e.daemon(LONG);
    let id = e.started("brief", "acme", "00000000000000c1");
    let (r, _reap) = running_with_groups(e.home("acme"), &id);
    assert!(group_alive(r.groups[0].pgid));

    let reply = e.call(json!({"op": "stop", "run": id, "business": "acme"}));
    assert_eq!(reply, json!({"state": "stopped"}));
    let r = record(e.home("acme"), &id);
    assert_eq!(r.state, State::Stopped);
    all_gone(&r);
}

#[test]
fn stop_of_a_run_that_ignores_sigterm_kills_it_after_5s_and_ends_stopped() {
    let e = env("stopkill", &["acme"]);
    let mut env = LONG.to_vec();
    env.push(("COS_IGNORE_TERM", "1"));
    let _d = e.daemon(&env);
    let id = e.started("ask", "acme", "00000000000000c2");
    let (_, _reap) = running_with_groups(e.home("acme"), &id);

    let t = Instant::now();
    let reply = e.call(json!({"op": "stop", "run": id, "business": "acme"}));
    assert!(t.elapsed() >= Duration::from_secs(5), "{:?}", t.elapsed());
    assert_eq!(reply, json!({"state": "stopped"}));
    let r = record(e.home("acme"), &id);
    assert_eq!((r.state, r.kind), (State::Stopped, Kind::Ask));
    all_gone(&r);
}

#[test]
fn a_run_past_its_deadline_is_failed_with_reason_deadline() {
    let e = env("deadline", &["acme"]);
    let mut env = LONG.to_vec();
    env.extend([("COS_IGNORE_TERM", "1"), ("COS_DAEMON_DEADLINE_SECS", "1")]);
    let _d = e.daemon(&env);
    let id = e.started("brief", "acme", "00000000000000d1");
    let (_, _reap) = running_with_groups(e.home("acme"), &id);

    let r = final_record(e.home("acme"), &id, Duration::from_secs(30));
    assert_eq!(r.state, State::Failed);
    assert_eq!(r.reason.as_deref(), Some("deadline"));
    all_gone(&r);
}

#[test]
fn a_run_that_ends_itself_on_sigterm_past_its_deadline_is_still_failed_deadline() {
    let e = env("deadline-term", &["acme"]);
    let mut env = LONG.to_vec();
    env.push(("COS_DAEMON_DEADLINE_SECS", "1"));
    let _d = e.daemon(&env);
    let id = e.started("brief", "acme", "00000000000000d2");
    let (_, _reap) = running_with_groups(e.home("acme"), &id);

    let r = final_record(e.home("acme"), &id, Duration::from_secs(30));
    assert_eq!(r.state, State::Failed, "{r:?}");
    assert_eq!(r.reason.as_deref(), Some("deadline"));
    all_gone(&r);
}

#[test]
fn sigterm_of_the_daemon_stops_its_runs_and_exits_0() {
    let e = env("term", &["acme"]);
    let mut d = e.daemon(LONG);
    let id = e.started("brief", "acme", "00000000000000e1");
    let (_, _reap) = running_with_groups(e.home("acme"), &id);

    d.signal(libc::SIGTERM);
    let status = d.wait(Duration::from_secs(30));
    assert_eq!(status.code(), Some(0), "{}", d.log());
    let r = record(e.home("acme"), &id);
    assert_eq!(r.state, State::Stopped);
    assert_eq!(r.reason.as_deref(), Some("daemon stopped"));
    all_gone(&r);
    assert!(!e.sock().exists(), "the socket outlived the daemon");
}

#[test]
fn sigterm_of_the_daemon_kills_a_run_that_ignores_sigterm_and_writes_daemon_stopped() {
    let e = env("termkill", &["acme"]);
    let mut env = LONG.to_vec();
    env.push(("COS_IGNORE_TERM", "1"));
    let mut d = e.daemon(&env);
    let id = e.started("brief", "acme", "00000000000000e2");
    let (_, _reap) = running_with_groups(e.home("acme"), &id);

    let t = Instant::now();
    d.signal(libc::SIGTERM);
    let status = d.wait(Duration::from_secs(30));
    assert!(t.elapsed() >= Duration::from_secs(5), "{:?}", t.elapsed());
    assert_eq!(status.code(), Some(0), "{}", d.log());
    let r = record(e.home("acme"), &id);
    assert_eq!(r.state, State::Stopped);
    assert_eq!(r.reason.as_deref(), Some("daemon stopped"));
    all_gone(&r);
}

#[test]
fn sigkill_and_restart_sweeps_a_run_without_a_watchdog_to_lost() {
    let e = env("restart", &["acme"]);
    let mut env = LONG.to_vec();
    env.push(("COS_STAND_IN_NO_WATCHDOG", "1"));
    let mut d = e.daemon(&env);
    let id = e.started("brief", "acme", "00000000000000f1");
    let (r, _reap) = running_with_groups(e.home("acme"), &id);

    d.signal(libc::SIGKILL);
    d.wait(BOUND);
    std::thread::sleep(Duration::from_millis(1500));
    assert_eq!(
        proc::start_of(r.pid.unwrap()),
        r.start,
        "precondition: the run lives"
    );
    assert!(
        group_alive(r.groups[0].pgid),
        "precondition: its grandchild lives"
    );
    assert_eq!(record(e.home("acme"), &id).state, State::Running);

    let _d2 = e.daemon(&env);
    let r = record(e.home("acme"), &id);
    assert_eq!(r.state, State::Lost);
    assert_eq!(r.reason.as_deref(), Some("daemon restarted"));
    all_gone(&r);
}

fn git(dir: &Path, args: &[&str]) {
    let mut cmd = Command::new("git");
    cmd.arg("-C")
        .arg(dir)
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args);
    let out = proc::run(&mut cmd, BOUND).unwrap();
    assert!(out.status.success(), "git {args:?}: {}", out.stderr);
}

#[test]
fn the_daemons_judge_group_is_swept_after_sigkill_and_ended_on_sigterm() {
    let e = env("judge", &["acme"]);
    let home = e.home("acme").to_owned();
    let repo = home.join("projects/demo-repo");
    std::fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    std::fs::write(repo.join("README.md"), "demo\n").unwrap();
    git(&repo, &["add", "README.md"]);
    git(&repo, &["commit", "-q", "-m", "init"]);

    let state = e.root.join("herdr");
    std::fs::create_dir_all(&state).unwrap();
    let pidfile = e.root.join("judge.pid");
    let fixture = |f: &str| format!("{FIXTURES}/{f}");
    let herdr = [
        ("COS_HC", fixture("fake-hc.sh")),
        ("COS_HERDR_APP", fixture("fake-herdr-app.sh")),
        ("FAKE_HC_STATE", state.to_string_lossy().into_owned()),
        ("COS_HERDR_GRACE_SECS", "0".to_owned()),
    ];
    let mut hire = Command::new(COS);
    hire.arg("--home")
        .arg(&home)
        .arg("--workspace")
        .arg(&home)
        .arg("--projects")
        .arg(home.join("projects"))
        .args(["agents", "spawn", "--prompt", "edit a file", "--repo"])
        .arg(&repo)
        .arg("--check")
        .arg(format!("echo $$ > {}; exec sleep 60", pidfile.display()))
        .arg("--program")
        .arg(fixture("fake-agent.sh"))
        .env("FAKE_HC_EXEC", "1")
        .env("HOME", &e.root)
        .env_remove("COS_BUSINESS");
    for (k, v) in &herdr {
        hire.env(k, v);
    }
    let out = proc::run(&mut hire, BOUND).unwrap();
    assert!(out.status.success(), "hire: {}", out.stderr);

    let env: Vec<(&str, &str)> = herdr.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let mut d = e.daemon(&env);
    let judge_group = |what: &str| {
        let pgid: libc::pid_t = eventually(what, BOUND, || {
            std::fs::read_to_string(&pidfile)
                .ok()
                .and_then(|s| s.trim().parse().ok())
        });
        let mirrored = eventually("the judge's group mirrored", BOUND, || {
            let text = std::fs::read_to_string(e.dir.join("daemon.groups.json")).ok()?;
            let groups: Vec<proc::Group> = serde_json::from_str(&text).ok()?;
            groups.into_iter().find(|g| g.pgid == pgid)
        });
        assert!(group_alive(mirrored.pgid));
        Reaper(mirrored)
    };
    let first = judge_group("the daemon's judge ran the check");

    d.signal(libc::SIGKILL);
    d.wait(BOUND);
    assert!(
        group_alive(first.pgid()),
        "precondition: the judge's sh outlived the daemon"
    );
    std::fs::remove_file(&pidfile).unwrap();

    let mut d2 = e.daemon(&env);
    assert!(
        !group_alive(first.pgid()),
        "the restarted daemon's sweep left the judge group:\n{}",
        d2.log()
    );

    let second = judge_group("the restarted daemon judged again");
    d2.signal(libc::SIGTERM);
    let status = d2.wait(Duration::from_secs(30));
    assert_eq!(status.code(), Some(0), "{}", d2.log());
    assert!(
        gone_within(second.pgid(), Duration::from_secs(5)),
        "SIGTERM left the judge group"
    );
    let hires: Vec<PathBuf> = std::fs::read_dir(cos::agents::root(&home))
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.path().join("state.json"))
        .filter(|p| p.exists())
        .collect();
    assert_eq!(hires.len(), 1, "{hires:?}");
    let state: Value = serde_json::from_str(&std::fs::read_to_string(&hires[0]).unwrap()).unwrap();
    assert_eq!(
        state["verdict"],
        Value::Null,
        "a check the daemon's shutdown killed became a verdict"
    );
}

#[test]
fn a_second_daemon_exits_0_already_running() {
    let e = env("twice", &["acme"]);
    let d = e.daemon(&[]);
    let mut again = e.daemon_from(Path::new(COS), &[], false);
    let status = again.wait(Duration::from_secs(10));
    assert_eq!(status.code(), Some(0));
    assert!(again.log().contains("already running"), "{}", again.log());
    assert_eq!(e.call(json!({"op": "hello"}))["pid"], json!(d.pid()));
}

#[test]
fn a_stale_socket_file_is_replaced() {
    let e = env("stale", &["acme"]);
    std::fs::create_dir_all(&e.dir).unwrap();
    let mut bind = Command::new(STAND_IN);
    bind.arg("stale-socket").arg(e.sock());
    let out = proc::run(&mut bind, BOUND).unwrap();
    assert!(out.status.success(), "{}", out.stderr);
    assert!(e.sock().exists());
    assert!(
        UnixStream::connect(e.sock()).is_err(),
        "precondition: stale"
    );
    let d = e.daemon(&[]);
    assert_eq!(e.call(json!({"op": "hello"}))["pid"], json!(d.pid()));
}

#[test]
fn an_exe_change_exits_1_once_no_run_is_live() {
    let e = env("exe", &["acme"]);
    let copy = e.root.join("cos");
    std::fs::copy(COS, &copy).unwrap();
    let mut d = e.daemon_from(&copy, LONG, true);
    let id = e.started("brief", "acme", "0000000000000a01");
    let (_, _reap) = running_with_groups(e.home("acme"), &id);

    std::fs::File::options()
        .write(true)
        .open(&copy)
        .unwrap()
        .set_modified(std::time::UNIX_EPOCH + Duration::from_secs(1_000_000_000))
        .unwrap();
    std::thread::sleep(Duration::from_secs(3));
    assert!(d.running(), "exited with a live run:\n{}", d.log());

    let reply = e.call(json!({"op": "stop", "run": id, "business": "acme"}));
    assert_eq!(reply, json!({"state": "stopped"}));
    let status = d.wait(Duration::from_secs(15));
    assert_eq!(status.code(), Some(1), "{}", d.log());
}

#[test]
fn a_peer_with_another_uid_is_closed_without_a_reply() {
    let e = env("peer", &["acme"]);
    // SAFETY: geteuid has no preconditions.
    let other = unsafe { libc::geteuid() } + 1;
    let d = e.daemon_from(
        Path::new(COS),
        &[("COS_DAEMON_UID", &other.to_string())],
        false,
    );
    eventually("the socket is bound", BOUND, || {
        e.sock().exists().then_some(())
    });
    let mut s = UnixStream::connect(e.sock()).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    let _ = s.write_all(b"{\"op\":\"hello\"}\n");
    let mut buf = Vec::new();
    let _ = s.read_to_end(&mut buf);
    assert!(buf.is_empty(), "a reply to another uid: {buf:?}");
    eventually("the refusal is logged", BOUND, || {
        d.log().contains(&format!("uid {}", other)).then_some(())
    });
}

#[test]
fn requests_outside_the_binding_are_refused_and_spawn_nothing() {
    let e = env("refuse", &["acme"]);
    let _d = e.daemon(&[]);
    let req = "0123456789abcdef";
    let bad = [
        "not json".to_owned(),
        json!({"op": "nope"}).to_string(),
        json!({"op": "hello", "extra": 1}).to_string(),
        json!({"op": "start", "kind": "brief", "business": "acme", "client_req": req, "question": "q"}).to_string(),
        json!({"op": "start", "kind": "ask", "business": "acme", "client_req": req}).to_string(),
        json!({"op": "start", "kind": "ask", "business": "acme", "client_req": req, "question": ""}).to_string(),
        json!({"op": "start", "kind": "ask", "business": "acme", "client_req": req, "question": "x".repeat(4001)}).to_string(),
        json!({"op": "start", "kind": "dream", "business": "acme", "client_req": req}).to_string(),
        json!({"op": "start", "kind": "brief", "business": "Acme!", "client_req": req}).to_string(),
        json!({"op": "start", "kind": "brief", "business": "acme", "client_req": "XYZ"}).to_string(),
        json!({"op": "start", "kind": "brief", "business": "acme"}).to_string(),
        json!({"op": "stop", "run": "../x", "business": "acme"}).to_string(),
        json!({"op": "subscribe", "since": 0}).to_string(),
        json!({"op": "drain", "force": true}).to_string(),
        json!({"op": "undrain", "all": true}).to_string(),
    ];
    for line in &bad {
        let reply = try_call(&e.sock(), line).unwrap();
        assert!(reply["error"].is_string(), "{line} -> {reply}");
        assert_eq!(reply.as_object().unwrap().len(), 1, "{reply}");
    }
    let unknown = e.start("brief", "nobody", req);
    let msg = unknown["error"].as_str().unwrap();
    assert!(msg.contains("nobody") && msg.contains("acme"), "{msg}");

    let mut s = UnixStream::connect(e.sock()).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    s.write_all(&vec![b'a'; 64 * 1024 + 1]).unwrap();
    let mut reply = String::new();
    BufReader::new(s).read_line(&mut reply).unwrap();
    let reply: Value = serde_json::from_str(&reply).unwrap();
    assert!(
        reply["error"].as_str().unwrap().contains("65536"),
        "{reply}"
    );

    assert!(
        records(e.home("acme")).is_empty(),
        "a refused request spawned"
    );
}

#[test]
fn caps_two_live_asks_per_business_and_four_live_runs_in_total() {
    let e = env("caps", &["acme", "beta"]);
    let _d = e.daemon(&[("COS_SLEEP_MS", "60000")]);
    e.started("ask", "acme", "0000000000000c01");
    e.started("ask", "acme", "0000000000000c02");
    let third = e.start("ask", "acme", "0000000000000c03");
    assert!(
        third["error"].as_str().unwrap().contains("2 asks"),
        "{third}"
    );
    e.started("brief", "acme", "0000000000000c04");
    e.started("brief", "beta", "0000000000000c05");
    let fifth = e.start("ask", "beta", "0000000000000c06");
    assert!(
        fifth["error"].as_str().unwrap().contains("4 runs"),
        "{fifth}"
    );
    assert_eq!(
        records(e.home("acme")).len() + records(e.home("beta")).len(),
        4
    );
}

#[test]
fn the_daemon_writes_its_stderr_to_daemon_log_in_its_directory() {
    let e = env("dlog", &["acme"]);
    let d = e.daemon(&[("COS_SLEEP_MS", "500")]);
    let id = e.started("brief", "acme", "0000000000000e01");
    let own = e.dir.join("daemon.log");
    eventually("the start is in daemon.log", BOUND, || {
        std::fs::read_to_string(&own)
            .ok()
            .filter(|t| t.contains(&format!("run {id} started")))
    });
    let mode = std::fs::metadata(&own).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "daemon.log");
    let captured = std::fs::read_to_string(&d.log).unwrap_or_default();
    assert!(
        !captured.contains(&format!("run {id} started")),
        "the start went to the inherited stderr: {captured}"
    );
}

#[test]
fn drain_refuses_every_later_start() {
    let e = env("drain", &["acme"]);
    let d = e.daemon(&[("COS_SLEEP_MS", "60000")]);
    assert_eq!(
        e.call(json!({"op": "drain"})),
        json!({"drained": true, "pid": d.pid()})
    );
    for (kind, req) in [("brief", "0000000000000f01"), ("ask", "0000000000000f02")] {
        let refused = e.start(kind, "acme", req);
        let msg = refused["error"].as_str().unwrap_or_default();
        assert!(msg.contains("draining"), "{kind}: {refused}");
    }
    assert!(
        records(e.home("acme")).is_empty(),
        "a drained daemon spawned"
    );
    assert_eq!(
        e.call(json!({"op": "hello"}))["version"],
        json!(env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn undrain_takes_starts_again() {
    let e = env("undrain", &["acme"]);
    let d = e.daemon(&[("COS_SLEEP_MS", "60000")]);
    assert_eq!(
        e.call(json!({"op": "drain"})),
        json!({"drained": true, "pid": d.pid()})
    );
    assert_eq!(e.call(json!({"op": "undrain"})), json!({"drained": false}));
    e.started("ask", "acme", "0000000000000f21");
}

#[test]
fn a_drained_daemon_waits_out_an_exe_change_until_undrained() {
    let e = env("drainexe", &["acme"]);
    let copy = e.root.join("cos");
    std::fs::copy(COS, &copy).unwrap();
    let mut d = e.daemon_from(&copy, &[], true);
    assert_eq!(e.call(json!({"op": "drain"}))["drained"], json!(true));
    std::fs::File::options()
        .write(true)
        .open(&copy)
        .unwrap()
        .set_modified(std::time::UNIX_EPOCH + Duration::from_secs(1_000_000_000))
        .unwrap();
    std::thread::sleep(Duration::from_secs(3));
    assert!(d.running(), "a drained daemon retired:\n{}", d.log());

    assert_eq!(e.call(json!({"op": "undrain"})), json!({"drained": false}));
    let status = d.wait(Duration::from_secs(15));
    assert_eq!(status.code(), Some(1), "{}", d.log());
}

#[test]
fn drain_fails_naming_a_live_run_and_leaves_it_and_later_starts_alone() {
    let e = env("drainlive", &["acme"]);
    let _d = e.daemon(&[("COS_SLEEP_MS", "60000")]);
    let id = e.started("brief", "acme", "0000000000000f11");
    let refused = e.call(json!({"op": "drain"}));
    let msg = refused["error"].as_str().unwrap_or_default();
    assert!(msg.contains(&id), "{refused}");
    assert_eq!(
        refused["live"],
        json!([{"run": id, "business": "acme"}]),
        "{refused}"
    );
    assert!(!record(e.home("acme"), &id).state.is_final());
    e.started("ask", "acme", "0000000000000f12");
}

#[test]
fn stop_refuses_another_business_and_a_run_the_daemon_does_not_own() {
    let e = env("stopno", &["acme", "beta"]);
    let _d = e.daemon(&[("COS_SLEEP_MS", "60000")]);
    let id = e.started("brief", "acme", "0000000000000d01");
    let wrong = e.call(json!({"op": "stop", "run": id, "business": "beta"}));
    assert!(wrong["error"].is_string(), "{wrong}");
    assert!(!record(e.home("acme"), &id).state.is_final());

    let mine = runs::new_id();
    runs::update(e.home("acme"), &mine, |_| {
        Some(Record::new(&mine, Kind::Ask, "acme", Owner::Itself))
    })
    .unwrap();
    let refused = e.call(json!({"op": "stop", "run": mine, "business": "acme"}));
    let msg = refused["error"].as_str().unwrap();
    assert!(msg.contains("self"), "{msg}");
    assert_eq!(record(e.home("acme"), &mine).state, State::Queued);
}

#[test]
fn idle_connections_close_after_5s_and_at_most_16_are_open() {
    let e = env("conns", &["acme"]);
    let _d = e.daemon(&[]);

    let mut idle = UnixStream::connect(e.sock()).unwrap();
    idle.set_read_timeout(Some(Duration::from_secs(15)))
        .unwrap();
    idle.write_all(b"{\"op\":").unwrap();
    let t = Instant::now();
    let mut buf = Vec::new();
    let _ = idle.read_to_end(&mut buf);
    let waited = t.elapsed();
    assert!(buf.is_empty(), "{buf:?}");
    assert!(
        waited >= Duration::from_millis(4500) && waited < Duration::from_secs(9),
        "{waited:?}"
    );

    let open: Vec<UnixStream> = (0..16)
        .map(|_| UnixStream::connect(e.sock()).unwrap())
        .collect();
    std::thread::sleep(Duration::from_millis(500));
    let mut over = UnixStream::connect(e.sock()).unwrap();
    over.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    let t = Instant::now();
    let mut buf = Vec::new();
    let read = over.read_to_end(&mut buf);
    assert!(read.is_ok() && buf.is_empty(), "{read:?} {buf:?}");
    assert!(
        t.elapsed() < Duration::from_secs(2),
        "the 17th was held open"
    );
    drop(open);

    eventually("a slot frees", Duration::from_secs(15), || {
        try_call(&e.sock(), "{\"op\":\"hello\"}").ok()
    });
}
