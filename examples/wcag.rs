//! The desktop app's WCAG measurement, as an autoresearch JSONL v1 evaluator:
//! one request line on stdin, one response line on stdout, logs on stderr.
//!
//! ```sh
//! cargo build --example wcag
//! target/debug/examples/wcag --cargo-home ~/.cargo --rustup-home ~/.rustup \
//!   --target-dir /abs/build-dir --chrome /abs/chrome --axe /abs/axe.min.js < request.json
//! ```
//!
//! It copies the candidate tree into the artifact directory and builds `demo`
//! and `preview` there, because tauri-build writes `gen/` beside the desktop
//! crate and the candidate worktree must stay untouched. It then serves the
//! demo home and an empty home through the preview and runs axe-core over every
//! view, dialog and dashboard of the window and over the report, Flows and Docs
//! pages it frames (as top-level pages: the frames are opaque-origin sandboxes
//! axe cannot enter), light and dark, each on a page loaded in that theme, with
//! reduced motion.
//!
//! Objective `wcag_nodes`: violation nodes plus colour-contrast nodes axe could
//! not decide, summed over every state and both themes; a node axe declined to
//! judge has not passed. Gates: `build`, and `states_rendered` (every state
//! reached its view and settled, and the window still draws its tabs).
//! axe decides part of WCAG: keyboard order, reflow at 320 px and what a screen
//! reader is told when a view changes are not in this number.

#[allow(dead_code)]
#[path = "../tests/common/cdp.rs"]
mod cdp;

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const TAGS: &[&str] = &["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"];
const WORKERS: usize = 3;
/// Chrome never reads the login keychain: a fresh profile otherwise asks it
/// for "Chrome Safe Storage" and stalls when nobody answers.
const CHROME_FLAGS: &[&str] = &[
    "--use-mock-keychain",
    "--password-store=basic",
    "--force-prefers-reduced-motion",
    "--hide-scrollbars",
];

/// Resolves once `root` exists and its element count has held still for one
/// second; false after twelve.
const SETTLE: &str = r#"(async (root) => {
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  let last = -1, same = 0;
  for (let i = 0; i < 96; i++) {
    const el = document.querySelector(root);
    const n = el ? el.querySelectorAll("*").length : -1;
    if (n > 0 && n === last) { if (++same >= 8) return true; } else { same = 0; last = n; }
    await sleep(125);
  }
  return false;
})"#;

/// `Math.random` from a fixed seed, so the docs graph lays out the same way on
/// every run and the nodes axe can and cannot judge do not move between them.
const SEEDED: &str = r#"(() => {
  let s = 0x2f6b4a1d;
  Math.random = () => {
    s |= 0; s = (s + 0x6d2b79f5) | 0;
    let t = Math.imul(s ^ (s >>> 15), 1 | s);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
})();"#;

struct Args {
    cargo_home: PathBuf,
    rustup_home: PathBuf,
    target_dir: PathBuf,
    chrome: PathBuf,
    axe: PathBuf,
}

fn args() -> Args {
    let mut m = BTreeMap::new();
    let mut it = std::env::args().skip(1);
    while let Some(k) = it.next() {
        if let Some(v) = it.next() {
            m.insert(k, PathBuf::from(v));
        }
    }
    let take = |k: &str| {
        m.get(k)
            .cloned()
            .unwrap_or_else(|| panic!("missing {k} <path>"))
    };
    Args {
        cargo_home: take("--cargo-home"),
        rustup_home: take("--rustup-home"),
        target_dir: take("--target-dir"),
        chrome: take("--chrome"),
        axe: take("--axe"),
    }
}

/// One audited state: a page of one preview, an optional click, and the
/// element that must settle before axe runs.
#[derive(Clone)]
struct State {
    name: String,
    home: usize,
    url: String,
    action: Option<String>,
    root: String,
    window: bool,
}

fn state(name: &str, home: usize, url: &str, action: Option<&str>, root: &str) -> State {
    State {
        name: name.into(),
        home,
        url: url.into(),
        action: action.map(Into::into),
        root: root.into(),
        window: !url.starts_with("/cosreport"),
    }
}

#[derive(Default)]
struct Tally {
    rendered: bool,
    error: Option<String>,
    millis: u128,
    /// Per theme: rule id → (impact, node targets).
    found: Vec<(String, String, String, Vec<String>)>,
}

