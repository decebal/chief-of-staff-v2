//! What a brain reads that is not a connector, inlined into its prompt, and
//! which connectors it may call. Under `cos` the brains have no file tools at
//! all: they read mail and tickets written by other people, and a brain that
//! can open any file can be talked into quoting `~/.ssh` into a brief that is
//! then served over MCP.

use std::fmt::Write as _;
use std::path::Path;

use serde_json::{Value, json};

use crate::READ_ONLY;
use crate::gather;
use crate::state;

/// Inlined context stops growing past this; the rest is named, not read.
pub const LIMIT: usize = 64 * 1024;

const FILES: &[&str] = &["profile.md", "goals.md", "principles.md"];

/// Runs kept in the prompt: enough to reconcile against, not the history.
const RECENT_RUNS: usize = 3;

/// The workspace, then every context file. Decisions come newest first, so a
/// truncated block drops the oldest.
pub fn block(home: &Path, ws: &Path) -> String {
    let mut s = workspace(ws);
    s.push_str(
        "\nContext files, inlined by cos (you have no file tools here; this is all of context/):\n",
    );
    let dir = home.join("context");
    for name in FILES {
        let body = std::fs::read_to_string(dir.join(name))
            .unwrap_or_else(|_| "(missing — say so in gaps)".into());
        let _ = write!(s, "\n### context/{name}\n\n{}\n", body.trim());
    }

    let mut decisions: Vec<_> = std::fs::read_dir(dir.join("decisions"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".md") && n != "README.md")
        .collect();
    decisions.sort_unstable_by(|a, b| b.cmp(a));

    let mut skipped = Vec::new();
    for name in decisions {
        let Ok(body) = std::fs::read_to_string(dir.join("decisions").join(&name)) else {
            continue;
        };
        let section = format!("\n### context/decisions/{name}\n\n{}\n", body.trim());
        if s.len() + section.len() > LIMIT {
            skipped.push(name);
        } else {
            s.push_str(&section);
        }
    }
    if !skipped.is_empty() {
        let _ = write!(
            s,
            "\n(Not inlined, over the {} KiB context limit: {}. Name them in gaps.)\n",
            LIMIT / 1024,
            skipped.join(", ")
        );
    }
    s
}

/// The `.chief-of-staff/` workspace: the profile, and the state's open work.
/// Completed work and the journal stay out; they are history, not today.
pub fn workspace(home: &Path) -> String {
    let profile = state::profile(home);
    let current = state::read(home);
    let (Some(profile), Ok(Some(st))) = (profile, &current) else {
        return match current {
            Err(e) => format!(
                "Chief of Staff workspace: .chief-of-staff/state.json failed validation ({e:#}). \
                 Use none of it; say so in gaps.\n"
            ),
            _ => "Chief of Staff workspace: none (.chief-of-staff/ has no profile and state). \
                  Goals, KPIs and deliverables are not tracked; say so in gaps, use \
                  context/goals.md if present, and never present an inferred goal as a \
                  confirmed one.\n"
                .into(),
        };
    };

    let open = |c: &str| -> Vec<Value> { state::open(st, c).cloned().collect() };
    let runs: Vec<&Value> = st["runs"]
        .as_array()
        .map(|r| r.iter().rev().take(RECENT_RUNS).collect())
        .unwrap_or_default();
    let incidents: Vec<&Value> = st["incidents"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|i| i["resolvedAt"].is_null())
        .collect();
    let view = json!({
        "revision": st["revision"],
        "goals": open("goals"),
        "kpis": st["kpis"],
        "deliverables": open("deliverables"),
        "commitments": open("commitments"),
        "sources": st["sources"],
        "recentRuns": runs,
        "openIncidents": incidents,
    });

    let mut s = String::from(
        "Chief of Staff workspace, inlined by cos. The profile and the goals in it are \
         confirmed by the user; proposals are marked `proposed`. Cite ids (G-1, D-2) as \
         goal_id and deliverable_id.\n",
    );
    let sources = state::approved_sources(&profile);
    if sources.is_empty() {
        s.push_str(
            "\nApproved sources: none. Read no connector; work from the facts and the \
             state, and list every source as not_reviewed in coverage.\n",
        );
    } else {
        s.push_str(
            "\nApproved sources (read only these, within scope and budget). Try every one: \
             load its tool with ToolSearch, then call it. Mark a source unavailable only \
             when ToolSearch finds no tool or the call fails, and quote the error in its \
             coverage note:\n",
        );
        for src in &sources {
            let _ = writeln!(
                s,
                "- {} ({}): scope {}; read budget {}",
                src.id,
                src.service,
                if src.scope.is_empty() {
                    "unstated"
                } else {
                    &src.scope
                },
                if src.read_budget.is_empty() {
                    "unstated"
                } else {
                    &src.read_budget
                }
            );
        }
    }
    let pretty = |v: &Value| serde_json::to_string_pretty(v).unwrap_or_default();
    let _ = write!(
        s,
        "\n### .chief-of-staff/profile.json\n\n```json\n{}\n```\n\n### .chief-of-staff/state.json (open work)\n\n```json\n{}\n```\n",
        pretty(&profile),
        pretty(&view)
    );
    s
}

