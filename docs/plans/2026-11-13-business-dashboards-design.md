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
- A crash, or a SIGTERM to the app, can leave an owned server running. The next launch stops that server when its pidfile still matches the process and its executable is the binary `dashboards.toml` declares for that port. The next launch does not adopt it.
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
5. Opening a dashboard whose server the app owns starts at most one server process, however many of its dashboards open. Quitting the app stops every server the app started and leaves no process on the port, including a process in the server's group that outlives the server (§4 Reaper). A SIGTERM or a crash that skips the quit path is covered by Goal 7.
6. A listener the app did not start is embedded when its identity probe passes, refused with a visible reason when it fails, and never signalled by the app.
7. After a crash, a force-quit or a SIGTERM, the next launch first takes the pidfile's port lock. A pidfile whose port lock another instance holds is left alone. With the lock free, the next launch stops the server the earlier app started when the pidfile's pid, start time, executable and process group all match the live process, and the executable is the canonicalised `binary` of a server declared in `dashboards.toml` whose `addr` port equals the pidfile's port. A pidfile under a free lock that does not match, or that matches no declared server, is removed without a signal. Until Q2 is decided, a member of that server's process group that ignores SIGTERM and outlives the leader is not stopped (Risks, Q2).
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

- **Q1 (go/no-go gate, before Build):** the design holds only if checks 1, 2 and 4 pass in the built app, and check 3 is recorded.
    1. The `http://127.0.0.1` iframe loads inside the `tauri://localhost` page without a mixed-content block.
    2. `typeof window.__TAURI__ === "undefined"` inside the frame. The main window sets `withGlobalTauri: true` (`apps/desktop/tauri.conf.json:10`), so the check decides whether the global reaches the frame.
    3. The `Origin` header WebKit sends on a `fetch` POST to `cosreport://localhost/docs/organize` from a page inside a dashboard frame. The in-app docs frame posts nothing while embedded (`src/docs.html:358,365`), so check 3 records the header and does not gate the design.
    4. The `X-Frame-Options`, `frame-ancestors` and `Content-Security-Policy` headers of `http://127.0.0.1:8790/`, read with `curl -sI`. A `DENY`, or a `frame-ancestors` that excludes `tauri://localhost`, fails the check. The control room's CSP has no `frame-ancestors` (`control-room/src/server.rs:247`).

    Fallbacks: if check 1 fails, dashboards open in the browser (Alternative F). If check 2 fails, dashboards are embedded in a child webview (Alternative E). If check 4 fails, the gazette opens in the browser (Alternative F) and the control room stays embedded. If check 3 shows `null` on a dashboard-frame POST, `origin_allowed` already refuses it (§5), and the spike notes the header.

    The spike also measures one identity probe to the control room while a frame is loaded, and records whether it answers within the probe timeout.

- **Q2 (owner decision, before Build):** does launch cleanup send one SIGKILL to a leftover's process group after the leader has exited? The leftover leader is not the app's child, so launchd reaps it once it exits on SIGTERM, the five checks then fail, and a group member that ignores SIGTERM keeps running.
    1. Yes: one group SIGKILL when the leader passed all five checks at SIGTERM time, `proc::group_alive(pgid)` is still true, and cleanup still holds `<port>.lock`. It mirrors `VERIFY(group_after_reap)` and carries the same check-then-kill window (Risks). Applying it adds a `VERIFY(leftover_group)` checkpoint, a `signal_group` matrix row and an item 23 case with `serve-with-member` after a simulated crash.
    2. No: keep the current admission, which signals only while the leader passes the five checks.

    Gate: Build does not start launch cleanup code until the owner answers. Until then the document describes option 2 (Goal 7, §4 Launch cleanup, Risks). The recommendation recorded in EGM round 6 (escalation C5) is option 1.

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
*   **Then**: the launch cleanup finds `4777.json`. The port lock is free, the pid is alive, its start time, executable and process group match the record, and its executable is the declared `control-room` binary, whose `addr` port is 4777. The cleanup stops that process group and removes the pidfile. The next open of a control-room tab starts a fresh server. If the pidfile names a dead pid, a live process that does not match, or an executable that is not the declared binary for port 4777 (for example after `[server.control-room]` is removed from `dashboards.toml`), the cleanup removes the pidfile and signals nothing.

### CUJ 8: A supervisor dies without a file change

*   **Given**: `founding-bets` has one live run, and its supervisor process is running.
*   **When**: the owner sends SIGKILL to the supervisor process (`kill -9`), and no file in the run directory changes. SIGTERM is not used: the supervisor traps it and writes a terminal outcome (`src/agents/supervise.rs:30-40`).
*   **Then**: within one tick (700 ms) the row's live count drops and the lost warning appears.

## Technical Plan (Architecture)

### 1. Terms

- **Business:** a registry entry, as in the base design. A `Business` has three directories, `home`, `workspace` and `projects` (`src/business.rs:20-24`).
- **Registry:** `businesses.toml`, at `COS_BUSINESSES`, else `$XDG_CONFIG_HOME/cos/businesses.toml`, else `~/.config/cos/businesses.toml` (`src/business.rs:40-66`). It holds the active business and each business's directories (`src/business.rs:27-29`). See the [base design](2026-11-12-multi-business-design.md).
- **cos:** the command-line tool and library in this repository, described in the [base design](2026-11-12-multi-business-design.md).
- **App business:** the business one app window shows. ⌘K or the sidebar sets it per window. Its variables are pinned at start by env (`COS_HOME` and the others listed at `apps/desktop/src/lib.rs:110-120`); a pinned window cannot switch (`apps/desktop/src/lib.rs:329-331`). A window whose triple has no registry business (`Triple.business` is `None`, labelled `explicit`, or `default` for the built-in defaults layer, `src/business.rs:347-352`) has no dashboards: `dashboards` returns an empty list and `dashboard_open` returns `Refusal::UnknownDashboard`, whatever `dashboards.toml` declares under that label (§5).
- **Active business:** the business the CLI uses, stored in the registry as `active`. `cos business list` prints it as `current:`.
- **Brief:** the report written by the scheduled `cos.brief` job, which runs on weekdays at 08:30 (`launchd/cos.brief.plist:20-24`). Briefs are stored under `out/briefs` (`src/store.rs:22`). See the [base design](2026-11-12-multi-business-design.md).
- **Profile:** the business's own `profile.json`, read by agents and reports (Alternative C). See the [base design](2026-11-12-multi-business-design.md).
- **Unsaved-Hire guard:** a business switch is refused while the Hire dialog holds unsaved edits (`apps/desktop/ui/app.js:153-163`). See the [base design](2026-11-12-multi-business-design.md).
- **Live, starting and lost runs:** a run is live when its supervisor pid is a `cos` process; starting when it has no supervisor pid within `START_GRACE` of creation (`src/agents/mod.rs:44`); lost otherwise (`src/agents/mod.rs:543-548`). See the [base design](2026-11-12-multi-business-design.md).
- **Run supervisor:** the `cos agents supervise` process of one agent run.
- **Server supervisor:** `servers::Supervisor`, the desktop component that starts and stops dashboard servers.
- **Managed server:** a `[server.<name>]` entry with a `binary`. **Unmanaged dashboard:** a dashboard with no `server` key; the app never starts it.
- **Owned:** a runtime slot only. This app instance spawned the process and holds its port lock until the reaper reaps it. Only owned slots are signalled on exit.
- **External:** a runtime state. The port answered, and the identity probe passed. The app embeds the listener and never signals it. It is not stored as a slot, so every open probes again.
- **Leftover:** a pidfile from an earlier app instance whose process still matches the record, and whose executable is the declared binary of the server on the pidfile's port.
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
                                    rows() row builder (cos::signals, no process spawn)
                                    publish() ─► Rows cache ◄─ brief_seen
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

The control-room markers come from the repository: the title at `control-room/static/index.html:8`, the script tag at `control-room/static/sales.html:9`, and the `/health` body at `control-room/src/server.rs:185-189`. The gazette marker is not measured yet. It is read from the live page during Build and recorded in the runbook. The placeholder `<measured in Build>` is refused by the tag rule below, so the file does not load until the real marker replaces it.

Validation, all at parse time (`dashboards::load` returns the first error with the key path):

- every URL is `http://127.0.0.1:<port>/...`; any other scheme or host is refused;
- every port, in a URL and in an `addr`, is from 1 to 65535; port 0 is refused, because a slot, a pidfile and a lock are named by the port, and launch cleanup admits only ports 1 to 65535 (`VERIFY(pidfile_path)`);
- every `probe.path` and `ready.path` matches `^/[!-~]*$`: it starts with `/` and holds only printable ASCII, with no space, CR or LF, because `dashboards::probe` writes it verbatim into the request line of an HTTP/1.0 GET;
- a dashboard with `server` names a declared server, and its URL's port equals that server's `addr` port;
- `addr` ports are unique across all `[server.*]` tables; a duplicate is refused with both keys named;
- a dashboard with no `server` whose URL port equals a declared server's `addr` port is refused, naming the server it must declare, so a slot key never maps to both a managed and an unmanaged dashboard;
- a server's `binary` is absolute; `addr` is `127.0.0.1:<port>` with a port from 1 to 65535; the argv is fixed as `[binary, "serve", addr]` and is not configurable;
- a probe marker and a `ready` marker have at least 12 characters, and, after trimming whitespace, do not match `^<[^<>]*>$` (a single tag with nothing outside it, such as `<title>`, `<div class=x>` or `<measured in Build>`);
- names are unique per business; business keys follow the registry's name rule;
- a business key not in the registry is reported as orphaned by `cos business dashboards`, not an error, so renaming a business never breaks the file.

The binary's existence and file type are checked when it is started, not at parse time, so `cos business list` works on a machine without the binary.

`cos business dashboards [<name>]` prints each dashboard, its server, and whether its URL answers its probe now (one HTTP GET, 3 s timeout). It never starts anything.

### 4. Server supervisor (desktop, new `apps/desktop/src/servers.rs`)

`servers::Supervisor` is managed Tauri state. `Supervisor::new` creates `<app data dir>/servers` and `<app log dir>/servers` with `create_dir_all`, because `atomic_write_bytes` writes a temporary file beside its target and does not create the parent directory (`src/state.rs:555-569`), and the port lock and the server log are opened in those directories. A failure is returned from `new`, and `setup` propagates it, as it does for `Start::from_env` (`apps/desktop/src/lib.rs:977`). It holds `inner: Arc<Mutex<Inner>>`, where `Inner { slots: HashMap<SocketAddrV4, Slot>, shutting_down: bool }`. A slot is keyed by the host and port of the dashboard's `url`. Validation (§3) makes that equal to the server's `addr` when a server is declared, and makes each port belong to at most one server, so two names for one port cannot start two processes; an unmanaged dashboard such as the gazette is keyed by its URL's port, which no declared server uses. The mutex guards lookup, insert and every slot transition. Probe, spawn and readiness run outside it: a `Reserved` or `Starting` slot carries an `Arc<Waiter>`, where `Waiter { cv: Condvar }` is waited on with the `inner` guard, and concurrent opens for the same key wait on it. The `ensure` that inserted the slot keeps its own clone of that `Arc<Waiter>` until it returns.

**Slots.** `Reserved(waiter)`, `Starting`, `Owned`, `Exited { status, target, exit, waiter }` and `Refused(refusal)`. An External listener has no slot: the open that finds one removes its `Reserved` slot, so every open probes again. `Reserved` is inserted under `inner` at lookup, before any probe, lock or spawn. `Starting` replaces `Reserved` when `ensure: start` takes the port lock, and lasts until `ensure` replaces it with `Owned` or `Refused`, or the reaper marks it `Exited`. `Starting` (once its child is recorded) and `Owned` keep the pid, the stop target, the port lock and the exit cell.

- **Stop target.** `StopTarget::Group(pgid)` or `StopTarget::Leader(pid)`, fixed once at ensure: start step 1 from `proc::pgid(pid)`: `Group(pid)` when it returns `Some(pid)`, and `Leader(pid)` when it returns another pgid or `None`. A `Leader` target means the child does not lead its own group, so the group is not one the app created; only the leader pid is signalled, through `proc::signal_pid`, and that pid cannot be reused while the leader is unreaped. `VERIFY(spawn_result)` refuses a `Leader` target, so an `Owned` slot always has a `Group` target, and a `Leader` target exists only in a `Starting` slot between its `Spawned` record and the failure epilogue, and in the `Exited` slot that follows it. The failure epilogue, the stop path and the reaper all use this one target: a signal reaches the group only for `Group`, and `proc::group_alive` is polled and `proc::kill_group` sent only for `Group`.
- **Child exit.** `ChildExit::Code(i32)` or `ChildExit::Signal(i32)`, built by the reaper from the `siginfo_t` that `waitid` fills: `Code(si_status)` for `CLD_EXITED`, `Signal(si_status)` for `CLD_KILLED` and `CLD_DUMPED`. It is shown as `exit <n>` or `signal <n>`, the `status` string of the `dashboard-exited` payload.
- **Exit cell.** An `Arc<Exit>`, where `Exit { status: Mutex<Option<ChildExit>>, done: Condvar }`, created at spawn and cloned into the slot and the reaper. A thread that waits for the exit clones the `Arc` under `inner`, releases `inner`, and waits on `done` with a deadline. The reaper sets `status` and calls `notify_all` on `done` after the reap and its group check. No waiter moves anything out of the slot, so the reaper's replacement of the slot never discards the cell.
- **Exited.** A child that has exited is `Exited { status, target, exit, waiter }`. The slot holds no port lock; the reaper holds it until it sets the exit cell. `exit` is the slot's exit cell and `target` its stop target, so a thread that finds the slot `Exited` can still wait for the reaper to finish and can check the group of a `Group` target. The exit cell is unset while the reaper still runs `waitpid` and its group check; once it is set, the reaper has released the port lock. `waiter` is the slot's `Arc<Waiter>` when the reaper marked a `Starting` slot, whose `ensure` is still running, and `None` when it marked an `Owned` slot. The pane shows `exited` with the payload `{addr, status, log_tail}` until the next open, where `log_tail` is the last 20 lines of `<app log dir>/servers/<port>.log`.
- **Slot identity.** `ensure` changes a slot only while the slot at its key still carries its own `Arc<Waiter>` (`Arc::ptr_eq`). Recording `Spawned`, writing the pidfile, the `Owned` transition and the failure epilogue's `Refused` each check this under `inner`.
- **Wake-ups.** Every transition out of `Reserved` or `Starting` (to `Starting`, `Owned`, `Exited`, `Refused`, or removal), and the `Spawned` record, calls `notify_all` on the slot's waiter under `inner`.

