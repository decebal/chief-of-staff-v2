//! Proposing: a headless `claude` with no tools specialises a library entry
//! to the business and returns a closed proposal of descriptive fields. The
//! contract always comes from the entry. Nothing here writes the policy.

use std::path::Path;
use std::sync::atomic::AtomicBool;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{CAPS, FieldDiff, LibraryEntry, ListCap, diff};
use crate::agents::policy::{Policy, Role};

/// A repo the specialise run may name: its directory and the paths a hire
/// there may change. Its checks are never sent: a check can carry a token.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RepoScope {
    pub name: String,
    pub scope: Vec<String>,
}

/// Everything a specialise run is shown besides the library entry. The type
/// has no field for anything else, which is the boundary.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SpecialiseContext {
    pub business: String,
    pub goals: Vec<String>,
    pub repos: Vec<RepoScope>,
    /// Id and title of every role still hired.
    pub staff: Vec<(String, String)>,
    pub brief: Option<String>,
}

const GOALS: ListCap = ListCap {
    items: 20,
    chars: 200,
};
const REPOS: usize = 30;
const STAFF: usize = 40;
const BUSINESS: usize = 80;

/// The context from `policy`, `state` (the workspace's `state.json`, goal
/// titles only), the business name and the user's brief. A brief over its
/// cap is refused rather than cut, so the user knows what was sent.
pub fn specialise_context(
    policy: &Policy,
    state: Option<&Value>,
    business: &str,
    brief: Option<&str>,
) -> Result<SpecialiseContext> {
    let brief = brief.map(str::trim).filter(|b| !b.is_empty());
    if let Some(b) = brief
        && b.chars().count() > CAPS.brief
    {
        bail!("the brief is over {} characters; shorten it", CAPS.brief);
    }
    let goals: Vec<String> = state
        .and_then(|s| s["goals"].as_array())
        .into_iter()
        .flatten()
        .filter_map(|g| g["title"].as_str())
        .map(|t| super::clean(t, GOALS.chars))
        .filter(|t| !t.is_empty())
        .take(GOALS.items)
        .collect();
    let repos = policy
        .repos
        .iter()
        .take(REPOS)
        .map(|(name, t)| RepoScope {
            name: super::clean(name, 100),
            scope: super::clean_list(&t.scope, CAPS.scope),
        })
        .collect();
    let staff = policy
        .roles
        .iter()
        .filter(|r| !r.retired)
        .take(STAFF)
        .map(|r| (r.id.clone(), super::clean(&r.title, CAPS.title)))
        .collect();
    Ok(SpecialiseContext {
        business: super::clean(business, BUSINESS),
        goals,
        repos,
        staff,
        brief: brief.map(str::to_owned),
    })
}

/// The prompt: the task, the entry's role and the context, as JSON.
pub fn specialise_prompt(entry: &LibraryEntry, ctx: &SpecialiseContext) -> String {
    let mut shown = entry.role.clone();
    shown.checks.clear();
    let role = serde_json::to_string_pretty(&shown).unwrap_or_default();
    let context = serde_json::to_string_pretty(ctx).unwrap_or_default();
    format!(
        "Specialise the library role below for this business. Keep its standard: every \
         principle, method step, anti-pattern and escalation rule must stay as demanding as the \
         original, cited source included. Make it concrete for these goals, repos and staff: \
         name the repos and paths where the work happens, and avoid duplicating a role the staff \
         already has. Do not invent tools, commands, people or systems the context does not \
         name. Return only the fields the schema asks for; the role's contract (checks, scope, \
         harness, expects, output) is kept as it is and is not yours to change. In `rationale`, \
         say in two or three sentences what you changed and why.\n\n\
         Library entry {id}@{version}, drawing on: {sources}\n\nRole:\n{role}\n\nContext:\n{context}",
        id = entry.id,
        version = entry.version,
        sources = entry
            .sources
            .iter()
            .map(|s| s.title.as_str())
            .collect::<Vec<_>>()
            .join("; "),
    )
}

/// What a specialise run returns: descriptive fields only. Unknown keys,
/// a check or a scope among them, fail to parse.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Proposal {
    pub title: String,
    pub mandate: String,
    pub serves: String,
    pub sandbox: String,
    pub judged: String,
    pub done_when: String,
    pub principles: Vec<String>,
    pub method: Vec<String>,
    pub anti_patterns: Vec<String>,
    pub output_format: String,
    pub escalation: Vec<String>,
    pub rationale: String,
}

const DESCRIPTIVE: &[&str] = &[
    "id",
    "title",
    "mandate",
    "serves",
    "sandbox",
    "judged",
    "done_when",
    "principles",
    "method",
    "anti_patterns",
    "output_format",
    "escalation",
];

