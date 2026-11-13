//! The window's commands through Tauri's IPC on the mock runtime, against the
//! real `cos` supervisor binary and a stand-in harness: spawn several agents
//! past the slot limit, watch one queue, tail, stop, clean, switch between
//! businesses and refuse the switches it must, and serve the report on its own
//! scheme.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use cos::business::{Business, Dirs, Sources};
use cos_desktop::Start;
use serde_json::{Value, json};
use tauri::WebviewWindowBuilder;
use tauri::test::{
    INVOKE_KEY, MockRuntime, get_ipc_response, mock_builder, mock_context, noop_assets,
};
use tauri::webview::InvokeRequest;

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
    assert!(
        cos.is_file(),
        "build the supervisor first: cargo build --bin cos"
    );
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
    let app = cos_desktop::builder(mock_builder(), Some(start))
        .build(mock_context(noop_assets()))
        .unwrap();
    let w = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    (app, w)
}

/// A run that never ended under `home`, whose supervisor is `supervisor`; returns its state file.
fn plant_run(home: &Path, id: &str, supervisor: i32) -> PathBuf {
    let dir = home.join("out/agents").join(id);
    std::fs::create_dir_all(&dir).unwrap();
    let spec = json!({
        "id": id, "harness": "claude", "program": "claude", "model": null, "repo": null,
        "base": null, "workdir": dir, "worktree": false, "title": "planted", "source": "test",
        "prompt": "planted", "deadline_secs": 60, "budget_usd": 1.0,
        "created": "2020-01-01T00:00:00Z",
    });
    let state = json!({
        "status": "running", "supervisor": supervisor, "pgid": null, "pgid_started": null,
        "started": null, "ended": null, "exit": null, "cost_usd": null, "tokens": null,
        "tool_calls": 0, "session": null, "last": "", "summary": "", "error": null,
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

fn switch(w: &Win, name: &str) -> Result<Value, Value> {
    call(w, "switch_business", json!({ "args": { "name": name } }))
}

fn error_text(e: &Value) -> &str {
    e.as_str().unwrap()
}

#[test]
fn the_window_spawns_queues_tails_stops_and_cleans_agents() {
    let cos = cos_bin();
    let root = scratch("spawn");
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

    let specs = call(
        &w,
        "agents_spawn",
        json!({ "business": "default", "args": {
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
    let entries = tail["entries"].as_array().unwrap();
    let bash = entries
        .iter()
        .find(|e| e["kind"] == "call")
        .expect("a call");
    assert_eq!(
        (&bash["tool"], &bash["detail"]),
        (&json!("Bash"), &json!("git status --short")),
        "{tail}"
    );
    let again = call(&w, "agents_tail", json!({ "id": running, "from": 0 })).unwrap();
    assert_eq!(
        again["entries"][0]["key"], tail["entries"][0]["key"],
        "the same range keys the same"
    );

    assert_eq!(
        call(
            &w,
            "agents_stop",
            json!({ "business": "default", "id": running })
        )
        .unwrap()["status"],
        "killed"
    );
    wait(&w, &queued, "running");
    assert_eq!(
        call(
            &w,
            "agents_stop",
            json!({ "business": "default", "id": queued })
        )
        .unwrap()["status"],
        "killed"
    );

    for id in &ids {
        call(
            &w,
            "agents_clean",
            json!({ "business": "default", "id": id, "force": true }),
        )
        .unwrap();
    }
    assert_eq!(call(&w, "agents_list", json!({})).unwrap(), json!([]));

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

    let fake = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/fake-claude.sh"
    );
    let refused = call(
        &w,
        "agents_spawn",
        json!({ "business": "default", "args": {
            "harness": "claude", "repo": repo, "role": "chief-of-staff", "deliverable": "D-1",
            "worktree": true, "deadlineSecs": 60, "budgetUsd": 1.0, "count": 1, "program": fake,
        }}),
    )
    .unwrap_err();
    assert!(refused.as_str().unwrap().contains("not hired"), "{refused}");

    let specs = call(
        &w,
        "agents_spawn",
        json!({ "business": "default", "args": {
            "harness": "claude", "repo": repo, "role": "builder", "deliverable": "D-1",
            "worktree": true, "deadlineSecs": 60, "budgetUsd": 1.0, "count": 1, "program": fake,
        }}),
    )
    .unwrap();
    let spec = &specs[0];
    let id = spec["id"].as_str().unwrap().to_owned();
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

    let until = Instant::now() + Duration::from_secs(20);
    let verdict = loop {
        let list = call(&w, "agents_list", json!({})).unwrap();
        let v = list
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["spec"]["id"] == id)
            .map(|r| r["state"]["verdict"].clone())
            .unwrap_or_default();
        if !v.is_null() {
            break v;
        }
        assert!(Instant::now() < until, "never judged");
        std::thread::sleep(Duration::from_millis(100));
    };
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
    assert_eq!(role["median_cost_usd"], 0.12);

    call(
        &w,
        "agents_clean",
        json!({ "business": "default", "id": id, "force": true }),
    )
    .unwrap();
}

#[test]
fn the_dialog_checks_fit_before_hiring_and_a_tester_run_takes_your_result() {
    let cos = cos_bin();
    let root = scratch("fit");
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
    let fake = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/fake-claude.sh"
    );
    let args = |role: &str, extra: Value| {
        let mut a = json!({
            "harness": "claude", "role": role, "worktree": true, "deadlineSecs": 60,
            "budgetUsd": 1.0, "count": 1, "program": fake,
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

    // The stand-in harness writes TEST-PLAN.md only when its prompt says
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
    let row = |w: &Win| {
        call(w, "agents_list", json!({}))
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["spec"]["id"] == id)
            .cloned()
            .unwrap_or_default()
    };
    let until = Instant::now() + Duration::from_secs(20);
    let verdict = loop {
        let v = row(&w)["state"]["verdict"].clone();
        if !v.is_null() {
            break v;
        }
        assert!(Instant::now() < until, "never judged");
        std::thread::sleep(Duration::from_millis(100));
    };
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
        cos_desktop::report_response(app.handle(), "/flows").status(),
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
    json!({
        "harness": "claude", "repo": repo, "prompt": prompt, "worktree": true,
        "deadlineSecs": 60, "budgetUsd": 1.0, "count": 1,
        "program": concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures/fake-claude.sh"),
    })
}

fn wait_judged(w: &Win, id: &str) -> Value {
    let until = Instant::now() + Duration::from_secs(20);
    loop {
        let v = call(w, "agents_list", json!({}))
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["spec"]["id"] == id)
            .map(|r| r["state"]["verdict"].clone())
            .unwrap_or_default();
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
    let mut b = tauri::http::Request::builder()
        .method("POST")
        .uri("cosreport://localhost/docs/organize")
        .header("x-cos", "docs");
    if let Some(name) = business {
        b = b.header("x-cos-business", name);
    }
    let req = b.body(body.to_string().into_bytes()).unwrap();
    cos_desktop::docs_response(app, &req)
}

#[test]
fn writes_land_in_new_business() {
    let cos = cos_bin();
    let root = scratch("writes");
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

    let specs = call(
        &w,
        "agents_spawn",
        json!({ "business": "acme", "args": spawn_args(&repo, "acme prompt") }),
    )
    .unwrap();
    let id = specs[0]["id"].as_str().unwrap().to_owned();
    assert!(
        b.home
            .join("out/agents")
            .join(&id)
            .join("spec.json")
            .is_file(),
        "the run is not under acme"
    );
    assert!(!a.home.join("out/agents").join(&id).exists());

    let verdict = wait_judged(&w, &id);
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
            call(&w, "agents_tail", json!({ "id": id, "from": 0 })),
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
    let a = dirs(&root, "default");
    seed(&a.workspace, None);
    let repo = repo_in(&a.projects, "demo-repo");
    let b = dirs(&root, "acme");
    seed(&b.workspace, Some("Acme launch goal"));

    let start = start(&root, cos, 2, Sources::default());
    register(&start, "default", &a, true);
    register(&start, "acme", &b, false);
    let (_app, w) = open(start);

    let specs = call(
        &w,
        "agents_spawn",
        json!({ "business": "default", "args": spawn_args(&repo, "SLEEP") }),
    )
    .unwrap();
    let id = specs[0]["id"].as_str().unwrap().to_owned();
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
