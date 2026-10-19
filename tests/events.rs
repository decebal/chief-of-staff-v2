use cos::claude::{Block, Event};

fn parse(line: &str) -> Event {
    serde_json::from_str(line).unwrap()
}

#[test]
fn tool_use_carries_id_name_and_subagent_parent() {
    let e = parse(
        r#"{"type":"assistant","parent_tool_use_id":"toolu_parent","session_id":"s","message":{"content":[
            {"type":"tool_use","id":"toolu_1","name":"mcp__claude_ai_Gmail__search_threads","input":{"query":"is:unread"}}]}}"#,
    );
    let Event::Assistant {
        message,
        parent_tool_use_id,
    } = e
    else {
        panic!("not assistant")
    };
    assert_eq!(parent_tool_use_id.as_deref(), Some("toolu_parent"));
    assert!(
        matches!(&message.content[0], Block::ToolUse { id, name, .. }
        if id == "toolu_1" && name == "mcp__claude_ai_Gmail__search_threads")
    );
}

#[test]
fn failed_tool_result_is_visible() {
    let e = parse(
        r#"{"type":"user","message":{"content":[
            {"type":"tool_result","tool_use_id":"toolu_1","is_error":true,"content":"needs to be connected in claude.ai"}]}}"#,
    );
    let Event::User { message } = e else {
        panic!("not user")
    };
    assert!(matches!(
        &message.content[0],
        Block::ToolResult { is_error: true, .. }
    ));
}

#[test]
fn result_carries_structured_output_and_cost() {
    let e = parse(
        r#"{"type":"result","subtype":"success","is_error":false,"structured_output":{"ok":true},
            "total_cost_usd":0.17,"num_turns":2,"permission_denials":[]}"#,
    );
    let Event::Result(o) = e else {
        panic!("not result")
    };
    assert_eq!(o.structured_output, Some(serde_json::json!({"ok": true})));
    assert!(!o.is_error);
}

#[test]
fn budget_stop_is_an_error_result() {
    let e = parse(
        r#"{"type":"result","subtype":"error_max_budget_usd","is_error":true,"errors":["Reached maximum budget ($0.5)"]}"#,
    );
    let Event::Result(o) = e else {
        panic!("not result")
    };
    assert!(o.is_error);
    assert_eq!(o.errors, ["Reached maximum budget ($0.5)"]);
}

#[test]
fn unknown_event_types_do_not_fail_the_stream() {
    assert!(matches!(
        parse(r#"{"type":"rate_limit_event","x":1}"#),
        Event::Other
    ));
    assert!(matches!(
        parse(r#"{"type":"system","subtype":"hook_started","hook_id":"h"}"#),
        Event::System { .. }
    ));
}
