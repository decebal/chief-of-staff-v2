# EGM review, round 7

*   **Document:** [business dashboards design](../../../plans/2026-11-13-business-dashboards-design.md)
*   **Round:** 7
*   **Reviewers:** design, comprehension, structural, style, codebase, readiness
*   **Findings:** 54 (27 major, 27 nit)
*   **Verdict:** FINDINGS
*   **Document status:** In Review (unchanged)

## Dispositions

*   FIX: 54. Applied to the design document. Each fix was carried through Goals, the Technical Plan, Risks, the Semantic ISA (instructions, VERIFY table, violations, test matrix), Confidence and the Detailed Implementation Plan.
*   CONTEST: 0.
*   DECLINE: 0.
*   ESCALATE: 0. The leftover-group SIGKILL (escalation C5 of round 6) is still waiting for the owner. This round records it as Open Question Q2 with a gate and does not change the behaviour.

Line numbers refer to the design document before this round's edits.

## Root-cause groups

*   **Exited slot keeps its exit cell** (D1, C3, C4, S4, S7, B11). `Slot::Exited { status, pgid, exit, waiter }`. `ensure: owned` drops an `Exited` slot with no waiter only once its exit cell is set; otherwise it waits up to 5 s on the cell, then refuses with "still exiting". The stop path collects `Exited` slots with an unset cell, waits on them within the shared budget, and polls `group_alive`. The stop path bullets are reordered: SIGTERM, wait, SIGKILL, wait 1 s, poll groups, log and return. Edited §4 Slots, Port lock, Reaper, ensure: owned, Stop path; Risks group row; matrix rows "reopen right after an owned child exits" and "quit while a reaper finishes"; item 8 `Slot`, `reap`, `ensure`, `stop_owned`; item 23 cases.
*   **dashboard_open wiring and the Dashboards source** (D2, C5, Y4, Y7, R6, R8). Verified `blocking` at `apps/desktop/src/lib.rs:275-282` takes `FnOnce() -> anyhow::Result<T> + 'static` and returns `Result<T, String>`; `businesses` copies `registry` under the lock first (`lib.rs:420-424`). §5 now gives the steps: `named()` then `StaleBusiness`; an `AppHandle` moved into `blocking`, which calls `app.state::<Supervisor>()` itself, with `T = Result<Running, Refusal>`; `dashboards::load` on every call (nothing cached); new `Refusal::InvalidConfig`, `UnknownDashboard` and `Internal`. Goal 8, Violations, the RENDER VERIFY row, item 8 `Refusal`, item 9 and item 25 updated.
*   **Unique server ports** (D3, B3). §3 validation refuses two `[server.*]` tables with one `addr` port and an unmanaged dashboard on a declared server's port. §4 lead paragraph, Launch cleanup step 2, item 1, item 19.
*   **Leftover pidfile removal and Q2** (C1, Y1, B1; D5, C2, S1, Y2, B5, R1). Verified against round 6 report C5: the owner has not decided the leftover-group SIGKILL. Added Q2 with options, the round 6 recommendation and a Build gate. Goal 7, §4 Launch cleanup and the Risks row point to Q2. The ISA pidfile-removal admission now matches the cleanup prose: a Leftover pidfile is removed once the leader fails the five checks or after the group SIGKILL.

## Findings

