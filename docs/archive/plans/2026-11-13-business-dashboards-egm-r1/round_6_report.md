# EGM review, round 6

*   **Document:** [business dashboards design](../../../plans/2026-11-13-business-dashboards-design.md)
*   **Round:** 6
*   **Reviewers:** design, comprehension, structural, style, codebase, readiness
*   **Findings:** 45 (25 major, 20 nit)
*   **Verdict:** FINDINGS
*   **Document status:** In Review (unchanged)

## Dispositions

*   FIX: 44 findings. Applied to the design document, with the Semantic ISA, Risks and the Detailed Implementation Plan updated to match.
*   ESCALATE: 1 finding (C5). The behaviour change is not applied. The document now states the current behaviour in Goal 7, §4 Launch cleanup and a new Risks row. Quoted verbatim below.
*   CONTEST: 0.
*   DECLINE: 0.

Line numbers in the table refer to the design document before this round's edits.

## Root-cause groups

Several findings described one defect from different sections. Each group got one definition, rewritten everywhere it appears.

*   **Exit cell** (D2, C1, S1, Y1, B1, R1). The non-`Clone` `Receiver<ExitStatus>` inside `Owned` and `Spawned` is replaced by `Arc<Exit>`, `Exit { status: Mutex<Option<ExitStatus>>, done: Condvar }`, cloned into the slot and the reaper. Waiters clone the `Arc` under `inner` and wait outside it. Defined in §4 Slots; used in §4 Reaper, Failure epilogue, Stop path; types and `reap` signature in item 8.
*   **Slot identity and the reaper race** (D1, D5, C2, B8, R4). `Exited { status, waiter: Option<Arc<Waiter>> }`: a `Starting` slot marked by the reaper keeps its waiter, so `ensure: owned` waits on it instead of dropping it. `ensure` writes only while the slot carries its own waiter (`Arc::ptr_eq`). The reaper starts in the critical section that records `Spawned`; `start_time` and `exe` are read before that, while the child cannot be reaped; the pidfile is written under `inner` only if the slot is not `Exited`. `dashboard-exited` is emitted for `Owned` slots only.
*   **Wake-ups** (S2, B6). One rule in §4 Slots: every transition out of `Reserved` or `Starting`, and the `Spawned` record, calls `notify_all`. Success, probe External and probe identity refusal now state it.
*   **No `External` slot** (S5, Y4). The variant is removed; an External listener has no slot.
*   **Spawn function** (D4 parts 1-2, C4, S4, Y2, B5, R2). `pub fn real_spawn(server: &Server, log: &Path) -> io::Result<SpawnResult>` maps the `anyhow` error with `io::Error::other`; the builder passes `Box::new(servers::real_spawn)`.
*   **Start wiring** (D4 part 3, S3, Y6, B4, R3). The builder clones the `Option<Start>` into `setup`, which builds the `Supervisor` in both branches. `AppState` gains `ui_state_file`.

## Findings

