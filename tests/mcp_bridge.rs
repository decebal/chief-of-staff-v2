//! The MCP tools act on the business each call names, the server's pin, or the
//! registry's default read at that call. Local tools run over stdio through the
//! real `cos mcp`; the remote server runs through `cos serve` over HTTP.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use cos::reports::Brief;
use cos::store::{self, Kind};
use serde_json::{Value, json};

const COS: &str = env!("CARGO_BIN_EXE_cos");
const PROTOCOL: &str = "2025-06-18";
const TOKEN: &str = "0123456789abcdef0123456789abcdef";
const TIMEOUT: Duration = Duration::from_secs(20);
const DAY: &str = "2026-11-25";

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Two businesses, `acme` (the registry's default) and `beta`, each with a
/// saved brief whose markdown names it.
struct Fixture {
    root: PathBuf,
    registry: PathBuf,
    /// The daemon socket directory: short, so the socket path fits `sun_path`.
    daemon_dir: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let daemon_dir = std::env::temp_dir().join(format!("cosm-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&daemon_dir);
        let root = scratch(name);
        for biz in ["acme", "beta"] {
            let base = root.join(biz);
            for dir in ["home", "ws", "projects"] {
                std::fs::create_dir_all(base.join(dir)).unwrap();
            }
            store::save(
                &base.join("home"),
                Kind::Briefs,
                DAY,
                &brief(),
                &format!("# {biz} brief\n"),
            )
            .unwrap();
        }
        let registry = root.join("cfg/businesses.toml");
        std::fs::create_dir_all(registry.parent().unwrap()).unwrap();
        let mut text = String::from("active = \"acme\"\n");
        for biz in ["acme", "beta"] {
            let base = root.join(biz);
            text += &format!(
                "\n[business.{biz}]\nhome = \"{}\"\nworkspace = \"{}\"\nprojects = \"{}\"\n",
                base.join("home").display(),
                base.join("ws").display(),
                base.join("projects").display(),
            );
        }
        std::fs::write(&registry, text).unwrap();
        Self {
            root,
            registry,
            daemon_dir,
        }
    }

    fn home(&self, business: &str) -> PathBuf {
        self.root.join(business).join("home")
    }
}

fn brief() -> Brief {
    Brief {
        headline: String::new(),
        needs_you: vec![],
        what_changed: vec![],
        week: Default::default(),
        calendar: vec![],
        backlog: vec![],
        coverage: vec![],
        gaps: vec![],
        upcoming: vec![],
        tickets: vec![],
    }
}

/// A `cos mcp` child speaking newline-delimited JSON-RPC on stdio.
struct Mcp {
    child: Child,
    stdin: Option<ChildStdin>,
    replies: Receiver<Value>,
    next: i64,
}

impl Mcp {
    /// Starts `cos <flags> mcp` with the fixture's registry, its daemon
    /// directory and no business environment, and completes the handshake.
    fn start(fx: &Fixture, flags: &[&str]) -> Self {
        Self::start_with(fx, flags, &[])
    }

    fn start_with(fx: &Fixture, flags: &[&str], env: &[(&str, &str)]) -> Self {
        let mut cmd = Command::new(COS);
        cmd.args(flags)
            .arg("mcp")
            .env("COS_BUSINESSES", &fx.registry)
            .env("COS_DAEMON_DIR", &fx.daemon_dir)
            .env("COS_HC", fx.root.join("no-hc"))
            .env("HOME", fx.root.join("home"))
            .env_remove("COS_BUSINESS")
            .env_remove("COS_HOME")
            .env_remove("COS_WORKSPACE")
            .env_remove("COS_PROJECTS")
            .env_remove("COS_RUN_BIN")
            .env_remove("COS_OWNER")
            .env_remove("COS_RUN_ID")
            .env_remove("COS_RUN_RECORD")
            .env_remove("XDG_CONFIG_HOME")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        for (k, v) in env {
            cmd.env(k, v);
        }
        let mut child = cmd.spawn().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (tx, replies) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if let Ok(value) = serde_json::from_str::<Value>(&line)
                    && tx.send(value).is_err()
                {
                    break;
                }
            }
        });
        let stdin = child.stdin.take();
        let mut mcp = Self {
            child,
            stdin,
            replies,
            next: 1,
        };
        let init = mcp.request(
            "initialize",
            json!({
                "protocolVersion": PROTOCOL,
                "capabilities": {},
                "clientInfo": { "name": "cos-test", "version": "0" },
            }),
        );
        assert!(init.get("result").is_some(), "{init}");
        mcp.send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));
        mcp
    }

    fn send(&mut self, msg: Value) {
        let stdin = self.stdin.as_mut().unwrap();
        writeln!(stdin, "{msg}").unwrap();
        stdin.flush().unwrap();
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next;
        self.next += 1;
        self.send(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
        let until = Instant::now() + TIMEOUT;
        loop {
            let left = until.saturating_duration_since(Instant::now());
            let value = self
                .replies
                .recv_timeout(left)
                .unwrap_or_else(|e| panic!("no reply to {method}: {e}"));
            if value.get("id") == Some(&json!(id)) {
                return value;
            }
        }
    }

    fn call(&mut self, name: &str, args: Value) -> Value {
        self.request("tools/call", json!({ "name": name, "arguments": args }))
    }

    /// Closes stdin, as a client does when it quits, and reaps `cos mcp`.
    #[cfg(feature = "stand-in")]
    fn close(&mut self, within: Duration) -> std::process::ExitStatus {
        drop(self.stdin.take());
        let until = cos::proc::Deadline::after(within);
        cos::proc::wait_until(&mut self.child, &until)
            .unwrap_or_else(|e| panic!("cos mcp did not exit once its stdin closed: {e:#}"))
    }

    fn tools(&mut self) -> Vec<Value> {
        let reply = self.request("tools/list", json!({}));
        reply["result"]["tools"].as_array().unwrap().clone()
    }
}

