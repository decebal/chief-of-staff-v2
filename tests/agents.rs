//! `cos agents` end to end against stand-in harnesses: the real supervisor
//! binary, real process groups, real git worktrees.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use cos::agents::{self, Harness, Request, State, Status};

const COS: &str = env!("CARGO_BIN_EXE_cos");
const FAKE_CLAUDE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/fake-claude.sh");
const FAKE_CODEX: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/fake-codex.sh");

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("cos-agents-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d.canonicalize().unwrap()
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
        &home,
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
        &home,
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
        &home,
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
        &home,
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
    let spec = agents::spawn(&home, req, Path::new(COS)).unwrap();
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
        &home,
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
        &home,
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
        calendar: vec![],
        backlog: vec![],
        coverage: vec![],
        gaps: vec![],
        upcoming: vec![],
    };
    cos::store::save(&root, cos::store::Kind::Briefs, "2026-10-16", &brief, "").unwrap();
    let tasks = agents::tasks(&root, agents::From::Brief).unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!((tasks[0].n, tasks[0].title.as_str()), (2, "fix CI"));
    assert_eq!(tasks[0].source, "brief 2026-10-16 #2");
}