| id | reviewer | sev | disposition | edit / evidence |
| --- | --- | --- | --- | --- |
| D1 | design | major | FIX | Slot identity group. §4 Slots (Exited, Slot identity, Wake-ups bullets), Reaper, ensure: owned, start steps 1-4, success, epilogue step (4). Item 8 `Slot::Exited { status, waiter }`. Matrix rows "open while a failed start runs its epilogue" and "child exits before its pidfile is written". Item 23 cases. Orig l.225, 227, 233, 240, 244, 636. |
| D2 | design | major | FIX | Exit cell group. Orig l.227, 244, 258, 636, 637. |
| D3 | design | major | FIX | Violations row "owned child exited after ready" now names the reaper's group SIGKILL under `VERIFY(group_after_reap)`. Orig l.515. |
| D4 | design | major | FIX | Spawn function and Start wiring groups. Verified: `src/proc.rs:11,165` (`anyhow::Result<Child>`); `apps/desktop/src/lib.rs:957-979` (Some branch consumed before setup); `Start` derives `Clone` at `lib.rs:27`. Item 8, item 9 builder. Orig l.635, 638, 654. |
| D5 | design | nit | FIX | Reaper emits `dashboard-exited` only for an `Owned` slot; a `Starting` exit is reported by `failed to start`. §4 Reaper, item 8 `reap`, item 8 `ensure`. Orig l.227, 640. |
| D6 | design | nit | FIX | Item 8 lists `ensure: owned, reserve, probe, start`. Orig l.640. |
| D7 | design | nit | FIX | Verified `std::fs::write` at `src/store.rs:32,54`. §6 and item 17a: mtime read from the handle before and after the read, re-read on mismatch (at most three attempts, then an error). Orig l.290, 701. |
| C1 | comprehension | major | FIX | Exit cell group. Orig l.636. |
| C2 | comprehension | nit | FIX | Slot identity group: `ensure` keeps its own `Arc<Waiter>` clone from reserve and notifies through it; `Exited` with a waiter is a keep-waiting case. §4 lead paragraph, ensure: owned, epilogue step (4). Orig l.233, 244. |
| C3 | comprehension | major | FIX | Verified: `atomic_write_bytes` takes `path.parent()` and never creates it (`src/state.rs:556-569`); no `create_dir` in `apps/desktop/src`. §6 lock bullet, S7, item 9 `ui_state`, item 25 case on a tempdir with no config dir. Orig l.294, 431, 656. |
| C4 | comprehension | major | FIX | Spawn function group. Orig l.654. |
| C5 | comprehension | major | ESCALATE | Behaviour change to launch cleanup's signal admission. Interim, doc only: Goal 7, §4 Launch cleanup step 3 and its note, and a new Risks row state that a group member outliving the leftover leader is not stopped. Orig l.52, 250, 252, 390. |
| C6 | comprehension | major | FIX | `Refusal::NotAdmissible { rule, path }`. Violations row aligned with §4: build command for `binary missing` only. Orig l.511, 642. |
| C7 | comprehension | nit | FIX | Stop path: after SIGKILL, wait up to 1 s more; on expiry log each pgid with an unset cell and return. Orig l.258. |
| C8 | comprehension | nit | FIX | Same edit as D6. Orig l.640. |
| S1 | structural | major | FIX | Exit cell group. Orig l.636. |
| S2 | structural | major | FIX | Wake-ups group. §4 success bullet: replace `Starting` with `Owned` under `inner` and `notify_all`. Item 23 case "the success path wakes a waiter". Orig l.243. |
| S3 | structural | major | FIX | Start wiring group. Verified `AppState` fields at `apps/desktop/src/lib.rs:80-88` keep only `registry`; `App::new` calls `AppState::boot(s)` at `lib.rs:235-236`. Orig l.654, 656. |
| S4 | structural | major | FIX | Spawn function group. Orig l.635. |
| S5 | structural | major | FIX | No-External-slot group. §1 External term, §4 Slots, ensure: owned, ensure: probe, Stop path, item 8 enum. Orig l.146, 225, 233, 235, 256, 260, 261, 636. |
| S6 | structural | nit | FIX | Epilogue step (1): TERM, wait up to 5 s on the exit cell, KILL if still unset and not `Exited`, then wait until set. Orig l.244. |
| S7 | structural | nit | FIX | Launch cleanup step 3: poll the five checks every 50 ms up to the 5 s deadline. Orig l.250. |
| S8 | structural | nit | FIX | Verified `openTab(view, pane)` at `apps/desktop/ui/app.js:124` and sync `openFromHash` at `app.js:148-151`. §5 and item 15: `openTab(view, pane, dash)`; `openFromHash` awaits `loadDashTabs()` for the dash view. Orig l.265, 686. |
| Y1 | style | major | FIX | Exit cell group. Orig l.636. |
| Y2 | style | major | FIX | Spawn function group. Orig l.635. |
| Y3 | style | nit | FIX | `VERIFY(ownership)` binding: `Leftover` requires the lock free, matching the VERIFY table. Orig l.448. |
| Y4 | style | nit | FIX | Same edit as S5. Orig l.235. |
| Y5 | style | nit | FIX | Same edit as S8. Orig l.265. |
| Y6 | style | nit | FIX | Start wiring group. Orig l.654. |
| B1 | codebase | major | FIX | Exit cell group. Orig l.636. |
| B2 | codebase | major | FIX | Verified `#[serde(default, rename = "business")]` at `src/business.rs:36`. Item 1: `servers` gets `default`, `business` gets `default`. Item 19 case: a file with only `[[business.longhand]]` parses. Orig l.582. |
| B3 | codebase | major | FIX | One name, `briefRef{file, mtime}`, filled from `Saved.mtime_ms`. Items 17 and 17b rewritten; §6 already used it. Orig l.291, 696, 706. |
| B4 | codebase | major | FIX | Start wiring group. Orig l.654. |
| B5 | codebase | major | FIX | Spawn function group. Orig l.635. |
| B6 | codebase | major | FIX | Wake-ups group. §4 ensure: probe (External removal and identity refusal now notify), success bullet, Slots Wake-ups rule. Matrix row "waiter while the first open probes"; item 23 case. Orig l.235, 243. |
| B7 | codebase | major | FIX | Verified `apps/desktop/ui/command.js:103` dispatches `businesses-loaded`, handled at `apps/desktop/ui/app.js:1152`. The Problem now names both refresh triggers. Orig l.34. |
| B8 | codebase | nit | FIX | Slot identity group: start step 2 records `Spawned` and starts the reaper in one critical section. Orig l.240. |
| B9 | codebase | nit | FIX | §4 lead paragraph uses `Inner { slots, shutting_down }`; `Waiter { cv: Condvar }` defined in §4 and item 8. Orig l.223, 636. |
| B10 | codebase | nit | FIX | Slot key is the host and port of the dashboard's `url`, equal to the server `addr` when one is declared. §4 lead paragraph, item 8 `ensure`. Orig l.223. |
| B11 | codebase | nit | FIX | Same edit as C6. Orig l.642. |
| R1 | readiness | major | FIX | Exit cell group. Orig l.636. |
| R2 | readiness | major | FIX | Spawn function group; `real_spawn` signature given in §4 and item 8. Orig l.635, 654. |
| R3 | readiness | nit | FIX | Start wiring group. Orig l.654. |
| R4 | readiness | nit | FIX | Slot identity group: `start_time` and `exe` read before the reaper starts; pidfile skipped and epilogue run when the slot is already `Exited`. Orig l.240. |
| R5 | readiness | nit | FIX | Items 20 and 25 gate the no-`ps` cases with `#[cfg(target_os = "macos")]`; item 4 confirms the off-macOS `ps` fallback. Orig l.612, 732, 787. |

