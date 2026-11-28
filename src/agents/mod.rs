//! `cos agents`: hire coding agents (Claude Code, Codex) on the prompts a
//! brief hands out, run them in Herdr, and judge what they leave.
//!
//! cos plans the hire (`hire.rs`), cuts and seeds a detached worktree, then
//! starts the harness interactive in a Herdr terminal (`hc run`) named
//! `cos-<id>` and labelled `hire=<id>`. Herdr owns that process for its whole
//! life: you talk to it, approve its prompts and take it over there. cos
//! follows the terminal's status (`follow.rs`), stops it at its deadline, and
//! judges the worktree once when it reports `done`, ends, or is marked done.
//! Everything cos keeps is under `out/agents/<id>/`: `spec.json`,
//! `state.json`, `seed.log`, `screen.txt` and the worktree.

use std::fmt;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::business::Triple;
use crate::herdr;
use crate::proc;
use crate::reports::Brief;
use crate::store::{self, Kind};

pub mod events;
pub mod fit;
pub mod follow;
pub mod harness;
pub mod hire;
pub mod judge;
pub mod ledger;
pub mod osc;
pub mod policy;
pub mod seed;
pub mod slots;
pub mod timeline;
pub mod view;

pub use follow::{Follow, mark_done, open_in_herdr, pause, say, screen, tick};
pub use judge::{Contract, Expects, Outcome, Verdict};

/// No agent runs longer than this, whatever it asks for.
pub const CEILING: Duration = Duration::from_secs(2 * 60 * 60);

/// A run whose preparing process has not been seen within this is presumed
/// to have lost it.
pub const START_GRACE: Duration = Duration::from_secs(15);

/// What a run from before Herdr followed it says on its page.
pub const LEGACY: &str =
    "ran before cos followed hires in Herdr: it cannot be resumed; hire again to carry the work on";

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

/// Where a spun-off prompt comes from in the latest saved brief: its actions
/// (`brief`) or its week's goals (`goals`).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum, schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum From {
    Brief,
    Goals,
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
    pub created: String,
    /// Agents allowed to run at once when this one was hired.
    #[serde(default)]
    pub slots: Option<usize>,
    /// What it is judged against.
    #[serde(default)]
    pub contract: Contract,
    /// The staff role it was hired into (`context/agents.json`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// Its Herdr terminal's name (`cos-<id>`); `None` for a run from before
    /// cos followed hires in Herdr.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal: Option<String>,
    /// The business's workspace and projects, pinned on the agent's command
    /// so a `cos` it runs resolves the business that hired it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub projects: Option<PathBuf>,
}

impl Spec {
    /// Run before cos followed hires in Herdr: readable, never resumed.
    pub fn legacy(&self) -> bool {
        self.terminal.is_none()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    #[default]
    Starting,
    /// Read from runs from before Herdr only.
    Queued,
    /// Its worktree is being seeded.
    Seeding,
    /// Its Herdr terminal is live.
    Running,
    Done,
    Failed,
    Killed,
    TimedOut,
    /// It ended without cos seeing how.
    Lost,
}

impl Status {
    pub fn terminal(self) -> bool {
        !matches!(
            self,
            Self::Starting | Self::Queued | Self::Seeding | Self::Running
        )
    }

    /// Being prepared, before Herdr has it.
    pub fn preparing(self) -> bool {
        matches!(self, Self::Starting | Self::Queued | Self::Seeding)
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

/// What cos last saw of a run's Herdr terminal. Every word here is Herdr's.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Seen {
    /// `t7`: the terminal for good on this core.
    pub address: String,
    /// `idle`, `working`, `done`, `blocked` or `error`; `None` when the
    /// program never said, or has exited.
    pub word: Option<String>,
    /// For `blocked`: `permission`, `question` or `auth`.
    pub kind: Option<String>,
    pub msg: Option<String>,
    /// Goes up each time it stops working.
    pub turn: u64,
    pub live: bool,
    pub exit_code: Option<i32>,
    /// Why its program ended, in Herdr's word (`exited`, `killed`, …).
    pub reason: Option<String>,
    /// hc's word on the first message, when the launch gave one; the prompt
    /// is otherwise on the agent's command line.
    pub first_message: Option<herdr::FirstMessage>,
    /// When cos last read it.
    pub at: String,
    /// When cos pressed its stop key at the deadline.
    pub interrupted: Option<String>,
}

impl Seen {
    /// Herdr says it is waiting on you, and on what.
    pub fn blocked(&self) -> Option<String> {
        (self.live && self.word.as_deref() == Some("blocked"))
            .then(|| self.kind.clone().unwrap_or_else(|| "a question".into()))
    }

