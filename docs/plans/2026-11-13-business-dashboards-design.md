# Business sidebar and per-business dashboards

*   **Author:** Decebal Dobrica
*   **Status:** In Review
*   **Date:** 2026-11-13
*   **Brief:** [business dashboards brief](2026-11-13-business-dashboards-brief.md)
*   **Base design:** [multi-business design](2026-11-12-multi-business-design.md)

--------------------------------------------------------------------------------

## Decision

The desktop app gets a left sidebar listing every registered business. Each row shows three signals (live agent runs, an unread brief and lost runs) and a clock on the one business that launchd schedules. Clicking a row runs the same switch as ⌘K. Each business may declare dashboards in a new hand-edited file, `~/.config/cos/dashboards.toml`, which no `cos` command writes. A dashboard is a loopback URL with an identity probe, and optionally a server binary that the app owns:

- The app starts an owned server the first time one of its dashboards opens. It records the server in a pidfile and holds a per-port lock until it reaps the server process. It stops the server, and any process left in its process group, when the app exits.
- A crash, or a SIGTERM to the app, can leave an owned server running. The next launch stops that server, and any member left in its process group, when its pidfile still matches the process and its executable is the binary `dashboards.toml` declares for that port. The next launch does not adopt it.
- A listener the app did not start is embedded only after its identity probe passes. The app never signals it.
- Founding Bets declares the control room and the salesroom as one owned server, `portfolio-control-room serve 127.0.0.1:4777`. Longhand declares its gazette at `:8790`, which launchd owns.

This design registers four businesses: `founding-bets`, `longhand`, `realestate` and `kronos`.

