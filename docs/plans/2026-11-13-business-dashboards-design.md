# Business sidebar and per-business dashboards

*   **Author:** Decebal Dobrica
*   **Status:** In Review
*   **Date:** 2026-11-13
*   **Brief:** [business dashboards brief](2026-11-13-business-dashboards-brief.md)
*   **Base design:** [multi-business design](2026-11-12-multi-business-design.md)

--------------------------------------------------------------------------------

## Decision

The desktop app gets a left sidebar listing every registered business. Each row shows three signals (live agent runs, an unread brief and lost runs) and a clock on the one business that launchd schedules. Clicking a row runs the same switch as ⌘K. Each business may declare dashboards in a new hand-edited file, `~/.config/cos/dashboards.toml`, which no `cos` command writes. A dashboard is a loopback URL with an identity probe, and optionally a server binary that the app owns:

- The app starts an owned server the first time one of its dashboards opens. It records the server in a pidfile and holds a per-port lock for as long as it owns the server. It stops the server when the app exits.
- A crash, or a SIGTERM to the app, can leave an owned server running. The next launch stops that server when its pidfile still matches the process. The next launch does not adopt it.
- A listener the app did not start is embedded only after its identity probe passes. The app never signals it.
- Founding Bets declares the control room and the salesroom as one owned server, `portfolio-control-room serve 127.0.0.1:4777`. Longhand declares its gazette at `:8790`, which launchd owns.

This design registers four businesses: `founding-bets`, `longhand`, `realestate` and `kronos`.