    /// The first message did not go in.
    pub fn unsent(&self) -> Option<String> {
        self.first_message.as_ref().filter(|f| !f.sent).map(|f| {
            f.reason
                .clone()
                .unwrap_or_else(|| "Herdr gave no reason".into())
        })
    }
}

/// What cos knows of a run; rewritten as it changes.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    pub status: Status,
    pub started: Option<String>,
    pub ended: Option<String>,
    pub exit: Option<i32>,
    pub error: Option<String>,
    /// What seeding did to the worktree, in order.
    pub seeded: Vec<String>,
    /// Build tools shimmed onto the agent's PATH to share the build slot.
    pub shimmed: Vec<String>,
    /// Set once judged.
    pub verdict: Option<Verdict>,
    /// What made cos judge it: "done at turn 2", "it ended", "marked done".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub judged_on: Option<String>,
    /// What you found when you ran what it drafted; never written by the agent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recorded: Option<Recorded>,
    /// Its Herdr terminal, as last seen.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub herdr: Option<Seen>,
    /// The process preparing it before Herdr has it, and when that process
    /// started (`started_at`), so a recycled pid is not taken for it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub launcher: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub launcher_started: Option<String>,
    /// Measured from the stream before hires ran interactive in Herdr; runs
    /// in Herdr leave them empty.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supervisor: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pgid: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pgid_started: Option<String>,
}

/// The outcome you record for a run, e.g. after running a tester's script.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recorded {
    pub outcome: Tested,
    #[serde(default)]
    pub notes: String,
    /// Links or paths to what shows it: logs, screenshots, a ticket comment.
    #[serde(default)]
    pub evidence: Vec<String>,
    #[serde(default)]
    pub at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Tested {
    Passed,
    Failed,
    Blocked,
}

impl fmt::Display for Tested {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Passed => "passed",
            Self::Failed => "failed",
            Self::Blocked => "blocked",
        })
    }
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
    /// Run slots; `None` takes `slots::run_slots()`.
    pub slots: Option<usize>,
    pub contract: Contract,
    /// Fields the caller chose; `context/agents.json` fills the rest.
    pub explicit: policy::Explicit,
    /// A role from `context/agents.json`: its contract and mandate fill what
    /// the request left empty.
    pub role: Option<String>,
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

/// The colour a run's role shows in the window: 1..=6, the same everywhere.
pub fn identity(role: Option<&str>) -> u8 {
    let key = role.unwrap_or("unassigned");
    let hash = key.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    });
    1 + (hash % 6) as u8
}

/// A run directory checked by `run_dir`: the only way from a run id to a path.
pub struct RunDir {
    path: PathBuf,
}

impl RunDir {
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Resolves a run id under `home`'s `out/agents`, or refuses it. A refused id
/// reads, writes, runs and kills nothing.
pub fn run_dir(home: &Path, id: &str) -> Result<RunDir> {
    let mut parts = Path::new(id).components();
    match (parts.next(), parts.next()) {
        (Some(Component::Normal(part)), None) if part == id => {}
        _ => bail!("not a run id: {id:?}"),
    }
    let root = std::fs::canonicalize(root(home)).with_context(|| format!("no agent {id}"))?;
    let path = root.join(id);
    let meta = std::fs::symlink_metadata(&path).with_context(|| format!("no agent {id}"))?;
    if !meta.is_dir() {
        bail!("{id} is not a run directory");
    }
    let spec: Spec =
        read_json(&path.join("spec.json")).with_context(|| format!("no agent {id}"))?;
    if spec.id != id {
        bail!("{id}'s spec.json names run {}", spec.id);
    }
    Ok(RunDir { path })
}

/// The agent prompts in the latest saved brief: its actions or its week's goals.
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
    let b = store::latest::<Brief>(home, Kind::Briefs)?
        .context("nothing saved yet: run `cos brief` first")?;
    let items = match from {
        From::Brief => b
            .value
            .needs_you
            .into_iter()
            .map(|a| (a.recommendation, a.agent_prompt))
            .collect(),
        From::Goals => b
            .value
            .week
            .goals
            .into_iter()
            .map(|g| (g.goal, g.agent_prompt))
            .collect(),
    };
    Ok(pick(&b.day, items))
}

