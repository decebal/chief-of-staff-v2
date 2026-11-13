//! The judge, end to end: a stand-in harness under the real supervisor in a
//! real git worktree, then the verdict `cos` records from what it left.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use cos::agents::{
    self, Contract, Expects, Harness, Outcome, Request, Spec, State, Status, Verdict,
};

const COS: &str = env!("CARGO_BIN_EXE_cos");
const FAKE_CLAUDE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/fake-claude.sh");

fn at(home: &Path) -> cos::business::Triple {
    cos::business::Triple {
        home: home.to_owned(),
        workspace: home.to_owned(),
        projects: home.join("projects"),
        business: None,
        layer: cos::business::Layer::Flags,
    }
}

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("cos-judge-{name}-{}", std::process::id()));
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

fn repo(root: &Path) -> PathBuf {
    let r = root.join("projects/demo-repo");
    std::fs::create_dir_all(&r).unwrap();
    git(&r, &["init", "-q", "-b", "main"]);
    std::fs::write(r.join("README.md"), "demo\n").unwrap();
    git(&r, &["add", "README.md"]);
    git(&r, &["commit", "-q", "-m", "init"]);
    r
}

/// Runs one fake agent (it writes `made-by-agent.txt` and claims success,
/// unless the prompt says FAIL) and returns its state once judged.
fn run(name: &str, prompt: &str, contract: Contract) -> (State, Verdict) {
    let (_, _, st, v) = judged(name, prompt, contract);
    (st, v)
}

