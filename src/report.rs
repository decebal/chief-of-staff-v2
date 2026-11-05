//! The HTML report: the brief and its week, the archive, diagrams, charts, and a
//! copyable agent prompt per action. One self-contained file under `out/`,
//! opened after every run, and the page `cos dash` serves live.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result};
use serde_json::{Value, json};

use crate::gather::Facts;
use crate::history;
use crate::proc;
use crate::reports::{Brief, ChangeKind, Milestone};
use crate::store::{self, Kind};

const TEMPLATE: &str = include_str!("report.html");
const MARKDY: &str = include_str!("../vendor/markdy/markdy-1.5.7.iife.min.js");
const DATA_SLOT: &str = "\"__COS_DATA__\"";
const MARKDY_SLOT: &str = "/*__MARKDY__*/";

/// Writes `out/reports/<day>.html` and `out/report.html` (always the latest).
pub fn build(home: &Path, ws: &Path) -> Result<PathBuf> {
    let html = render(&data(home, ws, false)?)?;
    let day = jiff::Zoned::now().date().to_string();
    let dir = home.join("out/reports");
    std::fs::create_dir_all(&dir)?;
    let dated = dir.join(format!("{day}.html"));
    std::fs::write(&dated, &html)?;
    std::fs::write(home.join("out/report.html"), &html)?;
    Ok(dated)
}

/// Everything the page shows. `live` makes the page poll `cos dash` for changes.
pub fn data(home: &Path, ws: &Path, live: bool) -> Result<Value> {
    let now = jiff::Zoned::now();
    let brief = store::latest::<Brief>(home, Kind::Briefs)?;
    let mut upcoming: Vec<&Milestone> = brief.iter().flat_map(|b| &b.value.upcoming).collect();
    upcoming.sort_by(|a, b| a.date.cmp(&b.date));
    upcoming.dedup_by(|a, b| a.date == b.date && a.what == b.what);
    let today = now.date().to_string();
    upcoming.retain(|m| m.date >= today);

    let diagrams = json!({
        "timeline": (!upcoming.is_empty()).then(|| timeline(&upcoming)),
        "flywheel": flywheel(brief.as_ref().map(|s| &s.value)),
        "architecture": architecture(),
    });

    let run = brief.as_ref().and_then(|b| brief_run(home, &b.day));
    Ok(json!({
        "generated_at": now.strftime("%Y-%m-%d %H:%M %Z").to_string(),
        "today": today,
        "now": now.strftime("%H:%M").to_string(),
        "now_ts": now.timestamp().to_string(),
        "run": run,
        "last_run": store::latest::<crate::pipeline::RunReceipt>(home, Kind::Runs)
            .ok()
            .flatten()
            .map(|s| s.value),
        "live": live,
        "name": profile_name(home, ws),
        "brief": brief.map(|s| json!({ "day": s.day, "report": s.value })),
        "facts": store::latest::<Facts>(home, Kind::Facts)?.map(|s| s.value),
        "history": history::load(home),
        "backfilled": history::backfilled(home),
        "work_orgs": crate::context::work_orgs(ws),
        "archive": archive(home),
        "diagrams": diagrams,
        "workspace": workspace(ws),
    }))
}

