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
    /// The call made, tool and arguments in full, so two calls compare equal
    /// only when the agent repeated itself exactly.
    pub call: Option<String>,
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
                        let name = block["name"].as_str().unwrap_or("?");
                        let d = describe(name, &block["input"]);
                        t.last = Some(if d.is_empty() {
                            format!("tool {name}")
                        } else {
                            format!("tool {name}: {}", clip(&d))
                        });
                        t.call = Some(format!("{name} {}", block["input"]));
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
            let command = item["command"].as_str().unwrap_or("?");
            t.last = Some(format!("run {}", clip(command)));
            t.call = Some(format!("run {command}"));
        }
        (Some("item.started"), Some("mcp_tool_call")) => {
            t.tool_calls = 1;
            let tool = format!(
                "{}.{}",
                item["server"].as_str().unwrap_or("?"),
                item["tool"].as_str().unwrap_or("?")
            );
            t.call = Some(format!("{tool} {}", item["arguments"]));
            t.last = Some(format!("tool {tool}"));
        }
        (Some("item.started"), Some("web_search")) => {
            t.tool_calls = 1;
            let query = item["query"].as_str().unwrap_or("");
            t.last = Some(format!("search {}", clip(query)));
            t.call = Some(format!("search {query}"));
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

/// One thing the activity feed shows.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Entry {
    /// Where it sits in `events.jsonl` (byte offset, block); the same line
    /// read twice gets the same key.
    pub key: String,
    pub kind: EntryKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    /// The command, path, pattern, query or message, clipped.
    pub detail: String,
    /// The tool call's id: on a call its own, on a result the call it answers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub call: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ok: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryKind {
    Start,
    Call,
    Result,
    Say,
    Denied,
    Failed,
    End,
}

/// Longest detail kept for a call or a message.
const DETAIL: usize = 300;

