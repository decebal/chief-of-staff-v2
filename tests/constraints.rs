//! The limiting factors on the Today view: collapsed after the actions, the
//! factors still limiting first, a count that reads without opening it, and
//! a strict, versioned file. The brief reads the open factors and may only
//! propose changes.

use cos::constraints::{Status, parse, prompt_block};
use cos::report::render;
use serde_json::{Value, json};

fn register() -> Value {
    json!({
        "version": 1,
        "as_of": "Mon 16 Nov",
        "question": "Why are we not released to all?",
        "factors": [
            { "id": "LF-3", "title": "Review is two people", "area": "review", "status": "watch",
              "evidence": "Every PR since 1 Nov reviewed by one of two people.", "recommendation": "A self-check gate before review.",
              "owner": "Alex", "confidence": "sand", "tickets": [] },
            { "id": "LF-1", "title": "Production is paused", "area": "release", "status": "limiting",
              "evidence": "The flag is unset; see https://github.com/northwind/vm/issues/23.", "recommendation": "Run the un-pause checklist.",
              "owner": "Alex", "confidence": "rock", "tickets": ["API-32"] },
            { "id": "LF-9", "title": "Old blocker", "area": "process", "status": "cleared",
              "evidence": "Gone.", "recommendation": "Nothing.", "owner": "-", "confidence": "rock" }
        ]
    })
}

fn data(constraints: Value) -> Value {
    json!({
        "now_ts": "2026-11-16T12:40:00Z",
        "today": "2026-11-16",
        "brief": { "day": "2026-11-16", "report": {
            "headline": "x",
            "needs_you": [ { "verb": "do", "owner": "Alex", "recommendation": "x", "why": "x", "next_step": "",
              "source": "", "deadline": "", "deadline_basis": "proposed", "goal_id": "", "deliverable_id": "", "agent_prompt": "" } ],
            "what_changed": [], "week": { "summary": "", "goals": [], "proposed": [], "decisions": [], "stop_doing": [] },
            "calendar": [], "backlog": [], "coverage": [], "gaps": [], "upcoming": [], "tickets": []
        } },
        "workspace": { "timezone": "Europe/London", "goals": [] },
        "history": { "days": [], "merged": [] },
        "constraints": constraints,
    })
}

fn markup(d: &Value) -> String {
    let html = render(d).unwrap();
    let body = html.find("<body>").unwrap();
    let script = html.find("<script>").unwrap();
    html[body..script].to_owned()
}

#[test]
fn limiting_factors_follow_the_actions_limiting_first() {
    let m = markup(&data(register()));
    let actions = m.find(r#"data-section="actions""#).unwrap();
    let section = m.find(r#"data-section="constraints""#).unwrap();
    assert!(actions < section);
    for count in [
        "1 limiting</span>",
        "1 to watch</span>",
        "judged Mon 16 Nov</span>",
    ] {
        assert!(m.contains(count), "{count}");
    }
    let order: Vec<usize> = [
        "data-status=\"limiting\"",
        "data-status=\"watch\"",
        "data-status=\"cleared\"",
    ]
    .iter()
    .map(|s| m.find(s).unwrap())
    .collect();
    assert!(order[0] < order[1] && order[1] < order[2]);
    assert!(m.contains(r#"href="https://github.com/northwind/vm/issues/23""#));
}

/// A card per factor, readable at any width: no table, the title in the
/// page's sans at full width (never a mono cell), the move that loosens it
/// before the evidence, which is folded; tickets as chips; links open out.
#[test]
fn each_factor_is_a_card_with_its_evidence_folded() {
    let m = markup(&data(register()));
    let at = m.find(r#"data-section="constraints""#).unwrap();
    let rest = &m[at + 1..];
    let section = &m[at..at + 1 + rest.find("data-section=").unwrap_or(rest.len())];
    assert!(!section.contains("<table"), "no table");
    assert_eq!(section.matches(r#"<article class="lf""#).count(), 3);
    assert!(section.contains(r#"<h4 class="lf-title">Production is paused</h4>"#));
    assert!(!section.contains(r#"class="num""#), "nothing set in mono");
    let card = &section[section
        .find(r#"<article class="lf" data-status="limiting""#)
        .unwrap()..];
    let card = &card[..card.find("</article>").unwrap()];
    let loosen = card.find(r#"class="lf-loosen""#).unwrap();
    let evidence = card
        .find(r#"<details class="lf-ev"><summary>"#)
        .expect("evidence folded");
    assert!(loosen < evidence, "the move first, the evidence behind it");
    assert!(card[evidence..].contains("The flag is unset"));
    assert!(
        card.contains(r#"<span class="src-chip" title="API-32">"#),
        "a ticket chip"
    );
    assert!(
        card.contains("<svg class=\"ico\""),
        "icons from the window's set"
    );
    assert!(
        section.contains(r#"<details class="lf-more"><summary>"#),
        "to watch folds under its count"
    );
    for a in section.split("<a ").skip(1) {
        assert!(
            a.contains(r#"target="_blank""#),
            "an external link opens out: <a {a}"
        );
    }
}

#[test]
fn no_register_no_section_and_a_broken_one_says_why() {
    assert!(!markup(&data(Value::Null)).contains(r#"data-section="constraints""#));
    let m = markup(&data(json!({ "error": "bad" })));
    assert!(m.contains("The limiting factors could not be read"));
}

#[test]
fn factor_text_cannot_inject_markup() {
    let mut r = register();
    r["factors"][0]["title"] = json!("<script>x</script>");
    assert!(!markup(&data(r)).contains("<script>x"));
}

#[test]
fn the_file_is_strict_and_versioned() {
    assert_eq!(
        parse(&register().to_string()).unwrap().factors[1].status,
        Status::Limiting
    );
    let mut extra = register();
    extra["factors"][0]["eta"] = json!("Friday");
    assert!(parse(&extra.to_string()).is_err());
    let mut v2 = register();
    v2["version"] = json!(2);
    assert!(parse(&v2.to_string()).is_err());
}

fn home_with(name: &str, file: Option<&str>) -> std::path::PathBuf {
    let home = std::env::temp_dir().join(format!("cos-constraints-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(home.join("context")).unwrap();
    if let Some(text) = file {
        std::fs::write(home.join("context/constraints.json"), text).unwrap();
    }
    home
}

#[test]
fn the_week_reads_open_factors_and_may_only_propose_changes() {
    let home = home_with("prompt", Some(&register().to_string()));
    let block = prompt_block(&home);
    assert!(block.contains("LF-1 [limiting") && block.contains("LF-3 [watch"));
    assert!(!block.contains("LF-9"), "cleared factors stay out: {block}");
    assert!(block.contains("Tickets: API-32."));
    assert!(block.contains("`week.decisions`") && block.contains("Do not rewrite the file"));

    assert_eq!(prompt_block(&home_with("none", None)), "");
    let broken = prompt_block(&home_with("broken", Some("{")));
    assert!(broken.contains("could not be read") && broken.contains("gaps"));
}
