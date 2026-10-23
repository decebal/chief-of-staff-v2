//! The hiring policy: what an agent is given when the spawn did not say,
//! from `context/agents.json`. Data, so changing how agents are hired for a
//! repo is an edit, not a rebuild. A missing or unreadable file hires on the
//! built-in defaults and says why in [`Policy::note`].

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::Request;
use super::watch::Limits;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Terms {
    /// Shell commands that must exit 0 when the agent ends.
    #[serde(default)]
    pub checks: Vec<String>,
    /// Paths the agent may change, relative to the repo.
    #[serde(default)]
    pub scope: Vec<String>,
    #[serde(default)]
    pub deadline_secs: Option<u64>,
    #[serde(default)]
    pub budget_usd: Option<f64>,
    #[serde(default)]
    pub stall_secs: Option<u64>,
    #[serde(default)]
    pub repeat: Option<usize>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    /// Applied to every agent.
    #[serde(default)]
    pub defaults: Terms,
    /// By repo directory name (`~/Projects/<name>`); wins over `defaults`.
    #[serde(default)]
    pub repos: BTreeMap<String, Terms>,
    /// Why the file was not used, when it was not.
    #[serde(skip)]
    pub note: Option<String>,
}

pub fn path(home: &Path) -> std::path::PathBuf {
    home.join("context/agents.json")
}

pub fn load(home: &Path) -> Policy {
    let p = path(home);
    let Ok(bytes) = std::fs::read(&p) else {
        return Policy::default();
    };
    serde_json::from_slice(&bytes).unwrap_or_else(|e| Policy {
        note: Some(format!("{} ignored: {e}", p.display())),
        ..Policy::default()
    })
}

impl Policy {
    /// The terms for one repo: its own entry over `defaults`, field by field.
    pub fn terms(&self, repo: Option<&Path>) -> Terms {
        let own = repo
            .and_then(|r| r.file_name())
            .and_then(|n| self.repos.get(n.to_string_lossy().as_ref()));
        let d = &self.defaults;
        let Some(o) = own else {
            return d.clone();
        };
        Terms {
            checks: if o.checks.is_empty() {
                d.checks.clone()
            } else {
                o.checks.clone()
            },
            scope: if o.scope.is_empty() {
                d.scope.clone()
            } else {
                o.scope.clone()
            },
            deadline_secs: o.deadline_secs.or(d.deadline_secs),
            budget_usd: o.budget_usd.or(d.budget_usd),
            stall_secs: o.stall_secs.or(d.stall_secs),
            repeat: o.repeat.or(d.repeat),
        }
    }

    /// Fills what the request left empty. A request's own checks and scope
    /// win whole; the policy never adds to them. Deadline and budget come
    /// from the policy only when `explicit` is false for them.
    pub fn apply(&self, req: &mut Request, explicit: Explicit) -> Limits {
        let t = self.terms(req.repo.as_deref());
        if req.contract.checks.is_empty() {
            req.contract.checks = t.checks;
        }
        if req.contract.scope.is_empty() {
            req.contract.scope = t.scope;
        }
        if let (false, Some(d)) = (explicit.deadline, t.deadline_secs) {
            req.deadline = std::time::Duration::from_secs(d);
        }
        if let (false, Some(b)) = (explicit.budget, t.budget_usd) {
            req.budget_usd = b;
        }
        let base = Limits::default();
        Limits {
            stall_secs: t.stall_secs.unwrap_or(base.stall_secs),
            repeat: t.repeat.unwrap_or(base.repeat),
        }
    }
}

/// Which request fields the caller set on purpose, so the policy leaves them.
#[derive(Debug, Clone, Copy, Default)]
pub struct Explicit {
    pub deadline: bool,
    pub budget: bool,
}