impl Drop for Mcp {
    fn drop(&mut self) {
        drop(self.stdin.take());
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn structured(reply: &Value) -> &Value {
    reply
        .pointer("/result/structuredContent")
        .unwrap_or(&Value::Null)
}

fn is_error(reply: &Value) -> bool {
    reply.pointer("/result/isError") == Some(&json!(true))
}

fn text(reply: &Value) -> String {
    reply
        .pointer("/result/content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|c| c["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

fn markdown(reply: &Value) -> String {
    structured(reply)["markdown"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

fn tool<'a>(tools: &'a [Value], name: &str) -> &'a Value {
    tools
        .iter()
        .find(|t| t["name"] == name)
        .unwrap_or_else(|| panic!("no tool {name} in {tools:?}"))
}

#[test]
fn unpinned_server_reads_the_default_on_each_call() {
    let fx = Fixture::new("default-flip");
    let mut mcp = Mcp::start(&fx, &[]);
    assert!(markdown(&mcp.call("latest_brief", json!({}))).contains("acme brief"));
    cos::business::set_active(&fx.registry, "beta").unwrap();
    assert!(markdown(&mcp.call("latest_brief", json!({}))).contains("beta brief"));
    assert!(
        markdown(&mcp.call("latest_brief", json!({ "business": "acme" }))).contains("acme brief")
    );
}

#[test]
fn pinned_server_refuses_another_business_and_keeps_its_pin() {
    let fx = Fixture::new("pinned");
    let mut mcp = Mcp::start(&fx, &["--business", "acme"]);
    let refused = mcp.call("latest_brief", json!({ "business": "beta" }));
    assert!(is_error(&refused), "{refused}");
    let message = text(&refused);
    assert!(message.contains("pinned to acme"), "{message}");
    assert!(message.contains("--business"), "{message}");

    cos::business::set_active(&fx.registry, "beta").unwrap();
    assert!(markdown(&mcp.call("latest_brief", json!({}))).contains("acme brief"));
    assert!(
        markdown(&mcp.call("latest_brief", json!({ "business": "acme" }))).contains("acme brief")
    );
}

#[test]
fn unregistered_business_is_refused_naming_the_registered_ones() {
    let fx = Fixture::new("unregistered");
    let mut mcp = Mcp::start(&fx, &[]);
    let reply = mcp.call("latest_brief", json!({ "business": "ghost" }));
    assert!(is_error(&reply), "{reply}");
    let message = text(&reply);
    assert!(message.contains("ghost"), "{message}");
    assert!(message.contains("acme, beta"), "{message}");
}

#[test]
fn an_unpinned_server_with_no_registry_takes_back_the_label_its_runs_carry() {
    let fx = Fixture::new("no-registry");
    let missing = fx.root.join("cfg/none.toml");
    let mut mcp = Mcp::start_with(&fx, &[], &[("COS_BUSINESSES", missing.to_str().unwrap())]);
    let absent = "r-20261125-091502-zzzz";
    let status = mcp.call(
        "run_status",
        json!({ "run": absent, "business": "default" }),
    );
    assert!(is_error(&status), "{status}");
    let message = text(&status);
    assert!(
        message.contains(&format!("no run {absent} under business default")),
        "{message}"
    );
    let listed = mcp.call("runs", json!({ "business": "default", "limit": 1 }));
    assert!(!is_error(&listed), "{listed}");
    let ghost = mcp.call("runs", json!({ "business": "ghost" }));
    assert!(is_error(&ghost), "{ghost}");
    assert!(text(&ghost).contains("not registered"), "{}", text(&ghost));
}

#[test]
fn only_a_registered_home_is_left_to_the_daemon_to_follow() {
    let fx = Fixture::new("registered-home");
    let acme = fx.home("acme");
    assert!(cos::bridge::registered_home(&fx.registry, &acme));
    assert!(cos::bridge::registered_home(
        &fx.registry,
        &acme.join("..").join("home")
    ));
    let stray = fx.root.join("stray/home");
    std::fs::create_dir_all(&stray).unwrap();
    assert!(!cos::bridge::registered_home(&fx.registry, &stray));
    assert!(!cos::bridge::registered_home(
        &fx.root.join("cfg/none.toml"),
        &acme
    ));
}

/// A stand-in daemon socket in `dir`: each connection's request line is kept,
/// and `answer` says what the n-th connection gets back (`None`: closed with
/// no reply). Stops accepting after `within`.
fn stub_daemon(
    dir: &Path,
    answer: impl Fn(usize) -> Option<Value> + Send + 'static,
    within: Duration,
) -> std::thread::JoinHandle<Vec<Value>> {
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::UnixListener;
    let _ = std::fs::remove_dir_all(dir);
    std::fs::create_dir_all(dir).unwrap();
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    let listener = UnixListener::bind(dir.join("daemon.sock")).unwrap();
    listener.set_nonblocking(true).unwrap();
    std::thread::spawn(move || {
        let until = Instant::now() + within;
        let mut seen = Vec::new();
        while Instant::now() < until {
            let Ok((stream, _)) = listener.accept() else {
                std::thread::sleep(Duration::from_millis(10));
                continue;
            };
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut line = String::new();
            BufReader::new(&stream).read_line(&mut line).unwrap();
            seen.push(serde_json::from_str::<Value>(&line).unwrap());
            if let Some(reply) = answer(seen.len()) {
                (&stream)
                    .write_all(format!("{reply}\n").as_bytes())
                    .unwrap();
            }
        }
        seen
    })
}

#[test]
fn a_lost_start_reply_is_asked_once_more_with_the_same_client_req() {
    let dir = std::env::temp_dir().join(format!("cosm-lost-{}", std::process::id()));
    let run = "r-20261125-091502-k3x9";
    let stub = stub_daemon(
        &dir,
        move |n| (n == 2).then(|| json!({ "run": run, "existing": false })),
        Duration::from_secs(3),
    );
    let started = cos::daemon::client::start(&dir, cos::runs::Kind::Brief, "acme", None);
    let seen = stub.join().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(started.unwrap(), (run.to_owned(), false));
    assert_eq!(seen.len(), 2, "{seen:?}");
    assert_eq!(seen[0], seen[1], "the retry changed the request");
    assert_eq!(seen[0]["client_req"].as_str().map(str::len), Some(16));
}

#[test]
fn a_start_whose_reply_is_lost_twice_is_an_error_and_never_a_third_ask() {
    let dir = std::env::temp_dir().join(format!("cosm-lost2-{}", std::process::id()));
    let stub = stub_daemon(&dir, |_| None, Duration::from_secs(3));
    let started = cos::daemon::client::start(&dir, cos::runs::Kind::Ask, "acme", Some("why?"));
    let seen = stub.join().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    let e = format!("{:#}", started.unwrap_err());
    assert!(e.contains("asked twice"), "{e}");
    assert_eq!(seen.len(), 2, "{seen:?}");
    assert_eq!(seen[0], seen[1]);
}

#[test]
fn a_daemon_directory_others_can_write_is_never_trusted_with_a_question() {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!("cosm-open-{}", std::process::id()));
    let stub = stub_daemon(
        &dir,
        |_| Some(json!({ "version": "x", "pid": 1, "run": "r-20261128-000000-aaaa" })),
        Duration::from_secs(2),
    );
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o777)).unwrap();
    let probed = cos::daemon::client::probe(&dir);
    let started = cos::daemon::client::start(
        &dir,
        cos::runs::Kind::Ask,
        "acme",
        Some("a private question"),
    );
    let seen = stub.join().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    let probed = format!("{:#}", probed.unwrap_err());
    assert!(probed.contains("writable"), "{probed}");
    assert!(started.is_err());
    assert!(seen.is_empty(), "a request reached the socket: {seen:?}");
}

#[test]
fn spin_offs_hint_names_the_business_the_hire_was_made_for() {
    let triple = |business: Option<&str>| cos::business::Triple {
        home: "/tmp/h".into(),
        workspace: "/tmp/w".into(),
        projects: "/tmp/p".into(),
        business: business.map(str::to_owned),
        layer: cos::business::Layer::Registry,
    };
    assert_eq!(
        cos::mcp::say_command(&triple(Some("beta")), "20261128-101500-ab12"),
        "cos --business beta agents say 20261128-101500-ab12 \"…\""
    );
    assert_eq!(
        cos::mcp::say_command(&triple(None), "20261128-101500-ab12"),
        "cos --home /tmp/h --workspace /tmp/w --projects /tmp/p agents say 20261128-101500-ab12 \"…\""
    );
}

#[test]
fn business_with_a_missing_directory_is_refused_by_name() {
    let fx = Fixture::new("missing-dir");
    std::fs::remove_dir_all(fx.root.join("beta/projects")).unwrap();
    let mut mcp = Mcp::start(&fx, &[]);
    let reply = mcp.call("latest_brief", json!({ "business": "beta" }));
    assert!(is_error(&reply), "{reply}");
    let message = text(&reply);
    assert!(message.contains("business \"beta\""), "{message}");
    assert!(message.contains("is not a directory"), "{message}");
    assert!(message.contains("cos business add"), "{message}");
}

#[test]
fn businesses_lists_each_with_its_default_and_the_pin() {
    let fx = Fixture::new("businesses");
    let mut mcp = Mcp::start(&fx, &[]);
    let listed = mcp.call("businesses", json!({}));
    assert_eq!(structured(&listed)["pinned"], Value::Null, "{listed}");
    let rows = structured(&listed)["businesses"]
        .as_array()
        .unwrap()
        .clone();
    let names: Vec<&str> = rows.iter().map(|r| r["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["acme", "beta"]);
    assert_eq!(rows[0]["default"], json!(true));
    assert_eq!(rows[1]["default"], json!(false));
    assert!(
        rows[1]["home"].as_str().unwrap().ends_with("beta/home"),
        "{listed}"
    );

    let mut pinned = Mcp::start(&fx, &["--business", "beta"]);
    let listed = pinned.call("businesses", json!({}));
    assert_eq!(structured(&listed)["pinned"], json!("beta"), "{listed}");
}

#[test]
fn spin_off_bounds_are_refused_before_any_hire() {
    let fx = Fixture::new("spin-off-bounds");
    let mut mcp = Mcp::start(&fx, &[]);
    let eleven: Vec<String> = (0..11).map(|_| "true".to_owned()).collect();
    let cases = [
        (
            json!({ "from": "brief", "item": 1, "checks": eleven }),
            "checks",
        ),
        (
            json!({ "from": "brief", "item": 1, "scope": ["/etc/hosts"] }),
            "scope",
        ),
        (
            json!({ "from": "brief", "item": 1, "scope": ["src/../../x"] }),
            "scope",
        ),
        (json!({ "from": "brief", "item": 0 }), "item"),
        (
            json!({ "from": "brief", "item": 1, "role": "Builder" }),
            "role",
        ),
        (
            json!({ "from": "brief", "item": 1, "role": "a".repeat(41) }),
            "role",
        ),
    ];
    for (args, field) in cases {
        let reply = mcp.call("spin_off", args.clone());
        assert!(is_error(&reply), "{args} was admitted: {reply}");
        assert!(text(&reply).contains(field), "{args}: {}", text(&reply));
    }
}

#[test]
fn paid_tools_are_not_read_only_and_the_saved_brief_is() {
    let fx = Fixture::new("annotations");
    let mut mcp = Mcp::start(&fx, &[]);
    let tools = mcp.tools();
    let read_only = |name: &str| tool(&tools, name)["annotations"]["readOnlyHint"].clone();
    assert_eq!(read_only("brief"), json!(false));
    assert_eq!(read_only("ask"), json!(false));
    assert_eq!(read_only("latest_brief"), json!(true));
    assert_eq!(read_only("businesses"), json!(true));
    for name in ["run_status", "runs"] {
        assert_eq!(read_only(name), json!(true), "{name}");
        assert_eq!(
            tool(&tools, name)["annotations"]["openWorldHint"],
            json!(false),
            "{name}"
        );
        assert!(
            tool(&tools, name)["inputSchema"]["properties"]["business"].is_object(),
            "{name}"
        );
    }
    assert!(tool(&tools, "latest_brief")["inputSchema"]["properties"]["business"].is_object());
    let brief = tool(&tools, "brief")["description"]
        .as_str()
        .unwrap_or_default();
    assert!(brief.contains("existing"), "{brief}");
}

#[test]
fn run_tools_refuse_out_of_range_arguments() {
    let fx = Fixture::new("run-bounds");
    let mut mcp = Mcp::start(&fx, &[]);
    let cases = [
        (
            "run_status",
            json!({ "run": "r-20261125-091502-k3x9", "wait_secs": 51 }),
            "wait_secs",
        ),
        ("run_status", json!({ "run": "../etc/passwd" }), "run id"),
        ("runs", json!({ "limit": 21 }), "limit"),
        ("runs", json!({ "limit": 0 }), "limit"),
        ("ask", json!({ "question": "" }), "question"),
        ("ask", json!({ "question": "x".repeat(4001) }), "question"),
    ];
    for (name, args, field) in cases {
        let reply = mcp.call(name, args.clone());
        assert!(is_error(&reply), "{name} {args} was admitted: {reply}");
        assert!(
            text(&reply).contains(field),
            "{name} {args}: {}",
            text(&reply)
        );
    }
    assert!(
        !fx.home("acme").join("out/run-records").exists(),
        "a refused call wrote a run record"
    );
}

#[test]
fn run_status_shows_a_dead_run_lost_and_writes_nothing() {
    let fx = Fixture::new("reader-lost");
    let home = fx.home("acme");
    let id = "r-20261125-091502-k3x9";
    let mut r =
        cos::runs::Record::new(id, cos::runs::Kind::Brief, "acme", cos::runs::Owner::Itself);
    r.state = cos::runs::State::Running;
    r.pid = Some(i32::MAX - 7);
    r.start = Some("1.000001".into());
    let dir = home.join(cos::runs::DIR);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join(format!("{id}.json"));
    std::fs::write(&file, serde_json::to_vec(&r).unwrap()).unwrap();
    let before = std::fs::read(&file).unwrap();

    let mut mcp = Mcp::start(&fx, &[]);
    let shown = mcp.call("run_status", json!({ "run": id }));
    assert!(!is_error(&shown), "{shown}");
    let view = structured(&shown);
    assert_eq!(view["state"], json!("lost"), "{shown}");
    assert_eq!(
        view["link"],
        json!(format!("cos://run/acme/{id}")),
        "{shown}"
    );
    assert_eq!(std::fs::read(&file).unwrap(), before, "a reader wrote");

    let listed = mcp.call("runs", json!({}));
    let rows = structured(&listed)["runs"].as_array().unwrap().clone();
    assert_eq!(rows.len(), 1, "{listed}");
    assert_eq!(rows[0]["state"], json!("lost"), "{listed}");

    let missing = mcp.call("run_status", json!({ "run": "r-20261125-091502-zzzz" }));
    assert!(is_error(&missing), "{missing}");
}

/// The source-level guard; `tests/mcp_view.rs` runs the view in Chrome.
#[test]
fn the_view_source_polls_run_status_on_the_runs_business() {
    let html = include_str!("../src/dashboard.html");
    assert!(
        html.contains("\"run_status\""),
        "the view never calls run_status"
    );
    assert!(
        html.contains("{ run, business, wait_secs: 45 }"),
        "the view does not wait 45 s on the run's own business"
    );
    assert!(html.contains("Running…"), "the view shows no running state");
    assert!(
        html.contains("FINAL"),
        "the view does not tell a final RunView from a running one"
    );
}

#[cfg(feature = "stand-in")]
const STAND_IN: &str = env!("CARGO_BIN_EXE_cos-stand-in");

#[cfg(feature = "stand-in")]
const BOUND: Duration = Duration::from_secs(60);

#[cfg(feature = "stand-in")]
fn eventually<T>(what: &str, within: Duration, mut probe: impl FnMut() -> Option<T>) -> T {
    let until = Instant::now() + within;
    loop {
        if let Some(v) = probe() {
            return v;
        }
        assert!(Instant::now() < until, "never: {what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// `cos daemon` on the fixture's socket directory with the stand-in as its
/// run program: SIGTERM, then SIGKILL, then reaped on drop.
#[cfg(feature = "stand-in")]
struct Daemon(Option<Child>);

#[cfg(feature = "stand-in")]
impl Daemon {
    fn start(fx: &Fixture, env: &[(&str, &str)]) -> Self {
        let log = std::fs::File::create(fx.root.join("daemon.log")).unwrap();
        let mut cmd = Command::new(COS);
        cmd.arg("daemon")
            .env("COS_DAEMON_DIR", &fx.daemon_dir)
            .env("COS_BUSINESSES", &fx.registry)
            .env("COS_RUN_BIN", STAND_IN)
            .env("COS_HC", fx.root.join("no-hc"))
            .env("HOME", fx.root.join("home"))
            .env_remove("COS_BUSINESS")
            .env_remove("COS_HOME")
            .env_remove("COS_WORKSPACE")
            .env_remove("COS_PROJECTS")
            .env_remove("COS_OWNER")
            .env_remove("COS_RUN_ID")
            .env_remove("COS_RUN_RECORD")
            .stdin(Stdio::null())
            .stdout(Stdio::from(log.try_clone().unwrap()))
            .stderr(Stdio::from(log));
        for (k, v) in env {
            cmd.env(k, v);
        }
        let d = Daemon(Some(cos::proc::spawn_grouped(&mut cmd).unwrap()));
        let sock = fx.daemon_dir.join("daemon.sock");
        eventually("the daemon answers hello", BOUND, || {
            cos::daemon::call(&sock, &json!({"op": "hello"}), Duration::from_secs(2)).ok()
        });
        d
    }
}

#[cfg(feature = "stand-in")]
impl Drop for Daemon {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            cos::proc::signal_pid(child.id() as libc::pid_t, libc::SIGTERM);
            let within = cos::proc::Deadline::after(Duration::from_secs(30));
            if cos::proc::wait_until(&mut child, &within).is_err() {
                cos::proc::kill_and_reap(&mut child);
            }
        }
    }
}

/// Kills a group a test saw, so a failing test leaks nothing.
#[cfg(feature = "stand-in")]
struct Reaper(cos::proc::Group);

#[cfg(feature = "stand-in")]
impl Drop for Reaper {
    fn drop(&mut self) {
        cos::proc::kill_recorded(&self.0);
    }
}

#[cfg(feature = "stand-in")]
fn record(home: &Path, id: &str) -> cos::runs::Record {
    cos::runs::read(home, id).unwrap().expect("record")
}

/// What the MCP App view does after Run new brief: `run_status` with
/// `wait_secs: 45` until the state is final.
#[cfg(feature = "stand-in")]
fn follow_like_the_view(mcp: &mut Mcp, started: &Value) -> Value {
    let run = started["run"].as_str().unwrap();
    let args = json!({ "run": run, "business": started["business"], "wait_secs": 45 });
    let until = Instant::now() + BOUND;
    loop {
        let reply = mcp.call("run_status", args.clone());
        assert!(!is_error(&reply), "{reply}");
        let view = structured(&reply).clone();
        if !matches!(view["state"].as_str(), Some("queued" | "running")) {
            return view;
        }
        assert!(Instant::now() < until, "{run} never ended: {view}");
    }
}

#[cfg(feature = "stand-in")]
#[test]
fn brief_answers_at_once_through_the_daemon_and_run_status_returns_the_report() {
    let fx = Fixture::new("via-daemon");
    let _daemon = Daemon::start(
        &fx,
        &[
            ("COS_SLEEP_MS", "2500"),
            ("COS_STAND_IN_DAY", DAY),
            ("COS_STAND_IN_COST", "0.42"),
        ],
    );
    let mut mcp = Mcp::start(&fx, &[]);

    let asked = Instant::now();
    let first = mcp.call("brief", json!({}));
    assert!(
        asked.elapsed() < Duration::from_secs(2),
        "brief took {:?}",
        asked.elapsed()
    );
    assert!(!is_error(&first), "{first}");
    let view = structured(&first).clone();
    let run = view["run"].as_str().expect("a run id").to_owned();
    assert!(cos::runs::is_id(&run), "{view}");
    assert_eq!(view["link"], json!(format!("cos://run/acme/{run}")));
    assert_eq!(view["business"], json!("acme"));
    assert_eq!(view["kind"], json!("brief"));
    assert_eq!(view["existing"], json!(false), "{view}");
    assert!(
        matches!(view["state"].as_str(), Some("queued" | "running")),
        "{view}"
    );
    let home = fx.home("acme");
    assert_eq!(record(&home, &run).owner, cos::runs::Owner::Daemon);

    let second = mcp.call("brief", json!({}));
    assert_eq!(structured(&second)["run"], json!(run), "{second}");
    assert_eq!(structured(&second)["existing"], json!(true), "{second}");

    cos::business::set_active(&fx.registry, "beta").unwrap();
    let done = follow_like_the_view(&mut mcp, &view);
    assert_eq!(done["state"], json!("done"), "{done}");
    assert_eq!(done["day"], json!(DAY), "{done}");
    assert_eq!(done["cost_usd"], json!(0.42), "{done}");
    assert!(
        done["markdown"]
            .as_str()
            .unwrap_or_default()
            .contains("acme brief"),
        "{done}"
    );
    let report = &done["report"];
    for field in [
        "headline",
        "needs_you",
        "what_changed",
        "calendar",
        "backlog",
        "coverage",
        "gaps",
        "week",
    ] {
        assert!(
            report.get(field).is_some(),
            "the view reads {field}: {done}"
        );
    }

    let listed = mcp.call("runs", json!({ "business": "acme", "limit": 1 }));
    let rows = structured(&listed)["runs"].as_array().unwrap().clone();
    assert_eq!(rows.len(), 1, "{listed}");
    assert_eq!(rows[0]["run"], json!(run), "{listed}");
}

#[cfg(feature = "stand-in")]
#[test]
fn ask_through_the_daemon_returns_its_answer_once_done() {
    let fx = Fixture::new("ask-daemon");
    let _daemon = Daemon::start(&fx, &[]);
    let mut mcp = Mcp::start(&fx, &[]);
    let asked = mcp.call(
        "ask",
        json!({ "question": "what slipped?", "business": "beta" }),
    );
    assert!(!is_error(&asked), "{asked}");
    let run = structured(&asked)["run"].as_str().unwrap().to_owned();
    assert_eq!(
        structured(&asked)["link"],
        json!(format!("cos://run/beta/{run}"))
    );
    assert_eq!(
        record(&fx.home("beta"), &run).owner,
        cos::runs::Owner::Daemon
    );
    let done = follow_like_the_view(&mut mcp, structured(&asked));
    assert_eq!(done["state"], json!("done"), "{done}");
    assert_eq!(done["kind"], json!("ask"), "{done}");
    assert_eq!(
        done["answer"],
        json!("stand-in answer: what slipped?"),
        "{done}"
    );
    assert!(done.get("report").is_none(), "{done}");
}

#[cfg(feature = "stand-in")]
#[test]
fn with_no_daemon_cos_mcp_owns_the_run_and_closing_stdin_stops_it() {
    let fx = Fixture::new("no-daemon");
    let mut mcp = Mcp::start_with(
        &fx,
        &[],
        &[
            ("COS_RUN_BIN", STAND_IN),
            ("COS_SLEEP_MS", "60000"),
            ("COS_GRANDCHILD_SECS", "60"),
        ],
    );
    let asked = Instant::now();
    let reply = mcp.call("brief", json!({}));
    assert!(
        asked.elapsed() < Duration::from_secs(2),
        "brief took {:?}",
        asked.elapsed()
    );
    assert!(!is_error(&reply), "{reply}");
    let run = structured(&reply)["run"].as_str().unwrap().to_owned();
    let home = fx.home("acme");

    let r = eventually("a running record with groups", BOUND, || {
        Some(record(&home, &run))
            .filter(|r| r.state == cos::runs::State::Running && !r.groups.is_empty())
    });
    assert_eq!(r.owner, cos::runs::Owner::Mcp);
    assert_eq!(r.owner_pid, Some(mcp.child.id() as libc::pid_t));
    let mut groups = r.groups.clone();
    groups.push(cos::proc::Group {
        pgid: r.pid.unwrap(),
        start: r.start.clone().unwrap(),
    });
    let _reapers: Vec<Reaper> = groups.iter().cloned().map(Reaper).collect();

    let status = mcp.close(Duration::from_secs(30));
    assert!(status.success(), "cos mcp exited {status}");
    let r = record(&home, &run);
    assert_eq!(r.state, cos::runs::State::Stopped, "{r:?}");
    for g in &groups {
        assert!(
            !cos::proc::group_alive(g.pgid),
            "group {} outlived cos mcp",
            g.pgid
        );
    }
}

#[cfg(feature = "stand-in")]
#[test]
fn with_no_daemon_cos_mcp_applies_the_daemons_caps_to_its_own_runs() {
    let fx = Fixture::new("own-caps");
    let mut mcp = Mcp::start_with(
        &fx,
        &[],
        &[("COS_RUN_BIN", STAND_IN), ("COS_SLEEP_MS", "60000")],
    );
    let ask = |mcp: &mut Mcp, business: &str| {
        mcp.call(
            "ask",
            json!({ "question": "what slipped?", "business": business }),
        )
    };
    for _ in 0..2 {
        let reply = ask(&mut mcp, "acme");
        assert!(!is_error(&reply), "{reply}");
        let run = structured(&reply)["run"].as_str().unwrap().to_owned();
        assert_eq!(
            structured(&reply)["link"],
            json!(format!("cos://run/acme/{run}"))
        );
    }
    let third = ask(&mut mcp, "acme");
    assert!(is_error(&third), "{third}");
    assert!(
        text(&third).contains("2 asks are live for acme"),
        "{}",
        text(&third)
    );
    for business in ["acme", "beta"] {
        let reply = mcp.call("brief", json!({ "business": business }));
        assert!(!is_error(&reply), "{reply}");
    }
    let fifth = ask(&mut mcp, "beta");
    assert!(is_error(&fifth), "{fifth}");
    assert!(text(&fifth).contains("4 runs are live"), "{}", text(&fifth));
    let count = |b: &str| {
        std::fs::read_dir(fx.home(b).join(cos::runs::DIR))
            .map(|d| {
                d.filter_map(Result::ok)
                    .filter(|f| f.path().extension().is_some_and(|x| x == "json"))
                    .count()
            })
            .unwrap_or(0)
    };
    assert_eq!(count("acme") + count("beta"), 4);
    let status = mcp.close(Duration::from_secs(30));
    assert!(status.success(), "cos mcp exited {status}");
}

#[cfg(feature = "stand-in")]
#[test]
fn with_no_daemon_finished_owned_runs_stop_counting_against_the_caps() {
    let fx = Fixture::new("own-caps-done");
    let mut mcp = Mcp::start_with(
        &fx,
        &[],
        &[("COS_RUN_BIN", STAND_IN), ("COS_SLEEP_MS", "3000")],
    );
    let ask = |mcp: &mut Mcp| {
        mcp.call(
            "ask",
            json!({ "question": "what slipped?", "business": "acme" }),
        )
    };
    let mut first = Vec::new();
    for _ in 0..2 {
        let reply = ask(&mut mcp);
        assert!(!is_error(&reply), "{reply}");
        first.push(structured(&reply)["run"].as_str().unwrap().to_owned());
    }
    let third = ask(&mut mcp);
    assert!(text(&third).contains("2 asks are live for acme"), "{third}");
    for run in &first {
        eventually("the ask is done", BOUND, || {
            Some(()).filter(|()| record(&fx.home("acme"), run).state.is_final())
        });
    }
    let again = eventually("an ask is admitted once both ended", BOUND, || {
        let reply = ask(&mut mcp);
        if is_error(&reply) {
            assert!(text(&reply).contains("2 asks are live"), "{reply}");
            return None;
        }
        Some(reply)
    });
    assert!(structured(&again)["run"].is_string(), "{again}");
    let status = mcp.close(Duration::from_secs(30));
    assert!(status.success(), "cos mcp exited {status}");
}

#[cfg(feature = "stand-in")]
#[test]
fn a_run_of_an_unregistered_triple_carries_no_link() {
    let fx = Fixture::new("no-link");
    let home = fx.home("acme");
    let ws = fx.root.join("acme/ws");
    let projects = fx.root.join("acme/projects");
    let mut mcp = Mcp::start_with(
        &fx,
        &[
            "--home",
            home.to_str().unwrap(),
            "--workspace",
            ws.to_str().unwrap(),
            "--projects",
            projects.to_str().unwrap(),
        ],
        &[("COS_RUN_BIN", STAND_IN)],
    );
    let asked = mcp.call("ask", json!({ "question": "what slipped?" }));
    assert!(!is_error(&asked), "{asked}");
    let view = structured(&asked).clone();
    assert!(view.get("link").is_none(), "{view}");
    assert_eq!(view["business"], json!("explicit"), "{view}");
    let run = view["run"].as_str().unwrap().to_owned();
    assert_eq!(record(&home, &run).business, "explicit");
    let done = follow_like_the_view(&mut mcp, &view);
    assert_eq!(done["state"], json!("done"), "{done}");
    assert!(done.get("link").is_none(), "{done}");
    let listed = mcp.call("runs", json!({}));
    let rows = structured(&listed)["runs"].as_array().unwrap().clone();
    assert!(rows.iter().all(|r| r.get("link").is_none()), "{listed}");
}

/// Removes every file of one run from a run-records directory.
#[cfg(feature = "stand-in")]
struct RunFiles(PathBuf, String);

#[cfg(feature = "stand-in")]
impl Drop for RunFiles {
    fn drop(&mut self) {
        let Ok(entries) = std::fs::read_dir(&self.0) else {
            return;
        };
        for f in entries.filter_map(Result::ok) {
            if f.file_name().to_string_lossy().starts_with(&self.1) {
                let _ = std::fs::remove_file(f.path());
            }
        }
    }
}

/// With no registry the triple is the built-in defaults, whose home is this
/// crate's directory: the run's record lands under its `out/` and is removed.
#[cfg(feature = "stand-in")]
#[test]
fn a_run_on_the_built_in_defaults_carries_no_link_and_the_default_label() {
    let fx = Fixture::new("defaults-link");
    let missing = fx.root.join("cfg/none.toml");
    std::fs::create_dir_all(fx.root.join("home/Projects")).unwrap();
    let mut mcp = Mcp::start_with(
        &fx,
        &[],
        &[
            ("COS_BUSINESSES", missing.to_str().unwrap()),
            ("COS_RUN_BIN", STAND_IN),
        ],
    );
    let asked = mcp.call("ask", json!({ "question": "what slipped?" }));
    assert!(!is_error(&asked), "{asked}");
    let view = structured(&asked).clone();
    let run = view["run"].as_str().unwrap().to_owned();
    let home = Path::new(env!("CARGO_MANIFEST_DIR"))
        .canonicalize()
        .unwrap();
    let _files = RunFiles(home.join(cos::runs::DIR), run.clone());
    assert!(view.get("link").is_none(), "{view}");
    assert_eq!(view["business"], json!("default"), "{view}");
    assert_eq!(record(&home, &run).business, "default");
    let done = follow_like_the_view(&mut mcp, &view);
    assert_eq!(done["state"], json!("done"), "{done}");
    assert!(done.get("link").is_none(), "{done}");
}

#[cfg(feature = "stand-in")]
#[test]
fn a_probe_error_other_than_refused_is_an_error_and_spends_nothing() {
    use std::os::unix::fs::PermissionsExt;
    let fx = Fixture::new("probe-error");
    std::fs::create_dir_all(&fx.daemon_dir).unwrap();
    std::fs::set_permissions(&fx.daemon_dir, std::fs::Permissions::from_mode(0o000)).unwrap();
    let mut mcp = Mcp::start_with(&fx, &[], &[("COS_RUN_BIN", STAND_IN)]);
    let reply = mcp.call("brief", json!({}));
    std::fs::set_permissions(&fx.daemon_dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(is_error(&reply), "{reply}");
    assert!(text(&reply).contains("daemon"), "{}", text(&reply));
    assert!(
        !fx.home("acme").join(cos::runs::DIR).exists(),
        "a probe error fell back to a paid run"
    );
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// `cos serve` on a free port, killed and reaped on drop.
struct Remote {
    child: Child,
    addr: String,
    session: Option<String>,
}

impl Drop for Remote {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct Reply {
    status: u16,
    session: Option<String>,
    messages: Vec<Value>,
}

impl Remote {
    fn start(fx: &Fixture) -> Self {
        let port = free_port();
        let addr = format!("127.0.0.1:{port}");
        let child = Command::new(COS)
            .args(["serve", "--addr", &addr, "--token", TOKEN])
            .env("COS_BUSINESSES", &fx.registry)
            .env("HOME", fx.root.join("home"))
            .env_remove("COS_BUSINESS")
            .env_remove("COS_HOME")
            .env_remove("COS_WORKSPACE")
            .env_remove("COS_PROJECTS")
            .env_remove("XDG_CONFIG_HOME")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut remote = Self {
            child,
            addr,
            session: None,
        };
        let until = Instant::now() + TIMEOUT;
        while TcpStream::connect(&remote.addr).is_err() {
            assert!(
                Instant::now() < until,
                "cos serve never listened on {}",
                remote.addr
            );
            std::thread::sleep(Duration::from_millis(50));
        }
        let init = remote.post(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": PROTOCOL,
                "capabilities": {},
                "clientInfo": { "name": "cos-test", "version": "0" },
            },
        }));
        assert_eq!(init.status, 200, "{:?}", init.messages);
        remote.session = init.session;
        assert!(
            init.messages.iter().any(|m| m.get("result").is_some()),
            "{:?}",
            init.messages
        );
        let initialized =
            remote.post(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));
        assert_eq!(initialized.status, 202);
        remote
    }

    fn post(&self, body: Value) -> Reply {
        let mut sock = TcpStream::connect(&self.addr).unwrap();
        sock.set_read_timeout(Some(TIMEOUT)).unwrap();
        let payload = body.to_string();
        let mut head = format!(
            "POST /mcp/{TOKEN} HTTP/1.1\r\nHost: {addr}\r\nContent-Type: application/json\r\n\
             Accept: application/json, text/event-stream\r\nContent-Length: {len}\r\nConnection: close\r\n",
            addr = self.addr,
            len = payload.len(),
        );
        if let Some(session) = &self.session {
            head += &format!("mcp-session-id: {session}\r\nMCP-Protocol-Version: {PROTOCOL}\r\n");
        }
        sock.write_all(format!("{head}\r\n{payload}").as_bytes())
            .unwrap();
        let mut raw = Vec::new();
        sock.read_to_end(&mut raw).unwrap();
        let text = String::from_utf8_lossy(&raw).into_owned();
        let (head, rest) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
        let status = head
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|code| code.parse().ok())
            .unwrap_or(0);
        let session = head.lines().find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.eq_ignore_ascii_case("mcp-session-id")
                .then(|| value.trim().to_owned())
        });
        let messages = if rest.trim_start().starts_with('{') {
            serde_json::from_str(rest.trim()).into_iter().collect()
        } else {
            rest.lines()
                .filter_map(|line| line.strip_prefix("data:"))
                .filter_map(|data| serde_json::from_str(data.trim()).ok())
                .collect()
        };
        Reply {
            status,
            session,
            messages,
        }
    }

