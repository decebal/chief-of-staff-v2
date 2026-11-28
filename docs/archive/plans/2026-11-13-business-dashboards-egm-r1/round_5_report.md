# EGM review, round 5

*   **Document:** [business dashboards design](../../../plans/2026-11-13-business-dashboards-design.md)
*   **Round:** 5
*   **Reviewers:** design, comprehension, structural, style, codebase
*   **Findings:** 33 (15 major, 18 nit)
*   **Verdict:** FINDINGS
*   **Document status:** In Review (unchanged)

## Dispositions

*   FIX: 32 findings. Applied to the design document, with the Semantic ISA and the Detailed Implementation Plan updated to match.
*   ESCALATE: 1 part (S3b). Not applied. Quoted verbatim below.
*   SPLIT: 1 finding (S3). Part (a) FIX, part (b) ESCALATE.
*   CONTEST: 0.
*   DECLINE: 0.

Line numbers in the table refer to the design document before this round's edits.

## Findings

| id | reviewer | sev | disposition | edit / evidence |
| --- | --- | --- | --- | --- |
| D1 | design | major | FIX | §4 Slots, Reaper, ensure: owned, epilogue step (3). `Exited { status, lock: File }` in item 8. The port lock moves into the `Exited` slot and is released when `ensure: owned` or the stop path drops it. Orig l.227, 233, 243, 620. |
| D2 | design | major | FIX | §4 Slots and `ensure: reserve`. `Reserved(Arc<Waiter>)` is inserted under `inner` at lookup, before probe, lock or spawn. The probe runs outside `inner`. Orig l.223, 225, 234. |
| D3 | design | major | FIX | Item 8 `SpawnFn` is `Box<dyn Fn(&Server, &Path) -> io::Result<SpawnResult> + Send + Sync>`. `SpawnResult { child: Child }`. The app uses `servers::real_spawn`, which calls `proc::spawn_grouped` (`src/proc.rs:165`). Orig l.619, 621, 637. |
| D4 | design | major | FIX | Item 9: `Start` gains `data_dir`, `log_dir`, `ui_state_file`, filled from `app.path()` in `from_env` and from a tempdir in tests. Evidence: `apps/desktop/src/lib.rs:28` (`Start`), `:54` (`app_config_dir`), `apps/desktop/tests/ipc.rs:86-108`. Orig l.637, 639. |
| D5 | design | nit | FIX | Overview and Goal 3 now say "On macOS". The off-macOS `ps` fallback is stated in item 4. Orig l.30, 44, 596. |
| D6 | design | nit | FIX | §5: dash.js compares the `dashboard-exited` payload `addr` with the open dashboard URL host and port, and ignores other addrs. Orig l.664. |
| D7 | design | nit | FIX | §1 App business: a pinned window with no registry entry has no dashboards, and `dashboards` returns an empty list. Evidence: `apps/desktop/src/lib.rs:110-120`. Orig l.263. |
| D8 | design | nit | FIX | S3 adds the `servers/` directory. S7 adds `<ui_state_file>.lock`. Orig l.292, 427. |
| C1 | comprehension | major | FIX | `ensure: reserve`, the `shutting_down` check, the `Reserved` insert and lock-take share one `inner` critical section. The stop-path text covers reservation to `Spawned`. Lifecycle rows added for quit before lock-take and for concurrent first opens, in the ISA matrix and item 23. Orig l.223, 225, 238, 258. |
| C2 | comprehension | major | FIX | Same edit as D1. Orig l.227, 620. |
| C3 | comprehension | nit | FIX | Same edit as D3. Orig l.637. |
| C4 | comprehension | nit | FIX | `Refusal::StaleBusiness { message }` added. `dashboard_open` maps the `named()` error to it. Orig l.625, 634. |
| C5 | comprehension | nit | FIX | `STORE(port_lock)` added to the Sinks list. Orig l.479. |
| S1 | structural | major | FIX | Same edit as D3. Orig l.619. |
| S2 | structural | major | FIX | Same edit as D1. Orig l.227, 620. |
| S3 | structural | major | SPLIT | (a) FIX: launch cleanup takes ports from digits-only `<port>.json` names. `VERIFY(pidfile_path)` admits those names, so the orig l.245 and l.458 contradiction is removed. (b) ESCALATE: the declared-binary requirement. See the ESCALATE section. Orig l.245, 458. |
| S4 | structural | nit | FIX | Same edit as C5. Orig l.479. |
| S5 | structural | nit | FIX | Same edit as D8. Orig l.427, 292. |
| S6 | structural | nit | FIX | Stop path: remove the pidfile only when the reaper has left it and the slot is `Exited`. Orig l.257. |
| Y1 | style | major | FIX | Same edit as D1. Orig l.227, 620. |
| Y2 | style | major | FIX | Same edit as D3. Orig l.619, 637. |
| Y3 | style | nit | FIX | Cite removed from the reaper paragraph. `src/proc.rs:234` says "a pgid stays reserved while any member lives" and does not cover an unreaped leader. Orig l.227. |
| Y4 | style | nit | FIX | Brief schedule cited as `launchd/cos.brief.plist:20-24`. Orig l.138. |
| Y5 | style | nit | FIX | Same edit as D5. Orig l.30. |
| Y6 | style | nit | FIX | `External` is dropped at `ensure: owned`, and every open re-probes. Orig l.233. |
| K1 | codebase | major | FIX | Same edit as D1. Orig l.227, 620. |
| K2 | codebase | major | FIX | Same edit as D3. Orig l.619, 621, 637. |
| K3 | codebase | major | FIX | Sidebar width is `12.5rem`. `tests/design.rs:288-308` rejects pixel lengths other than 1px and 2px. `tokens.css:90-97` has colour tokens only. Orig l.674. |
| K4 | codebase | major | FIX | Added `STORE(render_dash_plist)` (S9d) and `TOOL_CALL(install_dash_plist)` (S9c). Each has an Instructions row, a VERIFY row, a sink entry, and matrix rows. `install_dash_plist` requires `VERIFY(dash_port_held)` = Free and `VERIFY(dash_target)`. Orig l.312, 317, 432. |
| K5 | codebase | nit | FIX | Same edit as C4. Orig l.634. |
| K6 | codebase | nit | FIX | A waiter returns `Running` built from its own dashboard's `url`. Orig l.223. |
| K7 | codebase | nit | FIX | Same edit as C5. Orig l.479. |
| K8 | codebase | nit | FIX | "Only the app's `/` report sends them" (`hostBrief`). The `live` flag at `src/report.rs:35-36` does not gate the message. Orig l.679. |

