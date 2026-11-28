---
design: docs/plans/2026-11-28-role-library-design.md
design-sha256: c99f344ab936c72683c2932c001a09533cc3fd95178ff49019860de444f2d8a1
verdict: findings
---

## Round 2 findings
- r2-a #1 (blocking, process ownership): open. Mechanism closed: the owning thread kills `claude` through `stream_lines_until` and `kill_and_reap` (design L353-360, L372-380; `src/proc.rs:291-295` kill_and_reap, `src/proc.rs:210-211` process_group(0)). Open part: the requested grandchild check is absent. ISA matrix L571 and acceptance 7 (L677) check only the recorded pid, not the group (`group_alive`, `src/proc.rs:405`).
- r2-a #2 (blocking, COS_CLAUDE): closed. Inventory row L462 names the env var, who sets it, what runs, who is affected. Matches `src/claude.rs:118-119` (`command_with(program, ...)`).
- r1-a #2 (open): closed. Same mechanism as r2-a #1 (design L353-355): cancel, quit and SIGINT reach `claude`'s own group.
- r1-a #19 (open, nit): open. `"$comment": "Role"` placeholder still at design L512, L513, L514, L515.

## Findings
1. blocking §6 (L330, `--out`), §ISA Side-effect inventory (L458), §4 (L240-275, sole writer): `--out <file>` writes any user-named path with no exists or `create_new` guard, and no lock or digest. `cos roles propose sre --out context/agents.json` overwrites the policy outside `apply`, which breaks the "only write to agents.json" claim (L22-23). `--out .chief-of-staff/state.json` breaks I13 (`.claude/skills/prove-it/SKILL.md` L64-65: state.json only through `state::commit`). Evidence: design L330, L458. Fix: `propose` refuses an existing `--out` path (`create_new`) and any path under `home/context`, `home/.chief-of-staff` or `out/`. Add both cases to `tests/roles_cli.rs`.
2. blocking §5 (L305-306 fallback), §ISA sink table (L546), ISA matrix (L567), Security (L607-608, "No tool calls are possible"): the fallback `Toolset::Brain(vec![])` passes no `--strict-mcp-config`. `src/claude.rs:130-137` Brain branch adds only `--tools BRAIN_TOOLS --allowedTools`. `src/claude.rs:140-141` is the only branch with `--strict-mcp-config`. The sink's VERIFY (args `--tools ""` and `--strict-mcp-config`) and the matrix assert only `None`. On the fallback, "no tool calls" rests on `--permission-mode dontAsk` (`src/claude.rs:126`) alone, and the account connectors still load. Fix: the fallback variant also passes `--strict-mcp-config`, and the matrix asserts the args for it. Or drop the fallback.
3. blocking §3 (L213-220, "Load never drops a role"), Errors (L430): the design contradicts itself. A whole-file parse failure replaces every role with the template's. `src/agents/policy.rs:141-144` (`unwrap_or_else` to `Policy { note, ..default }`) and `:146-148` (seed roles when empty). `Role` and `Policy` are `deny_unknown_fields` (`policy.rs:55`, `:109`), so one bad field in one role (for example `"expects": "files"`, or a key an older cos lacks) drops all of them, and `validate` never runs, so no per-role Problem exists. Fix: parse roles one at a time (`Value` per role, `from_value` each). A role that fails becomes a Problem and keeps its raw JSON for display. Or restate the invariant as file-level and say that the Staff tab shows `note` for it. Add a test with one malformed role.
4. blocking §ISA harm (5) (L446), inventory row for `claude` (L462), §6 (L344-360): owner death is not covered. Harm (5) names "a `claude` child outlives its owner". The inventory says it "Ends at deadline, stop flag or exit". `spawn_grouped` (`src/proc.rs:210-214`, process_group(0)) puts `claude` in its own group. `stream_lines_on` (`src/proc.rs:165-206`) exits only on deadline, stop, the result event, or EOF. If cos is SIGKILLed, a desktop force-quit, or a crash, nothing signals the group. `claude` lives until its own exit or a SIGPIPE on its next write. Also, `kill_and_reap` has no VERIFY instruction in the instruction set (L474-484). Its clearance (L550) is a code property, not a check. Evidence: design grep finds no crash or owner-death text (L446 is the only mention). Fix: add the owner-death row to the inventory and name the bound. Record `claude`'s pgid and `proc::start_time` (`src/proc.rs:349`) under `out/` at spawn. The next cos start reaps that group after the I4 identity check. Or record the risk as accepted with its bound.

## Carried to Plan
- §ISA inventory "Propose run" (L455): the claude child also loads the install dir's CLAUDE.md and `.claude/agents` and the cwd environment (`src/claude.rs:120` `current_dir(install)`). "The user sees the exact text first" (L608) covers only the `-p` prompt. should.
- §4 step 6 and ISA L569 "A file with no fit stays without one": `Policy.fit` (`policy.rs:121-122`) and the existing `Option` fields of `Role` (`policy.rs:73-95`) have no `skip_serializing_if`, so `"fit": null`, `"done_when": null`, etc. are written. Only the new fields carry the skip rule (L85-86). should.
- §4 step 4 `Replace { id, role }`: the behaviour when `role.id != id` is unstated (L259-261 says only that id is kept). should.
- §4 step 7: `.bak` behaviour when `agents.json` is absent is unstated. nit.

## Counts
- Round 2: 4 items. Closed: r2-a #2, r1-a #2. Open: r2-a #1 (test part missing), r1-a #19 (nit).
- New: 4 blocking (findings 1-4). 4 carried (should 3, nit 1).
- Verdict: findings.
