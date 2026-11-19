//! Hiring, tracking and the scorecard end to end: `context/agents.json`
//! filling a hire, the verdict cos records from the worktree, and the ledger
//! every verdict lands in. Hires run in a stand-in Herdr (`fake_herdr`) that
//! runs the stand-in agent to the end of its turn.

#[allow(dead_code)]
mod fake_herdr;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use cos::agents::judge::Outcome;
use cos::agents::policy::{self, Explicit};
use cos::agents::{self, Contract, Harness, Request, Spec, State, ledger};
use fake_herdr::{Fake, agent, fake};

const COS: &str = env!("CARGO_BIN_EXE_cos");

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

/// A home with an optional policy, a repo named `demo-repo`, and a stand-in
/// Herdr that runs each hire's agent to the end of its turn.
fn setup(name: &str, policy: Option<&str>) -> (PathBuf, PathBuf, Fake) {
    let root = scratch(name);
    let f = fake(&root);
    f.exec(true);
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
    (home, r, f)
}

fn request(repo: &Path, prompt: &str) -> Request {
    Request {
        harness: Harness::Claude,
        program: Some(agent()),
        model: None,
        repo: Some(repo.to_path_buf()),
        worktree: true,
        title: "t".into(),
        source: "prompt".into(),
        prompt: prompt.into(),
        deadline: Duration::from_secs(30),
        slots: Some(8),
        contract: Contract::default(),
        explicit: Explicit::default(),
        role: None,
    }
}

/// Hires into the stand-in Herdr, follows it once: the agent has finished
/// its turn and said done, so cos judges it and appends the ledger row.
fn judged(home: &Path, req: Request) -> (Spec, State) {
    let spec = agents::spawn(&at(home), req, Path::new(COS)).unwrap();
    agents::tick(home).unwrap();
    let (spec, st) = agents::read(home, &spec.id).unwrap();
    assert!(st.verdict.is_some(), "never judged: {st:?}");
    (spec, st)
}

const POLICY: &str = r#"{
  "defaults": { "checks": ["true"], "deadline_secs": 900 },
  "repos": {
    "demo-repo": {
      "checks": ["test -f made-by-agent.txt"],
      "scope": ["made-by-agent.txt"],
      "deadline_secs": 45
    }
  }
}"#;

#[test]
fn the_repo_policy_fills_what_the_spawn_left_empty() {
    let (home, repo, _f) = setup("policy", Some(POLICY));
    let (spec, st) = judged(&home, request(&repo, "edit a file"));
    assert_eq!(spec.contract.checks, ["test -f made-by-agent.txt"]);
    assert_eq!(spec.contract.scope, ["made-by-agent.txt"]);
    assert_eq!(spec.deadline_secs, 45);
    assert_eq!(st.verdict.unwrap().outcome, Outcome::Accepted);
}

#[test]
fn a_policy_from_before_herdr_still_loads_and_its_budget_is_ignored() {
    let (home, _, _f) = setup(
        "old-budget",
        Some(
            r#"{
          "defaults": { "deadline_secs": 900, "budget_usd": 5, "stall_secs": 600, "repeat": 5 },
          "roles": [{ "id": "reviewer", "title": "Reviewer", "mandate": "Review.",
                      "budget_usd": 2, "stall_secs": 60, "repeat": 3 }],
          "repos": { "demo-repo": { "budget_usd": 9 } }
        }"#,
        ),
    );
    let p = policy::load(&home);
    assert!(
        p.note.is_none(),
        "an old budget must not fail the file: {:?}",
        p.note
    );
    assert_eq!(p.defaults.deadline_secs, Some(900));
    let out = serde_json::to_string(&p.terms_for(None, p.role("reviewer"))).unwrap();
    assert!(
        !out.contains("budget") && !out.contains("stall") && !out.contains("repeat"),
        "no retired term is written back: {out}"
    );
    assert!(
        !policy::SEED.contains("budget_usd"),
        "the template has no budget"
    );
}

#[test]
fn runs_in_herdr_have_no_cost_and_the_scorecard_says_so() {
    let (home, repo, _f) = setup("no-cost", Some(POLICY));
    judged(&home, request(&repo, "edit a file"));
    judged(&home, request(&repo, "edit a file"));
    let rows = ledger::read(&home);
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|r| r.cost_usd.is_none() && r.herdr));
    let score = &ledger::scorecard(&rows)[0];
    assert_eq!(
        (score.median_cost_usd, score.unmeasured),
        (None, 2),
        "Herdr measures no cost, and none is made up"
    );
    assert!(
        agents::view::scorecard(&ledger::scorecard(&rows)).contains("not measured in Herdr"),
        "the scorecard says why there is no cost"
    );
}

#[test]
fn what_the_caller_set_wins_over_the_policy() {
    let (home, repo, _f) = setup("explicit", Some(POLICY));
    let mut req = request(&repo, "edit a file");
    req.contract.checks = vec!["true".into()];
    req.deadline = Duration::from_secs(20);
    req.explicit = Explicit { deadline: true };
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
    assert_eq!(own.deadline_secs, Some(45));
}