/// A tracked deliverable as an agent's work: the kit's handoff for an agent
/// as the prompt, its title, and its finish line. The kit writes `doneWhen`;
/// `done_when` is read too, for state written before that.
pub fn deliverable_task(
    state: &serde_json::Value,
    profile: Option<&serde_json::Value>,
    id: &str,
) -> Result<(Task, Option<String>)> {
    let prompt = crate::handoff::deliverable(state, profile, id, crate::handoff::Receiver::Agent)?;
    let found = crate::state::find(state, "deliverables", id);
    let title = found
        .and_then(|d| d["title"].as_str())
        .unwrap_or(id)
        .to_owned();
    let done_when = found
        .and_then(|d| d["doneWhen"].as_str().or_else(|| d["done_when"].as_str()))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    let task = Task {
        n: 0,
        title,
        prompt,
        source: format!("deliverable {id}"),
    };
    Ok((task, done_when))
}

fn name(from: From) -> &'static str {
    match from {
        From::Brief => "brief",
        From::Goals => "brief goals",
    }
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

/// Hires and starts the agent in Herdr: refuses without Herdr or a free run
/// slot, cuts and seeds its worktree, then `hc run`. Returns once Herdr has
/// it. `cos` is the binary the agent's build shims call back into.
pub fn spawn(triple: &Triple, req: Request, cos: &Path) -> Result<Spec> {
    let spec = create(triple, req)?;
    let home = std::path::absolute(&triple.home)?;
    launch(&home, &spec.id, cos)?;
    read_json(&dir(&home, &spec.id).join("spec.json"))
}

/// `spawn`, returning once the run is recorded; seeding and the launch go
/// on in a thread of this process, which a long-lived caller (the window,
/// `cos mcp`) keeps alive. If this process ends first the run reads `lost`.
pub fn spawn_background(triple: &Triple, req: Request, cos: &Path) -> Result<Spec> {
    let spec = create(triple, req)?;
    let home = std::path::absolute(&triple.home)?;
    let (id, cos) = (spec.id.clone(), cos.to_path_buf());
    std::thread::spawn(move || {
        if let Err(e) = launch(&home, &id, &cos) {
            eprintln!("cos: {id} did not reach Herdr: {e:#}");
        }
    });
    Ok(spec)
}

/// Records the run (its directory, worktree and spec) after the checks
/// that refuse it: no Herdr, no free slot, an empty prompt.
pub fn create(triple: &Triple, req: Request) -> Result<Spec> {
    record_run(triple, req, true)
}

/// The spec and worktree a hire would run in, policy and role applied and
/// recorded on disk, without asking Herdr or launching anything. For tests.
pub fn prepare_spec(triple: &Triple, req: Request) -> Result<Spec> {
    record_run(triple, req, false)
}

fn record_run(triple: &Triple, mut req: Request, herdr_up: bool) -> Result<Spec> {
    if req.prompt.trim().is_empty() {
        bail!("empty prompt");
    }
    let home = std::path::absolute(&triple.home)?;
    let explicit = req.explicit;
    policy::load(&home).apply(&mut req, explicit)?;
    if herdr_up {
        herdr::require()?;
    }
    std::fs::create_dir_all(root(&home))?;
    let n = req.slots.unwrap_or_else(slots::run_slots);
    let _held = slots::hire_lock(&home)?;
    let busy = live_runs(&home);
    if busy >= n {
        bail!(
            "all {n} run slots are taken by hires still running: wait for one to end, or stop one (COS_AGENT_SLOTS raises the number)"
        );
    }
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
        // The judge diffs against this; `clean` only reads it for worktrees.
        (Some(repo), false) => (repo.clone(), git(repo, &["rev-parse", "HEAD"]).ok()),
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
        created: jiff::Timestamp::now().to_string(),
        slots: Some(n),
        contract: req.contract,
        role: req.role,
        terminal: Some(follow::terminal_name(&id)),
        workspace: Some(std::path::absolute(&triple.workspace)?),
        projects: Some(std::path::absolute(&triple.projects)?),
    };
    write_json(&d.join("spec.json"), &spec)?;
    events::record(
        &home,
        &id,
        timeline::Event::new(
            jiff::Timestamp::now().to_string(),
            timeline::Kind::Hired,
            timeline::By::Cos,
        ),
    );
    let me = std::process::id() as i32;
    let state = State {
        launcher: Some(me),
        launcher_started: started_at(me),
        ..State::default()
    };
    write_state(&home, &id, &state)?;
    Ok(spec)
}

