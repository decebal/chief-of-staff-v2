---
design: docs/plans/2026-11-28-role-library-design.md
design-sha256: bcd698257e77a5e80d4d244bd8a50a28084d88b272608bb2c6269955b993b06b
verdict: findings
---

## Code review findings
- #1: closed. `src/agents/hire.rs:135-136` cleans the draft and the original finish line through `roles::clean`; the raw `done_when` is pushed only as the cleaned `original` (`hire.rs:138-140`). Test: `tests/roles.rs:339` (`a_hand_edited_finish_line_cannot_forge_a_line_in_the_hires_prompt`).
- #2: closed. `src/proc.rs:408-423` ignores SIGTERM in the guard, sends SIGTERM to its own group (`kill(0)`), waits up to `SWEEP_GRACE` (300 ms), then SIGKILLs each remaining member from `group_members` (`proc_listpids`, `PROC_PGRP_ONLY`). Design has the `TOOL_CALL(guard_sweep)` row (`design.md:573`, `:641`, `:663`). Test: `tests/proc_guard.rs:109` covers the SIGTERM-ignoring member; the starter-survives case is missing (B8).
- #3: closed. `apps/desktop/src/lib.rs:1383-1386` refuses with `no cos binary to guard the proposal` before any thread starts; `claude` cannot run unguarded from the window. Test: `apps/desktop/tests/roles_ipc.rs:194`.
- #4: closed. `src/agents/roles/propose.rs:243` calls `check(&p)` (caps, one-line rules) before `merge`. Test: `tests/roles_propose.rs:233-234` (`overcap` mode, asserts `breaks its caps: method`).
- #6: closed. `src/agents/roles/cli.rs:333`, `:364`, `:384` return before `apply` under `--dry-run` for `new`, `edit`, `retire`/`restore`. Test: `tests/roles_cli.rs:180` (`a_dry_run_writes_nothing_for_any_command`) asserts `agents.json` bytes unchanged.
- #7: closed. Existing `--out` target refused: `tests/roles_cli.rs:140-144`. Path under `context/` refused: `:145-151`. Symlinked parent refused: `:211-223`.

## Findings
1. blocking §3 (Refuse list, `design.md:214-222`): text omits two refusals the code makes on `output`: an empty value and a `\n`/`\r`. Evidence: `src/agents/roles.rs:327-328` (`text(..., "output", o, c.output, true, true)`, where `required` and `one_line` are both true). Fix: add "an empty `output`, or one holding a `\n` or `\r`" to the §3 Refuse list. Add a `tests/roles.rs:215` case for `output` with a newline.

2. blocking §4 step 6 vs ISA `STORE(agents.json)` row (`design.md:637`): the row still says `apply` requires "no `Refuse` problem". §4 step 6 (`design.md:299-302`) and code (`src/agents/roles/write.rs:297-315`, `before_problems` at `:220`, filter at `:302-303`) refuse only an added Refuse problem. Fix: change the row to "no `Refuse` problem the change adds".

