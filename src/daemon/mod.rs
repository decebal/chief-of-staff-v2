//! `cos daemon`: owns the paid runs (`brief`, `ask`) that MCP clients and the
//! app start, for every registered business, on a unix socket only the user
//! can open, and follows every business's hires. A request is one JSON line
//! and gets one JSON line back, except `subscribe`, which streams.

use std::collections::{BTreeMap, HashMap};
use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, mpsc};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use anyhow::{Context, Result, bail, ensure};
use serde_json::{Map, Value, json};

use crate::business::{self, Business};
use crate::pipeline::{self, RunChild, RunSpec};
use crate::proc::{self, GONE_BOUND, Group};
use crate::runs::{self, Kind, Owner, State};

pub mod client;
pub mod install;

/// Longest request line, its newline excluded.
pub const LINE_MAX: usize = 64 * 1024;
/// A connection with no complete line within this is closed.
pub const IDLE: Duration = Duration::from_secs(5);
pub const CONNECTIONS: usize = 16;
pub const ASKS_PER_BUSINESS: usize = 2;
pub const RUNS_TOTAL: usize = 4;
pub const QUESTION_MAX: usize = 4000;
/// `sun_path` on macOS, its terminating NUL included.
const SUN_PATH: usize = 104;
const SOCKET: &str = "daemon.sock";
const LOCK: &str = "daemon.lock";
const GROUPS: &str = "daemon.groups.json";
pub const LOG: &str = "daemon.log";
const TICK: Duration = Duration::from_secs(1);
const FOLLOW_EVERY: Duration = Duration::from_secs(2);
/// A `stop` waits out the run's 5 s grace and the reap's bound.
const STOP_WAIT: Duration = Duration::from_secs(20);

/// `COS_DAEMON_DIR` when set, else `cos` under the user temp dir.
pub fn dir() -> Result<PathBuf> {
    dir_from(std::env::var_os("COS_DAEMON_DIR"))
}

pub fn dir_from(over: Option<OsString>) -> Result<PathBuf> {
    if let Some(d) = over.filter(|v| !v.is_empty()) {
        return Ok(PathBuf::from(d));
    }
    Ok(user_temp_dir()?.join("cos"))
}

/// `confstr(_CS_DARWIN_USER_TEMP_DIR)`: the same for every process of the
/// user, whatever its environment.
#[cfg(target_os = "macos")]
fn user_temp_dir() -> Result<PathBuf> {
    use std::os::unix::ffi::OsStringExt;
    let mut buf = vec![0u8; libc::PATH_MAX as usize];
    // SAFETY: confstr writes at most `buf.len()` bytes into the buffer.
    let n = unsafe {
        libc::confstr(
            libc::_CS_DARWIN_USER_TEMP_DIR,
            buf.as_mut_ptr().cast(),
            buf.len(),
        )
    };
    ensure!(
        n > 1 && n <= buf.len(),
        "confstr(_CS_DARWIN_USER_TEMP_DIR) gave no directory"
    );
    buf.truncate(n - 1);
    Ok(PathBuf::from(OsString::from_vec(buf)))
}

/// The shared temp dir here is writable by every user, and the daemon admits
/// peers only on macOS, so there is no default.
#[cfg(not(target_os = "macos"))]
fn user_temp_dir() -> Result<PathBuf> {
    bail!("the cos daemon runs on macOS only; set COS_DAEMON_DIR to a directory only you can write")
}

pub fn socket_path(dir: &Path) -> PathBuf {
    dir.join(SOCKET)
}

/// Refuses a socket path `bind` could not hold, naming its length.
pub fn check_socket_path(p: &Path) -> Result<()> {
    let n = p.as_os_str().len();
    ensure!(
        n < SUN_PATH,
        "socket path {} is {n} bytes; a unix socket path holds at most {} bytes",
        p.display(),
        SUN_PATH - 1
    );
    Ok(())
}

/// One request line to the daemon on `sock` and its one-line reply, within
/// `wait`.
pub fn call(sock: &Path, req: &Value, wait: Duration) -> Result<Value> {
    let s = UnixStream::connect(sock).with_context(|| format!("connect {}", sock.display()))?;
    exchange(s, req, wait)
}