/// The plan view: the state's records and a ready handoff per open
/// deliverable. `null` without a workspace; an `error` when it fails to load.
fn workspace(ws: &Path) -> Value {
    let st = match crate::state::read(ws) {
        Ok(Some(st)) => st,
        Ok(None) => return Value::Null,
        Err(e) => return json!({ "error": format!("{e:#}") }),
    };
    let profile = crate::state::profile(ws);
    let mut runs = st["runs"].as_array().cloned().unwrap_or_default();
    runs.sort_by_key(|run| {
        run["finishedAt"]
            .as_str()
            .or_else(|| run["invokedAt"].as_str())
            .and_then(|at| at.parse::<jiff::Timestamp>().ok())
    });
    let kpis: Vec<_> = st["kpis"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|k| {
            k["status"] != "retired"
                && k["targetBasis"] != "retired"
                && k["excludeFromActiveScore"] != true
        })
        .collect();
    let mut sources = st["sources"].as_array().cloned().unwrap_or_default();
    for source in &mut sources {
        if let Some((run, coverage)) = runs.iter().rev().find_map(|run| {
            run["coverage"].as_array()?.iter().find_map(|coverage| {
                (coverage["source"] == source["id"]).then_some((run, coverage))
            })
        }) {
            let at = run["finishedAt"]
                .as_str()
                .or_else(|| run["invokedAt"].as_str());
            let received = at.and_then(|v| v.parse::<jiff::Timestamp>().ok());
            let checked = source["checkedAt"]
                .as_str()
                .and_then(|v| v.parse::<jiff::Timestamp>().ok());
            if received < checked {
                continue;
            }
            for key in ["health", "review", "note"] {
                if !coverage[key].is_null() {
                    source[key] = coverage[key].clone();
                }
            }
            if let Some(at) = at {
                source["checkedAt"] = json!(at);
            }
        }
    }
    let mut handoff_state = st.clone();
    handoff_state["kpis"] = json!(
        kpis.iter()
            .filter(|k| k["status"] != "proposed" && k["targetBasis"] != "proposed")
            .collect::<Vec<_>>()
    );
    let handoffs: serde_json::Map<String, Value> = crate::state::open(&st, "deliverables")
        .filter_map(|d| {
            let id = d["id"].as_str()?;
            let text = crate::handoff::deliverable(
                &handoff_state,
                profile.as_ref(),
                id,
                crate::handoff::Receiver::Chat,
            )
            .ok()?;
            Some((id.to_owned(), json!(text)))
        })
        .collect();
    json!({
        "revision": st["revision"],
        "goals": st["goals"],
        "kpis": kpis,
        "deliverables": st["deliverables"],
        "commitments": st["commitments"],
        "sources": sources,
        "incidents": st["incidents"],
        "runs": runs,
        "timezone": profile.as_ref().and_then(|p| p["timezone"].as_str()),
        "dashboardTitle": profile.as_ref().and_then(|p| p["dashboardTitle"].as_str()),
        "portfolio": st["commitments"].as_array().and_then(|items| items.iter().find_map(|item| item.get("dashboard")?.get("portfolio"))),
        "staff": st["commitments"].as_array().and_then(|items| items.iter().find_map(|item| item.get("dashboard")?.get("staff"))),
        "schedule": profile.as_ref().map(|p| &p["schedule"]),
        "handoffs": handoffs,
        "tracking": profile.as_ref().is_some_and(crate::state::tracking_approved),
    })
}

/// When the saved brief was written and what its run cost: the brief file's
/// time, and the last tactician outcome in that day's audit log at or before
/// it (an `ask` later the same day is not the brief).
fn brief_run(home: &Path, day: &str) -> Option<Value> {
    let saved: jiff::Timestamp = std::fs::metadata(home.join(format!("out/briefs/{day}.json")))
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| jiff::Timestamp::try_from(t).ok())?;
    let audit = std::fs::read_to_string(home.join(format!("out/audit/{day}.jsonl"))).ok();
    let outcome = audit.as_deref().and_then(|text| {
        text.lines().rev().find_map(|line| {
            let v: Value = serde_json::from_str(line).ok()?;
            let at: jiff::Timestamp = v["at"].as_str()?.parse().ok()?;
            (v["agent"] == "tactician"
                && v.get("result").is_some()
                && at <= saved + jiff::SignedDuration::from_secs(5))
            .then(|| (at, v["cost_usd"].as_f64()))
        })
    });
    Some(match outcome {
        Some((at, cost)) => json!({ "at": at.to_string(), "cost_usd": cost }),
        None => json!({ "at": saved.to_string(), "cost_usd": null }),
    })
}

const HERO_SLOT: &str = "<!--__COS_HERO__-->";
const TODAY_SLOT: &str = "<!--__COS_TODAY__-->";
const TITLE: &str = "<title>Chief of staff</title>";

/// The page title: the profile's dashboard title, else "<first name>'s chief of staff".
pub fn title(data: &Value) -> String {
    data["workspace"]["dashboardTitle"]
        .as_str()
        .filter(|t| !t.trim().is_empty())
        .map(str::to_owned)
        .or_else(|| {
            data["name"]
                .as_str()
                .and_then(|n| n.split_whitespace().next())
                .map(|f| format!("{f}'s chief of staff"))
        })
        .unwrap_or_else(|| "Chief of staff".to_owned())
}

/// Embeds `data` and the Markdy bundle in the template, and the Today view
/// rendered from the same data. `</` is escaped so no string in the data can
/// close the `<script>` element early. The view goes into the markup before
/// the first `<script>` and the bundle and data after it, so text in the view
/// can never be taken for a slot.
pub fn render(data: &Value) -> Result<String> {
    anyhow::ensure!(
        [DATA_SLOT, MARKDY_SLOT, HERO_SLOT, TODAY_SLOT, TITLE]
            .iter()
            .all(|s| TEMPLATE.contains(s)),
        "report template lost a slot"
    );
    let page = crate::design::inline(TEMPLATE)?;
    let cut = page
        .find("<script>")
        .context("report template has no script")?;
    let (markup, scripts) = page.split_at(cut);
    let title = crate::today::esc(&title(data));
    let markup = markup
        .replacen(TITLE, &format!("<title>{title}</title>"), 1)
        .replacen("<!--__COS_TITLE__-->", &title, 1)
        .replacen(HERO_SLOT, &crate::today::hero(data), 1)
        .replacen(TODAY_SLOT, &crate::today::body(data), 1);
    let json = serde_json::to_string(data)?.replace("</", "<\\/");
    Ok(markup
        + &scripts
            .replacen(MARKDY_SLOT, MARKDY, 1)
            .replacen(DATA_SLOT, &json, 1))
}