/// Runs a hire that is not yet working: until the worktree is seeded and its
/// Herdr terminal is live. A `stop` file in the run directory ends it.
pub fn launch(home: &Path, id: &str, cos: &Path) -> Result<()> {
    let d = run_dir(home, id)?.path;
    let spec: Spec = read_json(&d.join("spec.json"))?;
    let mut state: State = read_json(&d.join("state.json")).unwrap_or_default();
    let stop_file = d.join("stop");
    let stopped = || stop_file.exists();
    let prepared = prepare(home, &spec, &mut state, cos);
    if stopped() {
        let _ = std::fs::remove_file(&stop_file);
        return follow::end(
            home,
            &spec,
            &mut state,
            Status::Killed,
            Some("stopped on request".into()),
        );
    }
    if let Err(e) = prepared {
        let why = format!("{e:#}");
        follow::end(home, &spec, &mut state, Status::Failed, Some(why.clone()))?;
        bail!(why);
    }
    let argv = environment(home, &spec, &state, cos, harness::argv(&spec));
    let name = follow::terminal_name(id);
    let mut labels = vec![
        ("hire".to_owned(), id.to_owned()),
        ("cos".to_owned(), "hire".to_owned()),
        ("harness".to_owned(), spec.harness.program().to_owned()),
    ];
    if let Some(r) = &spec.role {
        labels.push(("role".to_owned(), r.clone()));
    }
    let launched = herdr::run(&herdr::Launch {
        name: &name,
        cwd: &spec.workdir,
        labels: &labels,
        argv: &argv,
    });
    match launched {
        Ok(t) => {
            state.herdr = Some(follow::seen(&t, None));
            state.status = Status::Running;
            let at = jiff::Timestamp::now().to_string();
            state.started = Some(at.clone());
            state.launcher = None;
            state.launcher_started = None;
            if let Some(why) = state.herdr.as_ref().and_then(Seen::unsent) {
                state.error = Some(format!(
                    "Herdr did not send it the work ({why}): read its screen in Herdr before sending it again"
                ));
            }
            write_state(home, id, &state)?;
            events::record(
                home,
                id,
                timeline::Event::new(at.clone(), timeline::Kind::Started, timeline::By::Cos),
            );
            if let Some(h) = state.herdr.as_ref()
                && let Some(word) = h.word.as_deref().and_then(timeline::word)
            {
                events::record(
                    home,
                    id,
                    timeline::Event {
                        word: Some(word),
                        turn: Some(h.turn),
                        ..timeline::Event::new(at, timeline::Kind::Word, timeline::By::Herdr)
                    },
                );
            }
            Ok(())
        }
        Err(e) => {
            let why = format!("Herdr did not start it: {e:#}");
            follow::end(home, &spec, &mut state, Status::Failed, Some(why.clone()))?;
            bail!(why)
        }
    }
}

/// Seeds a worktree hire and writes the build-tool shims.
fn prepare(home: &Path, spec: &Spec, state: &mut State, cos: &Path) -> Result<()> {
    let d = dir(home, &spec.id);
    if let (true, Some(repo)) = (spec.worktree, spec.repo.as_deref()) {
        state.status = Status::Seeding;
        write_state(home, &spec.id, state)?;
        state.seeded =
            seed::run(repo, &spec.workdir, &d.join("seed.log")).context("seed the worktree")?;
    }
    let mut path = std::env::var("PATH").unwrap_or_default();
    // A cos started from the Dock has no login PATH: the tools in
    // ~/.cargo/bin are looked for there too.
    if let Some(cargo) = std::env::var_os("HOME").map(|h| Path::new(&h).join(".cargo/bin"))
        && cargo.is_dir()
    {
        path = format!("{path}:{}", cargo.display());
    }
    state.shimmed = slots::write_shims(&d.join("bin"), cos, &slots::build_lock()?, &path)?;
    write_state(home, &spec.id, state)
}

