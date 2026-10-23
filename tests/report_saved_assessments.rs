use cos::{report, state};
use serde_json::{Value, json};

#[test]
fn saved_receipts_override_old_coverage_without_rewriting_history_or_watermarks() {
    let root = std::env::temp_dir().join(format!("cos-report-saved-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    state::initialize(&root).unwrap();
    let mut candidate: Value = serde_json::from_str(state::DEMO_STATE).unwrap();
    candidate["sources"][0]["checkedAt"] = json!("2030-04-08T10:00:00Z");
    let source = candidate["sources"][0]["id"].clone();
    let watermark = candidate["sources"][0]["lastCompleteThrough"].clone();
    let run = |id: &str, at: &str, note: &str| {
        json!({
            "id": id, "invokedAt": at, "finishedAt": at,
            "mode": "publication_receipt", "status": "partial",
            "notificationDecision": "quiet",
            "coverage": [{"source": source, "health": "healthy", "review": "partial", "note": note}]
        })
    };
    candidate["runs"] = json!([
        run(
            "new",
            "2030-04-09T10:00:00Z",
            "Pushed; installed verification open"
        ),
        run("old", "2030-04-09T10:30:00+01:00", "Local patch only")
    ]);
    state::commit(&root, &candidate, 0, "Fictional report fixture").unwrap();

    let home = root.join("engine-home");
    std::fs::create_dir_all(&home).unwrap();
    let data = report::data(&home, &root, true).unwrap();
    let workspace = &data["workspace"];
    assert_eq!(workspace["runs"][1]["id"], "new");
    assert_eq!(
        workspace["sources"][0]["note"],
        "Pushed; installed verification open"
    );
    assert_eq!(workspace["sources"][0]["review"], "partial");
    assert_eq!(workspace["sources"][0]["lastCompleteThrough"], watermark);
    let persisted = state::read(&root).unwrap().unwrap();
    assert_eq!(persisted["runs"][1]["id"], "old");
    assert_ne!(
        persisted["sources"][0]["note"],
        workspace["sources"][0]["note"]
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn retired_targets_are_absent_but_proposals_and_unknown_readings_survive() {
    let root = std::env::temp_dir().join(format!("cos-report-retired-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    state::initialize(&root).unwrap();
    let mut candidate: Value = serde_json::from_str(state::DEMO_STATE).unwrap();
    let mut active = candidate["kpis"][0].clone();
    active["current"] = Value::Null;
    let mut retired = active.clone();
    retired["id"] = json!("retired");
    retired["status"] = json!("retired");
    retired["name"] = json!("Retired target");
    let mut excluded = active.clone();
    excluded["id"] = json!("excluded");
    excluded["excludeFromActiveScore"] = json!(true);
    let mut retired_basis = active.clone();
    retired_basis["id"] = json!("retired-basis");
    retired_basis["targetBasis"] = json!("retired");
    let mut proposed = active.clone();
    proposed["id"] = json!("later");
    proposed["status"] = json!("proposed");
    proposed["name"] = json!("Later target");
    candidate["kpis"] = json!([active, retired, excluded, retired_basis, proposed]);
    state::commit(&root, &candidate, 0, "Fictional KPI fixture").unwrap();

    let data = report::data(&root, &root, true).unwrap();
    let kpis = data["workspace"]["kpis"].as_array().unwrap();
    assert_eq!(kpis.len(), 2);
    assert!(kpis[0]["current"].is_null());
    assert_eq!(kpis[1]["id"], "later");
    let handoffs = data["workspace"]["handoffs"].as_object().unwrap();
    assert!(!handoffs.is_empty());
    let handoff_text = serde_json::to_string(handoffs).unwrap();
    assert!(!handoff_text.contains("Retired target"));
    assert!(!handoff_text.contains("Later target"));
    assert_eq!(
        state::read(&root).unwrap().unwrap()["kpis"]
            .as_array()
            .unwrap()
            .len(),
        5
    );
    std::fs::remove_dir_all(root).unwrap();
}
