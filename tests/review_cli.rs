#![cfg(target_os = "macos")]

use cos::{state, workflow::Workflow};
use serde_json::Value;
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::Path,
    process::{Command, Output},
};

const COS: &str = env!("CARGO_BIN_EXE_cos");

fn cos(root: &Path, args: &[&str]) -> Output {
    Command::new(COS)
        .arg("--home")
        .arg(root)
        .arg("--workspace")
        .arg(root)
        .arg("--projects")
        .arg(root)
        .arg("review")
        .args(args)
        .env_remove("COS_HOME")
        .env_remove("COS_WORKSPACE")
        .env_remove("COS_PROJECTS")
        .env_remove("COS_BUSINESS")
        .env_remove("XDG_CONFIG_HOME")
        .env("HOME", root.join("user-home"))
        .env("COS_BUSINESSES", root.join("none.toml"))
        .output()
        .unwrap()
}

fn shown(out: &Output) -> Value {
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

#[test]
fn review_cli_binds_reviews_resolves_and_gates_approval() {
    let root = std::env::temp_dir().join(format!("cos-review-cli-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let repo = root.join("project");
    fs::create_dir_all(&repo).unwrap();
    let mut git = Command::new("git");
    git.arg("init").arg("--quiet").arg(&repo);
    assert!(
        cos::proc::run(&mut git, std::time::Duration::from_secs(5))
            .unwrap()
            .status
            .success()
    );
    let draft = repo.join("design.md");
    fs::write(&draft, "fictional draft\n").unwrap();
    fs::write(repo.join("other.md"), "another draft\n").unwrap();
    let program = root.join("reviewer");
    fs::write(
        &program,
        "#!/bin/sh\nread -r draft || exit 11\nprintf '{\"findings\":[{\"description\":\"%s gap\",\"evidence\":\"%s\"}]}' \"$1\" \"$draft\"\n",
    )
    .unwrap();
    fs::set_permissions(&program, fs::Permissions::from_mode(0o700)).unwrap();
    state::initialize(&root).unwrap();
    let demo: Value = serde_json::from_str(state::DEMO_STATE).unwrap();
    state::commit(&root, &demo, 0, "Fictional state").unwrap();
    let id = demo["commitments"][0]["id"].as_str().unwrap().to_owned();
    let repo_arg = repo.to_str().unwrap();
    let draft_arg = draft.to_str().unwrap();
    let program_arg = program.to_str().unwrap();

    let bound = shown(&cos(
        &root,
        &[
            "bind",
            &id,
            "--repo",
            repo_arg,
            "--draft",
            draft_arg,
            "--owner",
            "Fictional owner",
        ],
    ));
    assert_eq!(bound["revision"], 2);
    assert_eq!(bound["approvable"], false);
    assert_eq!(bound["blocker"], "Independent review is required");
    let again = shown(&cos(
        &root,
        &[
            "bind",
            &id,
            "--repo",
            repo_arg,
            "--draft",
            draft_arg,
            "--owner",
            "Fictional owner",
        ],
    ));
    assert_eq!(again["revision"], 2, "an identical bind commits nothing");
    let other = repo.join("other.md");
    let rebind = cos(
        &root,
        &[
            "bind",
            &id,
            "--repo",
            repo_arg,
            "--draft",
            other.to_str().unwrap(),
            "--owner",
            "Fictional owner",
        ],
    );
    assert!(!rebind.status.success());

    let unpinned = cos(&root, &["enroll", &id, "--reviewer", program_arg]);
    assert!(!unpinned.status.success());
    let hint = String::from_utf8_lossy(&unpinned.stderr).into_owned();
    let digest = cos::reviewer::program_digest(&program).unwrap();
    assert!(hint.contains(&format!("--sha256 {digest}")), "{hint}");

    // Enrollment asks the OS for the owner's login, which a test cannot give; the
    // binding it would record is committed directly.
    let mut current = state::read(&root).unwrap().unwrap();
    let mut flow: Workflow =
        serde_json::from_value(current["commitments"][0]["workflow"].clone()).unwrap();
    flow.reviewer_binding = Some(cos::workflow::ReviewerBinding {
        program: program.canonicalize().unwrap(),
        sha256: digest,
    });
    current["commitments"][0]["workflow"] = serde_json::to_value(flow).unwrap();
    state::commit(&root, &current, 2, "Fictional enrollment").unwrap();

    let reviewed = shown(&cos(&root, &["run", &id]));
    let findings = reviewed["findings"].as_array().unwrap();
    assert_eq!(findings.len(), 5);
    assert_eq!(findings[0]["description"], "initial_zoom_out gap");
    assert_eq!(findings[0]["evidence"], "fictional draft");
    assert_eq!(reviewed["reviewedDigest"], reviewed["draftDigest"]);
    assert_eq!(
        reviewed["blocker"],
        "Each finding needs evidence and a resolution"
    );

    let review_digest = reviewed["reviewDigest"].as_str().unwrap().to_owned();
    let stale = cos(
        &root,
        &["resolve", &id, "0", "--review", "0000", "Accepted"],
    );
    assert!(!stale.status.success());
    let mut last = reviewed;
    for index in 0..5 {
        let digest = last["reviewDigest"].as_str().unwrap().to_owned();
        last = shown(&cos(
            &root,
            &[
                "resolve",
                &id,
                &index.to_string(),
                "--review",
                &digest,
                "Accepted",
                "for",
                "now",
            ],
        ));
    }
    assert_ne!(last["reviewDigest"], review_digest.as_str());
    assert_eq!(last["findings"][4]["resolution"], "Accepted for now");
    assert_eq!(last["approvable"], true);
    assert_eq!(last["approved"], false);

    let wrong = cos(&root, &["approve", &id, "--digest", &"0".repeat(64)]);
    assert!(!wrong.status.success());
    assert!(
        String::from_utf8_lossy(&wrong.stderr).contains("Design changed since it was displayed")
    );

    fs::write(&draft, "fictional draft, edited\n").unwrap();
    let edited = shown(&cos(&root, &["show", &id]));
    assert_eq!(edited["approvable"], false);
    assert_eq!(edited["blocker"], "Draft changed since review");
    fs::remove_dir_all(root).unwrap();
}
