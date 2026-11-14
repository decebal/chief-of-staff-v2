# EGM round 4: business dashboards design

- **Round:** 4
- **Document:** [business dashboards design](../2026-11-13-business-dashboards-design.md). Status stays `In Review`.
- **Reviewers:** design, comprehension, structural, style, codebase
- **Findings received:** 45 (major 27, nit 18)
- **Verdict:** FINDINGS
- **Dispositions:** FIX 44, CONTEST 0, DECLINE 0, ESCALATE 1

Line numbers in the table are the reviewer's line numbers in the round 3 text. Evidence checked in code: `src/business.rs:61` (`HOME is unset`), `src/business.rs:125-130` (`update` lock), `apps/desktop/ui/app.js:124-128` (`openTab` loop), `apps/desktop/ui/app.js:166-168` (`switchTo` reads `tab`, writes hash), `apps/desktop/tests/ipc.rs:1-5` (mock runtime), `src/proc.rs:234-236` (pgid reserved while members live).

Several findings name the same defect. Each defect was fixed once, and the duplicate rows point to the row that carries the edit.

## Dispositions

| id | reviewer | sev | disposition | edit / evidence |
| --- | --- | --- | --- | --- |
| d1 | design | major | FIX | `dashboard-exited` has one emitter, the reaper (§4 Slots, Reaper). `Supervisor` holds an `emit` closure and `inner: Arc<Mutex<Inner>>`; `new` takes both (item 8). `watch()` no longer emits it (item 9). Was :225, :611, :613, :625. |
| d2 | design | major | FIX | `shutting_down` moved into `Inner { slots, shutting_down }` under one mutex (item 8). Was :609. |
| d3 | design | major | FIX | Failure epilogue added to `ensure: start` (waiters, lock, pidfile, `Refused` slot). `Slot::Refused` added. ISA lifecycle row "first open fails, second open waits" and item 23 case added. Was :239. |
| d4 | design | major | FIX | `STORE(pidfile)` removal admitted for Owned or Starting after the reaper marks the slot `Exited` (sink table row, ISA matrix row). Was :482, :510. |
| d5 | design | major | FIX | Reaper uses `waitid(WEXITED \| WNOWAIT)`, marks `Exited` under `inner`, then reaps. Every `signal_group` on a slot checks `Exited` under `inner`. Stop path waits for `Spawned` first. Was :251. |
| d6 | design | major | FIX | §7 Timeout: `.new` removed, installed plist kept, `cos.dash` unloaded until login or rerun. S9a and S9d rows, `remove_dash_plist` admission, item 18 order, item 22 third case updated. Was :311. |
| d7 | design | nit | FIX | Reaper removes the pidfile at exit (§4 Reaper). Violations row updated. Was :225. |
| d8 | design | nit | FIX | `ui-state.json` read-modify-write under `ui-state.json.lock`, same pattern as `business::update` (`src/business.rs:125-130`). Was :288. |
| c1 | comprehension | major | FIX | Same edit as d1. Was :225, :625. |
| c2 | comprehension | major | FIX | Same edit as d2. Was :609. |
| c3 | comprehension | major | FIX | `STORE(port_lock)` instruction row, sink row for its open (cleared by `VERIFY(pidfile_path)`), `VERIFY(pidfile_path)` text updated. Was :227, :482. |
| c4 | comprehension | nit | FIX | Plist environment supplies `COS_BUSINESSES`, `XDG_CONFIG_HOME` and `HOME`, with the app environment as fallback. Cites `src/business.rs:61`. Was :290. |
| c5 | comprehension | nit | ESCALATE | Pidfile half applied through d7. Lock half not applied: see E1 below. Was :253. |
| s1 | structural | major | FIX | Quit waits for in-flight `ensure` (`Spawned` record, `Refused` or gone) within the shared 5 s deadline. `Spawned` is recorded right after spawn. ISA row "quit between spawn and recording the child" rewritten. Was :254. |
| s2 | structural | major | FIX | Same edit as d2. Was :609. |
| s3 | structural | major | FIX | Same edit as d4. Was :482. |
| s4 | structural | major | FIX | Payload defined as `{addr, status, log_tail}`. Reaper is the only emitter. Was :225, :613, :625. |
| s5 | structural | nit | FIX | Co-clearances named for `TOOL_CALL(spawn_server)` (probe result, port lock, `start_time` support). `VERIFY(binary_admissible)` refuses rule `platform`. Was :479, :438. |
| s6 | structural | nit | FIX | `VERIFY(dash_target)` admits `<LaunchAgents>/cos.dash.plist.new` exactly. Was :457. |
| s7 | structural | nit | FIX | `VERIFY(port_8790_held)` renamed `VERIFY(dash_port_held)` in every occurrence. Was :454. |
| s8 | structural | nit | FIX | Spawn error runs the failure epilogue: lock released, refusal `failed to start`. Was :236. |
| s9 | structural | nit | FIX | §5 cites `app.js:166-168`. Verified: `:166` reads `tab`, `:168` writes the hash. Was :259. |
| y1 | style | major | FIX | Goal 3 uses the §6 definition of "opened" (rendered, and Today visible then or later in the same page load). Was :46. |
| y2 | style | major | FIX | `Signals` gets `starting: usize`. `agents::count` returns it beside `live` (`src/agents/mod.rs:586`). Refresh rule uses `starting > 0`. Was :299. |
| y3 | style | major | FIX | Same edit as d2. Was :609. |
| y4 | style | major | FIX | Same edit as d1. Was :613. |
| y5 | style | major | FIX | Same edit as d4. Was :482. |
| y6 | style | major | FIX | `.lock` removed from the removal admit set. Lock files are never unlinked (port lock paragraph, `STORE(port_lock)`). Was :419, :452. |
| y7 | style | major | FIX | Signals row in Confidence moved from rock to sand, with the CUJ 8 zombie test as the mover. Was :540. |
| y8 | style | nit | FIX | `gui/$(id -u)` in §7 step 2 and item 18, matching `Taskfile.yml:220`. Was :309. |
| y9 | style | nit | FIX | Same edit as d6. Was :311. |
| y10 | style | nit | FIX | Overview names one config file, two `cos` modules, one subcommand, and kernel checks. Was :30. |
| y11 | style | nit | FIX | Inner type renamed `Spawned`. Was :610. |
| y12 | style | nit | FIX | Rollout lists the changes beyond the sidebar. Was :530. |
| y13 | style | nit | FIX | Verification Plan points to the named cases in items 19 to 25 instead of an unlinked plan. Was :771. |
| k1 | codebase | major | FIX | Same edit as s1. ISA lifecycle row updated. Was :254, :516. |
| k2 | codebase | major | FIX | Same edit as d2. Was :609. |
| k3 | codebase | major | FIX | Same edit as d1. The harness=false servers test gets the recording `emit` closure (item 8, item 23). Was :225, :611. |
| k4 | codebase | major | FIX | `VERIFY(binary_admissible)` refuses on platforms without `start_time` before any spawn. A `VERIFY(spawn_result)` refusal runs the failure epilogue. Violations row and item 23 case added. Was :451, :586. |
| k5 | codebase | major | FIX | S9d added to the side-effect inventory. `VERIFY(dash_target)` admits `.new`. Timeout removes `.new` only. Was :308. |
| k6 | codebase | major | FIX | Item 15 lists the `openTab` edit (`app.js:124-128`) and the `switchTo` hash edit (`app.js:166-168`). Was :659. |
| k7 | codebase | major | FIX | JS-gated cases removed from `apps/desktop/tests/ipc.rs` (item 25; the file header at lines 1-5 says mock runtime). They run as E2E steps in §Verification Plan. Was :753. |
| k8 | codebase | nit | FIX | `DASH_PORT` defined as a test-only seam. The rendered plist runs on the default 8790 (`launchd/cos.dash.plist:6-13`). Was :312. |
| k9 | codebase | nit | FIX | Same edit as s5. Was :479. |
| k10 | codebase | nit | FIX | Same edit as y11. Was :610. |

## ESCALATE

**E1 (c5, comprehension, nit, design.md:253).** Reviewer text, verbatim: "State that the reaper's transition to Exited (or stop_owned) removes the pidfile and drops the lock."

Not applied. The pidfile half is applied through d7. The lock half changes the port-flock lifetime that was approved: design line 15 says the app "holds a per-port lock for as long as it owns the server", and §4 Port lock says "keeps the lock for the life of its `Owned` slot". Releasing the lock when the child exits would let a second app instance start a server on that port before this instance replaces its `Exited` slot. The current text keeps the lock with the `Exited` slot until `ensure` drops that slot. Owner decision needed: release the lock at child exit, or keep it until the slot is dropped.
