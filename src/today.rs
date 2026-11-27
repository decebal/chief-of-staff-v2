//! The Today view, rendered here rather than in the page's script so it reads
//! before any script runs and so `tests/today.rs` can check what a reader
//! sees: the three actions first, no raw URL as text, every date in words.
//! Built from the same `report::data` value the page embeds.

use std::fmt::Write as _;

use jiff::civil::Date;
use jiff::tz::TimeZone;
use jiff::{Timestamp, ZonedDateTime};
use serde_json::Value;

use crate::reports::{Basis, Brief, ChangeKind, Completeness, Confidence, Health, Steer, Verb};

/// `<`, `>`, `&` and both quotes, so text is safe in content and attributes.
pub fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

/// What the view needs from `report::data`, read once.
pub(crate) struct Ctx<'a> {
    data: &'a Value,
    tz: TimeZone,
    now: ZonedDateTime,
    brief: Option<Brief>,
    day: String,
}

impl<'a> Ctx<'a> {
    pub(crate) fn new(data: &'a Value) -> Self {
        let tz = data["workspace"]["timezone"]
            .as_str()
            .and_then(|n| TimeZone::get(n).ok())
            .unwrap_or_else(TimeZone::system);
        let now = data["now_ts"]
            .as_str()
            .and_then(|t| t.parse::<Timestamp>().ok())
            .unwrap_or_else(Timestamp::now)
            .to_zoned_datetime(tz.clone());
        let brief = data["brief"]["report"]
            .is_object()
            .then(|| serde_json::from_value::<Brief>(data["brief"]["report"].clone()).ok())
            .flatten();
        let day = data["brief"]["day"].as_str().unwrap_or_default().to_owned();
        Ctx {
            data,
            tz,
            now,
            brief,
            day,
        }
    }

    fn today(&self) -> Date {
        self.now.date()
    }

    /// `Thu 29 Oct`; "today" and "tomorrow" where they read better.
    fn date(&self, iso: &str) -> String {
        match iso.get(..10).and_then(|d| d.parse::<Date>().ok()) {
            Some(d) if d == self.today() => "today".into(),
            Some(d) if Some(d) == self.today().tomorrow().ok() => "tomorrow".into(),
            Some(d) => day_name(d),
            None => iso.to_owned(),
        }
    }

    /// `Thu 29 Oct, 16:00` in the profile's timezone; the time alone today.
    pub(crate) fn stamp(&self, iso: &str) -> String {
        let Some(z) = parse_instant(iso).map(|t| t.to_zoned_datetime(self.tz.clone())) else {
            return self.date(iso);
        };
        if z.date() == self.today() {
            format!("today, {}", z.strftime("%H:%M"))
        } else {
            format!("{}, {}", day_name(z.date()), z.strftime("%H:%M"))
        }
    }

    fn goal_title(&self, id: &str) -> Option<String> {
        let ws = self.data["workspace"]["goals"].as_array();
        ws.into_iter()
            .flatten()
            .find(|g| g["id"] == id)
            .and_then(|g| g["title"].as_str().map(str::to_owned))
            .or_else(|| {
                self.brief.as_ref().and_then(|b| {
                    b.week
                        .goals
                        .iter()
                        .find(|g| g.goal_id == id)
                        .map(|g| g.goal.clone())
                })
            })
    }
}

fn day_name(d: Date) -> String {
    d.strftime("%a %-d %b").to_string()
}

fn parse_instant(s: &str) -> Option<Timestamp> {
    s.parse::<Timestamp>()
        .ok()
        .or_else(|| s.parse::<ZonedDateTime>().ok().map(|z| z.timestamp()))
}

