---
applyTo: "**"
excludeAgent: "cloud-agent"
---

# Reviewing cos pull requests

Use the `prove-it-review` skill (`.github/skills/prove-it-review/SKILL.md`)
for every pull request in this repository.

- Review each claim in the pull request description against the invariants
  in that skill. Report a problem with the file, the line and the input on
  which it fails.
- Skip formatting, naming and lint findings. CI runs `cargo fmt --check` and
  `cargo clippy --all-targets -- -D warnings`.
- `.claude/skills/prove-it/SKILL.md` is the Claude Code version of the same
  review. It runs commands and posts its own review. Do not follow its run or
  post steps; its invariant table matches the skill above.
- Treat any change under `src/claude.rs`, `src/lib.rs` (`READ_ONLY`),
  `src/mcp.rs`, `src/proc.rs`, `src/agents/`, `src/state.rs`,
`src/track.rs`, `src/context.rs`, `.claude/agents/` or
`plugins/chief-of-staff-starter/` as
  safety-relevant. For each, say which invariant you checked and why it holds.
- A file added under `context/` or `out/`, or a screenshot that is not from
  `task demo`, is personal data. Flag it.
