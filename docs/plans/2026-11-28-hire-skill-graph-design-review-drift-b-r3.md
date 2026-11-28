---
design: docs/plans/2026-11-28-hire-skill-graph-design.md
design-sha256: superseded
verdict: findings
---

## Earlier findings

Round-2 review: `docs/plans/2026-11-28-hire-skill-graph-design-review-drift-b-r2.md`.

1. closed: codex config parse, read and non-array failures fail the hire before Herdr. `codex_config` returns `Result` (`src/agents/skill_bundle.rs:35-66`); `prepare` propagates (`src/agents/mod.rs:730-731`); `launch` bails before `harness::argv` and `herdr::run` (`mod.rs:642-646`). Missing-file gap is new, see New finding 2. Test (`tests/hiring.rs:912-919`) not re-read this round.
2. closed for `skills_hidden` (`mod.rs:726,729,744`). E11 retention: open.
- carried a (`tests/skills_cli.rs` pins no `CODEX_HOME`): open. `cos()` sets `COS_SKILLS_HOME` and `COS_PRIME_URL` (lines 58-59, 195-196, 222-223), no `CODEX_HOME`. `tests/fake_herdr/mod.rs:51` pins it for hire tests.
- carried b (`tests/fake_herdr/mod.rs:52` pins `http://127.0.0.1:9/mcp`, design Built-as item 6 says "a refused `COS_PRIME_URL`"): open. The URL passes VERIFY-U (loopback), so the wording still conflates "refused by VERIFY-U" with "connection refused by the kernel".
- carried c (per process wording): closed. Design uses "one call in flight per home" (line 222-225).
- carried d (weak assertion in `tests/hiring.rs`): closed per r2; not re-read this round.
- carried e (Codex config read time): closed for the read time. Design E10 row now says `from prepare` (line 60), and VERIFY-C says "built in `prepare`". Open for the sentence that a config edited between plan and launch is picked up at launch.
- Carried to Plan, Stored list (`$CODEX_HOME` entries in `state.json`): open. Design line 505-506 still lists only `spec.json` and the bundle; `state.json` holds `skills_codex_config` (`mod.rs:730`).
- Carried to Plan, `prepare` error path persists `skills_hidden` without `skills_codex_config` (`mod.rs:644`): open, harmless, not stated.

## New findings

1. blocking §Built as item 4 / VERIFY-B (S3): the bundle step does not re-derive the source path from the catalog, despite the claim "the source path comes only from the catalog entry ... found by listing that root". The path is read back from `spec.json`: `launch` reads it (`src/agents/mod.rs:627`), `prepare` passes `spec.skills` (`mod.rs:724`), and `bundle` copies `p.path` after checking only `valid_name`, `SKILL.md` and the digest (`src/agents/skill_bundle.rs:127-137,148`). Nothing checks that the path is a direct child of a configured root, and `codex_block` (`src/agents/skills.rs:309`) prints the same path into the Codex prompt (S5). A writer of `out/agents/<id>/spec.json` outside the sandbox can point `cp -Rc` at any directory with a `SKILL.md` and a matching digest, and Claude then loads it through `--plugin-dir`. Fix: in `bundle` (and in `prepare` before `codex_block` is used), re-run `skills::catalog(roots)` and require that the picked name's entry with the same root label has canonical path equal to `p.path`; otherwise bail naming the skill. Add a test row to S3: a `spec.json` whose picked path points at a non-root directory with a correct digest is refused before any copy. Evidence for the claim is cited above.

2. blocking §Built as item 1 / VERIFY-C (S9): the missing-file rule uses `Path::exists()` (`src/agents/skill_bundle.rs:45`). `exists()` returns `false` when `stat` fails (for example `EACCES` on `$CODEX_HOME` or `$HOME/.codex`), so an unreadable config is treated as missing and the user's own `skills.config` entries are dropped. VERIFY-C says running on in that case "would re-enable skills the user switched off". Fix: use `std::fs::try_exists(&p)` and propagate `Err` with the path named; `Ok(false)` is the only "no entries of the user's" case. Add a test: parent directory mode 000, the hire fails before `hc run` (the existing `tests/hiring.rs:912-919` pattern).

3. blocking §Decision and §2 step 4 / Acceptance 3: `--no-skill` does not take a relevant global skill out of the session. `resolve` marks a name hidden only if it is `unwanted` (irrelevant, or unreviewed with `unreviewed: off`) or bundled (`src/agents/skills.rs:448-456`). A removed name with a `relevant` fact is not picked, not unwanted, so it is not hidden and stays loaded from `~/.claude/skills`; the Codex list does the same (`skills.rs:457-463`). The Decision paragraph says a hire "runs with a resolved skill set ... minus the ones it removed", and acceptance 3 says `--no-skill tdd` "keeps `tdd` out". `tests/skills.rs:205-237` asserts only that `tdd` is absent from `picked`; no test checks `hidden`. Fix: in `resolve`, add names in `asked.remove` that have a `claude`-root entry to `hidden` (and a `agents`/`codex` entry to `hidden_codex`), unless the repo holds the name (project filter stays). Alternatively, change the Decision and acceptance 3 to say removal is per-set only, and name that the session still loads the global copy. Add the test row to the S4 matrix.

## Carried to Plan

- E11 (Herdr argv retention): the cos side passes argv after `--` (`src/herdr.rs:216-226`); the `hc` side was not read. VERIFY-A's "no shell parses it" depends on `hc run` not joining argv. Verify, or change to "may keep it" and say it was not checked.
- `read_graph` freshness compares RFC 3339 strings lexically (`src/agents/skill_facts.rs:129-130`, `failed_at <= read_at`). jiff's `Display` varies fractional-second precision, so the comparison is not a time order. Compare parsed `Timestamp`s.
- `read_graph` error branch (`skill_facts.rs:173-177`) writes back the `cache` loaded before the Prime call. A concurrent writer's newer graph can be replaced by the older one. Cache-only; re-read is possible. Re-load inside the error branch or accept and say so.
- Project set is taken at plan from `repo` (`src/agents/hire.rs:90`), and at bundle from the seeded worktree (`skill_bundle.rs:105`). A repo skill that exists only in the checkout (untracked or uncommitted) is planned as `project`, so it is neither bundled nor hidden, and the worktree does not have it: the session sees no copy and the plan says "the repo's copy is used". Either check `project` against the worktree at plan (seed first) or name the case in the warning.
- A5 (reply shape of `prime_add_node`): `node_ref` (`skill_facts.rs:382-389`) tries `entity_id`, `id`, `node_id`. Not checked against the live server; the design says "read at Build once". Keep the Confidence row as sand until read.
- `fold` tie-breaking (`skill_facts.rs:233-238`): a later node wins a `curated_at` tie. Design says "ties by node order"; say which way.
- Design Built-as item 6 wording ("a refused `COS_PRIME_URL`") and `tests/fake_herdr/mod.rs:52`: reword to "a loopback port with no listener (connection refused)", or use a URL VERIFY-U rejects.
- Codex `-c skills.config=…` is one argv element, built with `toml::Value::to_string()` (`harness.rs:56`). Add a test that the value parses back as TOML with the user's entries intact (`tests/hiring.rs` row exists; confirm it parses, not only contains the names).
- `src/agents/mod.rs` is 1,354 lines before this work; the new code is in the three new modules, each under 500 lines (`skills.rs` 481, `skill_facts.rs` 389, `skill_bundle.rs` 160). Not a finding; recorded for the layer size rule.