/// What a feed shows for one stream line; `key` is the line's prefix.
pub fn entries(harness: Harness, line: &str, key: &str) -> Vec<Entry> {
    let Ok(v) = serde_json::from_str::<Value>(line) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut push = |kind, tool: Option<String>, detail: String, call: Option<String>, ok| {
        out.push(Entry {
            key: format!("{key}:{}", out.len()),
            kind,
            tool,
            detail,
            call,
            ok,
        });
    };
    match harness {
        Harness::Claude => match v["type"].as_str() {
            Some("system") if v["subtype"] == "init" => push(
                EntryKind::Start,
                None,
                v["model"].as_str().map_or("session started".into(), |m| {
                    format!("session started on {m}")
                }),
                None,
                None,
            ),
            Some("system") if v["subtype"] == "permission_denied" => push(
                EntryKind::Denied,
                text(&v["tool_name"]),
                clip_to(v["message"].as_str().unwrap_or("permission denied"), DETAIL),
                text(&v["tool_use_id"]),
                Some(false),
            ),
            Some("assistant") => {
                for b in v["message"]["content"].as_array().into_iter().flatten() {
                    match b["type"].as_str() {
                        Some("tool_use") => {
                            let name = b["name"].as_str().unwrap_or("?").to_owned();
                            let d = describe(&name, &b["input"]);
                            push(EntryKind::Call, Some(name), d, text(&b["id"]), None);
                        }
                        Some("text") => {
                            if let Some(t) = text(&b["text"]) {
                                push(EntryKind::Say, None, clip_to(&t, DETAIL), None, None);
                            }
                        }
                        _ => {}
                    }
                }
            }
            Some("user") => {
                for b in v["message"]["content"].as_array().into_iter().flatten() {
                    if b["type"] == "tool_result" {
                        let ok = b["is_error"].as_bool() != Some(true);
                        push(
                            EntryKind::Result,
                            None,
                            clip(&result_text(&b["content"])),
                            text(&b["tool_use_id"]),
                            Some(ok),
                        );
                    }
                }
            }
            Some("result") => {
                let ok = v["is_error"].as_bool() != Some(true);
                let cost = v["total_cost_usd"]
                    .as_f64()
                    .map(|c| format!(", ${c:.2}"))
                    .unwrap_or_default();
                push(
                    if ok {
                        EntryKind::End
                    } else {
                        EntryKind::Failed
                    },
                    None,
                    format!("ended {}{cost}", v["subtype"].as_str().unwrap_or("")),
                    None,
                    Some(ok),
                );
            }
            _ => {}
        },
        Harness::Codex => {
            let item = &v["item"];
            match (v["type"].as_str(), item["type"].as_str()) {
                (Some("thread.started"), _) => {
                    push(EntryKind::Start, None, "session started".into(), None, None)
                }
                (Some("item.started"), Some("command_execution")) => push(
                    EntryKind::Call,
                    Some("run".into()),
                    clip_to(item["command"].as_str().unwrap_or("?"), DETAIL),
                    text(&item["id"]),
                    None,
                ),
                (Some("item.completed"), Some("command_execution")) => push(
                    EntryKind::Result,
                    None,
                    clip(item["aggregated_output"].as_str().unwrap_or("")),
                    text(&item["id"]),
                    Some(item["exit_code"].as_i64().is_none_or(|c| c == 0)),
                ),
                (Some("item.started"), Some("mcp_tool_call")) => push(
                    EntryKind::Call,
                    Some(format!(
                        "{}.{}",
                        item["server"].as_str().unwrap_or("?"),
                        item["tool"].as_str().unwrap_or("?")
                    )),
                    clip_to(&item["arguments"].to_string(), DETAIL),
                    text(&item["id"]),
                    None,
                ),
                (Some("item.started"), Some("web_search")) => push(
                    EntryKind::Call,
                    Some("search".into()),
                    clip_to(item["query"].as_str().unwrap_or(""), DETAIL),
                    text(&item["id"]),
                    None,
                ),
                (Some("item.completed"), Some("file_change")) => {
                    let paths: Vec<&str> = item["changes"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|c| c["path"].as_str())
                        .collect();
                    push(
                        EntryKind::Call,
                        Some("edit".into()),
                        clip_to(&paths.join(", "), DETAIL),
                        text(&item["id"]),
                        Some(true),
                    )
                }
                (Some("item.completed"), Some("agent_message")) => {
                    if let Some(t) = text(&item["text"]) {
                        push(EntryKind::Say, None, clip_to(&t, DETAIL), None, None);
                    }
                }
                (Some("turn.failed"), _) => push(
                    EntryKind::Failed,
                    None,
                    text(&v["error"]["message"]).unwrap_or_else(|| "turn failed".into()),
                    None,
                    Some(false),
                ),
                (Some("error"), _) => push(
                    EntryKind::Failed,
                    None,
                    text(&v["message"]).unwrap_or_else(|| "error".into()),
                    None,
                    Some(false),
                ),
                _ => {}
            }
        }
    }
    out
}

/// What a Claude Code tool call did, from its input: the command, the path,
/// the pattern, the query; otherwise its input, compact.
pub fn describe(name: &str, input: &Value) -> String {
    let s = |k: &str| input[k].as_str().filter(|v| !v.is_empty());
    let d = match name {
        "Bash" => s("command").map(str::to_owned),
        "Read" | "Write" | "Edit" | "MultiEdit" | "NotebookEdit" => {
            s("file_path").or(s("notebook_path")).map(str::to_owned)
        }
        "Grep" | "Glob" => s("pattern").map(|p| match s("path") {
            Some(path) => format!("{p} in {path}"),
            None => p.to_owned(),
        }),
        "WebFetch" => s("url").map(str::to_owned),
        "WebSearch" => s("query").map(str::to_owned),
        "Task" | "Agent" => s("description").or(s("prompt")).map(str::to_owned),
        _ => None,
    };
    let d = d.unwrap_or_else(|| match input {
        Value::Object(m) if m.is_empty() => String::new(),
        Value::Null => String::new(),
        v => v.to_string(),
    });
    clip_to(&d, DETAIL)
}

/// A tool result's text: a string, or the text blocks of an array.
fn result_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Array(a) => a
            .iter()
            .filter_map(|b| b["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

/// Whitespace collapsed to single spaces, at most `n` characters.
fn clip_to(s: &str, n: usize) -> String {
    let flat = s.split_whitespace().collect::<Vec<_>>().join(" ");
    match flat.char_indices().nth(n) {
        Some((i, _)) => format!("{}…", &flat[..i]),
        None => flat,
    }
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
