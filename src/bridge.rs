//! The paid MCP tools' runs. `brief` and `ask` start a run on the daemon when
//! it answers, and otherwise as a child this `cos mcp` owns until stdin
//! closes. Every run tool answers with a `RunView`, read from run records
//! only, so the views work whoever owns the run and with the daemon down.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::{Context, Result, ensure};
use serde::Serialize;
use serde_json::Value;

use crate::business::Triple;
use crate::daemon::{self, client};
use crate::pipeline::{self, RunChild, RunSpec};
use crate::proc;
use crate::reports::Brief;
use crate::runs::{self, Kind, Owner, Record, State};
use crate::store;

pub const QUESTION_MAX: usize = daemon::QUESTION_MAX;
pub const WAIT_MAX: u64 = 50;
pub const LIMIT_MAX: u32 = 20;
pub const LIMIT_DEFAULT: u32 = 5;
const POLL: Duration = Duration::from_millis(50);

/// One run, as every run tool returns it.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct RunView {
    pub run: String,
    pub business: String,
    /// "brief" or "ask".
    pub kind: String,
    /// "queued", "running", "done", "failed", "deferred", "stopped" or "lost".
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// `cos://run/<business>/<run>`: opens the run in the Chief of Staff app.
    /// Absent when the run's business is not a registered one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
    /// Set by `brief` and `ask`: true when a brief for this business was
    /// already running and no new run was started.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub existing: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub day: Option<String>,
    /// The brief, once a brief is done.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub markdown: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
    /// The answer, once an ask is done.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub answer: Option<String>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct RunList {
    /// Newest first.
    pub runs: Vec<RunView>,
}

pub fn link(business: &str, run: &str) -> String {
    format!("cos://run/{business}/{run}")
}

impl RunView {
    /// Drops the link unless `registered`: a `default` or `explicit` label
    /// names no business the app can switch to.
    #[must_use]
    pub fn linked(mut self, registered: bool) -> Self {
        if !registered {
            self.link = None;
        }
        self
    }
}

fn word(v: impl Serialize) -> String {
    serde_json::to_value(v)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// The record and the state a reader shows: a live-looking record whose
/// process is gone is read again, and shown `lost` only if still not final.
pub fn observe(home: &Path, id: &str) -> Result<Option<(Record, State)>> {
    let mut failed = None;
    let mut last = None;
    let state = runs::shown_from(
        || match runs::read(home, id) {
            Ok(r) => {
                last.clone_from(&r);
                r
            }
            Err(e) => {
                failed = Some(e);
                None
            }
        },
        runs::process_gone,
    );
    if let Some(e) = failed {
        return Err(e);
    }
    Ok(last.zip(state))
}

/// Whether `home` is a business's home in `registry`: the homes the daemon
/// follows hires in.
pub fn registered_home(registry: &Path, home: &Path) -> bool {
    let canon = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_owned());
    let home = canon(home);
    crate::business::registered_homes(registry)
        .values()
        .any(|h| canon(h) == home)
}

/// `r` shown as `state`. `full` adds a done brief's report and markdown and a
/// done ask's answer.
pub fn view(home: &Path, r: &Record, state: State, full: bool) -> RunView {
    let mut v = RunView {
        run: r.id.clone(),
        business: r.business.clone(),
        kind: word(r.kind),
        state: word(state),
        reason: r.reason.clone(),
        link: Some(link(&r.business, &r.id)),
        existing: None,
        day: r.day.clone(),
        report: None,
        markdown: None,
        cost_usd: r.cost_usd,
        answer: None,
    };
    if full && state == State::Done {
        match r.kind {
            Kind::Brief => {
                if let Some(saved) = r
                    .day
                    .as_deref()
                    .and_then(|d| store::on_day::<Brief>(home, store::Kind::Briefs, d).ok())
                {
                    v.report = serde_json::to_value(&saved.value).ok();
                    v.markdown = Some(saved.markdown);
                }
            }
            Kind::Ask => v.answer = r.answer.clone(),
        }
    }
    v
}

/// The newest `limit` runs under `home`.
pub fn list(home: &Path, limit: usize) -> Vec<RunView> {
    let Ok(entries) = std::fs::read_dir(home.join(runs::DIR)) else {
        return Vec::new();
    };
    let mut ids: Vec<String> = entries
        .filter_map(Result::ok)
        .filter_map(|e| {
            let name = e.file_name();
            let id = name.to_str()?.strip_suffix(".json")?.to_owned();
            runs::is_id(&id).then_some(id)
        })
        .collect();
    ids.sort_unstable_by(|a, b| b.cmp(a));
    ids.iter()
        .filter_map(|id| observe(home, id).ok().flatten())
        .take(limit)
        .map(|(r, state)| view(home, &r, state, false))
        .collect()
}

/// A run `brief` or `ask` started, or the brief already running.
pub struct Started {
    pub run: String,
    pub existing: bool,
}

impl Started {
    /// The view `brief` and `ask` answer with. A brief that holds the lock but
    /// has not written its record yet is shown running.
    pub fn view(&self, home: &Path, business: &str, kind: Kind) -> RunView {
        let mut v = match observe(home, &self.run) {
            Ok(Some((r, state))) => view(home, &r, state, false),
            _ => RunView {
                run: self.run.clone(),
                business: business.to_owned(),
                kind: word(kind),
                state: word(if self.existing {
                    State::Running
                } else {
                    State::Queued
                }),
                reason: None,
                link: Some(link(business, &self.run)),
                existing: None,
                day: None,
                report: None,
                markdown: None,
                cost_usd: None,
                answer: None,
            },
        };
        v.existing = Some(self.existing);
        v
    }
}

