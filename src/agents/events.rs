//! `out/agents/<id>/events.jsonl`: a hire's transitions, one JSON line each.
//! Appends take `events.lock`, so two followers never double-log; a failed
//! append is audited and never fails the action it records.

use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

use anyhow::{Result, bail};
use jiff::Timestamp;

use super::State;
use super::dir;
use super::timeline::{Event, Kind, Summary, Totals, stored_summary, summary, totals};
use crate::claude;

pub fn path(home: &Path, id: &str) -> PathBuf {
    dir(home, id).join("events.jsonl")
}

/// Appends `ev` unless it repeats the log's last `Word` (the same word) or
/// the last `Judged` (the same verdict timestamp). Both checks run under the
/// lock, so two writers that saw one change record it once.
pub fn append(home: &Path, id: &str, ev: &Event) -> Result<()> {
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir(home, id).join("events.lock"))?;
    // SAFETY: flock on a descriptor this process owns; no memory is touched.
    if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX) } != 0 {
        bail!("lock {id}'s events: {}", std::io::Error::last_os_error());
    }
    let existing = read(home, id);
    let repeats = match ev.kind {
        Kind::Word => existing
            .iter()
            .rev()
            .find(|e| e.kind == Kind::Word)
            .is_some_and(|e| e.word == ev.word),
        Kind::Judged => existing
            .iter()
            .rev()
            .find(|e| e.kind == Kind::Judged)
            .is_some_and(|e| e.judged == ev.judged),
        _ => false,
    };
    if repeats {
        return Ok(());
    }
    let mut line = serde_json::to_vec(ev)?;
    line.push(b'\n');
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path(home, id))?
        .write_all(&line)?;
    Ok(())
}

/// `append`, for a writer that must not fail: a failed append goes to the
/// audit log with the run id, the kind and the error.
pub(crate) fn record(home: &Path, id: &str, ev: Event) {
    if let Err(e) = append(home, id, &ev) {
        let note = serde_json::json!({
            "event": serde_json::to_value(ev.kind).unwrap_or_default(),
            "error": format!("{e:#}"),
        });
        let _ = claude::audit_note(&claude::audit_path(home), id, note);
    }
}

/// Every event in the log, in order. Lines that do not parse are skipped.
pub fn read(home: &Path, id: &str) -> Vec<Event> {
    std::fs::read_to_string(path(home, id))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

type Cached = (u64, Option<SystemTime>, Totals);

static CACHE: Mutex<BTreeMap<PathBuf, Cached>> = Mutex::new(BTreeMap::new());

/// The clock at `now` for run `id`. The log is folded only when its length
/// or mtime changed since the last call; `now` is applied on every call.
pub fn tail_summary(home: &Path, id: &str, now: Timestamp, state: &State) -> Result<Summary> {
    let p = path(home, id);
    let meta = match std::fs::metadata(&p) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(stored_summary(state, now));
        }
        Err(e) => return Err(e.into()),
    };
    if meta.len() == 0 {
        return Ok(stored_summary(state, now));
    }
    let (len, mtime) = (meta.len(), meta.modified().ok());
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((l, m, t)) = cache.get(&p)
        && *l == len
        && *m == mtime
    {
        return Ok(summary(t, now));
    }
    let t = totals(&read(home, id));
    cache.insert(p, (len, mtime, t));
    Ok(summary(&t, now))
}
