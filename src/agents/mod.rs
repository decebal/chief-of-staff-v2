//! `cos agents`: spin off coding agents (Claude Code, Codex) on the prompts a
//! brief or review hands out, then watch them.
//!
//! Each agent gets its own detached worktree and a `cos agents supervise`
//! process that owns the harness for its whole life: one deadline, kill the
//! process group, reap, record. Everything an agent leaves is under
//! `out/agents/<id>/`: `spec.json`, `state.json`, `events.jsonl` (the
//! harness's raw stream) and the worktree.

use std::fmt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::proc;
use crate::reports::{Brief, Review};
use crate::store::{self, Kind};

pub mod harness;
pub mod seed;
pub mod slots;
mod supervise;
pub mod view;

pub use supervise::supervise;

/// No agent runs longer than this, whatever it asks for.
pub const CEILING: Duration = Duration::from_secs(2 * 60 * 60);

/// A supervisor that has not recorded its pid within this is presumed dead.
const START_GRACE: Duration = Duration::from_secs(15);

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum, schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum Harness {
    Claude,
    Codex,
}

impl Harness {
    pub fn program(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
        }
    }
}

/// Where a spun-off prompt comes from: the latest saved brief or review.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum, schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum From {
    Brief,
    Review,
}

/// What an agent was asked to do, fixed at spawn.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Spec {
    pub id: String,
    pub harness: Harness,
    /// The binary started; the harness's own name unless overridden.
    pub program: String,
    pub model: Option<String>,
    pub repo: Option<PathBuf>,
    /// The commit the worktree was cut from, so `clean` can see new work.
    pub base: Option<String>,
    pub workdir: PathBuf,
    pub worktree: bool,
    pub title: String,
    pub source: String,
    pub prompt: String,
    pub deadline_secs: u64,
    pub budget_usd: f64,
    pub created: String,
    /// Agents allowed to run at once while this one waits; fixed at spawn.
    #[serde(default)]
    pub slots: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    #[default]
    Starting,
    /// Waiting for a run slot (`slots::run_slots`).
    Queued,
    /// Its worktree is being seeded.
    Seeding,
    Running,
    Done,
    Failed,
    Killed,
    TimedOut,
    /// The supervisor died without recording an outcome.
    Lost,
}

impl Status {
    pub fn terminal(self) -> bool {
        !matches!(
            self,
            Self::Starting | Self::Queued | Self::Seeding | Self::Running
        )
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Starting => "starting",
            Self::Queued => "queued",
            Self::Seeding => "seeding",
            Self::Running => "running",
            Self::Done => "done",
            Self::Failed => "failed",
            Self::Killed => "killed",
            Self::TimedOut => "timed_out",
            Self::Lost => "lost",
        })
    }
}

/// What the supervisor has seen so far; rewritten as the harness streams.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct State {
    pub status: Status,
    pub supervisor: Option<i32>,
    pub pgid: Option<i32>,
    /// The harness leader's start time (`started_at`), checked before any
    /// signal to `pgid`.
    pub pgid_started: Option<String>,
    pub started: Option<String>,
    pub ended: Option<String>,
    pub exit: Option<i32>,
    pub cost_usd: Option<f64>,
    pub tokens: Option<u64>,
    pub tool_calls: u32,
    /// The harness's session id, for `claude --resume` / `codex resume`.
    pub session: Option<String>,
    /// The latest activity line.
    pub last: String,
    /// The final message.
    pub summary: String,
    pub error: Option<String>,
    /// What seeding did to the worktree, in order.
    #[serde(default)]
    pub seeded: Vec<String>,
    /// Build tools shimmed onto the harness's PATH to share the build slot.
    #[serde(default)]
    pub shimmed: Vec<String>,
}

pub struct Request {
    pub harness: Harness,
    /// Overrides the harness binary (tests use a stand-in).
    pub program: Option<String>,
    pub model: Option<String>,
    pub repo: Option<PathBuf>,
    /// Cut a detached worktree from `repo`'s HEAD instead of working in it.
    pub worktree: bool,
    pub title: String,
    pub source: String,
    pub prompt: String,
    pub deadline: Duration,
    pub budget_usd: f64,
    /// Run slots; `None` takes `slots::run_slots()`.
    pub slots: Option<usize>,
}

