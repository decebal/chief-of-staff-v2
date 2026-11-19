//! herdr-core (`hc`) and the Herdr window (`herdr-app`), as cos uses them:
//! every hire runs in a Herdr terminal, which owns the agent's process for
//! its whole life. Each call here is one bounded child (`proc::run`).
//! `COS_HC` and `COS_HERDR_APP` name stand-ins (tests, the preview).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::proc;

/// What every surface says when there is no Herdr to hire into.
pub const DOWN: &str = "Herdr isn't running: start it, or open Herdr";

/// One `hc` call that is not a launch or a server start.
const CALL: Duration = Duration::from_secs(10);
/// `hc server status`: the core answers at once or is not there.
const STATUS: Duration = Duration::from_secs(5);
/// `hc server start` waits for the new core to answer.
const START: Duration = Duration::from_secs(30);
/// `hc run --bg` returns once the program runs.
const RUN: Duration = Duration::from_secs(60);
/// A status answer is reused for this long by the window's polls.
const FRESH: Duration = Duration::from_secs(3);

pub fn hc() -> PathBuf {
    std::env::var_os("COS_HC")
        .filter(|v| !v.is_empty())
        .map_or_else(|| PathBuf::from("hc"), PathBuf::from)
}

fn app() -> PathBuf {
    std::env::var_os("COS_HERDR_APP")
        .filter(|v| !v.is_empty())
        .map_or_else(|| PathBuf::from("herdr-app"), PathBuf::from)
}

/// Whether herdr-core answers, and if not, why.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Server {
    pub up: bool,
    pub version: Option<String>,
    /// Why it is not up, in words; `None` when it is.
    pub why: Option<String>,
}

impl Server {
    fn down(why: String) -> Self {
        Self {
            up: false,
            version: None,
            why: Some(why),
        }
    }
}

/// `hc server status --json`, fresh.
pub fn server() -> Server {
    let out = match proc::run(
        Command::new(hc()).args(["server", "status", "--json"]),
        STATUS,
    ) {
        Ok(o) => o,
        Err(e) => {
            return Server::down(format!("{DOWN} (`{}` did not answer: {e:#})", hc_name()));
        }
    };
    if !out.status.success() {
        return Server::down(format!("{DOWN} ({})", error_text(&out.stderr)));
    }
    let v: serde_json::Value = serde_json::from_str(out.stdout.trim()).unwrap_or_default();
    Server {
        up: true,
        version: v["version"].as_str().map(str::to_owned),
        why: None,
    }
}

static LAST: Mutex<Option<(Instant, Server)>> = Mutex::new(None);

/// `server`, reused for a few seconds: the window asks on every render of
/// the hire dialog. A check that times out serves the last answer.
pub fn server_cached() -> Server {
    if let Ok(last) = LAST.lock()
        && let Some((at, s)) = last.as_ref()
        && at.elapsed() < FRESH
    {
        return s.clone();
    }
    let now = server();
    let timed_out = now.why.as_deref().is_some_and(|w| w.contains("timed out"));
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    if timed_out && let Some((_, s)) = last.as_ref() {
        return s.clone();
    }
    *last = Some((Instant::now(), now.clone()));
    now
}

fn forget() {
    if let Ok(mut last) = LAST.lock() {
        *last = None;
    }
}

/// Refuses with the fix when herdr-core does not answer.
pub fn require() -> Result<Server> {
    let s = server();
    match &s.why {
        None => Ok(s),
        Some(why) => bail!("{why}. Start it with `hc server start`, or open Herdr."),
    }
}

/// `hc server start`, then asks again.
pub fn start() -> Result<Server> {
    forget();
    let out = proc::run(Command::new(hc()).args(["server", "start"]), START)
        .with_context(|| format!("start herdr-core with `{} server start`", hc_name()))?;
    if !out.status.success() {
        bail!("hc server start: {}", error_text(&out.stderr));
    }
    let s = server();
    forget();
    Ok(s)
}

/// What a program says it is doing (OSC 7501's root record). `hc ls` gives
/// the word alone; `hc get` the record.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Word {
    /// `idle`, `working`, `done`, `blocked` or `error`.
    pub state: String,
    /// For `blocked`: `permission`, `question` or `auth`.
    pub kind: Option<String>,
    pub msg: Option<String>,
}

impl<'de> Deserialize<'de> for Word {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Either {
            Word(String),
            Record {
                #[serde(default)]
                state: String,
                #[serde(default)]
                kind: Option<String>,
                #[serde(default)]
                msg: Option<String>,
            },
        }
        Ok(match Either::deserialize(d)? {
            Either::Word(state) => Word {
                state,
                ..Word::default()
            },
            Either::Record { state, kind, msg } => Word { state, kind, msg },
        })
    }
}

/// hc's word on a launch's first message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FirstMessage {
    pub sent: bool,
    #[serde(default)]
    pub reason: Option<String>,
}

/// One terminal, as `hc get` and `hc run` describe it.
#[derive(Debug, Clone, Deserialize)]
pub struct Term {
    pub address: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
    /// `live` or `finished`.
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub status: Option<Word>,
    #[serde(default)]
    pub turn: u64,
    #[serde(default)]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub first_message: Option<FirstMessage>,
}

impl Term {
    pub fn live(&self) -> bool {
        self.state == "live"
    }
}

