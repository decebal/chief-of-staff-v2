# EGM round 10 report

- **Round:** 10
- **Document:** `docs/plans/2026-11-13-business-dashboards-design.md` (Status stays In Review)
- **Reviewers:** design, comprehension, structural, style, codebase, readiness
- **Findings:** 51 (major 18, nit 33)
- **Dispositions:** FIX 51, CONTEST 0, DECLINE 0, ESCALATE 0
- **Verdict:** FINDINGS

Several findings repeat across reviewers. Duplicates share one edit and are marked "dup of #n".

| id | reviewer | sev | disposition | edit/evidence |
| --- | --- | --- | --- | --- |
| 1 | design | major | FIX | Verified: `Triple::label()` returns `default` for the defaults layer (`src/business.rs:347-352`); `named()` compares only the label (`apps/desktop/src/lib.rs:264-271`). `brief_seen` now refuses a window with no registry business before `VERIFY(brief_seen)`. Edited §1 App business, §6 unread bullet, `VERIFY(brief_seen)` binding, VERIFY-before-sink `STORE(ui-state)`, matrix `STORE(ui-state)` refused entry, item 9 no-registry sentence, new item 25 case (defaults window, registered `default`, refused, pair unchanged). |
| 2 | design | major | FIX | Verified: `BusinessRow.app` set at `lib.rs:434`; `switch_business` (`lib.rs:326-361`) writes only the triple. Chose: cache holds rows without `app`; publish sets `app` from the App's business at emit time; sidebar highlights from `row.app`, as ⌘K does (`command.js:28`). Edited §6 Rendering and Shared row cache, item 9 rows bullet, item 13, new item 25 case (switch, third-business stamp change, `app` only on new business). |
| 3 | design | major | FIX | Verified: item 23 had no BinaryMissing, NotAdmissible, lock-held-at-start or platform case; item 19 had no non-loopback `addr` case. Added item 23 cases (missing binary, non-exec file, `cfg(not(target_os="macos"))` platform case, lock held by a second supervisor with a silent port, unmanaged no listener); added item 19 cases for `0.0.0.0:4777` and `localhost:4777`; item 19 now claims the `spawn_server` parse entries and `STORE(server_log)`; Verification "States without a CUJ" corrected. |
| 4 | design | nit | FIX | Item 9 helper now "compares the app business label (`Triple::label`, `src/business.rs:347`)"; the Tauri window label is `main` (`tauri.conf.json:13`). |
| 5 | design | nit | FIX | Verified `Taskfile.yml:210` comment. §7 step 1 and item 18: the comment is reworded to cover `cos.brief` only; a failed `cos.dash` render fails the task with `cos.brief` installed and loaded. |
| 6 | design | nit | FIX | S4 now names the `<app log dir>/servers` directory; `STORE(server_log)` binding states the fixed directory needs no VERIFY. |
| 7 | design | nit | FIX | Item 6: the replaced entry is excluded from the overlap list, as `add` already skips `other == name` (`src/business.rs:279-281`). Item 21 case added; new item 21a (`tests/business_cli.rs`) tests the printed warning. |
| 8 | comprehension | major | FIX | dup of #2. |
| 9 | comprehension | major | FIX | dup of #3; unmanaged-no-listener `NotRunning` case added to item 23 and `spawn_server` matrix row. |
| 10 | comprehension | major | FIX | `Identity` gains `owned` (true only at ensure: owned). §4 ensure: owned/probe/identity-after-readiness, `VERIFY(identity)` binding, Violations identity row (owned card text with no `lsof` or stop step), item 8 refusal kinds, item 14 card text, item 23 case asserts `owned: true`. |
| 11 | comprehension | nit | FIX | Verified `index.html:298-299` loads scripts. Item 12 adds `sidebar.js` and `dash.js` before `app.js`; item 15 no longer loads them. |
| 12 | comprehension | nit | FIX | Item 9 cites `Start` at `:28` (`lib.rs:27` is `#[derive]`). |
| 13 | comprehension | nit | FIX | dup of #4. |
| 14 | comprehension | nit | FIX | Every quit refusal carries hint "the app is quitting": §4 ensure: reserve, lock-take step, after-pidfile step; item 8 kinds. |
| 15 | comprehension | nit | FIX | dup of #6. |
| 16 | comprehension | nit | FIX | §6 Shared row cache: publish fills the cache with every registered business first when not full; §6 Events: `businesses-changed` is always the full list. Item 25 case: `brief_seen` before the first tick emits every row. |
| 17 | comprehension | nit | FIX | Confidence launch-cleanup row now points at the item 23 simulated-leftover cases plus E2E CUJ 7. |
| 18 | comprehension | nit | FIX | Item 25 case names the fixture (unmanaged dashboard on a `TcpListener` with its marker) and expects `Running{url, owned: false}`. |
| 19 | structural | major | FIX | dup of #3; item 23 port-lock case now also asserts the second supervisor cannot take the lock before the reap (Lifecycle row "port lock after the reap"). |
| 20 | structural | major | FIX | Item 23 cases added: quit while Starting before the fake spawn returns (no signal until the spawn record); owned child exit with no member left (asserts `group_alive` false and the gate skipping the kill, stated as the observable form of that entry); SIGSTOP case asserts the pidfile persists until Exited; forged pidfile overwritten by the spawn result. |
| 21 | structural | nit | FIX | Verified launch cleanup step 1 takes the lock (§4 :290). "lock free" reworded to "port lock taken by launch cleanup (step 1)" in `VERIFY(ownership)`, VERIFY-before-sink `signal_group`, matrix `signal_group` admitted, CUJ 7, Goal 7. |
| 22 | structural | nit | FIX | Item 14: `exited` shows `status` and `log_tail`, matching §4 Slots. |
| 23 | structural | nit | FIX | Verified `builder()` returns `Builder<R>` (`lib.rs:957`) and only setup is fallible (`:975-982`). Item 9: the supervisor is built in setup for both branches, with `emit` over `app.handle()`, returning `Err` on failure. |
| 24 | structural | nit | FIX | dup of #6. |
| 25 | structural | nit | FIX | Goal 5 now bounds the guarantee by the 5 s + 1 s budget and states a survivor is logged (§4 Stop path step 6). |
| 26 | structural | nit | FIX | dup of #12. |
| 27 | style | major | FIX | dup of #3. |
| 28 | style | nit | FIX | dup of #23. |
| 29 | style | nit | FIX | Verified `LAUNCHD` is `{{.HOME}}/Library/LaunchAgents` (`Taskfile.yml:5`) and used at `:215,217`. §7 paragraph, steps 1 and 7, and `VERIFY(dash_target)` now name `{{.LAUNCHD}}`. |
| 30 | style | nit | FIX | dup of #12. |
| 31 | codebase | major | FIX | Verified `run_task` (`tests/schedule.rs:125-145`) sets no `DASH_PORT` and `assert_bound_to` (`:156-208`) requires the dash plist and bootstrap. Item 22: `run_task` always passes a free ephemeral `DASH_PORT`; existing callers take the Free path; `failed_render_keeps_live_plists` still holds because the brief render fails first. |
| 32 | codebase | major | FIX | Verified the handler is an inline closure (`lib.rs:963-974`) and tests call `report_response`/`docs_response` directly (`tests/ipc.rs:290,358,1122`). Item 9 adds `pub fn cosreport_response(app, req) -> Response`; §5 Origin rule and item 25 route tests through it. |
| 33 | codebase | major | FIX | dup of #3. |
| 34 | codebase | major | FIX | dup of #20. |
| 35 | codebase | nit | FIX | dup of #5. |
| 36 | codebase | nit | FIX | dup of #16. |
| 37 | codebase | nit | FIX | dup of #14. |
| 38 | codebase | nit | FIX | §3 and item 7: a `<name>` with no entry prints `no dashboards for <name>`, exit 0, not checked against the registry; item 21a tests it. |
| 39 | codebase | nit | FIX | Verified the startup `.catch` only logs (`app.js:1154`). §5 Invalid file and item 14: `loadDashTabs()` catches, writes `#dash-notice`, resolves; routing still runs. |
| 40 | readiness | major | FIX | Launch cleanup step 3 compares the live pgid with the pid and the record's `pgid`; step 4 signals `-pid` only, never the record's `pgid`. Edited Goal 7, `VERIFY(ownership)`, VERIFY-before-sink `signal_group`, matrix `signal_group` refused entry, item 23 case. |
| 41 | readiness | major | FIX | dup of #3 and #9. |
| 42 | readiness | major | FIX | dup of #31. |
| 43 | readiness | major | FIX | Verified `Start.user_home` exists (`lib.rs:31`) and tests set it (`tests/ipc.rs:91`). §6 scheduled: plist at `<Start.user_home>/Library/LaunchAgents/cos.brief.plist`, `None` gives no clock, HOME fallback from `Start.user_home`; item 2 `scheduled` takes `home`; item 9 `Start` bullet. |
| 44 | readiness | major | FIX | §5 Opening and item 14: per-`openDash` sequence token; stale results are dropped and the server keeps running until quit; `dashboards-changed` that removes the open dashboard calls `hideDash()` and shows idle. |
| 45 | readiness | nit | FIX | dup of #23. |
| 46 | readiness | nit | FIX | Item 8: `ensure(&self, server: Option<&Server>, dashboard: &Dashboard)`. |
| 47 | readiness | nit | FIX | §3: `binary`, `addr`, `ready` required on a server; `name`, `url`, `probe` on a dashboard; item 19 case added. |
| 48 | readiness | nit | FIX | Confidence exit-path row and Risks SIGTERM row now point at the E2E CUJ 4 step; item 23 keeps the second-`stop_owned` case. |
| 49 | readiness | nit | FIX | dup of #7 (warning goes to stderr, item 7). |
| 50 | readiness | nit | FIX | Verified hash built by interpolation (`app.js:168`). §5 and item 15: `<d>` is `encodeURIComponent(name)`, read back with `URLSearchParams`. |
| 51 | readiness | nit | FIX | §6 Events: `dashboards-changed` has no payload; `businesses-changed` is the full row list in the `businesses` shape. Item 9 `watch()` references it. |
