//! Links out of the app: http, https and mailto go to the default browser;
//! the window's own pages and the dashboards it serves on the loopback
//! address stay in place; every other scheme is refused. The main window is
//! built in setup with both hooks, so the config must leave its creation to
//! the app.
//!
//! Links into the app: `cos://` in a closed grammar, held until the window is
//! ready, refused with the reason when the business, the run or the brief is
//! not there or the window is pinned elsewhere.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use cos::business::{Business, Sources};
use cos_desktop::{Link, Nav, Start, link_kind, open_link, parse_link, send_out};
use serde_json::{Value, json};
use tauri::test::{
    INVOKE_KEY, MockRuntime, get_ipc_response, mock_builder, mock_context, noop_assets,
};
use tauri::webview::InvokeRequest;
use tauri::{Emitter, Listener, WebviewWindowBuilder};

fn url(s: &str) -> tauri::Url {
    tauri::Url::parse(s).unwrap()
}

#[test]
fn http_links_go_to_the_browser_and_other_schemes_are_refused() {
    let mut opened = Vec::new();
    for out in [
        "https://github.com/northwind/vm/pull/44",
        "http://example.com/x",
        "mailto:alex@northwind.example",
    ] {
        assert!(send_out(&url(out), |u| opened.push(u)), "{out}");
    }
    assert_eq!(opened.len(), 3);
    assert_eq!(opened[0], "https://github.com/northwind/vm/pull/44");
    for refused in [
        "file:///etc/passwd",
        "javascript:alert(1)",
        "ftp://example.com/x",
        "vscode://file/x",
    ] {
        assert_eq!(link_kind(&url(refused)), Link::Refused, "{refused}");
        assert!(!send_out(&url(refused), |u| panic!(
            "{u} reached the opener"
        )));
    }
    for in_app in [
        "tauri://localhost/index.html",
        "cosreport://localhost/flows",
        "http://cosreport.localhost/docs",
        "http://tauri.localhost/",
        "http://127.0.0.1:8501/",
        "http://[::1]:3000/board",
        "about:blank",
    ] {
        assert_eq!(link_kind(&url(in_app)), Link::InApp, "{in_app}");
        assert!(!send_out(&url(in_app), |u| panic!("{u} left the app")));
    }
}

#[test]
fn the_main_window_is_built_with_the_link_hooks() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"));
    let conf: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(format!("{root}/tauri.conf.json")).unwrap())
            .unwrap();
    assert_eq!(conf["app"]["windows"][0]["label"], "main");
    assert_eq!(
        conf["app"]["windows"][0]["create"], false,
        "setup builds it"
    );
    let lib = std::fs::read_to_string(format!("{root}/src/lib.rs")).unwrap();
    let setup = &lib[lib
        .find(".filter(|w| !w.create)")
        .expect("setup builds the window")..];
    assert!(setup.contains(".on_navigation(") && setup.contains(".on_new_window("));
    assert!(
        setup.contains("tauri::webview::NewWindowResponse::Deny"),
        "never a window in the app"
    );
}

const RUN: &str = "r-20261126-090000-ab12";