#[test]
fn a_broken_policy_is_ignored_and_says_why() {
    let (home, _, _f) = setup("broken", Some(r#"{ "defaults": { "chekcs": [] } }"#));
    let p = policy::load(&home);
    assert_eq!(p.defaults, policy::Terms::default());
    assert!(
        p.note.unwrap().contains("unknown field"),
        "a typo must not hire silently"
    );
}

#[test]
fn the_ledger_outlives_clean_and_a_rejudge_replaces_its_row() {
    let (home, repo, _f) = setup("ledger", None);
    let mut good = request(&repo, "edit a file");
    good.contract.checks = vec!["true".into()];
    let mut bad = request(&repo, "edit a file");
    bad.contract.checks = vec!["false".into()];
    let (a, _) = judged(&home, good);
    let (b, _) = judged(&home, bad);

    agents::rejudge(&home, &a.id).unwrap();
    let raw = std::fs::read_to_string(ledger::path(&home)).unwrap();
    assert_eq!(raw.lines().count(), 3, "the re-judge appended a row");

    for id in [&a.id, &b.id] {
        agents::mark_done(&home, id).unwrap();
        agents::clean(&home, id, true).unwrap();
    }
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
}

#[test]
fn the_template_seeds_ten_roles_under_the_chief_of_staff() {
    let seed: policy::Policy = serde_json::from_str(policy::SEED).expect("the template parses");
    let ids: Vec<&str> = seed.roles.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "chief-of-staff",
            "builder",
            "reviewer",
            "ritual-shaper",
            "ritual-goldfish",
            "researcher",
            "investigator",
            "docs-keeper",
            "release-clerk",
            "tester"
        ]
    );
    assert!(seed.roles[0].head && seed.roles[1..].iter().all(|r| !r.head));
    let clerk = seed.role("release-clerk").unwrap();
    assert!(
        clerk.mandate.contains("Never tag, push, publish or deploy"),
        "the release role drafts only"
    );
    let goldfish = seed.role("ritual-goldfish").unwrap();
    assert_eq!(
        goldfish.scope,
        ["REVIEW.md"],
        "the goldfish writes its review and nothing else"
    );
    assert!(
        goldfish
            .checks
            .iter()
            .any(|c| c.contains("^verdict: (clean|findings)$")),
        "the review carries a verdict the approval reads"
    );
    assert!(
        seed.hire("chief-of-staff")
            .unwrap_err()
            .to_string()
            .contains("it is not hired")
    );
    assert!(
        seed.hire("janitor")
            .unwrap_err()
            .to_string()
            .contains("no role janitor")
    );
}

#[test]
fn a_home_whose_policy_predates_roles_gets_the_template_roles() {
    let (home, _, f) = setup("old-policy", Some(POLICY));
    let p = policy::load(&home);
    assert!(p.note.is_none(), "{:?}", p.note);
    assert_eq!(
        p.repos["demo-repo"].deadline_secs,
        Some(45),
        "its own terms kept"
    );
    assert_eq!(p.roles.len(), 10);
    drop(f);
    let (bare, _, _f) = setup("no-policy", None);
    assert_eq!(policy::load(&bare).roles.len(), 10);
}

#[test]
fn a_role_contract_wins_over_the_repo_and_the_repo_limits_win_over_the_role() {
    let p: policy::Policy = serde_json::from_str(
        r#"{
          "defaults": { "deadline_secs": 900 },
          "roles": [{ "id": "reviewer", "title": "Reviewer", "mandate": "Review.",
                      "expects": "file", "output": "REVIEW.md",
                      "checks": ["test -s REVIEW.md"], "scope": ["REVIEW.md"],
                      "deadline_secs": 300, "budget_usd": 2 }],
          "repos": { "demo-repo": { "checks": ["task test"], "scope": ["src/"], "deadline_secs": 45 } }
        }"#,
    )
    .unwrap();
    let t = p.terms_for(Some(Path::new("/x/demo-repo")), p.role("reviewer"));
    assert_eq!(t.checks, ["test -s REVIEW.md"]);
    assert_eq!(t.scope, ["REVIEW.md"]);
    assert_eq!(t.deadline_secs, Some(45), "the repo knows its build cost");
    let elsewhere = p.terms_for(Some(Path::new("/x/other")), p.role("reviewer"));
    assert_eq!(elsewhere.deadline_secs, Some(300));
}

