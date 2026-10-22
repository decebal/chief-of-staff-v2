//! Seeding a fresh worktree before its agent starts. A worktree has nothing
//! gitignored, and the harness sandbox refuses network, so an unseeded agent
//! meets a cold build or an install it cannot run. The supervisor seeds outside
//! the sandbox and verifies every step on disk; a step that did not happen
//! fails the agent before the harness starts.
//!
//! A repo that knows its own needs commits an executable `.cos/seed`, run in
//! the worktree with `COS_SEED_REPO` set to the main checkout. Otherwise:
//! clone `target/` for a Cargo project, and clone `node_modules/` (or install
//! from the lockfile) for a package.json project.

use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{Result, bail};

use crate::proc;

const STEP_DEADLINE: Duration = Duration::from_secs(300);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// The repo's own `.cos/seed`.
    Script(PathBuf),
    /// Copy-on-write clone of a gitignored directory from the main checkout.
    Clone { from: PathBuf, to: PathBuf },
    /// A lockfile install; `verify` must exist afterwards.
    Install {
        program: &'static str,
        args: &'static [&'static str],
        verify: PathBuf,
    },
}

impl Step {
    pub fn describe(&self) -> String {
        match self {
            Self::Script(p) => format!("ran {}", p.display()),
            Self::Clone { to, .. } => format!("cloned {}", name(to)),
            Self::Install { program, args, .. } => format!("{program} {}", args.join(" ")),
        }
    }
}

fn name(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// What seeding `tree`, cut from `repo`, would do.
pub fn plan(repo: &Path, tree: &Path) -> Vec<Step> {
    let script = tree.join(".cos/seed");
    if script
        .metadata()
        .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    {
        return vec![Step::Script(script)];
    }
    let mut steps = Vec::new();
    let clone = |dir: &str| {
        let (from, to) = (repo.join(dir), tree.join(dir));
        (from.is_dir() && !to.exists()).then_some(Step::Clone { from, to })
    };
    if tree.join("Cargo.toml").is_file() {
        steps.extend(clone("target"));
    }
    if tree.join("package.json").is_file() && !tree.join("node_modules").exists() {
        match clone("node_modules") {
            Some(step) => steps.push(step),
            None => steps.extend(install(tree)),
        }
    }
    steps
}

fn install(tree: &Path) -> Option<Step> {
    let verify = tree.join("node_modules");
    let (program, args): (&'static str, &'static [&'static str]) =
        if tree.join("bun.lock").is_file() || tree.join("bun.lockb").is_file() {
            ("bun", &["install", "--frozen-lockfile"])
        } else if tree.join("pnpm-lock.yaml").is_file() {
            ("pnpm", &["install", "--frozen-lockfile"])
        } else if tree.join("package-lock.json").is_file() {
            ("npm", &["ci"])
        } else if tree.join("yarn.lock").is_file() {
            ("yarn", &["install", "--frozen-lockfile"])
        } else {
            return None;
        };
    Some(Step::Install {
        program,
        args,
        verify,
    })
}

/// Runs every step, logging to `log`; returns what was done. Stops at the
/// first step that fails or leaves nothing on disk.
pub fn run(repo: &Path, tree: &Path, log: &Path) -> Result<Vec<String>> {
    let mut out = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log)?;
    let mut done = Vec::new();
    for step in plan(repo, tree) {
        let what = step.describe();
        writeln!(out, "== {what}")?;
        let mut cmd = match &step {
            Step::Script(p) => {
                let mut c = Command::new(p);
                c.env("COS_SEED_REPO", repo);
                c
            }
            Step::Clone { from, to } => clone_cmd(from, to),
            Step::Install { program, args, .. } => {
                let mut c = Command::new(program);
                c.args(*args);
                c
            }
        };
        cmd.current_dir(tree);
        let o = proc::run(&mut cmd, STEP_DEADLINE).map_err(|e| anyhow::anyhow!("{what}: {e:#}"))?;
        write!(out, "{}{}", o.stdout, o.stderr)?;
        if !o.status.success() {
            bail!(
                "{what}: {}",
                last_line(&o.stderr).unwrap_or("non-zero exit")
            );
        }
        let missing = match &step {
            Step::Script(_) => None,
            Step::Clone { to, .. } => (!to.is_dir()).then(|| to.clone()),
            Step::Install { verify, .. } => (!verify.is_dir()).then(|| verify.clone()),
        };
        if let Some(m) = missing {
            bail!("{what}: exited 0 but {} is missing", m.display());
        }
        done.push(what);
    }
    Ok(done)
}

#[cfg(target_os = "macos")]
fn clone_cmd(from: &Path, to: &Path) -> Command {
    let mut c = Command::new("cp");
    c.arg("-Rc").arg(from).arg(to);
    c
}

#[cfg(not(target_os = "macos"))]
fn clone_cmd(from: &Path, to: &Path) -> Command {
    let mut c = Command::new("cp");
    c.args(["-R", "--reflink=auto"]).arg(from).arg(to);
    c
}

fn last_line(s: &str) -> Option<&str> {
    s.lines().rev().find(|l| !l.trim().is_empty())
}
