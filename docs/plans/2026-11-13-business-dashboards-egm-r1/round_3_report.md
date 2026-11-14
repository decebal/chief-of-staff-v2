# EGM round 3 report: business dashboards design

- **Target:** `docs/plans/2026-11-13-business-dashboards-design.md` (Status stays In Review)
- **Round:** 3
- **Reviewers:** design, comprehension, structural, style, codebase
- **Findings:** 56 (32 major, 24 nit)
- **Dispositions:** 54 FIX, 1 CONTEST, 0 DECLINE, 1 ESCALATE
- **Verdict:** FINDINGS

## Escalated

One finding is not applied. It restates the open decision from round 2 (D4 / S11, stop path survivors of a reaped leader). Fixing it either way changes a rule the owner has not yet chosen.

- **C10** (comprehension, nit, design §4 Stop path, line 249). Verbatim:
  - problem: "Signals only while leader unreaped. Group member that ignores SIGTERM survives once leader exits and is reaped; SIGKILL then skipped. Goal 5 'leaves no process'. Low risk: control room is one process."
  - fix: "State single-process assumption, or check group_alive (src/proc.rs:253) before reaping leader."
  - Option 1 (state the assumption) weakens Goal 5. Option 2 (`group_alive` before reaping) changes the reap order. The round 2 escalation (D4 / S11) still waits for the owner.

## Edits applied to the design

- Goals: Goal 3 counts starting runs in the live count and defines "opened". Goal 8 adds `binary not admissible` and `exited`. Goal 1 is unchanged.
- CUJ 1: six rows (`acme`, `default`, `founding-bets`, `kronos`, `longhand`, `realestate`). No removal step was added.
- CUJ 8: `kill -9` (SIGKILL). SIGTERM is trapped by the supervisor.
- Q1: checks 1, 2 and 4 gate the design; check 3 is recorded. Check 3 now targets a dashboard-frame POST. Check 4 reads the gazette's frame headers. Fallbacks updated.
- Terms and §3: "Unmanaged dashboard" replaces "unmanaged server". The dashboards file location is calibrated to the app and CLI.
- §4 Supervisor: `shutting_down` is set under the slot mutex before the drain. `ensure` checks it after the pidfile is written and `child` is recorded, and stops its own group if set. The readiness poll checks it on each pass. Readiness prose now matches `VERIFY(readiness)`. `main.rs` uses `.build(...).expect(...).run(...)`. Launch cleanup uses a 5 s deadline per leftover. The `Exited` slot emits `dashboard-exited`.
- §5 Dashboard pane: each tab carries `data-dash`; `openTab` sets `aria-current` on one tab. `dashboards` returns a `Result`, and an error shows in `#dash-notice`. `dashboard_open` is async and runs `ensure` in `blocking`. The iframe takes only the URL in `Running`. Hash restore after a business switch is idle and calls no `dashboard_open`. The in-app docs frame does not POST (`src/docs.html:358,365`).
- §6 Signals: the unread brief filename must match `YYYY-MM-DD.json`. The mtime is whole milliseconds since the Unix epoch, from one shared helper. "Opened" means Today is visible when the report renders the brief; `brief_seen` is replayed on the switch to Today. `hostBrief` gates the `brief` message to the live `/` report only. The scheduled-business comparison names both sides of the registry.
- §7 and item 18: `task schedule` runs in seven ordered steps. The old pid comes from `launchctl print`. `DASH_PORT` is a Taskfile variable (default 8790). The `Held` case deletes the `.new` file and the installed `cos.dash.plist`. The `Timeout` case fails and changes neither plist.
- Semantic ISA: S9 is split into S9a (bootout), S9b (plist deletion) and S9c (bootstrap). New instructions `VERIFY(spawn_result)`, `VERIFY(pidfile_path)`, `VERIFY(dash_target)`, `VERIFY(port_8790_held)`, `TOOL_CALL(bootout_dash)`, `TOOL_CALL(bootstrap_dash)`, `TOOL_CALL(remove_dash_plist)`. `VERIFY(ownership)` gains a `Starting` output. `STORE(pidfile)` covers the removal path. Sink table, taint sinks, clearance table and matrix updated, including quit-between-spawn-and-child and readiness-timeout rows.
- Risks and Violations: the Q1 check 3 row is rewritten. The realestate row is narrowed to the shared projects directory. Invalid `dashboards.toml` shows as a notice. An `exited` violation row is added.
- Detailed plan: item 1 (`serde` rename of `servers`); item 2 (`latest_brief`, `mtime_ms`); item 4 (`pgid`, non-macOS fallback); item 8 (`shutting_down`, `Running`, `dashboard-exited`); item 9 (command signatures, events); item 12 (`#dash-notice`); item 14 (`dash.js` exit and idle handling); item 15 (`app.js` opened and idle rules); item 17 (`report.html` gate); new items 17a (`src/store.rs`) and 17b (`src/report.rs`); items 19, 20, 22, 23, 25 and the verification plan updated to match.