#[test]
fn a_reviewer_hired_by_role_is_judged_on_its_review_file() {
    let (home, repo, _f) = setup("role-reviewer", Some(POLICY));
    let mut req = request(&repo, "REVIEW and READONLY");
    req.role = Some("reviewer".into());
    let (spec, st) = judged(&home, req);
    assert_eq!(spec.role.as_deref(), Some("reviewer"));
    assert_eq!(spec.contract.expects, agents::Expects::File);
    assert_eq!(spec.contract.output.as_deref(), Some("REVIEW.md"));
    assert_eq!(
        spec.contract.checks,
        ["test -s REVIEW.md"],
        "the role's check, not the repo's"
    );
    assert!(
        spec.prompt.starts_with("Role: Reviewer."),
        "{}",
        spec.prompt
    );
    assert_eq!(st.verdict.unwrap().outcome, Outcome::Accepted);
    let rows = ledger::read(&home);
    assert_eq!(rows[0].role.as_deref(), Some("reviewer"));
    assert_eq!(
        ledger::scorecard(&rows)[0].role.as_deref(),
        Some("reviewer")
    );
}

#[test]
fn a_tester_drafts_the_plan_and_is_judged_on_it_then_you_record_the_result() {
    let (home, repo, _f) = setup("role-tester", None);
    let mut req = request(&repo, "TESTPLAN and READONLY");
    req.role = Some("tester".into());
    let (spec, st) = judged(&home, req);
    assert_eq!(spec.contract.expects, agents::Expects::File);
    assert_eq!(spec.contract.output.as_deref(), Some("TEST-PLAN.md"));
    assert_eq!(
        spec.contract.checks,
        ["test -s TEST-PLAN.md", "grep -qi evidence TEST-PLAN.md"]
    );
    assert!(spec.prompt.starts_with("Role: Tester."), "{}", spec.prompt);
    let v = st.verdict.unwrap();
    assert_eq!(v.outcome, Outcome::Accepted, "{v:?}");
    assert_eq!(v.changed, ["TEST-PLAN.md"]);

    let r = |notes: &str| agents::Recorded {
        outcome: agents::Tested::Failed,
        notes: notes.into(),
        evidence: vec!["https://ci.example/run/1".into(), " ".into()],
        at: String::new(),
    };
    assert!(
        agents::record(&home, &spec.id, r("")).is_ok(),
        "evidence alone is enough"
    );
    let st = agents::record(&home, &spec.id, r("step 3 timed out on staging")).unwrap();
    let rec = st.recorded.unwrap();
    assert_eq!(rec.outcome, agents::Tested::Failed);
    assert_eq!(
        rec.evidence,
        ["https://ci.example/run/1"],
        "blank lines dropped"
    );
    assert!(!rec.at.is_empty());
    agents::rejudge(&home, &spec.id).unwrap();
    let (_, again) = agents::read(&home, &spec.id).unwrap();
    assert_eq!(
        again.recorded.unwrap().notes,
        "step 3 timed out on staging",
        "a re-judge keeps what you recorded"
    );
}

#[test]
fn a_result_is_recorded_only_on_a_judged_run() {
    let (home, repo, _f) = setup("record-live", None);
    let spec = agents::prepare_spec(&at(&home), request(&repo, "edit a file")).unwrap();
    let err = agents::record(
        &home,
        &spec.id,
        agents::Recorded {
            outcome: agents::Tested::Passed,
            notes: "too early".into(),
            evidence: Vec::new(),
            at: String::new(),
        },
    )
    .unwrap_err();
    assert!(err.to_string().contains("once it is judged"), "{err}");
}

#[test]
fn hiring_the_head_or_an_unknown_role_starts_nothing() {
    let (home, repo, _f) = setup("role-refused", None);
    for role in ["chief-of-staff", "janitor"] {
        let mut req = request(&repo, "edit a file");
        req.role = Some(role.into());
        assert!(
            agents::spawn(&at(&home), req, Path::new(COS)).is_err(),
            "{role} was hired"
        );
    }
    assert!(agents::list(&home).unwrap().runs.is_empty());
}

#[test]
fn a_deliverable_hands_off_with_the_kits_done_when() {
    let st: serde_json::Value = serde_json::from_str(cos::state::DEMO_STATE).unwrap();
    let (task, done_when) = agents::deliverable_task(&st, None, "D-1").unwrap();
    assert_eq!(task.title, "Give the workshop deck to the reviewer");
    assert_eq!(task.source, "deliverable D-1");
    assert_eq!(
        done_when.as_deref(),
        Some("Reviewer receives the correct draft and confirms review availability"),
        "the kit writes doneWhen"
    );
    let mut old = st.clone();
    let d = &mut old["deliverables"][0];
    let finish = d["doneWhen"].take();
    d["done_when"] = finish;
    let (_, legacy) = agents::deliverable_task(&old, None, "D-1").unwrap();
    assert_eq!(legacy, done_when, "older state with done_when still reads");
}

#[test]
fn spawn_without_a_source_names_the_real_from_values() {
    let root = scratch("from-msg");
    let out = Command::new(COS)
        .args(["--home"])
        .arg(&root)
        .args(["agents", "spawn"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("--from brief|goals"), "{err}");
}
