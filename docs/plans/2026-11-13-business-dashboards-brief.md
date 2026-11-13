# Business sidebar and per-business dashboards: brief

**Author:** Decebal Dobrica
**Date:** 2026-11-13
**Mode:** BRAINSTORM (multi-lens review of the anchored feature hypothesis)
**Base design:** `docs/plans/2026-11-12-multi-business-design.md`

## Problem

The owner runs four businesses from pinned Codex threads (Founding Bets, Longhand, RealEstate, Kronos). Each has its own chief-of-staff workspace, and two have a live dashboard of their own: the Founding Bets control room and salesroom, and the Longhand gazette. The desktop app shows none of them. Switching is a ⌘K search, nothing in the window shows which business needs attention, and the dashboards live in browser tabs, so each one has to be started by hand from its repo.

## Goal

The app lists every registered business in a pinned sidebar, with signals for live agents, an unread brief, lost runs and the scheduled business. Clicking a row switches to it. A business's own dashboards open inside the app: the control room and salesroom for Founding Bets, which the app starts and stops itself, and the Longhand gazette, which launchd keeps running and the app only embeds.

## Decisions made with the owner

| # | Decision |
| --- | --- |
| U1 | Pinned sidebar of businesses; ⌘K stays. |
| U2 | Founding Bets shows the control room (`http://127.0.0.1:4777/`) and the salesroom (`http://127.0.0.1:4777/sales`), not the revenue desk (:4788). |
| U3 | Kronos home is `~/Projects/chronos`, seeded fresh. |
| U4 | The app starts the control room and stops it on quit (reconfirmed after the supervisor cost was shown). |
| U5 | Longhand embeds its gazette at `http://127.0.0.1:8790/#staff`; launchd owns it and the app never starts or stops it. |
| U6 (D1) | Dashboards live in a separate `~/.config/cos/dashboards.toml` keyed by business name, so no `cos` binary, old or new, rewrites them. |

## Proposed answers to the remaining decisions (for the design to confirm)

- **D2:** every embedded dashboard has an identity probe (path plus expected marker); a mismatch shows "port held by something else". `task schedule` refuses to load `cos.dash` when its port is already held.
- **D4:** pidfile per owned server (`pid`, start time, executable path). Owned servers stop on quit through a `RunEvent::Exit` handler; adopted servers are never killed; a stale owned pidfile is reclaimed on launch.
- **D5:** identity by the listener's executable path (`proc_pidpath`), on the start path only. No change to the control-room repo.
- **D6:** iframe with exact loopback origins in `frame-src`; `cosreport` rejects foreign `Origin`; spike on mixed content and `__TAURI__` in the subframe before build.
- **D7:** `lastSeenBrief` as `(filename, mtime)` in a new `ui-state.json`, written atomically.
- **D8:** three signals: live count, unread dot, lost warning.
- **D9:** nested homes allowed (realestate inside founder-mode), with a warning in `cos business add`.
- **D11:** the dashboard frame is destroyed on switch; the server keeps running until quit.

## Reviewer status

| Lens | Status |
| --- | --- |
| egm-design | completed |
| egm-codebase-details | completed |
| egm-zoom-out | completed |
| egm-comprehension | completed |
| egm-implementation-readiness | completed |
| egm-structural | completed |
| egm-inventive-catalyst | completed |

None of the reviewers had a shell, so nothing was run. They did not check the Longhand gazette's response headers, the git status of `founder-mode`, whether Tauri injects `__TAURI__` into cross-origin subframes, or how WKWebView treats mixed content for an `http://127.0.0.1` iframe under `tauri://`. One concept fact turned out wrong: the salesroom route. The synthesizer confirmed it against the control-room source.

## Overview

The hypothesis covers six concerns:

1. Register four businesses: founding-bets, longhand, realestate and kronos.
2. Add a per-business `dashboards` list to the registry.
3. Have the app start and stop the control-room server and adopt an existing listener.
4. Embed foreign-origin pages in the desktop window, which means a CSP change.
5. Add a pinned sidebar that shows a dot for live agents or an unread brief.
6. Add a clock that marks the business the launchd schedule runs for.

All seven lenses agree the concept is not ready to build. The registry writer would erase the new `dashboards` key. `cos dash` and the Longhand gazette both use port 8790. The app has no exit hook to stop a server on. "Adopt" and "stop on quit" contradict each other. The cheap refresh path the sidebar needs does not exist yet.

