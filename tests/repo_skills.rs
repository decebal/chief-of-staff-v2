//! The hired repo's skill set read from `allsource-skills` (the cos layer of
//! `docs/plans/2026-11-28-skill-graph-design.md`): names and a digest from the
//! tool, the export root built here, every failure `Unavailable`. The tool is
//! `tests/fixtures/fake-allsource-skills.sh`, copied into each test's own
//! directory and driven by the files beside it.

use std::ffi::OsStr;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use cos::agents::repo_skills::{self, RepoSkills, State};

const DIG: &str = "3f9a1c0be27d4e5f60718293a4b5c6d7e8f9a0b1c2d3e4f5a6b7c8d9e0f1a2b3";
const NOT_FOUND: &str = "allsource-skills not found; repo has .allsource/roles.toml";

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("cos-repo-skills-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d.canonicalize().unwrap()
}

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake-allsource-skills.sh")
}

/// A repo with a roles.toml, and a stand-in tool beside its own config files.
/// Returns the repo and the tool's path.
fn repo_and_tool(name: &str, skills: &str) -> (PathBuf, PathBuf) {
    let base = scratch(name);
    let repo = base.join("repo");
    std::fs::create_dir_all(repo.join(".allsource")).unwrap();
    std::fs::write(repo.join(".allsource/roles.toml"), "version = 1\n").unwrap();
    let dir = base.join("tool");
    std::fs::create_dir_all(&dir).unwrap();
    let bin = dir.join("allsource-skills");
    std::fs::copy(fixture(), &bin).unwrap();
    std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::write(dir.join("digest"), DIG).unwrap();
    std::fs::write(dir.join("skills"), skills).unwrap();
    (repo, bin)
}

fn set(tool: &Path, file: &str, value: &str) {
    std::fs::write(tool.parent().unwrap().join(file), value).unwrap();
}

fn calls(tool: &Path) -> String {
    std::fs::read_to_string(tool.parent().unwrap().join("calls.log")).unwrap_or_default()
}

fn export(repo: &Path) -> PathBuf {
    repo.join(".prime/skills").join(DIG).join("export/claude")
}

fn unavailable(read: &RepoSkills) -> &str {
    match &read.state {
        State::Unavailable { why } => why,
        other => panic!("expected unavailable, got {other:?}"),
    }
}

#[test]
fn a_store_built_by_install_reads_as_fresh_with_its_names_and_a_root_at_the_digest() {
    let (repo, tool) = repo_and_tool("fresh", "tdd ritual");
    let first = repo_skills::read_using(Some(&tool), &repo, "builder", Duration::from_secs(10));
    assert_eq!(first.state, State::Stale, "no export yet: {first:?}");
    assert!(first.root.is_none() && first.names.is_empty());

    let built = repo_skills::build_using(Some(&tool), &repo, "builder", Duration::from_secs(20));
    assert_eq!(
        built.state,
        State::Fresh { digest: DIG.into() },
        "{built:?}"
    );
    assert_eq!(built.names, ["ritual", "tdd"]);
    let root = built.root.expect("a repo root");
    assert_eq!(root.label, "repo");
    assert_eq!(root.dir, export(&repo));
    assert!(root.dir.join("tdd/SKILL.md").is_file());
    assert!(calls(&tool).contains("install --repo"), "{}", calls(&tool));
}

#[test]
fn a_stale_store_reads_as_stale_and_names_nothing() {
    let (repo, tool) = repo_and_tool("stale", "tdd");
    set(&tool, "roles_mode", "stale");
    let read = repo_skills::read_using(Some(&tool), &repo, "builder", Duration::from_secs(10));
    assert_eq!(read.state, State::Stale);
    assert!(read.root.is_none() && read.names.is_empty());
}

#[test]
fn a_missing_binary_is_unavailable_with_the_named_message() {
    let (repo, _tool) = repo_and_tool("missing", "tdd");
    let read = repo_skills::read_using(None, &repo, "builder", Duration::from_secs(10));
    assert_eq!(unavailable(&read), NOT_FOUND);
    assert!(read.root.is_none() && read.names.is_empty());
}

