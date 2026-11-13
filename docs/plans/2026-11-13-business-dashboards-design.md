# Business sidebar and per-business dashboards

*   **Author:** Decebal Dobrica
*   **Status:** In Review
*   **Date:** 2026-11-13
*   **Brief:** `docs/plans/2026-11-13-business-dashboards-brief.md`
*   **Base design:** `docs/plans/2026-11-12-multi-business-design.md`

--------------------------------------------------------------------------------

## Decision

The desktop app gets a left sidebar listing every registered business, with three signals per row (live agents, unread brief, lost runs) and a clock on the one business launchd schedules. Clicking a row runs the same switch as ⌘K. Each business may declare dashboards in a new file, `~/.config/cos/dashboards.toml`, which no `cos` command writes. A dashboard is a loopback URL with an identity probe, and optionally a server binary the app owns. The app starts an owned server the first time one of its dashboards opens, records a pidfile, and stops it when the app exits. A listener the app did not start is embedded only after its identity probe passes, and is never killed. Founding Bets declares the control room and the salesroom (one owned server, `portfolio-control-room serve 127.0.0.1:4777`); Longhand declares its gazette at `:8790`, which launchd owns. Four businesses are registered: `founding-bets`, `longhand`, `realestate` and `kronos`.

Two answers in the brief are superseded by the owner's later decisions: frames are allowed from `http://127.0.0.1:*` rather than two exact origins (brief D6 and its "wildcard frame origins" non-goal), and the executable-path check is used only to reclaim the app's own server, with external listeners identified by a page marker (brief D5).

This design supersedes one non-goal of the base design (`2026-11-12-multi-business-design.md:58`, "a dashboard … that follows the app's switch"): dashboards here follow the app's business. Per-business ports and launchd labels stay out of scope.

## Overview

The owner runs four businesses from pinned Codex threads. Each has a chief-of-staff workspace, and two have a dashboard of their own: the Founding Bets control room and salesroom (`~/Projects/founder-mode/portfolio-control-room/control-room`, a Rust server on `127.0.0.1:4777`) and the Longhand gazette (a node server on `127.0.0.1:8790`, run by the launchd job `com.longhand.chief-of-staff-dashboard`). The desktop app shows neither, and it shows only one business at a time, so nothing in the window says which business needs attention.

This design adds three things to the app: a sidebar of businesses with cheap status signals, a dashboard pane per business, and ownership of the one dashboard server the owner wants the app to run. It adds one file and one subcommand to `cos`, and registers the four businesses.

## The Problem

- **No overview.** ⌘K lists businesses with a live-run count, but only while the picker is open. The header's "N live in other businesses" badge refreshes only when the Agents tab opens (`apps/desktop/ui/app.js:141`). A new brief, a lost run or the scheduled business is invisible.
- **Dashboards live outside the app.** The control room has no installed binary and no service; it runs only as `cargo run -- serve` from its repo, started by hand. The gazette runs under launchd but is a browser tab.
- **The app cannot embed them today.** The window's CSP allows frames only from `cosreport:` (`apps/desktop/tauri.conf.json:22`).
- **Four workspaces are not registered.** The registry holds `acme` and `default` only. Kronos has no workspace at all.

## Goals

1. `cos business list` shows `founding-bets`, `longhand`, `realestate` and `kronos`, each `dirs ok, profile ok`.
2. The app shows a sidebar listing every registered business. A click switches the app's business through `switch_business`, with the same refusals as ⌘K (pinned, unknown name, missing dirs, missing profile) and the same unsaved-Hire guard.
3. Each sidebar row shows, without spawning a process on the refresh path:
   - the number of live agent runs,
   - a dot when the business has a brief newer than the last one opened in the app,
   - a warning when it has lost runs,
   - a clock when it is the business named by the installed `cos.brief` plist for the same registry.
4. A business with dashboards shows one tab per dashboard. Founding Bets shows "Control room" (`http://127.0.0.1:4777/`) and "Salesroom" (`http://127.0.0.1:4777/sales`); Longhand shows "Gazette" (`http://127.0.0.1:8790/#staff`).
5. Opening a dashboard whose server the app owns starts at most one server process, however many of its dashboards open. Quitting the app stops every server the app started, and leaves no process on the port.
6. A listener the app did not start is embedded when its identity probe passes, refused with a visible reason when it fails, and never stopped by the app.
7. After a crash or force-quit, the next launch finds the server it started by pidfile and either reclaims it as owned or removes the stale pidfile.
8. Every dashboard state is visible in its pane: starting, running (owned), running (external), refused (identity), binary missing, failed to start, stopped.

## Non-Goals

- The app building a dashboard server. The app never runs `cargo`; a missing binary shows the build command.
- Bundling a dashboard binary in the app (`externalBin` stays empty).
- The revenue desk (`:4788`). The owner chose the control room and salesroom only.
- Starting, stopping or restarting a launchd-owned dashboard (the Longhand gazette).
- Scheduling more than one business, per-business launchd labels, and moving the `cos dash` port.
- A generic plugin system for health checks, or non-loopback dashboard URLs.
- Dashboards that follow the CLI's active business. Only the app's business counts.
- A display name separate from the business slug.

## Open Questions

- **Q1 (spike, before build):** does WKWebView load an `http://127.0.0.1` iframe inside the `tauri://localhost` page without a mixed-content block, and is `window.__TAURI__` undefined inside that frame? The design assumes yes to the first and requires yes to the second; the spike settles both.
- **Q2:** the sidebar's order. Assumed: registry order as written in `businesses.toml`, the app's business highlighted.