## ESCALATE (not applied)

Finding S3, part (b), structural, major, `design doc l.458`. Verbatim fix text from the reviewer:

> Require the Leftover `exe` to equal a declared server `binary` (canonicalised) whose `addr` port matches the file name. A pidfile with no matching declared server: remove it and send no signal.

Why it is escalated: today launch cleanup stops any leftover whose four kernel checks match (§4 Launch cleanup; CUJ 7). This text would stop cleanup for a leftover whose server was removed or renamed in `dashboards.toml`, and for a leftover whose executable is not the declared binary. That narrows the approved "crash recovery stops leftover server" behaviour. The reviewer's forged-pidfile concern is also not closed by this check alone, because whoever can write the pidfile can usually write `dashboards.toml` (§8).

## Verification notes

*   Citations checked against the worktree: `src/proc.rs:234` (pgid text), `src/proc.rs:165` (`spawn_grouped` signature), `launchd/cos.brief.plist:20-24`, `tests/design.rs:288-308`, `apps/desktop/ui/tokens.css:90-97`, `apps/desktop/src/lib.rs:28` and `:54`, `apps/desktop/tests/ipc.rs:86-108`, `src/report.rs:35-36`.
*   No build, test or git command was run. Only the design document and this report were written.

## Round 5 escalations resolved

