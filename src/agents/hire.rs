//! Planning a hire before anything starts: the work, the repo it belongs
//! in, whether the role can do it (`fit`), and the contract it is judged on.
//! The desktop's hire dialog, `cos agents spawn` and the MCP `spin_off`
//! tool all plan through here and start nothing when the plan is refused.
//!
//! The repo is resolved from the workspace, in order: the deliverable's own
//! `repo`/`repos`, its goal's `repos`, `whereItLives` and `docs`, the ticket's
//! project in the profile's `jiraProjects` (`{ "key": "VM", "repos": [...] }`),
//! then a path or a monitored repo the work names. Each candidate is a local
//! checkout under the projects directory. No repo and a role that needs one
//! is a refusal, never a scratch directory.

use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::Serialize;
use serde_json::Value;

use super::fit::{self, Fit, Work};
use super::judge::Contract;
use super::policy::{self, Policy, Role};
use super::{Task, deliverable_task, infer_repo};

/// What to hire for.
pub enum Ask {
    /// A tracked deliverable, by id.
    Deliverable(String),
    /// A prompt: a brief's handoff or your own.
    Task(Task),
}

#[derive(Debug, Clone, Serialize)]
pub struct Plan {
    pub role: Option<String>,
    pub title: String,
    pub source: String,
    pub prompt: String,
    pub repo: Option<PathBuf>,
    /// Where the repo came from, in words.
    pub repo_from: String,
    /// Every checkout the work pointed at, the chosen one first.
    pub candidates: Vec<PathBuf>,
    /// The role may run in a scratch directory.
    pub scratch_ok: bool,
    pub fit: Fit,
    /// What the agent will be judged on, the policy applied.
    pub contract: Contract,
    /// Things to know before hiring that do not stop it.
    pub warnings: Vec<String>,
    /// Why nothing will start, when it will not.
    pub refused: Option<String>,
}

pub struct Where<'a> {
    pub home: &'a Path,
    pub workspace: &'a Path,
    pub projects: &'a Path,
}

/// The plan for `ask` hired into `role`. `repo` and the contract's checks,
/// scope and `done_when` are what the caller chose; the rest is filled here.
/// An unknown role, or the head, is an error.
pub fn plan(
    at: &Where,
    role: Option<&str>,
    ask: Ask,
    repo: Option<PathBuf>,
    asked: &Contract,
) -> Result<Plan> {
    let policy = policy::load(at.home);
    let role = role.map(|id| policy.hire(id)).transpose()?;
    let state = crate::state::read(at.workspace).ok().flatten();
    let profile = crate::state::profile(at.workspace);
    let vocab = policy.fit.clone().unwrap_or_default();

    let (task, done_when, work, deliverable) = match ask {
        Ask::Deliverable(id) => {
            let st = state
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("no workspace state to hand {id} off from"))?;
            let (task, done_when) = deliverable_task(st, profile.as_ref(), &id)?;
            let d = crate::state::find(st, "deliverables", &id).cloned();
            let blocker = d.as_ref().and_then(|d| d["blocker"].as_str()).unwrap_or("");
            let work = Work {
                title: task.title.clone(),
                text: format!("{}\n{blocker}", task.title),
                done_when: done_when.clone(),
            };
            (task, done_when, work, d)
        }
        Ask::Task(task) => {
            let work = Work {
                title: task.title.clone(),
                text: fit::strip(&vocab, &format!("{}\n{}", task.title, task.prompt)),
                done_when: None,
            };
            (task, None, work, None)
        }
    };
    let done_when = asked.done_when.clone().or(done_when);
    let work = Work {
        done_when: done_when.clone(),
        ..work
    };
    let fit = fit::check(&vocab, &policy.roles, role, &work);
    let scratch_ok = role.is_some_and(policy::scratch_ok);

    let (repo, repo_from, candidates) = match (repo, role) {
        (Some(r), _) => (Some(r.clone()), "you picked it".to_owned(), vec![r]),
        (None, Some(r)) if r.repo == Some(false) => (
            None,
            format!("the {} works without a repo", r.title),
            Vec::new(),
        ),
        (None, _) => {
            let found = resolve(
                state.as_ref(),
                profile.as_ref(),
                deliverable.as_ref(),
                &work.text,
                &task.prompt,
                at.projects,
            );
            (found.repo, found.from, found.candidates)
        }
    };

    let contract = contract(&policy, role, repo.as_deref(), asked, done_when.as_deref());
    let mut prompt = task.prompt.clone();
    if let (Some(draft), Some(original)) = (&contract.done_when, &contract.finish_line) {
        prompt.push_str(&format!(
            "\n\nYour finish line here: {draft} The deliverable's own finish line, \"{original}\", is a step for the user, outside your sandbox."
        ));
    }

    let mut warnings = Vec::new();
    if !fit::judgeable(&contract) {
        warnings.push(
            "nothing the judge can run: no checks, no scope and no output file, so it can only end `needs you`; add a check or a scope".into(),
        );
    }
    let refused = fit.refusal(&policy.roles).or_else(|| {
        (repo.is_none() && !scratch_ok).then(|| {
            let who = role.map_or("an agent without a role".to_owned(), |r| format!("the {}", r.title));
            format!(
                "no repo for this work ({repo_from}), and {who} needs one: pick the repo it belongs in"
            )
        })
    });

    Ok(Plan {
        role: role.map(|r| r.id.clone()),
        title: task.title,
        source: task.source,
        prompt,
        repo,
        repo_from,
        candidates,
        scratch_ok,
        fit,
        contract,
        warnings,
        refused,
    })
}

