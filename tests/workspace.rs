//! The workspace (`.chief-of-staff/`) lives apart from the home (`context/`,
//! `out/`): every state and profile read follows the workspace.

use std::path::PathBuf;

use cos::{context, gather, pipeline, report, state};
use serde_json::Value;

struct Dir(PathBuf);

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn dir(name: &str) -> Dir {
    let d = std::env::temp_dir().join(format!("cos-ws-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    Dir(d.canonicalize().unwrap())
}

fn seeded(name: &str) -> Dir {
    let ws = dir(name);
    state::initialize(&ws.0).unwrap();
    let demo: Value = serde_json::from_str(state::DEMO_STATE).unwrap();
    state::commit(&ws.0, &demo, 0, "Fictional fixture").unwrap();
    ws
}

fn first_goal() -> String {
    let demo: Value = serde_json::from_str(state::DEMO_STATE).unwrap();
    demo["goals"][0]["title"].as_str().unwrap().to_owned()
}

#[test]
fn brains_read_the_workspace_not_the_home() {
    let (home, ws) = (dir("home-a"), seeded("ws-a"));
    assert!(context::block(&home.0, &ws.0).contains(&first_goal()));
    assert!(!context::block(&home.0, &home.0).contains(&first_goal()));
}

#[test]
fn the_report_plan_view_follows_the_workspace() {
    let (home, ws) = (dir("home-b"), seeded("ws-b"));
    let split = report::data(&home.0, &ws.0, false).unwrap();
    assert_eq!(split["workspace"]["revision"], 1, "{}", split["workspace"]);
    let same = report::data(&home.0, &home.0, false).unwrap();
    assert!(same["workspace"].is_null());
}

#[test]
fn dash_version_moves_when_the_workspace_commits() {
    let (home, ws) = (dir("home-c"), seeded("ws-c"));
    let before = cos::dash::version(&home.0, &ws.0);
    std::fs::write(ws.0.join(".chief-of-staff/candidate.json"), "{}").unwrap();
    assert_ne!(before, cos::dash::version(&home.0, &ws.0));
}

#[test]
fn config_reads_workspace_profile() {
    let (home, ws) = (dir("home-e"), seeded("ws-e"));
    let path = state::runtime_dir(&ws.0).unwrap().join("profile.json");
    let mut profile: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    profile["jiraProjects"] = serde_json::json!(["ACME"]);
    std::fs::write(&path, profile.to_string()).unwrap();
    let config = pipeline::Config {
        home: home.0.clone(),
        install: home.0.clone(),
        workspace: ws.0.clone(),
        projects: home.0.clone(),
        budget: None,
        bins: pipeline::Bins::default(),
    };
    let run = config.brief_run(&gather::Facts::default());
    assert!(run.prompt.contains("project in (ACME)"), "{}", run.prompt);
}

#[test]
fn dash_version_ignores_agent_writes() {
    let (home, ws) = (dir("home-d"), seeded("ws-d"));
    let before = cos::dash::version(&home.0, &ws.0);
    std::fs::create_dir_all(home.0.join("out/agents/x")).unwrap();
    assert_eq!(before, cos::dash::version(&home.0, &ws.0));
}
