#![cfg(target_os = "macos")]

use cos::{
    reviewer, state,
    workflow::{Binding, ReviewStart, ReviewerBinding, Workflow},
};
use sha2::{Digest, Sha256};
use std::{fs, os::unix::fs::PermissionsExt};

#[test]
fn fresh_reviewers_receive_only_fixture_and_fail_closed() {
    let dir = std::env::temp_dir().join(format!("cos-isolation-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let secret = dir.join("unrelated.txt");
    fs::write(&secret, "synthetic unrelated data").unwrap();
    let program = dir.join("reviewer");
    let script = format!(
        "#!/bin/sh\nread -r draft || exit 11\n[ \"$draft\" = 'fictional draft' ] || exit 12\nif read -r leak < '{}' 2>/dev/null; then exit 13; fi\nprintf '{{\"findings\":[{{\"description\":\"%s\",\"evidence\":\"fictional draft\"}}]}}' \"$1\"\n",
        secret.display()
    );
    fs::write(&program, script.as_bytes()).unwrap();
    fs::set_permissions(&program, fs::Permissions::from_mode(0o700)).unwrap();
    let hash: String = Sha256::digest(script.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let review = reviewer::review(&program, &hash, b"fictional draft\n").unwrap();
    assert_eq!(review.phases.len(), 5);
    assert_eq!(review.findings.len(), 5);
    assert!(
        review
            .findings
            .iter()
            .all(|finding| finding.resolution.is_none())
    );
    assert!(reviewer::review(&program, "incorrect-hash", b"fictional draft\n").is_err());
    assert!(reviewer::review(&program, &hash, b"different draft\n").is_err());
    assert!(reviewer::review(&dir.join("missing"), &hash, b"fictional draft\n").is_err());
    let repo = dir.join("project");
    fs::create_dir(&repo).unwrap();
    let mut git = std::process::Command::new("git");
    git.arg("init").arg("--quiet").arg(&repo);
    assert!(
        cos::proc::run(&mut git, std::time::Duration::from_secs(5))
            .unwrap()
            .status
            .success()
    );
    let draft = repo.join("design.md");
    fs::write(&draft, "fictional draft\n").unwrap();
    state::initialize(&dir).unwrap();
    let mut candidate: serde_json::Value = serde_json::from_str(state::DEMO_STATE).unwrap();
    let id = candidate["commitments"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    candidate["commitments"][0]["workflow"] = serde_json::to_value(Workflow {
        binding: Binding {
            workspace: dir.clone(),
            repository: repo,
            owner: "Fictional owner".into(),
            draft,
        },
        review: None,
        approval: None,
        approval_action_ids: vec![],
        review_action_ids: vec![],
        latest_review_action_id: None,
        pending_review: None,
        last_review_failure: None,
        reviewer_binding: Some(ReviewerBinding {
            program: program.clone(),
            sha256: hash.clone(),
        }),
        enrollment_action_ids: vec![],
        latest_enrollment_action_id: None,
        resolution_actions: vec![],
    })
    .unwrap();
    state::commit(&dir, &candidate, 0, "Install fictional workflow").unwrap();
    assert_eq!(
        cos::workflow::begin_review(&dir, &id, 1, "fixture-review-action-1").unwrap(),
        ReviewStart::Started
    );
    reviewer::execute_pending(&dir, &id, "fixture-review-action-1").unwrap();
    assert_eq!(
        cos::workflow::begin_review(&dir, &id, 1, "fixture-review-action-1").unwrap(),
        ReviewStart::AlreadyCompleted
    );
    let saved = state::read(&dir).unwrap().unwrap();
    assert_eq!(saved["revision"], 3);
    assert_eq!(
        saved["commitments"][0]["workflow"]["review"]["findings"]
            .as_array()
            .unwrap()
            .len(),
        5
    );
    fs::remove_file(&program).unwrap();
    assert_eq!(
        cos::workflow::begin_review(&dir, &id, 3, "fixture-review-action-2").unwrap(),
        ReviewStart::Started
    );
    assert!(reviewer::execute_pending(&dir, &id, "fixture-review-action-2").is_err());
    let failed = state::read(&dir).unwrap().unwrap();
    assert_eq!(failed["revision"], 5);
    assert_eq!(
        failed["commitments"][0]["workflow"]["last_review_failure"]["kind"],
        "reviewer_failed"
    );
    assert!(failed["commitments"][0]["workflow"]["pending_review"].is_null());
    fs::remove_dir_all(dir).unwrap();
}