Three answers in the [brief](2026-11-13-business-dashboards-brief.md#decisions-made-with-the-owner) are superseded by later owner decisions. Frames are allowed from `http://127.0.0.1:*`, where the brief named two exact origins (brief D6 and its "wildcard frame origins" non-goal). The executable-path check (brief D5) applies only to pidfile cleanup, and external listeners are identified by a page marker. A server left by a crashed app is shut down at launch, where brief D4 adopts it.

This design supersedes one non-goal of the base design (`2026-11-12-multi-business-design.md:58-59`, "a dashboard … that follows the app's switch"): dashboards here follow the app's business.

## Overview

The owner runs four businesses from pinned Codex threads. Each has a chief-of-staff workspace, and two have a dashboard of their own: the Founding Bets control room and salesroom (`~/Projects/founder-mode/portfolio-control-room/control-room`, a Rust server on `127.0.0.1:4777`) and the Longhand gazette (a node server on `127.0.0.1:8790`, run by the launchd job `com.longhand.chief-of-staff-dashboard`). The desktop app shows neither, and it shows only one business at a time, so nothing in the window says which business needs attention.

This design adds three things to the app: a sidebar of businesses, a dashboard pane per business, and ownership of the one dashboard server the owner wants the app to run. On macOS, status signals are computed from file metadata and kernel checks (`proc::exe`, `kill(0)`), with no process spawn. Off macOS, `exe` falls back to `ps` (Detailed Implementation Plan, item 4). It adds one config file (`dashboards.toml`), two `cos` modules (`dashboards`, `signals`) and one subcommand, and registers the four businesses.

## The Problem

- **No overview.** ⌘K lists businesses with a live-run count, but only while the picker is open. The live-run count for the other businesses, shown in the Agents bar (`#other-live`, `apps/desktop/ui/index.html:180`), refreshes only when the Agents tab opens (`apps/desktop/ui/app.js:141`). A new brief, a lost run or the scheduled business is invisible.
- **Dashboards live outside the app.** The control room has no installed binary and no service. It runs only when someone starts `portfolio-control-room serve` from its repository (`control-room/src/main.rs:49-50`). The gazette runs under launchd but is a browser tab.
- **The app cannot embed them today.** The window's CSP allows frames only from `cosreport:` and `http://cosreport.localhost` (`apps/desktop/tauri.conf.json:22`).
- **Four businesses are not registered (the registry holds `acme` and `default`), and Kronos has no workspace yet.** The registry is `~/.config/cos/businesses.toml`.
- **`cos dash` and the Longhand gazette collide on port 8790.** `cos dash` is the local report server on 8790 (`src/main.rs:58-61`), the same port as the Longhand gazette. `task schedule` loads `cos.dash` whether or not that port is held (`Taskfile.yml:211-222`).

## Goals

1. `cos business list` shows `founding-bets`, `longhand`, `realestate` and `kronos`, each `dirs ok, profile ok`.
2. The app shows a sidebar listing every registered business. A click switches the app's business through `switch_business` (`apps/desktop/src/lib.rs:326`), with the same refusals as ⌘K (pinned, unknown name, missing dirs, missing profile) and the same unsaved-Hire guard (`apps/desktop/ui/app.js:153-163`).
3. On macOS, each sidebar row shows, without spawning a process to compute it:
    - the number of live agent runs, starting runs included,
    - a dot when the business has a brief newer than the last one opened in the app, where opened means the report rendered that brief and Today was visible then or later in the same page load (§6),
    - a warning when it has lost runs,
    - a clock when it is the business named by the installed `cos.brief` plist for the same registry.
4. A business with dashboards shows one tab per dashboard. Founding Bets shows "Control room" (`http://127.0.0.1:4777/`) and "Salesroom" (`http://127.0.0.1:4777/sales`); Longhand shows "Gazette" (`http://127.0.0.1:8790/#staff`).
5. Opening a dashboard whose server the app owns starts at most one server process, however many of its dashboards open. Quitting the app stops every server the app started and leaves no process on the port. A SIGTERM or a crash that skips the quit path is covered by Goal 7.
6. A listener the app did not start is embedded when its identity probe passes, refused with a visible reason when it fails, and never signalled by the app.
7. After a crash, a force-quit or a SIGTERM, the next launch stops the server the earlier app started when the pidfile's pid, start time, executable and process group all match the live process and the port lock is free. A pidfile that does not match is removed without a signal.
8. Every dashboard state is visible in its pane: starting, running (owned), running (external), refused (identity), not running, binary missing, binary not admissible, failed to start, exited.
9. With the gazette running, `task schedule` loads `cos.brief`, skips `cos.dash` and names the process holding port 8790.

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

- **Q1 (go/no-go gate, before Build):** the design holds only if checks 1, 2 and 4 pass in the built app, and check 3 is recorded.
    1. The `http://127.0.0.1` iframe loads inside the `tauri://localhost` page without a mixed-content block.
    2. `typeof window.__TAURI__ === "undefined"` inside the frame. The main window sets `withGlobalTauri: true` (`apps/desktop/tauri.conf.json:10`), so the check decides whether the global reaches the frame.
    3. The `Origin` header WebKit sends on a `fetch` POST to `cosreport://localhost/docs/organize` from a page inside a dashboard frame. The in-app docs frame posts nothing while embedded (`src/docs.html:358,365`), so check 3 records the header and does not gate the design.
    4. The `X-Frame-Options`, `frame-ancestors` and `Content-Security-Policy` headers of `http://127.0.0.1:8790/`, read with `curl -sI`. A `DENY`, or a `frame-ancestors` that excludes `tauri://localhost`, fails the check. The control room's CSP has no `frame-ancestors` (`control-room/src/server.rs:247`).

    Fallbacks: if check 1 fails, dashboards open in the browser (Alternative F). If check 2 fails, dashboards are embedded in a child webview (Alternative E). If check 4 fails, the gazette opens in the browser (Alternative F) and the control room stays embedded. If check 3 shows `null` on a dashboard-frame POST, `origin_allowed` already refuses it (§5), and the spike notes the header.

    The spike also measures one identity probe to the control room while a frame is loaded, and records whether it answers within the probe timeout.

## Critical User Journeys (CUJs)

### CUJ 1: See which business needs attention

*   **Given**: six businesses are registered: the four in Goal 1, and `acme` and `default`, which stay registered; `longhand` has a brief written after the one last opened in the app; `founding-bets` has one live agent run.
*   **When**: the owner opens the app.
*   **Then**: the sidebar shows six rows in alphabetical order (`acme`, `default`, `founding-bets`, `kronos`, `longhand`, `realestate`). `longhand` has the unread dot; `founding-bets` shows `1`. No process is spawned to compute the rows.

### CUJ 2: Switch from the sidebar

*   **Given**: the app is on `default`, not pinned by env.
*   **When**: the owner clicks `founding-bets`.
*   **Then**: the app switches exactly as ⌘K does: the header reads `business founding-bets`, the Today report is Founding Bets', and `cos business list` shows the registry's active business unchanged.

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
*   **Then**: the pane says the port is held by a listener that failed the identity probe, names the port and the next step (run `lsof -nP -iTCP:4777 -sTCP:LISTEN` and stop that process), and the app signals nothing.

### CUJ 6: The gazette, owned by launchd

*   **Given**: the gazette's launchd job is running.
*   **When**: the owner opens Longhand's "Gazette", then quits the app.
*   **Then**: the pane shows the gazette after its identity probe passes. The gazette's pid in `launchctl list com.longhand.chief-of-staff-dashboard` is the same before the app started and after it quit.

### CUJ 7: Recover after a crash

*   **Given**: the app started the control room, then was killed with `kill -9`.
*   **When**: the app launches again.
*   **Then**: the launch cleanup finds the pidfile. The port lock is free, the pid is alive, and its start time, executable and process group match the record, so the cleanup stops that process group and removes the pidfile. The next open of a control-room tab starts a fresh server. If the pidfile names a dead pid, or a live process that does not match, the cleanup removes the pidfile and signals nothing.

### CUJ 8: A supervisor dies without a file change

*   **Given**: `founding-bets` has one live run, and its supervisor process is running.
*   **When**: the owner sends SIGKILL to the supervisor process (`kill -9`), and no file in the run directory changes. SIGTERM is not used: the supervisor traps it and writes a terminal outcome (`src/agents/supervise.rs:30-40`).
*   **Then**: within one tick (700 ms) the row's live count drops and the lost warning appears.

## Technical Plan (Architecture)

### 1. Terms

- **Business:** a registry entry, as in the base design. A `Business` has three directories, `home`, `workspace` and `projects` (`src/business.rs:20-24`).
- **Registry:** `businesses.toml`, at `COS_BUSINESSES`, else `$XDG_CONFIG_HOME/cos/businesses.toml`, else `~/.config/cos/businesses.toml` (`src/business.rs:40-66`). It holds the active business and each business's directories (`src/business.rs:27-29`). See the [base design](2026-11-12-multi-business-design.md).
- **cos:** the command-line tool and library in this repository, described in the [base design](2026-11-12-multi-business-design.md).
- **App business:** the business one app window shows. ⌘K or the sidebar sets it per window. Its variables are pinned at start by env (`COS_HOME` and the others listed at `apps/desktop/src/lib.rs:110-120`); a pinned window cannot switch (`apps/desktop/src/lib.rs:329-331`). A pinned window whose business has no registry entry (the label `explicit`) has no dashboards, and `dashboards` returns an empty list.
- **Active business:** the business the CLI uses, stored in the registry as `active`. `cos business list` prints it as `current:`.
- **Brief:** the report written by the scheduled `cos.brief` job, which runs on weekdays at 08:30 (`launchd/cos.brief.plist:20-24`). Briefs are stored under `out/briefs` (`src/store.rs:22`). See the [base design](2026-11-12-multi-business-design.md).
- **Profile:** the business's own `profile.json`, read by agents and reports (Alternative C). See the [base design](2026-11-12-multi-business-design.md).
- **Unsaved-Hire guard:** a business switch is refused while the Hire dialog holds unsaved edits (`apps/desktop/ui/app.js:153-163`). See the [base design](2026-11-12-multi-business-design.md).
- **Live, starting and lost runs:** a run is live when its supervisor pid is a `cos` process; starting when it has no supervisor pid within `START_GRACE` of creation (`src/agents/mod.rs:44`); lost otherwise (`src/agents/mod.rs:543-548`). See the [base design](2026-11-12-multi-business-design.md).
- **Run supervisor:** the `cos agents supervise` process of one agent run.
- **Server supervisor:** `servers::Supervisor`, the desktop component that starts and stops dashboard servers.
- **Managed server:** a `[server.<name>]` entry with a `binary`. **Unmanaged dashboard:** a dashboard with no `server` key; the app never starts it.
- **Owned:** a runtime slot only. This app instance spawned the process and holds its port lock. Only owned slots are signalled on exit.
- **External:** a runtime state. The port answered, and the identity probe passed. The app embeds the listener and never signals it.
- **Leftover:** a pidfile from an earlier app instance whose process still matches the record.
- **Refusal:** the reason the dashboard pane shows instead of a page.
- **Dashboard:** a named page shown for one business: a loopback URL plus an identity probe.
- **Identity probe:** an HTTP GET of a path. It passes when the body contains the dashboard's page marker.
- **Today, Agents, Flows and Docs views:** the app's tabs (`apps/desktop/ui/app.js:118`; see the [base design](2026-11-12-multi-business-design.md)). Today shows the brief report served from `cosreport://localhost/` (`apps/desktop/src/lib.rs:442`). Agents shows the run list (`apps/desktop/ui/app.js:260`). Flows shows the flow view (`apps/desktop/ui/index.html:20`). Docs shows the documentation room (`apps/desktop/src/lib.rs:472`).
- **cos dash:** the `cos dash` command, which serves the live report pages on localhost (`src/main.rs:58-61`). launchd runs it as `cos.dash` (`launchd/cos.dash.plist`).
- **Runbook:** the section titled Setup Runbook (not code).
- **pr-proof:** the skill that records proof of a change for a pull request.
- **Legacy plist:** a `cos.brief` plist with no `--business` argument. It marks no business.

### 2. Components

```text
~/.config/cos/dashboards.toml ──read──► cos::dashboards (parse, validate) ◄── cos business dashboards
                                              │
                                              ▼
apps/desktop  ──────────────────────────────────────────────────────────────────────────
  sidebar (ui/sidebar.js) ◄─ businesses-changed ─ watcher thread (lib.rs watch)
        │ click                                   │ stamps: agents roots, out/briefs,
        ▼                                         │ cos.brief plist, registry, dashboards.toml
  switch_business (unchanged)                     │ kill(0) per live run supervisor
                                                  ▼
                                    businesses() row builder (cos::signals, no process spawn)
  dashboard pane (ui/dash.js)
        │ open(name)
        ▼
  dashboard_open ─► servers::Supervisor ─► ensure ─► spawn ─► pidfile + port lock (app data dir)
                          ▲
  launch cleanup (setup) ─┘   RunEvent::ExitRequested / Exit ─► stop_owned
```

### 3. `dashboards.toml` (cos crate, new `src/dashboards.rs`)

The file lives next to the registry: `COS_DASHBOARDS`, else the directory of the registry file, joined with `dashboards.toml`. The CLI takes the registry from `business::location()` (`src/business.rs:41`); the desktop takes it from the registry the app was started with (`apps/desktop/src/lib.rs:32,53`). Without `COS_DASHBOARDS`, the app reads the dashboards file beside the registry it started with; `COS_DASHBOARDS` overrides the path in both the CLI and the app. No `cos` command writes it; it is hand-edited. A missing file means no dashboards.

```toml
[server.control-room]
binary = "/Users/decebaldobrica/Projects/founder-mode/portfolio-control-room/target/release/portfolio-control-room"
addr = "127.0.0.1:4777"
ready = { path = "/health", contains = "\"status\":\"ok\"" }

[[business.founding-bets]]
name = "Control room"
url = "http://127.0.0.1:4777/"
server = "control-room"
probe = { path = "/", contains = "<title>Wolven portfolio control room" }

[[business.founding-bets]]
name = "Salesroom"
url = "http://127.0.0.1:4777/sales"
server = "control-room"
probe = { path = "/sales", contains = "<script src=\"/sales.js\"" }

[[business.longhand]]
name = "Gazette"
url = "http://127.0.0.1:8790/#staff"
probe = { path = "/", contains = "<measured in Build>" }
```

The control-room markers come from the repository: the title at `control-room/static/index.html:8`, the script tag at `control-room/static/sales.html:9`, and the `/health` body at `control-room/src/server.rs:185-189`. The gazette marker is not measured yet. It is read from the live page during Build and recorded in the runbook. The bare `<title>` marker that an earlier draft used is refused by the validation rule below.

Validation, all at parse time (`dashboards::load` returns the first error with the key path):

- every URL is `http://127.0.0.1:<port>/...`; any other scheme or host is refused;
- a dashboard with `server` names a declared server, and its URL's port equals that server's `addr` port;
- a server's `binary` is absolute; `addr` is `127.0.0.1:<port>`; the argv is fixed as `[binary, "serve", addr]` and is not configurable;
- a probe marker and a `ready` marker have at least 12 characters and are not a bare tag such as `<title>`;
- names are unique per business; business keys follow the registry's name rule;
- a business key not in the registry is reported as orphaned by `cos business dashboards`, not an error, so renaming a business never breaks the file.

The binary's existence and file type are checked when it is started, not at parse time, so `cos business list` works on a machine without the binary.

`cos business dashboards [<name>]` prints each dashboard, its server, and whether its URL answers its probe now (one HTTP GET, 3 s timeout). It never starts anything.

### 4. Server supervisor (desktop, new `apps/desktop/src/servers.rs`)

`servers::Supervisor` is managed Tauri state. It holds `Mutex<HashMap<SocketAddrV4, Slot>>`, keyed by the server's `addr`, so two names for one port cannot start two processes. The mutex guards lookup and insert only. Probe, spawn and readiness run outside it: a `Reserved` or `Starting` slot carries a per-key `Condvar`, and concurrent opens for the same key wait on it.

**Slots.** `Reserved`, `Starting`, `Owned`, `External`, `Exited { status, lock }` and `Refused(refusal)`. `Reserved` is inserted under `inner` at lookup, before any probe, lock or spawn, and carries a per-key `Condvar`. `Starting` replaces `Reserved` when `ensure: start` takes the port lock, and lasts until the slot becomes `Owned`, or until the failure epilogue replaces it with `Refused`. `Starting` and `Owned` keep the pid, the process group id, the port lock and the reaper's exit channel. A child that has exited is `Exited`, which keeps the port lock, and is replaced on the next open. The pane shows `exited` with the payload `{addr, status, log_tail}` until the next open, where `log_tail` is the last 20 lines of `<app log dir>/servers/<port>.log`.

**Reaper.** After spawn, one reaper thread owns the child and is its only waiter. It calls `waitid(P_PID, pid, WEXITED | WNOWAIT)`, which returns while the leader is an unreaped zombie. An unreaped zombie leader keeps its process group id in use. The reaper then, under the `inner` mutex, replaces an `Owned` or `Starting` slot with `Exited { status, lock }`, moving the slot's port lock into it, and removes the pidfile. It releases the mutex, emits `dashboard-exited` with the payload above, and calls `waitpid` to reap. The port lock stays with the `Exited` slot until `ensure: owned` or the stop path drops that slot. Every `signal_group` call on a slot runs under the `inner` mutex and is skipped once the slot is `Exited`, so no signal reaches a group whose leader has been reaped.

**Port lock.** Each app instance takes `flock(LOCK_EX | LOCK_NB)` on `<app data dir>/servers/<port>.lock` before it spawns, and keeps the lock for the life of its `Starting`, `Owned` and `Exited` slot. The kernel drops the lock when the process dies. A second instance that cannot take the lock does not signal the server it guards. Lock files are opened, locked and unlocked, never unlinked: unlinking a locked path lets a second instance lock a new inode at the same path.

**`ensure(server, dashboard) -> Result<Running, Refusal>`** runs these named steps, in this order:

- **ensure: owned.** Under `inner`, a slot in `Owned` whose child has not exited is returned as running (owned). An `Exited` slot is dropped here, and its port lock is released with it. An `External` slot is dropped here too, so every open re-probes. A `Reserved` or `Starting` slot is waited on through its Condvar. The waiter returns `Running` built from its own dashboard's `url` when the slot ends `Owned`, the refusal when it ends `Refused`, and runs this `ensure` again from this step when it ends `External` or is removed. A waiter spawns nothing.
- **ensure: reserve.** Still under `inner`, with no slot for the address: if `shutting_down` is set, refuse with `not running`; otherwise insert `Reserved`. The probe, the lock and the spawn run after this step, outside `inner`.
- **ensure: probe.** Otherwise, a TCP connect to `addr`. If it answers, run the identity probe (`VERIFY(identity)`): an HTTP GET of `probe.path` with a 3 s timeout, retried once. The control room accepts one connection at a time (`control-room/src/server.rs:14-22`) and sets 2 s socket timeouts (`control-room/src/server.rs:39-40`), so a single probe can wait behind another connection. A body that contains the marker gives External: embedded, never signalled, and the `Reserved` slot is removed so that waiters run `ensure` again. Any other answer refuses with `identity`, naming the status line and the first 200 bytes, and replaces the `Reserved` slot with `Refused`.
- **ensure: start.** Reached only when the port does not answer. Every refusal in this step replaces the `Reserved` slot with `Refused` and calls `notify_all`:
    - no `server` declared (unmanaged dashboard): refuse with `not running`, hint "its owner (for example launchd) is not running it". The gazette takes this path.
    - server declared: `VERIFY(binary_admissible)` checks that the path is absolute, a regular file, and executable. A missing file refuses with `binary missing` and the build command. Any other failure refuses with the rule name.
    - under `inner`, if `shutting_down` is set, refuse with `not running`. Otherwise take the port lock without blocking in the same critical section, and replace `Reserved` with `Starting`. If another instance holds the lock, refuse with `not running`, hint "another app instance is starting it; open the dashboard again".
    - spawn with the `SpawnFn` (Detailed Implementation Plan, item 8). Its app implementation, `servers::real_spawn`, calls `proc::spawn_grouped` (`src/proc.rs:165`, which sets its own process group). The working directory is the binary's parent directory. The control room bakes its root in at compile time (`control-room/src/main.rs:31`), so the working directory does not change what it serves. The environment is inherited from the app. Stdin is null. Stdout and stderr append to `<app log dir>/servers/<port>.log`, which is truncated to its last 1 MiB at each spawn. A spawn error runs the failure epilogue with `failed to start`. Otherwise `ensure` records `Spawned{pid, pgid, exit}` in the slot under the `inner` mutex and wakes the Condvar. It then reads `start_time` and `exe` (`proc::exe(pid)`, so launch cleanup compares the same resolved path), runs `VERIFY(spawn_result)`, and writes the pidfile atomically (`src/state.rs:555`) with `{pid, pgid, start_time, exe, addr}`. A refusal from `VERIFY(spawn_result)` runs the failure epilogue; no pidfile exists yet.
    - after the pidfile is written, `ensure` checks `shutting_down` under the `inner` mutex. If it is set, the failure epilogue runs with `not running`.
    - readiness: poll `ready` every 250 ms for at most 10 s. A pass needs two consecutive passing probes, with the reaper reporting no exit before the first probe and after the second. If the child exits during readiness, the failure epilogue runs with `failed to start` and the log's last 20 lines. A listener that answered during readiness belongs to another process: it is not embedded and not signalled. On timeout, the failure epilogue runs with `failed to start`.
    - success: the slot becomes `Owned`.
    - **Failure epilogue.** Every refusal after the port lock is taken runs these steps in order: (1) if a `Spawned` record exists and the reaper has not marked the slot `Exited`, `signal_group` TERM, then KILL, then wait on the reaper within the 5 s deadline; (2) remove the pidfile if it still exists. `VERIFY(ownership)` admits this only once the reaper has marked the slot `Exited`, so an unreaped group keeps its pidfile; (3) the port lock is released by step (4), which drops the slot that holds it; (4) under the `inner` mutex, replace the slot with `Refused(refusal)` and call `notify_all` on its Condvar. A waiter that wakes on `Refused` returns that refusal and spawns nothing. The next `ensure` on the same address removes a `Refused` slot first.

**Launch cleanup.** Runs once in `setup`, before any `ensure`. For each `<app data dir>/servers/<port>.json` whose `<port>` is digits only:

1. Take `<port>.lock` without blocking. If it is held, another instance owns the server: leave the pidfile and the process alone.
2. The lock is free. Compare the record with the kernel: the pid is alive, its start time equals `start_time`, its executable equals `exe`, and `pgid == pid`. When all four match, send SIGTERM to the process group, wait up to a 5 s deadline per leftover, send SIGKILL to survivors, and remove the pidfile. Otherwise remove the pidfile and signal nothing.

The four checks run again immediately before the SIGTERM and again immediately before the SIGKILL. Cleanup releases each `<port>.lock` after that port's pidfile is handled, so a later `ensure` on the port can take it. It runs synchronously in `setup`, so `ensure` does not run before it returns, and each leftover adds at most the 5 s deadline to setup.

**Stop path.** `stop_owned()` runs from `RunEvent::ExitRequested` and from `RunEvent::Exit`, which requires `apps/desktop/src/main.rs` to switch to `.build(...).expect(...).run(...)` (Detailed Implementation Plan, item 10). It is idempotent: a `shutting_down` flag makes a second call a no-op, and slots are drained once.

- First, set `shutting_down` under the `inner` mutex, then wait on each `Reserved` or `Starting` slot's Condvar until the slot has its `Spawned` record, is `Refused`, is `External`, or is gone. The wait shares the 5 s deadline below.
- For each `Owned` or `Starting` slot with a `Spawned` record that its reaper has not marked `Exited`: send SIGTERM to the process group, under the reaper protocol in Slots.
- Wait on each reaper channel within one shared 5 s deadline, then send SIGKILL to the survivors and wait again.
- Remove any pidfile the reaper left, once its slot is `Exited`, and drop each `Exited` slot, which releases its port lock.
- An `ensure` that reserved its key before `shutting_down` was set is covered by the wait above, from reservation to `Spawned`, `Refused`, `External` or removal. A `Reserved` slot still probing at the deadline has no child, so nothing is signalled, and its `ensure` refuses under `inner` at lock-take and spawns nothing. An `ensure` that reserves after the flag is set refuses at reserve. Once `Spawned` is recorded, the slot is signalled by the bullet above.
- An `External` slot is never signalled.

### 5. Dashboard pane (desktop UI)

- The header gets a tab group after Docs, with one tab per dashboard of the app's business. The data comes from the `dashboards` IPC command, which returns the app business's entries. Each tab is an `.app-tab` with `data-view="dash"` and `data-dash="<name>"`. `openTab` sets `aria-current` on the clicked tab only, matching `data-dash` as well as `data-view`; the loop at `apps/desktop/ui/app.js:125-128` otherwise marks every dashboard tab. dash.js binds click handlers on the tabs it creates, because the load-time binding at `apps/desktop/ui/app.js:144-146` does not reach them. `switchTo` (`apps/desktop/ui/app.js:166-168`) writes the hash as `tab=dash&dash=<name>` for the dashboard view; today it writes `tab` and `pane` only.
- `dashboards` returns `Result<Vec<DashboardRow>, String>`. On error, dash.js shows the message (the key path and rule) as a notice in `#dash-notice`, above the dashboard tabs. The sidebar still works.
- Opening a tab calls `dashboard_open(business, name)`, an `async` command that runs `ensure` inside `blocking`, as `businesses` does (`apps/desktop/src/lib.rs:424`), because `ensure` can wait for two 3 s probes and a 10 s readiness poll. It is guarded by `named()` (`apps/desktop/src/lib.rs:264`), so a stale business is refused. The pane shows the state while `ensure` runs, and sets the iframe `src` to the `url` in `Running`, the URL `ensure` verified. dash.js uses no other URL. On `dashboard-exited`, dash.js compares the payload's `addr` with the host and port of the open dashboard's `url`, and ignores an event for any other addr.
- The iframe is `sandbox="allow-scripts allow-same-origin allow-forms"` with no `allow-top-navigation`. `allow-forms` stays only if the live gazette page submits a form; the Build step checks the page and drops the flag otherwise. The control room answers 405 to every method except GET and HEAD (`control-room/src/server.rs:56-60`). `allow-same-origin` is set because the control room's CSP sends `connect-src 'self'` (`control-room/src/server.rs:247`), which needs a real origin. The gazette's own CSP is not read yet; Q1 check 4 reads its frame headers, and Build records its CSP.
- The frame is destroyed on business switch, as the page reloads on switch today. The server keeps running until quit. After a switch the pane is idle: the `dash` name in the hash selects a tab only if the new business has that dashboard, and nothing opens until the owner clicks.
- CSP in `apps/desktop/tauri.conf.json:22`: `frame-src` becomes `cosreport: http://cosreport.localhost http://127.0.0.1:*`. Nothing else changes, and `connect-src` stays `ipc: http://ipc.localhost`: the app's own page never fetches the dashboards; only the frames do.
- The `cosreport` handler (`apps/desktop/src/lib.rs:963-974`) calls `origin_allowed(&req)` first, and answers 403 when it returns false:
    - GET is allowed with no Origin, with `Origin: null` (the opaque report frames), or with an app origin.
    - POST requires an Origin that is present and is one of `tauri://localhost`, `http://tauri.localhost`, `cosreport://localhost` or `http://cosreport.localhost`. `null` and an absent Origin are refused. No in-app frame POSTs today: the docs frame is embedded and saves through the app (`src/docs.html:358,365`).
- `cosreport` serves the reports and accepts writes such as `/docs/organize`. Those writes are scoped by the `x-cos-business` header (`apps/desktop/src/lib.rs:479-484`), which any page that knows the business name can set (`src/docs.html:366` sets it from the query string). The Origin rule keeps a dashboard frame from setting it.
- The capability file `apps/desktop/capabilities/default.json` stays `windows: ["main"]` with no remote URLs, so IPC from `http://127.0.0.1` frames is denied. The design holds only if Q1 check 2 passes: `window.__TAURI__` is undefined inside such a frame.

### 6. Sidebar and signals

**Rendering.** `ui/index.html` wraps the header and `<main>` in a column beside a new `<aside id="sidebar">`. The `--sidebar*` tokens (`apps/desktop/ui/tokens.css:90-97`) style it. Rows come from the `businesses` IPC command, in alphabetical order: the registry is a `BTreeMap` (`src/business.rs:29`), and `write` re-serialises it sorted (`src/business.rs:116-123`). The app's business is highlighted. A click calls `switchTo(name)` (`apps/desktop/ui/app.js:159`). A refusal from `switchTo` (`apps/desktop/ui/app.js:163`) or from `switch_business` (`apps/desktop/src/lib.rs:329-351`) is caught, and its message is shown in a status line below the rows until the next click. A pinned app disables the rows and shows the same "pinned by …" title as ⌘K (`apps/desktop/ui/app.js:99-100`).

**One source.** The sidebar and ⌘K read the same `businesses` IPC command. `BusinessRow` (`apps/desktop/src/lib.rs:379-388`) gains `unread: bool` and `scheduled: bool`. Its existing `counts` field supplies `live` and `lost`. The Agents-bar button `#other-live` (`apps/desktop/ui/index.html:180`), its update code (`apps/desktop/ui/app.js:1121-1135`), its call at `app.js:141`, its click listener (`app.js:1151`), the `businesses-loaded` listener (`app.js:1152`), the dispatch at `apps/desktop/ui/command.js:103` and its CSS (`apps/desktop/ui/style.css:51`) are removed, because each row shows the same counts. The header count `#agent-count` stays.

**Signals.** Computed by a new `cos::signals` module, from file metadata and kernel checks, with no process spawn:

- **live, lost, starting:** `agents::count(home)` (`src/agents/mod.rs:561-592`) returns `starting` as its own count; `live` still includes Starting runs (`src/agents/mod.rs:586`). Its `classify` calls `is_cos` (`src/agents/mod.rs:545`), which after this design reads the executable with `proc::exe(pid)` instead of spawning `ps` (`src/agents/mod.rs:794-807` today). A recycled pid that now runs another `cos` process (for example `cos dash`) still counts as live, because the check compares the file name only. The same check gates the SIGKILL at `src/agents/mod.rs:675`; binding the supervisor's start time is not part of this design.
- **supervisors:** the supervisor pids of the non-terminal runs, collected for the tick.
- **unread:** the newest `out/briefs/YYYY-MM-DD.json` by filename (names that do not match `^\d{4}-\d{2}-\d{2}\.json$` are ignored), with its mtime in whole milliseconds since the Unix epoch (`cos::signals::mtime_ms`), compared with `lastSeenBrief[business]` in `<app config dir>/ui-state.json`. It is unread when its filename is greater than the stored filename, or equal to it with a greater mtime.
    - A business with no stored pair, and any brief, is unread.
    - **Opened** means the Today view is visible and the report has rendered the brief. The iframe reloads on every `report-changed` whatever view is active (`apps/desktop/ui/app.js:1140-1141`), so `cos-report-ready` can arrive while another view is shown. The host calls `brief_seen` only when Today is visible at that moment. It keeps the last ready payload in memory, and the switch to Today calls `brief_seen` with it, so a brief rendered during another view is marked seen on that switch.
    - `brief_seen(business, file, mtime)` writes the pair for the brief that the report rendered. The report frame sends `brief: "<file>"` and `mtime` in its `cos-report-ready` message (`src/report.html:455` sends only `type` today, so this is a change to the report frame). `store::latest` opens the brief once and takes its mtime from that handle, so the mtime describes the content the report rendered (item 17a). The host handler at `apps/desktop/ui/app.js:193-195` passes both on. The host never stats the brief to obtain a value: the stored mtime is the one the report sent, so a brief rewritten after render keeps the mtime of the content the owner saw. The filename is validated as in §Semantic ISA.
    - The report data carries `briefRef{file, mtime}` for the brief it rendered. The template posts `brief` and `mtime` only when the data also has `hostBrief: true`. `report_response` sets it for `/` (`apps/desktop/src/lib.rs:451`); `report::build` (`src/report.rs:24-31`) and the browser `cos dash` leave it unset. Archived reports (`/reports/<day>.html`, `src/dash.rs:156-166`) therefore send no `brief` field, and the host ignores a message without one.
    - `brief_seen` never lowers the stored pair: it keeps the greater of the stored and the sent `(filename, mtime)`.
    - `brief_seen` recomputes the business row and emits `businesses-changed`, so the dot clears at once. `ui-state.json` is not a tick stamp.
    - The read-modify-write runs under an exclusive lock on `<ui_state_file>.lock`, taken the way `business::update` takes its lock (`src/business.rs:125-130`). The file is written with `atomic_write_bytes` (`src/state.rs:555`), under `named()`.
    - An unparseable `ui-state.json` reads as empty. Entries for unregistered names are dropped on write.
- **scheduled:** the `cos.brief` plist at `~/Library/LaunchAgents/cos.brief.plist`, read with the `plist` crate. The crate is a direct dependency, already in the graph through tauri (`Cargo.lock:2437-2438`), and it reads binary plists. The business is the string after `--business` in `ProgramArguments`. It counts only when `EnvironmentVariables.COS_BUSINESSES` resolves to the same registry as the app's. The app side resolves its canonicalised `st.registry`. The plist side resolves through `business::location_from` (`src/business.rs:49`) with `COS_BUSINESSES`, `XDG_CONFIG_HOME` and `HOME` read from the plist's `EnvironmentVariables`; a key the plist lacks is read from the app's environment. `location_from` fails with `HOME is unset` (`src/business.rs:61`) when none of the three is set. Both paths are canonicalised before comparison.
    - A plist with no `--business` (a legacy plist) marks no business.
    - A parse error marks none and logs once.
    - The plist being on disk does not prove the job is loaded, so the tooltip says "installed for <name>".

**Refresh.** The watcher thread (`apps/desktop/src/lib.rs:891-909`) keeps its 700 ms tick.

- Each tick takes one stamp per business: `cos::dash::stamp([agents::root(home), home/out/briefs])` (`src/dash.rs:181-196`, metadata only), plus the plist's mtime. `dash::stamp` reads directories only, so `businesses.toml` and `dashboards.toml` each take their own mtime from one metadata call.
- Each tick calls `proc::alive(pid)` (one `kill(0)`) for every supervisor pid recorded for a non-terminal run.
- A business is recomputed when its stamp changed, one of its supervisors died, or it has `starting > 0`: a run turns `Lost` when `START_GRACE` passes, with no file change or pid to watch. A changed `businesses.toml` recomputes every row. A changed `dashboards.toml` emits `dashboards-changed`, which `dash.js` handles by calling `loadDashTabs()`.
- `businesses-changed` is emitted with all rows only when the computed rows differ from the rows last emitted. The existing `report-changed` and `agents-changed` emits stay (`apps/desktop/src/lib.rs:891-905`); the business stamps are added beside them.
- Cost per tick is one `read_dir` per directory, one metadata call per run entry (`src/dash.rs:184-186`), and one `kill(0)` per live run. Full re-reads happen only on change.
- The initial rows come from the `businesses` IPC command, the same row builder the tick uses.

### 7. `task schedule` and port 8790

`cos.dash` defaults to `127.0.0.1:8790` (`src/main.rs:60`), the port the Longhand gazette holds. `schedule` (`Taskfile.yml:211-222`) runs the `cos.brief` steps for every item first, then handles `cos.dash` in this order:

1. Render `cos.dash` to `cos.dash.plist.new` without moving it (S9d).
2. Read the old pid from the `pid = N` line of `launchctl print gui/$(id -u)/cos.dash`. It is empty when the job is not loaded, and step 4 is then skipped.
3. `TOOL_CALL(bootout_dash)`, admitted by `VERIFY(dash_target)` (S9a).
4. Poll every 250 ms for at most 10 s until the old pid has exited. If it is still alive, `VERIFY(dash_port_held)` returns `Timeout`: the task fails with a message, `TOOL_CALL(remove_dash_plist)` removes `.new` only, and the installed plist is kept. `cos.dash` stays unloaded until the next login or the next `task schedule` run.
5. Run `lsof -nP -iTCP:$DASH_PORT -sTCP:LISTEN`. `DASH_PORT` is a test-only seam. It changes only this check; the rendered plist runs `cos dash` on the default 8790 (`launchd/cos.dash.plist:6-13`), so tests set it to a free ephemeral port. `VERIFY(dash_port_held)` returns `Free` when there is no listener, and `Held{pid, command}` when the listener's pid is not the old pid.
6. `Free`: move `.new` over the installed plist and `TOOL_CALL(bootstrap_dash)`, as today (S9c).
7. `Held`: print the listener's command and pid, state that `cos.dash` is skipped, and `TOOL_CALL(remove_dash_plist)` on `.new` and `~/Library/LaunchAgents/cos.dash.plist` (S9b). The plist has `RunAtLoad` and `KeepAlive` (`launchd/cos.dash.plist:19-20`), so launchd would load it at the next login. Running `task schedule` again after the holder stops restores the plist.

`cos.brief` loads in either case. While the gazette holds the port, the `Held` case is the steady state (Goal 9).

### 8. Threat model

- Whoever can write the declared binary, or `dashboards.toml`, can run code as the owner the next time a dashboard opens. The design accepts this for a binary the owner builds in their own repo. The declared binary sits inside the `founding-bets` home, workspace and projects (`~/Projects/founder-mode`), so an agent with write access to that workspace can replace it. `dashboards.toml` is in the owner's config directory, and agents with file access could write it.
- The binary check is: absolute path, regular file, executable bit set. Those checks do not stop an agent that can write the binary.
- Alternative C's rejection is worded for this difference: `profile.json` is the business's own record, read by agents and reports, and mixing launcher configuration into it couples two owners.
- Loopback is not a trust boundary, because any local process can answer on a port. A page marker therefore proves what the page says, not who serves it. Ownership is proven by the kernel's view of the pid: start time, executable and process group.
- The `cosreport` Origin rule is in §5.

## Alternatives Considered

### Alternative A: launchd owns the control room

*   **Pros**: no supervisor, exit hook, pidfile or cleanup code in the app; survives an app crash by design; matches how the Longhand gazette already runs.
*   **Cons**: a plist and an install step in the founder-mode repo; the server runs whether or not the app is open.
*   **Reason for Rejection**: Rejected (owner decision U4, [brief §Decisions](2026-11-13-business-dashboards-brief.md#decisions-made-with-the-owner)): the owner wants the app to start the control room on demand and stop it on quit. Trade-off accepted: this design keeps the supervisor code, the pidfile and the crash gap in [Risks & Mitigations](#risks--mitigations).

### Alternative B: dashboards as fields in `businesses.toml`

*   **Pros**: one file per business; `cos business add` could set them.
*   **Cons**: `business::write` serialises only the known fields (`src/business.rs:116-123`), and `canonical()` rebuilds entries from three directories (`src/business.rs:162-168`). An older `cos` binary, including the one launchd runs from `~/.cargo/bin`, would erase the fields on the next `add`, `use` or `remove`.
*   **Reason for Rejection**: Rejected (owner decision U6, [brief §Decisions](2026-11-13-business-dashboards-brief.md#decisions-made-with-the-owner)): a separate file outside every workspace is not rewritten by any `cos` version.

### Alternative C: dashboards in each workspace's `profile.json`

*   **Pros**: travels with the workspace; `cos` never writes the profile.
*   **Cons**: the profile is the business's own preference record, shared with agents, and mixing launcher configuration into it couples two owners. Founding Bets' profile already carries an unrelated `dashboardUrl` (`http://127.0.0.1:4788`, `~/Projects/founder-mode/.chief-of-staff/profile.json:435`) that would conflict.
*   **Reason for Rejection**: Rejected (owner decision U6, [brief §Decisions](2026-11-13-business-dashboards-brief.md#decisions-made-with-the-owner)): the profile's content is read by agents and reports, so launcher configuration would couple two owners.

### Alternative D: exact frame origins in the CSP

*   **Pros**: the build allows only the two known ports.
*   **Cons**: each new dashboard port needs a rebuild of the app.
*   **Reason for Rejection**: Rejected (superseded owner decision D6, [brief §Decisions](2026-11-13-business-dashboards-brief.md#proposed-answers-to-the-remaining-decisions-for-the-design-to-confirm)): loopback on any port, with URLs validated to `127.0.0.1` and loaded only after the identity probe, keeps dashboards as data. A new port needs only a `dashboards.toml` edit.

### Alternative E: a child webview without capabilities

*   **Pros**: IPC isolation by construction, independent of the frame's CSP.
*   **Cons**: Tauri's multi-webview API is behind the `unstable` feature; layout and resize are kept in sync by hand.
*   **Reason for Rejection**: Not chosen (owner decision D6, [brief §Decisions](2026-11-13-business-dashboards-brief.md#proposed-answers-to-the-remaining-decisions-for-the-design-to-confirm)): the iframe with a capability file that has no remote URLs gives the same isolation on the stable API, if Q1 check 2 passes. If it fails, this alternative becomes the fallback.

### Alternative F: "open in browser" instead of embedding

*   **Pros**: no CSP change, no embed risk; `open` is already used (`src/report.rs:240-241`).
*   **Cons**: the dashboards stay outside the app, and nothing ties a tab to the business it belongs to; in-app switching between businesses and dashboards is lost.
*   **Reason for Rejection**: Rejected (owner decision U5, [brief §Decisions](2026-11-13-business-dashboards-brief.md#decisions-made-with-the-owner)): the gazette is embedded, so the owner wants the dashboards inside the app. If Q1 check 1 fails, this alternative becomes the fallback.

### Alternative G: identity by `/health` fields in the control room

*   **Pros**: a server-declared identity (service name and root) instead of a page marker.
*   **Cons**: a change in a second repository; does not help the gazette.
*   **Reason for Rejection**: Rejected (owner decision D2, [brief §Decisions](2026-11-13-business-dashboards-brief.md#proposed-answers-to-the-remaining-decisions-for-the-design-to-confirm)): a per-dashboard page marker covers both servers with no change outside this repo. Ownership is proven by the kernel's view of the pid, not by the page. The `/health` body is used only as the readiness check (`control-room/src/server.rs:185-189`).

### Alternative H: one guard process per server (shim)

*   **Pros**: the server dies with the app even after `kill -9`, through a pipe the shim watches.
*   **Cons**: a second long-lived process per server, and a new `cos` subcommand to supervise it.
*   **Reason for Rejection**: Rejected (owner decision D4, [brief §Decisions](2026-11-13-business-dashboards-brief.md#proposed-answers-to-the-remaining-decisions-for-the-design-to-confirm)): a pidfile with launch cleanup stops a leftover server with less machinery. The server keeps running between a crash and the next launch; this gap stays open (see NFR Security).

## Risks & Mitigations

| Risk | Impact | Mitigation |
| --- | --- | --- |
| The server is unsupervised between a crash and the next launch. | The port stays held and the process keeps serving loopback until the next launch or a reboot. | Launch cleanup stops it (CUJ 7). The gap is stated here and is not closed (Alternative H). |
| The app crashes between the spawn and the atomic pidfile write. | The server runs with no pidfile, so launch cleanup cannot find it. | None. The gap is stated here; the port stays held until the process exits or a reboot. |
| SIGTERM to the app, or a logout path that sends it, skips `RunEvent::Exit`. | Owned servers survive the app. | The next launch's cleanup stops them. The exit-path row in [Confidence](#confidence) stays sand until a test proves the ⌘Q path. |
| PID or process group reuse between the check and the signal. | An unrelated process group receives SIGTERM or SIGKILL. | The four checks run again immediately before each signal. Signals go only to a leader that is not reaped. A residual window between the last check and the signal remains. |
| Q1 fails. | Embedding does not hold. | Check 1 fails: dashboards open in the browser (Alternative F). Check 2 fails: child webview (Alternative E). |
| Q1 check 3 shows `null` on a dashboard-frame POST. | The POST is refused. | `origin_allowed` refuses `null` (§5). The spike records the header; the rule does not change. |
| Probe markers drift in the real pages. | The identity probe refuses a healthy dashboard. | Markers are read from the live pages in Build. A refusal shows what answered, so the drift is visible. Markers must be at least 12 characters and not a bare tag. |
| The `http://127.0.0.1:*` frame source. | Any local listener can be framed once its probe passes. | Embedding follows the probe. Frames have no IPC (capability file, §5). The Origin rule covers the report scheme. |
| `kill(0)` reports a zombie as alive (`src/proc.rs:246`). | A supervisor that its parent has not reaped keeps its run live. | CUJ 8's test measures this case during Build. |
| Tick cost grows with the number of runs. | More runs mean more syscalls per tick. | Cost is one metadata call per run entry and one `kill(0)` per live run. Full re-reads happen only on change. |
| `realestate`'s home is nested inside `founding-bets`'s home. | The two businesses share the projects directory `~/Projects/founder-mode`. | `cos business add` prints a warning naming the outer business (§Setup Runbook). Their stamp directories `out/agents` and `out/briefs` are disjoint (`src/agents/mod.rs:260-262`). Projects-directory overlap is checked in Build. |
| Workspace write access means code execution. | An agent that can write the binary or `dashboards.toml` runs code as the owner on the next open. | Accepted in §8, with the binary check kept. |

## Setup Runbook (not code)

- The four businesses are registered with `cos business add`. A registry entry names three directories: `home`, `workspace` and `projects` (`src/business.rs:20-24`). `add` refuses a business whose home or workspace equals another business's (`src/business.rs:277-285`).

| Name | home | workspace | projects |
| --- | --- | --- | --- |
| founding-bets | `~/Projects/founder-mode` | `~/Projects/founder-mode` | `~/Projects/founder-mode` |
| longhand | `~/Projects/fractional-projects/longhand` | same | `~/Projects/fractional-projects` |
| realestate | `~/Projects/founder-mode/real-estate-chief-of-staff` | same | `~/Projects/founder-mode` |
| kronos | `~/Projects/chronos` | same | `~/Projects/chronos` |

- `kronos` first gets `cos --workspace ~/Projects/chronos state init`, so `switch_business` finds a profile. The business is named `kronos` and its directory `chronos`. The spelling mismatch is deliberate (owner decision U3, [brief](2026-11-13-business-dashboards-brief.md#decisions-made-with-the-owner)).
- `realestate`'s home is nested inside `founding-bets`'s home. `cos business add` prints a warning naming the outer business and continues.
- The control-room binary is built once by hand with `cargo build --release` in `~/Projects/founder-mode/portfolio-control-room/control-room`. The workspace root is `portfolio-control-room/Cargo.toml` (`[workspace]` at line 4), so cargo writes the binary to `portfolio-control-room/target/release/portfolio-control-room`.
- `dashboards.toml` is written from the example in §3. The gazette's probe marker is read from the live page.

## Semantic ISA

Each side effect is an instruction in one of five cores: Cognitive, Memory, Execution, Normative and Meta-cognitive (see the `semantic-isa` skill, `SKILL.md`). A sink is an instruction with an effect outside the app. Every sink accepts input only after a named, deterministic VERIFY clears it.

### Side-effect inventory

| # | Side effect | Where | Reversible? | Who is affected if it misfires |
| --- | --- | --- | --- | --- |
| S1 | Spawn a server process from a binary named in `dashboards.toml` | `servers::ensure` | yes: stop the group | the owner: the binary runs as them (§8) |
| S2 | Signal a process group (SIGTERM, SIGKILL) | `servers::stop_owned`, launch cleanup, failed-start cleanup | no | any process whose process group is reused or wrongly adopted |
| S3 | Write and remove a pidfile; create and lock a port lock file, never unlinked | `<app data dir>/servers/<port>.json` and `<port>.lock`, and the `servers/` directory created on first use | yes | the app's own recovery |
| S4 | Append a server log, truncated to 1 MiB at spawn | `<app log dir>/servers/<port>.log` | yes | disk |
| S5 | HTTP GET to a loopback port (probe, ready) | `servers`, `cos::dashboards::probe` (`cos business dashboards`) | yes, read only | the local service probed |
| S6 | Load a loopback URL in an iframe | `ui/dash.js` | yes | the owner, if a foreign page is shown under a dashboard's name |
| S7 | Write `ui-state.json` (`lastSeenBrief`), and create its lock file `<ui_state_file>.lock` | `brief_seen` | yes | the unread dot only |
| S8 | `cos business add` warning for nested homes | `business.rs` | n/a, output only | none |
| S9a | `task schedule` boots out the loaded `cos.dash` job | `Taskfile.yml` (schedule) | partly: `launchctl bootstrap` restores it; after a Timeout run nothing does, so it stays unloaded until login or a rerun | the owner: the report server on 8790 stops |
| S9b | `task schedule` deletes `.new` and the installed `cos.dash.plist` when 8790 is held | `Taskfile.yml` (schedule) | no: only a rerun of `task schedule` restores the plist | the owner: `cos.dash` no longer starts at login |
| S9c | `task schedule` moves `.new` into place and bootstraps `cos.dash` when 8790 is free | `Taskfile.yml` (schedule) | yes: bootout | the owner: the report server starts |
| S9d | `task schedule` renders `cos.dash.plist.new` | `Taskfile.yml` (schedule) | yes: `.new` is removed on every outcome | none |
| S10 | Refuse a POST to `cosreport` without an allowed Origin | `lib.rs` scheme handler | yes | a frame that tried to reach the report scheme |

The app makes no non-loopback request; embedded pages keep their own network behaviour. Nothing new is sent to a model.

### Instructions

| Instruction | Core | Governance property | Sink? | Binding (closed schemas) |
| --- | --- | --- | --- | --- |
| `LOAD(dashboards.toml)` | Memory | deterministic parse + validation | no | in: path; out: `Dashboards {servers: map<name, Server{binary: abs path, addr: 127.0.0.1:port, ready: Probe}>, business: map<name, [Dashboard{name, url: loopback, server?: name, probe: Probe}]>}` |
| `PROBE(addr, probe)` | Execution | read-only HTTP GET, 3 s timeout | no | in: `{addr: 127.0.0.1:port, path: string, contains: string}`; out: `{answered: bool, matched: bool, status: u16?, head: string≤200}` |
| `VERIFY(binary_admissible)` | Normative | deterministic checkpoint | no | in: `Server`; out: admit or refusal `{rule, path}`; refuses with rule `platform` when `proc::start_time` is unavailable |
| `VERIFY(ownership)` | Normative | deterministic checkpoint | no | in: `{slot?, pidfile?, lock}`; out: `Owned{pid, pgid} \| Leftover{pid, pgid} \| Starting{pid, pgid} \| Stale \| Held`; `Starting` requires the slot's `child` set, the reaper not having marked it `Exited`, the lock held by this instance, and pgid equal to pid |
| `VERIFY(identity)` | Normative | deterministic checkpoint over `PROBE` output | no | in: `{probe result, marker}`; out: admit or refusal `{rule: identity, head}` |
| `TOOL_CALL(spawn_server)` | Execution | process spawn | **yes** (S1) | in: `{binary, addr, env}`, env inherited from the app, argv fixed `[binary, serve, addr]`, working directory the binary's parent; out: `{pid, pgid, start_time, exe}` |
| `VERIFY(readiness)` | Normative | deterministic checkpoint over `PROBE` output and the child's exit state | no | in: `{probe_1: PROBE, probe_2: PROBE, reaped_before_1: bool, reaped_after_2: bool}`; out: admit or refusal `{rule: readiness, log_tail}` |
| `VERIFY(log_path)` | Normative | deterministic checkpoint | no | in: `{addr}`; out: admit with `path` = `<app log dir>/servers/<port>.log`, or refusal `{rule: log_path}` |
| `STORE(server_log)` | Memory | truncate to the last 1 MiB, then append | **yes** (S4) | in: `{path}` from `VERIFY(log_path)`; out: unit |
| `VERIFY(brief_seen)` | Normative | deterministic checkpoint | no | in: `{business, file, mtime}`; out: admit or refusal `{rule: brief_seen}`; admit only when the business is the app's, `file` matches `^\d{4}-\d{2}-\d{2}\.json$`, `mtime` is an integer of at least 0 in ms since the Unix epoch, and `file` exists under `out/briefs` (a check only, not a value source) |
| `TOOL_CALL(signal_group)` | Execution | deterministic action | **yes** (S2) | in: `{pgid, signal: TERM \| KILL}`; out: unit |
| `STORE(pidfile)` | Memory | atomic write, or removal | **yes** (S3) | in: `{pid, pgid, start_time, exe, addr}` for a write (admitted by `VERIFY(spawn_result)`), or `{path}` for a removal (admitted by `VERIFY(pidfile_path)`); out: unit |
| `STORE(port_lock)` | Memory | open and flock `<port>.lock`, never unlink | **yes** (S3) | in: `{path}` admitted by `VERIFY(pidfile_path)`; out: unit |
| `RENDER(iframe src)` | Memory | UI action | **yes** (S6) | in: `{url: loopback}`; out: unit |
| `STORE(ui-state)` | Memory | atomic write | **yes** (S7) | in: `{business: registered name, file: YYYY-MM-DD.json, mtime: i64 (ms since the Unix epoch)}`; out: unit |
| `VERIFY(origin_allowed)` | Normative | deterministic checkpoint | no | in: `{method, Origin?}`; out: admit or 403 |
| `VERIFY(spawn_result)` | Normative | deterministic checkpoint | no | in: `{pid, pgid, start_time, exe, addr}` from `TOOL_CALL(spawn_server)`; out: admit or refusal `{rule: spawn_result}`; admit only when pid > 0, pgid == pid, exe non-empty, start_time present, and addr equal to the validated addr |
| `VERIFY(pidfile_path)` | Normative | deterministic checkpoint | no | in: `{addr, file}`; out: admit or refusal `{rule: pidfile_path}`; admit only when `file` is `<port>.json` or `<port>.lock` for the port of a validated addr, or, in launch cleanup, for the digits-only name of a `servers/<port>.json` entry, with no separator or `..`; a `.lock` path is opened and locked, never unlinked |
| `VERIFY(dash_target)` | Normative | deterministic checkpoint | no | in: `{label, plist}`; out: admit or refusal `{rule: dash_target}`; admit only when label is `cos.dash` and plist is `<LaunchAgents>/cos.dash.plist` or `<LaunchAgents>/cos.dash.plist.new`, exactly |
| `VERIFY(dash_port_held)` | Normative | deterministic checkpoint over `lsof` and `launchctl` output | no | in: `{old_pid?: pid, listener?: {pid, command}, old_pid_alive: bool}`, where `old_pid_alive` is read after the 10 s wait; out: `Free \| Held{pid, command} \| Timeout` |
| `TOOL_CALL(bootout_dash)` | Execution | launchctl bootout | **yes** (S9a) | in: `{label, plist}`, admitted by `VERIFY(dash_target)`; out: unit |
| `TOOL_CALL(bootstrap_dash)` | Execution | launchctl bootstrap | **yes** (S9c) | in: `{plist}`, admitted by `VERIFY(dash_port_held)` = Free and `VERIFY(dash_target)`; out: unit |
| `TOOL_CALL(remove_dash_plist)` | Execution | file removal | **yes** (S9b) | in: `{paths}`: `[<.new>, <installed plist>]` when `VERIFY(dash_port_held)` = Held, `[<.new>]` when it is Timeout; admitted also by `VERIFY(dash_target)`; out: unit |
| `STORE(render_dash_plist)` | Memory | write `cos.dash.plist.new` only | **yes** (S9d) | in: `{path}`, admitted by `VERIFY(dash_target)`; out: unit |
| `TOOL_CALL(install_dash_plist)` | Execution | move `.new` over the installed plist | **yes** (S9c) | in: `{from, to}`, admitted by `VERIFY(dash_port_held)` = Free and by `VERIFY(dash_target)` for both paths; out: unit |

`RENDER` sits in the Memory core, as the `semantic-isa` skill's table places it.

### Taint

- **Sources:**
    - `dashboards.toml`, which is hand-edited, and which agents with file access could write;
    - the pidfile, which is on disk and could be stale or forged;
    - every HTTP response from a loopback port, since any local process can answer;
    - the launchd plist;
    - `lsof` and `launchctl` output in `task schedule` (the listener's pid and command, and the old pid);
    - `ui-state.json`;
    - request `Origin` headers;
    - the brief filename and mtime that the report frame sends in `cos-report-ready`.
- **Propagation:** a `PROBE` result is tainted. A pidfile read is tainted until `VERIFY(ownership)` checks it against the kernel's view of the pid: start time, executable and process group.
- **Sinks:** `spawn_server`, `signal_group`, `STORE(pidfile)`, `STORE(port_lock)`, `STORE(server_log)`, `RENDER(iframe src)`, `STORE(ui-state)`, `STORE(render_dash_plist)`, `bootout_dash`, `bootstrap_dash`, `install_dash_plist`, `remove_dash_plist`.

### VERIFY before every sink

| Sink | Cleared by | Check |
| --- | --- | --- |
| `TOOL_CALL(spawn_server)` | `VERIFY(binary_admissible)`, the `PROBE` result with `answered: false` (ensure: probe), and the port lock taken by this instance (ensure: start) | path absolute, exists, regular file, executable bit set, platform supports `start_time`; addr `127.0.0.1:<port>`; argv fixed |
| `TOOL_CALL(signal_group)` | `VERIFY(ownership)` = Owned, Starting or Leftover | Owned: the slot is not marked `Exited` by its reaper, which marks it before it reaps. Starting: the slot's `child` is set and the slot is not marked `Exited`, and the lock is held by this instance. Leftover: pid alive, start time equal, executable equal, process group equal to pid, and the lock free. The four checks run again immediately before the signal. Held and Stale never reach this sink |
| `STORE(pidfile)` write | `VERIFY(spawn_result)` | the record is built from the spawn result, never copied from an earlier file |
| `STORE(pidfile)` removal | `VERIFY(ownership)` = Stale; Leftover after its group is gone; or Owned or Starting of this instance after its reaper marked it `Exited`; and `VERIFY(pidfile_path)` | the file is `<app data dir>/servers/<port>.json` for the validated port |
| `STORE(port_lock)` open | `VERIFY(pidfile_path)` | the file is `<app data dir>/servers/<port>.lock` for the validated port; it is never unlinked |
| `STORE(server_log)` | `VERIFY(log_path)` | the path is `<app log dir>/servers/<port>.log` for the validated addr port; no path separator or `..` in the name |
| `RENDER(iframe src)` | `VERIFY(identity)` for an External server, or `VERIFY(readiness)` for a server this instance spawned | the URL is one declared in `dashboards.toml` for the app's business (validated loopback), and `ensure` returned Running |
| `STORE(ui-state)` | `VERIFY(brief_seen)`, which includes `named()` and the registered-name check | the business named is the app's business; the filename matches `^\d{4}-\d{2}-\d{2}\.json$` and exists under that business's `out/briefs` |
| `TOOL_CALL(bootout_dash)` | `VERIFY(dash_target)` | label `cos.dash` and the exact plist path only |
| `TOOL_CALL(bootstrap_dash)` | `VERIFY(dash_port_held)` = Free, and `VERIFY(dash_target)` | runs only after the old pid is gone and no listener holds the port |
| `TOOL_CALL(remove_dash_plist)` | `VERIFY(dash_port_held)` = Held, and `VERIFY(dash_target)` | the `.new` file and the installed plist only; never a glob |
| `STORE(render_dash_plist)` | `VERIFY(dash_target)` | the `.new` path only; the installed plist is not written |
| `TOOL_CALL(install_dash_plist)` | `VERIFY(dash_port_held)` = Free, and `VERIFY(dash_target)` for both paths | `.new` is moved over the installed plist only after the old pid is gone and the port is Free |

### Violations

| Violation | Evidence recorded | Recovery |
| --- | --- | --- |
| invalid `dashboards.toml` | key path and rule, as a notice above the dashboard tabs and in `cos business dashboards` | no dashboards for that file until fixed; the sidebar still works |
| binary not admissible | path and failing rule | pane shows the build command; nothing spawned |
| identity probe failed | port, status line, first 200 bytes | pane shows "port held by something else", with the next step; nothing signalled |
| ownership not proven | pidfile contents, kernel start time, executable and process group | Stale: pidfile removed, nothing signalled. Held: left alone |
| server did not become ready | log tail | stop the group the instance started, remove the pidfile, show the log |
| owned child exited after ready | exit status and log tail | pane shows `exited`; the reaper removes the pidfile; nothing signalled; the next open starts a fresh server |
| child exited during readiness | log tail, and the head of the listener that answered | refuse with `failed to start`; the listener is neither embedded nor signalled |
| spawn result refused by `VERIFY(spawn_result)` (pgid, exe, start time or addr) | the failing field | failure epilogue: the new group is stopped, the lock is released, no pidfile is written; refuse with `failed to start` |
| POST to `cosreport` without an allowed Origin | origin and path, logged | 403 |
| plist unreadable or legacy | path, logged once | no clock |

### ISA test matrix

| Sink / checkpoint | Refused (tainted or failing input) | Admitted (VERIFY cleared) |
| --- | --- | --- |
| `spawn_server` | relative path; missing file; non-executable file; non-loopback addr; port already answering; port lock held by another instance; no `start_time` support on the platform | absolute executable, free loopback port, lock free: one process, pidfile written |
| `signal_group` | External listener; pidfile whose pid is alive but start time differs; pidfile whose executable differs; pidfile whose process group differs from its pid; lock held by another instance; Starting slot whose `child` is not yet set | child of this instance not marked `Exited`, in Starting (child set) or Owned; leftover with all four checks equal and the lock free |
| `STORE(pidfile)` | write attempted from an unverified pidfile read; removal of an Owned or Starting record whose reaper has not marked it `Exited`; removal of a path that `VERIFY(pidfile_path)` refuses | written from the spawn result; removal of a Stale record, of a Leftover record after its group is gone, or of an Owned or Starting record after its reaper marked it `Exited` |
| `STORE(port_lock)` | unlink of a lock file; open of a path that `VERIFY(pidfile_path)` refuses | open of `<port>.lock` for a validated addr |
| `RENDER(iframe src)` | listener whose body lacks the marker; URL not declared for the app business; non-loopback URL in the file (parse refusal); child exited during readiness | owned server ready; External listener with the marker |
| `STORE(ui-state)` | stale business name; filename outside `out/briefs`; non-date filename | app business, existing brief file named by the report frame |
| `STORE(server_log)` | path built from an unvalidated addr, or a name with a separator | `<app log dir>/servers/<port>.log` for a validated loopback addr |
| `VERIFY(origin_allowed)` | POST with `Origin: http://127.0.0.1:4777` to `cosreport://localhost/docs`; POST from a frame nested in a dashboard with `Origin: null`; POST with no Origin; GET with `Origin: http://127.0.0.1:4777` | POST with an app origin; GET with no Origin or `Origin: null` |
| Lifecycle: quit during readiness | quit while the readiness poll runs | the group is signalled and gone within the 5 s deadline; no child remains |
| Lifecycle: quit between spawn and recording the child | quit arrives after `spawn_grouped` returns and before `Spawned` is recorded | `stop_owned` waits for the record, signals the new group, and it is gone within the 5 s deadline; pidfile removed |
| Lifecycle: quit before the port lock is taken | `shutting_down` is set while the key is `Reserved` and its probe runs | the `ensure` refuses at lock-take under `inner`; no spawn; no pidfile |
| Lifecycle: concurrent first opens of one key | two `ensure` calls on one key with no slot | one probe, one lock and one spawn; the second waits on the Condvar and returns the first's `Running`, not an "another app instance" refusal |
| Lifecycle: readiness timeout | the readiness poll exhausts 10 s | the group is signalled, the pidfile removed, and `failed to start` returned with the log tail |
| Lifecycle: first open fails, second open waits | the readiness poll exhausts 10 s while a second `ensure` waits on the Condvar | the waiter returns the same `Refused`; one spawn only |
| Lifecycle: owned child exits | the child exits while its frame is shown | the slot is `Exited`; `dashboard-exited` is emitted; the pidfile is removed; nothing signalled |
| Lifecycle: second `stop_owned` | a second `ExitRequested` or `Exit` after the first | no-op; no signal sent |
| Lifecycle: crash, then launch | app killed with SIGKILL, then relaunched against the same pidfile | launch cleanup signals the leftover group; pidfile removed |
| Lifecycle: second instance | second app instance with the port lock held | the probe embeds the server; the second instance signals nothing on quit |
| Lifecycle: dead leftover | pidfile whose pid is dead | pidfile removed; nothing signalled |
| `bootout_dash`, `bootstrap_dash`, `remove_dash_plist` (`tests/schedule.rs`) | a label other than `cos.dash`; a plist path other than the exact ones; an old pid still alive after 10 s (the installed plist is not removed) | bootstrap only when the port is Free; removal of both files only when the port is Held by a pid other than the old pid, after the old pid exited; on Timeout, `.new` only |
| `install_dash_plist` (`tests/schedule.rs`) | the port is Held or Timeout; a move whose source or target is not one of the two exact plist paths | `.new` moved over the installed plist when the port is Free and the old pid has exited |

## Non-Functional Requirements

- **Security:** covered by the Semantic ISA and by §8. Stated gap: between an app crash (or a SIGTERM to the app) and the next launch, an owned server keeps running unsupervised on loopback.
- **Performance:** each watcher tick costs one `read_dir` per watched directory, one metadata call per run entry (`src/dash.rs:184-186`), and one `kill(0)` per live run. A full re-read runs only when a stamp changes. The probe has a 3 s timeout and runs only on open. The readiness poll runs every 250 ms for at most 10 s, only on open.
- **Process ownership:** every spawned server has an owner (the supervisor), a reaper thread that is its only waiter, a pidfile, a per-port lock, and one shared exit deadline. Launch cleanup stops leftovers. Nothing is spawned on a render, tick or keystroke path.
- **Rollout and rollback:** `dashboards.toml` absent means no dashboard tabs. Beyond the sidebar, this build removes the `#other-live` button (its counts are on the rows), has the report frame post `brief` and `mtime`, writes `ui-state.json`, and adds the watcher stamps. Rolling back the app leaves `dashboards.toml`, `ui-state.json` and the `servers/` files behind. Older builds read none of them, so a server still running at rollback has no supervisor until its process exits. The registry format is unchanged.

## Confidence

Rock: understood well enough to bet. Sand: the shape is known, and it will move until a named check settles it. Water: unmapped; spike it first.

| Piece | Tier | What moves it up |
| --- | --- | --- |
| `dashboards.toml` parse and validation | rock | n/a |
| Sidebar rendering and switch | rock | n/a |
| Signals from metadata and kernel checks (`proc::exe`, `kill(0)`) | sand | a test in item 25 that a zombie supervisor is counted dead; `kill(0)` alone reports it alive (§Risks, `kill(0)` row) |
| Supervisor spawn, ready, stop on exit | sand | the fake-server test suite in Build |
| Port lock and launch cleanup after a crash | sand | a test that kills the app with SIGKILL and relaunches against the same pidfile and lock, and checks that the leftover group is signalled and gone |
| Exit path: ⌘Q reaches `stop_owned` once | sand | a test that ⌘Q from the menu reaches `ExitRequested`, and that a second call is a no-op |
| Iframe embed of loopback pages inside `tauri://` | sand | Q1 passes: the frame loads, `window.__TAURI__` is undefined inside it, and the Origin on `cosreport` POSTs is recorded |
| Identity probe markers for the two real pages and the gazette | sand | markers read from the live pages once the binary is built; recorded in the runbook |
| `task schedule` 8790 guard | rock | n/a |

## Detailed Implementation Plan

### Target Files to Create/Modify

#### 1. `src/dashboards.rs` (Create)

*   **Type / Scope**: cos library module.
*   **Proposed Changes**:
    *   Types: `Dashboards { servers: BTreeMap<String, Server>, business: BTreeMap<String, Vec<Dashboard>> }`, where `servers` is `#[serde(rename = "server")]`, since the TOML table is `[server.<name>]`, `Server { binary: PathBuf, addr: SocketAddrV4, ready: Probe }`, `Dashboard { name: String, url: String, server: Option<String>, probe: Probe }`, `Probe { path: String, contains: String }`. All with `#[serde(deny_unknown_fields)]`.
    *   `location(registry: &Path) -> PathBuf`: `COS_DASHBOARDS`, else the directory of `registry` joined with `dashboards.toml`. The CLI passes `business::location()`; the desktop passes its `st.registry`.
    *   `load(path) -> Result<Dashboards>`: a missing file gives empty; otherwise parse and validate per §3. Errors name the TOML key path.
    *   `for_business(&self, name) -> &[Dashboard]` and `server_of(&self, d) -> Option<&Server>`.
    *   `probe(addr, probe, timeout) -> ProbeResult`: a plain `TcpStream` HTTP/1.0 GET, reads up to 64 KiB, `matched` when the body contains the marker. No new HTTP client dependency.
    *   `orphans(&self, registry) -> Vec<String>`: business keys not in the registry.

#### 2. `src/signals.rs` (Create)

*   **Type / Scope**: cos library module; reads metadata and kernel state only; no process spawn.
*   **Proposed Changes**:
    *   `pub struct Signals { pub live: usize, pub lost: usize, pub starting: usize, pub supervisors: Vec<i32>, pub latest_brief: Option<(String, i64)> }`.
    *   `pub fn read(home: &Path) -> Signals`: `live`, `lost` and `starting` from `agents::count`; `supervisors` from the non-terminal runs; `latest_brief` is the maximum `out/briefs/YYYY-MM-DD.json` filename, with its mtime in ms. `pub fn mtime_ms(meta: &std::fs::Metadata) -> i64` returns whole milliseconds since the Unix epoch, or 0 when the time is unavailable or predates the epoch. `store::latest` and `report::data` use the same function.
    *   `pub fn scheduled(plist: &Path, registry: &Path) -> Option<String>`: reads the plist with the `plist` crate. Returns `None` for a parse error, no `--business`, or a registry mismatch.
    *   `pub fn stamp(home: &Path) -> String`: `dash::stamp([agents::root(home), home.join("out/briefs")])`.

#### 3. `src/agents/mod.rs` (Modify)

*   **Type / Scope**: cos library.
*   **Proposed Changes**: `is_cos(pid)` (`:804-807`) reads the executable with `proc::exe(pid)` and compares its file name with `cos`. It no longer calls `comm` (`:794-801`), so `classify` (`:543-548`), `count` (`:561-592`) and every other caller of `is_cos` (`:675`, `:707`) spawn no `ps`. `count` returns `starting` beside `live` and `lost`. `started_at` (`:810-817`) is unchanged: it serves the process-group identity at `:824` and `src/agents/supervise.rs:98`, and is not on the refresh path.

#### 4. `src/proc.rs` (Modify)

*   **Type / Scope**: cos library, process helpers.
*   **Proposed Changes**: add next to `alive` (`:247`):
    *   `pub fn exe(pid: libc::pid_t) -> Option<PathBuf>` via `libc::proc_pidpath`;
    *   `pub fn start_time(pid: libc::pid_t) -> Option<(i64, i64)>` via `libc::proc_pidinfo(PROC_PIDTBSDINFO)`, returning `(pbi_start_tvsec, pbi_start_tvusec)`;
    *   `pub fn pgid(pid: libc::pid_t) -> Option<libc::pid_t>` via `libc::getpgid`, for launch cleanup's `pgid == pid` check;
    *   `pub fn signal_group(pgid: libc::pid_t, signal: libc::c_int)`, a `kill(-pgid, signal)` with the same guard as `kill_group`.

    `kill_group` (`:237-244`) stays SIGKILL only. `proc_pidpath` and `proc_pidinfo` are macOS-only and sit behind `#[cfg(target_os = "macos")]`. Off macOS, `exe` falls back to `ps -o comm= -p <pid>`, the lookup `is_cos` uses today (`src/agents/mod.rs:794-801`), and `start_time` returns `None`, so launch cleanup removes the pidfile and signals nothing. `VERIFY(binary_admissible)` refuses every spawn when `start_time` returns `None` (rule `platform`), so no child starts that `VERIFY(spawn_result)` would refuse. `pgid` uses `getpgid` on every platform.

#### 5. `src/lib.rs` and `Cargo.toml` (Modify)

*   **Type / Scope**: crate root and manifest of the cos crate.
*   **Proposed Changes**: `pub mod dashboards; pub mod signals;`. Add the direct dependency `plist = "1.10"` (already in the graph through tauri, `Cargo.lock:2437-2438`).

#### 6. `src/business.rs` (Modify)

*   **Type / Scope**: cos library.
*   **Proposed Changes**: `pub fn add(path, name, business, replace) -> Result<Vec<String>>` returns the names of existing businesses whose home contains the new home, or is contained by it (`Path::starts_with` on canonical paths), so the CLI can warn. Equal homes and workspaces are still refused (`:277-285`).

#### 7. `src/main.rs` (Modify)

*   **Type / Scope**: CLI.
*   **Proposed Changes**:
    *   `BusinessCmd::Add` prints `warning: <home> is inside <other>'s home` (or contains it) for each name `add` returns.
    *   New `BusinessCmd::Dashboards { name: Option<String> }`: prints each business's dashboards. Each line shows the business, name, URL, server (or `external`) and a probe result: `answers`, `answers, marker missing`, or `no listener`. Orphaned keys are listed at the end. The exit code is 0 unless the file is invalid.

#### 8. `apps/desktop/src/servers.rs` (Create)

*   **Type / Scope**: desktop module. The only code that spawns or signals a dashboard server.
*   **Proposed Changes**:
    *   `pub struct Supervisor { inner: Arc<Mutex<Inner>>, data_dir: PathBuf, log_dir: PathBuf, emit: Arc<dyn Fn(DashboardExited) + Send + Sync>, spawn: SpawnFn }`, managed state, where `pub struct Inner { slots: HashMap<SocketAddrV4, Slot>, shutting_down: bool }`. `slots` and `shutting_down` are read and written only under `inner`. The `Arc` lets reaper threads reach `inner`. `pub struct DashboardExited { addr: SocketAddrV4, status: String, log_tail: String }` is the `dashboard-exited` payload. `SpawnFn` is `Box<dyn Fn(&Server, &Path) -> io::Result<SpawnResult> + Send + Sync>`, where the `&Path` is the log path from `VERIFY(log_path)`, and `SpawnResult { child: Child }`. The app uses `servers::real_spawn`, which builds the `Command` (argv, working directory, inherited environment, stdin null, stdout and stderr to the log) and calls `proc::spawn_grouped`. Tests use a fake that returns the `Child` of a test binary.
    *   `pub enum Slot { Reserved(Arc<Waiter>), Starting(Starting), Owned(Owned), External, Exited { status: ExitStatus, lock: File }, Refused(Refusal) }`, with `pub struct Starting { waiter: Arc<Waiter>, lock: File, child: Option<Spawned> }`, where `Spawned` is `{pid, pgid, exit}` and is set right after the spawn returns; `stop_owned` signals a `Starting` slot only when `child` is set. Also `pub struct Owned { pid: pid_t, pgid: pid_t, exit: Receiver<ExitStatus>, lock: File }`.
    *   `pub fn new(data_dir: PathBuf, log_dir: PathBuf, emit: Arc<dyn Fn(DashboardExited) + Send + Sync>, spawn: SpawnFn) -> Supervisor`. The app passes an `emit` that calls `AppHandle::emit("dashboard-exited", ...)` and `servers::real_spawn`; tests pass a recording closure and a fake spawn.
    *   `pub fn launch_cleanup(&self)`: the steps in §4, run once in `setup`.
    *   `pub fn ensure(&self, server: Option<&Server>, d: &Dashboard) -> Result<Running, Refusal>`: the named steps in §4 (ensure: owned, probe, start). `pub struct Running { url: String, owned: bool }` carries the URL that `ensure` verified. The reaper emits `dashboard-exited` as described in §4 Slots.
    *   `pub fn stop_owned(&self)`: the stop path in §4. Idempotent.
    *   `pub enum Refusal { Identity { status: String, head: String }, BinaryMissing { path: PathBuf, build_hint: String }, NotAdmissible { rule: &'static str }, FailedToStart { log_tail: String }, NotRunning { hint: String }, StaleBusiness { message: String } }`, serialised to the pane.
    *   `fn admissible(server: &Server) -> Result<(), Refusal>`: `VERIFY(binary_admissible)`.

#### 9. `apps/desktop/src/lib.rs` (Modify)

*   **Type / Scope**: desktop commands, scheme handler, watcher, builder.
*   **Proposed Changes**:
    *   `pub mod servers;` in `apps/desktop/src/lib.rs`, so `tests/servers.rs` and the builder reach `Supervisor`.
    *   `BusinessRow` (`:379-388`) gains `unread: bool` and `scheduled: bool`. `businesses()` (`:419-440`) is the one row builder. It spawns nothing after §6.
    *   New commands, all registered in `generate_handler!` (`:983`): `dashboards(business) -> Result<Vec<DashboardRow{name, url, managed}>, String>`, `dashboard_open(business, name) -> Result<Running, Refusal>` (async, with `ensure` inside `blocking`, as `businesses` does at `apps/desktop/src/lib.rs:424`), `brief_seen(business, file, mtime)`. The three call `named()` first; `dashboard_open` maps its error to `Refusal::StaleBusiness { message }`, and the other two return it as a `String`.
    *   `watch()` (`:891-909`): per §6. It emits `businesses-changed` and `dashboards-changed`, and keeps `report-changed` and `agents-changed`. `dashboard-exited` is emitted by the reaper only (§4).
    *   `origin_allowed(req: &Request<Vec<u8>>) -> bool`, called first in the `cosreport` handler (`:963-974`), which answers 403 when it returns false.
    *   `builder()` manages `Supervisor::new(start.data_dir, start.log_dir, emit, servers::real_spawn)` in `setup`, where `emit` wraps `AppHandle::emit`, and calls `launch_cleanup`. `Start` gains `data_dir`, `log_dir` and `ui_state_file`. `from_env` fills them from `app.path()` (`apps/desktop/src/lib.rs:54` reads `app_config_dir` today), and tests fill them from a tempdir, as `settings_file` is filled in `apps/desktop/tests/ipc.rs:86-108`. The supervisor and `ui_state` read them from `Start`, never from `app.path()`.
    *   `pub fn on_run_event<R: Runtime>(app: &AppHandle<R>, event: RunEvent)` calls `stop_owned()` on `ExitRequested` and `Exit`.
    *   `ui_state` helper: reads and writes `Start.ui_state_file` with `atomic_write_bytes`.

#### 10. `apps/desktop/src/main.rs` (Modify)

*   **Type / Scope**: binary entry.
*   **Proposed Changes**: `fn main()` returns `()` (`apps/desktop/src/main.rs:3`), so `?` is not allowed. The `with_menu(builder(tauri::Builder::default(), None))` wrapping (`:4`) stays. `.run(tauri::generate_context!())` (`:5`) and its `.expect` (`:6`) become `.build(tauri::generate_context!()).expect("run the Chief of Staff window").run(cos_desktop::on_run_event)`.

#### 11. `apps/desktop/tauri.conf.json` (Modify)

*   **Type / Scope**: config.
*   **Proposed Changes**: `frame-src cosreport: http://cosreport.localhost http://127.0.0.1:*` (`:22`). Nothing else.

#### 12. `apps/desktop/ui/index.html` (Modify)

*   **Type / Scope**: markup.
*   **Proposed Changes**: wrap the existing header and `<main>` in `<div class="shell-main">` beside `<aside id="sidebar" aria-label="Businesses">`. Add a `#dash-tabs` slot in the header after Docs, a `#dash-notice` line above the tabs, and a `#view-dash` view holding `#dash-state` and `<iframe id="dash-frame" sandbox="allow-scripts allow-same-origin allow-forms">`. Remove the `#other-live` button in the Agents bar (`:180`).

#### 13. `apps/desktop/ui/sidebar.js` (Create)

*   **Type / Scope**: UI module.
*   **Proposed Changes**: `renderSidebar(rows)` builds one button per row with the live count, unread dot, lost warning and clock. Each signal has text for screen readers. A click calls `window.switchTo(name)`; a rejected call shows its message in a status line below the rows. It listens for `businesses-changed` and loads initial rows via `invoke("businesses")`. When pinned, rows are disabled with the pin title.

#### 14. `apps/desktop/ui/dash.js` (Create)

*   **Type / Scope**: UI module.
*   **Proposed Changes**: `loadDashTabs()` invokes `dashboards` and renders tabs. It also listens for `dashboards-changed` and calls `loadDashTabs()` again. `openDash(name)` shows "starting…", invokes `dashboard_open`, then sets `#dash-frame.src` to the `url` in Running, or renders the refusal card (reason, port, what answered, next command). The frame is cleared on hide. It listens for `dashboard-exited` and shows `exited` with the payload's `log_tail`. Restoring from the hash renders the idle state and calls no `dashboard_open`.

#### 15. `apps/desktop/ui/app.js` (Modify)

*   **Type / Scope**: UI.
*   **Proposed Changes**: expose `switchTo` on `window`; `openTab` (`:124-128`) matches `data-dash` as well as `data-view`, so one dashboard tab is marked current; `switchTo` (`:166-168`) writes `tab=dash&dash=<name>`, not only `tab` and `pane`; route the dashboard view in the existing view switcher; load `sidebar.js` and `dash.js`. Remove the `#other-live` code (`:1121-1135`), its call at `:141`, its click listener (`:1151`), the `businesses-loaded` listener (`:1152`) and the dispatch in `apps/desktop/ui/command.js:103`. In the `cos-report-ready` handler (`:193-195`), call `brief_seen(business, file, mtime)` only while Today is the visible view. The handler keeps the last ready payload; the switch to Today calls `brief_seen` with it. A message without a `brief` field is ignored. `VIEWS` (`:118`) gains `dash`, so the view list is today, agents, flows, docs, dash. `openFromHash` shows the dash view idle: the tabs render, the dashboard named by `dash` is selected but not opened, and no `dashboard_open` runs. With no dashboards it falls back to Today.

#### 16. `apps/desktop/ui/style.css` (Modify)

*   **Type / Scope**: styles.
*   **Proposed Changes**: the body becomes a row: sidebar of fixed width `12.5rem` (`tests/design.rs:288-308` admits no pixel length except 1px and 2px), then the main column; row states (app business, hover, disabled); signal glyphs; the refusal card. It uses only the existing tokens.

#### 17. `src/report.html` (Modify)

*   **Type / Scope**: report frame.
*   **Proposed Changes**: `cos-report-ready` (`:455`) carries `brief: "<file>"` and `mtime`, the filename and mtime of the brief the report rendered, from `DATA.briefRef`. Only the app's `/` report sends them, when `DATA.hostBrief` is true (set by `report_response` for `/`, item 17b). The `live` flag (`src/report.rs:35-36`) does not gate them.

#### 17a. `src/store.rs` (Modify)

*   **Type / Scope**: cos library, saved outputs.
*   **Proposed Changes**: `latest` keeps only names that match `YYYY-MM-DD.json`, the pattern its module comment already states (`src/store.rs:1`). It opens the chosen JSON once and takes the mtime from that handle with `mtime_ms`. `Saved` gains `mtime_ms: i64`.

#### 17b. `src/report.rs` (Modify)

*   **Type / Scope**: report render.
*   **Proposed Changes**: `data()` puts `briefRef: {file, mtime_ms}` from `Saved` into the data it renders. `report_response` for `/` (`apps/desktop/src/lib.rs:451`) adds `hostBrief: true`; `build` (`src/report.rs:24-31`) does not. Test: an archived render has no `hostBrief`, and its page posts no `brief` message.

#### 18. `Taskfile.yml` (Modify)

*   **Type / Scope**: schedule task.
*   **Proposed Changes**: the `cos.brief` steps for every item first, then the `cos.dash` steps of §7, in that order: render to `.new` without moving it; read the old pid from `launchctl print gui/$(id -u)/cos.dash`; bootout; wait up to 10 s, polling every 250 ms, for the old pid to exit; then, with `lsof -nP -iTCP:$DASH_PORT -sTCP:LISTEN`, either move `.new` into place and bootstrap (`Free`), print the holder's command and pid, skip `cos.dash`, and delete `.new` and the installed plist (`Held`), or on `Timeout` delete `.new` and fail. `DASH_PORT` is a test-only seam that defaults to 8790. `cos.brief` loads either way.

#### 19. `tests/dashboards.rs` (Create)

*   **Type / Scope**: cos integration tests.
*   **Proposed Changes**: parse and validation cases:
    *   non-loopback URL refused;
    *   unknown server refused;
    *   port mismatch refused;
    *   relative binary refused;
    *   unknown key refused;
    *   marker shorter than 12 characters refused, and a bare `<title>` refused;
    *   missing file reads as empty;
    *   orphan key reported;
    *   `cos business dashboards` output against a std `TcpListener` fixture that answers with and without the marker;
    *   a `[server.<name>]` table parses (the `servers` rename).

#### 20. `tests/signals.rs` (Create)

*   **Type / Scope**: cos integration tests.
*   **Proposed Changes**:
    *   live counted through `proc::exe`, with no `ps` spawned (asserted by putting a failing `ps` first on `PATH`);
    *   lost counted;
    *   starting counted, and included in live;
    *   latest brief by filename, with names outside `YYYY-MM-DD.json` ignored;
    *   `mtime_ms` of one file equals the `mtime` the report payload carries (one helper, one unit);
    *   `scheduled` returns the business only when `COS_BUSINESSES` resolves to the same registry; returns `None` for a legacy plist and a mismatched registry.

#### 21. `tests/business.rs` (Modify)

*   **Type / Scope**: cos integration tests.
*   **Proposed Changes**: a nested home warns and registers, and `add` returns the outer name; an equal home is still refused.

#### 22. `tests/schedule.rs` (Modify)

*   **Type / Scope**: cos integration tests.
*   **Proposed Changes**: with a `TcpListener` on a free ephemeral port passed as `DASH_PORT`, `task schedule` loads `cos.brief`, skips `cos.dash`, prints the holder, and leaves no `cos.dash.plist` in the LaunchAgents directory. A second case with no listener asserts that `cos.dash` is bootstrapped after the old pid is gone. A third case, with a fake `launchctl` whose old pid stays alive, asserts that the `cos.brief` steps ran, the task fails, `.new` is removed, and the installed `cos.dash.plist` is unchanged.

#### 23. `apps/desktop/tests/servers.rs` (Create, `harness = false`)

*   **Type / Scope**: desktop integration tests for the supervisor. The test binary doubles as the fake server: run as `<self> serve <addr>`, it binds `addr` and answers `/health` and `/` with a marker.
*   **Proposed Changes**: one case per ISA matrix row for `spawn_server`, `signal_group` and `STORE(pidfile)`, and per lifecycle row:
    *   one process for two dashboards on one server;
    *   `stop_owned` frees the port and leaves no child;
    *   a second `stop_owned` is a no-op;
    *   an external listener is never signalled;
    *   a pidfile with a wrong start time, wrong executable, or wrong process group is removed without a signal;
    *   a matching pidfile from a dead app has its group signalled by launch cleanup, and its pidfile is removed;
    *   a pidfile whose port lock is held by another instance is left alone;
    *   a server that never becomes ready has its group signalled, and its log tail is returned;
    *   quit during the readiness poll stops the group within the 5 s deadline;
    *   quit between spawn and recording the child stops the new group;
    *   an owned child that exits emits `dashboard-exited`, and its slot becomes `Exited`;
    *   the first open of a key fails while a second open waits on its Condvar: the waiter returns the same `Refused`, and only one spawn happens;
    *   a spawn whose result fails `VERIFY(spawn_result)` (fake spawn) is stopped, its lock released, no pidfile written, and `failed to start` returned;
    *   the `dashboard-exited` payload carries `log_tail`, and the reaper removes the pidfile at exit;
    *   quit before the port lock is taken: no spawn and no pidfile;
    *   two concurrent first opens of one key: one probe, one lock, one spawn, and the second returns the first's `Running`;
    *   an `Exited` slot keeps its port lock until `ensure: owned` drops it.

#### 24. `apps/desktop/Cargo.toml` (Modify)

*   **Type / Scope**: manifest.
*   **Proposed Changes**: `[[test]] name = "servers" harness = false`; add `libc = "0.2"`, which the desktop crate does not have yet. `cos` already depends on it.

#### 25. `apps/desktop/tests/ipc.rs` (Modify)

*   **Type / Scope**: desktop IPC tests.
*   **Proposed Changes**:
    *   `businesses` rows for three businesses, with the unread dot set and then cleared by `brief_seen`, and the clearance surviving a new `App` instance;
    *   `dashboard_open` refused for a stale business;
    *   `brief_seen` refused for a filename outside `out/briefs`;
    *   `cosreport` answers 403 to a POST with a foreign Origin, a POST with `Origin: null`, and a POST with no Origin; and 200 to a GET with `Origin: null`;
    *   `businesses` does not spawn `ps` (failing `ps` on `PATH`);
    *   CUJ 8: a SIGKILL to a run's supervisor without a file change turns the row to lost within one tick.
    *   `brief_seen` emits `businesses-changed`, and the emitted payload shows the dot cleared (the test observes the event, not only the `businesses` IPC).
    *   `brief_seen` never lowers the stored pair;
    *   the UI-gating cases (a `cos-report-ready` message without `brief` is ignored; `brief_seen` is not called while another view is visible and is called on the switch to Today) are not in this file. `ipc.rs` runs on the mock runtime with no webview, so they run as E2E steps in §Verification Plan.
    *   A `Starting` run turns Lost after `START_GRACE` with no file change, and the row updates without an IPC call.

#### 26. `README.md` (Modify)

*   **Type / Scope**: docs.
*   **Proposed Changes**: a "Dashboards" section with the `dashboards.toml` example, the ownership rule, and `cos business dashboards`; index lines for this design, its reviews and the plan.

#### 27. `docs/plans/2026-11-13-business-dashboards-design.md` (this document)

*   **Type / Scope**: design document, revised in place.

### Verification Plan

*   **Compilation/Build**: `task build`, `task desktop-check`.
*   **Unit and integration tests**: `task test` (fmt, clippy `-D warnings`, `cargo test`) and `cargo test --workspace`, which runs `apps/desktop/tests/ipc.rs` and `servers.rs`.
*   **Static analysis**: clippy `-D warnings` on both crates.
*   **ISA matrix**: every row in the test matrix above maps to a named case in the bullets of items 19 to 25, and each case runs in the test file its item names. The two UI-gating cases of item 25 run as E2E steps below.
*   **States without a CUJ**: the clock state is covered by `tests/signals.rs`; the binary-missing and failed-to-start states are covered by `apps/desktop/tests/servers.rs`.
*   **E2E (built app, real machine)**:
    *   run Q1 checks 1 to 4 before any other E2E step, and record check 3;
    *   register the four businesses;
    *   build the control room; write `dashboards.toml`;
    *   run CUJ 1 to 8;
    *   the two UI-gating cases of item 25 (a `cos-report-ready` message without `brief` is ignored; `brief_seen` is not called while another view is visible and is called on the switch to Today);
    *   capture `lsof -nP -iTCP:4777 -sTCP:LISTEN` before open, after two opens and after ⌘Q;
    *   capture `launchctl list com.longhand.chief-of-staff-dashboard` before and after;
    *   record screenshots with `pr-proof`.
