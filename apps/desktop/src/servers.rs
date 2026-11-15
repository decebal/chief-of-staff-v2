//! The dashboard server supervisor: the only code that spawns or signals a
//! dashboard server. Pidfiles and port locks live under `<data dir>/servers`,
//! server logs under `<log dir>/servers`.

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::net::{Ipv4Addr, SocketAddrV4, TcpStream};
use std::os::fd::AsRawFd;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use cos::dashboards::{Dashboard, Probe, ProbeResult, Server};
use serde::{Deserialize, Serialize};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
const PROBE_TIMEOUT: Duration = Duration::from_secs(3);
const UNMANAGED_HINT: &str = "its owner (for example launchd) is not running it";
const LOCK_HELD_HINT: &str = "another app instance is starting it; open the dashboard again";
const EXITING_HINT: &str =
    "the previous server on this port is still exiting; open the dashboard again";
const READY_EVERY: Duration = Duration::from_millis(250);
const READY_WITHIN: Duration = Duration::from_secs(10);
const TERM_GRACE: Duration = Duration::from_secs(5);
const EXITED_WAIT: Duration = Duration::from_secs(5);
const LOG_TAIL_LINES: usize = 20;
const LOG_KEEP: u64 = 1024 * 1024;

/// A dashboard `ensure` verified: its URL, and whether this app instance owns the server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Running {
    pub url: String,
    pub owned: bool,
}

/// Why the dashboard pane shows no page. Serialised as `{"kind": <variant>, <fields>}`;
/// `dash.js` switches on `kind`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind")]
pub enum Refusal {
    /// `owned` only when the listener is this app's Owned server; `spawned` only for the
    /// identity probe after readiness.
    Identity {
        status: String,
        head: String,
        owned: bool,
        spawned: bool,
    },
    BinaryMissing {
        path: PathBuf,
    },
    NotAdmissible {
        rule: String,
        path: PathBuf,
    },
    FailedToStart {
        log_tail: String,
    },
    NotRunning {
        hint: String,
    },
    StaleBusiness {
        message: String,
    },
    UnknownDashboard {
        name: String,
    },
    InvalidConfig {
        message: String,
    },
    Internal {
        message: String,
    },
}

/// The reaper's exit notice, one per child, before `waitpid`. Serialised it is the
/// `dashboard-exited` payload, which the app emits only for `owned`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExitNotice {
    pub addr: SocketAddrV4,
    /// `exit <n>` or `signal <n>`.
    pub status: String,
    pub log_tail: String,
    #[serde(skip)]
    pub owned: bool,
}

/// The kernel's view of a spawned child, read before it has a waiter, so its pid
/// cannot be reused during the reads. Built only from a `Child`: a pidfile record is
/// never copied from an earlier file.
#[derive(Debug, Clone)]
pub struct SpawnResult {
    pid: libc::pid_t,
    pgid: Option<libc::pid_t>,
    start_time: Option<(i64, i64)>,
    exe: Option<PathBuf>,
}

impl SpawnResult {
    pub fn read(child: &Child) -> Self {
        let pid = child.id() as libc::pid_t;
        Self {
            pid,
            pgid: cos::proc::pgid(pid),
            start_time: cos::proc::start_time(pid),
            exe: cos::proc::exe(pid),
        }
    }
}

/// `<data dir>/servers/<port>.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    pub pid: libc::pid_t,
    pub pgid: Option<libc::pid_t>,
    pub start_time: Option<(i64, i64)>,
    pub exe: Option<PathBuf>,
    pub addr: SocketAddrV4,
}

/// `VERIFY(ownership)` for the record behind a pidfile removal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ownership {
    /// `exited`: the reaper has marked the slot Exited.
    Owned {
        exited: bool,
    },
    Starting {
        exited: bool,
    },
    /// `stopped`: the leader failed the five checks after SIGTERM, or got the
    /// deadline SIGKILL.
    Leftover {
        stopped: bool,
    },
    Stale,
    Held,
}

#[derive(Debug)]
pub enum StoreError {
    /// `rule` is `pidfile_path` or `ownership`; the file is left in place.
    Refused {
        rule: &'static str,
    },
    Io(anyhow::Error),
}