/// One agent prompt from a saved run, numbered as the report numbers it.
#[derive(Debug, Clone, Serialize)]
pub struct Task {
    pub n: usize,
    pub title: String,
    pub prompt: String,
    pub source: String,
}

pub fn root(home: &Path) -> PathBuf {
    home.join("out/agents")
}

pub fn dir(home: &Path, id: &str) -> PathBuf {
    root(home).join(id)
}

pub fn events_path(home: &Path, id: &str) -> PathBuf {
    dir(home, id).join("events.jsonl")
}

/// The agent prompts in the latest saved brief (actions) or review (goals).
pub fn tasks(home: &Path, from: From) -> Result<Vec<Task>> {
    let pick = |day: &str, items: Vec<(String, String)>| -> Vec<Task> {
        items
            .into_iter()
            .enumerate()
            .filter(|(_, (_, p))| !p.trim().is_empty())
            .map(|(i, (title, prompt))| Task {
                n: i + 1,
                title,
                prompt,
                source: format!("{} {day} #{}", name(from), i + 1),
            })
            .collect()
    };
    Ok(match from {
        From::Brief => {
            let b = latest::<Brief>(home, Kind::Briefs)?;
            pick(
                &b.day,
                b.value
                    .needs_you
                    .into_iter()
                    .map(|a| (a.recommendation, a.agent_prompt))
                    .collect(),
            )
        }
        From::Review => {
            let r = latest::<Review>(home, Kind::Reviews)?;
            pick(
                &r.day,
                r.value
                    .goals
                    .into_iter()
                    .map(|g| (g.goal, g.agent_prompt))
                    .collect(),
            )
        }
    })
}

fn name(from: From) -> &'static str {
    match from {
        From::Brief => "brief",
        From::Review => "review",
    }
}

fn latest<T: DeserializeOwned>(home: &Path, kind: Kind) -> Result<store::Saved<T>> {
    store::latest::<T>(home, kind)?
        .context("nothing saved yet: run `cos brief` or `cos review` first")
}

/// The repo a prompt points at: the first path it names under `projects`
/// (`~/Projects/<repo>/…`) whose top directory is a git repo.
pub fn infer_repo(prompt: &str, projects: &Path) -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from)?;
    prompt
        .split(|c: char| c.is_whitespace() || "`'\"()[]<>,;".contains(c))
        .filter_map(|tok| {
            let tok = tok.trim_end_matches(['.', ':', '!', '?']);
            let path = match tok.strip_prefix("~/") {
                Some(rest) => home.join(rest),
                None if tok.starts_with('/') => PathBuf::from(tok),
                None => return None,
            };
            let first = path.strip_prefix(projects).ok()?.components().next()?;
            Some(projects.join(first))
        })
        .find(|repo| repo.join(".git").exists())
}

