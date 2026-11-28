//! Local mods through the window: the tabs' list, the `cosmod` scheme and the
//! data commands, on the mock runtime with a temporary mods folder.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use cos::business::Sources;
use cos_desktop::Start;
use serde_json::{Value, json};
use tauri::test::{
    INVOKE_KEY, MockRuntime, get_ipc_response, mock_builder, mock_context, noop_assets,
};
use tauri::webview::InvokeRequest;
use tauri::{Manager, WebviewWindowBuilder};

type Win = tauri::WebviewWindow<MockRuntime>;
type Reply = tauri::http::Response<Vec<u8>>;

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("cos-mods-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d.canonicalize().unwrap()
}

/// A socket directory nothing listens in, short enough for `sun_path`.
fn no_daemon() -> PathBuf {
    static N: AtomicUsize = AtomicUsize::new(0);
    std::env::temp_dir().join(format!(
        "cosm-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ))
}

fn start(root: &Path) -> Start {
    let install = root.join("install");
    std::fs::create_dir_all(&install).unwrap();
    Start {
        install: install.canonicalize().unwrap(),
        user_home: Some(root.join("user")),
        codex_home: Some(root.join("codex")),
        registry: root.join("businesses.toml"),
        settings_file: root.join("settings.json"),
        data_dir: root.join("data"),
        log_dir: root.join("logs"),
        ui_state_file: root.join("ui-state.json"),
        dashboards_file: None,
        sources: Sources::default(),
        cos_bin: None,
        slots: Some(1),
        daemon_dir: no_daemon(),
        mods_dir: root.join("mods"),
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

fn write_mod(mods: &Path, name: &str, manifest: &str) -> PathBuf {
    let dir = mods.join(name);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("mod.json"), manifest).unwrap();
    std::fs::write(
        dir.join("index.html"),
        format!("<!doctype html><title>{name}</title>"),
    )
    .unwrap();
    dir
}

fn cosmod(app: &tauri::App<MockRuntime>, method: &str, path: &str) -> Reply {
    let req = tauri::http::Request::builder()
        .method(method)
        .uri(format!("cosmod://localhost{path}"))
        .body(Vec::new())
        .unwrap();
    cos_desktop::mods_response(app.handle(), &req)
}

fn header<'a>(r: &'a Reply, name: &str) -> &'a str {
    r.headers()[name].to_str().unwrap()
}

#[test]
fn the_tabs_list_each_mod_folder_in_order_and_show_a_bad_one_with_its_error() {
    let root = scratch("list");
    let mods = root.join("mods");
    write_mod(
        &mods,
        "notes",
        r#"{"title":"Notes","icon":"notebook","order":2}"#,
    );
    write_mod(&mods, "counter", r#"{"title":"Counter"}"#);
    write_mod(&mods, "broken", "{not json");
    write_mod(&mods, "extra", r#"{"title":"Extra","surprise":1}"#);
    write_mod(&mods, "Upper", r#"{"title":"Ignored"}"#);
    std::fs::create_dir_all(mods.join("bare")).unwrap();
    std::fs::write(mods.join("bare/mod.json"), r#"{"title":"Bare"}"#).unwrap();
    std::fs::write(mods.join("loose.html"), "not a folder").unwrap();
    let (_app, w) = open(start(&root));

    let rows = call(&w, "mods_list", json!({})).unwrap();
    let rows = rows.as_array().unwrap();
    let names: Vec<&str> = rows.iter().map(|r| r["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["notes", "bare", "broken", "counter", "extra"]);
    assert_eq!(rows[0]["title"], "Notes");
    assert_eq!(rows[0]["icon"], "notebook");
    assert!(rows[0]["error"].is_null());
    assert_eq!(rows[3]["title"], "Counter");
    assert!(
        rows[1]["error"]
            .as_str()
            .unwrap()
            .contains("index.html is missing")
    );
    assert!(rows[2]["error"].as_str().unwrap().contains("mod.json"));
    assert!(rows[4]["error"].as_str().unwrap().contains("surprise"));
    assert!(
        mods.join("AGENTS.md").is_file(),
        "the guide is written on start"
    );
}

#[test]
fn a_mod_page_loads_only_its_own_files_under_a_strict_policy() {
    let root = scratch("scheme");
    let mods = root.join("mods");
    write_mod(&mods, "notes", r#"{"title":"Notes"}"#);
    std::fs::write(mods.join("notes/app.js"), "window.ok = true;").unwrap();
    let secret = root.join("secret.txt");
    std::fs::write(&secret, "outside the folder").unwrap();
    std::os::unix::fs::symlink(&secret, mods.join("notes/leak.txt")).unwrap();
    let (app, _w) = open(start(&root));

    let page = cosmod(&app, "GET", "/notes/index.html");
    assert_eq!(page.status(), 200);
    assert_eq!(header(&page, "Content-Type"), "text/html; charset=utf-8");
    let csp = header(&page, "Content-Security-Policy");
    assert!(
        csp.contains("default-src 'self' cosmod:") && csp.contains("connect-src 'none'"),
        "{csp}"
    );
    assert_eq!(header(&page, "X-Content-Type-Options"), "nosniff");
    assert_eq!(cosmod(&app, "GET", "/notes/").status(), 200);
    let script = cosmod(&app, "GET", "/notes/app.js");
    assert_eq!(
        header(&script, "Content-Type"),
        "text/javascript; charset=utf-8"
    );
    let tokens = cosmod(&app, "GET", "/notes/tokens.css");
    assert_eq!(tokens.status(), 200);
    assert!(String::from_utf8_lossy(tokens.body()).contains(":root"));
    let face = cosmod(&app, "GET", "/notes/vendor/geist/Geist-Variable.woff2");
    assert_eq!(header(&face, "Content-Type"), "font/woff2");

    assert_eq!(cosmod(&app, "GET", "/notes/../secret.txt").status(), 403);
    assert_eq!(
        cosmod(&app, "GET", "/notes/%2e%2e/secret.txt").status(),
        400
    );
    assert_eq!(cosmod(&app, "GET", "/notes/leak.txt").status(), 403);
    assert_eq!(cosmod(&app, "GET", "/nope/index.html").status(), 404);
    assert_eq!(cosmod(&app, "GET", "/notes/missing.js").status(), 404);
    assert_eq!(cosmod(&app, "POST", "/notes/index.html").status(), 405);
}

#[test]
fn mod_data_round_trips_for_the_app_business_and_refuses_what_it_must() {
    let root = scratch("data");
    let install = root.join("install");
    std::fs::create_dir_all(&install).unwrap();
    cos::state::initialize(&install).unwrap();
    let (_app, w) = open(start(&root));

    assert!(
        call(&w, "mod_read", json!({ "name": "notes" }))
            .unwrap()
            .is_null()
    );
    call(
        &w,
        "mod_save",
        json!({ "name": "notes", "data": { "items": [1, 2], "note": "hi" } }),
    )
    .unwrap();
    assert_eq!(
        call(&w, "mod_read", json!({ "name": "notes" })).unwrap(),
        json!({ "items": [1, 2], "note": "hi" })
    );
    assert!(install.join(".chief-of-staff/mods/notes.json").is_file());

    let array = call(&w, "mod_save", json!({ "name": "notes", "data": [1] })).unwrap_err();
    assert!(
        array.as_str().unwrap().contains("one JSON object"),
        "{array}"
    );
    let big = json!({ "blob": "x".repeat(300_000) });
    let big = call(&w, "mod_save", json!({ "name": "notes", "data": big })).unwrap_err();
    assert!(big.as_str().unwrap().contains("limit"), "{big}");
    assert!(call(&w, "mod_save", json!({ "name": "../up", "data": {} })).is_err());
    assert!(call(&w, "mod_read", json!({ "name": "Notes" })).is_err());

    std::fs::remove_dir_all(install.join(".chief-of-staff")).unwrap();
    let unset = call(&w, "mod_save", json!({ "name": "notes", "data": {} })).unwrap_err();
    assert!(
        unset.as_str().unwrap().contains("Set up the workspace"),
        "{unset}"
    );
}

#[test]
fn the_frame_navigates_only_to_the_mod_it_shows() {
    let root = scratch("nav");
    let (app, w) = open(start(&root));
    let url = |s: &str| tauri::Url::parse(s).unwrap();
    let shown = app.state::<cos_desktop::mods::Shown>();
    assert!(!shown.allows(&url("cosmod://localhost/notes/index.html")));

    assert!(call(&w, "mod_open", json!({ "name": "Notes" })).is_err());
    call(&w, "mod_open", json!({ "name": "notes" })).unwrap();
    assert!(shown.allows(&url("cosmod://localhost/notes/index.html?load=1")));
    assert!(shown.allows(&url("http://cosmod.localhost/notes/index.html")));
    assert!(!shown.allows(&url("cosmod://localhost/counter/index.html")));
    assert!(!shown.allows(&url("http://cosmod.localhost/counter/index.html")));
    assert!(!shown.allows(&url("cosmod://localhost/")));
    assert!(shown.allows(&url("cosreport://localhost/")));
    assert!(shown.allows(&url("https://example.com/")));
}

#[test]
fn the_mod_frame_has_an_opaque_origin_and_takes_replies_from_its_own_window_only() {
    let ui = Path::new(env!("CARGO_MANIFEST_DIR")).join("ui");
    let html = std::fs::read_to_string(ui.join("index.html")).unwrap();
    let frame = html
        .lines()
        .find(|l| l.contains(r#"id="mod-frame""#))
        .expect("the mod frame");
    assert!(
        frame.contains(r#"sandbox="allow-scripts allow-popups""#)
            && !frame.contains("allow-same-origin"),
        "{frame}"
    );
    let js = std::fs::read_to_string(ui.join("mods.js")).unwrap();
    assert!(
        js.contains("ev.source !== frame.contentWindow"),
        "mods.js checks the source"
    );
    assert!(js.contains(r#"invoke("mod_save""#) && js.contains(r#"invoke("mod_read""#));
}
