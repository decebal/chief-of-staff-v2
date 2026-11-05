//! The hiring policy: what an agent is given when the spawn did not say,
//! from `context/agents.json`. Data, so changing how agents are hired for a
//! repo, or what a role is held to, is an edit, not a rebuild. A missing or
//! unreadable file hires on the built-in defaults and says why in
//! [`Policy::note`]; a file without `roles` gets the template's roles.
//!
//! Precedence, field by field: what the spawn set, then the role's contract
//! (checks, scope, expects, output, done_when: a reviewer is judged on its
//! review file in any repo), then the repo's entry, then `defaults`. Limits
//! (deadline, budget, stall, repeat) take the repo before the role: the repo
//! knows what its build costs.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

use super::fit::Vocab;
use super::judge::Expects;
use super::watch::Limits;
use super::{Harness, Request};

/// The template every home is seeded from; its roles stand in for a home
/// whose own file has none.
pub const SEED: &str = include_str!("../../templates/context/agents.json");

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

/// One job on the staff: what it is for, where it works, and what it is
/// judged on. Handing work to a role grants nothing outside its worktree.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Role {
    /// The key a spawn names, e.g. `reviewer`.
    pub id: String,
    pub title: String,
    /// The chief of staff: it hires, it is not hired.
    #[serde(default)]
    pub head: bool,
    pub mandate: String,
    /// The goals and deliverables it serves, in words.
    #[serde(default)]
    pub serves: String,
    /// Where it works and what it may touch, in words.
    #[serde(default)]
    pub sandbox: String,
    /// How its work is judged, in words.
    #[serde(default)]
    pub judged: String,
    #[serde(default)]
    pub done_when: Option<String>,
    #[serde(default)]
    pub expects: Expects,
    /// The file it leaves, relative to its workdir, when it `expects: file`.
    #[serde(default)]
    pub output: Option<String>,
    #[serde(default)]
    pub harness: Option<Harness>,
    #[serde(default)]
    pub model: Option<String>,
    /// `false` hires it in a scratch directory with no repo.
    #[serde(default)]
    pub repo: Option<bool>,
    /// The kinds of work it does (`fit.kinds`); empty takes any.
    #[serde(default)]
    pub work: Vec<String>,
    /// Needs (`fit.needs`) it can draft for without meeting them: a tester
    /// writes the script for a live system that you run.
    #[serde(default)]
    pub drafts: Vec<String>,
    #[serde(default)]
    pub checks: Vec<String>,
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
    /// The staff, in the order the window shows it; the head first.
    #[serde(default)]
    pub roles: Vec<Role>,
    /// By repo directory name (`~/Projects/<name>`); wins over `defaults`.
    #[serde(default)]
    pub repos: BTreeMap<String, Terms>,
    /// The fit check's vocabulary; the template's when the file has none.
    #[serde(default)]
    pub fit: Option<Vocab>,
    /// Why the file was not used, when it was not.
    #[serde(skip)]
    pub note: Option<String>,
}

pub fn path(home: &Path) -> std::path::PathBuf {
    home.join("context/agents.json")
}

/// The template's policy; it parses, `tests/hiring.rs` says so.
pub fn seed() -> Policy {
    serde_json::from_str(SEED).unwrap_or_default()
}

pub fn load(home: &Path) -> Policy {
    let p = path(home);
    let mut policy = match std::fs::read(&p) {
        Err(_) => Policy::default(),
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|e| Policy {
            note: Some(format!("{} ignored: {e}", p.display())),
            ..Policy::default()
        }),
    };
    if policy.roles.is_empty() {
        policy.roles = seed().roles;
    }
    if policy.fit.is_none() {
        policy.fit = seed().fit;
    }
    policy
}

