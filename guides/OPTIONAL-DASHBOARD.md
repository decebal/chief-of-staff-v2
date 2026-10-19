# Optional private dashboard: build brief

This is a specification, not a working dashboard or deployment approval. The Codex skill works without it. Build a local prototype first only when requested; hosting, source ingestion, and private uploads need separate scope and access approval.

## Experience

Mobile-first, light theme, compact editorial lists, readable type, generous tap targets. Restore the last-used tab. Avoid decorative status clutter and charts without a decision purpose.

| View | Primary job |
| --- | --- |
| Goals | Show the user's confirmed direction, outcome finish lines, how goals reinforce/conflict, and next checkpoint. |
| KPIs | Track goal progress with units, target, actual, reporting window, source and evidence date; null is unavailable, not zero. |
| Plan | Default to this week's execution: deliverable, linked goal, owner, actual due date, blockers, and done criteria. Reverse planning is secondary. |
| Priorities | Show the three highest-impact next actions, their consequences, and direct evidence. Retain waiting/backlog work without burying urgent prerequisites. |
| Sources | Separate connection health from coverage/freshness, show failed scheduled modules, next permitted recovery and responsible owner. |

Give deliverables and priorities a **Delegate this** control that prepares a handoff, never automatically executes it. **Mark reviewed** acknowledges without completing. Store meeting timestamps separately from follow-up deadlines. Keep past completed weeks available in history but out of the default active plan.

## Data boundaries

Keep manual goals/plan edits separate from synchronized source snapshots. A sync must not overwrite the user's local edits or dispositions. Prefer stable record IDs and optimistic version checks. Never combine operational summaries and aggregate metric payloads on the same ambiguous write path.

Local prototype: no telemetry, remote fonts, analytics, external scripts, or automatic network calls. Do not claim browser local storage is encrypted or backed up. Escape rendered data, validate external links, and avoid raw HTML from sources. Do not expose a development server outside loopback.

Hosted version: choose the actual hosting service with the user, create a **new** owner-only destination, verify access before data upload and after deployment, and fail closed on access drift. A hidden URL is not access control. Keep credentials server-side; use supported protected service access and never place tokens in the browser bundle.

Upload only approved minimized fields. Use actual evidence dates, not sync time. Validate schema and destination before each write, read before write, and read back run ID/digest after it. Preserve last verified data on failure and label it stale. An acknowledged item is not approved or complete.

## Acceptance checks

- Goals link to KPIs and Plan; no fabricated target or deadline appears.
- This week's actionable work is readable at 320px without horizontal scrolling.
- Keyboard and screen-reader controls have labels and visible focus.
- Reviewed waiting work stays waiting; past meetings do not become overdue follow-ups.
- Delegate this includes goal, owner, due date/basis, blockers, sources and done criteria.
- Failed refresh retains old values/evidence dates and honest coverage.
- Concurrent saves conflict safely; restored tabs and empty/error states behave predictably.
- Hosted data and APIs deny unauthenticated access; access stays owner-only.

Do not clone another person's production site, hosting identity, database, approval ledger, or account configuration.