**Reaper.** `ensure` starts one reaper thread per child, under `inner`, in the same critical section that records `Spawned` in the slot (ensure: start), so the reaper never finds a `Starting` slot without its child. The reaper owns the `Child` and is its only waiter. It calls `waitid(P_PID, pid, WEXITED | WNOWAIT)`, which returns while the leader is an unreaped zombie. An unreaped zombie leader keeps its process group id in use. The reaper accepts the result only when `si_code` is `CLD_EXITED`, `CLD_KILLED` or `CLD_DUMPED`, and calls `waitid` again for any other code: Darwin's `waitid` has been reported to return for a stopped child (golang/go#19314, the reason Go does not use `waitid` on Darwin), and a stopped server must stay `Owned` so the stop path still signals it. It builds the `ChildExit` from that `siginfo_t` (Slots). The reaper then, under the `inner` mutex, marks the slot: an `Owned` slot becomes `Exited { status, target, exit, waiter: None }`, and a `Starting` slot becomes `Exited { status, target, exit, waiter: Some(its waiter) }`, with `notify_all` on that waiter; both keep the slot's exit cell and stop target. It moves the slot's port lock into the reaper thread and removes the pidfile. It releases the mutex. Only when the slot was `Owned` does it emit `dashboard-exited` with the payload above; a child that exits while `Starting` is reported by the `failed to start` refusal of its `ensure`, so the pane shows one state. It then calls `waitpid` to reap. After the reap, if the stop target is `Group(pgid)` and `proc::group_alive(pgid)` (`src/proc.rs:253`) is true, a process the server started is still in its group, and the reaper sends SIGKILL to the group with `proc::kill_group(pgid)` (`src/proc.rs:237`). A pgid stays reserved while any member lives (`src/proc.rs:234`), so a group that still has a member is the server's group. The reaper then drops the port lock, sets the exit cell's `status` and calls `notify_all` on its `done`. From that point another app instance can take the lock and start its own server. Every `signal_group` call on a slot (to its stop target, in the failure epilogue or the stop path) runs under the `inner` mutex and is skipped once the slot is `Exited`, so `signal_group` never reaches a group or leader that has been reaped. The reaper's group kill after the reap is the only signal sent to such a group; Risks names its check-then-kill window.

**Port lock.** Each app instance takes `flock(LOCK_EX | LOCK_NB)` on `<app data dir>/servers/<port>.lock` before it spawns, and keeps the lock while the slot is `Starting` or `Owned`. The reaper releases it once it has reaped the child and run the group check (Reaper), and then sets the exit cell, so an `Exited` slot whose exit cell is unset still has its lock held by its reaper. A slot that never spawned a child releases it when the failure epilogue drops the `Starting` slot. The kernel drops the lock when the process dies. A second instance that cannot take the lock does not signal the server it guards. Lock files are opened, locked and unlocked, never unlinked: unlinking a locked path lets a second instance lock a new inode at the same path.

**`ensure(server, dashboard) -> Result<Running, Refusal>`** runs these named steps, in this order:

