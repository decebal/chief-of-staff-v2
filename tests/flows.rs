//! `cos flows` and `cos ritual`: the step each flow is in, the scan under one
//! deadline, and the approval gate, all over a fake `cn`.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use cos::flows::{self, Markers, Step, Task};
use cos::ritual::{self, ApproveArgs, Check, ReviewAt};
use cos::state;
use serde_json::{Value, json};

struct Dir(PathBuf);

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn dir(name: &str) -> Dir {
    let d = std::env::temp_dir().join(format!("cos-flows-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    Dir(d.canonicalize().unwrap())
}

fn fake_cn() -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake-cn.sh");
    std::fs::canonicalize(p).unwrap()
}

fn repo(root: &Path, name: &str, tasks: &Value) -> PathBuf {
    let r = root.join(name);
    std::fs::create_dir_all(r.join(".chronis")).unwrap();
    std::fs::write(r.join(".chronis/tasks.json"), tasks.to_string()).unwrap();
    r
}

fn task(id: &str, kind: &str, status: &str, parent: Option<&str>) -> Value {
    json!({ "id": id, "title": format!("title {id}"), "status": status, "task_type": kind,
            "parent": parent, "blocked_by": [], "approved": null, "description": "",
            "archived": false })
}

fn with(mut t: Value, key: &str, value: Value) -> Value {
    t[key] = value;
    t
}

fn tasks(v: &Value) -> Vec<Task> {
    serde_json::from_value(v.clone()).unwrap()
}

fn step_of(flows: &[flows::Flow], id: &str) -> Step {
    flows
        .iter()
        .find(|f| f.id == id)
        .unwrap_or_else(|| panic!("no flow {id}"))
        .step
}

#[test]
fn markers_take_the_newest_line_of_each_key() {
    let m = Markers::parse(
        "goal\nritual-brief: a.md\nritual-design: old.md\nnot ritual-x\nritual-design: new.md\nritual- bad: x",
    );
    assert_eq!(m.get("brief"), Some("a.md"));
    assert_eq!(m.get("design"), Some("new.md"));
    assert_eq!(m.0.len(), 2);
}

#[test]
fn each_flow_gets_the_first_matching_step() {
    let d = dir("steps");
    std::fs::create_dir_all(d.0.join("docs/plans")).unwrap();
    std::fs::write(d.0.join("docs/plans/a-design.md"), "approved text").unwrap();
    let digest = flows::sha256_file(&d.0.join("docs/plans/a-design.md")).unwrap();
    std::fs::write(d.0.join("docs/plans/b-design.md"), "edited after approval").unwrap();

    let approved = |id: &str, design: &str, sha: &str, extra: &str| {
        with(
            with(task(id, "epic", "open", None), "approved", json!(true)),
            "description",
            json!(format!(
                "ritual-design: {design}\nritual-approved: {design} sha256:{sha} by Ann{extra}"
            )),
        )
    };

    let list = json!([
        // legacy: no markers, its step read from its beads
        task("legacy-build", "epic", "open", None),
        task("lb-1", "task", "open", Some("legacy-build")),
        task("legacy-done", "epic", "done", None),
        task("legacy-plan", "feature", "open", None),
        with(
            task("think", "epic", "open", None),
            "description",
            json!("ritual-brief: x-brief.md")
        ),
        with(
            task("spec", "epic", "open", None),
            "description",
            json!("ritual-brief: x.md\nritual-design: docs/plans/a-design.md")
        ),
        approved("plan", "docs/plans/a-design.md", &digest, ""),
        approved("drift", "docs/plans/b-design.md", &digest, ""),
        task("d-1", "task", "open", Some("drift")),
        approved("build", "docs/plans/a-design.md", &digest, ""),
        task("bu-1", "task", "done", Some("build")),
        task("bu-2", "task", "in-progress", Some("build")),
        approved("review", "docs/plans/a-design.md", &digest, ""),
        task("rv-1", "task", "done", Some("review")),
        approved(
            "ship",
            "docs/plans/a-design.md",
            &digest,
            "\nritual-review: https://example.invalid/pr/1 clean"
        ),
        task("sh-1", "task", "done", Some("ship")),
        approved(
            "rereview",
            "docs/plans/a-design.md",
            &digest,
            "\nritual-review: r1.md clean\nritual-review: r2.md findings",
        ),
        task("rr-1", "task", "done", Some("rereview")),
        approved(
            "shipped",
            "docs/plans/a-design.md",
            &digest,
            "\nritual-shipped: abc1234"
        ),
        task("sp-1", "task", "done", Some("shipped")),
        with(
            task("bug", "bug", "open", None),
            "description",
            json!("ritual-brief: docs/plans/crash-brief.md\nritual-branch: fix/NW-1-crash")
        ),
        task("bg-1", "task", "open", Some("bug")),
        task("legacy-ship", "epic", "open", None),
        task("ls-1", "task", "done", Some("legacy-ship")),
        with(
            task("reflect", "epic", "done", None),
            "description",
            json!("ritual-design: d.md")
        ),
        with(
            task("done", "epic", "done", None),
            "description",
            json!("ritual-report: docs/reports/r.md")
        ),
        // not flows
        with(
            task("archived", "epic", "open", None),
            "archived",
            json!(true)
        ),
        task("loose", "task", "open", None),
        task("lone-bug", "bug", "open", None),
    ]);
    let until = cos::proc::Deadline::after(Duration::from_secs(30));
    let check = flows::DriftCheck {
        dir: &d.0,
        until: &until,
    };
    let flows = flows::flows_in("r", Some(&check), &tasks(&list));

    let expect = [
        ("legacy-build", Step::Build),
        ("legacy-done", Step::Done),
        ("legacy-plan", Step::Plan),
        ("think", Step::Think),
        ("spec", Step::Spec),
        ("plan", Step::Plan),
        ("drift", Step::Drift),
        ("build", Step::Build),
        ("review", Step::Review),
        ("ship", Step::Ship),
        ("rereview", Step::Review),
        ("shipped", Step::Reflect),
        ("bug", Step::Build),
        ("legacy-ship", Step::Ship),
        ("reflect", Step::Reflect),
        ("done", Step::Done),
    ];
    for (id, step) in expect {
        assert_eq!(step_of(&flows, id), step, "{id}");
    }
    assert_eq!(
        flows.len(),
        expect.len(),
        "archived, loose tasks and a childless bug are not flows"
    );
    assert!(
        flows
            .iter()
            .find(|f| f.id == "legacy-build")
            .unwrap()
            .legacy
    );
    assert!(!flows.iter().find(|f| f.id == "spec").unwrap().legacy);
    assert!(flows.iter().all(|f| !f.drift_unchecked));

    let spent = cos::proc::Deadline::after(Duration::ZERO);
    let late = flows::DriftCheck {
        dir: &d.0,
        until: &spent,
    };
    let flows = flows::flows_in("r", Some(&late), &tasks(&list));
    let drift = flows.iter().find(|f| f.id == "drift").unwrap();
    assert!(
        drift.drift_unchecked,
        "no time left: drift is not ruled out"
    );
    assert_eq!(drift.step, Step::Build, "and not claimed either");
}

#[test]
fn a_bug_with_children_is_a_flow_and_grandchildren_count() {
    let list = json!([
        task("bug", "bug", "open", None),
        task("b1", "task", "done", Some("bug")),
        task("b2", "task", "open", Some("b1")),
    ]);
    let flows = flows::flows_in("r", None, &tasks(&list));
    assert_eq!(flows.len(), 1);
    assert_eq!(
        flows[0].beads.len(),
        2,
        "the grandchild is a bead of the flow"
    );
    assert_eq!(flows[0].step, Step::Build);
}

#[test]
fn ready_beads_are_open_with_every_blocker_done() {
    let list = json!([
        task("e", "epic", "open", None),
        task("a", "task", "done", Some("e")),
        with(
            task("b", "task", "open", Some("e")),
            "blocked_by",
            json!(["a"])
        ),
        with(
            task("c", "task", "open", Some("e")),
            "blocked_by",
            json!(["b"])
        ),
        with(
            task("d", "task", "open", Some("e")),
            "blocked_by",
            json!(["gone"])
        ),
    ]);
    let flows = flows::flows_in("r", None, &tasks(&list));
    let ready: Vec<&str> = flows[0]
        .beads
        .iter()
        .filter(|b| b.ready)
        .map(|b| b.id.as_str())
        .collect();
    assert_eq!(ready, vec!["b"]);
    assert_eq!(
        flows[0].counts.blocked, 2,
        "c waits on b; d waits on a task that is not there"
    );
    assert_eq!(flows[0].counts.done, 1);
}

#[test]
fn the_scan_reads_every_store_and_reports_the_ones_it_cannot() {
    let d = dir("scan");
    repo(&d.0, "alpha", &json!([task("e1", "epic", "open", None)]));
    let broken = repo(&d.0, "beta", &json!([]));
    std::fs::write(broken.join(".chronis/fail"), "").unwrap();
    std::fs::create_dir_all(d.0.join("no-store")).unwrap();

    let scan = flows::scan(&d.0, &fake_cn(), Duration::from_secs(30));
    assert_eq!(scan.repos, 2, "only repos with .chronis/");
    assert_eq!(scan.flows.len(), 1);
    assert_eq!(scan.flows[0].repo, "alpha");
    assert_eq!(scan.gaps.len(), 1);
    assert!(
        scan.gaps[0].starts_with("beta: cn list failed: store is locked"),
        "{}",
        scan.gaps[0]
    );
}

#[test]
fn a_linked_worktree_is_not_a_flow_store() {
    let d = dir("worktree");
    repo(&d.0, "main", &json!([task("e1", "epic", "open", None)]));
    let linked = repo(
        &d.0,
        "main-feature",
        &json!([task("e2", "epic", "open", None)]),
    );
    std::fs::write(
        linked.join(".git"),
        "gitdir: ../main/.git/worktrees/feature\n",
    )
    .unwrap();
    let found = flows::repos(&d.0).unwrap();
    assert_eq!(found, vec![d.0.join("main")]);
}

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=t", "-c", "user.email=t@example.invalid"])
        // The test identity has no signing key, so a global commit.gpgsign=true fails the commit.
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .output()
        .unwrap()
        .status
        .success();
    assert!(ok, "git {args:?}");
}

