//! Following a hire in Herdr: what its terminal says, its deadline, and the
//! one verdict. cos never recomputes a status: the word on a run is the one
//! its Herdr terminal reported.
//!
//! The judge runs exactly once per run, on whichever comes first: the
//! terminal reports `done` after the hire's first message, its program ends,
//! it is stopped at its deadline (the stop key, then `hc kill` after a
//! grace), or you mark it done. The verdict is on disk before anything else
//! reads it, under `judge.lock`, so two followers (the window and `cos
//! agents watch`) and a restart never judge it twice. `cos agents judge`
//! judges again on request.
//!
//! Every write to a terminal (`send`, `interrupt`, `kill`, `rm`) first reads
//! it back and checks its `hire` label, so cos only ever touches the
//! terminals it started.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Result, bail};

use super::timeline::{self, By, Event, Kind};
use super::{
    Seen, Spec, State, Status, Verdict, dir, events, judge, ledger, list, read, read_raw,
    write_state,
};
use crate::herdr::{self, Term};

/// Lines of screen and history kept when a run ends.
const SCREEN: usize = 200;

/// Between the stop key at the deadline and `hc kill`.
const GRACE: Duration = Duration::from_secs(10);

pub fn grace() -> Duration {
    std::env::var("COS_HERDR_GRACE_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .map_or(GRACE, Duration::from_secs)
}

/// The Herdr terminal's name for run `id`: within hc's 1-32 `[a-z0-9-_]`.
pub fn terminal_name(id: &str) -> String {
    format!("cos-{id}")
}

pub(crate) fn seen(t: &Term, prev: Option<&Seen>) -> Seen {
    let word = t.status.as_ref().filter(|w| !w.state.is_empty());
    Seen {
        address: t.address.clone(),
        word: word.map(|w| w.state.clone()),
        kind: word.and_then(|w| w.kind.clone()),
        msg: word.and_then(|w| w.msg.clone()),
        turn: t.turn,
        live: t.live(),
        exit_code: t.exit_code,
        reason: t.reason.clone(),
        first_message: t
            .first_message
            .clone()
            .or_else(|| prev.and_then(|p| p.first_message.clone())),
        at: jiff::Timestamp::now().to_string(),
        interrupted: prev.and_then(|p| p.interrupted.clone()),
    }
}

/// What `follow_all` keeps between passes, per home: its runs' stamp at the
/// last scan, and the runs that scan found waiting on a person when some run
/// was in Herdr.
#[derive(Default)]
pub struct Follow {
    seen: BTreeMap<PathBuf, (String, Option<Vec<String>>)>,
}

impl Follow {
    /// One pass over the hires of every business in `registry`, and of
    /// `also`: polls each live terminal (`tick`: one bounded `hc get` per
    /// run), stops runs at their deadline and judges each once. A home whose
    /// stamp has not moved and had nothing in Herdr is not read again; one
    /// with no run in Herdr is read from its files alone and left as it is.
    pub fn follow_all(&mut self, registry: &Path, also: Option<&Path>) {
        let mut homes: Vec<PathBuf> = crate::business::registered_homes(registry)
            .into_values()
            .collect();
        if let Some(home) = also
            && !homes.iter().any(|h| h == home)
        {
            homes.push(home.to_owned());
        }
        self.follow_homes(&homes);
    }

    /// `follow_all` over exactly `homes`; homes it followed before and not
    /// now are forgotten.
    pub fn follow_homes(&mut self, homes: &[PathBuf]) {
        self.seen.retain(|h, _| homes.contains(h));
        for home in homes {
            let stamp = crate::dash::stamp([super::root(home)]);
            if self
                .seen
                .get(home)
                .is_some_and(|(s, live)| *s == stamp && live.is_none())
            {
                continue;
            }
            let live = super::in_herdr(home);
            if !live.is_empty()
                && let Err(e) = tick(home)
            {
                eprintln!("cos: following hires in Herdr: {e:#}");
            }
            let blocked = (!live.is_empty()).then(|| {
                super::in_herdr(home)
                    .into_iter()
                    .filter(|(_, st)| st.herdr.as_ref().and_then(|h| h.blocked()).is_some())
                    .map(|(spec, _)| spec.id)
                    .collect()
            });
            self.seen.insert(
                home.clone(),
                (crate::dash::stamp([super::root(home)]), blocked),
            );
        }
    }

    /// The runs of `home` the last pass found waiting on a person; `None`
    /// when none of its runs was in Herdr.
    pub fn blocked(&self, home: &Path) -> Option<&[String]> {
        self.seen.get(home).and_then(|(_, b)| b.as_deref())
    }
}

/// One look at every hire Herdr holds: reads each live terminal, stops what
/// is past its deadline, judges what is due. Returns how many runs it read.
pub fn tick(home: &Path) -> Result<usize> {
    let mut n = 0;
    for (spec, state) in list(home)?.runs {
        if spec.legacy() {
            continue;
        }
        if state.status.terminal() {
            if state.verdict.is_none() {
                judge(home, &spec, &trigger(state.status), false)?;
                n += 1;
            }
            continue;
        }
        if state.herdr.is_some() {
            observe(home, &spec.id)?;
            n += 1;
        }
    }
    Ok(n)
}

thread_local! {
    static HELD: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// Runs `f` under the run's `state.lock`, so a read-modify-write of its state
/// cannot interleave with another follower's. Re-entrant on one thread:
/// `observe` reaches `close` and `judge` under the same lock.
fn with_state_lock<R>(home: &Path, id: &str, f: impl FnOnce() -> Result<R>) -> Result<R> {
    if HELD.with(|h| h.borrow().iter().any(|x| x == id)) {
        return f();
    }
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir(home, id).join("state.lock"))?;
    // SAFETY: flock on a descriptor this process owns; no memory is touched.
    if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX) } != 0 {
        bail!("lock {id}'s state: {}", std::io::Error::last_os_error());
    }
    HELD.with(|h| h.borrow_mut().push(id.to_owned()));
    let out = f();
    HELD.with(|h| h.borrow_mut().pop());
    out
}

