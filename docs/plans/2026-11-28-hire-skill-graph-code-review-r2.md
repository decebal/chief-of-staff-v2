---
range: 2e295db..706a955
prove-it: 🔴 DON'T MERGE
verdict: findings
---

## Round 1 findings

1. closed: `prime_search` reply without a `nodes` list is a failed read. Code: `src/agents/skill_facts.rs:145-153` (guard), `:173-186` (`failed_at` path). Test: `tests/prime.rs:440-467` `a_reply_without_a_nodes_list_keeps_the_last_graph`.
2. closed: `state.json` `skills_hidden` in the inventory. Design `docs/plans/2026-11-28-hire-skill-graph-design.md` `## Built as` E8 (extended) row, lines 61-62. Field at `src/agents/mod.rs:253-254`.
3. open: no test for S5 control-character stripping or the 300-char cap. `tests/hiring.rs:626-645` asserts only that `Skills for this work` is present. No `\x1b` or 2,000-char case found in the test files read (`tests/hiring.rs`, `tests/prime.rs`). `tests/skills.rs` not re-read this round. Code is correct: `src/agents/skills.rs:305-320`. Fix: add the test r1 asked for.
4. closed: `COS_PRIME_URL` with space or control byte refused before any socket. Code: `src/prime.rs:43-45` (`rest.bytes().any(|b| b <= 0x20 || b == 0x7f)`). Test not re-read in `tests/prime.rs:55-58` (not read this round).
5. closed: repo-held skill no longer counted as unreviewed global. Code: `src/agents/skills.rs:441-443` skip now precedes the count at `:445-447`.
6. closed: `SKILL.md` check uses `symlink_metadata`. Code: `src/agents/skill_bundle.rs:134`.
7. closed: two-writers `graph.json` row. Code: `src/agents/skill_facts.rs:79-82` (pid + per-process counter temp name, rename at `:83`). Test: `tests/prime.rs:469-509` `two_writers_at_once_leave_a_cache_that_parses`.

## prove-it

Gates not run this round. `/private/tmp` volume at 100% (154 MiB free, `df -h /private/tmp`); `pgrep -x rustc` returned pids 43136, 45373, 45451, 45537 (other sessions). No build started.

Invariants, read from code and `git diff --stat 2e295db..706a955`:

- I1 holds: `src/claude.rs` not in the stat.
- I2 holds: no path under `.chief-of-staff`, `context`, `out` in the stat.
- I3 holds: only new child is `seed::clone_cmd` (`cp -Rc`, `src/agents/seed.rs` now `pub(crate)`), via `proc::run` at `src/agents/skill_bundle.rs:148` with `left` budget.
- I4: not grepped (no `rg` available). Read files show no `libc::kill`. Unverified.
- I5 holds by diff: `src/mcp.rs` change is two `#[serde(default)]` arg fields and a `spin_off` call; `REMOTE_TOOLS` untouched. Line 31 not re-read.
- I8 holds: `src/agents/harness.rs` keeps `CLAUDE_SANDBOX`; new flags `--plugin-dir` and Codex `-c skills.config=…` only.
- I11: `Spec.skills` serde attribute not read. Unverified.
- I13 holds: `src/agents/mod.rs:603` (`write_json`), `:717`, `:744` (`write_state`).
- I14 holds: `src/context.rs` not in the stat.

VERIFY BY HAND (none run here):

