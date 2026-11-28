//! Bounded child processes. The caller owns the child for its whole life: one
//! deadline for the operation, kill the process group on expiry, then reap.

use std::io::{BufRead, BufReader, Read};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use anyhow::{Context, Result, bail};

/// Hard ceiling for any child `cos` starts. Lower per call, never higher.
pub const CEILING: Duration = Duration::from_secs(300);

/// Longest single wait between deadline checks, so a deadline that passed
/// while the machine slept fires within this of waking.
const POLL: Duration = Duration::from_secs(1);

/// The two clocks a deadline reads. On macOS `Instant` (mach uptime) stops
/// while the machine sleeps, so a deadline on it alone never fires across
/// hours asleep; the wall clock keeps counting.
pub trait Clock {
    fn mono(&self) -> Instant;
    fn wall(&self) -> SystemTime;
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn mono(&self) -> Instant {
        Instant::now()
    }
    fn wall(&self) -> SystemTime {
        SystemTime::now()
    }
}

/// A deadline on both clocks: expired when either has run out. A wall clock
/// stepped forward (NTP, a manual change) ends it early, never late.
pub struct Deadline<'c> {
    clock: &'c dyn Clock,
    mono: Instant,
    wall: SystemTime,
    pub total: Duration,
}

impl Deadline<'static> {
    pub fn after(total: Duration) -> Self {
        Deadline::on(&SystemClock, total)
    }

    /// For a child whose own ceiling sits above `CEILING` (the brief).
    pub fn capped(total: Duration, ceiling: Duration) -> Self {
        Deadline::new(&SystemClock, total.min(ceiling))
    }
}

impl<'c> Deadline<'c> {
    /// Starts now on `clock`, capped at `CEILING`.
    pub fn on(clock: &'c dyn Clock, total: Duration) -> Self {
        Self::new(clock, total.min(CEILING))
    }

    fn new(clock: &'c dyn Clock, total: Duration) -> Self {
        Self {
            clock,
            mono: clock.mono() + total,
            wall: clock.wall() + total,
            total,
        }
    }

    pub fn left(&self) -> Duration {
        let mono = self.mono.saturating_duration_since(self.clock.mono());
        let wall = self
            .wall
            .duration_since(self.clock.wall())
            .unwrap_or(Duration::ZERO);
        mono.min(wall)
    }

    pub fn expired(&self) -> bool {
        self.left().is_zero()
    }

    /// How long to block before checking again.
    pub fn slice(&self) -> Duration {
        self.left().min(POLL)
    }
}

/// How long a pipe may stay open after its child exited. Past this, whatever
/// still holds it is a leftover of the child's group and is killed.
const PIPE_GRACE: Duration = Duration::from_secs(2);

pub struct Output {
    pub status: ExitStatus,
    pub stdout: String,
    pub stderr: String,
}

/// Run to completion or kill at `deadline`. Never waits past `CEILING`.
pub fn run(cmd: &mut Command, deadline: Duration) -> Result<Output> {
    let until = Deadline::after(deadline);
    let mut child = spawn_grouped(cmd.stdout(Stdio::piped()).stderr(Stdio::piped()))?;
    let pgid = group_of(&child);
    let out = Drain::start(child.stdout.take());
    let err = Drain::start(child.stderr.take());
    let status = wait_until(&mut child, &until)?;
    Ok(Output {
        status,
        stdout: out.finish(pgid),
        stderr: err.finish(pgid),
    })
}