/// A terminal to start.
pub struct Launch<'a> {
    pub name: &'a str,
    pub cwd: &'a Path,
    pub labels: &'a [(String, String)],
    pub argv: &'a [String],
}

/// `hc run --bg`: the program starts in a new terminal and stays there. It
/// has no parent terminal, so it outlives whatever started cos.
pub fn run(l: &Launch) -> Result<Term> {
    let mut cmd = Command::new(hc());
    cmd.args(["run", "--bg", "--json", "--no-parent", "--no-notify"])
        .args(["--name", l.name])
        .arg("--cwd")
        .arg(l.cwd);
    for (k, v) in l.labels {
        cmd.arg("--label").arg(format!("{k}={v}"));
    }
    // `--` keeps the agent's own flags from being read as hc's.
    cmd.arg("--").args(l.argv);
    let out = proc::run(&mut cmd, RUN).context("start the hire in Herdr (`hc run`)")?;
    if !out.status.success() {
        bail!("hc run: {}", error_text(&out.stderr));
    }
    serde_json::from_str(out.stdout.trim())
        .with_context(|| format!("hc run printed no terminal: {}", out.stdout.trim()))
}

/// One terminal by address; `None` once it is removed.
pub fn get(address: &str) -> Result<Option<Term>> {
    let out = proc::run(Command::new(hc()).args(["get", address, "--json"]), CALL)?;
    if !out.status.success() {
        if kind(&out.stderr).as_deref() == Some("not_found") || out.status.code() == Some(3) {
            return Ok(None);
        }
        bail!("hc get {address}: {}", error_text(&out.stderr));
    }
    Ok(Some(serde_json::from_str(out.stdout.trim()).with_context(
        || format!("hc get {address} printed: {}", out.stdout.trim()),
    )?))
}

/// Pastes `text` into the terminal and presses enter.
pub fn send(address: &str, text: &str) -> Result<()> {
    simple(&["send", address, text], "send")
}

/// Presses the program's stop key: its turn ends, the program stays.
pub fn interrupt(address: &str) -> Result<()> {
    simple(&["interrupt", address], "interrupt")
}

/// Ends the terminal's program.
pub fn kill(address: &str) -> Result<()> {
    simple(&["kill", address], "kill")
}

/// Removes a finished terminal.
pub fn rm(address: &str) -> Result<()> {
    simple(&["rm", address], "rm")
}

/// The terminal's screen and its last `lines` of history.
pub fn read(address: &str, lines: usize) -> Result<String> {
    let out = proc::run(
        Command::new(hc()).args(["read", address, "--lines", &lines.to_string()]),
        CALL,
    )?;
    if !out.status.success() {
        bail!("hc read {address}: {}", error_text(&out.stderr));
    }
    Ok(out.stdout)
}

fn simple(args: &[&str], what: &str) -> Result<()> {
    let out = proc::run(Command::new(hc()).args(args), CALL)?;
    if out.status.success() {
        return Ok(());
    }
    match kind(&out.stderr).as_deref() {
        Some("blocked") => bail!(
            "it is waiting on you in Herdr: answer it there (Open in Herdr), then {what} again"
        ),
        Some("finished") => bail!("its Herdr terminal has already ended"),
        _ => bail!("hc {what}: {}", error_text(&out.stderr)),
    }
}

/// Brings the terminal up in the Herdr window. A terminal cos started has
/// no pane yet, so it gets a tab when the window cannot show it as is.
/// Returns what happened, in words.
pub fn focus(address: &str, name: &str) -> Result<String> {
    let app = app();
    let try_do = |args: &[&str]| -> bool {
        proc::run(Command::new(&app).arg("do").args(args), CALL)
            .is_ok_and(|o| o.status.success() && !o.stdout.contains("\"error\""))
    };
    if try_do(&["agent.focus", "--terminal", address])
        || try_do(&["terminal.keep", "--terminal", address])
    {
        return Ok(format!("{name} ({address}) is shown in Herdr"));
    }
    // The window is not open. Only a real app bundle is opened; a stand-in
    // is never launched as a GUI.
    let bundle = std::fs::canonicalize(&app).ok().and_then(|p| {
        p.ancestors()
            .find(|a| a.extension().is_some_and(|e| e == "app"))
            .map(Path::to_path_buf)
    });
    if let Some(bundle) = bundle {
        let _ = proc::run(Command::new("/usr/bin/open").arg(bundle), CALL);
        return Ok(format!(
            "Herdr is opening: the run is the terminal {name} ({address})"
        ));
    }
    bail!("Herdr's window is not open: open Herdr, then the terminal {name} ({address})")
}

fn hc_name() -> String {
    hc().to_string_lossy().into_owned()
}

/// `{error: {kind, message, hint}}` on stderr, as a line.
fn error_text(stderr: &str) -> String {
    let v: serde_json::Value = serde_json::from_str(stderr.trim()).unwrap_or_default();
    let e = &v["error"];
    match (e["message"].as_str(), e["hint"].as_str()) {
        (Some(m), Some(h)) if !h.is_empty() => format!("{m} ({h})"),
        (Some(m), _) => m.to_owned(),
        _ if stderr.trim().is_empty() => "no answer".to_owned(),
        _ => stderr.trim().lines().last().unwrap_or("").to_owned(),
    }
}

fn kind(stderr: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(stderr.trim()).ok()?;
    v["error"]["kind"].as_str().map(str::to_owned)
}
