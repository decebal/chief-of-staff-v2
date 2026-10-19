use cos::reports::{
    Basis, Brief, ChangeKind, MAX_ACTIONS, MAX_CHANGES, Review, Verb, brief_schema, review_schema,
};
use serde_json::{Value, json};

/// Every property the schema requires must be a field the struct writes, and
/// the other way round — otherwise `--json-schema` enforces one shape and
/// `serde` expects another.
fn required(schema: &Value) -> Vec<String> {
    let mut keys: Vec<String> = schema["required"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k.as_str().unwrap().to_owned())
        .collect();
    keys.sort();
    keys
}

fn keys(v: &Value) -> Vec<String> {
    let mut k: Vec<String> = v.as_object().unwrap().keys().cloned().collect();
    k.sort();
    k
}

fn brief_sample() -> Value {
    json!({
        "headline": "Release PR waits on you.",
        "needs_you": [{
            "verb": "delegate", "owner": "Morgan", "recommendation": "Hand the deck to the reviewer",
            "why": "The workshop tomorrow depends on it.", "next_step": "Send the draft link.",
            "source": "https://example.com/fictional/deck-request", "deadline": "2030-04-10",
            "deadline_basis": "confirmed", "goal_id": "G-1", "deliverable_id": "D-1",
            "agent_prompt": "… Stop before anything outward-facing."
        }],
        "what_changed": [{ "kind": "time_changed", "what": "Workshop moved to 10:00", "source": "cal" }],
        "calendar": [{ "start": "13:30", "end": "13:45", "what": "Stand-up", "prep": "bring blocker" }],
        "backlog": [{ "item": "Reply to A", "owner": "A", "source": "mail", "next_review": "Friday" }],
        "coverage": [{ "source": "email", "health": "healthy", "review": "partial",
                       "window_start": "2030-04-08T00:00:00Z", "window_end": "2030-04-09T09:00:00Z", "note": "first page" }],
        "gaps": [],
        "upcoming": [{ "date": "2030-04-30", "what": "Pilot deadline", "source": "G-1" }]
    })
}

fn review_sample() -> Value {
    json!({
        "summary": "One goal moved.",
        "goals": [{
            "goal_id": "G-1", "goal": "Win five pilots", "confidence": "sand", "kpi": "unknown",
            "evidence": "two calls", "weakened": "", "bottleneck": "deck", "recommendation": "sequence",
            "next": "deck first", "would_change": "a signed pilot", "agent_prompt": ""
        }],
        "proposed": [{ "goal_id": "G-1", "title": "Deck v2", "owner": "Alex", "owner_basis": "proposed",
                       "due_date": "", "due_basis": "unknown", "done_when": "reviewer confirms", "blocker": "" }],
        "decisions": [{ "decision": "promote E2E", "options": ["now", "later"], "recommendation": "now" }],
        "stop_doing": ["manual runs"],
        "what_changed": [],
        "gaps": [],
        "upcoming": []
    })
}

/// A brief saved before the kit's shape.
fn old_brief() -> Value {
    json!({
        "headline": "Two reviews block others; standup at 10.",
        "calendar": [{ "time": "10:00", "what": "Standup", "prep": "none" }],
        "must_do": [{ "item": "Review #81", "why": "blocks merge", "source": "gh:my-org/my-repo#81" }],
        "reply_to": [{ "from": "A", "subject": "SDK", "ask": "confirm version" }],
        "blockers": [{ "item": "TURN creds", "waiting_on": "infra", "unblock": "ping on ticket" }],
        "can_drop": ["newsletter"],
        "gaps": ["Notion not read: connector pending"],
        "celebrations": [{ "win": "Merged #80", "why_it_matters": "unblocked QA", "source": "gh:my-org/my-repo#80" }]
    })
}

fn old_review() -> Value {
    json!({
        "summary": "One bet moved.",
        "bets": [{ "bet": "E2E green", "confidence": "sand", "evidence": "3 PRs", "next": "shape scope" }],
        "drift": ["unplanned review"],
        "decisions": [{ "decision": "promote E2E", "options": ["now", "later"], "recommendation": "now" }],
        "stop_doing": ["manual runs"],
        "gaps": []
    })
}

