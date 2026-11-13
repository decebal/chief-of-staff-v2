//! The pre-hire check: which role can do which work, where the repo comes
//! from, and what an agent is held to when the deliverable's finish line is
//! outside its sandbox. Fixtures are fictional.

use std::path::{Path, PathBuf};
use std::process::Command;

use cos::agents::fit::{self, Call, Work};
use cos::agents::hire::{self, Ask, Where};
use cos::agents::policy;
use cos::agents::{Contract, Task};
use serde_json::{Value, json};

fn pinned(home: &Path) -> cos::business::Triple {
    cos::business::Triple {
        home: home.to_owned(),
        workspace: home.to_owned(),
        projects: home.join("projects"),
        business: None,
        layer: cos::business::Layer::Flags,
    }
}

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("cos-fit-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d.canonicalize().unwrap()
}

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
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
        .unwrap()
        .success();
    assert!(ok, "git {args:?}");
}

/// A checkout under `projects` whose origin is `github.com/<slug>`.
fn checkout(projects: &Path, slug: &str) -> PathBuf {
    let name = slug.rsplit('/').next().unwrap();
    let r = projects.join(name);
    std::fs::create_dir_all(&r).unwrap();
    git(&r, &["init", "-q", "-b", "main"]);
    git(
        &r,
        &[
            "remote",
            "add",
            "origin",
            &format!("https://github.com/{slug}.git"),
        ],
    );
    std::fs::write(r.join("README.md"), "fixture\n").unwrap();
    git(&r, &["add", "README.md"]);
    git(&r, &["commit", "-q", "-m", "init"]);
    r
}

fn deliverable(id: &str, goal: &str, title: &str, done_when: &str) -> Value {
    json!({ "id": id, "goalId": goal, "title": title, "owner": "Ana", "ownerBasis": "confirmed",
        "dueDate": null, "dueBasis": "unknown", "week": "2026-10-26", "blocker": null,
        "dependencies": [], "doneWhen": done_when, "status": "open",
        "source": format!("https://tracker.example/browse/{}", title.split(':').next().unwrap()),
        "evidenceAt": "2026-10-26T09:00:00Z", "reviewedAt": null, "meetingAt": null,
        "completionEvidence": null })
}

/// A workspace with a goal that names where it lives, two deliverables,
/// and a profile mapping the PAY project to a repo.
fn workspace(root: &Path) -> (PathBuf, PathBuf, PathBuf) {
    let (home, ws, projects) = (root.join("home"), root.join("ws"), root.join("projects"));
    for d in [&home, &ws, &projects] {
        std::fs::create_dir_all(d).unwrap();
    }
    cos::state::initialize(&ws).unwrap();
    let mut profile = cos::state::profile(&ws).unwrap();
    profile["monitoredRepos"] = json!(["acme/widget-sdk", "acme/payments"]);
    profile["jiraProjects"] = json!(["WID", { "key": "PAY", "repos": ["acme/payments"] }]);
    cos::state::atomic_write(&ws.join(".chief-of-staff/profile.json"), &profile).unwrap();
    let mut st: Value = serde_json::from_str(cos::state::DEMO_STATE).unwrap();
    st["goals"][0]["whereItLives"] = json!("acme/widget-sdk (develop); tracker WID");
    let goal = st["goals"][0]["id"].as_str().unwrap().to_owned();
    let mut other = st["goals"][0].clone();
    other["id"] = json!("G-9");
    other["title"] = json!("Fictional second goal");
    other.as_object_mut().unwrap().remove("whereItLives");
    st["goals"].as_array_mut().unwrap().push(other);
    st["deliverables"] = json!([
        deliverable(
            "D-1",
            &goal,
            "WID-42: Test the published widget SDK package against the staging video conference",
            "WID-42 resolved in Jira"
        ),
        deliverable(
            "D-2",
            "G-9",
            "PAY-7: Fix the rounding bug in refunds",
            "PAY-7 merged"
        ),
        deliverable(
            "D-3",
            "G-9",
            "Refresh the onboarding guide",
            "The guide matches what shipped"
        ),
    ]);
    cos::state::commit(&ws, &st, 0, "Fictional fixture").unwrap();
    (home, ws, projects)
}

fn plan(at: &Where, role: &str, ask: Ask) -> hire::Plan {
    hire::plan(at, Some(role), ask, None, &Contract::default()).unwrap()
}

