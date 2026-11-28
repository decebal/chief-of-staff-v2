//! `task --dry install-mcp` against a temp `HOME`: what the task would run,
//! without running it. Nothing here reaches the real client configs.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use cos::proc;

const TASKFILE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/Taskfile.yml");
const BOUND: Duration = Duration::from_secs(60);

fn home(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("cos-taskfile-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    root.canonicalize().unwrap()
}

/// go-task from the inherited `PATH`, so the run's own `PATH` can leave out
/// `codex`.
fn task_bin() -> PathBuf {
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&path)
        .map(|d| d.join("task"))
        .find(|p| p.is_file())
        .expect("go-task on PATH")
}

/// The dry run's log, stdout and stderr together. Its `PATH` holds no `codex`.
fn dry_install_mcp(home: &Path, codex_home: Option<&Path>) -> String {
    let mut cmd = Command::new(task_bin());
    cmd.arg("--taskfile")
        .arg(TASKFILE)
        .args(["--dry", "install-mcp"])
        .env("HOME", home)
        .env("PATH", "/usr/bin:/bin")
        .env_remove("CODEX_HOME");
    if let Some(c) = codex_home {
        cmd.env("CODEX_HOME", c);
    }
    let out = proc::run(&mut cmd, BOUND).unwrap();
    assert!(
        out.status.success(),
        "task --dry install-mcp: {}",
        out.stderr
    );
    format!("{}{}", out.stdout, out.stderr)
}

#[test]
fn install_mcp_registers_both_clients_through_cos_connect() {
    let home = home("both");
    std::fs::create_dir_all(home.join("Library/Application Support/Claude")).unwrap();
    std::fs::create_dir_all(home.join(".codex")).unwrap();
    let log = dry_install_mcp(&home, None);

    let bin = home.join(".cargo/bin/cos");
    let desktop = format!("{} connect claude-desktop --repair", bin.display());
    let codex = format!("{} connect codex --repair", bin.display());
    let d = log
        .find(&desktop)
        .unwrap_or_else(|| panic!("{desktop}\n{log}"));
    let c = log.find(&codex).unwrap_or_else(|| panic!("{codex}\n{log}"));
    assert!(d < c, "claude-desktop is registered before codex\n{log}");
    assert!(!log.contains("jq"), "{log}");
    assert!(!log.contains("codex mcp"), "{log}");
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn install_mcp_skips_a_client_that_is_not_installed() {
    let home = home("none");
    let log = dry_install_mcp(&home, None);
    assert!(!log.contains(" connect "), "{log}");
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn install_mcp_finds_codex_under_codex_home() {
    let home = home("codex-home");
    let codex_home = home.join("elsewhere");
    std::fs::create_dir_all(&codex_home).unwrap();
    let log = dry_install_mcp(&home, Some(&codex_home));
    assert!(log.contains(" connect codex --repair"), "{log}");
    assert!(!log.contains(" connect claude-desktop"), "{log}");
    let _ = std::fs::remove_dir_all(&home);
}
