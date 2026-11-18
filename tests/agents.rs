//! `cos agents` end to end, herdr owning every run: the stand-in harness run in
//! a real worktree exactly as a hire would, the verdict cos records from it, and
//! the reconciler that marks a run lost when its observer dies. The live-owner
//! tests stand a real `cos` process in for the observer (blocked on a build
//! lock), since that is what `is_cos` checks.

use std::fs::File;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use cos::agents::{self, Harness, Liveness, Request, Spec, State, Status};
use cos::business::{Layer, Triple};

const COS: &str = env!("CARGO_BIN_EXE_cos");
const FAKE_CLAUDE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/fake-claude.sh");
const FAKE_CODEX: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/fake-codex.sh");

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("cos-agents-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d.canonicalize().unwrap()
}

/// The triple a bare home stands for: workspace is the home, projects sit under it.
fn at(home: &Path) -> Triple {
    Triple {
        home: home.to_owned(),
        workspace: home.to_owned(),
        projects: home.join("projects"),
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
        deadline: Duration::from_secs(30),
        budget_usd: 1.0,
        slots: None,
        contract: Default::default(),
        explicit: Default::default(),
        role: None,
    }
}

fn state_bytes(home: &Path, id: &str) -> Vec<u8> {
    std::fs::read(home.join("out/agents").join(id).join("state.json")).unwrap()
}

fn outcome() -> agents::Recorded {
    agents::Recorded {
        outcome: cos::agents::Tested::Passed,
        notes: "checked".into(),
        evidence: vec![],
        at: String::new(),
    }
}

