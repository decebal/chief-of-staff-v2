//! `cos-stand-in`: one test binary standing in for a run child, `launchctl`
//! and `cos --version`. The first argument picks the mode: `run`,
//! `launchctl`, `version` (or `--version`), `stale-socket <path>` (a socket
//! file nothing listens on). Run through a link named `launchctl`, it is
//! `launchctl` mode whatever the arguments; `COS_STAND_IN_FAIL=<verb>` makes
//! that verb exit 5, and `COS_STAND_IN_BOOTOUT=<business>` makes `bootout`
//! start a brief of that business on the daemon in `COS_DAEMON_DIR`, log the
//! reply, then end that daemon. Any other first argument is read
//! as the argv the daemon and `cos mcp` give a run child
//! (`--home <h> --workspace <w> --projects <p> [--no-open] brief|ask -- <q>`),
//! so the stand-in can be `COS_RUN_BIN` as it is.
//!
//! Run mode environment: `COS_HOME` (when no `--home`), `COS_RUN_RECORD`,
//! `COS_OWNER=<pid>/<start>`, `COS_BUSINESS`, `COS_SLEEP_MS`,
//! `COS_GRANDCHILD_SECS`, `COS_GRANDCHILD_MARK`, `COS_GATE_HOLD_MS`,
//! `COS_EXIT_CODE`, `COS_IGNORE_TERM=1`, `COS_STAND_IN_NO_WATCHDOG` (no
//! owner watchdog even with `COS_OWNER`, as a run whose watchdog failed),
//! `COS_STAND_IN_DAY` and `COS_STAND_IN_COST` (a done brief's `day` and
//! `cost_usd`, as `cos brief` writes them once it saved that day's brief).

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::os::fd::AsRawFd;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use cos::proc::{group_alive, start_time};
use serde_json::{Map, Value, json};

const EXIT_OWNER_GONE: i32 = 76;
const EXIT_SIGNALLED: i32 = 143;
const GROUP_GRACE: Duration = Duration::from_secs(2);

fn main() {
    let argv0 = std::env::args_os().next().map(PathBuf::from);
    let args: Vec<String> = std::env::args().skip(1).collect();
    if argv0.as_deref().and_then(|p| p.file_name()) == Some("launchctl".as_ref()) {
        std::process::exit(launchctl(&args));
    }
    let code = match args.first().map(String::as_str) {
        Some("run") => run(Invocation::from_env(&args[1..])),
        Some("launchctl") => launchctl(&args[1..]),
        Some("version" | "--version") => version(),
        Some("stale-socket") => stale_socket(&args[1..]),
        Some(_) => run(Invocation::from_env(&args)),
        None => {
            eprintln!("cos-stand-in: run | launchctl | version");
            2
        }
    };
    std::process::exit(code);
}

fn launchctl(args: &[String]) -> i32 {
    let Some(log) = std::env::var_os("COS_STAND_IN_LOG") else {
        eprintln!("cos-stand-in launchctl: COS_STAND_IN_LOG unset");
        return 2;
    };
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log)
        .unwrap();
    writeln!(file, "{}", args.join(" ")).unwrap();
    if std::env::var("COS_STAND_IN_FAIL").ok().as_deref() == args.first().map(String::as_str) {
        eprintln!("cos-stand-in launchctl: {} failed", args.join(" "));
        return 5;
    }
    if args.first().map(String::as_str) == Some("bootout")
        && let Ok(business) = std::env::var("COS_STAND_IN_BOOTOUT")
        && let Some(dir) = std::env::var_os("COS_DAEMON_DIR")
    {
        bootout(
            &PathBuf::from(dir).join("daemon.sock"),
            &business,
            &mut file,
        );
    }
    0
}

