//! Skills as the user sees them: every skill on this machine joined with what
//! they decided about it. `cos skills ls` prints it; the desktop's Skills
//! panel renders the same rows.

use std::path::Path;

use anyhow::{Result, anyhow};
use serde::Serialize;

use super::hire::SkillsAsk;
use super::policy::{self, Policy};
use super::skill_facts::{self, Mode, SOURCE};
use super::skills::{self, Asked, Catalog, Graph, Unreviewed, Verdict};
use crate::prime::Prime;

/// What every skills command reads: the catalog under the user's skill
/// roots, the graph, and Prime for writes.
pub struct Ctx {
    pub policy: Policy,
    pub catalog: Catalog,
    pub graph: Graph,
    pub prime: Result<Prime, String>,
    pub unreviewed: Unreviewed,
}

pub fn load(home: &Path, read: Mode) -> Result<Ctx> {
    let policy = policy::load(home);
    let src = SkillsAsk::from_env(Asked::default(), read)
        .source
        .ok_or_else(|| anyhow!("neither COS_SKILLS_HOME nor HOME is set"))?;
    let settings = policy.skills_settings();
    let catalog = skills::catalog(&skills::roots(&src.user_home, &settings.roots));
    let graph = skill_facts::read_graph(
        home,
        src.prime.as_ref().map_err(String::as_str),
        &policy,
        read,
    );
    Ok(Ctx {
        policy,
        catalog,
        graph,
        prime: src.prime,
        unreviewed: settings.unreviewed,
    })
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Row {
    pub name: String,
    /// Root labels holding it, in root order.
    pub roots: Vec<String>,
    pub description: String,
    /// `None` until the user decides.
    pub verdict: Option<Verdict>,
    pub roles: Vec<String>,
    pub requires: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// The fact was not written by cos (no `source: cos-skills`).
    pub outside_cos: bool,
}

pub fn rows(catalog: &Catalog, graph: &Graph) -> Vec<Row> {
    catalog
        .skills
        .iter()
        .map(|(name, entries)| {
            let fact = graph.facts.get(name);
            Row {
                name: name.clone(),
                roots: entries.iter().map(|e| e.root.clone()).collect(),
                description: entries
                    .iter()
                    .map(|e| e.description.as_str())
                    .find(|d| !d.is_empty())
                    .unwrap_or_default()
                    .to_owned(),
                verdict: fact.map(|f| f.verdict),
                roles: fact.map(|f| f.roles.clone()).unwrap_or_default(),
                requires: fact.map(|f| f.requires.clone()).unwrap_or_default(),
                note: fact.and_then(|f| f.note.clone()),
                outside_cos: fact.is_some_and(|f| f.source.as_deref() != Some(SOURCE)),
            }
        })
        .collect()
}

fn verdict_word(v: Option<Verdict>) -> &'static str {
    match v {
        Some(Verdict::Relevant) => "relevant",
        Some(Verdict::Irrelevant) => "irrelevant",
        None => "unreviewed",
    }
}

/// One line per skill: name, verdict, roles, requires, roots.
pub fn table(rows: &[Row]) -> String {
    let width = rows.iter().map(|r| r.name.len()).max().unwrap_or(4).max(4);
    let mut out = String::new();
    for r in rows {
        let mut line = format!("{:<width$}  {:<10}", r.name, verdict_word(r.verdict));
        if !r.roles.is_empty() {
            line.push_str(&format!("  roles: {}", r.roles.join(", ")));
        }
        if !r.requires.is_empty() {
            line.push_str(&format!("  requires: {}", r.requires.join(", ")));
        }
        line.push_str(&format!("  [{}]", r.roots.join(", ")));
        if r.outside_cos {
            line.push_str("  (written outside cos)");
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}
