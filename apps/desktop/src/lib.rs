//! Chief of Staff desktop: the `cos` report and an agent launcher in one
//! window, over the `cos` library.
//!
//! The app launches and watches; it owns no agent. Each agent is a setsid'd
//! `cos agents supervise` process, so closing the window stops nothing, and
//! reopening it reconciles every agent from the state each one recorded. The
//! report renders in a sandboxed iframe with an opaque origin: a brief quotes
//! mail and tickets, and nothing in it may reach these commands.

pub mod settings;

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use cos::agents::{self, Harness, Spec, State};
use cos::business::{self, Business, Dirs, Inputs, Layer, Sources, Triple};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tauri::menu::{Menu, MenuItemBuilder, SubmenuBuilder};
use tauri::{AppHandle, Builder, Emitter, Manager, Runtime};

pub use settings::Settings;

/// What the window starts from: the install, the files it reads, and the env values it was
/// started with. Tests build one directly; the app reads it from the environment.
#[derive(Debug, Clone)]
pub struct Start {
    /// The clone this binary was built from: the built-in home and workspace.
    pub install: PathBuf,
    pub user_home: Option<PathBuf>,
    pub registry: PathBuf,
    pub settings_file: PathBuf,
    /// `COS_BUSINESS`, `COS_HOME`, `COS_WORKSPACE`, `COS_PROJECTS`.
    pub sources: Sources,
    /// `COS_BIN`.
    pub cos_bin: Option<PathBuf>,
    /// `COS_AGENT_SLOTS`, ahead of the settings file.
    pub slots: Option<usize>,
}

impl Start {
    fn from_env<R: Runtime>(app: &AppHandle<R>) -> anyhow::Result<Self> {
        let env_path = |var: &str| {
            std::env::var_os(var)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        };
        let install = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        Ok(Self {
            install: install.canonicalize().unwrap_or(install),
            user_home: env_path("HOME"),
            registry: business::location()?,
            settings_file: app.path().app_config_dir()?.join("settings.json"),
            sources: Sources {
                business: std::env::var("COS_BUSINESS").ok().filter(|v| !v.is_empty()),
                dirs: Dirs {
                    home: env_path("COS_HOME"),
                    workspace: env_path("COS_WORKSPACE"),
                    projects: env_path("COS_PROJECTS"),
                },
            },
            cos_bin: env_path("COS_BIN"),
            slots: std::env::var("COS_AGENT_SLOTS")
                .ok()
                .and_then(|v| v.parse().ok()),
        })
    }
}

/// Problems found at start, shown in the header; none of them stops the window.
#[derive(Default)]
struct Notes {
    start: Option<String>,
    settings: Option<String>,
    legacy: Option<String>,
}

/// The window's state: the app business, the `cos` binary, and what start found.
pub struct AppState {
    settings: Settings,
    triple: Triple,
    cos: Option<PathBuf>,
    /// The variable that set the triple at start; a pinned window cannot switch.
    pinned: Option<&'static str>,
    registry: PathBuf,
    notes: Notes,
}

fn chain(e: anyhow::Error) -> String {
    format!("{e:#}")
}

/// The built-in defaults as a triple, for a start whose registry gave none. Without a
/// readable `HOME` there are no defaults, so every directory is the install.
fn defaults(builtin: Option<&Business>, install: &Path) -> Triple {
    let (home, workspace, projects) = match builtin {
        Some(b) => (b.home.clone(), b.workspace.clone(), b.projects.clone()),
        None => (install.to_owned(), install.to_owned(), install.to_owned()),
    };
    Triple {
        home,
        workspace,
        projects,
        business: None,
        layer: Layer::Defaults,
    }
}

fn pinned_by(s: &Sources) -> Option<&'static str> {
    [
        (s.business.is_some(), "COS_BUSINESS"),
        (s.dirs.home.is_some(), "COS_HOME"),
        (s.dirs.workspace.is_some(), "COS_WORKSPACE"),
        (s.dirs.projects.is_some(), "COS_PROJECTS"),
    ]
    .into_iter()
    .find(|(set, _)| *set)
    .map(|(_, var)| var)
}

