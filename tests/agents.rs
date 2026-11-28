//! `cos agents` end to end against a stand-in Herdr (`tests/fixtures/
//! fake-hc.sh`): every hire becomes a Herdr terminal, cos follows it, stops
//! it at its deadline, and judges it exactly once. Real git worktrees; never
//! the real herdr-core.

#[allow(dead_code)]
mod fake_herdr;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use cos::agents::{self, Harness, Outcome, Request, Seen, Spec, State, Status};
use cos::business::{Layer, Triple};
use fake_herdr::{Fake, agent, fake};

const COS: &str = env!("CARGO_BIN_EXE_cos");

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("cos-agents-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d.canonicalize().unwrap()
}

/// The triple a bare home stands for: workspace is the home, projects sit beside it.
fn at(home: &Path) -> Triple {
    Triple {
        home: home.to_owned(),
        workspace: home.to_owned(),
        projects: home.parent().unwrap().join("projects"),
        business: None,
        layer: Layer::Flags,
    }
}

fn git(dir: &Path, args: &[&str]) {
    let st = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .status()
        .unwrap();
    assert!(st.success(), "git {args:?}");
}

fn repo(root: &Path) -> PathBuf {
    let r = root.join("projects/demo-repo");
    std::fs::create_dir_all(&r).unwrap();
    git(&r, &["init", "-q", "-b", "main"]);
    std::fs::write(r.join("README.md"), "demo\n").unwrap();
    git(&r, &["add", "README.md"]);
    git(&r, &["commit", "-q", "-m", "init"]);
    r
}

fn request(harness: Harness, program: &str, repo: Option<PathBuf>, prompt: &str) -> Request {
    Request {
        harness,
        program: Some(program.into()),
        model: None,
        repo,
        worktree: true,
        title: "test task".into(),
        source: "test".into(),
        prompt: prompt.into(),
        deadline: Duration::from_secs(1800),
        slots: Some(8),
        contract: cos::agents::Contract {
            checks: vec!["test -f made-by-agent.txt".into()],
            ..Default::default()
        },
        explicit: cos::agents::policy::Explicit { deadline: true },
        role: None,
    }
}

/// A home, a repo and a stand-in Herdr; the hire's agent does not run.
fn setup(name: &str) -> (PathBuf, PathBuf, PathBuf, Fake) {
    let root = scratch(name);
    let f = fake(&root);
    let home = root.join("home");
    let repo = repo(&root);
    (root, home, repo, f)
}

fn hire(home: &Path, repo: &Path, prompt: &str) -> Spec {
    agents::spawn(
        &at(home),
        request(Harness::Claude, "claude", Some(repo.to_path_buf()), prompt),
        Path::new(COS),
    )
    .unwrap()
}

fn state(home: &Path, id: &str) -> State {
    agents::read(home, id).unwrap().1
}

fn state_bytes(home: &Path, id: &str) -> Vec<u8> {
    std::fs::read(home.join("out/agents").join(id).join("state.json")).unwrap()
}

fn write_state(home: &Path, id: &str, st: &State) {
    std::fs::write(
        home.join("out/agents").join(id).join("state.json"),
        serde_json::to_vec(st).unwrap(),
    )
    .unwrap();
}

fn ledger_lines(home: &Path) -> usize {
    std::fs::read_to_string(agents::ledger::path(home))
        .unwrap_or_default()
        .lines()
        .count()
}

fn address(st: &State) -> String {
    st.herdr.as_ref().expect("a Herdr terminal").address.clone()
}

/// A scratch run recorded as failed and judged, so its state is final; no
/// Herdr is asked.
fn finished(home: &Path) -> String {
    let spec =
        agents::prepare_spec(&at(home), request(Harness::Claude, "claude", None, "x")).unwrap();
    let mut st = State {
        status: Status::Failed,
        error: Some("exited with code 1".into()),
        ..State::default()
    };
    st.verdict = Some(agents::judge::judge(&spec, &st, None));
    write_state(home, &spec.id, &st);
    spec.id
}

/// A run Herdr has: live in the terminal `address`, recorded without asking it.
fn in_herdr(home: &Path, address: &str) -> Spec {
    let spec =
        agents::prepare_spec(&at(home), request(Harness::Claude, "claude", None, "x")).unwrap();
    let st = State {
        status: Status::Running,
        herdr: Some(Seen {
            address: address.into(),
            word: Some("working".into()),
            live: true,
            ..Seen::default()
        }),
        ..State::default()
    };
    write_state(home, &spec.id, &st);
    spec
}

fn outcome() -> agents::Recorded {
    agents::Recorded {
        outcome: cos::agents::Tested::Passed,
        notes: "checked".into(),
        evidence: vec![],
        at: String::new(),
    }
}

