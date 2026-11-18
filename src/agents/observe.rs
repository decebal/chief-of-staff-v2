//! `cos agents observe <id>`: cos watching a hire that herdr owns. Started
//! detached by `spawn_in_herdr`; it never touches the pty. It reads herdr's
//! truth (`hc wait`/`hc get`/`hc read`), mirrors it into the run's state so
//! every `cos agents` command reads the same file as before, and judges the
//! worktree when the space ends. herdr owns the run for its whole life; this
//! only watches and judges — one owner per run.

use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};
use serde_json::Value;

use super::{
    RunDir, Spec, State, Status, harness, judge, ledger, load, run_dir, slots, write_state,
};

/// Seconds `hc wait` blocks before it checks in; the loop then re-reads state.
const TIMEOUT: &str = "30";

pub fn observe(home: &Path, id: &str) -> Result<()> {
    let run = run_dir(home, id)?;
    let (spec, mut state) = load(&run)?;

    state.supervisor = Some(std::process::id() as i32);
    if state.started.is_none() {
        state.started = Some(jiff::Timestamp::now().to_string());
    }
    state.status = Status::Running;
    // Build-tool shims so a check's `cargo` at judge time waits for the build
    // slot instead of racing the machine's other builds.
    let bin = run.path().join("bin");
    let path = std::env::var("PATH").unwrap_or_default();
    let cos = std::env::current_exe().context("locate the cos binary")?;
    state.shimmed = slots::write_shims(&bin, &cos, &slots::build_lock()?, &path)?;
    write_state(&run, &state)?;

    let t = crate::herdr::name(id);
    loop {
        // Observe, never drive: wait blocks until the space stops working or
        // the check-in elapses. It sends nothing to the pty.
        let _ = Command::new("hc")
            .args(["wait", &t, "--timeout", TIMEOUT])
            .status();
        let Some(g) = get(&t) else {
            return finish(
                home,
                &run,
                &spec,
                &mut state,
                Status::Lost,
                Some("herdr has no such terminal".into()),
            );
        };
        if g["state"].as_str() == Some("finished") {
            state.exit = g["exit_code"].as_i64().map(|c| c as i32);
            let mut failed = None;
            replay(&spec, &mut state, &t, &mut failed);
            let status = if run.path().join("stop").exists() {
                Status::Killed
            } else {
                match state.exit {
                    Some(0) if failed.is_none() => Status::Done,
                    _ => Status::Failed,
                }
            };
            return finish(home, &run, &spec, &mut state, status, failed);
        }
        state.status = Status::Running;
        let title = g["title"].as_str().unwrap_or_default();
        if !title.is_empty() {
            state.last = title.to_owned();
        } else if let Some(s) = g["status"]["state"].as_str() {
            state.last = format!("herdr: {s}");
        }
        write_state(&run, &state)?;
    }
}

/// `hc get <t> --json`, or `None` when herdr no longer knows the terminal.
fn get(t: &str) -> Option<Value> {
    let out = Command::new("hc").args(["get", t, "--json"]).output().ok()?;
    out.status
        .success()
        .then(|| serde_json::from_slice(&out.stdout).ok())
        .flatten()
}

/// Folds the harness stream out of the space's scrollback for the final cost,
/// summary and any error. Best effort: the pty history is finite, so a long
/// run can lose early lines — the verdict still reads the worktree directly.
fn replay(spec: &Spec, state: &mut State, t: &str, failed: &mut Option<String>) {
    let Ok(out) = Command::new("hc").args(["read", t]).output() else {
        return;
    };
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        apply(state, harness::read(spec.harness, line), failed);
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

/// Records how the space ended, then judges the worktree. The outcome lands on
/// disk before the verdict, so a watcher sees the hire end while it is judged.
fn finish(
    home: &Path,
    run: &RunDir,
    spec: &Spec,
    state: &mut State,
    status: Status,
    failed: Option<String>,
) -> Result<()> {
    state.status = status;
    state.ended = Some(jiff::Timestamp::now().to_string());
    state.error = match status {
        Status::Failed => failed.or_else(|| state.exit.map(|c| format!("exit code {c}"))),
        Status::Killed => Some("stopped on request".into()),
        Status::Lost => Some("herdr has no such terminal".into()),
        _ => None,
    };
    write_state(run, state)?;
    let bin = (!state.shimmed.is_empty()).then(|| run.path().join("bin"));
    state.verdict = Some(judge::judge(spec, state, bin.as_deref()));
    write_state(run, state)?;
    if let Some(row) = ledger::row(spec, state) {
        ledger::append(home, &row)?;
    }
    Ok(())
}