fn wait_dead(pid: i32) {
    let until = Instant::now() + Duration::from_secs(5);
    while cos::proc::alive(pid) {
        assert!(Instant::now() < until, "pid {pid} still alive");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Prepares a hire and runs its stand-in harness to completion in the worktree,
/// as a herdr space would, folds the stream into the recorded state, judges the
/// worktree and writes the result — what the observer records when a space ends.
fn run(home: &Path, req: Request) -> (Spec, State) {
    let spec = agents::prepare_spec(&at(home), req).unwrap();
    let out = agents::harness::command(&spec).output().unwrap();
    let mut state = State::default();
    let mut failed = None;
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        let t = agents::harness::read(spec.harness, line);
        if let Some(l) = t.last {
            state.last = l;
        }
        state.tool_calls += t.tool_calls;
        if t.cost_usd.is_some() {
            state.cost_usd = t.cost_usd;
        }
        if let Some(n) = t.tokens {
            state.tokens = Some(state.tokens.unwrap_or(0) + n);
        }
        if let Some(s) = t.summary {
            state.summary = s;
        }
        if t.session.is_some() {
            state.session = t.session;
        }
        if t.failed.is_some() {
            failed = t.failed;
        }
    }
    state.exit = out.status.code();
    state.status = if out.status.success() && failed.is_none() {
        Status::Done
    } else {
        Status::Failed
    };
    state.error = failed;
    state.verdict = Some(agents::judge::judge(&spec, &state, None));
    let d = agents::root(home).join(&spec.id);
    std::fs::write(d.join("state.json"), serde_json::to_vec(&state).unwrap()).unwrap();
    (spec, state)
}

/// A scratch run that failed at once, judged, so its state is final.
fn finished(home: &Path) -> String {
    run(home, request(Harness::Claude, FAKE_CLAUDE, None, "FAIL")).0.id
}

/// A run recorded as still live under `owner`, without launching a harness.
fn running(home: &Path, req: Request, owner: i32) -> Spec {
    let spec = agents::prepare_spec(&at(home), req).unwrap();
    let state = State {
        status: Status::Running,
        supervisor: Some(owner),
        ..State::default()
    };
    std::fs::write(
        agents::root(home).join(&spec.id).join("state.json"),
        serde_json::to_vec(&state).unwrap(),
    )
    .unwrap();
    spec
}

/// A real, live `cos` process standing in for a run's observer: it blocks on a
/// build lock the test holds, so `is_cos` sees a killable `cos` binary. Returned
/// with the held lock; dropping the lock lets it exit, so keep it for the test.
fn owner(root: &Path, tag: &str) -> (std::process::Child, File) {
    let lock = root.join(format!("owner-{tag}.lock"));
    let held = File::create(&lock).unwrap();
    // SAFETY: flock on a descriptor the test owns.
    assert_eq!(unsafe { libc::flock(held.as_raw_fd(), libc::LOCK_EX) }, 0);
    let child = Command::new(COS)
        .args(["agents", "build-slot", "--lock"])
        .arg(&lock)
        .args(["--", "/bin/true"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let until = Instant::now() + Duration::from_secs(5);
    while !cos::proc::alive(child.id() as i32) {
        assert!(Instant::now() < until, "owner never started");
        std::thread::sleep(Duration::from_millis(10));
    }
    (child, held)
}

#[test]
fn a_claude_hire_runs_in_its_worktree_and_records_the_run() {
    let root = scratch("done");
    let (home, repo) = (root.join("home"), repo(&root));
    let (spec, st) = run(
        &home,
        request(Harness::Claude, FAKE_CLAUDE, Some(repo.clone()), "edit a file"),
    );
    assert_eq!(st.status, Status::Done, "{st:?}");
    assert_eq!(st.cost_usd, Some(0.12));
    assert_eq!(st.tool_calls, 1);
    assert_eq!(st.session.as_deref(), Some("sess-1"));
    assert_eq!(st.summary, "Changed one file.");
    assert_ne!(spec.workdir, repo);
    assert!(spec.workdir.join("made-by-agent.txt").exists());
    assert!(
        !repo.join("made-by-agent.txt").exists(),
        "agent wrote into the main checkout"
    );
}

#[test]
fn clean_refuses_a_worktree_holding_work_unless_forced() {
    let root = scratch("clean");
    let (home, repo) = (root.join("home"), repo(&root));
    let (spec, _) = run(
        &home,
        request(Harness::Claude, FAKE_CLAUDE, Some(repo), "edit a file"),
    );

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
fn a_codex_hire_reports_tokens_and_its_final_message() {
    let root = scratch("codex");
    let (home, repo) = (root.join("home"), repo(&root));
    let (_, st) = run(
        &home,
        request(Harness::Codex, FAKE_CODEX, Some(repo), "review the parser"),
    );
    assert_eq!(st.status, Status::Done, "{st:?}");
    assert_eq!(st.tokens, Some(1500));
    assert_eq!(st.summary, "Added the file.");
    assert_eq!(st.session.as_deref(), Some("thread-9"));
}

#[test]
fn stop_relays_to_herdr_and_records_the_run_killed() {
    let root = scratch("kill");
    let home = root.join("home");
    let (mut child, _held) = owner(&root, "kill");
    let spec = running(
        &home,
        request(Harness::Claude, FAKE_CLAUDE, None, "SLEEP"),
        child.id() as i32,
    );

    let pid = child.id() as i32;
    let st = agents::stop(&home, &spec.id, Duration::from_millis(300)).unwrap();
    assert_eq!(st.status, Status::Killed);
    // stop SIGKILLed the observer; reap the zombie before reading its liveness.
    let _ = child.wait();
    assert!(!cos::proc::alive(pid), "the observer was left running");
}

#[test]
fn an_error_result_marks_the_run_failed_with_the_reason() {
    let root = scratch("fail");
    let home = root.join("home");
    let (_, st) = run(&home, request(Harness::Claude, FAKE_CLAUDE, None, "FAIL"));
    assert_eq!(st.status, Status::Failed);
    assert!(
        st.error.as_deref().unwrap_or("").contains("budget spent"),
        "{st:?}"
    );
}

#[test]
fn a_dead_observer_is_reported_lost() {
    let root = scratch("lost");
    let home = root.join("home");
    let (mut child, _held) = owner(&root, "lost");
    let spec = running(
        &home,
        request(Harness::Claude, FAKE_CLAUDE, None, "SLEEP"),
        child.id() as i32,
    );
    let dead = child.id() as i32;
    // SAFETY: the owner this test spawned; no memory is touched.
    unsafe {
        libc::kill(dead, libc::SIGKILL);
    }
    let _ = child.wait();
    wait_dead(dead);

    let (_, st) = agents::read(&home, &spec.id).unwrap();
    assert_eq!(st.status, Status::Lost);
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
        assert!(agents::clean(&home, id, true).is_err(), "clean {id:?}");
    }

    assert_eq!(state_bytes(&home, &good), good_before);
    assert_eq!(state_bytes(&other_home, &other), other_before);
    assert_eq!(std::fs::read(victim.join("state.json")).unwrap(), b"victim");
    assert!(agents_dir.join(&good).is_dir());
    assert!(agents::read(&home, &good).is_ok());
}

#[test]
fn spec_id_mismatch_refused() {
    let root = scratch("forged");
    let home = root.join("home");
    let (mut child, _held) = owner(&root, "forged");
    let spec = running(
        &home,
        request(Harness::Claude, FAKE_CLAUDE, None, "SLEEP"),
        child.id() as i32,
    );
    let before = state_bytes(&home, &spec.id);

    let forged = home.join("out/agents/forged");
    std::fs::create_dir_all(&forged).unwrap();
    std::fs::copy(
        home.join("out/agents").join(&spec.id).join("spec.json"),
        forged.join("spec.json"),
    )
    .unwrap();

    assert!(agents::read(&home, "forged").is_err());
    assert!(agents::stop(&home, "forged", Duration::from_millis(100)).is_err());
    assert_eq!(state_bytes(&home, &spec.id), before);
    assert!(
        cos::proc::alive(child.id() as i32),
        "the observer of {} was killed",
        spec.id
    );

    // SAFETY: the owner this test spawned; no memory is touched.
    unsafe {
        libc::kill(child.id() as i32, libc::SIGKILL);
    }
    let _ = child.wait();
}

#[test]
fn list_skips_refused_runs() {
    let root = scratch("skips");
    let home = root.join("home");
    let done = finished(&home);
    let (mut child, _held) = owner(&root, "skips");
    let live = running(
        &home,
        request(Harness::Claude, FAKE_CLAUDE, None, "SLEEP"),
        child.id() as i32,
    );

    let agents_dir = home.join("out/agents");
    std::os::unix::fs::symlink(agents_dir.join(&done), agents_dir.join("linked")).unwrap();
    std::fs::create_dir_all(agents_dir.join("stale")).unwrap();
    std::fs::copy(
        agents_dir.join(&done).join("spec.json"),
        agents_dir.join("stale/spec.json"),
    )
    .unwrap();

    let listing = agents::list(&home).unwrap();
    let mut listed: Vec<&str> = listing.runs.iter().map(|(s, _)| s.id.as_str()).collect();
    listed.sort_unstable();
    let mut expected = [done.as_str(), live.id.as_str()];
    expected.sort_unstable();
    assert_eq!(listed, expected);
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
    assert!(
        stderr.contains("linked") && stderr.contains("stale"),
        "{stderr}"
    );
    assert!(!agents_dir.join(&done).exists(), "finished run kept");
    assert!(agents_dir.join(&live.id).exists(), "running run removed");
    assert!(agents_dir.join("stale").exists(), "skipped id removed");

    // SAFETY: the owner this test spawned; no memory is touched.
    unsafe {
        libc::kill(child.id() as i32, libc::SIGKILL);
    }
    let _ = child.wait();
}

#[test]
fn watch_waits_on_skipped() {
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

    for verb in ["show", "kill"] {
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
                cos::mcp::AgentId { id: "../x".into() },
            )),
        );
    assert_eq!(res.is_error, Some(true), "{res:?}");
    assert!(!victim.join("stop").exists(), "stop_agent reached ../x");
    assert_eq!(std::fs::read(victim.join("state.json")).unwrap(), b"victim");
    assert_eq!(state_bytes(&home, &good), good_before);
}

#[test]
fn rejudge_each_run_kind() {
    let root = scratch("rejudge");
    let (home, repo) = (root.join("home"), repo(&root));
    let mut ids = Vec::new();
    ids.push(
        run(
            &home,
            request(Harness::Claude, FAKE_CLAUDE, Some(repo.clone()), "edit a file"),
        )
        .0
        .id,
    );
    ids.push(run(&home, request(Harness::Claude, FAKE_CLAUDE, None, "FAIL")).0.id);
    let mut in_place = request(Harness::Claude, FAKE_CLAUDE, Some(repo), "FAIL");
    in_place.worktree = false;
    ids.push(run(&home, in_place).0.id);

    for id in &ids {
        agents::rejudge(&home, id).unwrap_or_else(|e| panic!("rejudge {id}: {e:#}"));
        let (_, st) = agents::read(&home, id).unwrap();
        assert!(st.verdict.is_some(), "{id} has no verdict after rejudge");
    }
}

#[test]
fn clean_with_tree_gone() {
    let root = scratch("treegone");
    let (home, repo) = (root.join("home"), repo(&root));
    let (spec, _) = run(
        &home,
        request(Harness::Claude, FAKE_CLAUDE, Some(repo), "edit a file"),
    );
    std::fs::remove_dir_all(&spec.workdir).unwrap();

    agents::clean(&home, &spec.id, false).unwrap();
    assert!(!home.join("out/agents").join(&spec.id).exists());
}

#[test]
fn classify_matches_reconcile() {
    let root = scratch("classify");
    let home = root.join("home");
    let (mut child, _held) = owner(&root, "classify");
    let spec = running(
        &home,
        request(Harness::Claude, FAKE_CLAUDE, None, "SLEEP"),
        child.id() as i32,
    );
    let (_, st) = agents::read(&home, &spec.id).unwrap();
    assert_eq!(agents::classify(&spec, &st), Liveness::Live);

    // SAFETY: the owner this test spawned; no memory is touched.
    unsafe {
        libc::kill(child.id() as i32, libc::SIGKILL);
    }
    let _ = child.wait();
    let on_disk: State = serde_json::from_slice(&state_bytes(&home, &spec.id)).unwrap();
    assert_eq!(agents::classify(&spec, &on_disk), Liveness::Lost);

    let (_, repaired) = agents::read(&home, &spec.id).unwrap();
    assert_eq!(repaired.status, Status::Lost);
    assert_eq!(
        agents::classify(&spec, &State::default()),
        Liveness::Starting
    );
}

#[test]
fn count_is_read_only() {
    let root = scratch("count");
    let home = root.join("home");
    let (mut child, _held) = owner(&root, "count");
    let spec = running(
        &home,
        request(Harness::Claude, FAKE_CLAUDE, None, "SLEEP"),
        child.id() as i32,
    );
    assert_eq!(
        agents::count(&home).unwrap(),
        agents::Counts {
            live: 1,
            lost: 0,
            starting: 0,
            unreadable: 0
        }
    );

    // SAFETY: the owner this test spawned; no memory is touched.
    unsafe {
        libc::kill(child.id() as i32, libc::SIGKILL);
    }
    let _ = child.wait();

    let before = state_bytes(&home, &spec.id);
    assert_eq!(
        agents::count(&home).unwrap(),
        agents::Counts {
            live: 0,
            lost: 1,
            starting: 0,
            unreadable: 0
        }
    );
    assert_eq!(state_bytes(&home, &spec.id), before, "count wrote a record");

    let bad = finished(&home);
    std::fs::write(
        home.join("out/agents").join(&bad).join("state.json"),
        b"{ not json",
    )
    .unwrap();
    assert_eq!(
        agents::count(&home).unwrap(),
        agents::Counts {
            live: 0,
            lost: 1,
            starting: 0,
            unreadable: 1
        }
    );
}

#[test]
fn count_refuses_a_symlinked_run() {
    let root = scratch("count-link");
    let home = root.join("home");
    let other_home = root.join("other");
    let other = finished(&other_home);
    let target = other_home.join("out/agents").join(&other);
    let stale = State {
        status: Status::Running,
        ..State::default()
    };
    std::fs::write(
        target.join("state.json"),
        serde_json::to_vec(&stale).unwrap(),
    )
    .unwrap();
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
fn count_reports_a_starting_run_as_starting_and_live() {
    let root = scratch("count-starting");
    let home = root.join("home");
    let id = finished(&home);
    let starting = State {
        status: Status::Running,
        ..State::default()
    };
    std::fs::write(
        home.join("out/agents").join(&id).join("state.json"),
        serde_json::to_vec(&starting).unwrap(),
    )
    .unwrap();

    assert_eq!(
        agents::count(&home).unwrap(),
        agents::Counts {
            live: 1,
            lost: 0,
            starting: 1,
            unreadable: 0
        }
    );
}

#[test]
fn live_observers_holds_live_pids_only_and_writes_nothing() {
    let root = scratch("observers");
    let home = root.join("home");
    let (mut live_child, _live_held) = owner(&root, "live");
    let (mut dead_child, _dead_held) = owner(&root, "dead");
    let live = running(
        &home,
        request(Harness::Claude, FAKE_CLAUDE, None, "SLEEP"),
        live_child.id() as i32,
    );
    let lost = running(
        &home,
        request(Harness::Claude, FAKE_CLAUDE, None, "SLEEP"),
        dead_child.id() as i32,
    );

    // SAFETY: the owner this test spawned; no memory is touched.
    unsafe {
        libc::kill(dead_child.id() as i32, libc::SIGKILL);
    }
    let _ = dead_child.wait();

    let before = (state_bytes(&home, &live.id), state_bytes(&home, &lost.id));
    assert_eq!(agents::live_supervisors(&home), vec![live_child.id() as i32]);
    assert_eq!(
        (state_bytes(&home, &live.id), state_bytes(&home, &lost.id)),
        before,
        "live_supervisors wrote a record"
    );

    // SAFETY: the owner this test spawned; no memory is touched.
    unsafe {
        libc::kill(live_child.id() as i32, libc::SIGKILL);
    }
    let _ = live_child.wait();
}

#[test]
fn clean_waits_for_the_judging_observer() {
    let root = scratch("clean-judging");
    let home = root.join("home");
    let (mut child, _held) = owner(&root, "judging");
    let spec = agents::prepare_spec(&at(&home), request(Harness::Claude, FAKE_CLAUDE, None, "edit a file")).unwrap();
    // Ended, but the observer is still recording its verdict.
    let ending = State {
        status: Status::Done,
        supervisor: Some(child.id() as i32),
        ..State::default()
    };
    std::fs::write(
        agents::root(&home).join(&spec.id).join("state.json"),
        serde_json::to_vec(&ending).unwrap(),
    )
    .unwrap();

    let pid = child.id() as i32;
    let judging = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(300));
        // SAFETY: the owner the test spawned; no memory is touched.
        unsafe {
            libc::kill(pid, libc::SIGKILL);
        }
    });
    let started = Instant::now();
    agents::clean(&home, &spec.id, true).unwrap();
    judging.join().unwrap();
    // The judging thread SIGKILLed the observer; reap it before reading liveness.
    let _ = child.wait();

    assert!(
        started.elapsed() >= Duration::from_millis(250),
        "clean returned before the observer finished judging"
    );
    assert!(!cos::proc::alive(pid), "the observer was left running");
    assert!(
        !home.join("out/agents").join(&spec.id).exists(),
        "run dir not removed"
    );
}
