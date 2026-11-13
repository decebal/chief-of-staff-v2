---
design: docs/plans/2026-11-12-multi-business-design.md
design-sha256: superseded
verdict: findings
---

Previous findings (r4):

1. blocking Resolver/Decision: partially resolved. Decision (design:62-64), Resolver (design:146-154) and Accepted trade-offs (design:470-472) state the user decision consistently. Testing (design:420-423) and Acceptance 6 (design:503-504, `…` elides the triple) do not state the partial-fill result. See finding 1.
2. blocking ISA rejudge sink: resolved. Sink row design:384, instructions design:359-361, matrix design:413, test design:445. Caveat: finding 5.
3. blocking ISA clean sink: resolved. Sink row design:385, matrix design:414, test design:446. Caveat: finding 4.
4. should Resolver `state init`: not resolved. design:159-160 says it resolves like any command; design:394 says it still runs on a corrupt registry. Flagless `state init` reads the registry at Resolver step 3 (design:152), so it stops. See finding 9 for the fix.
5. should Desktop hash restore: resolved. design:245-248 names `openTab(tab, pane)`; `app.js:134-152` is the anonymous handler, `app.js:195` is `showPane`. Hand-check design:453.
6. should Desktop ⌘K and hand-check: resolved. design:58-60, hand-check design:451-455 covers copy, paste and undo.
7. should Desktop in-flight writes: resolved. design:252-257 lists the seven writes and the source of `business`. Matrix design:412.
8. should Desktop `businesses()` error: resolved. design:227-230; test design:447.
9. should Risks launchd after `use`: resolved. design:313-315, Departures design:79.
10. should ISA `task schedule`: partially resolved. Inventory row design:349 added; no sink row. See finding 6.
11. nit Testing spawn: not resolved. design:432-433 unchanged; test still passes when active business equals spawning triple.
12. nit Desktop migration wording: not resolved. design:215-216 unchanged.
13. nit Acceptance 5 queued run: not resolved. design:501-502 unchanged.
14. nit Registry lock helper: not resolved. design:134 still says "the lock helper in `src/agents/slots.rs`"; `slots.rs:44` `flock` is private and takes a `File`.
15. nit Terms `default`: not resolved. design:161, 185 and 493 still use `default` for the built-in layer.
16. nit Acceptance 4: not resolved. design:497 "focus inside nothing"; design:498-499 omits the Hire pane.
17. nit `Settings::save`: not resolved. design:213-216 says "never written again"; `settings.rs:91-98` serialises every field, so `Option` fields write `null` unless `skip_serializing_if` is set.
18. nit `default_home()` location: not resolved. design:153 gives no location; the library/binary split is unstated.

Findings (blocking first):

1. should Testing / Acceptance 6 (Decision consistency): the partial-override fill is stated only in Decision and Resolver. Evidence: design:62-64 vs design:420-423 ("`COS_HOME` with an active business gives the env triple", no projects value) and design:503-504 (`current: explicit $HOME/Projects/chief-of-staff-v2 …`, projects elided). Fix: add a test "`COS_HOME` alone with an active business: workspace = home, projects = `$HOME/Projects`, layer explicit", for CLI and app; write the full expected triple in Acceptance 6 with no `…`.

2. should ISA sinks, `agents_stop` and reconcile kill: both kill process groups from run-record fields, which design:372-374 lists as tainted. No sink row. Row design:382 ("each receives a triple that passed switch_business ... keeps its existing checks") is the only cover, and it does not name the check. Evidence: `src/agents/mod.rs:553-559` (stop: `libc::kill(pid, SIGKILL)`, `proc::kill_group(pgid)`), `mod.rs:485-486` (reconcile), `mod.rs:685-689` (`harness_group`: group alive and `started_at` matches `pgid_started`), `mod.rs:668-671` (`is_cos`); design:343, 347. Fix: add sink rows for `agents_stop` and reconcile, cleared by a named `VERIFY(run_pgid)` (supervisor `is_cos`; group alive with matching start time). Add to the instructions table and a matrix row: a record with an edited `pgid` kills nothing.

3. should ISA sinks, `agents_spawn`: runs `git worktree add` in `SpawnArgs.repo` (`mod.rs:371-389`) and starts a supervisor. Its only cover is `VERIFY(write_business)` (design:383), which checks the business name, not the repo. `checkout()` accepts any absolute path (`src/agents/hire.rs:419-424`, `:441`). Fix: add a spawn row cleared by `VERIFY(run_paths)` on `repo` (under the projects directory), or state that the repo field is trusted local UI input.

4. should Sinks / Departures: `rejudge` and `clean` refuse any run whose repo lies outside the projects directory (design:384-385, matrix design:414). Hire accepts such repos (`hire.rs:419-424`). Clean of an in-place run (`mod.rs:578`, worktree false) touches only the run directory, so the check refuses a run it would not harm. Nothing in Departures or Violations (design:399) says legitimate runs become un-cleanable. Fix: either make spawn enforce the projects directory and say so in Departures and Violations, or scope the clean check to worktree runs.

5. should Taint / sinks: `judge.rs:293` runs `sh -c <cmd>` in `spec.workdir`. `cmd` comes from the run record, which design:372-374 lists as tainted. `VERIFY(run_paths)` (design:384) checks only the path. Fix: state in Taint that check commands are user-authored contract text on the same user's files (not a privilege boundary), or add a verify for the command text.

6. should ISA sinks, `task schedule`: `BUSINESS` is substituted into plists and loaded with `launchctl bootstrap` (`Taskfile.yml:187-197`; design:204-206). No sink row. The name is validated only by `cos business home`, and the order of the plist write and that lookup is not stated. Fix: add the sink row, cleared by `VERIFY(business_name)` that must run before any plist write; state the order.

7. nit ISA sinks: row design:382 ("app commands in the inventory | the switch's VERIFY") overlaps rows design:383-385. Which VERIFY clears `agents_clean` is ambiguous. Fix: "other app commands" and name the exclusion.

8. nit ISA sinks: row design:380 has no check for `use` (sets `active`; design:193 requires the business to exist). Fix: add "`use`: name exists" to the row.

9. should Resolver / Violations, `state init` (carries r4 #4): design:159-160 and design:394 disagree. Fix: one rule, stated once in Resolver. Proposed: `state init` resolves like any command; with a broken registry and no flags it stops; with `--workspace` it proceeds without reading the registry. Make design:394 and the matrix (design:409) match.

Verdict: findings. Blocking 0, should 7 (findings 1-6, 9), nit 2 (findings 7-8). Unresolved r4 nits: 11-18.