#[test]
fn the_design_is_read_from_the_flow_branch_while_it_exists() {
    let design = "docs/plans/x-design.md";
    let (_d, r) = ritual_repo(
        "branch",
        "approved on the branch",
        task("e", "epic", "open", None),
    );
    git(&r, &["init", "-q", "-b", "main"]);
    git(&r, &["commit", "-q", "--allow-empty", "-m", "init"]);
    git(&r, &["checkout", "-q", "-b", "feat/x"]);
    git(&r, &["add", design]);
    git(&r, &["commit", "-q", "-m", "design"]);
    let committed = flows::sha256_file(&r.join(design)).unwrap();
    git(&r, &["checkout", "-q", "main"]);
    std::fs::create_dir_all(r.join("docs/plans")).unwrap();
    std::fs::write(r.join(design), "an unrelated working-tree copy").unwrap();

    let epic = with(
        with(task("e", "epic", "open", None), "approved", json!(true)),
        "description",
        json!(format!(
            "ritual-branch: feat/x\nritual-approved: {design} sha256:{committed} by Ann"
        )),
    );
    std::fs::write(
        r.join(".chronis/tasks.json"),
        json!([epic.clone()]).to_string(),
    )
    .unwrap();
    assert!(
        ritual::check(&fake_cn(), &r, "e").unwrap().ok(),
        "the branch copy is what was approved"
    );

    std::fs::remove_file(r.join(design)).unwrap();
    git(&r, &["checkout", "-q", "feat/x"]);
    std::fs::write(r.join(design), "edited after approval").unwrap();
    git(&r, &["commit", "-q", "-am", "edit"]);
    assert_eq!(
        ritual::check(&fake_cn(), &r, "e").unwrap(),
        Check::Changed {
            design: design.into()
        }
    );
}