/// The `cos` CLI that supervises agents, resolved once at start: `COS_BIN`, then the
/// settings file, then `cos` beside this executable, then the install's release and debug
/// builds, then `~/.cargo/bin/cos`.
fn cos_binary(
    env: Option<PathBuf>,
    saved: Option<PathBuf>,
    install: &Path,
    user_home: Option<&Path>,
) -> Option<PathBuf> {
    let beside = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|d| d.join("cos")));
    env.into_iter()
        .chain(saved)
        .chain(beside)
        .chain([
            install.join("target/release/cos"),
            install.join("target/debug/cos"),
        ])
        .chain(user_home.map(|h| h.join(".cargo/bin/cos")))
        .find(|p| p.is_file())
}

impl AppState {
    /// First-run migration when the registry is missing, then the triple from the env
    /// layer, the registry's active business, or the built-in defaults.
    fn boot(s: &Start) -> Self {
        let mut notes = Notes::default();
        let mut settings = Settings::load(&s.settings_file).unwrap_or_else(|e| {
            notes.settings = Some(chain(e));
            Settings::default()
        });
        settings.slots = s.slots.or(settings.slots);
        let inputs = Inputs {
            flags: Sources::default(),
            env: s.sources.clone(),
            install: s.install.clone(),
            user_home: s.user_home.clone(),
        };
        let builtin = match business::builtin(&inputs) {
            Ok(builtin) => {
                if !s.registry.exists() {
                    match business::first_run(&s.registry, &builtin) {
                        Ok(true) => notes.legacy = legacy_hint(&settings, &builtin),
                        Ok(false) => {}
                        Err(e) => notes.start = Some(chain(e)),
                    }
                }
                Some(builtin)
            }
            Err(e) => {
                notes.start = Some(chain(e));
                None
            }
        };
        let loaded = business::load(&s.registry);
        if let Err(e) = &loaded {
            notes.start.get_or_insert(format!("{e:#}"));
        }
        let resolved = business::resolve(&inputs, || match &loaded {
            Ok(registry) => Ok(registry.clone()),
            Err(e) => Err(anyhow::anyhow!("{e:#}")),
        });
        let triple = resolved.unwrap_or_else(|e| {
            notes.start.get_or_insert(chain(e));
            defaults(builtin.as_ref(), &s.install)
        });
        let cos = cos_binary(
            s.cos_bin.clone(),
            settings.cos.clone(),
            &s.install,
            s.user_home.as_deref(),
        );
        Self {
            settings,
            triple,
            cos,
            pinned: pinned_by(&s.sources),
            registry: s.registry.clone(),
            notes,
        }
    }
}

fn legacy_hint(settings: &Settings, builtin: &Business) -> Option<String> {
    settings
        .legacy(builtin)
        .filter(|legacy| legacy != builtin)
        .map(|legacy| {
            format!(
                "cos business add legacy {} {} {}",
                legacy.home.display(),
                legacy.workspace.display(),
                legacy.projects.display()
            )
        })
}

struct App(Mutex<AppState>);

/// One command's view of the app business. Taken once per command, so a switch never
/// lands between two reads of the same command.
#[derive(Clone)]
struct Current {
    business: String,
    home: PathBuf,
    workspace: PathBuf,
    projects: PathBuf,
    cos: Option<PathBuf>,
    slots: Option<usize>,
}

impl App {
    fn new(s: &Start) -> Self {
        Self(Mutex::new(AppState::boot(s)))
    }

    fn lock(&self) -> MutexGuard<'_, AppState> {
        self.0.lock().expect("app state lock")
    }

    fn get(&self) -> Current {
        let st = self.lock();
        Current {
            business: st.triple.label().to_owned(),
            home: st.triple.home.clone(),
            workspace: st.triple.workspace.clone(),
            projects: st.triple.projects.clone(),
            cos: st.cos.clone(),
            slots: st.settings.slots,
        }
    }
}

