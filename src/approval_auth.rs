use anyhow::{Context, Result, bail, ensure};
use std::path::Path;

/// Enroll a pinned reviewer only after the workspace owner's approval.
pub fn enroll_reviewer(
    workspace: &Path,
    commitment_id: &str,
    revision: i64,
    action_id: &str,
    program: &Path,
    expected_digest: &str,
) -> Result<()> {
    use crate::workflow::{ReviewerBinding, Workflow, valid_action_id};
    ensure!(valid_action_id(action_id), "Invalid action ID");
    let mut state = crate::state::read(workspace)?.context("Workspace state is missing")?;
    let current_revision = state["revision"].as_i64();
    let commitment = state["commitments"]
        .as_array_mut()
        .context("Commitments missing")?
        .iter_mut()
        .find(|item| item["id"].as_str() == Some(commitment_id))
        .context("Unknown commitment")?;
    let mut flow: Workflow = serde_json::from_value(commitment["workflow"].clone())
        .context("Workflow binding missing")?;
    let draft_digest = flow.draft_digest(workspace)?;
    ensure!(
        flow.pending_review.is_none(),
        "A review needs reconciliation"
    );
    let program = program
        .canonicalize()
        .context("Reviewer executable is missing")?;
    ensure!(
        crate::reviewer::program_digest(&program)? == expected_digest,
        "Reviewer executable changed"
    );
    let binding = ReviewerBinding {
        program,
        sha256: expected_digest.to_owned(),
    };
    if flow.latest_enrollment_action_id.as_deref() == Some(action_id) {
        ensure!(
            flow.reviewer_binding.as_ref() == Some(&binding),
            "Action ID belongs to another reviewer"
        );
        return Ok(());
    }
    ensure!(
        !flow.enrollment_action_ids.iter().any(|id| id == action_id),
        "Action ID was already used"
    );
    ensure!(
        current_revision == Some(revision),
        "Revision conflict: reread and reconcile"
    );
    authenticate(workspace, expected_digest, AuthPurpose::ReviewerEnrollment)?;
    ensure!(
        crate::reviewer::program_digest(&binding.program)? == expected_digest
            && flow.draft_digest(workspace)? == draft_digest,
        "Reviewer or draft changed during authentication"
    );
    flow.reviewer_binding = Some(binding);
    flow.review = None;
    flow.approval = None;
    flow.last_review_failure = None;
    flow.enrollment_action_ids.push(action_id.to_owned());
    flow.latest_enrollment_action_id = Some(action_id.to_owned());
    commitment["workflow"] = serde_json::to_value(flow)?;
    crate::state::commit(
        workspace,
        &state,
        revision,
        "Enroll isolated reviewer executable",
    )?;
    Ok(())
}

/// Authenticate the workspace owner before recording the displayed revision.
pub fn approve_owner(
    workspace: &Path,
    commitment_id: &str,
    revision: i64,
    action_id: &str,
    displayed_digest: &str,
) -> Result<()> {
    let state = crate::state::read(workspace)?.context("Workspace state is missing")?;
    let commitment =
        crate::state::find(&state, "commitments", commitment_id).context("Unknown commitment")?;
    let flow: crate::workflow::Workflow = serde_json::from_value(commitment["workflow"].clone())
        .context("Workflow binding missing")?;
    let current = flow.can_approve(workspace)?;
    ensure!(
        current == displayed_digest,
        "Design changed since it was displayed"
    );
    if flow
        .approval
        .as_ref()
        .is_some_and(|approval| approval.action_id == action_id)
        && flow.approval_is_current(workspace, commitment_id)
    {
        return Ok(());
    }
    ensure!(
        state["revision"].as_i64() == Some(revision),
        "Revision conflict: reread and reconcile"
    );
    authenticate(workspace, &current, AuthPurpose::DesignApproval)?;
    crate::workflow::approve(workspace, commitment_id, revision, action_id)
}

#[derive(Clone, Copy)]
pub enum AuthPurpose {
    DesignApproval,
    ReviewerEnrollment,
}

/// Authenticate the workspace owner through the operating system.
#[cfg(target_os = "macos")]
pub fn authenticate(workspace: &Path, draft_digest: &str, purpose: AuthPurpose) -> Result<()> {
    use block2::RcBlock;
    use objc2::{
        class, msg_send,
        rc::Retained,
        runtime::{AnyObject, Bool},
    };
    use objc2_foundation::NSString;
    use std::{os::unix::fs::MetadataExt, sync::mpsc, time::Duration};

    #[link(name = "LocalAuthentication", kind = "framework")]
    unsafe extern "C" {}

    let owner = std::fs::metadata(workspace.canonicalize()?)?.uid();
    ensure!(
        draft_digest.len() == 64 && draft_digest.bytes().all(|b| b.is_ascii_hexdigit()),
        "Invalid design digest"
    );
    // SAFETY: geteuid has no arguments and does not access memory.
    ensure!(
        owner == unsafe { libc::geteuid() },
        "Workspace owner does not match the signed-in user"
    );
    let (sender, receiver) = mpsc::channel();
    let reply = RcBlock::new(move |success: Bool, _error: *mut AnyObject| {
        let _ = sender.send(success.as_bool());
    });
    let action = match purpose {
        AuthPurpose::DesignApproval => "Approve reviewed design",
        AuthPurpose::ReviewerEnrollment => "Enroll independent reviewer",
    };
    let reason = NSString::from_str(&format!("{action} {}", &draft_digest[..12]));
    // SAFETY: LAContext owns the callback while evaluating Apple's documented policy 2.
    let context: Retained<AnyObject> = unsafe { msg_send![class!(LAContext), new] };
    unsafe {
        let _: () =
            msg_send![&*context, evaluatePolicy: 2isize, localizedReason: &*reason, reply: &*reply];
    }
    if receiver
        .recv_timeout(Duration::from_secs(120))
        .context("Authentication did not complete")?
    {
        Ok(())
    } else {
        bail!("Workspace owner authentication was not completed")
    }
}

#[cfg(not(target_os = "macos"))]
pub fn authenticate(_workspace: &Path, _draft_digest: &str, _purpose: AuthPurpose) -> Result<()> {
    bail!("Workspace owner authentication is unavailable on this platform")
}