fn own(prompt: &str) -> Ask {
    Ask::Task(Task {
        n: 0,
        title: prompt.lines().next().unwrap().into(),
        prompt: prompt.into(),
        source: "desktop".into(),
    })
}

#[test]
fn a_docs_keeper_on_a_test_deliverable_is_refused_and_the_tester_suggested() {
    let root = scratch("docs-keeper");
    let (home, ws, projects) = workspace(&root);
    checkout(&projects, "acme/widget-sdk");
    let at = Where {
        home: &home,
        workspace: &ws,
        projects: &projects,
    };
    let p = plan(&at, "docs-keeper", Ask::Deliverable("D-1".into()));
    assert_eq!(p.fit.call, Call::Misfit, "{p:?}");
    assert_eq!(p.fit.suggest.as_deref(), Some("tester"));
    assert!(p.fit.kinds.contains(&"test".to_owned()));
    let refused = p.refused.unwrap();
    assert!(refused.contains("hire the Tester instead"), "{refused}");

    let builder = plan(&at, "builder", Ask::Deliverable("D-1".into()));
    assert_eq!(
        builder.fit.call,
        Call::Misfit,
        "a builder can't reach staging"
    );
    assert!(builder.fit.reason.unwrap().contains("a live system"));
    assert_eq!(builder.fit.suggest.as_deref(), Some("tester"));

    let tester = plan(&at, "tester", Ask::Deliverable("D-1".into()));
    assert_eq!(tester.fit.call, Call::Fit, "{tester:?}");
    assert!(tester.refused.is_none(), "{:?}", tester.refused);
    assert!(
        tester.fit.notes.iter().any(|n| n.contains("you run it")),
        "{:?}",
        tester.fit.notes
    );
}

#[test]
fn the_repo_comes_from_the_goal_then_the_ticket_and_none_is_a_refusal() {
    let root = scratch("resolve");
    let (home, ws, projects) = workspace(&root);
    let at = Where {
        home: &home,
        workspace: &ws,
        projects: &projects,
    };
    let missing = plan(&at, "tester", Ask::Deliverable("D-1".into()));
    assert_eq!(missing.repo, None);
    assert!(
        missing
            .refused
            .as_deref()
            .unwrap()
            .contains("pick the repo"),
        "no checkout, no hire: {:?}",
        missing.refused
    );

    let sdk = checkout(&projects, "acme/widget-sdk");
    let pay = checkout(&projects, "acme/payments");
    let from_goal = plan(&at, "tester", Ask::Deliverable("D-1".into()));
    assert_eq!(from_goal.repo.as_deref(), Some(sdk.as_path()));
    assert!(
        from_goal.repo_from.starts_with("goal "),
        "{}",
        from_goal.repo_from
    );

    let from_ticket = plan(&at, "builder", Ask::Deliverable("D-2".into()));
    assert_eq!(from_ticket.repo.as_deref(), Some(pay.as_path()));
    assert_eq!(from_ticket.repo_from, "ticket PAY-7's project");

    let none = plan(&at, "docs-keeper", Ask::Deliverable("D-3".into()));
    assert_eq!(none.repo, None);
    assert!(none.refused.is_some(), "never a silent scratch directory");

    let picked = hire::plan(
        &at,
        Some("docs-keeper"),
        Ask::Deliverable("D-3".into()),
        Some(sdk.clone()),
        &Contract::default(),
    )
    .unwrap();
    assert_eq!(picked.repo.as_deref(), Some(sdk.as_path()));
    assert!(picked.refused.is_none(), "{:?}", picked.refused);

    let named = plan(&at, "builder", own("Implement the retry in payments"));
    assert_eq!(
        named.repo.as_deref(),
        Some(pay.as_path()),
        "the monitored repo the work names"
    );
}

#[test]
fn a_researcher_needs_no_repo_and_a_builder_without_one_is_refused_at_spawn_too() {
    let root = scratch("researcher");
    let (home, ws, projects) = workspace(&root);
    let at = Where {
        home: &home,
        workspace: &ws,
        projects: &projects,
    };
    let p = plan(
        &at,
        "researcher",
        own("Research the options for offline sync"),
    );
    assert_eq!(p.fit.call, Call::Fit);
    assert!(
        p.scratch_ok && p.repo.is_none() && p.refused.is_none(),
        "{p:?}"
    );

    let req = cos::agents::Request {
        harness: cos::agents::Harness::Claude,
        program: Some("/bin/false".into()),
        model: None,
        repo: None,
        worktree: false,
        title: "t".into(),
        source: "prompt".into(),
        prompt: "Implement it".into(),
        deadline: std::time::Duration::from_secs(5),
        budget_usd: 1.0,
        slots: None,
        contract: Contract::default(),
        explicit: policy::Explicit::default(),
        role: Some("builder".into()),
    };
    let err = cos::agents::spawn(&pinned(&home), req, Path::new("/bin/false")).unwrap_err();
    assert!(err.to_string().contains("works in a repo"), "{err}");
    assert!(
        cos::agents::list(&home).unwrap().runs.is_empty(),
        "nothing started"
    );
}