#[test]
fn brief_schema_matches_struct_at_every_level() {
    let schema = brief_schema();
    let brief =
        serde_json::to_value(serde_json::from_value::<Brief>(brief_sample()).unwrap()).unwrap();
    assert_eq!(required(&schema), keys(&brief));
    for key in [
        "needs_you",
        "what_changed",
        "calendar",
        "backlog",
        "coverage",
        "upcoming",
    ] {
        assert_eq!(
            required(&schema["properties"][key]["items"]),
            keys(&brief[key][0]),
            "{key}"
        );
    }
}

#[test]
fn review_schema_matches_struct_at_every_level() {
    let schema = review_schema();
    let review =
        serde_json::to_value(serde_json::from_value::<Review>(review_sample()).unwrap()).unwrap();
    assert_eq!(required(&schema), keys(&review));
    for key in ["goals", "proposed", "decisions"] {
        assert_eq!(
            required(&schema["properties"][key]["items"]),
            keys(&review[key][0]),
            "{key}"
        );
    }
}

#[test]
fn the_brief_schema_caps_actions_and_changes() {
    let s = brief_schema();
    assert_eq!(s["properties"]["needs_you"]["maxItems"], json!(MAX_ACTIONS));
    assert_eq!(
        s["properties"]["what_changed"]["maxItems"],
        json!(MAX_CHANGES)
    );
    assert_eq!(MAX_ACTIONS, 3);
    assert_eq!(MAX_CHANGES, 2);
}

#[test]
fn briefs_saved_in_the_old_shape_still_load() {
    let b: Brief = serde_json::from_value(old_brief()).unwrap();
    assert_eq!(b.needs_you.len(), 1);
    assert_eq!(b.needs_you[0].verb, Verb::Do);
    assert_eq!(b.needs_you[0].recommendation, "Review #81");
    assert_eq!(b.needs_you[0].deadline_basis, Basis::None);
    assert_eq!(b.what_changed[0].kind, ChangeKind::Done);
    assert_eq!(
        b.backlog.len(),
        3,
        "reply_to, blockers and can_drop move to the backlog"
    );
    let saved = serde_json::to_value(&b).unwrap();
    for gone in [
        "must_do",
        "reply_to",
        "blockers",
        "can_drop",
        "celebrations",
    ] {
        assert!(saved.get(gone).is_none(), "{gone} written back");
    }

    let r: Review = serde_json::from_value(old_review()).unwrap();
    assert_eq!(r.goals[0].goal, "E2E green");
    assert_eq!(r.goals[0].kpi, "unknown");
    assert!(r.stop_doing.iter().any(|s| s == "Drift: unplanned review"));
}

#[test]
fn review_rejects_confidence_outside_the_table() {
    let mut v = review_sample();
    v["goals"][0]["confidence"] = json!("medium");
    assert!(serde_json::from_value::<Review>(v).is_err());
}

#[test]
fn brief_markdown_leads_with_needs_you_and_omits_empty_sections() {
    let mut v = brief_sample();
    v["what_changed"] = json!([]);
    let b: Brief = serde_json::from_value(v).unwrap();
    let md = b.to_markdown("2030-04-09");
    let needs = md.find("## Needs you").unwrap();
    assert!(md.contains("**DELEGATE: Morgan** Hand the deck to the reviewer"));
    assert!(md.contains("(due 2030-04-10)"));
    assert!(!md.contains("What changed"), "an empty section is left out");
    assert!(
        md.find("## Calendar").unwrap() > needs,
        "detail sits behind the brief"
    );
    assert!(
        md.contains("email: healthy · partial"),
        "partial coverage is named"
    );
}

#[test]
fn review_markdown_shows_the_steer_and_lowercase_tiers() {
    let r: Review = serde_json::from_value(review_sample()).unwrap();
    let md = r.to_markdown("2030-04-09");
    assert!(md.contains("`sand` **Win five pilots**"));
    assert!(md.contains("sequence differently: deck first"));
    assert!(md.contains("owner Alex (proposed), no date"));
    assert!(!md.contains("Gaps"));
}
