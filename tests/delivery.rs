//! The delivery board on the Today view: Jira tickets beside their PRs, the
//! code and the dev cluster, collapsed after the actions, with no raw URL as
//! text and nothing a row says able to inject markup.

use cos::delivery::{Action, parse};
use cos::report::render;
use serde_json::{Value, json};

fn board() -> Value {
    json!({
        "version": 1,
        "as_of": "Mon 16 Nov, 12:40",
        "sources": ["Jira project API", "gh northwind/api-gateway", "kubectl api-gateway"],
        "rows": [
            { "key": "API-71", "summary": "Gateway returns one status", "url": "https://northwind.example/browse/API-71",
              "jira_status": "In Review",
              "prs": [ { "number": 44, "url": "https://github.com/northwind/vm/pull/44", "state": "merged" } ],
              "code": "merged to develop 13 Nov", "cluster": "a pre-merge build of #44",
              "action": "update_jira", "recommendation": "Move to Awaiting deployment." },
            { "key": "API-29", "summary": "An empty upload is a result", "url": "https://northwind.example/browse/API-29",
              "jira_status": "To Do", "prs": [], "code": "no PR", "cluster": "not deployed",
              "action": "work", "recommendation": "Start from the follow-up design." }
        ]
    })
}

fn data(delivery: Value) -> Value {
    json!({
        "now_ts": "2026-11-16T12:40:00Z",
        "today": "2026-11-16",
        "brief": { "day": "2026-11-16", "report": {
            "headline": "One review left.",
            "needs_you": [ { "verb": "do", "owner": "Alex", "recommendation": "Review #76", "why": "x", "next_step": "",
              "source": "", "deadline": "", "deadline_basis": "proposed", "goal_id": "", "deliverable_id": "", "agent_prompt": "" } ],
            "what_changed": [], "week": { "summary": "", "goals": [], "proposed": [], "decisions": [], "stop_doing": [] },
            "calendar": [], "backlog": [], "coverage": [], "gaps": [], "upcoming": [], "tickets": []
        } },
        "workspace": { "timezone": "Europe/London", "goals": [] },
        "history": { "days": [], "merged": [] },
        "delivery": delivery,
    })
}

fn markup(d: &Value) -> String {
    let html = render(d).unwrap();
    let body = html.find("<body>").unwrap();
    let script = html.find("<script>").unwrap();
    html[body..script].to_owned()
}

fn visible(m: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in m.chars() {
        match c {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                out.push(' ');
            }
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

#[test]
fn the_board_follows_the_actions_collapsed() {
    let m = markup(&data(board()));
    let actions = m.find(r#"data-section="actions""#).unwrap();
    let board = m.find(r#"data-section="delivery""#).unwrap();
    assert!(actions < board);
    assert!(m[..board].ends_with(r#"<details class="fold" id="delivery" "#));
    assert_eq!(m.matches(r#"<article class="dl" data-action="#).count(), 2);
    let text = visible(&m);
    assert!(text.contains("2 tickets · read Mon 16 Nov, 12:40 · 1 update Jira · 1 work it"));
}

/// A card per ticket, grouped by its next action in the order of the
/// filters: no table, the title in the page's sans at full width, the next
/// action as a chip, PRs as chips with their state, and every link opening
/// out of the app.
#[test]
fn each_ticket_is_a_card_grouped_by_its_next_action() {
    let m = markup(&data(board()));
    let at = m.find(r#"data-section="delivery""#).unwrap();
    let section = &m[at..];
    let section = &section[..section.find("</details>").unwrap()];
    assert!(!section.contains("<table"), "no table");
    assert!(!section.contains(r#"class="num""#), "nothing set in mono");
    assert!(section.contains(r#"<h4 class="dl-title">Gateway returns one status</h4>"#));
    let jira = section
        .find(r#"<article class="dl" data-action="update_jira""#)
        .unwrap();
    let work = section
        .find(r#"<article class="dl" data-action="work""#)
        .unwrap();
    assert!(jira < work, "update Jira before work it");
    let filters = section.find(r#"data-show="update_jira""#).unwrap();
    assert!(
        filters < section.find(r#"data-show="work""#).unwrap(),
        "the filters in the same order"
    );
    assert!(section.contains(r#"<span class="dl-next" data-action="update_jira">"#));
    assert!(section.contains(r#"class="src-chip dl-pr" data-state="merged""#));
    for a in section.split("<a ").skip(1) {
        assert!(
            a.contains(r#"target="_blank""#),
            "an external link opens out: <a {a}"
        );
    }
}

#[test]
fn keys_and_prs_are_links_never_raw_urls() {
    let m = markup(&data(board()));
    assert!(m.contains(r#"href="https://northwind.example/browse/API-71""#));
    assert!(m.contains(r#"href="https://github.com/northwind/vm/pull/44""#));
    let text = visible(&m);
    assert!(text.contains("API-71") && text.contains("#44"));
    assert!(!text.contains("https://"), "{text}");
}

#[test]
fn no_board_no_section_and_a_broken_one_says_why() {
    assert!(!markup(&data(Value::Null)).contains(r#"data-section="delivery""#));
    let m = markup(&data(
        json!({ "error": "context/delivery.json: `version` is 2" }),
    ));
    assert!(visible(&m).contains("The delivery board could not be read"));
}

#[test]
fn row_text_cannot_inject_markup() {
    let mut b = board();
    b["rows"][0]["summary"] = json!("<script>x</script>");
    b["rows"][0]["recommendation"] = json!("<img src=x onerror=y>");
    let m = markup(&data(b));
    assert!(!m.contains("<script>x") && !m.contains("<img src=x"));
}

#[test]
fn the_file_is_strict_and_versioned() {
    let ok = parse(&board().to_string()).unwrap();
    assert_eq!(ok.rows[1].action, Action::Work);
    let mut extra = board();
    extra["rows"][0]["owner"] = json!("Alex");
    assert!(parse(&extra.to_string()).is_err());
    let mut v2 = board();
    v2["version"] = json!(2);
    assert!(
        parse(&v2.to_string())
            .unwrap_err()
            .to_string()
            .contains("version")
    );
}