## Critical User Journeys (CUJs)

### CUJ 1: See which business needs attention

*   **Given**: four businesses are registered; `longhand` has a brief written after the one last opened in the app; `founding-bets` has one live agent run.
*   **When**: the owner opens the app.
*   **Then**: the sidebar shows four rows. `longhand` has the unread dot; `founding-bets` shows `1`. No row's signals required a process spawn to compute.

### CUJ 2: Switch from the sidebar

*   **Given**: the app is on `default`, not pinned by env.
*   **When**: the owner clicks `founding-bets`.
*   **Then**: the app switches exactly as ⌘K does: the header reads `business founding-bets`, the Today report is Founding Bets', and `cos business list` still marks the registry's active business unchanged.

### CUJ 3: Open the control room, then the salesroom

*   **Given**: `founding-bets` declares the control room and the salesroom on one owned server; nothing listens on `:4777`; the binary exists.
*   **When**: the owner opens "Control room", then "Salesroom".
*   **Then**: the first open starts one `portfolio-control-room` process and writes its pidfile; the pane shows the control room. The second open starts nothing; `lsof -nP -iTCP:4777 -sTCP:LISTEN` still lists one pid.

### CUJ 4: Quit stops what the app started

*   **Given**: CUJ 3 has run.
*   **When**: the owner quits with ⌘Q.
*   **Then**: `:4777` is free, the pidfile is gone, and no `portfolio-control-room` process has PPID 1.

### CUJ 5: A stranger holds the port

*   **Given**: some other program listens on `:4777`.
*   **When**: the owner opens "Control room".
*   **Then**: the pane says the port is held by something that failed the identity probe, names the port and the next step, and the app kills nothing.

### CUJ 6: The gazette, owned by launchd

*   **Given**: the gazette's launchd job is running.
*   **When**: the owner opens Longhand's "Gazette", then quits the app.
*   **Then**: the pane shows the gazette after its identity probe passes. The gazette's pid in `launchctl list com.longhand.chief-of-staff-dashboard` is the same before the app started and after it quit.

### CUJ 7: Recover after a crash

*   **Given**: the app started the control room, then was killed with `kill -9`.
*   **When**: the app launches again.
*   **Then**: the pidfile names a live process whose executable is the declared binary, so the app reclaims it as owned; the next quit stops it. If the pidfile names a dead pid, the app removes the pidfile.

## Technical Plan (Architecture)

### Terms

- **Business:** a registry entry, as in the base design.
- **Dashboard:** a named page shown in the app for one business: a loopback URL plus an identity probe.
- **Server:** a process that serves one or more dashboards. A server is **owned** when `dashboards.toml` gives it a binary, and **external** otherwise.
- **Owned (started):** an owned server this app instance spawned or reclaimed. Only these are stopped on exit.

### Components

```text
~/.config/cos/dashboards.toml ──read──► cos::dashboards (parse, validate) ◄── cos business dashboards
                                              │
                                              ▼
apps/desktop  ──────────────────────────────────────────────────────────────────────────
  sidebar (ui/sidebar.js) ◄─ sidebar-changed ─ watcher thread (lib.rs watch)
        │ click                                   │ stamps: every business's agents root,
        ▼                                         │ out/briefs, the cos.brief plist
  switch_business (unchanged)                     ▼
                                           cos::signals (live via proc::alive, unread, lost, scheduled)
  dashboard pane (ui/dash.js)
        │ open(name)
        ▼
  dashboard_open ─► servers::Supervisor ─► probe / spawn / reclaim ─► pidfile (app data dir)
                          ▲
  RunEvent::Exit ─────────┘ stop every Owned server (group SIGTERM, deadline, SIGKILL, reap)
```

### 1. `dashboards.toml` (cos crate, new `src/dashboards.rs`)

The file lives next to the registry: `COS_DASHBOARDS`, else the registry's directory (the parent of `business::location()`, `src/business.rs:41`) joined with `dashboards.toml`. No `cos` command writes it; it is hand-edited. A missing file means no dashboards.

```toml
[server.control-room]
binary = "/Users/decebaldobrica/Projects/founder-mode/portfolio-control-room/target/release/portfolio-control-room"
addr = "127.0.0.1:4777"
ready = { path = "/health", contains = "\"status\":\"ok\"" }

[[business.founding-bets]]
name = "Control room"
url = "http://127.0.0.1:4777/"
server = "control-room"
probe = { path = "/", contains = "<title>Portfolio control room" }

[[business.founding-bets]]
name = "Salesroom"
url = "http://127.0.0.1:4777/sales"
server = "control-room"
probe = { path = "/sales", contains = "sales.js" }

[[business.longhand]]
name = "Gazette"
url = "http://127.0.0.1:8790/#staff"
probe = { path = "/", contains = "<title>" }
```

The exact probe markers are read from the live pages during Build and recorded in the runbook; the values above show the shape.

Validation, all at parse time (`dashboards::load` returns the first error with the key path):

- every URL is `http://127.0.0.1:<port>/...`; any other scheme or host is refused;
- a dashboard with `server` names a declared server, and its URL's port equals that server's `addr` port;
- a server's `binary` is absolute; `addr` is `127.0.0.1:<port>`; the argv is fixed as `[binary, "serve", addr]` and is not configurable;
- names are unique per business; business keys follow the registry's name rule;
- a business key not in the registry is reported as orphaned by `cos business dashboards`, not an error, so renaming a business never breaks the file.

The binary's existence and file type are checked when it is started, not at parse time, so `cos business list` works on a machine without the binary.

