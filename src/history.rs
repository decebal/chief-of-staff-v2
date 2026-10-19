//! The archive that builds up over time: one metrics row per day, and every
//! merged PR seen. Both live in `out/history/` and only ever grow; a re-run
//! of the same day replaces that day's row.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::gather::{self, Facts, Pr};
use crate::reports::Brief;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct DayRow {
    pub date: String,
    pub review_queue: usize,
    pub mine_open: usize,
    /// Ready chronis tasks by priority (`p0`..`p3`).
    pub ready: BTreeMap<String, usize>,
    /// Brief-derived; kept from an earlier run that day when a review updates the row.
    /// Actions in the brief (`needs_you`); the name predates the kit's brief.
    #[serde(default)]
    pub must_do: Option<usize>,
    #[serde(default)]
    pub meeting_minutes: Option<u32>,
    #[serde(default)]
    pub wins: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Merged {
    pub repo: String,
    pub number: u64,
    pub title: String,
    /// `YYYY-MM-DD`.
    pub day: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct History {
    pub days: Vec<DayRow>,
    pub merged: Vec<Merged>,
}

fn dir(home: &Path) -> PathBuf {
    home.join("out/history")
}

pub fn load(home: &Path) -> History {
    let read = |name: &str| std::fs::read(dir(home).join(name)).ok();
    History {
        days: read("daily.json")
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default(),
        merged: read("merged.json")
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default(),
    }
}

/// Folds one run into the archive. The first run ever also seeds the merged
/// log with a year of history, so the calendar starts full rather than empty.
pub fn record(home: &Path, day: &str, facts: &Facts, brief: Option<&Brief>) -> Result<History> {
    let mut h = load(home);
    upsert_day(&mut h.days, row(day, facts, brief));

    let seed = if facts.unread.iter().any(|u| u == "code") {
        Vec::new()
    } else {
        backfill(home, day)
    };
    merge_prs(&mut h.merged, seed.iter().chain(&facts.merged));

    std::fs::create_dir_all(dir(home))?;
    std::fs::write(
        dir(home).join("daily.json"),
        serde_json::to_vec_pretty(&h.days)?,
    )?;
    std::fs::write(
        dir(home).join("merged.json"),
        serde_json::to_vec_pretty(&h.merged)?,
    )?;
    Ok(h)
}

/// `day`'s month so far, then the twelve whole months before it, as
/// `(from, to)` dates.
pub fn months_to(day: &str) -> Vec<(String, String)> {
    let Ok(d) = jiff::civil::Date::strptime("%Y-%m-%d", day) else {
        return vec![];
    };
    let first = d.first_of_month();
    let mut out = vec![(first.to_string(), d.to_string())];
    out.extend((1..=12).filter_map(|i| {
        let start = first.checked_sub(jiff::Span::new().months(i)).ok()?;
        Some((start.to_string(), start.last_of_month().to_string()))
    }));
    out
}

/// Months archived per run. A busy month is ~10 search pages, and GitHub's
/// secondary rate limit refused four months queried in parallel (HTTP 403),
/// so the year fills in over a few runs, one month at a time.
const MONTHS_PER_RUN: usize = 2;

/// Months (`YYYY-MM-01`) whose merged PRs are fully in the archive.
pub fn backfilled(home: &Path) -> Vec<String> {
    std::fs::read(dir(home).join("backfilled.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

/// Seeds merged PRs month by month. Months that succeed are recorded in
/// `backfilled.json`; the rest are picked up by later runs.
fn backfill(home: &Path, day: &str) -> Vec<Pr> {
    let done_path = dir(home).join("backfilled.json");
    let mut done: Vec<String> = std::fs::read(&done_path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    let todo: Vec<(String, String)> = months_to(day)
        .into_iter()
        .filter(|(from, _)| !done.contains(from))
        .collect();
    if todo.is_empty() {
        return vec![];
    }
    eprintln!(
        "cos: archiving merged PRs, {} month(s) left; {} this run",
        todo.len(),
        todo.len().min(MONTHS_PER_RUN)
    );
    let mut prs = Vec::new();
    for (from, to) in todo.into_iter().take(MONTHS_PER_RUN) {
        let mut gaps = Vec::new();
        let got = gather::merged_between(&from, &to, &mut gaps);
        if gaps.is_empty() {
            done.push(from);
            prs.extend(got);
        } else {
            for g in gaps {
                eprintln!(
                    "cos: backfill will retry next run: {}",
                    g.lines().next().unwrap_or(&g)
                );
            }
            break;
        }
    }
    let _ = std::fs::create_dir_all(dir(home));
    let _ = std::fs::write(
        &done_path,
        serde_json::to_vec_pretty(&done).unwrap_or_default(),
    );
    prs
}

pub fn row(day: &str, facts: &Facts, brief: Option<&Brief>) -> DayRow {
    let mut ready = BTreeMap::new();
    for t in &facts.tasks {
        *ready.entry(t.priority.clone()).or_insert(0) += 1;
    }
    DayRow {
        date: day.to_owned(),
        review_queue: facts.review_requested.len(),
        mine_open: facts.mine_open.len(),
        ready,
        must_do: brief.map(|b| b.needs_you.len()),
        meeting_minutes: brief.map(|b| {
            b.calendar
                .iter()
                .filter_map(|s| minutes(&s.start, &s.end))
                .sum()
        }),
        wins: brief.map(|b| {
            b.what_changed
                .iter()
                .filter(|c| c.kind == crate::reports::ChangeKind::Done)
                .count()
        }),
    }
}

/// Replaces the row for the same date, keeping brief-derived fields the new
/// row does not carry, and keeps rows in date order.
pub fn upsert_day(days: &mut Vec<DayRow>, new: DayRow) {
    match days.iter_mut().find(|d| d.date == new.date) {
        Some(old) => {
            let keep = old.clone();
            *old = new;
            old.must_do = old.must_do.or(keep.must_do);
            old.meeting_minutes = old.meeting_minutes.or(keep.meeting_minutes);
            old.wins = old.wins.or(keep.wins);
        }
        None => days.push(new),
    }
    days.sort_by(|a, b| a.date.cmp(&b.date));
}

/// Adds PRs not already present (by repo and number), in day order.
pub fn merge_prs<'a>(log: &mut Vec<Merged>, prs: impl Iterator<Item = &'a Pr>) {
    for p in prs {
        let Some(day) = p.closed.as_deref().and_then(|c| c.get(..10)) else {
            continue;
        };
        if log.iter().any(|m| m.repo == p.repo && m.number == p.number) {
            continue;
        }
        log.push(Merged {
            repo: p.repo.clone(),
            number: p.number,
            title: p.title.clone(),
            day: day.to_owned(),
        });
    }
    log.sort_by(|a, b| a.day.cmp(&b.day).then(a.number.cmp(&b.number)));
}

fn minutes(start: &str, end: &str) -> Option<u32> {
    let m = |t: &str| {
        let (h, mm) = t.split_once(':')?;
        Some(h.trim().parse::<u32>().ok()? * 60 + mm.get(..2)?.parse::<u32>().ok()?)
    };
    let (a, b) = (m(start)?, m(end)?);
    (b > a).then(|| b - a)
}
