---
range: 2e295db..56a0373
prove-it: 🟡 MERGE AFTER CHECK
verdict: clean
---

## Round 2 findings

1. closed: `prime_search` reply without a `nodes` list is a failed read. Code: `src/agents/skill_facts.rs:145-153` (guard), `:173-186` (`failed_at`, previous graph kept). Test: `tests/prime.rs:462-489` `a_reply_without_a_nodes_list_keeps_the_last_graph`.
2. closed: `skills_hidden` in the inventory. Design `## Built as` E8 (extended), `docs/plans/2026-11-28-hire-skill-graph-design.md:62`. Field `src/agents/mod.rs:254`.
3. closed: no test for S5 control-character stripping and 300-char cap. Test now present: `tests/skills.rs:400-421` `the_codex_prompt_block_strips_control_characters_and_caps_each_line` (input `\x1b[2J` + 2,000 `x`; asserts no `\x1b`, line length 300). Code `src/agents/skills.rs:305-320` filters `is_control` then `take(300)` per line. Not run (see prove-it).
4. closed: `COS_PRIME_URL` with space or control byte refused before any socket. Code `src/prime.rs:43-45`. Test `tests/prime.rs:52-88` (refusal list includes `\r\n` and a space case at `:61-62`; asserts `connections() == 0` before any socket).
5. closed: repo-held skill no longer counted as unreviewed global. Code `src/agents/skills.rs:441-443` (skip) precedes the count at `:445-447`.
6. closed: `SKILL.md` check uses `symlink_metadata`. Code `src/agents/skill_bundle.rs:134`, `src/agents/skills.rs:105`.
7. closed: two writers on `graph.json`. Code `src/agents/skill_facts.rs:79-83` (pid + per-process counter temp name, rename at `:83`). Test `tests/prime.rs:492-` `two_writers_at_once_leave_a_cache_that_parses`.
8. closed: r2 blocking 1, edge target is a Prime-supplied id. Code: `src/agents/skill_facts.rs:229-232` runs `node_id_ok` (ASCII alnum or `-`, 1-64) in `fold`; a bad id drops the id, the fact stays, `write_fact` then warns "no edge to it" (`:352-354`). `node` is in VERIFY-R, `docs/plans/2026-11-28-hire-skill-graph-design.md:418-425`. Test `tests/prime.rs:441-460` `a_node_id_that_is_not_an_id_never_reaches_an_edge`. Caveat: the test asserts the folded fact (`node == None`), not a captured `prime_add_edge` call; the only edge path reads `fact.node`, so it is covered by reading, not by a call-level test.
9. open (carried, unchanged): `apps/desktop/ui/app.js:455` `Not recorded: ${err}` and `:481` `ui.skillErr[r.name] = String(e)` show the raw backend error to the user. `logDev` at `:480` is correct; the user-facing string is not.
10. open (carried, unchanged): `tree_digest` runs over every picked skill on every `plan()` (`src/agents/skills.rs:418`), no cache. Not re-measured this round.
11. open (carried, doc only): `codex_config` reads `CODEX_HOME` or `$HOME/.codex` (`src/agents/skill_bundle.rs:39-43`); skill roots read `COS_SKILLS_HOME` or `$HOME`. Design Built as item 6 names the env var, not the split.
12. closed: `main.rs:1045` `irrelevant: _` ignored. Clap enforces one of the two: `src/main.rs:257-262` (`relevant` has `conflicts_with = "irrelevant"` and `required_unless_present = "irrelevant"`), so `irrelevant` alone always maps to `Verdict::Irrelevant` at `:1052-1056`.
13. open (carried, unchanged): `src/prime.rs:77` keeps raw authority (userinfo, `[::1]@evil`) as the Host header. Target stays loopback (`:39-73`); cosmetic.
14. Not re-read this round: `src/agents/skill_view.rs`, `apps/desktop/tests/ipc.rs`, `tests/hiring.rs` S3/S4 rows beyond the test names. Test names only, no bodies, for these.