`cos business dashboards [<name>]` prints each dashboard, its server, and whether its URL answers its probe now (one HTTP GET, 3 s timeout). It never starts anything.

### 2. Server supervisor (desktop, new `apps/desktop/src/servers.rs`)

`Supervisor` is managed Tauri state: `Mutex<HashMap<ServerKey, Slot>>`, keyed by the server's `addr`. A key per address, not per name, so two names for one port cannot start two processes.

`ensure(server) -> Result<Running, Refusal>` is single-flight under the mutex:

1. **Slot already Owned and its child alive:** return it.
2. **Pidfile present** (`<app data dir>/servers/<port>.json`, `{pid, start_time, exe, addr, app_pid}`):
   - The pid is alive, its start time equals the recorded one (`proc_pidinfo`), its executable path equals the declared binary (`proc_pidpath`), and the recorded `app_pid` is not alive. Then the server is **reclaimed as Owned**: the pidfile is rewritten with this app's pid, and readiness is probed.
   - The `app_pid` is alive and is not this process: another app instance owns the server. Treat it as external; never stop it.
   - Otherwise the pidfile is stale and is removed.
3. **Port answers:** a TCP connect to `addr` succeeds. Run the dashboard's identity probe: an HTTP GET of `probe.path` with a 3 s timeout, retried once because the control room serves one connection at a time with 2 s timeouts. If the body contains the marker, return **External** (embedded, never stopped); otherwise refuse with `identity`, giving what answered (status line and first 200 bytes).
4. **Port free:**
   - Check the binary: it must exist, be a regular executable file, and not be inside any registered business's `out/` directory. Otherwise refuse with `binary missing` and the build command.
   - Spawn with `proc::spawn_grouped` (own process group), stdin null, stdout and stderr appended to `<app log dir>/servers/<port>.log`. Write the pidfile atomically (`state::atomic_write_bytes`).
   - A reaper thread owns `child.wait()` and marks the slot Exited with its status, so the process is never left a zombie.
   - Poll `ready` every 250 ms until it passes or 10 s elapse. On timeout, stop the group and refuse with `failed to start` plus the log's last 20 lines.

`stop_owned()` runs from the `RunEvent::Exit` handler, and also from `ExitRequested`, so ⌘Q and the window's close both reach it:

- for each Owned slot: `kill(-pgid, SIGTERM)`;
- wait up to 5 s shared across all slots (one deadline, not one per server);
- then `kill_group` (SIGKILL) for the survivors;
- reap each child, and remove its pidfile.

An External slot is never signalled.

`apps/desktop/src/main.rs` changes from `.run(context)` to `.build(context)?.run(|app, event| ...)`, so the exit events reach the supervisor.

### 3. Dashboard pane (desktop UI)

- A new tab group appears in the header when the app's business has dashboards: one tab per dashboard, after Docs. The data comes from the `dashboards` IPC, which returns the app business's entries.
- Opening a tab calls `dashboard_open(business, name)`. It is guarded by `named()` like the other business-scoped commands, so a stale business is refused. The pane shows the state while `ensure` runs, then sets the iframe `src` only on `Running`.
- The iframe is `sandbox="allow-scripts allow-same-origin allow-forms allow-popups"`, without `allow-top-navigation`. The control room's own CSP (`connect-src 'self'`) needs a real origin, which is why `allow-same-origin` is set.
- The frame is destroyed on business switch (the page reloads on switch, as today); the server keeps running until exit.
- CSP in `tauri.conf.json`: `frame-src` gains `http://127.0.0.1:*`. Nothing else is added, and `connect-src` is unchanged: the app's own page never fetches the dashboards; only the frames do.
- The `cosreport` handler refuses any request whose `Origin` header is present and is not the app's own origin (`tauri://localhost`, `http://tauri.localhost`, `cosreport://localhost`, `http://cosreport.localhost`, or `null` from the opaque report frames). That closes the guessable `x-cos-business` header (`lib.rs:479-484`) to a loopback dashboard frame.
- The capability file (`capabilities/default.json`) stays `windows: ["main"]` with no `remote` URLs, so IPC from `http://127.0.0.1` frames is denied. Spike Q1 confirms `window.__TAURI__` is undefined in such a frame.

### 4. Sidebar and signals

**Rendering.** `ui/index.html` wraps the header and `<main>` in a column beside a new `<aside id="sidebar">`. The `--sidebar*` tokens in `tokens.css:90-97` style it. Rows are in registry order (Q2), and the app's business is highlighted. A click calls the existing `switchTo(name)` in `app.js`. A pinned app (`pinned_by` set) disables the rows and shows the same "pinned by …" title as ⌘K.

**Signals.** Computed by a new `cos::signals` module, pure over file metadata, never `ps`:

- **live:** for each run directory, use the run's state and `classify`, but replace `is_cos` with `proc::alive(supervisor)`. A recycled pid can show a run as live until the next full check, which is accepted for a sidebar count. The Agents tab keeps the exact count.
- **lost:** the number of runs `classify` marks Lost.
- **unread:** the newest `out/briefs/*.json` by filename, compared as `(filename, mtime)` with `lastSeenBrief[business]` in `<app config dir>/ui-state.json`.
  - A business never opened, with any brief, is unread.
  - `brief_seen(business)` writes the pair when the Today view has rendered that business's brief. It is guarded by `named()` and written with `atomic_write_bytes`.
  - An unparseable `ui-state.json` reads as empty.
  - Entries for unregistered names are dropped on write.