/// One request line on a connected socket and its one-line reply, within `wait`.
fn exchange(mut s: UnixStream, req: &Value, wait: Duration) -> Result<Value> {
    s.set_write_timeout(Some(IDLE))?;
    s.set_read_timeout(Some(wait))?;
    s.write_all(format!("{req}\n").as_bytes())?;
    let mut reply = Vec::new();
    std::io::BufRead::read_until(
        &mut std::io::BufReader::new(s.take(LINE_MAX as u64 + 1)),
        b'\n',
        &mut reply,
    )
    .context("read the daemon's reply")?;
    serde_json::from_slice(&reply).context("the daemon's reply")
}

#[derive(Debug)]
pub(crate) enum Request {
    Hello,
    Start {
        kind: Kind,
        business: String,
        client_req: String,
        question: Option<String>,
    },
    Stop {
        run: String,
        business: String,
    },
    Subscribe,
    Drain,
    Undrain,
}

/// One request line, against the `run.start` / `run.stop` bindings: closed
/// objects, every field checked.
pub(crate) fn parse(line: &str) -> std::result::Result<Request, String> {
    let v: Value = serde_json::from_str(line).map_err(|e| format!("not a JSON line: {e}"))?;
    let Value::Object(m) = v else {
        return Err("a request is a JSON object".into());
    };
    let req = match text(&m, "op")? {
        "hello" => {
            only(&m, &["op"])?;
            Request::Hello
        }
        "subscribe" => {
            only(&m, &["op"])?;
            Request::Subscribe
        }
        "drain" => {
            only(&m, &["op"])?;
            Request::Drain
        }
        "undrain" => {
            only(&m, &["op"])?;
            Request::Undrain
        }
        "stop" => {
            only(&m, &["op", "run", "business"])?;
            let run = text(&m, "run")?;
            if !runs::is_id(run) {
                return Err(format!("run {run:?} is not a run id"));
            }
            Request::Stop {
                run: run.to_owned(),
                business: business_field(&m)?,
            }
        }
        "start" => {
            let kind = match text(&m, "kind")? {
                "brief" => Kind::Brief,
                "ask" => Kind::Ask,
                k => return Err(format!("kind {k:?}: want brief or ask")),
            };
            let question = match kind {
                Kind::Brief => {
                    only(&m, &["op", "kind", "business", "client_req"])?;
                    None
                }
                Kind::Ask => {
                    only(&m, &["op", "kind", "business", "client_req", "question"])?;
                    let q = text(&m, "question")?;
                    let n = q.chars().count();
                    if !(1..=QUESTION_MAX).contains(&n) {
                        return Err(format!("question is {n} chars: want 1 to {QUESTION_MAX}"));
                    }
                    Some(q.to_owned())
                }
            };
            let client_req = text(&m, "client_req")?;
            let hex = |b: u8| b.is_ascii_digit() || (b'a'..=b'f').contains(&b);
            if client_req.len() != 16 || !client_req.bytes().all(hex) {
                return Err(format!(
                    "client_req {client_req:?}: want 16 lowercase hex digits"
                ));
            }
            Request::Start {
                kind,
                business: business_field(&m)?,
                client_req: client_req.to_owned(),
                question,
            }
        }
        op => return Err(format!("unknown op {op:?}")),
    };
    Ok(req)
}

fn only(m: &Map<String, Value>, allowed: &[&str]) -> std::result::Result<(), String> {
    match m.keys().find(|k| !allowed.contains(&k.as_str())) {
        Some(k) => Err(format!("unknown field {k:?}")),
        None => Ok(()),
    }
}

fn text<'a>(m: &'a Map<String, Value>, key: &str) -> std::result::Result<&'a str, String> {
    m.get(key)
        .ok_or_else(|| format!("missing field {key:?}"))?
        .as_str()
        .ok_or_else(|| format!("field {key:?} is not a string"))
}

fn business_field(m: &Map<String, Value>) -> std::result::Result<String, String> {
    let b = text(m, "business")?;
    if !runs::is_business(b) {
        return Err(format!("business {b:?} is not a business name"));
    }
    Ok(b.to_owned())
}

/// The refusal for a new `kind` run of `business` when the live runs, as
/// `(kind, business)`, already hold `RUNS_TOTAL` runs or `ASKS_PER_BUSINESS`
/// asks of that business; the daemon and a `cos mcp` that owns its runs give
/// the same one.
pub fn over_cap<'a>(
    kind: Kind,
    business: &str,
    live: impl Iterator<Item = (Kind, &'a str)>,
) -> Option<String> {
    let (mut total, mut asks) = (0, 0);
    for (k, b) in live {
        total += 1;
        if k == Kind::Ask && b == business {
            asks += 1;
        }
    }
    if total >= RUNS_TOTAL {
        return Some(format!("{RUNS_TOTAL} runs are live, the most run at once"));
    }
    (kind == Kind::Ask && asks >= ASKS_PER_BUSINESS)
        .then(|| format!("{ASKS_PER_BUSINESS} asks are live for {business}, the most per business"))
}