#[test]
fn cos_links_follow_the_closed_grammar() {
    assert_eq!(
        parse_link(&format!("cos://run/acme/{RUN}")),
        Ok(Nav::Run {
            business: "acme".into(),
            run: RUN.into()
        })
    );
    assert_eq!(
        parse_link("cos://brief/north-wind/2026-11-26"),
        Ok(Nav::Brief {
            business: "north-wind".into(),
            day: "2026-11-26".into()
        })
    );
    assert_eq!(
        parse_link("cos://settings/assistants"),
        Ok(Nav::SettingsAssistants)
    );
    assert_eq!(
        serde_json::to_value(Nav::Run {
            business: "acme".into(),
            run: RUN.into()
        })
        .unwrap(),
        json!({ "view": "run", "business": "acme", "run": RUN }),
        "the window.navigate binding"
    );
    assert_eq!(
        serde_json::to_value(Nav::SettingsAssistants).unwrap(),
        json!({ "view": "settings-assistants" })
    );
    let long = format!("cos://run/acme/{RUN}{}", "x".repeat(400));
    for refused in [
        "cos://run/../x".to_owned(),
        format!("cos://run/acme/../{RUN}"),
        format!("cos://runs/acme/{RUN}"),
        format!("cos://jobs/acme/{RUN}"),
        format!("cos://run/acme/{RUN}/extra"),
        format!("cos://run/acme/{RUN}/"),
        "cos://run/acme".to_owned(),
        format!("cos://run//{RUN}"),
        format!("cos://run/acme/{RUN}?open=1"),
        format!("cos://run/acme/{RUN}#top"),
        format!("cos://run/ac%6De/{RUN}"),
        format!("cos://run/Acme/{RUN}"),
        format!("COS://run/acme/{RUN}"),
        format!("cos:run/acme/{RUN}"),
        format!("cos:///run/acme/{RUN}"),
        format!("cos://user@run/acme/{RUN}"),
        "cos://run/acme/r-20261126-090000-AB12".to_owned(),
        "cos://brief/acme/2026-13-40".to_owned(),
        "cos://brief/acme/26-11-2026".to_owned(),
        "cos://settings/assistants/x".to_owned(),
        "cos://settings/general".to_owned(),
        "cos://settings".to_owned(),
        format!("https://run/acme/{RUN}"),
        String::new(),
        long,
    ] {
        let e = parse_link(&refused).expect_err(&refused);
        assert!(!e.is_empty(), "{refused}: a refusal names why");
        assert!(
            e.chars().count() <= 300,
            "{refused}: the URL in the refusal is cut to 200 chars: {e}"
        );
    }
}

type Win = tauri::WebviewWindow<MockRuntime>;

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("cos-links-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d.canonicalize().unwrap()
}

/// One business's directories, its workspace initialised so a switch to it is admitted.
fn business(root: &Path, name: &str) -> Business {
    let base = root.join(name);
    let b = Business {
        home: base.join("home"),
        workspace: base.join("ws"),
        projects: base.join("projects"),
    };
    for d in [&b.home, &b.workspace, &b.projects] {
        std::fs::create_dir_all(d).unwrap();
    }
    cos::state::initialize(&b.workspace).unwrap();
    b
}

fn start(root: &Path, pins: Sources) -> Start {
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
        sources: pins,
        cos_bin: None,
        slots: Some(1),
        daemon_dir: std::env::temp_dir().join(format!("cosl-{}-none", std::process::id())),
    }
}

/// A finished run record under `home`.
fn record(home: &Path, id: &str, business: &str) {
    let dir = home.join(cos::runs::DIR);
    std::fs::create_dir_all(&dir).unwrap();
    let r = json!({
        "id": id, "kind": "brief", "business": business, "state": "done", "owner": "daemon",
        "started": 1764061702, "ended": 1764061802, "day": "2026-11-26",
    });
    std::fs::write(dir.join(format!("{id}.json")), r.to_string()).unwrap();
}

type Events = std::sync::mpsc::Receiver<Value>;

