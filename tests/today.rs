//! What a reader sees on the Today view, rendered from report data: the three
//! actions first, no raw URL as text, every date and time in words.

use std::path::{Path, PathBuf};

use cos::agents::ledger::{self, Row};
use cos::agents::{Harness, Outcome, Status};
use cos::report::render;
use jiff::Timestamp;
use serde_json::{Value, json};

fn scratch(name: &str) -> (PathBuf, PathBuf) {
    let root = std::env::temp_dir().join(format!("cos-digest-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let ws = root.join("ws");
    std::fs::create_dir_all(ws.join(".chief-of-staff")).unwrap();
    std::fs::write(
        ws.join(".chief-of-staff/profile.json"),
        r#"{"timezone":"America/Chicago"}"#,
    )
    .unwrap();
    (root.join("home"), ws.canonicalize().unwrap())
}

/// A valid workspace state with no runs or incidents; `cos` reads only a
/// validated `state.json`.
fn demo_state() -> Value {
    serde_json::from_str(cos::state::DEMO_STATE).unwrap()
}

fn write(path: &Path, value: &Value) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, serde_json::to_vec(value).unwrap()).unwrap();
}

fn receipt(home: &Path, day: &str, at: &str, status: &str) {
    write(
        &home.join("out/runs").join(format!("{day}.json")),
        &json!({ "day": day, "at": at, "status": status, "attempts": 1, "cost_usd": 0.4 }),
    );
}

fn ts(s: &str) -> Timestamp {
    s.parse().unwrap()
}

#[test]
fn the_boundary_is_the_newest_ok_receipt_before_today() {
    let (home, ws) = scratch("boundary");
    receipt(
        &home,
        "2026-10-26",
        "2026-10-26T13:00:00.000000+00:00",
        "ok",
    );
    receipt(
        &home,
        "2026-10-27",
        "2026-10-27T13:00:00.000000+00:00",
        "ok",
    );
    receipt(
        &home,
        "2026-10-28",
        "2026-10-28T13:00:00.000000+00:00",
        "failed",
    );
    receipt(
        &home,
        "2026-10-29",
        "2026-10-29T13:00:00.000000+00:00",
        "ok",
    );
    assert_eq!(
        cos::digest::boundary(&home, &ws, ts("2026-10-29T14:30:00Z")),
        Some(ts("2026-10-27T13:00:00Z"))
    );
}

#[test]
fn the_boundary_day_is_in_the_profile_timezone() {
    let (home, ws) = scratch("tz");
    receipt(
        &home,
        "2026-10-27",
        "2026-10-27T13:00:00.000000+00:00",
        "ok",
    );
    receipt(
        &home,
        "2026-10-28",
        "2026-10-28T13:00:00.000000+00:00",
        "ok",
    );
    // 04:00Z on the 29th is still the 28th in Chicago, so the 28th is today.
    assert_eq!(
        cos::digest::boundary(&home, &ws, ts("2026-10-29T04:00:00Z")),
        Some(ts("2026-10-27T13:00:00Z"))
    );
}

#[test]
fn without_an_ok_receipt_the_last_complete_run_is_the_boundary() {
    let (home, ws) = scratch("fallback");
    receipt(
        &home,
        "2026-10-28",
        "2026-10-28T13:00:00.000000+00:00",
        "failed",
    );
    let mut st = demo_state();
    st["runs"] = json!([
        { "id": "r1", "status": "complete", "invokedAt": "2026-10-26T12:00:00+00:00",
          "finishedAt": "2026-10-26T12:30:00+00:00", "coverage": [] },
        { "id": "r2", "status": "partial", "invokedAt": "2026-10-27T12:00:00+00:00",
          "finishedAt": "2026-10-27T12:30:00+00:00", "coverage": [] }
    ]);
    write(&ws.join(".chief-of-staff/state.json"), &st);
    assert_eq!(
        cos::digest::boundary(&home, &ws, ts("2026-10-29T14:30:00Z")),
        Some(ts("2026-10-26T12:30:00Z"))
    );
}

#[test]
fn no_boundary_means_no_digest() {
    let (home, ws) = scratch("none");
    assert_eq!(
        cos::digest::boundary(&home, &ws, ts("2026-10-29T14:30:00Z")),
        None
    );
    assert!(cos::digest::build(&home, &ws, ts("2026-10-29T14:30:00Z"), None, &[]).is_none());
}

fn row(id: &str, judged: &str, outcome: Outcome, status: Status, title: &str) -> Row {
    Row {
        id: id.into(),
        judged: judged.into(),
        harness: Harness::Claude,
        model: None,
        repo: None,
        kind: "prompt".into(),
        status,
        outcome,
        overclaimed: false,
        checks: 1,
        cost_usd: None,
        secs: None,
        tool_calls: None,
        herdr: true,
        role: None,
        title: Some(title.into()),
    }
}