#[test]
fn a_ledger_row_carries_the_spec_title_and_a_row_without_one_still_reads() {
    let (_, home, _, _) = setup("ledger-title");
    let spec =
        agents::prepare_spec(&at(&home), request(Harness::Claude, "claude", None, "x")).unwrap();
    let st = State {
        status: Status::Failed,
        error: Some("exited with code 1".into()),
        ..State::default()
    };
    let judged = State {
        verdict: Some(agents::judge::judge(&spec, &st, None)),
        ..st
    };
    let row = agents::ledger::row(&spec, &judged).unwrap();
    assert_eq!(row.title.as_deref(), Some("test task"));
    let mut legacy = serde_json::to_value(&row).unwrap();
    legacy.as_object_mut().unwrap().remove("title");
    let old: agents::ledger::Row = serde_json::from_value(legacy).unwrap();
    assert_eq!(old.title, None);
}

#[test]
fn a_hire_becomes_a_herdr_terminal_named_labelled_in_its_worktree_with_the_prompt_first() {
    let (_, home, repo, f) = setup("launch");
    let spec = hire(&home, &repo, "Fix the parser in src/lib.rs");
    let st = state(&home, &spec.id);
    assert_eq!(st.status, Status::Running, "{st:?}");
    let t = address(&st);
    assert_eq!(
        f.terminals(),
        std::slice::from_ref(&t),
        "exactly one terminal"
    );
    let name = format!("cos-{}", spec.id);
    assert_eq!(f.fact(&t, "name"), name);
    assert_eq!(spec.terminal.as_deref(), Some(name.as_str()));
    assert!(name.len() <= 32);
    let labels = f.fact(&t, "labels");
    assert!(labels.contains(&format!("hire={}", spec.id)), "{labels}");
    assert_eq!(Path::new(&f.fact(&t, "cwd")), spec.workdir);
    assert_ne!(spec.workdir, repo, "a worktree, not the main checkout");

    let argv = f.argv(&t);
    let at = argv
        .iter()
        .position(|a| a == "claude")
        .expect("claude runs");
    assert_eq!(argv[at + 1], spec.prompt, "the prompt is its first message");
    assert!(argv.contains(&"--permission-mode".to_owned()));
    for one_shot in ["-p", "--print", "--output-format", "--max-budget-usd"] {
        assert!(
            !argv.contains(&one_shot.to_owned()),
            "{one_shot} in {argv:?}"
        );
    }
    assert!(
        argv.contains(&format!("COS_RUN_ID={}", spec.id)),
        "the agent knows its run: {argv:?}"
    );
    let calls = f.calls();
    assert!(calls.contains("run | --bg"), "{calls}");
    assert!(
        calls.contains("--no-parent"),
        "the terminal outlives whatever started cos: {calls}"
    );
    let shown = agents::view::show(&spec, &st);
    assert!(shown.contains(&format!("hc attach {name}")), "{shown}");
}

#[test]
fn the_agents_command_pins_the_business_that_hired_it() {
    let (root, home, repo, f) = setup("pin");
    let triple = Triple {
        home: home.clone(),
        workspace: root.join("ws"),
        projects: root.join("projects"),
        business: Some("acme".into()),
        layer: Layer::Flags,
    };
    let spec = agents::spawn(
        &triple,
        request(Harness::Claude, "claude", Some(repo), "edit"),
        Path::new(COS),
    )
    .unwrap();
    let argv = f.argv(&address(&state(&home, &spec.id)));
    assert_eq!(
        &argv[..3],
        &["/usr/bin/env", "-u", "COS_BUSINESS"],
        "{argv:?}"
    );
    for pin in [
        format!("COS_HOME={}", home.display()),
        format!("COS_WORKSPACE={}", root.join("ws").display()),
        format!("COS_PROJECTS={}", root.join("projects").display()),
    ] {
        assert!(argv.contains(&pin), "{pin} not in {argv:?}");
    }
    let claude = argv.iter().position(|a| a == "claude").unwrap();
    let last_pin = argv
        .iter()
        .position(|a| a.starts_with("COS_PROJECTS="))
        .unwrap();
    assert!(claude > last_pin, "the pins come before the program");
}

#[test]
fn codex_runs_interactive_with_its_prompt_after_the_separator() {
    let (_, home, repo, f) = setup("codex");
    let spec = agents::spawn(
        &at(&home),
        request(Harness::Codex, "codex", Some(repo), "review the parser"),
        Path::new(COS),
    )
    .unwrap();
    let argv = f.argv(&address(&state(&home, &spec.id)));
    let at = argv.iter().position(|a| a == "codex").unwrap();
    assert!(
        !argv.contains(&"exec".to_owned()),
        "never codex exec: {argv:?}"
    );
    assert_eq!(
        argv[argv.len() - 2..],
        ["--".to_owned(), spec.prompt.clone()]
    );
    assert!(argv[at..].contains(&"workspace-write".to_owned()));
}

