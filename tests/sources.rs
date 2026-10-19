//! Reads stay inside what the profile approved: local facts only for approved
//! `code` / `tasks` sources, a coverage row for every approved source, and
//! calendar entries only inside the window.

use std::path::PathBuf;

use cos::gather::{self, Facts, Reads};
use cos::pipeline::{cover_every_source, dates_at, hold_to_window};
use cos::reports::{Brief, Completeness, Health};
use cos::{context, state};
use serde_json::{Value, json};

struct Home(PathBuf);

impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn workspace(name: &str, sources: Value) -> Home {
    let d = std::env::temp_dir().join(format!("cos-sources-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let h = Home(d.canonicalize().unwrap());
    state::initialize(&h.0).unwrap();
    let mut p: Value = serde_json::from_str(state::PROFILE_TEMPLATE).unwrap();
    p["sources"] = sources;
    state::atomic_write(&h.0.join(".chief-of-staff/profile.json"), &p).unwrap();
    h
}

fn source(id: &str, service: &str, approved: bool) -> Value {
    json!({ "id": id, "service": service, "accountIdentity": "me", "scope": "s",
            "readsApproved": approved, "approvalRef": "test", "readBudget": "b" })
}

fn brief(coverage: Value, calendar: Value) -> Brief {
    serde_json::from_value(json!({
        "headline": "h", "needs_you": [], "what_changed": [], "calendar": calendar,
        "backlog": [], "coverage": coverage, "gaps": [], "upcoming": []
    }))
    .unwrap()
}

#[test]
fn local_reads_follow_the_profile() {
    let none = std::env::temp_dir().join(format!("cos-sources-none-{}", std::process::id()));
    assert_eq!(
        context::local_reads(&none),
        Reads::ALL,
        "no workspace: as before"
    );

    let h = workspace("connectors", json!([source("calendar", "calendar", true)]));
    assert_eq!(
        context::local_reads(&h.0),
        Reads {
            code: false,
            tasks: false
        }
    );

    let h = workspace(
        "both",
        json!([
            source("code", "GitHub", true),
            source("tasks", "chronis", true)
        ]),
    );
    assert_eq!(context::local_reads(&h.0), Reads::ALL);

    let h = workspace("unapproved", json!([source("code", "github", false)]));
    assert!(!context::local_reads(&h.0).code);
}

#[test]
fn unapproved_local_facts_are_never_collected() {
    let f = gather::collect(
        std::path::Path::new("/nonexistent"),
        "2026-10-11",
        Reads {
            code: false,
            tasks: false,
        },
    );
    assert_eq!(f.unread, ["tasks", "code"]);
    assert!(
        f.gaps.is_empty(),
        "nothing ran, so nothing failed: {:?}",
        f.gaps
    );
    let text = f.render();
    assert_eq!(text.matches("not read: no approved source").count(), 4);
}

#[test]
fn calendar_window_is_this_week_plus_tomorrow_on_sunday() {
    let at = |s: &str| s.parse::<jiff::Zoned>().unwrap();
    let sunday = dates_at(at("2026-10-18T19:00:00-05:00[America/Chicago]"));
    assert_eq!(sunday.calendar_end, "2026-10-19");
    let wednesday = dates_at(at("2026-10-14T09:00:00-05:00[America/Chicago]"));
    assert_eq!(wednesday.calendar_end, "2026-10-18");

    let mut b = brief(
        json!([]),
        json!([
            { "date": "2026-10-19", "start": "09:00", "end": "10:00", "what": "Mon", "prep": "" },
            { "date": "2026-10-21", "start": "18:00", "end": "19:00", "what": "Design review", "prep": "" },
            { "date": "2026-10-17", "start": "18:00", "end": "19:00", "what": "Past", "prep": "" },
            { "start": "20:00", "end": "21:00", "what": "Undated", "prep": "" }
        ]),
    );
    hold_to_window(&mut b, &sunday);
    let kept: Vec<&str> = b.calendar.iter().map(|s| s.what.as_str()).collect();
    assert_eq!(kept, ["Mon", "Undated"]);
    assert!(b.gaps[0].contains("2 calendar entries"));
    assert!(b.gaps[0].contains("Design review"));
}

#[test]
fn every_approved_source_gets_a_coverage_row() {
    let h = workspace(
        "coverage",
        json!([
            source("calendar", "calendar", true),
            source("email", "mail", true),
            source("notes", "notion", true),
            source("code", "github", true),
            source("tasks", "chronis", true),
            source("jira", "jira", false)
        ]),
    );
    let facts = Facts {
        gaps: vec!["cn ready in repo: boom".into()],
        ..Facts::default()
    };
    let mut b = brief(
        json!([{ "source": "email", "health": "healthy", "review": "partial",
                 "window_start": "", "window_end": "", "note": "read" }]),
        json!([]),
    );
    let called = vec![
        "ToolSearch".to_string(),
        "mcp__claude_ai_Google_Calendar__list_events".to_string(),
    ];
    cover_every_source(&mut b, &h.0, &facts, &called);

    let row = |id: &str| b.coverage.iter().find(|c| c.source == id).unwrap();
    assert_eq!(b.coverage.len(), 5, "jira is not approved");
    assert_eq!(row("email").note, "read", "a reported row is kept as is");
    assert_eq!(row("calendar").review, Completeness::Partial);
    assert!(row("calendar").note.contains("list_events"));
    assert_eq!(row("notes").review, Completeness::NotReviewed);
    assert!(row("notes").note.starts_with("Not attempted"));
    assert_eq!(row("code").health, Health::Healthy);
    assert!(row("code").note.starts_with("Collected by cos"));
    assert_eq!(row("tasks").health, Health::Unavailable);
    assert_eq!(b.gaps.len(), 4);
}