type Emit = Box<dyn Fn(ExitNotice) + Send + Sync>;
type Spawn = Box<dyn Fn(&Server, &Path) -> anyhow::Result<Child> + Send + Sync>;
type Connect = Box<dyn Fn(SocketAddrV4) -> bool + Send + Sync>;

/// What the failure epilogue, the stop path and the reaper signal, fixed once after the
/// spawn: the group when the child leads its own, the leader alone otherwise.
#[derive(Debug, Clone, Copy)]
enum Target {
    Group(libc::pid_t),
    Leader(libc::pid_t),
}

impl Target {
    fn of(spawn: &SpawnResult) -> Self {
        match spawn.pgid {
            Some(pgid) if pgid == spawn.pid => Self::Group(pgid),
            _ => Self::Leader(spawn.pid),
        }
    }

    fn signal(self, signal: libc::c_int) {
        match self {
            Self::Group(pgid) => cos::proc::signal_group(pgid, signal),
            Self::Leader(pid) => cos::proc::signal_pid(pid, signal),
        }
    }
}

/// Set by a child's reaper once it has reaped the child and released the port lock.
#[derive(Default)]
struct Notice {
    set: Mutex<bool>,
    changed: Condvar,
}

impl Notice {
    fn set(&self) {
        *self.set.lock().expect("notice lock") = true;
        self.changed.notify_all();
    }

    fn is_set(&self) -> bool {
        *self.set.lock().expect("notice lock")
    }

    /// Whether the notice was set within `timeout`; `None` waits for as long as it takes.
    fn wait(&self, timeout: Option<Duration>) -> bool {
        let end = timeout.map(|t| Instant::now() + t);
        let mut set = self.set.lock().expect("notice lock");
        while !*set {
            set = match end {
                None => self.changed.wait(set).expect("notice lock"),
                Some(end) => {
                    let left = end.saturating_duration_since(Instant::now());
                    if left.is_zero() {
                        return false;
                    }
                    self.changed.wait_timeout(set, left).expect("notice lock").0
                }
            };
        }
        true
    }
}

/// A spawn recorded in its slot.
#[derive(Clone)]
struct Recorded {
    target: Target,
    notice: Arc<Notice>,
}

/// One per key, the host and port of a dashboard's `url`. An External listener has none.
enum State {
    /// Inserted at lookup, before any probe, lock or spawn.
    Reserved,
    /// Holds `<port>.lock`, and the child once its spawn is recorded.
    Starting { lock: File, spawn: Option<Recorded> },
    /// Passed readiness and identity; the target is always a group.
    Owned { lock: File, spawn: Recorded },
    /// The reaper saw the child exit and holds the lock until its notice is set.
    /// `starting` while the `ensure` that spawned it has not finished its epilogue.
    Exited { spawn: Recorded, starting: bool },
    /// Returned to every waiter; the next open drops it.
    Refused(Refusal),
}

struct Slot {
    /// Tells a waiter whether the slot it waited on is still the one at its key.
    id: u64,
    state: State,
}

#[derive(Default)]
struct Slots {
    next: u64,
    by_key: HashMap<SocketAddrV4, Slot>,
}

/// Shared with each child's reaper thread.
pub struct Supervisor(Arc<Inner>);

struct Inner {
    data: PathBuf,
    logs: PathBuf,
    emit: Emit,
    spawn: Spawn,
    connect: Connect,
    /// Guards lookup, insert and every slot transition; probes, the spawn and
    /// readiness run outside it.
    slots: Mutex<Slots>,
    /// Woken on every transition out of Reserved or Starting, and on the spawn record.
    changed: Condvar,
}