- **scheduled:** parse `~/Library/LaunchAgents/cos.brief.plist` as XML. The business is the string after `--business` in `ProgramArguments`, and it counts only when `EnvironmentVariables.COS_BUSINESSES` equals the app's registry path.
  - A plist without `--business` (legacy, `COS_HOME`) marks no business.
  - A binary plist or a parse error marks none and logs once.
  - The plist being on disk does not prove the job is loaded, so the tooltip says "installed for <name>".

**Refresh.** The watcher thread (`lib.rs:891-909`) keeps its 700 ms tick:

- it adds one stamp per business: `cos::dash::stamp([agents::root(home), home/out/briefs])` (metadata only), plus the plist file's mtime;
- when a stamp changes it recomputes that business's signals and emits `sidebar-changed` with all rows;
- the initial rows come from the `sidebar` IPC;
- `businesses()` (which spawns `ps`) is never called on the tick.

### 5. Registration and setup (runbook, not code)

The four businesses are registered with `cos business add`:

| Name | home | workspace | projects |
|---|---|---|---|
| founding-bets | `~/Projects/founder-mode` | `~/Projects/founder-mode` | `~/Projects/founder-mode` |
| longhand | `~/Projects/fractional-projects/longhand` | same | `~/Projects/fractional-projects` |
| realestate | `~/Projects/founder-mode/real-estate-chief-of-staff` | same | `~/Projects/founder-mode` |
| kronos | `~/Projects/chronos` | same | `~/Projects/chronos` |

- `kronos` first gets `cos --workspace ~/Projects/chronos state init`, so `switch_business` finds a profile.
- `realestate`'s home is nested inside `founding-bets`'s. `cos business add` prints a warning naming the outer business and continues.
- The control-room binary is built once by hand: `cargo build --release` in `~/Projects/founder-mode/portfolio-control-room/control-room`.
- `dashboards.toml` is written from the example above, with the probe markers read from the live pages.

### 6. `task schedule` and port 8790

`cos.dash` defaults to `127.0.0.1:8790`, the port the Longhand gazette holds. `task schedule` boots out the old `cos.dash`, then refuses to bootstrap it again when `:8790` is still listening. It prints which process holds the port and that the brief job was loaded alone. The brief job is unaffected.

## Alternatives Considered

### Alternative A: launchd owns the control room

*   **Pros**: no supervisor, exit hook, pidfile or reclaim code in the app; survives an app crash by design; matches how the Longhand gazette already runs.
*   **Cons**: a plist and an install step in the founder-mode repo; the server runs whether or not the app is open.
*   **Reason for Rejection**: Rejected by author: the owner wants the app to start the control room on demand and stop it on quit (decision U4, reconfirmed after the supervisor cost was shown).

### Alternative B: dashboards as fields in `businesses.toml`

*   **Pros**: one file per business; `cos business add` could set them.
*   **Cons**: `business::write` serialises only the known fields (`src/business.rs:116-123`) and `canonical()` rebuilds entries from three dirs (`:162-168`), so any older `cos` binary, including the one launchd runs from `~/.cargo/bin`, erases them on the next `add`, `use` or `remove`.
*   **Reason for Rejection**: Rejected by author: a separate file is the only option that no binary version can rewrite (decision U6).

### Alternative C: dashboards in each workspace's `profile.json`

*   **Pros**: travels with the workspace; `cos` never writes the profile.
*   **Cons**: the profile is the business's own preference record, shared with agents; a binary path there lets any agent with workspace write access choose what the app executes. Founding Bets' profile already carries an unrelated `dashboardUrl` (`:4788`) that would conflict.
*   **Reason for Rejection**: Rejected by author: exec configuration stays in the user's config directory, outside every workspace.

### Alternative D: exact frame origins in the CSP

*   **Pros**: the build allows only the two known ports.
*   **Cons**: every new dashboard port needs a rebuild of the app.
*   **Reason for Rejection**: Rejected by author: loopback on any port, with URLs validated to `127.0.0.1` and loaded only after the identity probe, keeps dashboards as data.

### Alternative E: a child webview without capabilities

*   **Pros**: IPC isolation by construction, independent of the frame's CSP.
*   **Cons**: Tauri's multi-webview API is behind the `unstable` feature; layout and resize are kept in sync by hand.
*   **Reason for Rejection**: Rejected by author: the iframe plus a capability file with no remote URLs gives the same isolation on the stable API; spike Q1 confirms it.

### Alternative F: "open in browser" instead of embedding

*   **Pros**: no CSP change, no embed risk; `open` is already used (`src/report.rs:241`).
*   **Cons**: the dashboards stay outside the app, which is the problem being solved.
*   **Reason for Rejection**: Rejected by author: the owner asked for the dashboards inside the app.

### Alternative G: identity by `/health` fields in the control room

*   **Pros**: a server-declared identity (service name and root) instead of a page marker.
*   **Cons**: a change in a second repository; does not help the gazette.
*   **Reason for Rejection**: Rejected by author: a per-dashboard page marker covers both servers with no change outside this repo, and ownership is proven by the kernel's view of the pid, not by the page.

### Alternative H: one guard process per server (shim)

*   **Pros**: the server dies with the app even after `kill -9`, through a pipe the shim watches.
*   **Cons**: a second long-lived process per server and a new `cos` subcommand to supervise.
*   **Reason for Rejection**: Rejected by author: the pidfile reclaim covers the crash case on the next launch with less machinery; the server keeps running between a crash and the next launch, a gap this design states rather than closes.

## Semantic ISA

### Side-effect inventory

