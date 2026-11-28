//! `cos roles` end to end on a temp home: the library, adding, proposing to
//! a file, editing, retiring and restoring, the approval rules, and Ctrl-C
//! or a kill during a proposal. The model is
//! `tests/fixtures/fake-claude-proposal.sh`; fixtures are fictional.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use cos::agents::{policy, roles};

const COS: &str = env!("CARGO_BIN_EXE_cos");
const FAKE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/fake-claude-proposal.sh"
);

fn home(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("cos-roles-cli-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("projects")).unwrap();
    d.canonicalize().unwrap()
}

fn cos(home: &Path, args: &[&str]) -> Command {
    let mut c = Command::new(COS);
    c.arg("--home")
        .arg(home)
        .arg("--workspace")
        .arg(home)
        .arg("--projects")
        .arg(home.join("projects"))
        .arg("--no-open")
        .args(args)
        .env_remove("COS_BUSINESS")
        .env("COS_CLAUDE", FAKE)
        .env("FAKE_PID_DIR", home)
        .env("FAKE_PROPOSAL", "ok")
        .stdin(Stdio::null());
    c
}

fn run(home: &Path, args: &[&str]) -> Output {
    cos(home, args).output().unwrap()
}

fn text(o: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

fn ok(home: &Path, args: &[&str]) -> String {
    let o = run(home, args);
    assert!(o.status.success(), "cos {args:?}: {}", text(&o));
    text(&o)
}

fn fails(home: &Path, args: &[&str], want: &str) {
    let o = run(home, args);
    assert!(!o.status.success(), "cos {args:?} succeeded: {}", text(&o));
    assert!(text(&o).contains(want), "{want:?} not in: {}", text(&o));
}

fn agents_json(home: &Path) -> Option<Vec<u8>> {
    std::fs::read(policy::path(home)).ok()
}

#[test]
fn the_library_lists_twelve_entries_each_with_a_source() {
    let h = home("library");
    let out = ok(&h, &["roles", "list", "--library"]);
    for e in roles::library() {
        assert!(out.contains(&e.id), "{} missing", e.id);
    }
    assert_eq!(out.matches("sources: ").count(), 12, "{out}");
    let shown = ok(&h, &["roles", "show", "--library", "sre"]);
    assert!(shown.contains("\"principles\""), "{shown}");
}

#[test]
fn adding_a_library_role_needs_approval_and_keeps_the_template_staff() {
    let h = home("new");
    fails(&h, &["roles", "new", "appsec-reviewer"], "--yes");
    assert!(
        agents_json(&h).is_none(),
        "refused without approval, nothing written"
    );
    let out = ok(&h, &["roles", "new", "appsec-reviewer", "--yes"]);
    assert!(out.contains("template"), "{out}");
    let p = policy::load(&h);
    assert_eq!(
        p.role("appsec-reviewer").unwrap().from.as_deref(),
        Some("appsec-reviewer@1")
    );
    assert!(p.role("builder").is_some());
    let listed = ok(&h, &["roles", "list"]);
    assert!(
        listed.contains("appsec-reviewer") && listed.contains("appsec-reviewer@1"),
        "{listed}"
    );
}

#[test]
fn a_proposal_writes_only_its_out_file_and_then_adds_as_a_file() {
    let h = home("propose");
    let out = h.join("sre.json");
    let said = ok(
        &h,
        &[
            "roles",
            "propose",
            "sre",
            "--brief",
            "Rust services",
            "--out",
            out.to_str().unwrap(),
        ],
    );
    assert!(
        said.contains("Kept the USE method"),
        "rationale shown: {said}"
    );
    assert!(said.contains("mandate"), "the diff is shown: {said}");
    assert!(
        agents_json(&h).is_none(),
        "propose never writes agents.json"
    );
    let r: policy::Role = serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
    assert_eq!(r.checks, roles::entry("sre").unwrap().role.checks);
    assert_eq!(r.from.as_deref(), Some("sre@1"));
    ok(
        &h,
        &["roles", "new", "--file", out.to_str().unwrap(), "--yes"],
    );
    assert!(policy::load(&h).role("sre").is_some());

    fails(
        &h,
        &["roles", "propose", "sre", "--out", out.to_str().unwrap()],
        "never overwrites",
    );
    std::fs::create_dir_all(h.join("context")).unwrap();
    let inside = h.join("context/x.json");
    fails(
        &h,
        &["roles", "propose", "sre", "--out", inside.to_str().unwrap()],
        "may not write inside",
    );
}

#[test]
fn a_dry_run_prints_what_would_be_sent_and_calls_nothing() {
    let h = home("dry");
    let o = cos(
        &h,
        &[
            "--dry-run",
            "roles",
            "propose",
            "sre",
            "--brief",
            "Rust services",
        ],
    )
    .env("COS_CLAUDE", "/nonexistent/claude")
    .output()
    .unwrap();
    assert!(o.status.success(), "{}", text(&o));
    let out = text(&o);
    assert!(
        out.contains("recruiter") && out.contains("Rust services") && out.contains("sre@1"),
        "{out}"
    );
}

#[test]
fn an_unparseable_file_is_never_overwritten() {
    let h = home("broken");
    std::fs::create_dir_all(h.join("context")).unwrap();
    std::fs::write(policy::path(&h), "{ not json").unwrap();
    fails(
        &h,
        &["roles", "retire", "reviewer", "--yes"],
        "does not parse",
    );
    assert_eq!(agents_json(&h).unwrap(), b"{ not json");
}

#[test]
fn a_retired_role_is_not_hired_until_restored_and_the_plan_shows_the_preamble() {
    let h = home("retire");
    ok(&h, &["roles", "retire", "reviewer", "--yes"]);
    let spawn = [
        "--dry-run",
        "agents",
        "spawn",
        "--prompt",
        "Review the change",
        "--role",
        "reviewer",
    ];
    fails(&h, &spawn, "cos roles restore reviewer");
    ok(&h, &["roles", "restore", "reviewer", "--yes"]);
    ok(&h, &spawn);

    ok(&h, &["roles", "new", "appsec-reviewer", "--yes"]);
    let plan = ok(
        &h,
        &[
            "--dry-run",
            "agents",
            "spawn",
            "--prompt",
            "Review the login flow",
            "--role",
            "appsec-reviewer",
        ],
    );
    let v: serde_json::Value = serde_json::from_str(
        plan.lines()
            .take_while(|l| !l.starts_with("note"))
            .collect::<Vec<_>>()
            .join("\n")
            .trim(),
    )
    .unwrap();
    let pre = v["preamble"].as_str().unwrap();
    let at = |s: &str| pre.find(s).unwrap_or_else(|| panic!("no {s} in {pre}"));
    assert!(at("Principles:") < at("Method:") && at("Method:") < at("Never:"));
    assert!(at("Never:") < at("Hand back:") && at("Hand back:") < at("Stop and leave it"));
    assert!(pre.trim_end().ends_with("outside the worktree."), "{pre}");
}

#[test]
fn a_legacy_file_writes_and_names_the_dropped_keys() {
    let h = home("legacy");
    std::fs::create_dir_all(h.join("context")).unwrap();
    std::fs::write(
        policy::path(&h),
        r#"{ "defaults": { "budget_usd": 5 },
             "roles": [{ "id": "chief-of-staff", "title": "Chief of staff", "head": true, "mandate": "Brief." },
                       { "id": "reviewer", "title": "Reviewer", "mandate": "Review.", "budget_usd": 2 }] }"#,
    )
    .unwrap();
    let out = ok(&h, &["roles", "new", "principal-reviewer", "--yes"]);
    assert!(out.contains("dropped 2 retired key"), "{out}");
}

#[test]
fn a_contract_change_needs_a_person_typing_at_a_terminal() {
    let h = home("contract");
    ok(
        &h,
        &[
            "roles",
            "new",
            "--blank",
            "scribe",
            "--title",
            "Scribe",
            "--mandate",
            "Take notes.",
            "--yes",
        ],
    );
    let mut r = policy::load(&h).role("scribe").unwrap().clone();
    r.checks = vec!["test -s NOTES.md".into()];
    let file = h.join("scribe.json");
    std::fs::write(&file, serde_json::to_string(&r).unwrap()).unwrap();
    fails(
        &h,
        &[
            "roles",
            "edit",
            "scribe",
            "--file",
            file.to_str().unwrap(),
            "--yes",
        ],
        "need a person at a terminal: checks",
    );
    assert!(policy::load(&h).role("scribe").unwrap().checks.is_empty());

    // `script` gives the command a terminal; the typed answer comes on stdin
    // once the prompt is up, and stdin stays open until it exits, because
    // `script` turns an early EOF into a ^D ahead of the answer.
    let mut c = Command::new("/usr/bin/script");
    c.args(["-q", "/dev/null", COS, "--home"])
        .arg(&h)
        .arg("--workspace")
        .arg(&h)
        .args(["roles", "edit", "scribe", "--file"])
        .arg(&file)
        .env_remove("COS_BUSINESS")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = c.spawn().unwrap();
    use std::io::{Read, Write};
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = child.stdout.take().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut seen = Vec::new();
        let mut buf = [0u8; 256];
        while let Ok(n) = stdout.read(&mut buf) {
            if n == 0 {
                break;
            }
            seen.extend_from_slice(&buf[..n]);
            if String::from_utf8_lossy(&seen).contains("Type contract") {
                let _ = tx.send(());
            }
        }
        let _ = tx.send(());
    });
    rx.recv_timeout(Duration::from_secs(20)).expect("no prompt");
    stdin.write_all(b"contract\n").unwrap();
    let status = child.wait().unwrap();
    drop(stdin);
    assert!(status.success(), "the typed answer was refused");
    assert_eq!(
        policy::load(&h).role("scribe").unwrap().checks,
        ["test -s NOTES.md"]
    );
}