The sidebar and the four registrations need almost no new code. Most of the cost and risk sits in the app-owned process lifecycle and the generic dashboard schema.

## Corrections to the stated facts

- **The salesroom URL is wrong.** The control room routes `"/sales" | "/sales/"` only (`portfolio-control-room/control-room/src/server.rs:146-148`), so `/sales.html` returns 404. Use `http://127.0.0.1:4777/sales`.
- **The control room sets its own CSP**: `default-src 'self'; connect-src 'self'` (`server.rs:247`), and it fetches `/data/*.json`. The iframe therefore needs `sandbox="allow-scripts allow-same-origin"`. The existing report iframes use `allow-scripts allow-popups` (`apps/desktop/ui/index.html:157`), which gives the frame an opaque origin, so the control room's fetches would fail.
- **The control room serves one connection at a time** (`server.rs:14-23`) with 2 s read/write timeouts (`server.rs:38-41`). A health probe can queue behind a slow `/data/sales.json` snapshot (`server.rs:87-101`) and fail even though the server is fine.
- **The CSP has two sources, not one**: `frame-src cosreport: http://cosreport.localhost` (`apps/desktop/tauri.conf.json:22`).
- **The concept reverses a non-goal of the base design.** `docs/plans/2026-11-12-multi-business-design.md:58` puts per-business ports and dashboards out of scope. The new design must say it supersedes that line.
- **The app has no settings writer.** `Settings` derives only `Deserialize`, and the module doc says the file is "never written again" (`apps/desktop/src/settings.rs:1-3, 11`). `lastSeenBrief` needs a new writer.

## Analysis

### Contradictions

**C1. One registry file, but the CLI writer drops what it does not know.**
`Business` has three fields (`src/business.rs:19-24`). `write` serialises `Raw` only (`src/business.rs:116-123`), and every `add`, `use` and `remove` goes through `update` (`src/business.rs:125-130`). `canonical()` rebuilds `Business` from its three dirs (`src/business.rs:162-168`), so `add --replace` drops new fields as well. Version skew makes this worse: launchd runs `~/.cargo/bin/cos` (`launchd/cos.brief.plist:8`), which can lag the dev build. One old-binary `cos business use` would silently erase every business's dashboards.
Resolutions:
- (a) Add `#[serde(flatten)] extra: toml::Table` so the writer keeps unknown keys. This removes the `Eq` derive at `business.rs:19`.
- (b) Make `dashboards` a first-class field with `serde(default, skip_serializing_if = "Vec::is_empty")`, plus a round-trip test.
- (c) Keep dashboards out of the registry: either `~/.config/cos/dashboards.toml` keyed by business, or `<home>/context/dashboards.toml`.

Options (a) and (b) still lose data to an older binary. Option (c) is the only one that is immune to skew.

**C2. Two KeepAlive jobs want port 8790.**
`cos dash` defaults to 8790 (`src/main.rs:60`). `cos.dash.plist` passes no `--port` and sets `RunAtLoad` and `KeepAlive` (`launchd/cos.dash.plist:8-12, 19-21`). `task schedule` always installs both the brief job and the dash job (`Taskfile.yml:211-222`). Whichever job binds first wins, and the other crash-loops every 30 s. Because no-binary dashboards get no identity check, the Longhand pane can silently show a cos report under the gazette's name. `cos docs` also hardcodes `http://127.0.0.1:8790/docs` (`src/main.rs:470`).
Resolution: move the `cos dash` default (for example to 8791), pass `--port` in the plist and fix `main.rs:470`. Alternatively, retire the `cos.dash` job: the desktop app already serves the report through `cosreport:` (`apps/desktop/src/lib.rs:963-974`). Either way, give embed-only entries an optional identity probe (path plus expected body) and show a "port held by something else" pane on mismatch.

**C3. The app must own the server's lifetime, but the server outlives an app crash.**
`main.rs:4-6` calls `.run(generate_context!())` with no `RunEvent` handler. Cmd-Q goes through the predefined `.quit()` item (`lib.rs:925`). macOS has no PDEATHSIG. After a force-quit or crash, the control room is orphaned to PPID 1 and keeps :4777. On the next launch it passes the health check, gets adopted and is never stopped. That breaks the user's decision that the app starts and stops it.
Resolutions:
- **Pidfile.** Record `{pid, start time, exe}`. On launch, a live pid whose comm matches is reclaimed as owned. Copy `tunnel.rs:70-90` (`sweep_stale`) and the start-time guard at `agents/mod.rs:821-824`.
- **Guard shim.** A hidden `cos` subcommand holds a pipe from the app, or watches the app pid with kqueue, and kills the server's process group on EOF.

