use cos::staff;
use serde_json::json;
use std::path::Path;

#[test]
fn managed_projection_requires_matching_engine_history_and_keeps_proof_open() {
    let run = json!({"id":"fictional-run","revision":4,"repository":"/fictional/longhand","task":"fictional-task","host":"claude-code","epoch":2,"stage":"building","pending":null,"checkpoint":"fictional-digest","allow_commit":false,"allow_publish":false,"verification":null,"publication":null});
    let evidence = json!({"run":run,"gate_receipts":[]});
    let history = json!([{"kind":"handoff_completed","run":run}]);
    let projected = staff::project(&evidence, &history, Path::new("/fictional/longhand")).unwrap();
    assert_eq!(projected["epoch"], 2);
    assert_eq!(projected["status"], "active");
    assert_eq!(projected["timeline"][0]["kind"], "handoff_completed");
    assert!(projected["verification"].is_null());
    assert!(projected["publication"].is_null());
    assert_eq!(projected["allowPublish"], false);
    let mut stale = history.clone();
    stale[0]["run"]["revision"] = json!(3);
    assert!(staff::project(&evidence, &stale, Path::new("/fictional/longhand")).is_err());
    assert!(staff::project(&evidence, &history, Path::new("/other/project")).is_err());
}

#[test]
fn unresolved_operations_never_appear_as_active_success() {
    let run = json!({"id":"fictional-run","revision":2,"repository":"/fictional/longhand","task":"fictional-task","pending":"host_build","verification":null});
    let evidence = json!({"run":run,"gate_receipts":[]});
    let history = json!([{"kind":"intent","run":run}]);
    let projected = staff::project(&evidence, &history, Path::new("/fictional/longhand")).unwrap();
    assert_eq!(projected["status"], "needs_reconciliation");
    assert!(projected["verification"].is_null());
}

#[test]
fn evidence_sync_preserves_owner_parking_and_customer_unknowns() {
    use cos::state;
    use std::{fs, os::unix::fs::PermissionsExt};

    let root = std::env::temp_dir().join(format!("cos-parked-{}", std::process::id()));
    fs::create_dir_all(root.join("coordinator")).unwrap();
    let coordinator = root.join("coordinator").canonicalize().unwrap();
    state::initialize(&root).unwrap();
    let mut candidate: serde_json::Value = serde_json::from_str(state::DEMO_STATE).unwrap();
    let before_deliverables = candidate["deliverables"].clone();
    let before_kpis = candidate["kpis"].clone();
    let run = json!({"id":"fictional-run","revision":7,"repository":coordinator,"task":"fictional-task","host":"codex","epoch":2,"stage":"planning","pending":null,"checkpoint":"fixture","allow_commit":false,"allow_publish":false,"verification":null,"publication":null});
    let quote = |value: serde_json::Value| value.to_string().replace('\'', "'\\''");
    let engine = root.join("fixture-engine");
    fs::write(&engine, format!(
        "#!/bin/sh\ncase \"$3\" in\nevidence) printf '%s\\n' '{}' ;;\nhistory) printf '%s\\n' '{}' ;;\nresume) printf '%s\\n' '{}' ;;\n*) exit 19 ;;\nesac\n",
        quote(json!({"run":run,"gate_receipts":[]})),
        quote(json!([{"kind":"handoff_completed","run":run}])),
        quote(json!({"run":run}))
    )).unwrap();
    fs::set_permissions(&engine, fs::Permissions::from_mode(0o755)).unwrap();
    candidate["commitments"][0]["id"] = json!("C-longhand-dashboard");
    candidate["commitments"][0]["dashboard"] = json!({"staff":{
        "integration":{"coordinator":coordinator,"engineExecutable":engine},
        "enrollment":{"status":"parked_by_owner","parking":{"ownerRequested":true},"modelAccess":{"status":"not_authorized"}},
        "members":[{"id":"delivery","run":null}]
    }});
    state::commit(&root, &candidate, 0, "Fictional parked configuration").unwrap();
    let projected = staff::sync(&root, &coordinator, "fictional-run", "delivery").unwrap();
    assert_eq!(projected["status"], "parked");
    assert_eq!(projected["stage"], "planning");
    assert_eq!(projected["ownershipVerified"], true);
    let saved = state::read(&root).unwrap().unwrap();
    let staff = &saved["commitments"][0]["dashboard"]["staff"];
    assert_eq!(staff["enrollment"]["status"], "parked_by_owner");
    assert_eq!(
        staff["enrollment"]["modelAccess"]["status"],
        "not_authorized"
    );
    assert_eq!(staff["members"][0]["run"]["status"], "parked");
    assert_eq!(saved["deliverables"], before_deliverables);
    assert_eq!(saved["kpis"], before_kpis);
    fs::remove_dir_all(root).unwrap();
}
