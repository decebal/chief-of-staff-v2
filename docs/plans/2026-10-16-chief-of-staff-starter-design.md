# Codex-first Chief of Staff starter

## Decision

Create a local, shareable kit with one mode-routed skill, a supported plugin manifest/local marketplace, guided setup, durable private tracking, fictional examples and behavioral checks. Supply an optional dashboard build brief, not a predeployed site. This separates reusable logic from individual accounts, strategy, runtime records, and permissions.

## Alternatives

A prompt-only pack has the lowest setup effort but leaves persistence and operational boundaries implicit. A production-dashboard clone offers visual parity but brings account-specific configuration, hosting, data separation, and ongoing maintenance. The starter kit supports a useful manual review first and staged optional integrations.

## Components and ownership

The shared package maintainer owns versioned skill instructions, fixtures and tests. Each recipient owns their private profile, source connections, runtime state and schedule. The native Codex host owns connector authentication and automation execution. No MCP servers, credentials, network clients, lifecycle hooks, telemetry, remote repository, or schedule are included.

Read user-confirmed goals and local state, gather scoped source evidence, reconcile commitments, rank by impact/urgency, then return a short decision brief. Save only approved local tracking. The state helper provides validation, conflict detection and atomic journaled commits; it does not coordinate API calls or enforce permissions outside the helper.

## Maintenance and failure handling

Source systems remain authoritative for their own events and task status; local strategy/plan records capture confirmed user intent. Source failures preserve old evidence dates and coverage gaps. Authentication/permission errors stop the affected path. Source reconnects require the recipient's account. One owner/coordinating writer avoids unsupported distributed state merging. Backups are private and user-managed.

Update reusable package files without overwriting runtime data. Keep previous package versions for rollback. Use an explicit release allowlist plus per-file hashes; no recursive export of an active workspace. User edits require a privacy review before redistribution.

## Verification

Test state initialization and overwrite refusal, stale/concurrent writes, evidence requirements, completion retention, acknowledgment semantics, denial history, and malformed state rejection. Validate the skill and local references, scan release files for known personal identifiers/credential patterns, inspect the exact ZIP contents, and run a fresh unzip smoke test. Live plugin install, source connections, real-world ranking, scheduling, and hosted dashboard access remain recipient acceptance steps.

## Sources

Packaging follows [Build skills](https://developers.openai.com/codex/skills) and [Package your plugin](https://developers.openai.com/plugins/build/plugins), retrieved October 16, 2026. Operating rules are generalized from a working personal Chief of Staff; no original private data, account identity, deployment history, or permission grants are included.
