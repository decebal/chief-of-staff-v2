# Validation record

Version 0.2.0 · October 17, 2026

## Changed since 0.1.0

- A Claude Code plugin manifest (`plugins/chief-of-staff-starter/.claude-plugin/plugin.json`) and marketplace (`.claude-plugin/marketplace.json`) beside the Codex ones, pointing at the same skill directory.
- The skill and the state reference describe how the `cos` engine, when present, works on the same workspace. The kit's rules, references, helper and fixtures are otherwise unchanged.

## Verified locally

- The 21 state-helper tests from 0.1.0 pass unchanged (`python3 -m unittest test_state`), covering initialization, no-overwrite behavior, revision conflicts, concurrent writers, persistent locks, completion evidence, retained history, waiting-item acknowledgment, dependency cycles, KPI evidence, source coverage, denial history, malformed input and symlink refusal.
- `cos`'s Rust port of the helper passes the same 21 tests. Both validators were run over 25 candidate states, valid and invalid, and agreed on every one. Each read and committed state the other wrote.
- `python3 scripts/release.py` passes over the 25 allowlisted files.
- One independent fictional forward-test in 0.1.0 exercised a high-impact workshop prerequisite, unknown KPI progress, partial email coverage, an elapsed meeting without a follow-up deadline, and a draft-only handoff. It identified two presentation ambiguities: empty change sections and the word limit for separate handoffs. Both instructions were clarified. This is a small behavioral check, not broad live acceptance.

## Final packaging checks

Run `python3 scripts/release.py` from this package to verify the explicit file allowlist, internal links, safe starting permissions and common private-path/credential patterns. Use `--output ABSOLUTE_NEW_ZIP_PATH_OUTSIDE_THIS_FOLDER` to produce an archive with per-file SHA256 checksums and archive readback validation. Never redistribute a personalized runtime directory.

## Not claimed

Live plugin installation in a recipient's Codex or Claude Code client, connected account access, scheduled execution, automatic provider recovery, a hosted dashboard, and hosted owner-only access have not been tested by these local checks. v0.2 is a guided team pilot, not an unattended production service. Its helper protects local state commits, not simultaneous connector requests or permissions.