impl Supervisor {
    /// Creates `data_dir/servers` and `log_dir/servers`: `atomic_write_bytes` does not
    /// create the parent of the pidfile it writes.
    pub fn new(
        data_dir: &Path,
        log_dir: &Path,
        emit: impl Fn(ExitNotice) + Send + Sync + 'static,
        spawn: impl Fn(&Server, &Path) -> anyhow::Result<Child> + Send + Sync + 'static,
        connect: impl Fn(SocketAddrV4) -> bool + Send + Sync + 'static,
    ) -> io::Result<Self> {
        let data = data_dir.join("servers");
        let logs = log_dir.join("servers");
        for dir in [&data, &logs] {
            std::fs::create_dir_all(dir)
                .map_err(|e| io::Error::new(e.kind(), format!("create {}: {e}", dir.display())))?;
        }
        Ok(Self(Arc::new(Inner {
            data,
            logs,
            emit: Box::new(emit),
            spawn: Box::new(spawn),
            connect: Box::new(connect),
            slots: Mutex::default(),
            changed: Condvar::new(),
        })))
    }

    /// `server` is `None` for an unmanaged dashboard. Running only after the
    /// dashboard's own identity probe passed.
    pub fn ensure(
        &self,
        server: Option<&Server>,
        dashboard: &Dashboard,
    ) -> Result<Running, Refusal> {
        self.0.ensure(server, dashboard)
    }

    /// `STORE(pidfile)` write: `<data dir>/servers/<port>.json`, atomically, from the
    /// spawn result and the validated `addr`.
    pub fn write_pidfile(&self, spawn: &SpawnResult, addr: SocketAddrV4) -> Result<(), StoreError> {
        self.0.write_pidfile(spawn, addr)
    }

    /// `STORE(pidfile)` removal, admitted by `VERIFY(pidfile_path)` and
    /// `VERIFY(ownership)`. A pidfile already gone is not an error.
    pub fn remove_pidfile(&self, file: &Path, ownership: &Ownership) -> Result<(), StoreError> {
        self.0.remove_pidfile(file, ownership)
    }

    /// `STORE(port_lock)` open: `flock(LOCK_EX | LOCK_NB)` on `<data dir>/servers/<port>.lock`,
    /// admitted by `VERIFY(pidfile_path)`. `None` while another open file description
    /// holds it; dropping the file releases it.
    pub fn open_port_lock(&self, file: &Path) -> Result<Option<File>, StoreError> {
        self.0.open_port_lock(file)
    }
}

/// What `ensure` does with the slot it found at its key.
enum Found {
    Wait,
    Owned,
    Exiting(Arc<Notice>),
}

impl Inner {
    fn ensure(
        self: &Arc<Self>,
        server: Option<&Server>,
        dashboard: &Dashboard,
    ) -> Result<Running, Refusal> {
        let key = SocketAddrV4::new(Ipv4Addr::LOCALHOST, dashboard.port);
        loop {
            let mut slots = self.lock();
            let Some(slot) = slots.by_key.get(&key) else {
                slots.next += 1;
                let id = slots.next;
                slots.by_key.insert(
                    key,
                    Slot {
                        id,
                        state: State::Reserved,
                    },
                );
                drop(slots);
                return self.probe(key, id, server, dashboard);
            };
            let id = slot.id;
            let found = match &slot.state {
                State::Refused(_) => {
                    slots.by_key.remove(&key);
                    continue;
                }
                State::Exited {
                    spawn,
                    starting: false,
                } => {
                    if spawn.notice.is_set() {
                        slots.by_key.remove(&key);
                        continue;
                    }
                    Found::Exiting(spawn.notice.clone())
                }
                State::Owned { .. } => Found::Owned,
                State::Reserved | State::Starting { .. } | State::Exited { .. } => Found::Wait,
            };
            match found {
                Found::Wait => {
                    if let Some(refusal) = self.wait(slots, key, id) {
                        return Err(refusal);
                    }
                }
                Found::Owned => {
                    drop(slots);
                    return identity(key, &dashboard.probe, true, false).map(|()| Running {
                        url: dashboard.url.clone(),
                        owned: true,
                    });
                }
                Found::Exiting(notice) => {
                    drop(slots);
                    if !notice.wait(Some(EXITED_WAIT)) {
                        return Err(Refusal::NotRunning {
                            hint: EXITING_HINT.to_owned(),
                        });
                    }
                }
            }
        }
    }

