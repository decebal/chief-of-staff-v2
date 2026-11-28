//! The hired repo's own skill set, read from `allsource-skills`. The repo's
//! `.allsource/roles.toml` names what each role runs with. cos asks the tool
//! for a role's names and a digest, never for paths: the export root is built
//! here from the digest, and its entries reach a hire's bundle through the
//! same checks as every other picked skill.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::skills::{Root, valid_name};
use crate::proc;

/// Names the binary; unset, `allsource-skills` is looked up on `PATH`.
pub const BIN_ENV: &str = "COS_ALLSOURCE_SKILLS";

/// The label of the repo's root in a catalog.
pub const LABEL: &str = "repo";

const MARKER: &str = ".allsource/roles.toml";
const NOT_FOUND: &str = "allsource-skills not found; repo has .allsource/roles.toml";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "lowercase")]
pub enum State {
    /// The export for `digest` is built and holds the role's skills.
    Fresh { digest: String },
    /// No export for the current graph yet; `install` writes one.
    Stale,
    /// The repo has no `.allsource/roles.toml`.
    Absent,
    /// Could not be read; `why` says how.
    Unavailable { why: String },
}

#[derive(Debug, Clone, PartialEq)]
pub struct RepoSkills {
    pub root: Option<Root>,
    pub names: Vec<String>,
    pub state: State,
}

impl RepoSkills {
    fn none(state: State) -> Self {
        Self {
            root: None,
            names: Vec::new(),
            state,
        }
    }

    fn unavailable(why: impl Into<String>) -> Self {
        Self::none(State::Unavailable { why: why.into() })
    }
}

/// The binary `COS_ALLSOURCE_SKILLS` names, else the first `allsource-skills`
/// on `path`. A configured path that is not a file is not searched for.
pub fn locate(configured: Option<&OsStr>, path: Option<&OsStr>) -> Option<PathBuf> {
    match configured.filter(|c| !c.is_empty()) {
        Some(c) => Some(PathBuf::from(c)).filter(|p| p.is_file()),
        None => std::env::split_paths(path?)
            .map(|d| d.join("allsource-skills"))
            .find(|p| p.is_file()),
    }
}

/// `repo`'s skills for `role`, read within `deadline`.
pub fn read(repo: &Path, role: &str, deadline: Duration) -> RepoSkills {
    let bin = locate(
        std::env::var_os(BIN_ENV).as_deref(),
        std::env::var_os("PATH").as_deref(),
    );
    read_using(bin.as_deref(), repo, role, deadline)
}

/// `read` with the binary given. `None` is a binary that was not found.
pub fn read_using(bin: Option<&Path>, repo: &Path, role: &str, deadline: Duration) -> RepoSkills {
    if !repo.join(MARKER).is_file() {
        return RepoSkills::none(State::Absent);
    }
    if !valid_role(role) {
        return RepoSkills::unavailable(format!("{role:?} is not a role id"));
    }
    let Some(bin) = bin else {
        return RepoSkills::unavailable(NOT_FOUND);
    };
    if deadline.is_zero() {
        return RepoSkills::unavailable("no time left to read them");
    }
    let mut cmd = Command::new(bin);
    cmd.args(["roles", role, "--json", "--repo"]).arg(repo);
    let out = match proc::run(&mut cmd, deadline) {
        Ok(out) => out,
        Err(e) => return RepoSkills::unavailable(format!("{e:#}")),
    };
    match out.status.code() {
        Some(0) => fresh(repo, role, &out.stdout),
        Some(3) => match serde_json::from_str::<StaleJson>(&out.stdout) {
            Ok(s) if s.version == 1 && s.state == "stale" && valid_digest(&s.digest) => {
                RepoSkills::none(State::Stale)
            }
            _ => RepoSkills::unavailable("stale without the closed-schema JSON"),
        },
        _ => RepoSkills::unavailable(explain(&out)),
    }
}

/// Builds the store when it is stale (`install`, then one more read), within
/// `budget`. Any failure is `Unavailable`; nothing here panics or retries.
pub fn build(repo: &Path, role: &str, budget: Duration) -> RepoSkills {
    let bin = locate(
        std::env::var_os(BIN_ENV).as_deref(),
        std::env::var_os("PATH").as_deref(),
    );
    build_using(bin.as_deref(), repo, role, budget)
}

/// `build` with the binary given.
pub fn build_using(bin: Option<&Path>, repo: &Path, role: &str, budget: Duration) -> RepoSkills {
    let until = Instant::now() + budget;
    let first = read_using(bin, repo, role, budget);
    if first.state != State::Stale {
        return first;
    }
    let left = until.saturating_duration_since(Instant::now());
    if left.is_zero() {
        return RepoSkills::unavailable("no time left to build them");
    }
    if let Err(why) = install_using(bin, repo, left) {
        return RepoSkills::unavailable(format!("install: {why}"));
    }
    let left = until.saturating_duration_since(Instant::now());
    read_using(bin, repo, role, left)
}

fn install_using(bin: Option<&Path>, repo: &Path, deadline: Duration) -> Result<(), String> {
    let bin = bin.ok_or_else(|| NOT_FOUND.to_owned())?;
    let mut cmd = Command::new(bin);
    cmd.args(["install", "--repo"]).arg(repo);
    let out = proc::run(&mut cmd, deadline).map_err(|e| format!("{e:#}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(explain(&out))
    }
}

/// Where the export for `digest` lives: the only path cos takes from a digest.
pub fn export_dir(repo: &Path, digest: &str) -> PathBuf {
    repo.join(".prime/skills")
        .join(digest)
        .join("export/claude")
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct FreshJson {
    version: u32,
    state: String,
    digest: String,
    role: String,
    known: bool,
    skills: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StaleJson {
    version: u32,
    state: String,
    digest: String,
}

fn fresh(repo: &Path, role: &str, stdout: &str) -> RepoSkills {
    let Ok(parsed) = serde_json::from_str::<FreshJson>(stdout) else {
        return RepoSkills::unavailable("roles printed no closed-schema JSON");
    };
    if parsed.version != 1 || parsed.state != "fresh" || parsed.role != role {
        return RepoSkills::unavailable("roles answered for another role or schema");
    }
    if !valid_digest(&parsed.digest) {
        return RepoSkills::unavailable("digest is not 64 lowercase hex");
    }
    if let Some(bad) = parsed.skills.iter().find(|n| !valid_name(n)) {
        return RepoSkills::unavailable(format!("{bad:?} is not a skill name"));
    }
    let dir = export_dir(repo, &parsed.digest);
    if !dir.is_dir() {
        return RepoSkills::none(State::Stale);
    }
    let mut names = parsed.skills;
    names.sort();
    names.dedup();
    RepoSkills {
        root: Some(Root {
            label: LABEL.into(),
            dir,
        }),
        names,
        state: State::Fresh {
            digest: parsed.digest,
        },
    }
}

/// `^[a-z0-9][a-z0-9-]{0,63}$`: the role ids the tool is given.
fn valid_role(s: &str) -> bool {
    let b = s.as_bytes();
    !b.is_empty()
        && b.len() <= 64
        && (b[0].is_ascii_lowercase() || b[0].is_ascii_digit())
        && b.iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
}

fn valid_digest(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

/// The exit code and the tool's last stderr line, in words.
fn explain(out: &proc::Output) -> String {
    let last = out.stderr.lines().rev().find(|l| !l.trim().is_empty());
    match (out.status.code(), last) {
        (Some(code), Some(line)) => format!("exit {code}: {}", line.trim()),
        (Some(code), None) => format!("exit {code}"),
        (None, _) => "killed".into(),
    }
}