## prove-it

Verdict: 🟡 MERGE AFTER CHECK. No refuted claim found by reading. Gates not run: `df -h` on the repo volume shows 2.4Gi available (under the 3 GiB floor), and `pgrep -x rustc` lists 5 running compilers (94132 94335 94402 94934 95008). No build started.

Invariants (read from code and `git diff 2e295db..HEAD`):
- I1 holds: `src/claude.rs` not in the stat.
- I2 holds: no path under `.chief-of-staff`, `context`, `out` in the stat.
- I3 holds: no new `Command::new` in the diff (`git diff -G "Command::new" -- src` is empty). The copy `proc::run` at `src/agents/skill_bundle.rs:148` takes the remaining budget from `prepare` (`src/agents/mod.rs:723`).
- I4 holds by diff: no `libc::kill` added.
- I5 holds: `src/mcp.rs` adds two `#[serde(default)]` args and a `spin_off_hire` skill argument; `REMOTE_TOOLS` at `src/mcp.rs:31` is `["latest_brief"]`, unchanged.
- I8 holds: `CLAUDE_SANDBOX` (`src/agents/harness.rs:14`) and `--permission-mode acceptEdits` unchanged; `claude_settings` starts from `CLAUDE_SANDBOX` and only adds `skillOverrides`; new flags are `--plugin-dir` and Codex `-c skills.config=`.
- I11 not checked this round.
- I13 holds: state writes at `src/agents/mod.rs:603` (`write_json`), `:717`, `:744` (`write_state`).
- I14 holds: `src/context.rs` not in the stat.

VERIFY BY HAND (none run here):
1. Live Claude hire with a bundled skill, throwaway repo, capped: `cos --budget 0.25 agents spawn --role builder --skill <name> --repo <throwaway> --prompt "list the skills you can see"`, then `cos agents show <id>`. Pass: `spec.json` `skills.picked` names `<name>`; run dir has `skills/.claude-plugin/plugin.json`; Herdr command line carries `--plugin-dir` and `--settings` JSON with `skillOverrides`; agent lists `cos-hire:<name>` and not a hidden global skill.
2. Live Prime read, read only: `cos skills ls --json`. Pass: exit 0, parses, no write.
3. Remote MCP: `cos serve --token $(openssl rand -hex 32)`. Pass: `tools/list` names only `latest_brief`; `tools/call` for `spawn` is `-32602`.
4. Codex hire with a hidden skill: `cos --budget 0.25 agents spawn --harness codex --skill <name> ...`. Pass: Herdr command line carries `-c skills.config=[...]` with `enabled = false` for hidden names; Codex starts with it.
5. Codex config refusal: a `$CODEX_HOME/config.toml` that does not parse, then a spawn with a hidden skill. Pass: `state.json` status `failed` naming the file, no `hc run` (`src/agents/mod.rs:642-646`).
6. Gates, once disk has at least 3 GiB free and `pgrep -x rustc` is empty, one at a time: `cargo fmt --check`; `cargo clippy --all-targets -- -D warnings`; `cargo test --test hiring --test prime --test skills --test skills_cli --test design`; `cargo test -p <desktop crate> --test ipc`. Pass: all green. `tests/design.rs:940-1013` is the innerHTML scanner that guards S8.

## Sink to VERIFY

