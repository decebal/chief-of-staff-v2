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

use cos::agents::{self, Harness, Spec, State};
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

/// `cosreport://localhost/docs…`: the documentation room, through the same
/// handler `cos dash` uses, so reading and saving behave alike in both.
pub fn docs_response<R: Runtime>(
    app: &AppHandle<R>,
    req: &tauri::http::Request<Vec<u8>>,
) -> tauri::http::Response<Vec<u8>> {
    let s = app.state::<App>().get();
    let ask = cos::docs::Ask {
        method: req.method().as_str(),
        path: req.uri().path(),
        query: req.uri().query(),
        cos_header: req.headers().get("x-cos").and_then(|h| h.to_str().ok()),
        body: req.body(),
    };
    docs_http(
        cos::docs::handle(&s.home, &s.workspace, &s.projects, &ask).unwrap_or(cos::docs::Reply {
            status: 404,
            kind: "text/plain; charset=utf-8",
            body: b"not found".to_vec(),
            headers: Vec::new(),
        }),
    )
}

/// A docs room answer as the scheme response, its own headers included.
pub fn docs_http(r: cos::docs::Reply) -> tauri::http::Response<Vec<u8>> {
    let mut b = tauri::http::Response::builder()
        .status(r.status)
        .header("Content-Type", r.kind)
        .header("Cache-Control", "no-store");
    for (k, v) in r.headers {
        b = b.header(k, v);
    }
    b.body(r.body).expect("static response parts")
}

