//! Hiring, tracking and the scorecard end to end: `context/agents.json`
//! filling a hire, the verdict cos records from the worktree, and the ledger
//! every verdict lands in. Hires run in a stand-in Herdr (`fake_herdr`) that
//! runs the stand-in agent to the end of its turn.

#[allow(dead_code)]
mod fake_herdr;

#[allow(dead_code)]
#[path = "common/prime.rs"]
mod stand_in;

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
        skills: None,
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
fn the_skills_settings_load_beside_the_roles_and_their_absence_changes_nothing() {
    let extra = scratch("skills-root");
    let with = format!(
        r#"{{ "defaults": {{ "deadline_secs": 900 }},
             "skills": {{ "roots": ["{}", "relative/dir", "/nonexistent/cos-root"], "unreviewed": "off" }} }}"#,
        extra.display()
    );
    let (home, _, f) = setup("skills-with", Some(&with));
    drop(f);
    let p = policy::load(&home);
    assert!(p.note.is_none(), "{:?}", p.note);
    assert_eq!(p.defaults.deadline_secs, Some(900));
    assert!(!p.roles.is_empty());
    let s = p.skills_settings();
    assert_eq!(s.unreviewed, cos::agents::skills::Unreviewed::Off);
    assert_eq!(
        s.roots,
        std::slice::from_ref(&extra),
        "relative and missing roots are left out"
    );
    assert_eq!(s.notes.len(), 2, "{:?}", s.notes);

    let (home, _, f) = setup("skills-without", Some(POLICY));
    drop(f);
    let s = policy::load(&home).skills_settings();
    assert_eq!(s.unreviewed, cos::agents::skills::Unreviewed::On);
    assert!(s.roots.is_empty() && s.notes.is_empty());

    let (home, _, _f) = setup(
        "skills-typo",
        Some(r#"{ "skills": { "unreviewd": "off" } }"#),
    );
    let p = policy::load(&home);
    assert!(p.note.unwrap().contains("unknown field"));
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

/// The user's skill roots in the stand-in's `COS_SKILLS_HOME`, and a
/// stand-in Prime holding rust-perf (builder, requires tdd) and seo-audit
/// (irrelevant).
fn skilled(home: &Path, f: &Fake) -> stand_in::Prime {
    let user = home.parent().unwrap().join("user-home/.claude/skills");
    for (s, d) in [
        ("rust-perf", "Profile hot paths"),
        ("tdd", "Test first"),
        ("seo-audit", "SEO"),
    ] {
        std::fs::create_dir_all(user.join(s)).unwrap();
        std::fs::write(
            user.join(s).join("SKILL.md"),
            format!("---\ndescription: {d}\n---\n"),
        )
        .unwrap();
    }
    let p = stand_in::Prime::start();
    let node = |id: &str, name: &str, verdict: &str, roles: &[&str], requires: &[&str]| {
        serde_json::json!({"id": id, "type": "cos_skill", "properties": {"name": name,
            "verdict": verdict, "roles": roles, "requires": requires,
            "curated_at": "2026-11-28T10:00:00Z", "source": "cos-skills"}})
    };
    p.reply(
        "prime_search",
        serde_json::json!({"nodes": [
            node("a", "rust-perf", "relevant", &["builder"], &["tdd"]),
            node("b", "seo-audit", "irrelevant", &[], &[]),
        ]}),
    );
    f.prime(&p.url());
    p
}

fn task(prompt: &str) -> agents::hire::Ask {
    agents::hire::Ask::Task(agents::Task {
        n: 0,
        title: "t".into(),
        prompt: prompt.into(),
        source: "prompt".into(),
    })
}

fn skill_plan(home: &Path, repo: &Path, asked: cos::agents::skills::Asked) -> agents::hire::Plan {
    agents::hire::plan(
        &agents::hire::Where {
            home,
            workspace: home,
            projects: repo.parent().unwrap(),
        },
        Some("builder"),
        task("Implement the cache"),
        Some(repo.to_path_buf()),
        &Contract::default(),
        &agents::hire::SkillsAsk::from_env(asked, cos::agents::skill_facts::Mode::Live),
    )
    .unwrap()
}

fn run(harness: Harness) -> agents::hire::Run {
    agents::hire::Run {
        harness,
        program: Some(agent()),
        model: None,
        worktree: true,
        deadline: Duration::from_secs(30),
        slots: Some(8),
        explicit: Explicit::default(),
    }
}

#[test]
fn a_plan_resolves_the_roles_skills_and_the_hires_own_and_shows_them() {
    let (home, repo, f) = setup("skill-plan", Some(POLICY));
    let _prime = skilled(&home, &f);
    let p = skill_plan(&home, &repo, Default::default());
    let set = p.skills.as_ref().expect("skills resolved");
    let picked: Vec<&str> = set.picked.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(picked, ["rust-perf", "tdd"]);
    assert_eq!(set.hidden, ["rust-perf", "seo-audit", "tdd"]);
    let json = serde_json::to_value(&p).unwrap();
    assert_eq!(json["skills"]["picked"][0]["why"], "role");
    assert_eq!(json["skills"]["graph"]["state"], "fresh");

    let p = skill_plan(
        &home,
        &repo,
        cos::agents::skills::Asked {
            add: vec!["seo-audit".into()],
            remove: vec!["tdd".into()],
        },
    );
    let set = p.skills.unwrap();
    let picked: Vec<&str> = set.picked.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(picked, ["rust-perf", "seo-audit"]);
    assert!(
        set.warnings
            .iter()
            .any(|w| w.contains("rust-perf requires tdd")),
        "{:?}",
        set.warnings
    );
}

#[test]
fn a_codex_request_carries_the_skills_block_and_a_claude_one_does_not() {
    let (home, repo, f) = setup("skill-codex", Some(POLICY));
    let _prime = skilled(&home, &f);
    let codex = skill_plan(&home, &repo, Default::default())
        .request(run(Harness::Codex))
        .unwrap();
    assert!(
        codex.prompt.contains("Skills for this work"),
        "{}",
        codex.prompt
    );
    assert!(codex.prompt.contains("- rust-perf: Profile hot paths ("));
    assert!(codex.prompt.contains("rust-perf/SKILL.md)"));
    let claude = skill_plan(&home, &repo, Default::default())
        .request(run(Harness::Claude))
        .unwrap();
    assert!(!claude.prompt.contains("Skills for this work"));
    assert!(claude.skills.is_some());
}

#[test]
fn the_skill_set_is_written_to_the_spec_and_an_old_spec_still_reads() {
    let (home, repo, f) = setup("skill-spec", Some(POLICY));
    let _prime = skilled(&home, &f);
    let req = skill_plan(&home, &repo, Default::default())
        .request(run(Harness::Claude))
        .unwrap();
    let planned = req.skills.clone().unwrap();
    let spec = agents::prepare_spec(&at(&home), req).unwrap();
    let raw: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(agents::dir(&home, &spec.id).join("spec.json")).unwrap(),
    )
    .unwrap();
    let back: cos::agents::skills::SkillSet =
        serde_json::from_value(raw["skills"].clone()).unwrap();
    assert_eq!(back, planned);

    let mut old = raw.clone();
    old.as_object_mut().unwrap().remove("skills");
    let old: Spec = serde_json::from_value(old).unwrap();
    assert!(old.skills.is_none());
}

