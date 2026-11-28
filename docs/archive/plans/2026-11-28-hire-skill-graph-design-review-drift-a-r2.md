---
design: docs/plans/2026-11-28-hire-skill-graph-design.md
design-sha256: superseded
verdict: findings
---

## Round 1 findings

1. closed: E8 now lists `skills_hidden` and `skills_codex_config` (design:62); VERIFY-S covers both (design:73-75). Code: `mod.rs:254,257` fields, written `mod.rs:726,730`, asserted `tests/hiring.rs:910`. Residual: taint sink S7 (design:404-405) still names only `skills_bundled`. Carried below.
2. closed: §2 now says one Prime read in flight per home (design:221-225). Code: `skill_facts.rs:27-46` `IN_FLIGHT` keyed by home. Residual: Review history still says "one Prime call in flight per process" (design:609). Carried below.
3. open: `.agents/skills` missing from step 5 (design:258-262 names only `.claude/skills/`). Terms (design:155-157) and Built as item 5 name both; code `skills.rs:325-327` reads both. Step 5 is the text a builder follows.
4. closed: E10 names the fallback `$HOME/.codex/config.toml` (design:60). Code: `skill_bundle.rs:39-43`.
5. open: `$CODEX_HOME/config.toml` is named as a taint source at design:64 but not in the Sources list (design:397-399).
6. closed: an unreadable, unparseable or non-array `config.toml` fails the hire (VERIFY-C, design:69-72). Code: `skill_bundle.rs:44-54`, error via `prepare` `mod.rs:730-731`. Test `tests/hiring.rs:912-919` asserts the error names `config.toml` and no `run` reaches Herdr.
7. open: A2 row still tier `rock` (design:548). The name-invocation limit is noted there. Spike report `docs/reports/2026-11-28-codex-skill-switch-spike.md:44` also lists "Codex versions other than 0.172.0", which the design omits.
8. closed: Built as item 3 sizes are under 500 (`skill_facts.rs` 378, `skill_bundle.rs` 160, `skill_view.rs` 117, `skills.rs` 481). Built as claims checked: `harness::argv(spec, &Delivery)` (`harness.rs:33`), `Delivery` (`skill_bundle.rs:21-27`), `codex_config`, `hidden_codex` (`skill_bundle.rs:70`, `skills.rs:289`), native roots (`skills.rs:21`), `valid_name` before the TOML (`skill_bundle.rs:59`), test names `tests/skills.rs:381` and `tests/hiring.rs:856`, `COS_SKILLS_HOME` and `CODEX_HOME` pinned (`tests/fake_herdr/mod.rs:50-51`), `COS_PRIME_URL` space or control refusal (`prime.rs:43-45`), `prime_search` with no `nodes` list is a failed read (`skill_facts.rs:148-153`), repo skills not counted unreviewed (`skills.rs:441-443`), per-pid-and-counter temp names (`skill_facts.rs:81`).

## New findings

1. blocking Semantic ISA (Taint sources and sinks, VERIFY-A, E11): the Codex `-c skills.config=<TOML>` argv value is a sink with no entry in the sink list, and VERIFY-A does not cover it. Sinks are listed as S1 to S8 (design:401-405); S9 is defined only in Built as (design:66). VERIFY-A (design:439-441) admits argv only "through `serde_json` values and `Path` arguments". The code builds a TOML string and passes it as `format!("skills.config={c}")`, which is neither. The sources list (design:397-399) omits `$CODEX_HOME/config.toml`, which design:64 calls a taint source. A reader checking argv against VERIFY-A finds a violation the design blesses. Evidence: `harness.rs:55-56`, `skill_bundle.rs:65`, design:397-405, design:439-441. Fix: add S9 (the `-c skills.config` argv value, E11) to the sinks list; add `$CODEX_HOME/config.toml` to the sources list; extend VERIFY-A to name S9 as admitted only as the VERIFY-C `toml::Value` output, with hidden names through `valid_name`.

## Carried to Plan

- §4 (design:309) and Context (design:111) still give `harness::argv(spec, bundle)` and `harness::argv(&spec)`; Built as item 4 overrides them. Rewrite both to `argv(spec, &Delivery)`.
- Taint sink S7 (design:404-405) names only `skills_bundled`; add `skills_hidden` and `skills_codex_config`.
- Security and privacy "Stored" (design:496-497) does not list the user's `config.toml` `skills.config` entries copied into `state.json` (E8). Add.
- Test matrix (design:457-468) has no S9 row; S9 tests exist only in Built as prose. Add the row.
- A2 row (design:548): tier rock with a version limit unstated (spike:44). Add "0.172.0 only" or downgrade.
- Review history (design:609): replace "one Prime call in flight per process" with "per home".
- Context (design:131-133) and Prime counts are measured on 2026-11-28; the line refs (design:99-111, `record_run` at design:380) are stale against `mod.rs`. Fix at Plan.
- Step 5 wording (design:258-262) to name both project roots and say the bundle filter reads the seeded worktree (`skill_bundle.rs:105`, `skills.rs:325-327`).