/// Run with bounded input and output under one owned process deadline.
pub fn run_input(cmd: &mut Command, input: &[u8], deadline: Duration) -> Result<Output> {
    use std::io::Write;
    const LIMIT: usize = 128 * 1024;
    let until = Deadline::after(deadline);
    let mut child = spawn_tracked(
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped()),
    )?;
    let pgid = group_of(&child);
    let out = Drain::start_limited(child.stdout.take(), LIMIT);
    let err = Drain::start_limited(child.stderr.take(), LIMIT);
    let mut stdin = child.stdin.take().context("child stdin unavailable")?;
    let bytes = input.to_vec();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(stdin.write_all(&bytes));
    });
    let status = wait_until(&mut child, &until);
    let status = status?;
    let written = match rx.recv_timeout(PIPE_GRACE) {
        Ok(result) => result,
        Err(_) => {
            kill_group(pgid);
            rx.recv_timeout(PIPE_GRACE)
                .context("reviewer input pipe did not close")?
        }
    };
    written.context("reviewer input failed")?;
    let stdout = out.finish(pgid);
    let stderr = err.finish(pgid);
    if stdout.len() > LIMIT || stderr.len() > LIMIT {
        bail!("reviewer output exceeded limit");
    }
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

/// Spawn with stdout delivered line by line; `on_line` returning `false` stops
/// reading early. The child is killed and reaped on deadline, early stop, or error.
pub fn stream_lines(
    cmd: &mut Command,
    deadline: Duration,
    on_line: impl FnMut(&str) -> Result<bool>,
) -> Result<(ExitStatus, String)> {
    stream_lines_on(&SystemClock, cmd, deadline, on_line)
}

/// `stream_lines` against `clock`, the seam a test freezes `Instant` through.
pub fn stream_lines_on(
    clock: &dyn Clock,
    cmd: &mut Command,
    deadline: Duration,
    mut on_line: impl FnMut(&str) -> Result<bool>,
) -> Result<(ExitStatus, String)> {
    let until = Deadline::on(clock, deadline);
    let mut child = spawn_grouped(cmd.stdout(Stdio::piped()).stderr(Stdio::piped()))?;
    let pgid = group_of(&child);
    let err = Drain::start(child.stderr.take());
    let rx = lines(child.stdout.take().context("child stdout not piped")?);

    loop {
        if until.expired() {
            kill_and_reap(&mut child);
            bail!("timed out after {}s", until.total.as_secs());
        }
        match rx.recv_timeout(until.slice()) {
            Ok(line) => match on_line(&line) {
                Ok(true) => {}
                Ok(false) => break,
                Err(e) => {
                    kill_and_reap(&mut child);
                    return Err(e);
                }
            },
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    let status = wait_until(&mut child, &until)?;
    Ok((status, err.finish(pgid)))
}

/// Spawns `cmd` as the leader of a new process group, stdin closed, so one
/// signal to the group reaches everything it starts. While a run tracks its
/// groups (`track`), the child goes through the spawn gate.
pub fn spawn_grouped(cmd: &mut Command) -> Result<Child> {
    cmd.stdin(Stdio::null());
    spawn_tracked(cmd)
}

fn spawn_tracked(cmd: &mut Command) -> Result<Child> {
    if tracking() {
        for name in tracker().strip {
            cmd.env_remove(name);
        }
        return spawn_gated(cmd, register);
    }
    cmd.process_group(0)
        .spawn()
        .with_context(|| format!("spawn {:?}", cmd.get_program()))
}

/// Lines of `pipe`, read on their own thread, so the owner can poll with a
/// deadline instead of blocking in `read`.
pub fn lines<R: Read + Send + 'static>(pipe: R) -> mpsc::Receiver<String> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        for line in BufReader::new(pipe).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    rx
}

pub fn group_of(child: &Child) -> libc::pid_t {
    child.id() as libc::pid_t
}

/// A whole pipe read on its own thread. `finish` never blocks for long: a
/// grandchild that inherited the pipe would otherwise hold it open forever.
struct Drain(mpsc::Receiver<String>);

impl Drain {
    fn start_limited<R: Read + Send + 'static>(pipe: Option<R>, limit: usize) -> Self {
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let mut result = Vec::new();
            let mut chunk = [0; 8192];
            if let Some(mut pipe) = pipe {
                while let Ok(n) = pipe.read(&mut chunk) {
                    if n == 0 {
                        break;
                    }
                    let room = (limit + 1).saturating_sub(result.len());
                    result.extend_from_slice(&chunk[..n.min(room)]);
                }
            }
            let _ = tx.send(String::from_utf8_lossy(&result).into_owned());
        });
        Self(rx)
    }
    fn start<R: Read + Send + 'static>(pipe: Option<R>) -> Self {
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let mut buf = String::new();
            if let Some(mut p) = pipe {
                let _ = p.read_to_string(&mut buf);
            }
            let _ = tx.send(buf);
        });
        Self(rx)
    }

    fn finish(self, pgid: libc::pid_t) -> String {
        if let Ok(s) = self.0.recv_timeout(PIPE_GRACE) {
            return s;
        }
        kill_group(pgid);
        self.0.recv_timeout(PIPE_GRACE).unwrap_or_default()
    }
}