#[test]
fn the_binary_is_the_configured_path_else_the_first_allsource_skills_on_path() {
    let (_repo, tool) = repo_and_tool("locate", "tdd");
    let configured = OsStr::new(tool.to_str().unwrap());
    assert_eq!(
        repo_skills::locate(Some(configured), None),
        Some(tool.clone())
    );
    assert_eq!(
        repo_skills::locate(Some(OsStr::new("/nonexistent/allsource-skills")), None),
        None,
        "a configured binary that is not there is not searched for"
    );
    let dir = tool.parent().unwrap().to_path_buf();
    let other = scratch("locate-path");
    let path = std::env::join_paths([other.as_path(), dir.as_path()]).unwrap();
    assert_eq!(
        repo_skills::locate(None, Some(path.as_os_str())),
        Some(tool.clone())
    );
    assert_eq!(
        repo_skills::locate(None, Some(other.as_os_str())),
        None,
        "not on PATH: none"
    );
}

#[test]
fn a_bad_role_id_is_refused_before_any_spawn() {
    let (repo, tool) = repo_and_tool("bad-role", "tdd");
    let read = repo_skills::read_using(Some(&tool), &repo, "../x", Duration::from_secs(10));
    assert!(unavailable(&read).contains("not a role id"), "{read:?}");
    assert_eq!(calls(&tool), "", "the tool was never run");
}

#[test]
fn a_roles_call_past_its_deadline_is_killed_and_unavailable() {
    let (repo, tool) = repo_and_tool("sleepy", "tdd");
    set(&tool, "roles_mode", "sleep");
    let started = Instant::now();
    let read = repo_skills::read_using(Some(&tool), &repo, "builder", Duration::from_secs(1));
    assert!(
        started.elapsed() < Duration::from_secs(20),
        "waited {:?}",
        started.elapsed()
    );
    assert!(!unavailable(&read).is_empty(), "{read:?}");
    assert!(read.root.is_none());
}

#[test]
fn a_path_field_or_a_non_hex_digest_adds_no_root() {
    for mode in ["path", "nonhex"] {
        let (repo, tool) = repo_and_tool(mode, "tdd");
        set(&tool, "roles_mode", mode);
        let read = repo_skills::read_using(Some(&tool), &repo, "builder", Duration::from_secs(10));
        assert!(
            unavailable(&read).contains("closed-schema") || unavailable(&read).contains("hex"),
            "{mode}: {read:?}"
        );
        assert!(
            read.root.is_none() && read.names.is_empty(),
            "{mode}: {read:?}"
        );
    }
}

#[test]
fn garbage_output_and_an_io_failure_are_unavailable_with_the_tools_own_words() {
    let (repo, tool) = repo_and_tool("garbage", "tdd");
    set(&tool, "roles_mode", "garbage");
    let read = repo_skills::read_using(Some(&tool), &repo, "builder", Duration::from_secs(10));
    assert!(unavailable(&read).contains("closed-schema"), "{read:?}");

    set(&tool, "roles_mode", "fail");
    let read = repo_skills::read_using(Some(&tool), &repo, "builder", Duration::from_secs(10));
    assert!(
        unavailable(&read).contains("the store is unreadable"),
        "{read:?}"
    );
}

#[test]
fn a_skill_name_the_fact_rules_refuse_is_unavailable() {
    let (repo, tool) = repo_and_tool("badname", "Bad_Name");
    let built = repo_skills::build_using(Some(&tool), &repo, "builder", Duration::from_secs(20));
    assert!(
        unavailable(&built).contains("not a skill name"),
        "{built:?}"
    );
    assert!(built.root.is_none());
}

#[test]
fn a_repo_without_a_roles_file_is_never_asked() {
    let (repo, tool) = repo_and_tool("absent", "tdd");
    std::fs::remove_file(repo.join(".allsource/roles.toml")).unwrap();
    let read = repo_skills::read_using(Some(&tool), &repo, "builder", Duration::from_secs(10));
    assert_eq!(read.state, State::Absent);
    assert_eq!(calls(&tool), "");
}

#[test]
fn a_failed_install_is_unavailable_and_says_why() {
    let (repo, tool) = repo_and_tool("install-fails", "tdd");
    set(&tool, "install_mode", "fail");
    let built = repo_skills::build_using(Some(&tool), &repo, "builder", Duration::from_secs(20));
    assert!(
        unavailable(&built).contains("install: exit 2: install: the store could not be written"),
        "{built:?}"
    );
    assert!(built.root.is_none() && built.names.is_empty());
}

#[test]
fn a_zero_budget_builds_nothing_and_spawns_nothing() {
    let (repo, tool) = repo_and_tool("zero", "tdd");
    let built = repo_skills::build_using(Some(&tool), &repo, "builder", Duration::ZERO);
    assert!(unavailable(&built).contains("no time left"), "{built:?}");
    assert_eq!(calls(&tool), "");
}
