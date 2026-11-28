//! Local mods: each folder under the config directory's `mods/` is a page the
//! window shows in its own tab. The page loads from `cosmod:` with an opaque
//! origin and reaches the window only through `postMessage`; its data is kept
//! per business beside the workspace, never in the mod's own folder.

use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A mod's data is one JSON object, at most this many bytes.
pub const DATA_LIMIT: usize = 256 * 1024;

/// What every mod's page may load. A file of the same name in a mod's folder is never served.
const CSP: &str = "default-src 'self' cosmod:; script-src 'self' cosmod:; style-src 'self' cosmod:; \
img-src 'self' cosmod: data:; font-src 'self' cosmod:; connect-src 'none'; frame-src 'none'; \
object-src 'none'; base-uri 'none'; form-action 'none'";

const GUIDE: &str = r#"# Local mods

This folder holds the local mods of the Chief of Staff desktop app. Each
subfolder `<name>/` (lowercase letters, digits and dashes) is one mod: a page
the app shows in its own tab, after the built-in tabs. The app never writes into
a mod's folder. It rewrites only this file. `AGENTS.local.md`, if you keep notes
here, is yours and the app never touches it.

## Files

- `mod.json`: `{"title": "Notes", "icon": "notebook", "order": 10}`. `title` is
  required. `icon` is a Lucide name from the app's set (default `package`).
  `order` sorts the tabs, lowest first. A bad `mod.json` lists the mod with its
  error in its tab.
- `index.html`: the page. Load scripts and styles from the mod's own folder with
  relative paths (`app.js`, `app.css`). Inline scripts and `style` attributes are
  blocked; set styles from script instead.
- `tokens.css`: served to every mod by the app. Link it with
  `<link rel="stylesheet" href="tokens.css">` to match the window's colours, type
  and light or dark theme. `tokens.css` and `vendor/geist/*` are the app's names.

## Messages

The page runs in a sandboxed frame with an opaque origin and no network
(`connect-src 'none'`). Its URL ends in `?load=<id>`. Read that id from
`location.search` and send it with `cos-mod-ready` and with every later message:
the window answers only the id it issued for this showing. It talks to the
window through `window.parent.postMessage`:

- `{type: "cos-mod-ready", load}` gets `{type: "cos-mod-host", business, theme}`:
  whose data this is, and `"light"` or `"dark"`.
- `{type: "cos-mod-read", load, seq}` gets `{type: "cos-mod-data", seq, ok, data}`.
  `data` is `null` before the first save.
- `{type: "cos-mod-save", load, seq, data}` gets `{type: "cos-mod-saved", seq, ok, error}`.
  `data` must be one JSON object of at most 256 KiB.

A page that navigates the frame to another page loses its id, so keep one page
per mod. A mod should accept replies only from `event.source === window.parent`.

## Data

Saved data lives per business at `<workspace>/.chief-of-staff/mods/<name>.json`,
in the workspace's gitignored runtime directory. A business switch reloads the
window. A mod's frame reloads whenever its tab is selected again.
"#;

/// One mod folder: its title and icon from `mod.json`, or the error that keeps it from loading.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    pub name: String,
    pub title: String,
    pub icon: Option<String>,
    pub order: Option<f64>,
    pub error: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    title: String,
    icon: Option<String>,
    order: Option<f64>,
}

/// A mod's folder name, and its URL segment: `[a-z0-9-]+`, so it is always a
/// single safe path component and host label.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// Every mod folder under `dir`, by `order` then name. A missing `dir` has none.
pub fn list(dir: &Path) -> Vec<Row> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut rows: Vec<Row> = entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            (valid_name(&name) && e.path().is_dir()).then(|| row(dir, name))
        })
        .collect();
    rows.sort_by(|a, b| {
        rank(a)
            .total_cmp(&rank(b))
            .then_with(|| a.name.cmp(&b.name))
    });
    rows
}

fn rank(r: &Row) -> f64 {
    r.order.unwrap_or(f64::INFINITY)
}

fn row(dir: &Path, name: String) -> Row {
    match manifest(&dir.join(&name)) {
        Ok(m) => Row {
            name,
            title: m.title,
            icon: m.icon,
            order: m.order,
            error: None,
        },
        Err(error) => Row {
            title: name.clone(),
            name,
            icon: None,
            order: None,
            error: Some(error),
        },
    }
}

fn manifest(folder: &Path) -> Result<Manifest, String> {
    let text = std::fs::read_to_string(folder.join("mod.json"))
        .map_err(|_| "mod.json is missing or unreadable".to_owned())?;
    let mut m: Manifest = serde_json::from_str(&text).map_err(|e| format!("mod.json: {e}"))?;
    m.title = m.title.trim().to_owned();
    if m.title.is_empty() {
        return Err("mod.json: title is empty".into());
    }
    if m.icon.as_deref().is_some_and(|i| !valid_name(i)) {
        return Err("mod.json: icon is a Lucide name, lowercase with dashes".into());
    }
    if m.order.is_some_and(|o| !o.is_finite()) {
        return Err("mod.json: order must be a finite number".into());
    }
    if !folder.join("index.html").is_file() {
        return Err("index.html is missing".into());
    }
    Ok(m)
}

/// `Response` for the `cosmod` scheme.
pub type Reply = tauri::http::Response<Vec<u8>>;

/// The reply for `path` (`/<mod>/<file>`) under `dir`: GET only, a file inside
/// the mod's own folder, or the app's shared file of that name.
pub fn serve(dir: &Path, method: &tauri::http::Method, path: &str) -> Reply {
    if *method != tauri::http::Method::GET {
        return reply(405, "text/plain; charset=utf-8", b"GET only".to_vec());
    }
    match page(dir, path) {
        Ok((kind, bytes)) => reply(200, kind, bytes),
        Err((status, why)) => reply(status, "text/plain; charset=utf-8", why.into_bytes()),
    }
}

