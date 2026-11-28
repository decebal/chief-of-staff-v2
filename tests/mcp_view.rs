//! The MCP App view (`src/dashboard.html`) in headless Chrome, framed by a
//! stand-in host that answers its tool calls from a script. Run with
//! `COS_E2E_CHROME=<Chrome binary>` and `--include-ignored` (`task test-e2e`).

#[allow(dead_code)]
#[path = "common/cdp.rs"]
mod cdp;

use cdp::Chrome;
use serde_json::{Value, json};

const VIEW: &str = include_str!("../src/dashboard.html");
const RUN: &str = "r-20261125-091502-k3x9";

fn run_view(state: &str) -> Value {
    json!({
        "run": RUN,
        "business": "acme",
        "kind": "brief",
        "state": state,
        "link": format!("cos://run/acme/{RUN}"),
    })
}

/// The view framed by a host that answers each tool from `script`: a tool's
/// results in order, the last one repeated.
fn open(name: &str, script: Value) -> Chrome {
    cdp::chrome_path();
    let dir = cdp::profile(name);
    std::fs::create_dir_all(&dir).unwrap();
    let src = serde_json::to_string(VIEW).unwrap().replace("</", "<\\/");
    let script = script.to_string().replace("</", "<\\/");
    let host = format!(
        r#"<!doctype html><meta charset="utf-8"><body><iframe id="v" style="width:50rem;height:40rem"></iframe>
<script>
window.calls = [];
window.errors = [];
const script = {script};
window.addEventListener("message", (ev) => {{
  const m = ev.data;
  if (!m || m.jsonrpc !== "2.0" || m.id == null || !m.method) return;
  let result = {{}};
  if (m.method === "tools/call") {{
    calls.push(m.params);
    const q = script[m.params.name] || [{{ isError: true, content: [{{ type: "text", text: "unscripted" }}] }}];
    result = q.length > 1 ? q.shift() : q[0];
  }}
  ev.source.postMessage({{ jsonrpc: "2.0", id: m.id, result }}, "*");
}});
const f = document.getElementById("v");
f.addEventListener("load", () => {{
  if (!f.contentDocument.getElementById("run")) return;
  f.contentWindow.addEventListener("error", (e) => errors.push(String(e.message)));
  f.contentWindow.addEventListener("unhandledrejection", (e) => errors.push(String(e.reason)));
  window.framed = true;
}});
f.srcdoc = {src};
</script>"#
    );
    let page = dir.join("host.html");
    std::fs::write(&page, host).unwrap();
    let mut c = Chrome::start(&dir, &["--disable-gpu"]);
    c.open(&format!("file://{}", page.display()));
    c.wait_for(
        "window.framed && document.getElementById('v').contentDocument.getElementById('run')",
    );
    c
}

/// Run new brief, as a person does it: arm, then confirm.
fn run_new_brief(c: &mut Chrome) {
    c.eval("(() => { const b = document.getElementById('v').contentDocument.getElementById('run'); b.click(); b.click(); })()");
}

fn frame_text(c: &mut Chrome, id: &str) -> String {
    c.eval(&format!(
        "document.getElementById('v').contentDocument.getElementById({id:?}).textContent"
    ))
    .as_str()
    .unwrap_or_default()
    .to_owned()
}

#[test]
#[ignore = "needs COS_E2E_CHROME"]
fn run_new_brief_follows_the_run_on_its_business_and_renders_the_report() {
    let mut done = run_view("done");
    done["day"] = json!("2026-11-25");
    done["cost_usd"] = json!(0.42);
    done["report"] = json!({
        "headline": "Ship the acme invoice run",
        "needs_you": [], "what_changed": [], "calendar": [], "backlog": [],
        "coverage": [], "gaps": [], "week": {},
    });
    let mut started = run_view("queued");
    started["existing"] = json!(false);
    let mut c = open(
        "mcp-view-done",
        json!({
            "brief": [{ "structuredContent": started }],
            "run_status": [
                { "structuredContent": run_view("running") },
                { "structuredContent": done },
            ],
        }),
    );
    run_new_brief(&mut c);
    c.wait_for(
        "document.getElementById('v').contentDocument.getElementById('view').textContent.includes('Ship the acme invoice run')",
    );
    let calls = c.eval("calls");
    let names: Vec<&str> = calls
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["brief", "run_status", "run_status"], "{calls}");
    for poll in &calls.as_array().unwrap()[1..] {
        assert_eq!(
            poll["arguments"],
            json!({ "run": RUN, "business": "acme", "wait_secs": 45 }),
            "{calls}"
        );
    }
    assert_eq!(frame_text(&mut c, "status"), "Fresh run, $0.42.");
    assert_eq!(c.eval("errors"), json!([]));
}

#[test]
#[ignore = "needs COS_E2E_CHROME"]
fn a_run_that_ends_failed_or_a_refused_status_is_said_not_thrown() {
    let mut failed = run_view("failed");
    failed["reason"] = json!("deadline");
    let mut c = open(
        "mcp-view-failed",
        json!({
            "brief": [{ "structuredContent": run_view("running") }],
            "run_status": [{ "structuredContent": failed }],
        }),
    );
    run_new_brief(&mut c);
    let said = format!("Run {RUN} failed: deadline.");
    c.wait_for(&format!(
        "document.getElementById('v').contentDocument.getElementById('status').textContent === {said:?}"
    ));
    assert_eq!(c.eval("errors"), json!([]));
    drop(c);

    let mut c = open(
        "mcp-view-refused",
        json!({
            "brief": [{ "structuredContent": run_view("running") }],
            "run_status": [{ "isError": true, "content": [{ "type": "text", "text": "business \"acme\" is not registered; known: " }] }],
        }),
    );
    run_new_brief(&mut c);
    c.wait_for(
        "document.getElementById('v').contentDocument.getElementById('status').textContent.includes('not registered')",
    );
    assert_eq!(c.eval("errors"), json!([]));
    assert!(
        frame_text(&mut c, "view").contains("Nothing loaded yet"),
        "a refused status rendered a report"
    );
}
