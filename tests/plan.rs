//! What cos adds on the kit's workspace: connectors gated by the profile's
//! approved sources, the workspace inlined for the brains, handoffs from
//! tracked deliverables, runs recorded through the validated commit, and
//! review proposals written as a candidate, never committed.

use std::path::PathBuf;

use cos::reports::{Completeness, Coverage, Health, OwnerBasis, Proposal};
use cos::track::{self, Recorded};
use cos::{READ_ONLY, context, handoff, state};
use serde_json::{Value, json};

struct Home(PathBuf);

impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn home(name: &str) -> Home {
    let d = std::env::temp_dir().join(format!("cos-plan-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    Home(d.canonicalize().unwrap())
}

fn demo() -> Value {
    serde_json::from_str(state::DEMO_STATE).unwrap()
}

/// A workspace holding the kit's fictional demo state and `profile`.
fn workspace(name: &str, profile: Value) -> Home {
    let h = home(name);
    state::initialize(&h.0).unwrap();
    state::atomic_write(&h.0.join(".chief-of-staff/profile.json"), &profile).unwrap();
    state::commit(&h.0, &demo(), 0, "Fictional fixture").unwrap();
    h
}

fn profile(approved: bool, sources: Value) -> Value {
    let mut p: Value = serde_json::from_str(state::PROFILE_TEMPLATE).unwrap();
    p["localTracking"]["approved"] = json!(approved);
    p["sources"] = sources;
    p["timezone"] = json!("America/Chicago");
    p
}

fn email_source() -> Value {
    json!([
        { "id": "email", "service": "Gmail", "accountIdentity": "alex@example.com", "scope": "inbox, 2 days",
          "readsApproved": true, "approvalRef": "setup", "readBudget": "2 searches" },
        { "id": "notes", "service": "Notion", "accountIdentity": "alex", "scope": "all",
          "readsApproved": false, "approvalRef": null, "readBudget": null }
    ])
}

#[test]
fn work_orgs_come_only_from_an_approved_code_source() {
    let p = profile(
        false,
        json!([
            { "id": "code", "service": "GitHub", "readsApproved": true, "orgs": ["northwind", " "] },
            { "id": "old", "service": "GitHub", "readsApproved": false, "orgs": ["sidecorp"] },
            { "id": "email", "service": "Gmail", "readsApproved": true, "orgs": ["mailcorp"] }
        ]),
    );
    assert_eq!(state::code_orgs(&p), ["northwind"]);
    assert!(state::code_orgs(&profile(false, email_source())).is_empty());
}

#[test]
fn without_a_workspace_every_read_only_connector_stays() {
    let h = home("legacy");
    assert_eq!(context::tools(&h.0), READ_ONLY.to_vec());
    assert!(context::workspace(&h.0).contains("none"));
}

#[test]
fn a_profile_limits_connectors_to_its_approved_sources() {
    let h = workspace("gated", profile(false, email_source()));
    let tools = context::tools(&h.0);
    assert!(tools.contains(&"ToolSearch"));
    assert!(
        tools
            .iter()
            .any(|t| t.starts_with("mcp__claude_ai_Gmail__"))
    );
    assert!(
        !tools
            .iter()
            .any(|t| t.contains("Notion") || t.contains("Calendar") || t.contains("Atlassian")),
        "unapproved or unlisted sources stay off: {tools:?}"
    );
    assert!(
        tools.iter().all(|t| READ_ONLY.contains(t)),
        "never more than READ_ONLY"
    );
}

#[test]
fn a_profile_with_no_approved_source_allows_no_connector() {
    let h = workspace("none", profile(false, json!([])));
    assert_eq!(context::tools(&h.0), vec!["ToolSearch"]);
    assert!(context::workspace(&h.0).contains("Approved sources: none"));
}

#[test]
fn the_workspace_is_inlined_with_open_work_only() {
    let h = home("inline");
    state::initialize(&h.0).unwrap();
    state::atomic_write(
        &h.0.join(".chief-of-staff/profile.json"),
        &profile(false, email_source()),
    )
    .unwrap();
    let mut d = demo();
    let mut done = d["deliverables"][0].clone();
    done["id"] = json!("D-9");
    done["title"] = json!("Already shipped thing");
    done["status"] = json!("complete");
    done["completionEvidence"] =
        json!({ "reference": "https://example.com/r", "verifiedAt": "2030-04-09T12:00:00Z" });
    d["deliverables"].as_array_mut().unwrap().push(done);
    state::commit(&h.0, &d, 0, "Fixture").unwrap();

    let block = context::block(&h.0, &h.0);
    assert!(block.contains("### .chief-of-staff/state.json (open work)"));
    assert!(block.contains("Give the workshop deck to the reviewer"));
    assert!(
        !block.contains("Already shipped thing"),
        "completed work stays out"
    );
    assert!(block.contains("- email (gmail): scope inbox, 2 days; read budget 2 searches"));
    assert!(
        !block.contains("\"journal\""),
        "the journal is history, not today"
    );
}

#[test]
fn an_invalid_state_is_not_inlined() {
    let h = home("broken");
    state::initialize(&h.0).unwrap();
    std::fs::write(
        h.0.join(".chief-of-staff/state.json"),
        "{\"schemaVersion\": 2}",
    )
    .unwrap();
    let block = context::workspace(&h.0);
    assert!(block.contains("failed validation"), "{block}");
}

#[test]
fn a_handoff_has_the_nine_parts_and_grants_nothing() {
    let st = demo();
    let text = handoff::deliverable(&st, None, "D-1", handoff::Receiver::Chat).unwrap();
    for part in [
        "1. Outcome",
        "2. Why now",
        "3. Owner and timing",
        "4. Evidence",
        "5. Deliverable",
        "6. Done when",
        "7. Blockers and assumptions",
        "8. Permission boundary",
        "9. Report back",
    ] {
        assert!(text.contains(part), "missing {part}");
    }
    assert!(text.contains("finish line: Five signed pilot agreements by month end"));
    assert!(
        text.contains("Signed pilots: unknown of 5 agreements"),
        "null KPI is unknown, not zero"
    );
    assert!(text.contains("2030-04-10 (confirmed)"));
    assert!(text.contains("Coverage gaps: email partial"));
    assert!(text.contains("Read-only and draft-only"));

    let agent = handoff::deliverable(&st, None, "D-1", handoff::Receiver::Agent).unwrap();
    assert!(agent.contains("sandboxed worktree with no network"));
}

#[test]
fn completed_work_is_verified_not_handed_off_again() {
    let mut st = demo();
    st["deliverables"][0]["status"] = json!("complete");
    assert!(handoff::deliverable(&st, None, "D-1", handoff::Receiver::Chat).is_err());
    assert!(handoff::deliverable(&st, None, "D-404", handoff::Receiver::Chat).is_err());
}

fn coverage(health: Health, review: Completeness) -> Coverage {
    Coverage {
        source: "email".into(),
        health,
        review,
        window_start: "2030-04-09T09:00:00Z".into(),
        window_end: "2030-04-10T09:00:00Z".into(),
        note: "fixture".into(),
    }
}

#[test]
fn runs_are_recorded_only_when_tracking_is_approved() {
    let h = home("none-ws");
    let r = track::record_run(
        &h.0,
        "brief",
        &[],
        0,
        "2030-04-10T09:00:00+00:00",
        "2030-04-10T09:05:00+00:00",
    )
    .unwrap();
    assert_eq!(r, Recorded::NoWorkspace);

    let h = workspace("not-approved", profile(false, email_source()));
    let r = track::record_run(
        &h.0,
        "brief",
        &[],
        0,
        "2030-04-10T09:00:00+00:00",
        "2030-04-10T09:05:00+00:00",
    )
    .unwrap();
    assert_eq!(r, Recorded::NotApproved);
    assert_eq!(state::read(&h.0).unwrap().unwrap()["revision"], json!(1));
}

#[test]
fn a_reported_complete_review_never_advances_the_watermark() {
    let h = workspace("record", profile(true, email_source()));
    let r = track::record_run(
        &h.0,
        "brief",
        &[coverage(Health::Healthy, Completeness::Complete)],
        0,
        "2030-04-10T09:00:00+00:00",
        "2030-04-10T09:05:00+00:00",
    )
    .unwrap();
    assert_eq!(r, Recorded::Committed { revision: 2 });
    let st = state::read(&h.0).unwrap().unwrap();
    let email = state::find(&st, "sources", "email").unwrap();
    assert_eq!(email["review"], json!("partial"));
    assert!(email["lastCompleteThrough"].is_null());
    assert!(
        email["note"]
            .as_str()
            .unwrap()
            .contains("confirm before advancing")
    );
    assert_eq!(st["runs"][0]["mode"], json!("brief"));
    assert_eq!(
        st["journal"][1]["reason"],
        json!("cos brief: run receipt and source coverage")
    );
}

#[test]
fn an_access_failure_opens_one_incident() {
    let h = workspace("incident", profile(true, email_source()));
    for minute in ["05", "35"] {
        track::record_run(
            &h.0,
            "brief",
            &[coverage(
                Health::PermissionDenied,
                Completeness::NotReviewed,
            )],
            0,
            "2030-04-10T09:00:00+00:00",
            &format!("2030-04-10T09:{minute}:00+00:00"),
        )
        .unwrap();
    }
    let st = state::read(&h.0).unwrap().unwrap();
    let incidents = st["incidents"].as_array().unwrap();
    assert_eq!(incidents.len(), 1, "an open incident is not duplicated");
    assert_eq!(incidents[0]["kind"], json!("permission_denied"));
    assert_eq!(
        incidents[0]["observedAt"],
        json!("2030-04-10T09:05:00+00:00")
    );
}

#[test]
fn coverage_for_a_source_outside_the_profile_is_not_recorded() {
    let h = workspace("outside", profile(true, email_source()));
    let mut c = coverage(Health::Healthy, Completeness::Partial);
    c.source = "slack".into();
    track::record_run(
        &h.0,
        "brief",
        &[c],
        0,
        "2030-04-10T09:00:00+00:00",
        "2030-04-10T09:05:00+00:00",
    )
    .unwrap();
    let st = state::read(&h.0).unwrap().unwrap();
    assert!(state::find(&st, "sources", "slack").is_none());
}

#[test]
fn proposals_become_a_candidate_not_a_commit() {
    let h = workspace("propose", profile(false, json!([])));
    let proposals = vec![
        Proposal {
            goal_id: "G-1".into(),
            title: "Deck v2".into(),
            owner: "Alex".into(),
            owner_basis: OwnerBasis::Proposed,
            due_date: "2030-04-12".into(),
            due_basis: OwnerBasis::Proposed,
            done_when: "Reviewer confirms".into(),
            blocker: String::new(),
        },
        Proposal {
            goal_id: "G-9".into(),
            title: "Orphan".into(),
            owner: String::new(),
            owner_basis: OwnerBasis::Unknown,
            due_date: String::new(),
            due_basis: OwnerBasis::Unknown,
            done_when: "x".into(),
            blocker: String::new(),
        },
    ];
    let c = track::proposal_candidate(&h.0, &proposals, "cos review 2030-04-10").unwrap();
    assert_eq!(c.added, ["D-2"]);
    assert_eq!(c.skipped, ["Orphan"]);
    assert_eq!(c.revision, 1);
    assert_eq!(
        state::read(&h.0).unwrap().unwrap()["revision"],
        json!(1),
        "nothing committed"
    );

    let cand = state::load(&c.path).unwrap();
    let d2 = state::find(&cand, "deliverables", "D-2").unwrap();
    assert_eq!(d2["status"], json!("proposed"));
    assert_eq!(d2["ownerBasis"], json!("proposed"));
    state::commit(&h.0, &cand, c.revision, "Accept proposals").unwrap();
    assert_eq!(state::read(&h.0).unwrap().unwrap()["revision"], json!(2));
}
