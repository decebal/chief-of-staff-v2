//! The starter kit as shipped from this repo: its own tests and release
//! preflight still pass, and the Codex and Claude Code manifests agree.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serde_json::Value;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn json(rel: &str) -> Value {
    serde_json::from_slice(&std::fs::read(root().join(rel)).unwrap()).unwrap()
}

fn python() -> bool {
    let ok = Command::new("python3")
        .args([
            "-c",
            "import sys; sys.exit(0 if sys.version_info >= (3, 10) else 1)",
        ])
        .status()
        .is_ok_and(|s| s.success());
    if !ok {
        assert!(
            std::env::var_os("CI").is_none(),
            "CI must have Python 3.10+"
        );
        eprintln!("SKIPPED: no Python 3.10+");
    }
    ok
}

fn run(cmd: &mut Command) -> cos::proc::Output {
    cos::proc::run(cmd, Duration::from_secs(120)).unwrap()
}

#[test]
fn the_kits_own_state_tests_pass() {
    if !python() {
        return;
    }
    let scripts = cos::state::kit_dir(&root()).join("scripts");
    let out = run(Command::new("python3")
        .args(["-B", "-m", "unittest", "-q", "test_state"])
        .current_dir(&scripts));
    assert!(out.status.success(), "{}\n{}", out.stdout, out.stderr);
    assert!(out.stderr.contains("OK"), "{}", out.stderr);
}

#[test]
fn the_kits_release_preflight_passes() {
    if !python() {
        return;
    }
    let out = run(Command::new("python3")
        .args(["-B", "scripts/release.py"])
        .current_dir(root()));
    assert!(out.status.success(), "{}\n{}", out.stdout, out.stderr);
    let v: Value = serde_json::from_str(out.stdout.trim()).unwrap();
    let listed = json("release-files.json")["files"]
        .as_array()
        .unwrap()
        .len();
    assert_eq!(v["validatedFiles"].as_u64(), Some(listed as u64));
}

#[test]
fn codex_and_claude_manifests_agree_with_the_release() {
    let version = json("release-files.json")["version"].clone();
    let codex = json("plugins/chief-of-staff-starter/plugin.json");
    let claude = json("plugins/chief-of-staff-starter/.claude-plugin/plugin.json");
    assert_eq!(codex["version"], version);
    assert_eq!(claude["version"], version);
    assert_eq!(codex["name"], claude["name"]);

    let market = json(".claude-plugin/marketplace.json");
    let entry = &market["plugins"][0];
    assert_eq!(entry["name"], claude["name"]);
    let src = entry["source"].as_str().unwrap();
    assert!(
        root()
            .join(src)
            .join(".claude-plugin/plugin.json")
            .is_file(),
        "{src}"
    );

    for rel in [
        ".claude-plugin/marketplace.json",
        "plugins/chief-of-staff-starter/.claude-plugin/plugin.json",
    ] {
        let listed = json("release-files.json")["files"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f == rel);
        assert!(listed, "{rel} is not in release-files.json");
    }
}

#[test]
fn the_skill_is_where_both_clients_look() {
    let skill = cos::state::kit_dir(&root()).join("SKILL.md");
    let text = std::fs::read_to_string(&skill).unwrap();
    assert!(text.starts_with("---\nname: chief-of-staff-starter\n"));
    for reference in [
        "setup",
        "review",
        "strategy",
        "delegation",
        "scheduling",
        "state-and-recovery",
    ] {
        assert!(
            Path::new(&skill)
                .parent()
                .unwrap()
                .join(format!("references/{reference}.md"))
                .is_file(),
            "{reference}"
        );
    }
}

#[test]
fn the_profile_template_grants_nothing() {
    let p: Value = serde_json::from_str(cos::state::PROFILE_TEMPLATE).unwrap();
    assert!(!cos::state::tracking_approved(&p));
    assert!(cos::state::approved_sources(&p).is_empty());
    assert_eq!(p["schedule"]["enabled"], Value::Bool(false));
}