pub struct Options {
    pub registry: PathBuf,
    pub dir: PathBuf,
    /// The program a run child runs: `cos` itself unless a test names one.
    pub run_bin: PathBuf,
    pub deadline: Duration,
    /// The only peer uid admitted.
    pub uid: libc::uid_t,
}

impl Options {
    /// The registry (`COS_BUSINESSES`, …) and the socket directory
    /// (`COS_DAEMON_DIR`), plus three test seams: the run program
    /// (`COS_RUN_BIN`), a run deadline below `pipeline::BRIEF_CHILD`
    /// (`COS_DAEMON_DEADLINE_SECS`, never above it) and the admitted uid
    /// (`COS_DAEMON_UID`; the 0700 directory still admits only this user).
    pub fn from_env() -> Result<Self> {
        let run_bin = match std::env::var_os("COS_RUN_BIN").filter(|v| !v.is_empty()) {
            Some(p) => PathBuf::from(p),
            None => std::env::current_exe().context("locate the cos binary")?,
        };
        let deadline = match std::env::var("COS_DAEMON_DEADLINE_SECS") {
            Ok(v) => Duration::from_secs(
                v.parse()
                    .with_context(|| format!("COS_DAEMON_DEADLINE_SECS {v:?}"))?,
            )
            .min(pipeline::BRIEF_CHILD),
            Err(_) => pipeline::BRIEF_CHILD,
        };
        let uid = match std::env::var("COS_DAEMON_UID") {
            Ok(v) => v.parse().with_context(|| format!("COS_DAEMON_UID {v:?}"))?,
            // SAFETY: geteuid has no preconditions.
            Err(_) => unsafe { libc::geteuid() },
        };
        Ok(Options {
            registry: business::location()?,
            dir: dir()?,
            run_bin,
            deadline,
            uid,
        })
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

struct Stop {
    reason: &'static str,
    reply: mpsc::Sender<State>,
}

struct Live {
    business: String,
    kind: Kind,
    home: PathBuf,
    stop: mpsc::Sender<Stop>,
}

struct Daemon {
    opts: Options,
    pid: libc::pid_t,
    /// Live runs by id; each is followed by its own thread, which owns its
    /// `RunChild` and removes it here once reaped.
    runs: Mutex<BTreeMap<String, Live>>,
    /// The first reply to each `client_req`, while this daemon lives.
    replies: Mutex<HashMap<String, Value>>,
    /// One `start` at a time per business.
    gates: Mutex<HashMap<String, Arc<Mutex<()>>>>,
    stopping: AtomicBool,
    /// Set by `drain`, cleared by `undrain`: while set every `start` is
    /// refused and an executable change does not retire the daemon.
    draining: AtomicBool,
    conns: AtomicUsize,
}

/// Runs the daemon in the foreground. Returns the exit code: 0 on SIGTERM,
/// SIGINT or SIGHUP (its runs stopped first) and when another daemon holds
/// the lock; 1 when its executable changed and no run is live, so launchd
/// starts the new binary. `signals` comes from `runs::block_signals`.
pub fn serve(opts: Options, signals: libc::sigset_t) -> Result<i32> {
    let sock = socket_path(&opts.dir);
    check_socket_path(&sock)?;
    std::fs::create_dir_all(&opts.dir).with_context(|| opts.dir.display().to_string())?;
    std::fs::set_permissions(&opts.dir, std::fs::Permissions::from_mode(0o700))
        .with_context(|| format!("chmod 0700 {}", opts.dir.display()))?;
    log_to(&opts.dir.join(LOG))?;
    let Some(_held) = take_lock(&opts.dir.join(LOCK))? else {
        println!("cos daemon: already running");
        return Ok(0);
    };
    let pid = std::process::id() as libc::pid_t;
    let start = proc::start_of(pid)
        .context("the daemon needs process start times, which this platform does not give")?;

    sweep_runs(&opts.registry, pid, &start);
    let mirror = opts.dir.join(GROUPS);
    sweep_groups(&mirror);
    proc::track(Box::new(move |groups| write_groups(&mirror, groups)), &[]);

    match std::fs::remove_file(&sock) {
        Ok(()) => eprintln!("cos daemon: removed a stale socket"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e).with_context(|| sock.display().to_string()),
    }
    let listener = UnixListener::bind(&sock).with_context(|| sock.display().to_string())?;
    std::fs::set_permissions(&sock, std::fs::Permissions::from_mode(0o600))
        .with_context(|| format!("chmod 0600 {}", sock.display()))?;
    let exe = exe_stamp();
    eprintln!(
        "cos daemon {}: pid {pid}, socket {}",
        env!("CARGO_PKG_VERSION"),
        sock.display()
    );

    let d = Arc::new(Daemon {
        opts,
        pid,
        runs: Mutex::new(BTreeMap::new()),
        replies: Mutex::new(HashMap::new()),
        gates: Mutex::new(HashMap::new()),
        stopping: AtomicBool::new(false),
        draining: AtomicBool::new(false),
        conns: AtomicUsize::new(0),
    });
    {
        let d = d.clone();
        thread::spawn(move || d.accept(listener));
    }
    {
        let d = d.clone();
        thread::spawn(move || d.follow());
    }
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        loop {
            let mut sig = 0;
            // SAFETY: `signals` is an initialised set blocked in every thread.
            if unsafe { libc::sigwait(&signals, &mut sig) } == 0 && tx.send(sig).is_err() {
                return;
            }
        }
    });
    loop {
        match rx.recv_timeout(TICK) {
            Ok(sig) => {
                eprintln!("cos daemon: signal {sig}: stopping its runs");
                d.shutdown(&sock);
                return Ok(0);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => bail!("the signal thread ended"),
        }
        if exe_stamp() != exe && d.retire_if_idle() {
            eprintln!(
                "cos daemon: its executable changed; exiting for launchd to start the new one"
            );
            let _ = std::fs::remove_file(&sock);
            d.end_own_groups();
            return Ok(1);
        }
    }
}

/// Points this process's stderr at `file`, appended to and created `0600`, so
/// the daemon's log is in its directory however launchd or a shell started it.
fn log_to(file: &Path) -> Result<()> {
    let f = OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(file)
        .with_context(|| file.display().to_string())?;
    std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o600))
        .with_context(|| format!("chmod 0600 {}", file.display()))?;
    // SAFETY: dup2 onto stderr from a descriptor this function owns; the
    // duplicate outlives `f`.
    if unsafe { libc::dup2(f.as_raw_fd(), libc::STDERR_FILENO) } < 0 {
        return Err(std::io::Error::last_os_error())
            .with_context(|| format!("point stderr at {}", file.display()));
    }
    Ok(())
}