#[test]
fn no_herdr_refuses_with_the_fix_and_starts_nothing() {
    let (_, home, repo, f) = setup("down");
    f.down(true);
    let err = agents::spawn(
        &at(&home),
        request(Harness::Claude, "claude", Some(repo.clone()), "edit"),
        Path::new(COS),
    )
    .unwrap_err();
    let msg = format!("{err:#}");
    assert!(msg.contains("Herdr isn't running"), "{msg}");
    assert!(msg.contains("hc server start"), "the fix is named: {msg}");
    assert!(
        agents::list(&home).unwrap().runs.is_empty(),
        "nothing started"
    );
    assert!(f.terminals().is_empty());

    let s = cos::herdr::start().unwrap();
    assert!(s.up, "Start Herdr brings it up: {s:?}");
    assert!(f.calls().contains("server | start"));
    hire(&home, &repo, "edit");
}

#[test]
fn done_after_the_first_message_judges_once_and_a_restart_does_not_judge_again() {
    let (_, home, repo, f) = setup("done-once");
    f.exec(true);
    let spec = agents::spawn(
        &at(&home),
        request(Harness::Claude, &agent(), Some(repo), "edit a file"),
        Path::new(COS),
    )
    .unwrap();
    let t = address(&state(&home, &spec.id));
    assert_eq!(
        state(&home, &spec.id).verdict,
        None,
        "not judged before it is followed"
    );

    agents::tick(&home).unwrap();
    let st = state(&home, &spec.id);
    let v = st.verdict.clone().expect("judged on done");
    assert_eq!(v.outcome, Outcome::Accepted, "{v:?}");
    assert_eq!(st.judged_on.as_deref(), Some("done at turn 1"));
    assert_eq!(st.status, Status::Running, "still live in Herdr");
    assert_eq!(ledger_lines(&home), 1);

    // Another turn and done again, then a fresh follower (an app restart).
    f.set(&t, "turn", "2");
    agents::tick(&home).unwrap();
    agents::tick(&home).unwrap();
    let again = state(&home, &spec.id);
    assert_eq!(again.verdict, Some(v), "the verdict stands");
    assert_eq!(ledger_lines(&home), 1, "judged exactly once");

    agents::mark_done(&home, &spec.id).unwrap();
    assert_eq!(
        ledger_lines(&home),
        1,
        "marking it done does not judge again"
    );
    assert_eq!(state(&home, &spec.id).status, Status::Done);
}

#[test]
fn done_before_any_turn_is_not_judged() {
    let (_, home, repo, f) = setup("done-turn0");
    let spec = hire(&home, &repo, "edit");
    let t = address(&state(&home, &spec.id));
    f.set(&t, "status", "done");
    agents::tick(&home).unwrap();
    assert!(state(&home, &spec.id).verdict.is_none());
}

#[test]
fn an_unsent_first_message_is_surfaced_and_its_done_is_not_judged() {
    let (_, home, repo, f) = setup("unsent");
    std::fs::write(
        f.state.join("first_message"),
        r#"{"sent":false,"reason":"the agent was not ready"}"#,
    )
    .unwrap();
    let spec = hire(&home, &repo, "edit");
    let st = state(&home, &spec.id);
    let err = st.error.clone().unwrap_or_default();
    assert!(err.contains("did not send it the work"), "{err}");
    assert!(err.contains("the agent was not ready"), "{err}");
    let t = address(&st);
    assert!(f.sent(&t).is_empty(), "never resent blindly");
    f.set(&t, "status", "done");
    f.set(&t, "turn", "1");
    agents::tick(&home).unwrap();
    assert!(state(&home, &spec.id).verdict.is_none());
    assert!(agents::view::show(&spec, &state(&home, &spec.id)).contains("UNSENT"));
}

#[test]
fn the_deadline_interrupts_then_kills_and_the_judge_runs_once() {
    let (_, home, repo, f) = setup("deadline");
    let mut req = request(Harness::Claude, "claude", Some(repo), "edit");
    req.deadline = Duration::from_secs(0);
    let spec = agents::spawn(&at(&home), req, Path::new(COS)).unwrap();
    let t = address(&state(&home, &spec.id));

    agents::tick(&home).unwrap();
    let st = state(&home, &spec.id);
    assert!(
        f.calls().contains(&format!("interrupt | {t}")),
        "{}",
        f.calls()
    );
    assert!(!f.calls().contains("kill"), "the stop key first");
    assert_eq!(st.status, Status::Running);
    assert!(st.herdr.as_ref().unwrap().interrupted.is_some());

    agents::tick(&home).unwrap();
    let st = state(&home, &spec.id);
    assert!(f.calls().contains(&format!("kill | {t}")), "{}", f.calls());
    assert_eq!(st.status, Status::TimedOut);
    assert!(
        st.error.as_deref().unwrap_or("").contains("deadline"),
        "{st:?}"
    );
    assert_eq!(st.verdict.as_ref().unwrap().outcome, Outcome::Rejected);
    assert_eq!(f.fact(&t, "state"), "finished");
    assert!(
        agents::dir(&home, &spec.id).join("screen.txt").is_file(),
        "its last screen kept"
    );

    agents::tick(&home).unwrap();
    assert_eq!(ledger_lines(&home), 1, "judged exactly once");
    assert_eq!(f.calls().matches("kill |").count(), 1);
}