3. blocking §1/§6 sink, `RENDER(finish_line)` (`design.md:639`): text says the finish line is "cut to the caps like the preamble". Code: `src/agents/hire.rs:135` cleans the draft to `2 * super::roles::CAPS.text` (600), and `:136` cleans the original to `CAPS.text` (300). 600 is not in the §1 table. Evidence: `src/agents/fit.rs:286-313` (the draft embeds the original's subject and the output path, so it can exceed 300). Fix: either add a "finish line draft" row to the §1 table at 600 and make `hire.rs:135` read it, or state the 2× rule in §6/§ISA. Pick one.

4. blocking §6 guard bullets (`design.md:427-432`), and the ISA row `guard_sweep` (`design.md:641`, `:663`) which contradicts them: the text says the guard "waits with `proc::wait_until`" and "calls `kill_group` on its own group". Code: `src/proc.rs:389-405` polls `try_wait` with `thread::sleep` and calls `child.kill()` on timeout (`:400`), not `wait_until`. `src/proc.rs:408-423` signals the group with `libc::kill(0, SIGTERM)` and then `SIGKILL`s each member from `group_members`, and never calls `kill_group`. Fix: rewrite the bullets to describe the code's poll and the `guard_sweep` sequence, and point to the `guard_sweep` row.

5. blocking ISA test matrix, `guard bound` row (`design.md:667`): text says the fake and its child are "gone within 4 s". `tests/proc_guard.rs:84-92` asserts `gone_within(.., Duration::from_secs(6))` for both. The test is looser than the row. Fix: assert 4 s in the test, or change the row to 6 s.

6. blocking ISA test matrix, `kill_and_reap` row (`design.md:664`): text says SIGKILL to `cos roles propose` leaves the fake and its child "gone at the guard's deadline (2 s in the test)". `tests/roles_cli.rs:401-433` runs `cos roles propose sre` with the production run deadline and asserts 3 s. The deaths come from SIGPIPE on the next 0.5 s line, not from the guard's limit. The 2 s guard-deadline check is `tests/proc_guard.rs:63`, which does not use `slow` mode. Fix: change the row to name the SIGPIPE path and the 3 s bound, and point the guard-deadline claim at `tests/proc_guard.rs:63`.

7. blocking ISA sink table, `TOOL_CALL(kill_and_reap)` (`design.md:640`): text says the kill "runs only inside `stream_lines_until` ... has not yet reaped", and "no pid is stored anywhere else". Code: `src/proc.rs:323` calls `kill_and_reap` inside `wait_until`, a separate pub fn used by other spawns. `src/proc.rs:243-244` calls `err.finish(pgid)`, which at `:308-313` runs `kill_group(pgid)` after the child is reaped. The stored pgid is a reaped pid's id. Fix: name every call site (`stream_lines_until`, `wait_until`, `Drain::finish`), and either drop the post-reap `kill_group` in `Drain::finish` or state why it is safe (the group stays reserved while members live).

8. blocking test, `guard_sweep` row (`design.md:663`, first column): "Started by a process that is not a group leader, the guard's sweep never reaches its starter (the test run survives)". No test does this. `tests/proc_guard.rs:63-154` has no case where the starter stays outside the group and survives. Fix: add a `tests/proc_guard.rs` case that starts the guard from a `Command` (not `process_group(0)`), lets it sweep, and asserts the test process and a sibling are still alive.

9. blocking test, `kill_and_reap` row (`design.md:664`, refused column): "Cancel with no run is a no-op". The only `roles_propose_cancel` call is `apps/desktop/tests/roles_ipc.rs:270`, after a run has started. Code is `apps/desktop/src/lib.rs:1355-1365` (`Proposing::stop` does nothing when the slot is `None`). Fix: add a call to `roles_propose_cancel` with no run in `apps/desktop/tests/roles_ipc.rs` and assert `Ok`.

10. blocking §5 and §6 signatures (`design.md:410-411`, `:408-409`): text says `roles::propose(cfg, entry, ctx, stop)` is "the one function both surfaces call", and `claude::execute_until(program, install, audit, run, stop)`. Code: `src/agents/roles/propose.rs:226-233` takes `(cfg, entry, ctx, id, guard, stop)`, and `src/claude.rs:203-210` takes `(program, install, audit, run, guard, stop)`. Fix: update both signatures in §6 to the code's, and say what `guard` is.

## Carried
- `design.md` §Testing (`:725`) names `apps/desktop/tests/ipc.rs` for `roles_save` and `roles_propose`. The file with those tests is `apps/desktop/tests/roles_ipc.rs`. Not checked whether `ipc.rs` also holds them.
- `design.md:725-731` says `apps/desktop/tests/roles_page.rs` is added to `task test-e2e`. Not verified here. The prove-it note says it is `#[ignore]`.
- `design.md` §6 (`:363`): `cos roles show --json` prints the role. The flag is accepted and ignored (`src/main.rs:293-296`), and output is always JSON. Behaviour matches, wording does not say so.
- Caps `output` (200) and `scope` (40 items, 200 chars) have no per-rule test in `tests/roles.rs:215`. The generic "over cap" rule is untested for those two fields.
- The row "same role with plain fields renders `Your finish line here:`" (`design.md:662`) is covered by `tests/fit.rs:295-315` (a plain role with an outward term), not by a second assert on the `tests/roles.rs:339` fixture.
- Not run: no cargo (per instruction). All evidence is from reading code and tests. Test pass or fail is not established, only that the named test exists and what it asserts.
