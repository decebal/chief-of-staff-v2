//! A brief across sleep: the deadline reads the wall clock, a dark wake
//! defers the run before any model call, one transient failure gets one
//! retry, and every outcome leaves a receipt.

use std::cell::Cell;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant, SystemTime};

use cos::pipeline::{Bins, Config, Deferred, RunReceipt, RunStatus};
use cos::proc::{self, Clock};
use cos::store::{self, Kind};
use cos::wake::{self, Wake};

const FAKE_CLAUDE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/fake-claude.sh");

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("cos-sleep-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d.canonicalize().unwrap()
}

fn script(dir: &Path, name: &str, body: &str) -> PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    p
}

/// A home whose workspace approves no source, so no `gh` or `cn` runs, with
/// `pmset` reporting `caps` and a `caffeinate` that records its pid.
fn setup(name: &str, caps: &str, mode: Option<&str>) -> (Config, PathBuf) {
    let home = scratch(name);
    let rt = home.join(".chief-of-staff");
    std::fs::create_dir_all(&rt).unwrap();
    std::fs::write(rt.join("profile.json"), r#"{"sources": []}"#).unwrap();
    let bin = home.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let pmset = script(
        &bin,
        "pmset",
        &format!("echo 'Current System Capabilities are: {caps}'\necho 'Current Power State: 4'"),
    );
    let caffeinate = script(
        &bin,
        "caffeinate",
        &format!(
            "echo $$ > {}/caffeinate.pid\nexec /bin/sleep 60",
            home.display()
        ),
    );
    if let Some(m) = mode {
        std::fs::write(home.join("fake-mode"), m).unwrap();
    }
    let cfg = Config {
        home: home.clone(),
        install: home.clone(),
        workspace: home.clone(),
        projects: home.join("projects"),
        budget: None,
        bins: Bins {
            claude: FAKE_CLAUDE.into(),
            pmset: Some(pmset),
            caffeinate: Some(caffeinate),
        },
    };
    (cfg, home)
}

fn invocations(home: &Path) -> usize {
    std::fs::read_to_string(home.join("invocations"))
        .map(|s| s.lines().count())
        .unwrap_or(0)
}

fn receipt(home: &Path) -> RunReceipt {
    store::latest::<RunReceipt>(home, Kind::Runs)
        .unwrap()
        .expect("a run receipt")
        .value
}

fn audit(home: &Path) -> String {
    let dir = home.join("out/audit");
    std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| std::fs::read_to_string(e.path()).unwrap())
        .collect()
}

/// `Instant` frozen, as macOS uptime is while asleep; the wall clock jumps
/// once `slept` is set.
struct Asleep {
    mono: Instant,
    wall: SystemTime,
    slept: Cell<bool>,
}

impl Clock for Asleep {
    fn mono(&self) -> Instant {
        self.mono
    }
    fn wall(&self) -> SystemTime {
        if self.slept.get() {
            self.wall + Duration::from_secs(4 * 60 * 60)
        } else {
            self.wall
        }
    }
}

#[test]
fn the_wall_clock_ends_a_run_when_instant_stands_still() {
    let clock = Asleep {
        mono: Instant::now(),
        wall: SystemTime::now(),
        slept: Cell::new(false),
    };
    let mut cmd = Command::new("/bin/sh");
    cmd.args(["-c", "echo $$; exec /bin/sleep 60"]);
    let mut pid = 0;
    let started = Instant::now();
    let r = proc::stream_lines_on(&clock, &mut cmd, Duration::from_secs(300), |line| {
        pid = line.trim().parse().unwrap();
        clock.slept.set(true);
        Ok(true)
    });
    let err = r.expect_err("the deadline passed on the wall clock");
    assert!(err.to_string().contains("timed out after 300s"), "{err:#}");
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "waited on Instant"
    );
    assert!(pid > 0 && !proc::alive(pid), "child {pid} not reaped");
}

#[test]
fn a_deadline_is_capped_and_reads_both_clocks() {
    let clock = Asleep {
        mono: Instant::now(),
        wall: SystemTime::now(),
        slept: Cell::new(false),
    };
    let d = proc::Deadline::on(&clock, Duration::from_secs(9999));
    assert_eq!(d.total, proc::CEILING);
    assert!(!d.expired());
    assert!(d.slice() <= Duration::from_secs(1));
    clock.slept.set(true);
    assert!(d.expired());
}

#[test]
fn pmset_capabilities_tell_a_dark_wake() {
    let full =
        "Current System Capabilities are: CPU Graphics Audio Network \nCurrent Power State: 4";
    assert_eq!(wake::parse(full), Wake::Full);
    let dark = "Current System Capabilities are: CPU Network \nCurrent Power State: 4";
    assert_eq!(wake::parse(dark), Wake::Dark);
    assert!(matches!(wake::parse("garbage"), Wake::Unknown(_)));
    assert_eq!(wake::probe(None), Wake::Full);
}

#[test]
fn a_dark_wake_defers_the_brief_without_a_model_call() {
    let (cfg, home) = setup("dark", "CPU Network", Some("ok"));
    let err = cfg.brief().err().expect("deferred");
    assert!(err.is::<Deferred>(), "{err:#}");
    assert_eq!(err.to_string(), "Mac asleep (dark wake): brief deferred");
    assert_eq!(invocations(&home), 0, "claude ran in a dark wake");
    assert!(
        !home.join("caffeinate.pid").exists(),
        "held a dark wake awake"
    );
    let r = receipt(&home);
    assert_eq!(r.status, RunStatus::Deferred);
    assert_eq!((r.attempts, r.cost_usd), (0, 0.0));
    let page = std::fs::read_to_string(home.join("out/report.html")).unwrap();
    assert!(
        page.contains("Today&#39;s brief didn&#39;t run:")
            || page.contains("Today's brief didn't run:")
    );
    assert!(page.contains("brief deferred"));
}

