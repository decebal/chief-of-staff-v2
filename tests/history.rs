use std::collections::BTreeMap;

use cos::dash::archived;
use cos::gather::Pr;
use cos::history::{DayRow, Merged, merge_prs, upsert_day};
use cos::report::{label, timeline};
use cos::reports::Milestone;

fn pr(n: u64, closed: Option<&str>) -> Pr {
    Pr {
        repo: "org/repo".into(),
        number: n,
        title: format!("pr {n}"),
        draft: false,
        updated: "2026-10-16T10:00:00Z".into(),
        closed: closed.map(str::to_owned),
    }
}

#[test]
fn merged_log_dedupes_and_orders_by_day() {
    let mut log: Vec<Merged> = vec![];
    let a = [
        pr(2, Some("2026-10-15T09:00:00Z")),
        pr(1, Some("2026-10-14T09:00:00Z")),
    ];
    merge_prs(&mut log, a.iter());
    merge_prs(
        &mut log,
        [pr(2, Some("2026-10-15T09:00:00Z")), pr(3, None)].iter(),
    );
    assert_eq!(log.iter().map(|m| m.number).collect::<Vec<_>>(), [1, 2]);
    assert_eq!(log[0].day, "2026-10-14");
}

#[test]
fn a_review_rerun_keeps_the_briefs_numbers_for_that_day() {
    let brief_row = DayRow {
        date: "2026-10-16".into(),
        review_queue: 15,
        mine_open: 3,
        ready: BTreeMap::from([("p1".into(), 4)]),
        must_do: Some(5),
        meeting_minutes: Some(25),
        wins: Some(3),
    };
    let mut days = vec![brief_row];
    let review_row = DayRow {
        date: "2026-10-16".into(),
        review_queue: 14,
        ..DayRow::default()
    };
    upsert_day(&mut days, review_row);
    upsert_day(
        &mut days,
        DayRow {
            date: "2026-10-15".into(),
            ..DayRow::default()
        },
    );
    assert_eq!(days.len(), 2);
    assert_eq!(days[0].date, "2026-10-15");
    assert_eq!(days[1].review_queue, 14);
    assert_eq!(days[1].must_do, Some(5));
    assert_eq!(days[1].meeting_minutes, Some(25));
}

#[test]
fn diagram_labels_cannot_break_the_markdy_line() {
    assert_eq!(label("say \"hi\"\nnow"), "say 'hi' now");
    assert!(label(&"x".repeat(80)).chars().count() <= 44);
    let m = Milestone {
        date: "2026-10-19".into(),
        what: "7.0.0 \"release\"".into(),
        source: "cal".into(),
    };
    let src = timeline(&[&m]);
    assert!(src.contains("service M0 \"10-19 · 7.0.0 'release'\" focal=true"));
    assert!(src.lines().all(|l| l.matches('"').count() % 2 == 0));
}

#[test]
fn backfill_windows_cover_this_month_and_twelve_before() {
    let m = cos::history::months_to("2026-10-16");
    assert_eq!(m.len(), 13);
    assert_eq!(m[0], ("2026-10-01".to_owned(), "2026-10-16".to_owned()));
    assert_eq!(m[1], ("2026-09-01".to_owned(), "2026-09-30".to_owned()));
    assert_eq!(m[8], ("2026-02-01".to_owned(), "2026-02-28".to_owned()));
    assert_eq!(m[12], ("2025-10-01".to_owned(), "2025-10-31".to_owned()));
}

#[test]
fn only_dated_report_paths_are_served() {
    let home = std::path::Path::new("/h");
    assert!(archived(home, "/reports/2026-10-16.html").is_some());
    for bad in [
        "/reports/../.mcp-token",
        "/reports/2026-10-16.html/../x",
        "/reports/..%2f..%2fetc.html",
        "/out/.mcp-token",
        "/reports/20261016xx.html",
    ] {
        assert!(archived(home, bad).is_none(), "{bad}");
    }
}

