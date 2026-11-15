//! The dashboard server supervisor: the only code that spawns or signals a
//! dashboard server. Pidfiles and port locks live under `<data dir>/servers`,
//! server logs under `<log dir>/servers`.

use std::fs::OpenOptions;
use std::io;
use std::net::{SocketAddrV4, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::Duration;

use cos::dashboards::Server;
use serde::Serialize;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);

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

#[expect(dead_code)]
pub struct Supervisor {
    data: PathBuf,
    logs: PathBuf,
    emit: Emit,
    spawn: Spawn,
    connect: Connect,
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
        })
    }
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
