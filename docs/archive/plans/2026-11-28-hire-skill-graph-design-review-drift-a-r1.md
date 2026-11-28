---
design: docs/plans/2026-11-28-hire-skill-graph-design.md
design-sha256: superseded
verdict: findings
---

## Findings
1. blocking ISA (Side-effect inventory E8, VERIFY-S, Taint sinks S7): `state.json` gains a second new field, `skills_hidden`, which the design never inventories. Evidence: `src/agents/mod.rs:253` (`pub skills_hidden: Vec<String>`), written at `mod.rs:724` and `mod.rs:727` and persisted via `write_state` (`mod.rs:740`); read back at `mod.rs:645` and `mod.rs:647` to build argv (S4). E8 says only "gains `skills_bundled`", and VERIFY-S names only `skills` and `skills_bundled`. Fix: extend E8 to `skills_bundled` and `skills_hidden`, state `skills_hidden` is a subset of the set's names (VERIFY-R), and add it to VERIFY-S and the S7 test row (`tests/hiring.rs:906` already asserts it, so the test exists).

## Carried to Plan
- §4 Delivery, Codex paragraph is stale: it says "Nothing is bundled or hidden until the spike" and that the plan warns "Codex still sees every global skill". Code hides Codex skills (`skill_bundle.rs:58`, `skills.rs:457-463`) and the warning string is absent from `src`. Built as item 1 flags the change, but the paragraph should be rewritten, not left to contradict it.
- §2 Read: "process-wide `Mutex`" and "One call in flight per process" (also Carried: concurrency wording). Code is per home: `skill_facts.rs:27` `IN_FLIGHT: Mutex<BTreeSet<PathBuf>>`. Align the text with Built as item 2.
- Terms and §2 step 5 name only the repo's `.claude/skills/`. Code also reads `.agents/skills/` (`skills.rs:325-327`, `project_skills`). Built as item 5 flags it; update the Terms and step 5 text so a builder does not drop `.agents/skills`.
- E10 row says `$CODEX_HOME/config.toml`. Code falls back to `$HOME/.codex/config.toml` when `CODEX_HOME` is unset or empty (`skill_bundle.rs:37-41`). Add the fallback to the row.
- Taint sources omit `$CODEX_HOME/config.toml` (E10). Its `skills.config` entries flow into argv (S9). The file is the user's own, so the risk is low, but name it as a source and keep the VERIFY-C "parsed TOML value" rule as the control.
- `skill_bundle.rs:42-46`: an unparseable or non-array `config.toml` yields an empty carry-over, so `-c skills.config=` replaces the user's array with the hidden names only. Codex fails on an unparseable file anyway; state the behaviour in the Codex row or fail the hire instead.
- Confidence A2 row says "rock" and points at the spike. The spike (`docs/reports/2026-11-28-codex-skill-switch-spike.md`, "Not checked") did not test whether a disabled skill is still invocable by `$name`, nor any Codex version other than 0.172.0. Downgrade or note the limit in the row.
- Built as item 3 module sizes verified: `skills.rs` 481, `skill_facts.rs` 368, `skill_bundle.rs` 148, `skill_view.rs` 117 lines, all under 500.
- Built as claims verified true: `harness::argv(spec, &Delivery)` (`harness.rs:33`); `Delivery { bundle, hidden, codex_config }` (`skill_bundle.rs:21-27`); `codex_config` (`skill_bundle.rs:33`); `hidden_codex` (`skill_bundle.rs:58`, `skills.rs:289`); native roots `agents`, `codex` (`skills.rs:21`); `valid_name` before the TOML (`skill_bundle.rs:47`); `COS_SKILLS_HOME`, `CODEX_HOME`, `COS_PRIME_URL` pinned (`tests/fake_herdr/mod.rs:50-52`); test names `tests/skills.rs:381` and `tests/hiring.rs:856` exist and assert the S9 rows; `toml` is a `[dependencies]` entry (`Cargo.toml:36`). Not run: no build or test execution, per instructions.
