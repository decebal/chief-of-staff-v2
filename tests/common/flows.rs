//! A fictional machine's flows for the Flows page's browser tests: four repos,
//! a design at the gate, a drifted design, a flow being built with every bead state and an
//! external blocker and a hire at work on one bead, a review with findings, a done legacy flow, and as many
//! generated flows as the bead count asks for. Reads no home.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

pub const REPOS: [&str; 4] = ["beta-web", "gamma-data", "delta-ops", "alpha-api"];
pub const PER_FLOW: usize = 12;

/// The page on disk and what it should count.
pub struct FlowsPage {
    pub page: PathBuf,
    /// Every flow, done ones included.
    pub flows: usize,
    /// Starts and beads the Map draws: every flow that is not done and has an open bead.
    pub map_nodes: usize,
    /// The beads among them, without the starts.
    pub map_beads: usize,
    /// Generated flows in each of `REPOS`.
    pub generated: [usize; 4],
}

fn t(id: &str, title: &str, kind: &str, status: &str, parent: Option<&str>) -> Value {
    json!({ "id": id, "title": title, "status": status, "task_type": kind, "parent": parent,
            "blocked_by": [], "approved": null, "description": "", "archived": false })
}

fn with(mut v: Value, key: &str, value: Value) -> Value {
    v[key] = value;
    v
}

/// The hand-made flows of `alpha-api`.
fn alpha() -> Vec<Value> {
    let crit = "As the gateway, I want this.\n\n## Acceptance criteria\n- [x] Test: the store rejects a bad row (file: tests/store.rs)\n- [ ] Backfill runs under the deadline\n- [ ] All tests green: task test\n";
    vec![
        with(
            t("t-a000", "Rate limits per tenant", "epic", "open", None),
            "description",
            json!(
                "ritual-brief: docs/plans/a-brief.md\nritual-branch: feat/a\nritual-design: docs/plans/a-design.md"
            ),
        ),
        with(
            with(
                t(
                    "t-b000",
                    "Request log on the new store",
                    "epic",
                    "open",
                    None,
                ),
                "approved",
                json!(true),
            ),
            "description",
            json!(
                "ritual-design: docs/plans/b-design.md\nritual-plan: docs/plans/b-plan.md\nritual-approved: docs/plans/b-design.md sha256:00 by Alex"
            ),
        ),
        with(
            with(
                t(
                    "t-b001",
                    "US-001: table and migration",
                    "task",
                    "done",
                    Some("t-b000"),
                ),
                "done_at",
                json!("2026-11-10T09:00:00Z"),
            ),
            "claimed_by",
            json!("ann"),
        ),
        with(
            t(
                "t-b002",
                "US-002: write path",
                "task",
                "in-progress",
                Some("t-b000"),
            ),
            "claimed_by",
            json!("claude:1a2b3c4d"),
        ),
        with(
            t(
                "t-b003",
                "US-003: read path",
                "task",
                "open",
                Some("t-b000"),
            ),
            "blocked_by",
            json!(["t-b001"]),
        ),
        with(
            with(
                t(
                    "t-b004",
                    "US-004: backfill job",
                    "task",
                    "open",
                    Some("t-b000"),
                ),
                "blocked_by",
                json!(["t-b002"]),
            ),
            "description",
            json!(crit),
        ),
        with(
            t(
                "t-b005",
                "US-005: dashboards",
                "task",
                "open",
                Some("t-b000"),
            ),
            "blocked_by",
            json!(["t-b003", "t-b004", "t-b001"]),
        ),
        with(
            t(
                "t-b006",
                "US-006: retire the old writer",
                "task",
                "open",
                Some("t-b000"),
            ),
            "blocked_by",
            json!(["t-ext-1"]),
        ),
        with(
            with(
                t("t-c000", "Health endpoint", "epic", "open", None),
                "approved",
                json!(true),
            ),
            "description",
            json!(
                "ritual-design: docs/plans/c-design.md\nritual-review: docs/plans/c-code-review.md findings"
            ),
        ),
        t("t-c001", "Add /healthz", "task", "done", Some("t-c000")),
        t(
            "t-c002",
            "Wire the balancer check",
            "task",
            "done",
            Some("t-c000"),
        ),
        with(
            with(
                t("t-f000", "Retry budget", "epic", "open", None),
                "approved",
                json!(true),
            ),
            "description",
            json!(
                "ritual-design: docs/plans/f-design.md\nritual-plan: docs/plans/f-plan.md\nritual-approved: docs/plans/f-design.md sha256:00 by Alex"
            ),
        ),
        t("t-d000", "Old cleanup", "epic", "done", None),
        t("t-d001", "Remove the cron", "task", "done", Some("t-d000")),
    ]
}