/// The agent's command line with what it runs beside: the business it was
/// hired for, where the run is, and the build shims first on its PATH. Herdr
/// starts programs in its own environment, not cos's, so the business is
/// pinned here: `COS_BUSINESS` is cleared and the pinned directories decide
/// it. PATH is extended, not replaced.
fn environment(
    home: &Path,
    spec: &Spec,
    state: &State,
    cos: &Path,
    argv: Vec<String>,
) -> Vec<String> {
    let mut a = vec![
        "/usr/bin/env".to_owned(),
        "-u".to_owned(),
        "COS_BUSINESS".to_owned(),
        format!("COS_HOME={}", home.display()),
    ];
    let pins = [
        ("COS_WORKSPACE", &spec.workspace),
        ("COS_PROJECTS", &spec.projects),
    ];
    for (k, v) in pins {
        if let Some(v) = v {
            a.push(format!("{k}={}", v.display()));
        }
    }
    a.extend([
        format!("COS_RUN_ID={}", spec.id),
        format!("COS_BIN={}", cos.display()),
    ]);
    if !state.shimmed.is_empty() {
        a.extend([
            "/bin/sh".to_owned(),
            "-c".to_owned(),
            r#"PATH="$0:$PATH" exec "$@""#.to_owned(),
            dir(home, &spec.id)
                .join("bin")
                .to_string_lossy()
                .into_owned(),
        ]);
    }
    a.extend(argv);
    a
}

/// Why a run needs you now, in words; `None` when it does not. Herdr's
/// `blocked` first: a run waiting on you there waits on nothing else.
pub fn needs(spec: &Spec, state: &State) -> Option<String> {
    let seen = state.herdr.as_ref();
    if let Some(kind) = seen.and_then(Seen::blocked) {
        return Some(format!("waiting on you in Herdr ({kind})"));
    }
    if !state.status.terminal() && seen.and_then(Seen::unsent).is_some() {
        return Some("its first message did not go in: read its screen in Herdr".into());
    }
    if state.status == Status::Killed || (spec.legacy() && !state.status.terminal()) {
        return None;
    }
    state
        .verdict
        .as_ref()
        .filter(|v| v.outcome == Outcome::NeedsYou)
        .map(|_| {
            if state.status.terminal() {
                "ended: needs you".into()
            } else {
                "judged: needs you".into()
            }
        })
}

/// Hires still being prepared or live in Herdr: what the run slots count.
pub fn live_runs(home: &Path) -> usize {
    list(home)
        .map(|l| l.runs)
        .unwrap_or_default()
        .iter()
        .filter(|(spec, st)| !spec.legacy() && !st.status.terminal())
        .count()
}

/// Every run that resolves, newest first. An id that fails `run_dir` or `read`
/// is listed in `skipped` instead, so one bad directory hides nothing else.
#[derive(Default)]
pub struct Listing {
    pub runs: Vec<(Spec, State)>,
    pub skipped: Vec<String>,
}

/// Spec and state for one agent, reconciled.
pub fn read(home: &Path, id: &str) -> Result<(Spec, State)> {
    let (spec, state) = read_raw(home, id)?;
    let state = reconcile(home, &spec, state)?;
    Ok((spec, state))
}

/// Spec and state as they are on disk.
pub(crate) fn read_raw(home: &Path, id: &str) -> Result<(Spec, State)> {
    let d = run_dir(home, id)?.path;
    let spec: Spec = read_json(&d.join("spec.json")).with_context(|| format!("no agent {id}"))?;
    let state: State = read_json(&d.join("state.json")).unwrap_or_default();
    Ok((spec, state))
}

fn run_ids(home: &Path) -> Vec<String> {
    let mut ids: Vec<String> = std::fs::read_dir(root(home))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().join("spec.json").exists())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    ids.sort_unstable_by(|a, b| b.cmp(a));
    ids
}

pub fn list(home: &Path) -> Result<Listing> {
    let mut listing = Listing::default();
    for id in run_ids(home) {
        match read(home, &id) {
            Ok(run) => listing.runs.push(run),
            Err(_) => listing.skipped.push(id),
        }
    }
    Ok(listing)
}

/// A hire's place in the fleet, the one mapping from a stored state to what
/// the window and Today show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Phase {
    Starting,
    Working,
    NeedsYou,
    Paused,
    Done,
    Failed,
    Lost,
    TimedOut,
    Stopped,
}