#[test]
fn a_spent_budget_leaves_the_rest_as_gaps_without_running_cn() {
    let d = dir("budget");
    repo(&d.0, "alpha", &json!([task("e1", "epic", "open", None)]));
    let scan = flows::scan(&d.0, &fake_cn(), Duration::ZERO);
    assert!(scan.flows.is_empty());
    assert_eq!(
        scan.gaps,
        vec!["alpha: not read, the scan ran out of time".to_owned()]
    );
}

fn ritual_repo(name: &str, design_text: &str, epic: Value) -> (Dir, PathBuf) {
    let d = dir(name);
    let r = repo(&d.0, "r", &json!([epic]));
    std::fs::create_dir_all(r.join("docs/plans")).unwrap();
    std::fs::write(r.join("docs/plans/x-design.md"), design_text).unwrap();
    (d, r)
}

#[test]
fn check_passes_only_an_approved_unchanged_design() {
    let design = "docs/plans/x-design.md";
    let (_d, r) = ritual_repo("check", "v1", task("e", "epic", "open", None));
    assert_eq!(
        ritual::check(&fake_cn(), &r, "e").unwrap(),
        Check::Unapproved
    );

    let digest = flows::sha256_file(&r.join(design)).unwrap();
    let epic = with(
        with(task("e", "epic", "open", None), "approved", json!(true)),
        "description",
        json!(format!("ritual-approved: {design} sha256:{digest} by Ann")),
    );
    std::fs::write(r.join(".chronis/tasks.json"), json!([epic]).to_string()).unwrap();
    assert!(ritual::check(&fake_cn(), &r, "e").unwrap().ok());

    std::fs::write(r.join(design), "v2").unwrap();
    assert_eq!(
        ritual::check(&fake_cn(), &r, "e").unwrap(),
        Check::Changed {
            design: design.into()
        }
    );

    std::fs::remove_file(r.join(design)).unwrap();
    assert_eq!(
        ritual::check(&fake_cn(), &r, "e").unwrap(),
        Check::MissingDesign {
            design: Some(design.into())
        }
    );
    assert!(
        ritual::check(&fake_cn(), &r, "nope").is_err(),
        "an unknown epic is an error"
    );
}

