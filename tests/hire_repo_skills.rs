//! A hire into a repo that carries its own skill graph (`.allsource/`): the
//! role's repo skills planned, built at launch in the seeded worktree and
//! bundled, end to end with the real `allsource-skills`
//! (`docs/plans/2026-11-28-skill-graph-design.md`, the cos layer). Herdr is
//! the stand-in (`fake_herdr`). The real tool is found by `COS_ALLSOURCE_SKILLS`
//! or on `PATH`; when neither holds one, the end-to-end tests fail. The stand-in
//! tool (`tests/fixtures/fake-allsource-skills.sh`) covers install failures.

#[allow(dead_code)]
mod fake_herdr;

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use cos::agents::hire::{self, Ask, Plan, SkillsAsk};
use cos::agents::policy::Explicit;
use cos::agents::repo_skills::{self, BIN_ENV};
use cos::agents::skill_facts::Mode;
use cos::agents::skills::{self, Why};
use cos::agents::{self, Contract, Harness, Spec, Task};
use fake_herdr::{Fake, agent, fake};

const COS: &str = env!("CARGO_BIN_EXE_cos");
const STALE_LINE: &str = "repo skills: built at launch";
const POLICY: &str = r#"{ "defaults": { "checks": ["true"], "deadline_secs": 900 } }"#;