/// How to run a planned hire; what the plan does not decide.
pub struct Run {
    pub harness: super::Harness,
    pub program: Option<String>,
    pub model: Option<String>,
    /// Cut a worktree of the repo, rather than work in its checkout.
    pub worktree: bool,
    pub deadline: std::time::Duration,
    pub budget_usd: f64,
    pub slots: Option<usize>,
    pub explicit: policy::Explicit,
}

impl Plan {
    /// The spawn request, or the refusal. A refused plan starts nothing.
    pub fn request(self, run: Run) -> Result<super::Request> {
        if let Some(why) = self.refused {
            anyhow::bail!("not hired: {why}");
        }
        Ok(super::Request {
            harness: run.harness,
            program: run.program,
            model: run.model,
            worktree: run.worktree && self.repo.is_some(),
            repo: self.repo,
            title: self.title,
            source: self.source,
            prompt: self.prompt,
            deadline: run.deadline,
            budget_usd: run.budget_usd,
            slots: run.slots,
            contract: self.contract,
            explicit: run.explicit,
            role: self.role,
        })
    }
}

/// The contract the policy would give: the caller's checks and scope win
/// whole, then the role's, then the repo's. An outward-facing `done_when`
/// becomes the draft the agent can return; the original is kept as the
/// `finish_line`.
fn contract(
    policy: &Policy,
    role: Option<&Role>,
    repo: Option<&Path>,
    asked: &Contract,
    done_when: Option<&str>,
) -> Contract {
    let t = policy.terms_for(repo, role);
    let pick = |own: &[String], fallback: Vec<String>| {
        if own.is_empty() {
            fallback
        } else {
            own.to_vec()
        }
    };
    let mut c = Contract {
        checks: pick(&asked.checks, t.checks),
        scope: pick(&asked.scope, t.scope),
        expects: role.map_or(asked.expects, |r| r.expects),
        output: asked
            .output
            .clone()
            .or_else(|| role.and_then(|r| r.output.clone())),
        done_when: done_when
            .map(str::to_owned)
            .or_else(|| role.and_then(|r| r.done_when.clone())),
        finish_line: None,
    };
    let vocab = policy.fit.clone().unwrap_or_default();
    if let Some(original) = c.done_when.clone()
        && let Some(draft) = fit::draft_done_when(&vocab, &original, role)
    {
        c.done_when = Some(draft);
        c.finish_line = Some(original);
    }
    c
}

pub struct Resolved {
    pub repo: Option<PathBuf>,
    pub from: String,
    pub candidates: Vec<PathBuf>,
}