/// The exclusive `flock` on `daemon.lock`, retried once after 100 ms; `None`
/// when another daemon holds it.
fn take_lock(p: &Path) -> Result<Option<File>> {
    let f = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .mode(0o600)
        .open(p)
        .with_context(|| p.display().to_string())?;
    for attempt in 0..2 {
        // SAFETY: flock on a descriptor this function owns.
        if unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
            return Ok(Some(f));
        }
        let e = std::io::Error::last_os_error();
        if e.raw_os_error() != Some(libc::EWOULDBLOCK) {
            return Err(e).context("lock the daemon");
        }
        if attempt == 0 {
            thread::sleep(Duration::from_millis(100));
        }
    }
    Ok(None)
}

/// Every registered business's records that are not final: a daemon run
/// another daemon owned, and any run whose process is gone, are ended and
/// written `lost`.
fn sweep_runs(registry: &Path, pid: libc::pid_t, start: &str) {
    for (name, home) in business::registered_homes(registry) {
        let swept = runs::sweep_with(&home, "", |r| {
            if r.owner == Owner::Daemon
                && (r.owner_pid != Some(pid) || r.owner_start.as_deref() != Some(start))
            {
                Some("daemon restarted")
            } else if runs::process_gone(r) {
                Some("previous run died")
            } else {
                None
            }
        });
        for id in swept {
            eprintln!("cos daemon: {name}: run {id} had lost its owner; ended and marked lost");
        }
    }
}

/// The groups a previous daemon mirrored (the judge's checks and `git`, the
/// follower's `hc` calls), killed under the identity check.
fn sweep_groups(file: &Path) {
    let groups: Vec<Group> = match std::fs::read_to_string(file) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
            eprintln!("cos daemon: {} unreadable: {e}", file.display());
            Vec::new()
        }),
        Err(_) => Vec::new(),
    };
    for g in &groups {
        if proc::kill_recorded(g) {
            eprintln!("cos daemon: ended group {} a previous daemon left", g.pgid);
        }
    }
    let left = proc::wait_gone(&groups, GONE_BOUND);
    for g in &left {
        eprintln!(
            "cos daemon: group {} still alive after {}s",
            g.pgid,
            GONE_BOUND.as_secs()
        );
    }
    if let Err(e) = write_groups(file, &left) {
        eprintln!("cos daemon: {e:#}");
    }
}

