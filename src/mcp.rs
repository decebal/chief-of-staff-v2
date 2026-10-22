//! `cos` as an MCP server, so Claude Desktop, Codex and claude.ai can call it.
//!
//! Two modes. `Local` (stdio) offers every tool, including the paid runs.
//! `Remote` (HTTP behind a secret path) offers only the free read tools: a
//! leaked URL can read the last brief, never spend money or trigger a run.

use std::convert::Infallible;
use std::sync::Arc;

use anyhow::Result;
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
use crate::pipeline::Config;
use crate::reports::{Brief, Review};
use crate::store::{self, Kind};

const DASHBOARD_URI: &str = "ui://cos/dashboard";
const DASHBOARD_HTML: &str = include_str!("dashboard.html");
const APP_MIME: &str = "text/html;profile=mcp-app";

/// Tools a remote client may call. Everything else is local-only.
const REMOTE_TOOLS: &[&str] = &["latest_brief", "latest_review"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Local,
    Remote,
}

#[derive(Clone)]
pub struct Cos {
    cfg: Config,
    mode: Mode,
}

/// What every tool returns as structured content; the dashboard reads this.
#[derive(Serialize, schemars::JsonSchema)]
pub struct Report {
    /// "brief" or "review"; null when nothing is saved yet.
    kind: Option<String>,
    day: Option<String>,
    report: Option<Value>,
    markdown: Option<String>,
    /// Set only for a fresh run.
    cost_usd: Option<f64>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct AskArgs {
    /// The question for the chief of staff.
    question: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct SpinOffArgs {
    /// "brief" (an action) or "review" (a goal), from the latest saved run.
    from: agents::From,
    /// The item's number, as the report numbers it.
    item: usize,
    /// "claude" (default) or "codex".
    #[serde(default)]
    harness: Option<agents::Harness>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct DeliverableId {
    /// A deliverable id from `plan`, such as "D-1".
    deliverable: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct AgentId {
    /// An agent id from `agents`.
    id: String,
}

fn ui_meta() -> MetaObject {
    let Value::Object(map) = json!({ "ui": { "resourceUri": DASHBOARD_URI } }) else {
        unreachable!()
    };
    MetaObject::from(map)
}

fn structured(r: Report) -> CallToolResult {
    CallToolResult::structured(serde_json::to_value(r).unwrap_or(Value::Null))
}

fn failed(e: impl std::fmt::Display) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(format!("{e:#}"))])
}

#[tool_router]
impl Cos {
    pub fn new(cfg: Config, mode: Mode) -> Self {
        Self { cfg, mode }
    }

