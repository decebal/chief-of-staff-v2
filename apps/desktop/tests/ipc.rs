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

    let org = json!({
        "topics": { "VM SDK": ["demo-repo:README.md"] },
        "links": [],
        "marks": { "demo-repo:README.md": "keep_in_repo" }
    });
    call(&w, "docs_save", json!({ "organize": org })).unwrap();
    let saved = cos::docs::load_organize(&home).unwrap();
    assert_eq!(serde_json::to_value(&saved).unwrap(), org);
    let refused = call(&w, "docs_save", json!({ "organize": { "tpoics": {} } })).unwrap_err();
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
    let cos = Path::new(ROOT).join("target/debug/cos");
    assert!(
        cos.is_file(),
        "build the supervisor first: cargo build --bin cos"
    );
    let root = std::env::temp_dir().join(format!("cos-desktop-staff-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let root = root.canonicalize().unwrap();
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
        slots: Some(2),
        cos: Some(cos),
    };
    let app = cos_desktop::builder(mock_builder(), Some(settings))
        .build(mock_context(noop_assets()))
        .unwrap();
    let w = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();

    let staff = call(&w, "agents_roles", json!({})).unwrap();
    let roles = staff["roles"].as_array().unwrap();
    assert_eq!(roles.len(), 8, "{staff}");
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
        json!({ "args": {
            "harness": "claude", "repo": repo, "role": "chief-of-staff", "deliverable": "D-1",
            "worktree": true, "deadlineSecs": 60, "budgetUsd": 1.0, "count": 1, "program": fake,
        }}),
    )
    .unwrap_err();
    assert!(refused.as_str().unwrap().contains("not hired"), "{refused}");

    let specs = call(
        &w,
        "agents_spawn",
        json!({ "args": {
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

    let again = call(&w, "agents_rejudge", json!({ "id": id })).unwrap();
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

    call(&w, "agents_clean", json!({ "id": id, "force": true })).unwrap();
}

#[test]
fn the_dialog_checks_fit_before_hiring_and_a_tester_run_takes_your_result() {
    let cos = Path::new(ROOT).join("target/debug/cos");
    assert!(
        cos.is_file(),
        "build the supervisor first: cargo build --bin cos"
    );
    let root = std::env::temp_dir().join(format!("cos-desktop-fit-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let root = root.canonicalize().unwrap();
    let (home, ws, projects) = (root.join("home"), root.join("ws"), root.join("projects"));
    for d in [&home, &ws, &projects] {
        std::fs::create_dir_all(d).unwrap();
    }
    cos::state::initialize(&ws).unwrap();
    let mut st: Value = serde_json::from_str(cos::state::DEMO_STATE).unwrap();
    st["goals"][0]["whereItLives"] = json!("acme/demo-repo (main)");
    let mut d = st["deliverables"][0].clone();
    d["id"] = json!("D-2");
    d["title"] = json!("QA-9: Test the published package against the staging video conference");
    d["doneWhen"] = json!("QA-9 resolved in Jira");
    st["deliverables"].as_array_mut().unwrap().push(d);
    cos::state::commit(&ws, &st, 0, "Fictional fixture").unwrap();
    let repo = projects.join("demo-repo");
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

    let settings = cos_desktop::Settings {
        home: home.clone(),
        workspace: ws.clone(),
        projects: projects.clone(),
        slots: Some(2),
        cos: Some(cos),
    };
    let app = cos_desktop::builder(mock_builder(), Some(settings))
        .build(mock_context(noop_assets()))
        .unwrap();
    let w = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
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
        json!({ "args": a })
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
    let row = |w: &tauri::WebviewWindow<tauri::test::MockRuntime>| {
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
        json!({ "args": { "id": id, "outcome": "passed", "notes": "12 of 12 steps passed",
                          "evidence": ["https://ci.example/run/7"] } }),
    )
    .unwrap();
    assert_eq!(st["recorded"]["outcome"], "passed");
    assert_eq!(
        row(&w)["state"]["recorded"]["notes"],
        "12 of 12 steps passed"
    );
    call(&w, "agents_clean", json!({ "id": id, "force": true })).unwrap();
}
