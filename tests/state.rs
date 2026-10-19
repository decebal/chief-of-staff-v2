//! The kit's `test_state.py`, ported one test for one, plus a parity check:
//! the Python helper and `cos::state` must accept and refuse the same
//! candidates, since both write the same `.chief-of-staff/state.json`.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use cos::state;
use serde_json::{Value, json};

struct Ws(PathBuf);

impl Drop for Ws {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn ws(name: &str) -> Ws {
    let d = std::env::temp_dir().join(format!("cos-state-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let d = d.canonicalize().unwrap();
    state::initialize(&d).unwrap();
    Ws(d)
}

fn demo() -> Value {
    serde_json::from_str(state::DEMO_STATE).unwrap()
}

fn current(root: &Path) -> Value {
    state::load(&root.join(".chief-of-staff/state.json")).unwrap()
}

fn err(r: anyhow::Result<impl std::fmt::Debug>) -> String {
    format!("{:#}", r.expect_err("expected a refusal"))
}

#[test]
fn empty_and_demo_validate() {
    let w = ws("valid");
    state::validate(&current(&w.0)).unwrap();
    state::validate(&demo()).unwrap();
}

#[test]
fn initialize_does_not_overwrite() {
    let w = ws("init");
    assert!(state::initialize(&w.0).is_err());
    assert_eq!(current(&w.0)["revision"], json!(0));
}

#[test]
fn defaults_have_no_permission_or_schedule() {
    let w = ws("defaults");
    let p = state::load(&w.0.join(".chief-of-staff/profile.json")).unwrap();
    assert_eq!(p["schedule"]["enabled"], json!(false));
    assert!(
        p["permissions"]
            .as_object()
            .unwrap()
            .values()
            .all(|v| v == &json!(false))
    );
    assert_eq!(p["recovery"]["automaticRateLimitProbe"], json!(false));
    assert_eq!(p["sources"], json!([]));
    assert!(!state::tracking_approved(&p));
}

#[test]
fn commit_and_readback() {
    let w = ws("commit");
    state::commit(&w.0, &demo(), 0, "Use fictional fixture").unwrap();
    let saved = current(&w.0);
    assert_eq!(saved["revision"], json!(1));
    assert_eq!(saved["deliverables"], demo()["deliverables"]);
    assert_eq!(saved["journal"].as_array().unwrap().len(), 1);
}

#[test]
fn stale_writer_rejected() {
    let w = ws("stale");
    state::commit(&w.0, &demo(), 0, "First writer").unwrap();
    assert!(err(state::commit(&w.0, &demo(), 0, "Stale writer")).contains("Revision conflict"));
    assert_eq!(current(&w.0)["revision"], json!(1));
}

#[test]
fn concurrent_writers_only_one_wins() {
    let w = ws("concurrent");
    let gate = std::sync::Barrier::new(2);
    let results: Vec<bool> = std::thread::scope(|s| {
        let hs: Vec<_> = (0..2)
            .map(|_| {
                s.spawn(|| {
                    gate.wait();
                    state::commit(&w.0, &demo(), 0, "Concurrent fixture").is_ok()
                })
            })
            .collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    assert_eq!(results.iter().filter(|ok| **ok).count(), 1, "{results:?}");
    assert_eq!(current(&w.0)["revision"], json!(1));
}

#[test]
fn lock_is_not_stolen() {
    let w = ws("lock");
    let lock = w.0.join(".chief-of-staff/write.lock");
    std::fs::write(&lock, "").unwrap();
    assert!(err(state::commit(&w.0, &demo(), 0, "Must stop")).contains("write lock held"));
    assert!(lock.exists());
}

#[test]
fn reviewing_waiting_does_not_complete() {
    let w = ws("review");
    state::commit(&w.0, &demo(), 0, "Fixture").unwrap();
    let mut updated = current(&w.0);
    updated["commitments"][0]["reviewedAt"] = json!("2030-04-09T12:00:00Z");
    state::commit(&w.0, &updated, 1, "Acknowledge").unwrap();
    let item = &current(&w.0)["commitments"][0];
    assert_eq!(item["status"], json!("waiting"));
    assert!(item["dueDate"].is_null());
    assert!(!item["meetingAt"].is_null());
}

#[test]
fn completion_needs_evidence() {
    let mut d = demo();
    d["deliverables"][0]["status"] = json!("complete");
    assert!(err(state::validate(&d)).contains("requires evidence"));
}

#[test]
fn completed_work_cannot_be_reopened() {
    let w = ws("reopen");
    let mut d = demo();
    d["deliverables"][0]["status"] = json!("complete");
    d["deliverables"][0]["completionEvidence"] = json!({ "reference": "https://example.com/fictional/receipt", "verifiedAt": "2030-04-09T12:00:00Z" });
    state::commit(&w.0, &d, 0, "Verified fixture").unwrap();
    let mut updated = current(&w.0);
    updated["deliverables"][0]["status"] = json!("open");
    assert!(err(state::commit(&w.0, &updated, 1, "Bad reopening")).contains("cannot be reopened"));
}

#[test]
fn record_deletion_rejected() {
    let w = ws("delete");
    state::commit(&w.0, &demo(), 0, "Fixture").unwrap();
    let mut updated = current(&w.0);
    updated["commitments"] = json!([]);
    assert!(err(state::commit(&w.0, &updated, 1, "Bad deletion")).contains("cannot be deleted"));
}

#[test]
fn journal_tampering_rejected() {
    let w = ws("journal");
    state::commit(&w.0, &demo(), 0, "Fixture").unwrap();
    let mut updated = current(&w.0);
    updated["journal"][0]["reason"] = json!("Rewritten");
    assert!(err(state::commit(&w.0, &updated, 1, "Bad history")).contains("journal is immutable"));
}

#[test]
fn missing_goal_rejected() {
    let mut d = demo();
    d["deliverables"][0]["goalId"] = json!("absent");
    assert!(err(state::validate(&d)).contains("Missing linked goal"));
}

#[test]
fn dependency_cycle_rejected() {
    let mut d = demo();
    let mut second = d["deliverables"][0].clone();
    second["id"] = json!("D-2");
    second["dependencies"] = json!(["D-1"]);
    d["deliverables"][0]["dependencies"] = json!(["D-2"]);
    d["deliverables"].as_array_mut().unwrap().push(second);
    assert!(err(state::validate(&d)).contains("Cyclic"));
}

#[test]
fn measured_kpi_requires_evidence() {
    let mut d = demo();
    d["kpis"][0]["current"] = json!(0);
    assert!(err(state::validate(&d)).contains("requires source"));
}

#[test]
fn source_probe_does_not_advance_coverage() {
    let w = ws("probe");
    state::commit(&w.0, &demo(), 0, "Fixture").unwrap();
    let mut updated = current(&w.0);
    updated["sources"][0]["checkedAt"] = json!("2030-04-09T12:00:00Z");
    state::commit(&w.0, &updated, 1, "Healthy probe only").unwrap();
    let s = &current(&w.0)["sources"][0];
    assert_eq!(s["review"], json!("partial"));
    assert!(s["lastCompleteThrough"].is_null());
}

#[test]
fn partial_review_cannot_advance_watermark() {
    let w = ws("watermark");
    state::commit(&w.0, &demo(), 0, "Fixture").unwrap();
    let mut updated = current(&w.0);
    updated["sources"][0]["lastCompleteThrough"] = json!("2030-04-09T09:00:00Z");
    assert!(
        err(state::commit(&w.0, &updated, 1, "Incomplete coverage")).contains("Partial review")
    );
}

#[test]
fn original_denial_preserved() {
    let w = ws("denial");
    let mut d = demo();
    d["incidents"] = json!([{ "id": "I-1", "sourceId": "email", "kind": "permission_denied", "observedAt": "2030-04-09T09:00:00Z" }]);
    state::commit(&w.0, &d, 0, "Record fixture denial").unwrap();
    let mut updated = current(&w.0);
    updated["incidents"][0]["kind"] = json!("rate_limit");
    assert!(err(state::commit(&w.0, &updated, 1, "Bad relabel")).contains("identity is immutable"));
}

#[test]
fn schema_and_unknown_fields_rejected() {
    for (k, v) in [("schemaVersion", json!(2)), ("unexpected", json!("data"))] {
        let mut d = demo();
        d[k] = v;
        assert!(state::validate(&d).is_err(), "{k}");
    }
}

#[test]
fn invalid_candidate_preserves_state_and_releases_lock() {
    let w = ws("invalid");
    let before = current(&w.0);
    assert!(state::commit(&w.0, &json!({}), 0, "Invalid candidate").is_err());
    assert_eq!(current(&w.0), before);
    assert!(!w.0.join(".chief-of-staff/write.lock").exists());
}

#[cfg(unix)]
#[test]
fn symlinked_runtime_rejected() {
    let w = ws("symlink");
    let other = w.0.join("other");
    std::fs::create_dir(&other).unwrap();
    std::os::unix::fs::symlink(w.0.join(".chief-of-staff"), other.join(".chief-of-staff")).unwrap();
    assert!(err(state::runtime_dir(&other)).contains("symlinked"));
}

#[test]
fn a_workspace_inside_the_plugin_is_refused() {
    let kit = state::kit_dir(Path::new(env!("CARGO_MANIFEST_DIR")));
    assert!(err(state::runtime_dir(&kit)).contains("outside the installed plugin"));
}

/// Candidates both validators must agree on, accepted or refused.
fn parity_cases() -> Vec<(&'static str, Value)> {
    let mut cases = vec![("demo", demo()), ("empty", state::empty_state())];
    let mut push = |name: &'static str, f: &dyn Fn(&mut Value)| {
        let mut d = demo();
        f(&mut d);
        cases.push((name, d));
    };
    push("complete-no-evidence", &|d| {
        d["deliverables"][0]["status"] = json!("complete")
    });
    push("missing-goal", &|d| {
        d["deliverables"][0]["goalId"] = json!("absent")
    });
    push("kpi-measured-no-source", &|d| {
        d["kpis"][0]["current"] = json!(0)
    });
    push("kpi-bool", &|d| d["kpis"][0]["target"] = json!(true));
    push("bad-date", &|d| {
        d["deliverables"][0]["dueDate"] = json!("2030-4-10")
    });
    push("naive-time", &|d| {
        d["deliverables"][0]["evidenceAt"] = json!("2030-04-09T09:00:00")
    });
    push("offset-time", &|d| {
        d["deliverables"][0]["evidenceAt"] = json!("2030-04-09T11:00:00+02:00")
    });
    push("date-without-basis", &|d| {
        d["deliverables"][0]["dueBasis"] = json!("unknown")
    });
    push("no-done-when", &|d| {
        d["deliverables"][0]["doneWhen"] = json!(" ")
    });
    push("bad-owner-basis", &|d| {
        d["deliverables"][0]["ownerBasis"] = json!("guess")
    });
    push("deps-null", &|d| {
        d["deliverables"][0]["dependencies"] = Value::Null
    });
    push("self-dep", &|d| {
        d["deliverables"][0]["dependencies"] = json!(["D-1"])
    });
    push("commitment-no-review", &|d| {
        d["commitments"][0]["nextReview"] = Value::Null
    });
    push("bad-health", &|d| d["sources"][0]["health"] = json!("ok"));
    push("complete-no-watermark", &|d| {
        d["sources"][0]["review"] = json!("complete")
    });
    push("complete-ok", &|d| {
        d["sources"][0]["review"] = json!("complete");
        d["sources"][0]["lastCompleteThrough"] = json!("2030-04-09T09:00:00Z");
    });
    push("window-reversed", &|d| {
        d["sources"][0]["windowStart"] = json!("2030-04-10T00:00:00Z")
    });
    push("revision-without-journal", &|d| d["revision"] = json!(1));
    push("revision-float", &|d| d["revision"] = json!(0.0));
    push("duplicate-id", &|d| {
        let g = d["goals"][0].clone();
        d["goals"].as_array_mut().unwrap().push(g);
    });
    push("incident-unknown-source", &|d| {
        d["incidents"] = json!([{ "id": "I-1", "sourceId": "chat", "kind": "rate_limit", "observedAt": "2030-04-09T09:00:00Z" }]);
    });
    push("run-complete-before-invoked", &|d| {
        d["runs"] = json!([{ "id": "R-1", "status": "complete", "invokedAt": "2030-04-09T09:00:00Z", "finishedAt": "2030-04-09T08:00:00Z" }]);
    });
    push("resolved-without-ref", &|d| {
        d["incidents"] = json!([{ "id": "I-1", "sourceId": "email", "kind": "auth_failed", "observedAt": "2030-04-09T09:00:00Z", "resolvedAt": "2030-04-09T10:00:00Z" }]);
    });
    cases
}

fn python() -> Option<PathBuf> {
    let out = Command::new("python3")
        .args(["-c", "import sys; print(sys.version_info >= (3, 10))"])
        .output()
        .ok()?;
    (String::from_utf8_lossy(&out.stdout).trim() == "True").then(|| PathBuf::from("python3"))
}

#[test]
fn python_helper_and_rust_port_agree() {
    let Some(py) = python() else {
        assert!(
            std::env::var_os("CI").is_none(),
            "CI must have Python 3.10+ to check parity with the kit's helper"
        );
        eprintln!("SKIPPED: no Python 3.10+; parity with state.py not checked");
        return;
    };
    let script = state::kit_dir(Path::new(env!("CARGO_MANIFEST_DIR"))).join("scripts/state.py");
    let w = ws("parity");
    for (name, case) in parity_cases() {
        let file = w.0.join(format!("{name}.json"));
        std::fs::write(&file, serde_json::to_vec_pretty(&case).unwrap()).unwrap();
        let mut cmd = Command::new(&py);
        cmd.arg(&script).args(["check", "--file"]).arg(&file);
        let out = cos::proc::run(&mut cmd, Duration::from_secs(30)).unwrap();
        let rust = state::validate(&case);
        assert_eq!(
            out.status.success(),
            rust.is_ok(),
            "{name}: python {:?} / rust {:?}",
            out.stderr.trim(),
            rust.err().map(|e| e.to_string())
        );
    }
}

#[test]
fn python_reads_what_rust_commits() {
    let Some(py) = python() else {
        assert!(
            std::env::var_os("CI").is_none(),
            "CI must have Python 3.10+"
        );
        eprintln!("SKIPPED: no Python 3.10+");
        return;
    };
    let script = state::kit_dir(Path::new(env!("CARGO_MANIFEST_DIR"))).join("scripts/state.py");
    let w = ws("interop");
    state::commit(&w.0, &demo(), 0, "Rust writes").unwrap();
    let mut cmd = Command::new(&py);
    cmd.arg(&script).args(["read", "--workspace"]).arg(&w.0);
    let out = cos::proc::run(&mut cmd, Duration::from_secs(30)).unwrap();
    assert!(out.status.success(), "{}", out.stderr);
    let read: Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(read["revision"], json!(1));

    let mut next = read.clone();
    next["commitments"][0]["reviewedAt"] = json!("2030-04-09T12:00:00Z");
    let cand = w.0.join(".chief-of-staff/candidate.json");
    std::fs::write(&cand, serde_json::to_vec(&next).unwrap()).unwrap();
    let mut cmd = Command::new(&py);
    cmd.arg(&script)
        .args(["commit", "--workspace"])
        .arg(&w.0)
        .args(["--candidate"])
        .arg(&cand)
        .args(["--expected-revision", "1", "--reason", "Python writes"]);
    let out = cos::proc::run(&mut cmd, Duration::from_secs(30)).unwrap();
    assert!(out.status.success(), "{}", out.stderr);
    let after = state::read(&w.0).unwrap().unwrap();
    assert_eq!(
        after["revision"],
        json!(2),
        "Rust validates what Python committed"
    );
}