/// The repo the work belongs in, by the order in the module doc.
pub fn resolve(
    state: Option<&Value>,
    profile: Option<&Value>,
    deliverable: Option<&Value>,
    text: &str,
    prompt: &str,
    projects: &Path,
) -> Resolved {
    let monitored: Vec<String> = profile
        .and_then(|p| p["monitoredRepos"].as_array())
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect();
    let goal = deliverable
        .and_then(|d| d["goalId"].as_str())
        .and_then(|g| state.and_then(|s| crate::state::find(s, "goals", g)));

    let mut steps: Vec<(String, Vec<String>)> = Vec::new();
    if let Some(d) = deliverable {
        let id = d["id"].as_str().unwrap_or("the deliverable");
        steps.push((format!("{id}'s repo"), names(&[&d["repo"], &d["repos"]])));
    }
    if let Some(g) = goal {
        let id = g["id"].as_str().unwrap_or("its goal");
        let mut found = names(&[&g["repos"]]);
        let lives = g["whereItLives"].as_str().unwrap_or("");
        found.extend(slugs(lives));
        found.extend(names(&[&g["docs"]]).into_iter().map(|d| {
            d.split_once(':')
                .map_or(d.clone(), |(repo, _)| repo.to_owned())
        }));
        steps.push((format!("goal {id}"), found));
    }
    let mut tickets: Vec<String> = Vec::new();
    for v in [
        deliverable.map(|d| &d["source"]),
        deliverable.map(|d| &d["title"]),
    ]
    .into_iter()
    .flatten()
    {
        tickets.extend(ticket_keys(v.as_str().unwrap_or("")));
    }
    tickets.extend(ticket_keys(text));
    if let Some(g) = goal {
        for t in g["jira"].as_array().into_iter().flatten() {
            tickets.extend(ticket_keys(t.as_str().unwrap_or("")));
        }
    }
    let mut by_ticket = Vec::new();
    for key in &tickets {
        let project = key.split('-').next().unwrap_or("");
        for p in profile
            .and_then(|p| p["jiraProjects"].as_array())
            .into_iter()
            .flatten()
        {
            if p["key"].as_str() == Some(project) {
                by_ticket.extend(names(&[&p["repo"], &p["repos"]]));
            }
        }
    }
    if let Some(k) = tickets.first() {
        steps.push((format!("ticket {k}'s project"), by_ticket));
    }

    let mut chosen: Option<(String, PathBuf)> = None;
    let mut candidates: Vec<PathBuf> = Vec::new();
    for (from, wanted) in steps {
        for w in wanted {
            if let Some(p) = checkout(&w, &monitored, projects)
                && !candidates.contains(&p)
            {
                chosen.get_or_insert_with(|| (from.clone(), p.clone()));
                candidates.push(p);
            }
        }
    }
    if chosen.is_none() {
        if let Some(p) = infer_repo(prompt, projects) {
            chosen = Some(("the path the work names".into(), p.clone()));
            candidates.push(p);
        } else {
            for slug in &monitored {
                let name = slug.rsplit('/').next().unwrap_or(slug);
                if fit::has(text, name)
                    && let Some(p) = checkout(slug, &monitored, projects)
                    && !candidates.contains(&p)
                {
                    chosen.get_or_insert_with(|| {
                        ("the monitored repo the work names".into(), p.clone())
                    });
                    candidates.push(p);
                }
            }
        }
    }
    match chosen {
        Some((from, repo)) => Resolved {
            repo: Some(repo),
            from,
            candidates,
        },
        None => Resolved {
            repo: None,
            from: "nothing in the work, its goal, its ticket or the profile names a checkout under the projects directory".into(),
            candidates,
        },
    }
}

/// Strings from string-or-array fields.
fn names(fields: &[&Value]) -> Vec<String> {
    let mut out = Vec::new();
    for f in fields {
        match f {
            Value::String(s) if !s.trim().is_empty() => out.push(s.trim().to_owned()),
            Value::Array(a) => out.extend(
                a.iter()
                    .filter_map(Value::as_str)
                    .filter(|s| !s.trim().is_empty())
                    .map(|s| s.trim().to_owned()),
            ),
            _ => {}
        }
    }
    out
}

/// `owner/name` tokens in prose, e.g. "acme/api (develop); Jira API".
fn slugs(text: &str) -> Vec<String> {
    text.split(|c: char| c.is_whitespace() || "()[],;`'\"".contains(c))
        .map(|t| t.trim_end_matches(['.', ':']))
        .filter(|t| {
            let mut parts = t.split('/');
            matches!(
                (parts.next(), parts.next(), parts.next()),
                (Some(a), Some(b), None) if !a.is_empty() && !b.is_empty() && !a.contains(':')
            )
        })
        .map(str::to_owned)
        .collect()
}

/// Ticket keys such as `ABC-123`.
pub fn ticket_keys(text: &str) -> Vec<String> {
    text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
        .filter(|t| {
            t.split_once('-').is_some_and(|(p, n)| {
                p.len() >= 2
                    && p.chars().next().is_some_and(|c| c.is_ascii_uppercase())
                    && p.chars()
                        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
                    && !n.is_empty()
                    && n.chars().all(|c| c.is_ascii_digit())
            })
        })
        .map(str::to_owned)
        .collect()
}

/// The local checkout for an `owner/name` slug, a bare repo name, or a path.
fn checkout(wanted: &str, monitored: &[String], projects: &Path) -> Option<PathBuf> {
    let is_repo = |p: &Path| p.join(".git").exists();
    if wanted.starts_with('/') || wanted.starts_with("~/") {
        let p = match wanted.strip_prefix("~/") {
            Some(rest) => PathBuf::from(std::env::var_os("HOME")?).join(rest),
            None => PathBuf::from(wanted),
        };
        return is_repo(&p).then_some(p);
    }
    let slug = if wanted.contains('/') {
        Some(wanted.to_owned())
    } else {
        monitored
            .iter()
            .find(|s| s.rsplit('/').next() == Some(wanted))
            .cloned()
    };
    if let Some(slug) = slug
        && let Some((_, p)) = crate::docs::checkouts(projects, &[slug]).into_iter().next()
    {
        return Some(p);
    }
    let name = wanted.rsplit('/').next()?;
    let p = projects.join(name);
    is_repo(&p).then_some(p)
}