#[test]
fn an_ops_task_on_cos_itself_is_run_yourself_with_the_command() {
    let root = scratch("ops");
    let (home, ws, projects) = workspace(&root);
    let at = Where {
        home: &home,
        workspace: &ws,
        projects: &projects,
    };
    let p = plan(&at, "builder", own("refresh the dashboard for Friday"));
    assert_eq!(p.fit.call, Call::RunYourself);
    assert_eq!(p.fit.command.as_deref(), Some("cos report"));
    let why = p.refused.unwrap();
    assert!(
        why.contains("outside the worktree") && why.contains("cos report"),
        "{why}"
    );
}

#[test]
fn an_outward_finish_line_becomes_the_draft_and_stays_yours() {
    let root = scratch("done-when");
    let (home, ws, projects) = workspace(&root);
    checkout(&projects, "acme/widget-sdk");
    let at = Where {
        home: &home,
        workspace: &ws,
        projects: &projects,
    };
    let p = plan(&at, "tester", Ask::Deliverable("D-1".into()));
    assert_eq!(
        p.contract.finish_line.as_deref(),
        Some("WID-42 resolved in Jira")
    );
    let draft = p.contract.done_when.clone().unwrap();
    assert!(
        draft.starts_with("A draft ready for you to resolve WID-42: `TEST-PLAN.md`"),
        "{draft}"
    );
    assert!(
        p.prompt.contains("Your finish line here: A draft ready"),
        "the agent is told"
    );
    assert_eq!(p.contract.output.as_deref(), Some("TEST-PLAN.md"));
    assert!(fit::judgeable(&p.contract));

    let seed = policy::seed();
    let vocab = seed.fit.unwrap();
    assert_eq!(
        fit::draft_done_when(&vocab, "The guide matches what shipped", None),
        None
    );
    assert!(
        fit::draft_done_when(&vocab, "PR #12 merged", None)
            .unwrap()
            .contains("merge PR #12")
    );
    assert_eq!(
        fit::draft_done_when(&vocab, "The docs describe the API", None),
        None
    );
}

#[test]
fn a_contract_with_nothing_to_run_warns_before_hiring() {
    let root = scratch("judgeable");
    let (home, ws, projects) = workspace(&root);
    let repo = checkout(&projects, "acme/payments");
    let at = Where {
        home: &home,
        workspace: &ws,
        projects: &projects,
    };
    let p = hire::plan(
        &at,
        Some("builder"),
        own("Implement the refund export"),
        Some(repo),
        &Contract::default(),
    )
    .unwrap();
    assert!(p.refused.is_none());
    assert!(
        p.warnings
            .iter()
            .any(|w| w.contains("nothing the judge can run")),
        "{:?}",
        p.warnings
    );
    assert!(!fit::judgeable(&Contract::default()));
    assert!(fit::judgeable(&Contract {
        scope: vec!["docs/".into()],
        ..Default::default()
    }));
}

#[test]
fn the_vocabulary_matches_whole_words_and_skips_boilerplate() {
    assert!(fit::has("Test the GAR package", "gar"));
    assert!(!fit::has("a cigar box", "gar"));
    assert!(!fit::has("Give the deck to the reviewer", "review"));
    let vocab = policy::seed().fit.unwrap();
    let text = "Fix the parser\n8. Permission boundary\n   You run in a sandboxed worktree with no network. Stop before anything outward-facing (merging, sending).";
    let stripped = fit::strip(&vocab, text);
    assert_eq!(stripped.trim(), "Fix the parser");
    let roles = policy::seed().roles;
    let f = fit::check(
        &vocab,
        &roles,
        roles.iter().find(|r| r.id == "builder"),
        &Work {
            title: "Fix the parser".into(),
            text: stripped,
            done_when: None,
        },
    );
    assert_eq!(f.call, Call::Fit, "{f:?}");
    assert!(f.needs.is_empty(), "the boilerplate is not the work");
}
