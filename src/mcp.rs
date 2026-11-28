//! `cos` as an MCP server, so Claude Desktop, Codex and claude.ai can call it.
//!
//! Two modes. `Local` (stdio) offers every tool; each call acts on the business
//! it names, the server's pin, or the registry's default read at that call.
//! `Remote` (HTTP behind a secret path) offers only the free read tools on one
//! business fixed at start: a leaked URL can read the last brief, never spend
//! money, trigger a run or name another business.

use std::convert::Infallible;
use std::path::Path;
use std::sync::Arc;

use anyhow::{Result, bail, ensure};
use rmcp::handler::server::tool::schema_for_output;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::*;
use rmcp::service::RequestContext;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use rmcp::{ErrorData, RoleServer, ServerHandler, ServiceExt, tool, tool_router};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::agents;
use crate::bridge::{self, RunList, RunView, Runner};
use crate::business::{self, Business, Inputs, Layer, Triple};
use crate::pipeline::Config;
use crate::reports::Brief;
use crate::runs;
use crate::store::{self, Kind};

const DASHBOARD_URI: &str = "ui://cos/dashboard";
const DASHBOARD_HTML: &str = include_str!("dashboard.html");
const APP_MIME: &str = "text/html;profile=mcp-app";

/// Tools a remote client may call. Everything else is local-only.
const REMOTE_TOOLS: &[&str] = &["latest_brief"];

/// Arguments a remote call may carry. None: a remote tool that gains one lists it here.
const REMOTE_ARGS: &[&str] = &[];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Local,
    Remote,
}

/// How a local server picks the business a call acts on when the call names none.
#[derive(Clone)]
pub enum Pick {
    /// Flags or env named a business or directories when the server started: every call acts there.
    Pinned {
        triple: Triple,
        source: &'static str,
    },
    /// Nothing pinned the server: a call reads the registry's default at that moment.
    Registry { inputs: Inputs },
}

impl Pick {
    /// The pick for a server started with `inputs`, which resolved to `triple`.
    pub fn new(inputs: Inputs, triple: &Triple) -> Self {
        let source = match triple.layer {
            Layer::Flags if inputs.flags.business.is_some() => "--business",
            Layer::Flags => "--home, --workspace or --projects",
            Layer::Env if inputs.env.business.is_some() => "COS_BUSINESS",
            Layer::Env => "COS_HOME, COS_WORKSPACE or COS_PROJECTS",
            Layer::Registry | Layer::Defaults => return Self::Registry { inputs },
        };
        Self::Pinned {
            triple: triple.clone(),
            source,
        }
    }
}

#[derive(Clone)]
pub struct Cos {
    cfg: Config,
    mode: Mode,
    /// Set by `Cos::local`; `None` fixes every call to `cfg`.
    pick: Option<Pick>,
    /// The paid runs' starter; a remote server has none.
    runner: Option<Arc<Runner>>,
}

/// What every tool returns as structured content; the dashboard reads this.
#[derive(Serialize, schemars::JsonSchema)]
pub struct Report {
    /// "brief"; null when nothing is saved yet.
    kind: Option<String>,
    day: Option<String>,
    report: Option<Value>,
    markdown: Option<String>,
    /// Set only for a fresh run.
    cost_usd: Option<f64>,
}

