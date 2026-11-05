//! What a reader sees on the Today view, rendered from report data: the three
//! actions first, no raw URL as text, every date and time in words.

use cos::report::render;
use serde_json::{Value, json};

fn data(brief: Value) -> Value {
    json!({
        "now_ts": "2026-10-29T14:30:00Z",
        "today": "2026-10-29",
        "name": "Alex Rivera",
        "brief": { "day": "2026-10-29", "report": brief },
        "run": { "at": "2026-10-29T13:30:00Z", "cost_usd": 0.42 },
        "workspace": {
            "timezone": "America/Chicago",
            "goals": [{ "id": "G-1", "title": "Mobile 2.0 on the new gateway", "docs": ["api-gateway"] }]
        },
        "history": { "days": [], "merged": [
            { "repo": "northwind/api-gateway", "number": 7, "title": "x", "day": "2026-10-27" }
        ] },
        "work_orgs": ["northwind"],
    })
}

fn full_brief() -> Value {
    json!({
        "headline": "Release 3.4 waits on your review: https://github.com/northwind/api-gateway/pull/412",
        "needs_you": [
            { "verb": "do", "owner": "Alex", "recommendation": "Review api-gateway#412", "why": "The release waits on it; see https://example.test/ci/1.",
              "next_step": "Read the auth diff.", "source": "https://github.com/northwind/api-gateway/pull/412",
              "deadline": "2026-11-01", "deadline_basis": "confirmed", "goal_id": "G-1", "deliverable_id": "D-1", "agent_prompt": "Review it. Stop before posting." },
            { "verb": "decide", "owner": "Alex", "recommendation": "Rotation or SLA", "why": "41h wait.", "next_step": "",
              "source": "jira:API-77", "deadline": "2026-11-02", "deadline_basis": "proposed", "goal_id": "", "deliverable_id": "", "agent_prompt": "" }
        ],
        "what_changed": [ { "kind": "done", "what": "Merged.", "source": "https://github.com/northwind/api-gateway/pull/411" } ],
        "week": {
            "summary": "One review gates the beta.",
            "goals": [ { "goal_id": "G-1", "goal": "Mobile 2.0", "confidence": "rock", "kpi": "unknown", "evidence": "3.4 in review",
              "bottleneck": "One reviewer", "recommendation": "maintain", "next": "Ship 3.4.", "would_change": "A blocking finding", "agent_prompt": "" } ],
            "proposed": [], "decisions": [ { "decision": "Rotation or SLA?", "options": ["Rotation", "SLA"], "recommendation": "Rotation." } ],
            "stop_doing": ["Preview-env cost work."]
        },
        "calendar": [ { "date": "2026-10-29", "start": "16:00", "end": "17:00", "what": "Go/no-go", "prep": "Have #412 reviewed." } ],
        "backlog": [ { "item": "Reply to Sam", "owner": "Alex", "source": "mail: Sam, 'Window?'", "next_review": "Wednesday" } ],
        "coverage": [ { "source": "email", "health": "healthy", "review": "partial",
            "window_start": "2026-10-27T00:00:00+00:00", "window_end": "2026-10-29T08:00:00+00:00", "note": "Budget reached." } ],
        "gaps": [],
        "upcoming": [ { "date": "2026-11-01", "what": "Release 3.4", "source": "calendar" } ],
        "tickets": [ { "key": "API-77", "summary": "Smoke test", "status": "To Do", "priority": "High", "url": "https://northwind.example/browse/API-77", "goal_id": "G-1" } ]
    })
}

/// The markup before the first script: the hero and the Brief panel.
fn markup(d: &Value) -> String {
    let html = render(d).unwrap();
    let body = html.find("<body>").unwrap();
    let script = html.find("<script>").unwrap();
    html[body..script].to_owned()
}