/// Creates the agent's directory and worktree, records its spec, and starts
/// its supervisor detached. Returns once the supervisor is started; the agent
/// itself runs in the background.
pub fn spawn(home: &Path, req: Request, cos: &Path) -> Result<Spec> {
    if req.prompt.trim().is_empty() {
        bail!("empty prompt");
    }
    let home = std::path::absolute(home)?;
    let id = new_id()?;
    let d = dir(&home, &id);
    std::fs::create_dir_all(&d)?;

    let (workdir, base) = match (&req.repo, req.worktree) {
        (Some(repo), true) => {
            let base = git(repo, &["rev-parse", "HEAD"])?;
            let tree = d.join("tree");
            git(
                repo,
                &[
                    "worktree",
                    "add",
                    "--detach",
                    &tree.to_string_lossy(),
                    &base,
                ],
            )?;
            (tree, Some(base))
        }
        (Some(repo), false) => (repo.clone(), None),
        (None, _) => {
            let scratch = d.join("scratch");
            std::fs::create_dir_all(&scratch)?;
            (scratch, None)
        }
    };

    let spec = Spec {
        id: id.clone(),
        harness: req.harness,
        program: req
            .program
            .unwrap_or_else(|| req.harness.program().to_owned()),
        model: req.model,
        worktree: req.worktree && req.repo.is_some(),
        repo: req.repo,
        base,
        workdir,
        title: req.title,
        source: req.source,
        prompt: req.prompt,
        deadline_secs: req.deadline.min(CEILING).as_secs(),
        budget_usd: req.budget_usd,
        created: jiff::Timestamp::now().to_string(),
        slots: Some(req.slots.unwrap_or_else(slots::run_slots)),
    };
    write_json(&d.join("spec.json"), &spec)?;
    write_state(&home, &id, &State::default())?;

    let log = std::fs::File::create(d.join("supervisor.log"))?;
    let mut cmd = Command::new(cos);
    cmd.arg("--home")
        .arg(&home)
        .args(["agents", "supervise", &id])
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log);
    // SAFETY: setsid is async-signal-safe and touches no memory. Its own
    // session keeps a closed terminal or a Ctrl-C there from reaching the agent.
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    let mut child = cmd.spawn().context("start the agent supervisor")?;
    // Reaped here so a long-lived parent (`cos mcp`) collects no zombies.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(spec)
}

/// Spec and state for one agent, reconciled against the process table.
pub fn read(home: &Path, id: &str) -> Result<(Spec, State)> {
    let d = dir(home, id);
    let spec: Spec = read_json(&d.join("spec.json")).with_context(|| format!("no agent {id}"))?;
    let state: State = read_json(&d.join("state.json")).unwrap_or_default();
    let state = reconcile(home, &spec, state)?;
    Ok((spec, state))
}

/// Every agent, newest first.
pub fn list(home: &Path) -> Result<Vec<(Spec, State)>> {
    let mut ids: Vec<String> = std::fs::read_dir(root(home))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().join("spec.json").exists())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    ids.sort_unstable_by(|a, b| b.cmp(a));
    ids.iter().map(|id| read(home, id)).collect()
}

/// A running state whose supervisor is gone becomes `lost`, and the harness
/// it left behind is stopped: nothing else would ever reap it.
fn reconcile(home: &Path, spec: &Spec, state: State) -> Result<State> {
    if state.status.terminal() {
        return Ok(state);
    }
    match state.supervisor {
        Some(pid) if is_cos(pid) => return Ok(state),
        None if age(&spec.created) < START_GRACE => return Ok(state),
        _ => {}
    }
    // `state` may predate the supervisor's last write; now that it is gone it
    // cannot write again, so the file on disk is final.
    let mut state: State = read_json(&dir(home, &spec.id).join("state.json")).unwrap_or(state);
    if state.status.terminal() {
        return Ok(state);
    }
    let mut note = "supervisor exited without recording an outcome".to_owned();
    if let Some(pgid) = harness_group(&state) {
        proc::kill_group(pgid);
        note.push_str(&format!("; stopped its harness group {pgid}"));
    }
    state.status = Status::Lost;
    state.ended = Some(jiff::Timestamp::now().to_string());
    state.error = Some(note);
    write_state(home, &spec.id, &state)?;
    Ok(state)
}

