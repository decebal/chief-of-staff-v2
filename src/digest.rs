//! Since your last brief: what Today shows from the time the previous ok
//! brief ran, in three lanes (decisions, errors, done) and a chart of the
//! timed items.
//!
//! The window opens at the newest ok run receipt before today in the profile's
//! timezone, else at the newest complete run in `state.json`. An item whose
//! timestamp does not parse is left out and noted in the day's audit log.

use std::fmt::Write as _;
use std::path::Path;

use jiff::Timestamp;
use jiff::civil::Date;
use jiff::tz::TimeZone;
use serde_json::{Value, json};

use crate::agents::{self, Outcome, Status, ledger};
use crate::claude;
use crate::history;
use crate::pipeline::{RunReceipt, RunStatus};
use crate::reports::Brief;
use crate::state;
use crate::store::{self, Kind};
use crate::today::{self, Ctx};

/// Items shown per lane; the rest count as "and N more".
const LANE_SHOWN: usize = 3;
/// Chart drawing box, in its own units; the SVG scales to the page.
const CHART_W: f64 = 600.0;
const CHART_H: f64 = 120.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Lane {
    Decisions,
    Errors,
    Done,
}

impl Lane {
    const ALL: [Lane; 3] = [Lane::Decisions, Lane::Errors, Lane::Done];

    fn key(self) -> &'static str {
        match self {
            Self::Decisions => "decisions",
            Self::Errors => "errors",
            Self::Done => "done",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Decisions => "Decisions",
            Self::Errors => "Errors",
            Self::Done => "Done",
        }
    }

    fn empty(self) -> &'static str {
        match self {
            Self::Decisions => "Nothing is waiting on you.",
            Self::Errors => "No errors since your last brief.",
            Self::Done => "Nothing finished since your last brief.",
        }
    }
}

struct Item {
    lane: Lane,
    title: String,
    source: String,
    at: Option<Timestamp>,
}

fn zone(ws: &Path) -> TimeZone {
    state::profile(ws)
        .as_ref()
        .and_then(state::timezone)
        .unwrap_or_else(TimeZone::system)
}

/// Where the window opens, or `None` when no brief has run yet.
pub fn boundary(home: &Path, ws: &Path, now: Timestamp) -> Option<Timestamp> {
    let tz = zone(ws);
    let today = now.to_zoned_datetime(tz.clone()).date();
    receipt_boundary(home, today).or_else(|| run_boundary(home, ws, &tz, today))
}

/// The newest ok receipt whose day is before `today`.
fn receipt_boundary(home: &Path, today: Date) -> Option<Timestamp> {
    let today = today.to_string();
    let (_, receipt) = std::fs::read_dir(home.join("out/runs"))
        .ok()?
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            let day = name
                .strip_suffix(".json")
                .filter(|d| store::is_day(d))?
                .to_owned();
            let receipt: RunReceipt =
                serde_json::from_slice(&std::fs::read(e.path()).ok()?).ok()?;
            Some((day, receipt))
        })
        .filter(|(day, r)| r.status == RunStatus::Ok && *day < today)
        .max_by(|a, b| a.0.cmp(&b.0))?;
    stamp_of(home, "run receipt at", &receipt.at)
}

/// The newest complete run in `state.json` whose local day is before `today`.
fn run_boundary(home: &Path, ws: &Path, tz: &TimeZone, today: Date) -> Option<Timestamp> {
    let st = state::read(ws).ok().flatten()?;
    st["runs"]
        .as_array()?
        .iter()
        .filter(|r| r["status"] == "complete")
        .filter_map(|r| stamp_of(home, "run finishedAt", r["finishedAt"].as_str()?))
        .filter(|t| t.to_zoned_datetime(tz.clone()).date() < today)
        .max()
}

/// The digest as Today reads it, or `None` when there is no boundary.
/// `brief` and `herdr` are the current actions and the hires Herdr says
/// wait on the person; neither is time-filtered.
pub fn build(
    home: &Path,
    ws: &Path,
    now: Timestamp,
    brief: Option<&Brief>,
    herdr: &[Value],
) -> Option<Value> {
    let since = boundary(home, ws, now)?;
    let since_day = since.to_zoned_datetime(zone(ws)).date().to_string();
    let mut items = Vec::new();
    if let Some(b) = brief {
        items.extend(b.needs_you.iter().map(|a| Item {
            lane: Lane::Decisions,
            title: a.recommendation.clone(),
            source: a.source.clone(),
            at: None,
        }));
    }
    items.extend(herdr.iter().map(|h| Item {
        lane: Lane::Decisions,
        title: h["title"].as_str().unwrap_or_default().to_owned(),
        source: String::new(),
        at: None,
    }));
    ledger_items(home, since, &mut items);
    incident_items(home, ws, since, &mut items);
    last_run_item(home, &mut items);
    merged_items(home, since, &since_day, &mut items);
    Some(lanes_json(since, items))
}

