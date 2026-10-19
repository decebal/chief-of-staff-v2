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