fn judged(name: &str, prompt: &str, contract: Contract) -> (PathBuf, Spec, State, Verdict) {
    let root = scratch(name);
    let (home, repo) = (root.join("home"), repo(&root));
    let spec = agents::spawn(
        &at(&home),
        Request {
            harness: Harness::Claude,
            program: Some(FAKE_CLAUDE.into()),
            model: None,
            repo: Some(repo),
            worktree: true,
            title: "test task".into(),
            source: "test".into(),
            prompt: prompt.into(),
            deadline: Duration::from_secs(30),
            budget_usd: 1.0,
            slots: None,
            contract,
            explicit: Default::default(),
            role: None,
        },
        Path::new(COS),
    )
    .unwrap();
    let until = Instant::now() + Duration::from_secs(30);
    loop {
        let (spec, st) = agents::read(&home, &spec.id).unwrap();
        if let Some(v) = st.verdict.clone() {
            return (home, spec, st, v);
        }
        assert!(Instant::now() < until, "never judged: {st:?}");
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn contract(checks: &[&str], scope: &[&str]) -> Contract {
    Contract {
        done_when: Some("the file exists".into()),
        checks: checks.iter().map(|s| s.to_string()).collect(),
        scope: scope.iter().map(|s| s.to_string()).collect(),
        ..Default::default()
    }
}

fn expecting(expects: Expects, output: &str, checks: &[&str]) -> Contract {
    Contract {
        checks: checks.iter().map(|s| s.to_string()).collect(),
        expects,
        output: Some(output.into()),
        ..Default::default()
    }
}

#[test]
fn a_reviewer_that_leaves_its_file_is_accepted_without_a_diff_to_the_code() {
    let (_, v) = run(
        "expects-file",
        "REVIEW and READONLY",
        expecting(Expects::File, "REVIEW.md", &["test -s REVIEW.md"]),
    );
    assert_eq!(v.outcome, Outcome::Accepted, "{v:?}");
    assert_eq!(v.changed, ["REVIEW.md"]);
}

#[test]
fn an_agent_that_never_writes_its_expected_file_is_rejected_as_overclaiming() {
    let (st, v) = run(
        "expects-missing",
        "READONLY",
        expecting(Expects::File, "REVIEW.md", &["true"]),
    );
    assert_eq!(st.status, Status::Done);
    assert_eq!(v.outcome, Outcome::Rejected);
    assert!(v.overclaimed, "it claimed success");
    assert!(
        v.reasons
            .iter()
            .any(|r| r.contains("`REVIEW.md` is missing")),
        "{v:?}"
    );
}

#[test]
fn a_file_contract_never_asks_for_a_diff() {
    let (_, v) = run(
        "expects-file-nochecks",
        "REVIEW and READONLY",
        expecting(Expects::File, "REVIEW.md", &[]),
    );
    assert_eq!(v.outcome, Outcome::NeedsYou, "no checks still needs you");
    assert!(!v.reasons.iter().any(|r| r.contains("no changes")), "{v:?}");
}

#[test]
fn an_investigator_hired_to_change_nothing_is_judged_on_an_empty_diff() {
    let (_, ok) = run(
        "expects-none",
        "READONLY",
        expecting(Expects::Nothing, "ANSWER.md", &["true"]),
    );
    assert_eq!(ok.outcome, Outcome::Accepted, "{ok:?}");
    assert!(ok.changed.is_empty());

    let (_, bad) = run(
        "expects-none-edit",
        "edit a file",
        expecting(Expects::Nothing, "ANSWER.md", &["true"]),
    );
    assert_eq!(bad.outcome, Outcome::Rejected);
    assert!(
        bad.reasons[0].contains("hired to change nothing, changed 1 file(s): made-by-agent.txt"),
        "{bad:?}"
    );
}

#[test]
fn an_output_outside_the_workdir_is_refused() {
    let (_, v) = run(
        "expects-escape",
        "REVIEW and READONLY",
        expecting(Expects::File, "../REVIEW.md", &["true"]),
    );
    assert_eq!(v.outcome, Outcome::Rejected);
    assert!(
        v.reasons.iter().any(|r| r.contains("not a path inside")),
        "{v:?}"
    );
}

#[test]
fn passing_checks_and_an_in_scope_diff_are_accepted() {
    let (st, v) = run(
        "accepted",
        "edit a file",
        contract(&["test -f made-by-agent.txt"], &["made-by-agent.txt"]),
    );
    assert_eq!(st.status, Status::Done);
    assert_eq!(v.outcome, Outcome::Accepted, "{v:?}");
    assert_eq!(v.changed, ["made-by-agent.txt"]);
    assert!(v.reasons.is_empty() && !v.overclaimed);
    assert!(v.checks[0].passed);
}

#[test]
fn a_failing_check_rejects_an_agent_that_claimed_success() {
    let (st, v) = run(
        "overclaim",
        "edit a file",
        contract(&["test -f made-by-agent.txt", "echo nope; exit 3"], &[]),
    );
    assert_eq!(
        st.status,
        Status::Done,
        "the agent itself said it succeeded"
    );
    assert_eq!(v.outcome, Outcome::Rejected);
    assert!(v.overclaimed);
    assert!(v.checks[0].passed);
    let failed = &v.checks[1];
    assert_eq!((failed.passed, failed.exit), (false, Some(3)));
    assert_eq!(failed.tail, "nope");
    assert!(v.reasons[0].contains("exit 3"), "{:?}", v.reasons);
}

#[test]
fn a_change_outside_scope_needs_you() {
    let (_, v) = run("scope", "edit a file", contract(&["true"], &["src"]));
    assert_eq!(v.outcome, Outcome::NeedsYou);
    assert_eq!(v.out_of_scope, ["made-by-agent.txt"]);
    assert!(!v.overclaimed);
}

#[test]
fn without_checks_nothing_proves_it_done() {
    let (_, v) = run("nochecks", "edit a file", Contract::default());
    assert_eq!(v.outcome, Outcome::NeedsYou);
    assert!(v.reasons.iter().any(|r| r.contains("no checks")), "{v:?}");
    assert_eq!(v.changed, ["made-by-agent.txt"]);
}

#[test]
fn an_agent_that_failed_is_rejected_without_running_its_checks() {
    let (st, v) = run("failed", "FAIL please", contract(&["true"], &[]));
    assert_eq!(st.status, Status::Failed);
    assert_eq!(v.outcome, Outcome::Rejected);
    assert!(v.checks.is_empty());
    assert!(!v.overclaimed, "it did not claim success");
    assert!(v.reasons[0].starts_with("the agent ended failed"), "{v:?}");
}

#[test]
fn a_check_past_the_judging_budget_fails_and_the_rest_are_not_run() {
    let (_, mut spec, st, _) = judged("budget", "edit a file", Contract::default());
    spec.contract.checks = vec!["sleep 5".into(), "true".into()];
    let started = Instant::now();
    let v = agents::judge::judge_within(&spec, &st, None, Duration::from_secs(1));
    assert!(
        started.elapsed() < Duration::from_secs(4),
        "the budget did not hold"
    );
    assert_eq!(v.outcome, Outcome::Rejected);
    assert!(!v.checks[0].passed && v.checks[0].exit.is_none());
    assert!(v.checks[1].tail.contains("budget was spent"), "{v:?}");
}

#[test]
fn rejudge_records_a_new_verdict_after_the_work_is_fixed() {
    let (home, spec, _, first) = judged(
        "rejudge",
        "edit a file",
        contract(&["test -f fixed.txt"], &[]),
    );
    assert_eq!(first.outcome, Outcome::Rejected);

    std::fs::write(spec.workdir.join("fixed.txt"), "by hand\n").unwrap();
    let second = agents::rejudge(&home, &spec.id).unwrap();
    assert_eq!(second.outcome, Outcome::Accepted, "{second:?}");
    let (_, st) = agents::read(&home, &spec.id).unwrap();
    assert_eq!(st.verdict, Some(second));
}