fn main() {
    let a = args();
    let mut input = String::new();
    let _ = std::io::stdin().read_to_string(&mut input);
    let req: Value =
        serde_json::from_str(input.lines().next().unwrap_or("{}")).unwrap_or(Value::Null);
    std::panic::set_hook(Box::new(|p| {
        let msg = p.to_string();
        eprintln!("panic: {}", msg.chars().take(600).collect::<String>());
    }));
    let out = match catch_unwind(AssertUnwindSafe(|| evaluate(&a, &req))) {
        Ok(m) => m,
        Err(_) => failed("the evaluator panicked before measuring"),
    };
    let resp = json!({
        "protocol_version": 1,
        "result": { "status": "success", "output": {
            "evaluator_id": req["evaluator_id"],
            "run_id": req["run_id"],
            "baseline_commit": req["baseline_commit"],
            "evaluated_commit": req["evaluated_commit"],
            "measurements": out,
            "observations": [], "artifacts": [], "warnings": [],
        }},
    });
    let mut stdout = std::io::stdout();
    let _ = writeln!(stdout, "{resp}");
}

fn gate(name: &str, passed: bool, detail: Option<String>) -> Value {
    json!({ "kind": "hard_gate", "name": name, "outcome": { "passed": passed, "detail": detail } })
}

fn num(name: &str, kind: &str, value: f64) -> Value {
    json!({ "kind": "numeric", "name": name, "metric_kind": kind, "direction": "minimize", "value": value })
}

/// Every declared measurement, with the gates failed: the shape never changes.
fn failed(why: &str) -> Vec<Value> {
    let worst = 1.0e6;
    vec![
        gate("build", false, Some(why.into())),
        gate("states_rendered", false, Some(why.into())),
        num("wcag_nodes", "objective", worst),
        num("wcag_violation_nodes", "diagnostic", worst),
        num("wcag_unresolved_contrast", "diagnostic", worst),
        num("wcag_rules_failing", "diagnostic", worst),
        num("wcag_serious_or_critical", "diagnostic", worst),
        num("wcag_states", "diagnostic", 0.0),
    ]
}