#[test]
fn blocked_in_herdr_is_waiting_on_you_with_its_kind() {
    let (_, home, repo, f) = setup("blocked");
    let spec = hire(&home, &repo, "edit");
    let t = address(&state(&home, &spec.id));
    f.set(&t, "status", "blocked");
    f.set(&t, "kind", "permission");
    agents::tick(&home).unwrap();
    let st = state(&home, &spec.id);
    assert_eq!(
        st.herdr.as_ref().unwrap().blocked().as_deref(),
        Some("permission")
    );
    assert_eq!(
        agents::needs(&spec, &st).as_deref(),
        Some("waiting on you in Herdr (permission)")
    );
    assert!(agents::view::herdr_word(&spec, &st).contains("blocked: permission"));
    let waiting = agents::view::waiting_in_herdr(&agents::list(&home).unwrap().runs);
    assert_eq!(waiting.len(), 1);
    assert_eq!(waiting[0]["kind"], "permission");

    let err = agents::say(&home, &spec.id, "go on").unwrap_err();
    assert!(format!("{err:#}").contains("answer it there"), "{err:#}");

    f.set(&t, "status", "working");
    f.set(&t, "kind", "");
    agents::tick(&home).unwrap();
    assert_eq!(agents::needs(&spec, &state(&home, &spec.id)), None);
}

#[test]
fn reply_pause_and_mark_done_are_send_interrupt_and_kill() {
    let (_, home, repo, f) = setup("talk");
    let spec = hire(&home, &repo, "edit");
    let t = address(&state(&home, &spec.id));

    agents::say(&home, &spec.id, "  also update the README  ").unwrap();
    assert_eq!(f.sent(&t), ["also update the README"]);
    agents::pause(&home, &spec.id).unwrap();
    assert!(f.calls().contains(&format!("interrupt | {t}")));
    assert!(
        agents::screen(&home, &spec.id, 40)
            .unwrap()
            .contains(&spec.id)
    );
    assert!(
        f.calls().contains(&format!("read | {t} | --lines | 40")),
        "{}",
        f.calls()
    );

    std::fs::write(spec.workdir.join("made-by-agent.txt"), "by hand\n").unwrap();
    let st = agents::mark_done(&home, &spec.id).unwrap();
    assert!(f.calls().contains(&format!("kill | {t}")));
    assert_eq!(st.status, Status::Done);
    assert_eq!(st.judged_on.as_deref(), Some("marked done"));
    let ended = st.herdr.as_ref().unwrap();
    assert!(
        !ended.live && ended.reason.as_deref() == Some("killed"),
        "{ended:?}"
    );
    assert_eq!(st.verdict.unwrap().outcome, Outcome::Accepted);
    assert_eq!(ledger_lines(&home), 1);

    let err = agents::say(&home, &spec.id, "more").unwrap_err();
    assert!(format!("{err:#}").contains("has ended"), "{err:#}");

    agents::clean(&home, &spec.id, true).unwrap();
    assert!(f.calls().contains(&format!("rm | {t}")));
    assert!(!f.exists(&t), "its terminal is removed");
}

#[test]
fn open_in_herdr_shows_its_terminal_in_the_window() {
    let (_, home, repo, f) = setup("open");
    let spec = hire(&home, &repo, "edit");
    let t = address(&state(&home, &spec.id));
    let said = agents::open_in_herdr(&home, &spec.id).unwrap();
    assert!(said.contains(&t), "{said}");
    assert!(
        f.app_log()
            .contains(&format!("do agent.focus --terminal {t}")),
        "{}",
        f.app_log()
    );

    std::fs::write(f.state.join("app_closed"), "").unwrap();
    let err = agents::open_in_herdr(&home, &spec.id).unwrap_err();
    assert!(
        format!("{err:#}").contains(&format!("cos-{}", spec.id)),
        "names the terminal: {err:#}"
    );
}

#[test]
fn cos_never_touches_a_terminal_that_is_not_its_hire() {
    let (_, home, repo, f) = setup("not-ours");
    let spec = hire(&home, &repo, "edit");
    let t = address(&state(&home, &spec.id));
    f.set(&t, "labels", "hire=someone-else\n");
    assert!(agents::say(&home, &spec.id, "hi").is_err());
    assert!(agents::pause(&home, &spec.id).is_err());
    assert!(agents::mark_done(&home, &spec.id).is_err());
    let calls = f.calls();
    for verb in ["send |", "interrupt |", "kill |", "rm |"] {
        assert!(
            !calls.contains(verb),
            "{verb} reached a terminal cos did not start: {calls}"
        );
    }
    agents::tick(&home).unwrap();
    let st = state(&home, &spec.id);
    assert_eq!(st.status, Status::Lost, "{st:?}");
    assert!(!f.calls().contains("kill |"));
}

