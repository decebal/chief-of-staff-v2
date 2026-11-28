---
range: 2e295db..c6a0c63
prove-it: 🔴 DON'T MERGE
verdict: findings
---

## prove-it

Gates, run in this session, one build at a time:

1. `cargo fmt --check`: no output (clean).
2. `cargo clippy --all-targets -- -D warnings`: `Finished`, no warnings.
3. `cargo test --test hiring --test prime --test skills --test skills_cli`: 60 passed (hiring 27, prime 14, skills 12, skills_cli 7).
4. Not run: full `cargo test`, `cargo run --example demo`, `apps/desktop` `ipc`, `tests/design.rs`, `task desktop-check`.

Invariants (claim C1: the skill graph ships per-hire skill sets without weakening I1 to I14):

- I1 holds: `src/claude.rs` not in the diff.
- I2 holds: `git diff --stat` shows nothing under `.chief-of-staff`, `context`, `out`.
- I3 holds: the only new child is `seed::clone_cmd` (`cp -Rc`) via `proc::run` (`skill_bundle.rs:136`), owned, deadline-bounded, reaped. Change in `seed.rs` is visibility only.
- I4: no new `libc::kill` in the diff.
- I5 holds: `mcp.rs:31` `REMOTE_TOOLS = ["latest_brief"]`; `allowed()` at `mcp.rs:389` gates both `list_tools` (`:420`) and `call_tool` (`:430`).
- I6: `dash.rs` not in the diff.
- I8 holds in code: `harness.rs` Claude argv is `CLAUDE_SANDBOX` plus `skillOverrides` (`claude_settings`); only new flags are `--plugin-dir` and Codex `-c skills.config=`. No `bypassPermissions`, no `danger-full-access`.
- I11 holds: `Spec.skills`, `State.skills_bundled`, `State.skills_hidden` all `serde(default)`.
- I13 holds: `skills_hidden` and `skills_bundled` written through `write_state` (`mod.rs:724-727`). `state.rs` untouched.
- I14 holds: no `context::tools` change, no workspace tool in `REMOTE_TOOLS`.

VERIFY BY HAND (paid or live; none run here, none spend money in this review):

