//! Launch a hire into a herdr terminal — a live "space" you watch in herdr —
//! through the `hc` CLI. herdr owns that process for its whole life; cos only
//! decided the hire and judges its worktree afterward. One owner per run.

use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::agents::{Spec, harness};

/// The terminal name herdr shows and `hc` addresses. Within hc's 1-32 char
/// `[a-z0-9-_]` limit for every cos run id (`YYYYMMDD-HHMMSS-xxxx`).
pub(crate) fn name(id: &str) -> String {
    format!("cos-{id}")
}

/// The command herdr runs for the hire: the harness, wrapped in `env` that pins
/// the triple. The harness runs under herdr-core's environment, not cos's, so a
/// `cos` the hire itself runs would otherwise not resolve the business that
/// spawned it. `COS_BUSINESS` is cleared so the pinned dirs decide it.
pub(crate) fn launch(spec: &Spec, home: &Path, workspace: &Path, projects: &Path) -> Vec<String> {
    let inner = harness::command(spec);
    let program = inner.get_program().to_string_lossy().into_owned();
    let mut v = vec![
        "env".to_owned(),
        "-u".to_owned(),
        "COS_BUSINESS".to_owned(),
        format!("COS_HOME={}", home.display()),
        format!("COS_WORKSPACE={}", workspace.display()),
        format!("COS_PROJECTS={}", projects.display()),
        program,
    ];
    v.extend(inner.get_args().map(|a| a.to_string_lossy().into_owned()));
    v
}

/// Start the hire's harness in a new herdr terminal in its worktree, labelled
/// `hire=<id>` so `hc ls --label hire=<id>` finds it and `hc attach` opens it.
/// Returns what `hc run` printed: the terminal's address, as JSON.
pub fn open(spec: &Spec, home: &Path, workspace: &Path, projects: &Path) -> Result<String> {
    let mut hc = Command::new("hc");
    hc.args(["run", "--bg", "--json"])
        .args(["--name", &name(&spec.id)])
        .arg("--cwd")
        .arg(&spec.workdir)
        .args(["--label", &format!("hire={}", spec.id)])
        .args(["--label", &format!("harness={}", spec.harness.program())]);
    if let Some(role) = &spec.role {
        hc.args(["--label", &format!("role={role}")]);
    }
    // `--` stops hc reading the wrapped command's flags as its own.
    hc.arg("--").args(launch(spec, home, workspace, projects));

    let out = hc
        .output()
        .context("run hc — is herdr installed and its core running?")?;
    if !out.status.success() {
        bail!("hc run failed: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_is_prefixed_and_within_hc_limit() {
        let n = name("20261113-225320-f142");
        assert_eq!(n, "cos-20261113-225320-f142");
        assert!(n.len() <= 32);
        assert!(
            n.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        );
    }

    #[test]
    fn launch_pins_the_triple_and_clears_the_business() {
        let spec = Spec {
            id: "20261118-000000-0000".into(),
            harness: crate::agents::Harness::Claude,
            program: "claude".into(),
            model: None,
            worktree: false,
            repo: None,
            base: None,
            workdir: "/tmp/wt".into(),
            title: "t".into(),
            source: "test".into(),
            prompt: "do it".into(),
            deadline_secs: 60,
            budget_usd: 1.0,
            created: String::new(),
            slots: None,
            contract: Default::default(),
            limits: Default::default(),
            role: None,
        };
        let argv = launch(
            &spec,
            Path::new("/h/home"),
            Path::new("/h/ws"),
            Path::new("/h/projects"),
        );
        assert_eq!(&argv[..3], &["env", "-u", "COS_BUSINESS"]);
        assert!(argv.contains(&"COS_HOME=/h/home".to_owned()));
        assert!(argv.contains(&"COS_WORKSPACE=/h/ws".to_owned()));
        assert!(argv.contains(&"COS_PROJECTS=/h/projects".to_owned()));
        let claude = argv.iter().position(|a| a == "claude").unwrap();
        assert!(claude > 5, "the env prefix comes before the program");
        assert_eq!(argv[claude + 1], "-p", "the harness flags follow it");
    }
}
