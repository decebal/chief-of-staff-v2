//! The window's UI in an ordinary browser, over the real commands on Tauri's
//! mock runtime, for screenshots and console checks of every state:
//!
//! ```sh
//! cargo run -p cos-desktop --example preview -- /tmp/cos-demo 8793
//! ```
//!
//! then open <http://127.0.0.1:8793/>. It serves `ui/` as the app bundles it,
//! answers `invoke` on `POST /invoke/<cmd>`, and serves the report and the
//! docs room under `/cosreport/` the way the app's own scheme does. Bound to
//! 127.0.0.1; it reads and spawns in the home it is given, so point it at the
//! demo home, never a real one. `COS_PREVIEW_HARNESS=<path>` hires with
//! that stand-in (`tests/fixtures/fake-claude.sh`) instead of a paid run.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};

use serde_json::Value;
use tauri::WebviewWindowBuilder;
use tauri::test::{
    INVOKE_KEY, MockRuntime, get_ipc_response, mock_builder, mock_context, noop_assets,
};
use tauri::webview::InvokeRequest;

const UI: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/ui");

const SHIM: &str = r#"window.__TAURI__ = {
  core: {
    invoke: async (cmd, args = {}) => {
      const r = await fetch(`/invoke/${cmd}`, { method: "POST", body: JSON.stringify(args) });
      const v = await r.json();
      if (!r.ok) throw v;
      return v;
    },
  },
  event: {
    listen: async (name, cb) => { if (name === "agents-changed") setInterval(cb, 2000); return () => {}; },
  },
};
"#;

fn main() {
    let mut args = std::env::args().skip(1);
    let home = PathBuf::from(args.next().unwrap_or_else(|| "/tmp/cos-demo".into()));
    let port: u16 = args.next().and_then(|p| p.parse().ok()).unwrap_or(8793);
    let home = home.canonicalize().expect("the home exists");
    let cos = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/cos");
    let settings = cos_desktop::Settings {
        home: home.clone(),
        workspace: home.clone(),
        projects: home.join("projects"),
        slots: Some(2),
        cos: cos.is_file().then_some(cos),
    };
    let app = cos_desktop::builder(mock_builder(), Some(settings))
        .build(mock_context(noop_assets()))
        .expect("mock app");
    let w = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .expect("mock window");
    let listener = TcpListener::bind(("127.0.0.1", port)).expect("bind");
    eprintln!("preview on http://127.0.0.1:{port}/ for {}", home.display());
    for stream in listener.incoming().flatten() {
        if let Err(e) = serve(&app, &w, stream) {
            eprintln!("request failed: {e}");
        }
    }
}

fn serve(
    app: &tauri::App<MockRuntime>,
    w: &tauri::WebviewWindow<MockRuntime>,
    mut stream: TcpStream,
) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut first = String::new();
    reader.read_line(&mut first)?;
    let mut parts = first.split_whitespace();
    let (method, target) = (
        parts.next().unwrap_or("GET").to_owned(),
        parts.next().unwrap_or("/").to_owned(),
    );
    let mut len = 0;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line)?;
        if line.trim().is_empty() {
            break;
        }
        if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
            len = v.trim().parse().unwrap_or(0);
        }
    }
    let mut body = vec![0; len];
    reader.read_exact(&mut body)?;
    let (path, query) = target.split_once('?').unwrap_or((&target, ""));

    // Headers the scheme sets beyond the type (an SVG's sandbox), passed through.
    let mut extra = String::new();
    let (status, kind, bytes): (u16, String, Vec<u8>) =
        if let Some(cmd) = path.strip_prefix("/invoke/") {
            let mut args: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
            // A stand-in harness for every hire, so a preview never starts a paid run.
            if let (Some(program), Some(a)) = (
                std::env::var_os("COS_PREVIEW_HARNESS"),
                args.get_mut("args").and_then(Value::as_object_mut),
            ) && cmd == "agents_spawn"
            {
                a.insert(
                    "program".into(),
                    Value::String(program.to_string_lossy().into_owned()),
                );
            }
            let res = get_ipc_response(
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
            );
            match res {
                Ok(b) => (
                    200,
                    "application/json".into(),
                    serde_json::to_vec(&b.deserialize::<Value>().unwrap_or(Value::Null)).unwrap(),
                ),
                Err(e) => (
                    500,
                    "application/json".into(),
                    serde_json::to_vec(&e).unwrap(),
                ),
            }
        } else if let Some(rest) = path.strip_prefix("/cosreport") {
            let rest = if rest.is_empty() { "/" } else { rest };
            let r = if rest.starts_with("/docs") {
                let uri = if query.is_empty() {
                    rest.to_owned()
                } else {
                    format!("{rest}?{query}")
                };
                let req = tauri::http::Request::builder()
                    .method(method.as_str())
                    .uri(uri)
                    .body(body)
                    .expect("request");
                cos_desktop::docs_response(app.handle(), &req)
            } else {
                cos_desktop::report_response(app.handle(), rest)
            };
            let kind = r
                .headers()
                .get("Content-Type")
                .and_then(|h| h.to_str().ok())
                .unwrap_or("text/plain")
                .to_owned();
            for name in ["Content-Security-Policy", "X-Content-Type-Options"] {
                if let Some(v) = r.headers().get(name).and_then(|h| h.to_str().ok()) {
                    extra.push_str(&format!("{name}: {v}\r\n"));
                }
            }
            (r.status().as_u16(), kind, r.body().clone())
        } else if path == "/__preview.js" {
            (200, "text/javascript".into(), SHIM.as_bytes().to_vec())
        } else {
            let rel = if path == "/" {
                "index.html"
            } else {
                path.trim_start_matches('/')
            };
            let file = Path::new(UI).join(rel);
            let inside = Path::new(rel)
                .components()
                .all(|c| matches!(c, std::path::Component::Normal(_)));
            match std::fs::read(&file) {
                Ok(mut b) if inside => {
                    if rel == "index.html" {
                        b = String::from_utf8_lossy(&b)
                            .replace(
                                r#"<script src="app.js">"#,
                                r#"<script src="/__preview.js"></script><script src="app.js">"#,
                            )
                            .into_bytes();
                    }
                    if rel == "app.js" {
                        b = String::from_utf8_lossy(&b)
                            .replace("cosreport://localhost/", "/cosreport/")
                            .into_bytes();
                    }
                    (200, kind_of(rel).into(), b)
                }
                _ => (404, "text/plain".into(), b"not found".to_vec()),
            }
        };
    write!(
        stream,
        "HTTP/1.1 {status} X\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nCache-Control: no-store\r\n{extra}Connection: close\r\n\r\n",
        bytes.len()
    )?;
    stream.write_all(&bytes)
}

fn kind_of(rel: &str) -> &'static str {
    match rel.rsplit('.').next() {
        Some("html") => "text/html; charset=utf-8",
        Some("css") => "text/css",
        Some("js") => "text/javascript",
        Some("svg") => "image/svg+xml",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    }
}
