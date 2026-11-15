//! The dashboard server supervisor: the only code that spawns or signals a
//! dashboard server. Pidfiles and port locks live under `<data dir>/servers`,
//! server logs under `<log dir>/servers`.

use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io;
use std::net::{Ipv4Addr, SocketAddrV4, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::{Condvar, Mutex, MutexGuard};
use std::time::Duration;

use cos::dashboards::{Dashboard, Probe, ProbeResult, Server};
use serde::Serialize;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
const PROBE_TIMEOUT: Duration = Duration::from_secs(3);
const UNMANAGED_HINT: &str = "its owner (for example launchd) is not running it";

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

type Emit = Box<dyn Fn(ExitNotice) + Send + Sync>;
type Spawn = Box<dyn Fn(&Server, &Path) -> anyhow::Result<Child> + Send + Sync>;
type Connect = Box<dyn Fn(SocketAddrV4) -> bool + Send + Sync>;

/// One per key, the host and port of a dashboard's `url`. An External listener has none.
enum State {
    /// Inserted at lookup, before any probe, lock or spawn.
    Reserved,
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
    #[expect(dead_code)]
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
    /// Woken on every transition out of Reserved.
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
                State::Reserved => {
                    let id = slot.id;
                    if let Some(refusal) = self.wait(slots, key, id) {
                        return Err(refusal);
                    }
                }
            }
        }
    }

    fn lock(&self) -> MutexGuard<'_, Slots> {
        self.slots.lock().expect("supervisor lock")
    }

    /// Waits while slot `id` at `key` is Reserved. A copy of its refusal when it ends
    /// Refused; `None` when it was removed, so the caller runs `ensure` again.
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
                    state: State::Reserved,
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
            Err(start(server))
        };
        match &result {
            Ok(_) => self.settle(key, id, None),
            Err(r) => self.settle(key, id, Some(r.clone())),
        }
        result
    }
}

fn start(server: Option<&Server>) -> Refusal {
    match server {
        None => Refusal::NotRunning {
            hint: UNMANAGED_HINT.to_owned(),
        },
        Some(_) => Refusal::Internal {
            message: "starting a managed server is not implemented".to_owned(),
        },
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