/// Asks the supervisor to stop the agent, and waits up to `wait` for it to
/// record that. A supervisor that does not answer is killed, and its harness
/// with it.
pub fn stop(home: &Path, id: &str, wait: Duration) -> Result<State> {
    let (_, state) = read(home, id)?;
    if state.status.terminal() {
        return Ok(state);
    }
    std::fs::write(dir(home, id).join("stop"), "")?;
    let until = Instant::now() + wait;
    while Instant::now() < until {
        let (_, s) = read(home, id)?;
        if s.status.terminal() {
            return Ok(s);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let (_, mut s) = read(home, id)?;
    if let Some(pid) = s.supervisor.filter(|&p| is_cos(p)) {
        // SAFETY: signalling the supervisor this agent recorded; no memory is touched.
        unsafe {
            libc::kill(pid, libc::SIGKILL);
        }
    }
    if let Some(pgid) = harness_group(&s) {
        proc::kill_group(pgid);
    }
    s.status = Status::Killed;
    s.ended = Some(jiff::Timestamp::now().to_string());
    s.error = Some("supervisor did not answer the stop request; killed".into());
    write_state(home, id, &s)?;
    Ok(s)
}

/// Removes a finished agent's directory and worktree. Refuses while the
/// worktree holds uncommitted changes or commits past its base, unless
/// `force`: that work exists nowhere else.
pub fn clean(home: &Path, id: &str, force: bool) -> Result<()> {
    let (spec, state) = read(home, id)?;
    if !state.status.terminal() {
        bail!(
            "{id} is {}; stop it first (`cos agents kill {id}`)",
            state.status
        );
    }
    if spec.worktree && spec.workdir.exists() {
        if !force {
            let dirty = git(&spec.workdir, &["status", "--porcelain"])?;
            let ahead = match &spec.base {
                Some(base) => git(
                    &spec.workdir,
                    &["log", "--oneline", &format!("{base}..HEAD")],
                )?,
                None => String::new(),
            };
            if !dirty.is_empty() || !ahead.is_empty() {
                bail!(
                    "{id} left work in {}:\n{dirty}\n{ahead}\ncommit or copy it, or pass --force",
                    spec.workdir.display()
                );
            }
        }
        let repo = spec.repo.as_deref().context("worktree without a repo")?;
        git(
            repo,
            &[
                "worktree",
                "remove",
                "--force",
                &spec.workdir.to_string_lossy(),
            ],
        )?;
    }
    std::fs::remove_dir_all(dir(home, id))?;
    Ok(())
}

pub(crate) fn write_state(home: &Path, id: &str, state: &State) -> Result<()> {
    write_json(&dir(home, id).join("state.json"), state)
}

/// Write-then-rename, so a reader never sees half a file.
fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(value)?)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    Ok(serde_json::from_slice(&std::fs::read(path)?)?)
}

fn git(dir: &Path, args: &[&str]) -> Result<String> {
    let out = proc::run(
        Command::new("git").arg("-C").arg(dir).args(args),
        Duration::from_secs(60),
    )?;
    if !out.status.success() {
        bail!("git {}: {}", args.join(" "), out.stderr.trim());
    }
    Ok(out.stdout.trim().to_owned())
}

fn new_id() -> Result<String> {
    use std::io::Read;
    let mut b = [0u8; 2];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut b)?;
    let now = jiff::Zoned::now();
    Ok(format!(
        "{}-{:02x}{:02x}",
        now.strftime("%Y%m%d-%H%M%S"),
        b[0],
        b[1]
    ))
}

fn age(timestamp: &str) -> Duration {
    timestamp
        .parse::<jiff::Timestamp>()
        .ok()
        .and_then(|t| jiff::Timestamp::now().duration_since(t).try_into().ok())
        .unwrap_or_default()
}

fn comm(pid: i32) -> Option<String> {
    let out = Command::new("ps")
        .args(["-o", "comm=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    let s = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    (!s.is_empty()).then_some(s)
}

/// `pid` is alive and is a `cos` binary, not a recycled pid.
fn is_cos(pid: i32) -> bool {
    proc::alive(pid)
        && comm(pid).is_some_and(|c| Path::new(&c).file_name().is_some_and(|f| f == "cos"))
}

/// When `pid` started, as `ps` prints it; with the pid, this names one process.
pub(crate) fn started_at(pid: i32) -> Option<String> {
    let out = Command::new("ps")
        .args(["-o", "lstart=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    let s = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    (!s.is_empty()).then_some(s)
}

/// The harness group recorded in `state` still has its original leader, so
/// killing the group cannot reach a recycled pid.
fn harness_group(state: &State) -> Option<i32> {
    let pgid = state.pgid?;
    let started = state.pgid_started.as_deref()?;
    (proc::group_alive(pgid) && started_at(pgid).as_deref() == Some(started)).then_some(pgid)
}