/// The window on `start`, with a listener on `link-open` from before setup.
fn open(start: Start) -> (tauri::App<MockRuntime>, Win, Events) {
    let mut app = cos_desktop::builder(mock_builder(), Some(start))
        .build(mock_context(noop_assets()))
        .unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    app.listen_any("link-open", move |e| {
        let _ = tx.send(serde_json::from_str(e.payload()).unwrap());
    });
    #[allow(deprecated)]
    app.run_iteration(|_, _| {});
    let w = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    (app, w, rx)
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

fn next(rx: &Events, within: Duration) -> Option<Value> {
    let until = Instant::now() + within;
    rx.recv_timeout(until.saturating_duration_since(Instant::now()))
        .ok()
}

/// Two registered businesses, `acme` active with a finished run, and `beta`
/// with a finished run and a brief.
fn two(root: &Path, pins: Sources) -> (Start, Business, Business) {
    let s = start(root, pins);
    let (a, b) = (business(root, "acme"), business(root, "beta"));
    cos::business::add(&s.registry, "acme", a.clone(), false).unwrap();
    cos::business::add(&s.registry, "beta", b.clone(), false).unwrap();
    cos::business::set_active(&s.registry, "acme").unwrap();
    record(&a.home, RUN, "acme");
    record(&b.home, "r-20261126-091500-cd34", "beta");
    let briefs = b.home.join("out/briefs");
    std::fs::create_dir_all(&briefs).unwrap();
    std::fs::write(briefs.join("2026-11-20.json"), "{}").unwrap();
    (s, a, b)
}

#[test]
fn a_cold_start_link_is_held_until_the_window_is_ready_then_applied() {
    let root = scratch("cold");
    let (s, _a, _b) = two(&root, Sources::default());
    let (app, w, rx) = open(s);

    open_link(app.handle(), &format!("cos://run/acme/{RUN}"));
    assert_eq!(
        next(&rx, Duration::from_millis(300)),
        None,
        "nothing reaches a window that has not said it is ready"
    );
    let held = call(&w, "link_ready", json!({})).unwrap();
    assert_eq!(held["view"], "run", "{held}");
    assert_eq!(held["business"], "acme");
    assert_eq!(held["run"], RUN);
    assert_eq!(held["shown"], true, "{held}");
    assert_eq!(
        call(&w, "link_ready", json!({})).unwrap(),
        Value::Null,
        "a held link is applied once"
    );

    open_link(app.handle(), "cos://settings/assistants");
    let live = next(&rx, Duration::from_secs(2)).expect("a link while running reaches the window");
    assert_eq!(live["view"], "settings-assistants");
    assert_eq!(live["shown"], true);

    open_link(app.handle(), "cos://run/acme/../../etc/passwd");
    assert_eq!(
        next(&rx, Duration::from_millis(300)),
        None,
        "a link outside the grammar is ignored"
    );
    assert_eq!(call(&w, "link_ready", json!({})).unwrap(), Value::Null);
}

#[test]
fn the_deep_link_plugin_routes_an_opened_url_into_the_held_queue() {
    let root = scratch("plugin");
    let (s, _a, _b) = two(&root, Sources::default());
    let mut app = cos_desktop::with_deep_links(cos_desktop::builder(mock_builder(), Some(s)))
        .build(mock_context(noop_assets()))
        .unwrap();
    #[allow(deprecated)]
    app.run_iteration(|_, _| {});
    let w = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();

    // The event the plugin emits on macOS for `RunEvent::Opened`.
    app.emit("deep-link://new-url", vec![format!("cos://run/acme/{RUN}")])
        .unwrap();
    let held = call(&w, "link_ready", json!({})).unwrap();
    assert_eq!(held["view"], "run", "{held}");
    assert_eq!(held["run"], RUN);
    assert_eq!(held["shown"], true, "{held}");
}

#[test]
fn a_link_naming_another_business_switches_the_window_only() {
    let root = scratch("switch");
    let (s, _a, _b) = two(&root, Sources::default());
    let registry = s.registry.clone();
    let (app, w, rx) = open(s);
    call(&w, "link_ready", json!({})).unwrap();

    open_link(app.handle(), "cos://run/beta/r-20261126-091500-cd34");
    let shown = next(&rx, Duration::from_secs(2)).expect("link-open");
    assert_eq!(shown["shown"], true, "{shown}");
    assert_eq!(shown["business"], "beta");
    assert_eq!(
        call(&w, "paths", json!({})).unwrap()["business"],
        "acme",
        "the window switches, not the link: one that declines (unsaved Hire edits) stays on acme"
    );
    let switched = call(&w, "switch_business", json!({ "args": { "name": "beta" } })).unwrap();
    assert_eq!(switched["business"], "beta");
    assert_eq!(
        cos::business::load(&registry).unwrap().active.as_deref(),
        Some("acme"),
        "nothing persists: the registry's default stays"
    );

    open_link(app.handle(), "cos://brief/beta/2026-11-20");
    assert_eq!(
        next(&rx, Duration::from_millis(300)),
        None,
        "the switch reloads the window: a link waits until it listens again"
    );
    let brief = call(&w, "link_ready", json!({})).unwrap();
    assert_eq!(brief["view"], "brief");
    assert_eq!(brief["day"], "2026-11-20");
    assert_eq!(brief["shown"], true, "{brief}");
}

#[test]
fn a_pinned_window_refuses_a_link_to_another_business_and_says_why() {
    let root = scratch("pinned");
    let pins = Sources {
        business: Some("acme".into()),
        ..Sources::default()
    };
    let (s, _a, _b) = two(&root, pins);
    let (app, w, rx) = open(s);
    call(&w, "link_ready", json!({})).unwrap();

    open_link(app.handle(), "cos://run/beta/r-20261126-091500-cd34");
    let refused = next(&rx, Duration::from_secs(2)).expect("the refusal reaches the window");
    assert_eq!(refused["shown"], false, "{refused}");
    assert!(
        refused["refused"]
            .as_str()
            .unwrap()
            .contains("pinned by COS_BUSINESS"),
        "{refused}"
    );
    assert_eq!(call(&w, "paths", json!({})).unwrap()["business"], "acme");

    open_link(app.handle(), &format!("cos://run/acme/{RUN}"));
    let own = next(&rx, Duration::from_secs(2)).expect("link-open");
    assert_eq!(
        own["shown"], true,
        "its own business needs no switch: {own}"
    );
}

#[test]
fn links_to_what_is_not_there_are_refused_with_the_reason() {
    let root = scratch("missing");
    let (s, _a, _b) = two(&root, Sources::default());
    let (app, w, rx) = open(s);
    call(&w, "link_ready", json!({})).unwrap();

    for (url, why) in [
        (format!("cos://run/zeta/{RUN}"), "not registered"),
        ("cos://run/acme/r-20261126-235959-zz99".to_owned(), "no run"),
        (format!("cos://run/beta/{RUN}"), "no run"),
        ("cos://brief/acme/2026-01-02".to_owned(), "no brief"),
    ] {
        open_link(app.handle(), &url);
        let r = next(&rx, Duration::from_secs(2)).unwrap_or_else(|| panic!("{url}: no reply"));
        assert_eq!(r["shown"], false, "{url}: {r}");
        assert!(r["refused"].as_str().unwrap().contains(why), "{url}: {r}");
        assert_eq!(
            call(&w, "paths", json!({})).unwrap()["business"],
            "acme",
            "{url}: a refused link never switches"
        );
    }
}

#[test]
fn the_app_registers_the_cos_scheme_and_installs_checking_it() {
    let root = env!("CARGO_MANIFEST_DIR");
    let read = |rel: &str| std::fs::read_to_string(format!("{root}/{rel}")).unwrap();
    let plist = read("bundle/Info.plist");
    let types = &plist[plist
        .find("<key>CFBundleURLTypes</key>")
        .expect("Info.plist declares URL types")..];
    assert!(
        types.contains("<key>CFBundleURLSchemes</key>") && types.contains("<string>cos</string>"),
        "{types}"
    );
    let manifest = read("Cargo.toml");
    assert!(
        manifest.contains("tauri-plugin-deep-link = \"2.6\""),
        "{manifest}"
    );
    assert!(
        manifest.contains("tauri-plugin-single-instance = { version = \"2.4\""),
        "{manifest}"
    );
    let lib = read("src/lib.rs");
    let links = &lib[lib.find("pub fn with_links").expect("with_links")..];
    assert!(
        links.contains("tauri_plugin_single_instance::init")
            && links.contains("tauri_plugin_deep_link::init")
            && links.contains("on_open_url"),
        "the real app registers both plugins and routes their URLs"
    );
    assert!(read("src/main.rs").contains("with_links("));
    let task = std::fs::read_to_string(format!("{root}/../../Taskfile.yml")).unwrap();
    let app = &task[task.find("  desktop-app:").unwrap()..];
    let app = &app[..app.find("\n  desktop-check:").unwrap()];
    assert!(app.contains("lsregister -f"), "{app}");
    assert!(
        app.contains("-dump URLSchemeBinding") && app.contains("dev.decebal.cos"),
        "the last step reads the scheme back for this bundle: {app}"
    );
}

/// The body of `function <name>(` in `src`, up to the next top-level `}`.
fn js_fn<'a>(src: &'a str, name: &str) -> &'a str {
    let at = src
        .find(&format!("function {name}("))
        .unwrap_or_else(|| panic!("no function {name}"));
    let body = &src[at..];
    &body[..body.find("\n}\n").expect("function end")]
}

#[test]
fn a_link_never_discards_unsaved_hire_edits_and_a_refusal_leaves_no_archived_day() {
    let app = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/ui/app.js")).unwrap();
    let open = js_fn(&app, "openLink");
    let guard = open
        .find("hireHasEdits()")
        .expect("openLink checks Hire edits");
    let switch = open.find("switchTo(").expect("openLink switches");
    assert!(
        guard < switch,
        "the Hire check comes before the switch: {open}"
    );
    assert!(
        !open.contains("pendingDiscard"),
        "a link never arms the picker's discard: {open}"
    );
    let refused = js_fn(&app, "showLinkRefused");
    assert!(
        refused.contains("reloadReport()"),
        "a refusal over an archived day reloads the report: {refused}"
    );
}