## Dispositions

Legend: FIX = applied to the design. CONTEST = finding wrong, evidence given. DECLINE = no change, reason given. ESCALATE = not applied, see Escalated. "same as X" = same edit as row X. IDs are per reviewer, numbered in the order the reviewer listed them.

| id | reviewer | sev | disposition | edit / evidence |
|---|---|---|---|---|
| D1 | design | major | FIX | CUJ 1 (lines 82, 84): six rows, `acme` and `default` stay registered. No removal step, since both are existing registry entries. |
| D2 | design | major | FIX | §6 unread, item 17, item 17b: `hostBrief` gate; archives send no `brief`. Same as C5, K3. |
| D3 | design | major | FIX | Goal 3, §6 "Opened" definition, `brief_seen` only while Today is visible, replay on switch to Today. Item 15. Same as S8, K7. |
| D4 | design | major | FIX | §7 steps 1 to 7, S9a/S9b/S9c, `VERIFY(dash_target)`, `VERIFY(port_8790_held)`, sink and matrix rows; item 18. Same as C3, S4, Y5. |
| D5 | design | major | FIX | Q1 check 4 (gazette `X-Frame-Options`, `frame-ancestors`, CSP) and its fallback. §5 gazette note. |
| D6 | design | nit | FIX | Item 1: `#[serde(rename = "server")]` on `servers`. TOML table is `[server.control-room]` (§3, line 182). |
| D7 | design | nit | FIX | Terms: "Unmanaged dashboard". §4 ensure step: "no `server` declared (unmanaged dashboard)". |
| D8 | design | nit | FIX | §5: after a switch the pane is idle; the hash's `dash` name selects only if the new business has it. Item 15 `openFromHash`. |
| D9 | design | nit | FIX | §4: reaper emits `dashboard-exited`; Goal 8 adds `exited`; Violations row; items 8, 9, 14; matrix row "owned child exits". |
| D10 | design | nit | FIX | Risks, realestate row: shared projects directory only. Stamp directories disjoint (`src/agents/mod.rs:260-262`, `root = home/out/agents`). |
| D11 | design | nit | FIX | §6 unread: names must match `^\d{4}-\d{2}-\d{2}\.json$`. Item 2; item 17a filter in `store::latest`. |
| D12 | design | nit | FIX | §7 step 2: `pid = N` line of `launchctl print gui/$UID/cos.dash` before bootout; empty when not loaded. |
| D13 | design | nit | FIX | §5 and item 9: `dashboard_open` is async, `ensure` in `blocking` (`apps/desktop/src/lib.rs:424` pattern). |
| C1 | comprehension | major | FIX | §4 ensure: check after pidfile and `child` recorded, under the slot mutex. Stop path: `shutting_down` set before drain. Matrix row "quit between spawn and recording the child". Same as S2, Y4. |
| C2 | comprehension | major | FIX | `VERIFY(ownership)` gains `Starting{pid, pgid}` with its checks; `signal_group` sink row; matrix rows. Same as S3. |
| C3 | comprehension | major | FIX | Same as D4. |
| C4 | comprehension | major | FIX | `mtime` is whole ms since the Unix epoch: §6, `VERIFY(brief_seen)`, `STORE(ui-state)`, item 2 `mtime_ms`, item 20 test. Same as S7, Y2. |
| C5 | comprehension | major | FIX | `briefRef` in report data; `hostBrief` set only by `report_response` for `/` (`apps/desktop/src/lib.rs:451`); `build` leaves it unset (`src/report.rs:24-31`). Item 17b test. Same as D2, K3. |
| C6 | comprehension | nit | FIX | §7 and item 18 now agree: holder printed, `cos.dash` skipped, pid source named. Same as K5, Y6, S13. |
| C7 | comprehension | nit | FIX | Q1 check 3 rewritten; §5 Origin bullet notes no in-app POST (`src/docs.html:358,365`); Risks row rewritten. |
| C8 | comprehension | nit | FIX | §5: `data-dash` per tab, `openTab` sets `aria-current` on one tab, dash.js binds its own clicks. Same as S6, K4, Y3. |
| C9 | comprehension | nit | FIX | Item 4: `pub fn pgid(pid)` via `libc::getpgid` for the `pgid == pid` check (`:547` plan, `:243` design). |
| C10 | comprehension | nit | ESCALATE | No edit. Same open decision as round 2 D4 / S11. Verbatim text in Escalated. |
| C11 | comprehension | nit | FIX | §4 readiness prose: "no exit before the first probe and after the second", matching `VERIFY(readiness)` binding. |
| C12 | comprehension | nit | FIX | §5 and item 14: iframe `src` set only to the `url` in `Running`; dash.js uses no other URL. |
| S1 | structural | major | FIX | §4 stop path (line 247): `.build(...).expect(...).run(...)`, matching item 10 (`apps/desktop/src/main.rs:3`). |
| S2 | structural | major | FIX | Same as C1. |
| S3 | structural | major | FIX | Same as C2. |
| S4 | structural | major | FIX | Same as D4. |
| S5 | structural | major | FIX | `VERIFY(spawn_result)` and `VERIFY(pidfile_path)` added to instructions; `STORE(pidfile)` write and removal rows in sink table; matrix rows. |
| S6 | structural | major | FIX | Same as C8. |
| S7 | structural | major | FIX | Same as C4. |
| S8 | structural | major | FIX | Same as D3. |
| S9 | structural | major | FIX | §5: `dashboards` returns `Result<Vec<DashboardRow>, String>`; `#dash-notice`; Violations row; items 9 and 12. |
| S10 | structural | major | FIX | Taskfile variable `DASH_PORT` (default 8790), §7 step 5, item 18; item 22 test uses a free ephemeral port passed as `DASH_PORT`. Same as K6. |
| S11 | structural | nit | FIX | Item 4: off macOS, `exe` falls back to `ps -o comm=` (the lookup `is_cos` uses today, `src/agents/mod.rs:794-801`); `start_time` returns `None`, so cleanup signals nothing. |
| S12 | structural | nit | FIX | Goal 8 adds `binary not admissible` (`:581` refusal). |
| S13 | structural | nit | FIX | Same as C6. |
| S14 | structural | nit | FIX | §4 launch cleanup: "a 5 s deadline per leftover". |
| Y1 | style | major | FIX | Same as D1. |
| Y2 | style | major | FIX | Same as C4. |
| Y3 | style | major | FIX | Same as C8. |
| Y4 | style | major | FIX | Same as C1. |
| Y5 | style | major | FIX | Same as D4. |
| Y6 | style | nit | FIX | Same as C6. |
| Y7 | style | nit | FIX | §6 scheduled: app side is `st.registry` (canonicalised); plist side is `business::location_from` with the plist's environment. |
| Y8 | style | nit | FIX | §3: "Without `COS_DASHBOARDS`, the app reads the dashboards file beside the registry it started with; `COS_DASHBOARDS` overrides the path in both". |
| K1 | codebase | major | FIX | CUJ 8 and item 25: SIGKILL (`kill -9`). SIGTERM is trapped and writes a terminal outcome (`src/agents/supervise.rs:30-40`, cited from the finding; not re-read). |
| K2 | codebase | major | FIX | `store::latest` opens the chosen JSON once and takes mtime from that handle; `Saved` gains `mtime_ms` (`src/store.rs:39-43`, `:64-90` read). Item 17a. |
| K3 | codebase | major | FIX | Same as C5. |
| K4 | codebase | major | FIX | Same as C8. |
| K5 | codebase | major | FIX | Same as C6. |
| K6 | codebase | major | FIX | Same as S10. |
| K7 | codebase | major | FIX | Same as D3. |
| K8 | codebase | nit | CONTEST | The finding says the docs frame POSTs with `Origin: cosreport://localhost`. `src/docs.html:358` sets `embedded = window.parent !== window`, and `:365` POSTs `/docs/organize` only when `!embedded`. The in-app docs frame is embedded, so it posts nothing. Check 3 is rewritten anyway (C7). |
| K9 | codebase | nit | FIX | Goal 3: "starting runs included". `src/agents/mod.rs:586` counts `Liveness::Live \| Liveness::Starting` as live (confirmed by grep). |

## Checks run

- Read the full design (731 lines before edits) and the round 2 report (`2026-11-13-business-dashboards-egm-r1/round_2_report.md`).
- Read in source, read-only: `src/store.rs:39-43, 64-90`, `src/agents/mod.rs:255-266, 586`, `src/docs.html:356-366` (via grep).
- Not re-read: `src/agents/supervise.rs:30-40`, `control-room/src/server.rs:247`, `src/proc.rs:243-253`, `apps/desktop/ui/app.js` line numbers, `Taskfile.yml` line numbers. Their citations come from the round 3 findings and were carried into the design as stated.
- The gazette headers (Q1 check 4) were not fetched. The check is a Build step, and this round made no network request.
- No build, test, git or cargo command was run. Files edited: the design document and this report only.
