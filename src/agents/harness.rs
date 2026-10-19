//! How each harness is started, and how its JSONL stream reads.
//!
//! Both run with their own sandbox on and no network for shell commands, so
//! an agent can edit and test in its worktree but cannot push, comment or
//! send. That matches what every `cos` agent prompt ends with: stop before
//! anything outward-facing.

use std::process::Command;

use serde_json::Value;

use super::{Harness, Spec};

/// Claude Code's Seatbelt sandbox for Bash: writes limited to the worktree
/// and temp, network refused without a prompt, no retry outside the sandbox.
pub const CLAUDE_SANDBOX: &str = r#"{"sandbox":{"enabled":true,"autoAllowBashIfSandboxed":true,"allowUnsandboxedCommands":false,"failIfUnavailable":true,"network":{"strictAllowlist":true}}}"#;

pub fn command(spec: &Spec) -> Command {
    let mut cmd = Command::new(&spec.program);
    cmd.current_dir(&spec.workdir);
    match spec.harness {
        Harness::Claude => {
            cmd.args(["-p", &spec.prompt])
                .args(["--output-format", "stream-json", "--verbose"])
                .args(["--permission-mode", "acceptEdits"])
                .args(["--permission-prompts", "none"])
                .args(["--settings", CLAUDE_SANDBOX])
                .args(["--max-budget-usd", &spec.budget_usd.to_string()]);
            if let Some(m) = &spec.model {
                cmd.args(["--model", m]);
            }
        }
        Harness::Codex => {
            cmd.args(["exec", "--json", "--sandbox", "workspace-write"])
                .arg("--skip-git-repo-check")
                .arg("--cd")
                .arg(&spec.workdir);
            if let Some(m) = &spec.model {
                cmd.args(["--model", m]);
            }
            // `--` keeps a prompt starting with "review" or "resume" from
            // being taken as a subcommand.
            cmd.args(["--", &spec.prompt]);
        }
    }
    cmd
}

/// What one stream line says about the run. Lines that say nothing (or are
/// not JSON) read as the default.
#[derive(Debug, Default, PartialEq)]
pub struct Tick {
    pub last: Option<String>,
    pub tool_calls: u32,
    pub cost_usd: Option<f64>,
    pub tokens: Option<u64>,
    pub summary: Option<String>,
    pub session: Option<String>,
    pub failed: Option<String>,
}

pub fn read(harness: Harness, line: &str) -> Tick {
    let Ok(v) = serde_json::from_str::<Value>(line) else {
        return Tick::default();
    };
    match harness {
        Harness::Claude => claude(&v),
        Harness::Codex => codex(&v),
    }
}

/// `claude -p --output-format stream-json`: `system`/`init`, `assistant`
/// messages with `text` and `tool_use` blocks, one final `result`.
fn claude(v: &Value) -> Tick {
    let mut t = Tick::default();
    match v["type"].as_str() {
        Some("system") if v["subtype"] == "init" => t.session = text(&v["session_id"]),
        Some("assistant") => {
            for block in v["message"]["content"].as_array().into_iter().flatten() {
                match block["type"].as_str() {
                    Some("tool_use") => {
                        t.tool_calls += 1;
                        t.last = Some(format!("tool {}", block["name"].as_str().unwrap_or("?")));
                    }
                    Some("text") => t.last = text(&block["text"]).map(|s| clip(&s)),
                    _ => {}
                }
            }
        }
        Some("result") => {
            t.cost_usd = v["total_cost_usd"].as_f64();
            t.summary = text(&v["result"]);
            if v["is_error"].as_bool() == Some(true) {
                let errors: Vec<&str> = v["errors"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .collect();
                let subtype = v["subtype"].as_str().unwrap_or("error");
                t.failed = Some(if errors.is_empty() {
                    subtype.to_owned()
                } else {
                    format!("{subtype}: {}", errors.join("; "))
                });
            }
        }
        _ => {}
    }
    t
}

/// `codex exec --json`: `thread.started`, `item.started`/`item.completed`
/// (agent_message, command_execution, file_change, mcp_tool_call,
/// web_search), `turn.completed` with token usage, `turn.failed`, `error`.
fn codex(v: &Value) -> Tick {
    let mut t = Tick::default();
    let item = &v["item"];
    match (v["type"].as_str(), item["type"].as_str()) {
        (Some("thread.started"), _) => t.session = text(&v["thread_id"]),
        (Some("item.started"), Some("command_execution")) => {
            t.tool_calls = 1;
            t.last = Some(format!(
                "run {}",
                clip(item["command"].as_str().unwrap_or("?"))
            ));
        }
        (Some("item.started"), Some("mcp_tool_call")) => {
            t.tool_calls = 1;
            t.last = Some(format!(
                "tool {}.{}",
                item["server"].as_str().unwrap_or("?"),
                item["tool"].as_str().unwrap_or("?")
            ));
        }
        (Some("item.started"), Some("web_search")) => {
            t.tool_calls = 1;
            t.last = Some(format!(
                "search {}",
                clip(item["query"].as_str().unwrap_or(""))
            ));
        }
        (Some("item.completed"), Some("agent_message")) => {
            t.summary = text(&item["text"]);
            t.last = t.summary.as_deref().map(clip);
        }
        (Some("item.completed"), Some("file_change")) => {
            let paths: Vec<&str> = item["changes"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|c| c["path"].as_str())
                .collect();
            t.last = Some(format!("edit {}", clip(&paths.join(", "))));
        }
        (Some("turn.completed"), _) => {
            let u = &v["usage"];
            t.tokens = Some(
                u["input_tokens"].as_u64().unwrap_or(0) + u["output_tokens"].as_u64().unwrap_or(0),
            );
        }
        (Some("turn.failed"), _) => {
            t.failed = Some(text(&v["error"]["message"]).unwrap_or_else(|| "turn failed".into()));
        }
        (Some("error"), _) => {
            t.failed = Some(text(&v["message"]).unwrap_or_else(|| "error".into()));
        }
        _ => {}
    }
    t
}

fn text(v: &Value) -> Option<String> {
    v.as_str().filter(|s| !s.is_empty()).map(str::to_owned)
}

/// First line, at most 140 characters.
fn clip(s: &str) -> String {
    let line = s
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim();
    match line.char_indices().nth(140) {
        Some((i, _)) => format!("{}…", &line[..i]),
        None => line.to_owned(),
    }
}