Rule for both: a server the app started is killed on quit; an adopted server is never killed.

**C4. The adopt check is cheap but proves nothing about identity.**
`/health` returns a static `{"status":"ok"}` (`server.rs:185-189`). Any server returns that, including a control-room build from another checkout, which bakes in a different root through `CARGO_MANIFEST_DIR` (`control-room/src/main.rs:31`).
Resolutions:
- Add `service` and `root` fields to `/health`. This is a change in the control-room repo.
- Or map the listener pid to its executable (libproc `proc_pidpath`, or `lsof` on the start path only) and compare it with the declared binary.

**C5. Same-window embed versus no IPC for a foreign origin, and a static CSP versus dynamic URLs.**
The CSP is fixed at build time, but registry URLs are data. The capability grants `core:default` to `windows:["main"]` and lists no `remote` URLs (`capabilities/default.json:5-6`), so IPC from `http://127.0.0.1` should be denied. That is unproven, and `withGlobalTauri: true` (`tauri.conf.json:10`) raises the stakes.
There is also a gap the concept did not mention. The docs write guard checks only an `x-cos-business` header equal to the business name (`lib.rs:479-484`). The name is guessable, so a foreign frame that can POST to `cosreport://localhost/docs` passes the guard.
Resolutions:
- Add only the exact origins (`http://127.0.0.1:4777`, `http://127.0.0.1:8790`) to `frame-src`. No wildcard.
- Restrict registry URLs to http on loopback.
- Make the `cosreport` handler (`lib.rs:963`) reject any foreign `Origin`.
- Add a test that `__TAURI__` is undefined in the frame.
- Use sandbox `allow-scripts allow-same-origin allow-forms allow-popups`, without `allow-top-navigation`.
The alternative is a child webview with no capability, which sits behind the Tauri `unstable` feature.

**C6. The dot must be cheap, but it must reflect live agents.**
The `businesses` IPC calls `agents::count` (`lib.rs:398`). That goes through `classify`, then `is_cos`, then one `ps` spawn per live run (`agents/mod.rs:543-548, 585, 794-807`). The watcher stamps only the active business (`lib.rs:891-909`). Today `businesses()` runs only from the agents view and the picker (`ui/app.js:141`, `ui/command.js:100`).
Resolution: extend the watcher to stamp every business's agents root and `out/briefs` with `cos::dash::stamp` (`src/dash.rs:181-187`, `read_dir` plus metadata). Recompute dots only when a stamp changes, using `proc::alive` (`src/proc.rs:247-250`) with no spawn, and emit `sidebar-changed`. A recycled pid can light the dot falsely until the next full check, which is acceptable for a dot. Never put `businesses()` on the tick.

**C7. One fixed launchd label, four businesses.**
The user accepted this trade-off; it does not have to stay. Alternatives: a registry flag `brief = true`, with one job looping over the flagged businesses, or per-business labels `cos.brief.<name>`. If the single label stays:
- The plist file being present does not mean the job is loaded.
- `COS_BUSINESSES=@REGISTRY@` (`launchd/cos.brief.plist:16`) may name a different registry than the app's.
- Legacy plists set `COS_HOME` and have no `--business` (design doc :221-223).
- A `plutil` edit can leave a binary plist.
Label the clock "installed for X". Show it only when the registry paths match and the name exists. Show a separate legacy marker for old plists.

### Trade-offs