fn at(home: &Path) -> cos::business::Triple {
    cos::business::Triple {
        home: home.to_owned(),
        workspace: home.to_owned(),
        projects: home.join("projects"),
        business: None,
        layer: cos::business::Layer::Flags,
    }
}

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("cos-hire-repo-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d.canonicalize().unwrap()
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// The real `allsource-skills`, looked up on `PATH` alone: `BIN_ENV` is
/// set by the stand-in tests in this process, so it cannot name the real one.
/// Missing on `PATH` fails the test; it is never skipped.
fn real_tool() -> PathBuf {
    repo_skills::locate(None, std::env::var_os("PATH").as_deref())
        .expect("allsource-skills not on PATH: install it, the end-to-end hire tests drive it")
}

/// Points the hires at `tool`. The `Fake` from `fake()` holds the lock that
/// guards this process's environment.
fn use_tool(_held: &Fake, tool: &Path) {
    // SAFETY: the caller holds `fake_herdr`'s lock through `f`.
    unsafe { std::env::set_var(BIN_ENV, tool) }
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

/// An initialised repo with the ignores a graph repo carries, one commit.
fn repo_at(root: &Path) -> PathBuf {
    let repo = root.join("projects/graph-repo");
    std::fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    std::fs::write(
        repo.join(".gitignore"),
        ".prime/\n.allsource/generated.json\n",
    )
    .unwrap();
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", "init"]);
    repo
}

fn commit(repo: &Path) {
    git(repo, &["add", "-A"]);
    git(repo, &["commit", "-q", "-m", "graph"]);
}

/// A repo whose graph holds `tdd` (scaffolded by the real tool) for `builder`.
fn graph_repo(root: &Path, tool: &Path) -> PathBuf {
    let repo = repo_at(root);
    std::fs::create_dir_all(repo.join(".allsource")).unwrap();
    std::fs::write(
        repo.join(".allsource/skills.toml"),
        "version = 1\ntool = \"0.1.0\"\ntargets = [\"claude\", \"agents\"]\n",
    )
    .unwrap();
    run_tool(tool, &repo, &["new", "tdd"]);
    std::fs::write(
        repo.join(".allsource/roles.toml"),
        "version = 1\n\n[roles.builder]\nskills = [\"tdd\"]\n",
    )
    .unwrap();
    commit(&repo);
    repo
}

fn run_tool(tool: &Path, repo: &Path, args: &[&str]) {
    let out = Command::new(tool)
        .arg("--repo")
        .arg(repo)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "allsource-skills {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// The stand-in tool, with its own config files, in `root/tool`.
fn stand_in(root: &Path, skills: &str) -> PathBuf {
    let dir = root.join("tool");
    std::fs::create_dir_all(&dir).unwrap();
    let bin = dir.join("allsource-skills");
    std::fs::copy(fixture("fake-allsource-skills.sh"), &bin).unwrap();
    std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::write(dir.join("skills"), skills).unwrap();
    bin
}

fn home(root: &Path) -> PathBuf {
    let home = root.join("home");
    std::fs::create_dir_all(home.join("context")).unwrap();
    std::fs::write(home.join("context/agents.json"), POLICY).unwrap();
    home
}

fn plan(home: &Path, repo: &Path, role: Option<&str>) -> Plan {
    hire::plan(
        &hire::Where {
            home,
            workspace: home,
            projects: repo.parent().unwrap(),
        },
        role,
        Ask::Task(Task {
            n: 0,
            title: "t".into(),
            prompt: "Implement the cache".into(),
            source: "prompt".into(),
        }),
        Some(repo.to_path_buf()),
        &Contract::default(),
        &SkillsAsk::from_env(Default::default(), Mode::Live),
    )
    .unwrap()
}

fn run() -> agents::hire::Run {
    run_as(Harness::Claude)
}

fn run_as(harness: Harness) -> agents::hire::Run {
    agents::hire::Run {
        harness,
        program: Some(agent()),
        model: None,
        worktree: true,
        deadline: Duration::from_secs(30),
        slots: Some(8),
        explicit: Explicit::default(),
    }
}

fn picked<'a>(spec: &'a Spec, name: &str) -> Option<&'a skills::Picked> {
    spec.skills.as_ref()?.picked.iter().find(|p| p.name == name)
}

#[test]
fn a_fresh_repo_plans_its_role_skill_as_a_repo_pick_ahead_of_the_users_copy() {
    let root = scratch("plan-fresh");
    let f = fake(&root);
    let tool = real_tool();
    use_tool(&f, &tool);
    let h = home(&root);
    let repo = graph_repo(&root, &tool);
    run_tool(&tool, &repo, &["install"]);
    let user = root.join("user-home/.claude/skills/tdd");
    std::fs::create_dir_all(&user).unwrap();
    std::fs::write(user.join("SKILL.md"), "---\ndescription: mine\n---\n").unwrap();

    let p = plan(&h, &repo, Some("builder"));
    let set = p.skills.as_ref().expect("skills resolved");
    let tdd = set
        .picked
        .iter()
        .find(|s| s.name == "tdd")
        .expect("tdd picked");
    assert_eq!(tdd.why, Why::RepoRole);
    assert_eq!(tdd.root.as_deref(), Some("repo"));
    assert!(
        set.warnings
            .iter()
            .any(|w| w.contains("the repo copy is used")),
        "{:?}",
        set.warnings
    );
    assert!(
        !set.warnings.iter().any(|w| w.starts_with("repo skills:")),
        "a fresh store says nothing: {:?}",
        set.warnings
    );
    let json = serde_json::to_value(&p).unwrap();
    assert_eq!(json["skills"]["picked"][0]["why"], "repo-role");
}

#[test]
fn a_stale_repo_plans_built_at_launch_and_picks_nothing_from_the_repo_yet() {
    let root = scratch("plan-stale");
    let f = fake(&root);
    let tool = real_tool();
    use_tool(&f, &tool);
    let h = home(&root);
    let repo = graph_repo(&root, &tool);

    let p = plan(&h, &repo, Some("builder"));
    let set = p.skills.as_ref().unwrap();
    assert!(
        set.picked.iter().all(|s| s.why != Why::RepoRole),
        "{:?}",
        set.picked
    );
    assert!(
        set.warnings.iter().any(|w| w == STALE_LINE),
        "{:?}",
        set.warnings
    );
}

#[test]
fn a_role_less_hire_never_asks_the_tool_and_says_nothing_about_repo_skills() {
    let root = scratch("roleless");
    let f = fake(&root);
    use_tool(&f, &root.join("no-such-allsource-skills"));
    let h = home(&root);
    let repo = repo_at(&root);
    std::fs::create_dir_all(repo.join(".allsource")).unwrap();
    std::fs::write(repo.join(".allsource/roles.toml"), "version = 1\n").unwrap();

    let p = plan(&h, &repo, None);
    let set = p.skills.as_ref().expect("skills resolved");
    assert!(
        !set.warnings.iter().any(|w| w.contains("repo skills")),
        "{:?}",
        set.warnings
    );
}

#[test]
fn a_stale_repo_is_built_at_launch_and_its_role_skill_is_bundled() {
    let root = scratch("launch-stale");
    let f = fake(&root);
    let tool = real_tool();
    use_tool(&f, &tool);
    let h = home(&root);
    let repo = graph_repo(&root, &tool);
    let p = plan(&h, &repo, Some("builder"));
    let req = p.request(run()).unwrap();

    let spawned = agents::spawn(&at(&h), req, Path::new(COS)).unwrap();
    let (spec, st) = agents::read(&h, &spawned.id).unwrap();

    let tdd = picked(&spec, "tdd").expect("tdd picked at launch");
    assert_eq!(tdd.why, Why::RepoRole);
    assert_eq!(tdd.root.as_deref(), Some("repo"));
    let warnings = &spec.skills.as_ref().unwrap().warnings;
    assert!(
        warnings
            .iter()
            .any(|w| w == "repo skills: built at launch, 1 picked"),
        "{warnings:?}"
    );
    assert!(!warnings.iter().any(|w| w == STALE_LINE), "{warnings:?}");

    let copy = agents::dir(&h, &spec.id).join("skills/skills/tdd");
    assert_eq!(
        skills::tree_digest(&copy).unwrap(),
        tdd.digest.clone().unwrap(),
        "the bundled copy matches the digest it was picked with"
    );
    assert!(st.skills_bundled.contains(&"tdd".to_string()), "{st:?}");
    assert!(st.herdr.is_some(), "the hire started: {st:?}");
}

#[test]
fn a_store_removed_after_planning_is_built_again_and_the_hire_still_starts() {
    let root = scratch("store-gone");
    let f = fake(&root);
    let tool = real_tool();
    use_tool(&f, &tool);
    let h = home(&root);
    let repo = graph_repo(&root, &tool);
    run_tool(&tool, &repo, &["install"]);
    let p = plan(&h, &repo, Some("builder"));
    assert!(
        p.skills
            .as_ref()
            .unwrap()
            .picked
            .iter()
            .any(|s| s.name == "tdd")
    );
    std::fs::remove_dir_all(repo.join(".prime")).unwrap();
    let req = p.request(run()).unwrap();

    let spawned = agents::spawn(&at(&h), req, Path::new(COS)).unwrap();
    let (spec, st) = agents::read(&h, &spawned.id).unwrap();

    assert!(picked(&spec, "tdd").is_some(), "{:?}", spec.skills);
    assert!(st.skills_bundled.contains(&"tdd".to_string()), "{st:?}");
    assert!(st.herdr.is_some(), "{st:?}");
}

#[test]
fn a_failed_install_at_launch_starts_the_hire_with_no_repo_picks_and_says_why() {
    let root = scratch("install-fails");
    let f = fake(&root);
    let tool = stand_in(&root, "tdd");
    std::fs::write(tool.parent().unwrap().join("install_mode"), "fail").unwrap();
    use_tool(&f, &tool);
    let h = home(&root);
    let repo = repo_at(&root);
    std::fs::create_dir_all(repo.join(".allsource")).unwrap();
    std::fs::write(repo.join(".allsource/roles.toml"), "version = 1\n").unwrap();
    commit(&repo);
    let p = plan(&h, &repo, Some("builder"));
    let req = p.request(run()).unwrap();

    let spawned = agents::spawn(&at(&h), req, Path::new(COS)).unwrap();
    let (spec, st) = agents::read(&h, &spawned.id).unwrap();

    assert!(picked(&spec, "tdd").is_none(), "{:?}", spec.skills);
    let warnings = &spec.skills.as_ref().unwrap().warnings;
    assert!(
        warnings
            .iter()
            .any(|w| w.starts_with("repo skills: none (install: exit 2")),
        "{warnings:?}"
    );
    assert!(st.herdr.is_some(), "{st:?}");
}

#[test]
fn a_zero_budget_leaves_no_repo_picks_with_a_spec_warning_and_the_hire_starts() {
    let root = scratch("zero-budget");
    let f = fake(&root);
    let tool = real_tool();
    use_tool(&f, &tool);
    let h = home(&root);
    let repo = graph_repo(&root, &tool);
    let p = plan(&h, &repo, Some("builder"));
    let req = p.request(run()).unwrap();

    let created = agents::create(&at(&h), req).unwrap();
    agents::launch_with_repo_budget(&h, &created.id, Path::new(COS), Duration::ZERO).unwrap();
    let (spec, st) = agents::read(&h, &created.id).unwrap();

    assert!(picked(&spec, "tdd").is_none(), "{:?}", spec.skills);
    let warnings = &spec.skills.as_ref().unwrap().warnings;
    assert!(
        warnings
            .iter()
            .any(|w| w == "repo skills: none (no time left to read them)"),
        "{warnings:?}"
    );
    assert!(st.herdr.is_some(), "{st:?}");
}

#[test]
fn a_worktree_without_the_graph_says_so_at_launch() {
    let root = scratch("no-graph-in-tree");
    let f = fake(&root);
    let tool = real_tool();
    use_tool(&f, &tool);
    let h = home(&root);
    let repo = repo_at(&root);
    std::fs::create_dir_all(repo.join(".allsource")).unwrap();
    std::fs::write(
        repo.join(".allsource/roles.toml"),
        "version = 1\n\n[roles.builder]\nskills = [\"tdd\"]\n",
    )
    .unwrap();
    let p = plan(&h, &repo, Some("builder"));
    let req = p.request(run()).unwrap();

    let spawned = agents::spawn(&at(&h), req, Path::new(COS)).unwrap();
    let (spec, _st) = agents::read(&h, &spawned.id).unwrap();

    let warnings = &spec.skills.as_ref().unwrap().warnings;
    assert!(
        warnings
            .iter()
            .any(|w| w == "repo skills: none (the worktree has no .allsource/roles.toml)"),
        "{warnings:?}"
    );
}

#[test]
fn a_hire_into_a_repo_with_a_graph_carries_the_repo_skill_and_leaves_the_worktree_clean() {
    let root = scratch("e2e");
    let f = fake(&root);
    let tool = real_tool();
    use_tool(&f, &tool);
    let h = home(&root);
    let repo = graph_repo(&root, &tool);
    run_tool(&tool, &repo, &["install"]);
    let p = plan(&h, &repo, Some("builder"));
    let req = p.request(run()).unwrap();

    let spawned = agents::spawn(&at(&h), req, Path::new(COS)).unwrap();
    let (spec, st) = agents::read(&h, &spawned.id).unwrap();

    let tdd = picked(&spec, "tdd").expect("tdd picked");
    assert_eq!(tdd.why, Why::RepoRole);
    let copy = agents::dir(&h, &spec.id).join("skills/skills/tdd");
    assert!(copy.join("SKILL.md").is_file(), "{}", copy.display());
    assert_eq!(
        skills::tree_digest(&copy).unwrap(),
        tdd.digest.clone().unwrap()
    );
    assert_eq!(st.skills_bundled, ["tdd"], "{st:?}");
    assert_eq!(f.terminals().len(), 1, "one hire in Herdr");

    let status = Command::new("git")
        .arg("-C")
        .arg(&spec.workdir)
        .args(["status", "--porcelain"])
        .output()
        .unwrap();
    assert!(
        status.status.success() && status.stdout.is_empty(),
        "worktree dirty: {}",
        String::from_utf8_lossy(&status.stdout)
    );
}

#[test]
fn a_codex_hire_planned_fresh_lists_its_repo_skills_in_the_prompt() {
    let root = scratch("codex-fresh");
    let f = fake(&root);
    let tool = real_tool();
    use_tool(&f, &tool);
    let h = home(&root);
    let repo = graph_repo(&root, &tool);
    run_tool(&tool, &repo, &["install"]);
    let p = plan(&h, &repo, Some("builder"));
    let req = p.request(run_as(Harness::Codex)).unwrap();

    let spawned = agents::spawn(&at(&h), req, Path::new(COS)).unwrap();
    let (spec, _st) = agents::read(&h, &spawned.id).unwrap();

    assert!(
        spec.prompt.contains("/export/agents/tdd/SKILL.md"),
        "{}",
        spec.prompt
    );
}

#[test]
fn a_codex_hire_planned_stale_lists_its_repo_skills_once_built_at_launch() {
    let root = scratch("codex-stale");
    let f = fake(&root);
    let tool = real_tool();
    use_tool(&f, &tool);
    let h = home(&root);
    let repo = graph_repo(&root, &tool);
    let p = plan(&h, &repo, Some("builder"));
    assert!(
        !p.prompt.contains("/export/"),
        "planned stale: the prompt names no repo export yet"
    );
    let req = p.request(run_as(Harness::Codex)).unwrap();

    let spawned = agents::spawn(&at(&h), req, Path::new(COS)).unwrap();
    let (spec, st) = agents::read(&h, &spawned.id).unwrap();

    assert!(
        spec.prompt.contains("/export/agents/tdd/SKILL.md"),
        "launch rewrites the index from the built set: {}",
        spec.prompt
    );
    assert_eq!(
        spec.prompt.matches("Skills for this work").count(),
        1,
        "{}",
        spec.prompt
    );
    assert!(st.herdr.is_some(), "{st:?}");
}
