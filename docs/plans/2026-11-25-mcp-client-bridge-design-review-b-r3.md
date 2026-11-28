---
design: docs/plans/2026-11-25-mcp-client-bridge-design.md
design-sha256: superseded (was 8738f2956e1c847742055e4aab88066fc55c84c7e7fa381e7b1cf5d418980825)
verdict: findings
---

## Earlier findings

Round-2 New findings (from `2026-11-25-mcp-client-bridge-design-review-b-r2.md`):

1. closed: grandchild groups. The mechanism is in the design: groups set and record `groups` (design 68-70, 92-97), owner, watchdog and sweep kill them (98-104, 156-160). Gaps remain and are New findings 1, 3, 4 below.
2. closed: run link now has a view: run panel on Today with fields, Stop, "Brief ready", `answer` (design 360-366), Tauri `run_get` / `run_stop` (368-372), ipc cases (design 685). Still missing: the panel's `stateView` states (Carried).

Round-2 Carried to Plan:

3. closed: identity encoding `"<sec>.<usec>"` (design 66-67) and schema patterns (design 500, 504).
4. closed: writer of `pid`/`pgid`/`start`/`groups` is the run process (design 130-132).
5. closed: lock-losing child: exit 73 → daemon writes `failed`, reason `brief already running: <id>` (design 182). Lock taken before record write (design 82-86).
6. closed: reconcile race: every write re-reads under `<id>.lock` (design 126-128); readers never write (141-145); `queued` with null pid covered (143).
7. closed: sweep test has no `COS_OWNER` (design 643).
8. closed: exit 76 for owner gone (design 100-101). `DEFERRED_EXIT = 75` unchanged (`src/main.rs:352`). 73 and 76 unused in `src/`.
9. closed: App view loops `run_status` `wait_secs: 45` to a final state before rendering (design 257-259). Non-final `RunView` is not rendered.
10. open: Tauri shapes missing for `connect_status`, `connect_apply`, `business_make_default`, `record → BriefRun` mapping (design 231, 334-337). `run_stop` return shape also absent (design 370-371).
11. open: `cos connect status --json` keys and "green" definition absent (design 288-304).
12. open, partial: remote argument refusal stated (design 271, 616). The per-mode tool list for `list_tools` / `call_tool` (`src/mcp.rs:399-425`) not stated (design 273-279).
13. closed: watcher is `dash::stamp` polling (design 185).
14. open: `cos-stand-in` `[[bin]]` declaration and exclusion from install not stated (design 665).
15. open: `include_str!` module path not given (design 212-213).
16. open: which `cos` the daemon plist, Claude Desktop and Codex use after `task install` vs `task desktop-app` (design 288-289, 211; `Taskfile.yml:124`).
17. open: `task install` calls `cos daemon install`, which refuses while a run is live (design 214-215, 217-218). Behaviour not stated.
18. open: brief goal line unchanged: "switches to a business in the app, asks ... for a brief" (`docs/plans/2026-11-25-mcp-client-bridge-brief.md` lines 32-35). Acceptance 6 needs Make default.
19. open: `answer` `"maxLength":262144` counts characters; design says 262,144 bytes (design 508 vs 105-106).
20. closed: `cos ask` write point named (design 90-91).

Tally of the 20 items: closed 11 (1-9, 13, 20); open 8 (10, 11, 14-19); partial 1 (12).

## New findings

1. blocking §The run process 4-5 (design 98-104), §Terms groups (68-70): the run process's own group is in its own set. Record example has `groups[0].pgid` = run pid (design 116). `proc::kill_group` sends SIGKILL to `-pgid` (`src/proc.rs:238`), which includes the run process. Watchdog step 4 ("kills every group in its set, writes `lost` … exits 76") and SIGTERM step 5 ("kills every group … writes `stopped` … exits 143") kill the process before the write. No final state, no exit code. Test at design 642 ("writes lost, exits 76") fails as written. Fix: in steps 4 and 5, write the final state under `<id>.lock` first, then kill the other groups, then exit. Either spawn every child grouped so the run's own group holds only itself, or kill the own group as the last act after the write. Tests assert the record, not the exit code, when the own group was killed.