struct Owned {
    run: String,
    business: String,
    kind: Kind,
    stop: Sender<()>,
    thread: JoinHandle<()>,
}

/// The runs this `cos mcp` owns because no daemon answered: each followed by
/// a thread of its own up to `pipeline::BRIEF_CHILD`, held to the daemon's
/// caps (`daemon::over_cap`), and all stopped and reaped by `shutdown`.
#[derive(Default)]
pub struct Runner {
    owned: Mutex<Vec<Owned>>,
    /// `cos --budget` of this server, passed to every run it starts.
    budget: Option<f64>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// `COS_RUN_BIN` when set, else this `cos`.
fn run_bin() -> Result<PathBuf> {
    match std::env::var_os("COS_RUN_BIN").filter(|v| !v.is_empty()) {
        Some(p) => Ok(PathBuf::from(p)),
        None => std::env::current_exe().context("locate the cos binary"),
    }
}

impl Runner {
    pub fn with_budget(budget: Option<f64>) -> Self {
        Self {
            owned: Mutex::default(),
            budget,
        }
    }

    /// Starts `kind` on `triple`: through the daemon when it answers, else
    /// owned here. A probe that fails other than by the daemon being down is
    /// an error. A triple with no registered business name is owned here,
    /// since the daemon starts runs only for registered businesses, and so is
    /// a run under a budget, which the daemon does not carry.
    pub fn start(&self, triple: &Triple, kind: Kind, question: Option<&str>) -> Result<Started> {
        if let (Some(name), None) = (&triple.business, self.budget) {
            let dir = match daemon::dir() {
                Ok(d) => d,
                Err(_) if cfg!(not(target_os = "macos")) => {
                    return self.own(triple, kind, question);
                }
                Err(e) => return Err(e),
            };
            if let client::Probe::Up { .. } = client::probe(&dir)? {
                let (run, existing) = client::start(&dir, kind, name, question)?;
                return Ok(Started { run, existing });
            }
        }
        self.own(triple, kind, question)
    }

    fn own(&self, triple: &Triple, kind: Kind, question: Option<&str>) -> Result<Started> {
        let business = triple.label();
        let mut owned = lock(&self.owned);
        let (done, live): (Vec<Owned>, Vec<Owned>) =
            owned.drain(..).partition(|o| o.thread.is_finished());
        *owned = live;
        for o in done {
            let _ = o.thread.join();
        }
        if kind == Kind::Brief {
            let mine = owned
                .iter()
                .find(|o| o.kind == Kind::Brief && o.business == business)
                .map(|o| o.run.clone());
            let holder = match mine {
                Some(id) => Some(id),
                None => runs::brief_held(&triple.home)?,
            };
            if let Some(run) = holder {
                ensure!(
                    runs::is_id(&run),
                    "a brief is running for {business}; its run id is not readable yet"
                );
                return Ok(Started {
                    run,
                    existing: true,
                });
            }
        }
        if let Some(refused) = daemon::over_cap(
            kind,
            business,
            owned.iter().map(|o| (o.kind, o.business.as_str())),
        ) {
            anyhow::bail!(refused);
        }
        let child = pipeline::spawn_run(
            &run_bin()?,
            &RunSpec {
                kind,
                business,
                home: &triple.home,
                workspace: &triple.workspace,
                projects: &triple.projects,
                question,
                owner: Owner::Mcp,
                env: Vec::new(),
                client_req: None,
                budget: self.budget,
            },
        )?;
        let run = child.id().to_owned();
        let (stop, rx) = mpsc::channel();
        let thread = std::thread::spawn(move || follow(child, rx, pipeline::BRIEF_CHILD));
        owned.push(Owned {
            run: run.clone(),
            business: business.to_owned(),
            kind,
            stop,
            thread,
        });
        Ok(Started {
            run,
            existing: false,
        })
    }

    /// Stops every run this server owns and waits until each is reaped.
    pub fn shutdown(&self) {
        let all: Vec<Owned> = std::mem::take(&mut *lock(&self.owned));
        for o in &all {
            let _ = o.stop.send(());
        }
        for o in all {
            let _ = o.thread.join();
        }
    }
}

impl Drop for Runner {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Owns one run child until it is reaped: its exit, a stop, or `deadline`.
fn follow(mut child: RunChild, stop: Receiver<()>, deadline: Duration) {
    let until = proc::Deadline::capped(deadline, deadline);
    let ended = loop {
        match stop.recv_timeout(POLL) {
            Ok(()) | Err(RecvTimeoutError::Disconnected) => break child.kill(),
            Err(RecvTimeoutError::Timeout) => {}
        }
        match child.try_end() {
            Ok(Some(e)) => break Ok(e),
            Ok(None) => {}
            Err(e) => break Err(e),
        }
        if until.expired() {
            break child.stop_as(State::Failed, "deadline");
        }
    };
    if let Err(e) = ended {
        use std::io::Write;
        let _ = writeln!(std::io::stderr(), "cos mcp: run {}: {e:#}", child.id());
    }
}