Owner decisions of 2026-11-14 and the hired reviewer's items, applied to the design document. Line numbers refer to the design document after these edits. No build, test or git command was run.

| id | decision or item | where applied |
| --- | --- | --- |
| O1 | Group kill: after the reaper reaps the server leader, if `proc::group_alive(pgid)` (`src/proc.rs:253`) is true, SIGKILL the group (`proc::kill_group`, `src/proc.rs:237`) within the shared 5 s deadline. Goal 5 keeps "leaves no process on the port". | Decision l.15; Goal 5 l.50; §4 Reaper l.227, failure epilogue step (1) l.244, stop path l.258; Risks new row l.390 names the check-then-kill window (a pgid is reserved only while a member lives, `src/proc.rs:234`); PID-reuse row l.389; S2 l.426; `VERIFY(group_after_reap)` l.455; `signal_group` VERIFY row l.493; matrix `signal_group` l.526 and lifecycle row l.541; NFR Process ownership l.554; Confidence l.568; item 4 l.612; item 8 `fn reap` l.637; item 23 fake server mode and test l.751, l.772. |
| O2 | Port lock: released when the reaper reaps the child; the slot becomes `Exited(ExitStatus)` holding no lock, and another instance may then start its own server. | Decision l.15; Terms Owned l.145; §4 Slots l.225, Reaper l.227, Port lock l.229, ensure: owned l.233, epilogue step (3) l.244, stop path l.259; matrix lifecycle row l.540; item 8 `Slot`, `Spawned`, `Owned` l.636 and `fn reap` l.637; item 23 test l.771. Supersedes D1's "lock moves into `Exited`". |
| O3 | Launch cleanup scope: stop a leftover only when its canonicalised exe equals the `binary` of a declared server whose `addr` port matches the pidfile name, plus the pid, start time, exe and pgid checks. No matching declared server: remove the pidfile, send no signal. Resolves the S3(b) escalation. | Decision l.16; Goal 7 l.52; CUJ 7 l.121; Terms Leftover l.147; §4 Launch cleanup l.246-252 (five checks, missing or invalid `dashboards.toml` declares nothing); Risks new row l.391 and PID-reuse row l.389; `VERIFY(ownership)` l.448; Taint propagation l.485; `signal_group` VERIFY row l.493; Violations "ownership not proven" l.513; matrix `signal_group` l.526, crash row l.544, undeclared-leftover row l.542; Confidence l.567; item 8 `launch_cleanup(&self, dashboards: &Dashboards)` l.639; item 9 builder loads `dashboards.toml` for cleanup l.654; item 23 tests l.758-759. |
| a | 8790 guard must not install `cos.dash.plist` into `~/Library/LaunchAgents`. | Held-path removal of the installed plist was already present (§7 step 7, S9b, item 22). Added: the four `for: [brief, dash]` loops (`Taskfile.yml:211-222`; move at `:216-217`) become `for: [brief]`, so only §7 step 6 installs `cos.dash.plist`. §7 l.312; item 18 l.711. |
| b | `plist` crate as a direct dependency that reads binary plists. | Already present: §6 scheduled l.296; item 5 l.617 (`plist = "1.10"`, `Cargo.lock:2437`). No edit. |
| c | `Supervisor::new` must `create_dir_all` the `servers/` pidfile dir and log dir, since `atomic_write_bytes` does not create the parent (`src/state.rs:555-569`). | §4 opening l.223; S3 and S4 l.427-428; item 8 `new` returns `io::Result<Supervisor>` l.638; item 9 builder l.654 propagates with `?` as `apps/desktop/src/lib.rs:977` does; item 23 test l.760. |
| d | S5 "Where" must include `cos::dashboards::probe` used by `cos business dashboards`. | Already present: S5 l.429. No edit. |
