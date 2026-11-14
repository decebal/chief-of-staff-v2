# EGM round 8

- **Document:** [business dashboards design](../2026-11-13-business-dashboards-design.md)
- **Round:** 8
- **Reviewers:** design, comprehension, structural, style, codebase, readiness
- **Findings:** 24 (critical 0, major 11, nit 13)
- **Dispositions:** FIX 24, CONTEST 0, DECLINE 0, ESCALATE 0
- **Verdict:** FINDINGS
- **Status of the design:** In Review (unchanged)

## Root-cause rewrites

- **Stop target (F1, F3, F11, F13, F18).** Five reports of one defect: the stop path signalled the group of every `Starting` slot, while the epilogue, `VERIFY(ownership)` and the matrix restricted a `pgid != pid` child to its leader. One definition now lives in §4 Slots: `StopTarget::Group(pgid) | Leader(pid)`, fixed at ensure: start step 1 from `proc::pgid(pid)` (`Option`; `None` gives `Leader`). `Spawned`, `Owned`, `Exited` and `reap` carry `target` in place of `pgid`; the epilogue, the stop path (TERM, KILL, `group_alive` poll, log), the reaper's group check, `VERIFY(ownership)`, `VERIFY(group_after_reap)`, `VERIFY(spawn_result)` (`pgid == Some(pid)`), the sink table and the matrix all refer to it.
- **Child exit (F6, F8, F20).** One type, `ChildExit::Code(i32) | Signal(i32)`, built from `siginfo_t` (`CLD_EXITED` to `Code(si_status)`, `CLD_KILLED`/`CLD_DUMPED` to `Signal(si_status)`), replaces `ExitStatus` in `Exited.status` and the exit cell, and fills `DashboardExited.status`. The reaper loops on `waitid` until `si_code` is one of those three.
- **Binary missing (F14, F19).** `Refusal::BinaryMissing { path }` with a fixed Setup Runbook hint; Non-Goals, §4, Violations and item 8 agree.
- **Live supervisors (F12, F24).** `agents::live_supervisors(home)` (item 3) returns the pids of `Live` runs without `reconcile`; §6 Signals, Refresh and item 2 use it, and "died" is `kill(0)` false for a pid in the set kept from the last recompute.
- **Business check under one lock (F9, F21).** `named_st(&AppState, &str)` under one `app.lock()` for `dashboard_open`, `dashboards` and `brief_seen` (§5 step 1, item 9).

## Findings

| id | reviewer | sev | disposition | edit/evidence |
| --- | --- | --- | --- | --- |
| F1 | design | major | FIX | Confirmed at design l.275-278 vs l.262, l.477, l.556. New §4 Slots "Stop target" bullet; Stop path collects `(stop target, exit cell)`, signals the target, polls `group_alive` only for `Group`; `Spawned.target` from `proc::pgid` `Option` (None gives `Leader`); matrix row "quit while a `Leader`-target Starting slot is alive"; item 23 case. |
| F2 | design | major | FIX | Confirmed at l.264-267, l.492. Launch cleanup and `VERIFY(pidfile_path)` admit only `name == port.to_string()` for 1..=65535; non-canonical entries are left alone (removal of an unadmitted path would breach `VERIFY(pidfile_path)`, so not removed as the finding suggested); a record that does not parse or whose `addr` port differs from the filename is `Stale`. `VERIFY(ownership)`, `signal_group` matrix row, item 23 cases (`04777.json` with `4777.lock` held, `70000.json`, addr 4778 in `4777.json`). |
| F3 | comprehension | major | FIX | Same defect as F1; same edit. |
| F4 | comprehension | major | FIX | Confirmed: `RENDER` cleared by readiness only (l.527), ready marker generic (l.192). New ensure: start step "identity after readiness" runs `VERIFY(identity)` before `Owned`; control room exits on bind failure (`control-room/src/server.rs:11-12`). Sink table, Violations row, `RENDER` matrix refusal, lifecycle row, Risks row with the residual (foreign listener carrying the marker), item 8 `ensure`, item 23 case with new fake mode `<self> idle <addr>`, §5 latency text. |
| F5 | comprehension | nit | FIX | `ensure(dashboards.server_of(d), d)` at §5 step 3, matching item 1 l.621. |
| F6 | comprehension | nit | FIX | `ChildExit` (root-cause rewrite above). |
| F7 | comprehension | nit | FIX | Confirmed `openTab` has no hook (`apps/desktop/ui/app.js:124-142`). `window.hideDash()` exported by dash.js, called by `openTab` for any view other than `dash`: §5, items 14 and 15. |
| F8 | structural | major | FIX | Reaper loops on `waitid` until `CLD_EXITED`/`CLD_KILLED`/`CLD_DUMPED`, citing golang/go#19314 (upstream report, not reproduced here); `ChildExit` conversion stated; matrix row and item 23 case for SIGSTOP; Confidence row "Reaper `waitid` on macOS", sand. |
| F9 | structural | nit | FIX | Confirmed `named(cur: &Current)` (`apps/desktop/src/lib.rs:264`) and `get()` locking the same mutex (`:239-244`). `named_st` under one lock in §5 step 1 and item 9, covering `dashboards` and `brief_seen`. |
| F10 | structural | nit | FIX | Risks row for leftover mismatch now lists "rebuilt in place" with the same accepted outcome and the reason (`proc::exe` or `canonicalize` can fail or differ). |
| F11 | style | major | FIX | Same defect as F1; same edit. |
| F12 | style | major | FIX | Confirmed `run_ids` private (`src/agents/mod.rs:509`), `read_json` private (`:758`), `list` reconciles (`:521-529`, `:502-506`), `Counts` derives `Copy` (`:552`). New `agents::live_supervisors` in item 3; item 2 and §6 use it; item 20 case. |
| F13 | codebase | major | FIX | Same defect as F1; same edit. |
| F14 | codebase | major | FIX | Option (c) with a fixed hint: `BinaryMissing { path }`; l.58, §4 ensure: start, Violations, item 8. |
| F15 | codebase | nit | FIX | Confirmed `Taskfile.yml:200` and `launchd/cos.brief.plist:16`. A key the plist lacks uses launchd's environment: no `XDG_CONFIG_HOME`, `HOME` from the app. |
| F16 | codebase | nit | FIX | Rule defined as: after trim, not matching `^<[^<>]*>$`. Refuses `<title>`, `<div class=x>` and the placeholder `<measured in Build>`; admits both control-room markers. §3, Risks, item 19. |
| F17 | codebase | nit | FIX | `ensure` truncates the log before calling the `SpawnFn` (new `STORE(server_log)` step in §4); `real_spawn` only appends; ISA instruction row; item 23 log case checks truncation with the fake spawn. |
| F18 | readiness | major | FIX | Same defect as F1; same edit. |
| F19 | readiness | major | FIX | Same as F14; same edit. |
| F20 | readiness | nit | FIX | Same as F6; `ChildExit`. |
| F21 | readiness | nit | FIX | Same as F9; `named_st`. |
| F22 | readiness | nit | FIX | Confirmed `report::data` returns `Value` (`src/report.rs:36`) and is called at `apps/desktop/src/lib.rs:451`. `hostBrief` insertion moved to item 9; item 17b and item 17 point to item 9. |
| F23 | readiness | nit | FIX | One restore rule in §5, the switch bullet and item 15: a `dash` name the app business does not declare opens Today. |
| F24 | readiness | nit | FIX | `supervisors` holds only `Live` pids; "died" defined as `kill(0)` false for a pid in the set kept from the last recompute; the recompute drops the pid of a run that turned Lost (§6 Refresh). |