/// The `READ_ONLY` tools a profile source's `service` covers.
pub fn service_tools(service: &str) -> Vec<&'static str> {
    let service = service.to_lowercase();
    let prefixes: &[&str] = if service.contains("mail") {
        &["mcp__claude_ai_Gmail__"]
    } else if service.contains("calendar") {
        &["mcp__claude_ai_Google_Calendar__"]
    } else if ["jira", "confluence", "atlassian"]
        .iter()
        .any(|k| service.contains(k))
    {
        &["mcp__claude_ai_Atlassian_Rovo__"]
    } else if service.contains("notion") {
        &["mcp__claude_ai_Notion__"]
    } else {
        &[]
    };
    READ_ONLY
        .iter()
        .copied()
        .filter(|t| prefixes.iter().any(|p| t.starts_with(p)))
        .collect()
}

/// A source `cos` reads itself rather than through a connector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Local {
    /// PRs through `gh`.
    Code,
    /// Ready tasks through `cn`.
    Tasks,
}

pub fn local_kind(service: &str) -> Option<Local> {
    let service = service.to_lowercase();
    if ["code", "github", "gh"].contains(&service.as_str()) || service.contains("github") {
        Some(Local::Code)
    } else if ["tasks", "chronis", "cn"].contains(&service.as_str()) {
        Some(Local::Tasks)
    } else {
        None
    }
}

/// GitHub orgs the profile counts as work; empty without a workspace.
pub fn work_orgs(home: &Path) -> Vec<String> {
    state::profile(home)
        .map(|p| state::code_orgs(&p))
        .unwrap_or_default()
}

/// Which local facts `cos` may collect. With a workspace profile, only those
/// of an approved `code` / `tasks` source; without one, both, as before the
/// workspace existed.
pub fn local_reads(home: &Path) -> gather::Reads {
    let Some(profile) = state::profile(home) else {
        return gather::Reads::ALL;
    };
    let kinds: Vec<Local> = state::approved_sources(&profile)
        .iter()
        .filter_map(|s| local_kind(&s.service))
        .collect();
    gather::Reads {
        code: kinds.contains(&Local::Code),
        tasks: kinds.contains(&Local::Tasks),
    }
}

/// The tools a brain may call. With a workspace profile, only the connectors
/// of its approved sources (none approved means none); without one, every
/// `READ_ONLY` connector, as before the workspace existed.
pub fn tools(home: &Path) -> Vec<&'static str> {
    let Some(profile) = state::profile(home) else {
        return READ_ONLY.to_vec();
    };
    let mut allowed = vec!["ToolSearch"];
    for src in state::approved_sources(&profile) {
        for t in service_tools(&src.service) {
            if !allowed.contains(&t) {
                allowed.push(t);
            }
        }
    }
    allowed
}

/// MCP servers denied to every brain, from `context/deny-mcp.txt`: one tool
/// prefix per line, `#` comments. Denying a server keeps its tool definitions
/// out of the context each run pays to cache.
pub fn denied_servers(home: &Path) -> Vec<String> {
    std::fs::read_to_string(home.join("context/deny-mcp.txt"))
        .unwrap_or_default()
        .lines()
        .map(|l| l.split('#').next().unwrap_or("").trim())
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect()
}