## ESCALATE

**C5 (comprehension, major, §4 Launch cleanup step 3).** Launch cleanup cannot stop a member of a leftover's process group that ignores SIGTERM. The leftover leader is not the app's child, so once it exits on SIGTERM launchd reaps it, the five checks fail before the SIGKILL, and no SIGKILL is sent. The member keeps running and may keep the port. The reaper covers the same case on the quit path (`VERIFY(group_after_reap)`); the crash path does not.

Options:

1. Allow one group SIGKILL in launch cleanup when the leader passed all five checks at SIGTERM time, `proc::group_alive(pgid)` is still true, and cleanup still holds `<port>.lock`. This mirrors `VERIFY(group_after_reap)`: a pgid stays reserved while any member lives (`src/proc.rs:234`). It carries the same check-then-kill window as the reaper's group kill, already in Risks.
2. Keep the current admission, which signals only while the leader passes the five checks. The document now states the gap in Goal 7, §4 Launch cleanup and Risks.

Recommendation: option 1. It applies the owner's settled rule (group SIGKILL while `group_alive`) to the one path that lacks it, adds no new kind of signal, and makes Goal 7's "stops the server" true for the leftover's whole group. Applying it means extending `VERIFY(ownership)` or adding a `VERIFY(leftover_group)` checkpoint, a `signal_group` matrix row, and an item 23 case using `serve-with-member` after a simulated crash.