/// The business a tool acts on.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct BusinessArg {
    /// A business name from `businesses`. Omit it for this server's pin, or the
    /// registry's default at the time of the call.
    business: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct AskArgs {
    /// The question for the chief of staff, 1 to 4000 characters.
    #[schemars(length(min = 1, max = 4000))]
    question: String,
    /// A business name from `businesses`; see `latest_brief`.
    business: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct RunStatusArgs {
    /// A run id from `brief`, `ask` or `runs`.
    run: String,
    /// The run's business; see `latest_brief`.
    business: Option<String>,
    /// Seconds to wait for the run to end, 0 to 50 (default 0: answer now).
    #[schemars(range(min = 0, max = 50))]
    wait_secs: Option<u64>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct RunsArgs {
    /// A business name from `businesses`; see `latest_brief`.
    business: Option<String>,
    /// How many runs, 1 to 20 (default 5).
    #[schemars(range(min = 1, max = 20))]
    limit: Option<u32>,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SpinOffArgs {
    /// "brief" (an action) or "goals" (a goal of the week), from the latest
    /// saved brief.
    from: agents::From,
    /// The item's number, as the report numbers it.
    item: usize,
    /// "claude" (default) or "codex".
    #[serde(default)]
    harness: Option<agents::Harness>,
    /// Shell commands cos runs in the agent's workdir when it ends; each must
    /// exit 0 for the verdict to be "accepted". At most 10, each 1 to 500 characters.
    #[serde(default)]
    #[schemars(length(max = 10), inner(length(min = 1, max = 500)))]
    checks: Vec<String>,
    /// Paths the agent may change, relative to the repo; empty allows any. At
    /// most 50, each 1 to 300 characters, never starting with "/" or containing "..".
    #[serde(default)]
    #[schemars(length(max = 50), inner(length(min = 1, max = 300)))]
    scope: Vec<String>,
    /// A staff role from the hiring policy (builder, reviewer, tester, ...).
    /// The hire is refused when the role cannot do the work from its sandbox.
    #[serde(default)]
    role: Option<String>,
    /// A business name from `businesses`; see `latest_brief`.
    business: Option<String>,
}

impl SpinOffArgs {
    /// The `hire.spawn` binding's bounds, checked before anything is hired.
    fn bound(&self) -> Result<()> {
        ensure!(
            (1..=50).contains(&self.item),
            "item {} is outside 1 to 50",
            self.item
        );
        ensure!(
            self.checks.len() <= 10,
            "checks: at most 10 entries, got {}",
            self.checks.len()
        );
        for check in &self.checks {
            ensure!(
                !check.is_empty() && check.chars().count() <= 500,
                "checks: each entry is 1 to 500 characters"
            );
        }
        ensure!(
            self.scope.len() <= 50,
            "scope: at most 50 paths, got {}",
            self.scope.len()
        );
        for path in &self.scope {
            ensure!(
                !path.is_empty() && path.chars().count() <= 300,
                "scope: each path is 1 to 300 characters"
            );
            ensure!(
                !path.starts_with('/') && !path.contains(".."),
                "scope: {path:?} must be relative and must not contain \"..\""
            );
        }
        if let Some(role) = &self.role {
            ensure!(
                role.len() <= 40
                    && role.starts_with(|c: char| c.is_ascii_lowercase())
                    && role
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
                "role {role:?} is not a role name (a-z, 0-9, '-'; at most 40 characters)"
            );
        }
        Ok(())
    }
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct DeliverableId {
    /// A deliverable id from `plan`, such as "D-1".
    deliverable: String,
    /// A business name from `businesses`; see `latest_brief`.
    business: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct AgentId {
    /// An agent id from `agents`.
    pub id: String,
    /// A business name from `businesses`; see `latest_brief`.
    pub business: Option<String>,
}

fn ui_meta() -> MetaObject {
    let Value::Object(map) = json!({ "ui": { "resourceUri": DASHBOARD_URI } }) else {
        unreachable!()
    };
    MetaObject::from(map)
}

fn structured(r: impl Serialize) -> CallToolResult {
    CallToolResult::structured(serde_json::to_value(r).unwrap_or(Value::Null))
}

fn failed(e: impl std::fmt::Display) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(format!("{e:#}"))])
}

/// The triple a server started with `cfg` acts on: its own directories, no business name.
fn fixed(cfg: &Config) -> Triple {
    Triple {
        home: cfg.home.clone(),
        workspace: cfg.workspace.clone(),
        projects: cfg.projects.clone(),
        business: None,
        layer: Layer::Flags,
    }
}

#[tool_router]
impl Cos {
    pub fn new(cfg: Config, mode: Mode) -> Self {
        let budget = cfg.budget;
        Self {
            cfg,
            mode,
            pick: None,
            runner: (mode == Mode::Local).then(|| Arc::new(Runner::with_budget(budget))),
        }
    }

    /// A local server whose calls pick their business as `pick` says.
    pub fn local(cfg: Config, pick: Pick) -> Self {
        let budget = cfg.budget;
        Self {
            cfg,
            mode: Mode::Local,
            pick: Some(pick),
            runner: Some(Arc::new(Runner::with_budget(budget))),
        }
    }

    /// The most recent saved daily brief: at most three actions that need you,
    /// what changed, this week against your goals (each with rock/sand/water
    /// confidence and a steer, proposed deliverables, decisions), then
    /// calendar, backlog and source coverage. Free and instant; does not call
    /// Claude.
    #[tool(meta = ui_meta(), output_schema = schema_for_output::<Report>(),
           annotations(read_only_hint = true, open_world_hint = false))]
    async fn latest_brief(&self, Parameters(args): Parameters<BusinessArg>) -> CallToolResult {
        match self.triple(args.business.as_deref()) {
            Ok(t) => latest::<Brief>(&t.home, Kind::Briefs, "brief"),
            Err(e) => failed(e),
        }
    }

    /// Start a fresh daily brief. Calls Claude: costs money, and the run takes
    /// a few minutes. Answers at once with the run id and, for a registered
    /// business, a cos:// link to it in the app; follow it with run_status.
    /// While a brief for that business is already running, returns that run
    /// with existing: true and spends nothing. A run this server starts itself
    /// (no cos daemon) ends when this client closes. Prefer latest_brief unless
    /// asked to refresh.
    #[tool(meta = ui_meta(), output_schema = schema_for_output::<RunView>(),
           annotations(read_only_hint = false, open_world_hint = true))]
    async fn brief(&self, Parameters(args): Parameters<BusinessArg>) -> CallToolResult {
        self.start_run(args.business, runs::Kind::Brief, None).await
    }

    /// Ask the chief of staff a question about goals, priorities or today's
    /// work; the tactician answers it on the same sources as a brief. Calls
    /// Claude: costs money, and the run takes a few minutes. Answers at once
    /// with the run id; run_status returns the answer once the run is done. A
    /// run this server starts itself (no cos daemon) ends when this client
    /// closes.
    #[tool(output_schema = schema_for_output::<RunView>(),
           annotations(read_only_hint = false, open_world_hint = true))]
    async fn ask(&self, Parameters(args): Parameters<AskArgs>) -> CallToolResult {
        let n = args.question.chars().count();
        if !(1..=bridge::QUESTION_MAX).contains(&n) {
            return failed(format!(
                "question is {n} characters: want 1 to {}",
                bridge::QUESTION_MAX
            ));
        }
        self.start_run(args.business, runs::Kind::Ask, Some(args.question))
            .await
    }

    /// A run started by brief or ask: its state, and once it is done the
    /// brief (report, markdown, day, cost_usd) or the answer. With wait_secs,
    /// waits up to that many seconds (at most 50) for the run to end. Reads
    /// run records only: free, and works whoever started the run.
    #[tool(output_schema = schema_for_output::<RunView>(),
           annotations(read_only_hint = true, open_world_hint = false))]
    async fn run_status(&self, Parameters(args): Parameters<RunStatusArgs>) -> CallToolResult {
        if !runs::is_id(&args.run) {
            return failed(format!("run {:?} is not a run id", args.run));
        }
        let wait = args.wait_secs.unwrap_or(0);
        if wait > bridge::WAIT_MAX {
            return failed(format!("wait_secs {wait}: want 0 to {}", bridge::WAIT_MAX));
        }
        let t = match self.triple(args.business.as_deref()) {
            Ok(t) => t,
            Err(e) => return failed(e),
        };
        let until = tokio::time::Instant::now() + std::time::Duration::from_secs(wait);
        loop {
            match bridge::observe(&t.home, &args.run) {
                Ok(Some((r, state)))
                    if state.is_final() || tokio::time::Instant::now() >= until =>
                {
                    return structured(
                        bridge::view(&t.home, &r, state, true).linked(t.business.is_some()),
                    );
                }
                Ok(Some(_)) => {}
                Ok(None) => {
                    return failed(format!("no run {} under business {}", args.run, t.label()));
                }
                Err(e) => return failed(e),
            }
            let next = tokio::time::Instant::now() + std::time::Duration::from_millis(500);
            tokio::time::sleep_until(next.min(until)).await;
        }
    }

    /// The business's latest runs of brief and ask, newest first, whoever
    /// started them. Free; local only.
    #[tool(output_schema = schema_for_output::<RunList>(),
           annotations(read_only_hint = true, open_world_hint = false))]
    async fn runs(&self, Parameters(args): Parameters<RunsArgs>) -> CallToolResult {
        let limit = args.limit.unwrap_or(bridge::LIMIT_DEFAULT);
        if !(1..=bridge::LIMIT_MAX).contains(&limit) {
            return failed(format!("limit {limit}: want 1 to {}", bridge::LIMIT_MAX));
        }
        match self.triple(args.business.as_deref()) {
            Ok(t) => structured(RunList {
                runs: bridge::list(&t.home, limit as usize)
                    .into_iter()
                    .map(|v| v.linked(t.business.is_some()))
                    .collect(),
            }),
            Err(e) => failed(e),
        }
    }

    /// The tracked plan from `.chief-of-staff/state.json`: goals, KPIs, open
    /// deliverables and commitments, source coverage, open incidents. Free;
    /// local only.
    #[tool(annotations(read_only_hint = true, open_world_hint = false))]
    async fn plan(&self, Parameters(args): Parameters<BusinessArg>) -> CallToolResult {
        let t = match self.triple(args.business.as_deref()) {
            Ok(t) => t,
            Err(e) => return failed(e),
        };
        match crate::state::read(&t.workspace) {
            Ok(Some(st)) => {
                let open =
                    |c: &str| -> Vec<Value> { crate::state::open(&st, c).cloned().collect() };
                CallToolResult::structured(json!({
                    "revision": st["revision"],
                    "goals": open("goals"),
                    "kpis": st["kpis"],
                    "deliverables": open("deliverables"),
                    "commitments": open("commitments"),
                    "sources": st["sources"],
                    "incidents": st["incidents"],
                }))
            }
            Ok(None) => failed(format!(
                "no .chief-of-staff workspace; run `cos --workspace {} state init` and set up goals",
                t.workspace.display()
            )),
            Err(e) => failed(e),
        }
    }

    /// "Delegate this": the handoff for a tracked deliverable (outcome, why
    /// now, owner, evidence, done when, permission boundary). Prepares text
    /// only; starts and sends nothing. Free; local only.
    #[tool(annotations(read_only_hint = true, open_world_hint = false))]
    async fn handoff(&self, Parameters(args): Parameters<DeliverableId>) -> CallToolResult {
        let t = match self.triple(args.business.as_deref()) {
            Ok(t) => t,
            Err(e) => return failed(e),
        };
        let st = match crate::state::read(&t.workspace) {
            Ok(Some(st)) => st,
            Ok(None) => return failed("no .chief-of-staff workspace"),
            Err(e) => return failed(e),
        };
        let profile = crate::state::profile(&t.workspace);
        match crate::handoff::deliverable(
            &st,
            profile.as_ref(),
            &args.deliverable,
            crate::handoff::Receiver::Chat,
        ) {
            Ok(text) => CallToolResult::success(vec![ContentBlock::text(text)]),
            Err(e) => failed(e),
        }
    }

    /// Hired coding agents: status, what their Herdr terminal says, verdict,
    /// workdir. Free; local only.
    #[tool(annotations(read_only_hint = true, open_world_hint = false))]
    async fn agents(&self, Parameters(args): Parameters<BusinessArg>) -> CallToolResult {
        let t = match self.triple(args.business.as_deref()) {
            Ok(t) => t,
            Err(e) => return failed(e),
        };
        match agents::list(&t.home).map(|listing| listing.runs) {
            Ok(list) => {
                let rows: Vec<Value> = list
                    .iter()
                    .map(|(spec, st)| {
                        json!({ "id": spec.id, "title": spec.title, "source": spec.source,
                        "harness": spec.harness, "workdir": spec.workdir, "state": st })
                    })
                    .collect();
                let scorecard = agents::ledger::scorecard(&agents::ledger::read(&t.home));
                CallToolResult::structured(json!({ "agents": rows, "scorecard": scorecard }))
            }
            Err(e) => failed(e),
        }
    }

    /// Hire a coding agent (Claude Code or Codex) on one item's agent prompt,
    /// in a detached worktree of the repo it names, run interactive in a
    /// Herdr terminal (`cos-<id>`). Returns once the run is recorded; the
    /// worktree is seeded and the terminal started in the background. The
    /// agent is sandboxed and asks you in Herdr before anything
    /// outward-facing. Refused when herdr-core is not running. Costs money.
    /// Local only.
    #[tool(annotations(read_only_hint = false, open_world_hint = true))]
    async fn spin_off(&self, Parameters(args): Parameters<SpinOffArgs>) -> CallToolResult {
        let triple = match self.triple(args.business.as_deref()) {
            Ok(t) => t,
            Err(e) => return failed(e),
        };
        let run = move || -> Result<(agents::Spec, String)> {
            let cos = std::env::current_exe()?;
            let spec = spin_off_hire(&triple, args, &cos)?;
            follow_while_serving(triple.home.clone());
            let say = say_command(&triple, &spec.id);
            Ok((spec, say))
        };
        match tokio::task::spawn_blocking(run).await {
            Ok(Ok((spec, say))) => CallToolResult::success(vec![ContentBlock::text(format!(
                "hired {} into Herdr as the terminal {} (in {}): talk to it there (`hc attach {}`), or with `{say}`; it is judged once when it says done or ends",
                spec.id,
                spec.terminal.as_deref().unwrap_or(""),
                spec.workdir.display(),
                spec.terminal.as_deref().unwrap_or(""),
            ))]),
            Ok(Err(e)) => failed(e),
            Err(e) => failed(e),
        }
    }

    /// Stop a running agent and everything it started. Local only.
    #[tool(annotations(
        read_only_hint = false,
        destructive_hint = true,
        open_world_hint = false
    ))]
    pub async fn stop_agent(&self, Parameters(args): Parameters<AgentId>) -> CallToolResult {
        let home = match self.triple(args.business.as_deref()) {
            Ok(t) => t.home,
            Err(e) => return failed(e),
        };
        match tokio::task::spawn_blocking(move || {
            agents::stop(&home, &args.id, std::time::Duration::from_secs(10))
        })
        .await
        {
            Ok(Ok(st)) => CallToolResult::success(vec![ContentBlock::text(st.status.to_string())]),
            Ok(Err(e)) => failed(e),
            Err(e) => failed(e),
        }
    }

    /// The businesses this server can act on: each with its home and whether it
    /// is the registry's default, and the business this server is pinned to, if
    /// any. Free; local only.
    #[tool(annotations(read_only_hint = true, open_world_hint = false))]
    async fn businesses(&self) -> CallToolResult {
        let registry = match business::location().and_then(|path| business::load(&path)) {
            Ok(registry) => registry,
            Err(e) => return failed(e),
        };
        let rows: Vec<Value> = registry
            .businesses
            .iter()
            .map(|(name, b)| {
                json!({
                    "name": name,
                    "home": b.home.display().to_string(),
                    "default": registry.active.as_deref() == Some(name.as_str()),
                })
            })
            .collect();
        let pinned = match &self.pick {
            Some(Pick::Pinned { triple, .. }) => triple.business.clone(),
            _ => None,
        };
        CallToolResult::structured(json!({ "businesses": rows, "pinned": pinned }))
    }
}

