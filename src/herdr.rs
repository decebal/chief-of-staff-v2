//! Launch a hire into a herdr terminal — a live "space" you watch in herdr —
//! through the `hc` CLI. herdr owns that process for its whole life; cos only
//! decided the hire and judges its worktree afterward. One owner per run.

use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::agents::{Spec, harness};

/// The terminal name herdr shows and `hc` addresses. Within hc's 1-32 char
/// `[a-z0-9-_]` limit for every cos run id (`YYYYMMDD-HHMMSS-xxxx`).
fn name(id: &str) -> String {
    format!("cos-{id}")
}

/// Start the hire's harness in a new herdr terminal in its worktree, labelled
/// `hire=<id>` so `hc ls --label hire=<id>` finds it and `hc attach` opens it.
/// Returns what `hc run` printed: the terminal's address, as JSON.
pub fn open(spec: &Spec) -> Result<String> {
    let inner = harness::command(spec);
    let program = inner.get_program().to_string_lossy().into_owned();
    let args: Vec<String> = inner
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();

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
    // `--` stops hc reading the harness's own flags (claude's `-p`, codex's
    // `--`) as its own.
    hc.arg("--").arg(program).args(args);

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
}
