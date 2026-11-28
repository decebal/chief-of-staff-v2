//! `cos daemon install` / `uninstall`: the `cos.daemon` launchd job in the
//! user's GUI session, rendered from the embedded template.

use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};
use serde_json::json;

use super::client;
use crate::business;
use crate::proc;
use crate::runs::{self, Owner};

pub const LABEL: &str = "cos.daemon";
const TEMPLATE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/launchd/cos.daemon.plist"
));
/// A `bootout` waits for the old daemon to exit, which stops its runs first.
const LAUNCHCTL_WAIT: Duration = Duration::from_secs(60);
/// A `stop` waits out the run's grace and the reap's bound in the daemon.
const STOP_WAIT: Duration = Duration::from_secs(30);
const DRAIN_WAIT: Duration = Duration::from_secs(5);

/// Install refused because the daemon owns a live run; `--force` stops it.
#[derive(Debug)]
pub struct Busy(pub String);

impl std::fmt::Display for Busy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Busy {}

pub struct Install {
    pub bin: PathBuf,
    pub force: bool,
}

pub struct Installed {
    pub plist: PathBuf,
    pub loaded: bool,
}

/// `COS_LAUNCH_AGENTS`, else `$HOME/Library/LaunchAgents`.
pub fn agents_dir() -> Result<PathBuf> {
    if let Some(d) = std::env::var_os("COS_LAUNCH_AGENTS").filter(|v| !v.is_empty()) {
        return Ok(PathBuf::from(d));
    }
    Ok(home()?.join("Library").join("LaunchAgents"))
}