#[test]
fn the_lanes_hold_what_arrived_after_the_boundary() {
    let (home, ws) = scratch("lanes");
    receipt(
        &home,
        "2026-10-27",
        "2026-10-27T13:00:00.000000+00:00",
        "ok",
    );
    let rows = [
        row(
            "a",
            "2026-10-28T10:00:00Z",
            Outcome::NeedsYou,
            Status::Done,
            "Decide rotation",
        ),
        row(
            "b",
            "2026-10-27T10:00:00Z",
            Outcome::Rejected,
            Status::Failed,
            "Before the boundary",
        ),
        row(
            "c",
            "2026-10-28T11:00:00Z",
            Outcome::Rejected,
            Status::Done,
            "Check failed",
        ),
        row(
            "d",
            "2026-10-28T12:00:00Z",
            Outcome::Accepted,
            Status::Done,
            "Ship parser",
        ),
        row(
            "e",
            "2026-10-28T09:00:00Z",
            Outcome::Rejected,
            Status::Lost,
            "Vanished",
        ),
        row(
            "f",
            "not a time",
            Outcome::Accepted,
            Status::Done,
            "Unreadable",
        ),
    ];
    for r in &rows {
        ledger::append(&home, r).unwrap();
    }
    write(
        &home.join("out/history/merged.json"),
        &json!([
            { "repo": "org/api", "number": 1, "title": "old", "day": "2026-10-27", "merged_at": "2026-10-27T12:00:00Z" },
            { "repo": "org/api", "number": 2, "title": "shipped", "day": "2026-10-28", "merged_at": "2026-10-28T15:00:00Z" },
            { "repo": "org/api", "number": 3, "title": "no stamp, later day", "day": "2026-10-28" },
            { "repo": "org/api", "number": 4, "title": "no stamp, boundary day", "day": "2026-10-27" }
        ]),
    );
    let mut st = demo_state();
    st["incidents"] = json!([
        { "id": "I-1", "sourceId": "email", "kind": "rate_limit", "observedAt": "2026-10-28T08:00:00+00:00" },
        { "id": "I-0", "sourceId": "email", "kind": "rate_limit", "observedAt": "2026-10-27T08:00:00+00:00" }
    ]);
    write(&ws.join(".chief-of-staff/state.json"), &st);
    let d = cos::digest::build(&home, &ws, ts("2026-10-29T14:30:00Z"), None, &[]).unwrap();
    let lane = |k: &str| {
        d["lanes"][k]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["title"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>()
    };
    assert_eq!(lane("decisions"), ["Decide rotation"]);
    let mut errors = lane("errors");
    errors.sort();
    assert_eq!(errors, ["Check failed", "Vanished", "rate_limit on email"]);
    let mut done = lane("done");
    done.sort();
    assert_eq!(done, ["Ship parser", "no stamp, later day", "shipped"]);
}

#[test]
fn a_ledger_item_takes_its_spec_title_while_the_run_exists() {
    let (home, ws) = scratch("title");
    receipt(
        &home,
        "2026-10-27",
        "2026-10-27T13:00:00.000000+00:00",
        "ok",
    );
    ledger::append(
        &home,
        &row(
            "gone",
            "2026-10-28T10:00:00Z",
            Outcome::Accepted,
            Status::Done,
            "Row title",
        ),
    )
    .unwrap();
    let d = cos::digest::build(&home, &ws, ts("2026-10-29T14:30:00Z"), None, &[]).unwrap();
    assert_eq!(d["lanes"]["done"][0]["title"], "Row title");
}

#[test]
fn a_ledger_item_names_its_role_never_its_run_id() {
    let (home, ws) = scratch("no-id");
    receipt(
        &home,
        "2026-10-27",
        "2026-10-27T13:00:00.000000+00:00",
        "ok",
    );
    let mut staffed = row(
        "20261028-100000-a001",
        "2026-10-28T10:00:00Z",
        Outcome::Accepted,
        Status::Done,
        "Ship parser",
    );
    staffed.role = Some("builder".into());
    let mut untitled = row(
        "20261028-110000-a002",
        "2026-10-28T11:00:00Z",
        Outcome::Accepted,
        Status::Done,
        "",
    );
    untitled.title = None;
    ledger::append(&home, &staffed).unwrap();
    ledger::append(&home, &untitled).unwrap();
    let d = cos::digest::build(&home, &ws, ts("2026-10-29T14:30:00Z"), None, &[]).unwrap();
    let text = d.to_string();
    assert!(!text.contains("20261028-100000-a001"), "{text}");
    assert!(!text.contains("20261028-110000-a002"), "{text}");
    let done = d["lanes"]["done"].as_array().unwrap();
    assert!(
        done.iter().any(|i| i["source"] == "builder agent"),
        "{text}"
    );
    assert!(done.iter().any(|i| i["title"] == "An agent run"), "{text}");
}

#[test]
fn a_digest_item_shows_its_time_in_words_and_escapes_its_title() {
    let mut d = data(full_brief());
    d["digest"] = sample_digest();
    let m = markup(&d);
    let text = visible(&m);
    assert!(text.contains("Wed 28 Oct, 08:00"), "{text}");
    assert!(!text.contains("2026-10-28T"), "{text}");
    assert!(m.contains("&lt;img src=x onerror=1&gt;"), "{m}");
    assert!(!m.contains("<img src=x"), "{m}");
}

fn kpi_data(points: Vec<(&str, f64, &str)>) -> Value {
    let mut d = data(full_brief());
    d["workspace"]["kpis"] = json!([{
        "id": "K-2", "goalId": "G-1", "name": "<b>Signed</b> pilots", "unit": "agreements",
        "baseline": 0, "target": 5, "current": 3, "evidenceAt": "2026-11-20T09:00:00Z",
        "source": "email", "reportingWindow": "2030-04"
    }]);
    d["kpi_points"] = points
        .into_iter()
        .map(|(kpi, value, at)| json!({ "kpi": kpi, "value": value, "evidenceAt": at, "recordedAt": at }))
        .collect();
    d
}

#[test]
fn a_kpi_tile_with_two_points_shows_direction_and_a_sparkline() {
    let d = kpi_data(vec![
        ("<b>Signed</b> pilots", 1.0, "2026-11-13T09:00:00Z"),
        ("<b>Signed</b> pilots", 3.0, "2026-11-20T09:00:00Z"),
    ]);
    let m = markup(&d);
    assert!(m.contains("kit-tile__spark"), "no sparkline: {m}");
    assert!(
        m.contains("data-tone=\"ok\""),
        "a rise toward a higher target is favourable"
    );
    assert!(visible(&m).contains("+2"), "{}", visible(&m));
}

#[test]
fn a_kpi_tile_with_one_point_has_no_sparkline() {
    let d = kpi_data(vec![("<b>Signed</b> pilots", 3.0, "2026-11-20T09:00:00Z")]);
    let m = markup(&d);
    assert!(m.contains("kit-tile"), "{m}");
    assert!(!m.contains("kit-tile__spark"), "{m}");
}

#[test]
fn a_kpi_name_with_markup_renders_escaped() {
    let d = kpi_data(vec![("<b>Signed</b> pilots", 3.0, "2026-11-20T09:00:00Z")]);
    let m = markup(&d);
    assert!(m.contains("&lt;b&gt;Signed&lt;/b&gt; pilots"), "{m}");
    assert!(!m.contains("<b>Signed</b>"), "{m}");
}

fn sample_digest() -> Value {
    json!({
        "since": "2026-10-27T13:00:00Z",
        "lanes": {
            "decisions": [{ "title": "Rotation or SLA", "source": "jira:API-77", "at": null }],
            "errors": [{ "title": "<img src=x onerror=1>", "source": "", "at": "2026-10-28T13:00:00Z" }],
            "done": [{ "title": "Merged 7", "source": "gh:northwind/api-gateway#7", "at": "2026-10-28T15:00:00Z" }]
        }
    })
}

#[test]
fn the_digest_sits_after_the_actions_with_a_brief() {
    let mut d = data(full_brief());
    d["digest"] = sample_digest();
    let m = markup(&d);
    let actions = m.find("data-section=\"actions\"").unwrap();
    let digest = m.find("data-section=\"digest\"").expect("a digest section");
    assert!(actions < digest, "the digest sits before the actions");
    assert!(digest < m.find("data-section=\"week\"").unwrap());
}

#[test]
fn without_a_brief_the_digest_sits_after_herdr_waiting() {
    let d = json!({
        "now_ts": "2026-10-29T14:30:00Z",
        "today": "2026-10-29",
        "herdr_waiting": [{ "id": "h1", "title": "Hire", "kind": "blocked", "terminal": "cos-h1", "address": "1" }],
        "digest": sample_digest()
    });
    let m = markup(&d);
    let herdr = m.find("data-section=\"herdr\"").unwrap();
    let digest = m.find("data-section=\"digest\"").unwrap();
    assert!(
        herdr < digest && digest < m.find("class=\"celebrate\"").unwrap(),
        "{m}"
    );
}

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