fn evaluate(a: &Args, req: &Value) -> Vec<Value> {
    let wt = PathBuf::from(req["candidate_worktree"].as_str().unwrap_or(""));
    let art = PathBuf::from(req["artifact_directory"].as_str().unwrap_or("/nonexistent"));
    let commit = req["evaluated_commit"]
        .as_str()
        .unwrap_or("unknown")
        .to_owned();
    let work = art.join("wcag");
    let _ = std::fs::remove_dir_all(&work);
    // A HOME with no keychain in it: macOS asks for a missing keychain directory with a dialog.
    std::fs::create_dir_all(work.join("env/Library/Keychains")).expect("artifact directory");
    // SAFETY: still single-threaded; Chrome and the previews inherit these.
    unsafe {
        std::env::set_var("HOME", work.join("env"));
        std::env::set_var("PATH", "/usr/bin:/bin:/usr/sbin:/sbin");
        std::env::set_var("TZ", "UTC");
        std::env::set_var("COS_E2E_CHROME", &a.chrome);
        std::env::set_var("COS_E2E_DEADLINE", "60");
    }

    // The candidate's tree, copied: the build must not write into the worktree.
    let src = work.join("src");
    let copied = Command::new("/bin/cp")
        .arg("-Rc")
        .arg(&wt)
        .arg(&src)
        .status()
        .is_ok_and(|s| s.success());
    if !copied {
        return failed("could not copy the tree out of the worktree");
    }
    wait_for_idle_compiler();
    let started = Instant::now();
    let log = work.join("build.log");
    for (pkg, example) in [("cos", "demo"), ("cos-desktop", "preview")] {
        let ok = cargo(
            a,
            &src,
            &log,
            &["build", "--locked", "-p", pkg, "--example", example],
        );
        if !ok {
            return failed(&format!("cargo build of {pkg} example {example} failed"));
        }
    }
    let bin = a.target_dir.join("debug/examples");
    eprintln!("build {} ms", started.elapsed().as_millis());

    let demo = work.join("demo");
    let ok = Command::new(bin.join("demo"))
        .arg(&demo)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", work.join("env"))
        .env("TZ", "UTC")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success());
    if !ok {
        return failed("the demo home could not be written");
    }
    let empty = work.join("empty");
    std::fs::create_dir_all(&empty).expect("empty home");

    let homes = [&demo, &empty];
    let mut servers = Vec::new();
    let mut ports = Vec::new();
    for (i, home) in homes.iter().enumerate() {
        let port = free_port();
        let mut cmd = Command::new(bin.join("preview"));
        cmd.arg(home)
            .arg(port.to_string())
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", work.join("env"))
            .env("TZ", "UTC")
            .stdout(Stdio::null())
            .stderr(std::fs::File::create(work.join(format!("preview-{i}.log"))).expect("log"));
        if i == 0 {
            cmd.env("COS_CN", home.join("bin/cn"));
        }
        let child = cos::proc::spawn_grouped(&mut cmd).expect("start preview");
        servers.push(Server {
            pgid: cos::proc::group_of(&child),
            child,
        });
        ports.push(port);
    }
    for &p in &ports {
        if !wait_port(p) {
            return failed("a preview did not start listening");
        }
    }
    let axe = std::fs::read_to_string(&a.axe).expect("axe-core source");
    let states = states(ports[0], &work);
    let queue = Mutex::new(states.iter().cloned().enumerate().collect::<VecDeque<_>>());
    let results: Mutex<BTreeMap<usize, Tally>> = Mutex::new(BTreeMap::new());
    std::thread::scope(|s| {
        for _ in 0..WORKERS {
            s.spawn(|| {
                loop {
                    if started.elapsed() > Duration::from_secs(200) {
                        break;
                    }
                    let next = queue.lock().unwrap().pop_front();
                    let Some((i, st)) = next else { break };
                    let t0 = Instant::now();
                    let mut t = Tally {
                        rendered: true,
                        ..Tally::default()
                    };
                    for theme in ["light", "dark"] {
                        // Chrome sometimes exits as it starts; a fresh profile gets two more tries.
                        let mut one = Tally::default();
                        for attempt in 0..3 {
                            let profile = work.join(format!("chrome-{i}-{theme}-{attempt}"));
                            one = catch_unwind(AssertUnwindSafe(|| {
                                audit(&st, theme, &ports, &axe, &profile)
                            }))
                            .unwrap_or_else(|e| Tally {
                                error: e
                                    .downcast_ref::<String>()
                                    .map(|s| s.lines().next().unwrap_or("").to_owned()),
                                ..Tally::default()
                            });
                            if one.error.is_none() {
                                break;
                            }
                        }
                        t.rendered &= one.rendered;
                        t.error = t.error.or(one.error);
                        t.found.extend(one.found);
                    }
                    t.millis = t0.elapsed().as_millis();
                    eprintln!(
                        "state {} {} ms {}",
                        st.name,
                        t.millis,
                        t.error.as_deref().unwrap_or("")
                    );
                    results.lock().unwrap().insert(i, t);
                }
            });
        }
    });
    drop(servers);

    let results = results.into_inner().unwrap();
    let mut missing = Vec::new();
    let (mut viol, mut unres, mut severe) = (0usize, 0usize, 0usize);
    let mut rules = BTreeSet::new();
    let mut report = Vec::new();
    for (i, st) in states.iter().enumerate() {
        let t = results.get(&i);
        if !t.is_some_and(|t| t.rendered) {
            missing.push(st.name.clone());
        }
        for (theme, kind, id_impact, targets) in t.map(|t| t.found.as_slice()).unwrap_or(&[]) {
            let (id, impact) = id_impact.split_once('|').unwrap_or((id_impact, ""));
            if kind == "violation" {
                viol += targets.len();
                rules.insert(id.to_owned());
                if impact == "serious" || impact == "critical" {
                    severe += targets.len();
                }
            } else {
                unres += targets.len();
            }
            report.push(json!({ "state": st.name, "theme": theme, "kind": kind, "rule": id, "impact": impact, "targets": targets }));
        }
    }
    let _ = std::fs::write(
        art.join(format!("wcag-{}.json", &commit[..commit.len().min(12)])),
        serde_json::to_vec_pretty(
            &json!({ "states": states.len(), "missing": missing, "findings": report }),
        )
        .unwrap(),
    );
    eprintln!(
        "wcag: {} states, {viol} violation nodes, {unres} unresolved contrast, missing {missing:?}",
        states.len()
    );
    vec![
        gate("build", true, None),
        gate(
            "states_rendered",
            missing.is_empty(),
            (!missing.is_empty()).then(|| format!("not reached: {}", missing.join(", "))),
        ),
        num("wcag_nodes", "objective", (viol + unres) as f64),
        num("wcag_violation_nodes", "diagnostic", viol as f64),
        num("wcag_unresolved_contrast", "diagnostic", unres as f64),
        num("wcag_rules_failing", "diagnostic", rules.len() as f64),
        num("wcag_serious_or_critical", "diagnostic", severe as f64),
        num("wcag_states", "diagnostic", states.len() as f64),
    ]
}

