//! The window's role commands through Tauri's IPC on the mock runtime: the
//! staff with its digest, problems and library; saving through the one write
//! path; and a proposal the window owns, ended by Cancel or by quitting.
//! The model is cos's `tests/fixtures/fake-claude-proposal.sh`.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use cos::business::{Business, Sources};
use cos_desktop::Start;
use serde_json::{Value, json};
use tauri::test::{
    INVOKE_KEY, MockRuntime, get_ipc_response, mock_builder, mock_context, noop_assets,
};
use tauri::webview::InvokeRequest;
use tauri::{Listener, WebviewWindowBuilder};

type Win = tauri::WebviewWindow<MockRuntime>;

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("cos-desktop-roles-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d.canonicalize().unwrap()
}

fn cos_bin() -> PathBuf {
    let cos = Path::new(ROOT).join("target/debug/cos");
    assert!(cos.is_file(), "build cos first: cargo build --bin cos");
    cos
}

fn business(root: &Path) -> Business {
    let b = Business {
        home: root.join("home"),
        workspace: root.join("ws"),
        projects: root.join("projects"),
    };
    for d in [&b.home, &b.workspace, &b.projects] {
        std::fs::create_dir_all(d).unwrap();
    }
    b
}

fn open(root: &Path) -> (tauri::App<MockRuntime>, Win, Business) {
    open_with(root, Some(cos_bin()))
}