type Res<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// A write names the business it was issued for. A call sent before a switch
/// is refused here, before anything is read, written, run or killed.
fn named(cur: &Current, business: &str) -> Res<()> {
    if cur.business == business {
        return Ok(());
    }
    Err(format!(
        "this call names business {business:?} but the window is on {:?}: nothing written, run or killed",
        cur.business
    ))
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
    let st = app.lock();
    json!({
        "business": st.triple.label(),
        "home": st.triple.home,
        "workspace": st.triple.workspace,
        "projects": st.triple.projects,
        "cos": st.cos,
        "slots": st.settings.slots.unwrap_or_else(agents::slots::run_slots),
        "pinned": st.pinned,
        "startError": st.notes.start,
        "settingsError": st.notes.settings,
        "legacyHint": st.notes.legacy,
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SwitchArgs {
    name: String,
}

/// A business's directories must exist and be directories; the message names the repair.
fn check_dirs(name: &str, b: &Business) -> Res<()> {
    for (label, dir) in [
        ("home", &b.home),
        ("workspace", &b.workspace),
        ("projects", &b.projects),
    ] {
        if !dir.is_dir() {
            return Err(format!(
                "business {name:?}: {label} {} is not a directory; repair it with `cos business add`",
                dir.display()
            ));
        }
    }
    Ok(())
}

/// Points the window at another registered business. Nothing is written to disk.
#[tauri::command]
fn switch_business(app: tauri::State<'_, App>, args: SwitchArgs) -> Res<Value> {
    {
        let mut st = app.lock();
        if let Some(var) = st.pinned {
            return Err(format!(
                "pinned by {var}: this window's business comes from the environment; start it without {var} to switch"
            ));
        }
        let registry = business::load(&st.registry).map_err(chain)?;
        let found = registry.businesses.get(&args.name).ok_or_else(|| {
            let known: Vec<&str> = registry.businesses.keys().map(String::as_str).collect();
            format!(
                "business {:?} is not registered; known: {}",
                args.name,
                known.join(", ")
            )
        })?;
        check_dirs(&args.name, found)?;
        let dir = cos::state::runtime_dir(&found.workspace).map_err(chain)?;
        if !dir.join("profile.json").is_file() {
            return Err(format!(
                "{} has no profile.json; run `cos --workspace {} state init` first",
                dir.display(),
                found.workspace.display()
            ));
        }
        st.triple = Triple {
            home: found.home.clone(),
            workspace: found.workspace.clone(),
            projects: found.projects.clone(),
            business: Some(args.name.clone()),
            layer: Layer::Registry,
        };
    }
    Ok(paths(app))
}

#[derive(Serialize)]
struct Present {
    home: bool,
    workspace: bool,
    projects: bool,
}

#[derive(Serialize)]
struct Live {
    live: usize,
    lost: usize,
    unreadable: usize,
}

/// One business in the picker. A business whose home is missing carries no count.
#[derive(Serialize)]
struct BusinessRow {
    name: String,
    triple: Business,
    active: bool,
    app: bool,
    dirs: Present,
    missing: bool,
    #[serde(flatten)]
    counts: Option<Live>,
}

fn business_row(name: &str, b: &Business, active: bool, app: bool) -> BusinessRow {
    let dirs = Present {
        home: b.home.is_dir(),
        workspace: b.workspace.is_dir(),
        projects: b.projects.is_dir(),
    };
    let counts = dirs
        .home
        .then(|| agents::count(&b.home).ok())
        .flatten()
        .map(|c| Live {
            live: c.live,
            lost: c.lost,
            unreadable: c.unreadable,
        });
    BusinessRow {
        name: name.to_owned(),
        triple: b.clone(),
        active,
        app,
        missing: !dirs.home,
        dirs,
        counts,
    }
}

/// Every registered business with its directories and run counts, read without
/// touching any run.
#[tauri::command]
async fn businesses(app: tauri::State<'_, App>) -> Res<Vec<BusinessRow>> {
    let (registry, current) = {
        let st = app.lock();
        (st.registry.clone(), st.triple.business.clone())
    };
    blocking(move || {
        let reg = business::load(&registry)?;
        Ok(reg
            .businesses
            .iter()
            .map(|(name, b)| {
                business_row(
                    name,
                    b,
                    reg.active.as_deref() == Some(name),
                    current.as_deref() == Some(name),
                )
            })
            .collect())
    })
    .await
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
        "/flows" => Ok(cos::flows::page_cached(&s.workspace, &s.projects).into_bytes()),
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
    if *req.method() == tauri::http::Method::POST
        && req
            .headers()
            .get("x-cos-business")
            .and_then(|h| h.to_str().ok())
            != Some(s.business.as_str())
    {
        return docs_http(cos::docs::Reply {
            status: 403,
            kind: "text/plain; charset=utf-8",
            body: b"missing or stale X-Cos-Business header: nothing written".to_vec(),
            headers: Vec::new(),
        });
    }
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
async fn docs_save(app: tauri::State<'_, App>, business: String, organize: Value) -> Res<()> {
    let cur = app.get();
    named(&cur, &business)?;
    let home = cur.home;
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
            .runs
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
async fn agents_rejudge(
    app: tauri::State<'_, App>,
    business: String,
    id: String,
) -> Res<agents::Verdict> {
    let cur = app.get();
    named(&cur, &business)?;
    let home = cur.home;
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
fn plan(s: &Current, args: &SpawnArgs) -> anyhow::Result<agents::hire::Plan> {
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
async fn agents_spawn(
    app: tauri::State<'_, App>,
    business: String,
    args: SpawnArgs,
) -> Res<Vec<Spec>> {
    let s = app.get();
    named(&s, &business)?;
    let cos = s.cos.clone().ok_or(
        "no cos binary: set COS_BIN, build it (`cargo build`), or `task install`".to_owned(),
    )?;
    blocking(move || {
        let planned = plan(&s, &args)?;
        let triple = cos::business::Triple {
            home: s.home.clone(),
            workspace: s.workspace.clone(),
            projects: s.projects.clone(),
            business: None,
            layer: cos::business::Layer::Flags,
        };
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
                agents::spawn(&triple, req, &cos)
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
async fn agents_record(
    app: tauri::State<'_, App>,
    business: String,
    args: RecordArgs,
) -> Res<State> {
    let cur = app.get();
    named(&cur, &business)?;
    let home = cur.home;
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
async fn agents_stop(app: tauri::State<'_, App>, business: String, id: String) -> Res<State> {
    let cur = app.get();
    named(&cur, &business)?;
    let home = cur.home;
    blocking(move || agents::stop(&home, &id, Duration::from_secs(10))).await
}

#[tauri::command]
async fn agents_clean(
    app: tauri::State<'_, App>,
    business: String,
    id: String,
    force: bool,
) -> Res<()> {
    let cur = app.get();
    named(&cur, &business)?;
    let home = cur.home;
    blocking(move || agents::clean(&home, &id, force)).await
}

/// Activity from byte offset `from`: each tool call with what it ran, each
/// result, each message; and the offset to ask for next. Every entry has a
/// `key` from its place in the file, so a range read twice adds nothing.
#[tauri::command]
async fn agents_tail(app: tauri::State<'_, App>, id: String, from: u64) -> Res<Value> {
    let home = app.get().home;
    blocking(move || {
        let run = agents::run_dir(&home, &id)?;
        let (spec, _) = agents::read(&home, &id)?;
        let (entries, next) = agents::view::feed(&run.events_path(), spec.harness, from);
        let seed_log = std::fs::read_to_string(run.path().join("seed.log")).ok();
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

const SWITCH_MENU: &str = "switch-business";

/// ⌘K opens the picker from anywhere, frames included: the native menu takes the
/// key before the page does. Edit and Window keep the platform items, so copy,
/// paste and window commands still reach the focused frame.
fn app_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let switch = MenuItemBuilder::with_id(SWITCH_MENU, "Switch business…")
        .accelerator("CmdOrCtrl+K")
        .build(app)?;
    let app_sub = SubmenuBuilder::new(app, "Chief of Staff")
        .about(None)
        .separator()
        .item(&switch)
        .separator()
        .quit()
        .build()?;
    let edit = SubmenuBuilder::new(app, "Edit")
        .undo()
        .redo()
        .separator()
        .cut()
        .copy()
        .paste()
        .select_all()
        .build()?;
    let window = SubmenuBuilder::new(app, "Window")
        .minimize()
        .maximize()
        .separator()
        .close_window()
        .build()?;
    Menu::with_items(app, &[&app_sub, &edit, &window])
}

/// The window's menu bar. Menus are created on the main thread, so only the real
/// app adds it; the test harness builds without it.
pub fn with_menu<R: Runtime>(builder: Builder<R>) -> Builder<R> {
    builder.menu(app_menu).on_menu_event(|app, event| {
        if event.id().as_ref() == SWITCH_MENU {
            let _ = app.emit("switch-business", ());
        }
    })
}

/// The app on any runtime: the report scheme, the state (given, or read from the
/// environment and the config directory at setup), the watcher, the commands.
pub fn builder<R: Runtime>(builder: Builder<R>, start: Option<Start>) -> Builder<R> {
    let builder = match start {
        Some(s) => builder.manage(App::new(&s)),
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
                let start = Start::from_env(app.handle()).map_err(chain)?;
                app.manage(App::new(&start));
            }
            watch(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            paths,
            switch_business,
            businesses,
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
