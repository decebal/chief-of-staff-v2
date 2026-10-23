//! The verdict on a finished agent, from what it left rather than what it
//! said: the diff against the commit it started from, the contract's checks
//! re-run by `cos`, and whether the diff stayed inside the contract's scope.
//! The agent's summary is testimony; nothing here reads it.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::{Spec, State, Status};
use crate::proc;

/// One budget for the whole verdict: every git call and every check shares it.
pub const BUDGET: Duration = Duration::from_secs(300);

/// Lines of a check's output kept with its result.
const TAIL: usize = 20;

/// What the agent was hired to do, fixed at spawn.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Contract {
    /// The finish line, in words; a deliverable's `done_when`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub done_when: Option<String>,
    /// Shell commands run in the agent's workdir after it ends; each must exit 0.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub checks: Vec<String>,
    /// Paths the agent may change, relative to the repo; empty allows any.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scope: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// Every check passed, the diff is in scope and not empty.
    Accepted,
    /// Nothing failed, but nothing proves it either: look before trusting it.
    NeedsYou,
    /// The agent did not finish, or a check failed.
    Rejected,
}

impl std::fmt::Display for Outcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Accepted => "accepted",
            Self::NeedsYou => "needs_you",
            Self::Rejected => "rejected",
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Check {
    pub cmd: String,
    pub passed: bool,
    pub exit: Option<i32>,
    /// The last lines of its stdout and stderr, or why it did not finish.
    pub tail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Verdict {
    pub outcome: Outcome,
    pub reasons: Vec<String>,
    /// Files that differ from the agent's base commit, committed or not.
    pub changed: Vec<String>,
    pub out_of_scope: Vec<String>,
    pub checks: Vec<Check>,
    /// The agent ended claiming success and a check disagrees.
    pub overclaimed: bool,
    pub judged: String,
}

/// Judges within [`BUDGET`]. `bin` goes first on the checks' PATH: the
/// supervisor's build-tool shims, so a check's `cargo` waits for the build slot.
pub fn judge(spec: &Spec, state: &State, bin: Option<&Path>) -> Verdict {
    judge_within(spec, state, bin, BUDGET)
}

pub fn judge_within(spec: &Spec, state: &State, bin: Option<&Path>, budget: Duration) -> Verdict {
    let until = Instant::now() + budget.min(BUDGET);
    let c = &spec.contract;
    let mut reasons = Vec::new();

    let changed = match changed(spec, until) {
        Ok(files) => files,
        Err(e) => {
            reasons.push(format!("could not read the diff: {e:#}"));
            Vec::new()
        }
    };
    let out_of_scope: Vec<String> = if c.scope.is_empty() {
        Vec::new()
    } else {
        changed
            .iter()
            .filter(|f| !c.scope.iter().any(|s| Path::new(f).starts_with(s)))
            .cloned()
            .collect()
    };

    let finished = state.status == Status::Done;
    let checks: Vec<Check> = if finished {
        c.checks
            .iter()
            .map(|cmd| run_check(&spec.workdir, cmd, bin, until))
            .collect()
    } else {
        Vec::new()
    };
    let failed: Vec<&Check> = checks.iter().filter(|k| !k.passed).collect();

    let outcome = if !finished {
        reasons.push(format!(
            "the agent ended {}{}",
            state.status,
            state
                .error
                .as_deref()
                .map(|e| format!(": {e}"))
                .unwrap_or_default()
        ));
        Outcome::Rejected
    } else if !failed.is_empty() {
        for k in &failed {
            reasons.push(format!("check failed: `{}` ({})", k.cmd, exit_note(k)));
        }
        Outcome::Rejected
    } else {
        if c.checks.is_empty() {
            reasons.push("no checks in the contract: nothing proves it is done".into());
        }
        if !out_of_scope.is_empty() {
            reasons.push(format!(
                "{} file(s) changed outside its scope: {}",
                out_of_scope.len(),
                out_of_scope.join(", ")
            ));
        }
        if changed.is_empty() && spec.base.is_some() {
            reasons.push("no changes against its base commit".into());
        }
        if reasons.is_empty() {
            Outcome::Accepted
        } else {
            Outcome::NeedsYou
        }
    };

    Verdict {
        outcome,
        reasons,
        changed,
        out_of_scope,
        overclaimed: finished && !failed.is_empty(),
        checks,
        judged: jiff::Timestamp::now().to_string(),
    }
}

/// Tracked files that differ from the base (commits and uncommitted edits
/// alike), plus untracked files git does not ignore. No base, no diff.
fn changed(spec: &Spec, until: Instant) -> anyhow::Result<Vec<String>> {
    let Some(base) = spec.base.as_deref() else {
        return Ok(Vec::new());
    };
    let mut files = git_lines(&spec.workdir, &["diff", "--name-only", base], until)?;
    files.extend(git_lines(
        &spec.workdir,
        &["ls-files", "--others", "--exclude-standard"],
        until,
    )?);
    files.sort();
    files.dedup();
    Ok(files)
}

fn git_lines(dir: &Path, args: &[&str], until: Instant) -> anyhow::Result<Vec<String>> {
    let out = proc::run(
        Command::new("git").arg("-C").arg(dir).args(args),
        left(until),
    )?;
    anyhow::ensure!(
        out.status.success(),
        "git {}: {}",
        args.join(" "),
        out.stderr.trim()
    );
    Ok(out
        .stdout
        .lines()
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect())
}

fn run_check(workdir: &Path, cmd: &str, bin: Option<&Path>, until: Instant) -> Check {
    let fail = |tail: String| Check {
        cmd: cmd.to_owned(),
        passed: false,
        exit: None,
        tail,
    };
    let left = left(until);
    if left.is_zero() {
        return fail("not run: the judging budget was spent".into());
    }
    let mut c = Command::new("sh");
    c.arg("-c").arg(cmd).current_dir(workdir);
    if let Some(bin) = bin {
        let path = std::env::var_os("PATH").unwrap_or_default();
        let dirs = std::iter::once(PathBuf::from(bin)).chain(std::env::split_paths(&path));
        if let Ok(joined) = std::env::join_paths(dirs) {
            c.env("PATH", joined);
        }
    }
    match proc::run(&mut c, left) {
        Ok(out) => Check {
            cmd: cmd.to_owned(),
            passed: out.status.success(),
            exit: out.status.code(),
            tail: tail(&out.stdout, &out.stderr),
        },
        Err(e) => fail(format!("did not finish: {e:#}")),
    }
}

fn left(until: Instant) -> Duration {
    until.saturating_duration_since(Instant::now())
}

fn tail(stdout: &str, stderr: &str) -> String {
    let lines: Vec<&str> = stdout.lines().chain(stderr.lines()).collect();
    lines[lines.len().saturating_sub(TAIL)..].join("\n")
}

fn exit_note(k: &Check) -> String {
    match k.exit {
        Some(code) => format!("exit {code}"),
        None => k.tail.lines().next().unwrap_or("no exit code").to_owned(),
    }
}