fn review(path: &Path, design: &str, sha: &str, verdict: &str) -> PathBuf {
    let p = path.join(format!(
        "review-{verdict}-{}-{sha}.md",
        design.replace('/', "_")
    ));
    std::fs::write(
        &p,
        format!("---\ndesign: {design}\ndesign-sha256: {sha}\nverdict: {verdict}\n---\n\n1. nit\n"),
    )
    .unwrap();
    p
}

fn calls(r: &Path) -> String {
    std::fs::read_to_string(r.join(".chronis/calls.log")).unwrap_or_default()
}

#[test]
fn approve_refuses_anything_but_a_clean_review_of_this_exact_design() {
    let design = "docs/plans/x-design.md";
    let (d, r) = ritual_repo("approve-refuse", "v1", task("e", "epic", "open", None));
    let digest = flows::sha256_file(&r.join(design)).unwrap();
    let yes = |_: &str| Ok(true);
    let args = |review: Option<PathBuf>, without: bool| ApproveArgs {
        epic: "e",
        design,
        by: "Ann",
        review: review.map(ReviewAt::File),
        without_review: without,
        yes: false,
    };

    let cases = [
        (review(&d.0, design, &digest, "findings"), "not clean"),
        (review(&d.0, design, "0000", "clean"), "different version"),
        (
            review(&d.0, "docs/plans/other.md", &digest, "clean"),
            "the review is of",
        ),
    ];
    for (path, why) in cases {
        let err = ritual::approve(&fake_cn(), &r, &args(Some(path), false), yes).unwrap_err();
        assert!(format!("{err:#}").contains(why), "{why}: {err:#}");
    }
    assert!(
        ritual::approve(&fake_cn(), &r, &args(None, false), yes).is_err(),
        "no review, no flag"
    );

    let clean = review(&d.0, design, &digest, "clean");
    let declined = ritual::approve(&fake_cn(), &r, &args(Some(clean), false), |_| Ok(false));
    assert!(declined.is_err(), "a no at the prompt approves nothing");
    assert_eq!(calls(&r), "", "nothing was written to the store");
}

#[test]
fn approve_records_the_digest_then_approves() {
    let design = "docs/plans/x-design.md";
    let (d, r) = ritual_repo("approve-ok", "v1", task("e", "epic", "open", None));
    let digest = flows::sha256_file(&r.join(design)).unwrap();
    let clean = review(&d.0, design, &digest, "clean");
    let args = ApproveArgs {
        epic: "e",
        design,
        by: "Ann",
        review: Some(ReviewAt::File(clean)),
        without_review: false,
        yes: false,
    };

    let mut asked = String::new();
    let line = ritual::approve(&fake_cn(), &r, &args, |l| {
        asked = l.to_owned();
        Ok(true)
    })
    .unwrap();
    assert_eq!(
        line,
        format!("ritual-approved: {design} sha256:{digest} by Ann")
    );
    assert_eq!(asked, line, "the prompt shows exactly what is written");
    assert_eq!(
        calls(&r),
        format!("task edit e --append-description {line} --toon\napprove e --toon\n")
    );
}

#[test]
fn approve_without_review_says_so_on_the_epic() {
    let design = "docs/plans/x-design.md";
    let (_d, r) = ritual_repo("approve-bare", "v1", task("e", "epic", "open", None));
    let args = ApproveArgs {
        epic: "e",
        design,
        by: "Ann",
        review: None,
        without_review: true,
        yes: false,
    };
    let line = ritual::approve(&fake_cn(), &r, &args, |_| Ok(true)).unwrap();
    assert!(line.ends_with("by Ann without review"), "{line}");
}

