# EGM round 2 report: business dashboards design

- **Target:** `docs/plans/2026-11-13-business-dashboards-design.md` (Status stays In Review)
- **Round:** 2
- **Reviewers:** design, comprehension, structural, style, codebase, applycheck
- **Findings:** 71 (35 major, 36 nit)
- **Dispositions:** 66 FIX, 1 CONTEST, 2 DECLINE, 2 ESCALATE (the two ESCALATE items are one decision, stop path rule, listed as D4 and S11)
- **Verdict:** FINDINGS

## Escalated

The stop path (design §4, lines 249-250) asks for two incompatible rules. The owner must pick one. Nothing in that rule was changed.

- **D4 / S11:** signal survivors of a reaped leader, or leave them. Option 1 is group SIGKILL while the group is alive. Option 2 states that survivors of a reaped leader are left, and drops "leaves no process" from Goal 5. Option 1 re-opens the pgid-reuse check that round 1 (design-r1-4) removed. Option 2 weakens an approved goal.

## Edits applied to the design

- §1 Terms: registry lookup order with `XDG_CONFIG_HOME`; `lib.rs:110-120`; base-design and brief links on the new term lines; Flows added to views.
- Problem and Overview: Agents-bar `#other-live` wording; fragment fixed.
- §3: dashboards path from the app's registry (`st.registry`), not `business::location()`.
- §4: `Starting` carries the lock and an optional `child`; pidfile `exe` read from `proc::exe(pid)` after spawn; cleanup releases each lock and rechecks before SIGKILL; stop-path citation quotes `src/proc.rs:234-236`; the signalling rule is unchanged (see Escalated).
- §4 Risks: new row for a crash between spawn and pidfile write.
- §5: dashboard tabs are `.app-tab` with `data-view="dash"`; `allow-forms` kept only if the gazette needs it, checked in Build.
- §6: `#other-live` removal list complete (listener, dispatch, CSS); `cos-report-ready` sends `brief` and `mtime`; unread rule defined; archived reports send no brief; `brief_seen` monotonic, emits `businesses-changed`; `Starting` rows recomputed; `dashboards-changed` event; existing emits kept; refusal shown in the sidebar; `scheduled` with no `COS_BUSINESSES`; recycled-pid limit stated.
- §7 and item 18: `cos.dash.plist` is not left installed when 8790 is held; `cos.dash` bootout waits for the old pid before `lsof`. Goal 9 is unchanged.
- ISA: `VERIFY(readiness)`, `VERIFY(log_path)`, `STORE(server_log)`, `VERIFY(brief_seen)` added with closed schemas, sinks list, clearance table and matrix row; spawn binding gains `env` and `exe`; `STORE(pidfile)` and `RENDER` clearances name their VERIFY.
- §Confidence: `Supervisor spawn, ready, stop on exit` moved from rock to sand.
- Items 8 to 25: `Starting` type, `managed` rename, `pub mod servers`, `main.rs` `.build(...).expect(...).run(...)`, `#other-live` removal, `brief_seen(business, file, mtime)`, `dashboards-changed`, test additions for the event path, the plist, and the Starting-to-Lost transition.
- Alternatives B and C: `(U6)` duplicate removed; Alternative C Cons and Reason no longer argue security.
- Side-effect inventory S5 and NFR wording.
- Test 20: `binary plist` case removed.

## Dispositions

Legend: FIX = applied to the design. CONTEST = finding wrong, evidence given. DECLINE = no change, reason given. ESCALATE = not applied, see Escalated. "same as X" = same edit as row X.

