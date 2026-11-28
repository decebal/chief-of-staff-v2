# MCP client bridge: plan

Design: [2026-11-25-mcp-client-bridge-design.md](2026-11-25-mcp-client-bridge-design.md)
(approved, sha256 `7f9316ae2ff6b6886f028aae983ac3c44c9aa71ea97428fa07a5719fe087b2b9`).
The owner approved round 4 (`c1a65677…`), with the open blocking findings of
[review a](2026-11-25-mcp-client-bridge-design-review-a.md) and
[review b](2026-11-25-mcp-client-bridge-design-review-b.md) carried here as
US-001, then approved the
[cleanup spike](2026-11-25-mcp-client-bridge-cleanup-spike.md)'s five text
changes, which this sha holds.

### Quality gates

#### Epic-level (once, when every story is done)

- `task test` (fmt, clippy `-D warnings`, every test of the workspace, the
  desktop crate's included, with `--features cos/stand-in`)
- `task desktop-check`
- The design's Acceptance 1 to 8, run by hand against the real Claude Desktop
  and Codex on this Mac, each result recorded in the PR body

#### Story-level

- CLI and daemon stories: the story's integration test file passes alone
  (`cargo test --features stand-in --test <file>`; without the feature the
  stand-in cases compile out and the file passes by running nothing)
- App stories: the story's `apps/desktop/tests/*.rs` cases pass on the mock
  runtime
- UI stories: the window rendered through `examples/preview.rs` against the
  stand-in, light and dark, with `tests/design.rs` green

### Testing trophy

#### Unit
- [ ] [Logic] run id, link and record parsers; closed bindings reject extra fields (`src/runs/tests.rs`)
- [ ] [Logic] record transitions under the lock; final states never change (`src/runs/tests.rs`)
- [ ] [Logic] group identity: live leader with the same start, dead leader with live members, live leader with another start refused (`src/proc/tests.rs`)
- [ ] [Logic] the business table: pinned, unpinned, remote (`tests/mcp_bridge.rs`)
- [ ] [Logic] config editors: JSON key order, TOML comments and other tables kept (`src/connect/tests.rs`)
- [ ] [Logic] socket path length check (`src/daemon/tests.rs`)

#### Contract
- [ ] [MCP] `brief`, `ask`, `run_status`, `runs`, `businesses` schemas, and `read_only_hint` per tool (`tests/mcp_bridge.rs`)
- [ ] [Socket] each request's reply shape, refusals as `{"error"}` (`tests/daemon.rs`)
- [ ] [Tauri] `run_get`, `run_stop`, `connect_status`, `connect_apply`, `business_make_default` return shapes (`apps/desktop/tests/ipc.rs`)

#### Integration
- [ ] [Flow] every ISA test-matrix row in the design (`tests/daemon.rs`, `tests/run_process.rs`, `tests/mcp_bridge.rs`, `tests/connect.rs`)
- [ ] [Flow] the MCP App view's path: `brief`, then `run_status` until final, then the report fields (`tests/mcp_bridge.rs`)

#### Frontend
- [ ] [Component] run panel states (queued, running, lost, done, ask answer, refused Stop) through `stateView` (`apps/desktop/tests/ipc.rs`, `tests/design.rs`)
- [ ] [Component] Assistants card green and red rows (`apps/desktop/tests/ipc.rs`, `tests/design.rs`)
- [ ] [Component] cold-start link applied after ready (`apps/desktop/tests/links.rs`)

Block merge: every test above, and the epic gates. Block release: the same.
Follow-up allowed: none.

### Stories

#### US-001: run process cleanup holds the invariant [Backend, spike]

As the owner, I want every process group a run created to be gone before its
final state is written, whatever ended the run, so that no paid `claude`
outlives its run.

The spike resolves the four open blocking findings from round 4, against the
real `src/proc.rs`. It decides between the design's group list and the
alternative of keeping grandchildren in the run's own group (with
`proc::run` killing a timed-out grandchild by its pid tree). It records the
choice and its evidence in
`docs/plans/2026-11-25-mcp-client-bridge-cleanup-spike.md`. If the choice
changes the design's text, the flow goes back to Spec for that change.

