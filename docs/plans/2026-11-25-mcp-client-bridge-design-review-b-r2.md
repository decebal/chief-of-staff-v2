---
design: docs/plans/2026-11-25-mcp-client-bridge-design.md
design-sha256: superseded (was e344c79ccb2a0d2b3c26fb27d233ed8fc67b6b2c38f1fd6a8b716482ad6240cf)
verdict: findings
---

## Earlier findings

1. closed: design lines 84-90 (run-child watchdog kills its groups on owner death, exit 75), 128-131 (daemon start sweep), 143-145 (SIGTERM kills and reaps). Owner-death and daemon-restart cases for the run child's own group are covered. The grandchild gap that remains is New finding 1.
2. closed: one argv at design lines 163-166 (`cos --home <h> --workspace <w> --projects <p> --no-open brief`), matching `pipeline::brief_child` at `src/pipeline.rs:690-696`, which passes no `--business`.
3. closed: design lines 226-230 (`brief` returns `RunView`; view calls `run_status` `wait_secs: 45`; `latest_brief` keeps `Report`). `src/dashboard.html:97-108` reads `sc.kind`/`d.report`; see Carried for the non-final `RunView` case.
4. closed: brief lock (design lines 63-64, 80-83) taken by `cos brief` in any process, incl. `launchd/cos.brief.plist` (`ProgramArguments` runs `cos --business <name> brief`). Design line 203 covers it.
5. closed: `## Clients` table at design lines 198-204: each client's daemon-up and daemon-down path, subscribe reconnect every 2 s, `tests/ipc.rs` case at line 616.
6. closed: design lines 224-225 set `brief`/`ask` to `read_only_hint = false`. Current `src/mcp.rs:133` and `:152` are `read_only_hint = true`, which is the state the design changes.
7. closed: design lines 219-223 (`run_status` re-reads record every 500 ms, works with daemon down; `brief` while running returns `existing: true`).
8. closed: §C table design lines 238-243 uses `Layer::Flags`/`Env` as "pinned" (`src/business.rs:337-342`) and matches `resolve` errors (`src/business.rs:391-416`). Remote fixed.
9. closed: design lines 58-59 use `out/run-records/`, separate from `out/runs/` receipts (`src/store.rs:16-17`).
10. closed: design lines 124-126 (duplicate exits 0) and 180-181 (`KeepAlive = { SuccessfulExit = false }`).
11. closed: design lines 91-93 (`cos ask` writes `answer` into record, 256 KiB cap). `src/main.rs:491-494` prints to stdout as today.
12. closed: MCP `resources/subscribe` out of scope (design lines 25-28, 359-360).
13. closed: design lines 333-337 (cold-start link held until ready event; `links.rs` case at line 617).
14. closed: design lines 317-322 add `CFBundleURLTypes` and `lsregister -f` with read-back. Current `apps/desktop/bundle/Info.plist` has no `CFBundleURLTypes`; `Taskfile.yml:120` copies it.
15. closed: reverse `claude:`/`codex:` links removed from §E; out of scope at design lines 25-28.
16. closed: single socket path `confstr(_CS_DARWIN_USER_TEMP_DIR)/cos/daemon.sock`, design lines 65-69.
17. closed: design lines 186-189 (`task install` and `task setup` call `cos daemon install`) and 144-146 (exe-change exit 1 restarts the daemon).
18. closed: design lines 174-176 state the 420 s deadline is a product budget, decided in writing.
19. closed: design line 38 "one pair per machine, for the business `task schedule` names", matching `Taskfile.yml:201-268`.
20. closed: design lines 596-607 use Rust stand-ins in `cos-stand-in`. Shell fixtures `fake-hc.sh` and `fake-claude.sh` exist (`tests/fixtures/`) and are kept, not new.
21. closed: `tool_timeout_sec` 320 at design lines 274, 282-284, matching `Taskfile.yml:90`.
22. closed: design lines 290-291 name `toml_edit` 0.25 as a new direct dependency; `Cargo.lock:3861` holds `toml_edit 0.25.17`. `Cargo.toml` has no `toml_edit` line yet (to add).
23. closed: design line 617 gives `apps/desktop/tests/links.rs`.
24. closed: design lines 178-179 `include_str!`. Path is module-relative, see Carried.
25. closed: acceptance 7 (design lines 651-654) checks `ps -p <pid>` and `ps -g <pgid>`, not `pgrep -x cos`. It is blind to the grandchild gap (New finding 1).

Open earlier findings: 0.

## New findings

