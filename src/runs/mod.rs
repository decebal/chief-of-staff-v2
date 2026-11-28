//! Run records (`<home>/out/run-records/<id>.json`), the brief lock, and the
//! run process's own side of the cleanup invariant: every group a run
//! created is gone before anyone writes the run's final state.

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, Write as _};
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::sync::Mutex;
use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};

use crate::proc::{self, GONE_BOUND, Group};

pub const DIR: &str = "out/run-records";
const LOCK: &str = "brief.lock";

/// `cos brief` could not take the brief lock: another brief holds it.
pub const EXIT_BRIEF_HELD: i32 = 73;
/// The owner named in `COS_OWNER` is gone.
pub const EXIT_OWNER_GONE: i32 = 76;

/// The record id an owner hands its run child. Distinct from `COS_RUN_ID`,
/// which a hired agent's terminal carries as its hire id.
pub const RUN_RECORD_ENV: &str = "COS_RUN_RECORD";
/// `<pid>/<start>` of the run child's owner.
pub const OWNER_ENV: &str = "COS_OWNER";
/// SIGTERM, SIGINT or SIGHUP.
pub const EXIT_SIGNALLED: i32 = 143;

pub const ANSWER_MAX: usize = 262_144;
pub const CUT_NOTE: &str = "\n\n[answer cut at 262144 bytes]";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Brief,
    Ask,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
    Queued,
    Running,
    Done,
    Failed,
    Deferred,
    Stopped,
    Lost,
}

impl State {
    pub fn is_final(self) -> bool {
        !matches!(self, State::Queued | State::Running)
    }

