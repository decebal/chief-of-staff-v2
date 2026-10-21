//! The HTML report: both brains, the archive, diagrams, charts, and a
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
use crate::reports::{Brief, Change, ChangeKind, Milestone, Review};
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
    let review = store::latest::<Review>(home, Kind::Reviews)?;
    let mut upcoming: Vec<&Milestone> = brief
        .iter()
        .flat_map(|b| &b.value.upcoming)
        .chain(review.iter().flat_map(|r| &r.value.upcoming))
        .collect();
    upcoming.sort_by(|a, b| a.date.cmp(&b.date));
    upcoming.dedup_by(|a, b| a.date == b.date && a.what == b.what);
    let today = now.date().to_string();
    upcoming.retain(|m| m.date >= today);

    let diagrams = json!({
        "timeline": (!upcoming.is_empty()).then(|| timeline(&upcoming)),
        "flywheel": flywheel(brief.as_ref().map(|s| &s.value), review.as_ref().map(|s| &s.value)),
        "architecture": architecture(),
    });

    Ok(json!({
        "generated_at": now.strftime("%Y-%m-%d %H:%M %Z").to_string(),
        "today": today,
        "now": now.strftime("%H:%M").to_string(),
        "live": live,
        "name": profile_name(home, ws),
        "brief": brief.map(|s| json!({ "day": s.day, "report": s.value })),
        "review": review.map(|s| json!({ "day": s.day, "report": s.value })),
        "facts": store::latest::<Facts>(home, Kind::Facts)?.map(|s| s.value),
        "history": history::load(home),
        "backfilled": history::backfilled(home),
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
    let handoffs: serde_json::Map<String, Value> = crate::state::open(&st, "deliverables")
        .filter_map(|d| {
            let id = d["id"].as_str()?;
            let text = crate::handoff::deliverable(
                &st,
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
        "kpis": st["kpis"],
        "deliverables": st["deliverables"],
        "commitments": st["commitments"],
        "sources": st["sources"],
        "incidents": st["incidents"],
        "runs": st["runs"],
        "handoffs": handoffs,
        "tracking": profile.as_ref().is_some_and(crate::state::tracking_approved),
    })
}

/// Embeds `data` and the Markdy bundle in the template. `</` is escaped so no
/// string in the data can close the `<script>` element early.
pub fn render(data: &Value) -> Result<String> {
    anyhow::ensure!(
        TEMPLATE.contains(DATA_SLOT) && TEMPLATE.contains(MARKDY_SLOT),
        "report template lost a slot"
    );
    let json = serde_json::to_string(data)?.replace("</", "<\\/");
    Ok(TEMPLATE
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

pub fn flywheel(brief: Option<&Brief>, review: Option<&Review>) -> String {
    let goals = review.map_or("Goals: run cos review".to_owned(), |r| {
        let n = |c: &str| {
            r.goals
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
    let done = |c: &[Change]| c.iter().filter(|c| c.kind == ChangeKind::Done).count();
    let wins = brief.map(|b| done(&b.what_changed)).unwrap_or(0)
        + review.map(|r| done(&r.what_changed)).unwrap_or(0);
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
     service Tactician \"Left brain: tactician\"\n\
     service Strategist \"Right brain: strategist\"\n\
     client You \"You\"\n\
     beat flow \"How the chief of staff works\":\n  \
     Local -> Tactician \"facts\"\n  \
     Connectors -> Tactician \"today\"\n  \
     Local -> Strategist \"history\"\n  \
     Connectors -> Strategist \"goals\"\n  \
     Tactician -> You \"brief + prompts\"\n  \
     Strategist -> You \"review + bets\"\n"
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
