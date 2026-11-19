//! How each harness starts in its Herdr terminal: interactive, the hire's
//! prompt as its first message, its own sandbox on.
//!
//! Claude's Bash runs in Seatbelt with no network; a call that needs more
//! asks you in the terminal, which Herdr shows as `blocked`. Codex runs in
//! its workspace-write sandbox and asks the same way. Neither can push,
//! comment or send without you.

use super::{Harness, Spec};

/// Claude Code's Seatbelt sandbox for Bash: writes limited to the worktree
/// and temp, network refused without a prompt, no retry outside the sandbox.
pub const CLAUDE_SANDBOX: &str = r#"{"sandbox":{"enabled":true,"autoAllowBashIfSandboxed":true,"allowUnsandboxedCommands":false,"failIfUnavailable":true,"network":{"strictAllowlist":true}}}"#;

/// The agent's command line, program first.
pub fn argv(spec: &Spec) -> Vec<String> {
    let mut a = vec![spec.program.clone()];
    match spec.harness {
        Harness::Claude => {
            a.push(spec.prompt.clone());
            a.extend(["--permission-mode".into(), "acceptEdits".into()]);
            a.extend(["--settings".into(), CLAUDE_SANDBOX.into()]);
            if let Some(m) = &spec.model {
                a.extend(["--model".into(), m.clone()]);
            }
        }
        Harness::Codex => {
            a.extend(["--sandbox".into(), "workspace-write".into()]);
            a.extend(["--ask-for-approval".into(), "on-request".into()]);
            a.push("--cd".into());
            a.push(spec.workdir.to_string_lossy().into_owned());
            if let Some(m) = &spec.model {
                a.extend(["--model".into(), m.clone()]);
            }
            // `--` keeps a prompt starting with "review" or "resume" from
            // being taken as a subcommand.
            a.extend(["--".into(), spec.prompt.clone()]);
        }
    }
    a
}