#[test]
fn approve_with_yes_records_without_asking() {
    let design = "docs/plans/x-design.md";
    let (_d, r) = ritual_repo("approve-yes-lib", "v1", task("e", "epic", "open", None));
    let digest = flows::sha256_file(&r.join(design)).unwrap();
    let args = ApproveArgs {
        epic: "e",
        design,
        by: "Ann",
        review: None,
        without_review: true,
        yes: true,
    };
    let line = ritual::approve(&fake_cn(), &r, &args, |l| {
        unreachable!("--yes must not prompt, asked: {l}")
    })
    .unwrap();
    assert_eq!(
        line,
        format!("ritual-approved: {design} sha256:{digest} by Ann without review")
    );
    assert_eq!(
        calls(&r),
        format!("task edit e --append-description {line} --toon\napprove e --toon\n")
    );
}

#[test]
fn approve_refuses_a_file_other_than_the_epics_design() {
    let epic = with(
        task("e", "epic", "open", None),
        "description",
        json!("ritual-design: docs/plans/x-design.md"),
    );
    let (_d, r) = ritual_repo("approve-other", "v1", epic);
    std::fs::write(r.join("docs/plans/x-brief.md"), "not the design").unwrap();
    let args = ApproveArgs {
        epic: "e",
        design: "docs/plans/x-brief.md",
        by: "Ann",
        review: None,
        without_review: true,
        yes: false,
    };
    let err = ritual::approve(&fake_cn(), &r, &args, |_| Ok(true)).unwrap_err();
    assert!(
        format!("{err:#}").contains("the epic's design is"),
        "{err:#}"
    );
    assert_eq!(calls(&r), "");
}

#[test]
fn the_cli_refuses_to_approve_without_a_terminal() {
    let design = "docs/plans/x-design.md";
    let (d, r) = ritual_repo("approve-tty", "v1", task("e", "epic", "open", None));
    let digest = flows::sha256_file(&r.join(design)).unwrap();
    let clean = review(&d.0, design, &digest, "clean");

    let out = Command::new(env!("CARGO_BIN_EXE_cos"))
        .args(["ritual", "approve", "e", design, "--by", "Ann", "--review"])
        .arg(&clean)
        .arg("--repo")
        .arg(&r)
        .env("COS_CN", fake_cn())
        .stdin(Stdio::piped())
        .output()
        .unwrap();
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("needs a terminal"), "{err}");
    assert!(err.contains("--yes"), "{err}");
    assert!(err.contains("Flows tab"), "{err}");
    assert_eq!(calls(&r), "", "an agent's piped stdin approves nothing");
}

#[test]
fn the_cli_approves_with_yes_when_stdin_is_not_a_terminal() {
    let design = "docs/plans/x-design.md";
    let (d, r) = ritual_repo("approve-cli-yes", "v1", task("e", "epic", "open", None));
    let digest = flows::sha256_file(&r.join(design)).unwrap();
    let clean = review(&d.0, design, &digest, "clean");

    let out = Command::new(env!("CARGO_BIN_EXE_cos"))
        .args(["ritual", "approve", "e", design, "--by", "Ann", "--review"])
        .arg(&clean)
        .args(["--yes", "--repo"])
        .arg(&r)
        .env("COS_CN", fake_cn())
        .stdin(Stdio::piped())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let line = format!("ritual-approved: {design} sha256:{digest} by Ann");
    assert_eq!(stdout, format!("{line}\napproved e\n"));
    assert_eq!(
        calls(&r),
        format!("task edit e --append-description {line} --toon\napprove e --toon\n")
    );
}

#[test]
fn flows_read_nothing_unless_the_profile_approves_tasks() {
    let ws = dir("gate");
    state::initialize(&ws.0).unwrap();
    let mut p: Value = serde_json::from_str(state::PROFILE_TEMPLATE).unwrap();
    p["sources"] = json!([{ "id": "calendar", "service": "calendar", "accountIdentity": "me",
                            "scope": "s", "readsApproved": true, "approvalRef": "t", "readBudget": "b" }]);
    state::atomic_write(&ws.0.join(".chief-of-staff/profile.json"), &p).unwrap();
    let projects = dir("gate-projects");
    repo(
        &projects.0,
        "alpha",
        &json!([task("e1", "epic", "open", None)]),
    );

    let out = Command::new(env!("CARGO_BIN_EXE_cos"))
        .arg("--workspace")
        .arg(&ws.0)
        .arg("--projects")
        .arg(&projects.0)
        .args(["flows", "--json"])
        .env("COS_CN", fake_cn())
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("does not approve a tasks source"));
}

