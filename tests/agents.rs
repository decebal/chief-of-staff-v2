//! `cos agents` end to end against stand-in harnesses: the real supervisor
//! binary, real process groups, real git worktrees.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use cos::agents::{self, Harness, Liveness, Request, State, Status};
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

fn wait_for(home: &Path, id: &str, done: impl Fn(&State) -> bool) -> State {
    let until = Instant::now() + Duration::from_secs(20);
    loop {
        let (_, st) = agents::read(home, id).unwrap();
        if done(&st) {
            return st;
        }
        assert!(
            Instant::now() < until,
            "agent {id} stuck at {:?}",
            st.status
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn wait_dead(pid: i32) {
    let until = Instant::now() + Duration::from_secs(5);
    while cos::proc::alive(pid) {
        assert!(Instant::now() < until, "pid {pid} still alive");
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn sleeper(workdir: &Path) -> i32 {
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        if let Ok(s) = std::fs::read_to_string(workdir.join("sleeper.pid"))
            && let Ok(pid) = s.trim().parse()
        {
            return pid;
        }
        assert!(Instant::now() < until, "sleeper never started");
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn claude_agent_works_in_its_own_worktree_and_records_the_run() {
    let root = scratch("done");
    let (home, repo) = (root.join("home"), repo(&root));
    let spec = agents::spawn(
        &at(&home),
        request(
            Harness::Claude,
            FAKE_CLAUDE,
            Some(repo.clone()),
            "edit a file",
        ),
        Path::new(COS),
    )
    .unwrap();

    let st = wait_for(&home, &spec.id, |s| s.status.terminal());
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
    let spec = agents::spawn(
        &at(&home),
        request(
            Harness::Claude,
            FAKE_CLAUDE,
            Some(repo.clone()),
            "edit a file",
        ),
        Path::new(COS),
    )
    .unwrap();
    wait_for(&home, &spec.id, |s| s.status.terminal());

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
fn codex_agent_reports_tokens_and_its_final_message() {
    let root = scratch("codex");
    let (home, repo) = (root.join("home"), repo(&root));
    let spec = agents::spawn(
        &at(&home),
        request(Harness::Codex, FAKE_CODEX, Some(repo), "review the parser"),
        Path::new(COS),
    )
    .unwrap();
    let st = wait_for(&home, &spec.id, |s| s.status.terminal());
    assert_eq!(st.status, Status::Done, "{st:?}");
    assert_eq!(st.tokens, Some(1500));
    assert_eq!(st.summary, "Added the file.");
    assert_eq!(st.session.as_deref(), Some("thread-9"));
}

#[test]
fn kill_stops_the_harness_and_everything_it_started() {
    let root = scratch("kill");
    let (home, repo) = (root.join("home"), repo(&root));
    let spec = agents::spawn(
        &at(&home),
        request(Harness::Claude, FAKE_CLAUDE, Some(repo), "SLEEP"),
        Path::new(COS),
    )
    .unwrap();
    wait_for(&home, &spec.id, |s| s.status == Status::Running);
    let pid = sleeper(&spec.workdir);

    let st = agents::stop(&home, &spec.id, Duration::from_secs(10)).unwrap();
    assert_eq!(st.status, Status::Killed);
    wait_dead(pid);
}

#[test]
fn an_agent_past_its_deadline_is_stopped() {
    let root = scratch("deadline");
    let home = root.join("home");
    let mut req = request(Harness::Claude, FAKE_CLAUDE, None, "SLEEP");
    req.deadline = Duration::from_secs(1);
    let spec = agents::spawn(&at(&home), req, Path::new(COS)).unwrap();
    let pid = sleeper(&spec.workdir);
    let st = wait_for(&home, &spec.id, |s| s.status.terminal());
    assert_eq!(st.status, Status::TimedOut);
    wait_dead(pid);
}

#[test]
fn an_error_result_marks_the_agent_failed_with_the_reason() {
    let root = scratch("fail");
    let home = root.join("home");
    let spec = agents::spawn(
        &at(&home),
        request(Harness::Claude, FAKE_CLAUDE, None, "FAIL"),
        Path::new(COS),
    )
    .unwrap();
    let st = wait_for(&home, &spec.id, |s| s.status.terminal());
    assert_eq!(st.status, Status::Failed);
    assert!(
        st.error.as_deref().unwrap_or("").contains("budget spent"),
        "{st:?}"
    );
}

#[test]
fn a_dead_supervisor_is_reported_lost_and_its_harness_stopped() {
    let root = scratch("lost");
    let home = root.join("home");
    let spec = agents::spawn(
        &at(&home),
        request(Harness::Claude, FAKE_CLAUDE, None, "SLEEP"),
        Path::new(COS),
    )
    .unwrap();
    let st = wait_for(&home, &spec.id, |s| s.status == Status::Running);
    let pid = sleeper(&spec.workdir);
    let supervisor = st.supervisor.unwrap();
    // SAFETY: the supervisor this test spawned; no memory is touched.
    unsafe {
        libc::kill(supervisor, libc::SIGKILL);
    }
    wait_dead(supervisor);

    let st = wait_for(&home, &spec.id, |s| s.status.terminal());
    assert_eq!(st.status, Status::Lost);
    assert!(
        st.error
            .as_deref()
            .unwrap_or("")
            .contains("stopped its harness group"),
        "{st:?}"
    );
    wait_dead(pid);
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

/// A scratch run that failed at once, judged, so its state is final.
fn finished(home: &Path) -> String {
    let spec = agents::spawn(
        &at(home),
        request(Harness::Claude, FAKE_CLAUDE, None, "FAIL"),
        Path::new(COS),
    )
    .unwrap();
    wait_for(home, &spec.id, |s| s.verdict.is_some());
    spec.id
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
    let spec = agents::spawn(
        &at(&home),
        request(Harness::Claude, FAKE_CLAUDE, None, "SLEEP"),
        Path::new(COS),
    )
    .unwrap();
    wait_for(&home, &spec.id, |s| s.status == Status::Running);
    let pid = sleeper(&spec.workdir);
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
    assert!(cos::proc::alive(pid), "harness of {} was killed", spec.id);

    agents::stop(&home, &spec.id, Duration::from_secs(10)).unwrap();
}

#[test]
fn list_skips_refused_runs() {
    let root = scratch("skips");
    let home = root.join("home");
    let done = finished(&home);
    let sleeping = agents::spawn(
        &at(&home),
        request(Harness::Claude, FAKE_CLAUDE, None, "SLEEP"),
        Path::new(COS),
    )
    .unwrap();
    wait_for(&home, &sleeping.id, |s| s.status == Status::Running);
    let pid = sleeper(&sleeping.workdir);

    assert!(cos::proc::alive(pid), "the running agent is not running");
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
    let mut expected = [done.as_str(), sleeping.id.as_str()];
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
    assert!(
        agents_dir.join(&sleeping.id).exists(),
        "running run removed"
    );
    assert!(agents_dir.join("stale").exists(), "skipped id removed");

    agents::stop(&home, &sleeping.id, Duration::from_secs(10)).unwrap();
    wait_dead(pid);
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
        agents::spawn(
            &at(&home),
            request(
                Harness::Claude,
                FAKE_CLAUDE,
                Some(repo.clone()),
                "edit a file",
            ),
            Path::new(COS),
        )
        .unwrap()
        .id,
    );
    ids.push(
        agents::spawn(
            &at(&home),
            request(Harness::Claude, FAKE_CLAUDE, None, "FAIL"),
            Path::new(COS),
        )
        .unwrap()
        .id,
    );
    let mut in_place = request(Harness::Claude, FAKE_CLAUDE, Some(repo), "FAIL");
    in_place.worktree = false;
    ids.push(
        agents::spawn(&at(&home), in_place, Path::new(COS))
            .unwrap()
            .id,
    );

    for id in &ids {
        wait_for(&home, id, |s| s.verdict.is_some());
        agents::rejudge(&home, id).unwrap_or_else(|e| panic!("rejudge {id}: {e:#}"));
        let (_, st) = agents::read(&home, id).unwrap();
        assert!(st.verdict.is_some(), "{id} has no verdict after rejudge");
    }
}

#[test]
fn clean_with_tree_gone() {
    let root = scratch("treegone");
    let (home, repo) = (root.join("home"), repo(&root));
    let spec = agents::spawn(
        &at(&home),
        request(Harness::Claude, FAKE_CLAUDE, Some(repo), "edit a file"),
        Path::new(COS),
    )
    .unwrap();
    wait_for(&home, &spec.id, |s| s.verdict.is_some());
    std::fs::remove_dir_all(&spec.workdir).unwrap();

    agents::clean(&home, &spec.id, false).unwrap();
    assert!(!home.join("out/agents").join(&spec.id).exists());
}

#[test]
fn classify_matches_reconcile() {
    let root = scratch("classify");
    let home = root.join("home");
    let spec = agents::spawn(
        &at(&home),
        request(Harness::Claude, FAKE_CLAUDE, None, "SLEEP"),
        Path::new(COS),
    )
    .unwrap();
    let st = wait_for(&home, &spec.id, |s| s.status == Status::Running);
    assert_eq!(agents::classify(&spec, &st), Liveness::Live);
    let pid = sleeper(&spec.workdir);

    let supervisor = st.supervisor.unwrap();
    // SAFETY: the supervisor this test spawned; no memory is touched.
    unsafe {
        libc::kill(supervisor, libc::SIGKILL);
    }
    wait_dead(supervisor);
    let on_disk: State = serde_json::from_slice(&state_bytes(&home, &spec.id)).unwrap();
    assert_eq!(agents::classify(&spec, &on_disk), Liveness::Lost);

    let repaired = wait_for(&home, &spec.id, |s| s.status.terminal());
    assert_eq!(repaired.status, Status::Lost);
    wait_dead(pid);
    assert_eq!(
        agents::classify(&spec, &State::default()),
        Liveness::Starting
    );
}

#[test]
fn count_is_read_only() {
    let root = scratch("count");
    let home = root.join("home");
    let spec = agents::spawn(
        &at(&home),
        request(Harness::Claude, FAKE_CLAUDE, None, "SLEEP"),
        Path::new(COS),
    )
    .unwrap();
    let st = wait_for(&home, &spec.id, |s| s.status == Status::Running);
    assert_eq!(
        agents::count(&home).unwrap(),
        agents::Counts {
            live: 1,
            lost: 0,
            unreadable: 0
        }
    );
    let pid = sleeper(&spec.workdir);

    let supervisor = st.supervisor.unwrap();
    // SAFETY: the supervisor this test spawned; no memory is touched.
    unsafe {
        libc::kill(supervisor, libc::SIGKILL);
    }
    wait_dead(supervisor);

    let before = state_bytes(&home, &spec.id);
    assert_eq!(
        agents::count(&home).unwrap(),
        agents::Counts {
            live: 0,
            lost: 1,
            unreadable: 0
        }
    );
    assert_eq!(state_bytes(&home, &spec.id), before, "count wrote a record");
    assert!(cos::proc::alive(pid), "count stopped the harness");

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
            unreadable: 1
        }
    );

    agents::read(&home, &spec.id).unwrap();
    wait_dead(pid);
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
fn spawn_pins_triple_in_env() {
    let root = scratch("pins");
    let home = root.join("home");
    let workspace = root.join("workspace");
    let projects = root.join("projects");
    for dir in [&home, &workspace, &projects] {
        std::fs::create_dir_all(dir).unwrap();
    }
    let out = root.join("business-list.txt");
    let stub = root.join("stub-harness.sh");
    std::fs::write(
        &stub,
        format!(
            "#!/bin/sh\nCOS_BUSINESSES='{}' '{COS}' business list > '{}' 2>&1\nsleep 3\n",
            root.join("no-registry.toml").display(),
            out.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&stub, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();

    let triple = Triple {
        home: home.clone(),
        workspace: workspace.clone(),
        projects: projects.clone(),
        business: Some("acme".into()),
        layer: Layer::Flags,
    };
    // SAFETY: test-only env edit, removed below before any other read.
    unsafe {
        std::env::set_var("COS_BUSINESS", "acme");
        std::env::set_var("COS_WORKSPACE", root.join("stale-workspace"));
    }
    let spawned = agents::spawn(
        &triple,
        request(Harness::Claude, stub.to_str().unwrap(), None, "list"),
        Path::new(COS),
    );
    unsafe {
        std::env::remove_var("COS_BUSINESS");
        std::env::remove_var("COS_WORKSPACE");
    }
    let spec = spawned.unwrap();

    wait_for(&home, &spec.id, |s| s.status == Status::Running);
    wait_for(&home, &spec.id, |s| s.status.terminal());
    let listed = std::fs::read_to_string(&out).unwrap();
    assert!(listed.contains("current: explicit"), "{listed}");
    assert!(
        listed.contains(&format!("home={}", home.display())),
        "{listed}"
    );
    assert!(
        listed.contains(&format!("workspace={}", workspace.display())),
        "{listed}"
    );
    assert!(
        listed.contains(&format!("projects={}", projects.display())),
        "{listed}"
    );
}

#[test]
fn clean_waits_for_the_judging_supervisor() {
    let root = scratch("clean-judging");
    let home = root.join("home");
    std::fs::create_dir_all(&home).unwrap();
    let stub = root.join("quick-harness.sh");
    std::fs::write(&stub, "#!/bin/sh\nexit 0\n").unwrap();
    std::fs::set_permissions(&stub, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let triple = Triple {
        home: home.clone(),
        workspace: home.clone(),
        projects: root.clone(),
        business: None,
        layer: Layer::Flags,
    };
    let mut req = request(
        Harness::Claude,
        stub.to_str().unwrap(),
        None,
        "judge slowly",
    );
    req.contract.checks = vec!["sleep 2".into()];
    let spec = agents::spawn(&triple, req, Path::new(COS)).unwrap();

    let ended = wait_for(&home, &spec.id, |s| s.status.terminal());
    assert!(ended.verdict.is_none(), "judge finished before clean ran");
    let supervisor = ended.supervisor.expect("supervisor pid recorded");
    agents::clean(&home, &spec.id, true).unwrap();

    assert!(
        !cos::proc::alive(supervisor),
        "clean returned while the supervisor was still running"
    );
    let run = home.join("out/agents").join(&spec.id);
    assert!(!run.exists(), "run dir not removed");
}

#[test]
fn spawn_pins_relative_dirs_absolute() {
    let root = scratch("relative");
    let home = root.join("home");
    let workspace = root.join("workspace");
    let projects = root.join("projects");
    for dir in [&home, &workspace, &projects] {
        std::fs::create_dir_all(dir).unwrap();
    }
    let up = "../".repeat(std::env::current_dir().unwrap().components().count() - 1);
    let relative = |p: &Path| PathBuf::from(&up).join(p.strip_prefix("/").unwrap());
    let out = root.join("business-list.txt");
    let stub = root.join("stub-harness.sh");
    std::fs::write(
        &stub,
        format!(
            "#!/bin/sh\nCOS_BUSINESSES='{}' '{COS}' business list > '{}' 2>&1\nsleep 3\n",
            root.join("no-registry.toml").display(),
            out.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&stub, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();

    let triple = Triple {
        home: home.clone(),
        workspace: relative(&workspace),
        projects: relative(&projects),
        business: None,
        layer: Layer::Flags,
    };
    let spec = agents::spawn(
        &triple,
        request(Harness::Claude, stub.to_str().unwrap(), None, "list"),
        Path::new(COS),
    )
    .unwrap();

    wait_for(&home, &spec.id, |s| s.status.terminal());
    let listed = std::fs::read_to_string(&out).unwrap();
    let current = listed
        .lines()
        .find(|l| l.starts_with("current:"))
        .unwrap_or_else(|| panic!("no current: line in {listed}"));
    for (key, want) in [("workspace=", &workspace), ("projects=", &projects)] {
        let got = current
            .split_whitespace()
            .find_map(|w| w.strip_prefix(key))
            .unwrap_or_else(|| panic!("no {key} in {listed}"));
        assert!(Path::new(got).is_absolute(), "{key}{got}");
        assert_eq!(
            std::fs::canonicalize(got).unwrap(),
            std::fs::canonicalize(want).unwrap()
        );
    }
}