1. Live Claude hire with a bundled skill, throwaway repo, capped: `cos --budget 0.25 agents spawn --role builder --skill <name> --repo <throwaway> --prompt "list the skills you can see"`, then `cos agents show <id>`. Pass: `spec.json` `skills.picked` names `<name>`; run dir has `skills/.claude-plugin/plugin.json`; the Herdr command line carries `--plugin-dir` and a `--settings` JSON with `skillOverrides`; the agent lists `cos-hire:<name>` and not a hidden global skill.
2. Live Prime read, read only: `cos skills ls --json`. Pass: exit 0, parses; no write.
3. Remote MCP: `cos serve --token $(openssl rand -hex 32)`. Pass: `tools/list` names only `latest_brief`; `tools/call` for `spawn` is `-32602`.
4. Codex hire with a hidden skill: `cos --budget 0.25 agents spawn --harness codex --skill <name> ...`. Pass: the Herdr command line carries `-c skills.config=[...]` with `enabled = false` for hidden names; Codex starts with it.
5. Gates, once disk has room and one build at a time: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --test hiring --test prime --test skills --test skills_cli`. Pass: all green; the r1 count was 60 tests across those four binaries, re-measure.

## Sink to VERIFY

| Sink | VERIFY at file:line | Test |
|---|---|---|
| S1 TCP connect (E1) | VERIFY-U `src/prime.rs:39-73` (`Prime::new`); only socket `src/prime.rs:106`, inside `call` (`:89`). Callers: `src/agents/skill_facts.rs:146`, `:330`, `:358` via `write_fact` | `tests/prime.rs` loopback-acceptance test (not re-read this round) |
| S2 `prime_add_node` / `prime_add_edge` (E2) | VERIFY-W `src/agents/skill_facts.rs:278-316`; `COS_RUN_ID` `:285`. Target id of `prime_add_edge` (`:349-355`) comes from Prime, **not covered by VERIFY-W**. See New finding 1. Callers: `src/main.rs:1065` (set), `src/main.rs:1088` (TTY), `:1093` (COS_RUN_ID), `:1167` (curate), `apps/desktop/src/lib.rs` `skills_set` (`named` first) | `tests/skills_cli.rs` set/curate refusals (not re-read); `apps/desktop/tests/ipc.rs` (not run) |
| S3 `cp -Rc` src, dst (E4) | VERIFY-B: source `p.path` = canonical (`src/agents/skills.rs:423`); symlink/size in `tree_digest` (`skills.rs:161-194`); `valid_name` before dst join (`skill_bundle.rs:131`, dst `:143`); `SKILL.md` regular (`:134`); digest before (`:137`) and after copy (`:153`); copy `:148` | `tests/hiring.rs` changed-after-plan row, `tests/skills.rs` digest/symlink rows (not re-read) |
| S4 `--settings` JSON, `--plugin-dir` (E5) | VERIFY-A: `src/agents/harness.rs` `claude_settings` via `serde_json::Value`; `--plugin-dir` takes a `PathBuf`; names from catalog keys (`skills.rs:78-94`) | `tests/hiring.rs` claude bundle test; `tests/skills.rs` hidden-set test (not re-read) |
| S5 Codex prompt block (E6) | VERIFY-P `src/agents/skills.rs:305-320` (`is_control` filter, `take(300)` per line) | **none for the rule.** `tests/hiring.rs:626-645` presence only. See r1 #3 (open) |
| S6 `graph.json` (E3) | VERIFY-R `src/agents/skill_facts.rs:193-238`, `roles_checked` `:242-260`; malformed-reply guard `:145-153`; temp-and-rename `:74-85` | `tests/prime.rs:440-467`, `:469-509`, `:219-241` |
| S7 `spec.json` skills, `state.json` skills_bundled (E7, E8) | `src/agents/mod.rs:601` (`skills: req.skills`), `:603` `write_json`; `:724-725` `skills_bundled` from `bundle()` copied names only | `tests/hiring.rs` spec round-trip test at `:647-668` |
| S8 window DOM (E9) | VERIFY-D `apps/desktop/ui/app.js:380-484`: `el()` with `textContent`, `badge()`; reads at `:391-401`, rows at `:420-464`. Global `innerHTML` grep not run (no `rg`) | `apps/desktop/tests/ipc.rs` (not run); `tests/design.rs` `el()` gate (not run) |
| S9 Codex `-c skills.config` (E10, E11) | VERIFY-C `src/agents/skill_bundle.rs:35-66`: TOML `Value` built, parse error and non-array fail at `:48-54`, `valid_name` at `:59` | `tests/skills.rs` override test, `tests/hiring.rs` Codex keep-user-entries test (not re-read) |

## New findings

1. blocking `src/agents/skill_facts.rs:229` and `:349-355`: the edge target is the Prime-supplied node id (`fact.node = n["id"]`), passed into `prime_add_edge` as `node:cos_skill:<id>`. No VERIFY covers that id: VERIFY-R (design `## Semantic ISA`, lines ~413-417) lists name, requires, verdict, roles, curated_at only. Evidence: read of `skill_facts.rs:229` and `:349-361`; `as_ref` at `:365-371` prefixes but does not validate. Impact small (Prime is a local writer, the edge stays inside Prime), but S2 is a sink whose VERIFY does not cover its input. Fix: in `fold`, drop a node whose `id` is not a plain Prime id (`^[A-Za-z0-9_:.-]{1,128}$`, else a warning naming the skill); add `node` to VERIFY-R in the design; add a `tests/prime.rs` case with a hostile id asserting no `prime_add_edge` carries it.

## Carried

- `apps/desktop/ui/app.js:481` shows `String(e)` (raw backend error) to the user via `app.js:455` (`Not recorded: ${err}`). Breaks the UI copy rule (raw error to user) and error-channel rule; the `logDev` call at `:480` is right, the user-facing string is not.
- `tree_digest` runs over every picked skill on every `plan()` (`skills.rs:418`), including each dialog keystroke via `agents_fit`, with a 20 MiB cap per skill and no cache.
- `codex_config` reads `CODEX_HOME` or `$HOME/.codex`, but roots read `COS_SKILLS_HOME` or `$HOME` (`hire.rs:52-55`). Built-as item 6 covers the env var; the split is not documented.
- `src/main.rs:1044-1045` `irrelevant: _` ignored; neither flag may record `irrelevant` silently. Clap group not read.
- `src/prime.rs:77` keeps raw authority (userinfo, `[::1]@evil`) as the Host header. Target is still loopback; cosmetic.
- Not re-read this round: `src/agents/skill_view.rs`, `tests/skills.rs`, `tests/hiring.rs` S3/S4/S9 tests, `tests/prime.rs:55-58` and `:286`, `apps/desktop/tests/ipc.rs`. No gates run; review rests on reading.
