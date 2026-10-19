//! What `cos` writes to the workspace state, always through `state::commit`.
//!
//! A brief or review records its run receipt, each profile source's health and
//! coverage, and an incident per new access failure. Only when the profile
//! approves local tracking. The model's word that a source was read in full
//! is testimony, so `cos` records at most `partial` and never advances a
//! review watermark; the user confirms completeness.
//!
//! Review proposals are never committed here: `proposal_candidate` writes a
//! candidate file the user reads and commits with `cos state commit`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::{Value, json};

use crate::reports::{Completeness, Coverage, Health, Proposal, snake};
use crate::state;

#[derive(Debug, PartialEq, Eq)]
pub enum Recorded {
    NoWorkspace,
    NotApproved,
    Committed { revision: i64 },
}

impl std::fmt::Display for Recorded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoWorkspace => f.write_str("not tracked: no .chief-of-staff workspace"),
            Self::NotApproved => {
                f.write_str("not tracked: local tracking is not approved in the profile")
            }
            Self::Committed { revision } => write!(f, "tracked: state revision {revision}"),
        }
    }
}

fn incident_kind(h: Health) -> Option<&'static str> {
    match h {
        Health::RateLimited => Some("rate_limit"),
        Health::PermissionDenied => Some("permission_denied"),
        Health::AuthFailed => Some("auth_failed"),
        _ => None,
    }
}

fn window(c: &Coverage) -> Option<(String, String)> {
    let (a, b) = (json!(c.window_start), json!(c.window_end));
    let (ta, tb) = (
        state::timestamp(Some(&a)).ok()?,
        state::timestamp(Some(&b)).ok()?,
    );
    (ta <= tb).then(|| (c.window_start.clone(), c.window_end.clone()))
}

/// Records one run. `mode` is `brief` or `review`; times are ISO-8601.
pub fn record_run(
    home: &Path,
    mode: &str,
    coverage: &[Coverage],
    gaps: usize,
    invoked_at: &str,
    finished_at: &str,
) -> Result<Recorded> {
    let Some(profile) = state::profile(home) else {
        return Ok(Recorded::NoWorkspace);
    };
    if !state::tracking_approved(&profile) {
        return Ok(Recorded::NotApproved);
    }
    let ws = std::path::absolute(home)?;
    let current = state::read(&ws)?.context("profile.json without state.json")?;
    let revision = current["revision"].as_i64().unwrap_or(0);
    let mut cand = current.clone();

    let profile_ids: Vec<String> = profile["sources"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|s| s["id"].as_str().map(str::to_owned))
        .collect();
    let complete = !coverage.is_empty() && gaps == 0 && coverage.iter().all(Coverage::complete);
    let invoked_ms = state::timestamp(Some(&json!(invoked_at)))?.as_millisecond();
    let runs = cand["runs"].as_array_mut().context("runs")?;
    let base = format!("R-{mode}-{invoked_ms}");
    let id = (1..)
        .map(|n| {
            if n == 1 {
                base.clone()
            } else {
                format!("{base}-{n}")
            }
        })
        .find(|id| !runs.iter().any(|r| r["id"] == json!(id)))
        .unwrap_or(base);
    runs.push(json!({
        "id": id,
        "invokedAt": invoked_at,
        "finishedAt": finished_at,
        "mode": mode,
        "status": if complete { "complete" } else { "partial" },
        "coverage": coverage,
        "notificationDecision": "none",
    }));

    for c in coverage.iter().filter(|c| profile_ids.contains(&c.source)) {
        let review = match c.review {
            Completeness::NotReviewed => "not_reviewed",
            _ => "partial",
        };
        let mut note = c.note.clone();
        if c.review == Completeness::Complete {
            note = format!("{note} (reported complete; confirm before advancing the watermark)")
                .trim()
                .to_owned();
        }
        let sources = cand["sources"].as_array_mut().context("sources")?;
        let at = match sources.iter().position(|s| s["id"] == json!(c.source)) {
            Some(i) => i,
            None => {
                sources.push(json!({
                    "id": c.source,
                    "lastCompleteThrough": null,
                    "windowStart": null,
                    "windowEnd": null,
                }));
                sources.len() - 1
            }
        };
        let s = &mut sources[at];
        s["health"] = json!(snake(&c.health));
        s["review"] = json!(review);
        s["checkedAt"] = json!(finished_at);
        s["evidenceAt"] = json!(finished_at);
        s["note"] = json!(note);
        if let Some((start, end)) = window(c) {
            s["windowStart"] = json!(start);
            s["windowEnd"] = json!(end);
        }

        if let Some(kind) = incident_kind(c.health) {
            let incidents = cand["incidents"].as_array_mut().context("incidents")?;
            let open = incidents.iter().any(|i| {
                i["sourceId"] == json!(c.source)
                    && i["kind"] == json!(kind)
                    && i["resolvedAt"].is_null()
            });
            if !open {
                let n = incidents
                    .iter()
                    .filter_map(|i| i["id"].as_str()?.strip_prefix("I-")?.parse::<u64>().ok())
                    .max()
                    .unwrap_or(0)
                    + 1;
                incidents.push(json!({
                    "id": format!("I-{n}"),
                    "sourceId": c.source,
                    "kind": kind,
                    "observedAt": finished_at,
                    "retryNotBefore": null,
                    "approvalRef": null,
                    "recoveryClaimedAt": null,
                    "recoveryOutcome": null,
                    "resolvedAt": null,
                    "resolutionRef": null,
                }));
            }
        }
    }

    let reason = format!("cos {mode}: run receipt and source coverage");
    let out = state::commit(&ws, &cand, revision, &reason)?;
    Ok(Recorded::Committed {
        revision: out["revision"].as_i64().unwrap_or(revision + 1),
    })
}