    pub fn may_become(self, next: State) -> bool {
        use State::*;
        match self {
            Queued => matches!(next, Running | Failed | Lost | Stopped),
            Running => matches!(next, Done | Failed | Deferred | Stopped | Lost),
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Owner {
    Daemon,
    App,
    Mcp,
    /// A terminal or launchd run: nothing outlives it to reap it.
    #[serde(rename = "self")]
    Itself,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub id: String,
    pub kind: Kind,
    pub business: String,
    pub state: State,
    #[serde(default)]
    pub reason: Option<String>,
    pub owner: Owner,
    #[serde(default)]
    pub pid: Option<libc::pid_t>,
    #[serde(default)]
    pub start: Option<String>,
    /// The grandchild groups; the run's own group is `pid`.
    #[serde(default)]
    pub groups: Vec<Group>,
    #[serde(default)]
    pub owner_pid: Option<libc::pid_t>,
    #[serde(default)]
    pub owner_start: Option<String>,
    pub started: i64,
    #[serde(default)]
    pub ended: Option<i64>,
    #[serde(default)]
    pub said: Option<String>,
    #[serde(default)]
    pub cost_usd: Option<f64>,
    #[serde(default)]
    pub day: Option<String>,
    #[serde(default)]
    pub answer: Option<String>,
    #[serde(default)]
    pub client_req: Option<String>,
    /// The ending an owner asked for just before it signalled the run, which
    /// the run writes when it ends itself on that signal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stopping: Option<Ending>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ending {
    pub state: State,
    pub reason: String,
}

impl Record {
    /// A `queued` record started now.
    pub fn new(id: &str, kind: Kind, business: &str, owner: Owner) -> Self {
        Record {
            id: id.to_owned(),
            kind,
            business: business.to_owned(),
            state: State::Queued,
            reason: None,
            owner,
            pid: None,
            start: None,
            groups: Vec::new(),
            owner_pid: None,
            owner_start: None,
            started: now(),
            ended: None,
            said: None,
            cost_usd: None,
            day: None,
            answer: None,
            client_req: None,
            stopping: None,
        }
    }

    pub fn parse(text: &str) -> Result<Self> {
        let r: Record = serde_json::from_str(text).context("run record")?;
        r.check()?;
        Ok(r)
    }

    /// The `STORE(run_record)` binding beyond what the types hold.
    pub fn check(&self) -> Result<()> {
        ensure!(is_id(&self.id), "run id {:?}", self.id);
        ensure!(is_business(&self.business), "business {:?}", self.business);
        for s in [&self.start, &self.owner_start].into_iter().flatten() {
            ensure!(is_start(s), "start {s:?}");
        }
        ensure!(self.groups.len() <= 64, "{} groups", self.groups.len());
        for g in &self.groups {
            ensure!(g.pgid >= 2 && is_start(&g.start), "group {g:?}");
        }
        let longest = |v: &Option<String>, max: usize, what: &str| -> Result<()> {
            let n = v.as_deref().map_or(0, |s| s.chars().count());
            ensure!(n <= max, "{what} is {n} chars, over {max}");
            Ok(())
        };
        longest(&self.reason, 200, "reason")?;
        longest(&self.said, 8192, "said")?;
        longest(&self.answer, ANSWER_MAX, "answer")?;
        if let Some(e) = &self.stopping {
            ensure!(e.state.is_final(), "stopping as {:?}", e.state);
            longest(&Some(e.reason.clone()), 200, "stopping reason")?;
        }
        if let Some(d) = &self.day {
            ensure!(is_day(d), "day {d:?}");
        }
        if let Some(c) = &self.client_req {
            ensure!(
                c.len() == 16
                    && c.bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
                "client_req {c:?}"
            );
        }
        Ok(())
    }
}

fn digits(s: &str, n: usize) -> bool {
    s.len() == n && s.bytes().all(|b| b.is_ascii_digit())
}

/// `^r-[0-9]{8}-[0-9]{6}-[a-z0-9]{4}$`
pub fn is_id(s: &str) -> bool {
    let Some(rest) = s.strip_prefix("r-") else {
        return false;
    };
    let parts: Vec<&str> = rest.split('-').collect();
    matches!(parts.as_slice(), [d, t, x]
        if digits(d, 8) && digits(t, 6) && x.len() == 4
            && x.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit()))
}

/// `^[a-z0-9][a-z0-9-]{0,39}$`
pub fn is_business(s: &str) -> bool {
    let b = s.as_bytes();
    (1..=40).contains(&b.len())
        && b[0] != b'-'
        && b.iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
}

/// `^[0-9]+\.[0-9]{1,6}$`
fn is_start(s: &str) -> bool {
    s.split_once('.').is_some_and(|(a, b)| {
        !a.is_empty()
            && a.bytes().all(|c| c.is_ascii_digit())
            && (1..=6).contains(&b.len())
            && b.bytes().all(|c| c.is_ascii_digit())
    })
}

pub fn is_day(s: &str) -> bool {
    let p: Vec<&str> = s.split('-').collect();
    matches!(p.as_slice(), [y, m, d] if digits(y, 4) && digits(m, 2) && digits(d, 2))
}

/// The id for a run started at `at`: UTC time plus `suffix`.
pub fn id_at(at: jiff::Timestamp, suffix: &str) -> String {
    format!("r-{}-{suffix}", at.strftime("%Y%m%d-%H%M%S"))
}

pub fn new_id() -> String {
    const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let mut bytes = [0u8; 4];
    if File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .is_err()
    {
        let nanos = jiff::Timestamp::now().subsec_nanosecond() as u32 ^ std::process::id();
        bytes = nanos.to_ne_bytes();
    }
    let suffix: String = bytes
        .iter()
        .map(|b| ALPHABET[*b as usize % ALPHABET.len()] as char)
        .collect();
    id_at(jiff::Timestamp::now(), &suffix)
}

/// `COS_OWNER=<pid>/<start>`.
pub fn parse_owner(spec: &str) -> Result<(libc::pid_t, String)> {
    let (pid, start) = spec
        .split_once('/')
        .with_context(|| format!("COS_OWNER {spec:?}: want <pid>/<start>"))?;
    let pid: libc::pid_t = pid
        .parse()
        .with_context(|| format!("COS_OWNER pid {pid:?}"))?;
    ensure!(pid > 0, "COS_OWNER pid {pid}");
    ensure!(is_start(start), "COS_OWNER start {start:?}");
    Ok((pid, start.to_owned()))
}

fn now() -> i64 {
    jiff::Timestamp::now().as_second()
}

pub fn path(home: &Path, id: &str) -> PathBuf {
    home.join(DIR).join(format!("{id}.json"))
}

pub fn read(home: &Path, id: &str) -> Result<Option<Record>> {
    ensure!(is_id(id), "run id {id:?}");
    read_path(&path(home, id))
}

fn read_path(p: &Path) -> Result<Option<Record>> {
    match std::fs::read_to_string(p) {
        Ok(text) => Record::parse(&text)
            .with_context(|| p.display().to_string())
            .map(Some),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| p.display().to_string()),
    }
}

#[derive(Debug)]
pub enum Write {
    Written(Record),
    /// The transition was not allowed, or the record is final; the record as
    /// it stands.
    Unchanged(Option<Record>),
}

/// Runs `f` holding the exclusive `flock` on `<id>.lock`.
fn locked<T>(home: &Path, id: &str, f: impl FnOnce(&Path) -> Result<T>) -> Result<T> {
    ensure!(is_id(id), "run id {id:?}");
    let dir = home.join(DIR);
    std::fs::create_dir_all(&dir).with_context(|| dir.display().to_string())?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join(format!("{id}.lock")))?;
    loop {
        // SAFETY: flock on a descriptor this function owns until it returns.
        if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX) } == 0 {
            break;
        }
        let e = std::io::Error::last_os_error();
        if e.kind() != std::io::ErrorKind::Interrupted {
            return Err(e).context("lock run record");
        }
    }
    f(&path(home, id))
}