    fn write_pidfile(&self, spawn: &SpawnResult, addr: SocketAddrV4) -> Result<(), StoreError> {
        if addr.port() == 0 {
            return Err(StoreError::Refused {
                rule: "pidfile_path",
            });
        }
        let record = Record {
            pid: spawn.pid,
            pgid: spawn.pgid,
            start_time: spawn.start_time,
            exe: spawn.exe.clone(),
            addr,
        };
        let bytes = serde_json::to_vec(&record).map_err(|e| StoreError::Io(e.into()))?;
        cos::state::atomic_write_bytes(&self.data.join(format!("{}.json", addr.port())), &bytes)
            .map_err(StoreError::Io)
    }

    fn remove_pidfile(&self, file: &Path, ownership: &Ownership) -> Result<(), StoreError> {
        if !self.pidfile_path(file, "json") {
            return Err(StoreError::Refused {
                rule: "pidfile_path",
            });
        }
        let admitted = match *ownership {
            Ownership::Owned { exited } | Ownership::Starting { exited } => exited,
            Ownership::Leftover { stopped } => stopped,
            Ownership::Stale => true,
            Ownership::Held => false,
        };
        if !admitted {
            return Err(StoreError::Refused { rule: "ownership" });
        }
        match std::fs::remove_file(file) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(StoreError::Io(
                anyhow::Error::new(e).context(format!("remove {}", file.display())),
            )),
            _ => Ok(()),
        }
    }

    /// Opened through `std::fs`, so close-on-exec: a spawned server never inherits the
    /// lock, which would otherwise outlive a crash in a leftover. Never unlinked: a second
    /// instance would then lock a new inode at the same path.
    fn open_port_lock(&self, file: &Path) -> Result<Option<File>, StoreError> {
        if !self.pidfile_path(file, "lock") {
            return Err(StoreError::Refused {
                rule: "pidfile_path",
            });
        }
        let io = |what: &str, e: io::Error| {
            StoreError::Io(anyhow::Error::new(e).context(format!("{what} {}", file.display())))
        };
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(file)
            .map_err(|e| io("open", e))?;
        let taken = try_flock(&lock).map_err(|e| io("lock", e))?;
        Ok(taken.then_some(lock))
    }

    /// `VERIFY(pidfile_path)`: `file` is `<data dir>/servers/<port>.<ext>` with `<port>`
    /// the canonical decimal form of 1 to 65535.
    fn pidfile_path(&self, file: &Path, ext: &str) -> bool {
        if file.parent() != Some(self.data.as_path()) {
            return false;
        }
        let Some(stem) = file
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_suffix(ext))
            .and_then(|n| n.strip_suffix('.'))
        else {
            return false;
        };
        stem.parse::<u16>()
            .is_ok_and(|port| port != 0 && port.to_string() == stem)
    }

    fn lock(&self) -> MutexGuard<'_, Slots> {
        self.slots.lock().expect("supervisor lock")
    }

    /// Waits while slot `id` at `key` is Reserved, Starting, or Exited with its `ensure`
    /// still running. A copy of its refusal when it ends Refused; `None` otherwise, so
    /// the caller runs `ensure` again.
    fn wait(
        &self,
        mut slots: MutexGuard<'_, Slots>,
        key: SocketAddrV4,
        id: u64,
    ) -> Option<Refusal> {
        loop {
            slots = self.changed.wait(slots).expect("supervisor lock");
            match slots.by_key.get(&key) {
                Some(Slot {
                    id: now,
                    state:
                        State::Reserved | State::Starting { .. } | State::Exited { starting: true, .. },
                }) if *now == id => {}
                Some(Slot {
                    id: now,
                    state: State::Refused(refusal),
                }) if *now == id => return Some(refusal.clone()),
                _ => return None,
            }
        }
    }

    /// Replaces slot `id` at `key`, when it is still there, with Refused, or removes it
    /// for `None`; wakes the waiters either way.
    fn settle(&self, key: SocketAddrV4, id: u64, refusal: Option<Refusal>) {
        let mut slots = self.lock();
        if slots.by_key.get(&key).is_some_and(|s| s.id == id) {
            match refusal {
                Some(r) => {
                    slots.by_key.insert(
                        key,
                        Slot {
                            id,
                            state: State::Refused(r),
                        },
                    );
                }
                None => {
                    slots.by_key.remove(&key);
                }
            }
        }
        self.changed.notify_all();
    }

    fn probe(
        self: &Arc<Self>,
        key: SocketAddrV4,
        id: u64,
        server: Option<&Server>,
        dashboard: &Dashboard,
    ) -> Result<Running, Refusal> {
        if !(self.connect)(key) {
            return self.start(key, id, server, dashboard);
        }
        let result = identity(key, &dashboard.probe, false, false).map(|()| Running {
            url: dashboard.url.clone(),
            owned: false,
        });
        self.settle(key, id, result.as_ref().err().cloned());
        result
    }

    /// ensure: start, reached only when `connect` found no listener.
    fn start(
        self: &Arc<Self>,
        key: SocketAddrV4,
        id: u64,
        server: Option<&Server>,
        dashboard: &Dashboard,
    ) -> Result<Running, Refusal> {
        let refuse = |refusal: Refusal| {
            self.settle(key, id, Some(refusal.clone()));
            Err(refusal)
        };
        let Some(server) = server else {
            return refuse(Refusal::NotRunning {
                hint: UNMANAGED_HINT.to_owned(),
            });
        };
        if let Err(refusal) = binary_admissible(server, start_time_supported()) {
            return refuse(refusal);
        }
        let path = self.data.join(format!("{}.lock", server.addr.port()));
        let mut slots = self.lock();
        let lock = match self.open_port_lock(&path) {
            Ok(Some(lock)) => lock,
            Ok(None) => {
                drop(slots);
                return refuse(Refusal::NotRunning {
                    hint: LOCK_HELD_HINT.to_owned(),
                });
            }
            Err(e) => {
                drop(slots);
                let message = match e {
                    StoreError::Refused { rule } => format!("lock {}: {rule}", path.display()),
                    StoreError::Io(e) => format!("{e:#}"),
                };
                return refuse(Refusal::Internal { message });
            }
        };
        match slots.by_key.get_mut(&key) {
            Some(slot) if slot.id == id => slot.state = State::Starting { lock, spawn: None },
            _ => {
                drop(slots);
                return refuse(Refusal::Internal {
                    message: format!("slot for {key} replaced while reserved"),
                });
            }
        }
        drop(slots);

        let log = self.log_path(server.addr);
        store_server_log(&log);
        let child = match (self.spawn)(server, &log) {
            Ok(child) => child,
            Err(e) => {
                append_log(&log, &format!("spawn {}: {e:#}", server.binary.display()));
                return Err(self.epilogue(key, id, None, failed_to_start(&log)));
            }
        };
        let result = SpawnResult::read(&child);
        let recorded = Recorded {
            target: Target::of(&result),
            notice: Arc::default(),
        };
        self.record(key, id, child, &recorded);
        if let Err(field) = spawn_result_admissible(&result) {
            append_log(&log, &format!("spawn result refused: {field}"));
            return Err(self.epilogue(key, id, Some(recorded), failed_to_start(&log)));
        }
        {
            let slots = self.lock();
            if !matches!(
                slots.by_key.get(&key),
                Some(Slot { id: now, state: State::Starting { .. } }) if *now == id
            ) {
                drop(slots);
                return Err(self.epilogue(key, id, Some(recorded), failed_to_start(&log)));
            }
            if let Err(e) = self.write_pidfile(&result, server.addr) {
                drop(slots);
                append_log(&log, &format!("pidfile: {e:?}"));
                return Err(self.epilogue(key, id, Some(recorded), failed_to_start(&log)));
            }
        }
        if !self.ready(key, id, server) {
            return Err(self.epilogue(key, id, Some(recorded), failed_to_start(&log)));
        }
        if let Err(refusal) = identity(key, &dashboard.probe, false, true) {
            return Err(self.epilogue(key, id, Some(recorded), refusal));
        }
        let mut slots = self.lock();
        let Some(slot) = slots.by_key.get_mut(&key).filter(|s| s.id == id) else {
            drop(slots);
            return Err(self.epilogue(key, id, Some(recorded), failed_to_start(&log)));
        };
        let state = std::mem::replace(&mut slot.state, State::Reserved);
        let State::Starting {
            lock,
            spawn: Some(spawn),
        } = state
        else {
            slot.state = state;
            drop(slots);
            return Err(self.epilogue(key, id, Some(recorded), failed_to_start(&log)));
        };
        slot.state = State::Owned { lock, spawn };
        self.changed.notify_all();
        Ok(Running {
            url: dashboard.url.clone(),
            owned: true,
        })
    }

    /// Spawn step 2: records the spawn in this `ensure`'s Starting slot and starts its
    /// reaper in the same critical section, so no Starting slot has an unrecorded child.
    fn record(self: &Arc<Self>, key: SocketAddrV4, id: u64, child: Child, recorded: &Recorded) {
        let mut slots = self.lock();
        if let Some(Slot {
            state: State::Starting { spawn, .. },
            ..
        }) = slots.by_key.get_mut(&key).filter(|s| s.id == id)
        {
            *spawn = Some(recorded.clone());
        }
        let (inner, notice) = (Arc::clone(self), recorded.notice.clone());
        std::thread::spawn(move || inner.reap(key, id, child, &notice));
        self.changed.notify_all();
    }

    /// `VERIFY(readiness)`: two consecutive passing `ready` probes, 250 ms apart, within
    /// 10 s, with no exit reported by the reaper before the first or after the second.
    fn ready(&self, key: SocketAddrV4, id: u64, server: &Server) -> bool {
        let end = Instant::now() + READY_WITHIN;
        let mut passes = 0;
        loop {
            if self.exited(key, id) {
                return false;
            }
            match cos::dashboards::probe(server.addr, &server.ready, PROBE_TIMEOUT) {
                ProbeResult::Response { matched: true, .. } => passes += 1,
                _ => passes = 0,
            }
            if passes >= 2 {
                return !self.exited(key, id);
            }
            if Instant::now() >= end {
                return false;
            }
            std::thread::sleep(READY_EVERY);
        }
    }

    fn exited(&self, key: SocketAddrV4, id: u64) -> bool {
        exited_in(&self.lock(), key, id)
    }

    /// The failure epilogue of every refusal after the port lock is taken. Signals are
    /// sent under the supervisor lock and skipped once the reaper marked the slot Exited.
    fn epilogue(
        &self,
        key: SocketAddrV4,
        id: u64,
        spawn: Option<Recorded>,
        refusal: Refusal,
    ) -> Refusal {
        if let Some(spawn) = &spawn {
            let signal = |signal| {
                let slots = self.lock();
                if !exited_in(&slots, key, id) {
                    spawn.target.signal(signal);
                }
            };
            signal(libc::SIGTERM);
            if !spawn.notice.wait(Some(TERM_GRACE)) {
                signal(libc::SIGKILL);
                spawn.notice.wait(None);
            }
            let _ = self.remove_pidfile(
                &self.pidfile(key),
                &Ownership::Starting {
                    exited: self.exited(key, id),
                },
            );
        }
        let mut slots = self.lock();
        if let Some(slot) = slots.by_key.get_mut(&key).filter(|s| {
            s.id == id && matches!(s.state, State::Starting { .. } | State::Exited { .. })
        }) {
            slot.state = State::Refused(refusal.clone());
        }
        self.changed.notify_all();
        refusal
    }

    fn pidfile(&self, key: SocketAddrV4) -> PathBuf {
        self.data.join(format!("{}.json", key.port()))
    }

    /// The child's only waiter: reports its exit, reaps it, then frees the port lock.
    fn reap(&self, key: SocketAddrV4, id: u64, mut child: Child, notice: &Notice) {
        let status = wait_exited(child.id() as libc::pid_t);
        let (lock, owned) = {
            let mut slots = self.lock();
            let mut taken = (None, false);
            if let Some(slot) = slots.by_key.get_mut(&key).filter(|s| s.id == id) {
                let state = std::mem::replace(&mut slot.state, State::Reserved);
                slot.state = match state {
                    State::Starting {
                        lock,
                        spawn: Some(spawn),
                    } => {
                        taken = (Some(lock), false);
                        State::Exited {
                            spawn,
                            starting: true,
                        }
                    }
                    State::Owned { lock, spawn } => {
                        taken = (Some(lock), true);
                        State::Exited {
                            spawn,
                            starting: false,
                        }
                    }
                    other => other,
                };
                let ownership = match taken {
                    (Some(_), true) => Some(Ownership::Owned { exited: true }),
                    (Some(_), false) => Some(Ownership::Starting { exited: true }),
                    (None, _) => None,
                };
                if let Some(ownership) = ownership {
                    let _ = self.remove_pidfile(&self.pidfile(key), &ownership);
                }
            }
            self.changed.notify_all();
            taken
        };
        (self.emit)(ExitNotice {
            addr: key,
            status,
            log_tail: log_tail(&self.log_path(key)),
            owned,
        });
        let _ = child.wait();
        drop(lock);
        notice.set();
    }

    /// `VERIFY(log_path)`: built from the parsed port alone, so it always admits.
    fn log_path(&self, addr: SocketAddrV4) -> PathBuf {
        self.logs.join(format!("{}.log", addr.port()))
    }
}

