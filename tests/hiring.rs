//! Hiring, tracking and the scorecard end to end: `context/agents.json`
//! filling a spawn, the supervisor flagging trouble from the stream, and the
//! ledger every verdict lands in.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use cos::agents::judge::Outcome;
use cos::agents::policy::{self, Explicit};
use cos::agents::watch::Kind;
use cos::agents::{self, Contract, Harness, Request, Spec, State, Status, ledger};

const COS: &str = env!("CARGO_BIN_EXE_cos");
const FAKE_CLAUDE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/fake-claude.sh");

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("cos-hiring-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d.canonicalize().unwrap()
}

fn git(dir: &Path, args: &[&str]) {
    let st = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .status()
        .unwrap();
    assert!(st.success(), "git {args:?}");
}

/// A home with an optional policy, and a repo named `demo-repo`.
fn setup(name: &str, policy: Option<&str>) -> (PathBuf, PathBuf) {
    let root = scratch(name);
    let home = root.join("home");
    if let Some(p) = policy {
        std::fs::create_dir_all(home.join("context")).unwrap();
        std::fs::write(home.join("context/agents.json"), p).unwrap();
    }
    let r = root.join("projects/demo-repo");
    std::fs::create_dir_all(&r).unwrap();
    git(&r, &["init", "-q", "-b", "main"]);
    std::fs::write(r.join("README.md"), "demo\n").unwrap();
    git(&r, &["add", "README.md"]);
    git(&r, &["commit", "-q", "-m", "init"]);
    (home, r)
}

fn request(repo: &Path, prompt: &str) -> Request {
    Request {
        harness: Harness::Claude,
        program: Some(FAKE_CLAUDE.into()),
        model: None,
        repo: Some(repo.to_path_buf()),
        worktree: true,
        title: "t".into(),
        source: "prompt".into(),
        prompt: prompt.into(),
        deadline: Duration::from_secs(30),
        budget_usd: 1.0,
        slots: None,
        contract: Contract::default(),
        explicit: Explicit::default(),
    }
}

fn judged(home: &Path, req: Request) -> (Spec, State) {
    let spec = agents::spawn(home, req, Path::new(COS)).unwrap();
    let until = Instant::now() + Duration::from_secs(30);
    loop {
        let (spec, st) = agents::read(home, &spec.id).unwrap();
        if st.verdict.is_some() {
            return (spec, st);
        }
        assert!(Instant::now() < until, "never judged: {st:?}");
        std::thread::sleep(Duration::from_millis(100));
    }
}

const POLICY: &str = r#"{
  "defaults": { "checks": ["true"], "deadline_secs": 900 },
  "repos": {
    "demo-repo": {
      "checks": ["test -f made-by-agent.txt"],
      "scope": ["made-by-agent.txt"],
      "deadline_secs": 45,
      "stall_secs": 7,
      "repeat": 4
    }
  }
}"#;

#[test]
fn the_repo_policy_fills_what_the_spawn_left_empty() {
    let (home, repo) = setup("policy", Some(POLICY));
    let (spec, st) = judged(&home, request(&repo, "edit a file"));
    assert_eq!(spec.contract.checks, ["test -f made-by-agent.txt"]);
    assert_eq!(spec.contract.scope, ["made-by-agent.txt"]);
    assert_eq!(spec.deadline_secs, 45);
    assert_eq!((spec.limits.stall_secs, spec.limits.repeat), (7, 4));
    assert_eq!(st.verdict.unwrap().outcome, Outcome::Accepted);
}

#[test]
fn what_the_caller_set_wins_over_the_policy() {
    let (home, repo) = setup("explicit", Some(POLICY));
    let mut req = request(&repo, "edit a file");
    req.contract.checks = vec!["true".into()];
    req.deadline = Duration::from_secs(20);
    req.explicit = Explicit {
        deadline: true,
        budget: false,
    };
    let (spec, _) = judged(&home, req);
    assert_eq!(
        spec.contract.checks,
        ["true"],
        "the policy never adds checks"
    );
    assert_eq!(
        spec.contract.scope,
        ["made-by-agent.txt"],
        "scope was left empty"
    );
    assert_eq!(spec.deadline_secs, 20);
}

#[test]
fn a_repo_without_an_entry_takes_the_defaults() {
    let p = serde_json::from_str::<policy::Policy>(POLICY).unwrap();
    let t = p.terms(Some(Path::new("/x/other-repo")));
    assert_eq!(
        (t.checks, t.deadline_secs),
        (vec!["true".to_owned()], Some(900))
    );
    let own = p.terms(Some(Path::new("/x/demo-repo")));
    assert_eq!(own.budget_usd, None, "neither level sets a budget");
}

#[test]
fn a_broken_policy_is_ignored_and_says_why() {
    let (home, _) = setup("broken", Some(r#"{ "defaults": { "chekcs": [] } }"#));
    let p = policy::load(&home);
    assert_eq!(p.defaults, policy::Terms::default());
    assert!(
        p.note.unwrap().contains("unknown field"),
        "a typo must not hire silently"
    );
}

#[test]
fn an_agent_repeating_one_call_is_flagged_looping() {
    let (home, repo) = setup("loop", None);
    let (_, st) = judged(&home, request(&repo, "LOOP please"));
    assert_eq!(st.status, Status::Done);
    assert_eq!(st.flagged, [Kind::Looping]);
    assert!(st.flags.is_empty(), "current flags clear when it ends");
    assert_eq!(ledger::read(&home)[0].flagged, [Kind::Looping]);
}

#[test]
fn an_agent_gone_quiet_is_flagged_stalled_while_it_runs() {
    let (home, repo) = setup("stall", Some(r#"{ "defaults": { "stall_secs": 1 } }"#));
    let spec = agents::spawn(&home, request(&repo, "SLEEP"), Path::new(COS)).unwrap();
    let until = Instant::now() + Duration::from_secs(15);
    loop {
        let (_, st) = agents::read(&home, &spec.id).unwrap();
        if st.flags.iter().any(|f| f.kind == Kind::Stalled) {
            assert_eq!(st.status, Status::Running);
            break;
        }
        assert!(Instant::now() < until, "never flagged: {st:?}");
        std::thread::sleep(Duration::from_millis(100));
    }
    let st = agents::stop(&home, &spec.id, Duration::from_secs(10)).unwrap();
    assert_eq!(st.status, Status::Killed);
}

#[test]
fn the_ledger_outlives_clean_and_a_rejudge_replaces_its_row() {
    let (home, repo) = setup("ledger", None);
    let mut good = request(&repo, "edit a file");
    good.contract.checks = vec!["true".into()];
    let mut bad = request(&repo, "edit a file");
    bad.contract.checks = vec!["false".into()];
    let (a, _) = judged(&home, good);
    let (b, _) = judged(&home, bad);

    agents::rejudge(&home, &a.id).unwrap();
    let raw = std::fs::read_to_string(ledger::path(&home)).unwrap();
    assert_eq!(raw.lines().count(), 3, "the re-judge appended a row");

    agents::clean(&home, &a.id, true).unwrap();
    agents::clean(&home, &b.id, true).unwrap();
    let rows = ledger::read(&home);
    assert_eq!(rows.len(), 2, "one row per agent, worktrees gone");

    let score = &ledger::scorecard(&rows)[0];
    assert_eq!(score.repo.as_deref(), Some("demo-repo"));
    assert_eq!(
        (
            score.runs,
            score.accepted,
            score.rejected,
            score.overclaimed
        ),
        (2, 1, 1, 1)
    );
    assert_eq!(score.median_cost_usd, Some(0.12));
}