fn page(dir: &Path, path: &str) -> Result<(&'static str, Vec<u8>), (u16, String)> {
    let rest = path.strip_prefix('/').unwrap_or(path);
    if rest.contains(['%', '\\', '\0']) {
        return Err((400, "encoded or odd path: nothing served".into()));
    }
    let (name, file) = match rest.split_once('/') {
        Some((name, "")) => (name, "index.html"),
        Some((name, file)) => (name, file),
        None => (rest, "index.html"),
    };
    if !valid_name(name) {
        return Err((404, "no such mod".into()));
    }
    if !Path::new(file)
        .components()
        .all(|c| matches!(c, Component::Normal(_)))
    {
        return Err((403, "outside the mod folder: nothing served".into()));
    }
    let folder = dir
        .join(name)
        .canonicalize()
        .map_err(|_| (404, "no such mod".to_owned()))?;
    if let Some(bytes) = shared(file) {
        return Ok((content_type(file), bytes.to_vec()));
    }
    let target = folder
        .join(file)
        .canonicalize()
        .map_err(|_| (404, "not found".to_owned()))?;
    if !target.starts_with(&folder) {
        return Err((403, "outside the mod folder: nothing served".into()));
    }
    if !target.is_file() {
        return Err((404, "not found".into()));
    }
    let bytes = std::fs::read(&target).map_err(|_| (404, "not found".to_owned()))?;
    Ok((content_type(file), bytes))
}

/// The window's own files: the design tokens and the two Geist faces they name.
fn shared(file: &str) -> Option<&'static [u8]> {
    match file {
        "tokens.css" => Some(&include_bytes!("../ui/tokens.css")[..]),
        "vendor/geist/Geist-Variable.woff2" => {
            Some(&include_bytes!("../ui/vendor/geist/Geist-Variable.woff2")[..])
        }
        "vendor/geist/GeistMono-Variable.woff2" => {
            Some(&include_bytes!("../ui/vendor/geist/GeistMono-Variable.woff2")[..])
        }
        _ => None,
    }
}

fn content_type(file: &str) -> &'static str {
    let ext = Path::new(file)
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    match ext.as_deref() {
        Some("html" | "htm") => "text/html; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("woff2") => "font/woff2",
        Some("txt" | "md") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn reply(status: u16, kind: &str, body: Vec<u8>) -> Reply {
    tauri::http::Response::builder()
        .status(status)
        .header("Content-Type", kind)
        .header("Content-Security-Policy", CSP)
        .header("X-Content-Type-Options", "nosniff")
        .header("Cache-Control", "no-store")
        .body(body)
        .expect("static response parts")
}

/// The runtime directory's mods folder and the file for `name`, which must be a mod name.
fn data_paths(workspace: &Path, name: &str) -> Result<(PathBuf, PathBuf), String> {
    if !valid_name(name) {
        return Err(format!("{name:?} is not a mod name"));
    }
    let runtime = cos::state::runtime_dir(workspace).map_err(|e| format!("{e:#}"))?;
    let dir = runtime.join("mods");
    let file = dir.join(format!("{name}.json"));
    Ok((runtime, file))
}

/// The mod's saved data for the business whose workspace this is: null before its first save.
pub fn read(workspace: &Path, name: &str) -> Result<Value, String> {
    let (_, file) = data_paths(workspace, name)?;
    if !file.exists() {
        return Ok(Value::Null);
    }
    cos::state::load(&file).map_err(|e| format!("{e:#}"))
}

/// Saves the mod's data atomically. Refused unless it is one JSON object within
/// [`DATA_LIMIT`] and the workspace is set up.
pub fn save(workspace: &Path, name: &str, data: &Value) -> Result<(), String> {
    let (runtime, file) = data_paths(workspace, name)?;
    if !data.is_object() {
        return Err("Mod data must be one JSON object.".into());
    }
    let size = serde_json::to_vec(data).map_err(|e| e.to_string())?.len();
    if size > DATA_LIMIT {
        return Err(format!(
            "Mod data is {size} bytes; the limit is {DATA_LIMIT}."
        ));
    }
    if !runtime.is_dir() {
        return Err("Set up the workspace before saving mod data.".into());
    }
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{e:#}"))?;
    }
    cos::state::atomic_write(&file, data).map_err(|e| format!("{e:#}"))
}

/// The mod the frame shows. Its navigation guard lets the frame load only that mod's pages.
#[derive(Clone, Default)]
pub struct Shown(Arc<Mutex<Option<String>>>);

impl Shown {
    pub fn set(&self, name: &str) {
        if let Ok(mut shown) = self.0.lock() {
            *shown = Some(name.to_owned());
        }
    }

    /// False for a mod page of any mod but the one shown, or of none. Any other URL is allowed.
    pub fn allows(&self, url: &tauri::Url) -> bool {
        if !is_mod_url(url) {
            return true;
        }
        let name = url
            .path()
            .trim_start_matches('/')
            .split('/')
            .next()
            .unwrap_or_default();
        self.0
            .lock()
            .is_ok_and(|shown| shown.as_deref() == Some(name))
    }
}

fn is_mod_url(url: &tauri::Url) -> bool {
    url.scheme() == "cosmod"
        || (url.scheme() == "http" && url.host_str() == Some("cosmod.localhost"))
}

/// Writes `AGENTS.md` into `dir`, creating the folder. Nothing else in it is touched.
pub fn write_guide(dir: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(dir)?;
    cos::state::atomic_write_bytes(&dir.join("AGENTS.md"), GUIDE.as_bytes())
}
