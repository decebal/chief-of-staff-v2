use cos::{report, state};
use serde_json::{Value, json};

#[test]
fn configured_staff_remain_distinct_from_execution_and_customer_proof() {
    let root = std::env::temp_dir().join(format!("cos-staff-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    state::initialize(&root).unwrap();
    let mut candidate: Value = serde_json::from_str(state::DEMO_STATE).unwrap();
    let staff = json!({
        "scope": "Fictional project only",
        "mode": "assessment_only",
        "integration": {"managedRunVerified": false},
        "members": [{
            "id": "quality",
            "status": "configured",
            "methods": ["allstack-review", "allstack-qa"],
            "run": null,
            "taskRef": null,
            "lastReportAt": null
        }],
        "followUps": [{
            "staffId": "quality",
            "disposition": "awaiting_authority",
            "currentEvidence": "Installed customer proof remains open."
        }]
    });
    candidate["commitments"][0]["dashboard"] = json!({"staff": staff});
    let before_deliverables = candidate["deliverables"].clone();
    state::commit(&root, &candidate, 0, "Fictional staff configuration").unwrap();
    let data = report::data(&root, &root, true).unwrap();
    assert_eq!(data["workspace"]["staff"], staff);
    assert!(data["workspace"]["staff"]["members"][0]["run"].is_null());
    assert!(
        !data["workspace"]["staff"]["integration"]["managedRunVerified"]
            .as_bool()
            .unwrap()
    );
    let html = report::render(&data).unwrap();
    assert!(html.contains("id=\"tab-staff\""));
    assert!(html.contains("Run reference; evidence missing"));
    assert!(html.contains("No linked run"));
    assert_eq!(
        state::read(&root).unwrap().unwrap()["deliverables"],
        before_deliverables
    );
    std::fs::remove_dir_all(root).unwrap();
}