#[test]
fn an_agent_that_exits_ends_the_run_and_is_judged() {
    let (_, home, repo, f) = setup("exit");
    f.exec(true);
    let spec = agents::spawn(
        &at(&home),
        request(Harness::Claude, &agent(), Some(repo), "FAIL please"),
        Path::new(COS),
    )
    .unwrap();
    agents::tick(&home).unwrap();
    let st = state(&home, &spec.id);
    assert_eq!(st.status, Status::Failed);
    assert!(
        st.error
            .as_deref()
            .unwrap_or("")
            .contains("exited with code 1"),
        "{st:?}"
    );
    let v = st.verdict.unwrap();
    assert_eq!(v.outcome, Outcome::Rejected);
    assert!(v.checks.is_empty(), "a failed run's checks are not run");
}

#[test]
fn a_terminal_removed_outside_cos_ends_the_run_lost() {
    let (_, home, repo, f) = setup("removed");
    let spec = hire(&home, &repo, "edit");
    let t = address(&state(&home, &spec.id));
    std::fs::remove_dir_all(f.state.join("terms").join(&t)).unwrap();
    agents::tick(&home).unwrap();
    let st = state(&home, &spec.id);
    assert_eq!(st.status, Status::Lost);
    assert!(st.error.unwrap().contains("removed outside cos"));
}

#[test]
fn herdr_going_away_changes_nothing() {
    let (_, home, repo, f) = setup("core-gone");
    let spec = hire(&home, &repo, "edit");
    f.down(true);
    agents::tick(&home).unwrap();
    assert_eq!(state(&home, &spec.id).status, Status::Running);
}

#[test]
fn slots_count_the_runs_live_in_herdr() {
    let (_, home, repo, _f) = setup("slots");
    let mut one = request(Harness::Claude, "claude", Some(repo.clone()), "edit");
    one.slots = Some(1);
    let first = agents::spawn(&at(&home), one, Path::new(COS)).unwrap();
    let mut two = request(Harness::Claude, "claude", Some(repo.clone()), "edit");
    two.slots = Some(1);
    let err = agents::spawn(&at(&home), two, Path::new(COS)).unwrap_err();
    assert!(format!("{err:#}").contains("all 1 run slots"), "{err:#}");
    assert_eq!(agents::live_runs(&home), 1);

    agents::stop(&home, &first.id, Duration::from_secs(5)).unwrap();
    let mut three = request(Harness::Claude, "claude", Some(repo), "edit");
    three.slots = Some(1);
    agents::spawn(&at(&home), three, Path::new(COS)).unwrap();
}

#[test]
fn a_run_from_before_herdr_reads_but_cannot_be_resumed() {
    let root = scratch("legacy");
    let home = root.join("home");
    let d = agents::dir(&home, "20261101-090000-0ld1");
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(
        d.join("spec.json"),
        r#"{"id":"20261101-090000-0ld1","harness":"claude","program":"claude","model":null,
            "repo":null,"base":null,"workdir":"/nowhere","worktree":false,"title":"old",
            "source":"prompt","prompt":"p","deadline_secs":1800,"budget_usd":5.0,
            "created":"2026-11-01T09:00:00Z","limits":{"stall_secs":600,"repeat":5}}"#,
    )
    .unwrap();
    std::fs::write(
        d.join("state.json"),
        r#"{"status":"running","supervisor":null,"pgid":null,"pgid_started":null,"started":null,
            "ended":null,"exit":null,"cost_usd":0.42,"tokens":100,"tool_calls":3,"session":"s",
            "last":"x","summary":"its words","error":null,"flags":[],"flagged":["stalled"],
            "verdict":{"outcome":"needs_you","reasons":["no checks"],"changed":[],"out_of_scope":[],
            "checks":[],"overclaimed":false,"judged":"2026-11-01T09:10:00Z"}}"#,
    )
    .unwrap();
    let (spec, st) = agents::read(&home, "20261101-090000-0ld1").unwrap();
    assert!(spec.legacy());
    assert_eq!(st.status, Status::Lost);
    assert!(st.error.as_deref().unwrap().contains("cannot be resumed"));
    assert_eq!(
        st.verdict.unwrap().outcome,
        Outcome::NeedsYou,
        "its verdict still reads"
    );
    assert_eq!(st.cost_usd, Some(0.42));
    let err = agents::say(&home, "20261101-090000-0ld1", "hi").unwrap_err();
    assert!(format!("{err:#}").contains("cannot be resumed"), "{err:#}");
    assert_eq!(agents::live_runs(&home), 0, "it takes no slot");
}

#[test]
fn clean_refuses_a_worktree_holding_work_unless_forced() {
    let (_, home, repo, f) = setup("clean");
    f.exec(true);
    let spec = agents::spawn(
        &at(&home),
        request(Harness::Claude, &agent(), Some(repo), "edit a file"),
        Path::new(COS),
    )
    .unwrap();
    let live = agents::clean(&home, &spec.id, true).unwrap_err();
    assert!(format!("{live:#}").contains("stop it first"), "{live:#}");
    agents::mark_done(&home, &spec.id).unwrap();

    let refused = agents::clean(&home, &spec.id, false).unwrap_err();
    assert!(
        format!("{refused:#}").contains("made-by-agent.txt"),
        "{refused:#}"
    );
    assert!(spec.workdir.exists());

    agents::clean(&home, &spec.id, true).unwrap();
    assert!(!spec.workdir.exists());
    assert!(agents::read(&home, &spec.id).is_err());
}