#[test]
fn a_merged_entry_without_merged_at_still_reads_and_a_new_one_carries_closed_at() {
    let old: Merged =
        serde_json::from_str(r#"{"repo":"org/repo","number":1,"title":"pr 1","day":"2026-10-14"}"#)
            .unwrap();
    assert_eq!(old.merged_at, None);
    let mut log: Vec<Merged> = vec![];
    merge_prs(&mut log, [pr(2, Some("2026-10-15T09:00:00Z"))].iter());
    assert_eq!(log[0].merged_at.as_deref(), Some("2026-10-15T09:00:00Z"));
}

/// A home and a workspace whose state holds three KPIs: one finite and dated,
/// one with no evidence time, one whose current value is not a number.
fn kpi_home(name: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!("cos-kpi-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let ws = root.join("ws");
    std::fs::create_dir_all(ws.join(".chief-of-staff")).unwrap();
    let mut st: serde_json::Value = serde_json::from_str(cos::state::DEMO_STATE).unwrap();
    let kpi = |id: &str, name: &str, current: serde_json::Value, evidence: serde_json::Value| {
        serde_json::json!({
            "id": id, "goalId": "G-1", "name": name, "unit": "agreements",
            "baseline": 0, "target": 5, "current": current, "evidenceAt": evidence,
            "source": "email", "reportingWindow": "2030-04"
        })
    };
    st["kpis"] = serde_json::json!([
        kpi(
            "K-2",
            "Signed pilots",
            2.into(),
            "2026-11-20T09:00:00Z".into()
        ),
        kpi(
            "K-3",
            "Not yet measured",
            serde_json::Value::Null,
            serde_json::Value::Null
        ),
    ]);
    std::fs::write(
        ws.join(".chief-of-staff/state.json"),
        serde_json::to_vec(&st).unwrap(),
    )
    .unwrap();
    (root.join("home"), ws.canonicalize().unwrap())
}

#[test]
fn only_finite_dated_kpis_record_and_a_repeat_does_not() {
    let (home, ws) = kpi_home("checks");
    assert_eq!(cos::history::record_kpis(&home, &ws).unwrap(), 1);
    assert_eq!(cos::history::record_kpis(&home, &ws).unwrap(), 0);
    let points = cos::history::kpi_points(&home);
    assert_eq!(points.len(), 1, "{points:?}");
    assert_eq!(points[0].kpi, "Signed pilots");
    assert_eq!(points[0].value, 2.0);
    assert_eq!(points[0].evidence_at, "2026-11-20T09:00:00Z");
}

#[test]
fn two_threads_recording_one_point_append_one_line() {
    let (home, ws) = kpi_home("threads");
    let (h2, w2) = (home.clone(), ws.clone());
    let other = std::thread::spawn(move || cos::history::record_kpis(&h2, &w2).unwrap());
    let here = cos::history::record_kpis(&home, &ws).unwrap();
    let there = other.join().unwrap();
    assert_eq!(here + there, 1, "exactly one thread appends the point");
    let text = std::fs::read_to_string(home.join("out/history/kpis.jsonl")).unwrap();
    assert_eq!(text.lines().count(), 1);
}

#[test]
fn a_kpi_point_needs_a_finite_value_an_evidence_time_and_a_new_pair() {
    let st = serde_json::json!({"kpis": [
        {"name": "Good", "current": 2, "evidenceAt": "2026-11-20T09:00:00Z"},
        {"name": "No time", "current": 3, "evidenceAt": null},
        {"name": "Words", "current": "n/a", "evidenceAt": "2026-11-20T09:00:00Z"},
        {"name": "Bad time", "current": 4, "evidenceAt": "last week"},
        {"name": "Unset", "current": null, "evidenceAt": "2026-11-20T09:00:00Z"},
        {"name": "", "current": 5, "evidenceAt": "2026-11-20T09:00:00Z"},
        {"name": "  ", "current": 6, "evidenceAt": "2026-11-20T09:00:00Z"},
        {"name": "Good", "current": 2, "evidenceAt": "2026-11-20T09:00:00Z"}
    ]});
    let got = cos::history::new_kpi_points(&st, &[], "2026-11-25T10:00:00Z");
    assert_eq!(
        got.iter().map(|p| p.kpi.as_str()).collect::<Vec<_>>(),
        ["Good"]
    );
    assert!(cos::history::new_kpi_points(&st, &got, "2026-11-25T11:00:00Z").is_empty());
}
