# Private state and source recovery

Runtime files belong in the user's confirmed private workspace, not the plugin or shared package. v0.1 is designed for one owner and one coordinating writer. The helper has no network calls and is not an access-control or encryption system.

## State helper

Python 3.10+ is required. Resolve `scripts/state.py` relative to this skill.

```text
python3 SKILL_DIR/scripts/state.py init --workspace ABSOLUTE_WORKSPACE
python3 SKILL_DIR/scripts/state.py read --workspace ABSOLUTE_WORKSPACE
python3 SKILL_DIR/scripts/state.py check --file CANDIDATE_JSON
python3 SKILL_DIR/scripts/state.py commit --workspace ABSOLUTE_WORKSPACE --candidate CANDIDATE_JSON --expected-revision N --reason "Short source-grounded change"
```

In a `cos` checkout, `cos state init`, `cos state read`, `cos state check --file CANDIDATE_JSON` and `cos state commit --candidate CANDIDATE_JSON --expected-revision N --reason "..."` do the same against `<cos home>/.chief-of-staff/`, with the same lock and the same refusals. `cos state propose` writes the latest review's proposed deliverables into `.chief-of-staff/candidate.json` for the user to read before committing. `cos brief` and `cos review` commit only a run receipt and source health, record review coverage as `partial` at most, never advance a watermark, and do so only when `localTracking.approved` is true.

Use the host's file-editing tools to prepare candidate JSON in `.chief-of-staff/`, preserving the existing state. Do not put candidates in a shared folder. Read the current revision, make only the requested/evidenced change, validate, then commit. The helper uses an exclusive write lock, revision check, atomic replacement, and an append-only local change journal. It refuses overwrite on initialization, deletion of existing tracked records, unsupported schema versions, and reopening completed work. History should be archived explicitly in a later migration rather than bulk-deleted.

The journal is stored in the same atomic state snapshot. A completion requires a reference and verification date. That validates the record's shape, not the truth of its evidence. The agent must actually inspect evidence. Acknowledgment is a separate `reviewedAt` field and must not change status. Use `meetingAt` for an event and `dueDate` for a real follow-up deadline.

If a write lock remains after an interrupted process, stop. Confirm no writer is active and preserve a backup before an authorized lock cleanup; never use time alone to steal it. A revision conflict requires rereading and reconciling, not forced overwrite. Direct file edits can bypass the helper, so all coordinating chats must use it.

## Minimum records

- `goals`: id, title, owner, finishLine, targetDate, status; optional alignment and assumption notes.
- `kpis`: id, goalId, name, unit, baseline, target, current, evidenceAt, source, reportingWindow. Unknown readings stay null.
- `deliverables`: id, goalId, title, owner, ownerBasis, dueDate, dueBasis, week, blocker, dependencies, doneWhen, status, source, evidenceAt, reviewedAt, meetingAt, completionEvidence.
- `commitments`: id, title, owner, status, source, evidenceAt, nextReview, dueDate, meetingAt, reviewedAt, completionEvidence; optional goalId, estimate and dependency notes.
- `sources`: id, health, review, checkedAt, evidenceAt, lastCompleteThrough, windowStart, windowEnd, note. Connection health and review completeness are separate.
- `runs`: id, invokedAt, finishedAt, mode, status, coverage, notificationDecision. Reuse the original invocation identity on recovery.
- `incidents`: id, sourceId, kind, observedAt, retryNotBefore, approvalRef, recoveryClaimedAt, recoveryOutcome, resolvedAt, resolutionRef. Preserve original failure identity and history.
- `journal`: helper-owned entries; do not edit prior entries.

Saved source timestamps may advance only when justified by the actual source operation. Preserve old values when a source fails. Never label an old reporting window current or clear a coverage gap just because authentication works again.

## Read budgets and failures

Choose a conservative per-source budget during setup; it is an operating limit, not a statement about the provider's quota. Share results across participating workflows only within approved scope and freshness. Stop when the budget is consumed and mark remaining coverage partial. Do not execute overlapping searches to evade limits.

On a confirmed rate limit, record the source operation, time, and provider retry/reset information if given. Preserve the last complete watermark. A new run is not retry authority. Automatic recovery is **off** in this starter.

If the user later requests automatic recovery, treat that as a separate implementation task: record a source-specific approval, honor a provider reset or approved conservative cooldown, atomically reserve exactly one read-only probe per incident before the call, persist the result, and test restart/concurrent-call behavior. Do not claim the v0.1 state helper enforces provider-call serialization. Until that extension is implemented and verified, ask for a single scoped recovery check instead of automatically retrying.

Permission denials and authentication failures always stop that access path. A rate-limit approval does not cover them. A successful probe changes connection evidence only; it does not finish the review or authorize replay of failed writes. Reconnect through supported flows with the correct account, never by switching surfaces to bypass a block.

## Backup and retention

Keep private backups outside the release folder according to the user's policy. Minimize stored content to decisions, metadata, source links and evidence references; don't save raw message bodies, attachments, tokens, or account dumps. Review long-lived journal size before adding many concurrent workflows. v0.1 does not provide automated retention, cross-device merging, or recovery of deleted files.