    /// The JSON-RPC reply to request `id`.
    fn reply(&self, id: i64, body: Value) -> Value {
        let reply = self.post(body);
        assert_eq!(reply.status, 200, "{:?}", reply.messages);
        let found = reply
            .messages
            .iter()
            .find(|m| m.get("id") == Some(&json!(id)))
            .cloned();
        found.unwrap_or_else(|| panic!("no reply {id} in {:?}", reply.messages))
    }

    fn request(&self, id: i64, method: &str, params: Value) -> Value {
        self.reply(
            id,
            json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }),
        )
    }
}

#[test]
fn remote_server_declares_latest_brief_only_and_refuses_arguments_before_dispatch() {
    let fx = Fixture::new("remote");
    let remote = Remote::start(&fx);
    assert!(
        remote.session.is_some(),
        "initialize returned no session id"
    );

    let listed = remote.request(2, "tools/list", json!({}));
    let tools = listed["result"]["tools"].as_array().unwrap().clone();
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["latest_brief"]);
    assert!(
        tool(&tools, "latest_brief")["inputSchema"]["properties"]
            .get("business")
            .is_none()
    );

    let fresh = remote.request(
        3,
        "tools/call",
        json!({ "name": "latest_brief", "arguments": {} }),
    );
    assert!(markdown(&fresh).contains("acme brief"), "{fresh}");

    let named = remote.request(
        4,
        "tools/call",
        json!({ "name": "latest_brief", "arguments": { "business": "beta" } }),
    );
    assert_eq!(named["error"]["code"], json!(-32602), "{named}");
    assert!(named.get("result").is_none(), "{named}");

    for (id, name, args) in [
        (5, "brief", json!({})),
        (6, "businesses", json!({})),
        (8, "run_status", json!({ "run": "r-20261125-091502-k3x9" })),
        (9, "runs", json!({})),
    ] {
        let refused = remote.request(id, "tools/call", json!({ "name": name, "arguments": args }));
        assert_eq!(refused["error"]["code"], json!(-32602), "{name}: {refused}");
    }

    cos::business::set_active(&fx.registry, "beta").unwrap();
    let after_flip = remote.request(
        7,
        "tools/call",
        json!({ "name": "latest_brief", "arguments": {} }),
    );
    assert!(markdown(&after_flip).contains("acme brief"), "{after_flip}");
}