pub fn wait_until(child: &mut Child, until: &Deadline) -> Result<ExitStatus> {
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        if until.expired() {
            kill_and_reap(child);
            bail!("timed out waiting for child exit");
        }
        thread::sleep(Duration::from_millis(50));
    }
}

pub fn kill_and_reap(child: &mut Child) {
    kill_group(group_of(child));
    let _ = child.kill();
    let _ = child.wait();
}

/// SIGKILL to every process in group `pgid`. A pgid stays reserved while any
/// member lives, so this cannot reach an unrelated process once the group
/// has members; with none left it is a no-op (ESRCH).
pub fn kill_group(pgid: libc::pid_t) {
    if pgid > 0 {
        // SAFETY: signalling a process group we created; no memory is touched.
        unsafe {
            libc::kill(-pgid, libc::SIGKILL);
        }
    }
}

/// Whether `pid` exists (a zombie counts).
pub fn alive(pid: libc::pid_t) -> bool {
    // SAFETY: signal 0 only checks for existence and permission.
    pid > 0 && unsafe { libc::kill(pid, 0) } == 0
}

/// The executable `pid` runs, as the kernel records it.
#[cfg(target_os = "macos")]
pub fn exe(pid: libc::pid_t) -> Option<PathBuf> {
    use std::os::unix::ffi::OsStrExt;

    if pid <= 0 {
        return None;
    }
    let mut buf = vec![0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
    // SAFETY: the buffer is writable for the length passed.
    let n = unsafe { libc::proc_pidpath(pid, buf.as_mut_ptr().cast(), buf.len() as u32) };
    if n <= 0 {
        return None;
    }
    buf.truncate(n as usize);
    Some(PathBuf::from(std::ffi::OsStr::from_bytes(&buf)))
}

/// The command name `ps` reports for `pid`; not a full path.
#[cfg(not(target_os = "macos"))]
pub fn exe(pid: libc::pid_t) -> Option<PathBuf> {
    if pid <= 0 {
        return None;
    }
    let mut cmd = Command::new("ps");
    cmd.args(["-o", "comm=", "-p", &pid.to_string()]);
    let out = run(&mut cmd, Duration::from_secs(5)).ok()?;
    let name = out.stdout.trim();
    (out.status.success() && !name.is_empty()).then(|| PathBuf::from(name))
}

/// When `pid` started, as seconds and microseconds since the epoch. With
/// the pid it names one process: a reused pid has a different start time.
#[cfg(target_os = "macos")]
pub fn start_time(pid: libc::pid_t) -> Option<(i64, i64)> {
    if pid <= 0 {
        return None;
    }
    // SAFETY: `proc_bsdinfo` is plain data; all-zero is a valid value.
    let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<libc::proc_bsdinfo>() as libc::c_int;
    // SAFETY: the buffer is a `proc_bsdinfo` of the size passed.
    let n = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTBSDINFO,
            0,
            (&mut info as *mut libc::proc_bsdinfo).cast(),
            size,
        )
    };
    (n == size).then_some((info.pbi_start_tvsec as i64, info.pbi_start_tvusec as i64))
}

#[cfg(not(target_os = "macos"))]
pub fn start_time(_pid: libc::pid_t) -> Option<(i64, i64)> {
    None
}