/// Reads one run's terminal and acts on what it says.
pub fn observe(home: &Path, id: &str) -> Result<State> {
    with_state_lock(home, id, || observe_locked(home, id))
}

fn observe_locked(home: &Path, id: &str) -> Result<State> {
    let (spec, mut state) = read(home, id)?;
    let Some(prev) = state.herdr.clone() else {
        return Ok(state);
    };
    if state.status.terminal() {
        return Ok(state);
    }
    let t = match herdr::get(&prev.address) {
        Ok(t) => t,
        // Herdr is down: nothing is known, nothing changes.
        Err(_) => return Ok(state),
    };
    let Some(t) = t else {
        let mut gone = prev.clone();
        gone.live = false;
        gone.word = None;
        state.herdr = Some(gone);
        end(
            home,
            &spec,
            &mut state,
            Status::Lost,
            Some("its Herdr terminal was removed outside cos".into()),
        )?;
        return Ok(read_raw(home, id)?.1);
    };
    if !is_ours(&t, id) {
        let mut gone = prev.clone();
        gone.live = false;
        state.herdr = Some(gone);
        end(
            home,
            &spec,
            &mut state,
            Status::Lost,
            Some(format!("{} now names another terminal", prev.address)),
        )?;
        return Ok(read_raw(home, id)?.1);
    }
    let now = seen(&t, Some(&prev));
    let word = now.word.as_deref().and_then(timeline::word);
    if word.is_some() && word != prev.word.as_deref().and_then(timeline::word) {
        events::record(
            home,
            id,
            Event {
                word,
                turn: Some(now.turn),
                ..Event::new(now.at.clone(), Kind::Word, By::Herdr)
            },
        );
    }
    if prev.first_message.is_none() && now.first_message.is_some() {
        events::record(
            home,
            id,
            Event {
                turn: Some(now.turn),
                ..Event::new(now.at.clone(), Kind::FirstMessage, By::You)
            },
        );
    }
    let changed = Seen {
        at: prev.at.clone(),
        ..now.clone()
    } != prev;
    state.herdr = Some(now.clone());
    if !now.live {
        save_screen(home, id, &now.address);
        state.exit = now.exit_code;
        let (status, why) = match (now.exit_code, now.reason.as_deref()) {
            (Some(0), _) => (Status::Done, None),
            (_, Some("killed")) => (Status::Killed, Some("ended in Herdr".to_owned())),
            (Some(c), _) => (Status::Failed, Some(format!("exited with code {c}"))),
            (None, r) => (
                Status::Failed,
                Some(format!(
                    "its program ended ({})",
                    r.unwrap_or("no reason given")
                )),
            ),
        };
        end(home, &spec, &mut state, status, why)?;
        return Ok(read_raw(home, id)?.1);
    }
    // Only a change is written: the window refreshes on every write.
    if changed {
        write_state(home, id, &state)?;
    }
    if past_deadline(&spec, &state) {
        match &now.interrupted {
            None => {
                herdr::interrupt(&now.address)?;
                let mut s = read_raw(home, id)?.1;
                if let Some(h) = s.herdr.as_mut() {
                    h.interrupted = Some(jiff::Timestamp::now().to_string());
                }
                write_state(home, id, &s)?;
                events::record(
                    home,
                    id,
                    Event::new(
                        jiff::Timestamp::now().to_string(),
                        Kind::DeadlineStop,
                        By::Cos,
                    ),
                );
            }
            Some(at) if super::age(at) >= grace() => {
                finish(
                    home,
                    &spec,
                    Status::TimedOut,
                    Some(format!(
                        "stopped at its {}s deadline: interrupted, then ended {}s later",
                        spec.deadline_secs,
                        grace().as_secs()
                    )),
                    "its deadline",
                )?;
            }
            Some(_) => {}
        }
        return Ok(read_raw(home, id)?.1);
    }
    if now.word.as_deref() == Some("done")
        && now.turn >= 1
        && now.unsent().is_none()
        && state.verdict.is_none()
    {
        judge(home, &spec, &format!("done at turn {}", now.turn), false)?;
    }
    Ok(read_raw(home, id)?.1)
}