fn or<T: Clone>(own: &[T], fallback: &[T]) -> Vec<T> {
    if own.is_empty() { fallback } else { own }.to_vec()
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
            checks: or(&o.checks, &d.checks),
            scope: or(&o.scope, &d.scope),
            deadline_secs: o.deadline_secs.or(d.deadline_secs),
            budget_usd: o.budget_usd.or(d.budget_usd),
            stall_secs: o.stall_secs.or(d.stall_secs),
            repeat: o.repeat.or(d.repeat),
        }
    }

    pub fn role(&self, id: &str) -> Option<&Role> {
        self.roles.iter().find(|r| r.id == id)
    }

    /// The role a spawn may hire, or why not.
    pub fn hire(&self, id: &str) -> Result<&Role> {
        let Some(r) = self.role(id) else {
            let known: Vec<&str> = self.roles.iter().map(|r| r.id.as_str()).collect();
            bail!(
                "no role {id} in context/agents.json (roles: {})",
                known.join(", ")
            );
        };
        if r.head {
            bail!("{} hires; it is not hired", r.title);
        }
        Ok(r)
    }

    /// The terms for one repo under one role (see the precedence above).
    pub fn terms_for(&self, repo: Option<&Path>, role: Option<&Role>) -> Terms {
        let t = self.terms(repo);
        let Some(r) = role else {
            return t;
        };
        let repo_own = repo
            .and_then(|r| r.file_name())
            .and_then(|n| self.repos.get(n.to_string_lossy().as_ref()));
        let d = &self.defaults;
        let limit = |repo: Option<u64>, role: Option<u64>, def: Option<u64>| repo.or(role).or(def);
        Terms {
            checks: or(&r.checks, &t.checks),
            scope: or(&r.scope, &t.scope),
            deadline_secs: limit(
                repo_own.and_then(|o| o.deadline_secs),
                r.deadline_secs,
                d.deadline_secs,
            ),
            budget_usd: repo_own
                .and_then(|o| o.budget_usd)
                .or(r.budget_usd)
                .or(d.budget_usd),
            stall_secs: limit(
                repo_own.and_then(|o| o.stall_secs),
                r.stall_secs,
                d.stall_secs,
            ),
            repeat: repo_own.and_then(|o| o.repeat).or(r.repeat).or(d.repeat),
        }
    }

    /// Fills what the request left empty. A request's own checks and scope
    /// win whole; the policy never adds to them. Deadline and budget come
    /// from the policy only when `explicit` is false for them. A request
    /// naming an unknown role, or the head, is refused.
    pub fn apply(&self, req: &mut Request, explicit: Explicit) -> Result<Limits> {
        let role = req.role.as_deref().map(|id| self.hire(id)).transpose()?;
        if let (None, Some(r)) = (&req.repo, role)
            && !scratch_ok(r)
        {
            bail!(
                "the {} works in a repo and none was given: pick the repo the work belongs in",
                r.title
            );
        }
        let t = self.terms_for(req.repo.as_deref(), role);
        if req.contract.checks.is_empty() {
            req.contract.checks = t.checks;
        }
        if req.contract.scope.is_empty() {
            req.contract.scope = t.scope;
        }
        if let Some(r) = role {
            if req.contract.expects == Expects::Diff {
                req.contract.expects = r.expects;
            }
            if req.contract.output.is_none() {
                req.contract.output = r.output.clone();
            }
            if req.contract.done_when.is_none() {
                req.contract.done_when = r.done_when.clone();
            }
            if req.model.is_none() {
                req.model = r.model.clone();
            }
            if !req.prompt.starts_with("Role: ") {
                req.prompt = format!("{}\n\n{}", preamble(r), req.prompt);
            }
        }
        if let (false, Some(d)) = (explicit.deadline, t.deadline_secs) {
            req.deadline = std::time::Duration::from_secs(d);
        }
        if let (false, Some(b)) = (explicit.budget, t.budget_usd) {
            req.budget_usd = b;
        }
        let base = Limits::default();
        Ok(Limits {
            stall_secs: t.stall_secs.unwrap_or(base.stall_secs),
            repeat: t.repeat.unwrap_or(base.repeat),
        })
    }
}

/// A role that may run in a scratch directory: it needs no repo and is
/// judged on a file, not a diff.
pub fn scratch_ok(r: &Role) -> bool {
    r.repo == Some(false) && r.expects != Expects::Diff
}

/// What the agent reads first: its role, its mandate, what it must leave.
pub fn preamble(r: &Role) -> String {
    let mut s = format!("Role: {}. {}", r.title, r.mandate.trim());
    match (r.expects, r.output.as_deref()) {
        (Expects::File, Some(out)) => s.push_str(&format!(
            " Leave your result in `{out}`; you are judged on that file."
        )),
        (Expects::Nothing, Some(out)) => s.push_str(&format!(
            " Change nothing in the repo; write your answer to `{out}` only."
        )),
        (Expects::Nothing, None) => s.push_str(" Change nothing in the repo."),
        _ => {}
    }
    s.push_str(
        " Drafts only: nothing outward-facing; this hire grants nothing outside the worktree.",
    );
    s
}

/// Which request fields the caller set on purpose, so the policy leaves them.
#[derive(Debug, Clone, Copy, Default)]
pub struct Explicit {
    pub deadline: bool,
    pub budget: bool,
}