#[test]
fn spin_off_hires_into_herdr() {
    let (root, home, repo, f) = setup("spin-off");
    brief(
        &home,
        &format!("Fix the parser in `{}/src/lib.rs`.", repo.display()),
    );
    let triple = cos::business::Triple {
        home: home.clone(),
        workspace: home.clone(),
        projects: root.join("projects"),
        business: None,
        layer: cos::business::Layer::Flags,
    };
    let args = serde_json::from_value(serde_json::json!({ "from": "brief", "item": 1 })).unwrap();
    let spec = cos::mcp::spin_off_hire(&triple, args, Path::new(COS)).unwrap();
    assert_eq!(spec.repo.as_deref(), Some(repo.as_path()));
    let until = Instant::now() + Duration::from_secs(20);
    while state(&home, &spec.id).herdr.is_none() {
        assert!(Instant::now() < until, "never reached Herdr");
        std::thread::sleep(Duration::from_millis(50));
    }
    let t = address(&state(&home, &spec.id));
    assert_eq!(f.fact(&t, "name"), format!("cos-{}", spec.id));
    assert!(f.argv(&t).contains(&"claude".to_owned()));

    f.down(true);
    let args = serde_json::from_value(serde_json::json!({ "from": "brief", "item": 1 })).unwrap();
    let err = cos::mcp::spin_off_hire(&triple, args, Path::new(COS)).unwrap_err();
    assert!(
        format!("{err:#}").contains("Herdr isn't running"),
        "{err:#}"
    );
}

fn brief(home: &Path, prompt: &str) {
    use cos::reports::{Action, Basis, Brief, Verb};
    let action = |item: &str, prompt: &str| Action {
        verb: Verb::Do,
        owner: String::new(),
        recommendation: item.into(),
        why: String::new(),
        next_step: String::new(),
        source: String::new(),
        deadline: String::new(),
        deadline_basis: Basis::None,
        goal_id: String::new(),
        deliverable_id: String::new(),
        agent_prompt: prompt.into(),
    };
    let brief = Brief {
        headline: String::new(),
        needs_you: vec![action("fix the parser", prompt)],
        what_changed: vec![],
        week: Default::default(),
        calendar: vec![],
        backlog: vec![],
        coverage: vec![],
        gaps: vec![],
        upcoming: vec![],
        tickets: vec![],
    };
    cos::store::save(home, cos::store::Kind::Briefs, "2026-10-16", &brief, "").unwrap();
}

#[test]
fn infers_the_repo_from_a_path_the_prompt_names() {
    let root = scratch("infer");
    let repo = repo(&root);
    let projects = root.join("projects");
    let prompt = format!("Open `{}/src/lib.rs` and fix the parser.", repo.display());
    assert_eq!(agents::infer_repo(&prompt, &projects), Some(repo));
    assert_eq!(agents::infer_repo("no path here", &projects), None);
    assert_eq!(agents::infer_repo("/etc/hosts", &projects), None);
}

#[test]
fn tasks_number_items_as_the_report_does_and_skip_empty_prompts() {
    use cos::reports::{Action, Basis, Brief, Verb};
    let root = scratch("tasks");
    let action = |item: &str, prompt: &str| Action {
        verb: Verb::Do,
        owner: String::new(),
        recommendation: item.into(),
        why: String::new(),
        next_step: String::new(),
        source: String::new(),
        deadline: String::new(),
        deadline_basis: Basis::None,
        goal_id: String::new(),
        deliverable_id: String::new(),
        agent_prompt: prompt.into(),
    };
    let brief = Brief {
        headline: String::new(),
        needs_you: vec![action("reply", ""), action("fix CI", "fix it")],
        what_changed: vec![],
        week: Default::default(),
        calendar: vec![],
        backlog: vec![],
        coverage: vec![],
        gaps: vec![],
        upcoming: vec![],
        tickets: vec![],
    };
    cos::store::save(&root, cos::store::Kind::Briefs, "2026-10-16", &brief, "").unwrap();
    let tasks = agents::tasks(&root, agents::From::Brief).unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!((tasks[0].n, tasks[0].title.as_str()), (2, "fix CI"));
    assert_eq!(tasks[0].source, "brief 2026-10-16 #2");
    assert!(
        agents::tasks(&root, agents::From::Goals)
            .unwrap()
            .is_empty(),
        "a brief with no week has no goal prompts"
    );
}

