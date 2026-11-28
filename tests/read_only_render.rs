//! Rendering writes nothing. Today's data and markup, the Flows scan and the
//! hire snapshot read the business home and leave every file and directory
//! under it as it was: same paths, same lengths, same mtimes.
//!
//! The walk covers the whole demo home, the Chronis stores under its
//! `projects/` included: the test first shows that a `cn` read alone leaves
//! the home unchanged, so nothing is excluded from the walk.
//!
//! `report::data`'s callees, read and confirmed write-free before this test
//! landed: `history::load`, `history::backfilled`, `history::kpi_points`,
//! `kpis::by_goal`, `delivery::view`, `constraints::view`, `state::read`,
//! `state::profile`, `digest::build` (it reads runs with `agents::read_raw`;
//! it notes an unparsable timestamp in the audit log, and the demo home has
//! none) and `agents::snapshot`.

#[allow(dead_code)]
#[path = "../examples/demo.rs"]
mod demo;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use cos::agents::{self, Harness, Spec, State, Status};

type Listing = BTreeMap<PathBuf, (u64, Option<SystemTime>)>;

fn scratch(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("cos-readonly-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    root.canonicalize().unwrap()
}

/// Every entry under `root` (not following links): path, length, mtime.
fn walk(root: &Path) -> Listing {
    let mut out = Listing::new();
    let mut todo = vec![root.to_path_buf()];
    while let Some(dir) = todo.pop() {
        for e in std::fs::read_dir(&dir).unwrap().flatten() {
            let path = e.path();
            let meta = std::fs::symlink_metadata(&path).unwrap();
            if meta.is_dir() {
                todo.push(path.clone());
            }
            out.insert(path, (meta.len(), meta.modified().ok()));
        }
    }
    out
}

fn changes(before: &Listing, after: &Listing) -> Vec<String> {
    let mut out = Vec::new();
    for (p, v) in after {
        match before.get(p) {
            None => out.push(format!("created {}", p.display())),
            Some(b) if b != v => out.push(format!("changed {}", p.display())),
            _ => {}
        }
    }
    for p in before.keys().filter(|p| !after.contains_key(*p)) {
        out.push(format!("removed {}", p.display()));
    }
    out
}

#[test]
fn rendering_today_flows_and_the_hire_snapshot_writes_nothing() {
    let home = scratch("demo").join("home");
    std::fs::create_dir_all(&home).unwrap();
    demo::build(&home).unwrap();
    let projects = home.join("projects");
    let cn = home.join("bin/cn");

    let before = walk(&home);
    for repo in cos::flows::repos(&projects).unwrap() {
        cos::flows::read_tasks(&cn, &repo, Duration::from_secs(10)).unwrap();
    }
    let after = walk(&home);
    assert_eq!(
        changes(&before, &after),
        Vec::<String>::new(),
        "a cn read wrote under the home"
    );

    let before = walk(&home);
    let data = cos::report::data(&home, &home, false).unwrap();
    let hero = cos::today::hero(&data);
    let body = cos::today::body(&data);
    let tz = jiff::tz::TimeZone::UTC;
    let scan = cos::flows::scan(
        &projects,
        &cn,
        Duration::from_secs(30),
        Some(cos::flows::Hires {
            home: &home,
            tz: &tz,
        }),
    );
    let hires = agents::snapshot(&home, &tz, jiff::Timestamp::now());
    let after = walk(&home);
    assert_eq!(
        changes(&before, &after),
        Vec::<String>::new(),
        "rendering wrote under the home"
    );

    assert!(!hero.is_empty() && !body.is_empty());
    assert!(!scan.flows.is_empty(), "{:?}", scan.gaps);
    assert!(!hires.is_empty(), "the demo home has hires");
    assert_eq!(data["hires"].as_array().map(Vec::len), Some(hires.len()));
    for w in data["herdr_waiting"].as_array().unwrap() {
        let h = data["hires"]
            .as_array()
            .unwrap()
            .iter()
            .find(|h| h["id"] == w["id"])
            .unwrap_or_else(|| panic!("{w} waits in Herdr but is not a hire"));
        assert_eq!(h["phase"], "needs-you", "{h}");
    }
    assert!(
        !data["herdr_waiting"].as_array().unwrap().is_empty(),
        "the demo's Researcher waits on you in Herdr"
    );
}

fn spec(id: &str, created: &str) -> Spec {
    Spec {
        id: id.into(),
        harness: Harness::Claude,
        program: "claude".into(),
        model: None,
        repo: None,
        base: None,
        workdir: PathBuf::from("scratch"),
        worktree: false,
        title: "Prepare the board pack".into(),
        source: "bead:acme/demo-repo/t-1".into(),
        prompt: "do it".into(),
        deadline_secs: 1800,
        created: created.into(),
        slots: None,
        contract: Default::default(),
        role: Some("writer".into()),
        terminal: Some(format!("cos-{id}")),
        workspace: None,
        projects: None,
        skills: None,
    }
}

fn put(home: &Path, spec: &Spec, state: &State) {
    let d = agents::dir(home, &spec.id);
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join("spec.json"), serde_json::to_vec(spec).unwrap()).unwrap();
    std::fs::write(d.join("state.json"), serde_json::to_vec(state).unwrap()).unwrap();
}

/// A hire whose preparing process is gone before Herdr had it is what
/// `agents::list` reconciles to `lost`, writing its state. Rendering Today and
/// taking the snapshot leave it as stored.
#[test]
fn the_report_and_the_snapshot_leave_an_unreconciled_run_as_stored() {
    let root = scratch("lost");
    let (home, ws) = (root.join("home"), root.join("ws"));
    std::fs::create_dir_all(&ws).unwrap();
    let spec = spec("20260101-090000-a001", "2026-01-01T09:00:00Z");
    let state = State {
        status: Status::Starting,
        ..State::default()
    };
    put(&home, &spec, &state);

    let before = walk(&home);
    let data = cos::report::data(&home, &ws, false).unwrap();
    let hires = agents::snapshot(&home, &jiff::tz::TimeZone::UTC, jiff::Timestamp::now());
    assert_eq!(changes(&before, &walk(&home)), Vec::<String>::new());

    assert_eq!(hires.len(), 1);
    assert_eq!(hires[0].phase, agents::Phase::Starting);
    assert_eq!(hires[0].source, "bead:acme/demo-repo/t-1");
    assert_eq!(hires[0].deadline_secs, 1800);
    assert_eq!(data["hires"][0]["phase"], "starting", "{}", data["hires"]);
}