#[test]
fn a_plan_without_prime_still_plans_and_says_the_graph_is_missing() {
    let (home, repo, _f) = setup("skill-noprime", Some(POLICY));
    let p = skill_plan(&home, &repo, Default::default());
    let set = p.skills.unwrap();
    assert!(set.picked.is_empty());
    assert_eq!(set.graph, cos::agents::skills::GraphState::None);
    assert!(
        set.warnings
            .iter()
            .any(|w| w.contains("Prime did not answer")),
        "{:?}",
        set.warnings
    );
    assert!(p.refused.is_none());
}

fn argv_of(home: &Path, f: &Fake, id: &str) -> Vec<String> {
    let st = agents::read(home, id).unwrap().1;
    f.argv(&st.herdr.expect("a Herdr terminal").address)
}

fn flag(argv: &[String], name: &str) -> Option<String> {
    argv.iter()
        .position(|a| a == name)
        .map(|i| argv[i + 1].clone())
}

#[test]
fn a_claude_hire_loads_its_bundle_and_hides_the_rest_outside_its_worktree() {
    let (home, repo, f) = setup("skill-bundle", Some(POLICY));
    let _prime = skilled(&home, &f);
    let req = skill_plan(&home, &repo, Default::default())
        .request(run(Harness::Claude))
        .unwrap();
    let spec = agents::spawn(&at(&home), req, Path::new(COS)).unwrap();
    let run_dir = agents::dir(&home, &spec.id);
    let bundle = run_dir.join("skills");
    assert!(bundle.join(".claude-plugin/plugin.json").is_file());
    let planned = spec.skills.clone().unwrap();
    for p in &planned.picked {
        let copy = bundle.join("skills").join(&p.name);
        assert_eq!(
            cos::agents::skills::tree_digest(&copy).unwrap(),
            *p.digest.as_ref().unwrap()
        );
    }
    assert!(!run_dir.join("skills.tmp").exists());

    let argv = argv_of(&home, &f, &spec.id);
    assert_eq!(
        flag(&argv, "--plugin-dir"),
        Some(bundle.display().to_string())
    );
    let settings: serde_json::Value =
        serde_json::from_str(&flag(&argv, "--settings").unwrap()).unwrap();
    assert_eq!(settings["sandbox"]["enabled"], true, "the sandbox stays");
    let off: Vec<&String> = settings["skillOverrides"]
        .as_object()
        .unwrap()
        .keys()
        .collect();
    assert_eq!(off, ["rust-perf", "seo-audit", "tdd"]);
    assert!(
        settings["skillOverrides"]
            .as_object()
            .unwrap()
            .values()
            .all(|v| v == "off")
    );

    let st = Command::new("git")
        .arg("-C")
        .arg(&spec.workdir)
        .args(["status", "--porcelain"])
        .output()
        .unwrap();
    let dirty = String::from_utf8_lossy(&st.stdout);
    assert!(
        !dirty.contains("skills"),
        "nothing of the bundle in the worktree: {dirty}"
    );
}