- [ ] Test: a run that exits 0 while a grandchild group lives ends with that group gone before `done` is written (`tests/run_process.rs`), written first and failing
- [ ] Test: SIGKILL of the run process inside the spawn-to-record window leaves no live grandchild once the owner has reaped (`tests/run_process.rs`)
- [ ] Test: a self-owned `cos ask` SIGKILLed with a live grandchild is swept by the next run start for that business, and its record becomes `lost` (`tests/run_process.rs`)
- [ ] Test: Ctrl-C (SIGINT) on `cos brief` with `fake-claude.sh` leaves no fake-claude process (`tests/run_process.rs`)
- [ ] "Gone" is defined in code (`proc::group_gone`) and every writer of a final state waits for it with a bound and a failure path (not final, retried by the next sweep)
- [ ] Run records under `out/run-records/`, the brief lock with the last-holder sweep, the owner watchdog (`COS_OWNER`, exit 76), self-cleanup on SIGTERM, SIGINT and SIGHUP
- [ ] `pipeline::brief_child` split into `spawn_run` / `RunChild`; the app's `brief_run` uses it unchanged in behaviour
- [ ] The spike doc committed and indexed in `README.md`
- [ ] All tests green: `cargo test --features stand-in --test run_process`, `cargo test --lib`

#### US-002: MCP tools take a business per call [Backend]

As a Claude Desktop or Codex user, I want each tool to act on the business I
name, or the registry's default at that moment, so that the assistant and the
app never silently disagree.

- [ ] Test: unpinned server, Make default between two calls, `latest_brief` with no argument reads the new default (`tests/mcp_bridge.rs`), written first and failing
- [ ] Test: pinned server refuses another business, naming the pin; remote refuses any undeclared argument with `-32602` before dispatch; `Mode::Remote` holds no per-call resolver (`tests/mcp_bridge.rs`)
- [ ] `business?` on `latest_brief`, `plan`, `handoff`, `agents`, `spin_off`, `stop_agent`; the new `businesses` tool (local only)
- [ ] `spin_off` arguments bounded as the `hire.spawn` binding says (`checks` up to 10 entries of 500 chars, `scope` relative paths only)
- [ ] `brief` and `ask` annotated `read_only_hint = false`
- [ ] `business::check_dirs` and `business::registered_homes` moved into the lib; the app calls them
- [ ] All tests green: `cargo test --test mcp_bridge`

#### US-003: `cos connect` registers and repairs the clients [Backend]

As the owner, I want one command to show, register, repair and remove `cos` in
Claude Desktop and Codex, so that registration never silently goes missing.

- [ ] Test: with `HOME` and `CODEX_HOME` on temp dirs, `status` reports both unregistered; `claude-desktop` and `codex` register; `status --json` reports green (`tests/connect.rs`), written first and failing
- [ ] Test: malformed JSON and TOML untouched and named; a file changed between read and rename refused; a second write keeps the first `.cos.orig`; other servers value-equal (JSON) and text-equal (TOML) (`tests/connect.rs`)
- [ ] `toml_edit` 0.25 added as a direct dependency; `status --json` keys documented in `docs/usage.md`
- [ ] `--version` of the configured binary run once per path, size and mtime (5 s deadline, reaped), stale on timeout
- [ ] Codex `tool_timeout_sec = 320`
- [ ] All tests green: `cargo test --test connect`

#### US-004: Rust test stand-ins [Backend]

As a test author, I want one Rust stand-in binary for run children,
`launchctl` and `--version`, so that no new shell fixtures are needed.

