#![cfg(feature = "stand-in")]

use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use cos::proc::{Deadline, Output, group_alive, start_time};
use serde_json::Value;

const STAND_IN: &str = env!("CARGO_BIN_EXE_cos-stand-in");
const RUN_ID: &str = "r-20261125-091502-k3x9";
const BOUND: Duration = Duration::from_secs(20);

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn stand_in(mode: &str) -> Command {
    let mut cmd = Command::new(STAND_IN);
    cmd.arg(mode)
        .env_remove("COS_OWNER")
        .env_remove("COS_BUSINESS")
        .env_remove("COS_HOME");
    cmd
}

fn run(cmd: &mut Command) -> Output {
    cos::proc::run(cmd, BOUND).unwrap()
}

fn record_path(home: &Path) -> PathBuf {
    home.join("out/run-records").join(format!("{RUN_ID}.json"))
}

fn record(home: &Path) -> Value {
    let path = record_path(home);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap()
}

fn start_str(pid: libc::pid_t) -> String {
    let (sec, usec) = start_time(pid).expect("start time of a live process");
    format!("{sec}.{usec:06}")
}

/// Kills a group the stand-in left behind on drop, under the identity check:
/// only while its leader is gone or still has the recorded start.
struct Group {
    pgid: libc::pid_t,
    start: String,
}

impl Group {
    fn of(rec: &Value) -> Self {
        let g = &rec["groups"][0];
        Group {
            pgid: g["pgid"].as_i64().unwrap() as libc::pid_t,
            start: g["start"].as_str().unwrap().to_owned(),
        }
    }
}

impl Drop for Group {
    fn drop(&mut self) {
        let same = start_time(self.pgid).is_none_or(|(s, u)| format!("{s}.{u:06}") == self.start);
        if group_alive(self.pgid) && same {
            cos::proc::kill_group(self.pgid);
        }
    }
}

