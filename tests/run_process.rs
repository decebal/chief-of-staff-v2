//! The run process: run records, the brief lock, the start sweep, the owner
//! watchdog, signals and the self-cleanup that ends every group a run
//! created before its final state is written. The real `cos brief` and
//! `cos ask` run against `fake-claude.sh`; the owner-side cases run the
//! stand-in as the run child.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use cos::proc::{self, Deadline, group_alive};
use cos::runs::{self, Kind, Owner, Record, State};
use serde_json::Value;

const COS: &str = env!("CARGO_BIN_EXE_cos");
const FAKE_CLAUDE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/fake-claude.sh");
const AWAKE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/pmset-awake.sh");
const BOUND: Duration = Duration::from_secs(120);

fn home(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("run-process-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join(".chief-of-staff")).unwrap();
    std::fs::write(
        dir.join(".chief-of-staff/profile.json"),
        r#"{"sources": []}"#,
    )
    .unwrap();
    dir.canonicalize().unwrap()
}

fn cos(home: &Path, mode: &str) -> Command {
    std::fs::write(home.join("fake-mode"), mode).unwrap();
    let mut cmd = Command::new(COS);
    cmd.arg("--home")
        .arg(home)
        .arg("--workspace")
        .arg(home)
        .arg("--projects")
        .arg(home.join("projects"))
        .arg("--no-open")
        .env("COS_CLAUDE", FAKE_CLAUDE)
        .env("COS_PMSET", AWAKE)
        .env("FAKE_CLAUDE_DIR", home)
        .env("CLAUDE_CODE_OAUTH_TOKEN", "fictional-test-token")
        .env_remove("COS_BUSINESS")
        .env_remove("COS_OWNER")
        .env_remove("COS_RUN_ID")
        .env_remove("COS_RUN_RECORD")
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    cmd
}

fn wait(child: &mut Child) -> ExitStatus {
    proc::wait_until(child, &Deadline::after(BOUND)).unwrap()
}

fn records(home: &Path) -> Vec<Record> {
    let Ok(entries) = std::fs::read_dir(home.join(runs::DIR)) else {
        return vec![];
    };
    let mut out: Vec<Record> = entries
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .map(|e| Record::parse(&std::fs::read_to_string(e.path()).unwrap()).unwrap())
        .collect();
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

fn record(home: &Path, id: &str) -> Record {
    runs::read(home, id).unwrap().expect("record")
}

fn eventually<T>(what: &str, mut probe: impl FnMut() -> Option<T>) -> T {
    let until = Instant::now() + Duration::from_secs(60);
    loop {
        if let Some(v) = probe() {
            return v;
        }
        assert!(Instant::now() < until, "never: {what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// The fake claude's group, once it has started.
fn claude_group(home: &Path) -> Reaper {
    let pgid = eventually("fake claude started", || {
        std::fs::read_to_string(home.join("claude.pgid"))
            .ok()
            .and_then(|s| s.trim().parse::<libc::pid_t>().ok())
    });
    Reaper(pgid)
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

/// Kills a group the run left behind, so a failing test leaks nothing.
struct Reaper(libc::pid_t);

impl Drop for Reaper {
    fn drop(&mut self) {
        proc::kill_group(self.0);
    }
}

fn the_running_record(home: &Path) -> Record {
    eventually("a running record with groups", || {
        records(home)
            .into_iter()
            .find(|r| r.state == State::Running && !r.groups.is_empty())
    })
}

#[test]
fn a_run_that_exits_0_ends_its_lingering_grandchild_group_before_done() {
    let h = home("linger");
    let out = proc::run(cos(&h, "linger").arg("brief"), BOUND).unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", out.stderr);
    let claude = claude_group(&h);
    assert!(
        !group_alive(claude.0),
        "the fake claude's leftover member outlived `done`"
    );
    let recs = records(&h);
    assert_eq!(recs.len(), 1, "{recs:?}");
    let r = &recs[0];
    assert_eq!(
        (r.state, r.kind, r.owner),
        (State::Done, Kind::Brief, Owner::Itself)
    );
    assert_eq!(r.business, "explicit");
    assert!(r.pid.is_some() && r.start.is_some());
    assert!(r.groups.iter().any(|g| g.pgid == claude.0), "{r:?}");
    assert!(r.ended.is_some() && r.day.is_some());
    assert!(r.cost_usd.is_some_and(|c| (c - 0.5).abs() < 1e-9));
}

#[test]
fn ctrl_c_on_cos_brief_leaves_no_fake_claude_and_writes_stopped() {
    let h = home("sigint");
    let mut child = proc::spawn_grouped(cos(&h, "hang").arg("brief")).unwrap();
    let claude = claude_group(&h);
    let rec = the_running_record(&h);
    assert!(rec.groups.iter().any(|g| g.pgid == claude.0), "{rec:?}");

    proc::signal_pid(child.id() as libc::pid_t, libc::SIGINT);
    let status = wait(&mut child);
    assert_eq!(status.code(), Some(143));
    assert!(!group_alive(claude.0), "fake claude survived Ctrl-C");
    let rec = record(&h, &rec.id);
    assert_eq!(rec.state, State::Stopped);
    assert_eq!(rec.reason.as_deref(), Some("stopped"));
}

#[test]
fn a_brief_while_the_lock_is_held_exits_73_names_the_holder_and_writes_nothing() {
    let h = home("lock");
    let holder = "r-20261125-091502-k3x9";
    let runs::Taken::Held(_lock) = runs::take_brief_lock(&h, holder).unwrap() else {
        panic!("lock busy in a fresh home");
    };
    let out = proc::run(cos(&h, "ok").arg("brief").stdout(Stdio::piped()), BOUND).unwrap();
    assert_eq!(out.status.code(), Some(73), "{}", out.stderr);
    assert_eq!(out.stdout.trim(), holder);
    assert!(records(&h).is_empty());
    assert!(
        !h.join("invocations").exists(),
        "claude ran under a held lock"
    );
}

#[test]
fn a_sigkilled_brief_is_swept_by_the_next_brief_which_then_runs() {
    let h = home("brief-sweep");
    let mut first = proc::spawn_grouped(cos(&h, "hang").arg("brief")).unwrap();
    let claude = claude_group(&h);
    let old = the_running_record(&h);
    proc::signal_pid(first.id() as libc::pid_t, libc::SIGKILL);
    wait(&mut first);
    assert!(
        group_alive(claude.0),
        "precondition: the orphaned claude lives"
    );

    std::fs::remove_file(h.join("claude.pgid")).unwrap();
    let out = proc::run(cos(&h, "ok").arg("brief"), BOUND).unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", out.stderr);
    assert!(!group_alive(claude.0), "the sweep left the old claude");
    let swept = record(&h, &old.id);
    assert_eq!(swept.state, State::Lost);
    assert_eq!(swept.reason.as_deref(), Some("previous run died"));
    let new: Vec<_> = records(&h).into_iter().filter(|r| r.id != old.id).collect();
    assert_eq!(new.len(), 1);
    assert_eq!(new[0].state, State::Done);
}

#[test]
fn a_sigkilled_ask_is_swept_by_the_next_run_start_for_its_business() {
    let h = home("ask-sweep");
    let mut first =
        proc::spawn_grouped(cos(&h, "hang").args(["ask", "--", "what changed?"])).unwrap();
    let claude = claude_group(&h);
    let old = the_running_record(&h);
    assert_eq!(old.kind, Kind::Ask);
    proc::signal_pid(first.id() as libc::pid_t, libc::SIGKILL);
    wait(&mut first);
    assert!(
        group_alive(claude.0),
        "precondition: the orphaned claude lives"
    );

    let out = proc::run(cos(&h, "ok").args(["ask", "--", "and now?"]), BOUND).unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", out.stderr);
    assert!(!group_alive(claude.0), "the sweep left the old claude");
    let swept = record(&h, &old.id);
    assert_eq!(swept.state, State::Lost);
    assert_eq!(swept.reason.as_deref(), Some("previous run died"));
    let new: Vec<_> = records(&h).into_iter().filter(|r| r.id != old.id).collect();
    assert_eq!(new.len(), 1);
    assert_eq!(new[0].state, State::Done);
    assert_eq!(new[0].answer.as_deref(), Some("Answered."));
}

#[test]
fn sigkill_of_cos_brief_inside_the_spawn_window_never_runs_the_gated_child() {
    let h = home("cos-window");
    let mut child =
        proc::spawn_grouped(cos(&h, "ok").arg("brief").env("COS_GATE_HOLD_MS", "30000")).unwrap();
    let pid: libc::pid_t = eventually("cos held a gated child", || {
        std::fs::read_to_string(h.join("gate.pid"))
            .ok()
            .and_then(|s| s.trim().parse().ok())
    });
    let _g = Reaper(pid);
    assert!(proc::alive(pid));

    proc::signal_pid(child.id() as libc::pid_t, libc::SIGKILL);
    wait(&mut child);
    let until = Instant::now() + Duration::from_secs(5);
    while proc::alive(pid) && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(!proc::alive(pid), "the gated child lives on");
    assert!(
        !h.join("invocations").exists(),
        "claude ran outside a record"
    );
    let recs = records(&h);
    assert_eq!(recs.len(), 1, "{recs:?}");
    assert_eq!(recs[0].state, State::Running);
    assert!(recs[0].groups.is_empty(), "{:?}", recs[0]);
}

#[test]
fn owner_death_kills_the_runs_groups_writes_lost_and_exits_76() {
    let h = home("owner");
    let mut owner = proc::spawn_grouped(
        Command::new("sleep")
            .arg("60")
            .stdout(Stdio::null())
            .stderr(Stdio::null()),
    )
    .unwrap();
    let owner_pid = proc::group_of(&owner);
    let owner_start = proc::start_of(owner_pid).unwrap();
    let id = runs::new_id();
    runs::update(&h, &id, |_| {
        Some(Record {
            owner_pid: Some(owner_pid),
            owner_start: Some(owner_start.clone()),
            ..Record::new(&id, Kind::Brief, "acme", Owner::Daemon)
        })
    })
    .unwrap();

    let mut child = proc::spawn_grouped(
        cos(&h, "hang")
            .arg("brief")
            .env("COS_RUN_RECORD", &id)
            .env("COS_OWNER", format!("{owner_pid}/{owner_start}")),
    )
    .unwrap();
    let claude = claude_group(&h);
    let rec = the_running_record(&h);
    assert_eq!(rec.id, id);
    assert_eq!((rec.owner, rec.business.as_str()), (Owner::Daemon, "acme"));

    proc::kill_and_reap(&mut owner);
    let status = wait(&mut child);
    assert_eq!(status.code(), Some(76));
    assert!(
        !group_alive(claude.0),
        "the run's claude outlived its owner"
    );
    let rec = record(&h, &id);
    assert_eq!(rec.state, State::Lost);
    assert_eq!(rec.reason.as_deref(), Some("owner gone"));
}

#[test]
fn a_run_child_that_loses_the_brief_lock_is_failed_by_its_owner_naming_the_holder() {
    let h = home("child-73");
    std::fs::write(h.join("fake-mode"), "ok").unwrap();
    let holder = "r-20261125-091502-k3x9";
    let runs::Taken::Held(_lock) = runs::take_brief_lock(&h, holder).unwrap() else {
        panic!("lock busy in a fresh home");
    };
    let mut run = cos::pipeline::spawn_run(
        Path::new(COS),
        &cos::pipeline::RunSpec {
            kind: Kind::Brief,
            business: "acme",
            home: &h,
            workspace: &h,
            projects: &h.join("projects"),
            question: None,
            owner: Owner::App,
            env: vec![
                ("COS_CLAUDE".into(), FAKE_CLAUDE.into()),
                ("COS_PMSET".into(), AWAKE.into()),
                ("FAKE_CLAUDE_DIR".into(), h.clone().into()),
                (
                    "CLAUDE_CODE_OAUTH_TOKEN".into(),
                    "fictional-test-token".into(),
                ),
            ],
            client_req: None,
            budget: None,
        },
    )
    .unwrap();
    let ended = run.wait_until(BOUND).unwrap();
    assert_eq!(ended.status.code(), Some(73));
    let rec = record(&h, run.id());
    assert_eq!(rec.state, State::Failed);
    assert_eq!(
        rec.reason.as_deref(),
        Some("brief already running: r-20261125-091502-k3x9")
    );
    assert!(
        !h.join("invocations").exists(),
        "claude ran under a held lock"
    );
}

/// `cos ask` on `h` as a run child this test owns, against the fake claude.
fn owned_ask(h: &Path, mode: &str, budget: Option<f64>) -> cos::pipeline::RunChild {
    std::fs::write(h.join("fake-mode"), mode).unwrap();
    cos::pipeline::spawn_run(
        Path::new(COS),
        &cos::pipeline::RunSpec {
            kind: Kind::Ask,
            business: "acme",
            home: h,
            workspace: h,
            projects: &h.join("projects"),
            question: Some("what changed?"),
            owner: Owner::App,
            env: vec![
                ("COS_CLAUDE".into(), FAKE_CLAUDE.into()),
                ("COS_PMSET".into(), AWAKE.into()),
                ("FAKE_CLAUDE_DIR".into(), h.to_owned().into()),
                (
                    "CLAUDE_CODE_OAUTH_TOKEN".into(),
                    "fictional-test-token".into(),
                ),
            ],
            client_req: None,
            budget,
        },
    )
    .unwrap()
}

#[test]
fn a_cos_ask_in_a_hired_terminal_runs_under_a_record_of_its_own() {
    let h = home("hire-env");
    let out = proc::run(
        cos(&h, "ok")
            .args(["ask", "--", "what changed?"])
            .env("COS_RUN_ID", "20261128-101500-ab12"),
        BOUND,
    )
    .unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", out.stderr);
    let recs = records(&h);
    assert_eq!(recs.len(), 1, "{recs:?}");
    assert_eq!((recs[0].state, recs[0].kind), (State::Done, Kind::Ask));
}

#[test]
fn a_real_run_stopped_at_its_deadline_records_failed_with_reason_deadline() {
    let h = home("real-deadline");
    let mut run = owned_ask(&h, "hang", None);
    let claude = claude_group(&h);
    assert!(run.wait_until(Duration::from_secs(3)).is_err());
    assert!(!group_alive(claude.0), "fake claude outlived the deadline");
    let rec = record(&h, run.id());
    assert_eq!(rec.state, State::Failed, "{rec:?}");
    assert_eq!(rec.reason.as_deref(), Some("deadline"));
}

#[test]
fn the_run_childs_children_get_its_budget_but_never_its_record_id_or_owner() {
    let h = home("child-env");
    let mut run = owned_ask(&h, "ok", Some(0.5));
    let ended = run.wait_until(BOUND).unwrap();
    assert_eq!(ended.status.code(), Some(0), "{}", ended.said);
    let args = std::fs::read_to_string(h.join("last-args")).unwrap();
    assert!(args.contains("--max-budget-usd 0.5"), "{args}");
    let env = std::fs::read_to_string(h.join("child-env")).unwrap();
    assert!(!env.contains("COS_RUN_RECORD="), "{env}");
    assert!(!env.contains("COS_OWNER="), "{env}");
}

#[test]
fn a_brief_meeting_a_probe_over_a_dead_holder_takes_the_lock_once_the_probe_lets_go() {
    use std::os::fd::AsRawFd;
    let h = home("dead-holder");
    let dead = "r-20261125-080000-dead";
    runs::update(&h, dead, |_| {
        Some(Record {
            state: State::Running,
            pid: Some(999_999),
            start: Some("1.000001".into()),
            ..Record::new(dead, Kind::Brief, "acme", Owner::Itself)
        })
    })
    .unwrap();
    let lock = h.join(runs::DIR).join("brief.lock");
    std::fs::write(&lock, dead).unwrap();
    let probe = std::fs::File::open(&lock).unwrap();
    // SAFETY: flock on a descriptor this test owns.
    let held = unsafe { libc::flock(probe.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    assert_eq!(held, 0);
    let release = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(300));
        drop(probe);
    });
    let taken = runs::take_brief_lock(&h, &runs::new_id()).unwrap();
    release.join().unwrap();
    match taken {
        runs::Taken::Held(_) => {}
        runs::Taken::Busy(holder) => panic!("busy on a dead holder: {holder}"),
    }
}

#[cfg(feature = "stand-in")]
mod owned {
    use super::*;
    use cos::pipeline::{RunSpec, spawn_run};

    const STAND_IN: &str = env!("CARGO_BIN_EXE_cos-stand-in");

    fn spec<'a>(h: &'a Path, env: &[(&str, &str)]) -> RunSpec<'a> {
        RunSpec {
            kind: Kind::Brief,
            business: "acme",
            home: h,
            workspace: h,
            projects: h,
            question: None,
            owner: Owner::App,
            env: env
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).into()))
                .collect(),
            client_req: None,
            budget: None,
        }
    }

    #[test]
    fn sigkill_of_the_run_child_leaves_its_owner_to_end_the_grandchild_group() {
        let h = home("child-death");
        let mut run = spawn_run(
            Path::new(STAND_IN),
            &spec(
                &h,
                &[("COS_GRANDCHILD_SECS", "60"), ("COS_SLEEP_MS", "60000")],
            ),
        )
        .unwrap();
        let rec = the_running_record(&h);
        let g = Reaper(rec.groups[0].pgid);
        assert!(group_alive(g.0));

        proc::signal_pid(run.pid(), libc::SIGKILL);
        let ended = run.wait_until(BOUND).unwrap();
        assert_eq!(ended.status.code(), None);
        assert!(!group_alive(g.0), "the owner left the grandchild group");
        let rec = record(&h, run.id());
        assert_eq!(rec.state, State::Failed);
        assert_eq!(rec.reason.as_deref(), Some("signal 9"));
        assert_eq!(rec.owner, Owner::App);
    }

    #[test]
    fn sigkill_inside_the_spawn_window_never_execs_the_grandchild() {
        let h = home("window");
        let mark = h.join("execd");
        let mut run = spawn_run(
            Path::new(STAND_IN),
            &spec(
                &h,
                &[
                    ("COS_GRANDCHILD_SECS", "60"),
                    ("COS_GATE_HOLD_MS", "30000"),
                    ("COS_GRANDCHILD_MARK", mark.to_str().unwrap()),
                ],
            ),
        )
        .unwrap();
        let pid: libc::pid_t = eventually("the gated grandchild forked", || {
            std::fs::read_to_string(h.join("gate.pid"))
                .ok()
                .and_then(|s| s.trim().parse().ok())
        });
        let _g = Reaper(pid);
        assert!(proc::alive(pid));

        proc::signal_pid(run.pid(), libc::SIGKILL);
        run.wait_until(BOUND).unwrap();
        let until = Instant::now() + Duration::from_secs(5);
        while proc::alive(pid) && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(!proc::alive(pid), "the gated grandchild lives on");
        assert!(!mark.exists(), "the grandchild ran outside a record");
        let rec = record(&h, run.id());
        assert!(rec.groups.is_empty(), "{rec:?}");
        assert_eq!(rec.state, State::Failed);
    }

    #[test]
    fn a_gated_grandchild_execs_once_its_group_is_recorded() {
        let h = home("window-open");
        let mark = h.join("execd");
        let mut run = spawn_run(
            Path::new(STAND_IN),
            &spec(
                &h,
                &[
                    ("COS_GRANDCHILD_SECS", "60"),
                    ("COS_GATE_HOLD_MS", "200"),
                    ("COS_SLEEP_MS", "1000"),
                    ("COS_GRANDCHILD_MARK", mark.to_str().unwrap()),
                ],
            ),
        )
        .unwrap();
        let ended = run.wait_until(BOUND).unwrap();
        assert_eq!(ended.status.code(), Some(0));
        let rec = record(&h, run.id());
        assert_eq!(rec.state, State::Done);
        assert_eq!(rec.groups.len(), 1);
        let _g = Reaper(rec.groups[0].pgid);
        assert!(mark.exists(), "the released grandchild never ran");
        assert!(
            gone_within(rec.groups[0].pgid, Duration::from_secs(5)),
            "the owner's reap left the grandchild group"
        );
    }

    #[test]
    fn the_queued_record_carries_the_owner_and_the_child_env_names_it() {
        let h = home("queued");
        let mut run = spawn_run(Path::new(STAND_IN), &spec(&h, &[])).unwrap();
        run.wait_until(BOUND).unwrap();
        let rec = record(&h, run.id());
        assert_eq!(rec.owner, Owner::App);
        assert_eq!(rec.owner_pid, Some(std::process::id() as libc::pid_t));
        assert_eq!(
            rec.owner_start,
            proc::start_of(std::process::id() as libc::pid_t)
        );
        assert_eq!(rec.business, "acme");
        assert_eq!(rec.state, State::Done);
        let _: Value = serde_json::to_value(&rec).unwrap();
    }
}