#[test]
fn run_dir_refuses_bad_ids() {
    let root = scratch("refuse");
    let home = root.join("home");
    let good = finished(&home);
    let good_before = state_bytes(&home, &good);
    let agents_dir = home.join("out/agents");

    let victim = home.join("out/x");
    std::fs::create_dir_all(&victim).unwrap();
    std::fs::write(victim.join("state.json"), b"victim").unwrap();

    let other_home = root.join("other");
    let other = finished(&other_home);
    let other_before = state_bytes(&other_home, &other);

    std::os::unix::fs::symlink(agents_dir.join(&good), agents_dir.join("linked")).unwrap();
    std::fs::create_dir_all(agents_dir.join("broken")).unwrap();
    std::fs::write(agents_dir.join("broken/spec.json"), "{ not json").unwrap();

    let ids = [
        "",
        ".",
        "..",
        "../x",
        "a/b",
        other.as_str(),
        "linked",
        "broken",
    ];
    for id in ids {
        assert!(agents::run_dir(&home, id).is_err(), "run_dir {id:?}");
        assert!(agents::read(&home, id).is_err(), "read {id:?}");
        assert!(
            agents::record(&home, id, outcome()).is_err(),
            "record {id:?}"
        );
        assert!(
            agents::stop(&home, id, Duration::from_millis(100)).is_err(),
            "stop {id:?}"
        );
        assert!(agents::rejudge(&home, id).is_err(), "rejudge {id:?}");
        assert!(agents::say(&home, id, "hi").is_err(), "say {id:?}");
        assert!(agents::mark_done(&home, id).is_err(), "done {id:?}");
        assert!(agents::clean(&home, id, true).is_err(), "clean {id:?}");
    }

    assert_eq!(state_bytes(&home, &good), good_before);
    assert_eq!(state_bytes(&other_home, &other), other_before);
    assert_eq!(std::fs::read(victim.join("state.json")).unwrap(), b"victim");
    assert!(agents_dir.join(&good).is_dir());
    assert!(agents::read(&home, &good).is_ok());
}