/// `bootout` as launchd does it to the daemon on `sock`, with a `start` of a
/// `business` brief sent first and its reply logged as `start <business>:
/// <reply>`: SIGTERM, then wait until the socket stops answering.
fn bootout(sock: &std::path::Path, business: &str, log: &mut File) {
    let ask = |req: &Value| -> Option<Value> {
        use std::io::{BufRead, BufReader};
        let mut s = std::os::unix::net::UnixStream::connect(sock).ok()?;
        s.set_read_timeout(Some(Duration::from_secs(10))).ok()?;
        s.write_all(format!("{req}\n").as_bytes()).ok()?;
        let mut line = String::new();
        BufReader::new(s).read_line(&mut line).ok()?;
        serde_json::from_str(&line).ok()
    };
    let Some(pid) = ask(&json!({"op": "hello"})).and_then(|h| h["pid"].as_i64()) else {
        return;
    };
    let start = json!({"op": "start", "kind": "brief", "business": business,
                       "client_req": "00000000000b0070"});
    let reply = ask(&start).unwrap_or(Value::Null);
    writeln!(log, "start {business}: {reply}").unwrap();
    cos::proc::signal_pid(pid as libc::pid_t, libc::SIGTERM);
    let until = Instant::now() + Duration::from_secs(30);
    while Instant::now() < until && std::os::unix::net::UnixStream::connect(sock).is_ok() {
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Binds a unix socket at the path and exits, leaving its file behind. In a
/// process of its own, so no sibling can inherit the listener and keep it
/// answering.
fn stale_socket(args: &[String]) -> i32 {
    let Some(path) = args.first() else {
        eprintln!("cos-stand-in stale-socket <path>");
        return 2;
    };
    match std::os::unix::net::UnixListener::bind(path) {
        Ok(_) => 0,
        Err(e) => {
            eprintln!("cos-stand-in stale-socket {path}: {e}");
            1
        }
    }
}

fn version() -> i32 {
    let v = std::env::var("COS_STAND_IN_VERSION").unwrap_or_else(|_| "cos 0.0.0-stand-in".into());
    println!("{v}");
    0
}

struct Invocation {
    home: PathBuf,
    kind: String,
    question: Option<String>,
}

impl Invocation {
    /// cos's own argv after the mode; flags it does not know are skipped
    /// with their value.
    fn from_env(args: &[String]) -> Self {
        let mut home = std::env::var_os("COS_HOME").map(PathBuf::from);
        let mut kind = None;
        let mut question = None;
        let mut it = args.iter();
        while let Some(a) = it.next() {
            match a.as_str() {
                "--home" => home = it.next().map(PathBuf::from),
                "--workspace" | "--projects" | "--business" | "--budget" => {
                    it.next();
                }
                "brief" | "ask" if kind.is_none() => kind = Some(a.clone()),
                "--" => {
                    question = Some(it.by_ref().cloned().collect::<Vec<_>>().join(" "));
                }
                _ => {}
            }
        }
        Invocation {
            home: home.expect("--home or COS_HOME"),
            kind: kind.unwrap_or_else(|| "brief".into()),
            question,
        }
    }
}

struct Run {
    record: PathBuf,
    lock: PathBuf,
    groups: Mutex<Vec<(Child, String)>>,
    ending: Mutex<()>,
}

fn env_num<T: std::str::FromStr>(key: &str) -> Option<T> {
    std::env::var(key).ok().and_then(|v| v.parse().ok())
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

fn start_of(pid: libc::pid_t) -> Option<String> {
    start_time(pid).map(|(s, u)| format!("{s}.{u:06}"))
}

fn run(inv: Invocation) -> i32 {
    let signals = block_signals();
    let id = std::env::var("COS_RUN_RECORD").unwrap_or_else(|_| "r-20000101-000000-0000".into());
    let dir = inv.home.join("out/run-records");
    std::fs::create_dir_all(&dir).unwrap();
    let run: &'static Run = Box::leak(Box::new(Run {
        record: dir.join(format!("{id}.json")),
        lock: dir.join(format!("{id}.lock")),
        groups: Mutex::new(Vec::new()),
        ending: Mutex::new(()),
    }));

    let me = std::process::id() as libc::pid_t;
    let business = std::env::var("COS_BUSINESS").ok();
    run.write(|rec| {
        rec.insert("id".into(), json!(id));
        rec.insert("kind".into(), json!(inv.kind));
        if let Some(b) = &business {
            rec.insert("business".into(), json!(b));
        }
        rec.entry("business").or_insert(Value::Null);
        rec.entry("owner").or_insert(json!("self"));
        rec.insert("state".into(), json!("running"));
        rec.insert("reason".into(), Value::Null);
        rec.insert("pid".into(), json!(me));
        rec.insert("start".into(), json!(start_of(me)));
        rec.insert("groups".into(), json!([]));
        rec.insert("started".into(), json!(now()));
        rec.insert("ended".into(), Value::Null);
    });

    let ignore_term = std::env::var("COS_IGNORE_TERM").is_ok_and(|v| v == "1");
    std::thread::spawn(move || wait_signals(run, signals, ignore_term));
    if let Ok(spec) = std::env::var("COS_OWNER")
        && std::env::var("COS_STAND_IN_NO_WATCHDOG").is_err()
    {
        std::thread::spawn(move || watch_owner(run, &spec));
    }

    if let Some(secs) = env_num::<u64>("COS_GRANDCHILD_SECS") {
        run.spawn_grandchild(&inv.home, secs);
    }
    if let Some(ms) = env_num::<u64>("COS_SLEEP_MS") {
        std::thread::sleep(Duration::from_millis(ms));
    }

    let code = env_num::<i32>("COS_EXIT_CODE").unwrap_or(0);
    let answer = (inv.kind == "ask")
        .then(|| format!("stand-in answer: {}", inv.question.as_deref().unwrap_or("")));
    let _end = run.ending.lock().unwrap();
    run.write(|rec| {
        if code == 0 {
            rec.insert("state".into(), json!("done"));
        } else {
            rec.insert("state".into(), json!("failed"));
            rec.insert("reason".into(), json!(format!("exit {code}")));
        }
        if let Some(a) = &answer {
            rec.insert("answer".into(), json!(a));
        }
        if let (Ok(day), "brief") = (std::env::var("COS_STAND_IN_DAY"), inv.kind.as_str()) {
            rec.insert("day".into(), json!(day));
            rec.insert(
                "cost_usd".into(),
                json!(env_num::<f64>("COS_STAND_IN_COST").unwrap_or(0.0)),
            );
        }
        rec.insert("ended".into(), json!(now()));
    });
    code
}