/// `spin_off`'s hire: planned like every other, refused without Herdr,
/// started in Herdr in the background, pinned to the business `triple` names.
pub fn spin_off_hire(
    triple: &Triple,
    args: SpinOffArgs,
    cos: &std::path::Path,
) -> Result<agents::Spec> {
    args.bound()?;
    let task = agents::tasks(&triple.home, args.from)?
        .into_iter()
        .find(|t| t.n == args.item)
        .ok_or_else(|| anyhow::anyhow!("item {} has no agent prompt", args.item))?;
    let plan = agents::hire::plan(
        &agents::hire::Where {
            home: &triple.home,
            workspace: &triple.workspace,
            projects: &triple.projects,
        },
        args.role.as_deref(),
        agents::hire::Ask::Task(task),
        None,
        &agents::Contract {
            checks: args.checks,
            scope: args.scope,
            ..Default::default()
        },
    )?;
    let req = plan.request(agents::hire::Run {
        harness: args.harness.unwrap_or(agents::Harness::Claude),
        program: None,
        model: None,
        worktree: true,
        deadline: std::time::Duration::from_secs(1800),
        slots: None,
        explicit: agents::policy::Explicit { deadline: false },
    })?;
    agents::spawn_background(triple, req, cos)
}

/// The CLI line that talks to hire `id` of `triple`'s business, naming the
/// business (or its directories), since the CLI's own default may be another.
pub fn say_command(triple: &Triple, id: &str) -> String {
    let which = match &triple.business {
        Some(name) => format!("--business {name}"),
        None => format!(
            "--home {} --workspace {} --projects {}",
            triple.home.display(),
            triple.workspace.display(),
            triple.projects.display()
        ),
    };
    format!("cos {which} agents say {id} \"…\"")
}