1. blocking §A "Sweep" (design 128-131), §Run process 3 (84-90), §Acceptance 7 (651-654), §Testing "Stand-ins" (596-604): claude's process group is not the run child's group, so nothing the owner or sweep kills reaches it. `claude` is spawned by `proc::stream_lines` (`src/claude.rs:198`), which calls `spawn_grouped` with `process_group(0)` (`src/proc.rs:137`, `:166-167`). Its pgid is its own, not the brief child's. The run record stores one `pgid` (design 98-104). The existing kill in `pipeline::brief_child` (`src/pipeline.rs:708`) has the same hole today. Effect: a deadline, `stop`, SIGTERM, or run-child crash leaves the paid `claude` process alive. Acceptance 7 passes anyway because it checks only the run pgid. The stand-in `run` "spawns a grandchild group (to prove group kills reach it)" would also pass for a wrong reason: a group kill of the child's group does not reach a grandchild in its own group. Fix: when a run owns children, the grandchildren join the run child's group (`process_group(<run pgid>)` instead of 0), so one `kill_group` covers the tree. Or the run child records every pgid it spawns in the record (`groups` list) and the owner and sweep kill each one under `VERIFY(group_identity)`. Then: the stand-in grandchild must be in its own group, as in production, and acceptance 7 must also check `ps -g` for the claude pgid.

2. blocking §E "`cos://` links" (design 323-337), Acceptance 3 (647-649): `window.navigate` `view: run` has no target UI for a brief run. The app's run page covers hires only (Agents tab, `src/agents`, `apps/desktop/src/lib.rs:1085-1124` hire actions). `BriefRun` (`apps/desktop/src/lib.rs:1454-1465`) is in-memory per window and has no run id. Acceptance 3 ("opens the app on that run, including when the app was closed") cannot be checked. Fix: define the brief-run view: fields shown (state, reason, `said`, day link to the brief report, `answer` for `ask`), the Tauri command that returns it (name, args, return shape, from the record by business and run id), its `stateView` states, and its test in `apps/desktop/tests/ipc.rs`.

## Carried to Plan

- Identity encoding: `pid_start`/`owner_start` are strings in the schema (design 101-102, 457-458) and `leader_start` is a string (444-445), but `VERIFY(group_identity)` (542) says `proc::start_time`, which returns `(i64, i64)` (`src/proc.rs:287`). Hires store `ps -o lstart=` strings (`src/agents/mod.rs:1126`, one-second resolution). Pick one encoding and name it.
- Who writes `pid`/`pgid`/`pid_start` and when. The sweep depends on `pgid`; design 110-113 assigns only state writes. Say owner writes them right after spawn.
- Lock-losing `cos brief` child: design 76-83 writes `running` before the lock step, and exit 73 has no final state, so the record reads `lost` on reconcile. Also two `start brief` requests racing both spawn; the second child exits 73 and the daemon has already replied `{"run"}`. Specify the state and reply (or take the lock in the daemon before spawn).
- Reconcile race: a reader that read `running`, then sees the pid dead, can overwrite a `done` written in between. Re-read the record before writing `lost`. Also define a `queued` record with `pid` null (owner died between `queued` and spawn).
- Test that cannot fail: the daemon-restart sweep. The run child's watchdog (≤ 1 s) kills the groups first, so the sweep never has anything to kill and `tests/daemon.rs` "SIGKILL + restart" passes whether or not the sweep exists. Add a case with no `COS_OWNER` on the run child.
- Owner-gone exit 75 collides with `DEFERRED_EXIT = 75` (`src/main.rs:352`, `:362`) and `BriefOutcome.deferred` (`src/pipeline.rs:714`). Use a different code.
- MCP App view `accept` (`src/dashboard.html:97-108`): a non-final `RunView` has no `report`, and `renderBrief` reads `b.coverage` and `b.headline` (`:124-135`), so it throws. State that the view stores only final `RunView`s and renders "Running…" otherwise.
- Tauri command shapes are missing: `connect_status`, `connect_apply`, `business_make_default`, the brief-run view command (New finding 2), and the `record → BriefRun` mapping used by `brief-changed`.
- `cos connect status --json` keys are not given (only a table). Define "green" (is the `daemon` row required?).
- Remote argument rejection is new code: `call_tool` (`src/mcp.rs:412-425`) only checks `allowed()`. `list_tools` (`:399-410`) filters one router, so adding `business?` to local tools needs a per-mode tool list or schema filter. Say so under §C.
- Watcher for the daemon `subscribe` and for the app freshness: no `notify` crate in either `Cargo.toml`. Name the mechanism (poll, or add a crate).
- `cos-stand-in`: declare it as `[[bin]]` with `path = "tests/stand_in/main.rs"`. `cargo install` or `task install` would otherwise install it, and cargo auto-discovers `tests/<name>/main.rs` as a test target.
- `include_str!("../launchd/cos.daemon.plist")` (design 179) is relative to the module file. Give the module path.
- Two `cos` binaries: the daemon plist and `cos connect` default to the running binary, but `task desktop-app` copies `cos` into the app bundle (`Taskfile.yml:124`). Say which `cos` the daemon, Claude Desktop and Codex use after each install path.
- `task install` runs `cos daemon install`, which refuses while a run is live (design 185). `task install` then fails mid-brief. Say so.
- Goal wording (`brief` lines 32-35: "switches to a business in the app, asks ... for a brief") does not match the window-only decision. Acceptance 6 adds Make default. Update the brief's goal line.
- `answer` `maxLength: 262144` (design 462) counts characters; the cap is 256 KiB of bytes. Use `maxLength` in bytes or state the byte cap.
- `cos ask` record write point: name the `Cmd::Ask` branch (`src/main.rs:491-494`) as where `COS_RUN_ID` is read.
