//! Login for unattended runs. The interactive Claude Code login expires; a
//! scheduled brief then fails with a 401 and nobody is there to `/login`.
//! `claude setup-token` makes a long-lived token, which `cos` keeps in the
//! macOS Keychain and hands to every `claude` it starts.

use std::process::Command;
use std::time::Duration;

use crate::proc;

/// The variable `claude` reads; a bogus value is refused with a 401, so it is
/// honoured over the interactive login.
pub const ENV: &str = "CLAUDE_CODE_OAUTH_TOKEN";

/// Keychain item: `security add-generic-password -s cos-claude-oauth -a "$USER" -w`.
pub const KEYCHAIN_SERVICE: &str = "cos-claude-oauth";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// Already in the environment.
    Env,
    /// Loaded from the Keychain.
    Keychain,
    /// Neither: `claude` falls back to the interactive login.
    Interactive,
}

impl Source {
    pub fn describe(self) -> &'static str {
        match self {
            Self::Env => "long-lived token from the environment",
            Self::Keychain => "long-lived token from the Keychain",
            Self::Interactive => "the interactive login, which expires",
        }
    }
}

/// Puts the Keychain token into this process's environment, so every child
/// inherits it. Call first thing in `main`, before any thread exists.
pub fn load() -> Source {
    if std::env::var_os(ENV).is_some_and(|v| !v.is_empty()) {
        return Source::Env;
    }
    let Some(token) = keychain_token() else {
        return Source::Interactive;
    };
    // SAFETY: called at the top of `main`, before the tokio runtime or any
    // other thread starts, so nothing else reads the environment concurrently.
    unsafe {
        std::env::set_var(ENV, token);
    }
    Source::Keychain
}

fn keychain_token() -> Option<String> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    let mut cmd = Command::new("security");
    cmd.args(["find-generic-password", "-s", KEYCHAIN_SERVICE, "-w"]);
    let out = proc::run(&mut cmd, Duration::from_secs(5)).ok()?;
    let token = out.stdout.trim();
    (out.status.success() && !token.is_empty()).then(|| token.to_owned())
}

/// Posts a macOS notification; used when an unattended run fails, so the
/// failure is seen the morning it happens rather than found in a log later.
pub fn notify(title: &str, body: &str) {
    if !cfg!(target_os = "macos") {
        return;
    }
    let quote = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    let body: String = body.chars().take(220).collect();
    let script = format!(
        "display notification \"{}\" with title \"{}\"",
        quote(&body),
        quote(title)
    );
    let mut cmd = Command::new("osascript");
    cmd.args(["-e", &script]);
    let _ = proc::run(&mut cmd, Duration::from_secs(5));
}