#[test]
fn a_hire_whose_role_picks_nothing_still_hides_irrelevant_skills_with_no_plugin_dir() {
    let (home, repo, f) = setup("skill-hide-only", Some(POLICY));
    let _prime = skilled(&home, &f);
    let p = agents::hire::plan(
        &agents::hire::Where {
            home: &home,
            workspace: &home,
            projects: repo.parent().unwrap(),
        },
        Some("reviewer"),
        task("Review the change"),
        Some(repo.clone()),
        &Contract::default(),
        &agents::hire::SkillsAsk::from_env(
            Default::default(),
            cos::agents::skill_facts::Mode::Live,
        ),
    )
    .unwrap();
    let spec = agents::spawn(
        &at(&home),
        p.request(run(Harness::Claude)).unwrap(),
        Path::new(COS),
    )
    .unwrap();
    let argv = argv_of(&home, &f, &spec.id);
    assert_eq!(flag(&argv, "--plugin-dir"), None);
    let settings: serde_json::Value =
        serde_json::from_str(&flag(&argv, "--settings").unwrap()).unwrap();
    assert_eq!(settings["skillOverrides"]["seo-audit"], "off");
    assert!(!agents::dir(&home, &spec.id).join("skills").exists());
}

#[test]
fn a_skill_changed_after_planning_fails_the_hire_before_herdr_starts_it() {
    let (home, repo, f) = setup("skill-changed", Some(POLICY));
    let _prime = skilled(&home, &f);
    let req = skill_plan(&home, &repo, Default::default())
        .request(run(Harness::Claude))
        .unwrap();
    let user = home.parent().unwrap().join("user-home/.claude/skills");
    std::fs::write(user.join("rust-perf/extra.md"), "new").unwrap();
    let err = agents::spawn(&at(&home), req, Path::new(COS)).unwrap_err();
    assert!(
        format!("{err:#}").contains("rust-perf changed since the hire was planned"),
        "{err:#}"
    );
    assert!(
        !f.calls().lines().any(|c| c.starts_with("run")),
        "Herdr never ran it: {:?}",
        f.calls()
    );
}