/// Whether slot `id` at `key` is Exited.
fn exited_in(slots: &Slots, key: SocketAddrV4, id: u64) -> bool {
    matches!(
        slots.by_key.get(&key),
        Some(Slot { id: now, state: State::Exited { .. } }) if *now == id
    )
}

/// `VERIFY(spawn_result)`; the error names the failing field.
fn spawn_result_admissible(spawn: &SpawnResult) -> Result<(), &'static str> {
    if spawn.pid <= 0 {
        return Err("pid");
    }
    if spawn.pgid != Some(spawn.pid) {
        return Err("pgid");
    }
    if spawn.exe.as_ref().is_none_or(|e| e.as_os_str().is_empty()) {
        return Err("exe");
    }
    if spawn.start_time.is_none() {
        return Err("start_time");
    }
    Ok(())
}

/// `waitid(P_PID, pid, WEXITED | WNOWAIT)`: returns while the leader is an unreaped
/// zombie, which keeps its group id in use. Darwin's `waitid` has been reported to
/// return for a stopped child (golang/go#19314), so any other code waits again.
fn wait_exited(pid: libc::pid_t) -> String {
    loop {
        // SAFETY: `siginfo_t` is plain data; all-zero is a valid value.
        let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
        // SAFETY: `info` is a writable `siginfo_t`; WNOWAIT leaves the child unreaped.
        let r = unsafe {
            libc::waitid(
                libc::P_PID,
                pid as libc::id_t,
                &mut info,
                libc::WEXITED | libc::WNOWAIT,
            )
        };
        if r == -1 {
            let e = io::Error::last_os_error();
            if e.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return format!("wait failed: {e}");
        }
        // SAFETY: `waitid` filled `info` for a child state change.
        let code = unsafe { info.si_status() };
        match info.si_code {
            libc::CLD_EXITED => return format!("exit {code}"),
            libc::CLD_KILLED | libc::CLD_DUMPED => return format!("signal {code}"),
            _ => std::thread::sleep(Duration::from_millis(50)),
        }
    }
}