/// The ink band: greeting, the brief's headline as display type, the run.
pub fn hero(data: &Value) -> String {
    let cx = Ctx::new(data);
    let first = data["name"]
        .as_str()
        .and_then(|n| n.split_whitespace().next())
        .map(str::to_owned);
    let hour = cx.now.hour();
    let part = if hour < 12 {
        "Good morning"
    } else if hour < 18 {
        "Good afternoon"
    } else {
        "Good evening"
    };
    let greet = match first {
        Some(n) => format!("{part}, {}", esc(&n)),
        None => part.to_owned(),
    };
    let date = cx.today().strftime("%A %-d %B").to_string();
    let mut s = format!(r#"<p class="greet">{greet}<span class="greet-date">{date}</span></p>"#);
    if let Some(alert) = missed(&cx) {
        s.push_str(&alert);
    }
    match &cx.brief {
        Some(b) => {
            s.push_str(&headline(if b.headline.is_empty() {
                "The brief has no headline."
            } else {
                &b.headline
            }));
            let _ = write!(s, r#"<p class="run-pill" id="run-pill">{}</p>"#, run_pill(&cx, b));
        }
        None => s.push_str(
            r#"<h1 class="display" id="headline">No brief yet.</h1><p class="hero-lead">Run <code>cos brief</code>: your goals, this week, and at most three things that need you today.</p>"#,
        ),
    }
    s.push_str(LOADER);
    s.push_str(&pills(&cx));
    s
}

/// The hero's lead phrase, in words: up to the first `;`, `:` or `,`, at most eight.
pub fn lead_words(text: &str) -> usize {
    let mut n = 0;
    for w in text.split_whitespace() {
        n += 1;
        if n == 8 || w.ends_with([';', ':', ',']) {
            break;
        }
    }
    n
}

/// The brief's headline: named in full on the `h1`, each word an
/// `aria-hidden` span the reveal staggers. A link stays outside the spans so
/// it is never focusable inside hidden content.
fn headline(text: &str) -> String {
    let lead = lead_words(text);
    let mut s = format!(
        r#"<h1 class="display" id="headline" aria-label="{}">"#,
        esc(text)
    );
    for (i, w) in text.split_whitespace().enumerate() {
        if i > 0 {
            s.push(' ');
        }
        if url_of(w.trim_end_matches([',', ')', '.', ';', ':'])).is_some() {
            s.push_str(&prose(w));
            continue;
        }
        let _ = write!(
            s,
            r#"<span class="{}" aria-hidden="true" style="--i: {i}">{}</span>"#,
            if i < lead { "w lead-phrase" } else { "w" },
            esc(w)
        );
    }
    s.push_str("</h1>");
    s
}

/// `ms` as a reader says it: `40 s`, `12 min`, `1 h 5 min`.
pub(crate) fn span_words(ms: u64) -> String {
    let s = ms / 1000;
    if s < 90 {
        format!("{s} s")
    } else if s < 3600 {
        format!("{} min", s / 60)
    } else if s % 3600 / 60 == 0 {
        format!("{} h", s / 3600)
    } else {
        format!("{} h {} min", s / 3600, s % 3600 / 60)
    }
}

/// What a hire is doing, in words: its phase, then Herdr's word.
fn hire_word(h: &Value) -> &'static str {
    match (h["phase"].as_str(), h["word"].as_str()) {
        (Some("paused"), _) => "paused",
        (Some("needs-you"), _) => "waiting on you",
        (Some("starting"), _) => "starting",
        (Some("done"), _) => "done",
        (Some("failed"), _) => "failed",
        (Some("lost"), _) => "lost",
        (Some("timed-out"), _) => "ran out of time",
        (Some("stopped"), _) => "stopped",
        (_, Some("idle")) => "idle",
        _ => "working",
    }
}

/// A hire's role mark: the role's initial on its identity colour.
fn hire_mark(h: &Value, class: &str) -> String {
    let initial = h["role"]
        .as_str()
        .and_then(|r| r.chars().next())
        .map_or("·".to_owned(), |c| c.to_uppercase().to_string());
    format!(
        r#"<span class="{class} kit-id-{}" aria-hidden="true">{}</span>"#,
        h["identity"].as_u64().unwrap_or(1).clamp(1, 6),
        esc(&initial)
    )
}

/// One pill per hire at work (working, needs you or paused): its title and
/// role mark, Herdr's word and the active time, and a bar of the active time
/// against its deadline.
fn pills(cx: &Ctx) -> String {
    let hires: Vec<&Value> = cx.data["hires"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|h| {
            matches!(
                h["phase"].as_str(),
                Some("working" | "needs-you" | "paused")
            )
        })
        .collect();
    let mut s = String::from(r#"<div class="hero-hires" id="hero-hires">"#);
    if !hires.is_empty() {
        s.push_str(r#"<ul class="hero-pills" aria-label="Hires at work">"#);
    }
    for h in &hires {
        let phase = h["phase"].as_str().unwrap_or_default();
        let active = h["active_ms"].as_u64().unwrap_or(0);
        let deadline = h["deadline_secs"].as_u64().unwrap_or(0) * 1000;
        let bar = if deadline == 0 {
            String::new()
        } else {
            let pct = (active as f64 * 100.0 / deadline as f64).clamp(0.0, 100.0);
            format!(
                r#"<span class="hero-pill__bar{}" role="img" aria-label="{pct:.0}% of its deadline"><i style="width: {pct:.0}%"></i></span>"#,
                if phase == "working" {
                    " kit-is-running"
                } else {
                    ""
                }
            )
        };
        let role = h["role"]
            .as_str()
            .map(|r| format!("{} · ", esc(r)))
            .unwrap_or_default();
        let _ = write!(
            s,
            r#"<li class="hero-pill{}" data-phase="{}" data-glide="{}">{}<span class="hero-pill__main"><span class="hero-pill__title">{}</span><span class="hero-pill__word" aria-live="polite">{role}{} · {}</span>{bar}</span></li>"#,
            if phase == "needs-you" {
                " kit-is-waiting"
            } else {
                ""
            },
            esc(phase),
            esc(h["id"].as_str().unwrap_or_default()),
            hire_mark(h, "hero-pill__mark"),
            esc(h["title"].as_str().unwrap_or_default()),
            hire_word(h),
            span_words(active),
        );
    }
    if !hires.is_empty() {
        s.push_str("</ul>");
    }
    s.push_str(r#"<p class="hero-hires__stale" data-stale hidden>as last read</p></div>"#);
    s
}

/// The inbox column a hire's card sits in; `None` keeps it off Today.
fn column(h: &Value) -> Option<&'static str> {
    match h["phase"].as_str()? {
        "needs-you" => Some("needs-you"),
        "working" | "starting" | "paused" => Some("running"),
        "done" if h["done_today"] == true => Some("done"),
        _ => None,
    }
}

const COLUMNS: [(&str, &str); 3] = [
    ("needs-you", "Needs you"),
    ("running", "Running"),
    ("done", "Done today"),
];

/// The hires inbox: Needs you, Running and Done today, one list with filter
/// buttons on a narrow screen. Open in Herdr keeps its message; the run id
/// travels only in it. Without a hire in any column, an empty slot the live
/// update can fill.
fn hires(cx: &Ctx) -> String {
    let all: Vec<(&Value, &str)> = cx.data["hires"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|h| Some((h, column(h)?)))
        .collect();
    if all.is_empty() {
        return r#"<div id="hires-slot" hidden></div>"#.into();
    }
    let mut s = format!(
        r#"<section class="inbox" id="hires" data-section="hires" aria-labelledby="hires-h"><div class="sec-head"><h2 id="hires-h">Hires <span class="count num">{}</span></h2><span class="inbox-stale" data-stale id="hires-stale" hidden>as last read</span></div><div class="inbox-filter" role="group" aria-label="Show hires"><button type="button" class="inbox-tab" data-show="all" aria-pressed="true">All</button>"#,
        all.len()
    );
    for (key, label) in COLUMNS {
        let _ = write!(
            s,
            r#"<button type="button" class="inbox-tab" data-show="{key}" aria-pressed="false">{label}</button>"#
        );
    }
    s.push_str(r#"</div><div class="inbox-cols" data-show="all">"#);
    for (key, label) in COLUMNS {
        let cards: Vec<&Value> = all
            .iter()
            .filter(|(_, c)| *c == key)
            .map(|(h, _)| *h)
            .collect();
        let _ = write!(
            s,
            r#"<section class="inbox-col" data-col="{key}" aria-labelledby="hires-{key}-h"><h3 class="inbox-col__h" id="hires-{key}-h">{label} <span class="num">{}</span>{}</h3><ul class="inbox-list">"#,
            cards.len(),
            if key == "running" {
                r#"<span class="inbox-stale" data-stale hidden>as last read</span>"#
            } else {
                ""
            }
        );
        if cards.is_empty() {
            s.push_str(r#"<li class="inbox-empty">None right now.</li>"#);
        }
        for h in cards {
            let id = esc(h["id"].as_str().unwrap_or_default());
            let role = h["role"]
                .as_str()
                .map(|r| format!("{} · ", esc(r)))
                .unwrap_or_default();
            let open = if key == "done" {
                String::new()
            } else {
                format!(
                    r#"<button type="button" class="inbox-open" data-herdr-open="{id}" data-terminal="{}">Open in Herdr</button>"#,
                    esc(&crate::agents::follow::terminal_name(
                        h["id"].as_str().unwrap_or_default()
                    ))
                )
            };
            let _ = write!(
                s,
                r#"<li class="inbox-card" data-glide="{id}" data-phase="{}">{}<div class="inbox-main"><p class="inbox-title">{}</p><p class="inbox-meta">{role}{} · {}</p></div>{open}</li>"#,
                esc(h["phase"].as_str().unwrap_or_default()),
                hire_mark(h, "inbox-mark"),
                esc(h["title"].as_str().unwrap_or_default()),
                hire_word(h),
                span_words(h["active_ms"].as_u64().unwrap_or(0)),
            );
        }
        s.push_str("</ul></section>");
    }
    s.push_str("</div></section>");
    s
}

/// Where a brief run shows itself (`cos-brief-state` from the window): rays
/// and the message while it runs, a tick and "Brief ready" when it is done.
const LOADER: &str = r#"<p class="brief-loader" id="brief-loader" role="status" aria-live="polite" data-state="idle" hidden><span class="brief-loader__icon" aria-hidden="true"><svg class="brief-loader__rays" viewBox="0 0 16 16"><line x1="8" y1="1.75" x2="8" y2="4.75" opacity="0.2"/><line x1="8" y1="1.75" x2="8" y2="4.75" opacity="0.3" transform="rotate(45 8 8)"/><line x1="8" y1="1.75" x2="8" y2="4.75" opacity="0.4" transform="rotate(90 8 8)"/><line x1="8" y1="1.75" x2="8" y2="4.75" opacity="0.5" transform="rotate(135 8 8)"/><line x1="8" y1="1.75" x2="8" y2="4.75" opacity="0.6" transform="rotate(180 8 8)"/><line x1="8" y1="1.75" x2="8" y2="4.75" opacity="0.7" transform="rotate(225 8 8)"/><line x1="8" y1="1.75" x2="8" y2="4.75" opacity="0.85" transform="rotate(270 8 8)"/><line x1="8" y1="1.75" x2="8" y2="4.75" transform="rotate(315 8 8)"/></svg><svg class="brief-loader__check" viewBox="0 0 16 16"><path d="M3.5 8.4 6.6 11.3 12.5 4.9" pathLength="1"/></svg></span><span class="brief-loader__text"></span><span class="brief-loader__band" aria-hidden="true"></span></p>"#;

/// Longest failure reason the hero shows; the full text is in the receipt.
const REASON_CHARS: usize = 180;

/// Today's run left no brief: why, and which brief the page shows instead.
/// `None` once today's brief exists or no run was recorded today.
fn missed(cx: &Ctx) -> Option<String> {
    let r = &cx.data["last_run"];
    let status = r["status"].as_str()?;
    let day = r["day"].as_str()?;
    if status == "ok" || day.parse::<Date>().ok()? != cx.today() || cx.day == day {
        return None;
    }
    let reason = r["reason"].as_str().unwrap_or_default().trim();
    let mut why: String = reason.chars().take(REASON_CHARS).collect();
    if reason.chars().count() > REASON_CHARS {
        why.push('…');
    }
    if why.is_empty() {
        why = "no reason recorded".into();
    }
    if r["attempts"].as_u64().unwrap_or(0) > 1 {
        why.push_str(" (tried twice)");
    }
    let last = match cx.day.parse::<Date>() {
        Ok(d) if cx.brief.is_some() => format!("last brief {}", day_name(d)),
        _ => "no brief saved yet".into(),
    };
    Some(format!(
        r#"<p class="run-alert" role="status" data-run="{}"><b>Today's brief didn't run:</b> {}<span class="dot" aria-hidden="true">·</span>{}</p>"#,
        esc(status),
        prose(&why),
        esc(&last)
    ))
}

fn run_pill(cx: &Ctx, b: &Brief) -> String {
    let mut parts = Vec::new();
    let run = &cx.data["run"];
    let at = run["at"]
        .as_str()
        .and_then(parse_instant)
        .map(|t| t.to_zoned_datetime(cx.tz.clone()));
    parts.push(match at {
        Some(z) if z.date() == cx.today() => format!(
            r#"brief <b class="num">{}</b> <span class="tz">{}</span>"#,
            z.strftime("%H:%M"),
            esc(&z.strftime("%Z").to_string())
        ),
        Some(z) => format!(
            r#"brief {}, <b class="num">{}</b> <span class="tz">{}</span>"#,
            day_name(z.date()),
            z.strftime("%H:%M"),
            esc(&z.strftime("%Z").to_string())
        ),
        None => format!("brief {}", esc(&cx.date(&cx.day))),
    });
    if let Some(c) = run["cost_usd"].as_f64() {
        parts.push(format!(r#"<b class="num">${c:.2}</b>"#));
    }
    let n = b.coverage.len();
    if n > 0 {
        let full = b
            .coverage
            .iter()
            .filter(|c| c.health == Health::Healthy && c.review == Completeness::Complete)
            .count();
        parts.push(format!(
            r##"<a href="#coverage"><b class="num">{full}</b> of <b class="num">{n}</b> sources read in full</a>"##
        ));
    } else {
        parts.push("coverage not reported".into());
    }
    parts.join(r#"<span class="dot" aria-hidden="true">·</span>"#)
}

/// Without a brief the page's script fills these from the facts and the
/// saved assessments.
const FALLBACK: &str = r#"<div class="celebrate"><div class="card wins-card"><h3>What changed</h3><ul class="wins" id="wins"></ul></div><div class="card"><h3>Shipped, last 7 days</h3><div class="chart" id="chart-merged"></div></div><div class="card soon-card"><h3>Coming up</h3><ul class="soon" id="soon"></ul></div></div><div id="left-body"></div>"#;

/// The Brief panel: actions, the week, the day, then the detail.
pub fn body(data: &Value) -> String {
    let cx = Ctx::new(data);
    let Some(b) = &cx.brief else {
        let mut s = hires(&cx);
        s.push_str(&crate::digest::section(data));
        s.push_str(FALLBACK);
        s.push_str(&crate::constraints::section(data));
        s.push_str(&crate::delivery::section(data));
        return s;
    };
    let mut s = String::from(r#"<div class="today" data-ssr>"#);
    actions(&mut s, &cx, b);
    s.push_str(&crate::digest::section(data));
    s.push_str(&crate::constraints::section(data));
    s.push_str(&crate::delivery::section(data));
    week(&mut s, &cx, b);
    s.push_str(&crate::kpis::section(data));
    schedule(&mut s, &cx, b);
    ribbon(&mut s, &cx, b);
    changes(&mut s, b);
    coverage(&mut s, &cx, b);
    detail(&mut s, b);
    s.push_str("</div>");
    s
}

fn verb(a: &crate::reports::Action) -> (&'static str, String) {
    match a.verb {
        Verb::Do => ("do", "Do".into()),
        Verb::Decide => ("decide", "Decide".into()),
        Verb::Batch => ("batch", "Batch".into()),
        Verb::Delegate if !a.owner.trim().is_empty() => {
            ("delegate", format!("Delegate · {}", a.owner.trim()))
        }
        Verb::Delegate => ("delegate", "Delegate".into()),
    }
}

fn actions(s: &mut String, cx: &Ctx, b: &Brief) {
    s.push_str(&hires(cx));
    let n = b.needs_you.len();
    if n == 0 {
        s.push_str(r#"<p class="quiet" data-section="actions">Nothing needs you today.</p>"#);
        return;
    }
    let with_prompts = b
        .needs_you
        .iter()
        .filter(|a| !a.agent_prompt.trim().is_empty())
        .count();
    let _ = write!(
        s,
        r#"<section class="acts" data-section="actions" aria-labelledby="acts-h"><div class="sec-head"><h2 id="acts-h">Needs you <span class="count num">{n}</span></h2>{}</div><div class="act-grid">"#,
        if with_prompts > 1 {
            r#"<button type="button" class="ghost" data-copy="all">Copy all handoffs</button>"#
        } else {
            ""
        }
    );
    for (i, a) in b.needs_you.iter().enumerate() {
        let (cls, label) = verb(a);
        let due = if a.deadline.is_empty() || a.deadline_basis == Basis::None {
            String::new()
        } else {
            let word = if a.deadline_basis == Basis::Confirmed {
                "due"
            } else {
                "proposed"
            };
            format!(
                r#"<span class="due{}">{word} {}</span>"#,
                if a.deadline_basis == Basis::Confirmed {
                    ""
                } else {
                    " soft"
                },
                esc(&cx.date(&a.deadline))
            )
        };
        let _ = write!(
            s,
            r#"<article class="act act-{cls}"><header><span class="verb verb-{cls}">{}</span>{due}</header><h3>{}</h3>"#,
            esc(&label),
            esc(&a.recommendation)
        );
        if !a.why.is_empty() {
            let _ = write!(
                s,
                r#"<p class="why" tabindex="0" title="Show all">{}</p>"#,
                prose(&a.why)
            );
        }
        if !a.next_step.is_empty() {
            let _ = write!(
                s,
                r#"<p class="next"><b>Next</b> {}</p>"#,
                prose(&a.next_step)
            );
        }
        s.push_str(r#"<div class="chips">"#);
        if !a.goal_id.is_empty() {
            let title = cx.goal_title(&a.goal_id).unwrap_or_default();
            let _ = write!(
                s,
                r#"<span class="chip-goal" title="{}"><b>{}</b>{}</span>"#,
                esc(&title),
                esc(&a.goal_id),
                if title.is_empty() {
                    String::new()
                } else {
                    format!(" {}", esc(&title))
                }
            );
        }
        s.push_str(&chips(&a.source));
        s.push_str("</div>");
        if !a.agent_prompt.trim().is_empty() {
            let _ = write!(
                s,
                r#"<footer><button type="button" class="primary" data-copy="a{i}" data-hire="{}">Copy prompt</button><details class="peek"><summary>Read the handoff</summary><pre id="prompt-a{i}">{}</pre></details></footer>"#,
                i + 1,
                esc(&a.agent_prompt)
            );
        } else if let Some(href) = first_url(&a.source) {
            let _ = write!(
                s,
                r#"<footer><a class="button primary" href="{}" target="_blank" rel="noreferrer noopener">Open</a></footer>"#,
                esc(&href)
            );
        }
        s.push_str("</article>");
    }
    s.push_str("</div></section>");
}

fn glyph(c: Confidence) -> (&'static str, &'static str, &'static str) {
    match c {
        Confidence::Rock => ("rock", "●", "rock · bet it"),
        Confidence::Sand => ("sand", "◐", "sand · shape it first"),
        Confidence::Water => ("water", "○", "water · spike it first"),
    }
}

fn steer(s: Steer) -> &'static str {
    match s {
        Steer::Maintain => "maintain",
        Steer::Sequence => "sequence differently",
        Steer::ReduceScope => "reduce scope",
        Steer::Test => "test",
        Steer::Delegate => "delegate",
        Steer::Stop => "stop",
    }
}

/// The repos a goal names in the workspace (`docs`: `repo` or `repo:path`).
fn goal_repos(cx: &Ctx, id: &str) -> Vec<String> {
    cx.data["workspace"]["goals"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|g| g["id"] == id)
        .and_then(|g| g["docs"].as_array())
        .into_iter()
        .flatten()
        .filter_map(|d| d.as_str())
        .map(|d| d.split(':').next().unwrap_or(d).to_ascii_lowercase())
        .collect()
}

/// Merged PRs per day, Monday to today, in the repos given; the archive the
/// History tab charts, with the latest facts for the days not archived yet.
fn merged_this_week(cx: &Ctx, repos: &[String]) -> Vec<(Date, usize)> {
    let work: Vec<String> = cx.data["work_orgs"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|o| o.as_str().map(str::to_ascii_lowercase))
        .collect();
    let keep = |repo: &str| {
        let (org, name) = repo.split_once('/').unwrap_or(("", repo));
        (work.is_empty() || work.contains(&org.to_ascii_lowercase()))
            && repos.contains(&name.to_ascii_lowercase())
    };
    let today = cx.today();
    let monday = today
        .checked_sub(jiff::Span::new().days(i64::from(today.weekday().to_monday_zero_offset())))
        .unwrap_or(today);
    let mut seen = std::collections::BTreeSet::new();
    let mut days: Vec<(Date, usize)> = Vec::new();
    let mut d = monday;
    while d <= today {
        days.push((d, 0));
        d = match d.tomorrow() {
            Ok(n) => n,
            Err(_) => break,
        };
    }
    let mut add = |repo: &str, number: u64, day: &str| {
        if !keep(repo) || !seen.insert((repo.to_owned(), number)) {
            return;
        }
        if let Some(slot) = day
            .get(..10)
            .and_then(|d| d.parse::<Date>().ok())
            .and_then(|d| days.iter_mut().find(|(x, _)| *x == d))
        {
            slot.1 += 1;
        }
    };
    for m in cx.data["history"]["merged"]
        .as_array()
        .into_iter()
        .flatten()
    {
        add(
            m["repo"].as_str().unwrap_or_default(),
            m["number"].as_u64().unwrap_or_default(),
            m["day"].as_str().unwrap_or_default(),
        );
    }
    for p in cx.data["facts"]["merged"].as_array().into_iter().flatten() {
        let day = p["closed"]
            .as_str()
            .or(p["updated"].as_str())
            .unwrap_or_default();
        add(
            p["repo"].as_str().unwrap_or_default(),
            p["number"].as_u64().unwrap_or_default(),
            day,
        );
    }
    days
}

/// History's line chart, small: a 2px line in `--chart-1`, ringed dots.
fn sparkline(points: &[(Date, usize)]) -> String {
    if points.is_empty() {
        return String::new();
    }
    let (w, h, pad) = (112.0_f64, 32.0_f64, 4.0_f64);
    let max = points.iter().map(|p| p.1).max().unwrap_or(0).max(1) as f64;
    let x = |i: usize| {
        if points.len() == 1 {
            w / 2.0
        } else {
            pad + (w - 2.0 * pad) * i as f64 / (points.len() - 1) as f64
        }
    };
    let y = |v: usize| pad + (h - 2.0 * pad) * (1.0 - v as f64 / max);
    let total: usize = points.iter().map(|p| p.1).sum();
    let tip = points
        .iter()
        .map(|(d, v)| format!("{} {v}", d.strftime("%a")))
        .collect::<Vec<_>>()
        .join(" · ");
    let mut s = format!(
        r#"<span class="spark" data-tip="Merged this week: {}"><svg viewBox="0 0 {w} {h}" width="{w}" height="{h}" role="img" aria-label="{total} merged this week">"#,
        esc(&tip)
    );
    let _ = write!(
        s,
        r#"<line x1="{pad}" x2="{}" y1="{}" y2="{}" stroke="var(--chart-grid)"/>"#,
        w - pad,
        h - pad,
        h - pad
    );
    if points.len() > 1 {
        let pts: Vec<String> = points
            .iter()
            .enumerate()
            .map(|(i, p)| format!("{:.1},{:.1}", x(i), y(p.1)))
            .collect();
        let _ = write!(
            s,
            r#"<polyline fill="none" stroke="var(--chart-1)" stroke-width="2" stroke-linejoin="round" points="{}"/>"#,
            pts.join(" ")
        );
    }
    let last = points.len() - 1;
    let _ = write!(
        s,
        r#"<circle cx="{:.1}" cy="{:.1}" r="3" fill="var(--chart-1)" stroke="var(--card)" stroke-width="2"/></svg><b class="num">{total}</b></span>"#,
        x(last),
        y(points[last].1)
    );
    s
}

fn week(s: &mut String, cx: &Ctx, b: &Brief) {
    let w = &b.week;
    if w.summary.is_empty() && w.goals.is_empty() && w.decisions.is_empty() {
        return;
    }
    s.push_str(r##"<section class="week" data-section="week" aria-labelledby="week-h"><div class="sec-head"><h2 id="week-h">This week</h2><a class="more" href="#right">Steering</a></div>"##);
    if !w.summary.is_empty() {
        let _ = write!(s, r#"<p class="lead">{}</p>"#, prose(&w.summary));
    }
    if !w.goals.is_empty() {
        s.push_str(r#"<ol class="wk">"#);
        for g in &w.goals {
            let (cls, mark, tip) = glyph(g.confidence);
            let repos = goal_repos(cx, &g.goal_id);
            let spark = if repos.is_empty() {
                String::new()
            } else {
                sparkline(&merged_this_week(cx, &repos))
            };
            let _ = write!(
                s,
                r#"<li class="wk-row"><span class="conf conf-{cls}" data-tip="{tip}" tabindex="0" aria-label="{tip}">{mark}</span><div class="wk-main"><p class="wk-goal"><b>{}</b> <span class="steer">{}</span></p><p class="wk-next">{}</p>"#,
                esc(&g.goal),
                steer(g.recommendation),
                prose(&g.next)
            );
            if !g.bottleneck.is_empty() || !g.would_change.is_empty() || !g.kpi.is_empty() {
                s.push_str(r#"<details class="wk-more"><summary>"#);
                s.push_str(if g.bottleneck.is_empty() {
                    "KPI and what would change it"
                } else {
                    "Bottleneck, KPI, what would change it"
                });
                s.push_str("</summary><dl>");
                for (k, v) in [
                    ("Bottleneck", &g.bottleneck),
                    ("KPI", &g.kpi),
                    ("Evidence", &g.evidence),
                    ("Would change if", &g.would_change),
                ] {
                    if !v.is_empty() {
                        let _ = write!(s, "<dt>{k}</dt><dd>{}</dd>", prose(v));
                    }
                }
                s.push_str("</dl></details>");
            }
            let _ = write!(s, "</div>{spark}</li>");
        }
        s.push_str("</ol>");
    }
    if !w.decisions.is_empty() || !w.stop_doing.is_empty() {
        s.push_str(r#"<div class="wk-foot">"#);
        for d in &w.decisions {
            let _ = write!(
                s,
                r#"<details class="owe"><summary><span class="owe-k">You decide</span> {}</summary><p>{}</p><p><b>Recommend</b> {}</p></details>"#,
                esc(&d.decision),
                d.options
                    .iter()
                    .map(|o| esc(o))
                    .collect::<Vec<_>>()
                    .join(" · "),
                prose(&d.recommendation)
            );
        }
        if !w.stop_doing.is_empty() {
            let _ = write!(
                s,
                r#"<details class="stop"><summary>Stop doing <span class="num">{}</span></summary><ul>{}</ul></details>"#,
                w.stop_doing.len(),
                w.stop_doing
                    .iter()
                    .map(|x| format!("<li>{}</li>", prose(x)))
                    .collect::<String>()
            );
        }
        s.push_str("</div>");
    }
    s.push_str("</section>");
}

fn minutes(t: &str) -> Option<u32> {
    let (h, m) = t.get(..5)?.split_once(':')?;
    Some(h.parse::<u32>().ok()? * 60 + m.parse::<u32>().ok()?)
}

/// The week's calendar on a 07:00–20:00 axis, one row per day that has a
/// block, a milestone, or is today; a now-line on today.
fn ribbon(s: &mut String, cx: &Ctx, b: &Brief) {
    const START: u32 = 7 * 60;
    const END: u32 = 20 * 60;
    let pct = |m: u32| (m.clamp(START, END) - START) as f64 * 100.0 / (END - START) as f64;
    let today = cx.today();
    let monday = today
        .checked_sub(jiff::Span::new().days(i64::from(today.weekday().to_monday_zero_offset())))
        .unwrap_or(today);
    let sunday = monday
        .checked_add(jiff::Span::new().days(6))
        .unwrap_or(today);
    let brief_day = cx.day.parse::<Date>().ok();
    let slot_day = |date: &str| {
        if date.is_empty() {
            brief_day
        } else {
            date.parse::<Date>().ok()
        }
    };
    let mut days: Vec<Date> = b
        .calendar
        .iter()
        .filter_map(|c| slot_day(&c.date))
        .chain(
            b.upcoming
                .iter()
                .filter_map(|m| m.date.parse::<Date>().ok()),
        )
        .filter(|d| *d >= monday && *d <= sunday)
        .collect();
    if b.calendar.iter().any(|c| slot_day(&c.date) == Some(today)) || !days.is_empty() {
        days.push(today);
    }
    days.sort();
    days.dedup();
    if days.is_empty() {
        s.push_str(r#"<p class="quiet" data-section="ribbon">No calendar read for this week.</p>"#);
        return;
    }
    s.push_str(r#"<section class="ribbon" data-section="ribbon" aria-labelledby="rib-h"><div class="sec-head"><h2 id="rib-h">The week, by the hour</h2></div><div class="rib">"#);
    s.push_str(r#"<div class="rib-axis" aria-hidden="true"><span></span><div class="rib-track">"#);
    for h in (8..=20).step_by(2) {
        let _ = write!(
            s,
            r#"<span class="tick" style="left:{:.2}%">{h:02}:00</span>"#,
            pct(h * 60)
        );
    }
    s.push_str("</div></div>");
    let now_m = cx.now.hour() as u32 * 60 + cx.now.minute() as u32;
    for d in &days {
        let is_today = *d == today;
        let flags: Vec<String> = b
            .upcoming
            .iter()
            .filter(|m| m.date.parse::<Date>().ok() == Some(*d))
            .map(|m| {
                format!(
                    r#"<span class="flag" title="{}">{}</span>"#,
                    esc(&m.source),
                    esc(&m.what)
                )
            })
            .collect();
        let _ = write!(
            s,
            r#"<div class="rib-row{}"><span class="rib-day">{}{}</span><div class="rib-track">"#,
            if is_today { " is-today" } else { "" },
            if is_today {
                format!("Today <small>{}</small>", d.strftime("%a %-d"))
            } else {
                day_name(*d)
            },
            flags.join("")
        );
        for c in b.calendar.iter().filter(|c| slot_day(&c.date) == Some(*d)) {
            let (Some(a), Some(z)) = (minutes(&c.start), minutes(&c.end)) else {
                continue;
            };
            if z <= a {
                continue;
            }
            let past = *d < today || (is_today && z <= now_m);
            let tip = format!(
                "{}–{} {}{}",
                c.start,
                c.end,
                c.what,
                if c.prep.is_empty() {
                    String::new()
                } else {
                    format!(" · Prep: {}", c.prep)
                }
            );
            let _ = write!(
                s,
                r#"<span class="blk{}" tabindex="0" style="left:{:.2}%;width:{:.2}%" data-tip="{}"><span>{}</span></span>"#,
                if past { " past" } else { "" },
                pct(a),
                (pct(z) - pct(a)).max(0.6),
                esc(&tip),
                esc(&c.what)
            );
        }
        if is_today && (START..=END).contains(&now_m) {
            let _ = write!(
                s,
                r#"<i class="now" style="left:{:.2}%" role="img" aria-label="now {}"></i>"#,
                pct(now_m),
                cx.now.strftime("%H:%M")
            );
        }
        s.push_str("</div></div>");
    }
    s.push_str("</div>");
    let ahead: Vec<_> = b
        .calendar
        .iter()
        .filter(|c| {
            slot_day(&c.date) == Some(today)
                && !c.prep.is_empty()
                && minutes(&c.end).is_none_or(|e| e > now_m)
        })
        .collect();
    if !ahead.is_empty() {
        s.push_str(r#"<ul class="prep">"#);
        for c in ahead {
            let _ = write!(
                s,
                r#"<li><span class="num">{}</span> <b>{}</b> {}</li>"#,
                esc(&c.start),
                esc(&c.what),
                prose(&c.prep)
            );
        }
        s.push_str("</ul>");
    }
    let later: Vec<_> = b
        .upcoming
        .iter()
        .filter(|m| m.date.parse::<Date>().is_ok_and(|d| d > sunday))
        .take(4)
        .collect();
    if !later.is_empty() {
        s.push_str(r#"<p class="later"><span class="later-k">Later</span>"#);
        for m in later {
            let days = m
                .date
                .parse::<Date>()
                .ok()
                .and_then(|d| (d - today).get_days().into())
                .unwrap_or(0);
            let _ = write!(
                s,
                r#"<span class="flag soft" title="{}">{} <small>{} · in {days} days</small></span>"#,
                esc(&m.source),
                esc(&m.what),
                esc(&cx.date(&m.date))
            );
        }
        s.push_str("</p>");
    }
    s.push_str("</section>");
}

/// Half the schedule board's axis: five hours either side of now.
const ASB_WINDOW_MS: i64 = 5 * 3_600_000;

/// One marker and row on the schedule board: a calendar slot or a hire run.
struct Run {
    name: String,
    cadence: String,
    at_ms: i64,
    state: &'static str,
    word: String,
    percent: Option<u8>,
    when: String,
    took: String,
    note: String,
}

/// Horizontal position of `at_ms` on the axis, 0 to 100 percent, centred on now.
fn asb_position(at_ms: i64, now_ms: i64) -> f64 {
    (50.0 + (at_ms - now_ms) as f64 / ASB_WINDOW_MS as f64 * 50.0).clamp(0.0, 100.0)
}

/// rv2-kit's activity trace in a 100 x 24 box: a running run climbs from its
/// marker towards the now line by its progress; a finished run leaves a blip.
fn asb_trace(runs: &[Run], now_ms: i64) -> String {
    let marks: Vec<(f64, &str, f64)> = runs
        .iter()
        .map(|r| {
            (
                asb_position(r.at_ms, now_ms),
                r.state,
                f64::from(r.percent.unwrap_or(0)),
            )
        })
        .collect();
    (0..=50)
        .map(|step| {
            let x = f64::from(step) * 2.0;
            let mut level: f64 = 0.0;
            for (start, state, progress) in &marks {
                match *state {
                    "running" if *start <= 50.0 && x >= *start => {
                        let ramp = ((x - start) / (50.0 - start).max(1.0)).clamp(0.0, 1.0);
                        level = level.max(ramp * (0.3 + 0.7 * progress / 100.0));
                    }
                    "done" | "failed" | "needs-you" => {
                        let d = (x - start).abs();
                        if d < 3.0 {
                            level = level.max(0.6 * (1.0 - d / 3.0));
                        }
                    }
                    _ => {}
                }
            }
            format!("{x:.1},{:.1}", 23.0 - level * 21.0)
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Today's calendar slots and the hire runs that started today, in time order.
fn todays_runs(cx: &Ctx, b: &Brief) -> Vec<Run> {
    let today = cx.today();
    let now_ms = cx.now.timestamp().as_millisecond();
    let brief_day = cx.day.parse::<Date>().ok();
    let at = |m: u32| {
        today
            .at((m / 60) as i8, (m % 60) as i8, 0, 0)
            .to_zoned_datetime(cx.tz.clone())
            .ok()
            .map(|z| z.timestamp().as_millisecond())
    };
    let mut runs = Vec::new();
    for c in &b.calendar {
        let day = if c.date.is_empty() {
            brief_day
        } else {
            c.date.parse::<Date>().ok()
        };
        if day != Some(today) {
            continue;
        }
        let (Some(a), Some(z)) = (minutes(&c.start), minutes(&c.end)) else {
            continue;
        };
        let (Some(from), Some(to)) = (at(a), at(z)) else {
            continue;
        };
        if to <= from {
            continue;
        }
        let (state, percent) = if now_ms < from {
            ("upcoming", None)
        } else if now_ms < to {
            ("running", Some(((now_ms - from) * 100 / (to - from)) as u8))
        } else {
            ("done", None)
        };
        runs.push(Run {
            name: c.what.clone(),
            cadence: format!("Calendar, {}–{}", c.start, c.end),
            at_ms: from,
            state,
            word: match state {
                "upcoming" => "Upcoming",
                "running" => "Running",
                _ => "Done",
            }
            .into(),
            percent,
            when: if state == "upcoming" {
                format!("Next in {} min", (from - now_ms) / 60_000)
            } else {
                format!("Started {}", c.start)
            },
            took: span_words((to - from) as u64),
            note: if c.prep.is_empty() {
                String::new()
            } else {
                format!("Prep: {}", c.prep)
            },
        });
    }
    for h in cx.data["hires"].as_array().into_iter().flatten() {
        let Some(started) = h["started"]
            .as_str()
            .and_then(parse_instant)
            .map(|t| t.to_zoned_datetime(cx.tz.clone()))
        else {
            continue;
        };
        if started.date() != today {
            continue;
        }
        let phase = h["phase"].as_str().unwrap_or_default();
        let (state, word) = match phase {
            "working" | "starting" => ("running", "Running"),
            "needs-you" => ("needs-you", "Needs you"),
            "done" => ("done", "Done"),
            "paused" => ("paused", "Paused"),
            "lost" => ("failed", "Lost"),
            "timed-out" => ("failed", "Timed out"),
            "stopped" => ("stopped", "Stopped"),
            _ => ("failed", "Failed"),
        };
        let active = h["active_ms"].as_u64().unwrap_or(0);
        let deadline = h["deadline_secs"].as_u64().unwrap_or(0) * 1000;
        runs.push(Run {
            name: h["title"].as_str().unwrap_or_default().to_owned(),
            cadence: h["role"]
                .as_str()
                .map_or_else(|| "Hire".to_owned(), |r| format!("Hire, {r}")),
            at_ms: started.timestamp().as_millisecond(),
            state,
            word: word.into(),
            percent: (state == "running" && deadline > 0)
                .then(|| (active.min(deadline) * 100 / deadline) as u8),
            when: format!("Started {}", started.strftime("%H:%M")),
            took: span_words(active),
            note: format!("{}, {}", hire_word(h), span_words(active)),
        });
    }
    runs.sort_by_key(|r| r.at_ms);
    runs
}

/// rv2-kit AgentScheduleBoard on Today: a timeline centred on now (five hours
/// either side) with past markers carrying outcomes in words and upcoming ones
/// hollow, the activity trace, the status line, a row per run and its detail.
/// Read-only: no switch, Run now, Skip or Restore.
fn schedule(s: &mut String, cx: &Ctx, b: &Brief) {
    let runs = todays_runs(cx, b);
    let now_ms = cx.now.timestamp().as_millisecond();
    let ahead = runs.iter().filter(|r| r.state == "upcoming").count();
    let _ = write!(
        s,
        r#"<section class="kit-asb" id="schedule" data-section="schedule" data-now="{now_ms}" data-tz="{}" aria-labelledby="asb-h"><header class="kit-asb__head"><h2 class="kit-asb__title" id="asb-h">Today</h2><p class="kit-asb__count">{} of {} ahead</p></header>"#,
        esc(cx.tz.iana_name().unwrap_or("UTC")),
        ahead,
        runs.len()
    );
    if runs.is_empty() {
        s.push_str(r#"<div class="kit-asb__empty"><p>Nothing on the calendar today and nothing hired yet.</p></div></section>"#);
        return;
    }
    let _ = write!(
        s,
        r#"<section class="kit-asb__timeline" aria-label="Timeline"><svg class="kit-asb__trace" viewBox="0 0 100 24" preserveAspectRatio="none" aria-hidden="true"><polyline class="kit-asb__trace-line" points="{}"/></svg><div class="kit-asb__axis"><span class="kit-asb__now" style="left: 50.00%;" aria-hidden="true"></span>"#,
        asb_trace(&runs, now_ms)
    );
    let hour = cx
        .now
        .checked_sub(jiff::Span::new().hours(5))
        .ok()
        .and_then(|z| z.round(jiff::Unit::Hour).ok());
    let mut t = hour;
    while let Some(z) = t {
        let at = z.timestamp().as_millisecond();
        if at >= now_ms + ASB_WINDOW_MS {
            break;
        }
        if z.hour() % 2 == 0 && (at - now_ms).abs() < ASB_WINDOW_MS {
            let _ = write!(
                s,
                r#"<span class="kit-asb__tick" style="left: {:.2}%;">{}</span>"#,
                asb_position(at, now_ms),
                z.strftime("%H:00")
            );
        }
        t = z.checked_add(jiff::Span::new().hours(1)).ok();
    }
    for (i, r) in runs.iter().enumerate() {
        let when = if r.at_ms < now_ms { "earlier" } else { "later" };
        let label = match r.percent {
            Some(p) if r.state == "running" => {
                format!("{}, {when}, in progress, {p} percent", r.name)
            }
            _ => format!("{}, {when}, {}", r.name, r.word),
        };
        let _ = write!(
            s,
            r#"<button type="button" class="kit-asb__marker" data-state="{}" data-at="{}" data-percent="{}" data-pick="{i}" style="left: {:.2}%;" aria-label="{}"{}></button>"#,
            r.state,
            r.at_ms,
            r.percent.unwrap_or(0),
            asb_position(r.at_ms, now_ms),
            esc(&label),
            if (r.at_ms - now_ms).abs() < ASB_WINDOW_MS {
                ""
            } else {
                " hidden"
            }
        );
    }
    let status = match runs.iter().find(|r| r.state == "running") {
        Some(r) => format!(
            "Running now: {}, {} percent",
            r.name,
            r.percent.unwrap_or(0)
        ),
        None => match runs.iter().find(|r| r.at_ms >= now_ms) {
            Some(r) => format!("Next: {} in {} min", r.name, (r.at_ms - now_ms) / 60_000),
            None => "Nothing else scheduled today.".into(),
        },
    };
    let _ = write!(
        s,
        r#"</div><p class="kit-asb__status" role="status">{}</p></section><div class="kit-asb__body"><ul class="kit-asb__list" role="list">"#,
        esc(&status)
    );
    let selected = runs
        .iter()
        .position(|r| r.state == "running" || r.at_ms >= now_ms)
        .unwrap_or(runs.len() - 1);
    for (i, r) in runs.iter().enumerate() {
        let _ = write!(
            s,
            r#"<li class="kit-asb__row" data-pick="{i}"><button type="button" class="kit-asb__row-hit" id="asb-row-{i}" data-pick="{i}"{}><span class="kit-asb__name">{}</span><span class="kit-asb__meta">{}</span><span class="kit-asb__meta">{}</span><span class="kit-asb__outcome" data-outcome="{}">{}</span><span class="kit-asb__meta">{}</span></button></li>"#,
            if i == selected {
                r#" aria-current="true""#
            } else {
                ""
            },
            esc(&r.name),
            esc(&r.cadence),
            esc(&r.when),
            r.state,
            esc(&r.word),
            esc(&r.took)
        );
    }
    s.push_str("</ul>");
    for (i, r) in runs.iter().enumerate() {
        let _ = write!(
            s,
            r#"<aside class="kit-asb__panel" data-panel="{i}" aria-label="Detail"{}><h3 class="kit-asb__panel-title">{}</h3>{}<p class="kit-asb__meta">{} · {}</p><section class="kit-asb__block" aria-label="When"><h4 class="kit-asb__section">When</h4><ul class="kit-asb__runs"><li><span>{}</span><span class="kit-asb__outcome" data-outcome="{}">{}</span></li></ul></section></aside>"#,
            if i == selected { "" } else { " hidden" },
            esc(&r.name),
            if r.note.is_empty() {
                String::new()
            } else {
                format!(r#"<p class="kit-asb__meta">{}</p>"#, prose(&r.note))
            },
            esc(&r.cadence),
            esc(&r.took),
            esc(&r.when),
            r.state,
            esc(&r.word)
        );
    }
    s.push_str("</div></section>");
}

fn changes(s: &mut String, b: &Brief) {
    if b.what_changed.is_empty() {
        return;
    }
    s.push_str(r#"<section class="changed" data-section="changed" aria-labelledby="chg-h"><div class="sec-head"><h2 id="chg-h">What changed</h2></div><ul>"#);
    for c in &b.what_changed {
        let (cls, word) = match c.kind {
            ChangeKind::Done => ("done", "Done"),
            ChangeKind::Unblocked => ("done", "Unblocked"),
            ChangeKind::Risk => ("risk", "Risk"),
            ChangeKind::TimeChanged => ("time", "Time changed"),
        };
        let _ = write!(
            s,
            r#"<li><span class="kind kind-{cls}">{word}</span> {} {}</li>"#,
            prose(&c.what),
            chips(&c.source)
        );
    }
    s.push_str("</ul></section>");
}

fn coverage(s: &mut String, cx: &Ctx, b: &Brief) {
    if b.coverage.is_empty() && b.gaps.is_empty() {
        s.push_str(r#"<p class="quiet" id="coverage" data-section="coverage">This brief reported no coverage.</p>"#);
        return;
    }
    s.push_str(r#"<section class="cover" id="coverage" data-section="coverage"><span class="cover-k">Read this run</span>"#);
    for c in &b.coverage {
        let tone = match (c.health, c.review) {
            (Health::Healthy, Completeness::Complete) => "ok",
            (Health::Healthy, _) => "part",
            _ => "bad",
        };
        let health = serde_json::to_value(c.health)
            .ok()
            .and_then(|v| v.as_str().map(|s| s.replace('_', " ")))
            .unwrap_or_default();
        let review = serde_json::to_value(c.review)
            .ok()
            .and_then(|v| v.as_str().map(|s| s.replace('_', " ")))
            .unwrap_or_default();
        let window = match (c.window_start.is_empty(), c.window_end.is_empty()) {
            (false, false) => format!(
                "{} to {}",
                cx.stamp(&c.window_start),
                cx.stamp(&c.window_end)
            ),
            (false, true) => format!("from {}", cx.stamp(&c.window_start)),
            _ => String::new(),
        };
        let _ = write!(
            s,
            r#"<details class="src src-{tone}"><summary><i aria-hidden="true"></i>{} <small>{review}</small></summary><div class="src-pop"><p><b>{health}</b> · {review}{}</p><p>{}</p></div></details>"#,
            esc(&c.source),
            if window.is_empty() {
                String::new()
            } else {
                format!(" · {}", esc(&window))
            },
            prose(&c.note)
        );
    }
    for g in &b.gaps {
        let _ = write!(
            s,
            r#"<span class="src src-gap" title="{}"><i aria-hidden="true"></i>not read</span>"#,
            esc(g)
        );
    }
    s.push_str("</section>");
    if !b.gaps.is_empty() {
        let _ = write!(
            s,
            r#"<p class="quiet gaps">{}</p>"#,
            b.gaps
                .iter()
                .map(|g| prose(g))
                .collect::<Vec<_>>()
                .join(" ")
        );
    }
}

fn detail(s: &mut String, b: &Brief) {
    s.push_str(r#"<section class="detail" data-section="detail">"#);
    if !b.backlog.is_empty() {
        s.push_str(r#"<div class="card"><h3>Backlog · kept with an owner and a trigger</h3><ul class="plain">"#);
        for f in &b.backlog {
            let _ = write!(
                s,
                r#"<li><b>{}</b>{}<div class="sub">next review: {}</div>{}</li>"#,
                prose(&f.item),
                if f.owner.is_empty() {
                    String::new()
                } else {
                    format!(r#" <span class="muted">({})</span>"#, esc(&f.owner))
                },
                prose(&f.next_review),
                if f.source.is_empty() {
                    String::new()
                } else {
                    format!(r#"<div class="chips">{}</div>"#, chips(&f.source))
                }
            );
        }
        s.push_str("</ul></div>");
    }
    if !b.tickets.is_empty() {
        let _ = write!(
            s,
            r#"<div class="card" data-tickets><h3>Jira assigned to you · {} open</h3><div class="scroll"><table class="cov"><tr><th>ticket</th><th>summary</th><th>status</th><th>priority</th><th>goal</th></tr>"#,
            b.tickets.len()
        );
        let mut tickets: Vec<_> = b.tickets.iter().collect();
        tickets.sort_by(|a, b| a.key.cmp(&b.key));
        for t in tickets {
            let key = match url_of(&t.url) {
                Some(u) => format!(
                    r#"<a href="{}" target="_blank" rel="noreferrer noopener" title="{}">{}</a>"#,
                    esc(&u),
                    esc(&u),
                    esc(&t.key)
                ),
                None => esc(&t.key),
            };
            let _ = write!(
                s,
                r#"<tr><td class="num">{key}</td><td>{}</td><td>{}</td><td>{}</td><td class="ids">{}</td></tr>"#,
                prose(&t.summary),
                esc(&t.status),
                esc(&t.priority),
                esc(&t.goal_id)
            );
        }
        s.push_str("</table></div></div>");
    }
    s.push_str(r#"<div class="grid g2"><div class="card"><h3>Review queue by age</h3><div class="chart" id="chart-age"></div></div><div class="card"><h3>Ready tasks by repo and priority</h3><div class="chart" id="chart-tasks"></div></div></div>"#);
    s.push_str("</section>");
}

fn url_of(s: &str) -> Option<String> {
    let t = s.trim();
    ((t.starts_with("https://") || t.starts_with("http://"))
        && !t.contains(|c: char| c.is_whitespace() || c == '"' || c == '<' || c == '>'))
    .then(|| t.to_owned())
}

fn first_url(s: &str) -> Option<String> {
    s.split_whitespace()
        .find_map(|w| url_of(w.trim_end_matches([',', ')', '.', ';'])))
}

/// Body text with any raw URL folded into a short chip.
pub fn prose(s: &str) -> String {
    let mut out = String::new();
    for (i, word) in s.split(' ').enumerate() {
        if i > 0 {
            out.push(' ');
        }
        let trimmed = word.trim_end_matches([',', ')', '.', ';', ':']);
        match url_of(trimmed) {
            Some(u) => {
                out.push_str(&chip_for(&u));
                out.push_str(&esc(&word[trimmed.len()..]));
            }
            None => out.push_str(&esc(word)),
        }
    }
    out
}

const IC_PR: &str = r#"<svg class="ic" viewBox="0 0 16 16" aria-hidden="true"><circle cx="4" cy="3.5" r="1.75"/><circle cx="4" cy="12.5" r="1.75"/><circle cx="12" cy="12.5" r="1.75"/><path d="M4 5.25v5.5M12 10.75V6a2 2 0 0 0-2-2H7.5M9 2.5 7.5 4 9 5.5"/></svg>"#;
const IC_TICKET: &str = r#"<svg class="ic" viewBox="0 0 16 16" aria-hidden="true"><path d="M2 4.5h12v2.25a1.5 1.5 0 0 0 0 2.5v2.25H2V9.25a1.5 1.5 0 0 0 0-2.5z"/></svg>"#;
const IC_CAL: &str = r#"<svg class="ic" viewBox="0 0 16 16" aria-hidden="true"><rect x="2.5" y="3.5" width="11" height="10" rx="1.5"/><path d="M2.5 6.5h11M5.5 2v3M10.5 2v3"/></svg>"#;
const IC_MAIL: &str = r#"<svg class="ic" viewBox="0 0 16 16" aria-hidden="true"><rect x="2" y="3.5" width="12" height="9" rx="1.5"/><path d="m2.5 4.5 5.5 4 5.5-4"/></svg>"#;
const IC_LINK: &str = r#"<svg class="ic" viewBox="0 0 16 16" aria-hidden="true"><path d="M6.5 9.5 9.5 6.5M7 4.5l1-1a2.5 2.5 0 0 1 3.5 3.5l-1 1M9 11.5l-1 1A2.5 2.5 0 0 1 4.5 9l1-1"/></svg>"#;
const IC_FACT: &str = r#"<svg class="ic" viewBox="0 0 16 16" aria-hidden="true"><path d="M3 13V8M8 13V3M13 13V6"/></svg>"#;

/// A ticket or PR reference as a link chip: a URL by what it points at, a
/// bare key (`API-32`) as a ticket.
pub fn ref_chip(t: &str) -> String {
    match url_of(t.trim()) {
        Some(u) => chip_for(&u),
        None => chip(IC_TICKET, t.trim(), None, t),
    }
}

/// A source chip with any icon markup (a `design::icon`), linked when `href` is.
pub fn link_chip(icon: &str, label: &str, href: Option<&str>, title: &str) -> String {
    chip(icon, label, href, title)
}

fn chip(icon: &str, label: &str, href: Option<&str>, title: &str) -> String {
    match href {
        Some(h) => format!(
            r#"<a class="src-chip" href="{}" target="_blank" rel="noreferrer noopener" title="{}">{icon}{}</a>"#,
            esc(h),
            esc(title),
            esc(label)
        ),
        None => format!(
            r#"<span class="src-chip" title="{}">{icon}{}</span>"#,
            esc(title),
            esc(label)
        ),
    }
}

/// A Jira key at the end of `/browse/KEY-12`.
fn jira_key(path: &str) -> Option<&str> {
    let key = path
        .split("/browse/")
        .nth(1)?
        .split(['/', '?', '#'])
        .next()?;
    let (p, n) = key.split_once('-')?;
    (!p.is_empty()
        && p.chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        && !n.is_empty()
        && n.chars().all(|c| c.is_ascii_digit()))
    .then_some(key)
}

fn chip_for(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    if host == "github.com" {
        let parts: Vec<&str> = path.split(['/', '?', '#']).collect();
        if let [_org, repo, kind, n, ..] = parts.as_slice()
            && matches!(*kind, "pull" | "issues")
            && n.chars().all(|c| c.is_ascii_digit())
        {
            return chip(IC_PR, &format!("{repo}#{n}"), Some(url), url);
        }
        if let [org, repo, ..] = parts.as_slice() {
            return chip(IC_PR, &format!("{org}/{repo}"), Some(url), url);
        }
    }
    if let Some(key) = jira_key(path) {
        return chip(IC_TICKET, key, Some(url), url);
    }
    chip(IC_LINK, host.trim_start_matches("www."), Some(url), url)
}

/// A `source` as compact chips: GitHub PRs, Jira keys, calendar, mail, URLs.
pub fn chips(source: &str) -> String {
    let t = source.trim();
    if t.is_empty() {
        return String::new();
    }
    let urls: Vec<String> = t
        .split_whitespace()
        .filter_map(|w| url_of(w.trim_end_matches([',', ')', '.', ';'])))
        .collect();
    if !urls.is_empty() {
        return urls.iter().map(|u| chip_for(u)).collect();
    }
    let lower = t.to_ascii_lowercase();
    if lower.starts_with("gh:") {
        let raw = t[3..].trim();
        if let Some((repo, n)) = raw.split_once('#')
            && repo.contains('/')
            && n.chars().all(|c| c.is_ascii_digit())
        {
            // GitHub redirects /issues/<n> to the PR when it is one.
            let href = format!("https://github.com/{repo}/issues/{n}");
            let name = repo.rsplit('/').next().unwrap_or(repo);
            return chip(IC_PR, &format!("{name}#{n}"), Some(&href), &href);
        }
        return chip(IC_PR, raw, None, t);
    }
    if lower.starts_with("jira:") {
        return chip(IC_TICKET, t[5..].trim(), None, t);
    }
    if lower.starts_with("cal:") || lower == "calendar" {
        let label = t
            .get(4..)
            .map(str::trim)
            .filter(|s| !s.is_empty() && lower != "calendar");
        return chip(IC_CAL, label.unwrap_or("calendar"), None, t);
    }
    if lower.starts_with("mail:") || lower.starts_with("email:") || lower == "email" {
        let label = t.split_once(':').map_or(t, |(_, r)| r.trim());
        return chip(
            IC_MAIL,
            &short(if label.is_empty() { "email" } else { label }),
            None,
            t,
        );
    }
    if let Some(rest) = t.strip_prefix("cos facts:") {
        return chip(IC_FACT, &format!("facts · {}", rest.trim()), None, t);
    }
    chip(IC_FACT, &short(t), None, t)
}

fn short(s: &str) -> String {
    if s.chars().count() > 40 {
        format!("{}…", s.chars().take(39).collect::<String>())
    } else {
        s.to_owned()
    }
}