| # | Side effect | Where | Reversible? | Who is affected if it misfires |
|---|---|---|---|---|
| S1 | Spawn a server process from a binary named in `dashboards.toml` | `servers::ensure` | yes: stop the group | the owner: arbitrary code runs as them if the path is wrong |
| S2 | Signal a process group (SIGTERM, SIGKILL) | `servers::stop_owned`, failed-start cleanup | no | any process whose pid is reused or wrongly adopted |
| S3 | Write and remove a pidfile | `<app data dir>/servers/<port>.json` | yes | the app's own recovery |
| S4 | Append a server log | `<app log dir>/servers/<port>.log` | yes | disk |
| S5 | HTTP GET to a loopback port (probe, ready) | `servers` | yes, read only | the local service probed |
| S6 | Load a loopback URL in an iframe | `ui/dash.js` | yes | the owner, if a foreign page is shown under a dashboard's name |
| S7 | Write `ui-state.json` (`lastSeenBrief`) | `brief_seen` | yes | the unread dot only |
| S8 | `cos business add` warning for nested homes | `business.rs` | n/a, output only | none |
| S9 | `task schedule` refuses `cos.dash` when 8790 is held | `Taskfile.yml` | yes | the scheduled dash |
| S10 | Refuse foreign-Origin requests on `cosreport` | `lib.rs` scheme handler | yes | a frame that tried to reach the report scheme |

No network call leaves the machine: every URL is validated to `127.0.0.1`. Nothing new is sent to a model.

### Instructions

| Instruction | Core | Governance property | Sink? | Binding (closed schemas) |
|---|---|---|---|---|
| `LOAD(dashboards.toml)` | Memory | deterministic parse + validation | no | in: path; out: `Dashboards {servers: map<name, Server{binary: abs path, addr: 127.0.0.1:port, ready: Probe}>, business: map<name, [Dashboard{name, url: loopback, server?: name, probe: Probe}]>}` |
| `PROBE(addr, probe)` | Execution | deterministic I/O, read-only, 3 s timeout | no | in: `{addr: 127.0.0.1:port, path: string, contains: string}`; out: `{answered: bool, matched: bool, status: u16?, head: string≤200}` |
| `VERIFY(binary_admissible)` | Normative | deterministic checkpoint | no | in: `Server`; out: admit or refusal `{rule, path}` |
| `VERIFY(ownership)` | Normative | deterministic checkpoint | no | in: `{slot, pidfile?}`; out: `Owned{pid, pgid} \| External \| Stale` |
| `VERIFY(identity)` | Normative | deterministic checkpoint over `PROBE` output | no | in: `{probe result, marker}`; out: admit or refusal `{rule: identity, head}` |
| `TOOL_CALL(spawn_server)` | Execution | deterministic action | **yes** (S1) | in: `{binary, addr}` with argv fixed `[binary, serve, addr]`; out: `{pid, pgid, start_time}` |
| `TOOL_CALL(signal_group)` | Execution | deterministic action | **yes** (S2) | in: `{pgid, signal: TERM\|KILL}`; out: unit |
| `STORE(pidfile)` | Memory | atomic write | **yes** (S3) | in: `{pid, start_time, exe, addr, app_pid}`; out: unit |
| `RENDER(iframe src)` | Execution | UI action | **yes** (S6) | in: `{url: loopback}`; out: unit |
| `STORE(ui-state)` | Memory | atomic write | **yes** (S7) | in: `{business: registered name, file: YYYY-MM-DD.json, mtime: i64}`; out: unit |
| `VERIFY(origin_allowed)` | Normative | deterministic checkpoint | no | in: request `Origin`; out: admit or 403 |

### Taint

- **Sources:**
  - `dashboards.toml`, which is hand-edited, and agents with file access could write it;
  - the pidfile, which is on disk and could be stale or forged;
  - every HTTP response from a loopback port, since any local process can answer;
  - the launchd plist;
  - `ui-state.json`;
  - request `Origin` headers.
- **Propagation:** a `PROBE` result is tainted. A pidfile read is tainted until `VERIFY(ownership)` checks it against the kernel's view of the pid (`proc_pidinfo` start time, `proc_pidpath`).
- **Sinks:** `spawn_server`, `signal_group`, `STORE(pidfile)`, `RENDER(iframe src)`, `STORE(ui-state)`.

### VERIFY before every sink

| Sink | Cleared by | Check |
|---|---|---|
| `TOOL_CALL(spawn_server)` | `VERIFY(binary_admissible)` | path absolute, exists, regular file, executable bit set, not under any registered business's `out/`; addr `127.0.0.1:<port>`; argv fixed; port not answering |
| `TOOL_CALL(signal_group)` | `VERIFY(ownership)` = Owned | the slot was spawned by this process (child handle held), or reclaimed: pid alive, start time equals the pidfile's, exe equals the declared binary, the recorded `app_pid` is dead. External and Stale never reach this sink |
| `STORE(pidfile)` | `TOOL_CALL(spawn_server)` returned, or reclaim passed `VERIFY(ownership)` | the record is built from the child handle or the verified pid, never copied from the old file |
| `RENDER(iframe src)` | `VERIFY(identity)` | the URL is one declared in `dashboards.toml` for the app's business (validated loopback), and `ensure` returned Running (Owned and ready, or External with a matched probe marker) |
| `STORE(ui-state)` | `named()` + registered-name check | the business named is the app's business; the filename matches `^\d{4}-\d{2}-\d{2}\.json$` and exists under that business's `out/briefs` |

### Violations

