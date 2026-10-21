//! `cos agents supervise <id>`: the owner of one agent's harness. Started
//! detached by `spawn`; it lives exactly as long as the harness, plus the
//! time to record how it ended.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::RecvTimeoutError;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};

use super::{
    CEILING, Spec, State, Status, dir, harness, read, seed, slots, started_at, write_state,
};
use crate::proc;

/// How often the owner checks for a stop request while the harness is quiet.
const TICK: Duration = Duration::from_millis(250);

/// State is rewritten at most this often while lines stream in.
const FLUSH: Duration = Duration::from_millis(500);

static STOP: AtomicBool = AtomicBool::new(false);

extern "C" fn on_signal(_: libc::c_int) {
    STOP.store(true, Ordering::SeqCst);
}

pub fn supervise(home: &Path, id: &str) -> Result<()> {
    for sig in [libc::SIGTERM, libc::SIGINT, libc::SIGHUP] {
        // SAFETY: the handler only stores to an atomic, which is async-signal-safe.
        unsafe {
            libc::signal(sig, on_signal as *const () as libc::sighandler_t);
        }
    }
    let (spec, mut state) = read(home, id)?;
    let d = dir(home, id);
    let stop_file = d.join("stop");
    let mut events = OpenOptions::new()
        .create(true)
        .append(true)
        .open(d.join("events.jsonl"))?;
    let stderr = std::fs::File::create(d.join("harness.stderr.log"))?;
    let stopped = || STOP.load(Ordering::SeqCst) || stop_file.exists();

    state.supervisor = Some(std::process::id() as i32);
    state.status = Status::Queued;
    write_state(home, id, &state)?;
    let slots = spec.slots.unwrap_or_else(slots::run_slots);
    let _slot = loop {
        if let Some(held) = slots::try_run_slot(home, slots)? {
            break held;
        }
        if stopped() {
            let _ = std::fs::remove_file(&stop_file);
            return finish(home, id, &mut state, Status::Killed, None);
        }
        std::thread::sleep(TICK);
    };

    state.started = Some(jiff::Timestamp::now().to_string());
    let mut cmd = match prepare(home, id, &spec, &mut state) {
        Ok(cmd) => cmd,
        Err(e) => {
            finish(home, id, &mut state, Status::Failed, Some(format!("{e:#}")))?;
            return Err(e);
        }
    };
    cmd.stdout(Stdio::piped()).stderr(stderr);
    let mut child = match proc::spawn_grouped(&mut cmd) {
        Ok(c) => c,
        Err(e) => {
            finish(home, id, &mut state, Status::Failed, Some(format!("{e:#}")))?;
            return Err(e);
        }
    };
    let pgid = proc::group_of(&child);
    state.pgid = Some(pgid);
    state.pgid_started = started_at(pgid);
    state.status = Status::Running;
    write_state(home, id, &state)?;

    let rx = proc::lines(child.stdout.take().context("harness stdout not piped")?);
    let until = Instant::now() + Duration::from_secs(spec.deadline_secs).min(CEILING);
    let mut failed = None;
    let mut flushed = Instant::now();

    let status = loop {
        if stopped() {
            proc::kill_and_reap(&mut child);
            break Status::Killed;
        }
        let left = until.saturating_duration_since(Instant::now());
        if left.is_zero() {
            proc::kill_and_reap(&mut child);
            break Status::TimedOut;
        }
        match rx.recv_timeout(left.min(TICK)) {
            Ok(line) => {
                writeln!(events, "{line}")?;
                apply(&mut state, harness::read(spec.harness, &line), &mut failed);
                if flushed.elapsed() >= FLUSH {
                    write_state(home, id, &state)?;
                    flushed = Instant::now();
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                break exit(&mut child, &mut state, until, &stopped, failed.is_some());
            }
        }
    };
    // Whatever the harness started and left running goes with it.
    proc::kill_group(pgid);
    let _ = std::fs::remove_file(&stop_file);
    finish(home, id, &mut state, status, failed)
}

/// Seeds a worktree agent's tree, then builds the harness command with the
/// build-tool shims first on its PATH.
fn prepare(home: &Path, id: &str, spec: &Spec, state: &mut State) -> Result<Command> {
    let d = dir(home, id);
    if let (true, Some(repo)) = (spec.worktree, spec.repo.as_deref()) {
        state.status = Status::Seeding;
        write_state(home, id, state)?;
        state.seeded =
            seed::run(repo, &spec.workdir, &d.join("seed.log")).context("seed the worktree")?;
    }
    let mut cmd = harness::command(spec);
    let path = std::env::var("PATH").unwrap_or_default();
    let bin = d.join("bin");
    let cos = std::env::current_exe().context("locate the cos binary")?;
    state.shimmed = slots::write_shims(&bin, &cos, &slots::build_lock()?, &path)?;
    if !state.shimmed.is_empty() {
        let joined =
            std::env::join_paths(std::iter::once(bin).chain(std::env::split_paths(&path)))?;
        cmd.env("PATH", joined);
    }
    Ok(cmd)
}

/// The harness closed its stdout: wait for it to exit, still honouring the
/// deadline and a stop request.
fn exit(
    child: &mut Child,
    state: &mut State,
    until: Instant,
    stopped: &dyn Fn() -> bool,
    failed: bool,
) -> Status {
    loop {
        match child.try_wait() {
            Ok(Some(st)) => {
                state.exit = st.code();
                return if st.success() && !failed {
                    Status::Done
                } else {
                    Status::Failed
                };
            }
            Ok(None) => {}
            Err(_) => {
                proc::kill_and_reap(child);
                return Status::Failed;
            }
        }
        if stopped() {
            proc::kill_and_reap(child);
            return Status::Killed;
        }
        if Instant::now() >= until {
            proc::kill_and_reap(child);
            return Status::TimedOut;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn apply(state: &mut State, t: harness::Tick, failed: &mut Option<String>) {
    if let Some(last) = t.last {
        state.last = last;
    }
    state.tool_calls += t.tool_calls;
    if t.cost_usd.is_some() {
        state.cost_usd = t.cost_usd;
    }
    if let Some(n) = t.tokens {
        state.tokens = Some(state.tokens.unwrap_or(0) + n);
    }
    if let Some(s) = t.summary {
        state.summary = s;
    }
    if t.session.is_some() {
        state.session = t.session;
    }
    if t.failed.is_some() {
        *failed = t.failed;
    }
}

fn finish(
    home: &Path,
    id: &str,
    state: &mut State,
    status: Status,
    failed: Option<String>,
) -> Result<()> {
    state.status = status;
    state.ended = Some(jiff::Timestamp::now().to_string());
    state.error = match status {
        Status::Failed => failed.or_else(|| state.exit.map(|c| format!("exit code {c}"))),
        Status::TimedOut => Some("stopped at its deadline".into()),
        Status::Killed => Some("stopped on request".into()),
        _ => None,
    };
    write_state(home, id, state)
}
