//! Chief of Staff desktop: the `cos` report and an agent launcher in one
//! window, over the `cos` library.
//!
//! The app hires and judges; it owns no agent. Each hire runs in a Herdr
//! terminal, so closing the window stops nothing; while the window is open a
//! follower thread reads those terminals (`agents::tick`) for every
//! registered business, stops a run at its deadline and judges each once.
//! The report renders in a sandboxed iframe with an opaque origin: a brief
//! quotes mail and tickets, and nothing in it may reach these commands.

mod assistants;
mod links;
pub mod servers;
pub mod settings;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use cos::agents::{self, Harness, Spec, State};
use cos::business::{self, Business, Dirs, Inputs, Layer, Sources, Triple};
use cos::daemon::client;
use cos::pipeline::{self, RunChild};
use cos::runs;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tauri::menu::{Menu, MenuItemBuilder, SubmenuBuilder};
use tauri::{AppHandle, Builder, Emitter, Manager, Runtime};

pub use links::{Nav, open_link, parse_link};
pub use settings::Settings;

/// What the window starts from: the install, the files it reads, and the env values it was
/// started with. Tests build one directly; the app reads it from the environment.
#[derive(Debug, Clone)]
pub struct Start {
    /// The clone this binary was built from: the built-in home and workspace.
    pub install: PathBuf,
    pub user_home: Option<PathBuf>,
    /// `CODEX_HOME`: where Codex's `config.toml` lives, default `~/.codex`.
    pub codex_home: Option<PathBuf>,
    pub registry: PathBuf,
    pub settings_file: PathBuf,
    /// Holds `servers/`: pidfiles and port locks.
    pub data_dir: PathBuf,
    /// Holds `servers/`: one log per dashboard server.
    pub log_dir: PathBuf,
    pub ui_state_file: PathBuf,
    /// `COS_DASHBOARDS`; `None` means `dashboards.toml` beside `registry`.
    pub dashboards_file: Option<PathBuf>,
    /// `COS_BUSINESS`, `COS_HOME`, `COS_WORKSPACE`, `COS_PROJECTS`.
    pub sources: Sources,
    /// `COS_BIN`.
    pub cos_bin: Option<PathBuf>,
    /// `COS_AGENT_SLOTS`, ahead of the settings file.
    pub slots: Option<usize>,
    /// Where the `cos daemon` socket lives (`cos::daemon::dir`).
    pub daemon_dir: PathBuf,
}