fn write_groups(file: &Path, groups: &[Group]) -> Result<()> {
    let tmp = file.with_extension("json.tmp");
    let mut f = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(&tmp)
        .with_context(|| tmp.display().to_string())?;
    f.write_all(&serde_json::to_vec(groups)?)?;
    std::fs::rename(&tmp, file).with_context(|| file.display().to_string())
}

fn exe_stamp() -> Option<(u64, SystemTime)> {
    let m = std::fs::metadata(std::env::current_exe().ok()?).ok()?;
    Some((m.len(), m.modified().ok()?))
}

#[cfg(target_os = "macos")]
fn peer_uid(s: &UnixStream) -> std::io::Result<libc::uid_t> {
    let (mut uid, mut gid) = (0, 0);
    // SAFETY: getpeereid writes two integers this function owns.
    if unsafe { libc::getpeereid(s.as_raw_fd(), &mut uid, &mut gid) } == 0 {
        Ok(uid)
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(not(target_os = "macos"))]
fn peer_uid(_: &UnixStream) -> std::io::Result<libc::uid_t> {
    Err(std::io::Error::other(
        "peer credentials are read on macOS only",
    ))
}

enum Line {
    Text(String),
    TooLong,
    Closed,
}

/// The first line within `IDLE`; bytes after it are ignored.
fn read_line(s: &mut UnixStream) -> Line {
    let until = Instant::now() + IDLE;
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let left = until.saturating_duration_since(Instant::now());
        if left.is_zero() || s.set_read_timeout(Some(left)).is_err() {
            return Line::Closed;
        }
        match s.read(&mut chunk) {
            Ok(0) => return Line::Closed,
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                if let Some(end) = buf.iter().position(|b| *b == b'\n') {
                    if end > LINE_MAX {
                        return Line::TooLong;
                    }
                    buf.truncate(end);
                    if buf.last() == Some(&b'\r') {
                        buf.pop();
                    }
                    return Line::Text(String::from_utf8_lossy(&buf).into_owned());
                }
                if buf.len() > LINE_MAX {
                    return Line::TooLong;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => return Line::Closed,
        }
    }
}

/// The records under `home`'s `out/run-records/`.
fn records(home: &Path) -> Vec<runs::Record> {
    let Ok(entries) = std::fs::read_dir(home.join(runs::DIR)) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|e| {
            let name = e.file_name();
            let id = name.to_str()?.strip_suffix(".json")?.to_owned();
            runs::is_id(&id).then_some(id)
        })
        .filter_map(|id| runs::read(home, &id).ok().flatten())
        .collect()
}

/// Releases a connection slot however its thread ends.
struct Slot<'a>(&'a AtomicUsize);

impl Drop for Slot<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

