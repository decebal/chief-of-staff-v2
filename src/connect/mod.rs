//! `cos connect`: the `cos` entry in Claude Desktop's and Codex's MCP config.
//! A write touches only that entry, keeps the first original as
//! `<file>.cos.orig`, and refuses a file that changed after it was read.

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::Write;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result, bail};
use serde::Serialize;
use serde_json::{Map, Value, json};
use toml_edit::{Array, DocumentMut, InlineTable, Item, Table, TableLike, value};

use crate::proc;

/// What Codex needs for hires: `spin_off` seeds a worktree before it returns.
pub const CODEX_TIMEOUT_SEC: i64 = 320;
pub const VERSION_DEADLINE: Duration = Duration::from_secs(5);
/// The version a configured `cos` must print, and the one the app shows as its own.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Client {
    ClaudeDesktop,
    Codex,
}

impl Client {
    pub const ALL: [Client; 2] = [Client::ClaudeDesktop, Client::Codex];

    pub fn name(self) -> &'static str {
        match self {
            Client::ClaudeDesktop => "claude-desktop",
            Client::Codex => "codex",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.name() == name)
    }

    pub fn config(self, env: &Env) -> PathBuf {
        match self {
            Client::ClaudeDesktop => env
                .home
                .join("Library/Application Support/Claude/claude_desktop_config.json"),
            Client::Codex => env.codex_home().join("config.toml"),
        }
    }

    fn installed(self, env: &Env) -> bool {
        match self {
            Client::ClaudeDesktop => self.config(env).parent().is_some_and(Path::is_dir),
            Client::Codex => env.codex_home().is_dir() || env.on_path("codex"),
        }
    }
}

/// The environment the client configs are found from. Tests point `HOME` and
/// `CODEX_HOME` at temp dirs.
pub struct Env {
    pub home: PathBuf,
    pub codex_home: Option<PathBuf>,
    pub path: Option<OsString>,
    /// The business registry the daemon reads, pinned in the client's entry
    /// so `cos mcp` reads the same one whatever environment the client gives it.
    pub registry: Option<PathBuf>,
}

impl Env {
    pub fn from_process() -> Result<Self> {
        let home = std::env::var_os("HOME")
            .filter(|h| !h.is_empty())
            .context("HOME is not set")?;
        Ok(Self {
            home: PathBuf::from(home),
            codex_home: std::env::var_os("CODEX_HOME")
                .filter(|h| !h.is_empty())
                .map(PathBuf::from),
            path: std::env::var_os("PATH"),
            registry: crate::business::location().ok(),
        })
    }

    fn codex_home(&self) -> PathBuf {
        self.codex_home
            .clone()
            .unwrap_or_else(|| self.home.join(".codex"))
    }

    fn on_path(&self, name: &str) -> bool {
        self.path
            .as_deref()
            .is_some_and(|p| std::env::split_paths(p).any(|d| executable(&d.join(name))))
    }

    /// The `PATH` a client hands `cos mcp`: the tools the brains and hires
    /// call (`claude`, `gh`, `cn`, `hc`), whatever the client's own PATH is.
    pub fn tool_path(&self) -> String {
        let home = self.home.display();
        format!(
            "{home}/.cargo/bin:{home}/.local/bin:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin"
        )
    }
}

/// The daemon's `hello` is not wired into `status` yet, so every row reports
/// `unknown` and it never counts as a problem.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Daemon {
    Unknown,
}

impl Daemon {
    pub fn as_str(self) -> &'static str {
        match self {
            Daemon::Unknown => "unknown",
        }
    }
}

fn daemon_state() -> Daemon {
    Daemon::Unknown
}

#[derive(Clone, Debug, Serialize)]
pub struct Status {
    pub client: Client,
    pub config: PathBuf,
    pub installed: bool,
    pub registered: bool,
    pub command: Option<String>,
    pub command_ok: bool,
    pub version: Option<String>,
    pub version_ok: bool,
    pub version_stale: bool,
    pub timeout_sec: Option<i64>,
    pub daemon: Daemon,
    pub error: Option<String>,
    pub problems: Vec<&'static str>,
    pub ok: bool,
}

