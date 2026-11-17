//! Links out of the app: http, https and mailto go to the default browser;
//! the window's own pages and the dashboards it serves on the loopback
//! address stay in place; every other scheme is refused. The main window is
//! built in setup with both hooks, so the config must leave its creation to
//! the app.

use cos_desktop::{Link, link_kind, send_out};

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