/// The fleet phase of a run at the moment `clock` was read. A running hire is
/// paused while a pause is open, needs you while Herdr says blocked, its first
/// message did not go in or its verdict asks for you, and works otherwise.
pub fn phase(state: &State, clock: &timeline::Summary) -> Phase {
    match state.status {
        Status::Starting | Status::Queued | Status::Seeding => Phase::Starting,
        Status::Running if clock.paused_now => Phase::Paused,
        Status::Running => {
            let seen = state.herdr.as_ref();
            let blocked = seen.is_some_and(|h| h.word.as_deref() == Some("blocked"));
            let unsent = seen.and_then(Seen::unsent).is_some();
            let judged = state
                .verdict
                .as_ref()
                .is_some_and(|v| v.outcome == Outcome::NeedsYou);
            if blocked || unsent || judged {
                Phase::NeedsYou
            } else {
                Phase::Working
            }
        }
        Status::Done => Phase::Done,
        Status::Failed => Phase::Failed,
        Status::Killed => Phase::Stopped,
        Status::TimedOut => Phase::TimedOut,
        Status::Lost => Phase::Lost,
    }
}

/// Done, and ended on `now`'s calendar day in `tz` (the business profile's).
pub fn done_today(state: &State, tz: &jiff::tz::TimeZone, now: jiff::Timestamp) -> bool {
    state.status == Status::Done
        && state
            .ended
            .as_deref()
            .and_then(|e| e.parse::<jiff::Timestamp>().ok())
            .is_some_and(|e| {
                e.to_zoned_datetime(tz.clone()).date() == now.to_zoned_datetime(tz.clone()).date()
            })
}

/// One hire as Today and the window's live updates show it: no worktree,
/// prompt or process detail, and the run id only for the actions that need it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HireView {
    pub id: String,
    pub title: String,
    pub role: Option<String>,
    pub identity: u8,
    pub phase: Phase,
    pub active_ms: u64,
    pub paused_now: bool,
    pub started: Option<String>,
    pub ended: Option<String>,
    pub done_today: bool,
    pub deadline_secs: u64,
    /// Herdr's last word for its terminal, and its message.
    pub word: Option<String>,
    pub msg: Option<String>,
    pub source: String,
}

/// `spec` and `state` as a [`HireView`] at `now`. Reads the run's event log
/// for its clock; writes nothing.
pub fn hire_view(
    home: &Path,
    spec: &Spec,
    state: &State,
    tz: &jiff::tz::TimeZone,
    now: jiff::Timestamp,
) -> HireView {
    let clock = events::tail_summary(home, &spec.id, now, state)
        .unwrap_or_else(|_| timeline::stored_summary(state, now));
    let seen = state.herdr.as_ref();
    HireView {
        id: spec.id.clone(),
        title: spec.title.clone(),
        role: spec.role.clone(),
        identity: identity(spec.role.as_deref()),
        phase: phase(state, &clock),
        active_ms: clock.active_ms,
        paused_now: clock.paused_now,
        started: state.started.clone(),
        ended: state.ended.clone(),
        done_today: done_today(state, tz, now),
        deadline_secs: spec.deadline_secs,
        word: seen.and_then(|h| h.word.clone()),
        msg: seen.and_then(|h| h.msg.clone()),
        source: spec.source.clone(),
    }
}

/// Every run's spec and state exactly as stored, newest first. A run whose id
/// is refused or whose spec does not read is left out. Unlike [`list`] it
/// never reconciles: nothing is written and no process is signalled.
pub fn stored(home: &Path) -> Vec<(Spec, State)> {
    run_ids(home)
        .into_iter()
        .filter_map(|id| read_raw(home, &id).ok())
        .collect()
}

/// Every hire as last stored, newest first, for render paths. It reads each
/// run's `spec.json`, `state.json` and event log and nothing else: it never
/// reconciles, signals or writes, so a run lost while no follower ran shows
/// as last stored until `agents::tick` or `cos agents watch` reconciles it.
pub fn snapshot(home: &Path, tz: &jiff::tz::TimeZone, now: jiff::Timestamp) -> Vec<HireView> {
    stored(home)
        .iter()
        .map(|(spec, state)| hire_view(home, spec, state, tz, now))
        .collect()
}

/// Run counts under one home, for the picker's badge.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Counts {
    /// Includes the runs counted in `starting`.
    pub live: usize,
    pub lost: usize,
    /// Being prepared, before Herdr has it.
    pub starting: usize,
    /// Runs whose spec or state cannot be read, or whose id is refused.
    pub unreadable: usize,
}

/// What a run that has not ended is, from its record and the kernel alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Open {
    /// Herdr has it.
    Live,
    /// Being prepared by a process still there, or within `START_GRACE`.
    Starting,
    /// Its preparing process is gone, or it is from before Herdr.
    Lost,
}