#[test]
fn criteria_come_from_the_acceptance_heading_only() {
    let long = "y".repeat(400);
    let mut desc = format!(
        "As a user.\n- [ ] not a criterion, before the heading\n\n## Acceptance criteria\n- [ ] first\n- [x] second done\n  - [X] indented, done\nplain text\n- [ ] {long}\n"
    );
    for i in 0..25 {
        desc.push_str(&format!("- [ ] extra {i}\n"));
    }
    desc.push_str("\n## Notes\n- [ ] after the next heading\n");
    let (got, more) = flows::criteria(&desc);
    assert_eq!(got.len(), flows::CRITERIA_CAP);
    assert_eq!(more, 4 + 25 - flows::CRITERIA_CAP);
    assert_eq!((got[0].done, got[0].text.as_str()), (false, "first"));
    assert_eq!((got[1].done, got[1].text.as_str()), (true, "second done"));
    assert!(got[2].done, "an indented [X] counts");
    assert_eq!(got[3].text.chars().count(), flows::CRITERION_CHARS);
    assert!(got[3].text.ends_with('…'));
    assert!(got.iter().all(|c| !c.text.contains("heading")));
    assert_eq!(flows::criteria("no heading\n- [ ] x\n"), (vec![], 0));
}

#[test]
fn older_cn_output_still_parses() {
    let list = json!([
        task("e", "epic", "open", None),
        with(
            with(
                task("a", "task", "in-progress", Some("e")),
                "priority",
                json!(2)
            ),
            "claimed_by",
            json!({ "odd": true })
        ),
        with(
            with(
                task("b", "task", "done", Some("e")),
                "done_at",
                json!("2026-11-01T10:00:00Z")
            ),
            "created_at",
            json!("2026-10-01T10:00:00Z")
        ),
    ]);
    let flows = flows::flows_in("r", None, &tasks(&list));
    let a = flows[0].beads.iter().find(|b| b.id == "a").unwrap();
    assert_eq!(a.priority.as_deref(), Some("2"), "a number becomes text");
    assert_eq!(a.owner, None, "an object is dropped, not a parse failure");
    assert_eq!(flows[0].touched.as_deref(), Some("2026-11-01T10:00:00Z"));
    let bare: Vec<Task> =
        serde_json::from_value(json!([{ "id": "x", "status": "open", "task_type": "epic" }]))
            .unwrap();
    assert_eq!(bare[0].claimed_by, None);
}

#[test]
fn needs_you_is_gate_drift_or_findings() {
    let list = json!([
        with(
            task("gate", "epic", "open", None),
            "description",
            json!("ritual-design: d.md")
        ),
        with(
            with(task("rev", "epic", "open", None), "approved", json!(true)),
            "description",
            json!("ritual-design: d.md\nritual-review: r.md findings")
        ),
        task("rb", "task", "done", Some("rev")),
        with(
            with(task("ok", "epic", "open", None), "approved", json!(true)),
            "description",
            json!("ritual-design: d.md\nritual-review: r.md clean")
        ),
        task("ob", "task", "done", Some("ok")),
        task("quiet", "epic", "open", None),
        task("q1", "task", "open", Some("quiet")),
    ]);
    let flows = flows::flows_in("r", None, &tasks(&list));
    let need = |id: &str| flows.iter().find(|f| f.id == id).unwrap().needs_you;
    assert_eq!(need("gate"), Some(flows::NeedsYou::Gate));
    assert_eq!(need("rev"), Some(flows::NeedsYou::Findings));
    assert_eq!(need("ok"), None, "a clean review is Ship");
    assert_eq!(need("quiet"), None);
}