fn stamp_of(home: &Path, what: &str, raw: &str) -> Option<Timestamp> {
    raw.parse().ok().or_else(|| {
        let _ = claude::audit_note(
            &claude::audit_path(home),
            "digest",
            json!({ "unparsed": what, "value": raw }),
        );
        None
    })
}

fn ledger_items(home: &Path, since: Timestamp, items: &mut Vec<Item>) {
    for row in ledger::read(home) {
        let Some(at) = stamp_of(home, "ledger judged", &row.judged) else {
            continue;
        };
        if at <= since {
            continue;
        }
        let lane = match (row.outcome, row.status) {
            (Outcome::NeedsYou, _) => Lane::Decisions,
            (Outcome::Rejected, _) | (_, Status::Failed | Status::Lost | Status::TimedOut) => {
                Lane::Errors
            }
            (Outcome::Accepted, _) => Lane::Done,
        };
        let title = agents::read(home, &row.id)
            .ok()
            .map(|(spec, _)| spec.title)
            .filter(|t| !t.trim().is_empty())
            .or_else(|| row.title.clone().filter(|t| !t.trim().is_empty()))
            .unwrap_or_else(|| "An agent run".to_owned());
        items.push(Item {
            lane,
            title,
            source: row
                .role
                .as_deref()
                .map_or_else(|| "agent run".to_owned(), |r| format!("{r} agent")),
            at: Some(at),
        });
    }
}

fn incident_items(home: &Path, ws: &Path, since: Timestamp, items: &mut Vec<Item>) {
    let Some(st) = state::read(ws).ok().flatten() else {
        return;
    };
    for inc in st["incidents"].as_array().into_iter().flatten() {
        let Some(at) = inc["observedAt"]
            .as_str()
            .and_then(|raw| stamp_of(home, "incident observedAt", raw))
        else {
            continue;
        };
        if at <= since {
            continue;
        }
        let source = inc["sourceId"].as_str().unwrap_or_default().to_owned();
        let kind = inc["kind"].as_str().unwrap_or("incident");
        items.push(Item {
            lane: Lane::Errors,
            title: format!("{kind} on {source}"),
            source,
            at: Some(at),
        });
    }
}

fn last_run_item(home: &Path, items: &mut Vec<Item>) {
    let Ok(Some(saved)) = store::latest::<RunReceipt>(home, Kind::Runs) else {
        return;
    };
    let r = saved.value;
    if r.status != RunStatus::Failed {
        return;
    }
    let Some(at) = stamp_of(home, "run at", &r.at) else {
        return;
    };
    let reason = r.reason.trim();
    let title = if reason.is_empty() {
        "Last brief run failed".to_owned()
    } else {
        format!("Last brief run failed: {reason}")
    };
    items.push(Item {
        lane: Lane::Errors,
        title,
        source: String::new(),
        at: Some(at),
    });
}

/// A merged PR counts once its `merged_at` is after the boundary. A row
/// archived before `merged_at` existed counts by its day instead: a later day
/// than the boundary's, never the boundary's own.
fn merged_items(home: &Path, since: Timestamp, since_day: &str, items: &mut Vec<Item>) {
    for m in history::load(home).merged {
        let at = match m.merged_at.as_deref() {
            Some(raw) => match stamp_of(home, "merged_at", raw) {
                Some(t) => Some(t),
                None => continue,
            },
            None => None,
        };
        let counted = match at {
            Some(t) => t > since,
            None => m.day.as_str() > since_day,
        };
        if counted {
            items.push(Item {
                lane: Lane::Done,
                title: m.title,
                source: format!("gh:{}#{}", m.repo, m.number),
                at,
            });
        }
    }
}

fn lanes_json(since: Timestamp, items: Vec<Item>) -> Value {
    let mut lanes = serde_json::Map::new();
    for lane in Lane::ALL {
        let mut mine: Vec<&Item> = items.iter().filter(|i| i.lane == lane).collect();
        mine.sort_by_key(|i| std::cmp::Reverse(i.at));
        let rows: Vec<Value> = mine
            .into_iter()
            .map(|i| {
                json!({
                    "title": i.title,
                    "source": i.source,
                    "at": i.at.map(|t| t.to_string()),
                })
            })
            .collect();
        lanes.insert(lane.key().to_owned(), Value::Array(rows));
    }
    json!({ "since": since.to_string(), "lanes": lanes })
}