fn past_deadline(spec: &Spec, state: &State) -> bool {
    let from = state.started.as_deref().unwrap_or(&spec.created);
    super::age(from) >= Duration::from_secs(spec.deadline_secs).min(super::CEILING)
}

fn is_ours(t: &Term, id: &str) -> bool {
    t.labels.get("hire").map(String::as_str) == Some(id)
}

/// The run's terminal, read back and checked to be the one cos started.
fn ours(spec: &Spec, state: &State) -> Result<Term> {
    let Some(h) = &state.herdr else {
        bail!("{} has no Herdr terminal", spec.id);
    };
    match herdr::get(&h.address)? {
        Some(t) if is_ours(&t, &spec.id) => Ok(t),
        Some(_) => bail!(
            "{} now names another terminal; cos leaves it alone",
            h.address
        ),
        None => bail!("its Herdr terminal {} is gone", h.address),
    }
}

/// A live run, its terminal checked.
fn live(home: &Path, id: &str) -> Result<(Spec, State, Term)> {
    let (spec, state) = read(home, id)?;
    if spec.legacy() {
        bail!("{id} {}", super::LEGACY);
    }
    if state.status.preparing() {
        bail!("{id} is still being prepared: its worktree first, then Herdr");
    }
    if state.status.terminal() {
        bail!("{id} has ended ({}); hire again to carry on", state.status);
    }
    let t = ours(&spec, &state)?;
    if !t.live() {
        bail!("{id}'s Herdr terminal has ended");
    }
    Ok((spec, state, t))
}