#[test]
fn a_bead_carries_its_handoff_and_a_hire_waits_for_its_blockers() {
    let list = json!([
        task("e", "epic", "open", None),
        task("b1", "task", "done", Some("e")),
        with(
            with(
                task("b2", "task", "open", Some("e")),
                "blocked_by",
                json!(["b1"])
            ),
            "description",
            json!(
                "## Acceptance criteria\n- [ ] the gate lists every term\n- [x] a fixture fails it\n"
            ),
        ),
        with(
            task("b3", "task", "open", Some("e")),
            "blocked_by",
            json!(["b2"])
        ),
    ]);
    let ts = tasks(&list);
    let flows = flows::flows_in("vm", None, &ts);
    let b2 = flows[0].beads.iter().find(|b| b.id == "b2").unwrap();
    assert!(b2.open_blockers.is_empty(), "b1 is done");
    let h = &b2.handoff;
    for part in [
        "in the vm repo",
        "Flow: e title e",
        "Bead: b2 title b2",
        "- [ ] the gate lists every term",
        "- [x] a fixture fails it",
        "Waits on:\n- b1 title b1\n",
        "Unblocks:\n- b3 title b3",
        "cn show b2 --toon",
    ] {
        assert!(h.contains(part), "{part:?} not in\n{h}");
    }
    let b3 = flows[0].beads.iter().find(|b| b.id == "b3").unwrap();
    assert_eq!(b3.open_blockers, ["b2"]);
    assert!(b3.handoff.contains("b2 title b2 (not done yet)"));
    assert!(b3.handoff.contains("no acceptance criteria on this bead"));

    assert_eq!(flows::bead_to_hire("vm", &ts, "e", "b2").unwrap().id, "b2");
    let refused = |bead: &str| {
        format!(
            "{:#}",
            flows::bead_to_hire("vm", &ts, "e", bead).unwrap_err()
        )
    };
    assert!(refused("b3").contains("waits on b2"), "{}", refused("b3"));
    assert!(refused("b1").contains("already done"), "{}", refused("b1"));
    assert!(
        refused("zz").contains("not a bead of e"),
        "{}",
        refused("zz")
    );
}

#[test]
fn a_drifted_design_needs_you() {
    let d = dir("drift-needs");
    let r = d.0.join("repo");
    std::fs::create_dir_all(&r).unwrap();
    std::fs::write(r.join("d.md"), "changed").unwrap();
    let list = json!([with(
        with(task("e", "epic", "open", None), "approved", json!(true)),
        "description",
        json!("ritual-design: d.md\nritual-approved: d.md sha256:0000 by A")
    )]);
    let deadline = cos::proc::Deadline::after(Duration::from_secs(20));
    let check = flows::DriftCheck {
        dir: &r,
        until: &deadline,
    };
    let flows = flows::flows_in("r", Some(&check), &tasks(&list));
    assert_eq!(flows[0].step, Step::Drift);
    assert_eq!(flows[0].needs_you, Some(flows::NeedsYou::Drift));
}

#[test]
fn layers_are_carried_on_each_bead() {
    let list = json!([
        task("e", "epic", "open", None),
        task("a", "task", "done", Some("e")),
        with(
            task("b", "task", "open", Some("e")),
            "blocked_by",
            json!(["a"])
        ),
        with(
            task("c", "task", "open", Some("e")),
            "blocked_by",
            json!(["b", "a", "elsewhere"])
        ),
        with(
            task("x", "task", "open", Some("e")),
            "blocked_by",
            json!(["y"])
        ),
        with(
            task("y", "task", "open", Some("e")),
            "blocked_by",
            json!(["x"])
        ),
    ]);
    let flows = flows::flows_in("r", None, &tasks(&list));
    let layer = |id: &str| flows[0].beads.iter().find(|b| b.id == id).unwrap().layer;
    assert_eq!((layer("a"), layer("b"), layer("c")), (0, 1, 2));
    assert!(
        layer("x") <= 2 && layer("y") <= 2 && layer("x") != layer("y"),
        "a cycle is broken where it is found, so both still get a column"
    );
}

#[test]
fn the_page_inlines_the_scan_escaped() {
    let list = json!([
        with(
            task("e", "epic", "open", None),
            "title",
            json!("</script><!-- /*__COS_FLOWS__*/ export")
        ),
        task("a", "task", "done", Some("e")),
        with(
            task("b", "task", "open", Some("e")),
            "blocked_by",
            json!(["a"])
        ),
    ]);
    let scan = flows::Scan {
        flows: flows::flows_in("vm", None, &tasks(&list)),
        gaps: vec!["web-app: cn list failed".into()],
        repos: 2,
    };
    let page = flows::page_html(&scan, true).unwrap();
    assert_eq!(page.matches("<script>").count(), 1, "one script");
    assert_eq!(page.matches("</script>").count(), 1, "no title closes it");
    assert!(page.contains("\\u003c/script>\\u003c!-- /*__COS_FLOWS__*/ export"));
    assert!(page.contains("web-app: cn list failed"));
    assert!(
        !page.contains("/*__COS_TOKENS__*/"),
        "the tokens are inlined"
    );
    assert_eq!(
        page.matches("/*! vgpu 0.6.0 (MIT), vendored for cos */")
            .count(),
        1
    );
    let template = include_str!("../src/flows.html");
    for slot in [flows::FLOWS_SLOT, cos::design::VGPU_SLOT] {
        let err = flows::page_html_from(&template.replace(slot, ""), &scan, true).unwrap_err();
        assert!(err.to_string().contains("slot"), "{err}");
    }
}

