use cos::{report, state};
use serde_json::{Value, json};

#[test]
fn production_evidence_roundtrips_without_promoting_unknown_commercial_proof() {
    let root = std::env::temp_dir().join(format!("cos-production-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    state::initialize(&root).unwrap();
    let mut candidate: Value = serde_json::from_str(state::DEMO_STATE).unwrap();
    let production = json!({
        "scope": "Fictional project only",
        "assessedAt": "2030-04-09T10:00:00Z",
        "platforms": [{
            "id": "web",
            "deployment": {"status": "verified", "label": "Production deployed"},
            "payment": {"status": "unknown", "label": "Live payment unverified"},
            "traffic": {"status": "unknown", "label": "Traffic unverified"}
        }],
        "rows": [{"id": "fictional-bet", "kind": "bet", "outcome": {"status": "unknown"}}],
        "paymentSuccesses": null,
        "trafficUsers": null
    });
    candidate["commitments"][0]["dashboard"] = json!({"production": production});
    state::commit(&root, &candidate, 0, "Fictional production evidence").unwrap();

    let data = report::data(&root, &root, true).unwrap();
    assert_eq!(data["workspace"]["production"], production);
    assert!(data["workspace"]["production"]["paymentSuccesses"].is_null());
    assert!(data["workspace"]["production"]["trafficUsers"].is_null());
    assert_eq!(
        data["workspace"]["production"]["rows"][0]["outcome"]["status"],
        "unknown"
    );
    let html = report::render(&data).unwrap();
    assert!(html.contains("production-board"));
    assert!(html.contains("Live payment unverified"));
    assert!(html.contains("Traffic unverified"));
    let persisted = state::read(&root).unwrap().unwrap();
    assert_eq!(
        persisted["commitments"][0]["dashboard"]["production"],
        production
    );
    std::fs::remove_dir_all(root).unwrap();
}