pub fn status(env: &Env) -> Vec<Status> {
    Client::ALL.into_iter().map(|c| status_of(c, env)).collect()
}

fn status_of(client: Client, env: &Env) -> Status {
    let config = client.config(env);
    let mut s = Status {
        client,
        installed: client.installed(env),
        config: config.clone(),
        registered: false,
        command: None,
        command_ok: false,
        version: None,
        version_ok: false,
        version_stale: false,
        timeout_sec: None,
        daemon: daemon_state(),
        error: None,
        problems: Vec::new(),
        ok: false,
    };
    let entry = match Doc::read(client, &config) {
        Ok((_, doc)) => doc.entry(),
        Err(e) => {
            s.error = Some(format!("{e:#}"));
            s.problems.push("config");
            return s;
        }
    };
    let Some(entry) = entry else {
        s.problems.push("registered");
        return s;
    };
    s.registered = true;
    s.timeout_sec = entry.timeout_sec;
    if let Some(cmd) = &entry.command {
        let path = Path::new(cmd);
        s.command_ok = path.is_absolute() && executable(path);
        if s.command_ok {
            let p = probe(path, VERSION_DEADLINE);
            s.version_ok = p.version.as_deref() == Some(format!("cos {VERSION}").as_str());
            s.version_stale = p.stale;
            s.version = p.version;
            s.error = p.error;
        }
    }
    s.command = entry.command;
    if !s.command_ok {
        s.problems.push("command");
    }
    if !s.version_ok {
        s.problems.push("version");
    }
    if client == Client::Codex && s.timeout_sec.is_none_or(|t| t < CODEX_TIMEOUT_SEC) {
        s.problems.push("timeout");
    }
    s.ok = s.problems.is_empty();
    s
}

#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Registered,
    Repaired,
    AlreadyRegistered,
    Removed,
    NotRegistered,
}

/// Registers `bin mcp` in `client`'s config. An entry already there is left
/// alone unless `repair`.
pub fn register(client: Client, env: &Env, bin: &Path, repair: bool) -> Result<Outcome> {
    check_bin(bin)?;
    let bin = bin
        .to_str()
        .with_context(|| format!("--bin is not UTF-8: {}", bin.display()))?;
    let path = client.config(env);
    let (original, mut doc) = Doc::read(client, &path)?;
    let present = doc.entry().is_some();
    if present && !repair {
        return Ok(Outcome::AlreadyRegistered);
    }
    let registry = match &env.registry {
        Some(p) => Some(
            p.to_str()
                .with_context(|| format!("the registry path is not UTF-8: {}", p.display()))?,
        ),
        None => None,
    };
    doc.set(bin, &env.tool_path(), registry)?;
    write_checked(&path, original.as_deref(), &doc.render(), || {})?;
    Ok(if present {
        Outcome::Repaired
    } else {
        Outcome::Registered
    })
}

pub fn remove(client: Client, env: &Env) -> Result<Outcome> {
    let path = client.config(env);
    let (original, mut doc) = Doc::read(client, &path)?;
    if !doc.remove() {
        return Ok(Outcome::NotRegistered);
    }
    write_checked(&path, original.as_deref(), &doc.render(), || {})?;
    Ok(Outcome::Removed)
}

/// The running `cos`, resolved: the default `--bin`.
pub fn current_bin() -> Result<PathBuf> {
    let exe = std::env::current_exe().context("the running cos has no path")?;
    exe.canonicalize()
        .with_context(|| format!("resolve {}", exe.display()))
}

fn check_bin(bin: &Path) -> Result<()> {
    if !bin.is_absolute() {
        bail!("--bin must be an absolute path: {}", bin.display());
    }
    if !executable(bin) {
        bail!("--bin is not an executable file: {}", bin.display());
    }
    let p = probe(bin, VERSION_DEADLINE);
    match (&p.version, p.stale || p.error.is_some()) {
        (Some(v), false) if is_cos_version(v) => Ok(()),
        (Some(v), false) => bail!(
            "{} --version printed {v:?}, not `cos <version>`",
            bin.display()
        ),
        _ => bail!(
            "{} --version failed: {}",
            bin.display(),
            p.error.as_deref().unwrap_or("no output")
        ),
    }
}