impl Run {
    /// Applies `change` under the record's lock, unless the record is final.
    fn write(&self, change: impl FnOnce(&mut Map<String, Value>)) {
        let lock = File::create(&self.lock).unwrap();
        // SAFETY: flock on an fd this function owns for its whole call.
        unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX) };
        let mut rec: Map<String, Value> = std::fs::read_to_string(&self.record)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        let state = rec.get("state").and_then(Value::as_str).unwrap_or("queued");
        if !matches!(state, "queued" | "running") {
            return;
        }
        change(&mut rec);
        let tmp = self.record.with_extension("json.tmp");
        std::fs::write(&tmp, Value::Object(rec).to_string()).unwrap();
        std::fs::rename(&tmp, &self.record).unwrap();
    }

    /// A `sleep` leading its own group, with no stdio, so it neither holds the
    /// owner's pipes open nor dies with the run's group. It passes the spawn
    /// gate: `<home>/gate.pid` names it while it is held, `COS_GATE_HOLD_MS`
    /// holds it there before its group is recorded, and once it runs it
    /// creates `COS_GRANDCHILD_MARK`.
    fn spawn_grandchild(&self, home: &std::path::Path, secs: u64) {
        let mut cmd = match std::env::var_os("COS_GRANDCHILD_MARK") {
            Some(mark) => {
                let mut c = Command::new("/bin/sh");
                c.arg("-c")
                    .arg(r#"touch "$1"; exec sleep "$2""#)
                    .arg("sh")
                    .arg(mark)
                    .arg(secs.to_string());
                c
            }
            None => {
                let mut c = Command::new("sleep");
                c.arg(secs.to_string());
                c
            }
        };
        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0);
        let hold = env_num::<u64>("COS_GATE_HOLD_MS").unwrap_or(0);
        let mut groups = self.groups.lock().unwrap();
        let child = cos::proc::spawn_gated(&mut cmd, |g| {
            std::fs::write(home.join("gate.pid"), g.pgid.to_string())?;
            std::thread::sleep(Duration::from_millis(hold));
            let mut listed: Vec<Value> = groups
                .iter()
                .map(|(c, s)| json!({"pgid": c.id(), "start": s}))
                .collect();
            listed.push(json!({"pgid": g.pgid, "start": g.start}));
            self.write(|rec| {
                rec.insert("groups".into(), Value::Array(listed));
            });
            Ok(())
        })
        .unwrap();
        let start = start_of(child.id() as libc::pid_t).unwrap_or_default();
        groups.push((child, start));
    }

    /// Kills every recorded group, waits for each to empty, writes the final
    /// state and exits; the run's own group is the owner's to kill. A group
    /// still alive at the bound leaves the record as it is.
    fn end(&self, state: &str, reason: &str, code: i32) -> ! {
        let _end = self.ending.lock().unwrap();
        let mut groups = self.groups.lock().unwrap();
        for (child, _) in groups.iter() {
            cos::proc::kill_group(child.id() as libc::pid_t);
        }
        let until = Instant::now() + GROUP_GRACE;
        loop {
            for (child, _) in groups.iter_mut() {
                let _ = child.try_wait();
            }
            let live = groups
                .iter()
                .any(|(c, _)| group_alive(c.id() as libc::pid_t));
            if !live {
                break;
            }
            if Instant::now() >= until {
                std::process::exit(code);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        self.write(|rec| {
            rec.insert("state".into(), json!(state));
            rec.insert("reason".into(), json!(reason));
            rec.insert("ended".into(), json!(now()));
        });
        std::process::exit(code);
    }
}

/// Blocks SIGTERM, SIGINT and SIGHUP in this thread before any other thread
/// exists, so every thread inherits the mask and only `sigwait` sees them.
fn block_signals() -> libc::sigset_t {
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

fn wait_signals(run: &'static Run, set: libc::sigset_t, ignore_term: bool) {
    loop {
        let mut sig = 0;
        // SAFETY: `set` is an initialised signal set blocked in every thread.
        if unsafe { libc::sigwait(&set, &mut sig) } != 0 {
            continue;
        }
        if sig == libc::SIGTERM && ignore_term {
            continue;
        }
        let asked = std::fs::read_to_string(&run.record)
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .and_then(|v| {
                let e = &v["stopping"];
                Some((
                    e["state"].as_str()?.to_owned(),
                    e["reason"].as_str()?.to_owned(),
                ))
            });
        match asked {
            Some((state, reason)) => run.end(&state, &reason, EXIT_SIGNALLED),
            None => run.end("stopped", "stopped", EXIT_SIGNALLED),
        }
    }
}

fn watch_owner(run: &'static Run, spec: &str) {
    let (pid, start) = spec.split_once('/').unwrap_or((spec, ""));
    let pid: libc::pid_t = pid.parse().unwrap_or(0);
    loop {
        if start_of(pid).as_deref() != Some(start) {
            run.end("lost", "owner gone", EXIT_OWNER_GONE);
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}
