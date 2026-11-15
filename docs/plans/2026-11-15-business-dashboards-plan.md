# Business sidebar and per-business dashboards: plan

Design: docs/plans/2026-11-13-business-dashboards-design.md (approved, sha256 392f535177abe0d4c7f63a98c754b175cf70a70c4d5a94139a8fee16b685c3a4).

Epic: t-67a718. Branch: `feat/business-dashboards` (PR #27). Review: [design review](2026-11-13-business-dashboards-design-review.md), whose two "Carried into Build" items set the build order: the Q1 spike (US-001) comes first and every embedding story depends on it, and every Lifecycle row of the ISA test matrix is a test in `apps/desktop/tests/servers.rs` before the supervisor stories close.

An embedding story is one that sets or shows the dashboard frame: US-033 and the UI stories after it (US-034, US-045). The backend stories (the supervisor, `dashboard_open`, the Origin rule) are not embedding stories and do not depend on US-001: each fallback the design names (Alternative F, open in the browser; Alternative E, a child webview) changes only how the URL that `ensure` verified is shown (design Open Questions, Q1 fallbacks), and check 3 records the `Origin` header without gating the design (`origin_allowed` refuses `null` either way).

Shared files between stories that are ready at the same time: US-009 and US-013 both edit `Taskfile.yml`. Build gives them one worktree in sequence or assigns the file to one of them.

How stories cite the design:

- **Design items** are the numbered entries of the design's Detailed Implementation Plan. An item with several bullets is placed bullet by bullet, cited as `item 9 (Start)` or `item 23: <case>`. Every bullet lands in exactly one story.
- **Test files** (items 19 to 25) are owned by the story that creates or first changes the file. Later stories add the named cases to that file; each case is listed in exactly one story.
- **ISA rows** are the rows of the design's ISA test matrix, by their first-column name. A sink whose row spans several code paths (`signal_group`, `RENDER(iframe src)`) is split across stories, and each story names the entries of the row it owns.

### Quality gates

#### Epic-level (once, when every story is done)
- `task test` passes.
- `task desktop-check` passes, with desktop clippy run with `--all-targets` (US-013).
- CI is green on PR #27, running `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` on `macos-latest`.

#### Story-level
- Backend stories: run the story's scoped test file(s), `cargo test --test <file>` for the `cos` crate or `cargo test -p cos-desktop --test <file>` for the desktop crate, and nothing wider.
- UI stories: build and install the app with `task desktop-app`, drive the real app through the story's states, and capture a screenshot of each state with `pr-proof`.
- Spike and runbook stories: the commands' output is recorded in the story's record file.

### Testing trophy

#### Unit
- [ ] [Logic] `proc::exe`, `proc::start_time`, `proc::pgid`, `proc::signal_group`, `proc::signal_pid` against a live child (file: `tests/proc.rs`)
- [ ] [Logic] `dashboards.toml` parse and every validation rule of design §3 (file: `tests/dashboards.rs`)
- [ ] [Logic] `Counts.starting`, `live_supervisors`, `is_cos` through `proc::exe` (file: `tests/agents.rs`)
- [ ] [Logic] `signals::read`, `mtime_ms`, `scheduled`, latest brief by filename (file: `tests/signals.rs`)
- [ ] [Logic] `business::add` returns nested homes (file: `tests/business.rs`)
- [ ] [Logic] `store::latest` keeps `YYYY-MM-DD.json` names and reads a stable mtime (file: `tests/signals.rs`)
- [ ] [Logic] archived render has no `hostBrief` (file: `tests/report.rs`)

#### Contract
- [ ] [IPC] `Refusal` serialises as `{"kind": …, <fields>}` for every kind, and `Running` as `{url, owned}` (file: `apps/desktop/tests/servers.rs`)
- [ ] [Event] `dashboard-exited` payload is `{addr, status, log_tail}`, emitted only for `owned: true` (file: `apps/desktop/tests/servers.rs`)
- [ ] [IPC] `businesses` rows carry `unread` and `scheduled`; `businesses-changed` carries every registered row in the same shape (file: `apps/desktop/tests/ipc.rs`)
- [ ] [IPC] `dashboards(business)` returns `[{name, url, managed}]`; `dashboard_open` returns `Running` or a `Refusal` with `kind` (file: `apps/desktop/tests/ipc.rs`)
- [ ] [Message] report data carries `briefRef{file, mtime}`; `hostBrief` is set for `/` only (files: `tests/report.rs`, `apps/desktop/tests/ipc.rs`)
- [ ] [CLI] `cos business dashboards` output lines (file: `tests/dashboards.rs`) and exit codes (file: `tests/business_cli.rs`)

#### Integration
- [ ] [Flow] server supervisor: spawn, readiness, identity, reaper, port lock, stop path, launch cleanup, every Lifecycle row (file: `apps/desktop/tests/servers.rs`)
- [ ] [Flow] `task schedule` 8790 guard: render, bootout, Timeout, Free, Held (file: `tests/schedule.rs`)
- [ ] [Flow] watcher tick, shared row cache, `brief_seen`, CUJ 8 (file: `apps/desktop/tests/ipc.rs`)
- [ ] [Flow] `cosreport` Origin rule through `cosreport_response` (file: `apps/desktop/tests/ipc.rs`)

#### Frontend
- [ ] [Component] sidebar rows, signals, highlight, pinned state, refusal status line (built app, `pr-proof` screenshots)
- [ ] [Component] dashboard tabs, `#dash-notice`, refusal cards by `kind`, `exited` card, idle restore (built app, `pr-proof` screenshots)
- [ ] [Component] UI cases (a) to (g) of design item 25 (built app, `pr-proof` screenshots)

#### E2E
- [ ] [CUJ] CUJ 1 to 8 on the built app on the owner's machine, with the `lsof` and `launchctl list` captures of the design's Verification Plan (US-039)

Block merge: every ISA test matrix row (as the test cases each story names), every unit, contract and integration row above, the Frontend rows, and E2E CUJ 1 to 8. Block release: none beyond block merge. Follow-up allowed: none. The `Internal{message}` state, which no test can force, is checked by reading `dash.js` in review (design Verification Plan).

### Stories

#### US-001: Q1 go/no-go spike for loopback iframe embedding [Integration]
As the owner, I want the four Q1 checks run in the built app before any embedding code so that the build follows the iframe path or the named fallback.

Covers: design Open Question Q1 (checks 1 to 4 and the probe measurement); review "Carried into Build" item 2. ISA rows: none.
Blocked by: none. Priority: p1.

- [ ] No production code lands. Any spike change (a temporary `frame-src` entry, a test page) is reverted before close: `git diff origin/feat/business-dashboards --stat` lists only the record file.
- [ ] The control-room binary exists at `~/Projects/founder-mode/portfolio-control-room/target/release/portfolio-control-room`, built with `cargo build --release` in `portfolio-control-room/control-room` (Setup Runbook); `ls -l` output recorded.
- [ ] Check 1 recorded: an `http://127.0.0.1` iframe loads inside the `tauri://localhost` page without a mixed-content block (screenshot and console output).
- [ ] Check 2 recorded: `typeof window.__TAURI__` evaluated inside the frame, with its value.
- [ ] Check 3 recorded: the `Origin` header of a `fetch` POST to `cosreport://localhost/docs/organize` from a page inside a dashboard frame.
- [ ] Check 4 recorded: `curl -sI http://127.0.0.1:8790/` output, with the `X-Frame-Options`, `frame-ancestors` and `Content-Security-Policy` values. If `launchctl list com.longhand.chief-of-staff-dashboard` shows the gazette not loaded, the record reads `Check 4: pending (gazette not loaded)` with that output, and the gazette entry under Manual and external actions applies.
- [ ] The time one identity probe to the control room takes while a frame is loaded, and whether it answers within the 3 s probe timeout, recorded.
- [ ] Verdict line per check: pass, or the fallback the design names (Alternative E or F).
- [ ] All tests green: `test -s docs/plans/2026-11-15-business-dashboards-q1-spike.md` and the file contains a `Verdict:` line.

Mark each item [x] as you complete it. Only close when all are checked.

#### US-002: process helpers read exe, start time and process group without a spawn [Backend]
As the server supervisor, I want kernel-backed process helpers so that ownership is proven by the kernel's view of a pid.

Covers: design item 4. ISA rows: none directly (prerequisite of `signal_group`, `spawn_server`).
Blocked by: none. Priority: p1.

- [ ] Test: `exe(pid)` of a spawned child equals its canonical binary path; `start_time(pid)` is `Some` on macOS and stable across two reads; `pgid(pid)` of a child from `proc::spawn_grouped` equals its pid (file: `tests/proc.rs`), written first and failing.
- [ ] Test: `signal_group(pgid, SIGTERM)` ends a grouped child and its group member; `signal_pid(pid, SIGTERM)` ends the leader only and refuses `pid <= 0` (file: `tests/proc.rs`), written first and failing.
- [ ] `src/proc.rs` gains `exe`, `start_time`, `pgid`, `signal_group`, `signal_pid` beside `alive`; `kill_group` stays SIGKILL only.
- [ ] Off macOS, `exe` falls back to `ps -o comm= -p <pid>` and `start_time` returns `None` (cfg-gated code path present).
- [ ] All tests green: `cargo test --test proc`

Mark each item [x] as you complete it. Only close when all are checked.

#### US-003: agent counts report starting runs and live supervisors without `ps` [Backend]
As the row builder, I want `starting` counts and the live supervisor pids so that a dead supervisor turns its row lost on the next tick.

Covers: design items 3, 3a, 3b. ISA rows: none.
Blocked by: US-002. Priority: p1.

- [ ] Test: one Starting run counts as `starting: 1` and `live: 1`; the four existing `Counts` literals gain `starting` (file: `tests/agents.rs`), written first and failing.
- [ ] Test: `live_supervisors(home)` returns the supervisor pid of a Live run, not the dead pid of a Lost run, and writes no `state.json` (file: `tests/agents.rs`), written first and failing.
- [ ] `is_cos(pid)` compares the file name of `proc::exe(pid)` with `cos`; `comm` is deleted.
- [ ] `agents::slots::lock_exclusive` is `pub`.
- [ ] All tests green: `cargo test --test agents`

Mark each item [x] as you complete it. Only close when all are checked.

#### US-004: `dashboards.toml` location, parse and validation [Backend]
As the owner, I want `dashboards.toml` refused with a key path for every rule in design §3 so that only loopback dashboards with safe probe paths load.

Covers: design items 1 (`location`, `load`, `for_business`/`server_of`/`orphans`), 5, 19 (file owner; every parse and validation bullet). ISA rows: `PROBE` request (all entries); `STORE(server_log)` refused entries (parse refusals of `0.0.0.0:4777`, `localhost:4777`, `127.0.0.1:0`); `RENDER(iframe src)` refused "non-loopback URL in the file"; `spawn_server` refused "relative path" and "non-loopback addr".
Blocked by: none. Priority: p1.

- [ ] Test: each item 19 validation bullet as its own case: non-loopback URL, unknown server, port mismatch, relative binary, unknown key, duplicate dashboard name, invalid business key `Founding_Bets`, short and single-tag markers (`<title>`, `<div class=x>`, `<measured in Build>`) refused and the two control-room markers admitted, duplicate `addr` port naming both keys, unmanaged dashboard on a declared server's port, `probe.path`/`ready.path` with CR LF, space or no leading `/` refused and `/`, `/sales`, `/health` admitted, port 0 in `addr` and URL, `0.0.0.0:4777` and `localhost:4777`, each missing required key (file: `tests/dashboards.rs`), written first and failing.
- [ ] Test: missing file reads as empty; a `[server.*]` table parses; a file with only `[[business.longhand]]` (an unmanaged dashboard, no `server`) parses with no servers; an orphan key is reported by `orphans` (file: `tests/dashboards.rs`), written first and failing.
- [ ] `location(registry, explicit)` reads no environment.
- [ ] `src/lib.rs` exports `dashboards`.
- [ ] All tests green: `cargo test --test dashboards`

Mark each item [x] as you complete it. Only close when all are checked.

#### US-005: identity probe and `cos business dashboards` [Backend]
As the owner, I want `cos business dashboards` to print each dashboard and whether it answers its probe so that I can check the file without opening the app.

Covers: design items 1 (`probe`), 7 (`cos business dashboards` bullet), 19 (probe and CLI-output bullets), 21a (the three `cos business dashboards` bullets). ISA rows: none (`PROBE` is not a sink).
Blocked by: US-004. Priority: p1.

- [ ] Test: `probe` against `TcpListener` fixtures: marker present (`response`, matched), marker only past the first 64 KiB (`matched: false`), close without a status line (`no_response`), closed port (`no_listener`), accept and never reply (`no_response`); the request line is `GET <path> HTTP/1.0` with `Host` and `Connection: close` only (file: `tests/dashboards.rs`), written first and failing.
- [ ] Test: `cos business dashboards` prints `answers`, `answers, marker missing`, `no response`, `no listener` against those fixtures, orphans last (file: `tests/dashboards.rs`), written first and failing.
- [ ] Test: `cos business dashboards <name>` with no entry prints `no dashboards for <name>` and exits 0; with no `businesses.toml` every key is orphaned, exit 0, no `businesses.toml` created; an unparseable `businesses.toml` prints the load error and exits 1; an invalid `dashboards.toml` prints key path and rule and exits 1 before any name lookup (file: `tests/business_cli.rs`), written first and failing.
- [ ] All tests green: `cargo test --test dashboards --test business_cli`

Mark each item [x] as you complete it. Only close when all are checked.

#### US-006: `cos business add` warns about nested homes [Backend]
As the owner, I want a warning when a new business's home is inside another's so that the `realestate`/`founding-bets` overlap is visible.

Covers: design items 6, 7 (`cos business add` warning bullet), 21, 21a (nested-home warning bullet); side effect S8. ISA rows: none.
Blocked by: none. Priority: p2.

- [ ] Test: a nested home registers and `add` returns the outer name; an equal home is still refused; `add` with `replace` on an existing name does not return that name (file: `tests/business.rs`), written first and failing.
- [ ] Test: `cos business add` for a nested home prints `warning: <home> is inside <other>'s home` on stderr and exits 0 (file: `tests/business_cli.rs`), written first and failing.
- [ ] All tests green: `cargo test --test business --test business_cli`

Mark each item [x] as you complete it. Only close when all are checked.

#### US-007: `cos::signals` from metadata and kernel checks [Backend]
As the sidebar, I want live, lost, starting, unread and scheduled computed with no process spawn so that rows refresh on every tick cheaply.

Covers: design items 2, 20 (file owner; every bullet except the `mtime_ms`-equals-payload case, which is in US-008). ISA rows: none (the clock state has no matrix row).
Blocked by: US-003. Priority: p1.

- [ ] Test: live counted through `proc::exe` with no `ps` spawned, using the child-process mechanism of item 20 (re-run of the test binary with a `PATH` whose `ps` writes a marker), macOS only (file: `tests/signals.rs`), written first and failing.
- [ ] Test: lost counted; starting counted and included in live; `supervisors` holds the Live pid and not the Lost pid and writes no `state.json`; latest brief by filename with other names ignored (file: `tests/signals.rs`), written first and failing.
- [ ] Test: `scheduled` returns the business only when the plist's `COS_BUSINESSES` resolves to the same canonical registry; `None` for a legacy plist, a parse error and a mismatched registry (file: `tests/signals.rs`), written first and failing.
- [ ] No case calls `std::env::set_var`.
- [ ] `src/signals.rs` is created and declared in `src/lib.rs`; `Cargo.toml` adds `plist = "1.10"`.
- [ ] All tests green: `cargo test --test signals`

Mark each item [x] as you complete it. Only close when all are checked.

#### US-008: report frame carries the rendered brief's file and mtime [Backend]
As the unread dot, I want the report to name the brief it rendered and that content's mtime so that `brief_seen` stores what the owner saw.

Covers: design items 17, 17a, 17b (with its `tests/report.rs` case), 9 (`report_response` `hostBrief` bullet), 20 (`mtime_ms` of one file equals the payload `mtime`). ISA rows: none.
Blocked by: US-007. Priority: p1.

- [ ] Test: `store::latest` ignores names outside `YYYY-MM-DD.json` and returns `mtime_ms` equal to `signals::mtime_ms` of the file (file: `tests/signals.rs`), written first and failing.
- [ ] Test: with a second thread rewriting the brief in place while `store::latest` reads it, every call either returns content whose `mtime_ms` equals the modified time read after the bytes, or fails as a parse failure after three attempts; no call returns content paired with another write's `mtime_ms` (file: `tests/signals.rs`), written first and failing.
- [ ] Test: report data carries `briefRef{file, mtime}`; an archived render has no `hostBrief` and its page posts no `brief` (file: `tests/report.rs`), written first and failing.
- [ ] Test: `report_response` for `/` renders data with `hostBrief: true` and no other path does (file: `apps/desktop/tests/ipc.rs`), written first and failing.
- [ ] `src/report.html` posts `brief` and `mtime` in `cos-report-ready` only when `hostBrief` is true.
- [ ] All tests green: `cargo test --test signals --test report` and `cargo test -p cos-desktop --test ipc`

Mark each item [x] as you complete it. Only close when all are checked.

#### US-009: `task schedule` renders `cos.dash.plist.new` only [Backend]
As the owner, I want `task schedule` to render `cos.dash` beside the installed plist and handle `cos.brief` alone in the loops so that nothing replaces `cos.dash` before the port check.

Covers: design items 18 (the `for: [brief]` loops, the `DASH_PORT` var, the `:210` comment, the render step), 22 (file owner: `run_task` passes `DASH_PORT`, the `Taskfile.yml` text scan, the existing cases rechecked); design §7 step 1; side effect S9d. ISA rows: `STORE(render_dash_plist)` (all entries); `remove_dash_plist` entry "a failed `cos.dash` render (the installed plist is not removed)".
Blocked by: none. Priority: p1.

- [ ] Test: in a Free run `.new` is written and the installed plist is unchanged until the move; a failed `cos.dash` render removes `.new` only, fails the task, and leaves `cos.brief` installed and loaded (file: `tests/schedule.rs`), written first and failing.
- [ ] Test: a text scan of `Taskfile.yml` asserts the `cos.dash` steps name only label `cos.dash` and the two exact plist paths, the render writes only `{{.LAUNCHD}}/cos.dash.plist.new`, and a failed render removes `.new` (file: `tests/schedule.rs`), written first and failing.
- [ ] `run_task` always passes `DASH_PORT=<free ephemeral port>`; `assert_bound_to` callers, `failed_render_keeps_live_plists` and `unschedule_removes_plists` pass under the new order.
- [ ] All tests green: `cargo test --test schedule`

Mark each item [x] as you complete it. Only close when all are checked.

#### US-010: `task schedule` boots out `cos.dash` and waits for the old pid [Backend]
As the owner, I want the old `cos.dash` pid gone before the port check so that its own listener is never mistaken for the gazette.

Covers: design §7 steps 2 to 4; side effect S9a. ISA rows: `bootout_dash` entries of the `bootout_dash, bootstrap_dash, remove_dash_plist` row: "a label other than `cos.dash`", "a plist path other than the exact ones", "an old pid still alive after 10 s (the installed plist is not removed)", and Timeout keeping `.new` only. The first two are refused by construction (`VERIFY(dash_target)` is the fixed text of the Taskfile steps); their case is the US-009 `Taskfile.yml` text scan, which this story reruns.
Blocked by: US-009. Priority: p1.

- [ ] Test: with a fake `launchctl` whose old pid stays alive, the `cos.brief` steps ran, the task fails, `.new` is removed, and the installed `cos.dash.plist` written before the run is byte-identical afterwards (file: `tests/schedule.rs`), written first and failing.
- [ ] Test: with no job loaded (`print` exits non-zero), the empty old pid skips the wait and the task does not fail (file: `tests/schedule.rs`), written first and failing.
- [ ] The fake `launchctl` prints `pid = N` for `print` while loaded and exits non-zero otherwise, reached through `{{.LAUNCHCTL}}`.
- [ ] All tests green: `cargo test --test schedule`

Mark each item [x] as you complete it. Only close when all are checked.

#### US-011: `task schedule` installs and bootstraps `cos.dash` when 8790 is free [Backend]
As the owner, I want `cos.dash` moved into place and bootstrapped only when no listener holds its port so that it never fights the gazette.

Covers: design §7 steps 5 and 6 (`VERIFY(dash_port_held)` = Free); side effect S9c (two sinks: `install_dash_plist`, `bootstrap_dash`, cleared by the same check in one step). ISA rows: `install_dash_plist` (all entries); `bootstrap_dash` entries of the shared row: "bootstrap only when the port is Free".
Blocked by: US-010. Priority: p1.

- [ ] Test: with no listener on `DASH_PORT`, `cos.dash` is bootstrapped after the old pid is gone and `.new` is gone after the move (file: `tests/schedule.rs`), written first and failing.
- [ ] Test: with a listener on `DASH_PORT`, no move and no bootstrap happen (file: `tests/schedule.rs`), written first and failing.
- [ ] `lsof` non-zero exit with empty output is treated as Free; several LISTEN lines give one `Held` with the first non-old pid.
- [ ] All tests green: `cargo test --test schedule`

Mark each item [x] as you complete it. Only close when all are checked.

#### US-012: `task schedule` skips `cos.dash` and names the holder when 8790 is held [Backend]
As the owner, I want `task schedule` to load `cos.brief`, skip `cos.dash` and print the process holding 8790 so that the gazette keeps its port (Goal 9).

Covers: design §7 step 7, item 18 (`schedule` desc text); side effect S9b. ISA rows: `remove_dash_plist` entries of the shared row: "removal of both files only when the port is Held by a pid other than the old pid, after the old pid exited".
Blocked by: US-011. Priority: p1.

- [ ] Test: with a `TcpListener` on `DASH_PORT`, `task schedule` loads `cos.brief`, skips `cos.dash`, prints the holder's command and pid, and leaves no `cos.dash.plist` in the LaunchAgents directory (file: `tests/schedule.rs`), written first and failing.
- [ ] The `schedule` desc reads "weekday 08:30 brief; `cos.dash` only when port 8790 is free".
- [ ] All tests green: `cargo test --test schedule`

Mark each item [x] as you complete it. Only close when all are checked.

#### US-013: CI runs the desktop tests and `desktop-check` lints all targets [Integration]
As the owner, I want CI and `task desktop-check` to cover `apps/desktop/tests` so that `servers.rs` and `ipc.rs` gate the PR.

Covers: design items 18 (`desktop-check` `--all-targets`), 18a. ISA rows: none.
Blocked by: none. Priority: p2.

- [ ] Test: `rg -n 'cargo clippy --workspace --all-targets -- -D warnings' .github/workflows/ci.yml` and `rg -n 'cargo test --workspace' .github/workflows/ci.yml` each print one line, and `rg -n 'clippy -p cos-desktop --all-targets' Taskfile.yml` prints one line, checked failing before the edit.
- [ ] `.github/workflows/ci.yml` keeps `timeout-minutes: 10`; reading the first workspace CI run is the CI entry under Manual and external actions.
- [ ] All tests green: the three `rg` commands above each print one line.

Mark each item [x] as you complete it. Only close when all are checked.

#### US-014: desktop `Start` carries the new paths and setup manages it [Backend]
As every new desktop command, I want `data_dir`, `log_dir`, `ui_state_file` and `dashboards_file` read once at start so that tests stay hermetic.

Covers: design items 9 (`Start` bullet; the business-check helper bullet; the `builder()` bullet's part that keeps and manages `Start`), 9a, 25 (`start()` helper bullet). ISA rows: none.
Blocked by: none. Priority: p1.

- [ ] Test: a `Start` from the `start()` helper has the three paths under `root` and `dashboards_file: None`, and setup manages that `Start` as state (file: `apps/desktop/tests/ipc.rs`), written first and failing.
- [ ] Test: the business-check helper returns `named()`'s error text for a label that is not the app business and copies under one App-lock acquisition (file: `apps/desktop/tests/ipc.rs`), written first and failing.
- [ ] `Start::from_env` reads `COS_DASHBOARDS` once; `examples/preview.rs` builds with the new fields.
- [ ] All tests green: `cargo test -p cos-desktop --test ipc`

Mark each item [x] as you complete it. Only close when all are checked.

#### US-015: server supervisor harness, constructor and contracts [Backend]
As the supervisor's tests, I want the fake-server binary modes, `Supervisor::new`, `real_connect` and the `Running`/`Refusal` contracts in place so that every lifecycle case has a harness.

Covers: design items 8 (`Supervisor::new`, `real_connect`, Contracts), 9 (export `servers`; the `builder()` bullet's part that builds and manages the supervisor with `emit`, `real_spawn`, `real_connect`), 24, 23 (file owner: the `serve`, `serve-with-member`, `idle` modes, the intermediate-process leftover helper, macOS-only `main`; case "`Supervisor::new` on a fresh tempdir creates `servers/` under the data and log dirs"). ISA rows: none.
Blocked by: US-002, US-004, US-014. Priority: p1.

- [ ] Test: `Supervisor::new` on a fresh tempdir creates `servers/` under the data and log dirs; a directory-creation failure returns `Err` (file: `apps/desktop/tests/servers.rs`), written first and failing.
- [ ] Test (contract): every `Refusal` kind serialises as `{"kind": …, <fields>}` and `Running` as `{url, owned}` (file: `apps/desktop/tests/servers.rs`), written first and failing.
- [ ] `apps/desktop/Cargo.toml` has `[[test]] name = "servers" harness = false` and `libc = "0.2"`; off macOS the test `main` returns at once.
- [ ] Setup returns `Err` when `Supervisor::new` fails; the app's `emit` emits `dashboard-exited` only for `owned: true`.
- [ ] All tests green: `cargo test -p cos-desktop --test servers`

Mark each item [x] as you complete it. Only close when all are checked.

#### US-016: `ensure` reserve and probe: External listeners and unmanaged dashboards [Backend]
As the dashboard pane, I want `ensure` to embed a listener only after its identity probe and to refuse an unmanaged dashboard with nothing listening so that no foreign page is shown.

Covers: design items 8 (`ensure`: reserve, probe, slots Reserved and Refused, waiters, the unmanaged branch of ensure: start), 23 cases: identity retry rule; unmanaged dashboard `NotRunning`; external listener never signalled; second open waiting on a Reserved slot (identity refusal and External). ISA rows: Lifecycle: waiter while the first open probes; `spawn_server` refused "unmanaged dashboard (no `server`) with no listener"; `signal_group` refused "External listener"; `RENDER(iframe src)` server-side refused "listener whose body lacks the marker", admitted "External listener with the marker".
Blocked by: US-015. Priority: p1.

- [ ] Test: identity retry rule: a listener closing its first connection then answering with the marker gives Running (External) after two connections; a first reply without the marker gives `Identity` after one GET; a silent listener gives `Identity` with `status` `no response` and empty `head` after two attempts (file: `apps/desktop/tests/servers.rs`), written first and failing.
- [ ] Test: unmanaged dashboard with nothing listening returns `NotRunning` with hint "its owner (for example launchd) is not running it", nothing spawned (file: `apps/desktop/tests/servers.rs`), written first and failing.
- [ ] Test: a second `ensure` waiting on a Reserved slot returns the same refusal on identity and embeds on External, nothing spawned; an External listener is never signalled (file: `apps/desktop/tests/servers.rs`), written first and failing.
- [ ] All tests green: `cargo test -p cos-desktop --test servers`

Mark each item [x] as you complete it. Only close when all are checked.

#### US-040: `STORE(pidfile)` choke point: one write from the spawn result, one gated removal [Backend]
As the supervisor, I want the pidfile write and removal functions, with their gates, in place before any story writes or removes a pidfile so that every later story goes through them.

Covers: design item 8 (`STORE(pidfile)` choke point: the write function built from the spawn result and the removal function admitted by `VERIFY(pidfile_path)` and `VERIFY(ownership)`). Side effect S3 (pidfile). ISA rows: `STORE(pidfile)` refused "removal of a path that `VERIFY(pidfile_path)` refuses"; the gate behind refused "removal of an Owned or Starting record whose reaper has not marked it Exited" (its lifecycle case is in US-024).
Blocked by: US-015. Priority: p1.

- [ ] Test: the write function takes only a spawn result and a validated `addr` and writes `<data>/servers/<port>.json` with `{pid, pgid, start_time, exe, addr}` from them (file: `apps/desktop/tests/servers.rs`), written first and failing.
- [ ] Test: the removal function refuses `04777.json`, `70000.json`, `abc.json` and a path outside `<data>/servers` (`VERIFY(pidfile_path)`), and refuses an Owned or Starting record not marked Exited; each refusal leaves the file in place (file: `apps/desktop/tests/servers.rs`), written first and failing.
- [ ] All tests green: `cargo test -p cos-desktop --test servers`

Mark each item [x] as you complete it. Only close when all are checked.

#### US-017: `spawn_server` admission: binary checks and the port lock refuse before any spawn [Backend]
As the owner, I want a managed dashboard whose binary is missing, not admissible, or whose port another instance holds refused before any process starts so that a refused open never spawns.

Covers: design items 8 (`binary_admissible`, ensure: start lock-take), 23 cases: missing binary; non-executable file; rule `platform`; second supervisor holding the lock. ISA rows: `spawn_server` refused "missing file", "non-executable file", "port already answering", "port lock held by another instance", "no `start_time` support on the platform".
Blocked by: US-016. Priority: p1.

- [ ] Test: a missing binary gives `BinaryMissing{path}` with the fixed hint, nothing spawned, port lock not left held; a non-executable regular file gives `NotAdmissible{rule, path}`; `binary_admissible(server, false)` gives rule `platform` and `true` admits (file: `apps/desktop/tests/servers.rs`), written first and failing.
- [ ] Test: a second supervisor on the same data dir holding `<port>.lock` with nothing listening gives `NotRunning` "another app instance is starting it; open the dashboard again", nothing spawned; a port already answering is never spawned on (file: `apps/desktop/tests/servers.rs`), written first and failing.
- [ ] All tests green: `cargo test -p cos-desktop --test servers`

Mark each item [x] as you complete it. Only close when all are checked.

#### US-042: `spawn_server`: spawn, record, readiness and the reaper [Backend]
As the owner, I want the first open of a managed dashboard to start exactly one admitted server, record it and reap it so that two opens never start two processes.

Covers: design items 8 (`real_spawn`, ensure: start spawn, spawn steps 1 to 4, readiness, Success; Reaper steps 1, 2, 3 and 5), 23 cases: two concurrent first opens; first spawn writes its pidfile and log under `servers/`. Side effect S1. ISA rows: `spawn_server` admitted "absolute executable, free loopback port, lock free: one process, pidfile written"; Lifecycle: concurrent first opens of one key.
Blocked by: US-017, US-040. Priority: p1.

- [ ] Test: two concurrent first opens give one lock and one spawn, and the second returns Running with the same URL after its own identity probe; the first spawn writes `<data>/servers/<port>.json` through the US-040 write function and `<log>/servers/<port>.log` (file: `apps/desktop/tests/servers.rs`), written first and failing.
- [ ] The reaper uses `waitid(P_PID, pid, WEXITED | WNOWAIT)`, accepts only `CLD_EXITED`, `CLD_KILLED`, `CLD_DUMPED`, and is started in the critical section that records the spawn.
- [ ] All tests green: `cargo test -p cos-desktop --test servers`

Mark each item [x] as you complete it. Only close when all are checked.

#### US-018: `signal_group` in the failure epilogue [Backend]
As the owner, I want every failed start to stop the child it spawned, and only that child, so that no failed server keeps the port.

Covers: design item 8 (`ensure` failure epilogue, `VERIFY(spawn_result)`, `VERIFY(readiness)`), 23 cases: server that never becomes ready; first open fails while a second waits; second open during a failed start's epilogue; child exits at once; spawn whose pgid differs from its pid. Side effect S2 (epilogue path). ISA rows: Lifecycle: readiness timeout; Lifecycle: first open fails, second open waits; Lifecycle: open while a failed start runs its epilogue; Lifecycle: child exits before readiness passes; Lifecycle: spawn result with pgid not equal to pid; `signal_group` refused "group signal to a Starting child whose pgid differs from its pid, from the failure epilogue", admitted "child of this instance not marked Exited, in Starting" and "the leader alone of a Starting child whose pgid differs from its pid"; `RENDER(iframe src)` refused "child exited during readiness".
Blocked by: US-042. Priority: p1.

- [ ] Test: readiness timeout signals the group, removes the pidfile, returns `FailedToStart{log_tail}`; a waiter on that slot returns the same refusal with one spawn (file: `apps/desktop/tests/servers.rs`), written first and failing.
- [ ] Test: with the `emit` closure delaying 200 ms on `owned: false`, a new `ensure` during the epilogue waits and returns the same refusal, no second lock or spawn, no `dashboard-exited` (file: `apps/desktop/tests/servers.rs`), written first and failing.
- [ ] Test: a child that exits at once leaves no pidfile, returns `failed to start` with the log tail, and frees the lock after the reap (file: `apps/desktop/tests/servers.rs`), written first and failing.
- [ ] Test: a child whose pgid differs from its pid: the epilogue signals the leader alone, the reaper skips the group kill, the lock is released after the reap, no pidfile, `failed to start` (file: `apps/desktop/tests/servers.rs`), written first and failing.
- [ ] Test: with the injected spawn function returning a spawn result with `pid <= 0`, with an empty `exe`, and with no `start_time`, each `ensure` returns `failed to start`, logs the failing field, writes no pidfile and releases the lock; for `pid <= 0` nothing is signalled (file: `apps/desktop/tests/servers.rs`), written first and failing.
- [ ] All tests green: `cargo test -p cos-desktop --test servers`

Mark each item [x] as you complete it. Only close when all are checked.

#### US-019: identity after readiness and the owned-exit notice [Backend]
As the owner, I want a spawned server whose page lacks the dashboard's marker stopped and refused, and an owned server's exit shown, so that the pane never shows the wrong page or a dead one.

Covers: design item 8 (ensure: start "Identity after readiness"; Reaper step 4 with `VERIFY(group_after_reap)` skip path), 23 cases: foreign listener answering `ready` before the child binds; fake server whose page lacks the marker; owned child exits emits `dashboard-exited`; `dashboard-exited` payload carries `log_tail` and the reaper removes the pidfile. ISA rows: Lifecycle: foreign listener before the child binds; Lifecycle: owned child exits; `signal_group` refused "reaped leader whose group has no member left"; `RENDER(iframe src)` refused "foreign listener that answers `ready` before the child binds but lacks the dashboard's marker" and "the app's own child whose page passes `ready` but lacks the dashboard's marker"; admitted "owned server ready".
Blocked by: US-018. Priority: p1.

- [ ] Test: with `connect` reporting no listener, a child that never binds and a test listener answering `/health` with the `ready` marker and `/` without the dashboard marker: `Identity` with `spawned: true`, child stopped, no pidfile, listener not signalled (file: `apps/desktop/tests/servers.rs`), written first and failing.
- [ ] Test: a fake server that passes `ready` but lacks the requesting dashboard's marker: `Identity` with `spawned: true`, `owned: false`, child stopped, port free, no pidfile (file: `apps/desktop/tests/servers.rs`), written first and failing.
- [ ] Test: a fake server that passes `ready` and serves the requesting dashboard's marker gives `Running{url, owned: true}`; the slot is Owned, the pidfile is kept, nothing is signalled (file: `apps/desktop/tests/servers.rs`), written first and failing.
- [ ] Test (contract): an owned child that exits makes `emit` receive `owned: true` with `{addr, status, log_tail}`; the slot is Exited, the pidfile removed, `group_alive(pgid)` false and the group kill skipped (file: `apps/desktop/tests/servers.rs`), written first and failing.
- [ ] All tests green: `cargo test -p cos-desktop --test servers`

Mark each item [x] as you complete it. Only close when all are checked.

#### US-020: `STORE(server_log)` truncates and places the server log [Backend]
As the owner, I want each server's log at `<app log dir>/servers/<port>.log`, cut to its last 1 MiB before each spawn, so that the log tail is current and the disk is bounded.

Covers: design item 8 (`VERIFY(log_path)`, `STORE(server_log)` in ensure: start), 23 case: first spawn's log location and 1 MiB truncation before the fake spawn runs. Side effect S4. ISA rows: `STORE(server_log)` admitted entry (refused entries are parse refusals in US-004).
Blocked by: US-042. Priority: p1.

- [ ] Test: a log over 1 MiB written before the spawn is truncated to its last 1 MiB before the fake spawn runs, and the spawn appends to `<log dir>/servers/<port>.log` (file: `apps/desktop/tests/servers.rs`), written first and failing.
- [ ] The log path is built only from the parsed port; the code has no refusal branch for it, because `VERIFY(log_path)` always admits (design ISA table). The refused side of the `STORE(server_log)` row is the US-004 parse cases for `0.0.0.0:4777`, `localhost:4777` and `127.0.0.1:0`: those addresses never load, so they never reach this path.
- [ ] All tests green: `cargo test -p cos-desktop --test servers`

Mark each item [x] as you complete it. Only close when all are checked.

#### US-021: `STORE(port_lock)` is close-on-exec, never unlinked, released after the reap [Backend]
As a second app instance, I want the port lock held until the first instance's reaper finishes so that two instances never run two servers on one port.

Covers: design item 8 (Port lock), 23 cases: `lsof -p <child pid>` lists no `<port>.lock`; the lock keeps its inode across a spawn and a reap; second supervisor cannot take the lock while the first's child lives and takes it after the reap. Side effect S3 (lock). ISA rows: `STORE(port_lock)` (all entries; the launch-cleanup inode leg is in US-025); Lifecycle: port lock after the reap.
Blocked by: US-042. Priority: p1.

- [ ] Test: after a spawn, `lsof -p <child pid>` lists no `<port>.lock` (file: `apps/desktop/tests/servers.rs`), written first and failing.
- [ ] Test: the lock-open function refuses `04777.lock` and `70000.lock` (`VERIFY(pidfile_path)`), creates no file under `servers/` and signals nothing (file: `apps/desktop/tests/servers.rs`), written first and failing.
- [ ] Test: `<port>.lock` keeps its inode across a spawn and a reap (file: `apps/desktop/tests/servers.rs`), written first and failing.
- [ ] Test: while the first supervisor's owned child lives a second supervisor cannot take the lock; after the reap it takes it while the first slot is still Exited and starts its own server (file: `apps/desktop/tests/servers.rs`), written first and failing.
- [ ] All tests green: `cargo test -p cos-desktop --test servers`

Mark each item [x] as you complete it. Only close when all are checked.

#### US-022: `ensure: owned` reuses a running server per dashboard [Backend]
As the owner, I want the Salesroom to reuse the server the Control room started, after its own identity probe, so that one server serves both tabs (CUJ 3).

Covers: design item 8 (ensure: owned: Owned reuse, Exited slot wait up to 5 s, Refused drop, waiting on Reserved/Starting), 23 cases: one process for two dashboards; second dashboard with a marker the server never serves; success path wakes a waiter; reopen right after an owned child exits. ISA rows: Lifecycle: reopen right after an owned child exits; `RENDER(iframe src)` refused "second dashboard on an Owned slot whose page lacks that dashboard's marker", admitted "second dashboard on an Owned slot whose page carries its own marker".
Blocked by: US-042. Priority: p1.

- [ ] Test: Control room then Salesroom start one process; the second Running follows a passing probe of `/sales` with marker B (file: `apps/desktop/tests/servers.rs`), written first and failing.
- [ ] Test: a second dashboard probing a marker the fake server never serves gets `Identity` with `owned: true`; the slot stays Owned, nothing is signalled, the first dashboard still opens (file: `apps/desktop/tests/servers.rs`), written first and failing.
- [ ] Test: the success path wakes a waiter that runs its own identity probe and returns Running for its URL (file: `apps/desktop/tests/servers.rs`), written first and failing.
- [ ] Test: with `emit` delaying 200 ms, a reopen while the slot is Exited and its notice unset waits, then starts one fresh server with no "another app instance" refusal (file: `apps/desktop/tests/servers.rs`), written first and failing.
- [ ] Test: with `emit` delaying past 5 s, a reopen while the slot is Exited and its notice unset returns `NotRunning` with hint "the previous server on this port is still exiting; open the dashboard again" at the 5 s deadline, leaves the slot in place and spawns nothing (file: `apps/desktop/tests/servers.rs`), written first and failing.
- [ ] All tests green: `cargo test -p cos-desktop --test servers`

Mark each item [x] as you complete it. Only close when all are checked.

#### US-023: `stop_owned` on quit stops every owned server [Backend]
As the owner, I want ⌘Q to stop every server the app started, group members included, within the 5 s + 1 s budget so that quitting leaves no process on the port (Goal 5).

Covers: design items 8 (`stop_owned`, Stop path steps 1 to 7, quit refusals at reserve, lock-take and after the pidfile write), 9 (`on_run_event`), 10, 23 cases: `stop_owned` frees the port; second `stop_owned` no-op; quit during readiness; quit before the port lock is taken. Side effect S2 (stop path). ISA rows: Lifecycle: quit during readiness; Lifecycle: quit before the port lock is taken; Lifecycle: second `stop_owned`; `signal_group` admitted "child of this instance not marked Exited, in Owned".
Blocked by: US-018. Priority: p1.

- [ ] Test: `stop_owned` frees the port and leaves no child; a second call sends no signal (file: `apps/desktop/tests/servers.rs`), written first and failing.
- [ ] Test: quit during readiness stops the group within the 5 s deadline; quit while a Reserved probe blocks on the `connect` barrier returns at the deadline with nothing signalled, then the `ensure` returns `NotRunning` "the app is quitting", no spawn, no pidfile, and the test's own `flock` on `<port>.lock` succeeds (file: `apps/desktop/tests/servers.rs`), written first and failing.
- [ ] Test: an `ensure` called after `stop_owned` has returned refuses at reserve with `NotRunning` "the app is quitting"; no slot is inserted, nothing is probed or spawned (file: `apps/desktop/tests/servers.rs`), written first and failing.
- [ ] `apps/desktop/src/main.rs` uses `.build(...).expect(...).run(cos_desktop::on_run_event)`; `on_run_event` calls `stop_owned` on `ExitRequested` and `Exit`.
- [ ] All tests green: `cargo test -p cos-desktop --test servers`

Mark each item [x] as you complete it. Only close when all are checked.

#### US-043: `stop_owned` with several keys, group members and a second instance [Backend]
As the owner, I want quit to stop an Owned server before it waits on a probing slot, to kill a group member that outlives its leader, and to leave another instance's server alone so that quit frees every port it owns and only those.

Covers: design item 8 (Stop path, the order of signals across keys and the reaper's group kill under `stop_owned`), 23 cases: quit with one key Owned and another probing; `serve-with-member` under `stop_owned`; second instance. ISA rows: Lifecycle: quit with an Owned server and a probing slot; Lifecycle: group member outlives the leader on quit; Lifecycle: second instance; `signal_group` admitted "SIGKILL to the group of a reaped leader while `group_alive(pgid)` is true".
Blocked by: US-023. Priority: p1.

- [ ] Test: with one key Owned and another key's probe blocked, the Owned group gets SIGTERM before `stop_owned` waits on the Reserved slot and is gone without a SIGKILL (file: `apps/desktop/tests/servers.rs`), written first and failing.
- [ ] Test: `serve-with-member` under `stop_owned`: the reaper sends SIGKILL to the group after the reap; group gone and port free within the 5 s deadline; a second supervisor that embedded the server as External signals nothing on its own `stop_owned` (file: `apps/desktop/tests/servers.rs`), written first and failing.
- [ ] All tests green: `cargo test -p cos-desktop --test servers`

Mark each item [x] as you complete it. Only close when all are checked.

#### US-024: `stop_owned` races with spawn, reap and stopped servers [Backend]
As the owner, I want quit to handle a spawn mid-record, a reaper mid-finish, a leader-only target and a stopped server so that no ordering leaves a server behind.

Covers: design item 8 (Stop path step 2 and the spawn-record SIGTERM after quit began), 23 cases: quit between spawn and recording; barrier released after `stop_owned` returned; `stop_owned` while a slot is Exited with its notice unset; quit while a leader-target Starting slot is alive; SIGSTOP to the owned fake server. ISA rows: Lifecycle: quit between spawn and recording the child; Lifecycle: spawn recorded after `stop_owned` returned; Lifecycle: quit while a reaper finishes; Lifecycle: quit while a leader-target Starting slot is alive; Lifecycle: stopped server; `signal_group` refused "Starting slot with no recorded spawn" and "group signal to a Starting child whose pgid differs from its pid, from `stop_owned`", admitted "SIGTERM at the spawn record of a child recorded after quit began"; `STORE(pidfile)` refused "removal of an Owned or Starting record whose reaper has not marked it Exited".
Blocked by: US-023. Priority: p1.

- [ ] Test: the fake spawn waits on a barrier; `stop_owned` signals nothing until the spawn is recorded, the record sends SIGTERM, the group is gone within 5 s, no pidfile; with the barrier released after `stop_owned` returned, the `ensure` returns `NotRunning` "the app is quitting" and the group and pidfile are gone (file: `apps/desktop/tests/servers.rs`), written first and failing.
- [ ] Test: `stop_owned` while a slot is Exited with its notice unset waits within the deadline and returns with the group gone and the port free (file: `apps/desktop/tests/servers.rs`), written first and failing.
- [ ] Test: a leader-target Starting slot under `stop_owned`: only the leader is signalled, `group_alive` is not polled for it, and `stop_owned` returns once the leader is reaped (file: `apps/desktop/tests/servers.rs`), written first and failing.
- [ ] Test: SIGSTOP to the owned fake server: slot stays Owned, no `dashboard-exited`, `stop_owned` signals its group, the pidfile exists until the reaper marks Exited, the leader dies from the deadline SIGKILL, group gone within 5 s + 1 s (file: `apps/desktop/tests/servers.rs`), written first and failing.
- [ ] All tests green: `cargo test -p cos-desktop --test servers`

Mark each item [x] as you complete it. Only close when all are checked.

#### US-025: launch cleanup: canonical names, the port lock and Stale records [Backend]
As the owner, I want launch cleanup to skip non-canonical names, leave a pidfile whose lock another instance holds, and remove every Stale pidfile without a signal so that it never signals another instance's server or an undeclared one.

Covers: design items 8 (`launch_cleanup`: canonical-port filter, step 1 lock, step 2 Stale), 9 (the `builder()` bullet's call to `launch_cleanup` after a logged load error), 25 case: invalid `dashboards.toml` at setup with a live child pidfile; 23 cases: four kernel checks pass but no declared port, and with an empty value (the Stale legs of that case); empty value with another supervisor holding the lock; held lock left alone; `04777.json`; record `addr` port 4778; dead pid; `abc.json` and `70000.json` ignored and the lock inode kept across launch cleanup. ISA rows: `signal_group` refused entries "pidfile whose port has no declared server", "missing or invalid `dashboards.toml` at launch", "lock held by another instance", "pidfile named `04777.json`", "record `addr` port differs from its filename port"; Lifecycle: undeclared leftover; Lifecycle: dead leftover; `STORE(port_lock)` launch-cleanup inode leg. The admitted entries of `signal_group` for launch cleanup are in US-026.
Blocked by: US-042. Priority: p1.

- [x] Test: a pidfile passing the four kernel checks is removed without a signal when no declared server has its port and when `launch_cleanup` gets an empty value (file: `apps/desktop/tests/servers.rs`), written first and failing.
- [x] Test: a pidfile whose lock another supervisor holds is kept and nothing signalled, with and without an empty value; `04777.json` copied from a live `4777.json` is ignored; `4777.json` with `addr` port 4778 is removed without a signal; a dead pid's pidfile is removed; `abc.json` and `70000.json` are ignored; `<port>.lock` keeps its inode across cleanup (file: `apps/desktop/tests/servers.rs`), written first and failing.
- [x] Test: an invalid `dashboards.toml` at setup with a pidfile naming a live child of the test: setup completes, the pidfile is removed, the child is alive (file: `apps/desktop/tests/ipc.rs`), written first and failing.
- [x] All tests green: `cargo test -p cos-desktop --test servers --test ipc`

Mark each item [x] as you complete it. Only close when all are checked.

#### US-044: launch cleanup: the five checks refuse a mismatching leftover [Backend]
As the owner, I want launch cleanup to remove without a signal every pidfile whose live process differs from the record or from the declared binary so that a recycled pid is never signalled.

Covers: design item 8 (`launch_cleanup` step 3, the five checks), 23 cases: wrong start time, executable, process group or record `pgid`; four kernel checks pass but the declared `binary` is another file (the five-check leg of that case). ISA rows: `signal_group` refused entries "pidfile whose pid is alive but start time differs", "pidfile whose executable differs", "pidfile whose process group differs from its pid", "pidfile whose record `pgid` differs from its pid while the live group equals the pid", "pidfile whose executable matches the record but is not the declared `binary` for its port". The admitted entries of `signal_group` for launch cleanup are in US-026.
Blocked by: US-025. Priority: p1.

- [x] Test: each mismatching pidfile (start time, executable, process group, record `pgid`) is removed without a signal (file: `apps/desktop/tests/servers.rs`), written first and failing.
- [x] Test: a pidfile passing the four kernel checks whose declared `binary` is another file is removed without a signal (file: `apps/desktop/tests/servers.rs`), written first and failing.
- [x] All tests green: `cargo test -p cos-desktop --test servers`

Mark each item [x] as you complete it. Only close when all are checked.

#### US-026: launch cleanup stops a matching leftover and its group [Backend]
As the owner, I want the next launch after a crash to stop the server the crashed app started, including a member that ignores SIGTERM, so that the port is free again (Goal 7, CUJ 7).

Covers: design item 8 (Launch cleanup steps 4 and 5, `VERIFY(leftover_group)`, re-checks before each signal), 23 cases: matching pidfile from a dead app signalled and removed; leftover `serve-with-member` gets one group SIGKILL. ISA rows: Lifecycle: leftover group member outlives the leader; Lifecycle: crash, then launch; `signal_group` admitted "leftover with all five checks equal and the port lock taken by launch cleanup, signalled at `-pid`" and "one SIGKILL to a leftover's group after its leader passed the five checks at SIGTERM and exited". The refused entries of `signal_group` for launch cleanup are in US-025 and US-044.
Blocked by: US-044. Priority: p1.

- [x] Test: a leftover started through an intermediate process, whose executable is the declared test binary for its port, has its group signalled at `-pid` and its pidfile removed (file: `apps/desktop/tests/servers.rs`), written first and failing.
- [x] Test: a leftover `serve-with-member`: SIGTERM, leader exits, member survives, one SIGKILL to the group while `group_alive` and the lock is held; cleanup returns before 5 s with the pidfile removed; polling every 50 ms for up to 1 s shows the group gone and the port free (file: `apps/desktop/tests/servers.rs`), written first and failing.
- [x] The five checks run again immediately before the SIGTERM and the deadline SIGKILL; each leftover gets at most one SIGKILL.
- [x] All tests green: `cargo test -p cos-desktop --test servers`

Mark each item [x] as you complete it. Only close when all are checked.

#### US-027: `STORE(pidfile)`: a forged file is overwritten and every removal site uses the gate [Backend]
As the supervisor, I want a forged pidfile replaced by the spawn result and every removal path routed through the US-040 removal function so that a forged or stale file never steers a signal.

Covers: design item 8 (`STORE(pidfile)` choke point, the end state across every removal site), 23 case: forged pidfile planted after `launch_cleanup` and before the first spawn is overwritten with the spawn result. ISA rows: `STORE(pidfile)` refused "write attempted from an unverified pidfile read"; admitted "written from the spawn result" (lifecycle case); the removal legs (after Exited, Stale, Leftover) are exercised by US-018, US-019, US-024, US-025, US-026 and US-044, and this story checks they all go through the US-040 gate.
Blocked by: US-019, US-024, US-026. Priority: p1.

- [ ] Test: a forged pidfile at `<port>.json` planted after `launch_cleanup` and before the first spawn is overwritten with the spawn result's `{pid, pgid, start_time, exe}` and the validated `addr`, never copied (file: `apps/desktop/tests/servers.rs`), written first and failing.
- [ ] `rg -n 'remove_file' apps/desktop/src/servers.rs` lists only the US-040 removal function.
- [ ] All tests green: `cargo test -p cos-desktop --test servers`

Mark each item [x] as you complete it. Only close when all are checked.

#### US-028: `businesses` rows carry unread and scheduled, built without a spawn [Backend]
As the sidebar, I want each `businesses` row to carry `unread` and `scheduled` from one row builder that spawns nothing so that the sidebar and ⌘K read one source.

Covers: design item 9 (`businesses` rows and the synchronous row builder), 25 cases: `businesses` rows for three businesses with the unread dot set; `businesses` does not spawn `ps`. ISA rows: none.
Blocked by: US-007, US-014. Priority: p1.

- [x] Test (contract): `businesses` rows for three businesses carry `unread` and `scheduled`, with the dot set for a business with a brief and no stored pair (file: `apps/desktop/tests/ipc.rs`), written first and failing.
- [x] Test: `businesses` spawns no `ps`, through the child-process mechanism of item 20, macOS only (file: `apps/desktop/tests/ipc.rs`), written first and failing.
- [x] The plist path comes from `Start.user_home`; `None` gives no clock.
- [x] All tests green: `cargo test -p cos-desktop --test ipc`

Mark each item [x] as you complete it. Only close when all are checked.

#### US-029: watcher tick and shared row cache emit `businesses-changed` [Integration]
As the owner, I want rows to update within one 700 ms tick when a run dies, a brief lands or the registry changes so that the sidebar is current without an IPC call (CUJ 8).

Covers: design item 9 (shared row cache and publish function; `watch()`), design §6 Refresh and Events, 25 cases: CUJ 8; business removed from `businesses.toml` absent from the next payload; switch from A to B marks `app: true` on B only; Starting run turns Lost after `START_GRACE`. ISA rows: none.
Blocked by: US-028. Priority: p1.

- [ ] Test: SIGKILL to a run's supervisor without a file change turns the row lost within one tick; the test reaps the supervisor it kills (file: `apps/desktop/tests/ipc.rs`), written first and failing.
- [ ] Test: a Starting run turns Lost after `START_GRACE` with no file change and the row updates without an IPC call (file: `apps/desktop/tests/ipc.rs`), written first and failing.
- [ ] Test (contract): a business removed after the first tick is absent from the next `businesses-changed` payload, which carries every remaining row in the `businesses` shape; after a switch A to B, a stamp change elsewhere emits rows with `app: true` on B only (file: `apps/desktop/tests/ipc.rs`), written first and failing.
- [ ] Test: the first tick emits no `dashboards-changed`; a later `dashboards.toml` mtime change emits one with no payload (file: `apps/desktop/tests/ipc.rs`), written first and failing.
- [ ] All tests green: `cargo test -p cos-desktop --test ipc`

Mark each item [x] as you complete it. Only close when all are checked.

#### US-030: `STORE(ui-state)`: `brief_seen` records the rendered brief [Backend]
As the owner, I want opening Today to clear the unread dot only for the brief the report rendered, for the app's own registry business, so that the dot never clears by mistake.

Covers: design item 9 (`brief_seen` command; ui-state helper), design §6 unread bullets; side effect S7; 25 cases: dot cleared by `brief_seen` and surviving a new `App`; `brief_seen` refused for stale business, non-date filename, filename outside `out/briefs`; `mtime` greater than the file's refused; never lowers the pair; creates the missing config dir; emits `businesses-changed` with the dot cleared, holding every business before the first tick; shared cache keeps the cleared dot and a new brief re-lights it; defaults-layer `default` refused. ISA rows: `STORE(ui-state)` (all entries).
Blocked by: US-029. Priority: p1.

- [ ] Test: refused for a stale business, a non-date filename, a filename outside `out/briefs`, an `mtime` greater than the file's (pair unchanged), and a defaults-layer window with a registered `default` (pair unchanged) (file: `apps/desktop/tests/ipc.rs`), written first and failing.
- [ ] Test: admitted call clears the dot, survives a new `App`, never lowers the stored pair, and creates the missing config dir under `<ui_state_file>.lock` (file: `apps/desktop/tests/ipc.rs`), written first and failing.
- [ ] Test: an unparseable `ui-state.json` reads as empty (every business with a brief is unread) and the next admitted `brief_seen` replaces it with valid JSON; an entry for a name not in the registry is absent from the file after the next write (file: `apps/desktop/tests/ipc.rs`), written first and failing.
- [ ] Test: `brief_seen` emits `businesses-changed` with the dot cleared and, before the first tick, every registered business; after it, a stamp change elsewhere still emits `unread: false`, and a new brief re-lights the dot within one tick (file: `apps/desktop/tests/ipc.rs`), written first and failing.
- [ ] All tests green: `cargo test -p cos-desktop --test ipc`

Mark each item [x] as you complete it. Only close when all are checked.

#### US-031: `dashboards` IPC command [Backend]
As the dashboard pane, I want `dashboards` to load the file fresh, check the business, and return the app business's dashboards so that the tabs match the file.

Covers: design item 9 (the `dashboards` command), design §5 Fresh load (for `dashboards`), Invalid file (the error `dashboards` returns). ISA rows: none (the IPC-side `RENDER(iframe src)` entries are `dashboard_open` refusals, US-041).
Blocked by: US-004, US-014. Priority: p1.

- [ ] Test (contract): `dashboards(business)` returns `[{name, url, managed}]` for the app business, and a dashboard added to `dashboards.toml` after launch appears in the next call (file: `apps/desktop/tests/ipc.rs`), written first and failing.
- [ ] Test: an invalid `dashboards.toml` gives an error naming the key path and rule; a stale business gives `named()`'s error text (file: `apps/desktop/tests/ipc.rs`), written first and failing.
- [ ] All tests green: `cargo test -p cos-desktop --test ipc`

Mark each item [x] as you complete it. Only close when all are checked.

#### US-041: `dashboard_open` IPC command [Backend]
As the dashboard pane, I want `dashboard_open` to load the file fresh, check the business, and return `Running` or a typed `Refusal` so that the pane shows every state.

Covers: design item 9 (the `dashboard_open` command), design §5 No registry business, Fresh load (for `dashboard_open`), Opening; 25 cases: `dashboard_open` refused for a stale business; `UnknownDashboard` for an undeclared name and `InvalidConfig` with key path; unmanaged dashboard added after launch returns `Running{url, owned: false}`; window with no registry business gets an empty list and `UnknownDashboard`. ISA rows: `RENDER(iframe src)` refused "URL not declared for the app business" and "window with no registry business" (IPC side).
Blocked by: US-016, US-031. Priority: p1.

- [ ] Test: stale business gives `StaleBusiness`; undeclared name gives `UnknownDashboard`; invalid file gives `InvalidConfig` with key path (file: `apps/desktop/tests/ipc.rs`), written first and failing.
- [ ] Test (contract): an unmanaged dashboard added after launch, served by a `TcpListener` fixture with its marker, makes the next `dashboard_open` return `Running{url, owned: false}` (file: `apps/desktop/tests/ipc.rs`), written first and failing.
- [ ] Test: a window labelled `explicit` with a `[[business.explicit]]` entry gets an empty `dashboards` list and `UnknownDashboard` from `dashboard_open` (file: `apps/desktop/tests/ipc.rs`), written first and failing.
- [ ] All tests green: `cargo test -p cos-desktop --test ipc`

Mark each item [x] as you complete it. Only close when all are checked.

#### US-032: `VERIFY(origin_allowed)` guards the `cosreport` scheme [Backend]
As the owner, I want `cosreport` to refuse writes from any origin but the app's so that a dashboard frame cannot set `x-cos-business` and write.

Covers: design item 9 (`cosreport_response`), design §5 Origin rule; side effect S10; 25 cases: `post_organize` sends `Origin: tauri://localhost` through `cosreport_response`; the POST and GET Origin cases; `OPTIONS` and `PUT` refused. ISA rows: `VERIFY(origin_allowed)` (all entries).
Blocked by: US-014. Priority: p1.

- [ ] Test: through `cosreport_response`, a valid organize POST with matching `x-cos-business` gets 403 with `Origin: http://127.0.0.1:4777`, `Origin: null` and no Origin, and 200 with each app origin (`tauri://localhost`, `http://tauri.localhost`, `cosreport://localhost`, `http://cosreport.localhost`); each 403 body differs from `missing or stale X-Cos-Business header` (file: `apps/desktop/tests/ipc.rs`), written first and failing.
- [ ] Test: GET `/` gets 403 with `Origin: http://127.0.0.1:4777` and 200 with `Origin: null`, with none and with `Origin: tauri://localhost`; `OPTIONS`, `PUT` and `DELETE` get 403 with `Origin: http://127.0.0.1:4777` and with none (file: `apps/desktop/tests/ipc.rs`), written first and failing.
- [ ] The `cosreport` closure calls only `cosreport_response`; existing organize cases pass.
- [ ] All tests green: `cargo test -p cos-desktop --test ipc`

Mark each item [x] as you complete it. Only close when all are checked.

#### US-033: dashboard tabs, notice and hash routing [UI]
As the owner, I want one header tab per dashboard of the app's business, restored from the hash, with a notice for an invalid file so that the dashboards are one click away.

Covers: design items 11, 12 (`#dash-tabs`, `#dash-notice`, `#view-dash`, `#dash-state`, `#dash-frame`, `dash.js` script tag), 14 (`loadDashTabs`, `hideDash`, tab clicks, `dashboards-changed` gating), 15 (`openTab` `dash` argument; `VIEWS`; async `openFromHash`); design §5 Tabs, Startup and restore, Invalid file, CSP, Switch. UI case (g). ISA rows: none (frame `src` is set by US-034).
Blocked by: US-001, US-031. Priority: p1.

- [ ] Test: `rg -n 'id="(dash-tabs|dash-notice|view-dash|dash-state|dash-frame)"' apps/desktop/ui/index.html` prints five lines and `rg -n 'src="dash.js"' apps/desktop/ui/index.html` prints one, checked failing before the edit.
- [ ] Test (contract): relies on the US-031 `dashboards` shape test; `dash.js` reads only `name`, `url`, `managed`.
- [ ] `apps/desktop/tauri.conf.json` `frame-src` is `cosreport: http://cosreport.localhost http://127.0.0.1:*`, or the fallback US-001 recorded.
- [ ] Built app on `founding-bets`: launch with no hash and with `#tab=agents` both render "Control room" and "Salesroom"; screenshot each.
- [ ] Built app, UI case (g): an invalid `dashboards.toml` shows `#dash-notice` and no tabs; a valid file shows no notice; fixing the file while running clears the notice and renders the tabs; screenshots.
- [ ] Built app: `#tab=dash&dash=Control%20room` restores the tab idle, with no `dashboard_open` call; an undeclared name opens Today; screenshots.
- [ ] All tests green: `cargo test -p cos-desktop --test ipc` and `task desktop-app`

Mark each item [x] as you complete it. Only close when all are checked.

#### US-034: `RENDER(iframe src)`: open a dashboard and its refusal cards [UI]
As the owner, I want the pane to show the verified page or a card naming why not and the next step so that a foreign page never shows under a tab.

Covers: design items 14 (`openDash`, refusal cards by `kind`), 16 (refusal card styles); design §5 Opening (the Running and refusal outcomes), Sandbox (`allow-forms` kept only if the live gazette submits a form). Side effect S6. ISA rows: none (the server-side `RENDER(iframe src)` entries are tested in their backend stories; the UI-side entries are in US-045).
Blocked by: US-022, US-033, US-041. Priority: p1.

- [ ] Test: a text scan (`rg`) of `apps/desktop/ui/dash.js` finds one card branch per `Refusal` kind of the US-015 contract test and sets `#dash-frame.src` only from a Running result, checked failing before the edit.
- [ ] Test (contract): relies on the US-015 `Refusal`/`Running` contract tests; `dash.js` switches on `kind` only.
- [ ] Built app: "starting…", Running (owned), Running (external), each `Identity` variant (neither, `owned`, `spawned`, `no response`), `BinaryMissing`, `NotAdmissible`, `FailedToStart`, `NotRunning`, `StaleBusiness`, `UnknownDashboard`, `InvalidConfig` render their card text from design §Violations; screenshot each.
- [ ] `allow-forms` decision recorded from the live gazette page (kept or dropped) in the PR body. If the gazette is not loaded, the PR body records `allow-forms: pending (gazette not loaded)` with the `launchctl list com.longhand.chief-of-staff-dashboard` output, the flag stays, and the gazette entry under Manual and external actions applies.
- [ ] All tests green: `cargo test -p cos-desktop --test servers --test ipc` and `task desktop-app`

Mark each item [x] as you complete it. Only close when all are checked.

#### US-045: dashboard pane drops stale results, keeps an unchanged dashboard and shows the exit card [UI]
As the owner, I want stale results dropped, an unchanged dashboard kept across a file edit, and an owned server's exit shown so that no earlier, removed or dead page stays under a tab.

Covers: design items 14 (sequence token, `dashboard-exited` handling, §5 Reload rule), 16 (`exited` card style); design §5 Opening (dropping stale results and `dashboard-exited`), Reload. UI cases (d), (e), (f). ISA rows: `RENDER(iframe src)` refused "a refusal after switching from a shown dashboard tab to another" and "an in-flight result after `dashboards-changed` removes the open dashboard or changes its `url`" (UI side).
Blocked by: US-019, US-034. Priority: p1.

- [ ] Test: a text scan (`rg`) of `apps/desktop/ui/dash.js` finds the sequence-token check before a result is applied and a `dashboard-exited` listener that compares `addr`, checked failing before the edit.
- [ ] Test (contract): relies on the US-019 `dashboard-exited` contract test; `dash.js` reads only `addr`, `status` and `log_tail` from it.
- [ ] Built app, UI cases (d), (e), (f): a refusal after switching tabs shows no earlier frame; a `dashboards.toml` edit removing the opening dashboard leaves the pane idle with no frame; `dashboard-exited` for the shown owned dashboard clears the frame and shows `exited`, and one arriving while `openDash` is pending is dropped; screenshots.
- [ ] Built app, Reload kept branch: with a dashboard open, a `dashboards.toml` edit that changes only its `probe` marker to text the server does not serve keeps the frame shown (no idle state, no card); the next open of that dashboard shows the `Identity` card, so the edited probe applies only then; screenshots.
- [ ] All tests green: `cargo test -p cos-desktop --test servers` and `task desktop-app`

Mark each item [x] as you complete it. Only close when all are checked.

#### US-035: business sidebar with live signals [UI]
As the owner, I want a sidebar listing every business with live count, unread dot, lost warning and clock, that switches like ⌘K, so that I see which business needs attention (CUJ 1, CUJ 2).

Covers: design items 12 (`.shell-main`, `#sidebar`, `sidebar.js` script tag, `#other-live` removal), 13, 15 (expose `switchTo` and its `tab=dash&dash=<d>` hash write; `#other-live` code removal), 15a, 16 (sidebar layout at `12.5rem`, row states, glyphs, `#other-live` rule removal); design §6 Rendering, One source. ISA rows: none.
Blocked by: US-029. Priority: p1.

- [ ] Test: `rg -n 'id="sidebar"' apps/desktop/ui/index.html` prints one line and `rg -n 'other-live|businesses-loaded' apps/desktop/ui` prints nothing, both checked failing before the edit.
- [ ] Test (contract): relies on the US-028 and US-029 `businesses` and `businesses-changed` shape tests; `cargo test --test design` passes with the `12.5rem` width.
- [ ] Built app: six rows in alphabetical order, the app business highlighted, live count, unread dot, lost warning, clock titled `installed for <name>`, and a `missing: true` row reading `home is missing`; screenshot each.
- [ ] Built app: a click switches as ⌘K does; a refusal (unsaved Hire, missing dirs) shows in the status line; a pinned app disables rows titled `pinned by <var>`; screenshots.
- [ ] Built app, initial-result race: `sidebar.js` registers its `businesses-changed` listener before it invokes `businesses`; with the `businesses` reply held at a devtools breakpoint until a `businesses-changed` payload has been applied, the held result is dropped and the rows keep the payload's values; devtools screenshot.
- [ ] All tests green: `cargo test --test design` and `task desktop-app`

Mark each item [x] as you complete it. Only close when all are checked.

#### US-036: Today calls `brief_seen` when the brief is shown [UI]
As the owner, I want the unread dot to clear only when Today is visible and has rendered the brief so that a brief I have not seen stays marked.

Covers: design item 15 (`cos-report-ready` handler), design §6 Opened. UI cases (a), (b), (c). ISA rows: none (the `STORE(ui-state)` gate is US-030).
Blocked by: US-008, US-030, US-035. Priority: p1.

- [ ] Test: `rg -n 'brief_seen' apps/desktop/ui/app.js` prints the `cos-report-ready` handler's call and the switch-to-Today call, checked failing before the edit.
- [ ] Test (contract): relies on the US-008 `briefRef`/`hostBrief` tests; the handler reads only `brief` and `mtime`.
- [ ] Built app, UI case (a): a `cos-report-ready` without `brief` is not passed to `brief_seen` and the `cos-host {hire: true}` reply is still sent (devtools log screenshot).
- [ ] Built app, UI case (b): `brief_seen` is not called while another view is visible and is called on the switch to Today; screenshots.
- [ ] Built app, UI case (c): at launch with an unread brief on the app business, the dot clears once Today renders and stays cleared; screenshots.
- [ ] All tests green: `cargo test -p cos-desktop --test ipc` and `task desktop-app`

Mark each item [x] as you complete it. Only close when all are checked.

#### US-037: README documents dashboards and indexes the plan docs [Integration]
As a reader of the repository, I want a Dashboards section and index lines so that `dashboards.toml` and its ownership rule are findable.

Covers: design item 26. ISA rows: none.
Blocked by: US-005. Priority: p2.

- [ ] Test: `rg -n '^## Dashboards' README.md` prints one line, and `rg -n 'business-dashboards-(design|design-review|plan|q1-spike)' README.md` lists the design, its review, this plan and the US-001 record, checked failing before the edit.
- [ ] The section has the §3 example, the ownership rule (owned, external, leftover) and `cos business dashboards`.
- [ ] All tests green: `cargo test --test design`

Mark each item [x] as you complete it. Only close when all are checked.

#### US-038: Setup Runbook: register the businesses and write `dashboards.toml` [Integration]
As the owner, I want the four businesses registered and `dashboards.toml` written so that the app shows them (Goal 1).

Covers: design Setup Runbook (every bullet). ISA rows: none.
Blocked by: US-005, US-006. Priority: p1.

- [ ] Test: `cos business list` and `cos business dashboards`, run before any runbook step, are recorded failing: the four businesses are not all listed and no Gazette `answers` line is printed.
- [ ] `cos --workspace ~/Projects/chronos state init` runs; `cos business add` for `founding-bets`, `longhand`, `realestate`, `kronos` with the runbook's directories; the `realestate` add prints the nested-home warning.
- [ ] `cos business list` output shows all four, each `dirs ok, profile ok`, recorded.
- [ ] The control-room binary exists (built in US-001, or with the runbook's `cargo build --release`); `ls -l` output recorded.
- [ ] `~/.config/cos/dashboards.toml` is written from design §3 with the gazette marker read from the live page; the marker is recorded in the PR body. If the gazette is not loaded, the PR body records `gazette marker: pending (gazette not loaded)` with the `launchctl list com.longhand.chief-of-staff-dashboard` output, the Gazette entry is left out of the file, and the gazette entry under Manual and external actions applies.
- [ ] All tests green: `cos business dashboards` exits 0 with no invalid-file error and prints `answers` for Gazette, unless the gazette marker is recorded as pending.

Mark each item [x] as you complete it. Only close when all are checked.

#### US-039: E2E CUJ 1 to 8 on the built app with proof [Integration]
As the owner, I want every journey run on the real app and machine with recorded proof so that the feature is shown working end to end.

Covers: design Verification Plan E2E steps; CUJ 1 to 8. ISA rows: none new (every row is a test in its story).
Blocked by: US-012, US-013, US-020, US-021, US-027, US-032, US-036, US-037, US-038, US-043, US-045. Priority: p1.

- [ ] CUJ 1 and 2 on `task desktop-app`: six rows in order, unread on `longhand`, `1` on `founding-bets`, sidebar switch with `cos business list` active unchanged; screenshots.
- [ ] CUJ 3 and 4: `lsof -nP -iTCP:4777 -sTCP:LISTEN` captured before open, after two opens (one pid) and after ⌘Q (none); pidfile gone; no `portfolio-control-room` with PPID 1.
- [ ] CUJ 5: a stranger on `:4777` gives the identity card with the `lsof` step and no signal; screenshot.
- [ ] CUJ 6: `launchctl list com.longhand.chief-of-staff-dashboard` pid identical before launch and after quit; Gazette shown; screenshot. If the gazette is not loaded, the PR body records `CUJ 6: pending (gazette not loaded)` with that output, and the gazette entry under Manual and external actions applies.
- [ ] CUJ 7: `kill -9` of the app, relaunch, `4777.json` gone and `:4777` free, next open starts a fresh server.
- [ ] CUJ 8: `kill -9` of a live run's supervisor turns the row lost within one tick; screenshot.
- [ ] Proof recorded with `pr-proof` on PR #27.
- [ ] All tests green: every CUJ item above has its capture (screenshot, `lsof` or `launchctl` output) in the `pr-proof` record; only CUJ 6 may instead carry its recorded pending status.

Mark each item [x] as you complete it. Only close when all are checked.

### Functional requirements
- FR-1: `cos business list` must show `founding-bets`, `longhand`, `realestate` and `kronos`, each `dirs ok, profile ok` (Goal 1).
- FR-2: The app must show a sidebar listing every registered business in alphabetical order; a click must switch through `switch_business` with ⌘K's refusals and unsaved-Hire guard (Goal 2).
- FR-3: On macOS each row must show live runs (starting included), an unread dot, a lost warning, and a clock for the business the installed `cos.brief` plist names for the same registry, computed with no process spawn (Goal 3).
- FR-4: A business with dashboards must show one tab per dashboard as declared in `dashboards.toml` (Goal 4).
- FR-5: Opening dashboards of one owned server must start at most one process; quit must stop every server the app started, group members included, within the 5 s + 1 s budget (Goal 5).
- FR-6: A listener the app did not start must be embedded only after its identity probe passes, refused with a visible reason otherwise, and never signalled (Goal 6).
- FR-7: The next launch after a crash must stop a leftover only when the port lock is free and the pid, start time, executable, process group and declared binary all match; otherwise it must remove the pidfile without a signal (Goal 7).
- FR-8: Every dashboard state of Goal 8 must be visible in its pane.
- FR-9: With the gazette running, `task schedule` must load `cos.brief`, skip `cos.dash` and name the process holding 8790 (Goal 9).
- FR-10: `cosreport` must refuse every non-GET request without an app Origin.

### Non-goals
- The app building a dashboard server; the app never runs `cargo`.
- Bundling a dashboard binary in the app (`externalBin` stays empty).
- The revenue desk (`:4788`).
- Starting, stopping or restarting a launchd-owned dashboard (the Longhand gazette).
- Scheduling more than one business, per-business launchd labels, and moving the `cos dash` port.
- A generic plugin system for health checks, or non-loopback dashboard URLs.
- Dashboards that follow the CLI's active business.
- A display name separate from the business slug.

### Open questions
- **Q1 record location (owner decision).** The review says to record the spike result in the design, but any edit to the design's bytes voids its approval. This plan records it in `docs/plans/2026-11-15-business-dashboards-q1-spike.md` (US-001). Design item 27 ("revised in place") is therefore not placed in any story; folding the result into the design means a new approval.
- **Writes to the owner's real config (owner awareness).** US-038 writes `~/.config/cos/businesses.toml` and `~/.config/cos/dashboards.toml` on the owner's machine, as the Setup Runbook describes.
- **Embedding fallback.** If US-001 fails check 1 or 2, US-033, US-034 and US-045 change to the fallback the design names (Alternative F or E). Alternative E needs Tauri's `unstable` multi-webview feature; that path is `water` until US-001 says it is needed.

### Manual and external actions

Actions that need state no story can produce locally. No story waits on them as a dependency; each story that reads this state records a pending status when it is absent.

| Action | Owner | Trigger | Stories that record pending | Status |
| --- | --- | --- | --- | --- |
| Load the Longhand gazette (`launchctl bootstrap` of `com.longhand.chief-of-staff-dashboard`), which launchd owns and this feature never starts | owner | `launchctl list com.longhand.chief-of-staff-dashboard` shows it not loaded when US-001, US-034, US-038 or US-039 runs | US-001 check 4, US-034 `allow-forms`, US-038 gazette marker, US-039 CUJ 6 | open |
| Read the first CI run of `cargo test --workspace` on `feat/business-dashboards` with `gh run view` and record its duration against `timeout-minutes: 10` in the PR body; if it does not fit, move the desktop crate to its own job (the limit is not raised) | owner, or the Build session after a push | the first push after US-013 lands | none | open |
