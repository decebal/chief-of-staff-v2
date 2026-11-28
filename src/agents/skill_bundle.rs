//! Delivering a hire's skill set to Claude: the picked skills copied into a
//! session plugin under the run directory (`--plugin-dir`), outside the
//! worktree the judge reads, and the names `skillOverrides` switches off.
//! The repo's own `.claude/skills`, read from the seeded worktree, wins over
//! both: such a skill is neither copied nor hidden.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};

use super::seed;
use super::skills::{self, SkillSet, project_skills, tree_digest, valid_name};
use crate::proc;

pub const PLUGIN: &str = "cos-hire";

/// What a hire's command line carries for its skills.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Delivery {
    pub bundle: Option<PathBuf>,
    pub hidden: Vec<String>,
    /// Codex's `skills.config` value: the user's own entries, then one
    /// `enabled = false` per hidden skill.
    pub codex_config: Option<String>,
}

/// The `skills.config` array a Codex hire runs with, as TOML: the entries
/// already in `$CODEX_HOME/config.toml` (else `~/.codex/config.toml`) kept,
/// since `-c` replaces the whole array, then each hidden name disabled.
/// `None` when nothing is hidden. A config file that exists but cannot be
/// read, does not parse, or holds a `skills.config` that is not an array is
/// an error: running on would re-enable what the user switched off.
pub fn codex_config(hidden: &[String]) -> Result<Option<String>> {
    if hidden.is_empty() {
        return Ok(None);
    }
    let path = std::env::var_os("CODEX_HOME")
        .filter(|h| !h.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".codex")))
        .map(|d| d.join("config.toml"));
    let present = match &path {
        Some(p) => std::fs::exists(p).with_context(|| format!("look for {}", p.display()))?,
        None => false,
    };
    let mut entries: Vec<toml::Value> = match path {
        Some(p) if present => {
            let text =
                std::fs::read_to_string(&p).with_context(|| format!("read {}", p.display()))?;
            let table: toml::Table = text.parse().map_err(|e: toml::de::Error| {
                anyhow::anyhow!("{}: {}", p.display(), e.message())
            })?;
            match table.get("skills").and_then(|s| s.get("config")) {
                None => Vec::new(),
                Some(toml::Value::Array(a)) => a.clone(),
                Some(_) => bail!("{}: skills.config is not an array", p.display()),
            }
        }
        _ => Vec::new(),
    };
    for name in hidden.iter().filter(|n| valid_name(n)) {
        let mut e = toml::Table::new();
        e.insert("name".into(), toml::Value::String(name.clone()));
        e.insert("enabled".into(), toml::Value::Boolean(false));
        entries.push(toml::Value::Table(e));
    }
    Ok(Some(toml::Value::Array(entries).to_string()))
}

/// The names a Codex hire switches off: the plan's minus any the seeded
/// worktree holds itself.
pub fn hidden_codex(set: &SkillSet, worktree: &Path) -> Vec<String> {
    let project = project_skills(Some(worktree));
    set.hidden_codex
        .iter()
        .filter(|n| !project.contains(*n))
        .cloned()
        .collect()
}

pub fn bundle_dir(run_dir: &Path) -> PathBuf {
    run_dir.join("skills")
}

/// The names to switch off: the plan's hidden names minus any the seeded
/// worktree holds in its own `.claude/skills`.
pub fn hidden(set: &SkillSet, worktree: &Path) -> Vec<String> {
    let project = project_skills(Some(worktree));
    set.hidden
        .iter()
        .filter(|n| !project.contains(*n))
        .cloned()
        .collect()
}

/// Copies every bundled skill of `set` into `run_dir/skills` within
/// `budget`, verifying each tree digest before and after the copy. Builds in
/// `skills.tmp` and renames it, replacing anything an earlier launch left.
/// A skill is copied only when the catalog of `roots`, scanned again here,
/// holds an entry of that name at that canonical path: a path recorded in
/// `spec.json` is never trusted on its own. The configured roots are the
/// user's and, for a repo with a graph, the repo's export root (built by
/// `repo_skills` from the digest). Returns the names copied; none
/// copied leaves no directory.
pub fn bundle(
    run_dir: &Path,
    worktree: &Path,
    set: &SkillSet,
    roots: &[skills::Root],
    budget: Duration,
) -> Result<Vec<String>> {
    let until = Instant::now() + budget;
    let catalog = skills::catalog(roots);
    let project: BTreeSet<String> = project_skills(Some(worktree));
    let picked: Vec<&skills::Picked> = set
        .bundled()
        .filter(|p| !project.contains(&p.name))
        .collect();
    let tmp = run_dir.join("skills.tmp");
    let out = bundle_dir(run_dir);
    for stale in [&tmp, &out] {
        if stale.exists() {
            std::fs::remove_dir_all(stale).with_context(|| format!("clear {}", stale.display()))?;
        }
    }
    if picked.is_empty() {
        return Ok(Vec::new());
    }
    std::fs::create_dir_all(tmp.join(".claude-plugin"))?;
    std::fs::create_dir_all(tmp.join("skills"))?;
    std::fs::write(
        tmp.join(".claude-plugin/plugin.json"),
        serde_json::to_vec_pretty(&serde_json::json!({ "name": PLUGIN }))?,
    )?;
    let mut copied = Vec::new();
    for p in picked {
        let (Some(src), Some(digest)) = (&p.path, &p.digest) else {
            bail!("{} has no path or digest from its plan", p.name);
        };
        if !valid_name(&p.name) {
            bail!("{:?} is not a skill name", p.name);
        }
        let listed = catalog
            .skills
            .get(&p.name)
            .is_some_and(|es| es.iter().any(|e| &e.canonical == src));
        if !listed {
            bail!(
                "{} at {} is not in any skill root now: plan it again",
                p.name,
                src.display()
            );
        }
        if !std::fs::symlink_metadata(src.join("SKILL.md")).is_ok_and(|m| m.is_file()) {
            bail!("{}: no SKILL.md at {}", p.name, src.display());
        }
        if &tree_digest(src)? != digest {
            bail!(
                "{} changed since the hire was planned: plan it again",
                p.name
            );
        }
        let dst = tmp.join("skills").join(&p.name);
        let left = until.saturating_duration_since(Instant::now());
        if left.is_zero() {
            bail!("copying skills ran past its deadline");
        }
        let o = proc::run(&mut seed::clone_cmd(src, &dst), left)
            .with_context(|| format!("copy {}", p.name))?;
        if !o.status.success() {
            bail!("copy {}: {}", p.name, o.stderr.trim());
        }
        if &tree_digest(&dst).with_context(|| format!("{} as copied", p.name))? != digest {
            bail!("{} as copied does not match its digest", p.name);
        }
        copied.push(p.name.clone());
    }
    std::fs::rename(&tmp, &out)?;
    Ok(copied)
}
