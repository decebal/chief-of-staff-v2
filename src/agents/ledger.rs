//! Every judged agent, one JSON line each in `out/agents/ledger.jsonl`. It
//! outlives `cos agents clean`, so the scorecard covers agents whose
//! worktrees are gone. Every number in it comes from cos and the judge,
//! never from an agent's own words. Runs in Herdr carry no cost and no tool
//! count: Herdr does not measure them, and a missing number is never 0.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::judge::Outcome;
use super::{Harness, Spec, State, Status, root};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Row {
    pub id: String,
    pub judged: String,
    pub harness: Harness,
    pub model: Option<String>,
    /// The repo directory name; `None` for a scratch agent.
    pub repo: Option<String>,
    /// Where the prompt came from: `brief`, `review`, `deliverable` or `prompt`.
    pub kind: String,
    pub status: Status,
    pub outcome: Outcome,
    pub overclaimed: bool,
    pub checks: usize,
    /// Measured before hires ran in Herdr only; `None` for a run in Herdr.
    pub cost_usd: Option<f64>,
    pub secs: Option<u64>,
    #[serde(default)]
    pub tool_calls: Option<u32>,
    /// Ran in Herdr.
    #[serde(default)]
    pub herdr: bool,
    /// The staff role; `None` for rows written before roles.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// The spec's title; `None` for rows written before titles.
    #[serde(default)]
    pub title: Option<String>,
}

pub fn path(home: &Path) -> PathBuf {
    root(home).join("ledger.jsonl")
}

pub fn row(spec: &Spec, state: &State) -> Option<Row> {
    let v = state.verdict.as_ref()?;
    Some(Row {
        id: spec.id.clone(),
        judged: v.judged.clone(),
        harness: spec.harness,
        model: spec.model.clone(),
        repo: spec
            .repo
            .as_deref()
            .and_then(Path::file_name)
            .map(|n| n.to_string_lossy().into_owned()),
        kind: spec
            .source
            .split_whitespace()
            .next()
            .unwrap_or("prompt")
            .to_owned(),
        status: state.status,
        outcome: v.outcome,
        overclaimed: v.overclaimed,
        checks: v.checks.len(),
        cost_usd: state.cost_usd,
        secs: secs(state),
        tool_calls: None,
        herdr: !spec.legacy(),
        role: spec.role.clone(),
        title: Some(spec.title.clone()),
    })
}

/// One line per call, written in a single `write` on an append-only file,
/// so concurrent writers never interleave inside a line.
pub fn append(home: &Path, row: &Row) -> Result<()> {
    let mut line = serde_json::to_vec(row)?;
    line.push(b'\n');
    std::fs::create_dir_all(root(home))?;
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path(home))?
        .write_all(&line)?;
    Ok(())
}

/// Every row, the latest per agent (a re-judge replaces the first verdict).
/// Lines that do not parse are skipped.
pub fn read(home: &Path) -> Vec<Row> {
    let Ok(text) = std::fs::read_to_string(path(home)) else {
        return Vec::new();
    };
    let mut latest: BTreeMap<String, Row> = BTreeMap::new();
    for row in text
        .lines()
        .filter_map(|l| serde_json::from_str::<Row>(l).ok())
    {
        latest.insert(row.id.clone(), row);
    }
    latest.into_values().collect()
}

/// One line of the scorecard: how agents hired on the same terms did.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Score {
    pub role: Option<String>,
    pub harness: Harness,
    pub model: Option<String>,
    pub repo: Option<String>,
    pub runs: usize,
    pub accepted: usize,
    pub needs_you: usize,
    pub rejected: usize,
    pub overclaimed: usize,
    /// The median over the runs that measured a cost; `None` when none did.
    pub median_cost_usd: Option<f64>,
    /// Runs with no cost measured: every run in Herdr.
    pub unmeasured: usize,
    pub median_secs: Option<u64>,
}