| Sink | VERIFY at file:line | Test |
|---|---|---|
| S1 TCP connect (E1) | VERIFY-U `src/prime.rs:39-81` (`Prime::new`); only socket `src/prime.rs:106` in `call` (`:89`). Callers: `src/agents/skill_facts.rs:146` (prime_search via `read_graph`), `:334` and `:361` (writes) | `tests/prime.rs:52-88` (refusals before socket, connection count) |
| S2 `prime_add_node` / `prime_add_edge` (E2) | VERIFY-W `src/agents/skill_facts.rs:281-319` (`COS_RUN_ID` `:288`, name `:294`, roles `:300`, requires `:303-313`, note `:314-319`); edge target VERIFY-R `:231` and `node_ref` `:382-389` | `tests/prime.rs:301-357` (one node, one edge per requires), `:358-413` (failing VERIFY-W reaches nothing), `:441-460` (node id). Callers: `src/main.rs:1065`, `:1167`, `apps/desktop/src/lib.rs:1308`; `COS_RUN_ID` gate also `src/main.rs:1093` |
| S3 `cp -Rc` src, dst (E4) | VERIFY-B: canonical source `src/agents/skills.rs:99-115`; symlink refusal `skills.rs:161-172`; `valid_name` before dst `src/agents/skill_bundle.rs:131`, dst `:143`; `SKILL.md` regular `:134`; digest before `:137`, copy `:148`, digest after `:153` | `tests/hiring.rs:789` (changed after plan), `:810` (rebuild, repo skill not copied); `tests/skills.rs:148` (digest refuses symlink and size) |
| S4 `--settings` JSON, `--plugin-dir` (E5) | VERIFY-A: `src/agents/harness.rs:18-30` `claude_settings` via `serde_json::Value`; `--plugin-dir` `harness.rs:40-42`; names from catalog keys (`skills.rs:440-463`) | `tests/hiring.rs:699` (bundle and override), `:755` (no picks, no plugin dir); `tests/skills.rs:298` (hidden set) |
| S5 Codex prompt block (E6) | VERIFY-P `src/agents/skills.rs:305-320` (`is_control` filter, `take(300)` per line) | `tests/skills.rs:400-421` (closes r1 #3). Rule holds on the block only; U+2028 and other non-Cc separators pass `is_control` (carried) |
| S6 `graph.json` (E3) | VERIFY-R `src/agents/skill_facts.rs:229-241` (`fold`), `roles_checked` `:245-263`; malformed-reply guard `:145-153`; temp-and-rename `:74-85` | `tests/prime.rs:135-152` (malformed replies), `:440-460`, `:463-489`, `:492-` (two writers) |
| S7 `spec.json` skills, `state.json` skills_bundled (E7, E8) | `src/agents/mod.rs:601` (`skills: req.skills`), `:603` `write_json`; `prepare` `:721-734`; `State` fields `:250-257` | `tests/hiring.rs:648-668` (spec round trip, old spec reads) |
| S8 window DOM (E9) | VERIFY-D `apps/desktop/ui/app.js:380-484`: `el()` with `textContent`, `badge()`. No `innerHTML`, `outerHTML` or `insertAdjacentHTML` in `app.js` (grep: only a comment at line 2) | `tests/design.rs:940-1013` innerHTML scanner (not run); `apps/desktop/tests/ipc.rs` (not run) |
| S9 Codex `-c skills.config` (E10, E11) | VERIFY-C `src/agents/skill_bundle.rs:35-66` (TOML `Value`; parse error and non-array fail `:48-54`; `valid_name` `:59`); command line `src/agents/harness.rs:55-57`; failure before `hc run` `src/agents/mod.rs:729-731`, `:642-646` | `tests/skills.rs:381-398` (override built from valid names only), `tests/hiring.rs:856-` (keep user entries, switch off hidden, bad config fails before Herdr) |

## New findings

None blocking.

## Carried

- `apps/desktop/ui/app.js:455` and `:481`: raw backend error shown to the user (r2 carried, open).
- `src/agents/skills.rs:418`: `tree_digest` per picked skill per `plan()`, no cache (r2 carried, open).
- `src/agents/skill_bundle.rs:39-43` vs roots: `CODEX_HOME`/`$HOME` split, undocumented (r2 carried, open).
- `src/prime.rs:77`: raw authority kept as the Host header (r2 carried, open).
- `src/agents/skills.rs:311` `codex_block`: `is_control` does not strip U+2028 or U+2029 (non-Cc separators), so a description containing one still breaks the prompt line. Low; the description comes from a SKILL.md the user installed.
- `src/agents/skill_view.rs`: not re-read. Test bodies for the S3 and S4 rows were checked by name only.