/// The proposal's JSON Schema: closed, every field required, every bound
/// from [`CAPS`].
pub fn proposal_schema() -> Value {
    let text = |n: usize| json!({ "type": "string", "minLength": 1, "maxLength": n });
    let list = |c: ListCap| {
        json!({ "type": "array", "minItems": 1, "maxItems": c.items,
                "items": { "type": "string", "minLength": 1, "maxLength": c.chars } })
    };
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["title", "mandate", "serves", "sandbox", "judged", "done_when", "principles",
                     "method", "anti_patterns", "output_format", "escalation", "rationale"],
        "properties": {
            "title": text(CAPS.title),
            "mandate": text(CAPS.mandate),
            "serves": text(CAPS.text),
            "sandbox": text(CAPS.text),
            "judged": text(CAPS.text),
            "done_when": text(CAPS.text),
            "principles": list(CAPS.principles),
            "method": list(CAPS.method),
            "anti_patterns": list(CAPS.anti_patterns),
            "output_format": text(CAPS.output_format),
            "escalation": list(CAPS.escalation),
            "rationale": text(CAPS.rationale),
        }
    })
}

/// A proposal merged onto its entry, and what changed against the entry.
#[derive(Debug, Clone, Serialize)]
pub struct ProposalOut {
    pub role: Role,
    pub rationale: String,
    pub diff: Vec<FieldDiff>,
}

/// The entry's role with the proposal's descriptive fields: every contract
/// field stays the entry's. `id` overrides the entry's id.
pub fn merge(entry: &LibraryEntry, p: &Proposal, id: Option<&str>) -> ProposalOut {
    let mut role = entry.role.clone();
    if let Some(id) = id {
        role.id = id.to_owned();
    }
    role.title = p.title.clone();
    role.mandate = p.mandate.clone();
    role.serves = p.serves.clone();
    role.sandbox = p.sandbox.clone();
    role.judged = p.judged.clone();
    role.done_when = Some(p.done_when.clone());
    role.principles = p.principles.clone();
    role.method = p.method.clone();
    role.anti_patterns = p.anti_patterns.clone();
    role.output_format = Some(p.output_format.clone());
    role.escalation = p.escalation.clone();
    role.from = Some(format!("{}@{}", entry.id, entry.version));
    let diff = diff(&entry.role, &role, DESCRIPTIVE);
    ProposalOut {
        role,
        rationale: super::clean(&p.rationale, CAPS.rationale),
        diff,
    }
}

/// The recruiter's agent file and the prompt, as the user is shown them
/// before anything is sent.
pub fn sent_text(install: &Path, prompt: &str) -> String {
    let agent = std::fs::read_to_string(install.join(".claude/agents/recruiter.md"))
        .unwrap_or_else(|_| "(the recruiter agent file is missing from this install)".into());
    format!("{agent}\n\n---\n\n{prompt}")
}

/// One specialise run, owned by the calling thread: under `cos proc-guard`
/// when `guard` names the cos binary, ended on `stop`, never retried.
pub fn propose(
    cfg: &crate::pipeline::Config,
    entry: &LibraryEntry,
    ctx: &SpecialiseContext,
    id: Option<&str>,
    guard: Option<&Path>,
    stop: &AtomicBool,
) -> Result<ProposalOut> {
    let run = cfg.propose_run(specialise_prompt(entry, ctx));
    let audit = crate::claude::audit_path(&cfg.home);
    let out =
        crate::claude::execute_until(&cfg.bins.claude, &cfg.install, &audit, &run, guard, stop)?;
    let value = out
        .structured_output
        .context("the proposal run returned no structured_output")?;
    let p: Proposal =
        serde_json::from_value(value).context("the proposal did not match its schema")?;
    check(&p)?;
    Ok(merge(entry, &p, id))
}

/// The caps and one-line rules held here too: a model's schema is a request,
/// not a guarantee.
pub fn check(p: &Proposal) -> Result<()> {
    let mut bad = Vec::new();
    for (field, v, cap, one_line) in [
        ("title", &p.title, CAPS.title, true),
        ("mandate", &p.mandate, CAPS.mandate, true),
        ("serves", &p.serves, CAPS.text, false),
        ("sandbox", &p.sandbox, CAPS.text, false),
        ("judged", &p.judged, CAPS.text, false),
        ("done_when", &p.done_when, CAPS.text, false),
        ("output_format", &p.output_format, CAPS.output_format, false),
        ("rationale", &p.rationale, CAPS.rationale, false),
    ] {
        if v.trim().is_empty() || v.chars().count() > cap || (one_line && v.contains(['\n', '\r']))
        {
            bad.push(field);
        }
    }
    for (field, items, cap) in [
        ("principles", &p.principles, CAPS.principles),
        ("method", &p.method, CAPS.method),
        ("anti_patterns", &p.anti_patterns, CAPS.anti_patterns),
        ("escalation", &p.escalation, CAPS.escalation),
    ] {
        let item_bad = |i: &String| {
            i.trim().is_empty() || i.chars().count() > cap.chars || i.contains(['\n', '\r'])
        };
        if items.is_empty() || items.len() > cap.items || items.iter().any(item_bad) {
            bad.push(field);
        }
    }
    if !bad.is_empty() {
        bail!("the proposal breaks its caps: {}", bad.join(", "));
    }
    Ok(())
}
