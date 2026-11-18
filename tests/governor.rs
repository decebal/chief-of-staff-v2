//! Seeding, the seed plan, the machine-wide build lock and the build-tool
//! shims — the parts of the old governor that survive herdr owning every run.
//! Concurrency slots and deadline enforcement are herdr's now, not cos's.

use std::fs::File;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use cos::agents::{self, slots};

const COS: &str = env!("CARGO_BIN_EXE_cos");

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("cos-gov-{name}-{}", std::process::id()));
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

/// A repo whose `files` are committed, with `target/marker` left ignored.
fn repo(root: &Path, files: &[(&str, &str)]) -> PathBuf {
    let r = root.join("projects/seeded");
    std::fs::create_dir_all(r.join("target")).unwrap();
    git(&r, &["init", "-q", "-b", "main"]);
    std::fs::write(r.join(".gitignore"), "target/\n").unwrap();
    std::fs::write(r.join("target/marker"), "warm\n").unwrap();
    for (path, body) in files {
        let p = r.join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, body).unwrap();
    }
    git(&r, &["add", "-A"]);
    git(&r, &["commit", "-q", "-m", "init"]);
    r
}

/// Cuts the detached worktree a hire runs in, from the repo's HEAD.
fn worktree(repo: &Path, at: &Path) {
    git(
        repo,
        &["worktree", "add", "--detach", &at.to_string_lossy(), "HEAD"],
    );
}

#[test]
fn a_cargo_worktree_is_seeded_with_the_checkout_target_cloned() {
    let root = scratch("seed");
    let repo = repo(&root, &[("Cargo.toml", "[package]\nname = \"x\"\n")]);
    let tree = root.join("tree");
    worktree(&repo, &tree);

    let done = agents::seed::run(&repo, &tree, &root.join("seed.log")).unwrap();
    assert_eq!(done, vec!["cloned target".to_owned()]);
    let marker = std::fs::read_to_string(tree.join("target/marker")).unwrap();
    assert_eq!(marker, "warm\n");
}

#[test]
fn a_failing_seed_script_fails_the_seed_before_the_harness_runs() {
    let root = scratch("seed-fail");
    let repo = repo(
        &root,
        &[(".cos/seed", "#!/bin/sh\necho no network >&2\nexit 3\n")],
    );
    let script = repo.join(".cos/seed");
    Command::new("chmod")
        .arg("+x")
        .arg(&script)
        .status()
        .unwrap();
    git(&repo, &["add", ".cos/seed"]);
    git(&repo, &["commit", "-q", "-m", "exec"]);
    let tree = root.join("tree");
    worktree(&repo, &tree);

    let err = agents::seed::run(&repo, &tree, &root.join("seed.log"))
        .unwrap_err()
        .to_string();
    assert!(err.contains("seed") && err.contains("no network"), "{err}");
    assert!(!tree.join("made-by-agent.txt").exists());
}

#[test]
fn the_seed_plan_installs_from_the_lockfile_when_there_is_nothing_to_clone() {
    let root = scratch("plan");
    let (repo, tree) = (root.join("repo"), root.join("tree"));
    std::fs::create_dir_all(&repo).unwrap();
    std::fs::create_dir_all(&tree).unwrap();
    std::fs::write(tree.join("package.json"), "{}").unwrap();
    std::fs::write(tree.join("bun.lock"), "").unwrap();
    let plan = agents::seed::plan(&repo, &tree);
    assert_eq!(plan.len(), 1);
    assert_eq!(plan[0].describe(), "bun install --frozen-lockfile");

    std::fs::create_dir_all(repo.join("node_modules")).unwrap();
    assert_eq!(
        agents::seed::plan(&repo, &tree)[0].describe(),
        "cloned node_modules"
    );
}

#[test]
fn a_build_waits_for_the_machine_wide_lock() {
    let root = scratch("build-lock");
    let lock = root.join("build.lock");
    let held = File::create(&lock).unwrap();
    // SAFETY: flock on a descriptor the test owns.
    assert_eq!(unsafe { libc::flock(held.as_raw_fd(), libc::LOCK_EX) }, 0);

    let mut child = Command::new(COS)
        .args(["agents", "build-slot", "--lock"])
        .arg(&lock)
        .args(["--", "/bin/echo", "built"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    std::thread::sleep(Duration::from_millis(700));
    assert!(
        child.try_wait().unwrap().is_none(),
        "ran while the lock was held"
    );

    drop(held);
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    assert_eq!(String::from_utf8_lossy(&out.stdout), "built\n");
    assert!(String::from_utf8_lossy(&out.stderr).contains("waiting for the build slot"));
}

#[test]
fn shims_wrap_cargo_and_never_find_themselves() {
    let root = scratch("shims");
    let (bin, real) = (root.join("bin"), root.join("real"));
    std::fs::create_dir_all(&real).unwrap();
    let fake = real.join("cargo");
    std::fs::write(&fake, "#!/bin/sh\necho real-cargo \"$@\"\n").unwrap();
    Command::new("chmod").arg("+x").arg(&fake).status().unwrap();
    let path = std::env::join_paths([bin.clone(), real]).unwrap();

    let shimmed = slots::write_shims(
        &bin,
        Path::new(COS),
        &root.join("build.lock"),
        path.to_str().unwrap(),
    )
    .unwrap();
    assert_eq!(shimmed, vec!["cargo".to_owned()]);
    File::create(root.join("build.lock")).unwrap();
    let out = Command::new(bin.join("cargo"))
        .args(["build", "-q"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout), "real-cargo build -q\n");
}