pub fn is_cos_version(line: &str) -> bool {
    let Some(v) = line.strip_prefix("cos ") else {
        return false;
    };
    let core = v.split(['-', '+']).next().unwrap_or_default();
    let parts: Vec<&str> = core.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
}

fn executable(path: &Path) -> bool {
    fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Entry {
    pub command: Option<String>,
    pub timeout_sec: Option<i64>,
}

pub(crate) enum Doc {
    Json(Value),
    Toml(DocumentMut),
}

impl Doc {
    /// The file's bytes (`None` when it does not exist) and its parse. A
    /// parse failure names the file and refuses everything after.
    fn read(client: Client, path: &Path) -> Result<(Option<Vec<u8>>, Doc)> {
        let bytes = match fs::read(path) {
            Ok(b) => Some(b),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(e).with_context(|| format!("read {}", path.display())),
        };
        let raw = bytes.as_deref().unwrap_or_default();
        let doc = match client {
            Client::ClaudeDesktop => Doc::Json(
                parse_json(raw).with_context(|| format!("{}: not valid JSON", path.display()))?,
            ),
            Client::Codex => Doc::Toml(
                parse_toml(raw).with_context(|| format!("{}: not valid TOML", path.display()))?,
            ),
        };
        Ok((bytes, doc))
    }

    fn entry(&self) -> Option<Entry> {
        match self {
            Doc::Json(v) => json_entry(v),
            Doc::Toml(d) => toml_entry(d),
        }
    }

    fn set(&mut self, bin: &str, path: &str, registry: Option<&str>) -> Result<()> {
        match self {
            Doc::Json(v) => json_set(v, bin, path, registry),
            Doc::Toml(d) => toml_set(d, bin, path, registry),
        }
    }

    fn remove(&mut self) -> bool {
        match self {
            Doc::Json(v) => json_remove(v),
            Doc::Toml(d) => toml_remove(d),
        }
    }

    fn render(&self) -> Vec<u8> {
        match self {
            Doc::Json(v) => {
                let mut out = serde_json::to_vec_pretty(v).unwrap_or_default();
                out.push(b'\n');
                out
            }
            Doc::Toml(d) => d.to_string().into_bytes(),
        }
    }
}

pub(crate) fn parse_json(raw: &[u8]) -> Result<Value> {
    if raw.iter().all(u8::is_ascii_whitespace) {
        return Ok(Value::Object(Map::new()));
    }
    let v: Value = serde_json::from_slice(raw)?;
    if !v.is_object() {
        bail!("the top level is not an object");
    }
    Ok(v)
}

/// A parse error names the line, column and what was expected, never the
/// text: `TomlError`'s `Display` quotes the line, which can hold another
/// server's secret.
pub(crate) fn parse_toml(raw: &[u8]) -> Result<DocumentMut> {
    let text = std::str::from_utf8(raw)?;
    text.parse::<DocumentMut>().map_err(|e| {
        let at = e.span().map_or(text.len(), |s| s.start).min(text.len());
        let before = text.get(..at).unwrap_or(text);
        let line = before.matches('\n').count() + 1;
        let column = before.rsplit('\n').next().unwrap_or("").chars().count() + 1;
        anyhow::anyhow!("line {line}, column {column}: {}", e.message())
    })
}

pub(crate) fn json_entry(v: &Value) -> Option<Entry> {
    let cos = v.get("mcpServers")?.get("cos")?;
    Some(Entry {
        command: cos
            .get("command")
            .and_then(Value::as_str)
            .map(str::to_owned),
        timeout_sec: None,
    })
}

pub(crate) fn json_set(v: &mut Value, bin: &str, path: &str, registry: Option<&str>) -> Result<()> {
    let root = v
        .as_object_mut()
        .context("the top level is not an object")?;
    let servers = root
        .entry("mcpServers")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .context("mcpServers is not an object")?;
    let cos = servers.entry("cos").or_insert_with(|| json!({}));
    if !cos.is_object() {
        *cos = json!({});
    }
    let cos = cos.as_object_mut().expect("set to an object above");
    cos.insert("command".into(), json!(bin));
    cos.insert("args".into(), json!(["mcp"]));
    let env = cos.entry("env").or_insert_with(|| json!({}));
    if !env.is_object() {
        *env = json!({});
    }
    let env = env.as_object_mut().expect("set to an object above");
    env.insert("PATH".into(), json!(path));
    if let Some(r) = registry {
        env.insert("COS_BUSINESSES".into(), json!(r));
    }
    Ok(())
}

pub(crate) fn json_remove(v: &mut Value) -> bool {
    v.get_mut("mcpServers")
        .and_then(Value::as_object_mut)
        .is_some_and(|s| s.shift_remove("cos").is_some())
}

pub(crate) fn toml_entry(d: &DocumentMut) -> Option<Entry> {
    let cos = d
        .get("mcp_servers")?
        .as_table_like()?
        .get("cos")?
        .as_table_like()?;
    Some(Entry {
        command: cos.get("command").and_then(Item::as_str).map(str::to_owned),
        timeout_sec: cos.get("tool_timeout_sec").and_then(timeout_secs),
    })
}

/// Codex reads `tool_timeout_sec` as seconds, integer or float.
fn timeout_secs(item: &Item) -> Option<i64> {
    item.as_integer()
        .or_else(|| item.as_float().map(|f| f as i64))
}

pub(crate) fn toml_set(
    d: &mut DocumentMut,
    bin: &str,
    path: &str,
    registry: Option<&str>,
) -> Result<()> {
    let servers = d.as_table_mut().entry("mcp_servers").or_insert_with(|| {
        let mut t = Table::new();
        t.set_implicit(true);
        Item::Table(t)
    });
    let servers = servers
        .as_table_like_mut()
        .context("mcp_servers is not a table")?;
    if !servers.get("cos").is_some_and(Item::is_table_like) {
        servers.insert("cos", Item::Table(Table::new()));
    }
    let cos = servers
        .get_mut("cos")
        .and_then(Item::as_table_like_mut)
        .expect("set to a table above");
    put(cos, "command", bin);
    put(cos, "args", Array::from_iter(["mcp"]));
    match cos.get_mut("env").and_then(Item::as_table_like_mut) {
        Some(env) => {
            put(env, "PATH", path);
            if let Some(r) = registry {
                put(env, "COS_BUSINESSES", r);
            }
        }
        None => {
            let mut env = InlineTable::new();
            env.insert("PATH", path.into());
            if let Some(r) = registry {
                env.insert("COS_BUSINESSES", r.into());
            }
            cos.insert("env", value(env));
        }
    }
    let longer = cos
        .get("tool_timeout_sec")
        .and_then(timeout_secs)
        .is_some_and(|t| t >= CODEX_TIMEOUT_SEC);
    if !longer {
        put(cos, "tool_timeout_sec", CODEX_TIMEOUT_SEC);
    }
    Ok(())
}

/// `TableLike::insert` on an existing key drops the comment above it;
/// replacing the item in place keeps it.
fn put(t: &mut dyn TableLike, key: &str, v: impl Into<toml_edit::Value>) {
    match t.get_mut(key) {
        Some(item) if item.is_value() => *item = value(v),
        _ => {
            t.insert(key, value(v));
        }
    }
}

pub(crate) fn toml_remove(d: &mut DocumentMut) -> bool {
    d.get_mut("mcp_servers")
        .and_then(Item::as_table_like_mut)
        .is_some_and(|s| s.remove("cos").is_some())
}

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut s: OsString = path.as_os_str().to_owned();
    s.push(OsStr::new(suffix));
    PathBuf::from(s)
}

