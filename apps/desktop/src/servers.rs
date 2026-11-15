//! The dashboard server supervisor: the only code that spawns or signals a
//! dashboard server. Pidfiles and port locks live under `<data dir>/servers`,
//! server logs under `<log dir>/servers`.

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io;
use std::net::{Ipv4Addr, SocketAddrV4, TcpStream};
use std::os::fd::AsRawFd;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::{Condvar, Mutex, MutexGuard};
use std::time::Duration;

use cos::dashboards::{Dashboard, Probe, ProbeResult, Server};
use serde::{Deserialize, Serialize};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
const PROBE_TIMEOUT: Duration = Duration::from_secs(3);
const UNMANAGED_HINT: &str = "its owner (for example launchd) is not running it";
const LOCK_HELD_HINT: &str = "another app instance is starting it; open the dashboard again";

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

/// One per key, the host and port of a dashboard's `url`. An External listener has none.
enum State {
    /// Inserted at lookup, before any probe, lock or spawn.
    Reserved,
    /// Holds `<port>.lock` until the slot is replaced.
    Starting {
        #[expect(dead_code, reason = "held for its flock, released on drop")]
        lock: File,
    },
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

pub struct Supervisor {
    data: PathBuf,
    #[expect(dead_code)]
    logs: PathBuf,
    #[expect(dead_code)]
    emit: Emit,
    #[expect(dead_code)]
    spawn: Spawn,
    connect: Connect,
    /// Guards lookup, insert and every slot transition; probes run outside it.
    slots: Mutex<Slots>,
    /// Woken on every transition out of Reserved or Starting.
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
        Ok(Self {
            data,
            logs,
            emit: Box::new(emit),
            spawn: Box::new(spawn),
            connect: Box::new(connect),
            slots: Mutex::default(),
            changed: Condvar::new(),
        })
    }

    /// `server` is `None` for an unmanaged dashboard. Running only after the
    /// dashboard's own identity probe passed.
    pub fn ensure(
        &self,
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
            match slot.state {
                State::Refused(_) => {
                    slots.by_key.remove(&key);
                }
                State::Reserved | State::Starting { .. } => {
                    let id = slot.id;
                    if let Some(refusal) = self.wait(slots, key, id) {
                        return Err(refusal);
                    }
                }
            }
        }
    }

    /// `STORE(pidfile)` write: `<data dir>/servers/<port>.json`, atomically, from the
    /// spawn result and the validated `addr`.
    pub fn write_pidfile(&self, spawn: &SpawnResult, addr: SocketAddrV4) -> Result<(), StoreError> {
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

    /// `STORE(pidfile)` removal, admitted by `VERIFY(pidfile_path)` and
    /// `VERIFY(ownership)`. A pidfile already gone is not an error.
    pub fn remove_pidfile(&self, file: &Path, ownership: &Ownership) -> Result<(), StoreError> {
        if !self.pidfile_path(file) {
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

    /// `VERIFY(pidfile_path)`: `file` is `<data dir>/servers/<port>.json` with `<port>`
    /// the canonical decimal form of 1 to 65535, so the `.lock` beside it never matches.
    fn pidfile_path(&self, file: &Path) -> bool {
        if file.parent() != Some(self.data.as_path()) {
            return false;
        }
        let Some(stem) = file
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_suffix(".json"))
        else {
            return false;
        };
        stem.parse::<u16>()
            .is_ok_and(|port| port != 0 && port.to_string() == stem)
    }

    fn lock(&self) -> MutexGuard<'_, Slots> {
        self.slots.lock().expect("supervisor lock")
    }

    /// Waits while slot `id` at `key` is Reserved or Starting. A copy of its refusal when
    /// it ends Refused; `None` when it was removed, so the caller runs `ensure` again.
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
                    state: State::Reserved | State::Starting { .. },
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
        &self,
        key: SocketAddrV4,
        id: u64,
        server: Option<&Server>,
        dashboard: &Dashboard,
    ) -> Result<Running, Refusal> {
        let result = if (self.connect)(key) {
            identity(key, &dashboard.probe, false, false).map(|()| Running {
                url: dashboard.url.clone(),
                owned: false,
            })
        } else {
            Err(self.start(key, id, server))
        };
        match &result {
            Ok(_) => self.settle(key, id, None),
            Err(r) => self.settle(key, id, Some(r.clone())),
        }
        result
    }

    /// ensure: start, reached only when `connect` found no listener. The caller's settle
    /// replaces the slot with Refused, which drops a lock the slot still holds.
    fn start(&self, key: SocketAddrV4, id: u64, server: Option<&Server>) -> Refusal {
        let Some(server) = server else {
            return Refusal::NotRunning {
                hint: UNMANAGED_HINT.to_owned(),
            };
        };
        if let Err(refusal) = binary_admissible(server, start_time_supported()) {
            return refusal;
        }
        let path = self.data.join(format!("{}.lock", server.addr.port()));
        let mut slots = self.lock();
        // Opened through `std::fs`, so close-on-exec: a spawned server never inherits it.
        let taken = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .and_then(|file| Ok(try_flock(&file)?.then_some(file)));
        let lock = match taken {
            Ok(Some(lock)) => lock,
            Ok(None) => {
                return Refusal::NotRunning {
                    hint: LOCK_HELD_HINT.to_owned(),
                };
            }
            Err(e) => {
                return Refusal::Internal {
                    message: format!("lock {}: {e}", path.display()),
                };
            }
        };
        match slots.by_key.get_mut(&key) {
            Some(slot) if slot.id == id => slot.state = State::Starting { lock },
            _ => {
                return Refusal::Internal {
                    message: format!("slot for {key} replaced while reserved"),
                };
            }
        }
        drop(slots);
        Refusal::Internal {
            message: "starting a managed server is not implemented".to_owned(),
        }
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
