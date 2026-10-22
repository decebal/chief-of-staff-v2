//! Chief of Staff desktop: the `cos` report and an agent launcher in one
//! window, over the `cos` library.
//!
//! The app launches and watches; it owns no agent. Each agent is a setsid'd
//! `cos agents supervise` process, so closing the window stops nothing, and
//! reopening it reconciles every agent from `out/agents/*/state.json`. The
//! report renders in a sandboxed iframe with an opaque origin: a brief quotes
//! mail and tickets, and nothing in it may reach these commands.

pub mod settings;

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use cos::agents::{self, Harness, Request, Spec, State};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tauri::{AppHandle, Builder, Emitter, Manager, Runtime};

pub use settings::Settings;

struct App(Mutex<Settings>);

impl App {
    fn get(&self) -> Settings {
        self.0.lock().expect("settings lock").clone()
    }
}

type Res<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Blocking work off the main thread: a sync command would freeze the window.
async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> anyhow::Result<T> + Send + 'static,
) -> Res<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(err)?
        .map_err(|e| format!("{e:#}"))
}

#[tauri::command]
fn paths(app: tauri::State<'_, App>) -> Value {
    let s = app.get();
    json!({
        "home": s.home,
        "workspace": s.workspace,
        "projects": s.projects,
        "cos": s.cos_binary(),
        "slots": s.slots.unwrap_or_else(agents::slots::run_slots),
    })
}

/// Points the app at another workspace; it must already hold `.chief-of-staff/`.
#[tauri::command]
fn set_workspace<R: Runtime>(
    handle: AppHandle<R>,
    app: tauri::State<'_, App>,
    path: String,
) -> Res<Value> {
    let ws = PathBuf::from(path.trim());
    let dir = cos::state::runtime_dir(&ws).map_err(|e| format!("{e:#}"))?;
    if !dir.join("profile.json").is_file() {
        return Err(format!(
            "{} has no profile.json; run `cos --workspace {} state init` first",
            dir.display(),
            ws.display()
        ));
    }
    let mut s = app.0.lock().map_err(err)?;
    s.workspace = ws;
    s.save(&handle).map_err(|e| format!("{e:#}"))?;
    drop(s);
    Ok(paths(app))
}

/// `cosreport://localhost/` is the report; `/reports/<day>.html` an archived
/// one. Its own scheme gives the page its own origin and no app CSP, so the
/// report's inline scripts run in the iframe and nowhere else.
pub fn report_response<R: Runtime>(
    app: &AppHandle<R>,
    path: &str,
) -> tauri::http::Response<Vec<u8>> {
    let s = app.state::<App>().get();
    let body = match path {
        "/" | "" => cos::report::data(&s.home, &s.workspace, false)
            .and_then(|d| cos::report::render(&d))
            .map(String::into_bytes)
            .map_err(|e| format!("{e:#}")),
        _ => cos::dash::archived(&s.home, path)
            .ok_or_else(|| "not found".to_owned())
            .and_then(|f| std::fs::read(f).map_err(err)),
    };
    let (status, kind, bytes) = match body {
        Ok(b) => (200, "text/html; charset=utf-8", b),
        Err(e) => (404, "text/plain; charset=utf-8", e.into_bytes()),
    };
    tauri::http::Response::builder()
        .status(status)
        .header("Content-Type", kind)
        .header("Cache-Control", "no-store")
        .body(bytes)
        .expect("static response parts")
}

#[derive(Serialize)]
struct Row {
    spec: Spec,
    state: State,
}