/// Sends your reply into the run's terminal (`hc send`).
pub fn say(home: &Path, id: &str, text: &str) -> Result<State> {
    let text = text.trim();
    if text.is_empty() {
        bail!("write the reply first");
    }
    let (_, _, t) = live(home, id)?;
    herdr::send(&t.address, text)?;
    events::record(
        home,
        id,
        Event {
            chars: Some(text.chars().count()),
            ..Event::new(jiff::Timestamp::now().to_string(), Kind::Said, By::You)
        },
    );
    observe(home, id)
}

/// Presses the agent's stop key (`hc interrupt`): its turn ends, the agent
/// stays for your next message.
pub fn pause(home: &Path, id: &str) -> Result<State> {
    let (_, _, t) = live(home, id)?;
    herdr::interrupt(&t.address)?;
    events::record(
        home,
        id,
        Event {
            turn: Some(t.turn),
            ..Event::new(jiff::Timestamp::now().to_string(), Kind::Paused, By::You)
        },
    );
    observe(home, id)
}

/// Ends the run's terminal (`hc kill`) and judges it, once.
pub fn mark_done(home: &Path, id: &str) -> Result<State> {
    let (spec, state) = read(home, id)?;
    if state.status.terminal() {
        if !spec.legacy() {
            judge(home, &spec, "marked done", false)?;
        }
        return Ok(read_raw(home, id)?.1);
    }
    live(home, id)?;
    events::record(
        home,
        id,
        Event::new(
            jiff::Timestamp::now().to_string(),
            Kind::MarkedDone,
            By::You,
        ),
    );
    finish(home, &spec, Status::Done, None, "marked done")?;
    Ok(read_raw(home, id)?.1)
}

/// What the terminal shows now with `lines` of history; once it has ended,
/// what it showed then.
pub fn screen(home: &Path, id: &str, lines: usize) -> Result<String> {
    let (spec, state) = read(home, id)?;
    if let Some(h) = state.herdr.as_ref().filter(|h| h.live)
        && ours(&spec, &state).is_ok()
    {
        return herdr::read(&h.address, lines);
    }
    Ok(std::fs::read_to_string(dir(home, id).join("screen.txt")).unwrap_or_default())
}

/// Shows the run's terminal in the Herdr window.
pub fn open_in_herdr(home: &Path, id: &str) -> Result<String> {
    let (spec, state) = read(home, id)?;
    let t = ours(&spec, &state)?;
    herdr::focus(&t.address, &terminal_name(id))
}

/// Ends a live terminal with its last screen kept, then the run.
pub(crate) fn finish(
    home: &Path,
    spec: &Spec,
    status: Status,
    error: Option<String>,
    why: &str,
) -> Result<()> {
    with_state_lock(home, &spec.id, || {
        finish_locked(home, spec, status, error, why)
    })
}

fn finish_locked(
    home: &Path,
    spec: &Spec,
    status: Status,
    error: Option<String>,
    why: &str,
) -> Result<()> {
    let (_, mut state) = read_raw(home, &spec.id)?;
    if let Some(h) = state.herdr.clone().filter(|h| h.live) {
        let t = ours(spec, &state)?;
        if t.live() {
            save_screen(home, &spec.id, &h.address);
            if let Err(e) = herdr::kill(&h.address)
                && herdr::get(&h.address)?.is_some_and(|t| t.live())
            {
                return Err(e);
            }
        }
        state = read_raw(home, &spec.id)?.1;
        if let Some(h) = state.herdr.as_mut() {
            h.live = false;
            h.word = None;
            h.kind = None;
            h.reason = Some("killed".into());
        }
    }
    close(home, spec, &mut state, status, error, why)
}

/// Removes a finished run's terminal from Herdr, if it is still there.
pub(crate) fn remove_terminal(spec: &Spec, state: &State) -> Result<()> {
    let Some(h) = &state.herdr else {
        return Ok(());
    };
    let t = match herdr::get(&h.address) {
        Ok(Some(t)) if is_ours(&t, &spec.id) => t,
        Ok(_) => return Ok(()),
        Err(e) => bail!(
            "could not reach Herdr to remove its terminal {} ({e:#}): start Herdr, or clean again later",
            h.address
        ),
    };
    if t.live() {
        bail!(
            "its Herdr terminal {} is still live: stop it first",
            h.address
        );
    }
    herdr::rm(&h.address)
}