Three answers in the [brief](2026-11-13-business-dashboards-brief.md#decisions-made-with-the-owner) are superseded by later owner decisions. Frames are allowed from `http://127.0.0.1:*`, where the brief named two exact origins (brief D6 and its "wildcard frame origins" non-goal). The executable-path check (brief D5) applies only to pidfile cleanup, and external listeners are identified by a page marker. A server left by a crashed app is shut down at launch, where brief D4 adopts it.

This design supersedes one non-goal of the base design (`2026-11-12-multi-business-design.md:58-59`, "a dashboard … that follows the app's switch"): dashboards here follow the app's business.

## Overview

The owner runs four businesses from pinned Codex threads. Each has a chief-of-staff workspace, and two have a dashboard of their own: the Founding Bets control room and salesroom (`~/Projects/founder-mode/portfolio-control-room/control-room`, a Rust server on `127.0.0.1:4777`) and the Longhand gazette (a node server on `127.0.0.1:8790`, run by the launchd job `com.longhand.chief-of-staff-dashboard`). The desktop app shows neither, and it shows only one business at a time, so nothing in the window says which business needs attention.

This design adds three things to the app: a sidebar of businesses, a dashboard pane per business, and ownership of the one dashboard server the owner wants the app to run. On macOS, status signals are computed from file metadata and kernel checks (`proc::exe`, `kill(0)`), with no process spawn. Off macOS, `exe` falls back to `ps` (Detailed Implementation Plan, item 4). It adds one config file (`dashboards.toml`), two `cos` modules (`dashboards`, `signals`) and one subcommand, and registers the four businesses.

## The Problem

- **No overview.** ⌘K lists businesses with a live-run count, but only while the picker is open. The live-run count for the other businesses, shown in the Agents bar (`#other-live`, `apps/desktop/ui/index.html:180`), refreshes only when the Agents tab opens (`apps/desktop/ui/app.js:141`) or the ⌘K picker loads its rows (`apps/desktop/ui/command.js:103` dispatches `businesses-loaded`, handled at `apps/desktop/ui/app.js:1152`). A new brief, a lost run or the scheduled business is invisible.
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
5. Opening a dashboard whose server the app owns starts at most one server process, however many of its dashboards open. Quitting the app stops every server the app started and, within the stop path's 5 s + 1 s budget, leaves no process on the port, including a process in the server's group that outlives the server (§4 Reaper). A stop target still alive when the budget expires is logged and quit returns (§4 Stop path step 6). A spawn recorded after the stop path has returned is sent SIGTERM at its record and is otherwise left to launch cleanup (§4 Stop path, Risks). A SIGTERM or a crash that skips the quit path is covered by Goal 7.
6. A listener the app did not start is embedded when its identity probe passes, refused with a visible reason when it fails, and never signalled by the app.
7. After a crash, a force-quit or a SIGTERM, the next launch first takes the pidfile's port lock. A pidfile whose port lock another instance holds is left alone. With the lock free, the next launch takes it and stops the server the earlier app started when the pidfile's pid, start time, executable and process group all match the live process (the live process group equals the pid and the record's `pgid`), and the executable is the canonicalised `binary` of a server declared in `dashboards.toml` whose `addr` port equals the pidfile's port. When a member of that server's process group outlives the leader, launch cleanup sends one SIGKILL to the group (§4 Launch cleanup). A pidfile under a free lock that does not match, or that matches no declared server, is removed without a signal.
8. Every dashboard state is visible in its pane: starting, running (owned), running (external), refused (identity), not running, binary missing, binary not admissible, failed to start, exited, and the refusals that resolve no server: stale business, unknown dashboard, invalid `dashboards.toml`, and internal error.
9. With the gazette running, `task schedule` loads `cos.brief`, skips `cos.dash` and names the process holding port 8790.

## Non-Goals

- The app building a dashboard server. The app never runs `cargo`; a missing binary shows its path and points to the Setup Runbook, which holds the build command.
- Bundling a dashboard binary in the app (`externalBin` stays empty).
- The revenue desk (`:4788`). The owner chose the control room and salesroom only.
- Starting, stopping or restarting a launchd-owned dashboard (the Longhand gazette).
- Scheduling more than one business, per-business launchd labels, and moving the `cos dash` port.
- A generic plugin system for health checks, or non-loopback dashboard URLs.
- Dashboards that follow the CLI's active business. Only the app's business counts.
- A display name separate from the business slug.

## Open Questions

- **Q1 (go/no-go gate, before Build):** iframe embedding holds only if checks 1, 2 and 4 pass in the built app, and check 3 is recorded; otherwise the fallback below applies.
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
*   **Then**: the launch cleanup finds `4777.json` and takes the free `4777.lock`. The pid is alive, its start time, executable and process group match the record, and its executable is the declared `control-room` binary, whose `addr` port is 4777. The cleanup sends SIGTERM to that process group. If a member of the group ignores SIGTERM and outlives the leader, the cleanup sends one SIGKILL to the group while `proc::group_alive` reports it alive and the cleanup still holds `4777.lock`. It removes the pidfile. The next open of a control-room tab starts a fresh server. If the pidfile names a dead pid, a live process that does not match, or an executable that is not the declared binary for port 4777 (for example after `[server.control-room]` is removed from `dashboards.toml`), the cleanup removes the pidfile and signals nothing.

### CUJ 8: A supervisor dies without a file change

*   **Given**: `founding-bets` has one live run, and its supervisor process is running.
*   **When**: the owner sends SIGKILL to the supervisor process (`kill -9`), and no file in the run directory changes. SIGTERM is not used: the supervisor traps it and writes a terminal outcome (`src/agents/supervise.rs:30-40`).
*   **Then**: within one tick (700 ms) the row's live count drops and the lost warning appears.

## Technical Plan (Architecture)

### 1. Terms

- **Business:** a registry entry, as in the base design. A business has three directories, `home`, `workspace` and `projects` (`src/business.rs:20-24`).
- **Registry:** `businesses.toml`, at `COS_BUSINESSES`, else `$XDG_CONFIG_HOME/cos/businesses.toml`, else `~/.config/cos/businesses.toml` (`src/business.rs:40-66`). It holds the active business and each business's directories (`src/business.rs:27-29`).
- **Start registry:** the registry path the desktop app resolves once at start (`Start.registry`, `apps/desktop/src/lib.rs:32,53`). The path is fixed for the app's life; the file's contents are read again on each call that needs them.
- **Triple:** the home, workspace and projects one window uses, with the registry business it resolves to, if any, and the layer that chose it (`Triple`, `src/business.rs:337-343`). `Triple::label` (`src/business.rs:347`) is the business name, else `default` for the built-in defaults layer, else `explicit`.
- **cos:** the command-line tool and library in this repository, described in the [base design](2026-11-12-multi-business-design.md).
- **App business:** the business one app window shows. ⌘K or the sidebar sets it per window. Its variables are pinned at start by env (`COS_HOME` and the others listed at `apps/desktop/src/lib.rs:110-120`); a pinned window cannot switch (`apps/desktop/src/lib.rs:329-331`). A window whose triple has no registry business (labelled `explicit`, or `default` for the built-in defaults layer, `src/business.rs:347-352`) has no dashboards, whatever `dashboards.toml` declares under that label (§5), and records no seen brief: `brief_seen` refuses it (§6), so a registered business named `default` never takes another home's brief as seen.
- **Active business:** the business the CLI uses, stored in the registry as `active`. `cos business list` prints it as `current:`.
- **Brief:** the report written by the scheduled `cos.brief` job, which runs on weekdays at 08:30 (`launchd/cos.brief.plist:20-24`). Briefs are stored under `out/briefs` (`src/store.rs:22`).
- **Profile:** the business's own `profile.json`, read by agents and reports (Alternative C).
- **Unsaved-Hire guard:** a business switch is refused while the Hire dialog holds unsaved edits (`apps/desktop/ui/app.js:153-163`).
- **Live, starting and lost runs:** a run is live when its supervisor pid is a `cos` process; starting when it has no supervisor pid within `START_GRACE` of creation (`src/agents/mod.rs:44`); lost otherwise (`src/agents/mod.rs:543-548`).
- **Run supervisor:** the `cos agents supervise` process of one agent run.
- **Server supervisor:** the desktop component that starts and stops dashboard servers.
- **Managed server:** a `[server.<name>]` entry with a `binary`. **Unmanaged dashboard:** a dashboard with no `server` key; the app never starts it.
- **Owned:** a runtime state only. This app instance spawned the process and holds its port lock until the reaper reaps it. Only owned servers are signalled on exit.
- **External:** a runtime state. The port answered, and the identity probe passed. The app embeds the listener and never signals it. It is not stored, so every open probes again.
- **Leftover:** a pidfile from an earlier app instance whose process still matches the record, and whose executable is the declared binary of the server on the pidfile's port.
- **Refusal:** the reason the dashboard pane shows instead of a page.
- **Dashboard:** a named page shown for one business: a loopback URL plus an identity probe.
- **Identity probe:** an HTTP GET of a path. It passes when the body contains the dashboard's page marker.
- **Today, Agents, Flows and Docs views:** the app's tabs (`apps/desktop/ui/app.js:118`). Today shows the brief report served from `cosreport://localhost/` (`apps/desktop/src/lib.rs:442`). Agents shows the run list (`apps/desktop/ui/app.js:260`). Flows shows the flow view (`apps/desktop/ui/index.html:20`). Docs shows the documentation room (`apps/desktop/src/lib.rs:472`).
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
                                    row builder (cos::signals, no process spawn)
                                    publish ─► shared row cache ◄─ brief_seen
  dashboard pane (ui/dash.js)
        │ open(name)
        ▼
  dashboard_open ─► server supervisor ─► ensure ─► spawn ─► pidfile + port lock (app data dir)
                          ▲
  launch cleanup (setup) ─┘   RunEvent::ExitRequested / Exit ─► stop_owned
```

### 3. `dashboards.toml` (cos crate, new `src/dashboards.rs`)

The file lives next to the registry: `COS_DASHBOARDS`, else the directory of the registry file, joined with `dashboards.toml`. The CLI reads `COS_DASHBOARDS` from its environment and takes the registry from `business::location()` (`src/business.rs:41`). The desktop reads `COS_DASHBOARDS` once in `Start::from_env`, as it reads every other start input (`apps/desktop/src/lib.rs:43-67`), into `Start.dashboards_file`, and otherwise uses the start registry (§1); it never reads `COS_DASHBOARDS` after start, so tests that build `Start` directly stay hermetic. No `cos` command writes the file. A missing file means no dashboards.

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

The control-room markers come from the repository: the title at `control-room/static/index.html:8`, the script tag at `control-room/static/sales.html:9`, and the `/health` body at `control-room/src/server.rs:185-189`. The gazette marker is read from the live page during Build and recorded in the runbook. The placeholder `<measured in Build>` is refused by the tag rule below, so the file does not load until the real marker replaces it.

Validation, all at parse time; the first error is returned with its key path:

- every URL is `http://127.0.0.1:<port>/...`; any other scheme or host is refused;
- every port, in a URL and in an `addr`, is from 1 to 65535; port 0 is refused, because a slot, a pidfile and a lock are named by the port, and launch cleanup admits only ports 1 to 65535 (`VERIFY(pidfile_path)`);
- every `probe.path` and `ready.path` matches `^/[!-~]*$` (starts with `/`, printable ASCII, no space, CR or LF), because the probe writes it verbatim into the request line of an HTTP/1.0 GET;
- a dashboard with `server` names a declared server, and its URL's port equals that server's `addr` port;
- `addr` ports are unique across all `[server.*]` tables; a duplicate is refused with both keys named;
- a dashboard with no `server` whose URL port equals a declared server's `addr` port is refused, naming the server it must declare, so one port never maps to both a managed and an unmanaged dashboard;
- a server's `binary` is absolute; `addr` is `127.0.0.1:<port>`; the argv is fixed as `[binary, "serve", addr]` and is not configurable;
- a probe marker and a `ready` marker have at least 12 characters and, after trimming whitespace, do not match `^<[^<>]*>$` (a single tag with nothing outside it, such as `<title>`, `<div class=x>` or `<measured in Build>`);
- `binary`, `addr` and `ready` are required on a server, and `name`, `url` and `probe` on a dashboard; `server` is the only optional key; a missing key is refused with its key path;
- unknown keys are refused; names are unique per business; business keys follow the registry's name rule;
- a business key not in the registry is reported as orphaned by `cos business dashboards` and is not an error, so renaming a business never breaks the file.

The binary's existence and file type are checked when it is started, so `cos business list` works on a machine without the binary.

`cos business dashboards [<name>]` prints each dashboard, its server, and whether its URL answers its probe now (one `PROBE` attempt, 3 s timeout). It never starts anything, calls no `business::first_run` (which every other `business` subcommand calls, `src/main.rs:506,539,559,565,590`) and writes nothing. It loads the registry with `business::load` for the orphan report: a missing registry is empty (`src/business.rs:79`), so every key is reported orphaned, and a registry that fails to load prints the error to stderr and exits 1. The file is loaded and validated first: an invalid file prints its key path and rule to stderr and exits 1, before any name lookup. A `<name>` with no `[[business.<name>]]` entry in a valid file prints `no dashboards for <name>` and exits 0; the name is not checked against the registry.

### 4. Server supervisor (desktop, new `apps/desktop/src/servers.rs`)

The server supervisor is managed Tauri state. On construction it creates `<app data dir>/servers` and `<app log dir>/servers`, because `atomic_write_bytes` writes a temporary file beside its target and does not create the parent directory (`src/state.rs:555-569`). A failure aborts `setup`, as `Start::from_env` does (`apps/desktop/src/lib.rs:977`).

**Slots.** The supervisor keeps one slot per key, the host and port of the dashboard's `url`. Validation (§3) makes the key equal the server's `addr` and gives each port at most one server, so two names for one port cannot start two processes; an unmanaged dashboard such as the gazette is keyed by a port no declared server uses. One supervisor lock guards lookup, insert and every slot transition; probes, the spawn and readiness run outside it. A slot is in one of five states:

- **Reserved:** inserted at lookup, before any probe, lock or spawn.
- **Starting:** replaces Reserved when the port lock is taken. Holds the lock and, once the spawn is recorded, the pid and the stop target.
- **Owned:** the server passed readiness and identity. Holds the pid, the stop target (always a group) and the port lock.
- **Exited:** the reaper saw the child exit. Holds no port lock: the reaper holds it until it has finished. Keeps the stop target, the reaper's completion notice and, when the slot was Starting, a link to the `ensure` still running. When the slot was Owned, the pane shows `exited` with `{addr, status, log_tail}` until the next open, where `status` is `exit <n>` or `signal <n>` and `log_tail` is the last 20 lines of `<app log dir>/servers/<port>.log`. A Starting slot that turns Exited is reported by its `ensure`'s `failed to start` (Reaper step 2).
- **Refused:** holds a refusal, returned to every waiter and removed by the next open.

An External listener has no slot. Rules that hold for every slot:

- An `ensure` changes a slot only while the slot at its key is still the one it reserved. Recording the spawn, writing the pidfile, the Owned transition and the failure epilogue's Refused each check this under the supervisor lock.
- Concurrent opens of one key wait on the slot. Every transition out of Reserved or Starting, and the spawn record, wakes them.
- Each child has a completion notice that its reaper sets after the reap and the group check. A thread that needs the reaper finished waits on it with a deadline, without holding the supervisor lock. Replacing a slot never discards the notice.

**Stop target.** Fixed once after the spawn from `proc::pgid(pid)`: the process group when it returns the child's own pid, the leader pid alone otherwise (another pgid, or none). A leader target means the group is not one the app created; only the leader is signalled, through `proc::signal_pid`, and that pid cannot be reused while the leader is unreaped. `VERIFY(spawn_result)` refuses a leader target, so an Owned slot always has a group target, and a leader target exists only in a Starting slot between its spawn record and the failure epilogue, and in the Exited slot after it. The failure epilogue, the stop path and the reaper all use this one target; group signals, `proc::group_alive` polls and `proc::kill_group` apply only to a group target.

**Reaper.** One thread per child, started in the same critical section that records the spawn in the Starting slot, so no Starting slot has an unrecorded child. It is the child's only waiter. It calls `waitid(P_PID, pid, WEXITED | WNOWAIT)`, which returns while the leader is an unreaped zombie; an unreaped zombie leader keeps its process group id in use. It accepts the result only for `CLD_EXITED`, `CLD_KILLED` or `CLD_DUMPED`, and waits again for any other code: Darwin's `waitid` has been reported to return for a stopped child (golang/go#19314), and a stopped server must stay Owned so the stop path still signals it. The exit status is `exit <n>` for `CLD_EXITED` and `signal <n>` for the other two. In order, the reaper then:

1. Under the supervisor lock, marks the slot Exited (waking the waiters of a Starting slot), takes over the port lock and removes the pidfile.
2. Calls `emit` (item 8) with `{addr, status, log_tail, owned}`, where `owned` is true only when the slot was Owned; the app's `emit` emits `dashboard-exited` only for `owned: true`. A child that exits while Starting is reported by its `ensure`'s `failed to start`, so the pane shows one state.
3. Reaps the child with `waitpid`.
4. When the stop target is a group and `proc::group_alive(pgid)` (`src/proc.rs:253`) is true, a process the server started is still in its group, and the reaper sends SIGKILL to the group with `proc::kill_group(pgid)` (`src/proc.rs:237`). A pgid stays reserved while any member lives (`src/proc.rs:234`), so a group that still has a member is the server's group.
5. Releases the port lock and sets the completion notice. From then on another app instance can take the lock and start its own server.

Invariant: every signal to a slot's stop target (at a spawn record after quit began, in the failure epilogue or in the stop path) is sent under the supervisor lock and skipped once the slot is Exited, so no signal reaches a reaped leader or its group. The reaper's group kill after the reap is the only signal sent to such a group; Risks names its check-then-kill window.

**Port lock.** Before it spawns, each app instance takes `flock(LOCK_EX | LOCK_NB)` on `<app data dir>/servers/<port>.lock`, and keeps it while the slot is Starting or Owned. The reaper releases it after the reap and the group check, then sets the completion notice, so an Exited slot whose notice is unset still has its lock held by its reaper. A slot that never spawned a child releases the lock when the failure epilogue drops it. The kernel drops the lock when the process dies. That holds only because no spawned server inherits the lock: the lock file is opened through `std::fs::File`, which sets close-on-exec, and the child inherits no app file descriptor except null stdin and the log as stdout and stderr; an inherited lock would let a leftover hold `<port>.lock` after a crash, and Launch cleanup step 1 would then leave it running. An instance that cannot take the lock does not signal the server it guards. Lock files are opened, locked and unlocked, never unlinked: unlinking a locked path lets a second instance lock a new inode at the same path.

**`ensure(server, dashboard)`** returns Running (the verified URL and whether the server is owned) or a refusal. It runs these named steps in order:

- **ensure: owned.**
    - An Owned slot is reused. Outside the supervisor lock, `ensure` runs the requesting dashboard's identity probe (`VERIFY(identity)` over `PROBE`, with the identity retry rule of `PROBE`) on the key's address. A body with that dashboard's marker returns Running with that dashboard's `url`. Any other answer refuses with `identity` marked `owned`, naming the status line and the first 200 bytes, and leaves the Owned slot and its server as they are. The listener is the app's own server, so the pane names no `lsof` or stop step (item 14): the server is stopped only by the stop path or its own exit. Every dashboard on a shared server is checked against its own marker, including the second dashboard on a server another dashboard started (Salesroom after Control room).
    - An Exited slot with no running `ensure` is dropped once its completion notice is set, because its reaper has then released the port lock. If the notice is unset, `ensure` waits on it for up to 5 s (the reaper's remaining work is the `waitpid` of an exited leader and one group check), then runs again from this step. At the deadline it refuses with `not running`, hint "the previous server on this port is still exiting; open the dashboard again", and leaves the slot. A reopen right after an owned server exits therefore never takes the lock while its own reaper still holds it.
    - A Refused slot is dropped.
    - A Reserved or Starting slot, or an Exited slot whose `ensure` has not finished its failure epilogue, is waited on. When the slot ends Owned, the waiter runs this step again, so it returns Running only after its own dashboard's identity probe passes. When the slot ends Refused it returns a copy of that refusal; when the slot is removed it runs this step again. A waiter spawns nothing.
- **ensure: reserve.** Under the supervisor lock, with no slot for the key: if quit has begun, refuse with `not running`, hint "the app is quitting" (the hint of every quit refusal below); otherwise insert Reserved. The probe, the lock and the spawn run after this step.
- **ensure: probe.** `CONNECT`: a TCP connect to the key's address, through the injected `connect` function (item 8). If it answers, run the identity probe (`VERIFY(identity)` over `PROBE`, HTTP GET of `probe.path`, with the identity retry rule of `PROBE`). The control room accepts one connection at a time (`control-room/src/server.rs:14-22`) and sets 2 s socket timeouts (`control-room/src/server.rs:39-40`), so a single probe can wait behind another connection. A body with the marker gives External: embedded, never signalled; the Reserved slot is removed and its waiters run `ensure` again. Any other answer refuses with `identity`, not marked `owned`, naming the status line and the first 200 bytes, and replaces Reserved with Refused.
- **ensure: start.** Reached only when `CONNECT` reports no listener. Every refusal in this step replaces Reserved with Refused and wakes the waiters.
    - No `server` declared (unmanaged dashboard): refuse with `not running`, hint "its owner (for example launchd) is not running it". The gazette takes this path.
    - Server declared: `VERIFY(binary_admissible)` checks that the path is absolute, a regular file and executable, and that `proc::start_time` is supported on this platform (rule `platform`, item 4). A missing file refuses with `binary missing`, the path, and the fixed hint "build it by hand; the Setup Runbook has the command". Any other failure refuses with the rule name.
    - Under the supervisor lock: if quit has begun, refuse with `not running`, hint "the app is quitting". Otherwise take the port lock without blocking, in the same critical section, and replace Reserved with Starting. If another instance holds the lock, refuse with `not running`, hint "another app instance is starting it; open the dashboard again".
    - `VERIFY(log_path)` builds `<app log dir>/servers/<port>.log` from the validated port, which is a parsed integer, so the name has no separator or `..`; it has no refusal branch.
    - `STORE(server_log)`: truncate the log to its last 1 MiB, creating it if absent, before the spawn function runs, so the tests' fake spawn goes through the same truncation.
    - Spawn through the injected spawn function (item 8). The app's spawn calls `proc::spawn_grouped` (`src/proc.rs:165`, which puts the child in its own process group), with the binary's parent as working directory, the app's environment, null stdin, and stdout and stderr appended to the log. The control room bakes its root in at compile time (`control-room/src/main.rs:31`), so the working directory does not change what it serves. A spawn error runs the failure epilogue with `failed to start`. Otherwise, in this order:
        1. Read the child's pid, `pgid` (`proc::pgid`), `start_time` and `exe` (`proc::exe`, so launch cleanup compares the same resolved path), and fix the stop target. The child is unreaped and has no waiter yet, so its pid cannot be reused during these reads.
        2. Under the supervisor lock, record the spawn in the Starting slot, wake the waiters and start the reaper. If quit has begun, send SIGTERM to the stop target in the same critical section and add the slot to the stop path's collected set (§4 Stop path step 2); a quit that begins later signals it from the stop path.
        3. Run `VERIFY(spawn_result)` on the values from step 1 and the validated `addr`. A refusal runs the failure epilogue; no pidfile exists yet.
        4. Under the supervisor lock, if the slot is still this `ensure`'s Starting slot, write the pidfile atomically (`src/state.rs:555`) with `{pid, pgid, start_time, exe, addr}`. If the reaper has already marked it Exited, no pidfile is written and the failure epilogue runs with `failed to start` and the log's last 20 lines.
    - After the pidfile is written, if quit has begun, the failure epilogue runs with `not running`, hint "the app is quitting".
    - Readiness: poll `ready` every 250 ms for at most 10 s; each poll is one `PROBE` attempt, with no retry. A pass needs two consecutive passing probes, with no exit reported by the reaper before the first or after the second. A child exit during readiness runs the failure epilogue with `failed to start` and the log's last 20 lines; a listener that answered then belongs to another process and is neither embedded nor signalled. A timeout runs the failure epilogue with `failed to start`.
    - Identity after readiness: the `ready` marker is generic (`"status":"ok"`), so a foreign listener that bound the port between ensure: probe and the child's bind can pass readiness. `ensure` therefore runs the dashboard's identity probe after a readiness pass. A body without the marker runs the failure epilogue with `identity` marked `spawned` (not `owned`), naming the status line and the first 200 bytes. The listener that answered is either the app's own child, whose page lacks the marker (marker drift, Risks), or a foreign listener that took the port first; the epilogue has stopped the child, so the pane names the dashboard's `probe` as the first thing to check and gives the `lsof` step only for a listener still on the port (item 14). A foreign listener that also carries the marker is embedded, as an External listener would be; the child then fails to bind and exits (the control room returns an error from `TcpListener::bind`, `control-room/src/server.rs:11-12`), and the pane shows `failed to start` or `exited` (Risks).
    - Success: under the supervisor lock, replace this `ensure`'s Starting slot with Owned and wake the waiters. If the reaper marked the slot Exited after the second readiness probe, the failure epilogue runs with `failed to start`.
- **Failure epilogue.** Every refusal after the port lock is taken runs these steps in order:
    1. If a spawn was recorded: unless the reaper has marked the slot Exited, send TERM to the stop target under the supervisor lock; wait on the completion notice for up to 5 s; if it is still unset, send KILL to the stop target under the same Exited check, then wait until the notice is set. The final wait ends because SIGKILL cannot be caught and the reaper sets the notice after the reap. Both signals are admitted by `VERIFY(ownership)` = Starting; for a leader target only the leader is signalled, and `VERIFY(group_after_reap)` skips the group kill.
    2. Remove the pidfile if it still exists. `VERIFY(ownership)` admits this only once the reaper has marked the slot Exited, so an unreaped group keeps its pidfile.
    3. When a child was spawned, its reaper has released the port lock after the reap; when none was, the Starting slot still holds it and step 4 releases it.
    4. Under the supervisor lock, if the slot at the key is still this `ensure`'s (Starting, or Exited and linked to it), replace it with Refused; in every case wake the waiters. A waiter that wakes on Refused returns that refusal and spawns nothing.

**Launch cleanup.** Runs once in `setup`, before any `ensure`, with `dashboards.toml` loaded from its location (§3). A missing or invalid file gives no declared servers, so every pidfile under a free lock (step 1) is removed without a signal, and an invalid file is logged once. It handles each `<app data dir>/servers/<port>.json` whose `<port>` is admitted by `VERIFY(pidfile_path)`: the canonical decimal form of a port from 1 to 65535, so `04777.json` and `70000.json` are not admitted. Any other entry is left unread and unsignalled, because a non-canonical name would lock a different `<port>.lock` from the one the owning instance holds.

1. Take `<port>.lock` without blocking. If it is held, another instance owns the server: leave the pidfile and the process alone.
2. The lock is free. A record that does not parse, or whose `addr` port differs from `<port>`, is Stale: remove the pidfile and signal nothing. If no declared server has `<port>` as its `addr` port (validation allows at most one), remove the pidfile and signal nothing.
3. Run the five checks: the pid is alive, its start time equals `start_time`, its executable (`proc::exe`) equals `exe`, its live `proc::pgid` equals the pid and the record's `pgid`, and that executable equals the declared server's `binary`, both canonicalised with `std::fs::canonicalize` (a path that fails to canonicalise is a mismatch). If any fails, remove the pidfile and signal nothing.
4. All five pass: send SIGTERM to the process group. Every launch-cleanup signal in this step goes to the group whose id is the verified pid (`-pid`); the record's `pgid` is compared and never used as a target. Then poll the five checks every 50 ms, within a 5 s deadline per leftover, until the leader fails them.
    - The leader still passes at the deadline: send SIGKILL to the group.
    - The leader fails the checks before the deadline: it has exited, and launchd has reaped it, because the leftover is not the app's child. If `proc::group_alive(pid)` (`src/proc.rs:253`) is true, cleanup still holds `<port>.lock` and the deadline has not passed, send one SIGKILL to the group with `proc::kill_group(pid)` (`src/proc.rs:237`), admitted by `VERIFY(leftover_group)`. This is the reaper's rule (§4 Reaper) applied to a leftover.
5. Remove the pidfile.

The five checks run again immediately before the SIGTERM and immediately before a deadline SIGKILL, and `group_alive` runs immediately before the group SIGKILL after the leader's exit. Each leftover receives at most one SIGKILL. Cleanup does not wait for the group to be gone after the SIGKILL. It releases each `<port>.lock` after that port's pidfile is handled, so a later `ensure` on the port can take it. It runs synchronously in `setup`, so `ensure` does not run before it returns, and each leftover adds at most the 5 s deadline to setup.

**Stop path.** `stop_owned()` runs from `RunEvent::ExitRequested` and `RunEvent::Exit`, which requires the switch in `apps/desktop/src/main.rs` described in item 10. It is idempotent: a second call is a no-op, and slots are drained once. In order:

1. Under the supervisor lock, mark quit as begun. The 5 s deadline starts at that moment, steps 2 and 3 share it, and step 4's 1 s follows it. In the same critical section, collect every slot that has a child: each Owned slot, each Starting slot with a recorded spawn, and each Exited slot whose completion notice is unset. Send SIGTERM to the stop target of each Owned or Starting slot (the group for a group target, the leader alone for a leader target). An Exited slot is not signalled; its reaper's group check covers it.
2. Within the deadline, wait on each Reserved slot and each Starting slot with no recorded spawn until it has a recorded spawn, is Refused, is Exited, or is gone. A spawn recorded after step 1 is sent SIGTERM at its record and joins the collected set (ensure: start step 2), so a slot still probing does not use up the TERM grace of the servers signalled at step 1.
3. Outside the lock, wait on every collected completion notice within the same 5 s deadline.
4. Under the supervisor lock, send SIGKILL to the stop target of each slot still Owned or Starting. Wait up to 1 s more on the notices still unset.
5. Within the same 5 s + 1 s budget, poll `proc::group_alive(pgid)` every 50 ms for each collected group target whose notice is set, until it is false. A leader target is not polled: its group is not one the app created. A reaper sets its notice only after it has reaped the leader and, for a group still alive, sent SIGKILL to it, so this poll ends at once unless a member is mid-exit.
6. When the budget expires, log each stop target whose notice is unset or whose group is alive, and return, so quit never blocks on a reaper.
7. Remove any pidfile a reaper left once its slot is Exited, and drop each Exited slot whose notice is set.

An `ensure` that reserved before quit began is covered by step 2. A Reserved slot still probing at the deadline has no child, so nothing is signalled, and its `ensure` refuses at lock-take and spawns nothing. A Starting slot whose spawn function returns after `stop_owned` has returned gets SIGTERM at its record; its `ensure` then runs the failure epilogue (after the pidfile write, quit has begun) while the app process lives, and a server that outlives the process keeps its pidfile for launch cleanup (Risks). An `ensure` that reserves after quit began refuses at reserve. An External listener has no slot and is never signalled.

### 5. Dashboard pane (desktop UI)

- **Tabs.** The header gets a tab group after Docs, one tab per dashboard of the app's business, from the `dashboards` IPC command. Each tab is an `.app-tab` with `data-view="dash"` and `data-dash="<name>"`. `openTab(view, pane)` (`apps/desktop/ui/app.js:124`) gains a `dash` argument: for the dash view it marks `aria-current` only on the tab whose `data-dash` matches, where the loop at `apps/desktop/ui/app.js:125-128` would mark every dashboard tab. dash.js binds click handlers on the tabs it creates, because the load-time binding at `apps/desktop/ui/app.js:144-146` does not reach them; a click calls `openTab("dash", "", name)`, then `openDash(name)`. Opening any other view calls `window.hideDash()`, which clears the frame's `src`.
- **Startup and restore.** `openFromHash` (`apps/desktop/ui/app.js:148-151`) becomes async and awaits `loadDashTabs()` for every hash (none, `today`, `agents`, `flows`, `docs` or `dash`) before it routes, so the tabs render on every launch and the hash's dashboard exists before it is selected. The startup chain stays `loadPaths().then(loadRepos).then(openFromHash)` (`apps/desktop/ui/app.js:1154`), so `ui.business` is set by `loadPaths` (`apps/desktop/ui/app.js:97`) first. A switch reloads the page and runs the same chain. A `dash` name the app business does not declare (including a business with no dashboards) opens Today. `switchTo(name)` (`apps/desktop/ui/app.js:159,166-168`), whose `name` is the business, writes the hash as `tab=dash&dash=<d>` for the dashboard view, with `<d>` the `encodeURIComponent` of the `data-dash` of the current dashboard tab (names contain spaces, such as "Control room"); `openFromHash` reads it back with `URLSearchParams`, which decodes it. Currently `switchTo` writes `tab` and `pane` only (`apps/desktop/ui/app.js:168`).
- **Invalid file.** A `dashboards` error shows its message in `#dash-notice`, above the dashboard tabs: the key path and rule for an invalid file, and `named()`'s error text for a stale business (item 9). `loadDashTabs()` catches the error, renders no dashboard tabs and resolves, so `openFromHash` still routes. A successful `dashboards` call clears `#dash-notice`, so the notice goes at the first load after the file is fixed (the `dashboards-changed` reload). dash.js ignores `dashboards-changed` until `openFromHash`'s first `loadDashTabs()` has resolved, so no reload runs before `loadPaths` sets `ui.business` (`apps/desktop/ui/app.js:97,1154`). The sidebar still works.
- **No registry business.** A window with no registry business (§1) gets an empty `dashboards` list and `UnknownDashboard` from `dashboard_open`, before any load, so a `[[business.explicit]]` or `[[business.default]]` entry is never served outside the registry. A registered business named `default` keeps its dashboards.
- **Fresh load.** `dashboards` and `dashboard_open` load `dashboards.toml` from its location (§3) on every call, as `businesses` reads the registry (`apps/desktop/src/lib.rs:420-423`). Nothing is cached, so an edit that `dashboards-changed` reports is seen by the next call of either command.
- **Opening.** `dashboard_open(business, name)` is async, because `ensure` can wait for a 3 s `CONNECT`, two 3 s probes, a 10 s readiness poll and a final identity probe. It checks the business and copies the registry in one acquisition of the App lock, so a switch cannot land between the check and the copy; calling the existing `App::get` under that lock would deadlock on the same mutex (`apps/desktop/src/lib.rs:239-244`). Its outcomes, in order: a business that is not the app's gives `StaleBusiness` with `named()`'s error text (`apps/desktop/src/lib.rs:264`); no registry business gives `UnknownDashboard`; then, on the blocking pool (`apps/desktop/src/lib.rs:275-282`), a load error gives `InvalidConfig` with the key path and rule, an undeclared name gives `UnknownDashboard`, and otherwise `ensure` decides; a join error gives `Internal`.
- The pane shows the state while `ensure` runs and sets the iframe `src` to the URL in Running, the URL `ensure` verified; dash.js uses no other URL. Each `openDash` first calls `hideDash()`, so no earlier dashboard's page stays under the newly selected tab, then takes a new sequence token and shows "starting…". A refusal card leaves `src` empty. A result is applied only while its token is the latest and the dash view still shows that dashboard; a result that arrives after another tab or another view is dropped, and its server keeps running until quit, as after a switch. On `dashboard-exited`, dash.js ignores an event whose `addr` differs from the host and port of the open dashboard's `url`, and an event that arrives while an `openDash` token is pending. Otherwise, when the frame shows a Running with `owned: true` for that addr, it calls `hideDash()` and renders the `exited` card with `status` and `log_tail`, so no dead page stays under the card. A reopen on the same addr returns Running only after the previous server's reaper has set its completion notice (ensure: owned), which follows that reaper's `emit` (§4 Reaper steps 2 and 5).
- **Reload.** On `dashboards-changed`, dash.js reloads the tabs. When the open dashboard is still declared with the same `name`, `url` and `managed` (the fields `dashboards` returns), the frame, the pending `openDash` and its token are kept, and a later edit of its `probe` takes effect at the next open. Otherwise (the dashboard is removed, renamed, or its `url` or `managed` changed), dash.js calls `hideDash()`, shows the idle state and invalidates the pending token, so an in-flight result is dropped; its server keeps running until quit.
- **Sandbox.** The iframe is `sandbox="allow-scripts allow-same-origin allow-forms"` with no `allow-top-navigation`. `allow-forms` stays only if the live gazette page submits a form; Build checks the page and drops the flag otherwise. The control room answers 405 to every method except GET and HEAD (`control-room/src/server.rs:56-60`). `allow-same-origin` is set because the control room's CSP sends `connect-src 'self'` (`control-room/src/server.rs:247`), which needs a real origin. The gazette's own CSP is not read yet; Q1 check 4 reads its frame headers, and Build records its CSP.
- **Switch.** The frame is destroyed on business switch, as the page reloads on switch today. The server keeps running until quit. After a switch the pane is idle: the hash's `dash` name selects a tab when the new business declares that dashboard and opens Today otherwise; no dashboard opens until the owner clicks.
- **CSP.** In `apps/desktop/tauri.conf.json:22`, `frame-src` becomes `cosreport: http://cosreport.localhost http://127.0.0.1:*`. Nothing else changes; `connect-src` stays `ipc: http://ipc.localhost`, because only the frames fetch the dashboards.
- **Origin rule.** The `cosreport` handler closure (`apps/desktop/src/lib.rs:963-974`) calls one public function, `cosreport_response(app, req)`, which calls `origin_allowed` first, answers 403 when it is false, and otherwise dispatches to `docs_response` or `report_response` as the closure does today. The `cosreport` closure is the only path the app's frames reach. Every POST in `apps/desktop/tests/ipc.rs` goes through `cosreport_response` (item 25); GET-only tests that call `report_response` or `docs_response` directly (`apps/desktop/tests/ipc.rs:290,299,358,680,691,695`) test the handlers, not the rule. The preview example (`apps/desktop/examples/preview.rs:153-169`) calls the inner handlers and does not apply the rule; it serves a demo home in a browser and is not the app's scheme. The rule:
    - GET is allowed with no Origin, with `Origin: null` (the opaque report frames), or with an app origin.
    - POST requires an Origin that is present and is one of `tauri://localhost`, `http://tauri.localhost`, `cosreport://localhost` or `http://cosreport.localhost`. `null` and an absent Origin are refused.
    - Every other method (`OPTIONS`, `PUT`, `DELETE` and the rest) follows the POST rule. No in-app frame POSTs today: the docs frame saves through the app (`src/docs.html:358,365`).
- `cosreport` serves the reports and accepts writes such as `/docs/organize`, scoped by the `x-cos-business` header (`apps/desktop/src/lib.rs:479-484`), which any page that knows the business name can set (`src/docs.html:366` sets it from the query string). The Origin rule keeps a dashboard frame from setting it.
- `apps/desktop/capabilities/default.json` stays `windows: ["main"]` with no remote URLs, so IPC from `http://127.0.0.1` frames is denied. Iframe embedding holds only if Q1 check 2 passes; otherwise the child webview fallback applies (Alternative E).

### 6. Sidebar and signals

**Rendering.** `ui/index.html` wraps the header and `<main>` in a column beside a new `<aside id="sidebar">`, styled with the `--sidebar*` tokens (`apps/desktop/ui/tokens.css:90-97`). Rows come from the `businesses` IPC command, in alphabetical order: the registry is a `BTreeMap` (`src/business.rs:29`), and `write` re-serialises it sorted (`src/business.rs:116-123`). The row whose `app` is true is highlighted, as ⌘K marks it (`apps/desktop/ui/command.js:28`); `app` is set from the App's business when the rows are built or emitted, never from the cache (Refresh). A click calls `switchTo(name)` (`apps/desktop/ui/app.js:159`). A refusal from `switchTo` (`apps/desktop/ui/app.js:163`) or from `switch_business` (`apps/desktop/src/lib.rs:329-351`) is shown in a status line below the rows until the next click. sidebar.js reads `pinned` from `invoke("paths")`, as `loadPaths` does (`apps/desktop/ui/app.js:96-100`); a pinned app disables the rows and titles them `pinned by <var>`, the text of the header's `#biz-pin` (`apps/desktop/ui/app.js:99-100`).

**One source.** The sidebar and ⌘K read the same `businesses` IPC command. Its rows (`apps/desktop/src/lib.rs:379-388`) gain `unread` and `scheduled`; the existing `counts` supply `live` and `lost`. The Agents-bar button `#other-live` (`apps/desktop/ui/index.html:180`), its update code (`apps/desktop/ui/app.js:1121-1135`), its call at `app.js:141`, its click listener (`app.js:1151`), the `businesses-loaded` listener (`app.js:1152`), the dispatch at `apps/desktop/ui/command.js:103` and its CSS (`apps/desktop/ui/style.css:51`) are removed, because each row shows the same counts. The header count `#agent-count` stays.

**Signals.** Computed by the new `cos::signals` module from file metadata and kernel checks, with no process spawn:

- **live, lost, starting:** `agents::count(home)` (`src/agents/mod.rs:561-592`) returns `live`, `lost` and `unreadable` today (`src/agents/mod.rs:553-558`), with Starting runs folded into `live` (`src/agents/mod.rs:586`). After item 3 it also returns `starting`, and `live` still includes Starting runs. Its `classify` calls `is_cos` (`src/agents/mod.rs:545`), which after this design reads the executable with `proc::exe(pid)` instead of spawning `ps` (`src/agents/mod.rs:794-807` today). A recycled pid that now runs another `cos` process (for example `cos dash`) still counts as live, because the check compares the file name only. The same check gates the SIGKILL at `src/agents/mod.rs:675`; binding the supervisor's start time is out of scope.
- **supervisors:** the supervisor pids of the runs `classify` puts in Live, from `agents::live_supervisors(home)` (item 3), which reads spec and state without `reconcile`, as `count` does. Lost runs, whose pid is already dead, are not in the set.
- **unread:** the newest `out/briefs/YYYY-MM-DD.json` by filename (other names are ignored), with its mtime in whole milliseconds since the Unix epoch (`cos::signals::mtime_ms`), compared with `lastSeenBrief[business]` in `<app config dir>/ui-state.json`. It is unread when its filename is greater than the stored one, or equal with a greater mtime.
    - A business with no stored pair, and any brief, is unread.
    - **Opened** means the Today view is visible and the report has rendered the brief. The iframe reloads on every `report-changed` whatever view is active (`apps/desktop/ui/app.js:1140-1141`), so `cos-report-ready` can arrive while another view is shown. The host calls `brief_seen` only when Today is visible at that moment and `ui.business` is set. It keeps the last ready payload in memory, and the switch to Today calls `brief_seen` with it. At launch `reloadReport()` starts beside the `loadPaths` chain (`apps/desktop/ui/app.js:1154-1155`), so a payload can arrive before `loadPaths` sets `ui.business` (`apps/desktop/ui/app.js:97`); that payload is kept and sent once `openFromHash` has routed, if Today is visible then.
    - `brief_seen(business, file, mtime)` stores the pair for the brief the report rendered. The report frame sends `brief` and `mtime` in its `cos-report-ready` message (`src/report.html:455` sends only `type` today). `store::latest` takes the mtime from the open handle before and after reading the bytes, and reads again when they differ, so the mtime describes the rendered content (item 17a); briefs are rewritten in place (`src/store.rs:32,54`). The host handler at `apps/desktop/ui/app.js:193-195` passes both on. The host never stats the brief to obtain the stored value, so a brief rewritten after render keeps the mtime of the content the owner saw.
    - The report data carries `briefRef{file, mtime}`. The template posts `brief` and `mtime` only when the data also has `hostBrief: true`, which `report_response` sets for `/` only (`apps/desktop/src/lib.rs:451`); `report::build` (`src/report.rs:24-31`) and the browser `cos dash` leave it unset, and archived reports (`/reports/<day>.html`, `src/dash.rs:156-166`) send no `brief`. For a message without `brief`, the host neither calls `brief_seen` nor keeps the payload; it still sends the `cos-host {hire: true}` reply it sends today (`apps/desktop/ui/app.js:193-194`).
    - `brief_seen` never lowers the stored pair: it keeps the greater of the stored and the sent `(filename, mtime)`.
    - After the write, `brief_seen` publishes that business to the shared row cache, whose publish function rebuilds its row (Refresh), which emits `businesses-changed` with all rows, so the dot clears at once and the watcher's next emit does not restore it.
    - The read-modify-write runs under an exclusive lock on `<ui_state_file>.lock`, taken with `cos::agents::slots::lock_exclusive`, the helper `business::update` uses (`src/business.rs:108,125-130`); item 3a makes it public. Before taking the lock, the parent directory is created: nothing in the desktop crate creates the app config dir (`apps/desktop/src/lib.rs:54` only joins a path under it), and `atomic_write_bytes` does not create the parent (`src/state.rs:556-569`). The file is written with `atomic_write_bytes` (`src/state.rs:555`), after the business check under the App lock.
    - `brief_seen` refuses a window with no registry business (§1) before `VERIFY(brief_seen)`, as `dashboard_open` does (§5): the defaults layer is labelled `default`, the name of a registered business, so the label check alone would store another home's brief under that business.
    - `VERIFY(brief_seen)` refuses an `mtime` greater than the brief file's current `mtime_ms`. The metadata is a check only, never the stored value. A rewrite with `std::fs::write` (`src/store.rs:32,54`) sets the mtime to the time of the write, so a rendered mtime exceeds the current one only if the mtime was set back by hand (the brief then reads as unread until its next rewrite), and a forged far-future `mtime` cannot pin the stored pair above every later brief of the same name.
    - An unparseable `ui-state.json` reads as empty. Entries for unregistered names are dropped on write.
- **scheduled:** the `cos.brief` plist at `<Start.user_home>/Library/LaunchAgents/cos.brief.plist`; a `Start` with no `user_home` gives no clock. The row builder and the watcher take this path from `Start`, never from the process environment, so tests stay hermetic. The plist is read with the `plist` crate, a direct dependency already in the graph through tauri (`Cargo.lock:2437-2438`), which reads binary plists. The business is the string after `--business` in `ProgramArguments`. It counts only when `EnvironmentVariables.COS_BUSINESSES` resolves to the same registry as the app's. The app side resolves its canonicalised registry. The plist side resolves through `business::location_from` (`src/business.rs:49`) with `COS_BUSINESSES`, `XDG_CONFIG_HOME` and `HOME` from the plist's `EnvironmentVariables`; for a key the plist lacks it uses what launchd gives the job: no `XDG_CONFIG_HOME` (`Taskfile.yml:200`), and `HOME` from `Start.user_home`. The rendered plist always sets `COS_BUSINESSES` (`launchd/cos.brief.plist:16`), so the fallback applies only to a hand-edited plist. `location_from` fails with `HOME is unset` (`src/business.rs:61`) when none of the three is set. Both paths are canonicalised before comparison.
    - A plist with no `--business` (a legacy plist) marks no business.
    - A parse error marks none and logs once.
    - The plist being on disk does not prove the job is loaded, so the tooltip says "installed for <name>".

**Refresh.** The watcher thread (`apps/desktop/src/lib.rs:891-909`) keeps its 700 ms tick.

- Each tick takes one stamp per business: `cos::dash::stamp([agents::root(home), home/out/briefs])` (`src/dash.rs:181-196`, metadata only), plus the plist's mtime, which is part of every business's stamp, so a plist change recomputes every registered row. `dash::stamp` reads directories only, so `businesses.toml` and `dashboards.toml` each take their own mtime from one metadata call.
- Each tick calls `proc::alive(pid)` (one `kill(0)`) for every pid in the business's supervisor set from its last recompute.
- A business is recomputed when its stamp changed, one of its supervisors died (the recompute rebuilds the set, which then no longer holds the pid of a run that turned Lost), or it has `starting > 0`, since a run turns Lost when `START_GRACE` passes with no file change or pid to watch. A changed `businesses.toml` recomputes every registered business's row and replaces the whole cache (Shared row cache). A changed `dashboards.toml` emits `dashboards-changed`, which dash.js handles by calling `loadDashTabs()` (§5 Invalid file). The first tick records the `businesses.toml` and `dashboards.toml` mtimes without counting them as changes, so it emits no `dashboards-changed` at launch; the existing stamps start empty and emit on the first tick (`apps/desktop/src/lib.rs:892-899`), and the new ones do not copy that.
- **Events.** `dashboards-changed` carries no payload. `businesses-changed` carries the full list of registered businesses' rows, in the shape the `businesses` command returns (with `unread` and `scheduled`).
- **Shared row cache.** Managed state holds the rows last emitted, without `app`. The watcher keeps only its stamps and supervisor sets. The watcher and `brief_seen` publish through one function, which they give the names of the businesses to rebuild, never a row built earlier. The function builds those rows itself, while it holds the cache, so a row built before `brief_seen`'s write can never be published after `brief_seen`'s cleared row. When the cache does not yet hold every registered business, the function first fills it with every row from the row builder. It then replaces the cached row of each business it is given. A changed `businesses.toml` replaces the whole cache with the row builder's full output, so a business removed from the registry is dropped and is absent from the next `businesses-changed` payload. It then, when any cached row changed, emits `businesses-changed` with all cached rows while still holding the cache, setting each emitted row's `app` from the App's business at that moment. A switch changes no stamp and no cached row, so the next emit after it marks the new business. So `brief_seen`'s cleared dot is in the cache the watcher's next emit sends, and a new brief after `brief_seen` differs from the cached row and is emitted on the next tick. The existing `report-changed` and `agents-changed` emits stay (`apps/desktop/src/lib.rs:891-905`).
- **Row builder.** One synchronous row builder, which spawns no process, returns every registered business's row or one named row. The `businesses` command calls it on the blocking pool; the publish function calls it for the watcher and for `brief_seen`'s one business.
- Cost per tick is one `read_dir` per directory, one metadata call per run entry (`src/dash.rs:184-186`), and one `kill(0)` per live run. Full re-reads happen only on change.
- The initial rows come from the `businesses` command, which does not touch the cache. The watcher fills the cache on its first tick, or `brief_seen`'s publish fills it first when it runs earlier. sidebar.js registers its `businesses-changed` listener before it invokes `businesses`, and drops the `businesses` result when any `businesses-changed` payload has already been applied, so an initial result computed before a `brief_seen` write cannot bring a cleared dot back.

### 7. `task schedule` and port 8790

`cos.dash` defaults to `127.0.0.1:8790` (`src/main.rs:60`), the port the Longhand gazette holds. `schedule` (`Taskfile.yml:211-222`) renders, moves, boots out and bootstraps both items in four `for: [brief, dash]` loops today, so the move at `Taskfile.yml:216-217` installs `cos.dash.plist` into `{{.LAUNCHD}}` (`{{.HOME}}/Library/LaunchAgents`, `Taskfile.yml:5`) before any check, and launchd loads every plist in that directory at login. After this design the four loops iterate `brief` only, and `cos.dash` is moved into place only by step 6 below. The `cos.brief` steps run first, then `cos.dash` is handled in this order:

1. Render `cos.dash` to `{{.LAUNCHD}}/cos.dash.plist.new` without moving it (S9d). The shell redirect creates `.new` before the render can fail, so a failed render runs `TOOL_CALL(remove_dash_plist)` on `.new` only, in the same step, and then fails the task, leaving `cos.brief` installed and loaded and the installed `cos.dash.plist` unchanged; the render-before-replace invariant in the comment at `Taskfile.yml:210` now covers `cos.brief` only, and the comment says so.
2. Read the old pid from the `pid = N` line of `{{.LAUNCHCTL}} print gui/$(id -u)/cos.dash 2>/dev/null || true`, through the same `{{.LAUNCHCTL}}` seam as every other launchctl call (`Taskfile.yml:220,222`). `launchctl print` exits non-zero for a job that is not loaded; the old pid is then empty, and step 4 is skipped.
3. `TOOL_CALL(bootout_dash)`, admitted by `VERIFY(dash_target)` (S9a), as `{{.LAUNCHCTL}} bootout gui/$(id -u)/cos.dash 2>/dev/null || true`: a failed bootout of a job that is not loaded is ignored, as at `Taskfile.yml:220` today.
4. Poll every 250 ms for at most 10 s until the old pid has exited. If it is still alive, `VERIFY(dash_port_held)` returns `Timeout`: the task fails with a message, `TOOL_CALL(remove_dash_plist)` removes `.new` only, and the installed plist is kept. `cos.dash` stays unloaded until the next login or the next `task schedule` run.
5. Run `lsof -nP -iTCP:{{.DASH_PORT}} -sTCP:LISTEN`. `DASH_PORT` is a Taskfile var with default 8790, a test-only seam that changes only this check; the rendered plist runs `cos dash` on the default 8790 (`launchd/cos.dash.plist:6-13`). Tests pass it as the CLI arg `DASH_PORT=<port>`, as `run_task` passes `LAUNCHCTL` (`tests/schedule.rs:134`), set to a free ephemeral port. `lsof` exits non-zero when nothing listens; the step does not fail on that exit, and a non-zero exit with empty output means no listener. `VERIFY(dash_port_held)` returns `Free` when no listener is listed, and `Held{pid, command}` when any listed LISTEN line has a pid other than the old pid; `Held` reports the first such line's pid and command, so a listener with both an IPv4 and an IPv6 socket, or several listening pids, gives one `Held`.
6. `Free`: move `.new` over the installed plist and `TOOL_CALL(bootstrap_dash)`, as today (S9c).
7. `Held`: print the listener's command and pid, state that `cos.dash` is skipped, and `TOOL_CALL(remove_dash_plist)` on `.new` and `{{.LAUNCHD}}/cos.dash.plist` (S9b). The plist has `RunAtLoad` and `KeepAlive` (`launchd/cos.dash.plist:19-20`), so launchd would load it at the next login. Running `task schedule` again after the holder stops restores the plist.

`cos.brief` loads in either case. While the gazette holds the port, `Held` is the steady state (Goal 9).

### 8. Threat model

- Whoever can write the declared binary, or `dashboards.toml`, can run code as the owner the next time a dashboard opens. The design accepts this for a binary the owner builds in their own repo. The declared binary sits inside the `founding-bets` home, workspace and projects (`~/Projects/founder-mode`), so an agent with write access to that workspace can replace it. `dashboards.toml` is in the owner's config directory, and agents with file access could write it.
- The binary check is: absolute path, regular file, executable bit set. Those checks do not stop an agent that can write the binary.
- Alternative C's rejection is worded for this difference: `profile.json` is the business's own record, read by agents and reports, and mixing launcher configuration into it couples two owners.
- Loopback is not a trust boundary, because any local process can answer on a port. A page marker proves what the page says. Ownership is proven by the kernel's view of the pid: start time, executable and process group.
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
*   **Reason for Rejection**: Rejected (owner decision D2, [brief §Decisions](2026-11-13-business-dashboards-brief.md#proposed-answers-to-the-remaining-decisions-for-the-design-to-confirm)): a per-dashboard page marker covers both servers with no change outside this repo. Ownership is proven by the kernel's view of the pid. The `/health` body is used only as the readiness check (`control-room/src/server.rs:185-189`).

### Alternative H: one guard process per server (shim)

*   **Pros**: the server dies with the app even after `kill -9`, through a pipe the shim watches.
*   **Cons**: a second long-lived process per server, and a new `cos` subcommand to supervise it.
*   **Reason for Rejection**: Rejected (owner decision D4, [brief §Decisions](2026-11-13-business-dashboards-brief.md#proposed-answers-to-the-remaining-decisions-for-the-design-to-confirm)): a pidfile with launch cleanup stops a leftover server with less machinery. The server keeps running between a crash and the next launch; this gap stays open (see NFR Security).

## Risks & Mitigations

| Risk | Impact | Mitigation |
| --- | --- | --- |
| The server is unsupervised between a crash and the next launch. | The port stays held and the process keeps serving loopback until the next launch or a reboot. | Launch cleanup stops it (CUJ 7). The gap is stated here and is not closed (Alternative H). |
| A spawn function returns after `stop_owned` has returned (§4 Stop path). | The child gets SIGTERM at its spawn record, but nothing waits on it once the app process exits; a child that ignores SIGTERM survives the app. | Its `ensure` runs the failure epilogue while the process lives. A survivor whose pidfile was written is stopped by the next launch's cleanup; one without a pidfile falls under the next row. |
| The app crashes between the spawn and the atomic pidfile write. | The server runs with no pidfile, so launch cleanup cannot find it. | None. The port stays held until the process exits or a reboot. |
| SIGTERM to the app, or a logout path that sends it, skips `RunEvent::Exit`. | Owned servers survive the app. | The next launch's cleanup stops them. The exit-path row in [Confidence](#confidence) stays sand until the E2E CUJ 4 step proves the ⌘Q path. |
| PID or process group reuse between the check and the signal. | An unrelated process group receives SIGTERM or SIGKILL. | Launch cleanup runs its five checks again immediately before the SIGTERM and a deadline SIGKILL. Slot signals reach only a group whose leader is not reaped. A residual window between the last check and the signal remains. |
| A process in an owned server's group outlives the reaped leader. | It keeps the port, or keeps running with no owner. | The reaper checks `proc::group_alive(pgid)` after `waitpid` and sends SIGKILL to the group (§4 Reaper); the stop path polls until the group is gone within its 5 s + 1 s budget, including groups whose slot was already Exited when quit began. Residual check-then-kill window: if every member exits between the `group_alive` check and the `kill(-pgid, SIGKILL)`, the pgid is no longer reserved (`src/proc.rs:234`), and a new group that takes the same pgid in that window receives the SIGKILL. The window is not closed. |
| Check-then-kill window of launch cleanup's group SIGKILL after a leftover leader exits. | An unrelated process group receives SIGKILL. If every member exits between the `group_alive` check and the kill, the pgid is free (`src/proc.rs:234`). launchd reaps the leftover leader, so the window also spans the interval from that reap to the `group_alive` check (at most one 50 ms poll): when the group has no other member, the pgid is free from the reap on, and a new process whose pid equals the pgid and that leads its own group in that interval passes `group_alive`. | The kill is sent only when the leader passed all five checks at SIGTERM time, while cleanup holds `<port>.lock`, before the 5 s deadline, at most once per leftover (`VERIFY(leftover_group)`). The window is not closed. |
| A leftover's server is removed from `dashboards.toml`, its port changes, its `binary` path changes, or the binary is rebuilt in place (`cargo build --release` at the same path, Setup Runbook), before the next launch. A rebuild replaces the file the running process was started from, so `proc::exe(pid)` or `canonicalize` can fail or differ, which counts as a mismatch (§4 Launch cleanup step 3). | Launch cleanup removes the pidfile and does not stop the process, which keeps the port until it exits or a reboot. | Accepted (owner decision, 2026-11-14): cleanup signals only an executable that is a declared server's binary for that port. The next open probes the port; a listener that passes the identity probe is embedded as External, and one that fails it is refused with the `lsof` next step (CUJ 5). |
| Q1 fails. | Embedding does not hold. | Check 1 fails: dashboards open in the browser (Alternative F). Check 2 fails: child webview (Alternative E). Check 4 fails: the gazette opens in the browser (Alternative F) and the control room stays embedded. |
| Q1 check 3 shows `null` on a dashboard-frame POST. | The POST is refused. | `origin_allowed` refuses `null` (§5). The spike records the header; the rule does not change. |
| Probe markers drift in the real pages. | The identity probe refuses a healthy dashboard. | Markers are read from the live pages in Build. A refusal shows what answered, so the drift is visible; after a fresh spawn the refusal is marked `spawned` and its card names the dashboard's `probe` first (§Violations). Markers must be at least 12 characters and must not match `^<[^<>]*>$` (§3). |
| A foreign listener binds the port after ensure: probe found it free and before the child binds. | The generic `ready` marker passes on the foreign listener. | `ensure` runs the dashboard's identity probe after readiness (§4 ensure: start); a body without the marker is refused with `identity` marked `spawned` and the child is stopped. Residual: a foreign listener that carries the marker is embedded under the dashboard's name until the child, which cannot bind, exits and the pane shows `failed to start` or `exited`. While the slot is Owned, every other dashboard on it is embedded only after its own identity probe passes (§4 ensure: owned). Binding the listener to the child's pid is out of scope. |
| The `http://127.0.0.1:*` frame source. | Any local listener can be framed once its probe passes. | Embedding follows the probe. Frames have no IPC (capability file, §5). The Origin rule covers the report scheme. |
| A dead supervisor's pid is reused by a process that is not `cos` before the next tick. | `kill(0)` reports it alive, so the tick does not recompute the row, which stays live until a stamp of that business changes. | Accepted. The tick checks liveness only; the next recompute classifies through `is_cos` (`proc::exe` file name, item 3) and turns the run Lost. |
| `kill(0)` reports a zombie as alive (`src/proc.rs:246`). | A supervisor that its parent has not reaped keeps its run live, and the tick does not recompute the row. | Accepted. `agents::spawn` reaps the supervisor on a thread in the spawning process (`src/agents/mod.rs:482-485`), and a supervisor whose parent exits is reparented to launchd, which reaps it, so a zombie lasts only until that wait returns. The item 25 CUJ 8 test reaps the supervisor it kills, as `agents::spawn` does. |
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
| S1 | Spawn a server process from a binary named in `dashboards.toml` | `ensure` | yes: stop the group | the owner: the binary runs as them (§8) |
| S2 | Signal a process group, or a leader alone (SIGTERM, SIGKILL), through `proc::signal_group`, `proc::signal_pid` (a leader stop target, in the failure epilogue and in `stop_owned`) and `proc::kill_group` (the group kill after a reap, and launch cleanup's group kill after a leftover leader exits) | `stop_owned`, launch cleanup, failure epilogue, the reaper | no | any process whose process group is reused or wrongly adopted |
| S3 | Write and remove a pidfile; create and lock a port lock file, never unlinked | `<app data dir>/servers/<port>.json` and `<port>.lock`, and the `servers/` directory created by the supervisor | yes | the app's own recovery |
| S4 | Append a server log, truncated to 1 MiB at spawn | `<app log dir>/servers/<port>.log`, and the `<app log dir>/servers` directory created by the supervisor | yes | disk |
| S5 | TCP connect to a loopback port (`CONNECT`, ensure: probe), and HTTP GET to a loopback port (probe, ready) with a request path validated at parse time (§3) | supervisor (`connect`, `real_connect`), `cos::dashboards::probe` (`cos business dashboards`) | yes, read only | the local service probed; the control room serves one connection at a time (`control-room/src/server.rs:14-22`), so a connect or a GET can delay its other clients |
| S6 | Load a loopback URL in an iframe | `ui/dash.js` | yes | the owner, if a foreign page is shown under a dashboard's name |
| S7 | Write `ui-state.json` (`lastSeenBrief`), and create its parent directory and its lock file `<ui_state_file>.lock` | `brief_seen` | yes | the unread dot only |
| S8 | `cos business add` warning for nested homes | `business.rs` | n/a, output only | none |
| S9a | `task schedule` boots out the loaded `cos.dash` job | `Taskfile.yml` (schedule) | partly: `launchctl bootstrap` restores it; after a Timeout run nothing does, so it stays unloaded until login or a rerun | the owner: the report server on 8790 stops |
| S9b | `task schedule` deletes `.new` and the installed `cos.dash.plist` when 8790 is held | `Taskfile.yml` (schedule) | no: only a rerun of `task schedule` restores the plist | the owner: `cos.dash` no longer starts at login |
| S9c | `task schedule` moves `.new` into place and bootstraps `cos.dash` when 8790 is free | `Taskfile.yml` (schedule) | yes: bootout | the owner: the report server starts |
| S9d | `task schedule` renders `cos.dash.plist.new` | `Taskfile.yml` (schedule) | yes: `.new` is removed, or moved into place, on every outcome | none |
| S10 | Refuse a POST to `cosreport` without an allowed Origin | `lib.rs` scheme handler | yes | a frame that tried to reach the report scheme |

The app makes no non-loopback request; embedded pages keep their own network behaviour. Nothing new is sent to a model.

### Instructions

| Instruction | Core | Governance property | Sink? | Binding (closed schemas) |
| --- | --- | --- | --- | --- |
| `LOAD(dashboards.toml)` | Memory | deterministic parse + validation | no | in: path; out: servers `{binary: abs path, addr: 127.0.0.1:port (1 to 65535), ready: probe}` by name, and per business a list of dashboards `{name, url: loopback with port 1 to 65535, server?, probe}`, where a probe is `{path: ^/[!-~]*$, contains: marker}` |
| `CONNECT(addr)` | Execution | read-only TCP connect, no bytes sent, 3 s connect timeout (a timeout reports `answered: false`); the injected `connect` of `Supervisor::new`, `real_connect` in the app (item 8) | no: S5 sends nothing, and `addr` is a validated `127.0.0.1:<port>` | in: `{addr: 127.0.0.1:port}`; out: `answered: bool` (tainted: any local process can accept) |
| `PROBE(addr, probe)` | Execution | read-only HTTP GET, 3 s timeout per attempt, reading at most 64 KiB. A complete response is a parsed status line followed by EOF, by a body that satisfies `Content-Length`, or by reaching the 64 KiB cap; a marker past the first 64 KiB is reported as `matched: false`. Outcomes: `no_listener` (the connect fails), `no_response` (connected, but a close or the timeout comes before a complete response, including a close with no status line), `response`. Identity retry rule: an identity probe (`VERIFY(identity)`) is retried once only when its first attempt gives `no_listener` or `no_response`; a `response` without the marker refuses at once. Readiness polls and `cos business dashboards` make one attempt | no: S5 is a read-only GET to loopback, and its one tainted input, `path`, is admitted by `LOAD(dashboards.toml)` only when it matches `^/[!-~]*$` (§3), so it cannot add a header or a second request; the only headers are `Host: 127.0.0.1:<port>` and `Connection: close`, built from the validated port | in: `{addr: 127.0.0.1:port, path, contains}`; out: `no_listener \| no_response \| response{status, head: ≤200 bytes, matched}` |
| `VERIFY(binary_admissible)` | Normative | deterministic checkpoint | no | in: server; out: admit, `BinaryMissing{path}` for a missing file, or `NotAdmissible{rule, path}` for any other failure, including rule `platform` when `proc::start_time` is unavailable |
| `VERIFY(ownership)` | Normative | deterministic checkpoint | no | in: `{slot?, pidfile?, lock, declared: port → binary}`, `declared` built from the loaded servers; out: `Owned \| Leftover \| Starting \| Stale \| Held`. Leftover requires the four kernel checks (alive, start time, executable, live pgid equal to the pid and to the record's `pgid`), `canonicalize(exe) == canonicalize(declared[port])`, and the port lock taken by launch cleanup (step 1); its signals target `-pid`, never the record's `pgid`. A pidfile whose port has no declared server, that does not parse, or whose record `addr` port differs from its filename port is Stale. Starting requires a recorded spawn, the slot not marked Exited, and the lock held by this instance, and admits its stop target (group or leader alone). Owned always carries a group target |
| `VERIFY(identity)` | Normative | deterministic checkpoint over `PROBE` output | no | in: `{probe result, marker, owned, spawned}`; out: admit (a `response` with `matched`) or `Identity{status, head, owned, spawned}`, where `owned` is true only for an Owned slot reused at ensure: owned, and `spawned` is true only for the identity probe after readiness of a server this `ensure` spawned; both are false at ensure: probe. When the last attempt gave `no_listener` or `no_response`, `status` is `no listener` or `no response` and `head` is empty |
| `TOOL_CALL(spawn_server)` | Execution | process spawn | **yes** (S1) | in: `{binary, addr, env, log}`, env inherited from the app, argv fixed `[binary, serve, addr]`, working directory the binary's parent, `log` from `VERIFY(log_path)`; out: the child, from which `{pid, pgid, start_time, exe}` are read (ensure: start step 1) |
| `VERIFY(readiness)` | Normative | deterministic checkpoint over `PROBE` output and the child's exit state | no | in: `{probe_1, probe_2, exited_before_1, exited_after_2}`, where exited means the reaper marked the slot Exited; out: admit or `FailedToStart{log_tail}` |
| `VERIFY(log_path)` | Normative | deterministic checkpoint, run in ensure: start before the spawn | no | in: `{addr}` of a validated server; out: `<app log dir>/servers/<port>.log`; always admits, because the port is a parsed integer |
| `STORE(server_log)` | Memory | truncate to the last 1 MiB before the spawn function runs, then append from the child | **yes** (S4) | in: `{path}` from `VERIFY(log_path)`; out: unit; the `<app log dir>/servers` directory is at a fixed path, which no input names, so it needs no VERIFY |
| `VERIFY(brief_seen)` | Normative | deterministic checkpoint | no | in: `{business, file, mtime}`; out: admit or refusal `{rule: brief_seen}`; admit only when the app's triple has a registry business and it is `business` (checked under the App lock; a window with no registry business is refused before this checkpoint, §6), `file` matches `^\d{4}-\d{2}-\d{2}\.json$` and exists under `out/briefs`, and `mtime` is an integer of at least 0 in ms since the Unix epoch and not greater than the file's current `mtime_ms` (both file checks are checks only, never value sources) |
| `VERIFY(group_after_reap)` | Normative | deterministic checkpoint | no | in: `{target, reaped}`; out: admit or skip; admit only when the target is a group, this instance's reaper has just reaped the leader whose pid equals the pgid, and `proc::group_alive(pgid)` is true; a leader target is always skipped; admits KILL only |
| `VERIFY(leftover_group)` | Normative | deterministic checkpoint | no | in: `{pgid, passed_at_sigterm, leader_failed_checks, lock_held, before_deadline}`; out: admit or skip; admit only when the leader of `pgid` passed all five checks at the SIGTERM and then failed them, cleanup holds `<port>.lock`, the 5 s deadline has not passed, and `proc::group_alive(pgid)` is true immediately before the kill; admits KILL only, once per leftover |
| `TOOL_CALL(signal_group)` | Execution | deterministic action | **yes** (S2) | in: `{target: group(pgid) \| leader(pid), signal: TERM \| KILL}`; out: unit; a group uses `proc::signal_group`, or `proc::kill_group` for the reaper's KILL and launch cleanup's leftover-group KILL; a leader uses `proc::signal_pid` |
| `STORE(pidfile)` | Memory | atomic write, or removal | **yes** (S3) | in: `{pid, pgid, start_time, exe, addr}` for a write (admitted by `VERIFY(spawn_result)`), or `{path}` for a removal (admitted by `VERIFY(pidfile_path)`); out: unit; the `servers/` directory is at the fixed path `<app data dir>/servers`, which no input names, so it needs no VERIFY |
| `STORE(port_lock)` | Memory | open and flock `<port>.lock`, never unlink | **yes** (S3) | in: `{path}` admitted by `VERIFY(pidfile_path)`; out: unit; same fixed directory as `STORE(pidfile)` |
| `RENDER(iframe src)` | Memory | UI action | **yes** (S6) | in: `{url: loopback}`; out: unit |
| `STORE(ui-state)` | Memory | atomic write | **yes** (S7) | in: `{business: registered name, file: YYYY-MM-DD.json, mtime: ms since the Unix epoch}`; out: unit; the parent directory and `<ui_state_file>.lock` are at a fixed path set at start, which no input names, so they need no VERIFY |
| `VERIFY(origin_allowed)` | Normative | deterministic checkpoint | no | in: `{method, Origin?}`; out: admit or 403 |
| `VERIFY(spawn_result)` | Normative | deterministic checkpoint | no | in: `{pid, pgid?, start_time, exe}` read from the child of `TOOL_CALL(spawn_server)`, and `addr` from the validated server; out: admit or `FailedToStart{log_tail}`, with the failing field logged; admit only when pid > 0, pgid equals pid (a group stop target), exe non-empty, start_time present, and the record's addr taken from the validated server, never from the child |
| `VERIFY(pidfile_path)` | Normative | deterministic checkpoint | no | in: `{addr, file}`; out: admit or refusal `{rule: pidfile_path}`; admit only when `file` is `<port>.json` or `<port>.lock` for the port of a validated addr, or, in launch cleanup, for a `servers/<port>.json` entry whose `<port>` is the canonical decimal form of a port from 1 to 65535 (no leading zero, sign, separator or `..`); launch cleanup then compares the record's `addr` port with that port (`VERIFY(ownership)`); a `.lock` path is opened and locked, never unlinked |
| `VERIFY(dash_target)` | Normative | deterministic checkpoint | no | in: `{label, plist}`; out: admit or refusal `{rule: dash_target}`; admit only when label is `cos.dash` and plist is `{{.LAUNCHD}}/cos.dash.plist` or `{{.LAUNCHD}}/cos.dash.plist.new`, exactly, where `{{.LAUNCHD}}` is the Taskfile variable (`{{.HOME}}/Library/LaunchAgents`, `Taskfile.yml:5`) |
| `VERIFY(dash_port_held)` | Normative | deterministic checkpoint over `lsof` and `launchctl` output | no | in: `{old_pid?, listener?: {pid, command}, old_pid_alive}`, where `old_pid_alive` is read after the 10 s wait; out: `Free \| Held{pid, command} \| Timeout` |
| `TOOL_CALL(bootout_dash)` | Execution | launchctl bootout | **yes** (S9a) | in: `{label, plist}`, admitted by `VERIFY(dash_target)`; out: unit |
| `TOOL_CALL(bootstrap_dash)` | Execution | launchctl bootstrap | **yes** (S9c) | in: `{plist}`, admitted by `VERIFY(dash_port_held)` = Free and `VERIFY(dash_target)`; out: unit |
| `TOOL_CALL(remove_dash_plist)` | Execution | file removal | **yes** (S9b) | in: `{paths}`: `[<.new>, <installed plist>]` when `VERIFY(dash_port_held)` = Held, `[<.new>]` when it is Timeout or when the `cos.dash` render failed (§7 step 1); admitted also by `VERIFY(dash_target)`; out: unit |
| `STORE(render_dash_plist)` | Memory | write `cos.dash.plist.new` only | **yes** (S9d) | in: `{path}`, admitted by `VERIFY(dash_target)`; out: unit |
| `TOOL_CALL(install_dash_plist)` | Execution | move `.new` over the installed plist | **yes** (S9c) | in: `{from, to}`, admitted by `VERIFY(dash_port_held)` = Free and by `VERIFY(dash_target)` for both paths; out: unit |

`RENDER` sits in the Memory core, as the `semantic-isa` skill's table places it.

### Taint

- **Sources:**
    - `dashboards.toml`, which is hand-edited, and which agents with file access could write;
    - the pidfile, which is on disk and could be stale or forged;
    - every `CONNECT` result and every HTTP response from a loopback port, since any local process can answer;
    - the launchd plist;
    - `lsof` and `launchctl` output in `task schedule` (the listener's pid and command, and the old pid);
    - `ui-state.json`;
    - request `Origin` headers;
    - the brief filename and mtime that the report frame sends in `cos-report-ready`.
- **Propagation:** a `PROBE` result is tainted. A pidfile read is tainted until `VERIFY(ownership)` checks it against the kernel's view of the pid (start time, executable and process group) and against the `binary` that `dashboards.toml` declares for its port.
- **Sinks:** `spawn_server`, `signal_group`, `STORE(pidfile)`, `STORE(port_lock)`, `STORE(server_log)`, `RENDER(iframe src)`, `STORE(ui-state)`, `STORE(render_dash_plist)`, `bootout_dash`, `bootstrap_dash`, `install_dash_plist`, `remove_dash_plist`.

### VERIFY before every sink

| Sink | Cleared by | Check |
| --- | --- | --- |
| `TOOL_CALL(spawn_server)` | `VERIFY(binary_admissible)`, `CONNECT` returned `answered: false` (ensure: probe), and the port lock taken by this instance (ensure: start) | path absolute, exists, regular file, executable bit set, platform supports `start_time`; addr `127.0.0.1:<port>`; argv fixed |
| `TOOL_CALL(signal_group)` | `VERIFY(ownership)` = Owned, Starting or Leftover; for the reaper's SIGKILL after the reap, `VERIFY(group_after_reap)`; for launch cleanup's SIGKILL after a leftover leader exits, `VERIFY(leftover_group)` | Owned: the slot is not marked Exited by its reaper, which marks it before it reaps. Starting: a spawn is recorded, the slot is not marked Exited, and the lock is held by this instance; the target is the slot's stop target, the group for a group target and the unreaped leader alone for a leader target, in the failure epilogue and the stop path alike. Leftover: pid alive, start time equal, executable equal, live process group equal to the pid and to the record's `pgid`, executable equal to the declared `binary` for the pidfile's port (both canonicalised), and the port lock taken by launch cleanup (step 1); the target is `-pid`; the five checks run again immediately before the signal. After the reap: the reaper reaped the leader of `pgid` and `group_alive(pgid)` is true. After a leftover leader exits: the leader passed the five checks at the SIGTERM, cleanup holds the lock, the deadline has not passed, and `group_alive(pgid)` is true. Both check-then-kill windows are in Risks. Held and Stale never reach this sink |
| `STORE(pidfile)` write | `VERIFY(spawn_result)` | the record is built from the spawn result, never copied from an earlier file |
| `STORE(pidfile)` removal | `VERIFY(ownership)` = Stale; Leftover once its leader fails the five checks after SIGTERM (after the group SIGKILL when a member remained), or after the deadline SIGKILL; or Owned or Starting of this instance after its reaper marked it Exited; and `VERIFY(pidfile_path)` | the file is `<app data dir>/servers/<port>.json` for the validated port |
| `STORE(port_lock)` open | `VERIFY(pidfile_path)` | the file is `<app data dir>/servers/<port>.lock` for the validated port; it is never unlinked |
| `STORE(server_log)` | `VERIFY(log_path)` | the path is `<app log dir>/servers/<port>.log`, built only from the port of a validated `addr`, so the name has no path separator or `..` |
| `RENDER(iframe src)` | `VERIFY(identity)` with the requesting dashboard's own probe, in the same `ensure` call that returns Running: for an External server at ensure: probe; for a server this call spawned, after `VERIFY(readiness)` (the `ready` marker is generic, so readiness alone does not tie the listener to the dashboard); for an Owned slot reused at ensure: owned, including by a waiter, before Running is returned | the URL is one declared in `dashboards.toml` for the app's business (validated loopback), found by `dashboard_open` in a fresh load (an undeclared name is `UnknownDashboard`), and `ensure` returned Running |
| `STORE(ui-state)` | the no-registry-business refusal (§6), then `VERIFY(brief_seen)`, which includes the App-lock business check and the registered-name check | the app's triple has a registry business, and the business named is that business; the filename matches `^\d{4}-\d{2}-\d{2}\.json$` and exists under that business's `out/briefs`; `mtime` is not greater than the file's current `mtime_ms` |
| `TOOL_CALL(bootout_dash)` | `VERIFY(dash_target)` | label `cos.dash` and the exact plist path only |
| `TOOL_CALL(bootstrap_dash)` | `VERIFY(dash_port_held)` = Free, and `VERIFY(dash_target)` | runs only after the old pid is gone and no listener holds the port |
| `TOOL_CALL(remove_dash_plist)` | `VERIFY(dash_port_held)` = Held (`.new` and the installed plist) or Timeout (`.new` only), or a failed `cos.dash` render (`.new` only), and `VERIFY(dash_target)` | the `.new` file and the installed plist only; never a glob; on Timeout and on a failed render the installed plist is kept |
| `STORE(render_dash_plist)` | `VERIFY(dash_target)` | the `.new` path only; the installed plist is not written |
| `TOOL_CALL(install_dash_plist)` | `VERIFY(dash_port_held)` = Free, and `VERIFY(dash_target)` for both paths | `.new` is moved over the installed plist only after the old pid is gone and the port is Free |

### Violations

| Violation | Evidence recorded | Recovery |
| --- | --- | --- |
| invalid `dashboards.toml` | key path and rule, as a notice above the dashboard tabs and in `cos business dashboards` | no dashboards for that file until fixed; the first load after the fix clears the notice (§5 Invalid file); the sidebar still works |
| binary not admissible | path and failing rule | pane shows the path and the Setup Runbook hint for `binary missing`, and the rule and path for any other failure; nothing spawned |
| identity probe failed | port, status line, first 200 bytes, `owned` and `spawned` | neither (ensure: probe): pane shows "port held by something else", with the `lsof` next step; `owned` (the app's own server at ensure: owned): pane shows "the app's server on this port answered without this dashboard's marker; open it again, and check the dashboard's `probe` if it repeats", with no `lsof` or stop step; `spawned` (identity after readiness): pane shows "the server the app started, or a listener that took the port before it, answered without this dashboard's marker; the started server was stopped", says to check the dashboard's `probe`, and gives the `lsof` step for a listener still on the port. When `status` is `no response` or `no listener` (`VERIFY(identity)`), the card says the listener on the port sent no complete response to two attempts (or was gone at the second), in place of "port held by something else", and keeps the case's other text. The app signals no listener it did not spawn in any case |
| ownership not proven | pidfile contents, kernel start time, executable and process group, and the declared `binary` for the port (or its absence) | Stale: pidfile removed, nothing signalled. Held: left alone |
| server did not become ready | log tail | stop the group the instance started, remove the pidfile, show the log |
| owned child exited after ready | exit status and log tail | pane shows `exited`; the reaper removes the pidfile; nothing signalled unless a group member outlives the leader, and then the reaper sends SIGKILL to the group (`VERIFY(group_after_reap)`); the next open starts a fresh server |
| child exited during readiness | log tail, and the head of the listener that answered | refuse with `failed to start`; the listener is neither embedded nor signalled |
| listener lacks the dashboard's marker after readiness | status line and first 200 bytes | failure epilogue: the child's stop target is stopped, no pidfile kept; refuse with `identity` marked `spawned`; a listener the app did not spawn is neither embedded nor signalled |
| spawn result refused by `VERIFY(spawn_result)` (pgid, exe, start time or addr) | the failing field | failure epilogue: the new group is stopped (the leader alone when pgid differs from pid), the lock is released after the reap, no pidfile is written; refuse with `failed to start` |
| `dashboard_open` cannot resolve the dashboard | the load error (key path and rule), the undeclared name, or the join error | `InvalidConfig`, `UnknownDashboard` or `Internal`; nothing probed, spawned or signalled |
| POST to `cosreport` without an allowed Origin | origin and path, logged | 403 |
| plist unreadable or legacy | path, logged once | no clock |

### ISA test matrix

| Sink / checkpoint | Refused (tainted or failing input) | Admitted (VERIFY cleared) |
| --- | --- | --- |
| `spawn_server` | relative path; missing file; non-executable file; non-loopback addr; port already answering; port lock held by another instance; no `start_time` support on the platform; unmanaged dashboard (no `server`) with no listener | absolute executable, free loopback port, lock free: one process, pidfile written |
| `signal_group` | External listener; pidfile whose pid is alive but start time differs; pidfile whose executable differs; pidfile whose process group differs from its pid; pidfile whose record `pgid` differs from its pid while the live group equals the pid; pidfile whose executable matches the record but is not the declared `binary` for its port; pidfile whose port has no declared server; missing or invalid `dashboards.toml` at launch; lock held by another instance; Starting slot with no recorded spawn; reaped leader whose group has no member left (`group_alive` false); group signal to a Starting child whose pgid differs from its pid, from the failure epilogue or from `stop_owned`; pidfile named `04777.json` (non-canonical port) while another instance holds `4777.lock`; pidfile whose record `addr` port differs from its filename port | child of this instance not marked Exited, in Starting (spawn recorded) or Owned; SIGTERM at the spawn record of a child recorded after quit began; the leader alone of a Starting child whose pgid differs from its pid, before its reaper marks it Exited; leftover with all five checks equal and the port lock taken by launch cleanup, signalled at `-pid`; SIGKILL to the group of a reaped leader while `group_alive(pgid)` is true; one SIGKILL to a leftover's group after its leader passed the five checks at SIGTERM and exited, while `group_alive(pgid)` is true and cleanup holds the lock |
| `STORE(pidfile)` | write attempted from an unverified pidfile read; removal of an Owned or Starting record whose reaper has not marked it Exited; removal of a path that `VERIFY(pidfile_path)` refuses | written from the spawn result; removal of a Stale record, of a Leftover record once its leader fails the five checks (after the group SIGKILL when a member remained) or after the deadline SIGKILL, or of an Owned or Starting record after its reaper marked it Exited |
| `STORE(port_lock)` | unlink of a lock file; open of a path that `VERIFY(pidfile_path)` refuses; the lock file descriptor inherited by a spawned child | open of `<port>.lock` for a validated addr, close-on-exec |
| `RENDER(iframe src)` | listener whose body lacks the marker; URL not declared for the app business; window with no registry business; non-loopback URL in the file (parse refusal); child exited during readiness; foreign listener that answers `ready` before the child binds but lacks the dashboard's marker; the app's own child whose page passes `ready` but lacks the dashboard's marker (`identity` marked `spawned`, the child stopped); second dashboard on an Owned slot whose page lacks that dashboard's marker (the slot stays Owned, nothing signalled); a refusal after switching from a shown dashboard tab to another (no earlier frame stays shown); an in-flight result after `dashboards-changed` removes the open dashboard or changes its `url` (dropped, pane idle) | owned server ready; External listener with the marker; second dashboard on an Owned slot whose page carries its own marker |
| `PROBE` request (`LOAD(dashboards.toml)`, item 19) | `probe.path` or `ready.path` containing CR, LF or a space; a path not starting with `/`; a URL or `addr` with port 0 (parse refusals) | `/`, `/sales`, `/health` |
| `STORE(ui-state)` | stale business name; window with no registry business (defaults layer, label `default`) while a business named `default` is registered; filename outside `out/briefs`; non-date filename; `mtime` greater than the file's current mtime | app business, existing brief file named by the report frame, `mtime` not greater than the file's |
| `STORE(server_log)` | an unvalidated addr never reaches it: a non-loopback or malformed `addr` (`0.0.0.0:4777`, `localhost:4777`, `127.0.0.1:0`) is a parse refusal (item 19), and the name is built from a parsed port | `<app log dir>/servers/<port>.log` for a validated loopback addr |
| `VERIFY(origin_allowed)` (every non-GET case is sent to `cosreport://localhost/docs/organize` with an `x-cos-business` header equal to the app business and a valid body, so only the Origin decides; a 403 body differs from `docs_response`'s header refusal, `missing or stale X-Cos-Business header`, `apps/desktop/src/lib.rs:489`) | POST with `Origin: http://127.0.0.1:4777`; POST from a frame nested in a dashboard with `Origin: null`; POST with no Origin; GET of `/` with `Origin: http://127.0.0.1:4777`; `OPTIONS` or `PUT` with `Origin: http://127.0.0.1:4777` or no Origin | the same POST with `Origin: tauri://localhost`; GET of `/` with no Origin or `Origin: null` |
| Lifecycle: quit during readiness | quit while the readiness poll runs | the group is signalled and gone within the 5 s deadline; no child remains |
| Lifecycle: quit between spawn and recording the child | quit arrives while the spawn function runs, after its child has started and before the spawn is recorded (the fake spawn starts the child, then waits on a test barrier before it returns) | `stop_owned` signals nothing until the spawn is recorded; the record sends SIGTERM to the new group, which is gone within the 5 s deadline; no pidfile kept |
| Lifecycle: spawn recorded after `stop_owned` returned | the fake spawn's barrier is released only after `stop_owned` has returned | the spawn record sends SIGTERM to the new group; the `ensure` runs the failure epilogue and returns `NotRunning` with "the app is quitting"; the group is gone and no pidfile remains once the epilogue returns |
| Lifecycle: quit before the port lock is taken | quit begins while the key is Reserved and its probe runs (the test's `connect` function blocks on a barrier) | `stop_owned` returns at its 5 s deadline with nothing signalled; once the test releases `connect`, which reports no listener, the `ensure` refuses at lock-take with "the app is quitting"; no spawn; no pidfile; the port lock not held |
| Lifecycle: quit with an Owned server and a probing slot | quit begins while one key is Owned and another key's Reserved probe is blocked on the test's `connect` barrier | the Owned group gets SIGTERM when quit begins, before `stop_owned` waits on the Reserved slot, and is gone without a SIGKILL |
| Lifecycle: concurrent first opens of one key | two `ensure` calls on one key with no slot | one lock and one spawn; the second waits on the slot, runs its own identity probe once the slot is Owned, and returns Running with the same URL, without an "another app instance" refusal |
| Lifecycle: readiness timeout | the readiness poll exhausts 10 s | the group is signalled, the pidfile removed, and `failed to start` returned with the log tail |
| Lifecycle: first open fails, second open waits | the readiness poll exhausts 10 s while a second `ensure` waits on the slot | the waiter returns the same refusal; one spawn only |
| Lifecycle: open while a failed start runs its epilogue | the child exits during readiness, the reaper marks the slot Exited, and a new `ensure` arrives before the epilogue's Refused (the test's `emit` closure, which the reaper calls with `owned: false` after the Exited mark and before `waitpid`, signals the test thread and then sleeps 200 ms, so the epilogue is still waiting on the completion notice when the new `ensure` arrives) | the new `ensure` waits and returns the same refusal; no second lock or spawn; no `dashboard-exited` (`emit` never receives `owned: true`) |
| Lifecycle: waiter while the first open probes | a second `ensure` waits on a Reserved slot whose probe ends in an identity refusal, or in an External listener | identity: the waiter returns the same refusal; External: the waiter runs `ensure` again and embeds the listener; nothing spawned in either case |
| Lifecycle: child exits before readiness passes | the fake spawn's child exits at once; whichever check sees the exit first refuses: the step 1 reads (`VERIFY(spawn_result)`), the Exited check at step 4 of the spawn sequence, or readiness. No hook orders the exit against these steps, so the case asserts the outcome they share, and the step 4 branch alone has no deterministic case | no pidfile remains (none written, or removed by the reaper); `failed to start` with the log tail; the port lock is released after the reap |
| Lifecycle: owned child exits | the child exits while its frame is shown | the slot is Exited; `dashboard-exited` is emitted; the pidfile is removed; no group member remains, so nothing is signalled |
| Lifecycle: reopen right after an owned child exits | a new `ensure` on the key while the slot is Exited and its completion notice unset (the test's `emit` closure, which the reaper calls before `waitpid`, signals the test thread and then sleeps 200 ms) | the `ensure` waits on the notice, then drops the slot and starts one fresh server; no "another app instance" refusal |
| Lifecycle: quit while a reaper finishes | `stop_owned` runs while a slot is Exited with its completion notice unset (same `emit` delay) | `stop_owned` waits on that notice within the shared deadline and polls `group_alive(pgid)`; the group is gone and the port free before it returns |
| Lifecycle: spawn result with pgid not equal to pid | the fake spawn returns a child whose pgid differs from its pid | the epilogue signals the leader alone, never the group; the reaper skips the group kill; lock released after the reap; no pidfile; `failed to start` |
| Lifecycle: quit while a leader-target Starting slot is alive | the fake spawn returns a child whose pgid differs from its pid, and `stop_owned` runs after the spawn is recorded and before the epilogue finishes | `stop_owned` signals the leader alone, never the group, and does not poll `group_alive` for it; the leader is reaped and quit returns without spending the 5 s + 1 s budget on that group |
| Lifecycle: stopped server | the fake server receives SIGSTOP while Owned | the reaper does not accept the stop as an exit; the slot stays Owned; no `dashboard-exited`; `stop_owned` still signals its group; a stopped leader does not act on SIGTERM, so it dies from the SIGKILL sent at the 5 s deadline, and the group is gone within the 5 s + 1 s budget |
| Lifecycle: foreign listener before the child binds | the case's `connect` reports no listener, so ensure: probe finds the port free and spawns; the fake spawn's child never binds, and a test listener on the key's address answers `ready` without the dashboard's marker | readiness passes, the identity probe refuses with `identity` marked `spawned`; the child is stopped; no pidfile; the listener is neither embedded nor signalled |
| Lifecycle: port lock after the reap | a port lock taken while the first instance's owned child is alive; then the child exits and is reaped | before the reap the second instance cannot take the lock; after the reap it takes the lock while the first instance's slot is still Exited, and starts its own server |
| Lifecycle: group member outlives the leader on quit | the fake server forks a member into its group that ignores SIGTERM, then the app quits | the leader exits on SIGTERM and is reaped; `group_alive(pgid)` is true, so the reaper sends SIGKILL to the group; within the 5 s deadline the group is gone and the port is free |
| Lifecycle: leftover group member outlives the leader | after a simulated crash, the leftover runs `serve-with-member`, reparented away from the test process; its member ignores SIGTERM | launch cleanup sends SIGTERM; the leader exits and fails the five checks; `group_alive(pgid)` is true while cleanup holds the lock, so it sends one SIGKILL to the group before the 5 s deadline; the pidfile is removed when cleanup returns, and the group is gone and the port free within 1 s after it returns (cleanup does not wait for the group, §4 Launch cleanup) |
| Lifecycle: undeclared leftover | after a crash, the pidfile's process passes the four kernel checks but no declared server has its port, or the declared `binary` differs from its executable | pidfile removed; nothing signalled; the process keeps running |
| Lifecycle: second `stop_owned` | a second `ExitRequested` or `Exit` after the first | no-op; no signal sent |
| Lifecycle: crash, then launch | app killed with SIGKILL, then relaunched against the same pidfile and the same `dashboards.toml` | launch cleanup signals the leftover group, whose executable is the declared binary for the port; pidfile removed |
| Lifecycle: second instance | second app instance with the port lock held | the probe embeds the server; the second instance signals nothing on quit |
| Lifecycle: dead leftover | pidfile whose pid is dead | pidfile removed; nothing signalled |
| `bootout_dash`, `bootstrap_dash`, `remove_dash_plist` (`tests/schedule.rs`) | a label other than `cos.dash`; a plist path other than the exact ones; an old pid still alive after 10 s (the installed plist is not removed); a failed `cos.dash` render (the installed plist is not removed) | bootstrap only when the port is Free; removal of both files only when the port is Held by a pid other than the old pid, after the old pid exited; on Timeout and on a failed render, `.new` only |
| `STORE(render_dash_plist)` (`tests/schedule.rs`) | a render target other than `{{.LAUNCHD}}/cos.dash.plist.new`; a render that writes the installed plist | `.new` written, and the installed plist unchanged until step 6 moves `.new` into place |
| `install_dash_plist` (`tests/schedule.rs`) | the port is Held or Timeout; a move whose source or target is not one of the two exact plist paths | `.new` moved over the installed plist when the port is Free and the old pid has exited |

## Non-Functional Requirements

- **Security:** covered by the Semantic ISA and by §8. Stated gap: between an app crash (or a SIGTERM to the app) and the next launch, an owned server keeps running unsupervised on loopback.
- **Performance:** each watcher tick costs one `read_dir` per watched directory, one metadata call per run entry (`src/dash.rs:184-186`), and one `kill(0)` per live run. A full re-read runs only when a stamp changes. The probe has a 3 s timeout and runs only on open. The readiness poll runs every 250 ms for at most 10 s, only on open.
- **Process ownership:** every spawned server has an owner (the supervisor), a reaper thread that is its only waiter, a pidfile, a per-port lock released at the reap, and one shared exit deadline. After the reap, the reaper sends SIGKILL to any process left in the server's group. Launch cleanup stops leftovers whose executable is a declared server binary, and sends one SIGKILL to a leftover's group when a member outlives the leader. Nothing is spawned on a render, tick or keystroke path.
- **Rollout and rollback:** `dashboards.toml` absent means no dashboard tabs. Beyond the sidebar, this build removes the `#other-live` button (its counts are on the rows), has the report frame post `brief` and `mtime`, writes `ui-state.json`, and adds the watcher stamps. Rolling back the app leaves `dashboards.toml`, `ui-state.json` and the `servers/` files behind. Older builds read none of them, so a server still running at rollback has no supervisor until its process exits. The registry format is unchanged.

## Confidence

Rock: understood well enough to bet. Sand: the shape is known, and it will move until a named check settles it. Water: unmapped; spike it first.

| Piece | Tier | What moves it up |
| --- | --- | --- |
| `dashboards.toml` parse and validation | rock | n/a |
| Sidebar rendering and switch | rock | n/a |
| Signals from metadata and kernel checks (`proc::exe`, `kill(0)`) | sand | the item 25 CUJ 8 test: a SIGKILL to a reaped supervisor turns the row lost within one tick. An unreaped (zombie) supervisor and a supervisor pid reused by a non-`cos` process are accepted gaps (§Risks) |
| Supervisor spawn, ready, stop on exit | sand | the fake-server test suite in Build |
| Port lock and launch cleanup after a crash | sand | the item 23 simulated-leftover cases (a leftover started through an intermediate process: the matching leftover group is signalled and gone, a leftover group member that ignores SIGTERM is gone within 1 s after cleanup returns from the group SIGKILL, and an undeclared leftover is not signalled), plus E2E CUJ 7, which kills the real app with `kill -9` and relaunches |
| Group kill after the reap | sand | the item 23 test in which a group member ignores SIGTERM and is gone, with the port free, within the 5 s deadline |
| Reaper `waitid` on macOS (stopped child not taken for an exit) | sand | the item 23 test in which the fake server receives SIGSTOP, its slot stays Owned, and `stop_owned` still signals its group |
| Exit path: ⌘Q reaches `stop_owned` once | sand | the E2E CUJ 4 step: `lsof -nP -iTCP:4777 -sTCP:LISTEN` after ⌘Q shows no listener and the pidfile is gone; the item 23 case that a second `stop_owned` is a no-op |
| Iframe embed of loopback pages inside `tauri://` | sand | Q1 passes: the frame loads, `window.__TAURI__` is undefined inside it, the gazette's frame headers allow `tauri://localhost`, and the Origin on `cosreport` POSTs is recorded |
| Identity probe markers for the two real pages and the gazette | sand | markers read from the live pages once the binary is built; recorded in the runbook |
| `task schedule` 8790 guard | rock | n/a |

## Detailed Implementation Plan

Scope (owner decision 2026-11-14): per file, what changes, public signatures, IPC and event contracts, behaviour, and which tests cover which ISA matrix rows. Internal types, derives, serde attributes, field layouts, locking internals and pseudo-code are left to Build.

### Target Files to Create/Modify

#### 1. `src/dashboards.rs` (Create)

*   **Type / Scope**: cos library module.
*   **Proposed Changes**:
    *   `location(registry: &Path, explicit: Option<&Path>) -> PathBuf`: `explicit` when given, else `dashboards.toml` beside `registry`; it reads no environment. The CLI passes `business::location()` and `COS_DASHBOARDS` from its environment; the desktop passes the start registry and `Start.dashboards_file` (§3), in `setup`, `dashboards` and `dashboard_open`.
    *   `load(path) -> Result<Dashboards>`: a missing file gives an empty value (also the value setup uses when the load fails); otherwise parse and validate per §3. A file with only servers or only unmanaged dashboards parses. Errors name the TOML key path (both keys for a duplicate port).
    *   `for_business(name)` and `server_of(dashboard)` lookups; `orphans(registry)` lists business keys not in the registry.
    *   `probe(addr, probe, timeout) -> ProbeResult`: a plain TCP HTTP/1.0 GET with request line `GET <probe.path> HTTP/1.0` and the headers `Host: 127.0.0.1:<port>` and `Connection: close`, so a server that requires `Host` or builds URLs from it answers as it does to a browser; one attempt per call, and the supervisor applies the identity retry rule of `PROBE` (§Semantic ISA). `ProbeResult` is one of the three `PROBE` outcomes, `no_listener`, `no_response` or `response{status, head, matched}`, with complete response and the 64 KiB cap as the `PROBE` row defines them. No new HTTP client dependency.

#### 2. `src/signals.rs` (Create)

*   **Type / Scope**: cos library module; metadata and kernel state only; no process spawn.
*   **Proposed Changes**:
    *   `read(home) -> Signals`: `live`, `lost`, `starting` from `agents::count`; `supervisors` from `agents::live_supervisors`; the latest brief as the greatest `out/briefs/YYYY-MM-DD.json` filename with its mtime in ms.
    *   `mtime_ms(meta) -> i64`: whole milliseconds since the Unix epoch, 0 when unavailable or before the epoch. `store::latest` and `report::data` use it.
    *   `scheduled(plist, registry, home: Option<&Path>) -> Option<String>`: `plist` and `home` come from `Start.user_home` (§6); `home` is the `HOME` fallback for `location_from`; `None` for a parse error, no `--business`, or a registry mismatch (§6).
    *   `stamp(home) -> String`: `dash::stamp([agents::root(home), home/out/briefs])`.

#### 3. `src/agents/mod.rs` (Modify)

*   **Type / Scope**: cos library.
*   **Proposed Changes**:
    *   New `live_supervisors(home) -> Vec<i32>`: the supervisor pids of runs `count` would classify Live (Starting runs have no pid), with `count`'s filters and no `reconcile`, so nothing is written or signalled.
    *   `Counts` (`:553-558`) gains `starting`; `live` still includes Starting runs.
    *   `is_cos(pid)` (`:804-807`) compares the file name of `proc::exe(pid)` with `cos`, so `classify` (`:543-548`), `count` (`:561-592`) and the other callers of `is_cos` (`:675`, `:707`) spawn no `ps`. `comm` (`:794-801`), whose only caller is `is_cos` (`:806`), is deleted; its `ps -o comm=` lookup becomes the off-macOS fallback of `proc::exe` (item 4).

#### 3a. `src/agents/slots.rs` (Modify)

*   **Type / Scope**: cos library, lock helper.
*   **Proposed Changes**: `lock_exclusive` (`:51`) becomes `pub`, so the desktop takes `<ui_state_file>.lock` with it (§6). Its guard (`:36`) and the `slots` module (`src/agents/mod.rs:32`) are already public.

#### 3b. `tests/agents.rs` (Modify)

*   **Type / Scope**: cos integration tests.
*   **Proposed Changes**: the four `Counts` literals (`:686`, `:704`, `:721`, `:756`) gain `starting`. A new case counts one Starting run as `starting: 1` and `live: 1`.

#### 4. `src/proc.rs` (Modify)

*   **Type / Scope**: cos library, process helpers.
*   **Proposed Changes**: next to `alive` (`:247`):
    *   `exe(pid) -> Option<PathBuf>` (macOS: `proc_pidpath`);
    *   `start_time(pid) -> Option<(i64, i64)>` (macOS: `proc_pidinfo(PROC_PIDTBSDINFO)`, seconds and microseconds);
    *   `pgid(pid) -> Option<pid_t>` (`getpgid`, every platform), for launch cleanup's `pgid == pid` check and the stop target;
    *   `signal_group(pgid, signal)`: `kill(-pgid, signal)` with `kill_group`'s guard;
    *   `signal_pid(pid, signal)`: `kill(pid, signal)` for `pid > 0`, used only for a leader stop target, by the failure epilogue and by `stop_owned`.

    `kill_group` (`:237-244`) stays SIGKILL only; the reaper's group kill and launch cleanup's leftover-group kill use it with `group_alive` (`:253-256`). Off macOS, `exe` falls back to `ps -o comm= -p <pid>` and `start_time` returns `None`, so launch cleanup removes pidfiles without a signal and `VERIFY(binary_admissible)` refuses every spawn (rule `platform`), and no child starts that `VERIFY(spawn_result)` would refuse.

#### 5. `src/lib.rs` and `Cargo.toml` (Modify)

*   **Type / Scope**: crate root and manifest of the cos crate.
*   **Proposed Changes**: export the `dashboards` and `signals` modules. Add the direct dependency `plist = "1.10"` (already in the graph through tauri, `Cargo.lock:2437-2438`).

#### 6. `src/business.rs` (Modify)

*   **Type / Scope**: cos library.
*   **Proposed Changes**: `add(path, name, business, replace) -> Result<Vec<String>>` returns the existing businesses whose home contains the new home or is contained by it (canonical paths), so the CLI can warn. The entry being replaced (`replace` with the same `name`) is excluded, as the equal-home check already skips it (`:279-281`). Equal homes and workspaces are still refused (`:277-285`).

#### 7. `src/main.rs` (Modify)

*   **Type / Scope**: CLI.
*   **Proposed Changes**:
    *   `cos business add` prints `warning: <home> is inside <other>'s home` (or contains it) to stderr for each name `add` returns.
    *   New `cos business dashboards [<name>]`: one line per dashboard with business, name, URL, server (or `unmanaged`, §1) and probe result, one line per `PROBE` outcome: `answers` (`response`, matched), `answers, marker missing` (`response`, not matched), `no response` (`no_response`) or `no listener` (`no_listener`); orphaned keys last. The file is loaded and validated before any name lookup: an invalid file prints its key path and rule to stderr and exits 1. In a valid file, a `<name>` with no entry prints `no dashboards for <name>` and exits 0 (§3); every other valid run exits 0. The arm calls no `first_run` and writes nothing; the registry for the orphan report comes from `business::load`, a load error prints to stderr and exits 1 (§3).

#### 8. `apps/desktop/src/servers.rs` (Create)

*   **Type / Scope**: desktop module. The only code that spawns or signals a dashboard server; implements §4.
*   **Proposed Changes**:
    *   `Supervisor::new(data_dir, log_dir, emit, spawn, connect) -> io::Result<Supervisor>`: creates `data_dir/servers` and `log_dir/servers`. `emit` receives the reaper's exit notice `{addr, status, log_tail, owned}` for every child, after the Exited mark and before `waitpid` (§4 Reaper step 2); `spawn` takes the server and the log path and returns `anyhow::Result<std::process::Child>`; `connect` takes the key's address and reports whether a listener answered the TCP connect of ensure: probe (`CONNECT`). The app passes an `emit` that emits the Tauri event `dashboard-exited` only for `owned: true`, `real_spawn` and `real_connect`; tests pass a recording closure, a fake spawn and, where a case needs it, a `connect` that blocks on a test barrier.
    *   `real_spawn(server, log) -> anyhow::Result<std::process::Child>`: the spawn of §4 ensure: start, through `proc::spawn_grouped` (`src/proc.rs:165`, which returns `anyhow::Result<Child>`).
    *   `real_connect(addr) -> bool`: a TCP connect to `addr` with a 3 s connect timeout; true when it is accepted, false on refusal or timeout (`CONNECT`).
    *   `binary_admissible(server, start_time_supported: bool) -> Result<(), Refusal>`: `VERIFY(binary_admissible)` (§4 ensure: start); `ensure` passes whether `proc::start_time` is supported on this platform (item 4), and a test passes `false` to reach rule `platform` on macOS.
    *   `spawn_result_admissible(pid, pgid, start_time, exe) -> Result<(), &'static str>`: `VERIFY(spawn_result)` (§4 ensure: start step 3) over the four values `ensure` reads from the kernel, returning the first failing field: `pid` (`pid <= 0`), `pgid` (not equal to `pid`), `exe` (absent or empty), `start_time` (absent). It takes the values, not a `SpawnResult`, so `SpawnResult` keeps `SpawnResult::read(&Child)` as its only constructor and a pidfile record still comes only from a live child. A real child cannot have `pid <= 0`, an empty executable or no start time, so the tests reach those three fields through this function, not through a fake spawn.
    *   `launch_cleanup(&self, dashboards)`: §4 Launch cleanup, once in `setup`; the declared port-to-binary map comes from `dashboards`, and an empty value declares nothing.
    *   `ensure(&self, server: Option<&Server>, dashboard: &Dashboard) -> Result<Running, Refusal>`: `None` for an unmanaged dashboard; §4 ensure, including the per-dashboard identity probe on reuse and after readiness, and the 5 s wait on an Exited slot's reaper.
    *   `stop_owned(&self)`: §4 Stop path. Idempotent.
    *   **Contracts.**
        *   `Running` = `{url, owned}`: the URL `ensure` verified; the `Ok` value of `dashboard_open`.
        *   Event `dashboard-exited` = `{addr: "127.0.0.1:<port>", status: "exit <n>" | "signal <n>", log_tail}`, emitted by the app's `emit` for an Owned slot only (`owned: true`).
        *   `Refusal`, serialised as an object whose `kind` names the refusal and whose other keys are its fields, for example `{"kind":"Identity","status":"HTTP/1.1 200 OK","head":"…","owned":false,"spawned":false}`; dash.js switches on `kind`. Kinds returned by `ensure`: `Identity{status, head, owned, spawned}` (`owned` true only at ensure: owned, `spawned` true only for the identity probe after readiness, §4), `BinaryMissing{path}`, `NotAdmissible{rule, path}`, `FailedToStart{log_tail}`, `NotRunning{hint}` (every quit refusal has the hint "the app is quitting"). Kinds returned by `dashboard_open` (§5): `StaleBusiness{message}`, `UnknownDashboard{name}`, `InvalidConfig{message}`, `Internal{message}`. Every waiter on a Refused slot gets its own copy. ISA mapping: `VERIFY(identity)` to `Identity`, `VERIFY(binary_admissible)` to `BinaryMissing` or `NotAdmissible`, `VERIFY(readiness)` and `VERIFY(spawn_result)` to `FailedToStart`. The pane renders `BinaryMissing` as the path plus the fixed hint "build it by hand; the Setup Runbook has the command"; no build command is derived from the path.

#### 9. `apps/desktop/src/lib.rs` (Modify)

*   **Type / Scope**: desktop commands, scheme handler, watcher, builder.
*   **Proposed Changes**:
    *   Export the `servers` module, so `tests/servers.rs` and the builder reach the supervisor.
    *   `businesses` rows (`:379-388`) gain `unread: bool` and `scheduled: bool`. One synchronous row builder (§6 Refresh), built from the map in `businesses()` (`:424-438`), spawns nothing; `businesses()` (`:419-440`) copies what it needs under its lock and builds rows on the blocking pool. A shared row cache and one publish function (§6) serve the watcher and `brief_seen`, its only callers, which pass business names; the publish function builds those rows itself while it holds the cache (§6 Shared row cache). The cache holds rows without `app`; the publish function fills the cache with every registered business first when it is not full, replaces the whole cache on a changed `businesses.toml` (dropping unregistered names), and sets `app` from the App's business on each row it emits, so `businesses-changed` always carries the full list in the `businesses` shape.
    *   New IPC commands, registered in `generate_handler!` (`:983`):
        *   `dashboards(business) -> Result<Vec<{name, url, managed}>, String>`;
        *   `dashboard_open(business, name) -> Result<Running, Refusal>`, async, with the order and refusals of §5;
        *   `brief_seen(business, file, mtime) -> Result<(), String>`: `VERIFY(brief_seen)`, the locked write of §6, then the publish of that business, which rebuilds its row (§6).
    *   The three check the business with a new helper that compares the app business label (`Triple::label`, `src/business.rs:347`, as `named()` compares `Current.business`) with `business` and returns `named()`'s error text (`:264`), and copy what they need under the same App-lock acquisition (`brief_seen` also copies the business home whose `out/briefs` it checks, and the ui-state path). None calls `App::get` while holding the lock (`:239-244`). `dashboard_open` returns the error as `StaleBusiness`; the other two as a string. With no registry business (`triple.business` is `None`), checked after the label check and before any load or `VERIFY`, `dashboards` returns an empty list, `dashboard_open` returns `UnknownDashboard`, and `brief_seen` returns an error string and writes nothing. `dashboards` and `dashboard_open` load `dashboards.toml` on every call.
    *   `watch()` (`:891-909`): per §6 Refresh; publishes recomputed rows, emits `dashboards-changed` (no payload, §6 Events) except on its first tick, which only records the `businesses.toml` and `dashboards.toml` mtimes, keeps `report-changed` and `agents-changed`.
    *   `report_response` for `/` (`:451-452`): sets `hostBrief: true` in the report data before rendering. No other path sets it.
    *   `pub fn cosreport_response(app, req) -> Response`: calls `origin_allowed(req) -> bool` first and answers 403 when it is false, then dispatches to `docs_response` or `report_response` by path (§5). The `cosreport` closure (`:963-974`) calls only this function; item 25's Origin cases and its POST helper call it (§5 Origin rule).
    *   `Start` (`:28`) gains `data_dir`, `log_dir`, `ui_state_file` and `dashboards_file` (`COS_DASHBOARDS`, read once by `from_env`; `None` means beside the start registry, §3). Its existing `user_home` (`:31`) gives the `cos.brief` plist path and the `HOME` fallback of `scheduled` (§6); `None` means no clock. `from_env` fills the three paths from the app's path resolver (`:54` reads `app_config_dir` today); tests fill them from a tempdir and set `dashboards_file` to `None` or a tempdir file, as `settings_file` is filled in `apps/desktop/tests/ipc.rs:86-108`. `AppState` (`:80-88`) holds none of these paths, so setup manages the `Start` it used as state beside `App`; the commands, the watcher, the row builder, the supervisor and the ui-state helper read their paths and `user_home` from that managed `Start`, never from the path resolver.
    *   `builder()`: `builder()` returns `Builder<R>` (`:957`) and only `setup` returns a `Result` (`:975-982`), so the supervisor is built inside `setup` for both branches. Today only `App::new(&s)` is managed from a given `Start` (`:958-959`), and the `Start` from `Start::from_env` is dropped after `App::new` (`:976-978`). `builder()` keeps a given `Start` for setup (moved into the setup closure), and setup otherwise reads `Start::from_env`; either way setup manages that `Start` (item 9 `Start`). Setup builds `Supervisor::new` from that `Start` with an `emit` over `app.handle()`, `real_spawn` and `real_connect`, manages it, and returns `Err` on a directory-creation failure, as `Start::from_env` does (`:977`), so `tests/ipc.rs` reaches `dashboard_open` with a managed supervisor. Setup then runs `launch_cleanup` with `dashboards.toml` loaded from its location (§3), or an empty value after logging a load error once.
    *   `pub fn on_run_event(app: &tauri::AppHandle, event: tauri::RunEvent)`: the callback `.run()` takes (item 10); calls `stop_owned()` on `ExitRequested` and `Exit`.
    *   ui-state helper: creates the parent directory, takes `<ui_state_file>.lock` with `cos::agents::slots::lock_exclusive`, and writes with `atomic_write_bytes` (§6).

#### 9a. `apps/desktop/examples/preview.rs` (Modify)

*   **Type / Scope**: desktop example, built by `cargo test --workspace` and linted by `cargo clippy --workspace --all-targets -- -D warnings` (item 18a) and `task desktop-check` (item 18).
*   **Proposed Changes**: the `Start` literal (`:61-69`) gains the three new paths, under the example's temporary home (`data`, `logs`, `ui-state.json`), and `dashboards_file: None`. Its `/cosreport` branch (`:153-169`) keeps calling `docs_response` and `report_response` (§5 Origin rule).

#### 10. `apps/desktop/src/main.rs` (Modify)

*   **Type / Scope**: binary entry.
*   **Proposed Changes**: `.run(tauri::generate_context!())` and its `.expect` (`:5-6`) become `.build(tauri::generate_context!()).expect("run the Chief of Staff window").run(cos_desktop::on_run_event)`, so `RunEvent::ExitRequested` and `Exit` reach `stop_owned`. The `with_menu(builder(...))` wrapping (`:4`) stays.

#### 11. `apps/desktop/tauri.conf.json` (Modify)

*   **Type / Scope**: config.
*   **Proposed Changes**: `frame-src cosreport: http://cosreport.localhost http://127.0.0.1:*` (`:22`). Nothing else.

#### 12. `apps/desktop/ui/index.html` (Modify)

*   **Type / Scope**: markup.
*   **Proposed Changes**: wrap the header and `<main>` in `<div class="shell-main">` beside `<aside id="sidebar" aria-label="Businesses">`. Add `#dash-tabs` in the header after Docs, `#dash-notice` above the tabs, and a `#view-dash` view holding `#dash-state` and `<iframe id="dash-frame" sandbox="allow-scripts allow-same-origin allow-forms">`. Remove `#other-live` (`:180`). Add `<script src="sidebar.js">` and `<script src="dash.js">` after `command.js` and before `app.js` (`:298-299`), so `loadDashTabs` exists when `openFromHash` runs. These are classic scripts sharing one global scope with `app.js`, which declares top-level `invoke`, `listen`, `$` and `ui` (`apps/desktop/ui/app.js:4,5,11,23`): `sidebar.js` and `dash.js` each wrap their code in a function scope as `command.js` does (`apps/desktop/ui/command.js:2-3`), take `invoke` and `listen` from `window.__TAURI__`, and touch `ui` and `$` only after startup (in calls from `openFromHash` and in event handlers). `dash.js` exposes only `window.loadDashTabs`, `window.openDash` and `window.hideDash`; `sidebar.js` exposes nothing.

#### 13. `apps/desktop/ui/sidebar.js` (Create)

*   **Type / Scope**: UI module.
*   **Proposed Changes**: wraps its code in a function scope and exposes nothing on `window` (item 12). Renders one button per row with live count, unread dot, lost warning and clock, each with screen-reader text; the clock's title and screen-reader text read `installed for <name>` (§6 scheduled). A row with `missing: true` (home absent, `counts` null, `apps/desktop/src/lib.rs:385-410`) shows no live, unread or lost glyph and the text `home is missing`, as ⌘K does (`apps/desktop/ui/command.js:33-36`), and stays clickable, so the `switch_business` refusal appears in the status line. Registers the `businesses-changed` listener first; each payload (the full list, §6 Events) replaces all rows. Then takes initial rows from `invoke("businesses")`, and drops that result when a `businesses-changed` payload has already been applied (§6 Refresh). The row whose `app` is true is highlighted. A click calls `window.switchTo(name)`; a rejection shows in the status line. Reads `pinned` from `invoke("paths")` and, when set, disables the rows and titles them `pinned by <var>` (`apps/desktop/ui/app.js:99-100`).

#### 14. `apps/desktop/ui/dash.js` (Create)

*   **Type / Scope**: UI module.
*   **Proposed Changes**: wraps its code in a function scope and exposes only `window.loadDashTabs`, `window.openDash` and `window.hideDash` (item 12). `loadDashTabs()` invokes `dashboards` with `ui.business` and renders tabs; on any error, a stale business's included, it writes the error text to `#dash-notice`, renders no dashboard tabs and resolves; on success it clears `#dash-notice` (§5 Invalid file); its one startup call site is `openFromHash` (item 15); it runs again on `dashboards-changed`, which dash.js ignores until that first call has resolved, and keeps or clears the open dashboard by the rule of §5 Reload. A tab click calls `openTab("dash", "", name)`, then `openDash(name)`. `openDash(name)` calls `hideDash()`, takes a new sequence token, shows "starting…", invokes `dashboard_open`, and applies the result only while its token is the latest and the dash view still shows `name` (§5 Opening); it then sets `#dash-frame.src` to the Running URL or renders the refusal card by `kind` (reason, port, what answered, next command), leaving `src` empty. An `Identity` card with neither `owned` nor `spawned` names the `lsof -nP -iTCP:<port> -sTCP:LISTEN` step (CUJ 5); one with `owned: true` says the app's own server answered without this dashboard's marker, to open it again and check the dashboard's `probe` if it repeats, with no `lsof` or stop step; one with `spawned: true` says the server the app started, or a listener that took the port before it, answered without the marker and the started server was stopped, names the dashboard's `probe` to check, and gives the `lsof` step for a listener still on the port (§Violations). An `Identity` card whose `status` is `no response` or `no listener` uses the wording of §Violations for that case. `window.hideDash()` clears the frame's `src`. On `dashboard-exited` for the open dashboard's addr, with no `openDash` token pending and the frame showing a Running with `owned: true`, it calls `hideDash()` and renders the `exited` card with `status` and `log_tail`; any other `dashboard-exited` is dropped (§5 Opening). Restoring from the hash renders the idle state and calls no `dashboard_open`.

#### 15. `apps/desktop/ui/app.js` (Modify)

*   **Type / Scope**: UI.
*   **Proposed Changes**:
    *   Expose `switchTo` on `window`; `switchTo` (`:166-168`) writes `tab=dash&dash=<d>` for the dashboard view, with `<d>` passed through `encodeURIComponent` (§5).
    *   `openTab(view, pane)` (`:124-128`) gains `dash`: marks only the matching dashboard tab, and calls `window.hideDash()` for any other view.
    *   `VIEWS` (`:118`) gains `dash`; the view switcher routes it. `sidebar.js` and `dash.js` are loaded by `index.html` (item 12).
    *   `openFromHash` (`:148-151`) becomes async, awaits `loadDashTabs()` for every hash, then routes; the dash view opens idle (tab selected, no `dashboard_open`); an undeclared `dash` name opens Today. The startup chain (`:1154`) is unchanged.
    *   `cos-report-ready` handler (`:193-195`): calls `brief_seen` with `ui.business` and the payload's `brief` and `mtime`, only while Today is visible and `ui.business` is set; keeps the last payload that has `brief` and sends it on the switch to Today, and once `openFromHash` has routed at startup when Today is visible then (§6 Opened); ignores a payload without `brief`; always sends the `cos-host {hire: true}` reply (`:194`).
    *   Remove the `#other-live` code (`:1121-1135`), its call (`:141`), its click listener (`:1151`), the `businesses-loaded` listener (`:1152`), and, in `command.js`, its dispatch (item 15a).

#### 15a. `apps/desktop/ui/command.js` (Modify)

*   **Type / Scope**: UI, ⌘K picker.
*   **Proposed Changes**: remove the `businesses-loaded` dispatch in `loadRows` (`:103`); its only listener is removed in item 15. ⌘K keeps reading `businesses`.

#### 16. `apps/desktop/ui/style.css` (Modify)

*   **Type / Scope**: styles.
*   **Proposed Changes**: the body becomes a row: a `12.5rem` sidebar (`tests/design.rs:288-308` admits no pixel length except 1px and 2px), then the main column; row states (app business, hover, disabled); signal glyphs; the refusal card. Existing tokens only. Remove the `#other-live` rule (`:51`).

#### 17. `src/report.html` (Modify)

*   **Type / Scope**: report frame.
*   **Proposed Changes**: `cos-report-ready` (`:455`) carries `brief` and `mtime` from `briefRef`, only when `hostBrief` is true. The `live` flag (`src/report.rs:35-36`) does not gate them.

#### 17a. `src/store.rs` (Modify)

*   **Type / Scope**: cos library, saved outputs.
*   **Proposed Changes**: `latest` keeps only `YYYY-MM-DD.json` names (the pattern its module comment states, `src/store.rs:1`). It reads the full-resolution modified time (`Metadata::modified`) from the open handle before and after reading the bytes, retries from a new handle when they differ, at most three attempts, then fails as a parse failure does, because `save` and `save_json` rewrite in place (`src/store.rs:32,54`); two rewrites within one millisecond still differ at that resolution. The saved value gains that time as `mtime_ms`.

#### 17b. `src/report.rs` (Modify)

*   **Type / Scope**: report render.
*   **Proposed Changes**: `data()` adds `briefRef: {file, mtime}` for the rendered brief. `hostBrief` is set only by `report_response` in the desktop crate (item 9), never by `data()` or `build` (`src/report.rs:24-31`). Test, in `tests/report.rs` (Modify): an archived render has no `hostBrief`, and its page posts no `brief` message.

#### 18. `Taskfile.yml` (Modify)

*   **Type / Scope**: schedule task.
*   **Proposed Changes**: the four `for: [brief, dash]` loops (`:211-222`) become `for: [brief]`; the `cos.dash` steps of §7 follow, in order. `DASH_PORT` is a Taskfile var with default 8790 beside `LAUNCHCTL` (`:6`), a test-only seam (§7 step 5). The comment at `:210` is reworded to cover `cos.brief` only. The `schedule` desc (`:190`), which ends "the dashboard kept running", becomes "weekday 08:30 brief; `cos.dash` only when port 8790 is free"; a failed `cos.dash` render removes `.new` and fails the task with `cos.brief` already installed and loaded (§7 step 1). `desktop-check`'s clippy (`:129`) gains `--all-targets`, so `apps/desktop/tests/*.rs` and `examples/preview.rs` are linted.

#### 18a. `.github/workflows/ci.yml` (Modify)

*   **Type / Scope**: CI. The root `Cargo.toml` is a package and a workspace with members `[".", "apps/desktop"]` and no `default-members` (`Cargo.toml:10-11`), so the bare `cargo clippy --all-targets` and `cargo test` at `ci.yml:30-31` cover the root package only, and no desktop test runs in CI today.
*   **Proposed Changes**: `cargo clippy --all-targets -- -D warnings` becomes `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test` becomes `cargo test --workspace`, so `apps/desktop/tests/servers.rs` and `ipc.rs` run on `macos-latest` (`ci.yml:10`). Build records the first workspace run's duration against `timeout-minutes: 10` (`ci.yml:11`); if the run does not fit, the desktop crate moves to its own job and the limit stays.

#### 19. `tests/dashboards.rs` (Create)

*   **Type / Scope**: cos integration tests. Matrix rows: `PROBE` request; `STORE(server_log)`; the parse-refusal entries of `RENDER(iframe src)` (non-loopback URL) and of `spawn_server` (relative path, non-loopback addr).
*   **Proposed Changes**:
    *   non-loopback URL refused;
    *   unknown server refused;
    *   port mismatch refused;
    *   relative binary refused;
    *   unknown key refused;
    *   two dashboards with the same `name` in one business refused, with the key path;
    *   a business key that breaks the registry's name rule (`business::valid_name`, `src/business.rs:68-72`), such as `Founding_Bets`, refused;
    *   marker shorter than 12 characters refused; markers matching `^<[^<>]*>$` after trim refused (`<title>`, `<div class=x>`, the placeholder `<measured in Build>`); `<title>Wolven portfolio control room` and `<script src="/sales.js"` admitted;
    *   missing file reads as empty;
    *   orphan key reported;
    *   `cos business dashboards` output against a `TcpListener` fixture that answers with and without the marker (`answers`, `answers, marker missing`), against a closed port (`no listener`), and against a listener that accepts the connect and never replies (`no response`);
    *   `probe` against a fixture whose body carries the marker only past the first 64 KiB reports `response` with `matched: false`, and a fixture that closes without a status line gives `no_response`;
    *   a `[server.<name>]` table parses;
    *   a file with only `[[business.longhand]]` and no `[server.*]` table parses with no servers;
    *   two `[server.*]` tables with the same `addr` port refused, the error naming both keys;
    *   an unmanaged dashboard whose URL port equals a declared server's `addr` port refused;
    *   a `probe.path` or `ready.path` containing `\r\n`, a space, or not starting with `/` refused, with the key path; `/`, `/sales` and `/health` admitted;
    *   an `addr` of `127.0.0.1:0` and a URL `http://127.0.0.1:0/` refused;
    *   an `addr` of `0.0.0.0:4777` and of `localhost:4777` refused, with the key path;
    *   a server missing `binary`, `addr` or `ready`, and a dashboard missing `name`, `url` or `probe`, refused with the missing key's path; a dashboard with no `server` parses.

#### 20. `tests/signals.rs` (Create)

*   **Type / Scope**: cos integration tests. Covers the signal states (including the clock) that no matrix row names.
*   **Proposed Changes**:
    *   live counted through `proc::exe`, with no `ps` spawned, macOS only, because off macOS `exe` falls back to `ps` (item 4). No-`ps` mechanism, shared with item 25: the case re-runs its own test binary (`std::env::current_exe()`) as a child process filtered to that one case, with `PATH` set through `Command::env` to a directory whose `ps` writes a marker file and exits non-zero, and a variable that tells the child run to assert; the parent asserts the child passed and no marker exists. No case mutates the test process's own environment (`std::env::set_var` is `unsafe` in edition 2024, `Cargo.toml:4`, `apps/desktop/Cargo.toml:4`, and would race the parallel cases that spawn processes);
    *   lost counted;
    *   starting counted, and included in live;
    *   `supervisors` holds the pid of a Live run and not the dead pid of a Lost run, and reading it writes no `state.json`;
    *   latest brief by filename, with names outside `YYYY-MM-DD.json` ignored;
    *   `mtime_ms` of one file equals the `mtime` the report payload carries;
    *   `scheduled` returns the business only when `COS_BUSINESSES` resolves to the same registry; `None` for a legacy plist and a mismatched registry.

#### 21. `tests/business.rs` (Modify)

*   **Type / Scope**: cos integration tests.
*   **Proposed Changes**: a nested home warns and registers, and `add` returns the outer name; an equal home is still refused; `add` with `replace` on an existing name does not return that name.

#### 21a. `tests/business_cli.rs` (Modify)

*   **Type / Scope**: cos CLI tests.
*   **Proposed Changes**: `cos business add` for a nested home prints `warning: <home> is inside <other>'s home` on stderr and exits 0; `cos business dashboards <name>` for a name with no entry prints `no dashboards for <name>` and exits 0; `cos business dashboards` with no `businesses.toml` reports every key orphaned, exits 0 and leaves no `businesses.toml` behind (no `first_run`); with an unparseable `businesses.toml` it prints the load error on stderr and exits 1.

#### 22. `tests/schedule.rs` (Modify)

*   **Type / Scope**: cos integration tests. Matrix rows: `bootout_dash`, `bootstrap_dash`, `remove_dash_plist`, `STORE(render_dash_plist)`, `install_dash_plist`.
*   **Proposed Changes**: `run_task` (`:125-145`) always passes the CLI arg `DASH_PORT=<port>`, as it passes `LAUNCHCTL` (`:134`), set to a free ephemeral port with no listener, so the existing callers of `assert_bound_to` (`:218`, `:229`, `:239`, `:247`, `:283`) take the Free path on any machine, including one where the gazette holds 8790; `assert_bound_to` (`:156-208`) keeps its `cos.dash` plist and bootout-before-bootstrap assertions for that path. `failed_render_keeps_live_plists` (`:243-276`) still holds: the `|` registry path fails the `cos.brief` render first, before any move. `unschedule_removes_plists` (`:279`) is rechecked under the new order. New cases: with a `TcpListener` on a free ephemeral port passed as `DASH_PORT`, `task schedule` loads `cos.brief`, skips `cos.dash`, prints the holder, and leaves no `cos.dash.plist` in the LaunchAgents directory. With no listener, `cos.dash` is bootstrapped after the old pid is gone. With a fake `launchctl` whose old pid stays alive, the `cos.brief` steps ran, the task fails, `.new` is removed, and the installed `cos.dash.plist` is unchanged. With no job loaded, the empty old pid skips the wait and the task does not fail. The fake `launchctl`, reached through the `{{.LAUNCHCTL}}` seam, prints a `pid = N` line for `print` while the job is loaded and exits non-zero when it is not. In the Timeout case the installed `cos.dash.plist` written before the run is byte-identical afterwards, and in the Free case `.new` is gone after the move (`STORE(render_dash_plist)`). A text scan of `Taskfile.yml` asserts that the `cos.dash` steps name only the label `cos.dash` and the two exact plist paths (`VERIFY(dash_target)`), that the render step writes only `{{.LAUNCHD}}/cos.dash.plist.new`, and that it removes `.new` when the render fails (§7 step 1).

#### 23. `apps/desktop/tests/servers.rs` (Create, `harness = false`)

*   **Type / Scope**: desktop integration tests for the supervisor. Matrix rows: `spawn_server`, `signal_group`, `STORE(pidfile)`, `STORE(port_lock)`, `STORE(server_log)`, the server-side entries of `RENDER(iframe src)`, and every Lifecycle row. The test binary doubles as the fake server: `<self> serve <addr>` binds `addr` and answers `/health` with the `ready` marker, `/` with marker A and `/sales` with marker B, two distinct strings that pass the §3 marker rules; `<self> serve-with-member <addr>` does the same after forking one child that stays in the server's process group and ignores SIGTERM; `<self> idle <addr>` binds nothing and sleeps until signalled. A simulated leftover is started through an intermediate process that exits, so the leftover leader is reparented away from the test process, as after a crash. The file runs on macOS only: off macOS its `main` returns at once, because every spawn is refused there (item 4). CI runs it on `macos-latest` (`.github/workflows/ci.yml:10`) once item 18a makes CI run `cargo test --workspace`. In this file, "emits `dashboard-exited`" means the recording `emit` received `owned: true`.
*   **Proposed Changes**:
    *   one process for two dashboards on one server (probe `/` with marker A, then `/sales` with marker B), and the second open's Running follows a passing identity probe with the second dashboard's own marker;
    *   a second dashboard on an Owned slot, probing `/sales` with a marker the fake server never serves, is refused with `identity` and `owned: true`; the slot stays Owned, nothing is signalled, and the first dashboard still opens;
    *   a declared binary path that does not exist: `BinaryMissing{path}` with the fixed hint, nothing spawned, the port lock not left held;
    *   a regular file without the executable bit: `NotAdmissible{rule, path}`, nothing spawned;
    *   rule `platform`: `binary_admissible(server, false)` on an executable regular file gives `NotAdmissible{rule: platform}`, and `binary_admissible(server, true)` admits it (item 8);
    *   a second supervisor on the same data dir holds `<port>.lock` with nothing listening on the port: `ensure` returns `NotRunning` with the hint "another app instance is starting it; open the dashboard again", and nothing is spawned;
    *   an unmanaged dashboard (`server` `None`) with nothing listening: `NotRunning` with the hint "its owner (for example launchd) is not running it", nothing spawned (Goal 8 `not running`);
    *   `stop_owned` frees the port and leaves no child;
    *   a second `stop_owned` is a no-op;
    *   an external listener is never signalled;
    *   a pidfile with a wrong start time, wrong executable, wrong process group, or a record `pgid` that differs from its pid while the live group equals the pid, is removed without a signal;
    *   a forged pidfile planted at `<port>.json` after `launch_cleanup` and before the first spawn is overwritten with the spawn result's `{pid, pgid, start_time, exe}` and the validated `addr`, never copied (`STORE(pidfile)` write);
    *   a matching pidfile from a dead app, whose executable is the declared `binary` for its port (the test binary, declared in the test's dashboards), has its group signalled by launch cleanup, and its pidfile is removed;
    *   after a simulated crash, a leftover run as `serve-with-member`: launch cleanup sends SIGTERM, the leader exits, the member survives, and cleanup sends one SIGKILL to the group while `group_alive(pgid)` is true and it holds the lock; cleanup returns before the 5 s deadline with the pidfile removed; the test then polls `proc::group_alive(pgid)` and the port every 50 ms for up to 1 s and asserts the group gone and the port free (cleanup does not wait for the group, §4 Launch cleanup);
    *   a pidfile that passes the four kernel checks is removed without a signal when no declared server has its port, when the declared `binary` is another file, and when `launch_cleanup` gets an empty value;
    *   `launch_cleanup` with an empty value and a pidfile whose `<port>.lock` another supervisor holds: the pidfile is kept and nothing is signalled (§4 Launch cleanup step 1);
    *   `Supervisor::new` on a fresh tempdir creates `servers/` under the data and log dirs, and the first spawn writes its pidfile and log there;
    *   a pidfile whose port lock is held by another instance is left alone;
    *   a server that never becomes ready has its group signalled, no pidfile remains, and `FailedToStart{log_tail}` is returned with its log tail (Lifecycle: readiness timeout);
    *   quit during the readiness poll stops the group within the 5 s deadline;
    *   quit between spawn and recording the child: the fake spawn starts its child, then waits on a test barrier before it returns, and `stop_owned` runs while it waits: `stop_owned` signals nothing until the spawn is recorded, the record sends SIGTERM to the new group, which is gone within the 5 s deadline; no pidfile is kept (Lifecycle: quit between spawn and recording the child; `signal_group`: Starting slot with no recorded spawn);
    *   the same barrier released only after `stop_owned` has returned: the record sends SIGTERM to the new group, the `ensure` returns `NotRunning` with "the app is quitting", and the group is gone with no pidfile once it returns (Lifecycle: spawn recorded after `stop_owned` returned);
    *   quit with one key Owned and another key's Reserved probe blocked on the `connect` barrier: the Owned group receives SIGTERM before `stop_owned` waits on the Reserved slot and is gone without a SIGKILL (Lifecycle: quit with an Owned server and a probing slot);
    *   identity retry rule (`PROBE`), with an unmanaged dashboard and a test listener on its port: a listener that closes its first accepted connection without a reply and answers the second with the marker gives Running (External) after two connections; a listener whose first reply lacks the marker gives `Identity` after exactly one GET; a listener that never replies gives `Identity` with `status` `no response` and an empty `head` after two attempts;
    *   an owned child that exits emits `dashboard-exited`, and its slot becomes Exited; no member is left, so `group_alive(pgid)` is false after the reap, the port lock is released, and the reaper's group kill is skipped by `VERIFY(group_after_reap)` (the `signal_group` entry "reaped leader whose group has no member left" has no process to observe a signal, so the case asserts the gate's inputs and outcome);
    *   the first open of a key fails while a second open waits: the waiter returns the same refusal, and only one spawn happens;
    *   a second open that arrives after the reaper marked a Starting slot Exited and before its epilogue's Refused waits and returns that refusal; no second spawn; no `dashboard-exited`. The test's `emit` closure, called with `owned: false` before `waitpid`, signals the test thread and sleeps 200 ms, which holds the epilogue on the completion notice while the second open arrives;
    *   a second open waiting on a Reserved slot returns the same refusal when the probe refuses on identity, and embeds when the probe finds an External listener;
    *   a fake spawn whose child exits at once leaves no pidfile, returns `failed to start` with the log tail, and frees the port lock after the reap, whichever of the step 1 reads, the step 4 Exited check or readiness sees the exit first (Lifecycle: child exits before readiness passes);
    *   the success path wakes a waiter, which runs its own identity probe and returns Running for its dashboard's URL;
    *   the `dashboard-exited` payload carries `log_tail`, and the reaper removes the pidfile at exit;
    *   quit before the port lock is taken: the test's `connect` blocks on a barrier while the key is Reserved; `stop_owned` returns at its 5 s deadline with nothing signalled; the test then releases `connect`, which reports no listener, and the `ensure` returns `NotRunning` with the hint "the app is quitting"; no spawn, no pidfile, and the test's own `flock(LOCK_EX | LOCK_NB)` on `<port>.lock` succeeds;
    *   two concurrent first opens of one key: one lock, one spawn, and the second returns Running with the same URL after its own identity probe;
    *   while the first supervisor's owned child is alive, a second supervisor on the same data dir cannot take the port lock; once the reaper reaps the child, the second supervisor takes it while the first one's slot is still Exited;
    *   `serve-with-member` under `stop_owned`: the reaper sends SIGKILL to the group after the reap, and the group is gone and the port free within the 5 s deadline;
    *   reopen right after an owned child exits: the test's `emit` closure signals the test thread and sleeps 200 ms, so the slot is Exited with its completion notice unset; the new `ensure` waits and starts one fresh server, with no "another app instance" refusal;
    *   `stop_owned` while a slot is Exited with its completion notice unset (same `emit` delay) waits and returns with the group gone and the port free;
    *   a fake spawn whose child's pgid differs from its pid, the one `VERIFY(spawn_result)` failure a fake spawn can produce on a live child, because it returns a real child whose pid, executable and start time come from the kernel: the epilogue signals the leader alone, the reaper skips the group kill, the lock is released after the reap, no pidfile, `failed to start`;
    *   `spawn_result_admissible` refuses `pid <= 0` with `pid`, a pgid other than the pid with `pgid`, an absent or empty `exe` with `exe` and an absent start time with `start_time`, and admits a pid equal to its pgid with a non-empty `exe` and a start time;
    *   quit while a leader-target Starting slot is alive (pgid different from pid, `stop_owned` between the spawn record and the end of the epilogue): only the leader is signalled, `group_alive` is not polled for it, and `stop_owned` returns once the leader is reaped;
    *   SIGSTOP to the owned fake server: the slot stays Owned, no `dashboard-exited` is emitted, and `stop_owned` signals its group; its pidfile still exists after the SIGTERM and until the reaper marks the slot Exited (`STORE(pidfile)`: no removal of an unreaped Owned record); the stopped leader dies from the SIGKILL at the 5 s deadline, and the group is gone within the 5 s + 1 s budget;
    *   with a `connect` that reports no listener, so ensure: probe spawns, a fake spawn whose child never binds, while a test listener on the key's address answers `/health` with the `ready` marker and `/` without the dashboard's marker: `identity` refusal with `spawned: true`, the child stopped, no pidfile, the listener not signalled;
    *   a fake server that binds and passes `ready` but whose page lacks the requesting dashboard's marker: `identity` refusal with `spawned: true` and `owned: false`, the child stopped, the port free, no pidfile;
    *   the first spawn's log is at `<log dir>/servers/<port>.log`, and a log over 1 MiB written before the spawn is truncated to its last 1 MiB before the fake spawn runs (`STORE(server_log)`);
    *   after a spawn, `lsof -p <child pid>` lists no `<port>.lock`, so the child holds no inherited lock (`STORE(port_lock)`: close-on-exec);
    *   the `<port>.lock` file keeps its inode across a spawn, a reap and launch cleanup, and `servers/abc.json` and `servers/70000.json` are ignored by launch cleanup (`STORE(port_lock)`, `VERIFY(pidfile_path)`);
    *   a `servers/04777.json` copied from a live `4777.json` while another supervisor holds `4777.lock` is ignored by launch cleanup: nothing signalled, the other instance's server keeps running;
    *   a `servers/4777.json` whose record `addr` port is 4778 is removed by launch cleanup without a signal;
    *   second instance: with the first supervisor owning the server, a second supervisor on the same data dir embeds it as External through the probe, and its `stop_owned` signals nothing;
    *   a pidfile whose pid is dead is removed by launch cleanup, and nothing is signalled.

#### 24. `apps/desktop/Cargo.toml` (Modify)

*   **Type / Scope**: manifest.
*   **Proposed Changes**: `[[test]] name = "servers" harness = false`; add `libc = "0.2"`, which the desktop crate does not have yet (`cos` already depends on it).

#### 25. `apps/desktop/tests/ipc.rs` (Modify)

*   **Type / Scope**: desktop IPC tests. Matrix rows: `STORE(ui-state)`, `VERIFY(origin_allowed)`, the IPC-side entries of `RENDER(iframe src)` (undeclared URL, no registry business), and the setup side of the `signal_group` entry "missing or invalid `dashboards.toml` at launch".
*   **Proposed Changes**:
    *   the `start()` helper's `Start` literal (`:89-97`) gains `data_dir`, `log_dir` and `ui_state_file` under `root` (`data`, `logs`, `ui-state.json`), and `dashboards_file`, `None` or a tempdir file per case, so no case reads the developer's `COS_DASHBOARDS`;
    *   `post_organize` (`:1109-1123`) sends `Origin: tauri://localhost` and calls `cosreport_response` instead of `docs_response`, so the existing organize cases also pass the Origin rule;
    *   `businesses` rows for three businesses, with the unread dot set and then cleared by `brief_seen`, and the clearance surviving a new `App` instance;
    *   `dashboard_open` refused for a stale business;
    *   `dashboard_open` returns `UnknownDashboard` for a name not declared for the app business, and `InvalidConfig` with the key path when `dashboards.toml` is invalid;
    *   an unmanaged dashboard added to `dashboards.toml` after launch, whose URL port is served by a `TcpListener` fixture answering with its marker, is returned by the next `dashboards` call, and the next `dashboard_open` returns `Running{url, owned: false}` with its declared URL;
    *   `brief_seen` refused for a stale business, for a non-date filename, and for a filename outside `out/briefs`;
    *   through `cosreport_response` (item 9), every non-GET case is a request to `cosreport://localhost/docs/organize` with `x-cos-business` equal to the app business and a valid organize body, so only the Origin decides, and each 403 asserts a body that is not `docs_response`'s `missing or stale X-Cos-Business header` text (`apps/desktop/src/lib.rs:489`): 403 to that POST with `Origin: http://127.0.0.1:4777`, with `Origin: null`, and with no Origin; 200 to the same POST with `Origin: tauri://localhost`; for `/`, 403 to a GET with `Origin: http://127.0.0.1:4777` and 200 to a GET with `Origin: null` and with no Origin;
    *   `cosreport` answers 403 to `OPTIONS` and `PUT` of the same `/docs/organize` request (matching `x-cos-business`) with `Origin: http://127.0.0.1:4777` and with no Origin;
    *   `businesses` does not spawn `ps`, through the child-process mechanism of item 20, macOS only (item 4);
    *   `brief_seen` on a tempdir whose config directory does not exist creates it, takes the lock, and clears the dot;
    *   CUJ 8: a SIGKILL to a run's supervisor without a file change turns the row lost within one tick; the test reaps the supervisor it kills, as `agents::spawn` does (`src/agents/mod.rs:482-485`);
    *   `brief_seen` emits `businesses-changed`, and the emitted payload shows the dot cleared; called before the watcher's first tick, its payload still holds every registered business;
    *   shared row cache: after `brief_seen` clears a business's dot, a stamp change in another business makes the watcher emit all rows, and that payload still has `unread: false` for the first business; then a new brief in the first business turns its dot back on within one tick;
    *   `brief_seen` never lowers the stored pair;
    *   `brief_seen` refused for an `mtime` greater than the brief file's current mtime, and the stored pair unchanged;
    *   on a window with no registry business (label `explicit`), with a `[[business.explicit]]` entry in `dashboards.toml`, `dashboards` returns an empty list and `dashboard_open` returns `UnknownDashboard`;
    *   on a defaults-layer window (label `default`) with a business named `default` registered and a brief under the install home's `out/briefs`, `brief_seen("default", …)` is refused, and the stored `lastSeenBrief["default"]` pair is unchanged;
    *   a business removed from `businesses.toml` after the watcher's first tick is absent from the next `businesses-changed` payload, and the remaining businesses' rows are present;
    *   an invalid `dashboards.toml` at setup, with a pidfile whose pid is a live child of the test: setup completes, launch cleanup removes the pidfile, and the child is still alive (matrix `signal_group`: missing or invalid `dashboards.toml` at launch);
    *   after a switch from business A to business B, a stamp change in a third business makes the watcher emit all rows, and the payload has `app: true` on B only;
    *   a Starting run turns Lost after `START_GRACE` with no file change, and the row updates without an IPC call;
    *   the UI cases run as E2E steps in §Verification Plan, because `ipc.rs` runs on the mock runtime with no webview: (a) a `cos-report-ready` message without `brief` is not passed to `brief_seen` while the `cos-host` reply is still sent; (b) `brief_seen` is not called while another view is visible and is called on the switch to Today; (c) at launch with an unread brief on the app business, the dot clears once Today renders and stays cleared (§6 Opened, sidebar initial rows); (d) a refusal after switching from a shown dashboard tab to another shows no earlier frame; (e) a `dashboards.toml` edit that removes the dashboard being opened, before its result arrives, leaves the pane idle with no frame (`RENDER(iframe src)` rows); (f) a `dashboard-exited` for the shown owned dashboard clears the frame and shows the `exited` card, and one that arrives while an `openDash` is pending is dropped (§5 Opening); (g) a launch with an invalid `dashboards.toml` shows `#dash-notice` and no dashboard tabs, a launch with a valid file shows no notice, and fixing the invalid file while the app runs clears the notice and renders the tabs (§5 Invalid file).

#### 26. `README.md` (Modify)

*   **Type / Scope**: docs.
*   **Proposed Changes**: a "Dashboards" section with the `dashboards.toml` example, the ownership rule, and `cos business dashboards`; index lines for this design, its reviews and the plan.

#### 27. `docs/plans/2026-11-13-business-dashboards-design.md` (this document)

*   **Type / Scope**: design document, revised in place.

### Verification Plan

*   **Compilation/Build**: `task build`, `task desktop-check`.
*   **Unit and integration tests**: `task test` (fmt, clippy `-D warnings`, `cargo test`, root package only, `Taskfile.yml:32-34`) and `cargo test --workspace`, which runs `apps/desktop/tests/ipc.rs` and `servers.rs`; CI runs `cargo test --workspace` after item 18a.
*   **Static analysis**: `cargo clippy --workspace --all-targets -- -D warnings` (CI, item 18a) and `task desktop-check` with `--all-targets` (item 18).
*   **ISA matrix**: every row in the test matrix maps to a named case in items 19 to 25, each of which names the rows it covers, and each case runs in the test file its item names. The seven UI cases of item 25 run as E2E steps below.
*   **States without a CUJ**: the clock state is covered by `tests/signals.rs`; the binary-missing, binary-not-admissible, not-running (unmanaged, no listener) and failed-to-start states are covered by named cases in `apps/desktop/tests/servers.rs` (item 23). The internal-error state (`Internal{message}`) arises only from a join error of `dashboard_open`'s blocking task, which no test can force; it renders through the same refusal card as every other `kind`, and is checked by reading `dash.js` in review.
*   **E2E (built app, real machine)**:
    *   run Q1 checks 1 to 4 before any other E2E step, and record check 3;
    *   register the four businesses;
    *   build the control room; write `dashboards.toml`;
    *   run CUJ 1 to 8;
    *   on `founding-bets`, launch with no hash and with `#tab=agents`: the "Control room" and "Salesroom" tabs render in both cases;
    *   the seven UI cases (a) to (g) of item 25;
    *   capture `lsof -nP -iTCP:4777 -sTCP:LISTEN` before open, after two opens and after ⌘Q;
    *   capture `launchctl list com.longhand.chief-of-staff-dashboard` before and after;
    *   record screenshots with `pr-proof`.