- [ ] Test: a smoke test calls each mode and checks its output file (`tests/stand_in.rs`), written first and failing
- [ ] `[[bin]] name = "cos-stand-in"` with `test = false` and excluded from `cargo install` (its own `required-features = ["stand-in"]`), the feature on in `task test`
- [ ] `run` mode honours `COS_RUN_RECORD`, `COS_OWNER`, sleeps, spawns a grandchild group (null stdio), exits with a chosen code; accepts the run child's argv (`--home <h> … brief|ask -- <q>`, no `COS_BUSINESS`); SIGTERM writes the record's `stopping` (else `stopped`) and exits 143, `COS_IGNORE_TERM=1` ignores it; `launchctl` mode records argv; `version` mode prints a chosen version
- [ ] All tests green: `cargo test --features stand-in --test stand_in`

#### US-005: the daemon serves runs [Backend]

As an MCP client, I want a daemon that owns paid runs and outlives me, so that
a run started from Claude Desktop finishes after the chat closes.

- [ ] Test: start, `existing` on a second brief, `client_req` retry, stop (SIGTERM, then SIGKILL after 5 s), deadline, SIGTERM of the daemon → `stopped`, SIGKILL + restart → sweep and `lost`, a judge-style `sh` child swept from `daemon.groups.json`, a second daemon exits 0, stale socket, exe change → exit 1, peer uid check (`tests/daemon.rs`), written first and failing
- [ ] Socket at `confstr(_CS_DARWIN_USER_TEMP_DIR)/cos/daemon.sock`, 0700 dir, 0600 socket; 64 KiB lines, 5 s idle close, 16 connections
- [ ] `subscribe` polls `out/run-records/` with `dash::stamp` every second
- [ ] `agents::follow_all` in the lib; the daemon runs it every 2 s
- [ ] SIGTERM also stops the daemon's judge and `hc` groups
- [ ] All tests green: `cargo test --features stand-in --test daemon`

#### US-006: install the daemon under launchd [Backend]

As the owner, I want `cos daemon install` to load the daemon and keep it on
the current binary, so that I never manage it by hand.

- [ ] Test: with the stand-in `launchctl` and a temp `LaunchAgents`, install writes the plist (`KeepAlive.SuccessfulExit = false`, pinned `COS_BUSINESSES`, `PATH`), records `bootout` then `bootstrap`; drains the daemon first and refuses (exit 75) while a daemon run is live unless `--force`, which stops it and drains; a start sent between the drain and `bootout` is refused, never killed; a mute daemon fails the install; a daemon `bootout` leaves running fails it and is undrained; refuses a `bin` with `<` (`tests/daemon_install.rs`), written first and failing
- [ ] Template embedded with `include_str!` from the crate root path
- [ ] `task install` and `task setup` call `cos daemon install`; `task install` with a live run reports the refusal and leaves the old daemon on the new binary's next idle restart
- [ ] All tests green: `cargo test --features stand-in --test daemon_install`

#### US-007: paid MCP tools return a run id [Backend]

As a Codex user, I want `brief` and `ask` to answer at once with a run id and
`run_status` to wait for the result, so that no call hits the 60 s timeout.

- [ ] Test: with a temp daemon, `brief` answers within 2 s with `RunView` and a `cos://run/…` link; `run_status` with `wait_secs: 45` returns `done` with the report; a second `brief` returns `existing: true` (`tests/mcp_bridge.rs`), written first and failing
- [ ] Test: with no daemon, `brief` runs owned by `cos mcp`, and closing stdin kills and reaps it, with the record `stopped`; the daemon's caps hold for the runs `cos mcp` owns; a run of an unregistered triple carries no link (`tests/mcp_bridge.rs`)
- [ ] `runs` tool; `run_status` reads files only and re-reads before showing `lost`
- [ ] Hire following in `cos mcp` skipped while `hello` answers (cached 3 s)
- [ ] `src/dashboard.html`: Run new brief calls `brief`, then `run_status` until final, rendering a non-final `RunView` as "Running…" without throwing
- [ ] All tests green: `cargo test --features stand-in --test mcp_bridge`

#### US-008: the app follows runs from any client [UI]

As the owner, I want the app to show a run Claude Desktop started, live, and
to mark which business MCP clients default to, so that both sides agree.

