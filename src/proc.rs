//! Bounded child processes. The caller owns the child for its whole life: one
//! deadline for the operation, kill the process group on expiry, then reap.

use std::io::{BufRead, BufReader, Read};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};

/// Hard ceiling for any child `cos` starts. Lower per call, never higher.
pub const CEILING: Duration = Duration::from_secs(300);

/// How long a pipe may stay open after its child exited. Past this, whatever
/// still holds it is a leftover of the child's group and is killed.
const PIPE_GRACE: Duration = Duration::from_secs(2);

pub struct Output {
    pub status: ExitStatus,
    pub stdout: String,
    pub stderr: String,
}

/// Run to completion or kill at `deadline`. Never waits past `CEILING`.
pub fn run(cmd: &mut Command, deadline: Duration) -> Result<Output> {
    let deadline = deadline.min(CEILING);
    let mut child = spawn_grouped(cmd.stdout(Stdio::piped()).stderr(Stdio::piped()))?;
    let pgid = group_of(&child);
    let out = Drain::start(child.stdout.take());
    let err = Drain::start(child.stderr.take());
    let status = wait_until(&mut child, Instant::now() + deadline)?;
    Ok(Output {
        status,
        stdout: out.finish(pgid),
        stderr: err.finish(pgid),
    })
}

/// Spawn with stdout delivered line by line; `on_line` returning `false` stops
/// reading early. The child is killed and reaped on deadline, early stop, or error.
pub fn stream_lines(
    cmd: &mut Command,
    deadline: Duration,
    mut on_line: impl FnMut(&str) -> Result<bool>,
) -> Result<(ExitStatus, String)> {
    let until = Instant::now() + deadline.min(CEILING);
    let mut child = spawn_grouped(cmd.stdout(Stdio::piped()).stderr(Stdio::piped()))?;
    let pgid = group_of(&child);
    let err = Drain::start(child.stderr.take());
    let rx = lines(child.stdout.take().context("child stdout not piped")?);

    loop {
        let left = until.saturating_duration_since(Instant::now());
        if left.is_zero() {
            kill_and_reap(&mut child);
            bail!("timed out after {}s", deadline.min(CEILING).as_secs());
        }
        match rx.recv_timeout(left) {
            Ok(line) => match on_line(&line) {
                Ok(true) => {}
                Ok(false) => break,
                Err(e) => {
                    kill_and_reap(&mut child);
                    return Err(e);
                }
            },
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    let status = wait_until(&mut child, until)?;
    Ok((status, err.finish(pgid)))
}

/// Spawns `cmd` as the leader of a new process group, stdin closed, so one
/// signal to the group reaches everything it starts.
pub fn spawn_grouped(cmd: &mut Command) -> Result<Child> {
    cmd.stdin(Stdio::null()).process_group(0);
    cmd.spawn()
        .with_context(|| format!("spawn {:?}", cmd.get_program()))
}

/// Lines of `pipe`, read on their own thread, so the owner can poll with a
/// deadline instead of blocking in `read`.
pub fn lines<R: Read + Send + 'static>(pipe: R) -> mpsc::Receiver<String> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        for line in BufReader::new(pipe).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    rx
}

pub fn group_of(child: &Child) -> libc::pid_t {
    child.id() as libc::pid_t
}

/// A whole pipe read on its own thread. `finish` never blocks for long: a
/// grandchild that inherited the pipe would otherwise hold it open forever.
struct Drain(mpsc::Receiver<String>);

impl Drain {
    fn start<R: Read + Send + 'static>(pipe: Option<R>) -> Self {
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let mut buf = String::new();
            if let Some(mut p) = pipe {
                let _ = p.read_to_string(&mut buf);
            }
            let _ = tx.send(buf);
        });
        Self(rx)
    }

    fn finish(self, pgid: libc::pid_t) -> String {
        if let Ok(s) = self.0.recv_timeout(PIPE_GRACE) {
            return s;
        }
        kill_group(pgid);
        self.0.recv_timeout(PIPE_GRACE).unwrap_or_default()
    }
}

pub fn wait_until(child: &mut Child, until: Instant) -> Result<ExitStatus> {
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        if Instant::now() >= until {
            kill_and_reap(child);
            bail!("timed out waiting for child exit");
        }
        thread::sleep(Duration::from_millis(50));
    }
}

pub fn kill_and_reap(child: &mut Child) {
    kill_group(group_of(child));
    let _ = child.kill();
    let _ = child.wait();
}

/// SIGKILL to every process in group `pgid`. A pgid stays reserved while any
/// member lives, so this cannot reach an unrelated process once the group
/// has members; with none left it is a no-op (ESRCH).
pub fn kill_group(pgid: libc::pid_t) {
    if pgid > 0 {
        // SAFETY: signalling a process group we created; no memory is touched.
        unsafe {
            libc::kill(-pgid, libc::SIGKILL);
        }
    }
}

/// Whether `pid` exists (a zombie counts).
pub fn alive(pid: libc::pid_t) -> bool {
    // SAFETY: signal 0 only checks for existence and permission.
    pid > 0 && unsafe { libc::kill(pid, 0) } == 0
}

/// Whether any process is left in group `pgid`.
pub fn group_alive(pgid: libc::pid_t) -> bool {
    // SAFETY: signal 0 only checks for existence and permission.
    pgid > 0 && unsafe { libc::kill(-pgid, 0) } == 0
}