/// A candidate state written for the user to read and commit.
#[derive(Debug)]
pub struct Candidate {
    pub path: PathBuf,
    /// Pass as `--expected-revision`.
    pub revision: i64,
    pub added: Vec<String>,
    /// Proposals naming no tracked goal, left out.
    pub skipped: Vec<String>,
}

/// Writes the review's proposals into `.chief-of-staff/candidate.json` as
/// `proposed` deliverables. Nothing is committed.
pub fn proposal_candidate(home: &Path, proposals: &[Proposal], source: &str) -> Result<Candidate> {
    let ws = std::path::absolute(home)?;
    let current =
        state::read(&ws)?.context("no .chief-of-staff/state.json; run `cos state init`")?;
    let revision = current["revision"].as_i64().unwrap_or(0);
    let mut cand = current.clone();
    let now = state::now();
    let goals: Vec<String> = current["goals"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|g| g["id"].as_str().map(str::to_owned))
        .collect();
    let mut skipped = Vec::new();
    let deliverables = cand["deliverables"]
        .as_array_mut()
        .context("deliverables")?;
    let mut next = deliverables
        .iter()
        .filter_map(|d| d["id"].as_str()?.strip_prefix("D-")?.parse::<u64>().ok())
        .max()
        .unwrap_or(0);
    let mut ids = Vec::new();
    for p in proposals {
        if p.title.trim().is_empty() || p.done_when.trim().is_empty() {
            continue;
        }
        if !goals.contains(&p.goal_id) {
            skipped.push(p.title.clone());
            continue;
        }
        next += 1;
        let id = format!("D-{next}");
        let due_basis = snake(&p.due_basis);
        let due = (!p.due_date.is_empty() && due_basis != "unknown").then(|| p.due_date.clone());
        deliverables.push(json!({
            "id": id,
            "goalId": p.goal_id,
            "title": p.title,
            "owner": if p.owner.trim().is_empty() { Value::Null } else { json!(p.owner) },
            "ownerBasis": snake(&p.owner_basis),
            "dueDate": due,
            "dueBasis": due_basis,
            "week": null,
            "blocker": if p.blocker.is_empty() { Value::Null } else { json!(p.blocker) },
            "dependencies": [],
            "doneWhen": p.done_when,
            "status": "proposed",
            "source": source,
            "evidenceAt": now,
            "reviewedAt": null,
            "meetingAt": null,
            "completionEvidence": null,
        }));
        ids.push(id);
    }
    state::validate(&cand).context("the proposals do not fit the state; edit them by hand")?;
    let path = state::runtime_dir(&ws)?.join("candidate.json");
    state::atomic_write(&path, &cand)?;
    Ok(Candidate {
        path,
        revision,
        added: ids,
        skipped,
    })
}