fn pid(dir: &Path, name: &str) -> libc::pid_t {
    let at = Instant::now();
    loop {
        if let Ok(s) = std::fs::read_to_string(dir.join(name))
            && let Ok(n) = s.trim().parse()
        {
            return n;
        }
        assert!(at.elapsed() < Duration::from_secs(15), "no {name} pid");
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn gone_within(p: libc::pid_t, limit: Duration) -> bool {
    let at = Instant::now();
    while at.elapsed() < limit {
        if !cos::proc::alive(p) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

fn interrupted(name: &str, signal: libc::c_int) {
    let h = home(name);
    let mut child = cos(&h, &["roles", "propose", "sre"])
        .env("FAKE_PROPOSAL", "slow")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let (me, kid) = (pid(&h, "self"), pid(&h, "child"));
    // SAFETY: the test's own child.
    unsafe {
        libc::kill(child.id() as libc::pid_t, signal);
    }
    let _ = child.wait();
    assert!(
        gone_within(me, Duration::from_secs(3)),
        "the fake claude outlived cos"
    );
    assert!(
        gone_within(kid, Duration::from_secs(3)),
        "its child outlived cos"
    );
}

#[test]
fn ctrl_c_during_a_proposal_ends_claude_and_its_children() {
    interrupted("sigint", libc::SIGINT);
}

#[test]
fn killing_cos_during_a_proposal_still_ends_claude() {
    interrupted("sigkill", libc::SIGKILL);
}