/// Grouped by role, harness, model and repo; most runs first.
pub fn scorecard(rows: &[Row]) -> Vec<Score> {
    type Hire = (Option<String>, String, Option<String>, Option<String>);
    let mut groups: BTreeMap<Hire, Vec<&Row>> = BTreeMap::new();
    for r in rows {
        groups
            .entry((
                r.role.clone(),
                r.harness.program().to_owned(),
                r.model.clone(),
                r.repo.clone(),
            ))
            .or_default()
            .push(r);
    }
    let mut out: Vec<Score> = groups
        .into_values()
        .map(|rs| {
            let t = tally(&rs);
            Score {
                role: rs[0].role.clone(),
                harness: rs[0].harness,
                model: rs[0].model.clone(),
                repo: rs[0].repo.clone(),
                runs: rs.len(),
                accepted: t.accepted,
                needs_you: t.needs_you,
                rejected: t.rejected,
                overclaimed: t.overclaimed,
                median_cost_usd: t.median_cost_usd,
                unmeasured: t.unmeasured,
                median_secs: t.median_secs,
            }
        })
        .collect();
    out.sort_by_key(|a| std::cmp::Reverse(a.runs));
    out
}

/// One role's record across every hire: the role card's numbers.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RoleScore {
    /// `None` gathers the rows written before roles.
    pub role: Option<String>,
    pub runs: usize,
    pub accepted: usize,
    pub needs_you: usize,
    pub rejected: usize,
    pub overclaimed: usize,
    pub median_cost_usd: Option<f64>,
    pub unmeasured: usize,
    pub median_secs: Option<u64>,
}

/// Grouped by role alone, medians over every row of the role.
pub fn by_role(rows: &[Row]) -> Vec<RoleScore> {
    let mut groups: BTreeMap<Option<String>, Vec<&Row>> = BTreeMap::new();
    for r in rows {
        groups.entry(r.role.clone()).or_default().push(r);
    }
    groups
        .into_iter()
        .map(|(role, rs)| {
            let t = tally(&rs);
            RoleScore {
                role,
                runs: rs.len(),
                accepted: t.accepted,
                needs_you: t.needs_you,
                rejected: t.rejected,
                overclaimed: t.overclaimed,
                median_cost_usd: t.median_cost_usd,
                unmeasured: t.unmeasured,
                median_secs: t.median_secs,
            }
        })
        .collect()
}

struct Tally {
    accepted: usize,
    needs_you: usize,
    rejected: usize,
    overclaimed: usize,
    median_cost_usd: Option<f64>,
    unmeasured: usize,
    median_secs: Option<u64>,
}

fn tally(rs: &[&Row]) -> Tally {
    let count = |o: Outcome| rs.iter().filter(|r| r.outcome == o).count();
    let mut costs: Vec<f64> = rs.iter().filter_map(|r| r.cost_usd).collect();
    costs.sort_by(f64::total_cmp);
    let mut secs: Vec<u64> = rs.iter().filter_map(|r| r.secs).collect();
    secs.sort_unstable();
    Tally {
        accepted: count(Outcome::Accepted),
        needs_you: count(Outcome::NeedsYou),
        rejected: count(Outcome::Rejected),
        overclaimed: rs.iter().filter(|r| r.overclaimed).count(),
        median_cost_usd: median(&costs).copied(),
        unmeasured: rs.len() - costs.len(),
        median_secs: median(&secs).copied(),
    }
}

fn median<T>(sorted: &[T]) -> Option<&T> {
    sorted.get(sorted.len().checked_sub(1)? / 2)
}

/// From its start to its end, or to now for a run judged while still live.
fn secs(state: &State) -> Option<u64> {
    let start: jiff::Timestamp = state.started.as_deref()?.parse().ok()?;
    let end = match state.ended.as_deref() {
        Some(e) => e.parse().ok()?,
        None => jiff::Timestamp::now(),
    };
    let d: std::time::Duration = end.duration_since(start).try_into().ok()?;
    Some(d.as_secs())
}