#[test]
fn each_transient_failure_gets_one_retry_then_succeeds() {
    for (i, msg) in [
        "Request timed out",
        "API Error: Your computer went to sleep mid-response.",
        "API Error: read ECONNRESET",
        "MCP server claude.ai Gmail sent no response or progress for 472s; aborting",
    ]
    .iter()
    .enumerate()
    {
        let (cfg, home) = setup(
            &format!("once-{i}"),
            "CPU Graphics Audio Network",
            Some(&format!("once:{msg}")),
        );
        let done = cfg.brief().unwrap_or_else(|e| panic!("{msg}: {e:#}"));
        assert_eq!(invocations(&home), 2, "{msg}");
        assert!(
            (done.cost_usd - 0.75).abs() < 1e-9,
            "{msg}: cost {}",
            done.cost_usd
        );
        let r = receipt(&home);
        assert_eq!((r.status, r.attempts), (RunStatus::Ok, 2), "{msg}");
        assert!((r.cost_usd - 0.75).abs() < 1e-9);
        let log = audit(&home);
        assert_eq!(log.matches("\"result\":").count(), 2, "{log}");
        assert!(log.contains("\"retry\":2"), "{log}");
    }
}

#[test]
fn a_second_transient_failure_is_not_retried_again() {
    let (cfg, home) = setup("always", "CPU Graphics", Some("always:Request timed out"));
    let err = cfg.brief().err().expect("failed");
    assert_eq!(invocations(&home), 2);
    assert!(format!("{err:#}").contains("retried once after"), "{err:#}");
    let r = receipt(&home);
    assert_eq!((r.status, r.attempts), (RunStatus::Failed, 2));
    assert!((r.cost_usd - 0.5).abs() < 1e-9);
    assert_eq!(r.reason, "Request timed out");
}

#[test]
fn a_failed_question_reports_the_cost_of_both_attempts() {
    let (cfg, home) = setup(
        "ask-always",
        "CPU Graphics",
        Some("always:Request timed out"),
    );
    let err = cfg.ask("What changed?").err().expect("failed");
    assert_eq!(invocations(&home), 2);
    let message = format!("{err:#}");
    assert!(message.contains("$0.5000"), "{message}");
    assert!(message.contains("Request timed out"), "{message}");
}

#[test]
fn a_failed_notion_listing_reports_the_cost_of_both_attempts() {
    let (cfg, home) = setup(
        "notion-always",
        "CPU Graphics",
        Some("always:Request timed out"),
    );
    std::fs::write(
        home.join(".chief-of-staff/profile.json"),
        r#"{"sources": [], "monitoredRepos": ["example/project"]}"#,
    )
    .unwrap();
    let err = cfg.notion_pages().expect_err("failed");
    assert_eq!(invocations(&home), 2);
    let message = format!("{err:#}");
    assert!(message.contains("$0.5000"), "{message}");
    assert!(message.contains("Request timed out"), "{message}");
}

#[test]
fn a_budget_stop_is_never_retried() {
    let (cfg, home) = setup("budget", "CPU Graphics", Some("budget"));
    let err = cfg.brief().err().expect("failed");
    assert_eq!(invocations(&home), 1, "{err:#}");
    let r = receipt(&home);
    assert_eq!((r.status, r.attempts), (RunStatus::Failed, 1));
    assert!(r.reason.contains("maximum budget"), "{}", r.reason);
    assert!(!audit(&home).contains("\"retry\""));
}

#[test]
fn the_sleep_assertion_lives_for_the_run_and_is_reaped() {
    let (cfg, home) = setup("hold", "CPU Graphics", Some("ok"));
    std::fs::write(home.join("fake-hold"), "").unwrap();
    cfg.brief().unwrap();
    assert!(
        home.join("held").exists(),
        "no assertion held during the run"
    );
    let pid: libc::pid_t = std::fs::read_to_string(home.join("caffeinate.pid"))
        .expect("caffeinate started")
        .trim()
        .parse()
        .unwrap();
    assert!(!proc::alive(pid), "caffeinate {pid} outlived the run");
    assert_eq!(receipt(&home).status, RunStatus::Ok);
}

#[test]
fn hold_passes_its_own_pid_to_caffeinate() {
    let dir = scratch("args");
    let out = dir.join("args");
    let fake = script(
        &dir,
        "caffeinate",
        &format!("echo \"$@\" > {}\nexec /bin/sleep 60", out.display()),
    );
    let hold = wake::Hold::start(Some(&fake));
    let pid = hold.pid().expect("spawned");
    let until = Instant::now() + Duration::from_secs(5);
    let written = || std::fs::read_to_string(&out).is_ok_and(|s| s.ends_with('\n'));
    while !written() && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(20));
    }
    drop(hold);
    assert!(!proc::alive(pid));
    let args = std::fs::read_to_string(&out).unwrap();
    assert_eq!(args.trim(), format!("-i -w {}", std::process::id()));
}