/// While this server lives, one thread per home follows the hires it started
/// there: their deadlines and their one verdict, unless the daemon answers
/// `hello` and `home` is registered, which the daemon then follows. Each
/// thread ends when its home has none live.
fn follow_while_serving(home: std::path::PathBuf) {
    static FOLLOWING: std::sync::Mutex<std::collections::BTreeSet<std::path::PathBuf>> =
        std::sync::Mutex::new(std::collections::BTreeSet::new());
    let key = home.canonicalize().unwrap_or_else(|_| home.clone());
    let following = || FOLLOWING.lock().unwrap_or_else(|e| e.into_inner());
    if !following().insert(key.clone()) {
        return;
    }
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(std::time::Duration::from_secs(3));
            let registered = business::location()
                .is_ok_and(|registry| bridge::registered_home(&registry, &home));
            if !(registered && crate::daemon::client::answers())
                && let Err(e) = agents::tick(&home)
            {
                eprintln!("cos: following hires: {e:#}");
            }
            let mut set = following();
            if agents::live_runs(&home) == 0 {
                set.remove(&key);
                break;
            }
        }
    });
}

fn latest<T: serde::de::DeserializeOwned + Serialize>(
    home: &Path,
    kind: Kind,
    name: &str,
) -> CallToolResult {
    match store::latest::<T>(home, kind) {
        Ok(Some(saved)) => structured(Report {
            kind: Some(name.into()),
            day: Some(saved.day),
            report: serde_json::to_value(&saved.value).ok(),
            markdown: Some(saved.markdown),
            cost_usd: None,
        }),
        Ok(None) => structured(Report {
            kind: None,
            day: None,
            report: None,
            markdown: None,
            cost_usd: None,
        }),
        Err(e) => failed(e),
    }
}

