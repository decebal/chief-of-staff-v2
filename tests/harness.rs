use cos::agents::harness::{CLAUDE_SANDBOX, Tick, command, read};
use cos::agents::{Harness, Spec};

fn spec(harness: Harness, prompt: &str) -> Spec {
    Spec {
        id: "x".into(),
        harness,
        program: harness.program().into(),
        model: Some("m".into()),
        repo: None,
        base: None,
        workdir: "/tmp".into(),
        worktree: false,
        title: String::new(),
        source: String::new(),
        prompt: prompt.into(),
        deadline_secs: 60,
        budget_usd: 2.5,
        created: String::new(),
        slots: None,
    }
}

fn args(s: &Spec) -> Vec<String> {
    command(s)
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect()
}

#[test]
fn claude_runs_sandboxed_with_no_prompts_and_a_budget() {
    let a = args(&spec(Harness::Claude, "do it"));
    let has = |pair: [&str; 2]| a.windows(2).any(|w| w == pair);
    assert!(has(["--permission-mode", "acceptEdits"]));
    assert!(has(["--permission-prompts", "none"]));
    assert!(has(["--settings", CLAUDE_SANDBOX]));
    assert!(has(["--max-budget-usd", "2.5"]));
    let sandbox: serde_json::Value = serde_json::from_str(CLAUDE_SANDBOX).unwrap();
    assert_eq!(sandbox["sandbox"]["enabled"], true);
    assert_eq!(sandbox["sandbox"]["allowUnsandboxedCommands"], false);
    assert_eq!(sandbox["sandbox"]["network"]["strictAllowlist"], true);
}

#[test]
fn codex_prompt_cannot_be_read_as_a_subcommand() {
    let a = args(&spec(Harness::Codex, "review the parser"));
    assert_eq!(&a[..2], ["exec", "--json"]);
    assert!(a.windows(2).any(|w| w == ["--sandbox", "workspace-write"]));
    assert_eq!(&a[a.len() - 2..], ["--", "review the parser"]);
}

#[test]
fn reads_claude_stream_lines() {
    let tool = read(
        Harness::Claude,
        r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"1","name":"Edit","input":{}},{"type":"tool_use","id":"2","name":"Bash","input":{}}]}}"#,
    );
    assert_eq!(tool.tool_calls, 2);
    assert_eq!(tool.last.as_deref(), Some("tool Bash"));

    let done = read(
        Harness::Claude,
        r#"{"type":"result","subtype":"success","is_error":false,"result":"ok","total_cost_usd":0.3}"#,
    );
    assert_eq!(done.cost_usd, Some(0.3));
    assert_eq!(done.summary.as_deref(), Some("ok"));
    assert_eq!(done.failed, None);

    assert_eq!(read(Harness::Claude, "not json"), Tick::default());
}

#[test]
fn reads_codex_stream_lines() {
    let run = read(
        Harness::Codex,
        r#"{"type":"item.started","item":{"type":"command_execution","command":"cargo test"}}"#,
    );
    assert_eq!(
        (run.tool_calls, run.last.as_deref()),
        (1, Some("run cargo test"))
    );

    let failed = read(
        Harness::Codex,
        r#"{"type":"turn.failed","error":{"message":"quota"}}"#,
    );
    assert_eq!(failed.failed.as_deref(), Some("quota"));

    let long = "x".repeat(400);
    let msg = read(
        Harness::Codex,
        &format!(
            r#"{{"type":"item.completed","item":{{"type":"agent_message","text":"{long}"}}}}"#
        ),
    );
    assert_eq!(msg.summary.as_deref(), Some(long.as_str()));
    assert!(msg.last.unwrap().chars().count() <= 141);
}
