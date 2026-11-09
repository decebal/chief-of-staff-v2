//! `cos dash`: the report served live on localhost. The page polls
//! `/version`, which changes whenever a run writes to `out/`, and reloads
//! itself, so a tab left open picks up every scheduled brief.

use std::convert::Infallible;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::UNIX_EPOCH;

use anyhow::Result;
use bytes::Bytes;
use http_body_util::Full;
use hyper::{Request, Response, StatusCode, body::Incoming};

use crate::report;

/// Directories under the home whose writes mean "a run happened".
const WATCHED: &[&str] = &["out/briefs", "out/facts", "out/history"];

/// Largest `POST /docs/organize` body accepted.
const MAX_BODY: usize = 2 * 1024 * 1024;

pub async fn serve(home: PathBuf, ws: PathBuf, projects: PathBuf, port: u16) -> Result<()> {
    let addr = format!("127.0.0.1:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    let allowed: Arc<[String]> = [format!("127.0.0.1:{port}"), format!("localhost:{port}")].into();
    let paths: Arc<(PathBuf, PathBuf, PathBuf)> = Arc::new((home, ws, projects));
    eprintln!("cos: dashboard on http://{addr}  (Ctrl-C to stop)");
    loop {
        let (stream, _) = listener.accept().await?;
        let (paths, allowed) = (paths.clone(), allowed.clone());
        tokio::spawn(async move {
            let svc = hyper::service::service_fn(move |req: Request<Incoming>| {
                let (paths, allowed) = (paths.clone(), allowed.clone());
                async move {
                    if req.uri().path().starts_with("/docs") && host_ok(&allowed, &req) {
                        return Ok::<_, Infallible>(docs(&paths.0, &paths.1, &paths.2, req).await);
                    }
                    // The scan blocks on `cn` for up to its budget: off the runtime's workers.
                    if req.uri().path() == "/flows" && host_ok(&allowed, &req) {
                        let (ws, projects) = (paths.1.clone(), paths.2.clone());
                        let page = tokio::task::spawn_blocking(move || {
                            crate::flows::page_cached(&ws, &projects)
                        })
                        .await;
                        return Ok::<_, Infallible>(match page {
                            Ok(html) => {
                                respond(StatusCode::OK, "text/html; charset=utf-8", html.into())
                            }
                            Err(e) => text(StatusCode::INTERNAL_SERVER_ERROR, &format!("{e}")),
                        });
                    }
                    Ok::<_, Infallible>(route(&paths.0, &paths.1, &allowed, req))
                }
            });
            let _ = hyper::server::conn::http1::Builder::new()
                .serve_connection(hyper_util::rt::TokioIo::new(stream), svc)
                .await;
        });
    }
}

/// Only localhost Host headers: a DNS-rebound name pointing at 127.0.0.1
/// must not read the brief.
fn host_ok<B>(allowed: &[String], req: &Request<B>) -> bool {
    let host = req
        .headers()
        .get(hyper::header::HOST)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    allowed.iter().any(|a| a == host)
}

/// The documentation room's routes, with the request body read for a save.
async fn docs(
    home: &Path,
    ws: &Path,
    projects: &Path,
    req: Request<Incoming>,
) -> Response<Full<Bytes>> {
    use http_body_util::BodyExt;
    let (parts, body) = req.into_parts();
    let bytes = match http_body_util::Limited::new(body, MAX_BODY).collect().await {
        Ok(c) => c.to_bytes(),
        Err(_) => return text(StatusCode::PAYLOAD_TOO_LARGE, "body too large"),
    };
    let cos_header = parts.headers.get("x-cos").and_then(|h| h.to_str().ok());
    let ask = crate::docs::Ask {
        method: parts.method.as_str(),
        path: parts.uri.path(),
        query: parts.uri.query(),
        cos_header,
        body: &bytes,
    };
    match crate::docs::handle(home, ws, projects, &ask) {
        Some(r) => docs_http(r),
        None => text(StatusCode::NOT_FOUND, "not found"),
    }
}

/// A docs room answer as the HTTP response `cos dash` sends, its own headers
/// included.
pub fn docs_http(r: crate::docs::Reply) -> Response<Full<Bytes>> {
    let mut res = respond(
        StatusCode::from_u16(r.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
        r.kind,
        r.body.into(),
    );
    for (k, v) in r.headers {
        res.headers_mut()
            .insert(k, hyper::header::HeaderValue::from_static(v));
    }
    res
}

fn route(
    home: &Path,
    ws: &Path,
    allowed: &[String],
    req: Request<Incoming>,
) -> Response<Full<Bytes>> {
    if !host_ok(allowed, &req) {
        return text(StatusCode::FORBIDDEN, "forbidden");
    }
    let path = req.uri().path();
    match path {
        "/" => match report::data(home, ws, true).and_then(|d| report::render(&d)) {
            Ok(html) => respond(StatusCode::OK, "text/html; charset=utf-8", html.into()),
            Err(e) => text(StatusCode::INTERNAL_SERVER_ERROR, &format!("{e:#}")),
        },
        "/version" => text(StatusCode::OK, &version(home, ws)),
        "/agents" => match crate::agents::list(home) {
            Ok(list) => respond(
                StatusCode::OK,
                "text/html; charset=utf-8",
                crate::agents::view::html(
                    &list,
                    &crate::agents::ledger::scorecard(&crate::agents::ledger::read(home)),
                )
                .into(),
            ),
            Err(e) => text(StatusCode::INTERNAL_SERVER_ERROR, &format!("{e:#}")),
        },
        _ => match archived(home, path) {
            Some(file) => match std::fs::read(file) {
                Ok(b) => respond(StatusCode::OK, "text/html; charset=utf-8", b.into()),
                Err(_) => text(StatusCode::NOT_FOUND, "not found"),
            },
            None => text(StatusCode::NOT_FOUND, "not found"),
        },
    }
}

/// `/reports/YYYY-MM-DD.html` only; anything else is refused, so no path can
/// climb out of `out/reports`.
pub fn archived(home: &Path, path: &str) -> Option<PathBuf> {
    let day = path.strip_prefix("/reports/")?.strip_suffix(".html")?;
    let ok = day.len() == 10
        && day.bytes().enumerate().all(|(i, b)| {
            if i == 4 || i == 7 {
                b == b'-'
            } else {
                b.is_ascii_digit()
            }
        });
    ok.then(|| home.join("out/reports").join(format!("{day}.html")))
}

/// Newest modification time and file count across the watched directories
/// and the workspace's `.chief-of-staff/`.
pub fn version(home: &Path, ws: &Path) -> String {
    let dirs = WATCHED
        .iter()
        .map(|d| home.join(d))
        .chain([ws.join(crate::state::DIR)]);
    stamp(dirs)
}

/// Newest modification time and entry count across `dirs`; missing ones count
/// as empty.
pub fn stamp(dirs: impl IntoIterator<Item = PathBuf>) -> String {
    let (mut newest, mut count) = (0u128, 0usize);
    for dir in dirs {
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            count += 1;
            if let Ok(m) = entry.metadata().and_then(|m| m.modified()) {
                newest = newest.max(
                    m.duration_since(UNIX_EPOCH)
                        .map(|d| d.as_millis())
                        .unwrap_or(0),
                );
            }
        }
    }
    format!("{newest}-{count}")
}

fn text(status: StatusCode, body: &str) -> Response<Full<Bytes>> {
    respond(
        status,
        "text/plain; charset=utf-8",
        Bytes::copy_from_slice(body.as_bytes()),
    )
}

fn respond(status: StatusCode, kind: &str, body: Bytes) -> Response<Full<Bytes>> {
    Response::builder()
        .status(status)
        .header(hyper::header::CONTENT_TYPE, kind)
        .header(hyper::header::CACHE_CONTROL, "no-store")
        .body(Full::new(body))
        .expect("static response parts")
}
