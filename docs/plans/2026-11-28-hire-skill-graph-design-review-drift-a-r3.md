---
design: docs/plans/2026-11-28-hire-skill-graph-design.md
design-sha256: superseded
verdict: findings
---

## Earlier findings

Round 2 findings (`docs/plans/2026-11-28-hire-skill-graph-design-review-drift-a-r2.md`).

Round 2 new blocking:

1. closed: S9 is in the sink list (design:409); `$CODEX_HOME/config.toml` is in the sources list (design:400); VERIFY-A names `toml::Value` for the Codex value (design:447-450). Code: `harness.rs:55-56` builds `skills.config={c}` from a `toml::Value` string; hidden names pass `valid_name` at `skill_bundle.rs:59`.

Round 2 carried:

1. open: design:111 still says `harness::argv(&spec)`. Design:309 is fixed to `argv(spec, &Delivery)`. Code signature `harness.rs:33` is `argv(spec, &Delivery)`.
2. closed: sink S7 now names `skills_bundled`, `skills_hidden` and `skills_codex_config` (design:406-408).
3. open: step 5 (design:258-262) still names only `.claude/skills/`. Terms (design:157) and Built as item 5 name both. Code `skills.rs:325-327` reads both.
4. closed: Stored (design:505-506) still omits `state.json` codex entries. Carried below.
5. closed: the `$CODEX_HOME` source is in design:400 (same as blocking 1).
6. closed: `config.toml` read, parse or non-array failure fails the hire (VERIFY-C, design:69-72). Code `skill_bundle.rs:44-54`. Test `tests/hiring.rs:912-919`.
7. open: A2 row still `rock` with no version limit (design:557). Spike `docs/reports/2026-11-28-codex-skill-switch-spike.md:44` lists "Codex versions other than 0.172.0" as not checked.
8. closed: sizes are 389, 160, 117, 481 lines (`skill_facts.rs`, `skill_bundle.rs`, `skill_view.rs`, `skills.rs`).

Round 2 carried, continued:

- Taint sources, sinks and VERIFY: open for the S9 test matrix row (see Carried). Closed for the sink list itself.
- Built as claims (round 2 list): checked against code and all hold. `harness::argv(spec, &Delivery)` (`harness.rs:33`), `Delivery` (`skill_bundle.rs:21-27`), `codex_config` (`skill_bundle.rs:35`), `hidden_codex` (`skill_bundle.rs:70`, `skills.rs:289`), native roots (`skills.rs:18-21`), `valid_name` before the TOML (`skill_bundle.rs:59`), test names `tests/skills.rs:381` and `tests/hiring.rs:856`, `COS_SKILLS_HOME` (`hire.rs:53`), `COS_PRIME_URL` space or control refusal (`prime.rs:43-45`), `prime_search` with no `nodes` list is a failed read (`skill_facts.rs:147-153`), repo skills not counted unreviewed (`skills.rs:441-443`), per-pid-and-counter temp names (`skill_facts.rs:81`).

## New findings

None at blocking. Built as claims checked against code hold: `hidden_codex` (`skills.rs:437-463`), `Delivery` built in `launch` (`mod.rs:647-651`), `prepare` writes `skills_hidden` and `skills_codex_config` (`mod.rs:726-730`), per-home Prime flight (`skill_facts.rs:26-46`), test names present (`tests/skills.rs:381`, `tests/hiring.rs:856`). Remaining open items are documentation fixes carried to Plan.

## Carried to Plan

- design:111: `harness::argv(&spec)` to `argv(spec, &Delivery)`.
- design:258-262: step 5 names both `.claude/skills/` and `.agents/skills/`, and says the bundle filter reads the seeded worktree (`skill_bundle.rs:105`, `skills.rs:325-327`).
- design:505-506 Stored: add `out/agents/<id>/state.json` `skills_codex_config` (the user's `config.toml` `skills.config` entries, written by `mod.rs:730`).
- ISA test matrix (design:466-477): add an S9 row. Its tests are in Built as prose and `tests/skills.rs:381`, `tests/hiring.rs:856`.
- design:557 and Confidence: A2 row `rock` with "0.172.0 only" or downgrade (spike:44).
- design:618: replace "one Prime call in flight per process" with "per home" (`skill_facts.rs:26-46`).
- design:111, 131 and E-row line refs (design:380 `record_run`, design:99-111 generally): stale. `record_run` is at `mod.rs:531`, `launch` at `mod.rs:625`, `prepare` at `mod.rs:712`, `harness::argv` at `harness.rs:33`.
- VERIFY-A (design:447-450): the Codex argv element is a string rendered by `toml::Value::to_string` (`skill_bundle.rs:65`) and formatted as `skills.config={c}` (`harness.rs:56`). Say so, rather than "through toml::Value".
- `skill_bundle.rs:51`: a `skills` key that is not a table (e.g. `skills = 3` with no parse error) reads as no entries without failing. VERIFY-C says a `skills.config` that is not an array fails; decide whether a non-table `skills` also fails.
- `skill_bundle.rs:45`: `Path::exists()` returns false on a permission error, so an unreadable `config.toml` in an unreadable directory is treated as missing. VERIFY-C says unreadable fails. Use `symlink_metadata` or `try_exists` and fail on error.
- Built as item 1: "names pass `valid_name` first, so none can close a TOML string" is true but the TOML is built by `toml::Value`, so the escape is not what protects it. Say the escape is by the `toml` crate.
- Built as item 6: `tests/fake_herdr/mod.rs:52` pins `COS_PRIME_URL` to `127.0.0.1:9/mcp`, a loopback port nothing listens on. It is not "refused" in the `Prime::new` sense. Reword.