impl Cos {
    /// The triple a call acts on: the named business, the pin, or the registry's
    /// default read now. A server with no pick acts on its config only.
    fn triple(&self, business: Option<&str>) -> Result<Triple> {
        let Some(pick) = &self.pick else {
            ensure!(business.is_none(), "this server takes no business argument");
            return Ok(fixed(&self.cfg));
        };
        match (pick, business) {
            (Pick::Pinned { triple, .. }, None) => Ok(triple.clone()),
            (Pick::Pinned { triple, .. }, Some(name)) if triple.label() == name => {
                Ok(triple.clone())
            }
            (Pick::Pinned { triple, source }, Some(_)) => bail!(
                "pinned to {}: this server was started with {source}",
                triple.label()
            ),
            (Pick::Registry { inputs }, _) => {
                let mut inputs = inputs.clone();
                let registry = business::load(&business::location()?)?;
                if let Some(name) = business {
                    if !registry.businesses.contains_key(name) {
                        let known = registry.businesses.keys().cloned().collect::<Vec<_>>();
                        // A RunView names an unregistered default triple by its label.
                        let own = business::resolve(&inputs, || Ok(registry))?;
                        if own.business.is_none() && own.label() == name {
                            return Ok(own);
                        }
                        bail!(
                            "business {name:?} is not registered; known: {}",
                            known.join(", ")
                        );
                    }
                    inputs.flags.business = Some(name.to_owned());
                }
                let triple = business::resolve(&inputs, || Ok(registry))?;
                if let Some(name) = &triple.business {
                    business::check_dirs(
                        name,
                        &Business {
                            home: triple.home.clone(),
                            workspace: triple.workspace.clone(),
                            projects: triple.projects.clone(),
                        },
                    )?;
                }
                Ok(triple)
            }
        }
    }