/// Writes `next` over `cur` when the transition is allowed: from nothing
/// only to `queued` or `running`, and never from a final state.
fn apply(p: &Path, id: &str, cur: Option<Record>, next: Record) -> Result<Write> {
    let allowed = match &cur {
        Some(c) => !c.state.is_final() && (next.state == c.state || c.state.may_become(next.state)),
        None => matches!(next.state, State::Queued | State::Running),
    };
    if !allowed {
        return Ok(Write::Unchanged(cur));
    }
    ensure!(next.id == id, "record id {} under {id}", next.id);
    next.check()?;
    let tmp = p.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec(&next)?).with_context(|| tmp.display().to_string())?;
    std::fs::rename(&tmp, p).with_context(|| p.display().to_string())?;
    Ok(Write::Written(next))
}

/// Under the record lock: re-reads the record, and writes what `change`
/// returns if the transition is allowed.
pub fn update(
    home: &Path,
    id: &str,
    change: impl FnOnce(Option<&Record>) -> Option<Record>,
) -> Result<Write> {
    locked(home, id, |p| {
        let cur = read_path(p)?;
        if cur.as_ref().is_some_and(|r| r.state.is_final()) {
            return Ok(Write::Unchanged(cur));
        }
        match change(cur.as_ref()) {
            Some(next) => apply(p, id, cur, next),
            None => Ok(Write::Unchanged(cur)),
        }
    })
}

/// Whether the record's process is gone: its pid is dead or now another
/// process. With no pid yet (`queued`), whether its owner is gone. Never
/// gone where the platform gives no start times, since then no process can
/// be told from a dead one.
pub fn process_gone(r: &Record) -> bool {
    if proc::start_of(std::process::id() as libc::pid_t).is_none() {
        return false;
    }
    match (r.pid, &r.owner_pid) {
        (Some(pid), _) => proc::start_of(pid) != r.start || r.start.is_none(),
        (None, Some(owner)) => proc::start_of(*owner) != r.owner_start || r.owner_start.is_none(),
        (None, None) => true,
    }
}

/// The state a reader shows. A live-looking record whose process is gone is
/// read again first; only when it is still not final is it shown `lost`.
/// Readers never write.
pub fn shown(home: &Path, id: &str) -> Result<Option<State>> {
    ensure!(is_id(id), "run id {id:?}");
    Ok(shown_from(|| read(home, id).ok().flatten(), process_gone))
}

pub fn shown_from(
    mut read: impl FnMut() -> Option<Record>,
    gone: impl Fn(&Record) -> bool,
) -> Option<State> {
    let r = read()?;
    if r.state.is_final() || !gone(&r) {
        return Some(r.state);
    }
    let again = read()?;
    Some(if again.state.is_final() {
        again.state
    } else {
        State::Lost
    })
}

/// How a writer of a final state ends a run's groups.
pub trait Probe {
    /// `group.kill` under the identity check; whether it was sent.
    fn kill(&self, g: &Group) -> bool;
    fn gone(&self, g: &Group) -> bool;
}

pub struct Live;