fn save_screen(home: &Path, id: &str, address: &str) {
    if let Ok(text) = herdr::read(address, SCREEN) {
        let _ = std::fs::write(dir(home, id).join("screen.txt"), text);
    }
}

fn trigger(status: Status) -> String {
    match status {
        Status::Done => "it ended",
        Status::Failed => "it failed",
        Status::Killed => "it was stopped",
        Status::TimedOut => "its deadline",
        _ => "it was lost",
    }
    .to_owned()
}

/// Records how the run ended, then judges it once.
pub(crate) fn end(
    home: &Path,
    spec: &Spec,
    state: &mut State,
    status: Status,
    error: Option<String>,
) -> Result<()> {
    let why = trigger(status);
    close(home, spec, state, status, error, &why)
}

fn close(
    home: &Path,
    spec: &Spec,
    state: &mut State,
    status: Status,
    error: Option<String>,
    why: &str,
) -> Result<()> {
    with_state_lock(home, &spec.id, || {
        close_locked(home, spec, state, status, error, why)
    })
}

fn close_locked(
    home: &Path,
    spec: &Spec,
    state: &mut State,
    status: Status,
    error: Option<String>,
    why: &str,
) -> Result<()> {
    state.status = status;
    state.ended = Some(jiff::Timestamp::now().to_string());
    state.launcher = None;
    state.launcher_started = None;
    if error.is_some() {
        state.error = error;
    }
    write_state(home, &spec.id, state)?;
    events::record(
        home,
        &spec.id,
        Event {
            status: Some(status),
            ..Event::new(
                state.ended.clone().unwrap_or_default(),
                Kind::Ended,
                By::Cos,
            )
        },
    );
    judge(home, spec, why, false)?;
    *state = read_raw(home, &spec.id)?.1;
    Ok(())
}

/// The verdict on the worktree as it is, under the run's judge lock. Unless
/// `again`, a run already judged keeps its verdict. A live run is judged as
/// `done`: it said so, or you did.
pub(crate) fn judge(home: &Path, spec: &Spec, why: &str, again: bool) -> Result<Verdict> {
    with_state_lock(home, &spec.id, || judge_locked(home, spec, why, again))
}

fn judge_locked(home: &Path, spec: &Spec, why: &str, again: bool) -> Result<Verdict> {
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir(home, &spec.id).join("judge.lock"))?;
    // SAFETY: flock on a descriptor this process owns; no memory is touched.
    if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX) } != 0 {
        bail!(
            "lock {}'s judge: {}",
            spec.id,
            std::io::Error::last_os_error()
        );
    }
    let (_, mut state) = read_raw(home, &spec.id)?;
    if let (Some(v), false) = (&state.verdict, again) {
        return Ok(v.clone());
    }
    let mut as_judged = state.clone();
    if !as_judged.status.terminal() {
        as_judged.status = Status::Done;
    }
    let bin = dir(home, &spec.id).join("bin");
    let v = judge::judge(
        spec,
        &as_judged,
        (!state.shimmed.is_empty()).then_some(bin.as_path()),
    );
    if crate::proc::tracking_closed() {
        bail!(
            "{} is not judged: this process is stopping and may have killed its checks",
            spec.id
        );
    }
    state.verdict = Some(v.clone());
    state.judged_on = Some(why.to_owned());
    write_state(home, &spec.id, &state)?;
    events::record(
        home,
        &spec.id,
        Event {
            judged: Some(v.judged.clone()),
            outcome: Some(v.outcome),
            ..Event::new(v.judged.clone(), Kind::Judged, By::Cos)
        },
    );
    as_judged.verdict = Some(v.clone());
    if let Some(row) = ledger::row(spec, &as_judged) {
        ledger::append(home, &row)?;
    }
    Ok(v)
}