fn wait_gone(pgid: libc::pid_t, within: Duration) -> bool {
    let until = Instant::now() + within;
    while group_alive(pgid) {
        if Instant::now() > until {
            return false;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    true
}

fn wait_for_grandchild(home: &Path) -> Value {
    let until = Instant::now() + BOUND;
    loop {
        if let Ok(text) = std::fs::read_to_string(record_path(home))
            && let Ok(v) = serde_json::from_str::<Value>(&text)
            && v["groups"].as_array().is_some_and(|g| !g.is_empty())
        {
            return v;
        }
        assert!(
            Instant::now() < until,
            "stand-in never recorded its grandchild"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn spawn_sleeping(home: &Path, extra: &[(&str, &str)]) -> std::process::Child {
    let mut cmd = stand_in("run");
    cmd.env("COS_HOME", home)
        .env("COS_RUN_RECORD", RUN_ID)
        .env("COS_SLEEP_MS", "60000")
        .env("COS_GRANDCHILD_SECS", "60")
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    for (k, v) in extra {
        cmd.env(k, v);
    }
    cos::proc::spawn_grouped(&mut cmd).unwrap()
}

fn wait_bounded(child: &mut std::process::Child) -> ExitStatus {
    cos::proc::wait_until(child, &Deadline::after(BOUND)).unwrap()
}

#[test]
fn run_mode_exit_zero_writes_done_record() {
    let home = scratch("run-done");
    let out = run(stand_in("run")
        .env("COS_HOME", &home)
        .env("COS_RUN_RECORD", RUN_ID)
        .env("COS_BUSINESS", "acme"));
    assert_eq!(out.status.code(), Some(0), "{}", out.stderr);
    let rec = record(&home);
    assert_eq!(rec["id"], RUN_ID);
    assert_eq!(rec["kind"], "brief");
    assert_eq!(rec["business"], "acme");
    assert_eq!(rec["state"], "done");
    assert_eq!(rec["reason"], Value::Null);
    assert_eq!(rec["groups"], Value::Array(vec![]));
}

#[test]
fn run_mode_accepts_the_argv_cos_is_run_with() {
    let home = scratch("run-argv");
    let dir = home.join("out/run-records");
    std::fs::create_dir_all(&dir).unwrap();
    let queued = serde_json::json!({
        "id": RUN_ID, "kind": "ask", "business": "acme", "state": "queued",
        "reason": null, "owner": "daemon", "owner_pid": 4100,
        "owner_start": "1764058801.120004", "client_req": "req-7",
    });
    std::fs::write(dir.join(format!("{RUN_ID}.json")), queued.to_string()).unwrap();

    let mut cmd = Command::new(STAND_IN);
    cmd.env_remove("COS_OWNER")
        .env_remove("COS_BUSINESS")
        .env_remove("COS_HOME")
        .env("COS_RUN_RECORD", RUN_ID)
        .args(["--home"])
        .arg(&home)
        .args([
            "--workspace",
            "/nonexistent/ws",
            "--projects",
            "/nonexistent/p",
        ])
        .args(["ask", "--", "what --home is next?"]);
    let out = run(&mut cmd);
    assert_eq!(out.status.code(), Some(0), "{}", out.stderr);
    let rec = record(&home);
    assert_eq!(rec["kind"], "ask");
    assert_eq!(rec["business"], "acme");
    assert_eq!(rec["owner"], "daemon");
    assert_eq!(rec["client_req"], "req-7");
    assert_eq!(rec["state"], "done");
    assert_eq!(rec["answer"], "stand-in answer: what --home is next?");
}

#[test]
fn run_mode_exit_code_is_chosen_and_recorded() {
    let home = scratch("run-failed");
    let out = run(stand_in("run")
        .env("COS_HOME", &home)
        .env("COS_RUN_RECORD", RUN_ID)
        .env("COS_EXIT_CODE", "3"));
    assert_eq!(out.status.code(), Some(3));
    let rec = record(&home);
    assert_eq!(rec["state"], "failed");
    assert_eq!(rec["reason"], "exit 3");
}

#[test]
fn run_mode_grandchild_is_in_its_own_group_and_recorded() {
    let home = scratch("run-grandchild");
    let started = Instant::now();
    let out = run(stand_in("run")
        .env("COS_HOME", &home)
        .env("COS_RUN_RECORD", RUN_ID)
        .env("COS_GRANDCHILD_SECS", "60"));
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "grandchild held the pipes"
    );
    assert_eq!(out.status.code(), Some(0));
    let rec = record(&home);
    let groups = rec["groups"].as_array().unwrap();
    assert_eq!(groups.len(), 1, "{rec}");
    let reaper = Group::of(&rec);
    assert!(reaper.pgid > 1);
    assert!(
        group_alive(reaper.pgid),
        "grandchild group should outlive the stand-in"
    );
    assert!(reaper.start.contains('.'));
}

#[test]
fn run_mode_owner_gone_writes_lost_and_kills_grandchild() {
    let home = scratch("run-owner-gone");
    let mut owner = cos::proc::spawn_grouped(
        Command::new("sleep")
            .arg("60")
            .stdout(Stdio::null())
            .stderr(Stdio::null()),
    )
    .unwrap();
    let owner_pid = owner.id() as libc::pid_t;
    let owner_spec = format!("{owner_pid}/{}", start_str(owner_pid));

    let mut child = spawn_sleeping(&home, &[("COS_OWNER", &owner_spec)]);
    let rec = wait_for_grandchild(&home);
    let reaper = Group::of(&rec);

    cos::proc::kill_and_reap(&mut owner);

    let status = wait_bounded(&mut child);
    assert_eq!(status.code(), Some(76));
    let rec = record(&home);
    assert_eq!(rec["state"], "lost");
    assert_eq!(rec["reason"], "owner gone");
    assert!(
        wait_gone(reaper.pgid, Duration::from_secs(5)),
        "grandchild group still alive"
    );
}

#[test]
fn run_mode_sigterm_writes_stopped_and_kills_grandchild() {
    let home = scratch("run-sigterm");
    let mut child = spawn_sleeping(&home, &[]);
    let rec = wait_for_grandchild(&home);
    let reaper = Group::of(&rec);

    cos::proc::signal_pid(child.id() as libc::pid_t, libc::SIGTERM);

    let status = wait_bounded(&mut child);
    assert_eq!(status.code(), Some(143));
    let rec = record(&home);
    assert_eq!(rec["state"], "stopped");
    assert_eq!(rec["reason"], "stopped");
    assert!(
        wait_gone(reaper.pgid, Duration::from_secs(5)),
        "grandchild group still alive"
    );
}

#[test]
fn run_mode_can_ignore_sigterm() {
    use std::os::unix::process::ExitStatusExt;

    let home = scratch("run-ignore-term");
    let mut child = spawn_sleeping(&home, &[("COS_IGNORE_TERM", "1")]);
    let rec = wait_for_grandchild(&home);
    let _reaper = Group::of(&rec);

    cos::proc::signal_pid(child.id() as libc::pid_t, libc::SIGTERM);
    std::thread::sleep(Duration::from_secs(1));
    assert!(
        child.try_wait().unwrap().is_none(),
        "stand-in obeyed SIGTERM"
    );
    assert_eq!(record(&home)["state"], "running");

    cos::proc::signal_pid(child.id() as libc::pid_t, libc::SIGKILL);
    let status = wait_bounded(&mut child);
    assert_eq!(status.signal(), Some(libc::SIGKILL));
}

#[test]
fn launchctl_mode_appends_argv_line_per_call() {
    let dir = scratch("launchctl");
    let log = dir.join("launchctl.log");
    for args in [
        ["bootout", "gui/501", "/x.plist"],
        ["bootstrap", "gui/501", "/x.plist"],
    ] {
        let out = run(stand_in("launchctl")
            .args(args)
            .env("COS_STAND_IN_LOG", &log));
        assert_eq!(out.status.code(), Some(0));
    }
    let text = std::fs::read_to_string(&log).unwrap();
    assert_eq!(
        text,
        "bootout gui/501 /x.plist\nbootstrap gui/501 /x.plist\n"
    );
}

#[test]
fn version_mode_prints_chosen_version() {
    let out = run(stand_in("version").env("COS_STAND_IN_VERSION", "cos 9.8.7"));
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(out.stdout, "cos 9.8.7\n");
}
