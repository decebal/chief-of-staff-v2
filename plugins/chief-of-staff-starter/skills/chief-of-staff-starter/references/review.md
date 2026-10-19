# Evidence to priorities

## Scope and collection

Read profile and state. Establish the current time, timezone, actual source account, approved scope, and prior successful coverage. Keep a fixed upper bound for each scan so messages arriving mid-review do not create an endless moving target.

Use available native connectors first. Search/list identifiers and summaries before fetching full threads. Deduplicate candidates; expand only items that may change a decision, deadline, dependency, or completion. Follow pagination before claiming complete coverage. Avoid exact numeric quota claims: connector quotas and provider limits differ.

Use incremental windows since the last complete review with a modest overlap for delayed indexing, then periodically reconcile older open commitments and a bounded backlog. Record the actual window. A successful search, empty page, or recovery check does not prove the whole source was reviewed. Never move the watermark past an unreviewed gap.

Source-specific checks, only when enabled:

- **Email:** human asks, approvals, deadlines, and waiting threads. Read the relevant conversation before naming the next responder. Suppress bulk mail unless it changes an obligation.
- **Chat:** scoped DMs, mentions, relevant replies and project channels. Preserve permalinks. Activity coverage is not exact unread coverage.
- **Tasks:** active assigned/owned work, real due dates, blockers, estimates, and recent changes. Exclude evidenced completed work. Suggested ownership is not assignment.
- **Code:** relevant review requests, customer-blocking failures, security and release risks. Keep draft, implemented, reviewed, deployed, and accepted distinct.
- **Calendar:** current Monday-through-Sunday week using explicit local-midnight boundaries, plus tomorrow when it falls outside that week. Use relevant free/busy checks. Exclude elapsed time, respect personal commitments, and don't label unknown time free. Date-only commitments are not midnight deadlines. Do not book or move anything.

If an account is unavailable, continue independent sources and record the coverage gap. Stop affected access paths on permission/authentication failures; no fallback browser or alternate account to evade a denial.

## Reconcile and rank

For every retained item, establish the next action, responsible person, direct source, evidence date, actual deadline if any, estimate if useful, blocker, and next review trigger. Keep event time separate from follow-up due date. No reply does not prove no action was taken elsewhere.

Rank essential obligations, material customer/revenue/delivery risks, and consequential decisions alongside confirmed strategic outcomes. A necessary preparation/review task inherits the outcome's impact and shrinking review window. Effort changes capacity, not importance. Don't label all work for a favored project strategically important without naming the contribution.

Choose at most three actions. Batch routine follow-ups without discarding their individual source links and review triggers. If capacity does not support the plan, recommend the smallest useful trade-off or deferral rather than inventing a protected work block.

## Brief

Use **DECIDE**, **DO**, **DELEGATE: owner**, or **BATCH**, followed by a recommended action, business consequence, source link, and real deadline or clearly proposed timing. Delegation is a recommendation unless a verified handoff actually happened.

What changed uses **DONE**, **UNBLOCKED**, **RISK**, or **TIME CHANGED**, with linked evidence. An item needing the user's action belongs only in Needs you. Mark exactly what finished: draft-ready does not mean sent, and deployed does not mean customer-accepted.

Omit What changed when no verified consequential delta is established. Do not fill it with “no change” statements or unchanged source limitations. Mention coverage elsewhere only when it materially qualifies a recommendation. A separately requested handoff can follow the brief and is not subject to the brief's word limit.

Notify on material decisions, risk, timing changes, or verified completion/blocker removal that changes what happens next. New messages, routine checks, refresh success, and the clock alone do not warrant notifications. Keep quiet-run detail in local state when authorized. A missing source deserves interruption only when it materially changes confidence or the next action.
