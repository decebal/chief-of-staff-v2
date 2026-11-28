//! The Settings card's Assistants section: `cos connect` for Claude Desktop
//! and Codex, and the version each configured `cos` and the daemon run,
//! set against this app's.

use std::path::PathBuf;

use anyhow::{Context, bail};
use cos::connect::{self, Client, Outcome};
use cos::daemon::client::{self, Probe};
use serde::Serialize;
use tauri::{AppHandle, Manager, Runtime};

use crate::{App, Res, Start, blocking};

/// The lib's version: `connect::status` judges `version_ok` against it.
const APP_VERSION: &str = connect::VERSION;

#[derive(Serialize)]
pub struct Assistants {
    app_version: &'static str,
    /// The `cos` a Register or Repair writes into a client's config.
    cos: Option<PathBuf>,
    daemon: DaemonView,
    clients: Vec<ClientView>,
}

#[derive(Serialize)]
struct DaemonView {
    /// `up`, `down`, or `error` for a socket that neither answered nor refused.
    state: &'static str,
    version: Option<String>,
    error: Option<String>,
    skew: Option<String>,
}

#[derive(Serialize)]
struct ClientView {
    label: &'static str,
    #[serde(flatten)]
    status: connect::Status,
    skew: Option<String>,
}

#[derive(Serialize)]
pub struct Applied {
    outcome: &'static str,
    /// Claude Desktop reads its config only at launch.
    restart: bool,
    status: Assistants,
}

fn label(c: Client) -> &'static str {
    match c {
        Client::ClaudeDesktop => "Claude Desktop",
        Client::Codex => "Codex",
    }
}

fn env(start: &Start) -> anyhow::Result<connect::Env> {
    Ok(connect::Env {
        home: start
            .user_home
            .clone()
            .context("HOME is not set: there is no client config to read")?,
        codex_home: start.codex_home.clone(),
        path: std::env::var_os("PATH"),
        registry: Some(start.registry.clone()),
    })
}

fn skew(who: &str, version: &str) -> Option<String> {
    (version != APP_VERSION).then(|| format!("{who} runs cos {version}; this app is {APP_VERSION}"))
}

/// A `--version` line that is not a cos version names no skew; the row's
/// `version` problem says it instead.
fn client_skew(who: &str, printed: &str) -> Option<String> {
    connect::is_cos_version(printed)
        .then(|| printed.strip_prefix("cos ").unwrap_or(printed))
        .and_then(|v| skew(who, v))
}

fn daemon(start: &Start) -> DaemonView {
    match client::probe(&start.daemon_dir) {
        Ok(Probe::Down) => DaemonView {
            state: "down",
            version: None,
            error: None,
            skew: None,
        },
        Ok(Probe::Up { version, .. }) => DaemonView {
            state: "up",
            skew: skew("the cos daemon", &version),
            version: Some(version),
            error: None,
        },
        Err(e) => DaemonView {
            state: "error",
            version: None,
            error: Some(format!("{e:#}")),
            skew: None,
        },
    }
}

fn assistants(start: &Start, cos: Option<PathBuf>) -> anyhow::Result<Assistants> {
    let env = env(start)?;
    let clients = connect::status(&env)
        .into_iter()
        .map(|status| ClientView {
            label: label(status.client),
            skew: status
                .version
                .as_deref()
                .and_then(|v| client_skew(label(status.client), v)),
            status,
        })
        .collect();
    Ok(Assistants {
        app_version: APP_VERSION,
        cos,
        daemon: daemon(start),
        clients,
    })
}

fn outcome_name(o: &Outcome) -> &'static str {
    match o {
        Outcome::Registered => "registered",
        Outcome::Repaired => "repaired",
        Outcome::AlreadyRegistered => "already-registered",
        Outcome::Removed => "removed",
        Outcome::NotRegistered => "not-registered",
    }
}

#[tauri::command]
pub async fn connect_status<R: Runtime>(handle: AppHandle<R>) -> Res<Assistants> {
    let start = handle.state::<Start>().inner().clone();
    let cos = handle.state::<App>().get().cos;
    blocking(move || assistants(&start, cos)).await
}

/// `action` is `register`, `repair` or `remove`; Register and Repair write
/// the window's own `cos` binary.
#[tauri::command]
pub async fn connect_apply<R: Runtime>(
    handle: AppHandle<R>,
    client: String,
    action: String,
) -> Res<Applied> {
    let start = handle.state::<Start>().inner().clone();
    let cos = handle.state::<App>().get().cos;
    blocking(move || {
        let Some(c) = Client::parse(&client) else {
            bail!("no assistant {client:?}: claude-desktop or codex");
        };
        let env = env(&start)?;
        let bin = || -> anyhow::Result<PathBuf> {
            let b = cos.clone().context(
                "no cos binary: set COS_BIN, build it (`cargo build`), or `task install`",
            )?;
            b.canonicalize()
                .with_context(|| format!("resolve {}", b.display()))
        };
        let outcome = match action.as_str() {
            "register" => connect::register(c, &env, &bin()?, false)?,
            "repair" => connect::register(c, &env, &bin()?, true)?,
            "remove" => connect::remove(c, &env)?,
            other => bail!("no action {other:?}: register, repair or remove"),
        };
        let wrote = matches!(
            outcome,
            Outcome::Registered | Outcome::Repaired | Outcome::Removed
        );
        Ok(Applied {
            outcome: outcome_name(&outcome),
            restart: wrote && c == Client::ClaudeDesktop,
            status: assistants(&start, cos)?,
        })
    })
    .await
}