| id | reviewer | sev | disposition | edit / evidence |
| --- | --- | --- | --- | --- |
| D1 | design | major | FIX | Exited-slot group. Orig l.228, 232, 238, 650. |
| D2 | design | major | FIX | dashboard_open group. `lib.rs:275-282`. Orig l.276, 656, 665. |
| D3 | design | major | FIX | Unique-port group. Orig l.210-215, 258, 458. |
| D4 | design | major | FIX | `remove_dash_plist` VERIFY row admits Held (both files) or Timeout (`.new` only). Orig l.512. |
| D5 | design | major | FIX | Q2 group. Orig l.52, 261, 400. |
| D6 | design | nit | FIX | Cite `AppState::boot` at `lib.rs:148`, called by `App::new` at `:236`. Orig l.668. |
| D7 | design | nit | FIX | `start.registry` in setup; verified setup at `lib.rs:975-982` has `start`, no `st`. Orig l.255, 668; item 1 l.596. |
| D8 | design | nit | FIX | `spawn_server` binding out `SpawnResult {child}`; step 1 derives pid, pgid, start time and exe. Orig l.246, 460. |
| D9 | design | nit | FIX | `VERIFY(log_path)` named as an ensure: start step before the spawn; it has no refusal branch because the name comes from a `u16` port. ISA binding, VERIFY row, matrix row. Orig l.245, 462, 507, 541. |
| C1 | comprehension | major | FIX | Q2 group (pidfile removal). Orig l.505, 537. |
| C2 | comprehension | major | FIX | Q2 group. Orig l.400. |
| C3 | comprehension | major | FIX | Exited-slot group. Orig l.238. |
| C4 | comprehension | major | FIX | Exited-slot group (stop path). Orig l.266-268. |
| C5 | comprehension | major | FIX | dashboard_open group. Orig l.276, 665. |
| C6 | comprehension | major | FIX | Verified `kill(0)` counts a zombie (`src/proc.rs:246`) and `agents::spawn` reaps the supervisor on a thread (`src/agents/mod.rs:482-485`). Confidence now names the CUJ 8 test; the zombie case is an accepted gap in the Risks row; item 25 CUJ 8 test reaps what it kills. Orig l.406, 578, 808. |
| C7 | comprehension | nit | FIX | Same edit as D6. |
| C8 | comprehension | nit | FIX | `VERIFY(spawn_result)` takes `addr` from the validated `Server`. Orig l.248, 472. |
| C9 | comprehension | nit | FIX | S9d: "removed, or moved into place, on every outcome". Orig l.446. |
| S1 | structural | major | FIX | Q2 group. Orig l.400. |
| S2 | structural | nit | FIX | Same edit as D6. |
| S3 | structural | nit | FIX | Same edit as D7. |
| S4 | structural | nit | FIX | Exited-slot group. Orig l.238. |
| S5 | structural | nit | FIX | Verified `{{.LAUNCHCTL}}` seam at `Taskfile.yml:220,222`. §7 step 2 and item 18 use `{{.LAUNCHCTL}} print ... 2>/dev/null \|\| true`; item 22 fake prints `pid = N` and exits non-zero when not loaded. Orig l.324, 725, 762. |
| S6 | structural | nit | FIX | `STORE(pidfile)`, `STORE(port_lock)` and `STORE(ui-state)` bindings state that the directories and the ui-state lock file are fixed app paths that no input names, so they need no VERIFY. Orig l.437, 441, 467-470. |
| S7 | structural | nit | FIX | Exited-slot group (stop path order). Orig l.267. |
| Y1 | style | major | FIX | Q2 group (pidfile removal). Orig l.259, 505, 537. |
| Y2 | style | major | FIX | Q2 group. Orig l.400. |
| Y3 | style | major | FIX | Added the missing cases so the Verification Plan claim holds: item 23 (log path, lock inode and non-digit pidfile, second instance, dead leftover, reopen after exit, quit while a reaper finishes, pgid not equal to pid); item 25 (GET with foreign Origin, POST with app origin, GET with no Origin, `UnknownDashboard`, stale business and non-date filename for `brief_seen`); item 22 (no job loaded, `Taskfile.yml` text scan for `VERIFY(dash_target)`). Orig l.538-542, 558-560, 828. |
| Y4 | style | major | FIX | dashboard_open group (load on every call). Orig l.314, 665, 668. |
| Y5 | style | nit | FIX | Same edit as D6. |
| Y6 | style | nit | FIX | Risks Q1 row adds the check 4 fallback; Confidence iframe row adds the gazette frame headers. Orig l.402, 583. |
| Y7 | style | nit | FIX | Goal 8 lists stale business, unknown dashboard, invalid `dashboards.toml` and internal error. Orig l.53. |
| Y8 | style | nit | FIX | ensure: start names the `platform` rule of `VERIFY(binary_admissible)`. Orig l.243. |
| Y9 | style | nit | FIX | §7 step 3 and item 18: bootout uses `2>/dev/null \|\| true`, as `Taskfile.yml:220` does. Orig l.325. |
| Y10 | style | nit | FIX | `VERIFY(readiness)` inputs renamed `exited_before_1` and `exited_after_2` (slot marked `Exited`). Orig l.461. |
| B1 | codebase | major | FIX | Q2 group (pidfile removal). Orig l.259, 505. |
| B2 | codebase | major | FIX | Epilogue stop target: the group when `pgid == pid`, else the unreaped leader alone via new `proc::signal_pid`; the reaper's group kill requires `pgid == pid`. `VERIFY(ownership)` Starting, `signal_group` binding and VERIFY row, Violations, matrix row, item 4, item 8 `reap`, item 23 case. Orig l.253, 458, 466, 472, 503, 527. |
| B3 | codebase | major | FIX | Unique-port group. Orig l.211, 223, 258, 458. |
| B4 | codebase | major | FIX | Verified exhaustive literals at `tests/agents.rs:686,704,721,756` and `Counts` at `src/agents/mod.rs:553-558`. New item 3b; item 3 states `Counts` gains `starting`. Orig l.614. |
| B5 | codebase | major | FIX | Q2 group. Orig l.400. |
| B6 | codebase | nit | FIX | §6 states today's `Counts` and "after item 3". Orig l.294. |
| B7 | codebase | nit | FIX | Same edit as D6. |
| B8 | codebase | nit | FIX | Same edit as D7. |
| B9 | codebase | nit | FIX | S2 and the `signal_group` binding name `proc::signal_group`, `proc::signal_pid` and `proc::kill_group` as the implementations. Orig l.436, 466, 503. |
| B10 | codebase | nit | FIX | Same edit as Y6 (Risks row). |
| B11 | codebase | nit | FIX | Exited-slot group. Orig l.238. |
| R1 | readiness | major | FIX | Q2 group. Orig l.400. |
| R2 | readiness | major | FIX | Same edit as D4. |
| R3 | readiness | major | FIX | Verified the `Start` literal at `apps/desktop/examples/preview.rs:61-69`. New item 9a fills the three new fields. Orig l.668. |
| R4 | readiness | major | FIX | Verified `pub(crate) fn lock_exclusive` at `src/agents/slots.rs:51`, `pub struct Held` at `:36`, `pub mod slots` at `src/agents/mod.rs:32`. New item 3a makes `lock_exclusive` `pub`; §6 and the item 9 `ui_state` helper use it. Orig l.303, 670. |
| R5 | readiness | major | FIX | Same edit as C6. |
| R6 | readiness | major | FIX | dashboard_open group (`UnknownDashboard`, `InvalidConfig`, load per call, item 25 cases). Orig l.656, 665. |
| R7 | readiness | nit | FIX | Same edit as D6. |
| R8 | readiness | nit | FIX | dashboard_open group; a join error maps to `Refusal::Internal`. Orig l.665. |
