use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::state;

const PHASES: [&str; 5] = [
    "initial_zoom_out",
    "comprehension",
    "design_critique",
    "intermediate_zoom_out",
    "implementation_readiness",
];

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Finding {
    pub description: String,
    pub evidence: String,
    pub resolution: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Review {
    pub draft_digest: String,
    pub reviewer: String,
    pub input_boundary: String,
    pub phases: Vec<String>,
    pub findings: Vec<Finding>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Approval {
    pub commitment_id: String,
    pub binding_digest: String,
    pub draft_digest: String,
    pub review_digest: String,
    pub approved_at: String,
    pub action_id: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Binding {
    pub workspace: PathBuf,
    pub repository: PathBuf,
    pub owner: String,
    pub draft: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Workflow {
    pub binding: Binding,
    pub review: Option<Review>,
    pub approval: Option<Approval>,
    #[serde(default)]
    pub approval_action_ids: Vec<String>,
    #[serde(default)]
    pub review_action_ids: Vec<String>,
    #[serde(default)]
    pub latest_review_action_id: Option<String>,
    #[serde(default)]
    pub pending_review: Option<PendingReview>,
    #[serde(default)]
    pub last_review_failure: Option<ReviewFailure>,
    #[serde(default)]
    pub reviewer_binding: Option<ReviewerBinding>,
    #[serde(default)]
    pub enrollment_action_ids: Vec<String>,
    #[serde(default)]
    pub latest_enrollment_action_id: Option<String>,
    #[serde(default)]
    pub resolution_actions: Vec<ResolutionAction>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ResolutionAction {
    pub id: String,
    pub request_digest: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ReviewerBinding {
    pub program: PathBuf,
    pub sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct PendingReview {
    pub action_id: String,
    pub draft_digest: String,
    pub started_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ReviewFailure {
    pub action_id: String,
    pub kind: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ReviewStart {
    Started,
    AlreadyPending,
    AlreadyCompleted,
}

/// Call only after the owned reviewer process has exited and been reaped.
pub fn fail_review(
    workspace: &Path,
    commitment_id: &str,
    expected_revision: i64,
    action_id: &str,
) -> Result<()> {
    let mut candidate = state::read(workspace)?.context("Workspace state is missing")?;
    let current_revision = candidate["revision"].as_i64();
    let commitment = candidate["commitments"]
        .as_array_mut()
        .context("Commitments missing")?
        .iter_mut()
        .find(|item| item["id"].as_str() == Some(commitment_id))
        .context("Unknown commitment")?;
    let mut workflow: Workflow = serde_json::from_value(commitment["workflow"].clone())
        .context("Workflow binding missing")?;
    if workflow
        .last_review_failure
        .as_ref()
        .is_some_and(|failure| failure.action_id == action_id)
    {
        return Ok(());
    }
    ensure!(
        current_revision == Some(expected_revision),
        "Revision conflict: reread and reconcile"
    );
    ensure!(
        workflow
            .pending_review
            .as_ref()
            .is_some_and(|pending| pending.action_id == action_id),
        "Review intent is missing or changed"
    );
    workflow.pending_review = None;
    workflow.last_review_failure = Some(ReviewFailure {
        action_id: action_id.to_owned(),
        kind: "reviewer_failed".to_owned(),
    });
    commitment["workflow"] = serde_json::to_value(workflow)?;
    state::commit(
        workspace,
        &candidate,
        expected_revision,
        "Record failed design review",
    )?;
    Ok(())
}

fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub(crate) fn valid_action_id(id: &str) -> bool {
    id.len() >= 16 && id.len() <= 128 && id.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
}

/// A fresh action ID; reuse one only to retry the same request.
pub fn new_action_id() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    format!("cos-{}-{nanos:x}", std::process::id())
}

/// Attach a design draft in a Git checkout to a commitment. Rebinding is refused:
/// it would orphan the review and approval recorded against the old draft.
pub fn bind(
    workspace: &Path,
    commitment_id: &str,
    expected_revision: i64,
    repository: &Path,
    draft: &Path,
    owner: &str,
) -> Result<()> {
    let binding = Binding {
        workspace: workspace.canonicalize().context("Workspace is missing")?,
        repository: repository.canonicalize().context("Repository is missing")?,
        owner: owner.trim().to_owned(),
        draft: draft.canonicalize().context("Draft is missing")?,
    };
    let flow = Workflow {
        binding: binding.clone(),
        review: None,
        approval: None,
        approval_action_ids: Vec::new(),
        review_action_ids: Vec::new(),
        latest_review_action_id: None,
        pending_review: None,
        last_review_failure: None,
        reviewer_binding: None,
        enrollment_action_ids: Vec::new(),
        latest_enrollment_action_id: None,
        resolution_actions: Vec::new(),
    };
    flow.draft_bytes(workspace)?;
    let mut candidate = state::read(workspace)?.context("Workspace state is missing")?;
    let current_revision = candidate["revision"].as_i64();
    let commitment = candidate["commitments"]
        .as_array_mut()
        .context("Commitments missing")?
        .iter_mut()
        .find(|item| item["id"].as_str() == Some(commitment_id))
        .context("Unknown commitment")?;
    if !commitment["workflow"].is_null() {
        let existing: Workflow = serde_json::from_value(commitment["workflow"].clone())
            .context("Workflow binding is unreadable")?;
        ensure!(
            existing.binding == binding,
            "Commitment is already bound to another draft"
        );
        return Ok(());
    }
    ensure!(
        current_revision == Some(expected_revision),
        "Revision conflict: reread and reconcile"
    );
    commitment["workflow"] = serde_json::to_value(flow)?;
    state::commit(
        workspace,
        &candidate,
        expected_revision,
        "Bind design draft for review",
    )?;
    Ok(())
}

/// What `cos review show` prints: the digests a later command must echo back,
/// each finding by index, and why approval is or is not possible now.
pub fn status(workspace: &Path, commitment_id: &str) -> Result<serde_json::Value> {
    let current = state::read(workspace)?.context("Workspace state is missing")?;
    let commitment =
        state::find(&current, "commitments", commitment_id).context("Unknown commitment")?;
    let flow: Workflow = serde_json::from_value(commitment["workflow"].clone())
        .context("No design is bound to this commitment; run `cos review bind`")?;
    let findings: Vec<_> = flow
        .review
        .iter()
        .flat_map(|review| review.findings.iter().enumerate())
        .map(|(index, finding)| {
            serde_json::json!({
                "index": index,
                "description": finding.description,
                "evidence": finding.evidence,
                "resolution": finding.resolution,
            })
        })
        .collect();
    let approvable = flow.can_approve(workspace);
    Ok(serde_json::json!({
        "commitment": commitment_id,
        "revision": current["revision"],
        "binding": flow.binding,
        "reviewer": flow.reviewer_binding,
        "draftDigest": flow.draft_digest(workspace).ok(),
        "reviewDigest": flow.review_digest().ok(),
        "reviewedDigest": flow.review.as_ref().map(|review| &review.draft_digest),
        "pendingReview": flow.pending_review,
        "lastReviewFailure": flow.last_review_failure,
        "findings": findings,
        "approvable": approvable.as_ref().is_ok(),
        "blocker": approvable.err().map(|e| e.to_string()),
        "approved": flow.approval_is_current(workspace, commitment_id),
        "approval": flow.approval,
    }))
}

pub fn resolve_finding(
    workspace: &Path,
    commitment_id: &str,
    expected_revision: i64,
    action_id: &str,
    displayed_review_digest: &str,
    index: usize,
    resolution: &str,
) -> Result<()> {
    ensure!(valid_action_id(action_id), "Invalid action ID");
    ensure!(
        !resolution.trim().is_empty() && resolution.len() <= 4096,
        "A concise resolution is required"
    );
    let request_digest = digest(&serde_json::to_vec(&(
        commitment_id,
        displayed_review_digest,
        index,
        resolution,
    ))?);
    let mut candidate = state::read(workspace)?.context("Workspace state is missing")?;
    let current_revision = candidate["revision"].as_i64();
    let commitment = candidate["commitments"]
        .as_array_mut()
        .context("Commitments missing")?
        .iter_mut()
        .find(|item| item["id"].as_str() == Some(commitment_id))
        .context("Unknown commitment")?;
    let mut workflow: Workflow = serde_json::from_value(commitment["workflow"].clone())
        .context("Workflow binding missing")?;
    if let Some(action) = workflow
        .resolution_actions
        .iter()
        .find(|action| action.id == action_id)
    {
        ensure!(
            action.request_digest == request_digest,
            "Action ID belongs to another resolution"
        );
        return Ok(());
    }
    ensure!(
        current_revision == Some(expected_revision),
        "Revision conflict: reread and reconcile"
    );
    ensure!(
        workflow.pending_review.is_none() && workflow.last_review_failure.is_none(),
        "Review needs reconciliation"
    );
    ensure!(
        digest(&serde_json::to_vec(&workflow.review)?) == displayed_review_digest,
        "Review changed since it was displayed"
    );
    let draft_digest = workflow.draft_digest(workspace)?;
    let review = workflow
        .review
        .as_mut()
        .context("Independent review is required")?;
    ensure!(
        review.draft_digest == draft_digest,
        "Draft changed since review"
    );
    let finding = review.findings.get_mut(index).context("Unknown finding")?;
    finding.resolution = Some(resolution.to_owned());
    workflow.approval = None;
    workflow.resolution_actions.push(ResolutionAction {
        id: action_id.to_owned(),
        request_digest,
    });
    commitment["workflow"] = serde_json::to_value(workflow)?;
    state::commit(
        workspace,
        &candidate,
        expected_revision,
        "Record design review finding resolution",
    )?;
    Ok(())
}

pub fn begin_review(
    workspace: &Path,
    commitment_id: &str,
    expected_revision: i64,
    action_id: &str,
) -> Result<ReviewStart> {
    ensure!(valid_action_id(action_id), "Invalid action ID");
    let mut candidate = state::read(workspace)?.context("Workspace state is missing")?;
    let current_revision = candidate["revision"].as_i64();
    let commitment = candidate["commitments"]
        .as_array_mut()
        .context("Commitments missing")?
        .iter_mut()
        .find(|item| item["id"].as_str() == Some(commitment_id))
        .context("Unknown commitment")?;
    let mut workflow: Workflow = serde_json::from_value(commitment["workflow"].clone())
        .context("Workflow binding missing")?;
    let draft_digest = workflow.draft_digest(workspace)?;
    if let Some(pending) = &workflow.pending_review {
        ensure!(
            pending.action_id == action_id && pending.draft_digest == draft_digest,
            "Another review needs reconciliation"
        );
        return Ok(ReviewStart::AlreadyPending);
    }
    if workflow.latest_review_action_id.as_deref() == Some(action_id) {
        ensure!(
            workflow
                .review
                .as_ref()
                .is_some_and(|review| review.draft_digest == draft_digest),
            "Action ID belongs to an earlier draft"
        );
        return Ok(ReviewStart::AlreadyCompleted);
    }
    ensure!(
        !workflow.review_action_ids.iter().any(|id| id == action_id),
        "Action ID was already used for an earlier review"
    );
    ensure!(
        current_revision == Some(expected_revision),
        "Revision conflict: reread and reconcile"
    );
    workflow.pending_review = Some(PendingReview {
        action_id: action_id.to_owned(),
        draft_digest,
        started_at: state::now(),
    });
    workflow.review_action_ids.push(action_id.to_owned());
    commitment["workflow"] = serde_json::to_value(workflow)?;
    state::commit(
        workspace,
        &candidate,
        expected_revision,
        "Start independent design review",
    )?;
    Ok(ReviewStart::Started)
}

/// Caller must obtain the receipt from an isolated, trusted reviewer.
pub fn record_review(
    workspace: &Path,
    commitment_id: &str,
    expected_revision: i64,
    action_id: &str,
    review: Review,
) -> Result<()> {
    ensure!(valid_action_id(action_id), "Invalid action ID");
    let mut candidate = state::read(workspace)?.context("Workspace state is missing")?;
    let current_revision = candidate["revision"].as_i64();
    let commitment = candidate["commitments"]
        .as_array_mut()
        .context("Commitments missing")?
        .iter_mut()
        .find(|item| item["id"].as_str() == Some(commitment_id))
        .context("Unknown commitment")?;
    let mut workflow: Workflow = serde_json::from_value(commitment["workflow"].clone())
        .context("Workflow binding missing")?;
    ensure!(
        workflow.draft_digest(workspace)? == review.draft_digest,
        "Draft changed since review"
    );
    if workflow.latest_review_action_id.as_deref() == Some(action_id) {
        ensure!(
            workflow.review.as_ref() == Some(&review),
            "Action ID belongs to a different review"
        );
        return Ok(());
    }
    ensure!(
        workflow
            .pending_review
            .as_ref()
            .is_some_and(|pending| pending.action_id == action_id
                && pending.draft_digest == review.draft_digest),
        "Review intent is missing or changed"
    );
    ensure!(
        current_revision == Some(expected_revision),
        "Revision conflict: reread and reconcile"
    );
    ensure!(
        !review.reviewer.trim().is_empty()
            && !review.input_boundary.trim().is_empty()
            && review.phases.iter().map(String::as_str).eq(PHASES),
        "Review receipt is incomplete"
    );
    ensure!(
        review
            .findings
            .iter()
            .all(|finding| !finding.description.trim().is_empty()
                && !finding.evidence.trim().is_empty()
                && finding.resolution.is_none()),
        "Review findings must contain evidence and start unresolved"
    );
    workflow.review = Some(review);
    workflow.approval = None;
    workflow.latest_review_action_id = Some(action_id.to_owned());
    workflow.pending_review = None;
    workflow.last_review_failure = None;
    commitment["workflow"] = serde_json::to_value(workflow)?;
    state::commit(
        workspace,
        &candidate,
        expected_revision,
        "Record independent design review",
    )?;
    Ok(())
}

impl Workflow {
    pub fn review_digest(&self) -> Result<String> {
        ensure!(self.review.is_some(), "Independent review is required");
        Ok(digest(&serde_json::to_vec(&self.review)?))
    }

    pub fn draft_digest(&self, workspace: &Path) -> Result<String> {
        Ok(digest(&self.draft_bytes(workspace)?))
    }

    pub fn draft_bytes(&self, workspace: &Path) -> Result<Vec<u8>> {
        ensure!(
            !self.binding.owner.trim().is_empty(),
            "Implementation owner is required"
        );
        let actual_workspace = workspace.canonicalize()?;
        ensure!(
            self.binding.workspace.canonicalize()? == actual_workspace,
            "Workspace binding mismatch"
        );
        let repository = self.binding.repository.canonicalize()?;
        let mut git = std::process::Command::new("git");
        git.arg("-C")
            .arg(&repository)
            .args(["rev-parse", "--show-toplevel"]);
        let result = crate::proc::run(&mut git, std::time::Duration::from_secs(5))?;
        ensure!(
            result.status.success(),
            "Bound repository is not a Git checkout"
        );
        let root = Path::new(result.stdout.trim()).canonicalize()?;
        ensure!(
            root == repository,
            "Repository binding must name the Git checkout root"
        );
        let draft = self.binding.draft.canonicalize()?;
        ensure!(
            draft.starts_with(&repository) && draft.is_file(),
            "Draft must be inside the bound repository"
        );
        let mut bytes = Vec::new();
        std::fs::File::open(draft)?
            .take(1024 * 1024 + 1)
            .read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() <= 1024 * 1024,
            "Draft exceeds the review input limit"
        );
        Ok(bytes)
    }

    /// Checks receipt completeness; caller must verify reviewer authenticity.
    pub fn can_approve(&self, workspace: &Path) -> Result<String> {
        ensure!(
            self.pending_review.is_none(),
            "A review is in progress or needs reconciliation"
        );
        ensure!(
            self.last_review_failure.is_none(),
            "The last review failed; a new review is required"
        );
        let draft_digest = self.draft_digest(workspace)?;
        let review = self
            .review
            .as_ref()
            .context("Independent review is required")?;
        ensure!(
            review.draft_digest == draft_digest,
            "Draft changed since review"
        );
        ensure!(
            !review.reviewer.trim().is_empty() && !review.input_boundary.trim().is_empty(),
            "Reviewer identity and input boundary are required"
        );
        let enrolled = self
            .reviewer_binding
            .as_ref()
            .context("No reviewer is enrolled")?;
        ensure!(
            review.reviewer == format!("sha256:{}", enrolled.sha256),
            "Review does not match the enrolled reviewer"
        );
        ensure!(
            crate::reviewer::program_digest(&enrolled.program)? == enrolled.sha256,
            "Enrolled reviewer changed"
        );
        ensure!(
            review.phases.iter().map(String::as_str).eq(PHASES),
            "All review phases are required"
        );
        ensure!(
            review
                .findings
                .iter()
                .all(|finding| !finding.description.trim().is_empty()
                    && !finding.evidence.trim().is_empty()
                    && finding
                        .resolution
                        .as_ref()
                        .is_some_and(|r| !r.trim().is_empty())),
            "Each finding needs evidence and a resolution"
        );
        Ok(draft_digest)
    }

    pub fn approval_is_current(&self, workspace: &Path, commitment_id: &str) -> bool {
        let Some(approval) = &self.approval else {
            return false;
        };
        let Ok(draft_digest) = self.can_approve(workspace) else {
            return false;
        };
        let Ok(review) = serde_json::to_vec(&self.review) else {
            return false;
        };
        let Ok(binding) = serde_json::to_vec(&self.binding) else {
            return false;
        };
        approval.commitment_id == commitment_id
            && approval.binding_digest == digest(&binding)
            && approval.draft_digest == draft_digest
            && approval.review_digest == digest(&review)
    }
}

/// Caller must authenticate the founder and verify reviewer isolation before use.
pub fn approve(
    workspace: &Path,
    commitment_id: &str,
    expected_revision: i64,
    action_id: &str,
) -> Result<()> {
    ensure!(valid_action_id(action_id), "Invalid action ID");
    let mut candidate = state::read(workspace)?.context("Workspace state is missing")?;
    let current_revision = candidate["revision"].as_i64();
    let commitment = candidate["commitments"]
        .as_array_mut()
        .context("Commitments missing")?
        .iter_mut()
        .find(|item| item["id"].as_str() == Some(commitment_id))
        .context("Unknown commitment")?;
    let mut workflow: Workflow = serde_json::from_value(commitment["workflow"].clone())
        .context("Workflow binding missing")?;
    let draft_digest = workflow.can_approve(workspace)?;
    if let Some(approval) = &workflow.approval {
        if approval.action_id == action_id {
            ensure!(
                workflow.approval_is_current(workspace, commitment_id),
                "Action ID belongs to an earlier review"
            );
            return Ok(());
        }
        ensure!(
            !workflow.approval_is_current(workspace, commitment_id),
            "This design is already approved"
        );
    }
    ensure!(
        !workflow
            .approval_action_ids
            .iter()
            .any(|id| id == action_id),
        "Action ID was already used for an earlier approval"
    );
    ensure!(
        current_revision == Some(expected_revision),
        "Revision conflict: reread and reconcile"
    );
    let review_digest = digest(&serde_json::to_vec(&workflow.review)?);
    workflow.approval = Some(Approval {
        commitment_id: commitment_id.to_owned(),
        binding_digest: digest(&serde_json::to_vec(&workflow.binding)?),
        draft_digest,
        review_digest,
        approved_at: state::now(),
        action_id: action_id.to_owned(),
    });
    workflow.approval_action_ids.push(action_id.to_owned());
    commitment["workflow"] = serde_json::to_value(workflow)?;
    state::commit(
        workspace,
        &candidate,
        expected_revision,
        "Approve reviewed design revision",
    )?;
    Ok(())
}