fn open(spec: &Spec, state: &State) -> Open {
    if spec.legacy() {
        return Open::Lost;
    }
    if state.herdr.is_some() || !state.status.preparing() {
        return Open::Live;
    }
    if launcher_alive(state) || age(&spec.created) < START_GRACE {
        Open::Starting
    } else {
        Open::Lost
    }
}

fn launcher_alive(state: &State) -> bool {
    state.launcher.is_some_and(|pid| {
        proc::alive(pid) && started_at(pid).as_deref() == state.launcher_started.as_deref()
    })
}

/// Counts runs without reconciling them: nothing is written, nothing spawned
/// and no process signalled.
pub fn count(home: &Path) -> Result<Counts> {
    let mut counts = Counts::default();
    for run in open_runs(home) {
        let Some((spec, state)) = run else {
            counts.unreadable += 1;
            continue;
        };
        match open(&spec, &state) {
            Open::Live => counts.live += 1,
            Open::Starting => {
                counts.live += 1;
                counts.starting += 1;
            }
            Open::Lost => counts.lost += 1,
        }
    }
    Ok(counts)
}

/// The processes preparing hires right now, read the way `count` reads:
/// when one of them ends, the run it was preparing changes how it counts.
pub fn live_launchers(home: &Path) -> Vec<i32> {
    open_runs(home)
        .into_iter()
        .flatten()
        .filter(|(spec, state)| open(spec, state) == Open::Starting)
        .filter_map(|(_, state)| state.launcher)
        .collect()
}

/// The runs Herdr has that have not ended, read the way `count` reads: a
/// follower with none of them has nothing to ask Herdr.
pub fn in_herdr(home: &Path) -> Vec<(Spec, State)> {
    open_runs(home)
        .into_iter()
        .flatten()
        .filter(|(spec, state)| !spec.legacy() && state.herdr.is_some())
        .collect()
}

/// Runs under `home` that have not ended, without reconciling them; `None` for
/// a run whose spec or state cannot be read, or whose id is refused.
fn open_runs(home: &Path) -> Vec<Option<(Spec, State)>> {
    let Ok(root) = std::fs::canonicalize(root(home)) else {
        return Vec::new();
    };
    let mut runs = Vec::new();
    for id in run_ids(home) {
        let path = root.join(&id);
        if !std::fs::symlink_metadata(&path).is_ok_and(|m| m.is_dir()) {
            runs.push(None);
            continue;
        }
        let spec: Result<Spec> = read_json(&path.join("spec.json"));
        let state: Result<State> = read_json(&path.join("state.json"));
        let (Ok(spec), Ok(state)) = (spec, state) else {
            runs.push(None);
            continue;
        };
        if spec.id != id {
            runs.push(None);
            continue;
        }
        if !state.status.terminal() {
            runs.push(Some((spec, state)));
        }
    }
    runs
}

/// A run from before Herdr still marked live ends `lost`, and the harness
/// group its supervisor left is stopped. A hire whose preparing process is
/// gone before Herdr had it ends `lost` too.
fn reconcile(home: &Path, spec: &Spec, mut state: State) -> Result<State> {
    if state.status.terminal() {
        return Ok(state);
    }
    if spec.legacy() {
        if let Some(pid) = state.supervisor.filter(|&p| is_cos(p)) {
            proc::signal_pid(pid, libc::SIGKILL);
        }
        if let Some(pgid) = harness_group(&state) {
            proc::kill_group(pgid);
        }
        state.status = Status::Lost;
        state.ended = Some(jiff::Timestamp::now().to_string());
        state.error = Some(LEGACY.into());
        write_state(home, &spec.id, &state)?;
        return Ok(state);
    }
    if open(spec, &state) != Open::Lost {
        return Ok(state);
    }
    follow::end(
        home,
        spec,
        &mut state,
        Status::Lost,
        Some("cos stopped before the run reached Herdr".into()),
    )?;
    Ok(read_raw(home, &spec.id)?.1)
}

/// Judges an agent again and records the new verdict, e.g. after its checks
/// were fixed or its work was finished by hand.
pub fn rejudge(home: &Path, id: &str) -> Result<Verdict> {
    let (spec, state) = read(home, id)?;
    if state.status.preparing() {
        bail!("{id} is {}; it is judged once Herdr has it", state.status);
    }
    if state
        .herdr
        .as_ref()
        .is_some_and(|h| h.live && h.word.as_deref() == Some("working"))
    {
        bail!("{id} is working in Herdr; judge it when it stops");
    }
    follow::judge(home, &spec, "judged again", true)
}

