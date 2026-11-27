//! The Flows tab's approval card and button over a fictional repo: the flow
//! waiting at the gate, its design's digest and reviews read from the flow's
//! branch, and the shared approval, over a stand-in `cn` whose store changes.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use cos::flows;
use cos::ritual::{self, Check};
use serde_json::json;

const DESIGN: &str = "docs/plans/2026-01-05-search-design.md";
const LATEST: &str = "docs/plans/2026-01-05-search-design-review.md";

struct Dir(PathBuf);

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn dir(name: &str) -> Dir {
    let d = std::env::temp_dir().join(format!("cos-ritual-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    Dir(d.canonicalize().unwrap())
}

fn store_cn() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake-cn-store.sh")
}

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=t", "-c", "user.email=t@example.invalid"])
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .output()
        .unwrap()
        .status
        .success();
    assert!(ok, "git {args:?}");
}

fn review(sha: &str, verdict: &str, body: &str) -> String {
    format!("---\ndesign: {DESIGN}\ndesign-sha256: {sha}\nverdict: {verdict}\n---\n\n{body}")
}

/// `projects/web` on `main`, with the design and three review rounds
/// committed only on `feat/search`, and one epic waiting at the gate.
fn waiting(name: &str) -> (Dir, PathBuf, String) {
    let d = dir(name);
    let r = d.0.join("web");
    std::fs::create_dir_all(r.join(".chronis")).unwrap();
    git(&r, &["init", "-q", "-b", "main"]);
    git(&r, &["commit", "-q", "--allow-empty", "-m", "init"]);
    git(&r, &["checkout", "-q", "-b", "feat/search"]);
    std::fs::create_dir_all(r.join("docs/plans")).unwrap();
    std::fs::write(r.join(DESIGN), "# Search\n\nThe design.\n").unwrap();
    let sha = flows::sha256_file(&r.join(DESIGN)).unwrap();
    let plans = r.join("docs/plans");
    std::fs::write(
        plans.join("2026-01-05-search-design-review-1.md"),
        review("superseded", "findings", "1. blocking Scope: x.\n"),
    )
    .unwrap();
    std::fs::write(
        plans.join("2026-01-05-search-design-review-2.md"),
        review("superseded", "findings", "1. should A: y.\n2. nit B: z.\n"),
    )
    .unwrap();
    std::fs::write(
        r.join(LATEST),
        review(
            &sha,
            "findings",
            "1. should A: y.\n2. should B: y.\n3. nit C: z.\n",
        ),
    )
    .unwrap();
    git(&r, &["add", "docs"]);
    git(&r, &["commit", "-q", "-m", "design"]);
    git(&r, &["checkout", "-q", "main"]);
    assert!(
        !r.join(DESIGN).exists(),
        "the design lives on the branch only"
    );

    let epic = json!([{ "id": "t-s100", "title": "Search across the docs room", "status": "open",
        "task_type": "epic", "parent": null, "blocked_by": [], "approved": null, "archived": false,
        "description": format!("ritual-brief: docs/plans/2026-01-05-search-brief.md\nritual-branch: feat/search\nritual-design: {DESIGN}") }]);
    std::fs::write(r.join(".chronis/tasks.json"), epic.to_string()).unwrap();
    (d, r, sha)
}

#[test]
fn a_waiting_flow_yields_an_approval_card_with_its_step_and_review_counts() {
    let (d, _r, sha) = waiting("card");
    let (gates, gaps) = ritual::gates(&d.0, &store_cn(), Duration::from_secs(60), None);
    assert!(gaps.is_empty(), "{gaps:?}");
    assert_eq!(gates.len(), 1);
    let g = &gates[0];
    assert_eq!(
        (g.repo.as_str(), g.epic.as_str(), g.step),
        ("web", "t-s100", "approve")
    );
    assert_eq!(ritual::STRIP[2], g.step, "the gate sits after Spec");
    assert_eq!(g.design, DESIGN);
    assert_eq!(g.design_sha256.as_deref(), Some(sha.as_str()));
    assert_eq!(
        g.brief.as_deref(),
        Some("docs/plans/2026-01-05-search-brief.md")
    );
    let latest = g.review.as_ref().expect("the newest review");
    assert_eq!(latest.path, LATEST);
    assert_eq!(latest.verdict, "findings");
    assert!(latest.current, "it read this design");
    assert_eq!((latest.blocking, latest.should, latest.nit), (0, 2, 1));
    assert!(!g.clean());
    let rounds: Vec<(&str, usize, usize, usize)> = g
        .history
        .iter()
        .map(|h| {
            (
                h.path.rsplit('/').next().unwrap(),
                h.blocking,
                h.should,
                h.nit,
            )
        })
        .collect();
    assert_eq!(
        rounds,
        vec![
            ("2026-01-05-search-design-review-2.md", 0, 1, 1),
            ("2026-01-05-search-design-review-1.md", 1, 0, 0),
        ]
    );
    assert!(g.history.iter().all(|h| !h.current));
}

