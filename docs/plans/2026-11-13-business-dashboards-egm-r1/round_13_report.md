# EGM round 13 report

- **Round:** 13
- **Document:** `docs/plans/2026-11-13-business-dashboards-design.md` (Status stays In Review)
- **Reviewers:** design, comprehension, structural, style, codebase, readiness
- **Findings:** 27 (major 8, nit 19)
- **Dispositions:** FIX 27, CONTEST 0, DECLINE 0, ESCALATE 0
- **Verdict:** FINDINGS

Duplicates share one edit and are marked "dup of #n".

| id | reviewer | sev | disposition | edit/evidence |
| --- | --- | --- | --- | --- |
| 1 | design | major | FIX | Verified: the watcher seeds its stamps with `String::new()` and emits on the first tick (`apps/desktop/src/lib.rs:892-899`); `ui.business` is set by `loadPaths` (`apps/desktop/ui/app.js:97`) in the chain at `app.js:1154`. §6 Refresh and item 9 `watch()`: the first tick records the `businesses.toml` and `dashboards.toml` mtimes without emitting `dashboards-changed`. §5 Invalid file and item 14: dash.js ignores `dashboards-changed` until `openFromHash`'s first `loadDashTabs()` resolves, and a successful `dashboards` call clears `#dash-notice`. Violations "invalid `dashboards.toml`" recovery names the clear. New item 25 UI case (g) and E2E list: invalid file shows the notice, fixing it clears the notice and renders tabs. |
| 2 | design | nit | FIX | dup of #3. |
| 3 | comprehension | major | FIX | §5 Opening states one rule: a `dashboard-exited` is dropped when its `addr` differs or an `openDash` token is pending; otherwise, with a Running `owned: true` shown for that addr, dash.js calls `hideDash()` and renders the `exited` card. A reopen on the same addr returns Running only after the old reaper's completion notice, which follows its `emit` (§4 Reaper steps 2 and 5). Item 14 restates it; new item 25 UI case (f), E2E count updated to seven cases. |
| 4 | comprehension | nit | FIX | Item 12 reworded: `dash.js` exposes only `window.loadDashTabs`, `window.openDash` and `window.hideDash`; `sidebar.js` exposes nothing. Agrees with item 13. |
| 5 | structural | major | FIX | `PROBE` row now defines three outcomes, `no_listener`, `no_response`, `response{status, head, matched}`, and the identity retry rule in those terms. Item 1 `ProbeResult` is one of the three; item 7 maps each outcome to its CLI text. §4 ensure: owned points to the `PROBE` rule instead of restating it. Item 19 adds closed-port (`no listener`) and silent-listener (`no response`) cases. |
| 6 | structural | major | FIX | Option A, which makes the existing claim at ensure: start step 2 true. Step 2: if quit has begun, the spawn record sends SIGTERM to the stop target in the same critical section and joins the stop path's collected set. Stop path text covers a spawn returned after `stop_owned` returned: SIGTERM at record, the `ensure`'s failure epilogue runs while the process lives, a survivor keeps its pidfile for launch cleanup. Goal 5 and the §4 invariant name it; new Risks row; matrix row "Lifecycle: spawn recorded after `stop_owned` returned", `signal_group` admitted entry and an item 23 case added. |
| 7 | structural | nit | FIX | Stop path reordered: step 1 marks quit, collects and sends SIGTERM to every Owned slot and Starting slot with a recorded spawn; step 2 then waits on Reserved and spawn-less Starting slots within the same deadline. New matrix row "Lifecycle: quit with an Owned server and a probing slot" and an item 23 case. |
| 8 | structural | nit | FIX | Item 23 adds the identity retry cases: first connection closed without reply then marker on the second gives External after two connections; first reply without marker gives `Identity` after exactly one GET; a silent listener gives `Identity` with `status` `no response` after two attempts. |
| 9 | structural | nit | FIX | `VERIFY(identity)` row: after `no_listener` or `no_response`, `status` is `no listener` or `no response` and `head` is empty. Violations identity row and item 14 give the card wording for that case, replacing "port held by something else". |
| 10 | structural | nit | FIX | Item 23 fake server answers `/health` with the `ready` marker, `/` with marker A and `/sales` with marker B. The two-dashboard case uses `/` and `/sales`; the missing-marker case probes `/sales` with a marker the server never serves. |
| 11 | style | nit | FIX | §5 Startup and restore: "Currently `switchTo` writes `tab` and `pane` only (`apps/desktop/ui/app.js:168`)." |
| 12 | style | nit | FIX | Q1 and §5 capability line now say iframe embedding holds only if the checks pass, otherwise the named fallback applies. |
| 13 | style | nit | FIX | Item 9: `pub fn on_run_event(app: &tauri::AppHandle, event: tauri::RunEvent)`, the callback `.run()` takes (item 10). Desktop depends on `tauri = "2"` (`apps/desktop/Cargo.toml:28`). |
| 14 | style | nit | FIX | Item 13: the clock's title and screen-reader text read `installed for <name>` (§6 scheduled). |
| 15 | style | nit | FIX | Item 23 readiness case now asserts no pidfile remains and `FailedToStart{log_tail}`, matching matrix "Lifecycle: readiness timeout". |
| 16 | codebase | major | FIX | §4 Launch cleanup intro: "every pidfile under a free lock (step 1) is removed without a signal", agreeing with step 1 and Goal 7. New item 23 case: empty value plus a pidfile whose lock another supervisor holds keeps the pidfile and signals nothing. |
| 17 | codebase | major | FIX | Verified: every other `business_cmd` arm calls `first_run` (`src/main.rs:506,539,559,565,590`); `business::load` returns an empty registry for a missing file (`src/business.rs:79`). §3 and item 7: `dashboards` calls no `first_run` and writes nothing; a missing registry is empty, so every key is orphaned; a registry load error prints to stderr and exits 1, as the `?` in every other arm does. Item 21a adds both cases. |
| 18 | codebase | major | FIX | `PROBE` row defines a complete response: a parsed status line followed by EOF, a satisfied `Content-Length`, or the 64 KiB cap; a close or timeout before that, including a close with no status line, is `no_response`; a marker past 64 KiB is `matched: false`. Item 19 adds the past-64-KiB and no-status-line cases. |
| 19 | codebase | nit | FIX | `CONNECT` row and item 8 `real_connect`: 3 s connect timeout, a timeout reports false. §5 Opening's wait list names the 3 s `CONNECT`. |
| 20 | codebase | nit | FIX | Verified `tests/report.rs` exists. Item 17b names it (Modify). |
| 21 | codebase | nit | FIX | Item 17a compares the full-resolution `Metadata::modified` before and after the read, and stores that time as `mtime_ms`. |
| 22 | codebase | nit | FIX | New Risks row accepting a supervisor pid reused by a non-`cos` process: the tick checks liveness only, and the next recompute classifies through `is_cos`. Confidence signals row names both accepted gaps. |
| 23 | readiness | major | FIX | Verified: `ci.yml:30-31` runs bare `cargo clippy --all-targets` and `cargo test`; `Cargo.toml:10-11` has members `[".", "apps/desktop"]` and no `default-members`, so CI covers the root package only. New item 18a: CI runs `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`; Build records the first run against `timeout-minutes: 10` (`ci.yml:11`) and splits the desktop crate into its own job if it does not fit. Item 23 CI claim now conditional on item 18a; Verification Plan names the CI commands. |
| 24 | readiness | nit | FIX | Verified `Taskfile.yml:33` (root only) and `:129` (no `--all-targets`). Item 18 adds `--all-targets` to `desktop-check`; item 9a and Verification Plan static analysis name `cargo clippy --workspace --all-targets -- -D warnings` and `task desktop-check`. |
| 25 | readiness | nit | FIX | dup of #3. |
| 26 | readiness | nit | FIX | dup of #15. |
| 27 | readiness | nit | FIX | Verified direct handler calls at `apps/desktop/tests/ipc.rs:290,299,358,680,691,695`. §5 Origin rule cites all six. |