1. Live Claude hire with a bundled skill, throwaway repo, capped: `cos --budget 0.25 agents spawn --role builder --skill <name> --repo <throwaway> --prompt "list the skills you can see"`, then `cos agents show <id>`. Pass: `spec.json` `skills.picked` names `<name>`; run dir has `skills/.claude-plugin/plugin.json`; the Herdr command line carries `--plugin-dir` and a `--settings` JSON with `skillOverrides`; the agent lists `cos-hire:<name>` and not a hidden global skill. Covers design A1 (sand).
2. Live Prime read, read only: `cos skills ls --json`. Pass: exit 0, parses, and the raw reply has a `nodes` array (see finding 1). Makes no write.
3. Remote MCP: `cos serve --token $(openssl rand -hex 32)`. Pass: `tools/list` names only `latest_brief`; `tools/call` for `spawn` is `-32602`.
4. Codex hire with a hidden skill: `cos --budget 0.25 agents spawn --harness codex --skill <name> ...`. Pass: the Herdr command line carries `-c skills.config=[...]` with `enabled = false` for hidden names, and Codex starts with it. Covers design A2 (the spike's claim).

## Sink to VERIFY

| Sink | VERIFY at file:line | Test |
|---|---|---|
| S1 TCP connect (E1, E2) | VERIFY-U `prime.rs:37-78` (`Prime::new`: scheme, literal loopback or `localhost`, no resolver). Only socket: `prime.rs:103`, reached only through `Prime::call`. Fields are private, so no other constructor. Call sites: `skill_facts.rs:144`, `:321`, `:348` | `tests/prime.rs` `only_literal_loopback_addresses_and_localhost_are_accepted_before_any_socket` |
| S2 `prime_add_node` / `prime_add_edge` (E2) | VERIFY-W `skill_facts.rs:268-306` (`COS_RUN_ID` `:275`, verdict `:278`, catalog `:281`, roles via `policy.hire` `:287`, `requires` `:290-300`, note `:301`). Callers: `main.rs:1065` (`set`), `main.rs:1167` (`curate`; TTY gate `main.rs:1088`, `COS_RUN_ID` `:1093`), `lib.rs:1308` (`skills_set`, `named()` first) | `tests/skills_cli.rs` `set_refuses_what_verify_w_refuses_and_writes_nothing`, `curate_needs_a_terminal_and_names_set_instead`, `set_with_prime_down_exits_one_naming_the_url`; `apps/desktop/tests/ipc.rs` (not run) |
| S3 `cp -Rc` src, dst (E4) | VERIFY-B: source is `picked.path` = canonical catalog entry (`skills.rs:404-428`); `tree_digest` refuses symlinks and >20 MiB (`skills.rs:161-194`); name `valid_name` (`skill_bundle.rs:119`) before the dst join (`:131`); digest before (`:125`) and after copy (`:141`). Only copy site: `skill_bundle.rs:136` | `tests/hiring.rs` `a_skill_changed_after_planning_fails_the_hire_before_herdr_starts_it`, `bundling_again_rebuilds_and_a_repo_skill_is_neither_copied_nor_hidden`; `tests/skills.rs` `a_skill_whose_tree_cannot_be_digested_is_not_picked`, `the_tree_digest_covers_every_file_and_refuses_inner_symlinks_and_size` |
| S4 `--settings` JSON, `--plugin-dir` (E5) | VERIFY-A: `harness.rs` `claude_settings` builds via `serde_json::Value`; `--plugin-dir` takes a `Path` (`harness.rs` argv). Names are catalog-validated before they reach `SkillSet` (`skills.rs:94`) | `tests/hiring.rs` `a_claude_hire_loads_its_bundle_and_hides_the_rest_outside_its_worktree`; `tests/skills.rs` `hidden_holds_irrelevant_claude_skills_and_the_claude_originals_of_bundled_ones` |
| S5 Codex prompt block (E6) | VERIFY-P: `skills.rs:305-320` (`codex_block`: `is_control` stripped, `take(300)` per line) | `tests/hiring.rs:634` asserts only that the block is present. **No row for control stripping or the 300 cap.** See finding 3 |
| S6 `graph.json` (E3) | Temp-and-rename `skill_facts.rs:72-84`; `Ok` arm `:144-162`; fold `:183-186`. **Malformed-reply guard missing for well-formed JSON without `nodes`.** See finding 1 | `tests/prime.rs` `a_failed_read_keeps_the_last_graph_and_marks_it_stale` (Err path only). Two-writer row: none. See finding 7 |
| S7 `spec.json` skills, `state.json` skills_bundled (E7, E8) | `mod.rs:579-595` `record_run` writes `req.skills` (from `Resolve`); `mod.rs:720-727` `skills_bundled` from `bundle()`'s return (copied names only) | `tests/hiring.rs` `the_skill_set_is_written_to_the_spec_and_an_old_spec_still_reads` |
| S8 window DOM (E9) | VERIFY-D: `apps/desktop/ui/app.js` skills code (`:380-470`) uses `el()` with `textContent`; `rg innerHTML app.js` hits only comment `:2` and `setAttribute` on static ids (`:307`, `:833`) | `apps/desktop/tests/ipc.rs:3357`, `:3414` (markup as text; not run here); `tests/design.rs` `el()` gate (not run) |
| S9 Codex `-c skills.config` (E10, E11) | VERIFY-C: `skill_bundle.rs:33-54` (`toml::Value` array; user entries parsed and kept; hidden names pass `valid_name` at `:47`) | `tests/skills.rs` `the_codex_override_is_toml_built_from_valid_names_only`; `tests/hiring.rs` `a_codex_hire_switches_off_irrelevant_agents_skills_and_keeps_the_users_own_entries` |

Undeclared side effects (drift): `state.json` field `skills_hidden` (finding 2). Other new effects map to E1, E3, E4, E10, E11 in the design or its Built-as table.

## Findings

1. **blocking** `src/agents/skill_facts.rs:144-162` (fold `:183-186`): the `Ok` arm accepts any JSON reply. A reply that parses but has no `nodes` array (for example `{}`) folds to zero facts, writes a new `read_at`, and returns `GraphState::Fresh`. That is the S6 case the design says must keep the previous graph and add only `failed_at`. Evidence: read `skill_facts.rs:144-153` and `:183-186` (`search["nodes"].as_array().into_iter().flatten()`); `tests/prime.rs:133-148` covers status, length, huge, RPC error and non-JSON text only; no test sends a JSON reply without `nodes`. Fix: in the `Ok` arm, return an error unless `v["nodes"].is_array()`, so the `failed_at` path runs. Add a case to `every_malformed_reply_is_an_error_naming_what_was_wrong` that sends `{}` and asserts `graph.json` keeps its prior facts.

2. **blocking** `src/agents/mod.rs:253`, `:724`, `:727`: `state.json` gains `skills_hidden`, written by `prepare` and read by `launch` into argv (E5/E11). The design's inventory (E8) and Built-as table name only `skills_bundled`. Evidence: `mod.rs:250-254` declares the field; `rg -rn skills_hidden src` shows the writes at `:724` and `:727`. Fix: add a row for `state.json` gaining `skills_hidden` (feeds `skillOverrides` and `skills.config`) to the design's inventory, or fold it into E8.

3. **blocking** `tests/hiring.rs:634`, `src/agents/skills.rs:305-320`: S5 matrix row (control characters stripped, 300-char cap) has no test. The code is right on read (`is_control` filter, then `take(300)`), but nothing asserts it. The plan's epic gate requires every ISA matrix row to be a named passing test. Fix: add a test with a picked skill whose description is `"\x1b[2J"` followed by 2,000 characters; assert the Codex prompt has no ESC byte and no line longer than 300 characters.

4. **should** `src/prime.rs:40-46` and `:108`: `Prime::new` accepts any path, and `call` writes it unchecked into the request line `POST {path} HTTP/1.1`. A `COS_PRIME_URL` with a space or CR/LF produces a malformed request, or injects a header. Impact is small: loopback only, and the value comes from the user's environment. Fix: reject a path containing bytes `<= 0x20` or `0x7f` in `Prime::new`; add it to `only_literal_loopback_addresses_and_localhost_are_accepted_before_any_socket`.

5. **should** `src/agents/skills.rs:440-444` vs `:445-447`: `unreviewed_on` counts a catalog skill that has no fact and is not picked, before the `project.contains` skip. A repo-held skill (used in place) is counted as an unreviewed global, so the warning at `:468-472` overstates. Fix: move the `project.contains(name)` continue above the count.

6. **nit** `src/agents/skill_bundle.rs:122`: `src.join("SKILL.md").is_file()` follows a symlink, where VERIFY-B says "regular `SKILL.md`". No reachable effect: `tree_digest` refuses a symlink inside the tree (`skills.rs:170-172`). Fix: use `symlink_metadata(...).is_file()` for exactness.

7. **blocking** `src/agents/skill_facts.rs:76-82` (`store_cache`): the design's S6 matrix row "two writers at once leave a file that parses" has no test. The temp name is per pid and nanos, and the rename is atomic, so the code probably holds. Nothing exercises it. Fix: a test that runs two `store_cache` writers on one home and asserts `graph.json` parses afterwards.

Counts: 4 blocking (1, 2, 3, 7), 2 should (4, 5), 1 nit (6).

## Attacks tried

- I1, I2, I3, I4, I5, I8, I11, I13, I14 (see prove-it).
- S8 DOM: `rg -n "innerHTML|setAttribute|\.href|\.src\b" apps/desktop/ui/app.js`.
- S1 URL parsing: `Prime::new` refuses `http://127.0.0.1@evil`, `http://127.0.0.1.evil.test`, `10.0.0.1`, `example.com`, and a non-`http://` scheme (`tests/prime.rs:55-58`, `:286`).
- Cache concurrency: single flight per home (`skill_facts.rs:27-46`, `:141`); `tests/prime.rs` `ten_reads_at_once_open_one_connection_and_the_rest_use_the_file` passes.
- Project skill wins over the catalog: `resolve` skips hide for project names (`skills.rs:445-447`); `tests/skills.rs` `a_project_skill_wins_it_is_neither_bundled_nor_hidden` passes.

## Considered and dropped

- `SkillsAsk::off()` looks unused: it is called by `tests/fit.rs:131`, `:226`, `:364`. Kept as a test helper.
- S4 "fact named `x","permissions":{` reaches argv": dropped by `fold` (`skill_facts.rs:197-199`) before any argv; `tests/prime.rs:159` asserts `Bad Name` is not a fact.
- Userinfo or bracket tricks in `COS_PRIME_URL` (`@`, `[::1]`): parsed and refused or accepted only as loopback (`prime.rs:47-70`).

## Not verified

- Live Prime's reply shape (design A4) and live Claude `--plugin-dir` (A1): not contacted, per the review brief. Listed under VERIFY BY HAND.
- `read_graph` compares RFC 3339 strings for `failed_at <= read_at` (`skill_facts.rs:128-129`). Lexicographic order can disagree with time order when fractional seconds are trimmed. Not reproduced; at worst one extra Prime call.
- Desktop `ipc`, `tests/design.rs`, full `cargo test`, and the demo example: not run.