pub fn pgid(pid: libc::pid_t) -> Option<libc::pid_t> {
    if pid <= 0 {
        return None;
    }
    // SAFETY: reads the process table; no memory is touched.
    let g = unsafe { libc::getpgid(pid) };
    (g > 0).then_some(g)
}

/// `signal` to every process in group `pgid`, under `kill_group`'s guard.
pub fn signal_group(pgid: libc::pid_t, signal: libc::c_int) {
    if pgid > 0 {
        // SAFETY: signalling a process group; no memory is touched.
        unsafe {
            libc::kill(-pgid, signal);
        }
    }
}

/// `signal` to `pid` alone. A pid of 0 or below would address a group, so
/// it is refused.
pub fn signal_pid(pid: libc::pid_t, signal: libc::c_int) {
    if pid > 0 {
        // SAFETY: signalling one process; no memory is touched.
        unsafe {
            libc::kill(pid, signal);
        }
    }
}

/// Whether any process is left in group `pgid`.
pub fn group_alive(pgid: libc::pid_t) -> bool {
    // SAFETY: signal 0 only checks for existence and permission.
    pgid > 0 && unsafe { libc::kill(-pgid, 0) } == 0
}

/// `start_time` as a run record writes it: `"<sec>.<usec>"`.
pub fn start_of(pid: libc::pid_t) -> Option<String> {
    start_time(pid).map(|(s, u)| format!("{s}.{u:06}"))
}

/// A process group a run created: its pgid and its leader's start.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Group {
    pub pgid: libc::pid_t,
    pub start: String,
}

/// How long a writer of a final state waits for a run's groups to be gone.
pub const GONE_BOUND: Duration = Duration::from_secs(2);

/// The recorded group has ended: no member is left, or its pgid now leads
/// another process (the group ended and the pid was reused).
pub fn group_gone(g: &Group) -> bool {
    !group_alive(g.pgid) || start_of(g.pgid).is_some_and(|s| s != g.start)
}

/// Group identity: alive, and its leader either has the recorded start or is
/// gone. A pgid stays reserved while any member lives, so a live group with
/// no leader is still the recorded one, unless the pgid was freed and a new
/// group led by that exact pid has since lost its leader; that case is
/// accepted.
pub fn may_kill(g: &Group) -> bool {
    g.pgid > 1 && group_alive(g.pgid) && start_of(g.pgid).is_none_or(|s| s == g.start)
}

/// SIGKILL to a recorded group, only under `may_kill`. Whether it was sent.
pub fn kill_recorded(g: &Group) -> bool {
    let ok = may_kill(g);
    if ok {
        kill_group(g.pgid);
    }
    ok
}

/// Waits up to `bound` for every group to be gone, reaping a leader that is
/// this process's own child. Returns the groups still not gone.
pub fn wait_gone(groups: &[Group], bound: Duration) -> Vec<Group> {
    wait_gone_with(groups, bound, |g| {
        reap_if_child(g.pgid);
        group_gone(g)
    })
}

/// Reaps `pid` if it is this process's child and has exited, so a killed
/// leader does not keep its group alive as a zombie.
pub fn reap_if_child(pid: libc::pid_t) {
    if pid > 0 {
        // SAFETY: a non-blocking wait on one pid; ECHILD when it is not ours.
        unsafe {
            libc::waitpid(pid, std::ptr::null_mut(), libc::WNOHANG);
        }
    }
}

pub fn wait_gone_with(
    groups: &[Group],
    bound: Duration,
    gone: impl Fn(&Group) -> bool,
) -> Vec<Group> {
    let until = Instant::now() + bound;
    loop {
        let left: Vec<Group> = groups.iter().filter(|g| !gone(g)).cloned().collect();
        if left.is_empty() || Instant::now() >= until {
            return left;
        }
        thread::sleep(Duration::from_millis(20));
    }
}