fn run_dir_of(home: &Path) -> PathBuf {
    home.parent().unwrap().join("run")
}

#[test]
fn bundling_again_rebuilds_and_a_repo_skill_is_neither_copied_nor_hidden() {
    let (home, repo, f) = setup("skill-rebundle", Some(POLICY));
    let _prime = skilled(&home, &f);
    let set = skill_plan(&home, &repo, Default::default()).skills.unwrap();
    let roots = agents::hire::skill_roots(&home);
    let bundle = |set: &cos::agents::skills::SkillSet| {
        cos::agents::skill_bundle::bundle(
            &run_dir_of(&home),
            &repo,
            set,
            &roots,
            Duration::from_secs(60),
        )
    };
    let run_dir = run_dir_of(&home);
    std::fs::create_dir_all(run_dir.join("skills.tmp/skills/rust-perf")).unwrap();
    let first = bundle(&set).unwrap();
    assert_eq!(first, ["rust-perf", "tdd"]);

    let mut forged = set.clone();
    let elsewhere = home.parent().unwrap().join("elsewhere/rust-perf");
    std::fs::create_dir_all(elsewhere.parent().unwrap()).unwrap();
    let real = forged.picked[0].path.clone().unwrap();
    assert!(
        Command::new("cp")
            .arg("-R")
            .arg(&real)
            .arg(&elsewhere)
            .status()
            .unwrap()
            .success()
    );
    forged.picked[0].path = Some(elsewhere.canonicalize().unwrap());
    let e = format!("{:#}", bundle(&forged).unwrap_err());
    assert!(
        e.contains("not in any skill root"),
        "a path from spec.json alone is not trusted: {e}"
    );

    let again = bundle(&set).unwrap();
    assert_eq!(
        again, first,
        "an existing skills/ is replaced, not nested into"
    );
    assert!(!run_dir.join("skills/skills/rust-perf/rust-perf").exists());

    std::fs::create_dir_all(repo.join(".claude/skills/rust-perf")).unwrap();
    std::fs::write(
        repo.join(".claude/skills/rust-perf/SKILL.md"),
        "---\ndescription: repo\n---\n",
    )
    .unwrap();
    let with_repo = bundle(&set).unwrap();
    assert_eq!(with_repo, ["tdd"]);
    let hidden = cos::agents::skill_bundle::hidden(&set, &repo);
    assert!(!hidden.contains(&"rust-perf".to_string()), "{hidden:?}");
    assert!(hidden.contains(&"seo-audit".to_string()));
}

#[test]
fn a_codex_hire_gets_no_plugin_dir_and_no_bundle() {
    let (home, repo, f) = setup("skill-codex-hire", Some(POLICY));
    let _prime = skilled(&home, &f);
    let mut req = skill_plan(&home, &repo, Default::default())
        .request(run(Harness::Codex))
        .unwrap();
    req.program = Some(agent());
    let spec = agents::spawn(&at(&home), req, Path::new(COS)).unwrap();
    let argv = argv_of(&home, &f, &spec.id);
    assert_eq!(flag(&argv, "--plugin-dir"), None);
    assert!(!agents::dir(&home, &spec.id).join("skills").exists());
}