fn write_new(path: &Path, bytes: &[u8], mode: u32) -> Result<()> {
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(mode)
        .open(path)
        .with_context(|| format!("create {}", path.display()))?;
    f.write_all(bytes)?;
    f.set_permissions(fs::Permissions::from_mode(mode))?;
    f.sync_all()?;
    Ok(())
}

/// Replaces `path` with `new`, given the bytes it held when read
/// (`original`, `None` for no file). Keeps the first original as
/// `<path>.cos.orig`, never overwriting it. Refuses, and leaves the file as
/// it is, when its bytes no longer match `original` just before the rename.
/// `before_rename` runs between the temp write and that check. A symlinked
/// `path` stays a link: its target is the file replaced, and the siblings sit
/// beside the target.
pub fn write_checked(
    path: &Path,
    original: Option<&[u8]>,
    new: &[u8],
    before_rename: impl FnOnce(),
) -> Result<()> {
    let resolved;
    let path = if fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
        resolved = fs::canonicalize(path)
            .with_context(|| format!("{} is a symlink to a missing file", path.display()))?;
        resolved.as_path()
    } else {
        path
    };
    let mode = fs::metadata(path).map_or(0o600, |m| m.mode() & 0o7777);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    if let Some(bytes) = original {
        let orig = sibling(path, ".cos.orig");
        if !orig.exists() {
            write_new(&orig, bytes, mode)?;
        }
    }
    let tmp = sibling(path, ".cos.new");
    let _ = fs::remove_file(&tmp);
    write_new(&tmp, new, mode)?;
    before_rename();
    let unchanged = match (original, fs::read(path)) {
        (Some(o), Ok(now)) => o == now.as_slice(),
        (None, Err(e)) => e.kind() == std::io::ErrorKind::NotFound,
        _ => false,
    };
    if !unchanged {
        let _ = fs::remove_file(&tmp);
        bail!("{} changed while writing; try again", path.display());
    }
    fs::rename(&tmp, path).with_context(|| format!("rename into {}", path.display()))
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Probe {
    /// The first line `--version` printed.
    pub version: Option<String>,
    /// A timeout or spawn failure: `version` is the last good result for this path.
    pub stale: bool,
    pub error: Option<String>,
}