impl Probe for Live {
    fn kill(&self, g: &Group) -> bool {
        proc::kill_recorded(g)
    }
    fn gone(&self, g: &Group) -> bool {
        proc::reap_if_child(g.pgid);
        proc::group_gone(g)
    }
}

/// Writes to stderr, ignoring failure: an owned run's stderr is a pipe to an
/// owner that may be gone, and `eprintln!` panics on EPIPE.
macro_rules! say {
    ($($t:tt)*) => {{
        let _ = writeln!(std::io::stderr(), $($t)*);
    }};
}

/// Kills each group that passes the identity check and waits up to `bound`
/// for every one to be gone. Whether all are.
fn end_groups(id: &str, groups: &[Group], probe: &dyn Probe, bound: Duration) -> bool {
    for g in groups {
        probe.kill(g);
    }
    let left = proc::wait_gone_with(groups, bound, |g| probe.gone(g));
    for g in &left {
        say!(
            "cos: run {id}: group {} still alive after {}s; no final state written",
            g.pgid,
            bound.as_secs_f32()
        );
    }
    left.is_empty()
}

/// Under the record lock, on the record as it now stands: `plan` decides
/// from it whether to settle and which groups beyond `groups` and the
/// record's own to end (`None` ends only `groups` and writes nothing). Every
/// group is killed under the identity check and waited on up to `bound`,
/// then `change` is applied if the record is not final. A group still alive
/// at the bound leaves the record as it is, for the next reap or sweep. An
/// unreadable record still has `groups` ended, then is an error. Whether it
/// wrote.
pub fn settle_and_write(
    home: &Path,
    id: &str,
    groups: &[Group],
    probe: &dyn Probe,
    bound: Duration,
    plan: impl FnOnce(&Record) -> Option<Vec<Group>>,
    change: impl FnOnce(&mut Record),
) -> Result<bool> {
    locked(home, id, |p| {
        let cur = match read_path(p).and_then(|r| r.with_context(|| format!("no run record {id}")))
        {
            Ok(r) => r,
            Err(e) => {
                end_groups(id, groups, probe, bound);
                return Err(e);
            }
        };
        let Some(extra) = plan(&cur) else {
            end_groups(id, groups, probe, bound);
            return Ok(false);
        };
        let mut all = groups.to_vec();
        for g in cur.groups.iter().chain(&extra) {
            if !all.contains(g) {
                all.push(g.clone());
            }
        }
        if !end_groups(id, &all, probe, bound) || cur.state.is_final() {
            return Ok(false);
        }
        let mut next = cur.clone();
        change(&mut next);
        Ok(matches!(apply(p, id, Some(cur), next)?, Write::Written(_)))
    })
}

/// Holds the brief lock until dropped or the process exits.
pub struct BriefLock(#[allow(dead_code)] File);

pub enum Taken {
    Held(BriefLock),
    /// The run id the holder wrote.
    Busy(String),
}

/// Takes `<home>/out/run-records/brief.lock` without blocking and writes
/// `id` into it.
pub fn take_brief_lock(home: &Path, id: &str) -> Result<Taken> {
    let dir = home.join(DIR);
    std::fs::create_dir_all(&dir).with_context(|| dir.display().to_string())?;
    let p = dir.join(LOCK);
    let mut f = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&p)
        .with_context(|| p.display().to_string())?;
    if let Some(holder) = contend(home, &f).context("take the brief lock")? {
        return Ok(Taken::Busy(holder));
    }
    f.rewind()?;
    f.write_all(id.as_bytes())?;
    f.set_len(id.len() as u64)?;
    Ok(Taken::Held(BriefLock(f)))
}

/// The run id holding `home`'s brief lock, or `None` when it is free. Takes
/// and drops the lock to find out, so it never holds it.
pub fn brief_held(home: &Path) -> Result<Option<String>> {
    let p = home.join(DIR).join(LOCK);
    let f = match File::open(&p) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e).with_context(|| p.display().to_string()),
    };
    contend(home, &f).context("probe the brief lock")
}

