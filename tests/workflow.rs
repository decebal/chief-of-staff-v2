use cos::{
    state,
    workflow::{self, Binding, Finding, Review, ReviewerBinding, Workflow},
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{fs, os::unix::fs::PermissionsExt};

#[test]
fn approval_follows_reviewed_bytes_and_is_workspace_scoped() {
    let root = std::env::temp_dir().join(format!("cos-review-fixture-{}", std::process::id()));
    fs::create_dir_all(root.join("project")).unwrap();
    let mut git = std::process::Command::new("git");
    git.arg("init").arg("--quiet").arg(root.join("project"));
    assert!(
        cos::proc::run(&mut git, std::time::Duration::from_secs(5))
            .unwrap()
            .status
            .success()
    );
    fs::create_dir_all(root.join("other")).unwrap();
    let draft = root.join("project/design.md");
    fs::write(&draft, "Fictional design").unwrap();
    let program = root.join("fictional-reviewer");
    let program_bytes = b"#!/bin/sh\nexit 0\n";
    fs::write(&program, program_bytes).unwrap();
    fs::set_permissions(&program, fs::Permissions::from_mode(0o700)).unwrap();
    let reviewer_hash: String = Sha256::digest(program_bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    state::initialize(&root).unwrap();
    state::initialize(&root.join("other")).unwrap();
    let mut candidate: serde_json::Value = serde_json::from_str(state::DEMO_STATE).unwrap();
    let binding = Binding {
        workspace: root.clone(),
        repository: root.join("project"),
        owner: "Fictional owner".into(),
        draft: draft.clone(),
    };
    let mut flow = Workflow {
        binding,
        review: None,
        approval: None,
        approval_action_ids: Vec::new(),
        review_action_ids: Vec::new(),
        latest_review_action_id: None,
        pending_review: None,
        last_review_failure: None,
        reviewer_binding: Some(ReviewerBinding {
            program,
            sha256: reviewer_hash.clone(),
        }),
        enrollment_action_ids: Vec::new(),
        latest_enrollment_action_id: None,
        resolution_actions: Vec::new(),
    };
    assert!(flow.can_approve(&root).is_err());
    assert!(flow.can_approve(&root.join("other")).is_err());
    let draft_digest = flow.draft_digest(&root).unwrap();
    flow.review = Some(Review {
        draft_digest,
        reviewer: format!("sha256:{reviewer_hash}"),
        input_boundary: "Fictional draft only".into(),
        phases: [
            "initial_zoom_out",
            "comprehension",
            "design_critique",
            "intermediate_zoom_out",
            "implementation_readiness",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect(),
        findings: vec![Finding {
            description: "Missing fallback".into(),
            evidence: "Fictional section".into(),
            resolution: None,
        }],
    });
    assert!(flow.can_approve(&root).is_err());
    flow.review.as_mut().unwrap().findings[0].resolution = Some("Accepted by founder".into());
    candidate["commitments"][0]["workflow"] = serde_json::to_value(&flow).unwrap();
    let id = candidate["commitments"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    state::commit(&root, &candidate, 0, "Set fictional workflow").unwrap();
    state::commit(&root.join("other"), &candidate, 0, "Set fictional workflow").unwrap();
    assert!(workflow::approve(&root.join("other"), &id, 1, "fictional-action-001").is_err());
    assert_eq!(
        state::read(&root.join("other")).unwrap().unwrap()["revision"],
        json!(1)
    );
    workflow::approve(&root, &id, 1, "fictional-action-001").unwrap();
    let saved = state::read(&root).unwrap().unwrap();
    let flow: Workflow =
        serde_json::from_value(saved["commitments"][0]["workflow"].clone()).unwrap();
    assert!(flow.approval_is_current(&root, &id));
    assert!(!flow.approval_is_current(&root, "other-commitment"));
    let mut rebound = flow.clone();
    rebound.binding.owner = "Another fictional owner".into();
    assert!(!rebound.approval_is_current(&root, &id));
    let mut amended = flow.clone();
    amended.review.as_mut().unwrap().findings[0].resolution = Some("Different resolution".into());
    assert!(!amended.approval_is_current(&root, &id));
    amended.review.as_mut().unwrap().phases.pop();
    assert!(amended.can_approve(&root).is_err());
    workflow::approve(&root, &id, 1, "fictional-action-001").unwrap();
    assert_eq!(state::read(&root).unwrap().unwrap()["revision"], json!(2));
    assert!(workflow::approve(&root, &id, 1, "fictional-action-002").is_err());
    let mut new_review = flow.review.clone().unwrap();
    new_review.findings[0].resolution = None;
    assert_eq!(
        workflow::begin_review(&root, &id, 2, "fictional-review-001").unwrap(),
        workflow::ReviewStart::Started
    );
    assert_eq!(
        workflow::begin_review(&root, &id, 2, "fictional-review-001").unwrap(),
        workflow::ReviewStart::AlreadyPending
    );
    assert!(workflow::begin_review(&root, &id, 3, "fictional-review-002").is_err());
    workflow::record_review(&root, &id, 3, "fictional-review-001", new_review.clone()).unwrap();
    workflow::record_review(&root, &id, 3, "fictional-review-001", new_review).unwrap();
    let state = state::read(&root).unwrap().unwrap();
    assert_eq!(state["revision"], json!(4));
    assert!(state["commitments"][0]["workflow"]["approval"].is_null());
    assert!(
        cos::approval_auth::approve_owner(
            &root,
            &id,
            4,
            "fictional-action-002",
            &flow.review.as_ref().unwrap().draft_digest
        )
        .is_err()
    );
    let saved_flow: Workflow =
        serde_json::from_value(state["commitments"][0]["workflow"].clone()).unwrap();
    let shown_review = saved_flow.review_digest().unwrap();
    workflow::resolve_finding(
        &root,
        &id,
        4,
        "fixture-resolution-001",
        &shown_review,
        0,
        "Accepted for the fixture",
    )
    .unwrap();
    workflow::resolve_finding(
        &root,
        &id,
        4,
        "fixture-resolution-001",
        &shown_review,
        0,
        "Accepted for the fixture",
    )
    .unwrap();
    assert!(
        workflow::resolve_finding(
            &root,
            &id,
            4,
            "fixture-resolution-001",
            &shown_review,
            0,
            "Different decision"
        )
        .is_err()
    );
    assert_eq!(
        workflow::begin_review(&root, &id, 5, "fictional-review-002").unwrap(),
        workflow::ReviewStart::Started
    );
    workflow::fail_review(&root, &id, 6, "fictional-review-002").unwrap();
    assert!(workflow::begin_review(&root, &id, 7, "fictional-review-002").is_err());
    let failed = state::read(&root).unwrap().unwrap();
    assert_eq!(
        failed["commitments"][0]["workflow"]["last_review_failure"]["kind"],
        "reviewer_failed"
    );
    fs::write(&draft, "Changed fictional design").unwrap();
    assert!(!flow.approval_is_current(&root, &id));
    assert!(workflow::approve(&root, &id, 7, "fictional-action-001").is_err());
    fs::remove_dir_all(root).unwrap();
}