fn open_with(root: &Path, cos: Option<PathBuf>) -> (tauri::App<MockRuntime>, Win, Business) {
    let b = business(root);
    let install = root.join("install");
    std::fs::create_dir_all(&install).unwrap();
    let start = Start {
        install: install.canonicalize().unwrap(),
        user_home: Some(root.join("user")),
        registry: root.join("businesses.toml"),
        settings_file: root.join("settings.json"),
        data_dir: root.join("data"),
        log_dir: root.join("logs"),
        ui_state_file: root.join("ui-state.json"),
        dashboards_file: None,
        sources: Sources::default(),
        cos_bin: cos,
        slots: Some(2),
    };
    cos::business::add(&start.registry, "acme", b.clone(), false).unwrap();
    cos::business::set_active(&start.registry, "acme").unwrap();
    let mut app = cos_desktop::builder(mock_builder(), Some(start))
        .build(mock_context(noop_assets()))
        .unwrap();
    #[allow(deprecated)]
    app.run_iteration(|_, _| {});
    let w = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    (app, w, b)
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

fn analyst() -> Value {
    json!({ "id": "analyst", "title": "Analyst", "mandate": "Answer from the data given." })
}

#[test]
fn the_staff_carries_its_digest_problems_and_the_library() {
    let root = scratch("staff");
    let (_app, w, b) = open(&root);
    let staff = call(&w, "agents_roles", json!({})).unwrap();
    assert_eq!(staff["digest"], "absent");
    assert_eq!(staff["library"].as_array().unwrap().len(), 12, "{staff}");
    assert!(staff["roles"][1]["problems"].is_array());

    let mut p = cos::agents::policy::seed();
    let mut odd = cos::agents::policy::Role {
        id: "odd".into(),
        title: "Odd".into(),
        mandate: "x".into(),
        ..Default::default()
    };
    odd.work = vec!["juggling".into()];
    p.roles.push(odd);
    std::fs::create_dir_all(b.home.join("context")).unwrap();
    std::fs::write(
        cos::agents::policy::path(&b.home),
        serde_json::to_string(&p).unwrap(),
    )
    .unwrap();
    let staff = call(&w, "agents_roles", json!({})).unwrap();
    let odd = staff["roles"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["role"]["id"] == "odd")
        .unwrap();
    assert_eq!(odd["problems"][0]["field"], "work", "{odd}");
    assert_ne!(staff["digest"], "absent");

    let e = call(&w, "roles_library_entry", json!({ "id": "sre" })).unwrap();
    assert_eq!(e["role"]["id"], "sre");
}

#[test]
fn saving_goes_through_the_write_path_and_its_refusals() {
    let root = scratch("save");
    let (_app, w, b) = open(&root);
    let path = cos::agents::policy::path(&b.home);
    let save = |change: Value, digest: &str, allow: bool| {
        call(
            &w,
            "roles_save",
            json!({ "business": "acme", "change": change, "digest": digest, "allowContract": allow }),
        )
    };

    let foreign = call(
        &w,
        "roles_save",
        json!({ "business": "other", "change": { "add": analyst() }, "digest": "absent", "allowContract": false }),
    );
    assert!(
        foreign.is_err() && !path.exists(),
        "a foreign business writes nothing"
    );

    let wrote = save(json!({ "add": analyst() }), "absent", false).unwrap();
    let digest = wrote["digest"].as_str().unwrap().to_owned();
    assert!(wrote["notes"][0].as_str().unwrap().contains("template"));

    let stale = save(json!({ "retire": "analyst" }), "absent", false).unwrap_err();
    assert!(
        stale.to_string().contains("changed since you read it"),
        "{stale}"
    );

    let mut checked = analyst();
    checked["checks"] = json!(["test -s ANSWER.md"]);
    let replace = json!({ "replace": { "id": "analyst", "role": checked } });
    let before = std::fs::read(&path).unwrap();
    let refused = save(replace.clone(), &digest, false).unwrap_err();
    assert!(refused.to_string().contains("contract"), "{refused}");
    assert_eq!(std::fs::read(&path).unwrap(), before);
    save(replace, &digest, true).unwrap();

    let mut bad = analyst();
    bad["id"] = json!("escape");
    bad["output"] = json!("../x.md");
    let digest = call(&w, "agents_roles", json!({})).unwrap()["digest"]
        .as_str()
        .unwrap()
        .to_owned();
    let invalid = save(json!({ "add": bad }), &digest, false).unwrap_err();
    assert!(invalid.to_string().contains("output"), "{invalid}");
}

#[test]
fn without_a_cos_binary_nothing_is_proposed_unguarded() {
    let root = scratch("no-cos");
    let (_app, w, _b) = open_with(&root, None);
    let args = json!({ "business": "acme", "id": "sre", "newId": null, "brief": null });
    let e = call(&w, "roles_propose", args).unwrap_err();
    assert!(e.to_string().contains("no cos binary"), "{e}");
}

#[test]
fn cancel_with_no_proposal_running_does_nothing() {
    let root = scratch("cancel-idle");
    let (_app, w, _b) = open(&root);
    call(&w, "roles_propose_cancel", json!({})).unwrap();
    call(&w, "roles_propose_cancel", json!({})).unwrap();
}

fn pid(dir: &Path, name: &str) -> libc::pid_t {
    let at = Instant::now();
    loop {
        if let Ok(s) = std::fs::read_to_string(dir.join(name))
            && let Ok(n) = s.trim().parse()
        {
            return n;
        }
        assert!(at.elapsed() < Duration::from_secs(15), "no {name} pid");
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn gone_within(p: libc::pid_t, limit: Duration) -> bool {
    let at = Instant::now();
    while at.elapsed() < limit {
        if !cos::proc::alive(p) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

/// Every proposal case in one test: the stand-in's mode is process-wide.
#[test]
fn a_proposal_is_owned_by_the_window_and_ends_on_cancel_and_on_quit() {
    let root = scratch("propose");
    let (app, w, _b) = open(&root);
    let pids = root.join("pids");
    std::fs::create_dir_all(&pids).unwrap();
    // SAFETY: only this test in this binary reads or sets these.
    unsafe {
        std::env::set_var(
            "COS_CLAUDE",
            Path::new(ROOT).join("tests/fixtures/fake-claude-proposal.sh"),
        );
        std::env::set_var("FAKE_PID_DIR", &pids);
        std::env::set_var("FAKE_PROPOSAL", "ok");
    }
    let (tx, rx) = mpsc::channel::<Value>();
    w.listen("role-proposal", move |e| {
        let _ = tx.send(serde_json::from_str(e.payload()).unwrap());
    });
    let args = json!({ "business": "acme", "id": "performance-engineer", "newId": null, "brief": "Rust services" });

    let sent = call(&w, "roles_propose_prompt", args.clone()).unwrap();
    assert!(sent.as_str().unwrap().contains("Rust services"), "{sent}");

    call(&w, "roles_propose", args.clone()).unwrap();
    let got = rx.recv_timeout(Duration::from_secs(30)).unwrap();
    assert_eq!(got["ok"], true, "{got}");
    assert_eq!(
        got["proposal"]["role"]["checks"],
        json!(
            cos::agents::roles::entry("performance-engineer")
                .unwrap()
                .role
                .checks
        )
    );

    // SAFETY: as above.
    unsafe { std::env::set_var("FAKE_PROPOSAL", "slow") };
    call(&w, "roles_propose", args.clone()).unwrap();
    let (me, kid) = (pid(&pids, "self"), pid(&pids, "child"));
    let twice = call(&w, "roles_propose", args.clone()).unwrap_err();
    assert!(twice.to_string().contains("already running"), "{twice}");
    call(&w, "roles_propose_cancel", json!({})).unwrap();
    let got = rx.recv_timeout(Duration::from_secs(10)).unwrap();
    assert_eq!(got["cancelled"], true, "{got}");
    assert!(gone_within(me, Duration::from_secs(3)) && gone_within(kid, Duration::from_secs(3)));

    std::fs::remove_file(pids.join("self")).unwrap();
    std::fs::remove_file(pids.join("child")).unwrap();
    call(&w, "roles_propose", args).unwrap();
    let (me, kid) = (pid(&pids, "self"), pid(&pids, "child"));
    cos_desktop::on_run_event(app.handle(), tauri::RunEvent::Exit);
    assert!(
        gone_within(me, Duration::from_secs(3)),
        "quit left the proposal's claude"
    );
    assert!(
        gone_within(kid, Duration::from_secs(3)),
        "quit left its child"
    );
}
