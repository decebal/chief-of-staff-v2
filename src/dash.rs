//! `cos dash`: the report served live on localhost. The page polls
//! `/version`, which changes whenever a run writes to `out/`, and reloads
//! itself, so a tab left open picks up every scheduled brief and review.

use std::convert::Infallible;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::UNIX_EPOCH;

use anyhow::Result;
use bytes::Bytes;
use http_body_util::Full;
use hyper::{Request, Response, StatusCode, body::Incoming};

use crate::report;

/// Directories whose writes mean "a run happened".
const WATCHED: &[&str] = &[
    "out/briefs",
    "out/reviews",
    "out/facts",
    "out/history",
    crate::state::DIR,
];

pub async fn serve(home: PathBuf, port: u16) -> Result<()> {
    let addr = format!("127.0.0.1:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    let allowed: Arc<[String]> = [format!("127.0.0.1:{port}"), format!("localhost:{port}")].into();
    let home: Arc<Path> = home.into();
    eprintln!("cos: dashboard on http://{addr}  (Ctrl-C to stop)");
    loop {
        let (stream, _) = listener.accept().await?;
        let (home, allowed) = (home.clone(), allowed.clone());
        tokio::spawn(async move {
            let svc = hyper::service::service_fn(move |req| {
                let (home, allowed) = (home.clone(), allowed.clone());
                async move { Ok::<_, Infallible>(route(&home, &allowed, req)) }
            });
            let _ = hyper::server::conn::http1::Builder::new()
                .serve_connection(hyper_util::rt::TokioIo::new(stream), svc)
                .await;
        });
    }
}

fn route(home: &Path, allowed: &[String], req: Request<Incoming>) -> Response<Full<Bytes>> {
    // Only localhost Host headers: a DNS-rebound name pointing at 127.0.0.1
    // must not read the brief.
    let host = req
        .headers()
        .get(hyper::header::HOST)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    if !allowed.iter().any(|a| a == host) {
        return text(StatusCode::FORBIDDEN, "forbidden");
    }
    let path = req.uri().path();
    match path {
        "/" => match report::data(home, true).and_then(|d| report::render(&d)) {
            Ok(html) => respond(StatusCode::OK, "text/html; charset=utf-8", html.into()),
            Err(e) => text(StatusCode::INTERNAL_SERVER_ERROR, &format!("{e:#}")),
        },
        "/version" => text(StatusCode::OK, &version(home)),
        "/agents" => match crate::agents::list(home) {
            Ok(list) => respond(
                StatusCode::OK,
                "text/html; charset=utf-8",
                crate::agents::view::html(&list).into(),
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

/// Newest modification time and file count across the watched directories.
fn version(home: &Path) -> String {
    let (mut newest, mut count) = (0u128, 0usize);
    for dir in WATCHED {
        for entry in std::fs::read_dir(home.join(dir))
            .into_iter()
            .flatten()
            .flatten()
        {
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
