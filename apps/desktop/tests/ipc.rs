//! The window's commands through Tauri's IPC on the mock runtime, against a
//! stand-in Herdr (cos's `tests/fixtures/fake-hc.sh`, never the real core):
//! hire into Herdr, refuse past the slots or without Herdr, reply, pause,
//! open, mark done, stop, clean, switch between businesses and refuse the
//! switches it must, and serve the report on its own scheme.

#[allow(dead_code)]
#[path = "../../../tests/fake_herdr/mod.rs"]
mod fake_herdr;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use cos::business::{Business, Dirs, Sources};
use cos_desktop::{Checked, Start};
use serde_json::{Value, json};
use tauri::test::{
    INVOKE_KEY, MockRuntime, get_ipc_response, mock_builder, mock_context, noop_assets,
};
use tauri::webview::InvokeRequest;
use tauri::{Manager, WebviewWindowBuilder};

type Win = tauri::WebviewWindow<MockRuntime>;

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

/// A fresh directory for one test, named by the test so parallel tests never share one.
fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("cos-desktop-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d.canonicalize().unwrap()
}

fn cos_bin() -> PathBuf {
    let cos = Path::new(ROOT).join("target/debug/cos");
    assert!(cos.is_file(), "build cos first: cargo build --bin cos");
    cos
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

/// One business's three directories, created and nothing else.
fn dirs(root: &Path, name: &str) -> Business {
    let base = root.join(name);
    let b = Business {
        home: base.join("home"),
        workspace: base.join("ws"),
        projects: base.join("projects"),
    };
    for d in [&b.home, &b.workspace, &b.projects] {
        std::fs::create_dir_all(d).unwrap();
    }
    b
}

/// Initialises a workspace from the demo fixture; `goal` renames its first goal.
fn seed(workspace: &Path, goal: Option<&str>) -> Value {
    cos::state::initialize(workspace).unwrap();
    let mut demo: Value = serde_json::from_str(cos::state::DEMO_STATE).unwrap();
    if let Some(goal) = goal {
        demo["goals"][0]["title"] = json!(goal);
    }
    cos::state::commit(workspace, &demo, 0, "Fictional fixture").unwrap();
    demo
}

fn start(root: &Path, cos: PathBuf, slots: usize, pins: Sources) -> Start {
    let install = root.join("install");
    std::fs::create_dir_all(&install).unwrap();
    Start {
        install: install.canonicalize().unwrap(),
        user_home: Some(root.join("user")),
        registry: root.join("businesses.toml"),
        settings_file: root.join("settings.json"),
        data_dir: root.join("data"),
        log_dir: root.join("logs"),
        ui_state_file: root.join("ui-state.json"),
        dashboards_file: None,
        sources: pins,
        cos_bin: Some(cos),
        slots: Some(slots),
    }
}

fn register(start: &Start, name: &str, b: &Business, active: bool) {
    cos::business::add(&start.registry, name, b.clone(), false).unwrap();
    if active {
        cos::business::set_active(&start.registry, name).unwrap();
    }
}

fn open(start: Start) -> (tauri::App<MockRuntime>, Win) {
    let mut app = cos_desktop::builder(mock_builder(), Some(start))
        .build(mock_context(noop_assets()))
        .unwrap();
    // `build` does not run `setup`; on the mock runtime one iteration runs it and nothing else.
    #[allow(deprecated)]
    app.run_iteration(|_, _| {});
    let w = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    (app, w)
}

/// A hire still being prepared under `home` by `launcher`, created long ago;
/// returns its state file.
fn plant_run(home: &Path, id: &str, launcher: i32) -> PathBuf {
    plant(home, id, Some(launcher), "2020-01-01T00:00:00Z")
}

fn plant(home: &Path, id: &str, launcher: Option<i32>, created: &str) -> PathBuf {
    let dir = home.join("out/agents").join(id);
    std::fs::create_dir_all(&dir).unwrap();
    let spec = json!({
        "id": id, "harness": "claude", "program": "claude", "model": null, "repo": null,
        "base": null, "workdir": dir, "worktree": false, "title": "planted", "source": "test",
        "prompt": "planted", "deadline_secs": 60, "created": created,
        "terminal": format!("cos-{id}"),
    });
    let state = json!({
        "status": "starting", "launcher": launcher,
        "launcher_started": launcher.and_then(cos::agents::started_at),
    });
    std::fs::write(dir.join("spec.json"), spec.to_string()).unwrap();
    std::fs::write(dir.join("state.json"), state.to_string()).unwrap();
    dir.join("state.json")
}

fn call(w: &Win, cmd: &str, args: Value) -> Result<Value, Value> {
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

fn status(w: &Win, id: &str) -> String {
    let list = call(w, "agents_list", json!({})).unwrap();
    list.as_array()
        .unwrap()
        .iter()
        .find(|r| r["spec"]["id"] == id)
        .map(|r| r["state"]["status"].as_str().unwrap().to_owned())
        .unwrap_or_default()
}

fn wait(w: &Win, id: &str, want: &str) {
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

fn row_of(w: &Win, id: &str) -> Value {
    call(w, "agents_list", json!({}))
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["spec"]["id"] == id)
        .cloned()
        .unwrap_or_default()
}

/// Hired from the window for `business`; returns once its Herdr terminal is up.
fn hire(w: &Win, business: &str, args: Value) -> String {
    let specs = call(
        w,
        "agents_spawn",
        json!({ "business": business, "args": args }),
    )
    .unwrap();
    let id = specs[0]["id"].as_str().unwrap().to_owned();
    let until = Instant::now() + Duration::from_secs(20);
    while row_of(w, &id)["state"]["herdr"].is_null() {
        let st = &row_of(w, &id)["state"];
        assert!(
            Instant::now() < until && st["status"] != "failed",
            "never reached Herdr: {st}"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    id
}

fn address(w: &Win, id: &str) -> String {
    row_of(w, id)["state"]["herdr"]["address"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn switch(w: &Win, name: &str) -> Result<Value, Value> {
    call(w, "switch_business", json!({ "args": { "name": name } }))
}

fn error_text(e: &Value) -> &str {
    e.as_str().unwrap()
}

fn hire_args(repo: &Path, extra: Value) -> Value {
    let mut a = json!({
        "harness": "claude", "repo": repo, "worktree": true, "deadlineSecs": 600, "count": 1,
        "program": fake_herdr::agent(),
    });
    a.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    a
}

#[test]
fn the_window_hires_into_herdr_refuses_past_its_slots_or_without_herdr_and_cleans() {
    let cos = cos_bin();
    let root = scratch("spawn");
    let herdr = fake_herdr::fake(&root);
    let d = dirs(&root, "default");
    let demo = seed(&d.workspace, None);
    let (home, ws, projects) = (d.home.clone(), d.workspace.clone(), d.projects.clone());
    let repo = projects.join("demo-repo");
    std::fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    std::fs::write(repo.join("README.md"), "demo\n").unwrap();
    git(&repo, &["add", "README.md"]);
    git(&repo, &["commit", "-q", "-m", "init"]);

    let start = start(&root, cos, 1, Sources::default());
    register(&start, "default", &d, true);
    let (app, w) = open(start);

    let paths = call(&w, "paths", json!({})).unwrap();
    assert_eq!(paths["workspace"], json!(ws));
    assert_eq!(paths["slots"], 1);
    let repos = call(&w, "repos", json!({})).unwrap();
    assert_eq!(repos, json!([repo.to_string_lossy()]));

    let up = call(&w, "herdr_status", json!({})).unwrap();
    assert_eq!(up["up"], true, "{up}");

    let id = hire(
        &w,
        "default",
        hire_args(&repo, json!({ "prompt": "edit", "title": "one" })),
    );
    let row = row_of(&w, &id);
    assert_eq!(row["state"]["status"], "running");
    assert_eq!(row["spec"]["terminal"], format!("cos-{id}"));
    let t = address(&w, &id);
    assert_eq!(herdr.fact(&t, "name"), format!("cos-{id}"));
    assert!(
        row["spec"].get("budget_usd").is_none(),
        "no budget on a hire: {row}"
    );
    let argv = herdr.argv(&t);
    assert!(
        argv.contains(&format!("COS_WORKSPACE={}", ws.display())),
        "the hire is pinned to its business: {argv:?}"
    );

    let past = call(
        &w,
        "agents_spawn",
        json!({ "business": "default", "args": hire_args(&repo, json!({ "prompt": "edit" })) }),
    )
    .unwrap_err();
    assert!(past.as_str().unwrap().contains("all 1 run slots"), "{past}");

    call(&w, "agents_seed", json!({ "id": id })).unwrap();
    assert_eq!(
        call(
            &w,
            "agents_stop",
            json!({ "business": "default", "id": id })
        )
        .unwrap()["status"],
        "killed"
    );
    assert_eq!(herdr.fact(&t, "state"), "finished");
    call(
        &w,
        "agents_clean",
        json!({ "business": "default", "id": id, "force": true }),
    )
    .unwrap();
    assert!(!herdr.exists(&t), "its terminal was removed");
    assert_eq!(call(&w, "agents_list", json!({})).unwrap(), json!([]));

    herdr.down(true);
    let started = call(&w, "herdr_start", json!({})).unwrap();
    assert_eq!(started["up"], true, "Start Herdr brought it up: {started}");
    herdr.down(true);
    std::thread::sleep(Duration::from_secs(4));
    let down = call(&w, "herdr_status", json!({})).unwrap();
    assert_eq!(down["up"], false, "{down}");
    assert!(
        down["why"]
            .as_str()
            .unwrap()
            .contains("Herdr isn't running"),
        "{down}"
    );
    let refused = call(
        &w,
        "agents_spawn",
        json!({ "business": "default", "args": hire_args(&repo, json!({ "prompt": "edit" })) }),
    )
    .unwrap_err();
    let refused = refused.as_str().unwrap();
    assert!(
        refused.contains("Herdr isn't running") && refused.contains("hc server start"),
        "{refused}"
    );
    assert_eq!(call(&w, "agents_list", json!({})).unwrap(), json!([]));
    herdr.down(false);

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

    let org = json!({
        "topics": { "VM SDK": ["demo-repo:README.md"] },
        "links": [],
        "marks": { "demo-repo:README.md": "keep_in_repo" }
    });
    call(
        &w,
        "docs_save",
        json!({ "business": "default", "organize": org }),
    )
    .unwrap();
    let saved = cos::docs::load_organize(&home).unwrap();
    assert_eq!(serde_json::to_value(&saved).unwrap(), org);
    let refused = call(
        &w,
        "docs_save",
        json!({ "business": "default", "organize": { "tpoics": {} } }),
    )
    .unwrap_err();
    assert!(
        refused.as_str().unwrap().contains("unknown field"),
        "{refused}"
    );
    assert_eq!(
        cos::docs::load_organize(&home).unwrap(),
        saved,
        "a bad save changes nothing"
    );

    let profile_path = ws.join(".chief-of-staff/profile.json");
    let mut profile: Value = std::fs::read(&profile_path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_else(|| json!({}));
    profile["monitoredRepos"] = json!(["acme/demo-repo"]);
    std::fs::write(&profile_path, serde_json::to_vec(&profile).unwrap()).unwrap();
    git(
        &repo,
        &[
            "remote",
            "add",
            "origin",
            "git@github.com:acme/demo-repo.git",
        ],
    );
    std::fs::write(
        repo.join("flow.svg"),
        "<svg xmlns=\"http://www.w3.org/2000/svg\"><script>alert(1)</script></svg>",
    )
    .unwrap();
    let req = tauri::http::Request::builder()
        .method("GET")
        .uri("cosreport://localhost/docs/asset?id=demo-repo%3Aflow.svg")
        .body(Vec::new())
        .unwrap();
    let svg = cos_desktop::docs_response(app.handle(), &req);
    let h = svg.headers();
    assert_eq!(svg.status(), 200, "{}", String::from_utf8_lossy(svg.body()));
    assert_eq!(h["content-type"], "image/svg+xml");
    assert_eq!(
        h["content-security-policy"],
        "sandbox; default-src 'none'; style-src 'unsafe-inline'"
    );
    assert_eq!(h["x-content-type-options"], "nosniff");
}

#[test]
fn the_window_hires_by_role_from_a_deliverable_and_reads_the_staff_back() {
    let cos = cos_bin();
    let root = scratch("staff");
    let herdr = fake_herdr::fake(&root);
    herdr.exec(true);
    let d = dirs(&root, "default");
    let demo = seed(&d.workspace, None);
    let repo = d.projects.join("demo-repo");
    std::fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    std::fs::write(repo.join("README.md"), "demo\n").unwrap();
    git(&repo, &["add", "README.md"]);
    git(&repo, &["commit", "-q", "-m", "init"]);

    let start = start(&root, cos, 2, Sources::default());
    register(&start, "default", &d, true);
    let (_app, w) = open(start);

    let staff = call(&w, "agents_roles", json!({})).unwrap();
    let roles = staff["roles"].as_array().unwrap();
    let template: Value = serde_json::from_str(
        &std::fs::read_to_string(Path::new(ROOT).join("templates/context/agents.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        roles.len(),
        template["roles"].as_array().unwrap().len(),
        "{staff}"
    );
    assert_eq!(roles[0]["role"]["id"], "chief-of-staff");
    assert_eq!(roles[0]["role"]["head"], true);
    let reviewer = roles
        .iter()
        .find(|r| r["role"]["id"] == "reviewer")
        .unwrap();
    assert_eq!(reviewer["terms"]["checks"], json!(["test -s REVIEW.md"]));
    assert_eq!(reviewer["role"]["expects"], "file");

    let open = call(&w, "deliverables_open", json!({})).unwrap();
    assert_eq!(open[0]["id"], "D-1");
    assert_eq!(open[0]["goal"], demo["goals"][0]["title"]);
    let summary = call(&w, "brief_summary", json!({})).unwrap();
    assert_eq!(summary["day"], Value::Null, "no brief saved yet");
    assert_eq!(summary["openDeliverables"], 1);

    let refused = call(
        &w,
        "agents_spawn",
        json!({ "business": "default", "args": hire_args(&repo, json!({ "role": "chief-of-staff", "deliverable": "D-1" })) }),
    )
    .unwrap_err();
    assert!(refused.as_str().unwrap().contains("not hired"), "{refused}");

    let id = hire(
        &w,
        "default",
        hire_args(&repo, json!({ "role": "builder", "deliverable": "D-1" })),
    );
    let spec = &row_of(&w, &id)["spec"];
    assert_eq!(spec["role"], "builder");
    assert_eq!(spec["source"], "deliverable D-1");
    assert_eq!(spec["title"], demo["deliverables"][0]["title"]);
    assert_eq!(
        spec["contract"]["done_when"], demo["deliverables"][0]["doneWhen"],
        "the kit's doneWhen is the finish line"
    );
    let prompt = spec["prompt"].as_str().unwrap();
    assert!(prompt.starts_with("Role: Builder."), "{prompt}");
    assert!(prompt.contains("Handoff: D-1"), "{prompt}");

    let verdict = wait_judged(&w, &d.home, &id);
    assert_eq!(verdict["outcome"], "needs_you", "{verdict}");

    let again = call(
        &w,
        "agents_rejudge",
        json!({ "business": "default", "id": id }),
    )
    .unwrap();
    assert_eq!(again["outcome"], "needs_you");
    assert_eq!(again["changed"], json!(["made-by-agent.txt"]));

    let card = call(&w, "agents_scorecard", json!({})).unwrap();
    assert_eq!(card["scores"][0]["role"], "builder");
    assert_eq!(card["scores"][0]["harness"], "claude");
    let role = &card["roles"][0];
    assert_eq!(role["role"], "builder");
    assert_eq!(
        (role["runs"].as_u64(), role["needs_you"].as_u64()),
        (Some(1), Some(1))
    );
    assert_eq!(
        role["median_cost_usd"],
        Value::Null,
        "Herdr measures no cost"
    );
    assert_eq!(role["unmeasured"], 1);

    call(
        &w,
        "agents_done",
        json!({ "business": "default", "id": id }),
    )
    .unwrap();
    call(
        &w,
        "agents_clean",
        json!({ "business": "default", "id": id, "force": true }),
    )
    .unwrap();
}

#[test]
fn a_run_waiting_in_herdr_needs_you_and_the_window_replies_pauses_opens_and_marks_it_done() {
    let cos = cos_bin();
    let root = scratch("herdr-run");
    let herdr = fake_herdr::fake(&root);
    let d = dirs(&root, "default");
    seed(&d.workspace, None);
    let repo = repo_in(&d.projects, "demo-repo");
    let start = start(&root, cos, 2, Sources::default());
    register(&start, "default", &d, true);
    let (app, w) = open(start);

    let id = hire(
        &w,
        "default",
        hire_args(&repo, json!({ "prompt": "edit", "role": "builder" })),
    );
    let t = address(&w, &id);

    herdr.set(&t, "status", "blocked");
    herdr.set(&t, "kind", "permission");
    cos::agents::tick(&d.home).unwrap();
    let row = row_of(&w, &id);
    assert_eq!(
        row["needs"], "waiting on you in Herdr (permission)",
        "{row}"
    );
    assert_eq!(row["state"]["herdr"]["word"], "blocked");
    let blocked = call(
        &w,
        "agents_say",
        json!({ "business": "default", "id": id, "text": "go on" }),
    )
    .unwrap_err();
    assert!(
        blocked.as_str().unwrap().contains("answer it there"),
        "{blocked}"
    );

    let report = cos_desktop::report_response(app.handle(), "/");
    let html = String::from_utf8_lossy(report.body());
    let needs = html
        .find(r#"data-col="needs-you""#)
        .map(|at| &html[at..at + html[at..].find("</ul>").unwrap_or(0)])
        .unwrap_or_default();
    assert!(
        needs.contains("waiting on you"),
        "Today lists it under Needs you"
    );
    assert!(
        html.contains(&format!(r#"data-herdr-open="{id}""#)),
        "with Open in Herdr"
    );

    let shown = call(&w, "agents_open", json!({ "id": id })).unwrap();
    assert!(shown.as_str().unwrap().contains(&t), "{shown}");
    assert!(
        herdr
            .app_log()
            .contains(&format!("agent.focus --terminal {t}"))
    );

    herdr.set(&t, "status", "idle");
    herdr.set(&t, "kind", "");
    cos::agents::tick(&d.home).unwrap();
    assert_eq!(row_of(&w, &id)["needs"], Value::Null);
    let stale = call(
        &w,
        "agents_say",
        json!({ "business": "acme", "id": id, "text": "hi" }),
    )
    .unwrap_err();
    assert!(
        error_text(&stale).contains("nothing written, run or killed"),
        "{stale}"
    );
    call(
        &w,
        "agents_say",
        json!({ "business": "default", "id": id, "text": "Thanks, now finish" }),
    )
    .unwrap();
    assert_eq!(herdr.sent(&t), ["Thanks, now finish"]);
    call(
        &w,
        "agents_pause",
        json!({ "business": "default", "id": id }),
    )
    .unwrap();
    assert!(herdr.calls().contains(&format!("interrupt | {t}")));
    let screen = call(&w, "agents_screen", json!({ "id": id, "lines": 30 })).unwrap();
    assert!(screen.as_str().unwrap().contains(&id), "{screen}");

    let done = call(
        &w,
        "agents_done",
        json!({ "business": "default", "id": id }),
    )
    .unwrap();
    assert_eq!(done["status"], "done");
    assert_eq!(done["judged_on"], "marked done");
    assert!(herdr.calls().contains(&format!("kill | {t}")));
    let kept = call(&w, "agents_screen", json!({ "id": id })).unwrap();
    assert!(
        kept.as_str().unwrap().contains(&id),
        "its last screen is kept: {kept}"
    );
    call(
        &w,
        "agents_clean",
        json!({ "business": "default", "id": id, "force": true }),
    )
    .unwrap();
}

#[test]
fn the_fleet_row_carries_identity_and_clock_and_a_pause_is_one_span() {
    let cos = cos_bin();
    let root = scratch("timeline");
    let herdr = fake_herdr::fake(&root);
    let d = dirs(&root, "default");
    seed(&d.workspace, None);
    let repo = repo_in(&d.projects, "demo-repo");
    let start = start(&root, cos, 2, Sources::default());
    register(&start, "default", &d, true);
    let (_app, w) = open(start);

    let id = hire(
        &w,
        "default",
        hire_args(&repo, json!({ "prompt": "edit", "role": "builder" })),
    );
    let t = address(&w, &id);
    cos::agents::tick(&d.home).unwrap();

    let row = row_of(&w, &id);
    let identity = row["identity"].as_u64().unwrap();
    assert!((1..=6).contains(&identity), "{row}");
    assert!(row["active_ms"].as_u64().is_some(), "{row}");
    assert_eq!(row["paused_now"], false, "{row}");
    assert_eq!(row["done_today"], false, "{row}");
    assert!(
        ["starting", "working", "needs-you"].contains(&row["phase"].as_str().unwrap_or("")),
        "{row}"
    );

    call(
        &w,
        "agents_pause",
        json!({ "business": "default", "id": id }),
    )
    .unwrap();
    assert!(herdr.calls().contains(&format!("interrupt | {t}")));
    assert_eq!(row_of(&w, &id)["paused_now"], true);
    assert_eq!(row_of(&w, &id)["phase"], "paused");

    let snapshot = call(&w, "agents_snapshot", json!({ "business": "default" })).unwrap();
    let hire = snapshot
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["id"] == id.as_str())
        .unwrap_or_else(|| panic!("{snapshot}"))
        .clone();
    assert_eq!(hire["phase"], "paused", "{hire}");
    assert_eq!(hire["paused_now"], true, "{hire}");
    assert!(hire["deadline_secs"].as_u64().is_some(), "{hire}");
    assert!(hire["identity"].as_u64().is_some(), "{hire}");
    for detail in ["spec", "state", "prompt", "workdir"] {
        assert!(
            hire.get(detail).is_none(),
            "a HireView carries no {detail}: {hire}"
        );
    }
    let stale = call(&w, "agents_snapshot", json!({ "business": "acme" })).unwrap_err();
    assert!(!error_text(&stale).is_empty(), "{stale}");
    call(
        &w,
        "agents_say",
        json!({ "business": "default", "id": id, "text": "go on" }),
    )
    .unwrap();
    assert_eq!(row_of(&w, &id)["paused_now"], false);

    let tl = call(
        &w,
        "agents_timeline",
        json!({ "business": "default", "id": id }),
    )
    .unwrap();
    let paused = tl["spans"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|s| s["mode"] == "paused")
        .count();
    assert_eq!(paused, 1, "{tl}");
    assert_eq!(tl["summary"]["paused_now"], false, "{tl}");
}

#[test]
fn the_dialog_checks_fit_before_hiring_and_a_tester_run_takes_your_result() {
    let cos = cos_bin();
    let root = scratch("fit");
    let herdr = fake_herdr::fake(&root);
    herdr.exec(true);
    let d = dirs(&root, "default");
    cos::state::initialize(&d.workspace).unwrap();
    let mut st: Value = serde_json::from_str(cos::state::DEMO_STATE).unwrap();
    st["goals"][0]["whereItLives"] = json!("acme/demo-repo (main)");
    let mut d2 = st["deliverables"][0].clone();
    d2["id"] = json!("D-2");
    d2["title"] = json!("QA-9: Test the published package against the staging video conference");
    d2["doneWhen"] = json!("QA-9 resolved in Jira");
    st["deliverables"].as_array_mut().unwrap().push(d2);
    cos::state::commit(&d.workspace, &st, 0, "Fictional fixture").unwrap();
    let repo = d.projects.join("demo-repo");
    std::fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(
        &repo,
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/acme/demo-repo.git",
        ],
    );
    std::fs::write(repo.join("README.md"), "demo\n").unwrap();
    git(&repo, &["add", "README.md"]);
    git(&repo, &["commit", "-q", "-m", "init"]);

    let start = start(&root, cos, 2, Sources::default());
    register(&start, "default", &d, true);
    let (_app, w) = open(start);
    let fake = fake_herdr::agent();
    let args = |role: &str, extra: Value| {
        let mut a = json!({
            "harness": "claude", "role": role, "worktree": true, "deadlineSecs": 600,
            "count": 1, "program": fake,
        });
        a.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        json!({ "business": "default", "args": a })
    };

    let misfit = call(
        &w,
        "agents_fit",
        args("docs-keeper", json!({ "deliverable": "D-2" })),
    )
    .unwrap();
    assert_eq!(misfit["fit"]["call"], "misfit", "{misfit}");
    assert_eq!(misfit["fit"]["suggest"], "tester");
    assert_eq!(misfit["repo"], json!(repo), "found from the goal");
    let refused = call(
        &w,
        "agents_spawn",
        args("docs-keeper", json!({ "deliverable": "D-2" })),
    )
    .unwrap_err();
    assert!(
        refused
            .as_str()
            .unwrap()
            .contains("hire the Tester instead"),
        "{refused}"
    );

    let ops = call(
        &w,
        "agents_fit",
        args(
            "builder",
            json!({ "prompt": "refresh the dashboard for Friday" }),
        ),
    )
    .unwrap();
    assert_eq!(ops["fit"]["call"], "run_yourself");
    assert_eq!(ops["fit"]["command"], "cos report");

    let researcher = call(
        &w,
        "agents_fit",
        args(
            "researcher",
            json!({ "prompt": "Research the options for offline sync" }),
        ),
    )
    .unwrap();
    assert_eq!(researcher["refused"], Value::Null, "{researcher}");
    assert_eq!(researcher["scratch_ok"], true);

    let fit = call(
        &w,
        "agents_fit",
        args("tester", json!({ "deliverable": "D-2" })),
    )
    .unwrap();
    assert_eq!(fit["fit"]["call"], "fit", "{fit}");
    assert_eq!(fit["contract"]["finish_line"], "QA-9 resolved in Jira");
    assert!(
        fit["contract"]["done_when"]
            .as_str()
            .unwrap()
            .starts_with("A draft ready for you to resolve QA-9"),
        "{fit}"
    );

    // The stand-in agent writes TEST-PLAN.md only when its prompt says
    // TESTPLAN, so this hire is an own prompt rather than the deliverable.
    let specs = call(
        &w,
        "agents_spawn",
        args(
            "tester",
            json!({ "prompt": "QA-9: TESTPLAN and READONLY for the staging conference", "repo": repo }),
        ),
    )
    .unwrap();
    let id = specs[0]["id"].as_str().unwrap().to_owned();
    assert_eq!(specs[0]["role"], "tester");
    let row = |w: &Win| row_of(w, &id);
    let verdict = wait_judged(&w, &d.home, &id);
    assert_eq!(verdict["outcome"], "accepted", "{verdict}");
    assert_eq!(verdict["changed"], json!(["TEST-PLAN.md"]));

    let st = call(
        &w,
        "agents_record",
        json!({ "business": "default", "args": { "id": id, "outcome": "passed",
                          "notes": "12 of 12 steps passed",
                          "evidence": ["https://ci.example/run/7"] } }),
    )
    .unwrap();
    assert_eq!(st["recorded"]["outcome"], "passed");
    assert_eq!(
        row(&w)["state"]["recorded"]["notes"],
        "12 of 12 steps passed"
    );
    call(
        &w,
        "agents_done",
        json!({ "business": "default", "id": id }),
    )
    .unwrap();
    call(
        &w,
        "agents_clean",
        json!({ "business": "default", "id": id, "force": true }),
    )
    .unwrap();
}

#[test]
fn switch_changes_paths_report_flows() {
    let cos = cos_bin();
    let root = scratch("switch");
    let a = dirs(&root, "default");
    let demo = seed(&a.workspace, None);
    let b = dirs(&root, "acme");
    seed(&b.workspace, Some("Acme launch goal"));

    let start = start(&root, cos, 2, Sources::default());
    register(&start, "default", &a, true);
    register(&start, "acme", &b, false);
    let (app, w) = open(start);

    let before = call(&w, "paths", json!({})).unwrap();
    assert_eq!(before["business"], "default");
    assert_eq!(before["workspace"], json!(a.workspace));
    let report = cos_desktop::report_response(app.handle(), "/");
    let html = String::from_utf8_lossy(report.body()).into_owned();
    assert!(html.contains(demo["goals"][0]["title"].as_str().unwrap()));

    switch(&w, "acme").unwrap();

    let after = call(&w, "paths", json!({})).unwrap();
    assert_eq!(after["business"], "acme");
    assert_eq!(after["home"], json!(b.home));
    assert_eq!(after["workspace"], json!(b.workspace));
    assert_eq!(after["projects"], json!(b.projects));
    let report = cos_desktop::report_response(app.handle(), "/");
    let html = String::from_utf8_lossy(report.body()).into_owned();
    assert!(html.contains("Acme launch goal"), "report did not switch");
    assert_eq!(
        cos_desktop::flows_response(app.handle(), None).status(),
        200
    );

    let rows = call(&w, "businesses", json!({})).unwrap();
    let acme = rows
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == "acme")
        .unwrap();
    assert_eq!(acme["app"], true, "{rows}");
    assert_eq!(
        acme["active"], false,
        "switching does not change the registry"
    );
}

#[test]
fn switch_refusals_keep_triple() {
    let cos = cos_bin();
    let root = scratch("refusals");
    let a = dirs(&root, "default");
    seed(&a.workspace, None);
    let bare = dirs(&root, "bare");
    let dead = dirs(&root, "dead");
    seed(&dead.workspace, None);
    let acme = dirs(&root, "acme");
    seed(&acme.workspace, Some("Acme launch goal"));

    let start = start(&root, cos, 2, Sources::default());
    register(&start, "default", &a, true);
    register(&start, "bare", &bare, false);
    register(&start, "dead", &dead, false);
    register(&start, "acme", &acme, false);
    std::fs::remove_dir_all(&dead.workspace).unwrap();
    let (_app, w) = open(start.clone());

    let unknown = switch(&w, "nope").unwrap_err();
    assert!(error_text(&unknown).contains("nope"), "{unknown}");
    let gone = switch(&w, "dead").unwrap_err();
    assert!(error_text(&gone).contains("is not a directory"), "{gone}");
    let no_profile = switch(&w, "bare").unwrap_err();
    assert!(
        error_text(&no_profile).contains("state init"),
        "{no_profile}"
    );
    let extra = call(
        &w,
        "switch_business",
        json!({ "args": { "name": "acme", "home": "/tmp" } }),
    )
    .unwrap_err();
    assert!(error_text(&extra).contains("unknown field"), "{extra}");
    assert_eq!(call(&w, "paths", json!({})).unwrap()["business"], "default");

    let (_app2, w2) = open(pinned(&start, &a));
    let paths = call(&w2, "paths", json!({})).unwrap();
    assert_eq!(paths["business"], "explicit");
    assert_eq!(paths["pinned"], "COS_HOME");
    let pinned = switch(&w2, "acme").unwrap_err();
    assert!(
        error_text(&pinned).contains("pinned by COS_HOME"),
        "{pinned}"
    );
    assert_eq!(
        call(&w2, "paths", json!({})).unwrap()["home"],
        json!(a.home)
    );
}

/// The same files as `base`, started with `COS_HOME` set to `home` in the env layer.
fn pinned(base: &Start, home: &Business) -> Start {
    let mut s = base.clone();
    s.sources = Sources {
        dirs: Dirs {
            home: Some(home.home.clone()),
            ..Default::default()
        },
        ..Default::default()
    };
    s
}

#[test]
fn businesses_leaves_other_homes_untouched() {
    let cos = cos_bin();
    let root = scratch("others");
    let a = dirs(&root, "default");
    seed(&a.workspace, None);
    let other = dirs(&root, "other");
    let state = plant_run(&other.home, "run-dead", 999_999);
    let before = std::fs::read(&state).unwrap();

    let start = start(&root, cos, 2, Sources::default());
    register(&start, "default", &a, true);
    register(&start, "other", &other, false);
    let (_app, w) = open(start);

    let rows = call(&w, "businesses", json!({})).unwrap();
    let other_row = rows
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == "other")
        .unwrap();
    assert_eq!(
        (
            other_row["live"].as_u64(),
            other_row["lost"].as_u64(),
            other_row["unreadable"].as_u64()
        ),
        (Some(0), Some(1), Some(0)),
        "{rows}"
    );
    assert_eq!(
        std::fs::read(&state).unwrap(),
        before,
        "the other home's record changed"
    );
}

#[test]
fn businesses_marks_missing_home() {
    let cos = cos_bin();
    let root = scratch("missing");
    let a = dirs(&root, "default");
    seed(&a.workspace, None);
    let gone = dirs(&root, "gone");

    let start = start(&root, cos, 2, Sources::default());
    register(&start, "default", &a, true);
    register(&start, "gone", &gone, false);
    std::fs::remove_dir_all(&gone.home).unwrap();
    let (_app, w) = open(start);

    let rows = call(&w, "businesses", json!({})).unwrap();
    let row = rows
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == "gone")
        .unwrap();
    assert_eq!(row["missing"], true, "{rows}");
    assert_eq!(row["dirs"]["home"], false);
    assert!(
        row.get("live").is_none(),
        "a missing home shows no count: {row}"
    );
}

#[test]
fn paths_and_businesses_shape() {
    let cos = cos_bin();
    let root = scratch("shape");
    let a = dirs(&root, "default");
    seed(&a.workspace, None);
    let other = dirs(&root, "other");
    let gone = dirs(&root, "gone");

    let start = start(&root, cos, 2, Sources::default());
    register(&start, "default", &a, true);
    register(&start, "other", &other, false);
    register(&start, "gone", &gone, false);
    std::fs::remove_dir_all(&gone.home).unwrap();
    let (_app, w) = open(start);

    let paths = call(&w, "paths", json!({})).unwrap();
    assert_eq!(paths["business"], "default", "{paths}");
    for key in ["home", "workspace", "projects"] {
        assert!(paths[key].is_string(), "paths.{key} missing: {paths}");
    }

    let rows = call(&w, "businesses", json!({})).unwrap();
    let rows = rows.as_array().unwrap();
    assert_eq!(rows.len(), 3, "{rows:?}");
    for row in rows {
        for key in ["name", "triple", "active", "app", "dirs", "missing"] {
            assert!(row.get(key).is_some(), "{key} missing in {row}");
        }
        assert!(row["triple"]["home"].is_string(), "{row}");
        if row["missing"] == true {
            for key in ["live", "lost", "unreadable"] {
                assert!(row.get(key).is_none(), "missing home carries {key}: {row}");
            }
        } else {
            for key in ["live", "lost", "unreadable"] {
                assert!(row[key].is_u64(), "{key} missing in {row}");
            }
        }
    }
    let by_name = |name: &str| rows.iter().find(|r| r["name"] == name).unwrap();
    assert_eq!(by_name("default")["app"], true);
    assert_eq!(by_name("default")["active"], true);
    assert_eq!(by_name("other")["app"], false);
    assert_eq!(by_name("gone")["missing"], true);
}

/// Writes `home/out/briefs/<day>.json` and returns its mtime in ms.
fn write_brief(home: &Path, day: &str) -> i64 {
    let dir = home.join("out/briefs");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join(format!("{day}.json"));
    std::fs::write(&file, "{}").unwrap();
    cos::signals::mtime_ms(&std::fs::metadata(&file).unwrap())
}

/// The `cos.brief` plist under `user_home`, for `business` on `registry`.
fn brief_plist(user_home: &Path, business: &str, registry: &Path) {
    let path = user_home.join("Library/LaunchAgents/cos.brief.plist");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        &path,
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
             <plist version=\"1.0\"><dict>\n\
             <key>Label</key><string>cos.brief</string>\n\
             <key>ProgramArguments</key><array><string>/usr/local/bin/cos</string>\
             <string>--business</string><string>{business}</string><string>brief</string></array>\n\
             <key>EnvironmentVariables</key><dict>\
             <key>COS_BUSINESSES</key><string>{}</string></dict>\n\
             </dict></plist>\n",
            registry.display()
        ),
    )
    .unwrap();
}

/// `(unread, scheduled)` per business, from the `businesses` command.
fn row_flags(w: &Win) -> BTreeMap<String, (Value, Value)> {
    let rows = call(w, "businesses", json!({})).unwrap();
    rows.as_array()
        .unwrap()
        .iter()
        .map(|r| {
            (
                r["name"].as_str().unwrap().to_owned(),
                (r["unread"].clone(), r["scheduled"].clone()),
            )
        })
        .collect()
}

#[test]
fn businesses_rows_carry_unread_and_scheduled() {
    let cos = cos_bin();
    let root = scratch("unread");
    let a = dirs(&root, "default");
    seed(&a.workspace, None);
    let seen = dirs(&root, "seen");
    let clock = dirs(&root, "clock");
    write_brief(&a.home, "2026-11-14");
    let mtime = write_brief(&seen.home, "2026-11-14");

    let start = start(&root, cos, 2, Sources::default());
    register(&start, "default", &a, true);
    register(&start, "seen", &seen, false);
    register(&start, "clock", &clock, false);
    std::fs::write(
        &start.ui_state_file,
        json!({ "lastSeenBrief": { "seen": { "file": "2026-11-14.json", "mtime": mtime } } })
            .to_string(),
    )
    .unwrap();
    brief_plist(
        start.user_home.as_deref().unwrap(),
        "clock",
        &start.registry,
    );

    let (_app, w) = open(start.clone());
    let want = |d: bool, s: bool, c: bool| {
        BTreeMap::from([
            ("clock".to_owned(), (json!(false), json!(c))),
            ("default".to_owned(), (json!(d), json!(false))),
            ("seen".to_owned(), (json!(s), json!(false))),
        ])
    };
    assert_eq!(
        row_flags(&w),
        want(true, false, true),
        "a brief with no stored pair is unread; the plist's business is scheduled"
    );

    let mut bare = start;
    bare.user_home = None;
    let (_bare_app, w) = open(bare);
    assert_eq!(
        row_flags(&w),
        want(true, false, false),
        "a Start with no user home gives no clock"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn businesses_spawns_no_ps() {
    use std::os::unix::fs::PermissionsExt;
    use std::process::Stdio;

    /// Set only on the child run of this case, never in this process's environment.
    const NO_PS_ROOT: &str = "COS_DESKTOP_NO_PS_ROOT";
    const NO_PS_PID: &str = "COS_DESKTOP_NO_PS_PID";

    if let Some(root) = std::env::var_os(NO_PS_ROOT) {
        let pid: u64 = std::env::var(NO_PS_PID).unwrap().parse().unwrap();
        let root = PathBuf::from(root);
        let (_app, w) = open(start(&root, cos_bin(), 2, Sources::default()));
        let rows = call(&w, "businesses", json!({})).unwrap();
        let row = rows
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["name"] == "default")
            .unwrap();
        assert_eq!(row["live"].as_u64(), Some(1), "launcher {pid}: {rows}");
        assert!(row["unread"].is_boolean(), "{row}");
        assert!(row["scheduled"].is_boolean(), "{row}");
        return;
    }

    struct Reap(std::process::Child);
    impl Drop for Reap {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    let cos = cos_bin();
    let root = scratch("no-ps");
    let a = dirs(&root, "default");
    seed(&a.workspace, None);
    let start = start(&root, cos.clone(), 2, Sources::default());
    register(&start, "default", &a, true);

    let sup_home = root.join("launcher");
    std::fs::create_dir_all(&sup_home).unwrap();
    let launcher = Reap(
        Command::new(&cos)
            .arg("--home")
            .arg(&sup_home)
            .arg("--workspace")
            .arg(&sup_home)
            .arg("--projects")
            .arg(&sup_home)
            .arg("mcp")
            .env_remove("COS_HOME")
            .env_remove("COS_WORKSPACE")
            .env_remove("COS_PROJECTS")
            .env_remove("COS_BUSINESS")
            .env_remove("XDG_CONFIG_HOME")
            .env("HOME", root.join("user"))
            .env("COS_BUSINESSES", root.join("none.toml"))
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let pid = launcher.0.id() as i32;
    let until = Instant::now() + Duration::from_secs(10);
    while !cos::proc::exe(pid).is_some_and(|e| e.file_name().is_some_and(|f| f == "cos")) {
        assert!(Instant::now() < until, "cos {pid} never ran");
        std::thread::sleep(Duration::from_millis(20));
    }
    plant_run(&a.home, "live", pid);

    let bin = root.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let marker = root.join("ps-ran");
    let ps = bin.join("ps");
    std::fs::write(
        &ps,
        format!("#!/bin/sh\ntouch '{}'\nexit 1\n", marker.display()),
    )
    .unwrap();
    std::fs::set_permissions(&ps, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut path = std::ffi::OsString::from(&bin);
    if let Some(rest) = std::env::var_os("PATH") {
        path.push(":");
        path.push(rest);
    }

    let out = Command::new(std::env::current_exe().unwrap())
        .args([
            "businesses_spawns_no_ps",
            "--exact",
            "--test-threads=1",
            "--nocapture",
        ])
        .env("PATH", &path)
        .env(NO_PS_ROOT, &root)
        .env(NO_PS_PID, pid.to_string())
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.status.success(), "child run failed: {text}");
    assert!(text.contains("1 passed"), "child ran no case: {text}");
    assert!(!marker.exists(), "businesses spawned ps");
}

#[test]
fn first_start_writes_default_and_hints_legacy() {
    let cos = cos_bin();
    let root = scratch("first");
    let legacy = dirs(&root, "legacy");
    let start = start(&root, cos, 2, Sources::default());
    std::fs::write(
        &start.settings_file,
        json!({
            "home": legacy.home, "workspace": legacy.workspace,
            "projects": legacy.projects, "slots": 2,
        })
        .to_string(),
    )
    .unwrap();
    assert!(!start.registry.exists());
    let install = start.install.clone();
    let (_app, w) = open(start.clone());

    let reg = cos::business::load(&start.registry).unwrap();
    assert_eq!(reg.active.as_deref(), Some("default"));
    assert_eq!(reg.businesses["default"].home, install);
    let paths = call(&w, "paths", json!({})).unwrap();
    assert_eq!(paths["business"], "default");
    assert_eq!(paths["home"], json!(install));
    let hint = paths["legacyHint"].as_str().unwrap();
    assert!(hint.contains("cos business add"), "{paths}");
    assert!(hint.contains(legacy.home.to_str().unwrap()), "{paths}");
}

#[test]
fn corrupt_registry_runs_on_env_or_defaults() {
    let cos = cos_bin();
    let root = scratch("corrupt");
    let a = dirs(&root, "default");
    seed(&a.workspace, None);
    let start = start(&root, cos.clone(), 2, Sources::default());
    register(&start, "default", &a, true);
    std::fs::write(&start.registry, "active = [oops\n").unwrap();

    let (app, w) = open(start.clone());
    let paths = call(&w, "paths", json!({})).unwrap();
    assert_eq!(paths["business"], "default");
    assert_eq!(paths["home"], json!(start.install));
    assert!(
        paths["startError"]
            .as_str()
            .unwrap()
            .contains("businesses.toml"),
        "{paths}"
    );
    let picker = call(&w, "businesses", json!({})).unwrap_err();
    assert!(error_text(&picker).contains("businesses.toml"), "{picker}");
    drop(app);

    let (_app2, w2) = open(pinned(&start, &a));
    let paths = call(&w2, "paths", json!({})).unwrap();
    assert_eq!(paths["business"], "explicit");
    assert_eq!(paths["home"], json!(a.home));
    assert!(
        paths["startError"]
            .as_str()
            .unwrap()
            .contains("businesses.toml"),
        "{paths}"
    );
}

#[test]
fn corrupt_settings_reported() {
    let cos = cos_bin();
    let root = scratch("settings");
    let a = dirs(&root, "default");
    seed(&a.workspace, None);
    let start = start(&root, cos, 2, Sources::default());
    register(&start, "default", &a, true);
    std::fs::write(&start.settings_file, "{ not json").unwrap();

    let (_app, w) = open(start);
    let paths = call(&w, "paths", json!({})).unwrap();
    assert!(
        paths["settingsError"]
            .as_str()
            .unwrap()
            .contains("settings.json"),
        "{paths}"
    );
    assert_eq!(paths["business"], "default", "the run still starts");
}

#[test]
fn settings_never_writes_triple() {
    let cos = cos_bin();
    let root = scratch("never");
    let a = dirs(&root, "default");
    seed(&a.workspace, None);
    let b = dirs(&root, "acme");
    seed(&b.workspace, Some("Acme launch goal"));
    let start = start(&root, cos, 2, Sources::default());
    register(&start, "default", &a, true);
    register(&start, "acme", &b, false);
    let saved = json!({ "slots": 2 }).to_string();
    std::fs::write(&start.settings_file, &saved).unwrap();
    let (_app, w) = open(start.clone());

    switch(&w, "acme").unwrap();

    assert_eq!(
        std::fs::read_to_string(&start.settings_file).unwrap(),
        saved
    );
    let text = std::fs::read_to_string(&start.settings_file).unwrap();
    let file: Value = serde_json::from_str(&text).unwrap();
    for key in ["home", "workspace", "projects"] {
        assert!(file.get(key).is_none(), "{key} written to settings.json");
    }
}

#[test]
fn businesses_under_200ms() {
    let cos = cos_bin();
    let root = scratch("speed");
    let a = dirs(&root, "default");
    seed(&a.workspace, None);
    let start = start(&root, cos, 2, Sources::default());
    register(&start, "default", &a, true);
    for i in 0..10 {
        let b = dirs(&root, &format!("biz{i}"));
        for j in 0..50 {
            plant_run(&b.home, &format!("run-{j:02}"), 999_999);
        }
        register(&start, &format!("biz{i}"), &b, false);
    }
    let (_app, w) = open(start);

    // Best of three: one sample taken under parallel test load measures the scheduler.
    let mut took = Duration::MAX;
    let mut rows = json!(null);
    for _ in 0..3 {
        let began = Instant::now();
        rows = call(&w, "businesses", json!({})).unwrap();
        took = took.min(began.elapsed());
    }
    eprintln!("businesses() over 10 businesses x 50 runs: {took:?}");
    assert_eq!(rows.as_array().unwrap().len(), 11);
    assert!(
        took < Duration::from_millis(200),
        "businesses() took {took:?}"
    );
    let biz0 = rows
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == "biz0")
        .unwrap();
    assert_eq!(biz0["lost"], 50, "{rows}");
}

/// Every regular file under `dir`, by path, so a refused call shows as no change.
fn tree(dir: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn walk(dir: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if let Ok(bytes) = std::fs::read(&p) {
                out.insert(p, bytes);
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(dir, &mut out);
    out
}

/// A one-commit git repo named `name` under `projects`.
fn repo_in(projects: &Path, name: &str) -> PathBuf {
    let repo = projects.join(name);
    std::fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    std::fs::write(repo.join("README.md"), "demo\n").unwrap();
    git(&repo, &["add", "README.md"]);
    git(&repo, &["commit", "-q", "-m", "init"]);
    repo
}

fn spawn_args(repo: &Path, prompt: &str) -> Value {
    hire_args(repo, json!({ "prompt": prompt }))
}

/// Follows `home`'s hires until `id` is judged: the stand-in agent says done
/// at its first turn, so one look is usually enough.
fn wait_judged(w: &Win, home: &Path, id: &str) -> Value {
    let until = Instant::now() + Duration::from_secs(20);
    loop {
        cos::agents::tick(home).unwrap();
        let v = row_of(w, id)["state"]["verdict"].clone();
        if !v.is_null() {
            return v;
        }
        assert!(Instant::now() < until, "{id} never judged");
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn post_organize(
    app: &tauri::AppHandle<MockRuntime>,
    business: Option<&str>,
    body: &Value,
) -> tauri::http::Response<Vec<u8>> {
    organize_request(app, "POST", business, Some("tauri://localhost"), body)
}

fn organize_request(
    app: &tauri::AppHandle<MockRuntime>,
    method: &str,
    business: Option<&str>,
    origin: Option<&str>,
    body: &Value,
) -> tauri::http::Response<Vec<u8>> {
    let mut b = tauri::http::Request::builder()
        .method(method)
        .uri("cosreport://localhost/docs/organize")
        .header("x-cos", "docs");
    if let Some(name) = business {
        b = b.header("x-cos-business", name);
    }
    if let Some(o) = origin {
        b = b.header("origin", o);
    }
    let req = b.body(body.to_string().into_bytes()).unwrap();
    cos_desktop::cosreport_response(app, &req)
}

fn get_root(
    app: &tauri::AppHandle<MockRuntime>,
    origin: Option<&str>,
) -> tauri::http::Response<Vec<u8>> {
    let mut b = tauri::http::Request::builder()
        .method("GET")
        .uri("cosreport://localhost/");
    if let Some(o) = origin {
        b = b.header("origin", o);
    }
    cos_desktop::cosreport_response(app, &b.body(Vec::new()).unwrap())
}

#[test]
fn cosreport_post_needs_an_app_origin() {
    let cos = cos_bin();
    let root = scratch("origin-post");
    let a = dirs(&root, "default");
    seed(&a.workspace, None);
    let start = start(&root, cos, 2, Sources::default());
    register(&start, "default", &a, true);
    let (app, _w) = open(start);
    let org = json!({ "topics": {}, "links": [], "marks": {} });
    let before = tree(&a.home);

    for origin in [Some("http://127.0.0.1:4777"), Some("null"), None] {
        let r = organize_request(app.handle(), "POST", Some("default"), origin, &org);
        let body = String::from_utf8_lossy(r.body()).into_owned();
        assert_eq!(r.status(), 403, "{origin:?}: {body}");
        assert!(
            !body.contains("missing or stale X-Cos-Business header"),
            "{origin:?}: {body}"
        );
    }
    assert_eq!(tree(&a.home), before, "a refused POST wrote");

    for origin in [
        "tauri://localhost",
        "http://tauri.localhost",
        "cosreport://localhost",
        "http://cosreport.localhost",
    ] {
        let r = organize_request(app.handle(), "POST", Some("default"), Some(origin), &org);
        assert_eq!(
            r.status(),
            200,
            "{origin}: {}",
            String::from_utf8_lossy(r.body())
        );
    }
}

#[test]
fn cosreport_get_and_other_methods_follow_the_origin_rule() {
    let cos = cos_bin();
    let root = scratch("origin-get");
    let a = dirs(&root, "default");
    seed(&a.workspace, None);
    let start = start(&root, cos, 2, Sources::default());
    register(&start, "default", &a, true);
    let (app, _w) = open(start);
    let org = json!({ "topics": {}, "links": [], "marks": {} });

    let framed = get_root(app.handle(), Some("http://127.0.0.1:4777"));
    assert_eq!(
        framed.status(),
        403,
        "{}",
        String::from_utf8_lossy(framed.body())
    );
    for origin in [Some("null"), None, Some("tauri://localhost")] {
        let r = get_root(app.handle(), origin);
        assert_eq!(
            r.status(),
            200,
            "{origin:?}: {}",
            String::from_utf8_lossy(r.body())
        );
    }

    let before = tree(&a.home);
    for method in ["OPTIONS", "PUT", "DELETE"] {
        for origin in [Some("http://127.0.0.1:4777"), None] {
            let r = organize_request(app.handle(), method, Some("default"), origin, &org);
            let body = String::from_utf8_lossy(r.body()).into_owned();
            assert_eq!(r.status(), 403, "{method} {origin:?}: {body}");
            assert!(
                !body.contains("missing or stale X-Cos-Business header"),
                "{method} {origin:?}: {body}"
            );
        }
    }
    assert_eq!(tree(&a.home), before, "a refused request wrote");
}

#[test]
fn writes_land_in_new_business() {
    let cos = cos_bin();
    let root = scratch("writes");
    let herdr = fake_herdr::fake(&root);
    herdr.exec(true);
    let a = dirs(&root, "default");
    seed(&a.workspace, None);
    let b = dirs(&root, "acme");
    seed(&b.workspace, Some("Acme launch goal"));
    let repo = repo_in(&b.projects, "acme-repo");

    let start = start(&root, cos, 2, Sources::default());
    register(&start, "default", &a, true);
    register(&start, "acme", &b, false);
    let (app, w) = open(start);
    switch(&w, "acme").unwrap();
    let default_before = serde_json::to_value(cos::docs::load_organize(&a.home).unwrap()).unwrap();

    let org = json!({ "topics": { "Acme": ["acme-repo:README.md"] }, "links": [], "marks": {} });
    call(
        &w,
        "docs_save",
        json!({ "business": "acme", "organize": org }),
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(cos::docs::load_organize(&b.home).unwrap()).unwrap(),
        org
    );

    let org2 =
        json!({ "topics": { "Acme docs": ["acme-repo:README.md"] }, "links": [], "marks": {} });
    let posted = post_organize(app.handle(), Some("acme"), &org2);
    assert_eq!(
        posted.status(),
        200,
        "{}",
        String::from_utf8_lossy(posted.body())
    );
    assert_eq!(
        serde_json::to_value(cos::docs::load_organize(&b.home).unwrap()).unwrap(),
        org2
    );

    let id = hire(&w, "acme", spawn_args(&repo, "acme prompt"));
    assert!(
        b.home
            .join("out/agents")
            .join(&id)
            .join("spec.json")
            .is_file(),
        "the run is not under acme"
    );
    assert!(!a.home.join("out/agents").join(&id).exists());

    let verdict = wait_judged(&w, &b.home, &id);
    assert!(!verdict.is_null());
    let st = call(
        &w,
        "agents_record",
        json!({ "business": "acme", "args": { "id": id, "outcome": "passed", "notes": "checked" } }),
    )
    .unwrap();
    assert_eq!(st["recorded"]["outcome"], "passed");
    assert!(
        cos::agents::read(&b.home, &id)
            .unwrap()
            .1
            .recorded
            .is_some(),
        "the record is not under acme"
    );

    assert_eq!(
        serde_json::to_value(cos::docs::load_organize(&a.home).unwrap()).unwrap(),
        default_before,
        "default's organize changed"
    );
    call(&w, "agents_done", json!({ "business": "acme", "id": id })).unwrap();
    call(
        &w,
        "agents_clean",
        json!({ "business": "acme", "id": id, "force": true }),
    )
    .unwrap();
}

#[test]
fn stale_business_writes_refused() {
    let cos = cos_bin();
    let root = scratch("stale");
    let a = dirs(&root, "default");
    seed(&a.workspace, None);
    let repo = repo_in(&a.projects, "demo-repo");
    let b = dirs(&root, "acme");
    seed(&b.workspace, Some("Acme launch goal"));

    let start = start(&root, cos, 2, Sources::default());
    register(&start, "default", &a, true);
    register(&start, "acme", &b, false);
    let (app, w) = open(start);
    plant_run(&a.home, "run-a", 999_999);
    plant_run(&b.home, "run-b", 999_999);
    switch(&w, "acme").unwrap();
    let (before_a, before_b) = (tree(&root.join("default")), tree(&root.join("acme")));

    let org = json!({ "topics": { "Stale": ["demo-repo:README.md"] }, "links": [], "marks": {} });
    let refused = [
        (
            "docs_save",
            json!({ "business": "default", "organize": org }),
        ),
        (
            "agents_spawn",
            json!({ "business": "default", "args": spawn_args(&repo, "stale") }),
        ),
        (
            "agents_record",
            json!({ "business": "default", "args": { "id": "run-a", "outcome": "passed", "notes": "stale" } }),
        ),
        (
            "agents_rejudge",
            json!({ "business": "default", "id": "run-a" }),
        ),
        (
            "agents_stop",
            json!({ "business": "default", "id": "run-a" }),
        ),
        (
            "agents_clean",
            json!({ "business": "default", "id": "run-a", "force": true }),
        ),
        (
            "agents_say",
            json!({ "business": "default", "id": "run-a", "text": "stale" }),
        ),
        (
            "agents_pause",
            json!({ "business": "default", "id": "run-a" }),
        ),
        (
            "agents_done",
            json!({ "business": "default", "id": "run-a" }),
        ),
        ("brief_run", json!({ "business": "default" })),
        (
            "ritual_approve",
            json!({ "business": "default", "repo": "demo-repo", "epic": "t-x",
                    "design": "docs/d.md", "review": null, "without_review": true }),
        ),
    ];
    for (cmd, args) in refused {
        let e = call(&w, cmd, args).unwrap_err();
        assert!(
            error_text(&e).contains("nothing written, run or killed"),
            "{cmd}: {e}"
        );
    }
    let posted = post_organize(app.handle(), Some("default"), &org);
    assert_eq!(
        posted.status(),
        403,
        "{}",
        String::from_utf8_lossy(posted.body())
    );

    assert_eq!(tree(&root.join("default")), before_a, "default changed");
    assert_eq!(tree(&root.join("acme")), before_b, "acme changed");
}

#[test]
fn docs_post_checks_business_header() {
    let cos = cos_bin();
    let root = scratch("header");
    let a = dirs(&root, "default");
    seed(&a.workspace, None);
    let start = start(&root, cos, 2, Sources::default());
    register(&start, "default", &a, true);
    let (app, _w) = open(start);
    let org = json!({ "topics": {}, "links": [], "marks": {} });
    let before = tree(&a.home);

    let missing = post_organize(app.handle(), None, &org);
    assert_eq!(missing.status(), 403);
    assert!(
        String::from_utf8_lossy(missing.body()).contains("X-Cos-Business"),
        "{:?}",
        missing.body()
    );
    let mismatched = post_organize(app.handle(), Some("acme"), &org);
    assert_eq!(mismatched.status(), 403);
    assert_eq!(tree(&a.home), before, "a refused save wrote");

    let matched = post_organize(app.handle(), Some("default"), &org);
    assert_eq!(
        matched.status(),
        200,
        "{}",
        String::from_utf8_lossy(matched.body())
    );
}

#[test]
fn app_run_commands_refuse_bad_ids() {
    let cos = cos_bin();
    let root = scratch("badids");
    let a = dirs(&root, "default");
    seed(&a.workspace, None);
    let b = dirs(&root, "acme");
    seed(&b.workspace, Some("Acme launch goal"));

    let start = start(&root, cos, 2, Sources::default());
    register(&start, "default", &a, true);
    register(&start, "acme", &b, false);
    let (_app, w) = open(start);
    plant_run(&a.home, "run-a", 999_999);
    plant_run(&b.home, "run-b", 999_999);
    let swapped = plant_run(&a.home, "run-c", 999_999);
    let spec = swapped.parent().unwrap().join("spec.json");
    let mut doc: Value = serde_json::from_slice(&std::fs::read(&spec).unwrap()).unwrap();
    doc["id"] = json!("run-x");
    std::fs::write(&spec, doc.to_string()).unwrap();
    let before = (tree(&root.join("default")), tree(&root.join("acme")));

    for id in ["", ".", "..", "../x", "a/b", "run-b", "run-c"] {
        let fails = [
            call(
                &w,
                "agents_rejudge",
                json!({ "business": "default", "id": id }),
            ),
            call(
                &w,
                "agents_stop",
                json!({ "business": "default", "id": id }),
            ),
            call(
                &w,
                "agents_clean",
                json!({ "business": "default", "id": id, "force": true }),
            ),
            call(
                &w,
                "agents_record",
                json!({ "business": "default", "args": { "id": id, "outcome": "passed", "notes": "x" } }),
            ),
            call(&w, "agents_seed", json!({ "id": id })),
            call(&w, "agents_screen", json!({ "id": id })),
            call(&w, "agents_open", json!({ "id": id })),
            call(
                &w,
                "agents_say",
                json!({ "business": "default", "id": id, "text": "x" }),
            ),
            call(
                &w,
                "agents_done",
                json!({ "business": "default", "id": id }),
            ),
        ];
        for r in fails {
            assert!(r.is_err(), "id {id:?} accepted: {r:?}");
        }
    }
    assert_eq!(
        (tree(&root.join("default")), tree(&root.join("acme"))),
        before,
        "a refused id changed a run"
    );
}

#[test]
fn run_spawned_before_switch_survives() {
    let cos = cos_bin();
    let root = scratch("survives");
    let _herdr = fake_herdr::fake(&root);
    let a = dirs(&root, "default");
    seed(&a.workspace, None);
    let repo = repo_in(&a.projects, "demo-repo");
    let b = dirs(&root, "acme");
    seed(&b.workspace, Some("Acme launch goal"));

    let start = start(&root, cos, 2, Sources::default());
    register(&start, "default", &a, true);
    register(&start, "acme", &b, false);
    let (_app, w) = open(start);

    let id = hire(&w, "default", spawn_args(&repo, "keep working"));
    wait(&w, &id, "running");

    switch(&w, "acme").unwrap();
    assert_eq!(status(&w, &id), "", "acme lists default's run");

    switch(&w, "default").unwrap();
    assert_eq!(
        status(&w, &id),
        "running",
        "the run did not survive the switch"
    );

    assert_eq!(
        call(
            &w,
            "agents_stop",
            json!({ "business": "default", "id": id })
        )
        .unwrap()["status"],
        "killed"
    );
    call(
        &w,
        "agents_clean",
        json!({ "business": "default", "id": id, "force": true }),
    )
    .unwrap();
}

#[test]
fn setup_manages_the_start_it_was_given() {
    let cos = cos_bin();
    let root = scratch("managed");
    let a = dirs(&root, "default");
    seed(&a.workspace, None);
    let start = start(&root, cos, 2, Sources::default());
    assert_eq!(start.data_dir, root.join("data"));
    assert_eq!(start.log_dir, root.join("logs"));
    assert_eq!(start.ui_state_file, root.join("ui-state.json"));
    assert_eq!(start.dashboards_file, None);
    register(&start, "default", &a, true);
    let (app, _w) = open(start.clone());

    let managed = app.try_state::<Start>().expect("setup manages the Start");
    assert_eq!(managed.data_dir, start.data_dir);
    assert_eq!(managed.log_dir, start.log_dir);
    assert_eq!(managed.ui_state_file, start.ui_state_file);
    assert_eq!(managed.dashboards_file, None);
    assert_eq!(managed.registry, start.registry);
    assert_eq!(managed.user_home, start.user_home);
}

#[test]
fn invalid_dashboards_at_setup_removes_pidfile_unsignalled() {
    use std::os::unix::process::CommandExt;

    struct Reap(std::process::Child);
    impl Drop for Reap {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    let cos = cos_bin();
    let root = scratch("cleanup-invalid");
    let a = dirs(&root, "default");
    seed(&a.workspace, None);
    let mut start = start(&root, cos, 2, Sources::default());
    register(&start, "default", &a, true);
    let dashboards = root.join("dashboards.toml");
    std::fs::write(&dashboards, "[servers.fake\nbinary = ").unwrap();
    start.dashboards_file = Some(dashboards);

    let mut child = Reap(
        Command::new("sleep")
            .arg("60")
            .process_group(0)
            .spawn()
            .unwrap(),
    );
    let pid = child.0.id() as libc::pid_t;
    let servers = root.join("data").join("servers");
    std::fs::create_dir_all(&servers).unwrap();
    let pidfile = servers.join("4777.json");
    let record = json!({
        "pid": pid,
        "pgid": cos::proc::pgid(pid),
        "start_time": cos::proc::start_time(pid),
        "exe": cos::proc::exe(pid),
        "addr": "127.0.0.1:4777",
    });
    std::fs::write(&pidfile, serde_json::to_vec(&record).unwrap()).unwrap();

    let (app, _w) = open(start);
    assert!(app.try_state::<Start>().is_some(), "setup did not complete");
    assert!(!pidfile.exists(), "launch cleanup kept the pidfile");
    std::thread::sleep(Duration::from_millis(300));
    let alive = child.0.try_wait().unwrap().is_none();
    drop(child);
    assert!(alive, "launch cleanup signalled the child");
}

#[test]
fn business_check_refuses_a_stale_label_and_copies_under_one_lock() {
    let cos = cos_bin();
    let root = scratch("check");
    let a = dirs(&root, "default");
    seed(&a.workspace, None);
    let b = dirs(&root, "acme");
    seed(&b.workspace, Some("Acme launch goal"));
    let start = start(&root, cos, 2, Sources::default());
    register(&start, "default", &a, true);
    register(&start, "acme", &b, false);
    let (app, w) = open(start);
    let handle = app.handle().clone();

    let ok = cos_desktop::check_business(&handle, "default").unwrap();
    assert_eq!(
        ok,
        Checked {
            business: Some("default".into()),
            home: a.home.clone(),
        }
    );
    let stale = cos_desktop::check_business(&handle, "acme").unwrap_err();
    let named = call(&w, "agents_stop", json!({ "business": "acme", "id": "x" })).unwrap_err();
    assert_eq!(stale, error_text(&named));

    let home_of = |name: &str| if name == "default" { &a.home } else { &b.home };
    let done = std::sync::atomic::AtomicBool::new(false);
    let (mut admitted, mut refused) = (0, 0);
    std::thread::scope(|s| {
        s.spawn(|| {
            for i in 0..100 {
                switch(&w, if i % 2 == 0 { "acme" } else { "default" }).unwrap();
            }
            done.store(true, std::sync::atomic::Ordering::SeqCst);
        });
        while !done.load(std::sync::atomic::Ordering::SeqCst) {
            for name in ["default", "acme"] {
                match cos_desktop::check_business(&handle, name) {
                    Ok(c) => {
                        admitted += 1;
                        assert_eq!(c.business.as_deref(), Some(name));
                        assert_eq!(&c.home, home_of(name), "home copied from another business");
                    }
                    Err(e) => {
                        refused += 1;
                        assert!(e.contains("nothing written, run or killed"), "{e}");
                    }
                }
            }
        }
    });
    assert!(
        admitted > 0 && refused > 0,
        "{admitted} admitted, {refused} refused"
    );
}

/// The data object a rendered report page embeds.
fn embedded(html: &[u8]) -> Value {
    let html = std::str::from_utf8(html).unwrap();
    let at = html.find("const DATA = ").unwrap() + "const DATA = ".len();
    serde_json::Deserializer::from_str(&html[at..])
        .into_iter::<Value>()
        .next()
        .unwrap()
        .unwrap()
}

#[test]
fn only_the_root_report_is_rendered_for_the_host() {
    let cos = cos_bin();
    let root = scratch("host-brief");
    let d = dirs(&root, "default");
    seed(&d.workspace, None);
    let briefs = d.home.join("out/briefs");
    std::fs::create_dir_all(&briefs).unwrap();
    let file = briefs.join("2030-04-09.json");
    std::fs::write(
        &file,
        json!({ "headline": "Rendered.", "needs_you": [] }).to_string(),
    )
    .unwrap();
    let mtime = cos::signals::mtime_ms(&std::fs::metadata(&file).unwrap());

    let start = start(&root, cos, 2, Sources::default());
    register(&start, "default", &d, true);
    let (app, _w) = open(start);

    for path in ["/", ""] {
        let report = cos_desktop::report_response(app.handle(), path);
        assert_eq!(report.status(), 200);
        let data = embedded(report.body());
        assert_eq!(data["hostBrief"], json!(true), "{path:?}");
        assert_eq!(
            data["briefRef"],
            json!({ "file": "2030-04-09.json", "mtime": mtime })
        );
    }

    let archived = cos::report::build(&d.home, &d.workspace).unwrap();
    let day = archived.file_stem().unwrap().to_str().unwrap();
    let report = cos_desktop::report_response(app.handle(), &format!("/reports/{day}.html"));
    assert_eq!(report.status(), 200);
    let data = embedded(report.body());
    assert!(
        data.get("hostBrief").is_none(),
        "archived report has hostBrief"
    );
    assert_eq!(data["briefRef"]["file"], "2030-04-09.json");
}

const BUSINESSES_CHANGED: &str = "businesses-changed";
const DASHBOARDS_CHANGED: &str = "dashboards-changed";
/// The watcher's tick, plus slack for a loaded machine.
const ONE_TICK: Duration = Duration::from_millis(1200);

type Events = std::sync::mpsc::Receiver<(&'static str, String)>;

/// `open`, with a listener on each of `events` registered before setup starts the
/// watcher, so its first tick is never missed.
fn open_listening(start: Start, events: &[&'static str]) -> (tauri::App<MockRuntime>, Win, Events) {
    use tauri::Listener;
    let mut app = cos_desktop::builder(mock_builder(), Some(start))
        .build(mock_context(noop_assets()))
        .unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    for &event in events {
        let tx = tx.clone();
        app.listen_any(event, move |e| {
            let _ = tx.send((event, e.payload().to_owned()));
        });
    }
    #[allow(deprecated)]
    app.run_iteration(|_, _| {});
    let w = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    (app, w, rx)
}

/// The first `event` payload within `within` that `want` accepts; other events are dropped.
fn next_event(
    rx: &Events,
    event: &str,
    within: Duration,
    want: impl Fn(&Value) -> bool,
) -> Option<Value> {
    let until = Instant::now() + within;
    loop {
        let left = until.checked_duration_since(Instant::now())?;
        let (name, payload) = rx.recv_timeout(left).ok()?;
        let payload: Value = serde_json::from_str(&payload).unwrap();
        if name == event && want(&payload) {
            return Some(payload);
        }
    }
}

/// The names of every event received within `within`.
fn drain(rx: &Events, within: Duration) -> Vec<&'static str> {
    let until = Instant::now() + within;
    let mut seen = Vec::new();
    while let Some(left) = until.checked_duration_since(Instant::now()) {
        match rx.recv_timeout(left) {
            Ok((name, _)) => seen.push(name),
            Err(_) => break,
        }
    }
    seen
}

fn row<'a>(rows: &'a Value, name: &str) -> &'a Value {
    rows.as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == name)
        .unwrap_or(&Value::Null)
}

fn names(rows: &Value) -> Vec<&str> {
    rows.as_array()
        .unwrap()
        .iter()
        .map(|r| r["name"].as_str().unwrap())
        .collect()
}

/// Kills and reaps the child on drop, so a failed assertion leaks no process.
struct Reaped(std::process::Child);

impl Drop for Reaped {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn a_killed_launcher_turns_its_row_lost_within_one_tick() {
    let root = scratch("cuj8");
    let a = dirs(&root, "default");
    seed(&a.workspace, None);
    let cos = cos_bin();
    let sup_home = root.join("launcher");
    std::fs::create_dir_all(&sup_home).unwrap();
    let mut launcher = Reaped(
        Command::new(&cos)
            .arg("--home")
            .arg(&sup_home)
            .arg("--workspace")
            .arg(&sup_home)
            .arg("--projects")
            .arg(&sup_home)
            .arg("mcp")
            .env_remove("COS_HOME")
            .env_remove("COS_WORKSPACE")
            .env_remove("COS_PROJECTS")
            .env_remove("COS_BUSINESS")
            .env_remove("XDG_CONFIG_HOME")
            .env("HOME", root.join("user"))
            .env("COS_BUSINESSES", root.join("none.toml"))
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap(),
    );
    let pid = launcher.0.id() as i32;
    let until = Instant::now() + Duration::from_secs(10);
    while !cos::proc::exe(pid).is_some_and(|e| e.file_name().is_some_and(|f| f == "cos")) {
        assert!(Instant::now() < until, "cos {pid} never ran");
        std::thread::sleep(Duration::from_millis(20));
    }
    let state = plant_run(&a.home, "r1", pid);
    let before = std::fs::read(&state).unwrap();

    let start = start(&root, cos, 2, Sources::default());
    register(&start, "default", &a, true);
    let (_app, _w, rx) = open_listening(start, &[BUSINESSES_CHANGED]);
    assert!(
        next_event(&rx, BUSINESSES_CHANGED, Duration::from_secs(5), |rows| {
            row(rows, "default")["live"] == 1
        })
        .is_some(),
        "the first tick did not emit the live run"
    );

    launcher.0.kill().unwrap();
    launcher.0.wait().unwrap();
    let lost = next_event(&rx, BUSINESSES_CHANGED, ONE_TICK, |rows| {
        row(rows, "default")["lost"] == 1
    });
    assert!(lost.is_some(), "no lost row within one tick of the SIGKILL");
    assert_eq!(row(&lost.unwrap(), "default")["live"], 0);
    assert_eq!(std::fs::read(&state).unwrap(), before, "a run file changed");
}

#[test]
fn a_starting_run_turns_lost_after_the_grace_without_a_call() {
    let root = scratch("starting");
    let a = dirs(&root, "default");
    seed(&a.workspace, None);
    let margin = Duration::from_secs(4);
    let created = jiff::Timestamp::now()
        - jiff::SignedDuration::try_from(cos::agents::START_GRACE - margin).unwrap();
    plant(&a.home, "r1", None, &created.to_string());
    let lost_at = Instant::now() + margin;

    let start = start(&root, cos_bin(), 2, Sources::default());
    register(&start, "default", &a, true);
    let (_app, _w, rx) = open_listening(start, &[BUSINESSES_CHANGED]);
    let starting = next_event(&rx, BUSINESSES_CHANGED, margin, |_| true)
        .expect("the first tick emitted nothing");
    assert_eq!(
        (
            row(&starting, "default")["live"].clone(),
            row(&starting, "default")["lost"].clone()
        ),
        (json!(1), json!(0)),
        "a Starting run counts as live"
    );

    let lost = next_event(&rx, BUSINESSES_CHANGED, margin + ONE_TICK, |rows| {
        row(rows, "default")["lost"] == 1
    });
    assert!(lost.is_some(), "the Starting run never turned lost");
    assert!(
        Instant::now() >= lost_at,
        "turned lost before the grace passed"
    );
    assert_eq!(row(&lost.unwrap(), "default")["live"], 0);
}

#[test]
fn businesses_changed_carries_every_registered_row_and_the_app_business() {
    let root = scratch("changed-rows");
    let a = dirs(&root, "default");
    seed(&a.workspace, None);
    let b = dirs(&root, "bravo");
    seed(&b.workspace, None);
    let c = dirs(&root, "charlie");
    let start = start(&root, cos_bin(), 2, Sources::default());
    register(&start, "default", &a, true);
    register(&start, "bravo", &b, false);
    register(&start, "charlie", &c, false);

    let registry = start.registry.clone();
    let (_app, w, rx) = open_listening(start, &[BUSINESSES_CHANGED]);
    let first = next_event(&rx, BUSINESSES_CHANGED, Duration::from_secs(5), |_| true)
        .expect("the first tick emitted nothing");
    assert_eq!(names(&first), ["bravo", "charlie", "default"]);
    assert_eq!(row(&first, "default")["app"], true);

    switch(&w, "bravo").unwrap();
    write_brief(&c.home, "2026-11-14");
    let after_switch = next_event(&rx, BUSINESSES_CHANGED, ONE_TICK, |rows| {
        row(rows, "charlie")["unread"] == true
    })
    .expect("a new brief in charlie emitted nothing");
    let apps: Vec<(&str, bool)> = after_switch
        .as_array()
        .unwrap()
        .iter()
        .map(|r| (r["name"].as_str().unwrap(), r["app"].as_bool().unwrap()))
        .collect();
    assert_eq!(
        apps,
        [("bravo", true), ("charlie", false), ("default", false)]
    );
    assert_eq!(after_switch, call(&w, "businesses", json!({})).unwrap());

    cos::business::remove(&registry, "charlie").unwrap();
    let removed = next_event(&rx, BUSINESSES_CHANGED, ONE_TICK, |rows| {
        !names(rows).contains(&"charlie")
    })
    .expect("the removed business is still in the payload");
    assert_eq!(names(&removed), ["bravo", "default"]);
    assert_eq!(
        removed,
        call(&w, "businesses", json!({})).unwrap(),
        "the payload is not the businesses shape"
    );
}

#[test]
fn dashboards_changed_skips_the_first_tick_and_carries_no_payload() {
    let root = scratch("dash-changed");
    let a = dirs(&root, "default");
    seed(&a.workspace, None);
    let mut start = start(&root, cos_bin(), 2, Sources::default());
    register(&start, "default", &a, true);
    let file = root.join("dashboards.toml");
    std::fs::write(&file, "").unwrap();
    start.dashboards_file = Some(file.clone());

    let (_app, _w, rx) = open_listening(start, &[BUSINESSES_CHANGED, DASHBOARDS_CHANGED]);
    let launch = drain(&rx, 3 * ONE_TICK);
    assert!(
        launch.contains(&BUSINESSES_CHANGED),
        "the first tick emitted nothing"
    );
    assert!(
        !launch.contains(&DASHBOARDS_CHANGED),
        "dashboards-changed at launch: {launch:?}"
    );

    std::fs::write(&file, "# edited\n").unwrap();
    std::fs::File::options()
        .write(true)
        .open(&file)
        .unwrap()
        .set_modified(std::time::SystemTime::now() + Duration::from_secs(5))
        .unwrap();
    let changed = next_event(&rx, DASHBOARDS_CHANGED, ONE_TICK, |_| true);
    assert_eq!(
        changed,
        Some(Value::Null),
        "no payload-free dashboards-changed"
    );
}

fn brief_seen(w: &Win, business: &str, file: &str, mtime: i64) -> Result<Value, Value> {
    call(
        w,
        "brief_seen",
        json!({ "business": business, "file": file, "mtime": mtime }),
    )
}

fn unread(w: &Win) -> BTreeMap<String, Value> {
    row_flags(w)
        .into_iter()
        .map(|(name, (unread, _))| (name, unread))
        .collect()
}

fn ui_state(start: &Start) -> Value {
    serde_json::from_slice(&std::fs::read(&start.ui_state_file).unwrap()).unwrap()
}

fn seen_pair(file: &str, mtime: i64) -> Value {
    json!({ "file": file, "mtime": mtime })
}

#[test]
fn brief_seen_refusals_leave_the_stored_pair() {
    let cos = cos_bin();
    let root = scratch("seen-refused");
    let a = dirs(&root, "default");
    seed(&a.workspace, None);
    let b = dirs(&root, "bravo");
    let mtime = write_brief(&a.home, "2026-11-14");
    std::fs::write(a.home.join("out/briefs/notes.json"), "{}").unwrap();
    std::fs::write(a.home.join("out/2026-11-15.json"), "{}").unwrap();
    write_brief(&b.home, "2026-11-14");

    let start = start(&root, cos, 2, Sources::default());
    register(&start, "default", &a, true);
    register(&start, "bravo", &b, false);
    let stored =
        json!({ "lastSeenBrief": { "default": seen_pair("2026-11-13.json", 5) } }).to_string();
    std::fs::write(&start.ui_state_file, &stored).unwrap();
    let (_app, w) = open(start.clone());

    let stale = brief_seen(&w, "bravo", "2026-11-14.json", mtime).unwrap_err();
    assert!(
        error_text(&stale).contains("nothing written, run or killed"),
        "{stale}"
    );
    for (file, mtime, why) in [
        ("notes.json", 0, "a non-date filename"),
        ("2026-11-15.json", 0, "a filename outside out/briefs"),
        ("../2026-11-15.json", 0, "a path"),
        ("2026-11-14.json", mtime + 1, "an mtime past the file's"),
        ("2026-11-14.json", -1, "a negative mtime"),
    ] {
        let e = brief_seen(&w, "default", file, mtime).expect_err(why);
        assert!(error_text(&e).starts_with("brief_seen: "), "{why}: {e}");
    }
    assert_eq!(
        std::fs::read_to_string(&start.ui_state_file).unwrap(),
        stored,
        "a refusal changed the stored pair"
    );
    assert_eq!(unread(&w)["default"], true);
}

#[test]
fn brief_seen_refuses_the_defaults_layer_window_labelled_default() {
    let cos = cos_bin();
    let root = scratch("seen-defaults");
    let registered = dirs(&root, "registered");
    seed(&registered.workspace, None);
    let start = start(&root, cos, 2, Sources::default());
    register(&start, "default", &registered, false);
    write_brief(&registered.home, "2026-11-14");
    let mtime = write_brief(&start.install, "2026-11-14");
    let stored =
        json!({ "lastSeenBrief": { "default": seen_pair("2026-11-13.json", 5) } }).to_string();
    std::fs::write(&start.ui_state_file, &stored).unwrap();
    let (_app, w) = open(start.clone());
    let paths = call(&w, "paths", json!({})).unwrap();
    assert_eq!(paths["business"], "default", "{paths}");
    assert_eq!(
        paths["home"],
        json!(start.install),
        "not the defaults layer"
    );

    let e = brief_seen(&w, "default", "2026-11-14.json", mtime).unwrap_err();
    assert!(error_text(&e).contains("no registry business"), "{e}");
    assert_eq!(
        std::fs::read_to_string(&start.ui_state_file).unwrap(),
        stored,
        "the defaults layer stored a pair for the registered default"
    );
}

#[test]
fn brief_seen_clears_the_dot_and_never_lowers_the_pair() {
    let cos = cos_bin();
    let root = scratch("seen-clears");
    let a = dirs(&root, "default");
    seed(&a.workspace, None);
    let b = dirs(&root, "bravo");
    let c = dirs(&root, "charlie");
    let older = write_brief(&a.home, "2026-11-13");
    let mtime = write_brief(&a.home, "2026-11-14");
    write_brief(&b.home, "2026-11-14");
    write_brief(&c.home, "2026-11-14");

    let mut start = start(&root, cos, 2, Sources::default());
    start.ui_state_file = root.join("config/nested/ui-state.json");
    register(&start, "default", &a, true);
    register(&start, "bravo", &b, false);
    register(&start, "charlie", &c, false);
    let (_app, w) = open(start.clone());
    let all_unread = BTreeMap::from([
        ("bravo".to_owned(), json!(true)),
        ("charlie".to_owned(), json!(true)),
        ("default".to_owned(), json!(true)),
    ]);
    assert_eq!(unread(&w), all_unread);

    brief_seen(&w, "default", "2026-11-14.json", mtime).unwrap();
    let mut cleared = all_unread.clone();
    cleared.insert("default".to_owned(), json!(false));
    assert_eq!(unread(&w), cleared);
    assert!(
        root.join("config/nested/ui-state.json.lock").is_file(),
        "no lock beside the ui-state file"
    );
    assert_eq!(
        ui_state(&start)["lastSeenBrief"]["default"],
        seen_pair("2026-11-14.json", mtime)
    );

    brief_seen(&w, "default", "2026-11-13.json", older).unwrap();
    brief_seen(&w, "default", "2026-11-14.json", mtime - 1).unwrap();
    assert_eq!(
        ui_state(&start)["lastSeenBrief"]["default"],
        seen_pair("2026-11-14.json", mtime),
        "the stored pair was lowered"
    );

    let (_again, w2) = open(start);
    assert_eq!(unread(&w2), cleared, "a new App lost the cleared dot");
}

#[test]
fn an_unparseable_ui_state_reads_empty_and_unregistered_entries_drop() {
    let cos = cos_bin();
    let root = scratch("seen-unparseable");
    let a = dirs(&root, "default");
    seed(&a.workspace, None);
    let b = dirs(&root, "bravo");
    let mtime = write_brief(&a.home, "2026-11-14");
    let bravo = write_brief(&b.home, "2026-11-14");

    let start = start(&root, cos, 2, Sources::default());
    register(&start, "default", &a, true);
    register(&start, "bravo", &b, false);
    std::fs::write(&start.ui_state_file, "{ not json").unwrap();
    let (_app, w) = open(start.clone());
    assert_eq!(
        unread(&w),
        BTreeMap::from([
            ("bravo".to_owned(), json!(true)),
            ("default".to_owned(), json!(true)),
        ])
    );
    brief_seen(&w, "default", "2026-11-14.json", mtime).unwrap();
    assert_eq!(
        ui_state(&start),
        json!({ "lastSeenBrief": { "default": seen_pair("2026-11-14.json", mtime) } })
    );

    std::fs::write(
        &start.ui_state_file,
        json!({ "lastSeenBrief": {
            "ghost": seen_pair("2026-11-14.json", 1),
            "bravo": seen_pair("2026-11-14.json", bravo),
        } })
        .to_string(),
    )
    .unwrap();
    brief_seen(&w, "default", "2026-11-14.json", mtime).unwrap();
    assert_eq!(
        ui_state(&start),
        json!({ "lastSeenBrief": {
            "bravo": seen_pair("2026-11-14.json", bravo),
            "default": seen_pair("2026-11-14.json", mtime),
        } }),
        "an unregistered entry survived the write"
    );
}

#[test]
fn brief_seen_publishes_the_cleared_dot_through_the_shared_cache() {
    let root = scratch("seen-publish");
    let a = dirs(&root, "default");
    seed(&a.workspace, None);
    let b = dirs(&root, "bravo");
    let c = dirs(&root, "charlie");
    let mtime = write_brief(&a.home, "2026-11-14");
    write_brief(&c.home, "2026-11-14");
    let start = start(&root, cos_bin(), 2, Sources::default());
    register(&start, "default", &a, true);
    register(&start, "bravo", &b, false);
    register(&start, "charlie", &c, false);

    let (_app, w, rx) = open_listening(start, &[BUSINESSES_CHANGED]);
    brief_seen(&w, "default", "2026-11-14.json", mtime).unwrap();
    let cleared = next_event(&rx, BUSINESSES_CHANGED, ONE_TICK, |rows| {
        row(rows, "default")["unread"] == false
    })
    .expect("brief_seen emitted no cleared dot");
    assert_eq!(names(&cleared), ["bravo", "charlie", "default"]);
    assert_eq!(row(&cleared, "default")["app"], true);

    write_brief(&b.home, "2026-11-14");
    let elsewhere = next_event(&rx, BUSINESSES_CHANGED, ONE_TICK, |rows| {
        row(rows, "bravo")["unread"] == true
    })
    .expect("a stamp change in bravo emitted nothing");
    assert_eq!(
        row(&elsewhere, "default")["unread"],
        false,
        "the watcher's emit brought the dot back"
    );

    write_brief(&a.home, "2026-11-15");
    assert!(
        next_event(&rx, BUSINESSES_CHANGED, ONE_TICK, |rows| {
            row(rows, "default")["unread"] == true
        })
        .is_some(),
        "a new brief did not re-light the dot within one tick"
    );
}

/// A cache missing a registered business is what `brief_seen` meets before the watcher's
/// first tick. Registering `charlie` behind an unchanged registry mtime leaves the
/// watcher's cache without it for good, so only `brief_seen`'s fill can emit its row.
#[test]
fn brief_seen_fills_a_cache_missing_a_registered_business() {
    let root = scratch("seen-fill");
    let a = dirs(&root, "default");
    seed(&a.workspace, None);
    let b = dirs(&root, "bravo");
    let c = dirs(&root, "charlie");
    let mtime = write_brief(&a.home, "2026-11-14");
    write_brief(&c.home, "2026-11-14");
    let start = start(&root, cos_bin(), 2, Sources::default());
    register(&start, "default", &a, true);
    register(&start, "bravo", &b, false);

    let (_app, w, rx) = open_listening(start.clone(), &[BUSINESSES_CHANGED]);
    let first = next_event(&rx, BUSINESSES_CHANGED, ONE_TICK, |_| true)
        .expect("the first tick emitted nothing");
    assert_eq!(names(&first), ["bravo", "default"]);

    let staged = root.join("staged.toml");
    std::fs::copy(&start.registry, &staged).unwrap();
    cos::business::add(&staged, "charlie", c.clone(), false).unwrap();
    let registry_mtime = std::fs::metadata(&start.registry)
        .unwrap()
        .modified()
        .unwrap();
    std::fs::File::options()
        .write(true)
        .open(&staged)
        .unwrap()
        .set_modified(registry_mtime)
        .unwrap();
    std::fs::rename(&staged, &start.registry).unwrap();

    brief_seen(&w, "default", "2026-11-14.json", mtime).unwrap();
    let cleared = next_event(&rx, BUSINESSES_CHANGED, ONE_TICK, |rows| {
        row(rows, "default")["unread"] == false
    })
    .expect("brief_seen emitted no cleared dot");
    assert_eq!(names(&cleared), ["bravo", "charlie", "default"]);
    assert_eq!(row(&cleared, "charlie")["unread"], true);
    assert_eq!(row(&cleared, "default")["app"], true);
}

const DASHBOARDS_TOML: &str = r#"
[server.control-room]
binary = "/opt/control-room/portfolio-control-room"
addr = "127.0.0.1:4777"
ready = { path = "/health", contains = "\"status\":\"ok\"" }

[[business.default]]
name = "Control room"
url = "http://127.0.0.1:4777/"
server = "control-room"
probe = { path = "/", contains = "<title>Wolven portfolio control room" }

[[business.bravo]]
name = "Bravo board"
url = "http://127.0.0.1:4800/"
probe = { path = "/", contains = "<title>Bravo board</title>" }
"#;

fn dashboards(w: &Win, business: &str) -> Result<Value, Value> {
    call(w, "dashboards", json!({ "business": business }))
}

/// Two registered businesses, `default` the app's, with `dashboards.toml` beside the registry.
fn dashboards_app(name: &str, file: &str) -> (Start, PathBuf, tauri::App<MockRuntime>, Win) {
    let root = scratch(name);
    let a = dirs(&root, "default");
    seed(&a.workspace, None);
    let b = dirs(&root, "bravo");
    let start = start(&root, cos_bin(), 2, Sources::default());
    register(&start, "default", &a, true);
    register(&start, "bravo", &b, false);
    let path = root.join("dashboards.toml");
    std::fs::write(&path, file).unwrap();
    let (app, w) = open(start.clone());
    (start, path, app, w)
}

#[test]
fn dashboards_returns_the_app_business_and_loads_the_file_fresh() {
    let (_start, path, _app, w) = dashboards_app("dash-fresh", DASHBOARDS_TOML);
    assert_eq!(
        dashboards(&w, "default").unwrap(),
        json!([{ "name": "Control room", "url": "http://127.0.0.1:4777/", "managed": true }])
    );

    let added = format!(
        "{DASHBOARDS_TOML}
[[business.default]]
name = \"Gazette\"
url = \"http://127.0.0.1:8790/#staff\"
probe = {{ path = \"/\", contains = \"<h1>Chief of staff gazette\" }}
"
    );
    std::fs::write(&path, added).unwrap();
    assert_eq!(
        dashboards(&w, "default").unwrap(),
        json!([
            { "name": "Control room", "url": "http://127.0.0.1:4777/", "managed": true },
            { "name": "Gazette", "url": "http://127.0.0.1:8790/#staff", "managed": false },
        ]),
        "a dashboard added after launch is missing from the next call"
    );
}

#[test]
fn dashboards_refuses_an_invalid_file_and_a_stale_business() {
    let bad = DASHBOARDS_TOML.replace(
        "url = \"http://127.0.0.1:4777/\"",
        "url = \"https://127.0.0.1:4777/\"",
    );
    let (_start, _path, _app, w) = dashboards_app("dash-refused", &bad);

    let e = dashboards(&w, "default").unwrap_err();
    assert!(
        error_text(&e).contains("business.default[0].url: must be http://127.0.0.1:<port>/"),
        "{e}"
    );

    let stale = dashboards(&w, "bravo").unwrap_err();
    assert_eq!(
        error_text(&stale),
        "this call names business \"bravo\" but the window is on \"default\": nothing written, run or killed"
    );
}

fn dashboard_open(w: &Win, business: &str, name: &str) -> Result<Value, Value> {
    call(
        w,
        "dashboard_open",
        json!({ "business": business, "name": name }),
    )
}

#[test]
fn dashboard_open_refuses_a_stale_business_an_undeclared_name_and_an_invalid_file() {
    let (_start, path, _app, w) = dashboards_app("dash-open-refused", DASHBOARDS_TOML);

    assert_eq!(
        dashboard_open(&w, "bravo", "Bravo board").unwrap_err(),
        json!({
            "kind": "StaleBusiness",
            "message": "this call names business \"bravo\" but the window is on \"default\": nothing written, run or killed",
        })
    );
    assert_eq!(
        dashboard_open(&w, "default", "Bravo board").unwrap_err(),
        json!({ "kind": "UnknownDashboard", "name": "Bravo board" }),
        "a dashboard of another business must not open"
    );

    let bad = DASHBOARDS_TOML.replace(
        "url = \"http://127.0.0.1:4777/\"",
        "url = \"https://127.0.0.1:4777/\"",
    );
    std::fs::write(&path, bad).unwrap();
    let e = dashboard_open(&w, "default", "Control room").unwrap_err();
    assert_eq!(e["kind"], "InvalidConfig", "{e}");
    assert!(
        e["message"]
            .as_str()
            .unwrap()
            .contains("business.default[0].url: must be http://127.0.0.1:<port>/"),
        "{e}"
    );
}

/// A listener answering every request with a page that carries `marker`.
fn marker_page(marker: &'static str) -> u16 {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for mut s in listener.incoming().flatten() {
            let _ = s.set_read_timeout(Some(Duration::from_secs(2)));
            let mut req = Vec::new();
            let mut chunk = [0u8; 1024];
            while !req.windows(4).any(|w| w == b"\r\n\r\n") {
                match s.read(&mut chunk) {
                    Ok(n) if n > 0 => req.extend_from_slice(&chunk[..n]),
                    _ => break,
                }
            }
            let body = format!("<html><head>{marker}</head></html>");
            let _ = write!(
                s,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    port
}

#[test]
fn dashboard_open_runs_an_unmanaged_dashboard_added_after_launch() {
    let (_start, path, _app, w) = dashboards_app("dash-open-unmanaged", DASHBOARDS_TOML);
    let port = marker_page("<title>Fixture board</title>");
    let url = format!("http://127.0.0.1:{port}/");

    assert_eq!(
        dashboard_open(&w, "default", "Fixture").unwrap_err(),
        json!({ "kind": "UnknownDashboard", "name": "Fixture" })
    );

    let added = format!(
        "{DASHBOARDS_TOML}
[[business.default]]
name = \"Fixture\"
url = \"{url}\"
probe = {{ path = \"/\", contains = \"<title>Fixture board</title>\" }}
"
    );
    std::fs::write(&path, added).unwrap();
    assert_eq!(
        dashboard_open(&w, "default", "Fixture").unwrap(),
        json!({ "url": url, "owned": false }),
        "a dashboard added after launch did not open"
    );
}

#[test]
fn dashboard_open_on_a_window_with_no_registry_business_is_unknown() {
    let root = scratch("dash-open-explicit");
    let a = dirs(&root, "default");
    seed(&a.workspace, None);
    let base = start(&root, cos_bin(), 2, Sources::default());
    register(&base, "default", &a, true);
    std::fs::write(
        root.join("dashboards.toml"),
        r#"
[[business.explicit]]
name = "Explicit board"
url = "http://127.0.0.1:4801/"
probe = { path = "/", contains = "<title>Explicit board</title>" }
"#,
    )
    .unwrap();
    let (_app, w) = open(pinned(&base, &a));
    assert_eq!(
        call(&w, "paths", json!({})).unwrap()["business"],
        "explicit"
    );

    assert_eq!(dashboards(&w, "explicit").unwrap(), json!([]));
    assert_eq!(
        dashboard_open(&w, "explicit", "Explicit board").unwrap_err(),
        json!({ "kind": "UnknownDashboard", "name": "Explicit board" })
    );
}

#[test]
fn run_brief_starts_one_owned_brief_for_the_app_business_and_says_when_it_ends() {
    let root = scratch("brief-run");
    let stub = root.join("cos-stub.sh");
    let seen = root.join("args.txt");
    std::fs::write(
        &stub,
        format!(
            "#!/bin/sh\necho \"$@\" > '{}'\necho \"business=$COS_BUSINESS home=$COS_HOME\" >> '{}'\nsleep 1\necho 'brief: done' >&2\nexit 0\n",
            seen.display(),
            seen.display()
        ),
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();
    let a = dirs(&root, "default");
    let start = start(&root, stub, 1, Sources::default());
    register(&start, "default", &a, true);
    let (_app, w) = open(start);

    assert_eq!(
        call(&w, "brief_status", json!({})).unwrap()["running"],
        false
    );
    let stale = call(&w, "brief_run", json!({ "business": "acme" })).unwrap_err();
    assert!(
        error_text(&stale).contains("nothing written, run or killed"),
        "{stale}"
    );
    let started = call(&w, "brief_run", json!({ "business": "default" })).unwrap();
    assert_eq!(started["running"], true);
    assert_eq!(started["business"], "default");
    let twice = call(&w, "brief_run", json!({ "business": "default" })).unwrap_err();
    assert!(error_text(&twice).contains("already running"), "{twice}");
    let until = Instant::now() + Duration::from_secs(20);
    let ended = loop {
        let s = call(&w, "brief_status", json!({})).unwrap();
        if s["running"] == false {
            break s;
        }
        assert!(Instant::now() < until, "the brief never ended");
        std::thread::sleep(Duration::from_millis(100));
    };
    assert_eq!(ended["ok"], true, "{ended}");
    let said = std::fs::read_to_string(&seen).unwrap();
    let (args, env) = said.split_once('\n').unwrap();
    assert_eq!(
        args,
        format!(
            "--home {} --workspace {} --projects {} --no-open brief",
            a.home.display(),
            a.workspace.display(),
            a.projects.display()
        ),
        "the app business's own directories, and never opening the report"
    );
    assert_eq!(
        env.trim_end(),
        format!("business= home={}", a.home.display()),
        "the env is pinned to the same business"
    );
}

/// A fictional repo under `projects` whose one epic waits at the approval
/// gate: the design and a review with findings committed on its branch only.
fn waiting_flow(projects: &Path, name: &str, design: &str, review: &str) -> String {
    let r = projects.join(name);
    std::fs::create_dir_all(r.join(".chronis")).unwrap();
    git(&r, &["init", "-q", "-b", "main"]);
    git(&r, &["config", "user.name", "Ann Example"]);
    git(&r, &["commit", "-q", "--allow-empty", "-m", "init"]);
    git(&r, &["checkout", "-q", "-b", "feat/x"]);
    std::fs::create_dir_all(r.join("docs/plans")).unwrap();
    std::fs::write(r.join(design), format!("# {name}\n")).unwrap();
    let sha = cos::flows::sha256_file(&r.join(design)).unwrap();
    std::fs::write(
        r.join(review),
        format!(
            "---\ndesign: {design}\ndesign-sha256: {sha}\nverdict: findings\n---\n\n1. should A: x.\n2. nit B: y.\n"
        ),
    )
    .unwrap();
    git(&r, &["add", "docs"]);
    git(&r, &["commit", "-q", "-m", "design"]);
    git(&r, &["checkout", "-q", "main"]);
    let epic = json!([{ "id": format!("t-{name}"), "title": format!("{name} feature"), "status": "open",
        "task_type": "epic", "parent": null, "blocked_by": [], "approved": null, "archived": false,
        "description": format!("ritual-branch: feat/x\nritual-design: {design}") }]);
    std::fs::write(r.join(".chronis/tasks.json"), epic.to_string()).unwrap();
    sha
}

#[test]
fn the_flows_tab_approves_a_waiting_design_only_on_a_clean_review_or_by_choice() {
    let root = scratch("ritual");
    let a = dirs(&root, "default");
    let projects = a.projects.clone();
    let design = "docs/plans/2026-01-05-x-design.md";
    let review = "docs/plans/2026-01-05-x-design-review.md";
    let sha = waiting_flow(&projects, "web", design, review);
    waiting_flow(&projects, "api", design, review);
    // Only this test reads COS_CN.
    unsafe {
        std::env::set_var("COS_CN", format!("{ROOT}/tests/fixtures/fake-cn-store.sh"));
    }
    let start = start(&root, cos_bin(), 1, Sources::default());
    register(&start, "default", &a, true);
    let (app, w) = open(start);
    let gates = |w: &Win| call(w, "ritual_gates", json!({})).unwrap();

    let res = gates(&w);
    assert_eq!(res["steps"][2], "approve");
    let list = res["gates"].as_array().unwrap();
    assert_eq!(list.len(), 2, "{res}");
    let web = list.iter().find(|g| g["repo"] == "web").unwrap();
    assert_eq!(web["epic"], "t-web");
    assert_eq!(web["step"], "approve");
    assert_eq!(web["clean"], false);
    assert_eq!(web["by"], "Ann Example");
    assert_eq!(web["design_sha256"], sha.as_str());
    assert_eq!(web["review"]["verdict"], "findings");
    assert_eq!(
        [
            &web["review"]["blocking"],
            &web["review"]["should"],
            &web["review"]["nit"]
        ],
        [&json!(0), &json!(1), &json!(1)]
    );

    let approve = |repo: &str, review: Option<&str>, without: bool| {
        call(
            &w,
            "ritual_approve",
            json!({ "business": "default", "repo": repo, "epic": format!("t-{repo}"), "design": design,
                    "review": review, "without_review": without }),
        )
    };
    let refused = approve("web", Some(review), false).unwrap_err();
    assert!(error_text(&refused).contains("not clean"), "{refused}");
    assert!(
        !projects.join("web/.chronis/calls.log").exists(),
        "a refusal writes nothing"
    );
    let outside = approve("../web", None, true).unwrap_err();
    assert!(error_text(&outside).contains("no repo named"), "{outside}");
    let unnamed = call(
        &w,
        "ritual_open",
        json!({ "repo": "web", "epic": "t-web", "path": "Cargo.toml" }),
    )
    .unwrap_err();
    assert!(
        error_text(&unnamed).contains("not on t-web's card"),
        "{unnamed}"
    );

    let web_dir = projects.join("web");
    git(&web_dir, &["checkout", "-q", "feat/x"]);
    std::fs::write(
        web_dir.join(review),
        format!("---\ndesign: {design}\ndesign-sha256: {sha}\nverdict: clean\n---\n"),
    )
    .unwrap();
    git(&web_dir, &["commit", "-q", "-am", "clean review"]);
    git(&web_dir, &["checkout", "-q", "main"]);
    let res = gates(&w);
    let web = res["gates"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["repo"] == "web")
        .unwrap()
        .clone();
    assert_eq!(web["clean"], true, "{web}");

    let done = approve("web", Some(review), false).unwrap();
    assert_eq!(done["approved"], true, "{done}");
    assert_eq!(
        done["line"],
        format!("ritual-approved: {design} sha256:{sha} by Ann Example")
    );
    assert!(done["check"].as_str().unwrap().starts_with("approved:"));

    let bare = approve("api", None, true).unwrap();
    assert_eq!(bare["approved"], true, "{bare}");
    assert!(
        bare["line"]
            .as_str()
            .unwrap()
            .ends_with("by Ann Example without review")
    );
    assert_eq!(gates(&w)["gates"], json!([]), "both flows left the gate");

    // The Flows page's Open names a repo, an epic and a key; the value comes
    // from the epic's own line, and anything unsafe is refused before `open`.
    let evil = projects.join("evil");
    std::fs::create_dir_all(evil.join(".chronis")).unwrap();
    std::fs::write(evil.join("run.command"), "echo hi").unwrap();
    let epic = json!([{ "id": "t-evil", "title": "evil", "status": "open", "task_type": "epic",
        "parent": null, "blocked_by": [], "approved": null, "archived": false,
        "description": "ritual-report: run.command\nritual-plan: ../web/x.md\nritual-pr: https://evil.example/x\nritual-brief: /etc/hosts.md" }]);
    std::fs::write(evil.join(".chronis/tasks.json"), epic.to_string()).unwrap();
    let open_err = |repo: &str, epic: &str, key: &str| {
        call(
            &w,
            "flow_open",
            json!({ "repo": repo, "epic": epic, "key": key }),
        )
        .unwrap_err()
        .as_str()
        .unwrap()
        .to_owned()
    };
    for (repo, epic, key, why) in [
        ("web", "t-web", "branch", "not a file a flow names"),
        ("../web", "t-web", "design", "no repo named"),
        ("web", "t-nope", "design", "not in this repo's task store"),
        ("web", "t-web", "plan", "names no plan"),
        ("evil", "t-evil", "report", "not a Markdown or HTML file"),
        ("evil", "t-evil", "plan", "leaves the repo"),
        ("evil", "t-evil", "pr", "only GitHub links"),
        ("evil", "t-evil", "brief", "not a path inside"),
    ] {
        let err = open_err(repo, epic, key);
        assert!(err.contains(why), "{repo} {epic} {key}: {err}");
    }

    // The Flows page's Hire names a bead; the prompt and the repo come from
    // the store, and a bead that waits on another is refused.
    let beads = projects.join("beads");
    std::fs::create_dir_all(beads.join(".chronis")).unwrap();
    let task = |id: &str,
                kind: &str,
                status: &str,
                parent: Option<&str>,
                blocked: &[&str],
                desc: &str| {
        json!({ "id": id, "title": format!("title {id}"), "status": status, "task_type": kind,
            "parent": parent, "blocked_by": blocked, "approved": null, "archived": false, "description": desc })
    };
    let list = json!([
        task("t-ep", "epic", "open", None, &[], ""),
        task("t-b1", "task", "done", Some("t-ep"), &[], ""),
        task(
            "t-b2",
            "task",
            "open",
            Some("t-ep"),
            &["t-b1"],
            "## Acceptance criteria\n- [ ] the gate lists every term\n"
        ),
        task("t-b3", "task", "open", Some("t-ep"), &["t-b2"], ""),
    ]);
    std::fs::write(beads.join(".chronis/tasks.json"), list.to_string()).unwrap();
    let bead = |repo: &str, b: &str| {
        call(
            &w,
            "flow_bead",
            json!({ "repo": repo, "epic": "t-ep", "bead": b }),
        )
    };
    let b2 = bead("beads", "t-b2").unwrap();
    assert_eq!(b2["title"], "t-b2 title t-b2");
    assert_eq!(b2["repo"], json!(beads));
    assert_eq!(b2["criteria"], 1);
    assert_eq!(b2["source"], "bead:beads/t-b2");
    let prompt = b2["prompt"].as_str().unwrap();
    assert!(
        prompt.contains("- [ ] the gate lists every term")
            && prompt.contains("cn show t-b2 --toon"),
        "{prompt}"
    );
    let blocked = bead("beads", "t-b3").unwrap_err();
    assert!(error_text(&blocked).contains("waits on t-b2"), "{blocked}");
    let outside = bead("../beads", "t-b2").unwrap_err();
    assert!(error_text(&outside).contains("no repo named"), "{outside}");

    let page = cos_desktop::flows_response(app.handle(), Some("fresh=1"));
    assert_eq!(
        page.headers()
            .get("Content-Security-Policy")
            .and_then(|v| v.to_str().ok()),
        Some(cos::flows::PAGE_CSP)
    );
    assert!(
        !cos::flows::PAGE_CSP.contains("connect-src"),
        "the page fetches nothing"
    );
    let html = String::from_utf8_lossy(page.body());
    assert!(
        html.contains("t-ep"),
        "the page reads the app business's projects"
    );
}

#[test]
fn the_flows_frame_runs_script_without_same_origin() {
    let html = std::fs::read_to_string(format!("{ROOT}/apps/desktop/ui/index.html")).unwrap();
    let tag = html
        .split("<iframe")
        .find(|t| t.contains("id=\"flows-frame\""))
        .expect("the flows frame");
    let tag = &tag[..tag.find('>').unwrap()];
    assert!(
        tag.contains("sandbox=\"allow-scripts allow-popups\""),
        "{tag}"
    );
    assert!(!tag.contains("allow-same-origin"), "{tag}");
}

#[test]
fn the_header_holds_the_brand_the_tabs_and_one_business_picker() {
    let html = std::fs::read_to_string(format!("{ROOT}/apps/desktop/ui/index.html")).unwrap();
    let start = html
        .find(r#"<header class="app-header">"#)
        .expect("the header");
    let header = &html[start..start + html[start..].find("</header>").unwrap()];
    for gone in [
        r#"id="brief-run""#,
        r#"id="slots""#,
        "slots busy",
        "workspace</span>",
        r#"id="ws-path""#,
    ] {
        assert!(!header.contains(gone), "{gone} is in the header");
    }
    assert!(header.contains(r#"class="app-nav""#));
    assert!(header.contains(r#"id="dash-tabs""#), "the dashboards");
    assert!(header.contains(r#"id="biz-open""#), "the business picker");
    for gone in [r#"id="settings""#, r#"id="ws-switch""#] {
        assert!(
            !header.contains(gone),
            "{gone}: the picker is the one switch"
        );
    }
    assert!(!html.contains(r#"id="sidebar""#), "no business sidebar");
    assert!(!html.contains("sidebar.js"));
    let picker = &html[html.find(r#"<dialog id="switch-dialog""#).expect("picker")..];
    let picker = &picker[..picker.find("</dialog>").unwrap()];
    assert!(picker.contains(r#"id="ws-name""#), "the business's name");
    assert!(
        picker.contains(r#"<details class="ws-where">"#),
        "its paths, folded"
    );
    assert!(
        picker.contains(r#"id="biz-pin""#),
        "why the window is pinned"
    );

    let section = |id: &str| {
        let at = html.find(&format!(r#"<section id="{id}""#)).expect(id);
        html[at..at + html[at..].find("</section>").unwrap()].to_string()
    };
    // Run brief lives on the report's own tab bar, not in the window.
    assert!(!html.contains(r#"id="brief-run""#));
    assert!(!html.contains(r#"id="brief-state""#));
    assert!(!section("view-today").contains("page-bar"));
    assert!(
        section("view-agents")
            .contains(r#"<span id="slots" class="badge num" data-variant="outline" hidden>"#)
    );

    let js = std::fs::read_to_string(format!("{ROOT}/apps/desktop/ui/app.js")).unwrap();
    assert!(
        js.contains(r#"$("slots").hidden = ui.slots == null;"#),
        "no slots text before paths answers"
    );
    assert!(
        js.contains(r#"$("ws-name").textContent = p.business;"#),
        "the name, never the path"
    );
}

#[test]
fn only_the_report_frame_runs_a_brief_and_passes_it_nothing() {
    let js = std::fs::read_to_string(format!("{ROOT}/apps/desktop/ui/app.js")).unwrap();
    let start = js.find("const reportState").expect("the report handler");
    let handler = &js[start..start + js[start..].find("\n});\n").unwrap()];
    assert!(handler.contains("const frame = $(\"report\").contentWindow;"));
    assert!(handler.contains("if (ev.source !== frame) return;"));
    assert!(handler.contains(r#"{ type: "cos-host", hire: true, runBrief: true }"#));
    assert!(handler.contains("} else if (d.type === \"cos-run-brief\") {\n    runBrief();\n"));
    assert_eq!(
        js.matches("cos-run-brief").count(),
        1,
        "no other listener runs it"
    );
    // Two mentions: its definition and the report handler. Neither takes an argument.
    assert_eq!(js.matches("runBrief(").count(), 2);
    assert!(js.contains("async function runBrief() {"));
    let body = &js[js.find("async function runBrief() {").unwrap()..];
    let body = &body[..body.find("\n}\n").unwrap()];
    assert!(
        body.contains(r#"await invoke("brief_run", { business: ui.business })"#),
        "the window names its own business; nothing comes from the page"
    );
    assert!(body.contains(r#"if (briefNow?.state === "running") return;"#));
}
