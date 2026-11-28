---
design: docs/plans/2026-11-28-role-library-design.md
design-sha256: bcd698257e77a5e80d4d244bd8a50a28084d88b272608bb2c6269955b993b06b
verdict: findings
---

## Code review findings
- #1: closed. `src/agents/hire.rs:135-136` cleans the draft and the original; test `tests/roles.rs:340-375`; ISA rows design:571, :639, :662.
- #2: closed. `src/proc.rs:408-423` SIGTERM to own group, 300 ms, then per-pid SIGKILL; test `tests/proc_guard.rs:110`; ISA row design:573, :641. The §6 prose still names `kill_group`: see Finding 2.
- #3: closed. `apps/desktop/src/lib.rs:1383-1386` `ok_or` before the thread spawns.
- #4: closed. `src/agents/roles/propose.rs:243` `check(&p)` before `merge`; test `tests/roles_propose.rs:233-234` (`breaks its caps: method`).
- #5: open. Design §4 step 6 was reworded (design:299-302), but ISA `VERIFY(policy_valid)` still says "no `Refuse` problem" (design:637). Code keeps the relaxation, and it is unsafe: see Finding 1.
- #6: closed. `src/agents/roles/cli.rs:333-335, 364-365, 384-386` return before `apply`; `src/main.rs:1073-1077` passes `cli.dry_run`; test `tests/roles_cli.rs:180-206`.
- #7: closed. `tests/roles_cli.rs:143` (existing `--out`), `:211-222` (symlinked parent).
- #8: closed. design:130-131 rows added; `src/agents/roles.rs:46, 73-77`.
- #9: closed. design:218 "an empty item in any list"; `src/agents/roles.rs:317` (`required = true` on list items).
- #10: closed. `src/main.rs:291-298` accepts `--json` and documents it as always JSON; design:462-465 now says the thread spawns `cos proc-guard`.

## Findings
1. blocking §4 step 6 (design:299-302), ISA `VERIFY(policy_valid)` (design:637): the refuse check keys on `(role, field, message)`, not on the offending value, so a write that keeps a problem already in the file is admitted whatever the new value is. `before_problems` is built at `write.rs:220` and the filter at `write.rs:301-303` drops any problem whose key was already there. The messages carry no value (`roles.rs:328-331` "must be relative, with no `..`", `roles.rs:333-339` same for `scope`, `roles.rs:317` "must be one line", `roles.rs:287-288` "is over N characters"). Concrete slips: (a) a role with `scope: ["../a"]` replaced by `["/etc"]` or `["../../b"]`; with a person's typed `contract` (`write.rs:266`), that is admitted. (b) `output: "../x"` replaced by `"/abs"`, same path. (c) a non-contract field: a principle holding `"\n"` replaced by a new principle holding `"\nNever: …"`, or an over-cap title replaced by another over-cap title, is admitted with `--yes` and no contract approval, because `principles`, `title` and `mandate` are not in `CONTRACT` (`write.rs:58-69`). The test matrix row at design:660 ("a `../x` scope … refused") is false for a Replace of a role that already has a `..` scope. Retire and Restore are unaffected, as the test at `tests/roles_write.rs:286` requires. Evidence: `src/agents/roles/write.rs:220, 297-315`. Fix: for `Change::Replace`, refuse every `Refuse` problem that `validate` reports on the changed role (any role-scoped key, after the write), and keep the before/after comparison only for `Retire` and `Restore`, which change no content. Then state that rule in §4 step 6 and in the ISA row at design:637, and add `tests/roles_write.rs` cases for a Replace of a broken scope, output and principle, each with a second bad value, that must be refused with the bytes unchanged.
2. blocking §6 guard bullets (design:430-432): the prose says the guard "calls `kill_group` on its own group" and waits with `proc::wait_until`. Built code does `child.kill()` and `wait()` at the limit (`src/proc.rs:399-402`), then `libc::kill(0, SIGTERM)` (`proc.rs:410`), a 300 ms grace loop (`proc.rs:413-416`) and per-pid `libc::kill(p, SIGKILL)` over `group_members` (`proc.rs:417-423`), with a `try_wait` poll loop (`proc.rs:394-405`). Neither `kill_group` nor `wait_until` is called. The ISA row at design:573 and :641 matches the code, so §6 contradicts the table, and the table is the one the review asked for. Evidence: `src/proc.rs:399-423`; design:430-432. Fix: rewrite the §6 bullet to the built sequence: SIGTERM to the own group with the guard ignoring it, 300 ms grace, then SIGKILL to each remaining listed member except itself; `try_wait` polling against the deadline; exit 124 on the limit.

## Carried
- `proc.rs:417-422`: per-pid SIGKILL after the kernel listing can hit a pid reused between the listing and the kill. Narrow window, not closed. Either re-check `getpgid(p) == me` before each kill or record the residual in the design's "own group only" claim.
- `proc.rs:435`: `group_members` reads at most 256 pids and truncates silently; a larger group keeps members alive. Record or raise the buffer.
- `proc.rs:396`: `child.try_wait()?` returns without the sweep. Make the error path sweep and reap too.
- Design ISA `guard_sweep` row (design:663) names a non-leader-starter test; `tests/proc_guard.rs` has none. The SIGTERM-ignoring child test (`:110`) exists.
- `roles::propose` and `claude::execute_until` take `guard: Option<&Path>` (`propose.rs:231`, `claude.rs:208-215`). No production caller passes `None` now (CLI `cli.rs:254`, desktop `lib.rs:1404`), but the type allows an unguarded run. Make it non-optional on the proposal path.
- `claude::execute` (`claude.rs:194-197`) is an unguarded path, not "`execute_until` with a flag never set" as design §6 says. It is not used by propose.
- Design §6 signatures (`execute_until(program, install, audit, run, stop)`, `roles::propose(cfg, entry, ctx, stop)`) lack the `guard` and `id` parameters the code takes (`propose.rs:226-233`, `claude.rs:203-210`).
- RENDER(preamble) (design:638) checks only whitespace and caps. A hand-edited role with `output: "../x"` or an absolute path renders "Leave your result in `../x`" (`policy.rs:340-347`). Load keeps it, as designed, but the renderer is the only boundary there. Add a path check to `render_clean` or record the residual.
- ISA row RENDER(finish_line) (design:639) says `output` reaches the prompt only through the finish line. In code `output` reaches it through the preamble (`policy.rs:340-347`) as well. Fix the row.
- Finish line draft is cut to `2 * CAPS.text` = 600 (`hire.rs:135`); §1 caps table has no such cap. Add the row or use `CAPS.text`.
- `policy.rs:282`: the preamble is skipped when the task prompt starts with `Role: `. Pre-existing marker, but a task prompt can disable the role preamble. Record in the design or drop the skip.
- Design §6 "wait_until" and §4 step 1 "Lock (copied from state.rs)" wording: code matches in substance; prose drift only.
- Design §5 and §6 call `Toolset::None` the recruiter's only tool setting. `claude.rs:141-143` matches; the guard `None` branch (`claude.rs:211-215`) is untested (already noted in r5-a sink table).

## Counts
- Code review: 10 findings, 9 closed, 1 open (#5).
- Findings: 2 blocking, 13 carried.