/// Called with the run's live groups each time one is added; an error
/// refuses the spawn that added it.
pub type Hook = Box<dyn Fn(&[Group]) -> Result<()> + Send + Sync>;

struct Tracker {
    hook: Option<Hook>,
    groups: Vec<Group>,
    closed: bool,
    strip: &'static [&'static str],
}

static TRACKER: std::sync::Mutex<Tracker> = std::sync::Mutex::new(Tracker {
    hook: None,
    groups: Vec::new(),
    closed: false,
    strip: &[],
});

fn tracker() -> std::sync::MutexGuard<'static, Tracker> {
    TRACKER.lock().unwrap_or_else(|e| e.into_inner())
}

fn tracking() -> bool {
    let t = tracker();
    t.hook.is_some() || t.closed
}

/// From now on every child this process puts in a group of its own goes
/// through the spawn gate and into the set `hook` sees, with the variables
/// named in `strip` removed from its environment.
pub fn track(hook: Hook, strip: &'static [&'static str]) {
    let mut t = tracker();
    *t = Tracker {
        hook: Some(hook),
        groups: Vec::new(),
        closed: false,
        strip,
    };
}

/// Refuses every further tracked spawn and returns the set, for cleanup.
pub fn close_tracking() -> Vec<Group> {
    let mut t = tracker();
    t.closed = true;
    t.groups.clone()
}

/// Whether `close_tracking` has run: this process is ending its groups, so a
/// child that ended meanwhile may have been killed rather than finished.
pub fn tracking_closed() -> bool {
    tracker().closed
}

/// Back to plain spawns, after the run's final state is written. Never on a
/// path that exits next: until then a closed tracker refuses every spawn.
pub fn untrack() {
    let mut t = tracker();
    *t = Tracker {
        hook: None,
        groups: Vec::new(),
        closed: false,
        strip: &[],
    };
}

fn register(g: &Group) -> Result<()> {
    let mut t = tracker();
    if t.closed {
        bail!("the run is ending: no new process starts");
    }
    t.groups.retain(|x| !group_gone(x));
    t.groups.push(g.clone());
    let result = match &t.hook {
        Some(hook) => hook(&t.groups),
        None => Ok(()),
    };
    if result.is_err() {
        t.groups.pop();
    }
    result
}

static SPAWN: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Spawns `cmd` as the leader of a new group, held before `exec` until
/// `register` has accepted the group. If `register`
/// fails, the child is killed before it runs anything; if this process dies
/// first, the child reads EOF on the gate and exits without exec.
///
/// `Command::spawn` returns only once the child has exec'd, so the spawn
/// runs on a scoped thread while this one learns the pid from the child
/// itself, registers it, then opens the gate.
pub fn spawn_gated(
    cmd: &mut Command,
    register: impl FnOnce(&Group) -> Result<()>,
) -> Result<Child> {
    use std::os::fd::AsRawFd;

    let _one = SPAWN.lock().unwrap_or_else(|e| e.into_inner());
    settle_fork_handlers();
    cmd.process_group(0);
    let (gate_r, gate_w) = std::io::pipe().context("gate pipe")?;
    let (pid_r, pid_w) = std::io::pipe().context("gate pipe")?;
    let fds = (
        gate_r.as_raw_fd(),
        gate_w.as_raw_fd(),
        pid_r.as_raw_fd(),
        pid_w.as_raw_fd(),
    );
    // SAFETY: the hook calls only close, getpid, write and read, all
    // async-signal-safe, on descriptors this function keeps open until the
    // spawn returns.
    unsafe {
        cmd.pre_exec(move || hold_at_gate(fds.0, fds.1, fds.2, fds.3));
    }
    let program = format!("{:?}", cmd.get_program());
    thread::scope(|s| {
        let spawning = s.spawn(|| cmd.spawn());
        let pid = read_pid(fds.2, &spawning);
        let registered = match pid {
            Some(pid) => match start_of(pid) {
                Some(start) => {
                    let g = Group { pgid: pid, start };
                    register(&g).inspect_err(|_| kill_group(pid))
                }
                None => {
                    kill_group(pid);
                    Err(anyhow::anyhow!("gated child {pid} has no start time"))
                }
            },
            None => Err(anyhow::anyhow!("gated child reported no pid")),
        };
        if registered.is_ok() {
            use std::io::Write;
            let _ = (&gate_w).write_all(&[1]);
        }
        drop(gate_w);
        let spawned = spawning
            .join()
            .map_err(|_| anyhow::anyhow!("spawn thread panicked"))?;
        match (registered, spawned) {
            (Ok(()), Ok(child)) => Ok(child),
            (Ok(()), Err(e)) => Err(e).with_context(|| format!("spawn {program}")),
            (Err(e), Ok(mut child)) => {
                let ended = child.try_wait().ok().flatten();
                kill_and_reap(&mut child);
                Err(match ended {
                    Some(status) => e.context(format!("the child had already ended: {status}")),
                    None => e,
                })
            }
            (Err(e), Err(se)) => Err(e.context(format!("spawn {program}: {se}"))),
        }
    })
}