#[test]
fn list_skips_refused_runs() {
    let root = scratch("skips");
    let home = root.join("home");
    let done = finished(&home);
    let agents_dir = home.join("out/agents");
    std::os::unix::fs::symlink(agents_dir.join(&done), agents_dir.join("linked")).unwrap();
    std::fs::create_dir_all(agents_dir.join("stale")).unwrap();
    std::fs::copy(
        agents_dir.join(&done).join("spec.json"),
        agents_dir.join("stale/spec.json"),
    )
    .unwrap();

    let listing = agents::list(&home).unwrap();
    let listed: Vec<&str> = listing.runs.iter().map(|(s, _)| s.id.as_str()).collect();
    assert_eq!(listed, [done.as_str()]);
    let mut skipped = listing.skipped.clone();
    skipped.sort();
    assert_eq!(skipped, ["linked", "stale"]);

    let out = Command::new(COS)
        .arg("--home")
        .arg(&home)
        .args(["agents", "clean"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("skipped:"), "{stderr}");
    assert!(!agents_dir.join(&done).exists(), "finished run kept");
    assert!(agents_dir.join("stale").exists(), "skipped id removed");
}

#[test]
fn watch_waits_on_skipped_and_ends_when_nothing_is_live() {
    let root = scratch("watch");
    let home = root.join("home");
    let done = finished(&home);
    let agents_dir = home.join("out/agents");
    std::os::unix::fs::symlink(agents_dir.join(&done), agents_dir.join("linked")).unwrap();

    let mut child = Command::new(COS)
        .arg("--home")
        .arg(&home)
        .args(["agents", "watch"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    std::thread::sleep(Duration::from_secs(5));
    let still = child.try_wait().unwrap().is_none();
    let _ = child.kill();
    let _ = child.wait();
    assert!(still, "watch exited with a skipped id present");

    let quiet = root.join("quiet");
    finished(&quiet);
    let mut child = Command::new(COS)
        .arg("--home")
        .arg(&quiet)
        .args(["agents", "watch"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let until = Instant::now() + Duration::from_secs(5);
    let status = loop {
        if let Some(st) = child.try_wait().unwrap() {
            break st;
        }
        assert!(
            Instant::now() < until,
            "watch never exited with every run terminal"
        );
        std::thread::sleep(Duration::from_millis(50));
    };
    assert!(status.success());
}

#[test]
fn cli_and_mcp_refuse_escaping_ids() {
    let root = scratch("escape");
    let home = root.join("home");
    let good = finished(&home);
    let good_before = state_bytes(&home, &good);
    let victim = home.join("out/x");
    std::fs::create_dir_all(&victim).unwrap();
    std::fs::write(victim.join("state.json"), b"victim").unwrap();

    for verb in ["show", "kill", "done", "screen"] {
        let out = Command::new(COS)
            .arg("--home")
            .arg(&home)
            .args(["agents", verb, "../x"])
            .output()
            .unwrap();
        assert!(!out.status.success(), "agents {verb} ../x succeeded");
    }
    assert!(!victim.join("stop").exists(), "kill reached ../x");

    let cfg = cos::pipeline::Config {
        home: home.clone(),
        install: home.clone(),
        workspace: root.join("ws"),
        projects: root.join("projects"),
        budget: None,
        bins: Default::default(),
    };
    let server = cos::mcp::Cos::new(cfg, cos::mcp::Mode::Local);
    let res = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(
            server.stop_agent(rmcp::handler::server::wrapper::Parameters(
                cos::mcp::AgentId {
                    id: "../x".into(),
                    business: None,
                },
            )),
        );
    assert_eq!(res.is_error, Some(true), "{res:?}");
    assert!(!victim.join("stop").exists(), "stop_agent reached ../x");
    assert_eq!(std::fs::read(victim.join("state.json")).unwrap(), b"victim");
    assert_eq!(state_bytes(&home, &good), good_before);
}

#[test]
fn count_is_read_only_and_reads_herdr_runs_as_live() {
    let root = scratch("count");
    let home = root.join("home");
    let live = in_herdr(&home, "t1");
    assert_eq!(
        agents::count(&home).unwrap(),
        agents::Counts {
            live: 1,
            lost: 0,
            starting: 0,
            unreadable: 0
        }
    );

    let bad = finished(&home);
    std::fs::write(
        home.join("out/agents").join(&bad).join("state.json"),
        b"{ not json",
    )
    .unwrap();
    let before = state_bytes(&home, &live.id);
    assert_eq!(
        agents::count(&home).unwrap(),
        agents::Counts {
            live: 1,
            lost: 0,
            starting: 0,
            unreadable: 1
        }
    );
    assert_eq!(state_bytes(&home, &live.id), before, "count wrote a record");
}

#[test]
fn a_hire_whose_preparing_process_is_gone_is_lost() {
    let root = scratch("launcher");
    let home = root.join("home");
    let spec =
        agents::prepare_spec(&at(&home), request(Harness::Claude, "claude", None, "x")).unwrap();
    // `prepare_spec` records this test as the launcher: alive, so starting.
    assert_eq!(
        agents::count(&home).unwrap(),
        agents::Counts {
            live: 1,
            lost: 0,
            starting: 1,
            unreadable: 0
        }
    );
    assert_eq!(
        agents::live_launchers(&home),
        vec![std::process::id() as i32]
    );

    let mut gone = Command::new("true").spawn().unwrap();
    let dead = gone.id() as i32;
    gone.wait().unwrap();
    let st = State {
        launcher: Some(dead),
        launcher_started: Some("0.000000".into()),
        ..State::default()
    };
    write_state(&home, &spec.id, &st);
    let mut old: Spec = spec.clone();
    old.created = "2020-01-01T00:00:00Z".into();
    std::fs::write(
        agents::dir(&home, &spec.id).join("spec.json"),
        serde_json::to_vec(&old).unwrap(),
    )
    .unwrap();
    assert_eq!(agents::count(&home).unwrap().lost, 1);
    assert!(agents::live_launchers(&home).is_empty());
    let (_, read) = agents::read(&home, &spec.id).unwrap();
    assert_eq!(read.status, Status::Lost);
    assert!(read.error.unwrap().contains("before the run reached Herdr"));
}

#[test]
fn count_refuses_a_symlinked_run() {
    let root = scratch("count-link");
    let home = root.join("home");
    let other_home = root.join("other");
    let other = in_herdr(&other_home, "t9").id;
    let target = other_home.join("out/agents").join(&other);
    let before = state_bytes(&other_home, &other);

    let agents_dir = home.join("out/agents");
    std::fs::create_dir_all(&agents_dir).unwrap();
    std::os::unix::fs::symlink(&target, agents_dir.join(&other)).unwrap();

    assert_eq!(
        agents::count(&home).unwrap(),
        agents::Counts {
            live: 0,
            lost: 0,
            starting: 0,
            unreadable: 1
        }
    );
    assert_eq!(
        state_bytes(&other_home, &other),
        before,
        "count wrote a record"
    );
}

#[test]
fn two_followers_keep_ended_and_verdict() {
    let (_, home, repo, f) = setup("lost-update");
    let spec = hire(&home, &repo, "edit");
    let t = address(&state(&home, &spec.id));
    let id = spec.id.clone();

    let observer = {
        let (home, id, state_dir) = (home.clone(), id.clone(), f.state.clone());
        std::thread::spawn(move || {
            for turn in 1..=300 {
                std::fs::write(
                    state_dir.join("terms").join(&t).join("turn"),
                    turn.to_string(),
                )
                .unwrap();
                agents::follow::observe(&home, &id).unwrap();
            }
        })
    };
    let closer = {
        let (home, id) = (home.clone(), id.clone());
        std::thread::spawn(move || agents::mark_done(&home, &id).map(|_| ()))
    };
    observer.join().unwrap();
    closer.join().unwrap().unwrap();

    let st = state(&home, &id);
    assert_eq!(st.status, Status::Done, "{st:?}");
    assert!(st.verdict.is_some(), "verdict lost: {st:?}");
    assert_eq!(ledger_lines(&home), 1, "judged exactly once");
}
