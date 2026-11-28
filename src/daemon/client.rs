//! A client's side of the daemon socket: the probe that decides whether the
//! daemon is up, `start` and `stop`.

use std::io::ErrorKind;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail, ensure};
use serde_json::json;

use super::{check_socket_path, socket_path};
use crate::runs::{self, Kind};

/// A refusal slower than this is not read as "down".
pub const DOWN_WITHIN: Duration = Duration::from_millis(200);
const HELLO_WAIT: Duration = Duration::from_secs(2);
const START_WAIT: Duration = Duration::from_secs(10);
/// Past the daemon's own wait on the run it stops.
const STOP_WAIT: Duration = super::STOP_WAIT.saturating_add(Duration::from_secs(10));
/// How long a `hello` answer stands for the hire followers.
pub const HELLO_CACHE: Duration = Duration::from_secs(3);

#[derive(Debug)]
pub enum Probe {
    Down,
    Up { version: String, pid: i64 },
}

/// Connects to the socket in `dir`. `ENOENT` or `ECONNREFUSED` within
/// `DOWN_WITHIN` is `Down`; any other failure is an error, so a lost reply
/// never becomes a second paid run.
pub fn probe(dir: &Path) -> Result<Probe> {
    let sock = socket_path(dir);
    check_socket_path(&sock)?;
    if !trusted(dir)? {
        return Ok(Probe::Down);
    }
    let began = Instant::now();
    let stream = match UnixStream::connect(&sock) {
        Ok(s) => s,
        Err(e)
            if matches!(e.raw_os_error(), Some(libc::ENOENT | libc::ECONNREFUSED))
                && began.elapsed() <= DOWN_WITHIN =>
        {
            return Ok(Probe::Down);
        }
        Err(e) => {
            return Err(e).with_context(|| format!("the cos daemon's socket {}", sock.display()));
        }
    };
    let hello = super::exchange(stream, &json!({"op": "hello"}), HELLO_WAIT)
        .with_context(|| format!("the cos daemon at {} did not answer hello", sock.display()))?;
    Ok(Probe::Up {
        version: hello["version"].as_str().unwrap_or_default().to_owned(),
        pid: hello["pid"].as_i64().unwrap_or_default(),
    })
}

/// Whether `dir` and its socket are this user's alone, before anything is
/// sent there: `false` when neither exists yet, an error when either is
/// another user's, writable by others, a link, or the socket is not one.
fn trusted(dir: &Path) -> Result<bool> {
    use std::os::unix::fs::{FileTypeExt, MetadataExt};
    // SAFETY: geteuid has no preconditions.
    let me = unsafe { libc::geteuid() };
    let meta = match std::fs::symlink_metadata(dir) {
        Ok(m) => m,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(false),
        Err(e) => {
            return Err(e).with_context(|| format!("the cos daemon's directory {}", dir.display()));
        }
    };
    ensure!(
        meta.is_dir(),
        "the cos daemon's directory {} is not a directory",
        dir.display()
    );
    ensure!(
        meta.uid() == me,
        "the cos daemon's directory {} belongs to uid {}, not you",
        dir.display(),
        meta.uid()
    );
    ensure!(
        meta.mode() & 0o022 == 0,
        "the cos daemon's directory {} is writable by others (mode {:o})",
        dir.display(),
        meta.mode() & 0o777
    );
    let sock = socket_path(dir);
    let meta = match std::fs::symlink_metadata(&sock) {
        Ok(m) => m,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(false),
        Err(e) => {
            return Err(e).with_context(|| format!("the cos daemon's socket {}", sock.display()));
        }
    };
    ensure!(
        meta.file_type().is_socket() && meta.uid() == me,
        "{} is not a socket of yours",
        sock.display()
    );
    Ok(true)
}

/// Whether the daemon answers `hello`, at most one probe per `HELLO_CACHE`.
pub fn answers() -> bool {
    static LAST: Mutex<Option<(Instant, bool)>> = Mutex::new(None);
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((at, up)) = *last
        && at.elapsed() < HELLO_CACHE
    {
        return up;
    }
    let up = super::dir()
        .and_then(|d| probe(&d))
        .is_ok_and(|p| matches!(p, Probe::Up { .. }));
    *last = Some((Instant::now(), up));
    up
}

/// A fresh 16-hex retry key.
pub fn client_req() -> String {
    let mut bytes = [0u8; 8];
    let read = std::fs::File::open("/dev/urandom")
        .and_then(|mut f| std::io::Read::read_exact(&mut f, &mut bytes));
    if read.is_err() {
        let nanos = jiff::Timestamp::now().as_nanosecond() as u64;
        bytes = (nanos ^ u64::from(std::process::id())).to_be_bytes();
    }
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// `start` on the daemon in `dir`: the run id and whether it was already
/// running. A reply that does not arrive is asked once more with the same
/// `client_req`, which the daemon answers with its first reply.
pub fn start(
    dir: &Path,
    kind: Kind,
    business: &str,
    question: Option<&str>,
) -> Result<(String, bool)> {
    let sock = socket_path(dir);
    ensure!(trusted(dir)?, "no cos daemon socket at {}", sock.display());
    let mut req = json!({
        "op": "start",
        "kind": kind,
        "business": business,
        "client_req": client_req(),
    });
    if let Some(q) = question {
        req["question"] = json!(q);
    }
    let reply = match super::call(&sock, &req, START_WAIT) {
        Ok(v) => v,
        Err(e) if lost_reply(&e) => super::call(&sock, &req, START_WAIT)
            .context("the cos daemon did not answer start, asked twice")?,
        Err(e) => return Err(e).context("start on the cos daemon"),
    };
    if let Some(err) = reply["error"].as_str() {
        bail!("the cos daemon refused the run: {err}");
    }
    let run = reply["run"].as_str().unwrap_or_default().to_owned();
    ensure!(
        runs::is_id(&run),
        "the cos daemon answered no run id: {reply}"
    );
    Ok((run, reply["existing"] == json!(true)))
}

/// `stop` on the daemon in `dir` for a run it owns: the run's final state.
/// The reply can take up to the daemon's `STOP_WAIT`.
pub fn stop(dir: &Path, business: &str, run: &str) -> Result<runs::State> {
    let sock = socket_path(dir);
    check_socket_path(&sock)?;
    ensure!(trusted(dir)?, "no cos daemon socket at {}", sock.display());
    let req = json!({ "op": "stop", "run": run, "business": business });
    let reply = super::call(&sock, &req, STOP_WAIT).context("stop on the cos daemon")?;
    if let Some(err) = reply["error"].as_str() {
        bail!("the cos daemon did not stop run {run}: {err}");
    }
    serde_json::from_value(reply["state"].clone())
        .with_context(|| format!("the cos daemon answered no state: {reply}"))
}

fn lost_reply(e: &anyhow::Error) -> bool {
    e.chain()
        .filter_map(|c| c.downcast_ref::<std::io::Error>())
        .any(|io| {
            matches!(
                io.kind(),
                ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::UnexpectedEof
            )
        })
        || e.chain().any(|c| {
            c.downcast_ref::<serde_json::Error>()
                .is_some_and(|j| j.is_eof())
        })
}