    /// `brief` or `ask` on the business the call names: the run id at once.
    async fn start_run(
        &self,
        business: Option<String>,
        kind: runs::Kind,
        question: Option<String>,
    ) -> CallToolResult {
        let Some(runner) = self.runner.clone() else {
            return failed("this server starts no paid runs");
        };
        let t = match self.triple(business.as_deref()) {
            Ok(t) => t,
            Err(e) => return failed(e),
        };
        let start = move || -> Result<RunView> {
            let started = runner.start(&t, kind, question.as_deref())?;
            Ok(started
                .view(&t.home, t.label(), kind)
                .linked(t.business.is_some()))
        };
        match tokio::task::spawn_blocking(start).await {
            Ok(Ok(view)) => structured(view),
            Ok(Err(e)) => failed(e),
            Err(e) => failed(e),
        }
    }

    fn allowed(&self, tool: &str) -> bool {
        self.mode == Mode::Local || REMOTE_TOOLS.contains(&tool)
    }
}

/// A remote client sees no `business` argument, so the schema drops the property
/// local tools declare.
fn remote_schema(tool: &mut Tool) {
    let schema = Arc::make_mut(&mut tool.input_schema);
    let empty = match schema.get_mut("properties") {
        Some(Value::Object(props)) => {
            props.remove("business");
            props.is_empty()
        }
        _ => false,
    };
    if empty {
        schema.remove("properties");
    }
}