fn failed_to_start(log: &Path) -> Refusal {
    Refusal::FailedToStart {
        log_tail: log_tail(log),
    }
}

/// The last 20 lines of `log`, empty when it cannot be read.
fn log_tail(log: &Path) -> String {
    let text = std::fs::read(log).unwrap_or_default();
    let text = String::from_utf8_lossy(&text);
    let lines: Vec<&str> = text.lines().collect();
    let mut tail = lines[lines.len().saturating_sub(LOG_TAIL_LINES)..].join("\n");
    if !tail.is_empty() {
        tail.push('\n');
    }
    tail
}

/// `STORE(server_log)`: cuts `log` to its last 1 MiB, creating it if absent. Best effort:
/// a log that cannot be cut does not stop the spawn.
fn store_server_log(log: &Path) {
    let cut = || -> io::Result<()> {
        let mut f = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(log)?;
        let len = f.metadata()?.len();
        if len <= LOG_KEEP {
            return Ok(());
        }
        f.seek(SeekFrom::Start(len - LOG_KEEP))?;
        let mut tail = Vec::with_capacity(LOG_KEEP as usize);
        (&f).take(LOG_KEEP).read_to_end(&mut tail)?;
        f.set_len(0)?;
        f.seek(SeekFrom::Start(0))?;
        f.write_all(&tail)
    };
    let _ = cut();
}