| Violation | Evidence recorded | Recovery |
|---|---|---|
| invalid `dashboards.toml` | key path and rule, in the pane and `cos business dashboards` | no dashboards for that file until fixed; the sidebar still works |
| binary not admissible | path and failing rule | pane shows the build command; nothing spawned |
| identity probe failed | port, status line, first 200 bytes | pane shows "port held by something else"; nothing killed |
| ownership not proven | pidfile contents, kernel start time and exe | treat as External or Stale; never signal |
| server did not become ready | log tail | stop the group it just started, remove the pidfile, show the log |
| foreign Origin on `cosreport` | origin and path, logged | 403 |
| plist unreadable or legacy | path, logged once | no clock |

### ISA test matrix

| Sink | Refused (tainted or failing input) | Admitted (VERIFY cleared) |
|---|---|---|
| `spawn_server` | relative path; missing file; non-executable file; binary under a business's `out/`; non-loopback addr; port already answering | absolute executable outside `out/`, free loopback port: one process, pidfile written |
| `signal_group` | external listener; pidfile whose pid is alive but start time differs; pidfile whose exe differs; pidfile whose `app_pid` is alive | child spawned by this supervisor; reclaimed pid with matching start time and exe and dead `app_pid` |
| `STORE(pidfile)` | write attempted from an unverified pidfile read | written from the child handle after spawn |
| `RENDER(iframe src)` | listener whose body lacks the marker; URL not declared for the app business; non-loopback URL in the file (parse refusal) | owned server ready; external listener with the marker |
| `STORE(ui-state)` | stale business name; filename outside `out/briefs`; non-date filename | app business, existing brief file |
| `VERIFY(origin_allowed)` | `Origin: http://127.0.0.1:4777` POST to `cosreport://localhost/docs` | app origin; `null` from the report frame |

## Non-Functional Requirements

- **Security:** covered by the Semantic ISA. One stated gap: between an app crash and the next launch, an owned server keeps running unsupervised on loopback.
- **Performance:** the watcher tick adds one `stamp` per business, which costs a `read_dir` plus metadata on two directories. With four businesses that is eight directory reads per 700 ms, and no process spawns. The supervisor's probe has a 3 s timeout and runs only on open.
- **Process ownership:** every spawned server has an owner (the supervisor), a reaper thread, a pidfile, and one shared exit deadline. Nothing is spawned on a render, tick or keystroke path.
- **Rollout and rollback:** `dashboards.toml` absent means the app behaves as today, plus the sidebar. Rolling back the app leaves `dashboards.toml` and `ui-state.json` behind, unread by older builds. The registry format is unchanged.

## Confidence

| Piece | Tier | What moves it up |
|---|---|---|
| `dashboards.toml` parse and validation | rock | n/a |
| Sidebar rendering and switch | rock | n/a |
| Signals from metadata | rock | n/a |
| Supervisor spawn, ready, stop on exit | rock | the fake-server test suite in Build |
| Reclaim after crash (start time + exe) | sand | a test that kills the app process with SIGKILL and relaunches against the same pidfile |
| Iframe embed of loopback pages inside `tauri://` | sand | spike Q1: load `http://127.0.0.1:<port>` in the frame in the built app and read `typeof window.__TAURI__` from it |
| Identity probe markers for the two real pages | sand | read the live pages once the binary is built; record the markers in the runbook |
| `task schedule` 8790 guard | rock | n/a |

## Detailed Implementation Plan

### Target Files to Create/Modify

#### 1. `src/dashboards.rs` (Create)

*   **Type / Scope**: cos library module.
*   **Proposed Changes**:
    *   Types: `Dashboards { servers: BTreeMap<String, Server>, business: BTreeMap<String, Vec<Dashboard>> }`, `Server { binary: PathBuf, addr: SocketAddrV4, ready: Probe }`, `Dashboard { name: String, url: Url-as-String, server: Option<String>, probe: Probe }`, `Probe { path: String, contains: String }`. All `#[serde(deny_unknown_fields)]`.
    *   `location() -> Result<PathBuf>`: `COS_DASHBOARDS`, else the parent of `business::location()`.
    *   `load(path) -> Result<Dashboards>`: a missing file gives empty; otherwise parse and validate per the Technical Plan. Errors name the TOML key path.
    *   `for_business(&self, name) -> &[Dashboard]` and `server_of(&self, d) -> Option<&Server>`.
    *   `probe(addr, probe, timeout) -> ProbeResult`: a plain `TcpStream` HTTP/1.0 GET, read up to 64 KiB, `matched` when the body contains the marker. No new HTTP client dependency.
    *   `orphans(&self, registry) -> Vec<String>`: business keys not in the registry.

#### 2. `src/signals.rs` (Create)

*   **Type / Scope**: cos library module, pure over the filesystem; no process spawn.
*   **Proposed Changes**:
    *   `Signals { live: usize, lost: usize, latest_brief: Option<(String, i64)> }`.
    *   `read(home) -> Signals`: walks `agents::root(home)` with the same run-reading code as `agents::count` and calls `agents::classify`, but decides "live" with `proc::alive` on the supervisor pid instead of `is_cos`. `latest_brief` is the max `out/briefs/*.json` filename with its mtime.
    *   `scheduled(plist: &Path, registry: &Path) -> Option<String>`: XML plist reader for the two keys used (`ProgramArguments` strings, `EnvironmentVariables.COS_BUSINESSES`). It returns `None` for a binary plist, a parse error, no `--business`, or a registry mismatch.
    *   `stamp(home) -> String`: `dash::stamp([agents::root(home), home.join("out/briefs")])`.