impl ServerHandler for Cos {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
        )
        .with_server_info(Implementation::new(
            "chief-of-staff",
            env!("CARGO_PKG_VERSION"),
        ))
        .with_instructions(
            "Chief of staff. latest_brief is free and instant: use it first. brief and ask \
                 call Claude and cost money; use them only when the user asks for a fresh run \
                 or a question the saved brief cannot answer. They answer at once with a run \
                 id and, for a registered business, a cos:// link; run_status with wait_secs \
                 returns the result once the run is done.",
        )
    }

    async fn list_tools(
        &self,
        _req: Option<PaginatedRequestParams>,
        _ctx: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let tools = Self::tool_router()
            .list_all()
            .into_iter()
            .filter(|t| self.allowed(&t.name))
            .map(|mut t| {
                if self.mode == Mode::Remote {
                    remote_schema(&mut t);
                }
                t
            })
            .collect();
        Ok(ListToolsResult::with_all_items(tools))
    }

    async fn call_tool(
        &self,
        req: CallToolRequestParams,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        if !self.allowed(&req.name) {
            return Err(ErrorData::invalid_params(
                format!("tool {} is local-only", req.name),
                None,
            ));
        }
        if self.mode == Mode::Remote
            && let Some(arg) = req
                .arguments
                .as_ref()
                .and_then(|args| args.keys().find(|k| !REMOTE_ARGS.contains(&k.as_str())))
        {
            return Err(ErrorData::invalid_params(
                format!("tool {} takes no argument {arg}", req.name),
                None,
            ));
        }
        let tcc = rmcp::handler::server::tool::ToolCallContext::new(self, req, ctx);
        Self::tool_router().call(tcc).await
    }

    async fn list_resources(
        &self,
        _req: Option<PaginatedRequestParams>,
        _ctx: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        Ok(ListResourcesResult::with_all_items(vec![
            Resource::new(DASHBOARD_URI, "dashboard")
                .with_title("Chief of staff dashboard")
                .with_mime_type(APP_MIME),
        ]))
    }

    async fn read_resource(
        &self,
        req: ReadResourceRequestParams,
        _ctx: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, ErrorData> {
        if req.uri != DASHBOARD_URI {
            return Err(ErrorData::invalid_params(
                format!("unknown resource {}", req.uri),
                None,
            ));
        }
        let html = crate::design::inline(DASHBOARD_HTML)
            .map_err(|e| ErrorData::internal_error(format!("{e:#}"), None))?;
        Ok(ReadResourceResult::new(vec![
            ResourceContents::text(html, req.uri).with_mime_type(APP_MIME),
        ])
        .into())
    }
}