/// Appends one `cos:` line to the server log, so the log tail names what failed.
fn append_log(log: &Path, line: &str) {
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(log) {
        let _ = writeln!(f, "cos: {line}");
    }
}

/// `VERIFY(binary_admissible)`: `binary` is absolute, a regular file and executable, and
/// `proc::start_time` works on this platform (rule `platform`).
pub fn binary_admissible(server: &Server, start_time_supported: bool) -> Result<(), Refusal> {
    let path = &server.binary;
    let refuse = |rule: &str| {
        Err(Refusal::NotAdmissible {
            rule: rule.to_owned(),
            path: path.clone(),
        })
    };
    if !path.is_absolute() {
        return refuse("absolute");
    }
    let meta = match std::fs::metadata(path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Err(Refusal::BinaryMissing { path: path.clone() });
        }
        Err(_) => return refuse("regular_file"),
    };
    if !meta.is_file() {
        return refuse("regular_file");
    }
    if meta.permissions().mode() & 0o111 == 0 {
        return refuse("executable");
    }
    if !start_time_supported {
        return refuse("platform");
    }
    Ok(())
}

fn start_time_supported() -> bool {
    cos::proc::start_time(std::process::id() as libc::pid_t).is_some()
}

/// `flock(LOCK_EX | LOCK_NB)`; false when another open file description holds it.
fn try_flock(file: &File) -> io::Result<bool> {
    // SAFETY: flock on a descriptor this process owns; no memory is touched.
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
        return Ok(true);
    }
    let e = io::Error::last_os_error();
    if e.raw_os_error() == Some(libc::EWOULDBLOCK) {
        Ok(false)
    } else {
        Err(e)
    }
}