impl Daemon {
    fn accept(self: Arc<Self>, listener: UnixListener) {
        if let Err(e) = listener.set_nonblocking(true) {
            eprintln!("cos daemon: listener: {e}");
        }
        while !self.stopping.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((stream, _)) => {
                    if self.conns.fetch_add(1, Ordering::SeqCst) >= CONNECTIONS {
                        self.conns.fetch_sub(1, Ordering::SeqCst);
                        drop(stream);
                        continue;
                    }
                    let d = self.clone();
                    thread::spawn(move || {
                        let _slot = Slot(&d.conns);
                        // An accepted socket inherits the listener's O_NONBLOCK on macOS.
                        if stream.set_nonblocking(false).is_ok() {
                            d.connection(stream);
                        }
                    });
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(20));
                }
                Err(e) => {
                    eprintln!("cos daemon: accept: {e}");
                    thread::sleep(Duration::from_millis(100));
                }
            }
        }
    }

    fn connection(self: &Arc<Self>, mut stream: UnixStream) {
        match peer_uid(&stream) {
            Ok(uid) if uid == self.opts.uid => {}
            Ok(uid) => {
                eprintln!(
                    "cos daemon: refused a connection from uid {uid}: only uid {} is admitted",
                    self.opts.uid
                );
                return;
            }
            Err(e) => {
                eprintln!("cos daemon: refused a connection: its peer: {e}");
                return;
            }
        }
        let reply = match read_line(&mut stream) {
            Line::Closed => return,
            Line::TooLong => json!({"error": format!("request over {LINE_MAX} bytes")}),
            Line::Text(line) => match parse(&line) {
                Err(e) => json!({ "error": e }),
                Ok(Request::Subscribe) => {
                    self.subscribe(stream);
                    return;
                }
                Ok(req) => self
                    .answer(req)
                    .unwrap_or_else(|e| json!({"error": format!("{e:#}")})),
            },
        };
        let _ = stream.set_write_timeout(Some(IDLE));
        let _ = stream.write_all(format!("{reply}\n").as_bytes());
    }

    fn answer(self: &Arc<Self>, req: Request) -> Result<Value> {
        match req {
            Request::Hello => Ok(json!({"version": env!("CARGO_PKG_VERSION"), "pid": self.pid})),
            Request::Start {
                kind,
                business,
                client_req,
                question,
            } => self.start(kind, &business, &client_req, question.as_deref()),
            Request::Stop { run, business } => self.stop(&run, &business),
            Request::Drain => Ok(self.drain()),
            Request::Undrain => {
                self.draining.store(false, Ordering::SeqCst);
                eprintln!("cos daemon: undrained; taking runs again");
                Ok(json!({"drained": false}))
            }
            Request::Subscribe => bail!("subscribe streams"),
        }
    }

    /// The registered business `name`, its directories checked.
    fn registered(&self, name: &str) -> Result<Business> {
        let reg = business::load(&self.opts.registry)?;
        match reg.businesses.get(name) {
            Some(b) => {
                business::check_dirs(name, b)?;
                Ok(b.clone())
            }
            None => {
                let known: Vec<&str> = reg.businesses.keys().map(String::as_str).collect();
                bail!(
                    "business {name:?} is not registered; registered: {}",
                    if known.is_empty() {
                        "none".to_owned()
                    } else {
                        known.join(", ")
                    }
                )
            }
        }
    }

    fn start(
        self: &Arc<Self>,
        kind: Kind,
        name: &str,
        client_req: &str,
        question: Option<&str>,
    ) -> Result<Value> {
        let b = self.registered(name)?;
        let gate = lock(&self.gates)
            .entry(name.to_owned())
            .or_default()
            .clone();
        let _one = lock(&gate);
        if let Some(first) = lock(&self.replies).get(client_req) {
            return Ok(first.clone());
        }
        let reply = self.start_new(kind, name, &b, client_req, question)?;
        lock(&self.replies).insert(client_req.to_owned(), reply.clone());
        Ok(reply)
    }

    fn start_new(
        self: &Arc<Self>,
        kind: Kind,
        name: &str,
        b: &Business,
        client_req: &str,
        question: Option<&str>,
    ) -> Result<Value> {
        if kind == Kind::Brief {
            let mine = lock(&self.runs)
                .iter()
                .find(|(_, l)| l.kind == Kind::Brief && l.business == name)
                .map(|(id, _)| id.clone());
            let holder = match mine {
                Some(id) => Some(id),
                None => runs::brief_held(&b.home)?,
            };
            if let Some(id) = holder {
                ensure!(
                    runs::is_id(&id),
                    "a brief is running for {name}; its run id is not readable yet"
                );
                return Ok(json!({"run": id, "existing": true}));
            }
        }
        let mut live = lock(&self.runs);
        ensure!(
            !self.stopping.load(Ordering::SeqCst),
            "the daemon is stopping"
        );
        ensure!(
            !self.draining.load(Ordering::SeqCst),
            "the daemon is draining for a reinstall; start the run again once it is back"
        );
        if let Some(refused) = over_cap(
            kind,
            name,
            live.values().map(|l| (l.kind, l.business.as_str())),
        ) {
            bail!(refused);
        }
        let child = pipeline::spawn_run(
            &self.opts.run_bin,
            &RunSpec {
                kind,
                business: name,
                home: &b.home,
                workspace: &b.workspace,
                projects: &b.projects,
                question,
                owner: Owner::Daemon,
                env: Vec::new(),
                client_req: Some(client_req),
                budget: None,
            },
        )?;
        let id = child.id().to_owned();
        let (tx, rx) = mpsc::channel();
        live.insert(
            id.clone(),
            Live {
                business: name.to_owned(),
                kind,
                home: b.home.clone(),
                stop: tx,
            },
        );
        drop(live);
        eprintln!("cos daemon: {name}: run {id} started");
        let d = self.clone();
        let run = id.clone();
        thread::spawn(move || d.follow_run(run, child, rx));
        Ok(json!({"run": id, "existing": false}))
    }

    /// Owns one run child until it is reaped: its exit, a `stop`, or its
    /// deadline. Every `stop` sent meanwhile gets the final state.
    fn follow_run(&self, id: String, mut child: RunChild, rx: mpsc::Receiver<Stop>) {
        let until = proc::Deadline::capped(self.opts.deadline, self.opts.deadline);
        let mut waiting = Vec::new();
        let ended = loop {
            match rx.recv_timeout(Duration::from_millis(50)) {
                Ok(stop) => {
                    waiting.push(stop.reply);
                    break child.stop_as(State::Stopped, stop.reason);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break child.kill(),
            }
            match child.try_end() {
                Ok(Some(e)) => break Ok(e),
                Ok(None) => {}
                Err(e) => break Err(e),
            }
            if until.expired() {
                break child.stop_as(State::Failed, "deadline");
            }
        };
        let home = lock(&self.runs).get(&id).map(|l| l.home.clone());
        let state = match ended {
            Ok(e) => e.state,
            Err(e) => {
                eprintln!("cos daemon: run {id}: {e:#}");
                home.and_then(|h| runs::read(&h, &id).ok().flatten())
                    .map_or(State::Failed, |r| r.state)
            }
        };
        lock(&self.runs).remove(&id);
        while let Ok(stop) = rx.try_recv() {
            waiting.push(stop.reply);
        }
        for w in waiting {
            let _ = w.send(state);
        }
        eprintln!(
            "cos daemon: run {id} ended {}",
            serde_json::to_value(state).unwrap_or_default()
        );
    }

    fn stop(&self, run: &str, name: &str) -> Result<Value> {
        let b = self.registered(name)?;
        let read = || -> Result<runs::Record> {
            runs::read(&b.home, run)?.with_context(|| format!("no run {run} under business {name}"))
        };
        let r = read()?;
        ensure!(
            r.owner == Owner::Daemon,
            "run {run} was started by {}, not the daemon: stop it where it was started",
            serde_json::to_value(r.owner)?
                .as_str()
                .unwrap_or("another owner")
        );
        let tx = lock(&self.runs)
            .get(run)
            .filter(|l| l.business == name)
            .map(|l| l.stop.clone());
        let state = match tx {
            Some(tx) => {
                let (reply, rx) = mpsc::channel();
                let sent = tx.send(Stop {
                    reason: "stopped",
                    reply,
                });
                match (sent, rx.recv_timeout(STOP_WAIT)) {
                    (Ok(()), Ok(state)) => state,
                    (_, Err(mpsc::RecvTimeoutError::Timeout)) => {
                        bail!("run {run} did not end within {}s", STOP_WAIT.as_secs())
                    }
                    _ => read()?.state,
                }
            }
            None => {
                let r = read()?;
                ensure!(r.state.is_final(), "run {run} is not live in this daemon");
                r.state
            }
        };
        ensure!(
            state.is_final(),
            "run {run} has not ended: a process of its is still alive; the next sweep ends it"
        );
        Ok(json!({ "state": state }))
    }

    /// `{drained: true, pid}` once no run is live, from then on refusing every
    /// `start`; while a run is live, an error naming each as `live:
    /// [{run, business}]`, and nothing changes. Taken under the runs lock, so
    /// no `start` lands between the count and the flag.
    fn drain(&self) -> Value {
        let live = lock(&self.runs);
        if live.is_empty() {
            self.draining.store(true, Ordering::SeqCst);
            eprintln!("cos daemon: drained; refusing new runs");
            return json!({"drained": true, "pid": self.pid});
        }
        let named: Vec<String> = live
            .iter()
            .map(|(id, l)| format!("{id} ({})", l.business))
            .collect();
        let rows: Vec<Value> = live
            .iter()
            .map(|(id, l)| json!({"run": id, "business": l.business}))
            .collect();
        json!({
            "error": format!("runs are live: {}", named.join(", ")),
            "live": rows,
        })
    }

    /// Streams `{run, business, kind, state}` for every record that changes
    /// under any registered home, polling each `out/run-records/` every
    /// second, until the client closes or the daemon stops. The first line is
    /// `{"subscribed": true}`, once the baseline is read: a change after it is
    /// streamed.
    fn subscribe(&self, mut stream: UnixStream) {
        let _ = stream.set_read_timeout(None);
        let _ = stream.set_write_timeout(Some(IDLE));
        let closed = Arc::new(AtomicBool::new(false));
        if let Ok(mut r) = stream.try_clone() {
            let closed = closed.clone();
            thread::spawn(move || {
                let mut buf = [0u8; 256];
                while matches!(r.read(&mut buf), Ok(n) if n > 0) {}
                closed.store(true, Ordering::SeqCst);
            });
        }
        let mut stamps: HashMap<PathBuf, String> = HashMap::new();
        let mut known: HashMap<String, State> = HashMap::new();
        let mut first = true;
        while !closed.load(Ordering::SeqCst) && !self.stopping.load(Ordering::SeqCst) {
            for home in business::registered_homes(&self.opts.registry).into_values() {
                let stamp = crate::dash::stamp([home.join(runs::DIR)]);
                if stamps.get(&home) == Some(&stamp) {
                    continue;
                }
                stamps.insert(home.clone(), stamp);
                for r in records(&home) {
                    if known.get(&r.id) == Some(&r.state) {
                        continue;
                    }
                    known.insert(r.id.clone(), r.state);
                    if first {
                        continue;
                    }
                    let line = json!({"run": r.id, "business": r.business, "kind": r.kind, "state": r.state});
                    if stream.write_all(format!("{line}\n").as_bytes()).is_err() {
                        let _ = stream.shutdown(std::net::Shutdown::Both);
                        return;
                    }
                }
            }
            if first {
                let ack = json!({"subscribed": true});
                if stream.write_all(format!("{ack}\n").as_bytes()).is_err() {
                    let _ = stream.shutdown(std::net::Shutdown::Both);
                    return;
                }
            }
            first = false;
            thread::sleep(TICK);
        }
        let _ = stream.shutdown(std::net::Shutdown::Both);
    }

    fn follow(&self) {
        let mut follow = crate::agents::Follow::default();
        while !self.stopping.load(Ordering::SeqCst) {
            follow.follow_all(&self.opts.registry, None);
            thread::sleep(FOLLOW_EVERY);
        }
    }

    /// Stops taking runs when none is live and it is not drained; whether it
    /// did. A drained daemon waits for the `bootout` of the install that
    /// drained it: launchd would restart a retired one undrained.
    fn retire_if_idle(&self) -> bool {
        let live = lock(&self.runs);
        if self.draining.load(Ordering::SeqCst) {
            return false;
        }
        if live.is_empty() {
            self.stopping.store(true, Ordering::SeqCst);
        }
        live.is_empty()
    }

    /// Stops accepting, stops every run it owns (reason `daemon stopped`),
    /// then ends its own groups.
    fn shutdown(&self, sock: &Path) {
        let live: Vec<(String, mpsc::Sender<Stop>)> = {
            let runs = lock(&self.runs);
            self.stopping.store(true, Ordering::SeqCst);
            runs.iter()
                .map(|(id, l)| (id.clone(), l.stop.clone()))
                .collect()
        };
        let _ = std::fs::remove_file(sock);
        let waits: Vec<(String, mpsc::Receiver<State>)> = live
            .into_iter()
            .filter_map(|(id, tx)| {
                let (reply, rx) = mpsc::channel();
                tx.send(Stop {
                    reason: "daemon stopped",
                    reply,
                })
                .ok()
                .map(|()| (id, rx))
            })
            .collect();
        for (id, rx) in waits {
            match rx.recv_timeout(STOP_WAIT) {
                Ok(_) => {}
                Err(_) => eprintln!(
                    "cos daemon: run {id} did not end within {}s",
                    STOP_WAIT.as_secs()
                ),
            }
        }
        self.end_own_groups();
    }

    /// Kills every group the daemon itself started (judge checks, `git`,
    /// `hc`) under the identity check, refusing any new one, and mirrors
    /// what is left.
    fn end_own_groups(&self) {
        let groups = proc::close_tracking();
        for g in &groups {
            proc::kill_recorded(g);
        }
        let left = proc::wait_gone(&groups, GONE_BOUND);
        for g in &left {
            eprintln!(
                "cos daemon: group {} still alive after {}s",
                g.pgid,
                GONE_BOUND.as_secs()
            );
        }
        if let Err(e) = write_groups(&self.opts.dir.join(GROUPS), &left) {
            eprintln!("cos daemon: {e:#}");
        }
    }
}

#[cfg(test)]
mod tests;