- **ensure: owned.** Under `inner`, an `Owned` slot is reused: `ensure` releases `inner` and runs the requesting dashboard's identity probe (`VERIFY(identity)` with `d.probe`, 3 s timeout, retried once) on the key's address. A body with `d`'s marker returns `Running` (owned) built from `d.url`. Any other answer refuses with `identity`, naming the status line and the first 200 bytes, and leaves the `Owned` slot and its server as they are: the server is this instance's and is stopped only by the stop path or its own exit. So every dashboard on a shared server is checked against its own marker, including the second dashboard on a server another dashboard started (Salesroom after Control room). An `Exited` slot whose `waiter` is `None` is dropped here once its exit cell is set, because its reaper has then released the port lock. If the cell is still unset, `ensure` clones the cell, releases `inner`, waits on `done` for up to 5 s (the reaper's remaining work is the `waitpid` of a leader that has already exited and one group check), and runs this `ensure` again from this step. If the cell is still unset at the deadline, it refuses with `not running`, hint "the previous server on this port is still exiting; open the dashboard again", and leaves the slot in place. So a reopen right after an owned server exits never takes the lock while its own reaper still holds it. A `Refused` slot is dropped here. A `Reserved` or `Starting` slot, or an `Exited` slot that still carries its waiter (its `ensure` has not finished the failure epilogue), is waited on through that waiter. When the slot ends `Owned`, the waiter runs this `ensure` again from this step, so it returns `Running` built from its own dashboard's `url` only after its own dashboard's identity probe passes. It returns a clone of the refusal when the slot ends `Refused`, and runs this `ensure` again from this step when the slot is removed. It keeps waiting while the slot is `Reserved`, `Starting`, or `Exited` with a waiter. A waiter spawns nothing.
- **ensure: reserve.** Still under `inner`, with no slot for the key: if `shutting_down` is set, refuse with `not running`; otherwise insert `Reserved` with a new `Arc<Waiter>` and keep a clone. The probe, the lock and the spawn run after this step, outside `inner`.
- **ensure: probe.** Otherwise, a TCP connect to the key's address. If it answers, run the identity probe (`VERIFY(identity)`): an HTTP GET of `probe.path` with a 3 s timeout, retried once. The control room accepts one connection at a time (`control-room/src/server.rs:14-22`) and sets 2 s socket timeouts (`control-room/src/server.rs:39-40`), so a single probe can wait behind another connection. A body that contains the marker gives External: embedded, never signalled. Under `inner` the `Reserved` slot is removed and its waiter notified, so waiters run `ensure` again. Any other answer refuses with `identity`, naming the status line and the first 200 bytes, and under `inner` replaces the `Reserved` slot with `Refused` and calls `notify_all`.
- **ensure: start.** Reached only when the port does not answer. Every refusal in this step replaces the `Reserved` slot with `Refused` and calls `notify_all`:
    - no `server` declared (unmanaged dashboard): refuse with `not running`, hint "its owner (for example launchd) is not running it". The gazette takes this path.
    - server declared: `VERIFY(binary_admissible)` checks that the path is absolute, a regular file, and executable, and that `proc::start_time` is supported on this platform (rule `platform`, item 4). A missing file refuses with `binary missing`, the path, and the fixed hint "build it by hand; the Setup Runbook has the command". Any other failure refuses with the rule name.
    - under `inner`, if `shutting_down` is set, refuse with `not running`. Otherwise take the port lock without blocking in the same critical section, and replace `Reserved` with `Starting`. If another instance holds the lock, refuse with `not running`, hint "another app instance is starting it; open the dashboard again".
    - `VERIFY(log_path)` builds the log path `<app log dir>/servers/<port>.log` from the validated server's `addr` port. The port is a parsed `u16`, so the name has no separator or `..` and the step admits every validated server; it has no refusal branch.
    - `STORE(server_log)`: `ensure` truncates the log at that path to its last 1 MiB, creating it if absent, before it calls the `SpawnFn`, so the fake `SpawnFn` of the tests runs through the same truncation.
    - spawn with the `SpawnFn` (Detailed Implementation Plan, item 8), passing the log path from `VERIFY(log_path)`. Its app implementation, `servers::real_spawn(server: &Server, log: &Path) -> io::Result<SpawnResult>`, calls `proc::spawn_grouped` (`src/proc.rs:165`, which sets its own process group and returns `anyhow::Result<Child>`) and maps that error with `io::Error::other`. The working directory is the binary's parent directory. The control room bakes its root in at compile time (`control-room/src/main.rs:31`), so the working directory does not change what it serves. The environment is inherited from the app. Stdin is null. Stdout and stderr append to `<app log dir>/servers/<port>.log`, which `ensure` truncated in the step above. A spawn error runs the failure epilogue with `failed to start`. Otherwise, in this order:
        1. From the returned `SpawnResult { child }`, take `pid` (`child.id()`) and read `pgid` (`proc::pgid(pid)`, an `Option`), `start_time` and `exe` (`proc::exe(pid)`, so launch cleanup compares the same resolved path). The stop target is fixed from `pgid` (Slots). The child is unreaped and has no waiter yet, so its pid cannot be reused during these reads.
        2. Under `inner`, record `Spawned{pid, target, exit}` in its own `Starting` slot, call `notify_all`, and start the reaper thread with the `Child`, the stop target and a clone of the exit cell. A quit from this point signals the slot's stop target (Stop path).
        3. Run `VERIFY(spawn_result)` on the values from step 1 and the validated server's `addr`. A refusal runs the failure epilogue; no pidfile exists yet.
        4. Under `inner`, if the slot is still its own `Starting`, write the pidfile atomically (`src/state.rs:555`) with `{pid, pgid, start_time, exe, addr}`. If the reaper has already marked it `Exited`, no pidfile is written and the failure epilogue runs with `failed to start` and the log's last 20 lines.
    - after the pidfile is written, `ensure` checks `shutting_down` under the `inner` mutex. If it is set, the failure epilogue runs with `not running`.
    - readiness: poll `ready` every 250 ms for at most 10 s. A pass needs two consecutive passing probes, with the reaper reporting no exit before the first probe and after the second. If the child exits during readiness, the failure epilogue runs with `failed to start` and the log's last 20 lines. A listener that answered during readiness belongs to another process: it is not embedded and not signalled. On timeout, the failure epilogue runs with `failed to start`.
    - identity after readiness: the `ready` marker is generic (`"status":"ok"`), so a foreign listener that bound the port between ensure: probe and the child's bind can pass readiness. `ensure` therefore runs the dashboard's identity probe (`VERIFY(identity)`, 3 s timeout, retried once) on the key's address after a readiness pass. A body without the dashboard's marker runs the failure epilogue with `identity`, naming the status line and the first 200 bytes. A foreign listener that also carries the marker is embedded, as an External listener would be; the child then fails to bind and exits (the control room returns an error from `TcpListener::bind`, `control-room/src/server.rs:11-12`), and the pane shows `failed to start` or `exited` (Risks).
    - success: under `inner`, replace its own `Starting` slot with `Owned`, keeping the pid, stop target, port lock and exit cell, and call `notify_all`. If the reaper marked the slot `Exited` after the second readiness probe, the failure epilogue runs with `failed to start`.
    - **Failure epilogue.** Every refusal after the port lock is taken runs these steps in order: (1) if a `Spawned` record exists, then under `inner` clone the exit cell and, unless the reaper has marked the slot `Exited`, send TERM to the stop target; release `inner` and wait on the exit cell for up to 5 s; if it is still unset, under `inner` and again only while the slot is not `Exited`, send KILL to the stop target, then wait on the cell until it is set. The stop target (Slots) is the process group for `Group` and the leader pid alone for `Leader`, the case of a `spawn_result` refusal on `pgid`; both are admitted by `VERIFY(ownership)` = `Starting`. The final wait ends because SIGKILL cannot be caught and the reaper sets the cell after the reap and its group check; for a `Leader` target, `VERIFY(group_after_reap)` skips the group kill; (2) remove the pidfile if it still exists. `VERIFY(ownership)` admits this only once the reaper has marked the slot `Exited`, so an unreaped group keeps its pidfile; (3) when a child was spawned, the reaper has released the port lock after the reap; when none was, the `Starting` slot still holds it and step (4) releases it by dropping that slot; (4) under the `inner` mutex, if the slot at the key still carries its own waiter (a `Starting` slot, or an `Exited` slot holding that waiter), replace it with `Refused(refusal)`; in every case call `notify_all` through the `ensure`'s own clone of the waiter. A waiter that wakes on `Refused` returns that refusal and spawns nothing. The next `ensure` on the same address removes a `Refused` slot first.

**Launch cleanup.** Runs once in `setup`, before any `ensure`, with the `Dashboards` loaded from `dashboards::location(&start.registry)`. A missing or invalid `dashboards.toml` gives no declared servers, so every pidfile is removed without a signal, and an invalid file is logged once. For each `<app data dir>/servers/<port>.json` whose `<port>` is admitted by `VERIFY(pidfile_path)`: the canonical decimal form of a `u16` from 1 to 65535, so the name equals `port.to_string()` (no leading zero, sign or separator; `04777.json` and `70000.json` are not admitted). Any other entry is left alone, unread and unsignalled, because a non-canonical name would lock a different `<port>.lock` file from the one the owning instance holds.

1. Take `<port>.lock` without blocking. If it is held, another instance owns the server: leave the pidfile and the process alone.
2. The lock is free. A record that does not parse, or whose `addr` port differs from `<port>`, is `Stale`: remove the pidfile and signal nothing. Find the declared server whose `addr` port equals `<port>`; validation (§3) allows at most one. If there is none, remove the pidfile and signal nothing.
3. Compare the record with the kernel and the declaration: the pid is alive, its start time equals `start_time`, its executable (`proc::exe(pid)`) equals `exe`, `pgid == pid`, and that executable equals the declared server's `binary`, both canonicalised with `std::fs::canonicalize`; a path that fails to canonicalise is a mismatch. When all five match, send SIGTERM to the process group, then poll the five checks every 50 ms, up to a 5 s deadline per leftover, until the pid fails them. If they still pass at the deadline, send SIGKILL to the group. Remove the pidfile. Otherwise remove the pidfile and signal nothing.

The five checks run again immediately before the SIGTERM and again immediately before the SIGKILL. The leftover leader is not the app's child: once it exits, launchd reaps it, the pid fails the checks, and no SIGKILL is sent. A member of the leftover's group that ignores SIGTERM therefore keeps running, pending Q2 (Risks). The pidfile is removed once the leader fails the checks or the group SIGKILL is sent; cleanup does not wait for the group to be gone. Cleanup releases each `<port>.lock` after that port's pidfile is handled, so a later `ensure` on the port can take it. It runs synchronously in `setup`, so `ensure` does not run before it returns, and each leftover adds at most the 5 s deadline to setup.

**Stop path.** `stop_owned()` runs from `RunEvent::ExitRequested` and from `RunEvent::Exit`, which requires `apps/desktop/src/main.rs` to switch to `.build(...).expect(...).run(...)` (Detailed Implementation Plan, item 10). It is idempotent: a `shutting_down` flag makes a second call a no-op, and slots are drained once.

- First, set `shutting_down` under the `inner` mutex, then wait on each `Reserved` or `Starting` slot's waiter until the slot has its `Spawned` record, is `Refused`, is `Exited`, or is gone. The wait shares the 5 s deadline below.
- Under `inner`, collect one entry `(stop target, exit cell)` per slot that has a child: each `Owned` slot, each `Starting` slot with a `Spawned` record, and each `Exited` slot whose exit cell is unset (its reaper is still between the mark and the cell). Send SIGTERM to the stop target (Slots) of each `Owned` or `Starting` entry, under the reaper protocol in Slots: the group for `Group`, the leader pid alone for `Leader`, a `Starting` slot whose `spawn_result` refusal has not finished its epilogue. An `Exited` entry is not signalled; its reaper's group check covers it.
- Release `inner`. Within one shared 5 s deadline, wait on every collected exit cell.
- Then, under `inner`, send SIGKILL to the stop target of each slot still `Owned` or `Starting`. Release `inner`, and wait on the cells still unset for up to 1 s more.
- Within the same 5 s + 1 s budget, poll `proc::group_alive(pgid)` every 50 ms for each collected `Group(pgid)` target whose cell is set, until it is false. A `Leader` target is not polled: its group is not one the app created and can stay alive for reasons the app does not own. A reaper sets its cell only after it has reaped the leader and, for a `Group` target whose `proc::group_alive(pgid)` was true, sent SIGKILL to the group (Reaper), so this poll ends at once unless a member is mid-exit.
- When the budget expires, log each stop target whose cell is still unset or whose `Group` is still alive, and return, so quit never blocks on a reaper.
- Remove any pidfile the reaper left, once its slot is `Exited`, and drop each `Exited` slot whose exit cell is set. Its reaper has already released the port lock.
- An `ensure` that reserved its key before `shutting_down` was set is covered by the wait above, from reservation to `Spawned`, `Refused`, `Exited` or removal. A `Reserved` slot still probing at the deadline has no child, so nothing is signalled, and its `ensure` refuses under `inner` at lock-take and spawns nothing. An `ensure` that reserves after the flag is set refuses at reserve. Once `Spawned` is recorded, the slot is signalled by the bullet above.
- An External listener has no slot and is never signalled.

### 5. Dashboard pane (desktop UI)

- The header gets a tab group after Docs, with one tab per dashboard of the app's business. The data comes from the `dashboards` IPC command, which returns the app business's entries. Each tab is an `.app-tab` with `data-view="dash"` and `data-dash="<name>"`. `openTab(view, pane)` (`apps/desktop/ui/app.js:124`) becomes `openTab(view, pane, dash)`: for `view === "dash"` it sets `aria-current` only on the tab whose `data-dash` equals `dash`; the loop at `apps/desktop/ui/app.js:125-128` otherwise marks every dashboard tab. dash.js binds click handlers on the tabs it creates, because the load-time binding at `apps/desktop/ui/app.js:144-146` does not reach them, and those handlers pass the tab's name as `dash`. `openFromHash` (`apps/desktop/ui/app.js:148-151`) becomes async and awaits `loadDashTabs()` first, for every hash (none, `today`, `agents`, `flows`, `docs` or `dash`), and only then routes the hash, so the dashboard tabs render on every launch and the hash's dashboard exists before it is selected. The startup chain stays `loadPaths().then(loadRepos).then(openFromHash)` (`apps/desktop/ui/app.js:1154`), so `ui.business`, which `loadDashTabs()` passes to `dashboards`, is set by `loadPaths` (`apps/desktop/ui/app.js:97`) before the call. A switch reloads the page, which runs the same chain. One rule covers restore: a `dash` name that the app business does not declare (including a business with no dashboards) opens Today. When `openTab` opens any view other than `dash`, it calls `window.hideDash()` (exported by dash.js), which clears `#dash-frame.src`. `switchTo(name)` (`apps/desktop/ui/app.js:159,166-168`), whose `name` is the business, writes the hash as `tab=dash&dash=<d>` for the dashboard view, where `<d>` is the `data-dash` of the `aria-current` dashboard tab; today it writes `tab` and `pane` only.
- `dashboards` returns `Result<Vec<DashboardRow>, String>`. On error, dash.js shows the message (the key path and rule) as a notice in `#dash-notice`, above the dashboard tabs. The sidebar still works.
- **No registry business.** When `st.triple.business` is `None` (§1), `dashboards` returns an empty list and `dashboard_open` returns `Refusal::UnknownDashboard`, before any load, so a `[[business.explicit]]` or `[[business.default]]` entry is never served to a window outside the registry. A registered business named `default` has `Some("default")` and keeps its dashboards.
- **Dashboards source.** `dashboards` and `dashboard_open` each call `dashboards::load(&dashboards::location(&registry))` on every call, with `registry` read from `AppState.registry` under the `App` lock, as `businesses` reads it (`apps/desktop/src/lib.rs:420-423`). No `Dashboards` is cached after setup, so an edit that `dashboards-changed` reports is seen by the next call of either command.
- Opening a tab calls `dashboard_open(app: AppHandle, business, name) -> Result<Running, Refusal>`, an `async` command, because `ensure` can wait for two 3 s probes, a 10 s readiness poll and a final identity probe. In order:
    1. Take `app.lock()` once. Under that guard, check the business with `named_st(&st, &business)`, a new helper that compares `st.triple.label()` with `business` and returns the same error text as `named()` (`apps/desktop/src/lib.rs:264`), and clone `st.registry`; then drop the guard. `named()` takes a `Current`, and its only builder, `App::get`, takes the same non-reentrant mutex (`apps/desktop/src/lib.rs:239-244`), so calling `get()` under `lock()` deadlocks, and two separate locks would let a switch land between the check and the copy. A `named_st` error returns `Refusal::StaleBusiness { message }`. Under the same guard, a `None` `st.triple.business` returns `Refusal::UnknownDashboard { name }` (No registry business, above).
    2. Move a clone of the `AppHandle`, `registry`, `business` and `name` into `blocking` (`apps/desktop/src/lib.rs:275-282`). `blocking` needs a `'static` closure and the `Supervisor` is not `Clone` (it holds a boxed `SpawnFn`), so the closure calls `app.state::<Supervisor>()` itself. The closure returns `anyhow::Result<Result<Running, Refusal>>`; its body always returns `Ok(..)`, so `T` is `Result<Running, Refusal>`.
    3. Inside the closure: a `dashboards::load` error gives `Ok(Err(Refusal::InvalidConfig { message }))`, with the key path and rule; a `name` not declared for `business` gives `Ok(Err(Refusal::UnknownDashboard { name }))`; otherwise `Ok(supervisor.ensure(dashboards.server_of(d), d))`.
    4. An `Err(String)` from `blocking` (a join error) maps to `Refusal::Internal { message }`.

    The pane shows the state while `ensure` runs, and sets the iframe `src` to the `url` in `Running`, the URL `ensure` verified. dash.js uses no other URL. On `dashboard-exited`, dash.js compares the payload's `addr` with the host and port of the open dashboard's `url`, and ignores an event for any other addr.
- The iframe is `sandbox="allow-scripts allow-same-origin allow-forms"` with no `allow-top-navigation`. `allow-forms` stays only if the live gazette page submits a form; the Build step checks the page and drops the flag otherwise. The control room answers 405 to every method except GET and HEAD (`control-room/src/server.rs:56-60`). `allow-same-origin` is set because the control room's CSP sends `connect-src 'self'` (`control-room/src/server.rs:247`), which needs a real origin. The gazette's own CSP is not read yet; Q1 check 4 reads its frame headers, and Build records its CSP.
- The frame is destroyed on business switch, as the page reloads on switch today. The server keeps running until quit. After a switch the pane is idle: the `dash` name in the hash selects a tab when the new business declares that dashboard, and opens Today otherwise (the restore rule above); no dashboard opens until the owner clicks.
- CSP in `apps/desktop/tauri.conf.json:22`: `frame-src` becomes `cosreport: http://cosreport.localhost http://127.0.0.1:*`. Nothing else changes, and `connect-src` stays `ipc: http://ipc.localhost`: the app's own page never fetches the dashboards; only the frames do.
- The `cosreport` handler (`apps/desktop/src/lib.rs:963-974`) calls `origin_allowed(&req)` first, and answers 403 when it returns false:
    - GET is allowed with no Origin, with `Origin: null` (the opaque report frames), or with an app origin.
    - POST requires an Origin that is present and is one of `tauri://localhost`, `http://tauri.localhost`, `cosreport://localhost` or `http://cosreport.localhost`. `null` and an absent Origin are refused.
    - Every other method (`OPTIONS`, `PUT`, `DELETE` and the rest) follows the POST rule, so a dashboard frame gets 403 for each. No in-app frame POSTs today: the docs frame is embedded and saves through the app (`src/docs.html:358,365`).
- `cosreport` serves the reports and accepts writes such as `/docs/organize`. Those writes are scoped by the `x-cos-business` header (`apps/desktop/src/lib.rs:479-484`), which any page that knows the business name can set (`src/docs.html:366` sets it from the query string). The Origin rule keeps a dashboard frame from setting it.
- The capability file `apps/desktop/capabilities/default.json` stays `windows: ["main"]` with no remote URLs, so IPC from `http://127.0.0.1` frames is denied. The design holds only if Q1 check 2 passes: `window.__TAURI__` is undefined inside such a frame.

### 6. Sidebar and signals

**Rendering.** `ui/index.html` wraps the header and `<main>` in a column beside a new `<aside id="sidebar">`. The `--sidebar*` tokens (`apps/desktop/ui/tokens.css:90-97`) style it. Rows come from the `businesses` IPC command, in alphabetical order: the registry is a `BTreeMap` (`src/business.rs:29`), and `write` re-serialises it sorted (`src/business.rs:116-123`). The app's business is highlighted. A click calls `switchTo(name)` (`apps/desktop/ui/app.js:159`). A refusal from `switchTo` (`apps/desktop/ui/app.js:163`) or from `switch_business` (`apps/desktop/src/lib.rs:329-351`) is caught, and its message is shown in a status line below the rows until the next click. `BusinessRow` has no pinned field: sidebar.js reads `pinned` from `invoke("paths")`, as `loadPaths` does (`apps/desktop/ui/app.js:96-100`). A pinned app disables the rows and gives them the same `pinned by <var>` text as the header's `#biz-pin` (`apps/desktop/ui/app.js:99-100`) as their title.

**One source.** The sidebar and ⌘K read the same `businesses` IPC command. `BusinessRow` (`apps/desktop/src/lib.rs:379-388`) gains `unread: bool` and `scheduled: bool`. Its existing `counts` field supplies `live` and `lost`. The Agents-bar button `#other-live` (`apps/desktop/ui/index.html:180`), its update code (`apps/desktop/ui/app.js:1121-1135`), its call at `app.js:141`, its click listener (`app.js:1151`), the `businesses-loaded` listener (`app.js:1152`), the dispatch at `apps/desktop/ui/command.js:103` and its CSS (`apps/desktop/ui/style.css:51`) are removed, because each row shows the same counts. The header count `#agent-count` stays.

**Signals.** Computed by a new `cos::signals` module, from file metadata and kernel checks, with no process spawn:

- **live, lost, starting:** `agents::count(home)` (`src/agents/mod.rs:561-592`) returns `live`, `lost` and `unreadable` today (`src/agents/mod.rs:553-558`), with Starting runs folded into `live` (`src/agents/mod.rs:586`). After item 3 it also returns `starting` as its own count, and `live` still includes Starting runs. Its `classify` calls `is_cos` (`src/agents/mod.rs:545`), which after this design reads the executable with `proc::exe(pid)` instead of spawning `ps` (`src/agents/mod.rs:794-807` today). A recycled pid that now runs another `cos` process (for example `cos dash`) still counts as live, because the check compares the file name only. The same check gates the SIGKILL at `src/agents/mod.rs:675`; binding the supervisor's start time is not part of this design.
- **supervisors:** the supervisor pids of the runs `classify` puts in `Live`, from `agents::live_supervisors(home)` (item 3), which reads spec and state without `reconcile`, as `count` does. Lost runs, whose pid is already dead, are not in the set.
- **unread:** the newest `out/briefs/YYYY-MM-DD.json` by filename (names that do not match `^\d{4}-\d{2}-\d{2}\.json$` are ignored), with its mtime in whole milliseconds since the Unix epoch (`cos::signals::mtime_ms`), compared with `lastSeenBrief[business]` in `<app config dir>/ui-state.json`. It is unread when its filename is greater than the stored filename, or equal to it with a greater mtime.
    - A business with no stored pair, and any brief, is unread.
    - **Opened** means the Today view is visible and the report has rendered the brief. The iframe reloads on every `report-changed` whatever view is active (`apps/desktop/ui/app.js:1140-1141`), so `cos-report-ready` can arrive while another view is shown. The host calls `brief_seen` only when Today is visible at that moment. It keeps the last ready payload in memory, and the switch to Today calls `brief_seen` with it, so a brief rendered during another view is marked seen on that switch.
    - `brief_seen(business, file, mtime)` writes the pair for the brief that the report rendered. The report frame sends `brief: "<file>"` and `mtime` in its `cos-report-ready` message (`src/report.html:455` sends only `type` today, so this is a change to the report frame). `store::latest` opens the brief once and reads its mtime from that handle before and after reading the bytes, and reads again when the two differ, so the mtime describes the content the report rendered (item 17a). Briefs are rewritten in place with `std::fs::write` (`src/store.rs:32,54`), so a rewrite during the read changes both the bytes and the mtime. The host handler at `apps/desktop/ui/app.js:193-195` passes both on. The host never stats the brief to obtain a value: the stored mtime is the one the report sent, so a brief rewritten after render keeps the mtime of the content the owner saw. The filename is validated as in §Semantic ISA.
    - The report data carries `briefRef{file, mtime}` for the brief it rendered. The template posts `brief` and `mtime` only when the data also has `hostBrief: true`. `report_response` sets it for `/` (`apps/desktop/src/lib.rs:451`); `report::build` (`src/report.rs:24-31`) and the browser `cos dash` leave it unset. Archived reports (`/reports/<day>.html`, `src/dash.rs:156-166`) therefore send no `brief` field. For a message without one, the host neither calls `brief_seen` nor keeps it as the last ready payload; it still answers with the `cos-host {hire: true}` reply it sends today (`apps/desktop/ui/app.js:193-194`).
    - `brief_seen` never lowers the stored pair: it keeps the greater of the stored and the sent `(filename, mtime)`.
    - After the write, `brief_seen` recomputes the business row with `rows` and publishes it to the shared row cache (Refresh), which emits `businesses-changed` with all rows, so the dot clears at once. `ui-state.json` is not a tick stamp; the shared cache is what keeps the watcher's next emit from restoring the cleared dot.
    - The read-modify-write runs under an exclusive lock on `<ui_state_file>.lock`, taken with `cos::agents::slots::lock_exclusive`, the helper `business::update` reaches through `business::lock` (`src/business.rs:108,125-130`). That helper is `pub(crate)` today (`src/agents/slots.rs:51`), so item 3a makes it `pub`; its `Held` guard is already `pub` (`src/agents/slots.rs:36`) and the `slots` module is `pub` (`src/agents/mod.rs:32`). Before it opens the lock, `ui_state` calls `create_dir_all` on the parent of `ui_state_file`: nothing in the desktop crate creates the app config dir (`apps/desktop/src/lib.rs:54` only joins a path under it), and `atomic_write_bytes` does not create the parent (`src/state.rs:556-569`). The file is written with `atomic_write_bytes` (`src/state.rs:555`), after the business passed `named_st` under the `App` lock (item 9).
    - `VERIFY(brief_seen)` refuses an `mtime` greater than the brief file's current `mtime_ms`. The check reads the file's metadata and never uses it as the stored value. A rewrite with `std::fs::write` (`src/store.rs:32,54`) sets the mtime to the time of the write, so the mtime the report rendered is not greater than the current one unless the file's mtime was set back by hand (the brief then reads as unread until its next rewrite), and a forged far-future `mtime` cannot pin the stored pair above every later brief of the same name.
    - An unparseable `ui-state.json` reads as empty. Entries for unregistered names are dropped on write.
- **scheduled:** the `cos.brief` plist at `~/Library/LaunchAgents/cos.brief.plist`, read with the `plist` crate. The crate is a direct dependency, already in the graph through tauri (`Cargo.lock:2437-2438`), and it reads binary plists. The business is the string after `--business` in `ProgramArguments`. It counts only when `EnvironmentVariables.COS_BUSINESSES` resolves to the same registry as the app's. The app side resolves its canonicalised `st.registry`. The plist side resolves through `business::location_from` (`src/business.rs:49`) with `COS_BUSINESSES`, `XDG_CONFIG_HOME` and `HOME` read from the plist's `EnvironmentVariables`. For a key the plist lacks, the plist side uses what launchd gives the job: no `XDG_CONFIG_HOME` (`Taskfile.yml:200`), and `HOME` from the app's environment, since launchd sets it to the same user's home. The rendered plist always sets `COS_BUSINESSES` (`launchd/cos.brief.plist:16`), so the fallback applies only to a hand-edited plist. `location_from` fails with `HOME is unset` (`src/business.rs:61`) when none of the three is set. Both paths are canonicalised before comparison.
    - A plist with no `--business` (a legacy plist) marks no business.
    - A parse error marks none and logs once.
    - The plist being on disk does not prove the job is loaded, so the tooltip says "installed for <name>".

**Refresh.** The watcher thread (`apps/desktop/src/lib.rs:891-909`) keeps its 700 ms tick.

- Each tick takes one stamp per business: `cos::dash::stamp([agents::root(home), home/out/briefs])` (`src/dash.rs:181-196`, metadata only), plus the plist's mtime. `dash::stamp` reads directories only, so `businesses.toml` and `dashboards.toml` each take their own mtime from one metadata call.
- Each tick calls `proc::alive(pid)` (one `kill(0)`) for every pid in the business's `supervisors`, the set kept from its last recompute.
- A business is recomputed when its stamp changed, one of its supervisors died (`kill(0)` is false for a pid in that set; the recompute rebuilds the set, which then no longer holds the pid of a run that turned Lost), or it has `starting > 0`: a run turns `Lost` when `START_GRACE` passes, with no file change or pid to watch. A changed `businesses.toml` recomputes every row. A changed `dashboards.toml` emits `dashboards-changed`, which `dash.js` handles by calling `loadDashTabs()`.
- **Shared row cache.** One `Rows(Mutex<BTreeMap<String, BusinessRow>>)` is managed state, and it holds the rows last emitted. The watcher keeps only its stamps and `supervisors` sets; it holds no rows of its own. Both the watcher and `brief_seen` publish through one function, `publish(app, rows)`: under the `Rows` mutex it replaces the cached row of each business it is given (a changed `businesses.toml` replaces the whole map), and, when any cached row changed, emits `businesses-changed` with all cached rows before it releases the mutex. So `brief_seen`'s cleared dot is in the cache the watcher's next emit sends, and a new brief after `brief_seen` differs from the cached `unread: false` row and is emitted on the next tick. The existing `report-changed` and `agents-changed` emits stay (`apps/desktop/src/lib.rs:891-905`); the business stamps are added beside them.
- **Row builder.** `fn rows(registry: &Path, current: Option<&str>, ui_state_file: &Path, only: Option<&str>) -> Res<Vec<BusinessRow>>` is synchronous and spawns no process. It returns every registered business's row, or only the one named by `only`. The `businesses` command calls it inside `blocking`, the watcher calls it on its own thread, and `brief_seen` calls it with `only = Some(business)`.
- Cost per tick is one `read_dir` per directory, one metadata call per run entry (`src/dash.rs:184-186`), and one `kill(0)` per live run. Full re-reads happen only on change.
- The initial rows come from the `businesses` IPC command, which calls `rows` and does not touch the cache. The watcher fills the cache on its first tick with every row.

### 7. `task schedule` and port 8790

`cos.dash` defaults to `127.0.0.1:8790` (`src/main.rs:60`), the port the Longhand gazette holds. `schedule` (`Taskfile.yml:211-222`) renders, moves, boots out and bootstraps both items in four `for: [brief, dash]` loops today, so the move at `Taskfile.yml:216-217` installs `cos.dash.plist` into `~/Library/LaunchAgents` before any check, and launchd loads every plist in that directory at login. After this design the four loops iterate `brief` only, and `cos.dash` is never moved into place except by step 6 below. The `cos.brief` steps run first, then `cos.dash` is handled in this order:

1. Render `cos.dash` to `cos.dash.plist.new` without moving it (S9d).
2. Read the old pid from the `pid = N` line of `{{.LAUNCHCTL}} print gui/$(id -u)/cos.dash 2>/dev/null || true`, through the same `{{.LAUNCHCTL}}` seam as every other launchctl call (`Taskfile.yml:220,222`). `launchctl print` exits non-zero for a job that is not loaded, which `|| true` tolerates. The old pid is then empty, and step 4 is skipped.
3. `TOOL_CALL(bootout_dash)`, admitted by `VERIFY(dash_target)` (S9a), as `{{.LAUNCHCTL}} bootout gui/$(id -u)/cos.dash 2>/dev/null || true`: a bootout of a job that is not loaded fails and is ignored, as at `Taskfile.yml:220` today.
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
| PID or process group reuse between the check and the signal. | An unrelated process group receives SIGTERM or SIGKILL. | Launch cleanup runs its five checks again immediately before each signal. `signal_group` reaches only a group whose leader is not reaped. A residual window between the last check and the signal remains. |
| A process in the server's group outlives the reaped leader. | It keeps the port, or keeps running with no owner. | The reaper checks `proc::group_alive(pgid)` after `waitpid` and sends SIGKILL to the group (§4 Reaper); the stop path polls until the group is gone within its 5 s + 1 s budget, including groups whose slot was already `Exited` when quit began. Residual check-then-kill window: if every member exits between the `group_alive` check and the `kill(-pgid, SIGKILL)`, the pgid is no longer reserved (`src/proc.rs:234`), and a new group that takes the same pgid in that window receives the SIGKILL. The window is not closed. |
| A member of a leftover's process group ignores SIGTERM and outlives the leftover leader. | After the leader exits, launchd reaps it, launch cleanup's five checks fail before the SIGKILL, and the member keeps running and may keep the port. | Pending Q2 (Open Questions). Until the owner answers, launch cleanup sends SIGKILL only while the leader passes the five checks (§4 Launch cleanup), and this case is not mitigated. |
| A leftover's server is removed from `dashboards.toml`, its port changes, its `binary` path changes, or the binary is rebuilt in place (`cargo build --release` at the same path, Setup Runbook), before the next launch. A rebuild replaces the file the running process was started from, so `proc::exe(pid)` or `canonicalize` can fail or differ, which counts as a mismatch (§4 Launch cleanup step 3). | Launch cleanup removes the pidfile and does not stop the process, which keeps the port until it exits or a reboot. | Accepted (owner decision, 2026-11-14): cleanup signals only an executable that is a declared server's binary for that port. The next open probes the port; a listener that passes the identity probe is embedded as External, and one that fails it is refused with the `lsof` next step (CUJ 5). |
| Q1 fails. | Embedding does not hold. | Check 1 fails: dashboards open in the browser (Alternative F). Check 2 fails: child webview (Alternative E). Check 4 fails: the gazette opens in the browser (Alternative F) and the control room stays embedded. |
| Q1 check 3 shows `null` on a dashboard-frame POST. | The POST is refused. | `origin_allowed` refuses `null` (§5). The spike records the header; the rule does not change. |
| Probe markers drift in the real pages. | The identity probe refuses a healthy dashboard. | Markers are read from the live pages in Build. A refusal shows what answered, so the drift is visible. Markers must be at least 12 characters and must not match `^<[^<>]*>$` (§3). |
| A foreign listener binds the port after ensure: probe found it free and before the child binds. | The generic `ready` marker passes on the foreign listener. | `ensure` runs the dashboard's identity probe after readiness (§4 ensure: start); a body without the marker is refused with `identity` and the child is stopped. Residual: a foreign listener that carries the marker is embedded under the dashboard's name until the child, which cannot bind, exits and the pane shows `failed to start` or `exited`. While the slot is `Owned`, every other dashboard on it is embedded only after its own identity probe passes (§4 ensure: owned). The window is not closed; binding the listener to the child's pid is not part of this design. |
| The `http://127.0.0.1:*` frame source. | Any local listener can be framed once its probe passes. | Embedding follows the probe. Frames have no IPC (capability file, §5). The Origin rule covers the report scheme. |
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
| S1 | Spawn a server process from a binary named in `dashboards.toml` | `servers::ensure` | yes: stop the group | the owner: the binary runs as them (§8) |
| S2 | Signal a process group, or a leader alone (SIGTERM, SIGKILL), through `proc::signal_group`, `proc::signal_pid` (a `Leader` stop target, in the failure epilogue and in `stop_owned`) and `proc::kill_group` (the reaper's group kill) | `servers::stop_owned`, launch cleanup, failed-start cleanup, the reaper's group kill after the reap | no | any process whose process group is reused or wrongly adopted |
| S3 | Write and remove a pidfile; create and lock a port lock file, never unlinked | `<app data dir>/servers/<port>.json` and `<port>.lock`, and the `servers/` directory created by `Supervisor::new` | yes | the app's own recovery |
| S4 | Append a server log, truncated to 1 MiB at spawn | `<app log dir>/servers/<port>.log`, in the directory created by `Supervisor::new` | yes | disk |
| S5 | HTTP GET to a loopback port (probe, ready), with a request path validated at parse time (§3) | `servers`, `cos::dashboards::probe` (`cos business dashboards`) | yes, read only | the local service probed |
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
| `LOAD(dashboards.toml)` | Memory | deterministic parse + validation | no | in: path; out: `Dashboards {servers: map<name, Server{binary: abs path, addr: 127.0.0.1:port (1 to 65535), ready: Probe}>, business: map<name, [Dashboard{name, url: loopback with port 1 to 65535, server?: name, probe: Probe}]>}`, `Probe {path: ^/[!-~]*$, contains: marker}` |
| `PROBE(addr, probe)` | Execution | read-only HTTP GET, 3 s timeout | no: S5 is a read-only GET to loopback, and its one tainted input, `path`, is admitted by `LOAD(dashboards.toml)` only when it matches `^/[!-~]*$` (§3), so it cannot add a header or a second request | in: `{addr: 127.0.0.1:port, path: string matching ^/[!-~]*$, contains: string}`; out: `{answered: bool, matched: bool, status: u16?, head: string≤200}` |
| `VERIFY(binary_admissible)` | Normative | deterministic checkpoint | no | in: `Server`; out: admit, `Refusal::BinaryMissing { path }` for a missing file, or `Refusal::NotAdmissible { rule, path }` for any other failure; refuses with rule `platform` when `proc::start_time` is unavailable |
| `VERIFY(ownership)` | Normative | deterministic checkpoint | no | in: `{slot?, pidfile?, lock, declared: map<port, binary>}`, where `declared` is built from the loaded `dashboards.toml` servers; out: `Owned{pid, target} \| Leftover{pid, pgid} \| Starting{pid, target} \| Stale \| Held`; `Leftover` requires the four kernel checks, `canonicalize(exe) == canonicalize(declared[port])`, and the lock free; a pidfile whose port has no declared server, that does not parse, or whose record `addr` port differs from its filename port is `Stale`; `Starting` requires the slot's `child` set, the reaper not having marked it `Exited`, and the lock held by this instance, and admits its stop target (§4 Slots): the group for `Group`, the leader pid alone for `Leader`; `Owned` always carries a `Group` target |
| `VERIFY(identity)` | Normative | deterministic checkpoint over `PROBE` output | no | in: `{probe result, marker}`; out: admit or `Refusal::Identity { status, head }` |
| `TOOL_CALL(spawn_server)` | Execution | process spawn | **yes** (S1) | in: `{binary, addr, env, log}`, env inherited from the app, argv fixed `[binary, serve, addr]`, working directory the binary's parent, `log` from `VERIFY(log_path)`; out: `SpawnResult {child}`; ensure: start step 1 derives `{pid, pgid, start_time, exe}` from `child` |
| `VERIFY(readiness)` | Normative | deterministic checkpoint over `PROBE` output and the child's exit state | no | in: `{probe_1: PROBE, probe_2: PROBE, exited_before_1: bool, exited_after_2: bool}`, where `exited` means the reaper has marked the slot `Exited`; out: admit or `Refusal::FailedToStart { log_tail }` |
| `VERIFY(log_path)` | Normative | deterministic checkpoint, run in ensure: start before the spawn | no | in: `{addr}` of a validated server; out: `path` = `<app log dir>/servers/<port>.log`; always admits, because the port is a parsed `u16` |
| `STORE(server_log)` | Memory | truncate to the last 1 MiB in `ensure` before the `SpawnFn` runs, then append from the child | **yes** (S4) | in: `{path}` from `VERIFY(log_path)`; out: unit |
| `VERIFY(brief_seen)` | Normative | deterministic checkpoint | no | in: `{business, file, mtime}`; out: admit or refusal `{rule: brief_seen}`; admit only when the business is the app's, `file` matches `^\d{4}-\d{2}-\d{2}\.json$`, `mtime` is an integer of at least 0 in ms since the Unix epoch and not greater than the file's current `mtime_ms`, and `file` exists under `out/briefs` (both checks only, not value sources); the business check is `named_st` under the `App` lock |
| `VERIFY(group_after_reap)` | Normative | deterministic checkpoint | no | in: `{target: StopTarget, reaped: bool}`; out: admit or skip; admit only when the target is `Group(pgid)`, this instance's reaper has just reaped the leader whose pid equals `pgid`, and `proc::group_alive(pgid)` is true; a `Leader` target is always skipped; the only signal it admits is KILL |
| `TOOL_CALL(signal_group)` | Execution | deterministic action | **yes** (S2) | in: `{target: Group(pgid) \| Leader(pid), signal: TERM \| KILL}`; out: unit; `Group` is `proc::signal_group`, or `proc::kill_group` for the reaper's KILL; `Leader` is `proc::signal_pid` |
| `STORE(pidfile)` | Memory | atomic write, or removal | **yes** (S3) | in: `{pid, pgid, start_time, exe, addr}` for a write (admitted by `VERIFY(spawn_result)`), or `{path}` for a removal (admitted by `VERIFY(pidfile_path)`); out: unit; the `servers/` directory is created by `Supervisor::new` at the fixed path `<app data dir>/servers`, which no input names, so it needs no VERIFY |
| `STORE(port_lock)` | Memory | open and flock `<port>.lock`, never unlink | **yes** (S3) | in: `{path}` admitted by `VERIFY(pidfile_path)`; out: unit; same fixed directory as `STORE(pidfile)` |
| `RENDER(iframe src)` | Memory | UI action | **yes** (S6) | in: `{url: loopback}`; out: unit |
| `STORE(ui-state)` | Memory | atomic write | **yes** (S7) | in: `{business: registered name, file: YYYY-MM-DD.json, mtime: i64 (ms since the Unix epoch)}`; out: unit; the parent directory and `<ui_state_file>.lock` are at the fixed path `AppState.ui_state_file`, set from `Start`, which no input names, so they need no VERIFY |
| `VERIFY(origin_allowed)` | Normative | deterministic checkpoint | no | in: `{method, Origin?}`; out: admit or 403 |
| `VERIFY(spawn_result)` | Normative | deterministic checkpoint | no | in: `{pid, pgid: Option<pid_t>, start_time, exe}` derived from the `child` of `TOOL_CALL(spawn_server)` (ensure: start step 1), and `addr` from the validated `Server`; out: admit or `Refusal::FailedToStart { log_tail }`, with the failing field logged; admit only when pid > 0, pgid == Some(pid) (a `Group` stop target), exe non-empty, start_time present, and the record's addr taken from the validated server, never from the child |
| `VERIFY(pidfile_path)` | Normative | deterministic checkpoint | no | in: `{addr, file}`; out: admit or refusal `{rule: pidfile_path}`; admit only when `file` is `<port>.json` or `<port>.lock` for the port of a validated addr, or, in launch cleanup, for a `servers/<port>.json` entry whose `<port>` is the canonical decimal form of a `u16` from 1 to 65535 (`name == port.to_string()`: no leading zero, sign, separator or `..`); launch cleanup then compares the record's `addr` port with that port (`VERIFY(ownership)`); a `.lock` path is opened and locked, never unlinked |
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
- **Propagation:** a `PROBE` result is tainted. A pidfile read is tainted until `VERIFY(ownership)` checks it against the kernel's view of the pid (start time, executable and process group) and against the `binary` that `dashboards.toml` declares for its port.
- **Sinks:** `spawn_server`, `signal_group`, `STORE(pidfile)`, `STORE(port_lock)`, `STORE(server_log)`, `RENDER(iframe src)`, `STORE(ui-state)`, `STORE(render_dash_plist)`, `bootout_dash`, `bootstrap_dash`, `install_dash_plist`, `remove_dash_plist`.

### VERIFY before every sink

| Sink | Cleared by | Check |
| --- | --- | --- |
| `TOOL_CALL(spawn_server)` | `VERIFY(binary_admissible)`, the `PROBE` result with `answered: false` (ensure: probe), and the port lock taken by this instance (ensure: start) | path absolute, exists, regular file, executable bit set, platform supports `start_time`; addr `127.0.0.1:<port>`; argv fixed |
| `TOOL_CALL(signal_group)` | `VERIFY(ownership)` = Owned, Starting or Leftover; or, for the reaper's SIGKILL after the reap, `VERIFY(group_after_reap)` | Owned: the slot is not marked `Exited` by its reaper, which marks it before it reaps. Starting: the slot's `child` is set and the slot is not marked `Exited`, and the lock is held by this instance; the target is the slot's stop target (§4 Slots), the group for `Group` and the unreaped leader pid alone for `Leader`, in the failure epilogue and the stop path alike. Leftover: pid alive, start time equal, executable equal, process group equal to pid, executable equal to the declared `binary` for the pidfile's port (both canonicalised), and the lock free. The five checks run again immediately before the signal. After the reap: the reaper reaped the leader of `pgid` and `group_alive(pgid)` is true; the residual check-then-kill window is in Risks. Held and Stale never reach this sink |
| `STORE(pidfile)` write | `VERIFY(spawn_result)` | the record is built from the spawn result, never copied from an earlier file |
| `STORE(pidfile)` removal | `VERIFY(ownership)` = Stale; Leftover once its leader fails the five checks after SIGTERM, or after the group SIGKILL (the group may still have a member, pending Q2); or Owned or Starting of this instance after its reaper marked it `Exited`; and `VERIFY(pidfile_path)` | the file is `<app data dir>/servers/<port>.json` for the validated port |
| `STORE(port_lock)` open | `VERIFY(pidfile_path)` | the file is `<app data dir>/servers/<port>.lock` for the validated port; it is never unlinked |
| `STORE(server_log)` | `VERIFY(log_path)` | the path is `<app log dir>/servers/<port>.log`, built only from the `u16` port of a validated `addr`, so the name has no path separator or `..` |
| `RENDER(iframe src)` | `VERIFY(identity)` with the requesting dashboard's own probe, in the same `ensure` call that returns Running: for an External server at ensure: probe; for a server this call spawned, after `VERIFY(readiness)` (the `ready` marker is generic, so readiness alone does not tie the listener to the dashboard); for an `Owned` slot reused at ensure: owned, including by a waiter, before Running is returned | the URL is one declared in `dashboards.toml` for the app's business (validated loopback), found by `dashboard_open` in a fresh load (an undeclared name is `Refusal::UnknownDashboard`), and `ensure` returned Running |
| `STORE(ui-state)` | `VERIFY(brief_seen)`, which includes `named_st` and the registered-name check | the business named is the app's business; the filename matches `^\d{4}-\d{2}-\d{2}\.json$` and exists under that business's `out/briefs`; `mtime` is not greater than the file's current `mtime_ms` |
| `TOOL_CALL(bootout_dash)` | `VERIFY(dash_target)` | label `cos.dash` and the exact plist path only |
| `TOOL_CALL(bootstrap_dash)` | `VERIFY(dash_port_held)` = Free, and `VERIFY(dash_target)` | runs only after the old pid is gone and no listener holds the port |
| `TOOL_CALL(remove_dash_plist)` | `VERIFY(dash_port_held)` = Held (`.new` and the installed plist) or Timeout (`.new` only), and `VERIFY(dash_target)` | the `.new` file and the installed plist only; never a glob; on Timeout the installed plist is kept |
| `STORE(render_dash_plist)` | `VERIFY(dash_target)` | the `.new` path only; the installed plist is not written |
| `TOOL_CALL(install_dash_plist)` | `VERIFY(dash_port_held)` = Free, and `VERIFY(dash_target)` for both paths | `.new` is moved over the installed plist only after the old pid is gone and the port is Free |

### Violations

| Violation | Evidence recorded | Recovery |
| --- | --- | --- |
| invalid `dashboards.toml` | key path and rule, as a notice above the dashboard tabs and in `cos business dashboards` | no dashboards for that file until fixed; the sidebar still works |
| binary not admissible | path and failing rule | pane shows the path and the Setup Runbook hint for `binary missing`, and the rule and path for any other failure; nothing spawned |
| identity probe failed | port, status line, first 200 bytes | pane shows "port held by something else", with the next step; nothing signalled |
| ownership not proven | pidfile contents, kernel start time, executable and process group, and the declared `binary` for the port (or its absence) | Stale: pidfile removed, nothing signalled. Held: left alone |
| server did not become ready | log tail | stop the group the instance started, remove the pidfile, show the log |
| owned child exited after ready | exit status and log tail | pane shows `exited`; the reaper removes the pidfile; nothing signalled unless a group member outlives the leader, and then the reaper sends SIGKILL to the group (`VERIFY(group_after_reap)`); the next open starts a fresh server |
| child exited during readiness | log tail, and the head of the listener that answered | refuse with `failed to start`; the listener is neither embedded nor signalled |
| listener lacks the dashboard's marker after readiness | status line and first 200 bytes | failure epilogue: the child's stop target is stopped, no pidfile kept; refuse with `identity`; the listener is neither embedded nor signalled |
| spawn result refused by `VERIFY(spawn_result)` (pgid, exe, start time or addr) | the failing field | failure epilogue: the new group is stopped (the leader alone when pgid differs from pid), the lock is released after the reap, no pidfile is written; refuse with `failed to start` |
| `dashboard_open` cannot resolve the dashboard | the load error (key path and rule), the undeclared name, or the join error | `Refusal::InvalidConfig`, `Refusal::UnknownDashboard` or `Refusal::Internal`; nothing probed, spawned or signalled |
| POST to `cosreport` without an allowed Origin | origin and path, logged | 403 |
| plist unreadable or legacy | path, logged once | no clock |

### ISA test matrix

| Sink / checkpoint | Refused (tainted or failing input) | Admitted (VERIFY cleared) |
| --- | --- | --- |
| `spawn_server` | relative path; missing file; non-executable file; non-loopback addr; port already answering; port lock held by another instance; no `start_time` support on the platform | absolute executable, free loopback port, lock free: one process, pidfile written |
| `signal_group` | External listener; pidfile whose pid is alive but start time differs; pidfile whose executable differs; pidfile whose process group differs from its pid; pidfile whose executable matches the record but is not the declared `binary` for its port; pidfile whose port has no declared server; missing or invalid `dashboards.toml` at launch; lock held by another instance; Starting slot whose `child` is not yet set; reaped leader whose group has no member left (`group_alive` false); group signal to a Starting child whose pgid differs from its pid, from the failure epilogue or from `stop_owned`; pidfile named `04777.json` (non-canonical port) while another instance holds `4777.lock`; pidfile whose record `addr` port differs from its filename port | child of this instance not marked `Exited`, in Starting (child set) or Owned; the leader alone of a Starting child whose pgid differs from its pid, before its reaper marks it `Exited`; leftover with all five checks equal and the lock free; SIGKILL to the group of a reaped leader while `group_alive(pgid)` is true |
| `STORE(pidfile)` | write attempted from an unverified pidfile read; removal of an Owned or Starting record whose reaper has not marked it `Exited`; removal of a path that `VERIFY(pidfile_path)` refuses | written from the spawn result; removal of a Stale record, of a Leftover record once its leader fails the five checks or after the group SIGKILL, or of an Owned or Starting record after its reaper marked it `Exited` |
| `STORE(port_lock)` | unlink of a lock file; open of a path that `VERIFY(pidfile_path)` refuses | open of `<port>.lock` for a validated addr |
| `RENDER(iframe src)` | listener whose body lacks the marker; URL not declared for the app business; window with no registry business (`Triple.business` is `None`); non-loopback URL in the file (parse refusal); child exited during readiness; foreign listener that answers `ready` before the child binds but lacks the dashboard's marker; second dashboard on an `Owned` slot whose page lacks that dashboard's marker (the slot stays `Owned`, nothing signalled) | owned server ready; External listener with the marker; second dashboard on an `Owned` slot whose page carries its own marker |
| `PROBE` request (`LOAD(dashboards.toml)`, item 19) | `probe.path` or `ready.path` containing CR, LF or a space; a path not starting with `/`; a URL or `addr` with port 0 (parse refusals) | `/`, `/sales`, `/health` |
| `STORE(ui-state)` | stale business name; filename outside `out/briefs`; non-date filename; `mtime` greater than the file's current mtime | app business, existing brief file named by the report frame, `mtime` not greater than the file's |
| `STORE(server_log)` | an unvalidated addr never reaches it: a non-loopback or malformed `addr` is a parse refusal (item 19), and the name is built from a `u16` port | `<app log dir>/servers/<port>.log` for a validated loopback addr |
| `VERIFY(origin_allowed)` | POST with `Origin: http://127.0.0.1:4777` to `cosreport://localhost/docs`; POST from a frame nested in a dashboard with `Origin: null`; POST with no Origin; GET with `Origin: http://127.0.0.1:4777`; `OPTIONS` or `PUT` with `Origin: http://127.0.0.1:4777` or no Origin | POST with an app origin; GET with no Origin or `Origin: null` |
| Lifecycle: quit during readiness | quit while the readiness poll runs | the group is signalled and gone within the 5 s deadline; no child remains |
| Lifecycle: quit between spawn and recording the child | quit arrives after `spawn_grouped` returns and before `Spawned` is recorded | `stop_owned` waits for the record, signals the new group, and it is gone within the 5 s deadline; pidfile removed |
| Lifecycle: quit before the port lock is taken | `shutting_down` is set while the key is `Reserved` and its probe runs | the `ensure` refuses at lock-take under `inner`; no spawn; no pidfile |
| Lifecycle: concurrent first opens of one key | two `ensure` calls on one key with no slot | one lock and one spawn; the second waits on the slot's waiter, runs its own identity probe once the slot is `Owned`, and returns `Running` with the same URL, not an "another app instance" refusal |
| Lifecycle: readiness timeout | the readiness poll exhausts 10 s | the group is signalled, the pidfile removed, and `failed to start` returned with the log tail |
| Lifecycle: first open fails, second open waits | the readiness poll exhausts 10 s while a second `ensure` waits on the slot's waiter | the waiter returns the same `Refused`; one spawn only |
| Lifecycle: open while a failed start runs its epilogue | the child exits during readiness, the reaper marks the slot `Exited` with its waiter, and a new `ensure` arrives before the epilogue's `Refused` | the new `ensure` waits on that waiter and returns the same `Refused`; no second lock or spawn; no `dashboard-exited` is emitted |
| Lifecycle: waiter while the first open probes | a second `ensure` waits on a `Reserved` slot whose probe ends in an identity refusal, or in an External listener | identity: the waiter returns the same `Refused`; External: the waiter runs `ensure` again and embeds the listener; nothing spawned in either case |
| Lifecycle: child exits before its pidfile is written | the fake spawn's child exits at once, and the reaper marks the slot `Exited` before step 4 of the spawn sequence | no pidfile is written; `failed to start` with the log tail; the port lock is released after the reap |
| Lifecycle: owned child exits | the child exits while its frame is shown | the slot is `Exited`; `dashboard-exited` is emitted; the pidfile is removed; no group member remains, so nothing is signalled |
| Lifecycle: reopen right after an owned child exits | a new `ensure` on the key while the slot is `Exited` with no waiter and its exit cell is unset (the test's `emit` closure, which the reaper calls before `waitpid`, signals the test thread and then sleeps 200 ms) | the `ensure` waits on the exit cell, then drops the slot and starts one fresh server; no "another app instance" refusal |
| Lifecycle: quit while a reaper finishes | `stop_owned` runs while a slot is `Exited` with its exit cell unset (same `emit` delay) | `stop_owned` waits on that cell within the shared deadline and polls `group_alive(pgid)`; the group is gone and the port free before it returns |
| Lifecycle: spawn result with pgid not equal to pid | the fake spawn returns a child whose pgid differs from its pid | the epilogue signals the leader alone, never the group; the reaper skips the group kill; lock released after the reap; no pidfile; `failed to start` |
| Lifecycle: quit while a `Leader`-target Starting slot is alive | the fake spawn returns a child whose pgid differs from its pid, and `stop_owned` runs after `Spawned` is recorded and before the epilogue finishes | `stop_owned` signals the leader alone, never the group, and does not poll `group_alive` for it; the leader is reaped and quit returns without spending the 5 s + 1 s budget on that group |
| Lifecycle: stopped server | the fake server receives SIGSTOP while `Owned` | the reaper's `waitid` result for the stop is not accepted; the slot stays `Owned`; no `dashboard-exited`; `stop_owned` still signals its group; a stopped leader does not act on SIGTERM, so it dies from the SIGKILL sent at the 5 s deadline, and the group is gone within the 5 s + 1 s budget |
| Lifecycle: foreign listener before the child binds | the fake spawn's child never binds, and a test listener on the key's address answers `ready` without the dashboard's marker | readiness passes, the identity probe refuses with `identity`; the child is stopped; no pidfile; the listener is neither embedded nor signalled |
| Lifecycle: port lock after the reap | a port lock taken while the first instance's owned child is alive; then the child exits and is reaped | before the reap the second instance cannot take the lock; after the reap it takes the lock while the first instance's slot is still `Exited`, and starts its own server |
| Lifecycle: group member outlives the leader on quit | the fake server forks a member into its group that ignores SIGTERM, then the app quits | the leader exits on SIGTERM and is reaped; `group_alive(pgid)` is true, so the reaper sends SIGKILL to the group; within the 5 s deadline the group is gone and the port is free |
| Lifecycle: undeclared leftover | after a crash, the pidfile's process passes the four kernel checks but no declared server has its port, or the declared `binary` differs from its executable | pidfile removed; nothing signalled; the process keeps running |
| Lifecycle: second `stop_owned` | a second `ExitRequested` or `Exit` after the first | no-op; no signal sent |
| Lifecycle: crash, then launch | app killed with SIGKILL, then relaunched against the same pidfile and the same `dashboards.toml` | launch cleanup signals the leftover group, whose executable is the declared binary for the port; pidfile removed |
| Lifecycle: second instance | second app instance with the port lock held | the probe embeds the server; the second instance signals nothing on quit |
| Lifecycle: dead leftover | pidfile whose pid is dead | pidfile removed; nothing signalled |
| `bootout_dash`, `bootstrap_dash`, `remove_dash_plist` (`tests/schedule.rs`) | a label other than `cos.dash`; a plist path other than the exact ones; an old pid still alive after 10 s (the installed plist is not removed) | bootstrap only when the port is Free; removal of both files only when the port is Held by a pid other than the old pid, after the old pid exited; on Timeout, `.new` only |
| `install_dash_plist` (`tests/schedule.rs`) | the port is Held or Timeout; a move whose source or target is not one of the two exact plist paths | `.new` moved over the installed plist when the port is Free and the old pid has exited |

## Non-Functional Requirements

- **Security:** covered by the Semantic ISA and by §8. Stated gap: between an app crash (or a SIGTERM to the app) and the next launch, an owned server keeps running unsupervised on loopback.
- **Performance:** each watcher tick costs one `read_dir` per watched directory, one metadata call per run entry (`src/dash.rs:184-186`), and one `kill(0)` per live run. A full re-read runs only when a stamp changes. The probe has a 3 s timeout and runs only on open. The readiness poll runs every 250 ms for at most 10 s, only on open.
- **Process ownership:** every spawned server has an owner (the supervisor), a reaper thread that is its only waiter, a pidfile, a per-port lock released at the reap, and one shared exit deadline. After the reap, the reaper sends SIGKILL to any process left in the server's group. Launch cleanup stops leftovers whose executable is a declared server binary. Nothing is spawned on a render, tick or keystroke path.
- **Rollout and rollback:** `dashboards.toml` absent means no dashboard tabs. Beyond the sidebar, this build removes the `#other-live` button (its counts are on the rows), has the report frame post `brief` and `mtime`, writes `ui-state.json`, and adds the watcher stamps. Rolling back the app leaves `dashboards.toml`, `ui-state.json` and the `servers/` files behind. Older builds read none of them, so a server still running at rollback has no supervisor until its process exits. The registry format is unchanged.

## Confidence

Rock: understood well enough to bet. Sand: the shape is known, and it will move until a named check settles it. Water: unmapped; spike it first.

| Piece | Tier | What moves it up |
| --- | --- | --- |
| `dashboards.toml` parse and validation | rock | n/a |
| Sidebar rendering and switch | rock | n/a |
| Signals from metadata and kernel checks (`proc::exe`, `kill(0)`) | sand | the item 25 CUJ 8 test: a SIGKILL to a reaped supervisor turns the row lost within one tick. An unreaped (zombie) supervisor is an accepted gap (§Risks, `kill(0)` row) |
| Supervisor spawn, ready, stop on exit | sand | the fake-server test suite in Build |
| Port lock and launch cleanup after a crash | sand | a test that kills the app with SIGKILL and relaunches against the same pidfile, lock and `dashboards.toml`, and checks that the leftover group is signalled and gone, and that an undeclared leftover is not signalled |
| Group kill after the reap | sand | the item 23 test in which a group member ignores SIGTERM and is gone, with the port free, within the 5 s deadline |
| Reaper `waitid` on macOS (stopped child not taken for an exit) | sand | the item 23 test in which the fake server receives SIGSTOP, its slot stays `Owned`, and `stop_owned` still signals its group |
| Exit path: ⌘Q reaches `stop_owned` once | sand | a test that ⌘Q from the menu reaches `ExitRequested`, and that a second call is a no-op |
| Iframe embed of loopback pages inside `tauri://` | sand | Q1 passes: the frame loads, `window.__TAURI__` is undefined inside it, the gazette's frame headers allow `tauri://localhost`, and the Origin on `cosreport` POSTs is recorded |
| Identity probe markers for the two real pages and the gazette | sand | markers read from the live pages once the binary is built; recorded in the runbook |
| `task schedule` 8790 guard | rock | n/a |

## Detailed Implementation Plan

### Target Files to Create/Modify

#### 1. `src/dashboards.rs` (Create)

*   **Type / Scope**: cos library module.
*   **Proposed Changes**:
    *   Types: `Dashboards { servers: BTreeMap<String, Server>, business: BTreeMap<String, Vec<Dashboard>> }`, where `servers` is `#[serde(default, rename = "server")]`, since the TOML table is `[server.<name>]`, and `business` is `#[serde(default)]`, so a file with only unmanaged dashboards, or only servers, parses (as `src/business.rs:36` does for the registry), `Server { binary: PathBuf, addr: SocketAddrV4, ready: Probe }`, `Dashboard { name: String, url: String, server: Option<String>, probe: Probe }`, `Probe { path: String, contains: String }`. All with `#[serde(deny_unknown_fields)]`. `Dashboards` also derives `Default`, the empty value that setup passes to `launch_cleanup` when the load fails (item 9).
    *   `location(registry: &Path) -> PathBuf`: `COS_DASHBOARDS`, else the directory of `registry` joined with `dashboards.toml`. The CLI passes `business::location()`; the desktop passes the registry it started with: `start.registry` in `setup`, and `AppState.registry` in the `dashboards` and `dashboard_open` commands (§5).
    *   `load(path) -> Result<Dashboards>`: a missing file gives empty; otherwise parse and validate per §3, including unique server `addr` ports, no unmanaged dashboard on a declared server's port, ports from 1 to 65535, and `probe.path` and `ready.path` matching `^/[!-~]*$`. Errors name the TOML key path (both keys for a duplicate port).
    *   `for_business(&self, name) -> &[Dashboard]` and `server_of(&self, d) -> Option<&Server>`.
    *   `probe(addr, probe, timeout) -> ProbeResult`: a plain `TcpStream` HTTP/1.0 GET whose request line is `GET <probe.path> HTTP/1.0`, with the path already validated by `load`; reads up to 64 KiB, `matched` when the body contains the marker. No new HTTP client dependency.
    *   `orphans(&self, registry) -> Vec<String>`: business keys not in the registry.

#### 2. `src/signals.rs` (Create)

*   **Type / Scope**: cos library module; reads metadata and kernel state only; no process spawn.
*   **Proposed Changes**:
    *   `pub struct Signals { pub live: usize, pub lost: usize, pub starting: usize, pub supervisors: Vec<i32>, pub latest_brief: Option<(String, i64)> }`.
    *   `pub fn read(home: &Path) -> Signals`: `live`, `lost` and `starting` from `agents::count`; `supervisors` from `agents::live_supervisors` (item 3); `latest_brief` is the maximum `out/briefs/YYYY-MM-DD.json` filename, with its mtime in ms. `pub fn mtime_ms(meta: &std::fs::Metadata) -> i64` returns whole milliseconds since the Unix epoch, or 0 when the time is unavailable or predates the epoch. `store::latest` and `report::data` use the same function.
    *   `pub fn scheduled(plist: &Path, registry: &Path) -> Option<String>`: reads the plist with the `plist` crate. Returns `None` for a parse error, no `--business`, or a registry mismatch.
    *   `pub fn stamp(home: &Path) -> String`: `dash::stamp([agents::root(home), home.join("out/briefs")])`.

#### 3. `src/agents/mod.rs` (Modify)

*   **Type / Scope**: cos library.
*   **Proposed Changes**: new `pub fn live_supervisors(home: &Path) -> Vec<i32>`: the `state.supervisor` pids of the runs that `count` would put in `live` as `Liveness::Live` (Starting runs have no pid), with the same filters as `count` (canonical root, directory check, readable spec and state through `read_json`, id match, non-terminal status) and no `reconcile`, so nothing is written and nothing is signalled. `list` (`:521`) is not usable here: it goes through `read`, `load` and `reconcile` (`:498-506`), and `run_ids` (`:509`) and `read_json` (`:758`) are private. `Counts` stays `Copy` (`:552`), so the pids are not added to it. `is_cos(pid)` (`:804-807`) reads the executable with `proc::exe(pid)` and compares its file name with `cos`. It no longer calls `comm` (`:794-801`), whose only caller is `is_cos` (`:806`), so `comm` is deleted: its `ps -o comm=` lookup moves into the off-macOS fallback of `proc::exe` (item 4), and leaving it would fail clippy `-D warnings` on `dead_code`. Therefore `classify` (`:543-548`), `count` (`:561-592`) and every other caller of `is_cos` (`:675`, `:707`) spawn no `ps`. `Counts` (`:553-558`) gains `pub starting: usize`, and `count` fills it beside `live` and `lost`; `live` still includes Starting runs. `started_at` (`:810-817`) is unchanged: it serves the process-group identity at `:824` and `src/agents/supervise.rs:98`, and is not on the refresh path.

#### 3a. `src/agents/slots.rs` (Modify)

*   **Type / Scope**: cos library, lock helper.
*   **Proposed Changes**: `lock_exclusive` (`:51`) becomes `pub`, so the desktop `ui_state` helper takes `<ui_state_file>.lock` with it (§6). `Held` (`:36`) and the `slots` module (`src/agents/mod.rs:32`) are already `pub`.

#### 3b. `tests/agents.rs` (Modify)

*   **Type / Scope**: cos integration tests.
*   **Proposed Changes**: the four exhaustive `agents::Counts { live, lost, unreadable }` literals (`:686`, `:704`, `:721`, `:756`) gain `starting`. A new case counts one Starting run as `starting: 1` and `live: 1`.

#### 4. `src/proc.rs` (Modify)

*   **Type / Scope**: cos library, process helpers.
*   **Proposed Changes**: add next to `alive` (`:247`):
    *   `pub fn exe(pid: libc::pid_t) -> Option<PathBuf>` via `libc::proc_pidpath`;
    *   `pub fn start_time(pid: libc::pid_t) -> Option<(i64, i64)>` via `libc::proc_pidinfo(PROC_PIDTBSDINFO)`, returning `(pbi_start_tvsec, pbi_start_tvusec)`;
    *   `pub fn pgid(pid: libc::pid_t) -> Option<libc::pid_t>` via `libc::getpgid`, for launch cleanup's `pgid == pid` check and the stop target of a new child (§4 Slots);
    *   `pub fn signal_group(pgid: libc::pid_t, signal: libc::c_int)`, a `kill(-pgid, signal)` with the same guard as `kill_group`;
    *   `pub fn signal_pid(pid: libc::pid_t, signal: libc::c_int)`, a `kill(pid, signal)` for `pid > 0`, used for a `Leader` stop target only, by the failure epilogue and by `stop_owned` (§4).

    `kill_group` (`:237-244`) stays SIGKILL only. The reaper's group kill after the reap uses the existing `group_alive` (`:253-256`) and `kill_group`. `proc_pidpath` and `proc_pidinfo` are macOS-only and sit behind `#[cfg(target_os = "macos")]`. Off macOS, `exe` falls back to `ps -o comm= -p <pid>`, the lookup `is_cos` uses today through `comm` (`src/agents/mod.rs:794-801`, deleted by item 3), and `start_time` returns `None`, so launch cleanup removes the pidfile and signals nothing. `VERIFY(binary_admissible)` refuses every spawn when `start_time` returns `None` (rule `platform`), so no child starts that `VERIFY(spawn_result)` would refuse. `pgid` uses `getpgid` on every platform.

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
    *   `pub struct Supervisor { inner: Arc<Mutex<Inner>>, data_dir: PathBuf, log_dir: PathBuf, emit: Arc<dyn Fn(DashboardExited) + Send + Sync>, spawn: SpawnFn }`, managed state, where `pub struct Inner { slots: HashMap<SocketAddrV4, Slot>, shutting_down: bool }`. `slots` and `shutting_down` are read and written only under `inner`. The `Arc` lets reaper threads reach `inner`. `#[derive(Clone, Serialize)] pub struct DashboardExited { addr: SocketAddrV4, status: String, log_tail: String }` is the `dashboard-exited` payload; `AppHandle::emit` needs `Serialize + Clone`, and `addr` serialises as the string `127.0.0.1:<port>` that dash.js compares with the open dashboard's host and port. `SpawnFn` is `Box<dyn Fn(&Server, &Path) -> io::Result<SpawnResult> + Send + Sync>`, where the `&Path` is the log path from `VERIFY(log_path)`, and `SpawnResult { child: Child }`. The app uses `pub fn real_spawn(server: &Server, log: &Path) -> io::Result<SpawnResult>`, which builds the `Command` (argv, working directory, inherited environment, stdin null, stdout and stderr appended to the log that `ensure` has already truncated), calls `proc::spawn_grouped` (`src/proc.rs:165`, `anyhow::Result<Child>`), and maps its error with `io::Error::other`. Tests use a fake that returns the `Child` of a test binary.
    *   `pub struct Waiter { cv: Condvar }`, waited on with the `inner` guard. `pub struct Exit { status: Mutex<Option<ChildExit>>, done: Condvar }` is the exit cell of §4 Slots, shared as `Arc<Exit>` by the slot, the reaper, the failure epilogue and `stop_owned`.
    *   `#[derive(Clone, Copy)] pub enum StopTarget { Group(pid_t), Leader(pid_t) }` (§4 Slots, Stop target), fixed at ensure: start step 1 from `proc::pgid(pid)`: `Group(pid)` for `Some(pid)`, `Leader(pid)` for another pgid or `None`. `#[derive(Clone, Copy, Serialize)] pub enum ChildExit { Code(i32), Signal(i32) }` (§4 Slots, Child exit), built from `siginfo_t`: `CLD_EXITED` gives `Code(si_status)`, `CLD_KILLED` and `CLD_DUMPED` give `Signal(si_status)`; its `Display` (`exit <n>`, `signal <n>`) fills `DashboardExited.status`.
    *   `pub enum Slot { Reserved(Arc<Waiter>), Starting(Starting), Owned(Owned), Exited { status: ChildExit, target: StopTarget, exit: Arc<Exit>, waiter: Option<Arc<Waiter>> }, Refused(Refusal) }`, with `pub struct Starting { waiter: Arc<Waiter>, lock: File, child: Option<Spawned> }`, where `pub struct Spawned { pid: pid_t, target: StopTarget, exit: Arc<Exit> }` is set right after the spawn returns, in the critical section that starts the reaper; `stop_owned` signals a `Starting` slot only when `child` is set, and only through its `target`. Also `pub struct Owned { pid: pid_t, target: StopTarget, exit: Arc<Exit>, lock: File }`, whose `target` is always `Group`, since `VERIFY(spawn_result)` refuses `Leader`. `Exited` holds no `File`: the reaper takes the lock out of the slot when it marks it `Exited`, and drops it after the reap and the group check, just before it sets `exit`. `Exited.exit` and `Exited.target` are carried over from the slot, so `ensure: owned` and `stop_owned` can wait for that reaper and check the group of a `Group` target. `Exited.waiter` is `Some` only when the reaper marked a `Starting` slot. There is no `External` variant.
    *   `fn reap(inner: Arc<Mutex<Inner>>, addr: SocketAddrV4, mut child: Child, target: StopTarget, pidfile: PathBuf, log: PathBuf, emit: Arc<dyn Fn(DashboardExited) + Send + Sync>, exit: Arc<Exit>)`: the reaper thread body of §4. In order: `waitid(P_PID, pid, WEXITED | WNOWAIT)` in a loop until `si_code` is `CLD_EXITED`, `CLD_KILLED` or `CLD_DUMPED`, and the `ChildExit` built from that `siginfo_t`; under `inner`, replace `Owned` with `Exited { status, target, exit, waiter: None }`, or `Starting` with `Exited { status, target, exit, waiter: Some(waiter) }` and `notify_all` on that waiter, take its `lock`, remove the pidfile; emit only when the slot was `Owned`; `waitpid`; if `target` is `Group(pgid)` and `proc::group_alive(pgid)`, `proc::kill_group(pgid)` (`VERIFY(group_after_reap)`); drop the lock; set `exit.status` and `notify_all` on `exit.done`.
    *   `pub fn new(data_dir: PathBuf, log_dir: PathBuf, emit: Arc<dyn Fn(DashboardExited) + Send + Sync>, spawn: SpawnFn) -> io::Result<Supervisor>`. It calls `create_dir_all` on `data_dir/servers` and `log_dir/servers` before it returns, since `atomic_write_bytes` does not create the parent (`src/state.rs:555-569`). The app passes an `emit` that calls `AppHandle::emit("dashboard-exited", ...)` and `Box::new(servers::real_spawn)`; tests pass a recording closure and a fake spawn.
    *   `pub fn launch_cleanup(&self, dashboards: &Dashboards)`: the steps in §4, run once in `setup`. It builds the `declared` port-to-binary map for `VERIFY(ownership)` from `dashboards.servers`; an empty `Dashboards` (missing or invalid file) declares nothing, so every pidfile is removed without a signal.
    *   `pub fn ensure(&self, server: Option<&Server>, d: &Dashboard) -> Result<Running, Refusal>`: the named steps in §4 (ensure: owned, reserve, probe, start, with the identity probe after readiness); ensure: owned runs `d`'s identity probe outside `inner` before it returns `Running` for an `Owned` slot, and a waiter whose slot ends `Owned` runs ensure: owned again, so every `Running` has passed `d`'s own marker in that call; ensure: owned waits up to 5 s on the exit cell of an `Exited` slot with no waiter before it drops the slot. The slot key is the host and port of `d.url`. `#[derive(Clone, Serialize)] pub struct Running { url: String, owned: bool }` carries the URL that `ensure` verified, and is the `Ok` value of `dashboard_open`. The reaper emits `dashboard-exited` for an `Owned` slot only (§4 Reaper).
    *   `pub fn stop_owned(&self)`: the stop path in §4, including the wait on `Exited` slots whose exit cell is unset. Idempotent.
    *   `#[derive(Clone, Debug, Serialize)] #[serde(tag = "kind")] pub enum Refusal { Identity { status: String, head: String }, BinaryMissing { path: PathBuf }, NotAdmissible { rule: &'static str, path: PathBuf }, FailedToStart { log_tail: String }, NotRunning { hint: String }, StaleBusiness { message: String }, UnknownDashboard { name: String }, InvalidConfig { message: String }, Internal { message: String } }`, serialised to the pane as an object whose `kind` is the variant name and whose other keys are its fields, for example `{"kind":"Identity","status":"HTTP/1.0 200 OK","head":"…"}` or `{"kind":"FailedToStart","log_tail":"…"}`; dash.js switches on `kind`. `Clone` lets every waiter on a `Refused` slot return its own copy of the slot's refusal (§4 ensure: owned). The Semantic ISA refusals map onto these variants: `VERIFY(identity)` to `Identity`, `VERIFY(binary_admissible)` to `BinaryMissing` or `NotAdmissible`, and `VERIFY(readiness)` and `VERIFY(spawn_result)` to `FailedToStart`. `ensure` returns the first five; `dashboard_open` returns the last four (§5). The pane renders `BinaryMissing` as the path plus the fixed hint "build it by hand; the Setup Runbook has the command" (§4 ensure: start); no build command is derived from the path.
    *   `fn admissible(server: &Server) -> Result<(), Refusal>`: `VERIFY(binary_admissible)`.

#### 9. `apps/desktop/src/lib.rs` (Modify)

*   **Type / Scope**: desktop commands, scheme handler, watcher, builder.
*   **Proposed Changes**:
    *   `pub mod servers;` in `apps/desktop/src/lib.rs`, so `tests/servers.rs` and the builder reach `Supervisor`.
    *   `BusinessRow` (`:379-388`) gains `unread: bool` and `scheduled: bool`, and derives `Clone` and `PartialEq` for the shared cache's comparison, as do `Present` (`:363-364`) and `Live` (`:370-371`), which derive only `Serialize` today; `Business` already derives both (`src/business.rs:19`). The one row builder is the new synchronous `fn rows(registry: &Path, current: Option<&str>, ui_state_file: &Path, only: Option<&str>) -> Res<Vec<BusinessRow>>` (§6 Refresh), built from the map in `businesses()` (`:424-438`); it spawns nothing. `businesses()` (`:419-440`) copies `registry`, `triple.business` and `ui_state_file` under its lock and calls `rows` inside `blocking`. New managed state `Rows(Mutex<BTreeMap<String, BusinessRow>>)` and `fn publish<R: Runtime>(app: &AppHandle<R>, rows: Vec<BusinessRow>)`, which updates the cache and emits `businesses-changed` with all cached rows under the `Rows` mutex when any row changed (§6 Refresh); the watcher and `brief_seen` are its only callers.
    *   New commands, all registered in `generate_handler!` (`:983`): `dashboards(business) -> Result<Vec<DashboardRow{name, url, managed}>, String>`, `dashboard_open(app: AppHandle<R>, business, name) -> Result<Running, Refusal>` (async; the steps in §5: `named_st` under one `lock()`, then a closure moved into `blocking` (`:275-282`) that loads `Dashboards`, resolves `name`, and calls `app.state::<Supervisor>().ensure(..)`, with `T = Result<Running, Refusal>`), `brief_seen(business, file, mtime)`. The three take `app.lock()` once and check the business with the new `fn named_st(st: &AppState, business: &str) -> Res<()>`, which compares `st.triple.label()` and returns `named()`'s error text, then copy what they need under the same guard: `registry` and `triple.business` for `dashboards` and `dashboard_open`, and `registry`, `triple.business`, `triple.home` (whose `out/briefs` `VERIFY(brief_seen)` checks) and `ui_state_file` for `brief_seen`. `dashboards` returns an empty list and `dashboard_open` returns `Refusal::UnknownDashboard` when `triple.business` is `None` (§5); none calls `App::get` while holding `lock()`, which would deadlock on the same mutex (`:239-244`). `dashboard_open` maps the error to `Refusal::StaleBusiness { message }`, and the other two return it as a `String`. `dashboards` and `dashboard_open` load `dashboards.toml` from `dashboards::location(&st.registry)` on every call, with `st` the `AppState` under the `App` lock; nothing is cached. `dashboard_open` maps a load error to `Refusal::InvalidConfig`, an undeclared name to `Refusal::UnknownDashboard`, and a `blocking` join error to `Refusal::Internal`.
    *   `watch()` (`:891-909`): per §6. It keeps stamps and `supervisors` sets, no rows; it publishes recomputed rows through `publish` and emits `dashboards-changed`, and keeps `report-changed` and `agents-changed`. `dashboard-exited` is emitted by the reaper only (§4).
    *   `report_response` for `/` (`:451`): `cos::report::data` returns a `serde_json::Value` (`src/report.rs:36`), so the `/` arm binds it mutably and sets the flag before rendering: `.and_then(|mut d| { d["hostBrief"] = serde_json::Value::Bool(true); cos::report::render(&d) })`, replacing the `.and_then(|d| cos::report::render(&d))` at `:451-452`. No other path sets it.
    *   `origin_allowed(req: &Request<Vec<u8>>) -> bool`, called first in the `cosreport` handler (`:963-974`), which answers 403 when it returns false.
    *   `builder()` clones the `Option<Start>` it is given (`Start` derives `Clone`, `apps/desktop/src/lib.rs:27`) into the `setup` closure. The `Some` branch still manages `App::new(&s)` before setup (`:958-959`). In `setup`, the `Start` is that clone, or, when it is `None`, the `Start::from_env` result whose `App` setup manages today (`:976-978`). From that one `Start`, setup manages `Supervisor::new(start.data_dir, start.log_dir, emit, Box::new(servers::real_spawn))?`, where `emit` wraps `AppHandle::emit` and `?` propagates a directory-creation failure as `Start::from_env` does (`:977`). Both branches therefore build the supervisor, and `tests/ipc.rs` reaches `dashboard_open` with a managed `Supervisor`. Setup then calls `launch_cleanup(&dashboards)` with `dashboards::load(&dashboards::location(&start.registry))`, or an empty `Dashboards` when that load fails, after logging the error once. `Start` gains `data_dir`, `log_dir` and `ui_state_file`. `from_env` fills them from `app.path()` (`apps/desktop/src/lib.rs:54` reads `app_config_dir` today), and tests fill them from a tempdir, as `settings_file` is filled in `apps/desktop/tests/ipc.rs:86-108`. `AppState` (`:80-88`) keeps only `registry` from `Start` today, so it gains `ui_state_file: PathBuf`, set by `AppState::boot` (`:148`), which `App::new` calls (`:236`). The supervisor reads its directories from `Start` and `ui_state` reads `AppState.ui_state_file`, never `app.path()`.
    *   `pub fn on_run_event<R: Runtime>(app: &AppHandle<R>, event: RunEvent)` calls `stop_owned()` on `ExitRequested` and `Exit`.
    *   `brief_seen`, after `VERIFY(brief_seen)` and the write, calls `rows(.., only = Some(business))` and `publish` (§6).
    *   `ui_state` helper: calls `create_dir_all` on the parent of `AppState.ui_state_file`, then takes `<ui_state_file>.lock` with `cos::agents::slots::lock_exclusive` (made `pub` in item 3a), and reads and writes the file with `atomic_write_bytes` (§6).

#### 9a. `apps/desktop/examples/preview.rs` (Modify)

*   **Type / Scope**: desktop example, built by `cargo test --workspace` and clippy `--all-targets`.
*   **Proposed Changes**: the `cos_desktop::Start` struct literal (`:61-69`) gains `data_dir`, `log_dir` and `ui_state_file`, filled from the example's temporary home (`home.join("data")`, `home.join("logs")`, `home.join("ui-state.json")`).

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
*   **Proposed Changes**: `renderSidebar(rows)` builds one button per row with the live count, unread dot, lost warning and clock. Each signal has text for screen readers. A click calls `window.switchTo(name)`; a rejected call shows its message in a status line below the rows. It listens for `businesses-changed` and loads initial rows via `invoke("businesses")`. It reads `pinned` from `invoke("paths")`; when it is set, rows are disabled and titled `pinned by <var>`, the header's `#biz-pin` text (`apps/desktop/ui/app.js:99-100`).

#### 14. `apps/desktop/ui/dash.js` (Create)

*   **Type / Scope**: UI module.
*   **Proposed Changes**: `loadDashTabs()` invokes `dashboards` with `ui.business` and renders tabs; its one startup call site is `openFromHash`, which awaits it for every hash (item 15). It also listens for `dashboards-changed` and calls `loadDashTabs()` again. `openDash(name)` shows "starting…", invokes `dashboard_open`, then sets `#dash-frame.src` to the `url` in Running, or renders the refusal card (reason, port, what answered, next command). `window.hideDash()` clears `#dash-frame.src`; `openTab` calls it whenever it opens a view other than `dash` (item 15). It listens for `dashboard-exited` and shows `exited` with the payload's `log_tail`. Restoring from the hash renders the idle state and calls no `dashboard_open`.

#### 15. `apps/desktop/ui/app.js` (Modify)

*   **Type / Scope**: UI.
*   **Proposed Changes**: expose `switchTo` on `window`; `openTab(view, pane)` (`:124-128`) becomes `openTab(view, pane, dash)` and, for the dash view, marks only the tab whose `data-dash` equals `dash` (§5); for any other view it calls `window.hideDash()` (item 14); `switchTo` (`:166-168`) writes `tab=dash&dash=<d>`, with `<d>` the `data-dash` of the `aria-current` dashboard tab, not only `tab` and `pane`; route the dashboard view in the existing view switcher; load `sidebar.js` and `dash.js`. Remove the `#other-live` code (`:1121-1135`), its call at `:141`, its click listener (`:1151`), the `businesses-loaded` listener (`:1152`) and the dispatch in `apps/desktop/ui/command.js:103`. In the `cos-report-ready` handler (`:193-195`), call `brief_seen(business, file, mtime)` only while Today is the visible view. The handler keeps the last ready payload that has a `brief` field; the switch to Today calls `brief_seen` with it. A message without a `brief` field is not passed to `brief_seen` and is not kept; the handler still sends the `cos-host {hire: true}` reply (`:194`) for every `cos-report-ready`. `VIEWS` (`:118`) gains `dash`, so the view list is today, agents, flows, docs, dash. `openFromHash` (`:148-151`) becomes async and awaits `loadDashTabs()` for every hash, so the dashboard tabs render on every launch, including one with no hash or a `today` or `agents` hash; the startup chain `loadPaths().then(loadRepos).then(openFromHash)` (`:1154`) is unchanged, so `ui.business` is set first. It then routes the hash; for the dash view it shows the view idle: the tabs render, the dashboard named by `dash` is selected but not opened, and no `dashboard_open` runs. A `dash` name the app business does not declare, including a business with no dashboards, opens Today (§5).

#### 16. `apps/desktop/ui/style.css` (Modify)

*   **Type / Scope**: styles.
*   **Proposed Changes**: the body becomes a row: sidebar of fixed width `12.5rem` (`tests/design.rs:288-308` admits no pixel length except 1px and 2px), then the main column; row states (app business, hover, disabled); signal glyphs; the refusal card. It uses only the existing tokens.

#### 17. `src/report.html` (Modify)

*   **Type / Scope**: report frame.
*   **Proposed Changes**: `cos-report-ready` (`:455`) carries `brief: "<file>"` and `mtime`, the filename and mtime of the brief the report rendered, from `DATA.briefRef.file` and `DATA.briefRef.mtime`. Only the app's `/` report sends them, when `DATA.hostBrief` is true (set by `report_response` for `/`, item 9). The `live` flag (`src/report.rs:35-36`) does not gate them.

#### 17a. `src/store.rs` (Modify)

*   **Type / Scope**: cos library, saved outputs.
*   **Proposed Changes**: `latest` keeps only names that match `YYYY-MM-DD.json`, the pattern its module comment already states (`src/store.rs:1`). It opens the chosen JSON once and takes the mtime from that handle with `mtime_ms` before and after reading the bytes. When the two differ, it reads again from a new handle, at most three attempts, and then returns an error as a parse failure does, because `save` and `save_json` rewrite the file in place (`src/store.rs:32,54`). `Saved` gains `mtime_ms: i64`.

#### 17b. `src/report.rs` (Modify)

*   **Type / Scope**: report render.
*   **Proposed Changes**: `data()` puts `briefRef: {file, mtime}` into the data it renders, with `file` the brief's filename and `mtime` filled from `Saved.mtime_ms`; §6 and item 17 use the same names. `hostBrief` is set by `report_response` in the desktop crate (item 9), never by `data()` or `build` (`src/report.rs:24-31`). Test: an archived render has no `hostBrief`, and its page posts no `brief` message.

#### 18. `Taskfile.yml` (Modify)

*   **Type / Scope**: schedule task.
*   **Proposed Changes**: the four `for: [brief, dash]` loops (`:211-222`) become `for: [brief]`, so no step outside the `cos.dash` sequence renders, moves or bootstraps `cos.dash`. The `cos.brief` steps run first, then the `cos.dash` steps of §7, in that order: render to `.new` without moving it; read the old pid from `{{.LAUNCHCTL}} print gui/$(id -u)/cos.dash 2>/dev/null || true` (empty when the job is not loaded); bootout with `{{.LAUNCHCTL}} bootout ... 2>/dev/null || true`, as `:220` does today; wait up to 10 s, polling every 250 ms, for the old pid to exit; then, with `lsof -nP -iTCP:$DASH_PORT -sTCP:LISTEN`, either move `.new` into place and bootstrap (`Free`), print the holder's command and pid, skip `cos.dash`, and delete `.new` and the installed plist (`Held`), or on `Timeout` delete `.new` and fail. `DASH_PORT` is a test-only seam that defaults to 8790. `cos.brief` loads either way.

#### 19. `tests/dashboards.rs` (Create)

*   **Type / Scope**: cos integration tests.
*   **Proposed Changes**: parse and validation cases:
    *   non-loopback URL refused;
    *   unknown server refused;
    *   port mismatch refused;
    *   relative binary refused;
    *   unknown key refused;
    *   marker shorter than 12 characters refused; markers matching `^<[^<>]*>$` after trim refused (`<title>`, `<div class=x>`, the placeholder `<measured in Build>`); `<title>Wolven portfolio control room` and `<script src="/sales.js"` admitted;
    *   missing file reads as empty;
    *   orphan key reported;
    *   `cos business dashboards` output against a std `TcpListener` fixture that answers with and without the marker;
    *   a `[server.<name>]` table parses (the `servers` rename);
    *   a file with only `[[business.longhand]]` and no `[server.*]` table parses with empty `servers`;
    *   two `[server.*]` tables with the same `addr` port refused, the error naming both keys;
    *   an unmanaged dashboard whose URL port equals a declared server's `addr` port refused;
    *   a `probe.path` or `ready.path` containing `\r\n`, a space, or not starting with `/` refused, with the key path; `/`, `/sales` and `/health` admitted;
    *   an `addr` of `127.0.0.1:0` and a URL `http://127.0.0.1:0/` refused.

#### 20. `tests/signals.rs` (Create)

*   **Type / Scope**: cos integration tests.
*   **Proposed Changes**:
    *   live counted through `proc::exe`, with no `ps` spawned (asserted by putting a failing `ps` first on `PATH`), under `#[cfg(target_os = "macos")]` because off macOS `exe` falls back to `ps` (item 4);
    *   lost counted;
    *   starting counted, and included in live;
    *   `supervisors` holds the pid of a Live run and not the dead pid of a Lost run, and reading it writes no `state.json` (no `reconcile`);
    *   latest brief by filename, with names outside `YYYY-MM-DD.json` ignored;
    *   `mtime_ms` of one file equals the `mtime` the report payload carries (one helper, one unit);
    *   `scheduled` returns the business only when `COS_BUSINESSES` resolves to the same registry; returns `None` for a legacy plist and a mismatched registry.

#### 21. `tests/business.rs` (Modify)

*   **Type / Scope**: cos integration tests.
*   **Proposed Changes**: a nested home warns and registers, and `add` returns the outer name; an equal home is still refused.

#### 22. `tests/schedule.rs` (Modify)

*   **Type / Scope**: cos integration tests.
*   **Proposed Changes**: with a `TcpListener` on a free ephemeral port passed as `DASH_PORT`, `task schedule` loads `cos.brief`, skips `cos.dash`, prints the holder, and leaves no `cos.dash.plist` in the LaunchAgents directory. A second case with no listener asserts that `cos.dash` is bootstrapped after the old pid is gone. A third case, with a fake `launchctl` whose old pid stays alive, asserts that the `cos.brief` steps ran, the task fails, `.new` is removed, and the installed `cos.dash.plist` is unchanged. A fourth case, with no job loaded, asserts that the empty old pid skips the wait and the task does not fail. The fake `launchctl`, reached through the `{{.LAUNCHCTL}}` seam, prints a `pid = N` line for `print` while the job is loaded and exits non-zero when it is not. A text scan of `Taskfile.yml` asserts that the `cos.dash` steps name only the label `cos.dash` and the two exact plist paths (`VERIFY(dash_target)`).

#### 23. `apps/desktop/tests/servers.rs` (Create, `harness = false`)

*   **Type / Scope**: desktop integration tests for the supervisor. The test binary doubles as the fake server: run as `<self> serve <addr>`, it binds `addr` and answers `/health` and `/` with a marker; run as `<self> serve-with-member <addr>`, it does the same after forking one child that stays in the server's process group and ignores SIGTERM; run as `<self> idle <addr>`, it binds nothing and sleeps until signalled.
*   **Proposed Changes**: one case per ISA matrix row for `spawn_server`, `signal_group` and `STORE(pidfile)`, and per lifecycle row:
    *   one process for two dashboards on one server, and the second open's `Running` follows a passing identity probe with the second dashboard's own marker;
    *   a second dashboard on an `Owned` slot whose marker the fake server's page lacks is refused with `identity`; the slot stays `Owned`, nothing is signalled, and the first dashboard still opens;
    *   `stop_owned` frees the port and leaves no child;
    *   a second `stop_owned` is a no-op;
    *   an external listener is never signalled;
    *   a pidfile with a wrong start time, wrong executable, or wrong process group is removed without a signal;
    *   a matching pidfile from a dead app, whose executable is the declared `binary` for its port (the test binary, declared in the test's `Dashboards`), has its group signalled by launch cleanup, and its pidfile is removed;
    *   a pidfile that passes the four kernel checks is removed without a signal when no declared server has its port, when the declared `binary` is another file, and when `launch_cleanup` gets an empty `Dashboards`;
    *   `Supervisor::new` on a fresh tempdir creates `servers/` under the data and log dirs, and the first spawn writes its pidfile and log there;
    *   a pidfile whose port lock is held by another instance is left alone;
    *   a server that never becomes ready has its group signalled, and its log tail is returned;
    *   quit during the readiness poll stops the group within the 5 s deadline;
    *   quit between spawn and recording the child stops the new group;
    *   an owned child that exits emits `dashboard-exited`, and its slot becomes `Exited`;
    *   the first open of a key fails while a second open waits on its waiter: the waiter returns the same `Refused`, and only one spawn happens;
    *   a second open that arrives after the reaper marked a `Starting` slot `Exited` and before its epilogue's `Refused` waits and returns that `Refused`; no second spawn; no `dashboard-exited`;
    *   a second open waiting on a `Reserved` slot returns the same `Refused` when the probe refuses on identity, and embeds when the probe finds an External listener;
    *   a fake spawn whose child exits at once leaves no pidfile, returns `failed to start`, and frees the port lock after the reap;
    *   the success path wakes a waiter, which runs its own identity probe and returns `Running` for its dashboard's URL;
    *   a spawn whose result fails `VERIFY(spawn_result)` (fake spawn) is stopped, its lock released, no pidfile written, and `failed to start` returned;
    *   the `dashboard-exited` payload carries `log_tail`, and the reaper removes the pidfile at exit;
    *   quit before the port lock is taken: no spawn and no pidfile;
    *   two concurrent first opens of one key: one lock, one spawn, and the second returns `Running` with the same URL after its own identity probe;
    *   once the reaper reaps an owned child, the port lock is free: a second `Supervisor` on the same data dir takes it while the first one's slot is still `Exited`;
    *   the fake server, run as `<self> serve-with-member <addr>`, forks a member into its group that ignores SIGTERM; on `stop_owned` the reaper sends SIGKILL to the group after the reap, and the group is gone and the port free within the 5 s deadline;
    *   reopen right after an owned child exits: the test's `emit` closure signals the test thread and sleeps 200 ms, so the slot is `Exited` with its exit cell unset; the new `ensure` waits on the cell and starts one fresh server, with no "another app instance" refusal;
    *   `stop_owned` while a slot is `Exited` with its exit cell unset (same `emit` delay) waits on that cell and returns with the group gone and the port free;
    *   a fake spawn whose child's pgid differs from its pid: the epilogue signals the leader alone, the reaper skips the group kill, the lock is released after the reap, no pidfile, `failed to start`;
    *   quit while a `Leader`-target `Starting` slot is alive (fake spawn with pgid different from pid, `stop_owned` called between `Spawned` and the end of the epilogue): only the leader is signalled, `group_alive` is not polled for it, and `stop_owned` returns once the leader is reaped;
    *   SIGSTOP to the owned fake server: the slot stays `Owned`, no `dashboard-exited` is emitted, and `stop_owned` signals its group; the stopped leader dies from the SIGKILL at the 5 s deadline, and the test asserts the group is gone within the 5 s + 1 s budget;
    *   a fake spawn whose child never binds, while a test listener on the key's address answers `/health` with the `ready` marker and `/` without the dashboard's marker: `identity` refusal, the child stopped, no pidfile, the listener not signalled;
    *   the log of the first spawn is at `<log dir>/servers/<port>.log` for the validated port, and a log of more than 1 MiB written before the spawn is truncated to its last 1 MiB by `ensure` before the fake `SpawnFn` runs (`STORE(server_log)`);
    *   the `<port>.lock` file keeps its inode across a spawn, a reap and launch cleanup, and `servers/abc.json` and `servers/70000.json` entries are ignored by launch cleanup (`STORE(port_lock)`, `VERIFY(pidfile_path)`);
    *   a `servers/04777.json` pidfile copied from a live `4777.json` while another `Supervisor` holds `4777.lock` is ignored by launch cleanup: nothing signalled, the other instance's server keeps running;
    *   a `servers/4777.json` whose record `addr` port is 4778 is removed by launch cleanup without a signal;
    *   second instance: with the first `Supervisor` owning the server, a second `Supervisor` on the same data dir embeds it as External through the probe, and its `stop_owned` signals nothing;
    *   a pidfile whose pid is dead is removed by launch cleanup, and nothing is signalled.

#### 24. `apps/desktop/Cargo.toml` (Modify)

*   **Type / Scope**: manifest.
*   **Proposed Changes**: `[[test]] name = "servers" harness = false`; add `libc = "0.2"`, which the desktop crate does not have yet. `cos` already depends on it.

#### 25. `apps/desktop/tests/ipc.rs` (Modify)

*   **Type / Scope**: desktop IPC tests.
*   **Proposed Changes**:
    *   `businesses` rows for three businesses, with the unread dot set and then cleared by `brief_seen`, and the clearance surviving a new `App` instance;
    *   `dashboard_open` refused for a stale business;
    *   `dashboard_open` returns `UnknownDashboard` for a name not declared for the app business, and `InvalidConfig` with the key path when `dashboards.toml` is invalid;
    *   a dashboard added to `dashboards.toml` after launch is returned by the next `dashboards` call and opened by the next `dashboard_open` (no cache);
    *   `brief_seen` refused for a stale business, for a non-date filename, and for a filename outside `out/briefs`;
    *   `cosreport` answers 403 to a POST with a foreign Origin, a POST with `Origin: null`, a POST with no Origin, and a GET with `Origin: http://127.0.0.1:4777`; and 200 to a POST with `Origin: tauri://localhost`, a GET with `Origin: null`, and a GET with no Origin;
    *   `businesses` does not spawn `ps` (failing `ps` on `PATH`), under `#[cfg(target_os = "macos")]` (item 4);
    *   `brief_seen` on a tempdir whose config directory does not exist creates it, takes the lock, and clears the dot;
    *   CUJ 8: a SIGKILL to a run's supervisor without a file change turns the row to lost within one tick; the test reaps the supervisor it kills, as `agents::spawn` does (`src/agents/mod.rs:482-485`).
    *   `brief_seen` emits `businesses-changed`, and the emitted payload shows the dot cleared (the test observes the event, not only the `businesses` IPC).
    *   shared row cache: after `brief_seen` clears a business's dot, a stamp change in another business makes the watcher emit all rows, and that payload still has `unread: false` for the first business; then a new brief file in the first business turns its dot back on within one tick;
    *   `brief_seen` never lowers the stored pair;
    *   `brief_seen` refused for an `mtime` greater than the brief file's current mtime, and the stored pair unchanged;
    *   on a window whose triple has no registry business (label `explicit`), with a `[[business.explicit]]` entry in `dashboards.toml`, `dashboards` returns an empty list and `dashboard_open` returns `UnknownDashboard`;
    *   `cosreport` answers 403 to `OPTIONS` and `PUT` with `Origin: http://127.0.0.1:4777` and with no Origin;
    *   the UI-gating cases (a `cos-report-ready` message without `brief` is not passed to `brief_seen` while the `cos-host` reply is still sent; `brief_seen` is not called while another view is visible and is called on the switch to Today) are not in this file. `ipc.rs` runs on the mock runtime with no webview, so they run as E2E steps in §Verification Plan.
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
    *   on `founding-bets`, launch with no hash and with `#tab=agents`: the "Control room" and "Salesroom" tabs render in both cases;
    *   the two UI-gating cases of item 25 (a `cos-report-ready` message without `brief` is not passed to `brief_seen`, and the `cos-host` reply is still sent; `brief_seen` is not called while another view is visible and is called on the switch to Today);
    *   capture `lsof -nP -iTCP:4777 -sTCP:LISTEN` before open, after two opens and after ⌘Q;
    *   capture `launchctl list com.longhand.chief-of-staff-dashboard` before and after;
    *   record screenshots with `pr-proof`.