#[test]
fn a_codex_hire_switches_off_irrelevant_agents_skills_and_keeps_the_users_own_entries() {
    let (home, repo, f) = setup("skill-codex-hide", Some(POLICY));
    let p = skilled(&home, &f);
    let agents_root = home
        .parent()
        .unwrap()
        .join("user-home/.agents/skills/gstack-qa");
    std::fs::create_dir_all(&agents_root).unwrap();
    std::fs::write(agents_root.join("SKILL.md"), "---\ndescription: QA\n---\n").unwrap();
    p.reply(
        "prime_search",
        serde_json::json!({"nodes": [
            {"id": "a", "type": "cos_skill", "properties": {"name": "rust-perf", "verdict": "relevant",
              "roles": ["builder"], "curated_at": "2026-11-28T10:00:00Z", "source": "cos-skills"}},
            {"id": "c", "type": "cos_skill", "properties": {"name": "gstack-qa", "verdict": "irrelevant",
              "curated_at": "2026-11-28T10:00:00Z", "source": "cos-skills"}}
        ]}),
    );
    let codex_home = home.parent().unwrap().join("codex-home");
    std::fs::create_dir_all(&codex_home).unwrap();
    std::fs::write(
        codex_home.join("config.toml"),
        "[[skills.config]]\nname = \"mine\"\nenabled = true\n",
    )
    .unwrap();
    let codex_req = || {
        let mut req = skill_plan(&home, &repo, Default::default())
            .request(run(Harness::Codex))
            .unwrap();
        req.program = Some(agent());
        req
    };
    let spec = agents::spawn(&at(&home), codex_req(), Path::new(COS)).unwrap();
    let argv = argv_of(&home, &f, &spec.id);
    let i = argv
        .iter()
        .position(|a| a.starts_with("skills.config="))
        .expect("a skills.config override");
    assert_eq!(argv[i - 1], "-c");
    let value = argv[i].strip_prefix("skills.config=").unwrap();
    let parsed: toml::Table = format!("v = {value}").parse().unwrap();
    let entries = parsed["v"].as_array().unwrap();
    let names: Vec<&str> = entries
        .iter()
        .map(|e| e["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["mine", "gstack-qa"], "{value}");
    assert_eq!(
        entries[0]["enabled"].as_bool(),
        Some(true),
        "the user's entry as it was"
    );
    assert_eq!(entries[1]["enabled"].as_bool(), Some(false));
    let st = agents::read(&home, &spec.id).unwrap().1;
    assert_eq!(st.skills_hidden, ["gstack-qa"]);

    for broken in ["skills = 3\n[[", "[skills]\nconfig = \"nope\"\n"] {
        std::fs::write(codex_home.join("config.toml"), broken).unwrap();
        let runs_before = f.calls().lines().filter(|c| c.starts_with("run")).count();
        let err = agents::spawn(&at(&home), codex_req(), Path::new(COS)).unwrap_err();
        assert!(format!("{err:#}").contains("config.toml"), "{err:#}");
        let runs_after = f.calls().lines().filter(|c| c.starts_with("run")).count();
        assert_eq!(runs_after, runs_before, "Herdr never ran it: {broken:?}");
    }

    use std::os::unix::fs::PermissionsExt;
    std::fs::write(codex_home.join("config.toml"), "").unwrap();
    std::fs::set_permissions(&codex_home, std::fs::Permissions::from_mode(0o000)).unwrap();
    let runs_before = f.calls().lines().filter(|c| c.starts_with("run")).count();
    let denied = agents::spawn(&at(&home), codex_req(), Path::new(COS));
    std::fs::set_permissions(&codex_home, std::fs::Permissions::from_mode(0o755)).unwrap();
    let err = denied.unwrap_err();
    assert!(format!("{err:#}").contains("config.toml"), "{err:#}");
    let runs_after = f.calls().lines().filter(|c| c.starts_with("run")).count();
    assert_eq!(
        runs_after, runs_before,
        "an unreadable config is not read as no config"
    );
}