#[test]
fn the_button_refuses_a_review_with_findings_then_approves_a_clean_one() {
    let (d, r, sha) = waiting("approve");
    let cn = store_cn();
    let refused =
        ritual::approve_in_app(&cn, &r, "t-s100", DESIGN, "Ann", Some(LATEST), false).unwrap_err();
    assert!(format!("{refused:#}").contains("not clean"), "{refused:#}");
    assert!(
        !r.join(".chronis/calls.log").exists(),
        "nothing was written"
    );
    assert_eq!(ritual::check(&cn, &r, "t-s100").unwrap(), Check::Unapproved);

    git(&r, &["checkout", "-q", "feat/search"]);
    std::fs::write(r.join(LATEST), review(&sha, "clean", "")).unwrap();
    git(&r, &["commit", "-q", "-am", "clean review"]);
    git(&r, &["checkout", "-q", "main"]);
    let (gates, _) = ritual::gates(&d.0, &cn, Duration::from_secs(60), None);
    assert!(gates[0].clean(), "the card offers a plain Approve");

    let (line, after) =
        ritual::approve_in_app(&cn, &r, "t-s100", DESIGN, "Ann", Some(LATEST), false).unwrap();
    assert_eq!(
        line,
        format!("ritual-approved: {DESIGN} sha256:{sha} by Ann")
    );
    assert_eq!(
        after,
        Check::Approved {
            design: DESIGN.into()
        }
    );
    let (gates, _) = ritual::gates(&d.0, &cn, Duration::from_secs(60), None);
    assert!(gates.is_empty(), "an approved flow leaves the gate");
}

#[test]
fn the_steps_carry_the_beads_under_each_flow() {
    let (d, r, _sha) = waiting("steps");
    let path = r.join(".chronis/tasks.json");
    let mut tasks: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    tasks
        .as_array_mut()
        .unwrap()
        .push(json!({ "id": "t-s101", "title": "Index the brief",
        "status": "done", "task_type": "task", "parent": "t-s100", "blocked_by": [],
        "approved": null, "archived": false, "description": "" }));
    std::fs::write(&path, tasks.to_string()).unwrap();
    let (gates, _gaps, flows) = ritual::board(&d.0, &store_cn(), Duration::from_secs(60), None);
    assert_eq!(gates.len(), 1);
    let f = flows
        .iter()
        .find(|f| f.epic == "t-s100")
        .expect("the flow's steps");
    assert_eq!((f.repo.as_str(), f.step), ("web", "spec"));
    let beads: Vec<(&str, &str)> = f
        .beads
        .iter()
        .map(|b| (b.id.as_str(), b.status.as_str()))
        .collect();
    assert_eq!(beads, vec![("t-s101", "done")]);
}

#[test]
fn findings_count_each_severity_below_the_front_matter() {
    let text = "---\ndesign: d.md\nverdict: findings\n---\n\n1. blocking A.\n2. should B.\n  3. nit C.\n10. nit D.\nnot 4. should\n5.should no space\n";
    assert_eq!(ritual::findings(text), (1, 1, 2));
    assert_eq!(ritual::findings("1. must X.\n"), (1, 0, 0));
}

#[test]
fn check_tells_where_to_approve() {
    assert!(
        Check::Unapproved
            .next()
            .is_some_and(|n| n.contains("Flows tab in Chief of Staff"))
    );
    assert_eq!(Check::Approved { design: "d".into() }.next(), None);
}

#[test]
fn an_unreadable_design_says_the_next_move_not_gits_error() {
    let git = "docs/d.md is not committed on feat/x: git show feat/x:docs/d.md failed: fatal: path 'docs/d.md' does not exist in 'feat/x'";
    let said = ritual::unreadable_design("docs/d.md", Some("feat/x"), git);
    assert_eq!(
        said,
        "The design is not committed on feat/x yet. Commit docs/d.md on that branch, then refresh this tab to approve it."
    );
    for said in [
        said,
        ritual::unreadable_design("docs/d.md", None, "fatal: does not exist"),
        ritual::unreadable_design("docs/d.md", None, "exit 128"),
    ] {
        assert!(
            !said.contains("fatal") && !said.contains("git show"),
            "{said}"
        );
    }
}
