//! The governor: how many agents run at once, and one build at a time.
//!
//! A run slot is `out/agents/.slots/run-<i>.lock` under `flock`, held by a
//! supervisor for its whole life. The kernel drops the lock when the process
//! exits, however it exits, so a slot cannot leak and a queue needs no
//! bookkeeping. The build slot is one machine-wide `flock` that the `cargo`
//! shim on every agent's PATH takes before it execs the real `cargo`.

use std::fs::{File, OpenOptions};
use std::os::fd::{AsRawFd, IntoRawFd};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

/// Tools whose runs are compute-bound and so share the one build slot.
const BUILD_TOOLS: &[&str] = &["cargo"];

/// Agents running at once: `COS_AGENT_SLOTS`, else a quarter of the cores.
/// Ten worktrees on twelve cores put every build past its gate timeout.
pub fn run_slots() -> usize {
    std::env::var("COS_AGENT_SLOTS")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|&n: &usize| n >= 1)
        .unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map(|n| (n.get() / 4).max(1))
                .unwrap_or(1)
        })
}

/// A held run slot; dropping it (or exiting) frees it.
pub struct Held {
    _lock: File,
}

fn slots_dir(home: &Path) -> PathBuf {
    super::root(home).join(".slots")
}

fn flock(file: &File, nonblocking: bool) -> bool {
    let op = libc::LOCK_EX | if nonblocking { libc::LOCK_NB } else { 0 };
    // SAFETY: flock on a descriptor this process owns; no memory is touched.
    unsafe { libc::flock(file.as_raw_fd(), op) == 0 }
}

/// The first free run slot, or `None` when all `slots` are held.
pub fn try_run_slot(home: &Path, slots: usize) -> Result<Option<Held>> {
    let dir = slots_dir(home);
    std::fs::create_dir_all(&dir)?;
    for i in 0..slots {
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(dir.join(format!("run-{i}.lock")))?;
        if flock(&file, true) {
            return Ok(Some(Held { _lock: file }));
        }
    }
    Ok(None)
}

/// The machine-wide build lock: one per user, shared by every `cos` home.
pub fn build_lock() -> Result<PathBuf> {
    let path = std::env::temp_dir().join("cos-build.lock");
    OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .with_context(|| format!("create {}", path.display()))?;
    Ok(path)
}

/// `cos agents build-slot`: wait for the build lock, then exec `program`.
/// The descriptor survives the exec, so the lock is held until the build and
/// every compiler it started have exited.
pub fn exec_holding_build_slot(lock: &Path, program: &Path, args: &[String]) -> Result<()> {
    // Read-only: flock needs no write access, and a sandboxed agent has none
    // outside its worktree.
    let file = File::open(lock).with_context(|| format!("open {}", lock.display()))?;
    if !flock(&file, true) {
        eprintln!("cos: waiting for the build slot (another agent is compiling)");
        if !flock(&file, false) {
            bail!(
                "flock {}: {}",
                lock.display(),
                std::io::Error::last_os_error()
            );
        }
    }
    let fd = file.into_raw_fd();
    // SAFETY: clearing FD_CLOEXEC on a descriptor this process owns.
    unsafe {
        libc::fcntl(fd, libc::F_SETFD, 0);
    }
    let err = Command::new(program).args(args).exec();
    bail!("exec {}: {err}", program.display())
}

/// Writes one shim per build tool found on `path` into `bin`; returns the
/// tools shimmed. A tool that is not installed is left alone.
pub fn write_shims(bin: &Path, cos: &Path, lock: &Path, path: &str) -> Result<Vec<String>> {
    std::fs::create_dir_all(bin)?;
    let mut shimmed = Vec::new();
    for tool in BUILD_TOOLS {
        let Some(real) = which(tool, path, bin) else {
            continue;
        };
        let script = format!(
            "#!/bin/sh\nexec {} agents build-slot --lock {} -- {} \"$@\"\n",
            quote(cos),
            quote(lock),
            quote(&real)
        );
        let shim = bin.join(tool);
        std::fs::write(&shim, script)?;
        std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o755))?;
        shimmed.push((*tool).to_owned());
    }
    Ok(shimmed)
}

/// `tool` on `path`, skipping `skip` so a shim never finds itself.
fn which(tool: &str, path: &str, skip: &Path) -> Option<PathBuf> {
    std::env::split_paths(path)
        .filter(|d| d != skip)
        .map(|d| d.join(tool))
        .find(|p| {
            p.metadata()
                .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        })
}

fn quote(p: &Path) -> String {
    format!("'{}'", p.to_string_lossy().replace('\'', r"'\''"))
}