/// Takes the brief lock on `f` without blocking (`None`), or names the run
/// holding it. The file can name a run that has already ended or died: a
/// holder writes its id just after taking the lock, and a probe holds the
/// lock for an instant without writing. So for up to a second an empty,
/// partial, finished or dead id means try the lock again.
fn contend(home: &Path, f: &File) -> Result<Option<String>> {
    let p = home.join(DIR).join(LOCK);
    let until = std::time::Instant::now() + Duration::from_secs(1);
    loop {
        // SAFETY: flock on a descriptor the caller owns.
        if unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
            return Ok(None);
        }
        let e = std::io::Error::last_os_error();
        if e.raw_os_error() != Some(libc::EWOULDBLOCK) {
            return Err(e.into());
        }
        let holder = std::fs::read_to_string(&p).unwrap_or_default();
        let holder = holder.trim();
        let ended = || {
            read(home, holder)
                .is_ok_and(|r| r.is_some_and(|r| r.state.is_final() || process_gone(&r)))
        };
        if (is_id(holder) && !ended()) || std::time::Instant::now() >= until {
            return Ok(Some(holder.to_owned()));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// The start sweep: every record of this home that is not final and whose
/// process is gone has its groups killed and, once they are gone, is
/// written `lost`. Returns the ids written.
pub fn sweep(home: &Path, skip: &str) -> Vec<String> {
    sweep_with(home, skip, |r| {
        process_gone(r).then_some("previous run died")
    })
}

/// The sweep for every record of this home that is not final and for which
/// `why` names a reason, asked again under the record lock: the run's own
/// group and its groups are killed and, once they are gone, the record is
/// written `lost` with that reason. Returns the ids written.
pub fn sweep_with(
    home: &Path,
    skip: &str,
    why: impl Fn(&Record) -> Option<&'static str>,
) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(home.join(DIR)) else {
        return Vec::new();
    };
    let mut swept = Vec::new();
    for entry in entries.filter_map(Result::ok) {
        let name = entry.file_name();
        let Some(id) = name.to_str().and_then(|n| n.strip_suffix(".json")) else {
            continue;
        };
        if !is_id(id) || id == skip {
            continue;
        }
        let r = match read(home, id) {
            Ok(Some(r)) => r,
            Ok(None) => continue,
            Err(e) => {
                say!("cos: sweep skips {id}: {e:#}");
                continue;
            }
        };
        if r.state.is_final() || why(&r).is_none() {
            continue;
        }
        let reason = std::cell::Cell::new("");
        let plan = |r: &Record| {
            if r.state.is_final() {
                return None;
            }
            reason.set(why(r)?);
            Some(match (r.pid, r.start.clone()) {
                (Some(pid), Some(start)) if pid > 1 => vec![Group { pgid: pid, start }],
                _ => vec![],
            })
        };
        match settle_and_write(home, id, &[], &Live, GONE_BOUND, plan, |r| {
            r.state = State::Lost;
            r.reason = Some(reason.get().into());
            r.ended = Some(now());
        }) {
            Ok(true) => swept.push(id.to_owned()),
            Ok(false) => {}
            Err(e) => say!("cos: sweep of {id}: {e:#}"),
        }
    }
    swept
}

/// The owner's reap, after `wait` returned for the run child `pid`: kills
/// every group in the record and the run's own group, waits for them to be
/// gone, then writes `state` (or `failed` with the exit as reason) unless
/// the run wrote its own final state. The state the record ends in.
pub fn reap(
    home: &Path,
    id: &str,
    own: Option<Group>,
    status: ExitStatus,
    ending: Option<(State, &str)>,
) -> Result<State> {
    use std::os::unix::process::ExitStatusExt;

    let state = ending.map_or(State::Failed, |(s, _)| s);
    let own: Vec<Group> = own.into_iter().collect();
    settle_and_write(
        home,
        id,
        &own,
        &Live,
        GONE_BOUND,
        |_| Some(vec![]),
        |r| {
            r.reason = Some(match ending {
                Some((_, reason)) => reason.to_owned(),
                None => match (status.code(), status.signal()) {
                    (Some(EXIT_BRIEF_HELD), _) if r.kind == Kind::Brief => {
                        let named =
                            std::fs::read_to_string(home.join(DIR).join(LOCK)).unwrap_or_default();
                        let named = named.trim();
                        let holder = if is_id(named) { named } else { "unknown" };
                        format!("brief already running: {holder}")
                    }
                    (Some(c), _) => format!("exit {c}"),
                    (None, Some(s)) => format!("signal {s}"),
                    (None, None) => format!("{status}"),
                },
            });
            r.state = state;
            r.ended = Some(now());
        },
    )?;
    Ok(read(home, id)?.map_or(state, |r| r.state))
}

/// Writes the ending an owner wants onto the record before it signals the
/// run, so a run that ends itself on the signal writes that ending rather
/// than a plain `stopped`. Nothing is written once the record is final.
pub fn ask_to_end(home: &Path, id: &str, state: State, reason: &str) -> Result<()> {
    update(home, id, |cur| {
        let mut r = cur?.clone();
        r.stopping = Some(Ending {
            state,
            reason: reason.to_owned(),
        });
        Some(r)
    })?;
    Ok(())
}

/// `answer` capped at `ANSWER_MAX` bytes, cut at a char boundary, saying so.
pub fn cap_answer(answer: String) -> String {
    if answer.len() <= ANSWER_MAX {
        return answer;
    }
    let mut end = ANSWER_MAX - CUT_NOTE.len();
    while !answer.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}{CUT_NOTE}", &answer[..end])
}

