use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{io::Read, path::Path, process::Command, time::Duration};

use crate::{
    proc,
    workflow::{self, Finding, Review, ReviewStart, Workflow},
};

/// Journal the review intent, then run it in this process. Retrying with the same
/// action ID resumes a pending review or reports the completed one.
pub fn run(workspace: &Path, commitment_id: &str, action_id: &str) -> Result<ReviewStart> {
    let current = crate::state::read(workspace)?.context("Workspace state is missing")?;
    let revision = current["revision"]
        .as_i64()
        .context("State revision is missing")?;
    let start = workflow::begin_review(workspace, commitment_id, revision, action_id)?;
    if start != ReviewStart::AlreadyCompleted {
        execute_pending(workspace, commitment_id, action_id)?;
    }
    Ok(start)
}

/// Execute a journaled intent; a supervisor must own this process to completion.
pub fn execute_pending(workspace: &Path, commitment_id: &str, action_id: &str) -> Result<()> {
    let after_start = crate::state::read(workspace)?.context("Workspace state is missing")?;
    let pending = crate::state::find(&after_start, "commitments", commitment_id)
        .context("Unknown commitment")?;
    let recorded: Workflow =
        serde_json::from_value(pending["workflow"].clone()).context("Workflow binding missing")?;
    let binding = recorded
        .reviewer_binding
        .as_ref()
        .context("No reviewer is enrolled")?;
    let draft = recorded.draft_bytes(workspace)?;
    if !recorded.pending_review.as_ref().is_some_and(|pending| {
        pending.action_id == action_id && pending.draft_digest == digest(&draft)
    }) {
        let rev = after_start["revision"]
            .as_i64()
            .context("State revision is missing")?;
        workflow::fail_review(workspace, commitment_id, rev, action_id)?;
        bail!("Draft changed before review started");
    }
    let result = review(&binding.program, &binding.sha256, &draft);
    let current = crate::state::read(workspace)?.context("Workspace state is missing")?;
    let current_revision = current["revision"]
        .as_i64()
        .context("State revision is missing")?;
    match result {
        Ok(receipt) => {
            if let Err(error) = workflow::record_review(
                workspace,
                commitment_id,
                current_revision,
                action_id,
                receipt,
            ) {
                let latest =
                    crate::state::read(workspace)?.context("Workspace state is missing")?;
                let latest_revision = latest["revision"]
                    .as_i64()
                    .context("State revision is missing")?;
                workflow::fail_review(workspace, commitment_id, latest_revision, action_id)?;
                return Err(error);
            }
            Ok(())
        }
        Err(error) => {
            workflow::fail_review(workspace, commitment_id, current_revision, action_id)?;
            Err(error)
        }
    }
}

const PHASES: [&str; 5] = [
    "initial_zoom_out",
    "comprehension",
    "design_critique",
    "intermediate_zoom_out",
    "implementation_readiness",
];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Output {
    findings: Vec<OutputFinding>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OutputFinding {
    description: String,
    evidence: String,
}

fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub fn program_digest(program: &Path) -> Result<String> {
    const LIMIT: usize = 64 * 1024 * 1024;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = std::fs::metadata(program)?;
        ensure!(
            metadata.is_file() && metadata.permissions().mode() & 0o111 != 0,
            "Reviewer must be an executable file"
        );
    }
    let mut data = Vec::new();
    std::fs::File::open(program)?
        .take(LIMIT as u64 + 1)
        .read_to_end(&mut data)?;
    ensure!(data.len() <= LIMIT, "Reviewer exceeds size limit");
    Ok(digest(&data))
}

/// Run each phase in a fresh, networkless process with only draft bytes on stdin.
pub fn review(program: &Path, expected_digest: &str, draft: &[u8]) -> Result<Review> {
    ensure!(
        cfg!(target_os = "macos"),
        "Isolated reviewer is unavailable on this platform"
    );
    ensure!(
        !draft.is_empty() && draft.len() <= 1024 * 1024,
        "Invalid review input size"
    );
    let program = program.canonicalize().context("Reviewer is unavailable")?;
    ensure!(
        program_digest(&program).context("Cannot read reviewer")? == expected_digest,
        "Reviewer executable changed"
    );
    let program_path = program.to_str().context("Reviewer path is not UTF-8")?;
    let quoted_path = serde_json::to_string(program_path)?;
    let profile = format!(
        "(version 1)(allow default)(deny network*)(deny mach-lookup)(deny ipc*)(deny process-fork)(deny process-info*)(deny sysctl-read)(deny file-read* (subpath \"/\"))(deny file-write* (subpath \"/\"))(allow file-read* (literal \"/\") (subpath \"/System/Library\") (subpath \"/usr/lib\") (literal \"/bin/sh\") (literal {quoted_path}))"
    );
    let deadline = proc::Deadline::after(Duration::from_secs(120));
    let mut findings = Vec::new();
    for phase in PHASES {
        let remaining = deadline.left();
        ensure!(!remaining.is_zero(), "Review deadline exceeded");
        let mut command = Command::new("/usr/bin/sandbox-exec");
        command
            .env_clear()
            .current_dir("/")
            .arg("-p")
            .arg(&profile)
            .arg(&program)
            .arg(phase);
        let output = proc::run_input(&mut command, draft, remaining)?;
        if !output.status.success() {
            bail!("Reviewer failed during {phase}");
        }
        let parsed: Output =
            serde_json::from_str(&output.stdout).context("Reviewer returned invalid findings")?;
        ensure!(parsed.findings.len() <= 100, "Too many review findings");
        for finding in parsed.findings {
            ensure!(
                !finding.description.trim().is_empty() && !finding.evidence.trim().is_empty(),
                "Reviewer finding lacks evidence"
            );
            findings.push(Finding {
                description: finding.description,
                evidence: finding.evidence,
                resolution: None,
            });
        }
        ensure!(
            program_digest(&program)? == expected_digest,
            "Reviewer executable changed during review"
        );
    }
    Ok(Review {
        draft_digest: digest(draft),
        reviewer: format!("sha256:{expected_digest}"),
        input_boundary: "Five fresh networkless processes; draft supplied on stdin".into(),
        phases: PHASES.into_iter().map(str::to_owned).collect(),
        findings,
    })
}