    /// The most recent saved daily brief: at most three actions that need you,
    /// what changed, then calendar, backlog and source coverage. Free and
    /// instant; does not call Claude.
    #[tool(meta = ui_meta(), output_schema = schema_for_output::<Report>(),
           annotations(read_only_hint = true, open_world_hint = false))]
    async fn latest_brief(&self) -> CallToolResult {
        self.latest::<Brief>(Kind::Briefs, "brief")
    }

    /// The most recent saved weekly review: each goal with rock/sand/water
    /// confidence and a steer, proposed deliverables, decisions. Free and
    /// instant; does not call Claude.
    #[tool(meta = ui_meta(), output_schema = schema_for_output::<Report>(),
           annotations(read_only_hint = true, open_world_hint = false))]
    async fn latest_review(&self) -> CallToolResult {
        self.latest::<Review>(Kind::Reviews, "review")
    }

    /// Run a fresh daily brief now. Calls Claude: costs money (about $0.30) and
    /// can take a few minutes. Prefer latest_brief unless asked to refresh.
    #[tool(meta = ui_meta(), output_schema = schema_for_output::<Report>(),
           annotations(read_only_hint = true, open_world_hint = true))]
    async fn brief(&self) -> CallToolResult {
        let cfg = self.cfg.clone();
        match tokio::task::spawn_blocking(move || cfg.brief()).await {
            Ok(Ok(done)) => structured(Report {
                kind: Some("brief".into()),
                day: Some(jiff::Zoned::now().date().to_string()),
                report: serde_json::to_value(&done.value).ok(),
                markdown: Some(done.markdown),
                cost_usd: Some(done.cost_usd),
            }),
            Ok(Err(e)) => failed(e),
            Err(e) => failed(e),
        }
    }

    /// Run a fresh weekly review now. Calls Claude: costs money (about $1–2)
    /// and can take a few minutes. Prefer latest_review unless asked to refresh.
    #[tool(meta = ui_meta(), output_schema = schema_for_output::<Report>(),
           annotations(read_only_hint = true, open_world_hint = true))]
    async fn review(&self) -> CallToolResult {
        let cfg = self.cfg.clone();
        match tokio::task::spawn_blocking(move || cfg.review()).await {
            Ok(Ok(done)) => structured(Report {
                kind: Some("review".into()),
                day: Some(jiff::Zoned::now().date().to_string()),
                report: serde_json::to_value(&done.value).ok(),
                markdown: Some(done.markdown),
                cost_usd: Some(done.cost_usd),
            }),
            Ok(Err(e)) => failed(e),
            Err(e) => failed(e),
        }
    }

    /// Ask the chief of staff a question about priorities or today's work. It
    /// routes to the tactical and/or strategic brain and merges their answers.
    /// Calls Claude: costs money (about $1–1.50) and can take a few minutes.
    #[tool(annotations(read_only_hint = true, open_world_hint = true))]
    async fn ask(&self, Parameters(args): Parameters<AskArgs>) -> CallToolResult {
        let cfg = self.cfg.clone();
        match tokio::task::spawn_blocking(move || cfg.ask(&args.question)).await {
            Ok(Ok(done)) => CallToolResult::success(vec![ContentBlock::text(format!(
                "{}\n\n(cost ${:.2})",
                done.markdown, done.cost_usd
            ))]),
            Ok(Err(e)) => failed(e),
            Err(e) => failed(e),
        }
    }

    /// The tracked plan from `.chief-of-staff/state.json`: goals, KPIs, open
    /// deliverables and commitments, source coverage, open incidents. Free;
    /// local only.
    #[tool(annotations(read_only_hint = true, open_world_hint = false))]
    async fn plan(&self) -> CallToolResult {
        match crate::state::read(&self.cfg.workspace) {
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
            Ok(None) => {
                failed("no .chief-of-staff workspace; run `cos state init` and set up goals")
            }
            Err(e) => failed(e),
        }
    }

    /// "Delegate this": the handoff for a tracked deliverable (outcome, why
    /// now, owner, evidence, done when, permission boundary). Prepares text
    /// only; starts and sends nothing. Free; local only.
    #[tool(annotations(read_only_hint = true, open_world_hint = false))]
    async fn handoff(&self, Parameters(args): Parameters<DeliverableId>) -> CallToolResult {
        let st = match crate::state::read(&self.cfg.workspace) {
            Ok(Some(st)) => st,
            Ok(None) => return failed("no .chief-of-staff workspace"),
            Err(e) => return failed(e),
        };
        let profile = crate::state::profile(&self.cfg.workspace);
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

    /// Spun-off coding agents: status, tool calls, cost, latest activity,
    /// workdir. Free; local only.
    #[tool(annotations(read_only_hint = true, open_world_hint = false))]
    async fn agents(&self) -> CallToolResult {
        match agents::list(&self.cfg.home) {
            Ok(list) => {
                let rows: Vec<Value> = list
                    .iter()
                    .map(|(spec, st)| {
                        json!({ "id": spec.id, "title": spec.title, "source": spec.source,
                        "harness": spec.harness, "workdir": spec.workdir, "state": st })
                    })
                    .collect();
                CallToolResult::structured(json!({ "agents": rows }))
            }
            Err(e) => failed(e),
        }
    }

    /// Start a coding agent (Claude Code or Codex) on one item's agent prompt,
    /// in a detached worktree of the repo it names. Returns at once; the agent
    /// runs in the background, sandboxed, and stops before anything
    /// outward-facing. Costs money (capped per agent). Local only.
    #[tool(annotations(read_only_hint = false, open_world_hint = true))]
    async fn spin_off(&self, Parameters(args): Parameters<SpinOffArgs>) -> CallToolResult {
        let home = self.cfg.home.clone();
        let projects = self.cfg.projects.clone();
        let budget = self.cfg.budget.unwrap_or(5.0);
        let run = move || -> Result<agents::Spec> {
            let task = agents::tasks(&home, args.from)?
                .into_iter()
                .find(|t| t.n == args.item)
                .ok_or_else(|| anyhow::anyhow!("item {} has no agent prompt", args.item))?;
            let cos = std::env::current_exe()?;
            agents::spawn(
                &home,
                agents::Request {
                    harness: args.harness.unwrap_or(agents::Harness::Claude),
                    program: None,
                    model: None,
                    repo: agents::infer_repo(&task.prompt, &projects),
                    worktree: true,
                    title: task.title,
                    source: task.source,
                    prompt: task.prompt,
                    deadline: std::time::Duration::from_secs(1800),
                    budget_usd: budget,
                    slots: None,
                },
                &cos,
            )
        };
        match tokio::task::spawn_blocking(run).await {
            Ok(Ok(spec)) => CallToolResult::success(vec![ContentBlock::text(format!(
                "started {} in {} — follow with the agents tool or `cos agents tail -f {}`",
                spec.id,
                spec.workdir.display(),
                spec.id
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
    async fn stop_agent(&self, Parameters(args): Parameters<AgentId>) -> CallToolResult {
        let home = self.cfg.home.clone();
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
}

impl Cos {
    fn latest<T: serde::de::DeserializeOwned + Serialize>(
        &self,
        kind: Kind,
        name: &str,
    ) -> CallToolResult {
        match store::latest::<T>(&self.cfg.home, kind) {
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

    fn allowed(&self, tool: &str) -> bool {
        self.mode == Mode::Local || REMOTE_TOOLS.contains(&tool)
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
            "Chief of staff. latest_brief / latest_review are free and instant: use them \
                 first. brief, review and ask call Claude and cost money; use them only when \
                 the user asks for a fresh run or a question the saved brief cannot answer.",
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
        Ok(ReadResourceResult::new(vec![
            ResourceContents::text(DASHBOARD_HTML, req.uri).with_mime_type(APP_MIME),
        ])
        .into())
    }
}

pub async fn stdio(cfg: Config, mode: Mode) -> Result<()> {
    Cos::new(cfg, mode)
        .serve(rmcp::transport::stdio())
        .await?
        .waiting()
        .await?;
    Ok(())
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