/// Points stderr at `/dev/null`, so writes after the owner (the read end of
/// the pipe) is gone neither fail nor panic.
fn silence_stderr() {
    if let Ok(null) = OpenOptions::new().write(true).open("/dev/null") {
        // SAFETY: dup2 onto fd 2; `null` stays open until it returns.
        unsafe {
            libc::dup2(null.as_raw_fd(), libc::STDERR_FILENO);
        }
    }
}

/// Blocks SIGTERM, SIGINT and SIGHUP in the calling thread. Called first in
/// `main`, before any thread exists, so every thread inherits the mask and
/// only the run's `sigwait` sees them; `Command` resets the mask in children.
pub fn block_signals() -> libc::sigset_t {
    // SAFETY: plain signal-set manipulation on a local value.
    unsafe {
        let mut set: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut set);
        libc::sigaddset(&mut set, libc::SIGTERM);
        libc::sigaddset(&mut set, libc::SIGINT);
        libc::sigaddset(&mut set, libc::SIGHUP);
        libc::pthread_sigmask(libc::SIG_BLOCK, &set, std::ptr::null_mut());
        set
    }
}

/// The run process's record, its tracked groups, its owner watchdog and its
/// signal handling.
pub struct Session {
    home: PathBuf,
    id: String,
    ended: Mutex<bool>,
    _lock: Option<BriefLock>,
}

