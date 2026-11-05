//! Headless Claude Code over `--output-format stream-json`: the same transport
//! the official Agent SDKs use, driven from Rust.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use serde_json::Value;

use crate::proc;

/// One headless run. Every field maps to a `claude` flag.
#[derive(Clone)]
pub struct Run<'a> {
    pub agent: &'a str,
    pub model: &'a str,
    pub prompt: String,
    pub json_schema: Option<Value>,
    pub max_budget_usd: f64,
    pub tools: Toolset,
    pub deadline: Duration,
    /// MCP server prefixes denied outright (`context/deny-mcp.txt`).
    pub denied: Vec<String>,
}

#[derive(Clone)]
pub enum Toolset {
    /// The connectors, limited to this allowlist (`context::tools`).
    Brain(Vec<&'static str>),
    /// No tools and no MCP servers: routing and merging only.
    None,
}

/// The only built-in tool a brain gets. `cos` runs `cn` and `gh` and inlines
/// `context/` itself, so a brain needs no Bash and no file reads.
pub const BRAIN_TOOLS: &str = "ToolSearch";

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    System {
        subtype: String,
    },
    Assistant {
        message: Message,
        parent_tool_use_id: Option<String>,
    },
    User {
        message: Message,
    },
    Result(Outcome),
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
pub struct Message {
    #[serde(default)]
    pub content: Vec<Block>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Block {
    Text {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    ToolResult {
        tool_use_id: String,
        #[serde(default)]
        is_error: bool,
        #[serde(default)]
        content: Value,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
pub struct Outcome {
    pub subtype: String,
    pub is_error: bool,
    #[serde(default)]
    pub result: Option<String>,
    #[serde(default)]
    pub structured_output: Option<Value>,
    #[serde(default)]
    pub errors: Vec<String>,
    #[serde(default)]
    pub total_cost_usd: f64,
    #[serde(default)]
    pub num_turns: u32,
    #[serde(default)]
    pub permission_denials: Vec<Value>,
    #[serde(default)]
    pub api_error_status: Option<Box<Value>>,
    /// Every tool the run called, from the stream rather than the model's word.
    #[serde(skip)]
    pub called: Vec<String>,
}

/// The `claude` invocation for `run`, with `home` as cwd so the repo's
/// CLAUDE.md and `.claude/agents` load.
pub fn command(home: &Path, run: &Run) -> Command {
    command_with(Path::new("claude"), home, run)
}

/// `command` with `program` standing in for `claude`.
pub fn command_with(program: &Path, home: &Path, run: &Run) -> Command {
    let mut cmd = Command::new(program);
    cmd.current_dir(home)
        .args(["-p", &run.prompt])
        .args(["--output-format", "stream-json", "--verbose"])
        .args(["--agent", run.agent, "--model", run.model])
        // User settings carry ~80k tokens of rules and skills a brief never reads.
        .args(["--setting-sources", "project", "--disable-slash-commands"])
        .args(["--permission-mode", "dontAsk"])
        .args(["--max-budget-usd", &run.max_budget_usd.to_string()])
        .arg("--no-session-persistence");
    match &run.tools {
        Toolset::Brain(allowed) => {
            cmd.args(["--tools", BRAIN_TOOLS])
                .arg("--allowedTools")
                .arg(allowed.join(","));
            if !run.denied.is_empty() {
                cmd.arg("--disallowedTools").arg(run.denied.join(","));
            }
        }
        // An empty --tools also drops ToolSearch, which makes every MCP tool
        // load eagerly; --strict-mcp-config with no config removes them instead.
        Toolset::None => {
            cmd.args(["--tools", "", "--strict-mcp-config"]);
        }
    }
    if let Some(schema) = &run.json_schema {
        cmd.args(["--json-schema", &schema.to_string()]);
    }
    cmd
}

/// A run that ended in an error result, with what it cost and whether one
/// more attempt could succeed.
#[derive(Debug)]
pub struct Failed {
    pub reason: String,
    pub cost_usd: f64,
    pub transient: bool,
}

impl std::fmt::Display for Failed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.reason)
    }
}

impl std::error::Error for Failed {}

/// Whether `o` failed on the connection or the machine rather than on the
/// request: a timeout, a dropped socket, a stalled MCP server, a sleep
/// mid-response. A budget, auth or schema stop never is.
pub fn transient(o: &Outcome) -> bool {
    if o.subtype.starts_with("error_max") {
        return false;
    }
    let text = error_reason(o).to_lowercase();
    if ["login", "api key", "authenticat", "401", "oauth", "budget"]
        .iter()
        .any(|k| text.contains(k))
    {
        return false;
    }
    [
        "request timed out",
        "went to sleep mid-response",
        "econnreset",
        "sent no response or progress",
    ]
    .iter()
    .any(|k| text.contains(k))
}

/// Runs `run` and appends every tool call to `audit`. An error result comes
/// back as a [`Failed`].
pub fn execute(program: &Path, home: &Path, audit: &Path, run: &Run) -> Result<Outcome> {
    let mut cmd = command_with(program, home, run);
    let mut log = AuditLog::open(audit, run.agent)?;
    let mut outcome = None;
    let mut called = Vec::new();
    let (status, stderr) = proc::stream_lines(&mut cmd, run.deadline, |line| {
        if let Ok(raw) = serde_json::from_str::<Value>(line)
            && raw["type"] == "system"
            && raw["subtype"] == "init"
        {
            log.init(&raw)?;
        }
        let Ok(event) = serde_json::from_str::<Event>(line) else {
            return Ok(true);
        };
        match event {
            Event::Assistant {
                message,
                parent_tool_use_id,
            } => {
                for block in message.content {
                    if let Block::ToolUse { id, name, input } = block {
                        log.tool_use(&id, &name, &input, parent_tool_use_id.as_deref())?;
                        called.push(name);
                    }
                }
                Ok(true)
            }
            Event::User { message } => {
                for block in message.content {
                    if let Block::ToolResult {
                        tool_use_id,
                        is_error: true,
                        content,
                    } = block
                    {
                        log.tool_error(&tool_use_id, &content)?;
                    }
                }
                Ok(true)
            }
            Event::Result(o) => {
                outcome = Some(o);
                Ok(false)
            }
            Event::System { .. } | Event::Other => Ok(true),
        }
    })?;

    let Some(mut outcome) = outcome else {
        bail!(
            "claude exited ({status}) without a result event: {}",
            stderr.trim()
        );
    };
    outcome.called = called;
    log.outcome(&outcome)?;
    if outcome.is_error {
        return Err(Failed {
            reason: error_reason(&outcome),
            cost_usd: outcome.total_cost_usd,
            transient: transient(&outcome),
        }
        .into());
    }
    Ok(outcome)
}

/// Appends a note between attempts, so the audit shows both runs and why
/// the second started.
pub fn audit_note(audit: &Path, agent: &str, note: serde_json::Value) -> Result<()> {
    let mut log = AuditLog::open(audit, agent)?;
    let mut v = serde_json::json!({
        "at": jiff::Timestamp::now().to_string(),
        "agent": agent,
    });
    if let (Some(m), Some(n)) = (v.as_object_mut(), note.as_object()) {
        m.extend(n.clone());
    }
    log.write(v)
}

/// Why a run failed, in the user's terms. An auth failure arrives as
/// `subtype: "success"` with no `errors` and the reason only in `result`.
pub fn error_reason(o: &Outcome) -> String {
    let mut parts: Vec<String> = o.errors.clone();
    if let Some(r) = o.result.as_deref().map(str::trim).filter(|r| !r.is_empty()) {
        parts.push(r.to_owned());
    }
    if let Some(s) = o.api_error_status.as_ref().filter(|s| !s.is_null()) {
        parts.push(format!("API status {s}"));
    }
    let reason = if parts.is_empty() {
        "no reason given".to_owned()
    } else {
        parts.join("; ")
    };
    let lower = reason.to_lowercase();
    let hint = if ["login", "api key", "authenticat", "401", "oauth"]
        .iter()
        .any(|k| lower.contains(k))
    {
        " — Claude Code is not logged in: run `claude` and `/login`, then retry"
    } else {
        ""
    };
    // An error result can still carry `subtype: "success"`; naming it reads
    // as if the run worked.
    if o.subtype == "success" || o.subtype.is_empty() {
        format!("claude run failed: {reason}{hint}")
    } else {
        format!("claude run failed ({}): {reason}{hint}", o.subtype)
    }
}

struct AuditLog {
    file: std::fs::File,
    agent: String,
}

impl AuditLog {
    fn open(path: &Path, agent: &str) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .with_context(|| format!("open {}", path.display()))?;
        Ok(Self {
            file,
            agent: agent.to_owned(),
        })
    }

