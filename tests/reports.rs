use cos::reports::{Basis, Brief, ChangeKind, MAX_ACTIONS, MAX_CHANGES, Verb, brief_schema};
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
        "week": week_sample(),
        "calendar": [{ "start": "13:30", "end": "13:45", "what": "Stand-up", "prep": "bring blocker" }],
        "backlog": [{ "item": "Reply to A", "owner": "A", "source": "mail", "next_review": "Friday" }],
        "coverage": [{ "source": "email", "health": "healthy", "review": "partial",
                       "window_start": "2030-04-08T00:00:00Z", "window_end": "2030-04-09T09:00:00Z", "note": "first page" }],
        "gaps": [],
        "upcoming": [{ "date": "2030-04-30", "what": "Pilot deadline", "source": "G-1" }],
        "tickets": [{ "key": "PLAT-7", "summary": "Canary pipeline", "status": "In Progress",
                      "priority": "High", "url": "https://example.com/fictional/PLAT-7", "goal_id": "G-1" }]
    })
}

fn week_sample() -> Value {
    json!({
        "summary": "One goal moved.",
        "goals": [{
            "goal_id": "G-1", "goal": "Win five pilots", "confidence": "sand", "kpi": "unknown",
            "evidence": "two calls", "bottleneck": "deck", "recommendation": "sequence",
            "next": "deck first", "would_change": "a signed pilot", "agent_prompt": ""
        }],
        "proposed": [{ "goal_id": "G-1", "title": "Deck v2", "owner": "Alex", "owner_basis": "proposed",
                       "due_date": "", "due_basis": "unknown", "done_when": "reviewer confirms", "blocker": "" }],
        "decisions": [{ "decision": "promote E2E", "options": ["now", "later"], "recommendation": "now" }],
        "stop_doing": ["manual runs"]
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
        "tickets",
    ] {
        assert_eq!(
            required(&schema["properties"][key]["items"]),
            keys(&brief[key][0]),
            "{key}"
        );
    }
    let week = &schema["properties"]["week"];
    assert_eq!(required(week), keys(&brief["week"]));
    for key in ["goals", "proposed", "decisions"] {
        assert_eq!(
            required(&week["properties"][key]["items"]),
            keys(&brief["week"][key][0]),
            "week.{key}"
        );
    }
}

#[test]
fn jira_tickets_reach_the_brief_markdown_and_old_briefs_have_none() {
    let brief: Brief = serde_json::from_value(brief_sample()).unwrap();
    let md = brief.to_markdown("2030-04-09");
    assert!(md.contains("## Jira assigned to you"), "{md}");
    assert!(md.contains(
        "[PLAT-7](https://example.com/fictional/PLAT-7) Canary pipeline — In Progress · G-1"
    ));
    let old: Brief = serde_json::from_value(old_brief()).unwrap();
    assert!(old.tickets.is_empty());
}

#[test]
fn the_tickets_rule_follows_the_profile_and_refuses_odd_project_keys() {
    let ws = std::env::temp_dir().join(format!("cos-tickets-rule-{}", std::process::id()));
    std::fs::create_dir_all(ws.join(".chief-of-staff")).unwrap();
    assert_eq!(cos::pipeline::tickets_rule(&ws), "", "no profile, no rule");
    std::fs::write(
        ws.join(".chief-of-staff/profile.json"),
        r#"{ "jiraProjects": ["WEB", "VM", "X) OR (1=1"] }"#,
    )
    .unwrap();
    let rule = cos::pipeline::tickets_rule(&ws);
    assert!(rule.contains("project in (WEB, VM)"), "{rule}");
    assert!(
        !rule.contains("1=1"),
        "a key that is not alphanumeric never reaches JQL"
    );
    std::fs::remove_dir_all(ws).unwrap();
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
    assert!(
        b.week.is_empty(),
        "a brief saved before the week part has none"
    );
}

#[test]
fn a_brief_saved_without_the_week_part_loads_and_writes_no_week_section() {
    let mut v = brief_sample();
    v.as_object_mut().unwrap().remove("week");
    let b: Brief = serde_json::from_value(v).unwrap();
    assert!(b.week.is_empty());
    assert!(!b.to_markdown("2030-04-09").contains("## This week"));
}

#[test]
fn the_week_rejects_confidence_outside_the_table() {
    let mut v = brief_sample();
    v["week"]["goals"][0]["confidence"] = json!("medium");
    assert!(serde_json::from_value::<Brief>(v).is_err());
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
fn the_week_sits_behind_today_and_shows_the_steer_and_lowercase_tiers() {
    let b: Brief = serde_json::from_value(brief_sample()).unwrap();
    let md = b.to_markdown("2030-04-09");
    let week = md.find("## This week").unwrap();
    assert!(md.find("## Needs you").unwrap() < week);
    assert!(week < md.find("## Calendar").unwrap());
    assert!(md.contains("`sand` **Win five pilots**"));
    assert!(md.contains("sequence differently: deck first"));
    assert!(md.contains("owner Alex (proposed), no date"));
    assert!(md.contains("## Stop doing\n\n- manual runs"));
}