/// Saves the documentation room's organisation. The room asks for it by
/// `postMessage`, which the window checks came from the Docs frame.
#[tauri::command]
async fn docs_save(app: tauri::State<'_, App>, organize: Value) -> Res<()> {
    let home = app.get().home;
    blocking(move || {
        let org: cos::docs::Organize = serde_json::from_value(organize)?;
        cos::docs::save_organize(&home, &org)
    })
    .await
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

/// The staff: every role in `context/agents.json` (the template's when the
/// file has none) with the terms it hires on outside any repo, and why the
/// file was not used, if it was not.
#[tauri::command]
async fn agents_roles(app: tauri::State<'_, App>) -> Res<Value> {
    let home = app.get().home;
    blocking(move || {
        let p = agents::policy::load(&home);
        let roles: Vec<Value> = p
            .roles
            .iter()
            .map(|r| json!({ "role": r, "terms": p.terms_for(None, Some(r)) }))
            .collect();
        Ok(json!({ "roles": roles, "defaults": p.defaults, "note": p.note }))
    })
    .await
}

/// How agents did, from every verdict in the ledger: by role, harness, model
/// and repo (`scores`), and by role alone (`roles`).
#[tauri::command]
async fn agents_scorecard(app: tauri::State<'_, App>) -> Res<Value> {
    let home = app.get().home;
    blocking(move || {
        let rows = agents::ledger::read(&home);
        Ok(json!({
            "scores": agents::ledger::scorecard(&rows),
            "roles": agents::ledger::by_role(&rows),
        }))
    })
    .await
}

/// Judges an ended agent again from what it left.
#[tauri::command]
async fn agents_rejudge(app: tauri::State<'_, App>, id: String) -> Res<agents::Verdict> {
    let home = app.get().home;
    blocking(move || agents::rejudge(&home, &id)).await
}

/// The workspace's open deliverables, the work a role can be hired for.
#[tauri::command]
async fn deliverables_open(app: tauri::State<'_, App>) -> Res<Vec<Value>> {
    let ws = app.get().workspace;
    blocking(move || {
        let Some(st) = cos::state::read(&ws)? else {
            return Ok(Vec::new());
        };
        Ok(cos::state::open(&st, "deliverables")
            .map(|d| {
                let goal = cos::state::find(&st, "goals", d["goalId"].as_str().unwrap_or(""));
                json!({
                    "id": d["id"], "title": d["title"], "status": d["status"],
                    "goalId": d["goalId"], "goal": goal.map(|g| &g["title"]),
                    "owner": d["owner"], "dueDate": d["dueDate"],
                    "doneWhen": d.get("doneWhen").or_else(|| d.get("done_when")),
                })
            })
            .collect())
    })
    .await
}

/// The chief of staff's latest brief, as counts: what it handed out.
#[tauri::command]
async fn brief_summary(app: tauri::State<'_, App>) -> Res<Value> {
    let s = app.get();
    blocking(move || {
        let open = cos::state::read(&s.workspace)?
            .map_or(0, |st| cos::state::open(&st, "deliverables").count());
        let Some(b) = cos::store::latest::<cos::reports::Brief>(&s.home, cos::store::Kind::Briefs)?
        else {
            return Ok(json!({ "day": null, "openDeliverables": open }));
        };
        let v = &b.value;
        let prompts = v
            .needs_you
            .iter()
            .filter(|a| !a.agent_prompt.trim().is_empty())
            .count()
            + v.week
                .goals
                .iter()
                .filter(|g| !g.agent_prompt.trim().is_empty())
                .count();
        Ok(json!({
            "day": b.day,
            "actions": v.needs_you.len(),
            "goals": v.week.goals.len(),
            "prompts": prompts,
            "openDeliverables": open,
        }))
    })
    .await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SpawnArgs {
    harness: Harness,
    repo: Option<String>,
    /// Ignored when `deliverable` is set: the handoff is the prompt.
    #[serde(default)]
    prompt: String,
    /// A staff role from `context/agents.json`.
    #[serde(default)]
    role: Option<String>,
    /// A tracked deliverable to hand off, by id.
    #[serde(default)]
    deliverable: Option<String>,
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
    /// What the agent is judged against when it ends.
    #[serde(default)]
    contract: agents::Contract,
}

/// The hire as planned from the dialog's fields: the repo, the fit, the
/// contract, and why it would be refused.
fn plan(s: &Settings, args: &SpawnArgs) -> anyhow::Result<agents::hire::Plan> {
    let ask = match args.deliverable.as_deref().filter(|d| !d.trim().is_empty()) {
        Some(id) => agents::hire::Ask::Deliverable(id.to_owned()),
        None => {
            if args.prompt.trim().is_empty() {
                anyhow::bail!("write the prompt first");
            }
            agents::hire::Ask::Task(agents::Task {
                n: 0,
                title: args
                    .title
                    .clone()
                    .filter(|t| !t.trim().is_empty())
                    .unwrap_or_else(|| {
                        args.prompt
                            .lines()
                            .next()
                            .unwrap_or("")
                            .chars()
                            .take(80)
                            .collect()
                    }),
                prompt: args.prompt.clone(),
                source: args.source.clone().unwrap_or_else(|| "desktop".into()),
            })
        }
    };
    let repo = args
        .repo
        .as_deref()
        .filter(|r| !r.trim().is_empty())
        .map(PathBuf::from);
    let mut p = agents::hire::plan(
        &agents::hire::Where {
            home: &s.home,
            workspace: &s.workspace,
            projects: &s.projects,
        },
        args.role.as_deref().filter(|r| !r.trim().is_empty()),
        ask,
        repo,
        &args.contract,
    )?;
    if let Some(t) = args.title.as_deref().filter(|t| !t.trim().is_empty()) {
        p.title = t.to_owned();
    }
    Ok(p)
}

/// The pre-hire check the dialog shows before anything starts.
#[tauri::command]
async fn agents_fit(app: tauri::State<'_, App>, args: SpawnArgs) -> Res<agents::hire::Plan> {
    let s = app.get();
    blocking(move || plan(&s, &args)).await
}

#[tauri::command]
async fn agents_spawn(app: tauri::State<'_, App>, args: SpawnArgs) -> Res<Vec<Spec>> {
    let s = app.get();
    let cos = s.cos_binary().ok_or(
        "no cos binary: set COS_BIN, build it (`cargo build`), or `task install`".to_owned(),
    )?;
    blocking(move || {
        let planned = plan(&s, &args)?;
        (0..args.count.clamp(1, 32))
            .map(|_| {
                let req = planned.clone().request(agents::hire::Run {
                    harness: args.harness,
                    program: args.program.clone(),
                    model: args.model.clone().filter(|m| !m.trim().is_empty()),
                    worktree: args.worktree,
                    deadline: Duration::from_secs(args.deadline_secs),
                    budget_usd: args.budget_usd,
                    slots: s.slots,
                    // The window always sends both.
                    explicit: agents::policy::Explicit {
                        deadline: true,
                        budget: true,
                    },
                })?;
                agents::spawn(&s.home, req, &cos)
            })
            .collect()
    })
    .await
}

#[derive(Deserialize)]
struct RecordArgs {
    id: String,
    outcome: agents::Tested,
    #[serde(default)]
    notes: String,
    #[serde(default)]
    evidence: Vec<String>,
}

/// Stores what you found when you ran what a judged agent drafted.
#[tauri::command]
async fn agents_record(app: tauri::State<'_, App>, args: RecordArgs) -> Res<State> {
    let home = app.get().home;
    blocking(move || {
        agents::record(
            &home,
            &args.id,
            agents::Recorded {
                outcome: args.outcome,
                notes: args.notes,
                evidence: args.evidence,
                at: String::new(),
            },
        )
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

/// Activity from byte offset `from`: each tool call with what it ran, each
/// result, each message; and the offset to ask for next. Every entry has a
/// `key` from its place in the file, so a range read twice adds nothing.
#[tauri::command]
async fn agents_tail(app: tauri::State<'_, App>, id: String, from: u64) -> Res<Value> {
    let home = app.get().home;
    blocking(move || {
        let (spec, _) = agents::read(&home, &id)?;
        let (entries, next) =
            agents::view::feed(&agents::events_path(&home, &id), spec.harness, from);
        let seed_log = std::fs::read_to_string(agents::dir(&home, &id).join("seed.log")).ok();
        Ok(json!({ "entries": entries, "next": next, "seedLog": seed_log }))
    })
    .await
}

/// Agent prompts from the latest brief (actions or week goals), numbered as the report numbers them.
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
            let app = ctx.app_handle().clone();
            std::thread::spawn(move || {
                let path = req.uri().path().to_owned();
                let res = if path.starts_with("/docs") {
                    docs_response(&app, &req)
                } else {
                    report_response(&app, &path)
                };
                responder.respond(res)
            });
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
            agents_fit,
            agents_spawn,
            agents_record,
            agents_stop,
            agents_clean,
            agents_tail,
            agent_tasks,
            agents_roles,
            agents_scorecard,
            agents_rejudge,
            deliverables_open,
            brief_summary,
            repos,
            docs_save,
        ])
}