pub enum Begun {
    /// The brief lock is held by this run id; nothing was written.
    Busy(String),
    Running(&'static Session),
}

impl Session {
    /// Brief lock (for a brief), the start sweep, then `running`. From here
    /// every grouped child goes through the spawn gate into the record.
    /// `signals` comes from `block_signals`.
    pub fn begin(
        home: &Path,
        kind: Kind,
        business: &str,
        signals: libc::sigset_t,
    ) -> Result<Begun> {
        let id = match std::env::var(RUN_RECORD_ENV) {
            Ok(v) if !v.is_empty() => {
                ensure!(is_id(&v), "{RUN_RECORD_ENV} {v:?} is not a run id");
                v
            }
            _ => new_id(),
        };
        let owner = match std::env::var(OWNER_ENV) {
            Ok(v) if !v.is_empty() => Some(parse_owner(&v)?),
            _ => None,
        };
        let lock = match kind {
            Kind::Brief => match take_brief_lock(home, &id)? {
                Taken::Held(l) => Some(l),
                Taken::Busy(holder) => return Ok(Begun::Busy(holder)),
            },
            Kind::Ask => None,
        };
        let me = std::process::id() as libc::pid_t;
        let start = proc::start_of(me);
        ensure!(
            start.is_some(),
            "run records need process start times, which this platform does not give"
        );
        for swept in sweep(home, &id) {
            say!("cos: run {swept} had died; its processes are ended and it is marked lost");
        }
        let written = update(home, &id, |cur| {
            let mut r = match cur {
                Some(r) => r.clone(),
                None => {
                    let mut r = Record::new(&id, kind, business, Owner::Itself);
                    if let Some((pid, start)) = &owner {
                        r.owner_pid = Some(*pid);
                        r.owner_start = Some(start.clone());
                    }
                    r
                }
            };
            r.state = State::Running;
            r.pid = Some(me);
            r.start = start.clone();
            r.groups = Vec::new();
            Some(r)
        })?;
        if let Write::Unchanged(r) = written {
            bail!(
                "run {id} cannot start: its record is {:?}",
                r.map(|r| r.state)
            );
        }
        let session: &'static Session = Box::leak(Box::new(Session {
            home: home.to_owned(),
            id: id.clone(),
            ended: Mutex::new(false),
            _lock: lock,
        }));
        let hold = std::env::var("COS_GATE_HOLD_MS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok());
        proc::track(
            Box::new(move |groups| {
                // Test seam: holds the newest gated child before its group is
                // recorded, naming it in `<home>/gate.pid`.
                if let (Some(ms), Some(g)) = (hold, groups.last()) {
                    std::fs::write(session.home.join("gate.pid"), g.pgid.to_string())?;
                    std::thread::sleep(Duration::from_millis(ms));
                }
                match update(&session.home, &session.id, |cur| {
                    let mut r = cur?.clone();
                    r.groups = groups.to_vec();
                    Some(r)
                })? {
                    Write::Written(_) => Ok(()),
                    Write::Unchanged(r) => bail!(
                        "run {} is {:?}: no new process starts",
                        session.id,
                        r.map(|r| r.state)
                    ),
                }
            }),
            &[RUN_RECORD_ENV, OWNER_ENV],
        );
        std::thread::spawn(move || session.wait_signals(signals));
        if let Some((pid, start)) = owner {
            std::thread::spawn(move || session.watch_owner(pid, &start));
        }
        Ok(Begun::Running(session))
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    /// Self-cleanup, then the final state. Whether it was written.
    pub fn finish(
        &self,
        state: State,
        reason: Option<String>,
        change: impl FnOnce(&mut Record),
    ) -> bool {
        let mut ended = self.ended.lock().unwrap_or_else(|e| e.into_inner());
        let wrote = self.settle(&mut ended, state, reason, change);
        proc::untrack();
        wrote
    }

    /// Ends every tracked group and writes the final state. Tracking stays
    /// closed, so a spawn racing the end is refused rather than started
    /// outside any record.
    fn settle(
        &self,
        ended: &mut bool,
        state: State,
        reason: Option<String>,
        change: impl FnOnce(&mut Record),
    ) -> bool {
        if *ended {
            return false;
        }
        *ended = true;
        let groups = proc::close_tracking();
        settle_and_write(
            &self.home,
            &self.id,
            &groups,
            &Live,
            GONE_BOUND,
            |_| Some(vec![]),
            |r| {
                r.state = state;
                r.reason = reason;
                r.ended = Some(now());
                change(r);
            },
        )
        .unwrap_or_else(|e| {
            say!("cos: run {}: final state not written: {e:#}", self.id);
            false
        })
    }

    /// Self-cleanup with `state`, then exit `code`, holding the end so no
    /// other path writes or exits meanwhile.
    fn end(&self, state: State, reason: &str, code: i32) -> ! {
        let mut ended = self.ended.lock().unwrap_or_else(|e| e.into_inner());
        self.settle(&mut ended, state, Some(reason.to_owned()), |_| {});
        std::process::exit(code);
    }

    fn wait_signals(&self, set: libc::sigset_t) {
        loop {
            let mut sig = 0;
            // SAFETY: `set` is an initialised signal set blocked in every thread.
            if unsafe { libc::sigwait(&set, &mut sig) } == 0 {
                let asked = read(&self.home, &self.id)
                    .ok()
                    .flatten()
                    .and_then(|r| r.stopping);
                match asked {
                    Some(e) => self.end(e.state, &e.reason, EXIT_SIGNALLED),
                    None => self.end(State::Stopped, "stopped", EXIT_SIGNALLED),
                }
            }
        }
    }

    fn watch_owner(&self, pid: libc::pid_t, start: &str) {
        loop {
            if proc::start_of(pid).as_deref() != Some(start) {
                silence_stderr();
                self.end(State::Lost, "owner gone", EXIT_OWNER_GONE);
            }
            std::thread::sleep(Duration::from_secs(1));
        }
    }
}

#[cfg(test)]
mod tests;