    fn tool_use(
        &mut self,
        id: &str,
        name: &str,
        input: &Value,
        parent: Option<&str>,
    ) -> Result<()> {
        self.write(serde_json::json!({
            "at": jiff::Timestamp::now().to_string(),
            "agent": self.agent,
            "id": id,
            "tool": name,
            "input": input,
            "subagent_of": parent,
        }))
    }

    /// Which MCP servers the session connected, and which connector tools it
    /// exposed: a source the brief calls unavailable is checkable here.
    fn init(&mut self, raw: &Value) -> Result<()> {
        let servers: Vec<Value> = raw["mcp_servers"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|s| {
                s["name"]
                    .as_str()
                    .is_some_and(|n| n.starts_with("claude.ai"))
            })
            .map(|s| serde_json::json!({"name": s["name"], "status": s["status"]}))
            .collect();
        let tools: Vec<&Value> = raw["tools"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|t| {
                t.as_str()
                    .is_some_and(|t| !t.starts_with("mcp__") || t.starts_with("mcp__claude_ai_"))
            })
            .collect();
        self.write(serde_json::json!({
            "at": jiff::Timestamp::now().to_string(),
            "agent": self.agent,
            "init": {"mcp_servers": servers, "tools": tools},
        }))
    }

    fn tool_error(&mut self, id: &str, content: &Value) -> Result<()> {
        self.write(serde_json::json!({
            "at": jiff::Timestamp::now().to_string(),
            "agent": self.agent,
            "id": id,
            "error": content,
        }))
    }

    fn outcome(&mut self, o: &Outcome) -> Result<()> {
        self.write(serde_json::json!({
            "at": jiff::Timestamp::now().to_string(),
            "agent": self.agent,
            "result": o.subtype,
            "cost_usd": o.total_cost_usd,
            "turns": o.num_turns,
            "denied": o.permission_denials,
        }))
    }

    fn write(&mut self, v: Value) -> Result<()> {
        writeln!(self.file, "{v}")?;
        Ok(())
    }
}

pub fn audit_path(home: &Path) -> PathBuf {
    let day = jiff::Zoned::now().date();
    home.join("out/audit").join(format!("{day}.jsonl"))
}