/// libSystem's fork-child handler finishes libnotify's one-time setup, and
/// aborts the child (SIGKILL, `_os_once_gate_corruption_abort`) when another
/// thread was inside that setup at the fork. Completing it once before the
/// first fork leaves no setup in flight to inherit.
#[cfg(target_os = "macos")]
fn settle_fork_handlers() {
    unsafe extern "C" {
        fn notify_is_valid_token(val: libc::c_int) -> bool;
    }
    static ONCE: std::sync::Once = std::sync::Once::new();
    // SAFETY: a pure query on libnotify's process-wide state.
    ONCE.call_once(|| unsafe {
        notify_is_valid_token(0);
    });
}

#[cfg(not(target_os = "macos"))]
fn settle_fork_handlers() {}

/// The pid the gated child writes before it blocks, or `None` when the spawn
/// ended without one.
fn read_pid(
    fd: std::os::fd::RawFd,
    spawning: &thread::ScopedJoinHandle<'_, std::io::Result<Child>>,
) -> Option<libc::pid_t> {
    let mut buf = [0u8; 4];
    let mut got = 0;
    loop {
        let mut pfd = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: one valid pollfd.
        let ready = unsafe { libc::poll(&mut pfd, 1, 50) };
        if ready > 0 {
            // SAFETY: reads into the unfilled tail of `buf`.
            let n = unsafe { libc::read(fd, buf[got..].as_mut_ptr().cast(), buf.len() - got) };
            match n {
                n if n > 0 => {
                    got += n as usize;
                    if got == buf.len() {
                        return Some(libc::pid_t::from_ne_bytes(buf));
                    }
                }
                0 => return None,
                _ if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted => {}
                _ => return None,
            }
        } else if spawning.is_finished() {
            return None;
        }
    }
}

/// Runs in the child between fork and exec.
fn hold_at_gate(
    gate_r: libc::c_int,
    gate_w: libc::c_int,
    pid_r: libc::c_int,
    pid_w: libc::c_int,
) -> std::io::Result<()> {
    // SAFETY: async-signal-safe calls on descriptors inherited from the parent.
    unsafe {
        libc::close(gate_w);
        libc::close(pid_r);
        let me = libc::getpid().to_ne_bytes();
        if libc::write(pid_w, me.as_ptr().cast(), me.len()) != me.len() as isize {
            return Err(std::io::Error::last_os_error());
        }
        libc::close(pid_w);
        let mut byte = 0u8;
        loop {
            match libc::read(gate_r, (&mut byte as *mut u8).cast(), 1) {
                1 => {
                    libc::close(gate_r);
                    return Ok(());
                }
                n if n < 0
                    && std::io::Error::last_os_error().kind()
                        == std::io::ErrorKind::Interrupted => {}
                _ => return Err(std::io::Error::from_raw_os_error(libc::ECANCELED)),
            }
        }
    }
}

#[cfg(test)]
mod tests;