#### 3. `src/agents/mod.rs` (Modify)

*   **Type / Scope**: cos library.
*   **Proposed Changes**: expose the run walk used by `count()` (`:561-592`) as `pub fn runs(home) -> impl Iterator<Item = (Spec, State)>` so `signals::read` reuses it. `count()` keeps `is_cos`. No behaviour change.

#### 4. `src/lib.rs` (Modify)

*   **Type / Scope**: crate root.
*   **Proposed Changes**: `pub mod dashboards; pub mod signals;`.

#### 5. `src/business.rs` (Modify)

*   **Type / Scope**: cos library.
*   **Proposed Changes**: `add` returns the name of any existing business whose home contains, or is contained by, the new home (`Path::starts_with` on canonical paths), so the CLI can warn. It still refuses equal homes as today (`:282-287`).

#### 6. `src/main.rs` (Modify)

*   **Type / Scope**: CLI.
*   **Proposed Changes**:
    *   `BusinessCmd::Add` prints `warning: <home> is inside <other>'s home` (or contains it) when `add` reports one.
    *   New `BusinessCmd::Dashboards { name: Option<String> }`: prints each business's dashboards. Each line shows the business, name, URL, server (or `external`), and a probe result: `answers`, `answers, marker missing`, or `no listener`. Orphaned keys are listed at the end. The exit code is 0 unless the file is invalid.

#### 7. `apps/desktop/src/servers.rs` (Create)

*   **Type / Scope**: desktop module, the only place that spawns or signals a server.
*   **Proposed Changes**:
    *   `Supervisor` (managed state) holds `Mutex<HashMap<SocketAddrV4, Slot>>`, the pidfile dir and the log dir.
    *   `Slot` is one of `Owned { child: Arc<Mutex<Option<Child>>>, pgid, started }`, `Reclaimed { pid, pgid, start_time }`, `External`, or `Exited(status)`.
    *   `ensure(&self, server: Option<&Server>, d: &Dashboard) -> Result<Running, Refusal>` follows the five steps in the Technical Plan.
    *   `Refusal` is an enum `{ Identity{status, head}, BinaryMissing{path, build_hint}, NotAdmissible{rule}, FailedToStart{log_tail}, ForeignOwner }` that serialises to the pane.
    *   `stop_owned(&self)`: group SIGTERM, one shared 5 s `proc::Deadline`, then `proc::kill_group`, reap, and remove the pidfiles.
    *   Kernel checks: `start_time(pid)` via `libc::proc_pidinfo(PROC_PIDTBSDINFO)` (`pbi_start_tvsec`, `pbi_start_tvusec`), and `exe(pid)` via `libc::proc_pidpath`.
    *   `admissible(server, registry)` implements `VERIFY(binary_admissible)`.

#### 8. `apps/desktop/src/lib.rs` (Modify)

*   **Type / Scope**: desktop commands, scheme handler, watcher, builder.
*   **Proposed Changes**:
    *   New commands:
        *   `sidebar() -> Vec<SidebarRow{name, app, live, lost, unread, scheduled, dirs}>`;
        *   `dashboards(business) -> Vec<DashboardRow{name, url, owned}>`;
        *   `dashboard_open(business, name) -> Result<Running, Refusal>`;
        *   `brief_seen(business, file)`.

        `dashboards`, `dashboard_open` and `brief_seen` call `named()` first. All four are registered in `generate_handler!`.
    *   `watch()` (`:891-909`): keeps the per-business `signals::stamp` and the plist mtime, recomputes changed rows, and emits `sidebar-changed`.
    *   The `cosreport` handler (`:963-974`) calls `origin_allowed(&req)` first and responds 403 otherwise.
    *   `builder()` manages `Supervisor::new(app data dir, app log dir)` in `setup`.
    *   New `pub fn on_run_event(app, event)` calls `stop_owned()` on `ExitRequested` and `Exit`.
    *   A `ui_state` helper reads and writes `<app config dir>/ui-state.json` with `atomic_write_bytes`.

#### 9. `apps/desktop/src/main.rs` (Modify)

*   **Type / Scope**: binary entry.
*   **Proposed Changes**: `.build(generate_context!())` then `.run(cos_desktop::on_run_event)`.

#### 10. `apps/desktop/tauri.conf.json` (Modify)

*   **Type / Scope**: config.
*   **Proposed Changes**: `frame-src cosreport: http://cosreport.localhost http://127.0.0.1:*`. Nothing else.

#### 11. `apps/desktop/ui/index.html` (Modify)

*   **Type / Scope**: markup.
*   **Proposed Changes**: wrap the existing header and `<main>` in `<div class="shell-main">` beside `<aside id="sidebar" aria-label="Businesses">`. Add a `#dash-tabs` slot in the header after Docs, and a `#view-dash` view holding `#dash-state` and `<iframe id="dash-frame" sandbox="allow-scripts allow-same-origin allow-forms allow-popups">`.

#### 12. `apps/desktop/ui/sidebar.js` (Create)

*   **Type / Scope**: UI module.
*   **Proposed Changes**: `renderSidebar(rows)` builds one button per row with the live count, unread dot, lost warning and clock. Each signal has text for screen readers. A click calls `window.switchTo(name)`. It listens for `sidebar-changed` and loads initial rows via `invoke("sidebar")`. When pinned, rows are disabled with the pin title.

#### 13. `apps/desktop/ui/dash.js` (Create)