- [ ] Test: a stand-in daemon's subscribe stream emits `brief-changed` (with the run id) and `report-changed` on the mock runtime; `brief_run` with the daemon up sends `start`; in fallback it keeps its `RunChild` in the managed map (`apps/desktop/tests/ipc.rs`), written first and failing
- [ ] Test: `business_make_default` sets the registry's active business; the picker data marks it (`apps/desktop/tests/ipc.rs`)
- [ ] The subscribe thread reconnects every 2 s; `follow_herdr` skips `tick` while `hello` answers
- [ ] Rendered in `examples/preview.rs`, light and dark; `tests/design.rs` green
- [ ] All tests green: `cargo test -p cos-desktop --test ipc`

#### US-009: Assistants card in Settings [UI]

As the owner, I want a Settings card that shows both clients' health and
repairs them, so that a missing registration is visible and one click away.

- [ ] Test: `connect_status` and `connect_apply` on temp `HOME`/`CODEX_HOME`; version skew against the app's version and the daemon's `hello` named (`apps/desktop/tests/ipc.rs`), written first and failing
- [ ] Every state through `stateView` with an icon and art; "restart Claude Desktop to load it" after a Claude Desktop write
- [ ] Rendered in `examples/preview.rs`, light and dark; `tests/design.rs` green
- [ ] All tests green: `cargo test -p cos-desktop --test ipc`, `cargo test --test design`

#### US-010: `cos://` links open the app on a run [UI]

As a Claude Desktop user, I want to click a run's link and land on that run in
the app, even when it was closed, so that the chat and the app are one flow.

- [ ] Test: accepted and refused links (each refusal in the design's matrix); a cold-start link held until ready; a link to a pinned window refused with the reason (`apps/desktop/tests/links.rs`), written first and failing
- [ ] Test: `run_get` on queued, running, reader-lost, done brief and done ask; `run_stop` per owner (daemon, this app, refused for self and mcp) (`apps/desktop/tests/ipc.rs`)
- [ ] `tauri-plugin-deep-link` 2.6 and `tauri-plugin-single-instance` 2.4; `CFBundleURLTypes` in `apps/desktop/bundle/Info.plist`; `task desktop-app` runs `lsregister -f` and fails when the scheme is not registered
- [ ] Run panel on Today; ask answers rendered with marked and DOMPurify, the docs room's renderer
- [ ] Rendered in `examples/preview.rs`, light and dark; `tests/design.rs` green
- [ ] All tests green: `cargo test -p cos-desktop --test links --test ipc`

#### US-011: Taskfile and docs follow the new commands [Backend]

As a new user, I want `task install-mcp` and the docs to use `cos connect` and
the daemon, so that the documented path is the working one.

- [ ] Test: `task --dry install-mcp` prints `cos connect <client> --repair` for each installed client and no `jq` or `codex mcp add` (`tests/kit.rs` or a new `tests/taskfile.rs`), written first and failing
- [ ] `docs/design.md` MCP section: the daemon, run tools, business per call, links; `README.md` and `docs/usage.md` updated
- [ ] All tests green: `cargo test --test taskfile`

### Functional requirements

- FR-1: A paid MCP tool returns within 2 s with a run id, and a link when the business is registered.
- FR-2: One business never has two briefs running at once, whoever starts them.
- FR-3: Every process group a run created is gone before its final state is written.
- FR-4: Each local MCP tool acts on the named business, the pin, or the registry default read per call.
- FR-5: The remote MCP server stays `latest_brief` only, on one business, with no daemon client.
- FR-6: `cos connect` changes only the `cos` entry in a client config and keeps the pre-`cos` original.
- FR-7: A `cos://` link only navigates, within a closed grammar.

### Non-goals

- MCP resource subscriptions.
- Links from the app into Claude Desktop or Codex.
- Progress notifications.
- Routing the scheduled brief through the daemon.

### Open questions

- US-001's spike may change the design's cleanup mechanism; if so, that part
  returns to Spec for approval before US-005 starts.