/// One generated flow: twelve beads in four columns of three, the first
/// column done, one bead in progress, the rest ready or blocked.
fn generated(i: usize) -> Vec<Value> {
    let epic = format!("t-e{i:03}");
    let owners = ["ann", "claude:aaaa1111", "bo"];
    let mut out = vec![t(
        &epic,
        &format!("Generated flow {i:03}"),
        "epic",
        "open",
        None,
    )];
    for j in 0..PER_FLOW {
        let id = format!("t-e{i:03}x{j:02}");
        let status = match j {
            0..=2 => "done",
            3 => "in-progress",
            _ => "open",
        };
        let mut b = t(
            &id,
            &format!("Generated bead {i:03}.{j:02}"),
            "task",
            status,
            Some(&epic),
        );
        if j >= 3 {
            b = with(b, "blocked_by", json!([format!("t-e{i:03}x{:02}", j - 3)]));
        }
        if j == 3 {
            b = with(b, "claimed_by", json!(owners[i % 3]));
        }
        out.push(with(
            b,
            "created_at",
            json!(format!("2026-11-{:02}T10:00:00Z", 1 + i % 28)),
        ));
    }
    out
}

fn hire(id: &str, title: &str, phase: cos::agents::Phase, bead: &str) -> cos::agents::HireView {
    cos::agents::HireView {
        id: id.into(),
        title: title.into(),
        role: Some("builder".into()),
        identity: 1,
        phase,
        active_ms: 600_000,
        paused_now: false,
        started: Some("2026-11-20T09:00:00Z".into()),
        ended: None,
        done_today: false,
        deadline_secs: 3600,
        word: Some("working".into()),
        msg: None,
        source: format!("bead:alpha-api/{bead}"),
    }
}

/// Writes the page for about `beads` beads into `dir`.
pub fn write(dir: &Path, beads: usize) -> FlowsPage {
    let bulk = beads.saturating_sub(10) / PER_FLOW;
    let mut per: Vec<Vec<Value>> = vec![Vec::new(); 4];
    per[3] = alpha();
    let mut generated_in = [0usize; 4];
    for i in 0..bulk {
        per[i % 4].extend(generated(i));
        generated_in[i % 4] += 1;
    }
    let mut flows = Vec::new();
    for (repo, list) in REPOS.iter().zip(&per) {
        let tasks: Vec<cos::flows::Task> =
            serde_json::from_value(Value::Array(list.clone())).unwrap();
        flows.extend(cos::flows::flows_in(repo, None, &tasks));
    }
    // No design file exists to hash, so the drift is set rather than found.
    for f in flows.iter_mut().filter(|f| f.id == "t-f000") {
        f.step = cos::flows::Step::Drift;
        f.needs_you = Some(cos::flows::NeedsYou::Drift);
    }
    for f in flows.iter_mut().filter(|f| f.id == "t-b000") {
        f.hires = vec![
            hire(
                "run-read",
                "Read path",
                cos::agents::Phase::Working,
                "t-b003",
            ),
            hire("run-table", "Table", cos::agents::Phase::Done, "t-b001"),
        ];
    }
    let scan = cos::flows::Scan {
        flows,
        gaps: vec![],
        repos: REPOS.len(),
    };
    let page = dir.join("flows.html");
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(&page, cos::flows::page_html(&scan, true).unwrap()).unwrap();
    FlowsPage {
        page,
        flows: scan.flows.len(),
        map_nodes: 7 + bulk * (PER_FLOW + 1),
        map_beads: 6 + bulk * PER_FLOW,
        generated: generated_in,
    }
}