fn home() -> Result<PathBuf> {
    std::env::var_os("HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .context("HOME is unset")
}

/// `COS_LAUNCHCTL`, else `/bin/launchctl`.
fn launchctl() -> PathBuf {
    std::env::var_os("COS_LAUNCHCTL")
        .filter(|v| !v.is_empty())
        .map_or_else(|| PathBuf::from("/bin/launchctl"), PathBuf::from)
}

/// The Taskfile's `TOOL_PATH`: where `claude`, `gh`, `cn` and `hc` live.
pub fn tool_path(home: &Path) -> String {
    let h = home.display();
    format!("{h}/.cargo/bin:{h}/.local/bin:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin")
}

fn domain() -> String {
    // SAFETY: geteuid has no preconditions.
    format!("gui/{}", unsafe { libc::geteuid() })
}

/// The plist with `bin`, `PATH` and `COS_BUSINESSES` substituted; each is
/// refused when it holds `<`, `&` or a newline, so no value can add a key.
pub fn render(bin: &Path, path: &str, registry: &Path) -> Result<String> {
    let bin = bin.to_str().context("bin is not UTF-8")?;
    ensure!(bin.starts_with('/'), "bin {bin:?} is not an absolute path");
    let registry = registry.to_str().context("COS_BUSINESSES is not UTF-8")?;
    ensure!(
        registry.starts_with('/'),
        "COS_BUSINESSES {registry:?} is not an absolute path; launchd starts the daemon in /"
    );
    for (what, v) in [("bin", bin), ("PATH", path), ("COS_BUSINESSES", registry)] {
        ensure!(
            !v.contains(['<', '&', '\n', '\r']),
            "{what} {v:?} holds `<`, `&` or a newline, which the plist cannot carry"
        );
    }
    let text = TEMPLATE
        .replace("@BIN@", bin)
        .replace("@PATH@", path)
        .replace("@REGISTRY@", registry);
    plist::Value::from_reader_xml(text.as_bytes()).context("the rendered plist does not parse")?;
    Ok(text)
}

/// Each registered business's runs the daemon owns that are not final and
/// whose process is alive, as `(business, run)`.
fn live_daemon_runs(registry: &Path) -> Vec<(String, String)> {
    let mut live = Vec::new();
    for (name, home) in business::registered_homes(registry) {
        let Ok(entries) = std::fs::read_dir(home.join(runs::DIR)) else {
            continue;
        };
        for e in entries.filter_map(Result::ok) {
            let file = e.file_name();
            let Some(id) = file.to_str().and_then(|f| f.strip_suffix(".json")) else {
                continue;
            };
            if !runs::is_id(id) {
                continue;
            }
            if let Ok(Some(r)) = runs::read(&home, id)
                && r.owner == Owner::Daemon
                && !r.state.is_final()
                && !runs::process_gone(&r)
            {
                live.push((name.clone(), r.id));
            }
        }
    }
    live.sort();
    live
}

fn named(runs: &[(String, String)]) -> String {
    runs.iter()
        .map(|(b, id)| format!("{id} ({b})"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Sends `stop` for each run to the daemon on `sock`.
fn stop_runs(sock: &Path, live: &[(String, String)]) -> Result<()> {
    for (business, run) in live {
        let reply = super::call(
            sock,
            &json!({"op": "stop", "run": run, "business": business}),
            STOP_WAIT,
        )
        .with_context(|| format!("stop run {run} ({business})"))?;
        if let Some(e) = reply.get("error") {
            bail!(
                "stop run {run} ({business}): {}",
                e.as_str().unwrap_or("refused")
            );
        }
        eprintln!("cos daemon: stopped run {run} ({business})");
    }
    Ok(())
}

fn busy(live: &[(String, String)]) -> anyhow::Error {
    Busy(format!(
        "the daemon owns live runs: {}. Installing reloads it, which stops them; \
         rerun with --force to stop them now, or wait for them to end",
        named(live)
    ))
    .into()
}

enum Drain {
    /// The daemon with this pid now refuses every `start`.
    Drained(i64),
    Live(Vec<(String, String)>),
    /// No daemon on the socket (`ENOENT` or `ECONNREFUSED`), or one built
    /// before `drain` that answers it as an unknown op.
    Silent,
}

/// `drain` on `sock`. A connect that fails other than by nothing listening,
/// a reply that does not come within `DRAIN_WAIT` or does not parse, and any
/// other refusal are errors: a slow daemon is not a gone one.
fn drain(sock: &Path) -> Result<Drain> {
    let stream = match UnixStream::connect(sock) {
        Ok(s) => s,
        Err(e) if matches!(e.raw_os_error(), Some(libc::ENOENT | libc::ECONNREFUSED)) => {
            return Ok(Drain::Silent);
        }
        Err(e) => return Err(e).with_context(|| format!("connect {}", sock.display())),
    };
    let reply = super::exchange(stream, &json!({"op": "drain"}), DRAIN_WAIT)
        .with_context(|| format!("the cos daemon at {} did not answer drain", sock.display()))?;
    if reply["drained"] == json!(true) {
        return Ok(Drain::Drained(reply["pid"].as_i64().unwrap_or_default()));
    }
    let error = reply["error"].as_str().unwrap_or("no reason given");
    if error.starts_with("unknown op") {
        return Ok(Drain::Silent);
    }
    let live: Vec<(String, String)> = reply["live"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| {
            Some((
                r["business"].as_str()?.to_owned(),
                r["run"].as_str()?.to_owned(),
            ))
        })
        .collect();
    ensure!(!live.is_empty(), "the daemon refused drain: {error}");
    Ok(Drain::Live(live))
}

/// Leaves the daemon refusing new runs with none live, so the `bootout` that
/// follows kills nothing: `drain` on its socket, which fails while a run is
/// live. `force` stops the runs that drain named and drains once more. With
/// no daemon answering, proceeds only when no record names a live daemon run,
/// `force` or not. The drained daemon's pid, when one drained.
fn quiesce(sock: &Path, registry: &Path, force: bool) -> Result<Option<i64>> {
    let mut stopped = false;
    loop {
        match drain(sock)? {
            Drain::Drained(pid) => return Ok(Some(pid)),
            Drain::Live(live) if force && !stopped => {
                stop_runs(sock, &live)?;
                stopped = true;
            }
            Drain::Live(live) => return Err(busy(&live)),
            Drain::Silent => {
                let live = live_daemon_runs(registry);
                return if live.is_empty() {
                    Ok(None)
                } else {
                    Err(busy(&live))
                };
            }
        }
    }
}

/// Drains the daemon (see `quiesce`), drains it once more without `force`
/// just before the plist is written and the job booted out, then
/// `bootstrap`. A daemon that drained and still answers with its pid after
/// the `bootout` was not started by launchd as `cos.daemon`; the install
/// fails there. Every failure after a drain sends `undrain`, so the daemon
/// takes runs again if it is still up.
pub fn install(opts: &Install) -> Result<Installed> {
    let registry = business::location()?;
    let text = render(&opts.bin, &tool_path(&home()?), &registry)?;
    let daemon_dir = super::dir()?;
    let sock = super::socket_path(&daemon_dir);
    let mut drained = quiesce(&sock, &registry, opts.force)?;
    let installed = replace(&text, &daemon_dir, &registry, &mut drained);
    if installed.is_err() && drained.is_some() {
        let _ = super::call(&sock, &json!({"op": "undrain"}), DRAIN_WAIT);
    }
    installed
}

fn replace(
    text: &str,
    daemon_dir: &Path,
    registry: &Path,
    drained: &mut Option<i64>,
) -> Result<Installed> {
    // Between the first drain and here a daemon can have come up undrained
    // (launchd relaunching one that retired, or a `--force` stop taking long).
    if let Some(pid) = quiesce(&super::socket_path(daemon_dir), registry, false)? {
        *drained = Some(pid);
    }
    let drained = *drained;
    let dir = agents_dir()?;
    std::fs::create_dir_all(&dir).with_context(|| dir.display().to_string())?;
    let plist = dir.join(format!("{LABEL}.plist"));
    let new = dir.join(format!("{LABEL}.plist.new"));
    std::fs::write(&new, text).with_context(|| new.display().to_string())?;
    std::fs::rename(&new, &plist).with_context(|| plist.display().to_string())?;

    let domain = domain();
    // Not loaded is the usual reason this fails, and bootstrap then succeeds.
    launchctl_run(&["bootout", &format!("{domain}/{LABEL}")])?;
    if let Some(pid) = drained
        && let Ok(client::Probe::Up { pid: up, .. }) = client::probe(daemon_dir)
        && up == pid
    {
        bail!(
            "the cos daemon (pid {pid}) still runs after `launchctl bootout {domain}/{LABEL}`: \
             launchd did not start it, so it cannot stop it. Stop it (kill {pid}) and install again"
        );
    }
    let out = launchctl_run(&["bootstrap", &domain, plist.to_str().context("plist path")?])?;
    ensure!(
        out.status.success(),
        "launchctl bootstrap {domain} {}: {}",
        plist.display(),
        out.stderr.trim()
    );
    Ok(Installed {
        plist,
        loaded: true,
    })
}

/// Boots the job out and removes its plist; both may already be gone.
pub fn uninstall() -> Result<PathBuf> {
    let plist = agents_dir()?.join(format!("{LABEL}.plist"));
    launchctl_run(&["bootout", &format!("{}/{LABEL}", domain())])?;
    match std::fs::remove_file(&plist) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e).with_context(|| plist.display().to_string()),
    }
    Ok(plist)
}

fn launchctl_run(args: &[&str]) -> Result<proc::Output> {
    let bin = launchctl();
    proc::run(Command::new(&bin).args(args), LAUNCHCTL_WAIT)
        .with_context(|| format!("{} {}", bin.display(), args.join(" ")))
}