/// Records what you found when you ran what a judged agent drafted. Stored
/// on the run's state only; nothing is sent anywhere.
pub fn record(home: &Path, id: &str, mut r: Recorded) -> Result<State> {
    let (_, mut state) = read(home, id)?;
    if state.verdict.is_none() {
        bail!(
            "{id} is {}; record a result once it is judged",
            state.status
        );
    }
    if r.notes.trim().is_empty() && r.evidence.is_empty() {
        bail!("say what happened: notes or evidence");
    }
    r.evidence.retain(|e| !e.trim().is_empty());
    r.at = jiff::Timestamp::now().to_string();
    state.recorded = Some(r);
    write_state(home, id, &state)?;
    Ok(state)
}

/// Stops an agent: a live Herdr terminal is ended (its last screen kept), a
/// hire still being prepared is told to stop and given `wait` to do so.
pub fn stop(home: &Path, id: &str, wait: Duration) -> Result<State> {
    let (spec, state) = read(home, id)?;
    if state.status.terminal() {
        return Ok(state);
    }
    if state.herdr.is_some() {
        follow::finish(
            home,
            &spec,
            Status::Killed,
            Some("stopped on request".into()),
            "stopped",
        )?;
        return Ok(read_raw(home, id)?.1);
    }
    std::fs::write(dir(home, id).join("stop"), "")?;
    let until = proc::Deadline::after(wait);
    while !until.expired() {
        let (_, s) = read_raw(home, id)?;
        if s.status.terminal() {
            return Ok(s);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let (_, mut s) = read_raw(home, id)?;
    if !s.status.terminal() {
        follow::end(
            home,
            &spec,
            &mut s,
            Status::Killed,
            Some("stopped while it was being prepared".into()),
        )?;
    }
    Ok(read_raw(home, id)?.1)
}

/// Removes a finished agent's directory, worktree and Herdr terminal.
/// Refuses while the worktree holds uncommitted changes or commits past its
/// base, unless `force`: that work exists nowhere else.
pub fn clean(home: &Path, id: &str, force: bool) -> Result<()> {
    let (spec, state) = read(home, id)?;
    if !state.status.terminal() {
        bail!(
            "{id} is {}; stop it first (`cos agents kill {id}`)",
            state.status
        );
    }
    if spec.worktree && spec.workdir.exists() && !force {
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
    follow::remove_terminal(&spec, &state)?;
    if spec.worktree && spec.workdir.exists() {
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
    std::fs::remove_dir_all(run_dir(home, id)?.path)?;
    Ok(())
}

pub(crate) fn write_state(home: &Path, id: &str, state: &State) -> Result<()> {
    write_json(&dir(home, id).join("state.json"), state)
}

static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

/// Write-then-rename, so a reader never sees half a file. The temp name is
/// unique per process and write, so writers never share one.
fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let n = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let mut name = path.as_os_str().to_owned();
    name.push(format!(".{}.{n}.tmp", std::process::id()));
    let tmp = PathBuf::from(name);
    let written = std::fs::write(&tmp, serde_json::to_vec_pretty(value)?)
        .and_then(|()| std::fs::rename(&tmp, path));
    if written.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    Ok(written?)
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
    let now = jiff::ZonedDateTime::now();
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

/// When `pid` started, from the kernel; with the pid, this names one process.
pub fn started_at(pid: i32) -> Option<String> {
    proc::start_time(pid).map(|(s, us)| format!("{s}.{us:06}"))
}

/// `pid` is alive and is a `cos` binary, not a recycled pid.
fn is_cos(pid: i32) -> bool {
    proc::alive(pid) && proc::exe(pid).is_some_and(|p| p.file_name().is_some_and(|f| f == "cos"))
}

/// The harness group a supervisor from before Herdr recorded, if it still
/// has its original leader (as `ps` printed its start), so killing it cannot
/// reach a recycled pid.
fn harness_group(state: &State) -> Option<i32> {
    let pgid = state.pgid?;
    let started = state.pgid_started.as_deref()?;
    if !proc::group_alive(pgid) {
        return None;
    }
    let out = proc::run(
        Command::new("ps").args(["-o", "lstart=", "-p", &pgid.to_string()]),
        Duration::from_secs(5),
    )
    .ok()?;
    (out.stdout.trim() == started).then_some(pgid)
}