#[test]
fn the_page_policy_runs_only_its_own_code_and_fetches_nothing() {
    assert_eq!(
        flows::PAGE_CSP,
        "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; font-src data:; img-src data:"
    );
    assert_eq!(
        flows::DASH_CSP,
        format!("{}; frame-ancestors 'none'", flows::PAGE_CSP)
    );
}

fn store(desc: &str) -> Vec<Task> {
    tasks(&json!([with(
        task("e", "epic", "open", None),
        "description",
        json!(desc)
    )]))
}

#[test]
fn open_targets_come_only_from_the_epics_own_lines() {
    use flows::Target;
    let ok = store(
        "ritual-design: docs/plans/d.md\nritual-review: https://github.com/o/r/pull/1 clean\nritual-pr: https://github.com/o/r/pull/2\nritual-report: ./docs/reports/r.html",
    );
    assert_eq!(
        flows::open_target(&ok, "e", "design").unwrap(),
        Target::Path("docs/plans/d.md".into())
    );
    assert_eq!(
        flows::open_target(&ok, "e", "review").unwrap(),
        Target::Url("https://github.com/o/r/pull/1".into())
    );
    assert_eq!(
        flows::open_target(&ok, "e", "pr").unwrap(),
        Target::Url("https://github.com/o/r/pull/2".into())
    );
    assert_eq!(
        flows::open_target(&ok, "e", "report").unwrap(),
        Target::Path("docs/reports/r.html".into())
    );
    let refused = |desc: &str, epic: &str, key: &str| {
        flows::open_target(&store(desc), epic, key)
            .unwrap_err()
            .to_string()
    };
    for (desc, epic, key, why) in [
        ("ritual-design: d.md", "e", "branch", "not a file"),
        ("ritual-design: d.md", "other", "design", "not in this repo"),
        ("ritual-brief: b.md", "e", "design", "names no design"),
        (
            "ritual-plan: /etc/passwd.md",
            "e",
            "plan",
            "not a path inside",
        ),
        (
            "ritual-plan: docs/../../x.md",
            "e",
            "plan",
            "leaves the repo",
        ),
        (
            "ritual-report: run.command",
            "e",
            "report",
            "not a Markdown",
        ),
        ("ritual-report: Evil.app", "e", "report", "not a Markdown"),
        (
            "ritual-pr: https://evil.example/x",
            "e",
            "pr",
            "only GitHub",
        ),
        (
            "ritual-review: http://github.com/x findings",
            "e",
            "review",
            "only GitHub",
        ),
        ("ritual-design: file:///x.md", "e", "design", "only GitHub"),
        ("ritual-pr: docs/x.md", "e", "pr", "only GitHub"),
    ] {
        let err = refused(desc, epic, key);
        assert!(err.contains(why), "{desc} / {key}: {err}");
    }
}

#[test]
fn contained_follows_links_and_keeps_to_the_root() {
    let d = dir("contained");
    let root = d.0.join("repo");
    let worktree = d.0.join("repo-feature");
    std::fs::create_dir_all(root.join("docs")).unwrap();
    std::fs::create_dir_all(&worktree).unwrap();
    std::fs::write(root.join("docs/d.md"), "# d").unwrap();
    std::fs::write(worktree.join("w.md"), "# w").unwrap();
    std::fs::write(root.join("run.command"), "echo hi").unwrap();
    std::fs::write(d.0.join("outside.md"), "# o").unwrap();
    std::os::unix::fs::symlink(root.join("run.command"), root.join("x.md")).unwrap();
    std::os::unix::fs::symlink(d.0.join("outside.md"), root.join("out.md")).unwrap();

    let got = flows::contained(&root, &root.join("docs/d.md")).unwrap();
    assert_eq!(got, root.join("docs/d.md").canonicalize().unwrap());
    assert!(
        flows::contained(&worktree, &worktree.join("w.md")).is_ok(),
        "a sibling worktree is its own root"
    );
    let err = flows::contained(&root, &root.join("x.md"))
        .unwrap_err()
        .to_string();
    assert!(err.contains("not a Markdown"), "{err}");
    let err = flows::contained(&root, &root.join("out.md"))
        .unwrap_err()
        .to_string();
    assert!(err.contains("outside"), "{err}");
    assert!(flows::contained(&root, &root.join("missing.md")).is_err());
}