struct Cached {
    size: u64,
    mtime: SystemTime,
    probe: Probe,
}

static PROBES: Mutex<BTreeMap<PathBuf, Cached>> = Mutex::new(BTreeMap::new());

/// `<bin> --version`, run once per path, size and mtime for this process.
pub fn probe(bin: &Path, deadline: Duration) -> Probe {
    let meta = match fs::metadata(bin) {
        Ok(m) => m,
        Err(e) => {
            return Probe {
                error: Some(format!("{}: {e}", bin.display())),
                ..Probe::default()
            };
        }
    };
    let (size, mtime) = (
        meta.len(),
        meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
    );
    if let Some(c) = probes().get(bin)
        && c.size == size
        && c.mtime == mtime
    {
        return c.probe.clone();
    }
    let mut cmd = Command::new(bin);
    cmd.arg("--version");
    match proc::run(&mut cmd, deadline.min(VERSION_DEADLINE)) {
        Ok(out) => {
            let line = out.stdout.lines().next().map(str::trim).unwrap_or_default();
            let fresh = Probe {
                version: (!line.is_empty()).then(|| line.to_owned()),
                stale: false,
                error: (!out.status.success()).then(|| format!("--version: {}", out.status)),
            };
            probes().insert(
                bin.to_owned(),
                Cached {
                    size,
                    mtime,
                    probe: fresh.clone(),
                },
            );
            fresh
        }
        Err(e) => {
            let last = probes().get(bin).map(|c| c.probe.clone());
            Probe {
                stale: last.is_some(),
                version: last.and_then(|p| p.version),
                error: Some(format!("{} --version: {e:#}", bin.display())),
            }
        }
    }
}

fn probes() -> std::sync::MutexGuard<'static, BTreeMap<PathBuf, Cached>> {
    PROBES.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
mod tests;