/// Every view and state the window and its pages show over the demo home
/// (0), and the empty states over an empty home (1).
fn states(port: u16, work: &Path) -> Vec<State> {
    let guide = "api-gateway%3Adocs%2Fguides%2Frequest-log-migration.md";
    let map = "api-gateway%3Adocs%2FREADME.md";
    let mut v = vec![
        state("window-today", 0, "/#tab=today", None, "#view-today.active"),
        state(
            "window-agents-staff",
            0,
            "/#tab=agents&pane=staff",
            None,
            "#view-agents.active",
        ),
        state(
            "window-agents-runs",
            0,
            "/#tab=agents&pane=runs",
            None,
            "#view-agents.active",
        ),
        state(
            "window-agents-scorecard",
            0,
            "/#tab=agents&pane=scorecard",
            None,
            "#view-agents.active",
        ),
        state(
            "window-agents-run",
            0,
            "/#tab=agents&pane=runs",
            Some("document.querySelector('#view-agents tbody tr').click()"),
            "#view-agents.active",
        ),
        state(
            "window-hire-dialog",
            0,
            "/#tab=agents&pane=staff",
            Some("document.querySelector('#hire-open').click()"),
            "#hire-dialog[open]",
        ),
        state(
            "window-switch-dialog",
            0,
            "/#tab=today",
            Some("window.openSwitcher(() => {})"),
            "#switch-dialog[open]",
        ),
        state("window-flows", 0, "/#tab=flows", None, "#view-flows.active"),
        state("window-docs", 0, "/#tab=docs", None, "#view-docs.active"),
        state("empty-today", 1, "/#tab=today", None, "#view-today.active"),
        state(
            "empty-runs",
            1,
            "/#tab=agents&pane=runs",
            None,
            "#view-agents.active",
        ),
        state("flows", 0, "/cosreport/flows", None, "body"),
        state("docs-bets", 0, "/cosreport/docs", None, "body"),
        state(
            "docs-topic",
            0,
            &format!("/cosreport/docs#select={guide}"),
            None,
            "body",
        ),
        state(
            "docs-reader",
            0,
            &format!("/cosreport/docs#doc={map}&mode=wide"),
            None,
            "body",
        ),
        state(
            "docs-focus",
            0,
            &format!("/cosreport/docs#doc={guide}&mode=focus"),
            None,
            "body",
        ),
    ];
    // The report's tabs and the window's dashboards, as many as they draw.
    let found = catch_unwind(AssertUnwindSafe(|| {
        let mut c = cdp::Chrome::start(&work.join("chrome-tabs"), CHROME_FLAGS);
        c.open(&format!("http://127.0.0.1:{port}/cosreport/"));
        let tabs = c.eval(r#"[...document.querySelectorAll('[role="tab"][aria-controls]')].map((t) => t.getAttribute("aria-controls"))"#);
        c.open(&format!("http://127.0.0.1:{port}/"));
        c.pause(Duration::from_millis(1500));
        let dashes = c.eval(
            r#"[...document.querySelectorAll('.app-tab[data-view="dash"]')].map((t) => t.dataset.dash)"#,
        );
        (tabs, dashes)
    }))
    .unwrap_or((Value::Null, Value::Null));
    let names = |v: &Value| -> Vec<String> {
        v.as_array()
            .into_iter()
            .flatten()
            .filter_map(|t| t.as_str().map(str::to_owned))
            .collect()
    };
    for d in names(&found.1) {
        let slug: String = d
            .chars()
            .map(|ch| {
                if ch.is_ascii_alphanumeric() {
                    ch.to_ascii_lowercase()
                } else {
                    '-'
                }
            })
            .collect();
        v.push(state(
            &format!("window-dash-{slug}"),
            0,
            &format!("/#tab=dash&dash={}", d.replace(' ', "%20")),
            None,
            "#view-dash.active",
        ));
    }
    let tabs = names(&found.0);
    if tabs.is_empty() {
        v.push(state(
            "report-tabs",
            0,
            "/cosreport/#none",
            None,
            "[role=tab]",
        ));
    }
    for t in tabs {
        v.push(state(
            &format!("report-{t}"),
            0,
            &format!("/cosreport/#{t}"),
            None,
            &format!("#{t}"),
        ));
    }
    v
}

/// One state in one theme, on a page loaded in that theme: switching the
/// scheme under a rendered page leaves some `light-dark()` colours stale.
fn audit(st: &State, theme: &str, ports: &[u16], axe: &str, profile: &Path) -> Tally {
    let mut c = cdp::Chrome::start(profile, CHROME_FLAGS);
    c.call("Page.setBypassCSP", json!({ "enabled": true }));
    c.call(
        "Page.addScriptToEvaluateOnNewDocument",
        json!({ "source": SEEDED }),
    );
    c.call(
        "Emulation.setEmulatedMedia",
        json!({ "features": [
            { "name": "prefers-color-scheme", "value": theme },
            { "name": "prefers-reduced-motion", "value": "reduce" },
        ]}),
    );
    c.open(&format!("http://127.0.0.1:{}{}", ports[st.home], st.url));
    if let Some(act) = &st.action {
        c.pause(Duration::from_millis(1500));
        c.eval(&format!("{act}; true"));
    }
    let root = serde_json::to_string(&st.root).unwrap();
    let mut rendered = c.eval(&format!("{SETTLE}({root})")) == json!(true);
    if st.window {
        rendered &=
            c.eval(r#"document.querySelectorAll(".app-nav .app-tab").length >= 4"#) == json!(true);
    }
    c.eval(&format!("{axe}\n;true"));
    let tags = serde_json::to_string(TAGS).unwrap();
    let run = format!(
        r#"axe.run(document, {{ runOnly: {{ type: "tag", values: {tags} }}, resultTypes: ["violations", "incomplete"], iframes: false }}).then((r) => {{
  const pick = (x) => ({{ id: x.id + "|" + (x.impact || ""), targets: x.nodes.map((n) => n.target.join(" ")) }});
  return {{ v: r.violations.map(pick), u: r.incomplete.filter((x) => x.id === "color-contrast").map(pick) }};
}})"#
    );
    let mut found = Vec::new();
    let r = c.eval(&run);
    for (key, kind) in [("v", "violation"), ("u", "unresolved-contrast")] {
        for x in r[key].as_array().into_iter().flatten() {
            let targets = x["targets"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|t| t.as_str().map(str::to_owned))
                .collect();
            found.push((
                theme.to_owned(),
                kind.to_owned(),
                x["id"].as_str().unwrap_or("").to_owned(),
                targets,
            ));
        }
    }
    Tally {
        rendered,
        found,
        ..Tally::default()
    }
}