/// Serves until the client closes stdin, then stops and reaps every run this
/// server started itself.
pub async fn stdio(cfg: Config, pick: Pick) -> Result<()> {
    let cos = Cos::local(cfg, pick);
    let runner = cos.runner.clone();
    let served = match cos.serve(rmcp::transport::stdio()).await {
        Ok(service) => service
            .waiting()
            .await
            .map(drop)
            .map_err(anyhow::Error::from),
        Err(e) => Err(e.into()),
    };
    if let Some(runner) = runner {
        tokio::task::spawn_blocking(move || runner.shutdown()).await?;
    }
    served
}

/// Streamable HTTP at `/mcp/<token>`, remote mode. Any other path is a 404.
/// `public_host` is the tunnel hostname, so its Host header is accepted.
pub async fn http(cfg: Config, addr: &str, token: &str, public_host: Option<String>) -> Result<()> {
    anyhow::ensure!(token.len() >= 32, "token must be at least 32 characters");
    let mut hosts = vec!["localhost".to_owned(), "127.0.0.1".to_owned()];
    hosts.extend(public_host);
    let svc = StreamableHttpService::new(
        move || Ok(Cos::new(cfg.clone(), Mode::Remote)),
        Arc::new(LocalSessionManager::default()),
        StreamableHttpServerConfig::default().with_allowed_hosts(hosts),
    );
    let secret: Arc<str> = format!("/mcp/{token}").into();
    let listener = tokio::net::TcpListener::bind(addr).await?;
    eprintln!("cos: serving read-only MCP on http://{addr}/mcp/<token>");
    loop {
        let (stream, _) = listener.accept().await?;
        let (svc, secret) = (svc.clone(), secret.clone());
        tokio::spawn(async move {
            let handler =
                hyper::service::service_fn(move |req: hyper::Request<hyper::body::Incoming>| {
                    let (svc, secret) = (svc.clone(), secret.clone());
                    async move {
                        if req.uri().path() != &*secret {
                            use http_body_util::{BodyExt, Full};
                            let not_found = hyper::Response::builder()
                                .status(404)
                                .body(Full::new(bytes::Bytes::new()).boxed())
                                .expect("static response");
                            return Ok::<_, Infallible>(not_found);
                        }
                        Ok(svc.handle(req).await)
                    }
                });
            let _ = hyper::server::conn::http1::Builder::new()
                .serve_connection(hyper_util::rt::TokioIo::new(stream), handler)
                .await;
        });
    }
}
