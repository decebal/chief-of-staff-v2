//! The window's commands through Tauri's IPC on the mock runtime, against the
//! real `cos` supervisor binary and a stand-in harness: spawn several agents
//! past the slot limit, watch one queue, tail, stop, clean, refuse a bad
//! workspace, and serve the report on its own scheme.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tauri::WebviewWindowBuilder;
use tauri::test::{INVOKE_KEY, get_ipc_response, mock_builder, mock_context, noop_assets};
use tauri::webview::InvokeRequest;

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

fn scratch() -> PathBuf {
    let d = std::env::temp_dir().join(format!("cos-desktop-ipc-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d.canonicalize().unwrap()
}

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .status()
        .unwrap()
        .success();
    assert!(ok, "git {args:?}");
}

fn call(
    w: &tauri::WebviewWindow<tauri::test::MockRuntime>,
    cmd: &str,
    args: Value,
) -> Result<Value, Value> {
    get_ipc_response(
        w,
        InvokeRequest {
            cmd: cmd.into(),
            callback: tauri::ipc::CallbackFn(0),
            error: tauri::ipc::CallbackFn(1),
            url: "tauri://localhost".parse().unwrap(),
            body: tauri::ipc::InvokeBody::Json(args),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        },
    )
    .map(|b| b.deserialize::<Value>().unwrap())
}

fn status(w: &tauri::WebviewWindow<tauri::test::MockRuntime>, id: &str) -> String {
    let list = call(w, "agents_list", json!({})).unwrap();
    list.as_array()
        .unwrap()
        .iter()
        .find(|r| r["spec"]["id"] == id)
        .map(|r| r["state"]["status"].as_str().unwrap().to_owned())
        .unwrap_or_default()
}

fn wait(w: &tauri::WebviewWindow<tauri::test::MockRuntime>, id: &str, want: &str) {
    let until = Instant::now() + Duration::from_secs(20);
    loop {
        let now = status(w, id);
        if now == want {
            return;
        }
        assert!(Instant::now() < until, "{id} stuck at {now}, wanted {want}");
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[test]
fn the_window_spawns_queues_tails_stops_and_cleans_agents() {
    let cos = Path::new(ROOT).join("target/debug/cos");
    assert!(
        cos.is_file(),
        "build the supervisor first: cargo build --bin cos"
    );
    let root = scratch();
    let (home, ws, projects) = (root.join("home"), root.join("ws"), root.join("projects"));
    for d in [&home, &ws, &projects] {
        std::fs::create_dir_all(d).unwrap();
    }
    cos::state::initialize(&ws).unwrap();
    let demo: Value = serde_json::from_str(cos::state::DEMO_STATE).unwrap();
    cos::state::commit(&ws, &demo, 0, "Fictional fixture").unwrap();
    let repo = projects.join("demo-repo");
    std::fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    std::fs::write(repo.join("README.md"), "demo\n").unwrap();
    git(&repo, &["add", "README.md"]);
    git(&repo, &["commit", "-q", "-m", "init"]);

    let settings = cos_desktop::Settings {
        home: home.clone(),
        workspace: ws.clone(),
        projects: projects.clone(),
        slots: Some(1),
        cos: Some(cos),
    };
    let app = cos_desktop::builder(mock_builder(), Some(settings))
        .build(mock_context(noop_assets()))
        .unwrap();
    let w = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();

    let paths = call(&w, "paths", json!({})).unwrap();
    assert_eq!(paths["workspace"], json!(ws));
    assert_eq!(paths["slots"], 1);
    let repos = call(&w, "repos", json!({})).unwrap();
    assert_eq!(repos, json!([repo.to_string_lossy()]));

    let specs = call(
        &w,
        "agents_spawn",
        json!({ "args": {
            "harness": "claude", "repo": repo, "prompt": "SLEEP", "title": "two at once",
            "worktree": true, "deadlineSecs": 60, "budgetUsd": 1.0, "count": 2,
            "program": concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures/fake-claude.sh"),
        }}),
    )
    .unwrap();
    let ids: Vec<String> = specs
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(ids.len(), 2);
    assert_ne!(
        specs[0]["workdir"], specs[1]["workdir"],
        "each copy gets its own worktree"
    );

    let until = Instant::now() + Duration::from_secs(20);
    let (running, queued) = loop {
        let (a, b) = (status(&w, &ids[0]), status(&w, &ids[1]));
        match (a.as_str(), b.as_str()) {
            ("running", "queued") => break (ids[0].clone(), ids[1].clone()),
            ("queued", "running") => break (ids[1].clone(), ids[0].clone()),
            _ => assert!(
                Instant::now() < until,
                "never one running, one queued: {a} {b}"
            ),
        }
        std::thread::sleep(Duration::from_millis(100));
    };

    let tail = call(&w, "agents_tail", json!({ "id": running, "from": 0 })).unwrap();
    assert!(!tail["lines"].as_array().unwrap().is_empty(), "{tail}");

    assert_eq!(
        call(&w, "agents_stop", json!({ "id": running })).unwrap()["status"],
        "killed"
    );
    wait(&w, &queued, "running");
    assert_eq!(
        call(&w, "agents_stop", json!({ "id": queued })).unwrap()["status"],
        "killed"
    );

    for id in &ids {
        call(&w, "agents_clean", json!({ "id": id, "force": true })).unwrap();
    }
    assert_eq!(call(&w, "agents_list", json!({})).unwrap(), json!([]));

    let refused = call(&w, "set_workspace", json!({ "path": projects })).unwrap_err();
    assert!(
        refused.as_str().unwrap().contains("profile.json"),
        "{refused}"
    );

    let report = cos_desktop::report_response(app.handle(), "/");
    assert_eq!(report.status(), 200);
    let html = String::from_utf8_lossy(report.body());
    let goal = demo["goals"][0]["title"].as_str().unwrap();
    assert!(
        html.contains(goal),
        "report does not show the workspace goal"
    );
    assert_eq!(
        cos_desktop::report_response(app.handle(), "/../../etc/passwd").status(),
        404
    );
}