/// Text a reader sees: tags, attributes and the copyable handoff blocks
/// (`<pre>`, passed to an agent verbatim) removed.
fn visible(markup: &str) -> String {
    let mut s = markup.to_owned();
    while let Some(a) = s.find("<pre") {
        let b = s[a..].find("</pre>").map_or(s.len(), |e| a + e + 6);
        s.replace_range(a..b, " ");
    }
    let mut out = String::new();
    let mut in_tag = false;
    for c in s.chars() {
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
fn the_actions_come_before_every_other_section() {
    let m = markup(&data(full_brief()));
    let sections: Vec<&str> = m
        .match_indices("data-section=\"")
        .map(|(i, _)| {
            let rest = &m[i + 14..];
            &rest[..rest.find('"').unwrap()]
        })
        .collect();
    assert_eq!(sections.first(), Some(&"actions"), "sections: {sections:?}");
    assert!(sections.contains(&"week") && sections.contains(&"ribbon"));
    let cards = m.matches("<article class=\"act ").count();
    assert_eq!(cards, 2);
    let first_card = m.find("<article class=\"act ").unwrap();
    for other in [
        "data-section=\"week\"",
        "data-section=\"ribbon\"",
        "data-section=\"coverage\"",
    ] {
        assert!(
            first_card < m.find(other).unwrap(),
            "{other} before the actions"
        );
    }
}

#[test]
fn no_raw_url_is_shown_as_text() {
    let m = markup(&data(full_brief()));
    let text = visible(&m);
    assert!(
        !text.contains("http://") && !text.contains("https://"),
        "{text}"
    );
    assert!(m.contains(r#"href="https://github.com/northwind/api-gateway/pull/412""#));
    assert!(text.contains("api-gateway#412"));
    assert!(text.contains("API-77"));
}

#[test]
fn dates_and_times_read_as_words() {
    let m = markup(&data(full_brief()));
    let text = visible(&m);
    let b = text.as_bytes();
    for i in 0..b.len().saturating_sub(9) {
        let iso = b[i..i + 10].iter().enumerate().all(|(j, c)| {
            if j == 4 || j == 7 {
                *c == b'-'
            } else {
                c.is_ascii_digit()
            }
        });
        assert!(
            !iso,
            "raw date in {:?}",
            &text[i.saturating_sub(30)..(i + 20).min(text.len())]
        );
    }
    assert!(!text.contains("T00:00") && !text.contains("+00:00"));
    // 13:30Z is 08:30 in Chicago, which the hero shows once with its zone.
    assert!(text.contains("08:30") && text.contains("CDT"), "{text}");
    assert!(text.contains("due Sun 1 Nov"), "{text}");
    assert!(text.contains("Thursday 29 October"));
    assert!(
        text.contains("Tue 27 Oct, 00:00") || text.contains("Mon 26 Oct, 19:00"),
        "{text}"
    );
}

#[test]
fn an_old_brief_renders_without_empty_cards() {
    let old = json!({
        "headline": "Saved before the week part.",
        "needs_you": [],
        "coverage": []
    });
    let m = markup(&data(old));
    assert!(m.contains("data-ssr"));
    for absent in [
        "data-section=\"week\"",
        "data-tickets",
        "<article class=\"act ",
    ] {
        assert!(!m.contains(absent), "{absent} rendered for an old brief");
    }
    assert!(m.contains("Nothing needs you today."));
    assert!(m.contains("class=\"quiet\" data-section=\"ribbon\""));
}

#[test]
fn without_a_brief_the_page_keeps_its_saved_views() {
    let m = markup(&json!({ "now_ts": "2026-10-29T14:30:00Z", "today": "2026-10-29" }));
    assert!(m.contains("id=\"left-body\"") && !m.contains("data-ssr"));
    assert!(m.contains("No brief yet."));
}

fn failed_today(mut d: Value, status: &str, reason: &str) -> Value {
    d["last_run"] = json!({
        "day": "2026-10-29", "at": "2026-10-29T13:30:00Z", "status": status,
        "reason": reason, "attempts": 2, "cost_usd": 0.5
    });
    d
}

#[test]
fn a_failed_run_says_so_and_keeps_the_last_brief_below() {
    let mut d = failed_today(
        data(full_brief()),
        "failed",
        "API Error: Your computer went to sleep mid-response.",
    );
    d["brief"]["day"] = json!("2026-10-28");
    let m = markup(&d);
    let text = visible(&m);
    assert!(
        m.contains(r#"class="run-alert" role="status" data-run="failed""#),
        "{m}"
    );
    assert!(text.contains("Today's brief didn't run:"), "{text}");
    assert!(
        text.contains("went to sleep mid-response. (tried twice)"),
        "{text}"
    );
    assert!(text.contains("last brief Wed 28 Oct"), "{text}");
    let alert = m.find("run-alert").unwrap();
    let headline = m.find("id=\"headline\"").unwrap();
    assert!(alert < headline, "the alert sits above the old headline");
    assert!(text.contains("Release 3.4 waits on your review"));
    assert!(
        m.contains("<article class=\"act "),
        "the old brief's actions are gone"
    );
}

#[test]
fn a_deferred_run_without_any_brief_says_none_is_saved() {
    let d = failed_today(
        json!({ "now_ts": "2026-10-29T14:30:00Z", "today": "2026-10-29" }),
        "deferred",
        "Mac asleep (dark wake): brief deferred",
    );
    let text = visible(&markup(&d));
    assert!(
        text.contains("Mac asleep (dark wake): brief deferred"),
        "{text}"
    );
    assert!(text.contains("no brief saved yet"), "{text}");
    assert!(text.contains("No brief yet."));
}

#[test]
fn no_alert_once_today_has_a_brief_or_the_run_is_old() {
    let today = failed_today(data(full_brief()), "failed", "Request timed out");
    assert!(!markup(&today).contains("run-alert"));
    let mut old = failed_today(data(full_brief()), "failed", "Request timed out");
    old["brief"]["day"] = json!("2026-10-27");
    old["last_run"]["day"] = json!("2026-10-28");
    assert!(!markup(&old).contains("run-alert"));
    let mut ok = failed_today(data(full_brief()), "ok", "");
    ok["brief"]["day"] = json!("2026-10-28");
    assert!(!markup(&ok).contains("run-alert"));
}

#[test]
fn view_text_cannot_inject_markup() {
    let mut b = full_brief();
    b["needs_you"][0]["recommendation"] = json!("<img src=x onerror=alert(1)>");
    b["headline"] = json!("<!--__COS_TODAY__--> /*__MARKDY__*/ \"__COS_DATA__\"");
    let d = data(b);
    let html = render(&d).unwrap();
    let m = markup(&d);
    assert!(!m.contains("<img src=x"));
    assert_eq!(html.matches("const DATA = ").count(), 1);
    assert!(m.contains("&lt;!--__COS_TODAY__--&gt;"));
    assert!(
        m.contains("/*__MARKDY__*/"),
        "the bundle replaced text in the markup"
    );
}