impl Start {
    fn from_env<R: Runtime>(app: &AppHandle<R>) -> anyhow::Result<Self> {
        let env_path = |var: &str| {
            std::env::var_os(var)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        };
        let install = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let config = app.path().app_config_dir()?;
        Ok(Self {
            install: install.canonicalize().unwrap_or(install),
            user_home: env_path("HOME"),
            codex_home: env_path("CODEX_HOME"),
            registry: business::location()?,
            settings_file: config.join("settings.json"),
            data_dir: app.path().app_data_dir()?,
            log_dir: app.path().app_log_dir()?,
            ui_state_file: config.join("ui-state.json"),
            dashboards_file: env_path("COS_DASHBOARDS"),
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
            daemon_dir: cos::daemon::dir()?,
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

/// The `cos` CLI a hire's build shims call back into, resolved once at start: `COS_BIN`, then the
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
    /// The registry name; `None` when the triple came from the env or defaults layer.
    registered: Option<String>,
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
            registered: st.triple.business.clone(),
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
    same_label(&cur.business, business)
}

fn same_label(label: &str, business: &str) -> Res<()> {
    if label == business {
        return Ok(());
    }
    Err(format!(
        "this call names business {business:?} but the window is on {label:?}: nothing written, run or killed"
    ))
}

/// What a business-checked command reads of the app business, copied in the same
/// App-lock acquisition as the check, so a switch cannot land between the two.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checked {
    /// The registry business; `None` when the triple came from the env or defaults layer.
    pub business: Option<String>,
    pub home: PathBuf,
}

/// Refuses a call naming a business other than the app business label, with `named()`'s text.
pub fn check_business<R: Runtime>(app: &AppHandle<R>, business: &str) -> Res<Checked> {
    let st = app.state::<App>();
    let st = st.lock();
    same_label(st.triple.label(), business)?;
    Ok(Checked {
        business: st.triple.business.clone(),
        home: st.triple.home.clone(),
    })
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

#[tauri::command]
fn tax_read(app: tauri::State<'_, App>) -> Res<Value> {
    let dir = cos::state::runtime_dir(&app.get().workspace).map_err(err)?;
    let path = dir.join("tax-planning.json");
    if !path.exists() {
        return Ok(Value::Null);
    }
    cos::state::load(&path).map_err(err)
}

#[tauri::command]
fn tax_save(app: tauri::State<'_, App>, data: Value) -> Res<()> {
    let dir = cos::state::runtime_dir(&app.get().workspace).map_err(err)?;
    if !dir.is_dir() {
        return Err("Set up the workspace before saving tax planning.".into());
    }
    let bytes = serde_json::to_vec(&data).map_err(err)?;
    if !data.is_object() || bytes.len() > 32_768 {
        return Err("Invalid tax planning data.".into());
    }
    cos::state::atomic_write(&dir.join("tax-planning.json"), &data).map_err(err)
}

/// Points the window at another registered business. Nothing is written to disk.
#[tauri::command]
fn switch_business(
    app: tauri::State<'_, App>,
    links: tauri::State<'_, links::Links>,
    args: SwitchArgs,
) -> Res<Value> {
    switch_in(&mut app.lock(), &args.name)?;
    links.unready();
    Ok(paths(app))
}

/// Points `st` at the registered business `name`, unless the window is pinned.
fn switch_in(st: &mut AppState, name: &str) -> Res<()> {
    st.triple = switch_target(st, name)?;
    Ok(())
}

/// Where `switch_in` would point `st`, or why it may not; `st` is untouched.
fn switch_target(st: &AppState, name: &str) -> Res<Triple> {
    if let Some(var) = st.pinned {
        return Err(format!(
            "pinned by {var}: this window's business comes from the environment; start it without {var} to switch"
        ));
    }
    let registry = business::load(&st.registry).map_err(chain)?;
    let found = registry.businesses.get(name).ok_or_else(|| {
        let known: Vec<&str> = registry.businesses.keys().map(String::as_str).collect();
        format!(
            "business {name:?} is not registered; known: {}",
            known.join(", ")
        )
    })?;
    business::check_dirs(name, found).map_err(chain)?;
    let dir = cos::state::runtime_dir(&found.workspace).map_err(chain)?;
    if !dir.join("profile.json").is_file() {
        return Err(format!(
            "{} has no profile.json; run `cos --workspace {} state init` first",
            dir.display(),
            found.workspace.display()
        ));
    }
    Ok(Triple {
        home: found.home.clone(),
        workspace: found.workspace.clone(),
        projects: found.projects.clone(),
        business: Some(name.to_owned()),
        layer: Layer::Registry,
    })
}

/// Sets the registry's active business, the one MCP clients act on when a
/// call names none. The window stays on its own business.
#[tauri::command]
async fn business_make_default<R: Runtime>(app: AppHandle<R>, name: String) -> Res<Value> {
    let registry = app.state::<App>().lock().registry.clone();
    let set = name.clone();
    blocking(move || business::set_active(&registry, &set)).await?;
    publish(&app, Rebuild::All);
    Ok(json!({ "active": name }))
}

#[derive(Serialize, Clone, PartialEq)]
struct Present {
    home: bool,
    workspace: bool,
    projects: bool,
}

#[derive(Serialize, Clone, PartialEq)]
struct Live {
    live: usize,
    lost: usize,
    unreadable: usize,
}

/// One business in the picker. A business whose home is missing carries
/// no count.
#[derive(Serialize, Clone, PartialEq)]
struct BusinessRow {
    name: String,
    triple: Business,
    active: bool,
    app: bool,
    dirs: Present,
    missing: bool,
    unread: bool,
    scheduled: bool,
    #[serde(flatten)]
    counts: Option<Live>,
}

/// The brief the owner last saw in Today, as the report rendered it.
#[derive(Serialize, Deserialize)]
struct SeenBrief {
    file: String,
    mtime: i64,
}

#[derive(Serialize, Deserialize, Default)]
struct UiState {
    #[serde(rename = "lastSeenBrief", default)]
    last_seen_brief: BTreeMap<String, SeenBrief>,
}

/// An unreadable or unparseable file reads as empty.
fn read_ui_state(path: &Path) -> UiState {
    std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

/// `^\d{4}-\d{2}-\d{2}\.json$`: a bare name, so it can only name a file in `out/briefs`.
fn brief_name(file: &str) -> bool {
    let b = file.as_bytes();
    b.len() == 15
        && file.ends_with(".json")
        && b[..10].iter().enumerate().all(|(i, c)| match i {
            4 | 7 => *c == b'-',
            _ => c.is_ascii_digit(),
        })
}

/// `VERIFY(brief_seen)`'s file checks. The file's metadata is a check only, never the
/// value stored: a brief rewritten after render keeps the mtime the owner saw.
fn verify_brief_seen(home: &Path, file: &str, mtime: i64) -> Res<()> {
    if !brief_name(file) {
        return Err(format!(
            "brief_seen: {file:?} is not a YYYY-MM-DD.json brief name: nothing written"
        ));
    }
    if mtime < 0 {
        return Err(format!(
            "brief_seen: mtime {mtime} is before the Unix epoch: nothing written"
        ));
    }
    let dir = home.join("out/briefs");
    let meta = std::fs::metadata(dir.join(file))
        .ok()
        .filter(std::fs::Metadata::is_file)
        .ok_or_else(|| {
            format!(
                "brief_seen: no brief {file:?} under {}: nothing written",
                dir.display()
            )
        })?;
    let current = cos::signals::mtime_ms(&meta);
    if mtime > current {
        return Err(format!(
            "brief_seen: mtime {mtime} is past {file}'s current mtime {current}: nothing written"
        ));
    }
    Ok(())
}

fn lock_path(file: &Path) -> PathBuf {
    let mut path = file.as_os_str().to_owned();
    path.push(".lock");
    PathBuf::from(path)
}

/// `STORE(ui-state)`: keeps the greater of the stored and the given pair for `business`
/// and drops entries for unregistered names. Nothing else creates the config dir, and
/// `atomic_write_bytes` needs the parent to exist.
fn store_seen(
    ui_state_file: &Path,
    registry: &Path,
    business: &str,
    seen: SeenBrief,
) -> anyhow::Result<()> {
    if let Some(dir) = ui_state_file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let _held = agents::slots::lock_exclusive(&lock_path(ui_state_file))?;
    let registered = business::load(registry)?.businesses;
    let mut state = read_ui_state(ui_state_file);
    state
        .last_seen_brief
        .retain(|name, _| registered.contains_key(name));
    let lower = state
        .last_seen_brief
        .get(business)
        .is_some_and(|s| (seen.file.as_str(), seen.mtime) < (s.file.as_str(), s.mtime));
    if !lower {
        state.last_seen_brief.insert(business.to_owned(), seen);
    }
    cos::state::atomic_write_bytes(ui_state_file, &serde_json::to_vec(&state)?)
}

/// Records the brief the report rendered in Today as seen, then publishes the business's
/// row so the dot clears at once.
#[tauri::command]
async fn brief_seen<R: Runtime>(
    app: AppHandle<R>,
    business: String,
    file: String,
    mtime: i64,
) -> Res<()> {
    let checked = check_business(&app, &business)?;
    // The defaults layer is labelled `default`, which a registered business may also be named.
    if checked.business.is_none() {
        return Err(format!(
            "brief_seen: the window on {business:?} has no registry business: nothing written"
        ));
    }
    verify_brief_seen(&checked.home, &file, mtime)?;
    blocking(move || {
        let start = app.state::<Start>();
        store_seen(
            &start.ui_state_file,
            &start.registry,
            &business,
            SeenBrief { file, mtime },
        )?;
        publish(&app, Rebuild::Named(&[business]));
        Ok(())
    })
    .await
}

/// One dashboard tab of the app business.
#[derive(Serialize)]
struct DashboardTab {
    name: String,
    url: String,
    managed: bool,
}

/// The app business's dashboards, from `dashboards.toml` read on every call.
#[tauri::command]
async fn dashboards<R: Runtime>(app: AppHandle<R>, business: String) -> Res<Vec<DashboardTab>> {
    let checked = check_business(&app, &business)?;
    // The defaults layer is labelled `default`, which a registered business may also be named.
    let Some(name) = checked.business else {
        return Ok(Vec::new());
    };
    blocking(move || {
        let start = app.state::<Start>();
        let file = cos::dashboards::location(&start.registry, start.dashboards_file.as_deref());
        let loaded = cos::dashboards::load(&file)?;
        Ok(loaded
            .for_business(&name)
            .iter()
            .map(|d| DashboardTab {
                name: d.name.clone(),
                url: d.url.clone(),
                managed: d.server.is_some(),
            })
            .collect())
    })
    .await
}

/// One dashboard of the app business, from `dashboards.toml` read on every call; `ensure`
/// decides whether it runs.
#[tauri::command]
async fn dashboard_open<R: Runtime>(
    app: AppHandle<R>,
    business: String,
    name: String,
) -> Result<servers::Running, servers::Refusal> {
    use servers::Refusal;
    let checked =
        check_business(&app, &business).map_err(|message| Refusal::StaleBusiness { message })?;
    let Some(owner) = checked.business else {
        return Err(Refusal::UnknownDashboard { name });
    };
    tauri::async_runtime::spawn_blocking(move || {
        let start = app.state::<Start>();
        let file = cos::dashboards::location(&start.registry, start.dashboards_file.as_deref());
        let loaded = cos::dashboards::load(&file).map_err(|e| Refusal::InvalidConfig {
            message: format!("{e:#}"),
        })?;
        let Some(dashboard) = loaded.for_business(&owner).iter().find(|d| d.name == name) else {
            return Err(Refusal::UnknownDashboard { name });
        };
        app.state::<servers::Supervisor>()
            .ensure(loaded.server_of(dashboard), dashboard)
    })
    .await
    .map_err(|e| Refusal::Internal {
        message: e.to_string(),
    })?
}

/// What the row builder reads, copied from the managed `Start` and the App lock.
struct RowSource {
    registry: PathBuf,
    user_home: Option<PathBuf>,
    ui_state_file: PathBuf,
}

impl RowSource {
    fn new(registry: PathBuf, start: &Start) -> Self {
        Self {
            registry,
            user_home: start.user_home.clone(),
            ui_state_file: start.ui_state_file.clone(),
        }
    }

    /// The business the installed `cos.brief` plist runs for on this registry.
    fn clock(&self) -> Option<String> {
        let home = self.user_home.as_deref()?;
        let registry = self.registry.canonicalize().ok()?;
        cos::signals::scheduled(&home.join(BRIEF_PLIST), &registry, Some(home))
    }

    /// Every registered business's row, or only `only`'s, each with `app` false. Reads
    /// files and kernel state only: it spawns no process.
    fn rows(&self, only: Option<&str>) -> anyhow::Result<Vec<BusinessRow>> {
        Ok(self.rows_of(&business::load(&self.registry)?, only))
    }

    fn rows_of(&self, reg: &business::Registry, only: Option<&str>) -> Vec<BusinessRow> {
        let seen = read_ui_state(&self.ui_state_file).last_seen_brief;
        let clock = self.clock();
        reg.businesses
            .iter()
            .filter(|(name, _)| only.is_none_or(|o| o == name.as_str()))
            .map(|(name, b)| {
                let unread = cos::signals::latest_brief(&b.home).is_some_and(|brief| {
                    seen.get(name).is_none_or(|s| {
                        (brief.file.as_str(), brief.mtime) > (s.file.as_str(), s.mtime)
                    })
                });
                BusinessRow {
                    active: reg.active.as_deref() == Some(name),
                    unread,
                    scheduled: clock.as_deref() == Some(name),
                    ..business_row(name, b)
                }
            })
            .collect()
    }
}

/// Under `Start.user_home`.
const BRIEF_PLIST: &str = "Library/LaunchAgents/cos.brief.plist";

/// The rows last emitted on `businesses-changed`, without `app`.
#[derive(Default)]
struct RowCache(Mutex<BTreeMap<String, BusinessRow>>);

/// The rows `publish` rebuilds.
enum Rebuild<'a> {
    /// The registry changed: every row, replacing the whole cache.
    All,
    Named(&'a [String]),
}

/// The one way rows reach `businesses-changed`. Rows are built here while the cache is
/// held, never passed in, so a row built before another publisher's write cannot land
/// after that publisher's row. A cache missing a registered business is first filled
/// with every row. Emits all cached rows, with `app` from the App's business at that
/// moment, only when a cached row changed.
fn publish<R: Runtime>(app: &AppHandle<R>, rebuild: Rebuild<'_>) {
    let cache = app.state::<RowCache>();
    let mut cache = cache.0.lock().expect("row cache lock");
    let start = app.state::<Start>();
    let source = RowSource::new(start.registry.clone(), &start);
    let Ok(reg) = business::load(&source.registry) else {
        return;
    };
    let full = reg.businesses.keys().all(|n| cache.contains_key(n));
    let mut next = match rebuild {
        Rebuild::All => BTreeMap::new(),
        Rebuild::Named(_) => cache.clone(),
    };
    let rows = match rebuild {
        Rebuild::Named(names) if full => names
            .iter()
            .flat_map(|n| source.rows_of(&reg, Some(n)))
            .collect(),
        _ => source.rows_of(&reg, None),
    };
    for row in rows {
        next.insert(row.name.clone(), row);
    }
    if next == *cache {
        return;
    }
    *cache = next;
    let current = app.state::<App>().lock().triple.business.clone();
    let rows: Vec<BusinessRow> = cache
        .values()
        .map(|r| BusinessRow {
            app: current.as_deref() == Some(r.name.as_str()),
            ..r.clone()
        })
        .collect();
    let _ = app.emit("businesses-changed", rows);
}

fn business_row(name: &str, b: &Business) -> BusinessRow {
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
        active: false,
        app: false,
        missing: !dirs.home,
        dirs,
        unread: false,
        scheduled: false,
        counts,
    }
}

/// Every registered business with its directories, run counts, unread brief and clock,
/// read without touching any run.
#[tauri::command]
async fn businesses(
    app: tauri::State<'_, App>,
    start: tauri::State<'_, Start>,
) -> Res<Vec<BusinessRow>> {
    let (source, current) = {
        let st = app.lock();
        (
            RowSource::new(st.registry.clone(), &start),
            st.triple.business.clone(),
        )
    };
    blocking(move || {
        let mut rows = source.rows(None)?;
        for row in &mut rows {
            row.app = current.as_deref() == Some(row.name.as_str());
        }
        Ok(rows)
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
    let page = match path {
        "/" | "" => Some(links::DayPage::Latest),
        p => match p.strip_prefix("/brief/") {
            Some(day) => links::day_page(&s.home, day),
            None => cos::dash::archived(&s.home, p).map(links::DayPage::Archived),
        },
    };
    let body = match page {
        Some(links::DayPage::Latest) => cos::report::data(&s.home, &s.workspace, false)
            .and_then(|mut d| {
                d["hostBrief"] = true.into();
                cos::report::render(&d)
            })
            .map(String::into_bytes)
            .map_err(|e| format!("{e:#}")),
        Some(links::DayPage::Archived(f)) => std::fs::read(f).map_err(err),
        None => Err("not found".to_owned()),
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

/// `cosreport://localhost/flows`: the Flows page of the app business's
/// projects, its script held to its own inline code by `flows::PAGE_CSP`;
/// `?fresh=1` (the page's Refresh) scans again inside the cache's lifetime.
/// Everything else in the query belongs to the page.
pub fn flows_response<R: Runtime>(
    app: &AppHandle<R>,
    query: Option<&str>,
) -> tauri::http::Response<Vec<u8>> {
    let s = app.state::<App>().get();
    let fresh = query.is_some_and(|q| q.split('&').any(|p| p == "fresh=1"));
    let pages = app.state::<FlowPages>().0.clone();
    let held = (!fresh)
        .then(|| pages.cached(&s.workspace, &s.projects))
        .flatten();
    let body = match held {
        Some((age, html)) => {
            if age >= cos::flows::STALE {
                flows_behind(app.clone());
            }
            html
        }
        None => pages.scan(
            &s.workspace,
            &s.projects,
            Some(&s.home),
            fresh,
            &flows_progress(app),
        ),
    };
    tauri::http::Response::builder()
        .status(200)
        .header("Content-Type", "text/html; charset=utf-8")
        .header("Cache-Control", "no-store")
        .header("Content-Security-Policy", cos::flows::PAGE_CSP)
        .body(body.into_bytes())
        .expect("static response parts")
}

/// The Flows pages this window serves, one held per workspace and projects
/// directory, so a business switch never shows another business's flows.
pub struct FlowPages(pub std::sync::Arc<cos::flows::Pages>);

#[derive(Clone, Serialize)]
struct FlowsProgress {
    done: usize,
    total: usize,
    repo: String,
}

/// `flows-progress` for each repo a scan moves to, so the tab can say
/// "reading 3 of 8 repos" while the frame waits.
fn flows_progress<R: Runtime>(app: &AppHandle<R>) -> impl Fn(usize, usize, &str) + Sync + use<R> {
    let app = app.clone();
    move |done, total, repo| {
        let _ = app.emit(
            "flows-progress",
            FlowsProgress {
                done,
                total,
                repo: repo.to_owned(),
            },
        );
    }
}

/// Scans the flows on a thread of its own unless a scan is running, keeps
/// the page, and emits `flows-changed` so the tab swaps it in.
fn flows_behind<R: Runtime>(app: AppHandle<R>) {
    std::thread::spawn(move || {
        let s = app.state::<App>().get();
        let pages = app.state::<FlowPages>().0.clone();
        if pages
            .try_scan(
                &s.workspace,
                &s.projects,
                Some(&s.home),
                &flows_progress(&app),
            )
            .is_some()
        {
            let _ = app.emit("flows-changed", ());
        }
    });
}

/// Where a link the window or one of its frames asks for goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Link {
    /// The window's own pages, the report scheme and the dashboards the app
    /// serves on the loopback address: they load in place.
    InApp,
    /// http, https and mailto: the default browser or mail app.
    Browser,
    /// Anything else (file:, javascript:, a custom scheme): not opened.
    Refused,
}

pub fn link_kind(url: &tauri::Url) -> Link {
    let local = url.host_str().is_some_and(|h| {
        h == "localhost" || h.ends_with(".localhost") || h == "127.0.0.1" || h == "[::1]"
    });
    match url.scheme() {
        "tauri" | "cosreport" | "about" | "data" | "blob" | "ipc" | "asset" => Link::InApp,
        "http" | "https" if local => Link::InApp,
        "http" | "https" | "mailto" => Link::Browser,
        _ => Link::Refused,
    }
}

/// Hands `url` to `open` when it may leave the app; a refused one is told on
/// the dev channel (the scheme only: the address may carry anything). True
/// when it went out.
pub fn send_out(url: &tauri::Url, open: impl FnOnce(String)) -> bool {
    match link_kind(url) {
        Link::Browser => {
            open(url.to_string());
            true
        }
        Link::InApp => false,
        Link::Refused => {
            eprintln!(
                "link refused: the app opens http, https and mailto, not {}:",
                url.scheme()
            );
            false
        }
    }
}

/// `open <url>` on a thread of its own: the hooks run on the window's thread.
fn open_in_browser(url: String) {
    std::thread::spawn(move || {
        if let Err(e) = cos::report::open(Path::new(&url)) {
            eprintln!("link: could not open it in the browser: {e:#}");
        }
    });
}

/// The whole `cosreport` scheme: the Origin rule, then the docs room or the report.
pub fn cosreport_response<R: Runtime>(
    app: &AppHandle<R>,
    req: &tauri::http::Request<Vec<u8>>,
) -> tauri::http::Response<Vec<u8>> {
    let path = req.uri().path();
    if !origin_allowed(req) {
        let origin = req
            .headers()
            .get("origin")
            .map(|o| o.to_str().unwrap_or("?"));
        eprintln!(
            "cosreport: refused {} {path} from origin {origin:?}",
            req.method()
        );
        return tauri::http::Response::builder()
            .status(403)
            .header("Content-Type", "text/plain; charset=utf-8")
            .header("Cache-Control", "no-store")
            .body(b"origin not allowed: nothing written".to_vec())
            .expect("static response parts");
    }
    if path.starts_with("/docs") {
        docs_response(app, req)
    } else if path == "/flows" {
        flows_response(app, req.uri().query())
    } else {
        report_response(app, path)
    }
}

/// The app's own origins. A dashboard frame is `http://127.0.0.1:*`, and a frame
/// nested in one sends `null`, so neither may write through `x-cos-business`.
const APP_ORIGINS: [&str; 4] = [
    "tauri://localhost",
    "http://tauri.localhost",
    "cosreport://localhost",
    "http://cosreport.localhost",
];

/// GET admits no Origin, `null` (the opaque report frames) or an app origin;
/// every other method needs an app origin.
fn origin_allowed(req: &tauri::http::Request<Vec<u8>>) -> bool {
    let origin = req.headers().get("origin").map(|o| o.to_str().ok());
    let app = matches!(origin, Some(Some(o)) if APP_ORIGINS.contains(&o));
    if *req.method() == tauri::http::Method::GET {
        app || matches!(origin, None | Some(Some("null")))
    } else {
        app
    }
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
    /// Why it needs you now, in words; `None` when it does not.
    needs: Option<String>,
    identity: u8,
    /// Active time at the moment of the call; the window ticks it on.
    active_ms: u64,
    paused_now: bool,
    phase: agents::Phase,
    /// Done, and ended today in the business profile's timezone.
    done_today: bool,
}

#[tauri::command]
async fn agents_list(app: tauri::State<'_, App>) -> Res<Vec<Row>> {
    let cur = app.get();
    let (home, ws) = (cur.home, cur.workspace);
    blocking(move || {
        let now = jiff::Timestamp::now();
        let tz = cos::digest::zone(&ws);
        Ok(agents::list(&home)?
            .runs
            .into_iter()
            .map(|(spec, state)| {
                let needs = agents::needs(&spec, &state);
                let clock = agents::events::tail_summary(&home, &spec.id, now, &state)
                    .unwrap_or_else(|_| agents::timeline::stored_summary(&state, now));
                Row {
                    identity: agents::identity(spec.role.as_deref()),
                    active_ms: clock.active_ms,
                    paused_now: clock.paused_now,
                    phase: agents::phase(&state, &clock),
                    done_today: agents::done_today(&state, &tz, now),
                    spec,
                    state,
                    needs,
                }
            })
            .collect())
    })
    .await
}

/// Every hire as last stored, for Today's live inbox. Read-only: it calls
/// `agents::snapshot` and nothing else, so it never reconciles, signals or
/// writes (the fleet's `agents_list` does reconcile).
#[tauri::command]
async fn agents_snapshot(
    app: tauri::State<'_, App>,
    business: String,
) -> Res<Vec<agents::HireView>> {
    let cur = app.get();
    named(&cur, &business)?;
    let (home, ws) = (cur.home, cur.workspace);
    blocking(move || {
        Ok(agents::snapshot(
            &home,
            &cos::digest::zone(&ws),
            jiff::Timestamp::now(),
        ))
    })
    .await
}

/// A run's timeline at now: spans, hand-offs and marks, read from its
/// event log (`agents::timeline::fold`).
#[tauri::command]
async fn agents_timeline(
    app: tauri::State<'_, App>,
    business: String,
    id: String,
) -> Res<agents::timeline::Timeline> {
    let cur = app.get();
    named(&cur, &business)?;
    let home = cur.home;
    blocking(move || {
        let (spec, state) = agents::read(&home, &id)?;
        let events = agents::events::read(&home, &id);
        Ok(agents::timeline::fold(
            &spec,
            &state,
            &events,
            jiff::Timestamp::now(),
        ))
    })
    .await
}

/// Your message into the run's Herdr terminal (`hc send`).
#[tauri::command]
async fn agents_say(
    app: tauri::State<'_, App>,
    business: String,
    id: String,
    text: String,
) -> Res<State> {
    let cur = app.get();
    named(&cur, &business)?;
    let home = cur.home;
    blocking(move || agents::say(&home, &id, &text)).await
}

/// The agent's stop key (`hc interrupt`): its turn ends, it stays.
#[tauri::command]
async fn agents_pause(app: tauri::State<'_, App>, business: String, id: String) -> Res<State> {
    let cur = app.get();
    named(&cur, &business)?;
    let home = cur.home;
    blocking(move || agents::pause(&home, &id)).await
}

/// Ends the run's Herdr terminal (`hc kill`) and judges it, once.
#[tauri::command]
async fn agents_done(app: tauri::State<'_, App>, business: String, id: String) -> Res<State> {
    let cur = app.get();
    named(&cur, &business)?;
    let home = cur.home;
    blocking(move || agents::mark_done(&home, &id)).await
}

/// Brings the run's terminal up in the Herdr window; says what happened.
#[tauri::command]
async fn agents_open(app: tauri::State<'_, App>, id: String) -> Res<String> {
    let home = app.get().home;
    blocking(move || agents::open_in_herdr(&home, &id)).await
}

/// The run's last screen and `lines` of history (`hc read`); once it has
/// ended, what it showed then.
#[tauri::command]
async fn agents_screen(
    app: tauri::State<'_, App>,
    id: String,
    lines: Option<usize>,
) -> Res<String> {
    let home = app.get().home;
    blocking(move || agents::screen(&home, &id, lines.unwrap_or(80).min(2000))).await
}

/// Whether herdr-core answers. Asked on every render of the hire dialog, so
/// the answer is reused for a few seconds.
#[tauri::command]
async fn herdr_status() -> Res<cos::herdr::Server> {
    blocking(|| Ok(cos::herdr::server_cached())).await
}

/// Start Herdr: `hc server start`, then ask again.
#[tauri::command]
async fn herdr_start() -> Res<cos::herdr::Server> {
    blocking(cos::herdr::start).await
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

/// Every skill on this machine with what the user decided about it, the
/// graph's state, and the hireable roles the panel offers.
#[tauri::command]
async fn skills_catalog(app: tauri::State<'_, App>) -> Res<Value> {
    let home = app.get().home;
    blocking(move || {
        let ctx = agents::skill_view::load(&home, agents::skill_facts::Mode::Live)?;
        let roles: Vec<&str> = ctx
            .policy
            .roles
            .iter()
            .filter(|r| !r.head)
            .map(|r| r.id.as_str())
            .collect();
        Ok(json!({
            "rows": agents::skill_view::rows(&ctx.catalog, &ctx.graph),
            "graph": ctx.graph.state,
            "warnings": ctx.graph.warnings,
            "roles": roles,
            "prime": ctx.prime.as_ref().err(),
        }))
    })
    .await
}

/// Records the user's decision about one skill in Prime. Refused unless it
/// names the window's business.
#[tauri::command]
async fn skills_set(
    app: tauri::State<'_, App>,
    business: String,
    decision: agents::skill_facts::Decision,
) -> Res<Vec<String>> {
    let s = app.get();
    named(&s, &business)?;
    blocking(move || {
        let ctx = agents::skill_view::load(&s.home, agents::skill_facts::Mode::Live)?;
        let prime = ctx
            .prime
            .as_ref()
            .map_err(|e| anyhow::anyhow!("Prime is not reachable: {e}"))?;
        agents::skill_facts::write_fact(&s.home, prime, &ctx.catalog, &ctx.policy, &decision)
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
    /// Identical agents to start; each gets its own worktree.
    count: usize,
    source: Option<String>,
    /// Overrides the harness binary; the window never sends it (tests do).
    #[serde(default)]
    program: Option<String>,
    /// What the agent is judged against when it ends.
    #[serde(default)]
    contract: agents::Contract,
    /// Skills added to or removed from the role's.
    #[serde(default)]
    skills: agents::skills::Asked,
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
        &agents::hire::SkillsAsk::from_env(args.skills.clone(), agents::skill_facts::Mode::Live),
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
                    slots: s.slots,
                    // The window always sends it.
                    explicit: agents::policy::Explicit { deadline: true },
                })?;
                agents::spawn_background(&triple, req, &cos)
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

/// What seeding its worktree did, step by step.
#[tauri::command]
async fn agents_seed(app: tauri::State<'_, App>, id: String) -> Res<Value> {
    let home = app.get().home;
    blocking(move || {
        let run = agents::run_dir(&home, &id)?;
        let seed_log = std::fs::read_to_string(run.path().join("seed.log")).ok();
        Ok(json!({ "seedLog": seed_log }))
    })
    .await
}

/// Agent prompts from the latest brief (actions or week goals), numbered as the report numbers them.
#[tauri::command]
async fn agent_tasks(app: tauri::State<'_, App>, from: agents::From) -> Res<Vec<agents::Task>> {
    let home = app.get().home;
    blocking(move || agents::tasks(&home, from)).await
}

/// The window business's latest brief run, from whichever client started it.
/// `brief-changed` carries one of these for asks too, with `kind` saying so.
#[derive(Default, Clone, Serialize)]
struct BriefRun {
    running: bool,
    /// The run id, once a run record exists.
    run: Option<String>,
    kind: Option<runs::Kind>,
    /// The record's state as a reader shows it.
    state: Option<runs::State>,
    /// The business it runs for.
    business: Option<String>,
    /// Seconds since the epoch.
    started: Option<u64>,
    ended: Option<u64>,
    ok: Option<bool>,
    deferred: bool,
    /// Why it failed, in its own last words.
    said: Option<String>,
}

impl BriefRun {
    fn of(r: &runs::Record, shown: runs::State) -> Self {
        let fin = shown.is_final();
        let said = match shown {
            runs::State::Done => None,
            runs::State::Lost if !r.state.is_final() => Some("its process is gone".to_owned()),
            _ => r.said.clone().or_else(|| r.reason.clone()),
        };
        BriefRun {
            running: !fin,
            run: Some(r.id.clone()),
            kind: Some(r.kind),
            state: Some(shown),
            business: Some(r.business.clone()),
            started: u64::try_from(r.started).ok(),
            ended: r.ended.and_then(|e| u64::try_from(e).ok()),
            ok: fin.then_some(shown == runs::State::Done),
            deferred: shown == runs::State::Deferred,
            said: if fin { said } else { None },
        }
    }
}

#[derive(Default)]
struct Brief(Mutex<BriefRun>);

/// The runs this window started itself because no daemon answered, by run
/// id. Each is followed by its own thread up to `pipeline::BRIEF_CHILD`, and
/// all are stopped and reaped when the app quits.
#[derive(Default)]
struct AppRuns(Mutex<BTreeMap<String, Arc<Mutex<RunChild>>>>);

/// The ids of the runs this window owns and has not reaped yet.
pub fn owned_runs<R: Runtime>(app: &AppHandle<R>) -> Vec<String> {
    app.state::<AppRuns>()
        .0
        .lock()
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default()
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
}

/// Starts `cos brief` for the app business. When the daemon answers, it
/// owns the run (`start`); when it is down, this process does (`spawn_run`,
/// owner `app`). A probe that fails other than by the daemon being down is
/// an error, so a lost reply never starts a second paid run. Returns at
/// once; `brief-changed` fires as it starts and ends.
#[tauri::command]
async fn brief_run<R: Runtime>(handle: AppHandle<R>, business: String) -> Res<BriefRun> {
    let s = handle.state::<App>().get();
    named(&s, &business)?;
    let dir = handle.state::<Start>().daemon_dir.clone();
    let h = handle.clone();
    let snapshot = blocking(move || {
        if let Some(name) = &s.registered
            && let client::Probe::Up { .. } = client::probe(&dir)?
        {
            let (run, _existing) = client::start(&dir, runs::Kind::Brief, name, None)?;
            let mut last = None;
            let shown = runs::shown_from(
                || {
                    last = runs::read(&s.home, &run).ok().flatten();
                    last.clone()
                },
                runs::process_gone,
            );
            let snap = match (last, shown) {
                (Some(r), Some(shown)) => BriefRun::of(&r, shown),
                _ => BriefRun {
                    running: true,
                    run: Some(run.clone()),
                    kind: Some(runs::Kind::Brief),
                    business: Some(s.business.clone()),
                    started: Some(now_secs()),
                    ..BriefRun::default()
                },
            };
            let brief = h.state::<Brief>();
            let mut b = brief.0.lock().map_err(|e| anyhow::anyhow!("{e}"))?;
            // A subscribe event for this run may have landed first with a later state.
            if b.run.as_deref() == Some(run.as_str()) && !b.running {
                return Ok(b.clone());
            }
            *b = snap.clone();
            return Ok(snap);
        }
        own_brief(&h, &s)
    })
    .await?;
    let _ = handle.emit("brief-changed", &snapshot);
    Ok(snapshot)
}

/// The daemon is down: `cos brief` as a child this process owns, kept in
/// `AppRuns` and followed by a thread to its deadline.
fn own_brief<R: Runtime>(handle: &AppHandle<R>, s: &Current) -> anyhow::Result<BriefRun> {
    let cos = s.cos.clone().ok_or_else(|| {
        anyhow::anyhow!("no cos binary: set COS_BIN, build it (`cargo build`), or `task install`")
    })?;
    let owned = handle.state::<AppRuns>();
    let mut owned = owned.0.lock().map_err(|e| anyhow::anyhow!("{e}"))?;
    let brief = handle.state::<Brief>();
    let mut b = brief.0.lock().map_err(|e| anyhow::anyhow!("{e}"))?;
    if !owned.is_empty() {
        anyhow::bail!("a brief is already running");
    }
    let child = pipeline::spawn_run(
        &cos,
        &pipeline::RunSpec {
            kind: runs::Kind::Brief,
            business: &s.business,
            home: &s.home,
            workspace: &s.workspace,
            projects: &s.projects,
            question: None,
            owner: runs::Owner::App,
            env: Vec::new(),
            client_req: None,
            budget: None,
        },
    )?;
    let run = child.id().to_owned();
    let child = Arc::new(Mutex::new(child));
    owned.insert(run.clone(), child.clone());
    *b = BriefRun {
        running: true,
        run: Some(run.clone()),
        kind: Some(runs::Kind::Brief),
        state: Some(runs::State::Queued),
        business: Some(s.business.clone()),
        started: Some(now_secs()),
        ..BriefRun::default()
    };
    let snapshot = b.clone();
    let h = handle.clone();
    let home = s.home.clone();
    std::thread::spawn(move || follow_owned(h, run, child, home));
    Ok(snapshot)
}

/// Owns one run child until it is reaped: its exit, a stop through
/// `AppRuns` (`run_stop`, which reaps it and leaves the record final), or
/// `pipeline::BRIEF_CHILD`.
fn follow_owned<R: Runtime>(
    handle: AppHandle<R>,
    run: String,
    child: Arc<Mutex<RunChild>>,
    home: PathBuf,
) {
    let deadline = pipeline::BRIEF_CHILD;
    let until = cos::proc::Deadline::capped(deadline, deadline);
    let ended = loop {
        std::thread::sleep(Duration::from_millis(100));
        let mut c = child.lock().unwrap_or_else(|e| e.into_inner());
        if c.is_reaped() {
            break Ok(None);
        }
        match c.try_end() {
            Ok(Some(e)) => break Ok(Some(e)),
            Ok(None) => {}
            Err(e) => break Err(e),
        }
        if until.expired() {
            break c.stop_as(runs::State::Failed, "deadline").and_then(|_| {
                Err(anyhow::anyhow!(
                    "cos brief did not finish in {}s",
                    deadline.as_secs()
                ))
            });
        }
    };
    if let Ok(mut owned) = handle.state::<AppRuns>().0.lock() {
        owned.remove(&run);
    }
    let state = handle.state::<Brief>();
    let Ok(mut b) = state.0.lock() else {
        return;
    };
    if b.run.as_deref() != Some(run.as_str()) {
        return;
    }
    b.running = false;
    b.ended = Some(now_secs());
    match ended {
        Ok(Some(e)) => {
            let ok = e.status.success();
            b.ok = Some(ok);
            b.state = Some(e.state);
            b.deferred = e.status.code() == Some(75);
            b.said = (!ok).then_some(e.said);
        }
        Ok(None) => {
            let record = runs::read(&home, &run).ok().flatten();
            let state = record.as_ref().map(|r| r.state);
            b.ok = Some(state == Some(runs::State::Done));
            b.state = state;
            b.said = record
                .and_then(|r| r.reason)
                .or_else(|| Some("stopped".to_owned()));
        }
        Err(e) => {
            b.ok = Some(false);
            b.said = Some(format!("{e:#}"));
        }
    }
    let snapshot = b.clone();
    drop(b);
    let _ = handle.emit("brief-changed", &snapshot);
    let _ = handle.emit("report-changed", json!({ "run": run }));
}

/// Stops and reaps every run this window owns.
fn stop_owned_runs<R: Runtime>(app: &AppHandle<R>) {
    let Some(owned) = app.try_state::<AppRuns>() else {
        return;
    };
    let all: Vec<_> = owned
        .0
        .lock()
        .map(|mut m| std::mem::take(&mut *m).into_values().collect())
        .unwrap_or_default();
    for child in all {
        let mut c = child.lock().unwrap_or_else(|e| e.into_inner());
        if let Err(e) = c.kill() {
            eprintln!("cos: run {}: {e:#}", c.id());
        }
    }
}

/// Follows every run under every registered home through the daemon's
/// `subscribe` stream, one connection at a time, reconnecting every 2 s while
/// it is down or after it hangs up. Each event for the window's business
/// becomes `brief-changed` with a `BriefRun` read from its record, and a
/// final state also `report-changed`. Absent the daemon, `watch` and the
/// window's own runs say the same from the files.
fn follow_runs<R: Runtime>(handle: AppHandle<R>) {
    std::thread::spawn(move || {
        let dir = handle.state::<Start>().daemon_dir.clone();
        loop {
            let _ = subscribe(&handle, &dir);
            std::thread::sleep(Duration::from_secs(2));
        }
    });
}

fn subscribe<R: Runtime>(handle: &AppHandle<R>, dir: &Path) -> anyhow::Result<()> {
    use std::io::{BufRead, Read, Write};
    let sock = cos::daemon::socket_path(dir);
    cos::daemon::check_socket_path(&sock)?;
    let mut s = std::os::unix::net::UnixStream::connect(&sock)?;
    s.set_write_timeout(Some(cos::daemon::IDLE))?;
    s.write_all(b"{\"op\":\"subscribe\"}\n")?;
    // The stream carries changes only: a brief already running is read once here.
    let cur = handle.state::<App>().get();
    if let (Some(business), Ok(Some(run))) = (&cur.registered, runs::brief_held(&cur.home)) {
        on_run_event_line(handle, &json!({ "run": run, "business": business }));
    }
    recheck_brief(handle);
    let mut reader = std::io::BufReader::new(s);
    let mut line = Vec::new();
    loop {
        line.clear();
        let n = (&mut reader)
            .take(cos::daemon::LINE_MAX as u64 + 1)
            .read_until(b'\n', &mut line)?;
        if n == 0 || line.last() != Some(&b'\n') {
            return Ok(());
        }
        if let Ok(event) = serde_json::from_slice::<Value>(&line) {
            on_run_event_line(handle, &event);
        }
    }
}

fn on_run_event_line<R: Runtime>(handle: &AppHandle<R>, event: &Value) {
    let (Some(id), Some(business)) = (event["run"].as_str(), event["business"].as_str()) else {
        return;
    };
    let s = handle.state::<App>().get();
    if !runs::is_id(id) || s.registered.as_deref() != Some(business) {
        return;
    }
    let mut last = None;
    let Some(shown) = runs::shown_from(
        || {
            last = runs::read(&s.home, id).ok().flatten();
            last.clone()
        },
        runs::process_gone,
    ) else {
        return;
    };
    let Some(record) = last.filter(|r| r.business == business) else {
        return;
    };
    let snapshot = BriefRun::of(&record, shown);
    if record.kind == runs::Kind::Brief
        && let Ok(mut b) = handle.state::<Brief>().0.lock()
    {
        *b = snapshot.clone();
    }
    let _ = handle.emit("brief-changed", &snapshot);
    if shown.is_final() {
        let _ = handle.emit("report-changed", json!({ "run": id }));
    }
}

/// Reads the shown brief's record again when it shows running and no thread
/// here owns it: the daemon's stream starts from a baseline, so a final state
/// written while no stream was open is never streamed.
fn recheck_brief<R: Runtime>(handle: &AppHandle<R>) {
    let brief = handle.state::<Brief>();
    let (run, business) = {
        let Ok(b) = brief.0.lock() else {
            return;
        };
        match (&b.run, &b.business) {
            (Some(run), Some(business)) if b.running => (run.clone(), business.clone()),
            _ => return,
        }
    };
    if handle
        .state::<AppRuns>()
        .0
        .lock()
        .is_ok_and(|m| m.contains_key(&run))
    {
        return;
    }
    let home = handle.state::<App>().get().home;
    if runs::shown(&home, &run).is_ok_and(|s| s.is_some_and(|s| s.is_final())) {
        on_run_event_line(handle, &json!({ "run": run, "business": business }));
    }
}

/// When the shown brief is another business's, as after a switch, the brief
/// already running for the window's business: the stream sends changes only,
/// and dropped this business's while the window showed another.
fn seed_brief<R: Runtime>(handle: &AppHandle<R>) {
    let cur = handle.state::<App>().get();
    let Some(business) = &cur.registered else {
        return;
    };
    let shown = handle
        .state::<Brief>()
        .0
        .lock()
        .ok()
        .and_then(|b| b.business.clone());
    if shown.as_deref() == Some(business.as_str()) {
        return;
    }
    if let Ok(Some(run)) = runs::brief_held(&cur.home) {
        on_run_event_line(handle, &json!({ "run": run, "business": business }));
    }
}

/// The window business's latest brief: running, or how it ended.
#[tauri::command]
fn brief_status<R: Runtime>(handle: AppHandle<R>) -> Res<BriefRun> {
    seed_brief(&handle);
    recheck_brief(&handle);
    Ok(handle.state::<Brief>().0.lock().map_err(err)?.clone())
}

/// A run as the run panel shows it: what MCP `run_status` returns, plus who
/// started it, when, and whether this window can stop it.
#[derive(Serialize)]
struct RunPanel {
    #[serde(flatten)]
    view: cos::bridge::RunView,
    owner: runs::Owner,
    started: i64,
    stoppable: bool,
}

fn owner_word(o: runs::Owner) -> String {
    serde_json::to_value(o)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// One run of the window's business, read from its record and, once done,
/// its brief or answer, as `run_status` with `wait_secs: 0` reads it. Writes
/// nothing.
#[tauri::command]
async fn run_get<R: Runtime>(handle: AppHandle<R>, business: String, run: String) -> Res<RunPanel> {
    let s = handle.state::<App>().get();
    if !runs::is_id(&run) {
        return Err(format!("run {run:?} is not a run id"));
    }
    named(&s, &business)?;
    let owned = handle
        .state::<AppRuns>()
        .0
        .lock()
        .is_ok_and(|m| m.contains_key(&run));
    blocking(move || {
        let (r, state) = cos::bridge::observe(&s.home, &run)?
            .ok_or_else(|| anyhow::anyhow!("no run {run} under business {}", s.business))?;
        let stoppable = !state.is_final()
            && match r.owner {
                runs::Owner::Daemon => s.registered.is_some(),
                runs::Owner::App => owned,
                runs::Owner::Mcp | runs::Owner::Itself => false,
            };
        Ok(RunPanel {
            view: cos::bridge::view(&s.home, &r, state, true).linked(s.registered.is_some()),
            owner: r.owner,
            started: r.started,
            stoppable,
        })
    })
    .await
}

/// Stops a live run of the window's business: through the daemon when it
/// owns the run, or this window's own `RunChild`. A run started anywhere else
/// is refused untouched; it is stopped where it was started.
#[tauri::command]
async fn run_stop<R: Runtime>(handle: AppHandle<R>, business: String, run: String) -> Res<Value> {
    let s = handle.state::<App>().get();
    if !runs::is_id(&run) {
        return Err(format!("run {run:?} is not a run id"));
    }
    named(&s, &business)?;
    let dir = handle.state::<Start>().daemon_dir.clone();
    let h = handle.clone();
    let state = blocking(move || {
        let r = runs::read(&s.home, &run)?
            .ok_or_else(|| anyhow::anyhow!("no run {run} under business {}", s.business))?;
        if r.state.is_final() {
            return Ok(r.state);
        }
        let refuse = || anyhow::anyhow!("not stoppable here: started by {}", owner_word(r.owner));
        match r.owner {
            runs::Owner::Daemon => {
                let name = s.registered.as_deref().ok_or_else(|| {
                    anyhow::anyhow!("the daemon stops runs of registered businesses only")
                })?;
                client::stop(&dir, name, &run)
            }
            runs::Owner::App => {
                let child = h
                    .state::<AppRuns>()
                    .0
                    .lock()
                    .map_err(|e| anyhow::anyhow!("{e}"))?
                    .get(&run)
                    .cloned()
                    .ok_or_else(refuse)?;
                let mut c = child.lock().unwrap_or_else(|e| e.into_inner());
                if c.is_reaped() {
                    return Ok(runs::read(&s.home, &run)?.map_or(r.state, |r| r.state));
                }
                let state = c.kill()?.state;
                drop(c);
                h.state::<AppRuns>()
                    .0
                    .lock()
                    .map_err(|e| anyhow::anyhow!("{e}"))?
                    .remove(&run);
                Ok(state)
            }
            runs::Owner::Mcp | runs::Owner::Itself => Err(refuse()),
        }
    })
    .await?;
    Ok(json!({ "state": state }))
}

/// The main checkout named `repo` under the app business's projects
/// directory, if it keeps a task store: the only repos a ritual command acts in.
fn flow_repo(projects: &Path, repo: &str) -> anyhow::Result<PathBuf> {
    cos::flows::repos(projects)?
        .into_iter()
        .find(|p| p.file_name().is_some_and(|n| n == repo))
        .ok_or_else(|| {
            anyhow::anyhow!(
                "no repo named {repo} with a task store under {}",
                projects.display()
            )
        })
}

/// The Flows tab's approval cards: each flow waiting at the gate. Reads
/// stores only when the profile approves the `tasks` source, like `/flows`.
#[tauri::command]
async fn ritual_gates(app: tauri::State<'_, App>) -> Res<Value> {
    let s = app.get();
    blocking(move || {
        if !cos::context::local_reads(&s.workspace).tasks {
            return Ok(json!({
                "gates": [],
                "gaps": ["the profile does not approve a tasks source, so no task store was read"],
                "steps": cos::ritual::STRIP,
                "flows": [],
            }));
        }
        let tz = cos::digest::zone(&s.workspace);
        let (gates, gaps, flows) = cos::ritual::board(
            &s.projects,
            &cos::flows::cn_program(),
            cos::flows::SCAN_BUDGET,
            Some(cos::flows::Hires {
                home: &s.home,
                tz: &tz,
            }),
        );
        let gates: Vec<Value> = gates
            .iter()
            .map(|g| {
                let mut v = serde_json::to_value(g).unwrap_or_default();
                v["clean"] = json!(g.clean());
                v["by"] = json!(
                    flow_repo(&s.projects, &g.repo)
                        .ok()
                        .and_then(|r| cos::ritual::approver(&s.workspace, &r))
                );
                v
            })
            .collect();
        Ok(json!({ "gates": gates, "gaps": gaps, "steps": cos::ritual::STRIP, "flows": flows }))
    })
    .await
}

/// The card's Approve: the approval `cos ritual approve` records, after the
/// inline confirm in the window. `by` is the approver `ritual_gates` showed.
#[tauri::command(rename_all = "snake_case")]
async fn ritual_approve(
    app: tauri::State<'_, App>,
    business: String,
    repo: String,
    epic: String,
    design: String,
    review: Option<String>,
    without_review: bool,
) -> Res<Value> {
    let s = app.get();
    named(&s, &business)?;
    blocking(move || {
        let dir = flow_repo(&s.projects, &repo)?;
        let by = cos::ritual::approver(&s.workspace, &dir).ok_or_else(|| {
            anyhow::anyhow!(
                "no approver name: set \"owner\" in the workspace profile, or git user.name in {repo}"
            )
        })?;
        let (line, check) = cos::ritual::approve_in_app(
            &cos::flows::cn_program(),
            &dir,
            &epic,
            &design,
            &by,
            review.as_deref(),
            without_review,
        )?;
        Ok(json!({ "line": line, "approved": check.ok(), "check": check.describe() }))
    })
    .await
}

/// Opens a file the card links to (the brief, the design, a review) in its
/// default app. Only paths the waiting flow's card names.
#[tauri::command]
async fn ritual_open(
    app: tauri::State<'_, App>,
    repo: String,
    epic: String,
    path: String,
) -> Res<()> {
    let s = app.get();
    blocking(move || {
        let dir = flow_repo(&s.projects, &repo)?;
        let tz = cos::digest::zone(&s.workspace);
        let (gates, _) = cos::ritual::gates(
            &s.projects,
            &cos::flows::cn_program(),
            cos::flows::SCAN_BUDGET,
            Some(cos::flows::Hires {
                home: &s.home,
                tz: &tz,
            }),
        );
        let g = gates
            .iter()
            .find(|g| g.repo == repo && g.epic == epic)
            .ok_or_else(|| anyhow::anyhow!("{epic} is not waiting for approval in {repo}"))?;
        let named = g.brief.as_deref() == Some(path.as_str())
            || g.design == path
            || g.review.iter().chain(&g.history).any(|r| r.path == path);
        anyhow::ensure!(named, "{path} is not on {epic}'s card");
        let path = cos::flows::openable_path(&path)?.to_owned();
        let tasks =
            cos::flows::read_tasks(&cos::flows::cn_program(), &dir, Duration::from_secs(20))?;
        open_located(&s.home, &dir, &tasks, &epic, &path)
    })
    .await
}

/// Locates `path` for `epic` (its branch's worktree, the repo, or the
/// exported committed copy) and opens it, only when the canonical file sits
/// inside the directory it was found under and is still Markdown or HTML.
fn open_located(
    home: &Path,
    dir: &Path,
    tasks: &[cos::flows::Task],
    epic: &str,
    path: &str,
) -> anyhow::Result<()> {
    let branch = tasks
        .iter()
        .find(|t| t.id == epic)
        .map(|t| cos::flows::Markers::parse(t.description.as_deref().unwrap_or("")))
        .and_then(|m| m.get("branch").map(str::to_owned));
    let export = home.join("out/ritual").join(epic);
    let at = cos::ritual::locate(dir, branch.as_deref(), path, &export)?;
    let file = cos::flows::contained(&at.root, &at.file)?;
    cos::report::open(&file)
}

/// The Flows page's Hire: one bead read again from its repo's task store
/// (`flows::bead_to_hire`), as the Hire dialog's work. The page names the
/// bead; the prompt, the repo and the refusals come from here.
#[tauri::command]
async fn flow_bead(
    app: tauri::State<'_, App>,
    repo: String,
    epic: String,
    bead: String,
) -> Res<Value> {
    let s = app.get();
    blocking(move || {
        let dir = flow_repo(&s.projects, &repo)?;
        let tasks =
            cos::flows::read_tasks(&cos::flows::cn_program(), &dir, Duration::from_secs(20))?;
        let b = cos::flows::bead_to_hire(&repo, &tasks, &epic, &bead)?;
        Ok(json!({
            "id": b.id,
            "title": format!("{} {}", b.id, b.title),
            "prompt": b.handoff,
            "repo": dir,
            "owner": b.owner,
            "criteria": b.criteria.len() + b.criteria_more,
            "source": format!("bead:{repo}/{}", b.id),
        }))
    })
    .await
}

/// The Flows page's Open: a file or link one of `epic`'s `ritual-*` lines
/// names (`flows::OPEN_KEYS`), never a path or URL from the page. Checked by
/// `flows::open_target` and `flows::contained` before `open` sees it.
#[tauri::command]
async fn flow_open(app: tauri::State<'_, App>, repo: String, epic: String, key: String) -> Res<()> {
    let s = app.get();
    blocking(move || {
        anyhow::ensure!(
            cos::flows::OPEN_KEYS.contains(&key.as_str()),
            "{key} is not a file a flow names"
        );
        let dir = flow_repo(&s.projects, &repo)?;
        let tasks =
            cos::flows::read_tasks(&cos::flows::cn_program(), &dir, Duration::from_secs(20))?;
        match cos::flows::open_target(&tasks, &epic, &key)? {
            cos::flows::Target::Url(url) => cos::report::open(Path::new(&url)),
            cos::flows::Target::Path(path) => open_located(&s.home, &dir, &tasks, &epic, &path),
        }
    })
    .await
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

/// What the watcher saw of one business at its last recompute.
struct Watched {
    stamp: String,
    starting: usize,
    launchers: Vec<i32>,
}

fn modified(path: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// Emits `report-changed` when a run or the workspace writes, and
/// `agents-changed` when an agent does, so the window refreshes on events
/// instead of every view polling. Reads metadata only: `agents::list` may
/// reconcile and write, too dear per tick. An agent's state write renames into
/// its directory, which moves the directory's mtime that `stamp` reads.
///
/// Publishes a business's row when its stamp changed, a process preparing a hire
/// it last saw died, or it has a Starting run, which turns Lost with no file change
/// or pid to watch. A changed registry publishes every row; a changed `dashboards.toml` emits
/// `dashboards-changed`. The first tick only records those two files' mtimes.
fn watch<R: Runtime>(handle: AppHandle<R>) {
    std::thread::spawn(move || {
        let start = handle.state::<Start>().inner().clone();
        let plist = start.user_home.as_ref().map(|h| h.join(BRIEF_PLIST));
        let dashboards =
            cos::dashboards::location(&start.registry, start.dashboards_file.as_deref());
        let (mut report, mut fleet) = (String::new(), String::new());
        let mut registry_mtime = modified(&start.registry);
        let mut dashboards_mtime = modified(&dashboards);
        let mut homes = business::registered_homes(&start.registry);
        let mut watched: BTreeMap<String, Watched> = BTreeMap::new();
        loop {
            let now = modified(&start.registry);
            let registry_changed = now != registry_mtime;
            if registry_changed {
                registry_mtime = now;
                homes = business::registered_homes(&start.registry);
            }
            let plist_mtime = plist.as_deref().and_then(modified);
            watched.retain(|name, _| homes.contains_key(name));
            let mut recompute = Vec::new();
            for (name, home) in &homes {
                let stamp = format!("{}-{plist_mtime:?}", cos::signals::stamp(home));
                let due = registry_changed
                    || watched.get(name).is_none_or(|w| {
                        w.stamp != stamp
                            || w.starting > 0
                            || w.launchers.iter().any(|&pid| !cos::proc::alive(pid))
                    });
                if due {
                    // Read before the row is built: a launcher that dies after this
                    // read is still in the set, so the next tick sees it die.
                    let signals = cos::signals::read(home);
                    watched.insert(
                        name.clone(),
                        Watched {
                            stamp,
                            starting: signals.starting,
                            launchers: signals.launchers,
                        },
                    );
                    recompute.push(name.clone());
                }
            }
            if registry_changed {
                publish(&handle, Rebuild::All);
            } else if !recompute.is_empty() {
                publish(&handle, Rebuild::Named(&recompute));
            }
            let now = modified(&dashboards);
            if now != dashboards_mtime {
                let _ = handle.emit("dashboards-changed", ());
                dashboards_mtime = now;
            }

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

/// Follows the hires in Herdr while the window is open, for every registered
/// business and the window's own: polls each live terminal (`agents::tick`:
/// one bounded `hc get` per run), stops runs at their deadline and judges
/// each once. Polling rather than `hc events`: every read is a bounded child
/// and nothing long-lived has to be owned, reconnected or reaped. A home with
/// no run in Herdr is read from its files alone and left as it is. Its writes
/// reach the window through `watch`; a change in which of the window's runs
/// wait on you in Herdr rebuilds the report, whose Today lists them.
///
/// While the daemon answers `hello` it follows every registered home's hires,
/// so this follows only the window's home and only when it is unregistered;
/// `hello` is asked at most once per `client::HELLO_CACHE`.
fn follow_herdr<R: Runtime>(handle: AppHandle<R>) {
    std::thread::spawn(move || {
        let start = handle.state::<Start>().inner().clone();
        let mut waiting = String::new();
        let mut follow = agents::Follow::default();
        let mut hello: Option<(Instant, bool)> = None;
        loop {
            let up = match hello {
                Some((at, up)) if at.elapsed() < client::HELLO_CACHE => up,
                _ => {
                    let up = matches!(
                        client::probe(&start.daemon_dir),
                        Ok(client::Probe::Up { .. })
                    );
                    hello = Some((Instant::now(), up));
                    up
                }
            };
            let current = handle.state::<App>().get().home;
            if !up {
                follow.follow_all(&start.registry, Some(&current));
            } else if business::registered_homes(&start.registry)
                .values()
                .any(|h| *h == current)
            {
                std::thread::sleep(Duration::from_secs(2));
                continue;
            } else {
                follow.follow_homes(std::slice::from_ref(&current));
            }
            let now = follow
                .blocked(&current)
                .map(|b| b.join(","))
                .unwrap_or_default();
            if now != waiting {
                let _ = handle.emit("report-changed", ());
                waiting = now;
            }
            std::thread::sleep(Duration::from_secs(2));
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

/// `cos://` links. Only the real app adds the plugins: single-instance ends a
/// second process of the same app, which on the test harness is the test.
/// Single-instance goes first, so a link that starts a second process is
/// handed to this one; a cold-start link is read once setup has run.
pub fn with_links<R: Runtime>(builder: Builder<R>) -> Builder<R> {
    with_deep_links(
        builder.plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            for url in argv.iter().filter(|a| a.starts_with("cos://")) {
                open_link(app, url);
            }
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.set_focus();
            }
        })),
    )
}

/// The deep-link plugin and the setup that routes its URLs into `open_link`;
/// safe under the test harness, unlike single-instance.
pub fn with_deep_links<R: Runtime>(builder: Builder<R>) -> Builder<R> {
    use tauri_plugin_deep_link::DeepLinkExt;
    builder.plugin(tauri_plugin_deep_link::init()).plugin(
        tauri::plugin::Builder::<R>::new("cos-links")
            .setup(|app, _| {
                let handle = app.clone();
                app.deep_link().on_open_url(move |event| {
                    for url in event.urls() {
                        open_link(&handle, url.as_str());
                    }
                });
                if let Ok(Some(urls)) = app.deep_link().get_current() {
                    for url in urls {
                        open_link(app, url.as_str());
                    }
                }
                Ok(())
            })
            .build(),
    )
}

/// The supervisor's `emit`: `dashboard-exited` for an owned server only. A server that
/// exits while starting is reported by its `ensure` instead.
pub fn emit_exited<R: Runtime>(app: &AppHandle<R>, notice: &servers::ExitNotice) {
    if notice.owned {
        let _ = app.emit("dashboard-exited", notice);
    }
}

/// The callback `.run()` takes: quit stops every server and every run this instance
/// started. Both events reach it on ⌘Q; the second stop is a no-op.
pub fn on_run_event(app: &tauri::AppHandle, event: tauri::RunEvent) {
    if let tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit = event {
        if let Some(supervisor) = app.try_state::<servers::Supervisor>() {
            supervisor.stop_owned();
        }
        stop_owned_runs(app);
    }
}

/// The app on any runtime: the report scheme, the state (given, or read from the
/// environment and the config directory at setup), the watcher, the commands.
pub fn builder<R: Runtime>(builder: Builder<R>, start: Option<Start>) -> Builder<R> {
    builder
        .manage(Brief::default())
        .manage(AppRuns::default())
        .manage(links::Links::default())
        .manage(FlowPages(Default::default()))
        .register_asynchronous_uri_scheme_protocol("cosreport", |ctx, req, responder| {
            let app = ctx.app_handle().clone();
            std::thread::spawn(move || responder.respond(cosreport_response(&app, &req)));
        })
        .setup(move |app| {
            let start = match start {
                Some(s) => s,
                None => Start::from_env(app.handle()).map_err(chain)?,
            };
            let handle = app.handle().clone();
            let supervisor = servers::Supervisor::new(
                &start.data_dir,
                &start.log_dir,
                move |notice| emit_exited(&handle, &notice),
                servers::real_spawn,
                servers::real_connect,
            )?;
            let file = cos::dashboards::location(&start.registry, start.dashboards_file.as_deref());
            let dashboards = cos::dashboards::load(&file).unwrap_or_else(|e| {
                eprintln!("dashboards: {e:#}; launch cleanup declares no servers");
                cos::dashboards::Dashboards::default()
            });
            supervisor.launch_cleanup(&dashboards);
            app.manage(supervisor);
            app.manage(App::new(&start));
            app.manage(start);
            app.manage(RowCache::default());
            // The main window is built here rather than from the config alone
            // (`"create": false`), so its links can be routed: a frame's
            // `target="_blank"` link or a navigation away from the app opens
            // in the default browser, never inside the window.
            let windows: Vec<_> = app
                .config()
                .app
                .windows
                .iter()
                .filter(|w| !w.create)
                .cloned()
                .collect();
            for w in windows {
                tauri::WebviewWindowBuilder::from_config(app.handle(), &w)?
                    .on_navigation(|url| match link_kind(url) {
                        Link::InApp => true,
                        _ => {
                            send_out(url, open_in_browser);
                            false
                        }
                    })
                    .on_new_window(|url, _| {
                        send_out(&url, open_in_browser);
                        tauri::webview::NewWindowResponse::Deny
                    })
                    .build()?;
            }
            watch(app.handle().clone());
            follow_herdr(app.handle().clone());
            follow_runs(app.handle().clone());
            // Off the window's path, so the Flows tab's first open shows a page at once.
            flows_behind(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            paths,
            switch_business,
            business_make_default,
            businesses,
            brief_seen,
            brief_run,
            brief_status,
            run_get,
            run_stop,
            links::link_ready,
            ritual_gates,
            ritual_approve,
            ritual_open,
            flow_open,
            flow_bead,
            dashboards,
            dashboard_open,
            tax_read,
            tax_save,
            agents_list,
            agents_snapshot,
            agents_fit,
            agents_spawn,
            agents_record,
            agents_stop,
            agents_clean,
            agents_seed,
            agents_say,
            agents_pause,
            agents_done,
            agents_open,
            agents_timeline,
            agents_screen,
            herdr_status,
            herdr_start,
            agent_tasks,
            agents_roles,
            skills_catalog,
            skills_set,
            agents_scorecard,
            agents_rejudge,
            deliverables_open,
            brief_summary,
            repos,
            docs_save,
            assistants::connect_status,
            assistants::connect_apply,
        ])
}