/// `VERIFY(identity)` over `PROBE`: retried once only when the first attempt finds no
/// listener or no complete response; a response without the marker refuses at once.
fn identity(addr: SocketAddrV4, probe: &Probe, owned: bool, spawned: bool) -> Result<(), Refusal> {
    let mut result = cos::dashboards::probe(addr, probe, PROBE_TIMEOUT);
    if matches!(result, ProbeResult::NoListener | ProbeResult::NoResponse) {
        result = cos::dashboards::probe(addr, probe, PROBE_TIMEOUT);
    }
    let (status, head) = match result {
        ProbeResult::Response { matched: true, .. } => return Ok(()),
        ProbeResult::Response { status, head, .. } => (status, head),
        ProbeResult::NoListener => ("no listener".to_owned(), String::new()),
        ProbeResult::NoResponse => ("no response".to_owned(), String::new()),
    };
    Err(Refusal::Identity {
        status,
        head,
        owned,
        spawned,
    })
}

/// Starts `[binary, "serve", addr]` as the leader of its own group, in the binary's
/// directory, with stdout and stderr appended to `log`. The log is opened through
/// `std::fs`, close-on-exec, so the child inherits no other app descriptor.
pub fn real_spawn(server: &Server, log: &Path) -> anyhow::Result<Child> {
    let out = OpenOptions::new().create(true).append(true).open(log)?;
    let err = out.try_clone()?;
    let mut cmd = Command::new(&server.binary);
    cmd.arg("serve")
        .arg(server.addr.to_string())
        .stdout(out)
        .stderr(err);
    if let Some(dir) = server.binary.parent() {
        cmd.current_dir(dir);
    }
    cos::proc::spawn_grouped(&mut cmd)
}

/// Whether a listener accepts a TCP connect to `addr` within 3 s.
pub fn real_connect(addr: SocketAddrV4) -> bool {
    TcpStream::connect_timeout(&addr.into(), CONNECT_TIMEOUT).is_ok()
}