| Choice | Gain | Cost |
| --- | --- | --- |
| App-owned control room (user's decision) | Nothing to install outside the app | Process supervisor, exit hook, orphan recovery, adopt and refuse states, logs and tests, all for one server |
| launchd owns the control room | No app code; survives app crash; matches Longhand | Contradicts the user's decision; adds a plist in the founder-mode repo |
| iframe embed with a fixed CSP list | Small change; same window | A rebuild for each new port; the mixed-content spike is unproven |
| Child webview, no capability | IPC isolation by construction | `unstable` feature; layout and resize sync by hand |
| "Open in browser" (`open` as at `src/report.rs:241`) | No CSP change, no embed risk | Loses the in-window view |
| Dashboards in the registry | One file | Data loss under version skew (C1) |
| Dashboards in a separate file | CLI never rewrites it | Drift when a business is renamed or removed; orphan entries need a lint |

### Ideal Final Result

- The registry round-trips keys it does not know, or dashboards live where the CLI never writes.
- The server's lifetime is tied to the app by a pipe or a pidfile. A crash leaves no orphan, and the app adopts only servers it did not start, never killing them.
- Listener identity comes from the executable path or a `/health` identity field, not a generic body.
- `cosreport` rejects foreign origins, and the CSP lists exact dashboard origins.
- Sidebar dots are recomputed only on a metadata stamp change, with no spawn on the refresh path.
- `cos.dash` moves off 8790 or is retired.
- Every refused or failed state shows the URL, the port, what answered and the next command, instead of a blank iframe.

## Goals vs Non-Goals

**Goals**
- Four businesses registered, each with explicit home, workspace and projects dirs and a working profile. Kronos is bootstrapped with `mkdir` and `cos --workspace ~/Projects/chronos state init` before the first sidebar click, because `switch_business` refuses a business without a profile (`lib.rs:344-351`).
- A pinned sidebar that calls the same `switch_business` (`lib.rs:326`) and reload path as Cmd-K, including the pin refusal, the unsaved-hire guard and the stale-write refusal (design doc :236-271).
- Founding-bets embeds the control room (`/`) and the salesroom (`/sales`), both served by one process.
- Longhand embeds the gazette at `:8790/#staff`. The app never starts or stops it.
- The app stops the control room on quit only if the app started it.
- Every dashboard state is visible in the pane: starting, owned, adopted, refused, missing binary, crashed, or external down.

**Non-Goals**
- The app building the control room. The app never runs `cargo`.
- Bundling the control-room binary (`externalBin: []`, `tauri.conf.json:29`).
- A revenue desk (:4788) embed. The brief should state the reason in one line.
- Wildcard frame origins or a generic pluggable health-check spec.
- Scheduling more than one business, unless C7 is reopened.
- Dashboards that follow the CLI-active business. Only the app business counts.

## Alternatives weighed

1. **Smallest path (zoom-out).** Fix the 8790 port. Register the four businesses as runbook steps (`cos business add`, `seed_context`, `src/business.rs:171, 269`). Build the sidebar from the existing data with a live-agents-only dot. Dashboards are `{name, url}` only, opened via `open` or an iframe with two fixed origins. The control room runs under a launchd plist in the founder-mode repo after `cargo install --path`. Defer the unread dot, the clock and the app-owned lifecycle. This removes C3 and C4 entirely, but it reverses a user decision.
2. **Hypothesis as stated, hardened.** Keep app ownership, but add a pidfile or shim, an identity check, an ownership rule (owned vs adopted), a state machine, logs at `<home>/out/logs/dash-<name>.log` and an exit hook. Reuse `tunnel.rs:129-130, 173-187` (`process_group(0)`, then SIGTERM, a wait and SIGKILL) and `proc::spawn_grouped` / `kill_group` (`src/proc.rs:165-169, 237-244`). Do not reuse `proc::run`: its 300 s ceiling (`proc.rs:14, 104-116`) is wrong for a server. Do not reuse the `setsid` detach (`agents/mod.rs:473-480`).
3. **Model servers separately from views (design).** Split the schema into `servers` (binary, addr, health) and `views` (name, url, server ref). The control room and salesroom are one server with two views. Starts are single-flight, guarded by `Mutex<HashMap<server, State>>`, and the app builds `serve ADDR` from the server's addr. This avoids a double spawn when both views open.
4. **Split the delivery (structural).** Part A is the registry schema, in the cos crate, as data plus validation and a pure `health()` function. Part B is the sidebar, dot and clock, with a schedule parser in the cos crate shared with `cos business list`. Part C is embedding and the supervisor, with supervision in a new `apps/desktop/src/dashboards.rs`, not in cos. Part D is the runbook: seed four businesses, build the control room, resolve 8790.

## Key decisions and open risks

### Decisions needed before planning

| # | Question | Blocks |
| --- | --- | --- |
| D1 | Where dashboards live: registry with `flatten`, registry with a first-class field, or a separate file. Can CLI writers set `binary` at all? Reviewers recommend hand-edit only, shown in `cos business list`. | Schema |
| D2 | Port 8790: move the `cos dash` default or retire the `cos.dash` job. | Longhand embed acceptance |
| D3 | Reconfirm app-owned vs launchd-owned control room now that the supervisor cost is visible. | Supervisor |
| D4 | Ownership rule: owned is stopped on quit; adopted is never stopped. Recover orphans by pidfile or by guard shim. | Supervisor |
| D5 | Identity match: add a `/health` identity field (cross-repo change) or check the listener's executable path. | Adopt |
| D6 | Embedding: iframe with exact origins, or a child webview. Includes the mixed-content and IPC-in-subframe spike. | Embed |
| D7 | "Unread": which event writes `lastSeenBrief`; compare `(filename, mtime)`, because briefs are `out/briefs/{day}.json` and a same-day rerun overwrites the file (`src/report.rs:170`, `src/store.rs:32-35`); key by name and prune removed entries; store it in a separate `ui-state.json` written with `atomic_write_bytes`, where an unparseable file counts as empty. | Dot |
| D8 | Dot semantics: split live (count), unread (dot) and lost (warning), or one dot with a stated precedence and a tooltip. `lost` is the case that needs action, and the concept's dot drops it. | Sidebar |
| D9 | Nested homes: realestate sits inside founding-bets' home. `add` checks equality only (`src/business.rs:282-286`), and `git_repos` (`lib.rs:873-883`) could offer realestate as a spawn target. Allow nesting, warn, or reject. | Registration |
| D10 | Naming: the registry concept "dashboards" collides with `cos dash`, so consider `embeds` or `views`. Settle kronos vs chronos. The registry has no display-name field, so the UI shows slugs unless one is added. | Docs and UI |
| D11 | Iframe lifetime on a switch. The design reloads on switch (design doc :259), so control-room state is lost. Either destroy the frame on switch, or keep only the last-shown business's frame. The server keeps running until quit. | Embed |

### Open risks

- **Data loss.** An old `cos` binary strips `dashboards` from the registry (C1).
- **Wrong page embedded.** Because of the 8790 race, the Longhand pane can show a cos report with no error (C2).
- **Orphan control room** after a crash or SIGKILL, adopted on the next launch and never stopped (C3).
- **Stranger adopted.** A generic `/health` body matches another checkout or another service (C4).
- **Docs write guard bypass** from a foreign frame through a guessable header (`lib.rs:479-484`) (C5).
- **Health probe false failure** behind the single-threaded server. The probe timeout must exceed 2 s and retry.
- **Two app instances.** There is no single-instance plugin (`apps/desktop/Cargo.toml:18-28`). The second instance adopts, and when the first quits it kills the shared server.
- **Quitting the app kills open browser tabs on :4777.** The user accepted this; surface it on quit.
- **Stale binary.** The root is baked by `CARGO_MANIFEST_DIR`, so the binary is valid only beside its checkout. Nothing rebuilds it after control-room changes. If it is missing, the pane should show the build command.
- **Registry as exec config.** A `binary` field means agents that can write the registry (design doc :387-390) can get code executed. Validate an absolute path, a regular file, not under any `out/`, and a fixed argv `[serve, addr]`.
- **Migration order.** If the existing `default` business already owns `~/Projects/founder-mode`, `add founding-bets` is refused (`src/business.rs:282-287`), and `remove default` is refused while it is active (`:245`). The order needs a script.
- **Cmd-K inside foreign-origin frames is untested.** It was hand-checked only for `cosreport` frames (design doc :508).

### Acceptance outline (end to end)

- `cos business list` shows four businesses, and kronos reports `profile ok`.
- On a fresh app, the sidebar shows four rows in a defined order.
- Opening the control room spawns one process. A second open spawns nothing new (`lsof -i :4777` shows one pid). The salesroom is served by the same process at `/sales`.
- After Cmd-Q, :4777 is free and no PPID-1 `portfolio-control-room` remains.
- After `kill -9` of the app, the next launch reclaims and later stops the orphan, or the shim has already killed it.
- A foreign listener on :4777 is refused with a message, and nothing is killed.
- The Longhand gazette pid in `launchctl list` is unchanged across app start and quit.
- A new brief file lights the dot. Opening the brief clears it, and the cleared state survives a restart.
- The clock appears only on the business named by a plist that points at the same registry.
- After the upgrade, an old `cos business use` either preserves dashboards or is refused.