/// The digest section of Today; empty when the data holds no digest.
pub fn section(data: &Value) -> String {
    let Some(d) = data.get("digest").filter(|d| d.is_object()) else {
        return String::new();
    };
    let cx = Ctx::new(data);
    let now = data["now_ts"]
        .as_str()
        .and_then(|t| t.parse::<Timestamp>().ok())
        .unwrap_or_else(Timestamp::now);
    let since = d["since"]
        .as_str()
        .and_then(|s| s.parse::<Timestamp>().ok());
    let total: usize = Lane::ALL.iter().map(|l| lane_items(d, *l).len()).sum();
    let since_text = d["since"]
        .as_str()
        .map(|s| {
            format!(
                r#"<p class="kit-digest__since">since {}</p>"#,
                today::esc(&cx.stamp(s))
            )
        })
        .unwrap_or_default();
    let mut s = format!(
        r#"<section class="kit-digest" data-section="digest" aria-labelledby="digest-h"><div class="kit-digest__head"><h2 class="kit-digest__heading" id="digest-h">Since your last brief <span class="count num">{total}</span></h2>{since_text}</div>"#
    );
    if let Some(since) = since {
        s.push_str(&chart(d, since, now));
    }
    s.push_str(r#"<div class="kit-digest__lanes">"#);
    for lane in Lane::ALL {
        lane_section(&cx, d, lane, &mut s);
    }
    s.push_str("</div></section>");
    s
}

fn lane_items(d: &Value, lane: Lane) -> &[Value] {
    d["lanes"][lane.key()]
        .as_array()
        .map_or(&[][..], Vec::as_slice)
}

fn lane_section(cx: &Ctx, d: &Value, lane: Lane, s: &mut String) {
    let items = lane_items(d, lane);
    let key = lane.key();
    let _ = write!(
        s,
        r#"<section class="kit-digest__lane" data-lane="{key}" aria-labelledby="digest-{key}-h"><h3 class="kit-digest__lane-title" id="digest-{key}-h">{} <span class="count num">{}</span></h3>"#,
        lane.label(),
        items.len()
    );
    if items.is_empty() {
        let _ = write!(s, r#"<p class="kit-digest__empty">{}</p>"#, lane.empty());
    } else {
        s.push_str(r#"<ol class="kit-digest__cards">"#);
        for item in items.iter().take(LANE_SHOWN) {
            card(cx, item, s);
        }
        s.push_str("</ol>");
        if items.len() > LANE_SHOWN {
            let _ = write!(
                s,
                r#"<p class="kit-digest__more">and {} more</p>"#,
                items.len() - LANE_SHOWN
            );
        }
    }
    s.push_str("</section>");
}

fn card(cx: &Ctx, item: &Value, s: &mut String) {
    let title = item["title"].as_str().unwrap_or_default();
    let source = item["source"].as_str().unwrap_or_default();
    let when = match item["at"].as_str() {
        Some(at) => format!(
            r#"<time class="kit-digest__time" datetime="{}">{}</time>"#,
            today::esc(at),
            today::esc(&cx.stamp(at))
        ),
        None => String::new(),
    };
    let _ = write!(
        s,
        r#"<li class="kit-digest__card"><p class="kit-digest__title">{}</p><p class="kit-digest__meta">{when}{}</p></li>"#,
        today::prose(title),
        today::chips(source)
    );
}

/// Cumulative count of the timed items from the boundary to now, drawn as a
/// trace over an area, as the kit's digest chart is.
fn chart(d: &Value, since: Timestamp, now: Timestamp) -> String {
    let mut timed: Vec<Timestamp> = Lane::ALL
        .iter()
        .flat_map(|l| lane_items(d, *l))
        .filter_map(|i| i["at"].as_str()?.parse().ok())
        .collect();
    if timed.is_empty() {
        return String::new();
    }
    timed.sort();
    let span = (now.as_second() - since.as_second()).max(1) as f64;
    let n = timed.len() as f64;
    let points: Vec<(f64, f64)> = timed
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let at = (t.as_second() - since.as_second()) as f64;
            let x = (at / span).clamp(0.0, 1.0) * CHART_W;
            (x, CHART_H - (i + 1) as f64 / n * CHART_H)
        })
        .collect();
    let line = points
        .iter()
        .enumerate()
        .map(|(i, (x, y))| format!("{} {x:.1} {y:.1}", if i == 0 { "M" } else { "L" }))
        .collect::<Vec<_>>()
        .join(" ");
    let last_x = points.last().map_or(0.0, |p| p.0);
    let first_x = points.first().map_or(0.0, |p| p.0);
    let area = format!("{line} L {last_x:.1} {CHART_H:.1} L {first_x:.1} {CHART_H:.1} Z");
    format!(
        r#"<svg class="kit-digest__chart" viewBox="0 0 {CHART_W} {CHART_H}" preserveAspectRatio="none" aria-hidden="true"><path class="kit-digest__area" d="{area}"/><path class="kit-digest__trace" d="{line}"/></svg>"#
    )
}