fn cargo(a: &Args, src: &Path, log: &Path, args: &[&str]) -> bool {
    let out = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log)
        .expect("build log");
    let mut cmd = Command::new(a.cargo_home.join("bin/cargo"));
    cmd.args(args)
        .current_dir(src)
        .env_clear()
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", a.cargo_home.join("bin").display()),
        )
        .env("CARGO_HOME", &a.cargo_home)
        .env("RUSTUP_HOME", &a.rustup_home)
        .env("CARGO_TARGET_DIR", &a.target_dir)
        .env("HOME", src.parent().unwrap().join("env"))
        .stdout(out.try_clone().expect("log"))
        .stderr(out);
    let Ok(mut child) = cos::proc::spawn_grouped(&mut cmd) else {
        return false;
    };
    let pgid = cos::proc::group_of(&child);
    let until = Instant::now() + Duration::from_secs(240);
    loop {
        match child.try_wait() {
            Ok(Some(s)) => return s.success(),
            Ok(None) if Instant::now() < until => std::thread::sleep(Duration::from_millis(250)),
            _ => {
                cos::proc::kill_group(pgid);
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
        }
    }
}

/// One build on the machine at a time: wait up to two minutes for any other.
fn wait_for_idle_compiler() {
    let until = Instant::now() + Duration::from_secs(120);
    while Instant::now() < until {
        let busy = Command::new("/usr/bin/pgrep")
            .args(["-x", "rustc"])
            .stdout(Stdio::null())
            .status()
            .is_ok_and(|s| s.success());
        if !busy {
            return;
        }
        std::thread::sleep(Duration::from_secs(2));
    }
}

struct Server {
    child: Child,
    pgid: libc::pid_t,
}

impl Drop for Server {
    fn drop(&mut self) {
        cos::proc::kill_group(self.pgid);
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn free_port() -> u16 {
    TcpListener::bind(("127.0.0.1", 0))
        .and_then(|l| l.local_addr())
        .map(|a| a.port())
        .expect("a free port")
}

fn wait_port(port: u16) -> bool {
    let until = Instant::now() + Duration::from_secs(30);
    while Instant::now() < until {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}