2. blocking §E run panel and §A `run_stop` (design 368-372), VERIFY `run.stop` (design 609), ISA `run.stop` (477-483): `run_stop` "sends `stop` to the daemon", and the daemon refuses any record whose owner is not `daemon`. The panel shows Stop "while it runs" for every run (design 362-363). Runs owned by `self` (terminal, the 08:30 `cos.brief` launchd job) and by `app` (fallback) are refused whenever the daemon is up. The design also says `run_stop` "stops an app-owned run in fallback", but gives no place where the app keeps the `RunChild` handle. `Brief(Mutex<BriefRun>)` holds no child (`apps/desktop/src/lib.rs:1454-1470`), and `spawn_run` returns the child to its caller (design 187-189). Fix: show Stop only for owner `daemon`, or for `app` owner when this window holds the `RunChild` in a new state field. Define `run_stop`'s return and refusal text for the other owners. Add an ipc case per owner.

3. blocking §A step 5 and §Clients (design 167-169), S13 (design 424): the daemon's hire follower runs checks in the daemon process: `proc::run` for `sh -c <check>` and `git -C` (`src/agents/judge.rs:263`, `:301`; `src/agents/mod.rs:1075`, `:1125`). Those children have no run record, so the groups set (design 92-94) does not reach them, and the sweep (design 156-160) covers run records only. SIGKILL the daemon during a check: the check's group survives up to its 300 s `judge::BUDGET`, unowned. Acceptance 7 (design 720-723) does not check it. Fix: the daemon keeps its own group registry (`{pgid, start}`, same identity rule) for its children, the restart sweep kills those groups, and `tests/daemon.rs` gains a case: SIGKILL the daemon during a judge check, restart, group gone.

4. blocking §Run process 3, §Stopping a run (design 134-139), §Daemon 187-189: nothing acts when a run child dies by signal (SIGKILL, crash) while its owner lives. `RunChild::wait_until` returns on child exit (design 187-189) but the design gives no step for that. The child's grandchild groups (`claude`) keep running, unowned, and the record stays `running` until readers show `lost` (design 141-145). Only the daemon's 420 s deadline (design 203-205) would reach them, and the design does not say the deadline survives the child's exit. This contradicts design 97: "Every other path reaches every group." It is also the reparented-to-init leak the process-ownership rule forbids. Fix: on child exit with no final state, the owner (daemon, app fallback, `cos mcp`) runs the **Stopping a run** steps at once: kill the record's groups under `VERIFY(group_identity)`, write `lost` with reason `run exited`. Add a test: SIGKILL the run child (no daemon death), assert the stand-in grandchild group is gone and the record is final.

## Carried to Plan

- Run panel stateView states are not named (`stateView` requires an icon and art for each region state, design-gate rule). List: loading, running, done, failed, lost, Stop refused.
- `brief-changed` payload (`BriefRun`, `apps/desktop/src/lib.rs:1454-1465`) has no run id. The panel must re-call `run_get` on each event, or the payload gains `run`. Define which.
- Acceptance 2 ("the app shows the run as running") has no named surface for an MCP-started run: the panel opens only from a link. Name `renderBrief` (`apps/desktop/ui/app.js:1400`, `:1770`) as the surface, or say the panel opens on any running run.
- "The same renderer the report uses" for `ask` answers: the report uses markdy inside its frame (`src/report.rs:19`); the shell panel is in `ui/app.js`, which loads `marked` and DOMPurify. Name the one renderer.
- `run_get` is "the same function as MCP `run_status`", but `run_status` does not exist yet and lives in `src/mcp.rs`. Name the function's module and signature so both call it.
- Group removal after reap: the design removes a `pgid` from the set when its child is reaped (design 93-94). A `claude` group whose members outlive the leader is then unrecorded. Keep a group in the record until `group_alive` is false.
- Name the guard type that removes a group on reap. `spawn_grouped` (`src/proc.rs:166`) returns a bare `Child`, and its callers reap in three ways (`wait_until`, `kill_and_reap`, `Drain::finish`). `pipeline::brief_child` (`src/pipeline.rs:703-708`) kills after `wait_until`.
- Acceptance 3 "including when the app was closed" tests the cold-start hold only in `apps/desktop/tests/links.rs`. No test drives the run panel from a link on the mock runtime.
- The record's `groups` cap of 64 (design 501) needs a stated overflow rule.