*   **Type / Scope**: UI module.
*   **Proposed Changes**: `loadDashTabs()` invokes `dashboards` and renders tabs. `openDash(name)` shows "starting…", invokes `dashboard_open`, then sets `#dash-frame.src` on Running or renders the refusal card (reason, port, what answered, next command). The frame is cleared on hide.

#### 14. `apps/desktop/ui/app.js` (Modify)

*   **Type / Scope**: UI.
*   **Proposed Changes**: expose `switchTo` on `window`; route the dashboard view in the existing view switcher; load `sidebar.js` and `dash.js`. After the Today report posts `cos-report-ready` (`:189-202`), call `brief_seen` with the brief file the report rendered.

#### 15. `apps/desktop/ui/style.css` (Modify)

*   **Type / Scope**: styles.
*   **Proposed Changes**:
    *   the body becomes a row: sidebar of fixed width 200 px, then the main column;
    *   row states (app business, hover, disabled);
    *   signal glyphs;
    *   the refusal card.

    It uses only the existing tokens.

#### 16. `Taskfile.yml` (Modify)

*   **Type / Scope**: build and schedule tasks.
*   **Proposed Changes**: in `schedule`, after the `cos.dash` bootout, skip bootstrapping `cos.dash` when `lsof -nP -iTCP:8790 -sTCP:LISTEN` finds a listener, and print the holder. The brief job is loaded either way.

#### 17. `tests/dashboards.rs` (Create)

*   **Type / Scope**: cos integration tests.
*   **Proposed Changes**: parse and validation cases:
    *   non-loopback URL refused;
    *   unknown server refused;
    *   port mismatch refused;
    *   relative binary refused;
    *   unknown key refused;
    *   missing file reads as empty;
    *   orphan key reported;
    *   `cos business dashboards` output against a std `TcpListener` fixture that answers with and without the marker.

#### 18. `tests/signals.rs` (Create)

*   **Type / Scope**: cos integration tests.
*   **Proposed Changes**:
    *   live counted with `proc::alive`, with no `ps` spawned (asserted by putting a failing `ps` first on `PATH`);
    *   lost counted;
    *   latest brief by filename;
    *   `scheduled` returns the business only when `COS_BUSINESSES` matches, and returns `None` for a legacy plist, a binary plist and a mismatched registry.

#### 19. `tests/business.rs` (Modify)

*   **Type / Scope**: cos integration tests.
*   **Proposed Changes**: a nested home warns and registers; an equal home is still refused.

#### 20. `tests/schedule.rs` (Modify)

*   **Type / Scope**: cos integration tests.
*   **Proposed Changes**: with a `TcpListener` on 8790 in the sandbox, `task schedule` loads `cos.brief` and skips `cos.dash` with the holder printed.

#### 21. `apps/desktop/tests/servers.rs` (Create, `harness = false`)

*   **Type / Scope**: desktop integration tests for the supervisor. The test binary doubles as the fake server: run as `<self> serve <addr>`, it binds `addr` and answers `/health` and `/` with a marker.
*   **Proposed Changes**: one case per ISA matrix row for `spawn_server`, `signal_group` and `STORE(pidfile)`:
    *   one process for two dashboards on one server;
    *   `stop_owned` frees the port and leaves no child;
    *   an external listener is never signalled;
    *   a pidfile with a wrong start time, wrong exe or live `app_pid` is not reclaimed;
    *   a matching pidfile from a dead app pid is reclaimed and then stopped;
    *   a server that never becomes ready is stopped and its log tail returned;
    *   a binary under a business's `out/` is refused.

#### 22. `apps/desktop/Cargo.toml` (Modify)

*   **Type / Scope**: manifest.
*   **Proposed Changes**: `[[test]] name = "servers" harness = false`; add `libc = "0.2"` (already a dependency of `cos`).

#### 23. `apps/desktop/tests/ipc.rs` (Modify)

*   **Type / Scope**: desktop IPC tests.
*   **Proposed Changes**:
    *   `sidebar` rows for three businesses, with the unread dot set and then cleared by `brief_seen`, and the clearance surviving a new `App` instance;
    *   `dashboard_open` refused for a stale business;
    *   `brief_seen` refused for a filename outside `out/briefs`;
    *   `cosreport` 403 on a foreign `Origin` and 200 on `null`;
    *   `sidebar` does not spawn `ps` (failing `ps` on `PATH`).

#### 24. `README.md` (Modify)

*   **Type / Scope**: docs.
*   **Proposed Changes**: a "Dashboards" section with the `dashboards.toml` example, the ownership rule, and `cos business dashboards`; index lines for this design, its reviews and the plan.

#### 25. `docs/plans/2026-11-13-business-dashboards-design.md` (Create)

*   **Type / Scope**: this document, moved from `design_draft.md`.

### Verification Plan

*   **Compilation/Build**: `task build`, `task desktop-check`.
*   **Unit and integration tests**: `task test` (fmt, clippy `-D warnings`, `cargo test`) and `cargo test --workspace`, which runs `apps/desktop/tests/ipc.rs` and `servers.rs`.
*   **Static analysis**: clippy `-D warnings` on both crates.
*   **ISA matrix**: every row in the test matrix above maps to a named test; the plan lists the names.
*   **E2E (built app, real machine)**:
    *   register the four businesses;
    *   build the control room; write `dashboards.toml`;
    *   run CUJ 1–7;
    *   capture `lsof -nP -iTCP:4777 -sTCP:LISTEN` before open, after two opens and after ⌘Q;
    *   capture `launchctl list com.longhand.chief-of-staff-dashboard` before and after;
    *   record screenshots with `pr-proof`.