pub fn open(path: &Path) -> Result<()> {
    let mut cmd = Command::new("open");
    cmd.arg(path);
    let out = proc::run(&mut cmd, Duration::from_secs(10)).context("run `open`")?;
    anyhow::ensure!(out.status.success(), "open failed: {}", out.stderr.trim());
    Ok(())
}

/// Dated reports on disk, newest first.
fn archive(home: &Path) -> Vec<String> {
    let mut days: Vec<String> = std::fs::read_dir(home.join("out/reports"))
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|e| {
            e.file_name()
                .to_str()?
                .strip_suffix(".html")
                .map(str::to_owned)
        })
        .collect();
    days.sort_by(|a, b| b.cmp(a));
    days
}

// Diagram sources are generated here from data, never written by the model:
// Markdy's grammar moves fast, and a fixed subset (node kinds `service`,
// `client`, `database`, `hub`, `station`; flows only inside a beat) is what
// the vendored 1.5.7 was verified against. `__THEME__` is filled per mode.

/// Markdy labels are double-quoted and line-based.
pub fn label(s: &str) -> String {
    let flat: String = s
        .chars()
        .map(|c| {
            if c == '"' {
                '\''
            } else if c.is_control() {
                ' '
            } else {
                c
            }
        })
        .collect();
    let flat = flat.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() > 44 {
        format!("{}…", flat.chars().take(43).collect::<String>())
    } else {
        flat
    }
}

pub fn timeline(upcoming: &[&Milestone]) -> String {
    let mut s = String::from("scene theme=__THEME__ type=timeline\n");
    for (i, m) in upcoming.iter().take(6).enumerate() {
        let date = m.date.get(5..).unwrap_or(&m.date);
        let focal = if i == 0 { " focal=true" } else { "" };
        s.push_str(&format!(
            "service M{i} \"{}\"{focal}\n",
            label(&format!("{date} · {}", m.what))
        ));
    }
    s.push_str("beat reveal \"Coming up\":\n  show $nodes stagger=60ms\n");
    s
}

pub fn flywheel(brief: Option<&Brief>) -> String {
    let week = brief.map(|b| &b.week).filter(|w| !w.goals.is_empty());
    let goals = week.map_or("Goals: run cos brief".to_owned(), |w| {
        let n = |c: &str| {
            w.goals
                .iter()
                .filter(|g| g.confidence.to_string() == c)
                .count()
        };
        format!(
            "Goals: {} rock · {} sand · {} water",
            n("rock"),
            n("sand"),
            n("water")
        )
    });
    let actions = brief.map_or("Today: run cos brief".to_owned(), |b| {
        format!("Today: {} need you", b.needs_you.len())
    });
    let wins = brief.map_or(0, |b| {
        b.what_changed
            .iter()
            .filter(|c| c.kind == ChangeKind::Done)
            .count()
    });
    format!(
        "scene theme=__THEME__ type=loop\n\
         hub Goals \"Goals → KPIs\"\n\
         station Bets \"{}\"\n\
         station Actions \"{}\" focal=true\n\
         station Wins \"{}\"\n\
         beat cycle \"Goals flywheel\":\n  \
         Bets -> Actions \"this week's deliverables\"\n  \
         Actions -> Wins \"done, with evidence\"\n  \
         Wins -> Bets \"KPI evidence\"\n",
        label(&goals),
        label(&actions),
        label(&format!("Done: {wins} verified"))
    )
}

pub fn architecture() -> String {
    "scene theme=__THEME__ type=architecture\n\
     layout LR\n\
     database Local \"GitHub · chronis\"\n\
     service Connectors \"Calendar · Gmail · Jira · Notion\"\n\
     service Tactician \"Daily brief: tactician\"\n\
     client You \"You\"\n\
     beat flow \"How the chief of staff works\":\n  \
     Local -> Tactician \"facts\"\n  \
     Connectors -> Tactician \"goals · week · today\"\n  \
     Tactician -> You \"brief + prompts\"\n"
        .to_owned()
}

/// The workspace profile's `owner`, else the `**Name**:` line of
/// `context/profile.md`, if either is filled in.
fn profile_name(home: &Path, ws: &Path) -> Option<String> {
    if let Some(owner) = crate::state::profile(ws)
        .and_then(|p| p["owner"].as_str().map(str::to_owned))
        .filter(|o| !o.trim().is_empty())
    {
        return Some(owner);
    }
    let profile = std::fs::read_to_string(home.join("context/profile.md")).ok()?;
    profile
        .lines()
        .find_map(|l| l.trim_start_matches("- ").strip_prefix("**Name**:"))
        .map(|n| n.trim().to_owned())
        .filter(|n| !n.is_empty())
}