#[tauri::command]
async fn agents_list(app: tauri::State<'_, App>) -> Res<Vec<Row>> {
    let home = app.get().home;
    blocking(move || {
        Ok(agents::list(&home)?
            .into_iter()
            .map(|(spec, state)| Row { spec, state })
            .collect())
    })
    .await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SpawnArgs {
    harness: Harness,
    repo: Option<String>,
    prompt: String,
    title: Option<String>,
    model: Option<String>,
    worktree: bool,
    deadline_secs: u64,
    budget_usd: f64,
    /// Identical agents to start; each gets its own worktree.
    count: usize,
    source: Option<String>,
    /// Overrides the harness binary; the window never sends it (tests do).
    #[serde(default)]
    program: Option<String>,
}

#[tauri::command]
async fn agents_spawn(app: tauri::State<'_, App>, args: SpawnArgs) -> Res<Vec<Spec>> {
    let s = app.get();
    let cos = s.cos_binary().ok_or(
        "no cos binary: set COS_BIN, build it (`cargo build`), or `task install`".to_owned(),
    )?;
    blocking(move || {
        let repo = args
            .repo
            .filter(|r| !r.trim().is_empty())
            .map(PathBuf::from);
        let title = args
            .title
            .filter(|t| !t.trim().is_empty())
            .unwrap_or_else(|| {
                args.prompt
                    .lines()
                    .next()
                    .unwrap_or("")
                    .chars()
                    .take(80)
                    .collect()
            });
        (0..args.count.clamp(1, 32))
            .map(|_| {
                agents::spawn(
                    &s.home,
                    Request {
                        harness: args.harness,
                        program: args.program.clone(),
                        model: args.model.clone().filter(|m| !m.trim().is_empty()),
                        worktree: args.worktree && repo.is_some(),
                        repo: repo.clone(),
                        title: title.clone(),
                        source: args.source.clone().unwrap_or_else(|| "desktop".into()),
                        prompt: args.prompt.clone(),
                        deadline: Duration::from_secs(args.deadline_secs),
                        budget_usd: args.budget_usd,
                        slots: s.slots,
                    },
                    &cos,
                )
            })
            .collect()
    })
    .await
}

#[tauri::command]
async fn agents_stop(app: tauri::State<'_, App>, id: String) -> Res<State> {
    let home = app.get().home;
    blocking(move || agents::stop(&home, &id, Duration::from_secs(10))).await
}

#[tauri::command]
async fn agents_clean(app: tauri::State<'_, App>, id: String, force: bool) -> Res<()> {
    let home = app.get().home;
    blocking(move || agents::clean(&home, &id, force)).await
}

/// Activity lines from byte offset `from`, and the offset to ask for next.
#[tauri::command]
async fn agents_tail(app: tauri::State<'_, App>, id: String, from: u64) -> Res<Value> {
    let home = app.get().home;
    blocking(move || {
        let (spec, _) = agents::read(&home, &id)?;
        let (lines, next) =
            agents::view::activity(&agents::events_path(&home, &id), spec.harness, from);
        let seed_log = std::fs::read_to_string(agents::dir(&home, &id).join("seed.log")).ok();
        Ok(json!({ "lines": lines, "next": next, "seedLog": seed_log }))
    })
    .await
}

/// Agent prompts from the latest brief or review, numbered as the report numbers them.
#[tauri::command]
async fn agent_tasks(app: tauri::State<'_, App>, from: agents::From) -> Res<Vec<agents::Task>> {
    let home = app.get().home;
    blocking(move || agents::tasks(&home, from)).await
}

/// Git repos directly under the projects directory, for the spawn form.
#[tauri::command]
fn repos(app: tauri::State<'_, App>) -> Vec<String> {
    git_repos(&app.get().projects)
}

fn git_repos(projects: &Path) -> Vec<String> {
    let mut found: Vec<String> = std::fs::read_dir(projects)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.join(".git").exists())
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    found.sort();
    found
}

/// Emits `report-changed` when a run or the workspace writes, and
/// `agents-changed` when an agent does, so the window refreshes on events
/// instead of every view polling. Reads metadata only: `agents::list` runs
/// `ps` per live agent, too dear per tick. An agent's state write renames into
/// its directory, which moves the directory's mtime that `stamp` reads.
fn watch<R: Runtime>(handle: AppHandle<R>) {
    std::thread::spawn(move || {
        let (mut report, mut fleet) = (String::new(), String::new());
        loop {
            let s = handle.state::<App>().get();
            let now = cos::dash::version(&s.home, &s.workspace);
            if now != report {
                let _ = handle.emit("report-changed", ());
                report = now;
            }
            let now = cos::dash::stamp([agents::root(&s.home)]);
            if now != fleet {
                let _ = handle.emit("agents-changed", ());
                fleet = now;
            }
            std::thread::sleep(Duration::from_millis(700));
        }
    });
}

/// The app on any runtime: the report scheme, the settings (given, or loaded
/// from the environment and the config directory), the watcher, the commands.
pub fn builder<R: Runtime>(builder: Builder<R>, settings: Option<Settings>) -> Builder<R> {
    let builder = match settings {
        Some(s) => builder.manage(App(Mutex::new(s))),
        None => builder,
    };
    builder
        .register_asynchronous_uri_scheme_protocol("cosreport", |ctx, req, responder| {
            let (app, path) = (ctx.app_handle().clone(), req.uri().path().to_owned());
            std::thread::spawn(move || responder.respond(report_response(&app, &path)));
        })
        .setup(|app| {
            if app.try_state::<App>().is_none() {
                app.manage(App(Mutex::new(Settings::load(app.handle()))));
            }
            watch(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            paths,
            set_workspace,
            agents_list,
            agents_spawn,
            agents_stop,
            agents_clean,
            agents_tail,
            agent_tasks,
            repos,
        ])
}