| id | reviewer | sev | disposition | edit / evidence |
|---|---|---|---|---|
| D1 | design | major | FIX | §7, item 18, test 22: skip and remove `cos.dash.plist` when 8790 held. `Taskfile.yml:211-217` renders and moves both plists unconditionally; `launchd/cos.dash.plist:19-20` RunAtLoad, KeepAlive. |
| D2 | design | major | FIX | item 15: remove `app.js:1151-1152`, `command.js:103` dispatch, `style.css:51`. `app.js:1152` calls `showOtherLive`; only listener of `businesses-loaded`. |
| D3 | design | major | FIX | item 10: `.build(...).expect(...).run(...)`. `apps/desktop/src/main.rs:3` is `fn main()`, so `?` does not compile. |
| D4 | design | major | ESCALATE | §4 stop path, lines 249-250. Rule conflict, see Escalated. |
| D5 | design | major | FIX | §6 unread, item 17: frame sends `brief` and `mtime`; host never stats; `brief_seen` monotonic; archives send no brief. `store.rs:29-35` rewrites `{day}.json` in place. |
| D6 | design | major | FIX | ISA: `STORE(server_log)`, `VERIFY(log_path)`, sink list, clearance row, matrix row. Inventory S4 at line 402. |
| D7 | design | nit | FIX | §1 Registry: `COS_BUSINESSES`, else `$XDG_CONFIG_HOME/cos/businesses.toml`, else `~/.config/cos/businesses.toml`. `src/business.rs:40-66`. |
| D8 | design | nit | FIX | §1 App business cites `apps/desktop/src/lib.rs:110-120`. `pinned_by` spans 110-120; env list at 112-115. |
| C1 | comprehension | major | FIX | §6 live: "exact, no recycled-pid caveat" replaced with the limit. `src/agents/mod.rs:804-806` checks `file_name == "cos"` only. |
| C2 | comprehension | major | FIX | §6 Refresh: a business with a `Starting` run is recomputed each tick until none is Starting. `src/agents/mod.rs:44`, `:546`. |
| C3 | comprehension | major | FIX | §6 unread: `brief_seen` recomputes the row and emits `businesses-changed`. Item 25: test observes the event. |
| C4 | comprehension | major | FIX | §6 unread: only the live report sends `brief`; `brief_seen` keeps the greater pair. `src/dash.rs:156-166` archive route. |
| C5 | comprehension | major | FIX | §6 Refresh: `dashboards.toml` stamp emits `dashboards-changed`; item 14 `dash.js` listens and calls `loadDashTabs()`. |
| C6 | comprehension | nit | FIX | §4 stop path rationale quotes `src/proc.rs:234-236` ("while any member lives"). Rule unchanged. |
| C7 | comprehension | nit | FIX | same as D3. |
| C8 | comprehension | nit | FIX | same as D7. |
| C9 | comprehension | nit | FIX | §6 Refresh and item 9: `report-changed` and `agents-changed` stay (`apps/desktop/src/lib.rs:891-905`). |
| C10 | comprehension | nit | FIX | §4 Risks: new row for a crash between spawn and pidfile write. Stated gap. |
| S1 | structural | major | FIX | same as D1. |
| S2 | structural | major | FIX | same as C3. |
| S3 | structural | major | FIX | item 8: `Starting { waiter, lock, child: Option<Child> }`; §4 line 224 agrees. `stop_owned` signals only when `child` is set. |
| S4 | structural | major | FIX | §4 spawn: `exe = proc::exe(pid)` after spawn; spawn output `{pid, pgid, start_time, exe}`. Same as K7. |
| S5 | structural | major | FIX | ISA: `VERIFY(readiness)`, `VERIFY(brief_seen)` added; STORE(pidfile), RENDER, STORE(ui-state) clearances name them. |
| S6 | structural | major | FIX | §1, Problem, §6, item 12: `#other-live` is in the Agents bar (`apps/desktop/ui/index.html:180`); `#agent-count` stays (`index.html:19`). |
| S7 | structural | major | FIX | §5 and item 15: `data-view="dash"` on `.app-tab`; `VIEWS` gains `dash`; fallback to Today when new business has no dashboards. `app.js:118,166`. |
| S8 | structural | major | FIX | §3 and item 1: `dashboards::location(registry)`; desktop passes `st.registry` (`apps/desktop/src/lib.rs:32,53`). |
| S9 | structural | major | FIX | §6 Rendering: refusal from `switchTo` (`app.js:163`) or `switch_business` (`lib.rs:329-351`) shown in a status line. |
| S10 | structural | nit | FIX | same as C1. |
| S11 | structural | nit | ESCALATE | Citation part applied as C6. Grandchildren sentence depends on D4. |
| S12 | structural | nit | FIX | §4 launch cleanup: lock released per port; cleanup synchronous in `setup`. |
| S13 | structural | nit | FIX | same as D3. |
| S14 | structural | nit | FIX | same as D7. |
| S15 | structural | nit | FIX | item 9: `DashboardRow{name, url, managed}`. |
| S16 | structural | nit | FIX | ISA inventory S5 Where: adds `cos::dashboards::probe` and `cos business dashboards`. |
| S17 | structural | nit | FIX | §1 views and item 15: Flows added. `app.js:118`, `index.html:20`. |
| S18 | structural | nit | FIX | Overview: "Status signals are computed from file metadata...". |
| S19 | structural | nit | FIX | §Confidence: supervisor row moved to sand; its "what moves it up" names a pending suite. |
| Y1 | style | major | FIX | same as S3. |
| Y2 | style | major | FIX | §6 unread and ISA binding: frame sends the rendered mtime; `VERIFY(brief_seen)` checks existence only. Same as K3. |
| Y3 | style | major | FIX | same as D2. |
| Y4 | style | major | FIX | §7 and item 18: bootout, bounded wait (10 s, 250 ms poll) for old pid, then `lsof`; held case removes installed plist. `Taskfile.yml:219-222`. |
| Y5 | style | nit | FIX | same as D3. |
| Y6 | style | nit | FIX | §4 launch cleanup: the four checks rerun before the SIGKILL as well. Matches Risks row. |
| Y7 | style | nit | FIX | same as C6. |
| Y8 | style | nit | FIX | same as D7. |
| Y9 | style | nit | FIX | §5: `allow-forms` kept only if the live gazette submits a form; Build checks. Control room answers 405 except GET/HEAD (`control-room/src/server.rs:56-60`). |
| Y10 | style | nit | FIX | same as S19. |
| Y11 | style | nit | FIX | same as S18. |
| Y12 | style | nit | FIX | Alternative B: trailing "(U6)" removed. |
| Y13 | style | nit | FIX | NFR Security: "Stated gap:" (singular). |
| K1 | codebase | major | FIX | same as D3. |
| K2 | codebase | major | FIX | same as S3. |
| K3 | codebase | major | FIX | same as Y2. Frame sends mtime; host does not stat for a value. |
| K4 | codebase | major | FIX | same as D1. |
| K5 | codebase | major | FIX | same as C1. Disclosed, not changed. `src/agents/mod.rs:675` SIGKILL gate uses the same `is_cos`. |
| K6 | codebase | nit | FIX | citation as C6. Rule choice in D4. |
| K7 | codebase | nit | FIX | same as S4. |
| K8 | codebase | nit | FIX | item 9: `pub mod servers;` added. |
| K9 | codebase | nit | FIX | §6 Refresh: `businesses.toml` and `dashboards.toml` stamped by their own mtime (`src/dash.rs:181-196` reads directories only). |
| K10 | codebase | nit | FIX | same as D8. |
| K11 | codebase | nit | FIX | same as S17. |
| K12 | codebase | nit | FIX | same as S12. |
| A1 | applycheck | major | CONTEST | Doc line 226 and the ensure order (probe at 231, lock at 235) are the consistent reading. Lock before probe would refuse the second instance at "lock", which contradicts Lifecycle row "second instance: the probe embeds the server" (design line 479). Row ordering left as is. Owner can reverse this in a separate decision. |
| A2 | applycheck | major | FIX | ISA `TOOL_CALL(spawn_server)` in: `{binary, addr, env}`, env inherited from the app (line 236). |
| A3 | applycheck | major | FIX | test 20: "binary plist that names no business" removed (line 652). |
| A4 | applycheck | major | FIX | Alternative C Cons (line 326): security clause removed; Reason no longer points to §8. |
| A5 | applycheck | major | FIX | Term lines 133, 134, 137, 138, 139, 140, 150 link the base design. |
| A6 | applycheck | major | FIX | Line 22 links the brief decisions for D4, D5, D6. |
| A7 | applycheck | nit | FIX | §6 scheduled: a plist with no `COS_BUSINESSES` resolves to the default location (line 283). |
| A8 | applycheck | nit | DECLINE | Doc cites `server.rs:14-22` (accept loop) and `:39-40` (timeouts), which are the exact lines. Not a conflict. |
| A9 | applycheck | nit | DECLINE | Reviewer reported no hits. No change. |

## Checks run

- Read the full design (718 lines before edits) and the round 1 disposition table (`goldfish_review_report.md`).
- Verified in source: `apps/desktop/src/main.rs:3-6`, `apps/desktop/ui/app.js:118,166,1121-1152`, `apps/desktop/ui/command.js:103`, `apps/desktop/ui/index.html:180`, `apps/desktop/ui/style.css:51`, `src/business.rs:40-66`, `src/dash.rs:178-196`, `src/proc.rs:234`, `src/agents/mod.rs:795-806`, `Taskfile.yml:200-232`, `apps/desktop/src/lib.rs:110-120`.
- No build, test, git or cargo command was run. No file other than the design and this report was edited.
