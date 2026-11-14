# Goldfish Annotated Review Report

**Target:** `docs/plans/2026-11-13-business-dashboards-design.md` (full file, sha256 `a853bd18…`)
**Findings resolved:** Stage 5 (`zoom_out_report.md`), Stage 6 (`comprehension_report.md`) and Stage 7 round 1 (`critic_report.md`).

Each finding gets one suggestion: PROPOSE FIX, CONTEST, ESCALATE or DECLINE. Duplicates across stages share one fix and are listed together.

## Decisions the fixes change (owner attention)

Three fixes change behaviour the owner described, not just wording:

1. **Crash recovery stops the server instead of reclaiming it.** This changes CUJ 7 and Goal 7. On launch, a pidfile whose pid is alive, whose start time, executable and process group all match, and whose port lock is free is stopped and removed; the next open starts a fresh server. This removes `app_pid`, the `Reclaimed` slot, the re-probe after reclaim and `ForeignOwner`, the source of five findings (M6, design-r1-3, design-r1-5, comprehension 2/3/19, N1).
2. **A per-port lock file replaces `app_pid` for multi-instance safety.** The app holds an exclusive `flock` on `servers/<port>.lock` for as long as it owns the server. A second app instance that cannot take the lock treats the server as external: it embeds it after the probe and never signals it. This fixes design-r1-2's cross-instance race.
3. **The header's "N live in other businesses" badge is removed;** the sidebar shows the same counts per row (M3).

## Prioritized Findings & AI Suggestions

### Process ownership (irreversible sink S2)

| Id | Sev | Suggestion | Proposed edit |
|---|---|---|---|
| M6, design-r1-5, design-r1-3, comp-2, comp-3, comp-19, N1, design-r1-16 | Major | PROPOSE FIX | §2 step 2 becomes **launch cleanup**, run once in `setup`. For each pidfile:<br>• if its port lock can be taken, the pid is alive, `start_time`, `exe` and `pgid == pid` all match the record, then stop the group (SIGTERM, deadline, SIGKILL) and remove the pidfile;<br>• in every other case where the lock can be taken, remove the pidfile;<br>• if the lock is held, another instance owns the server: leave it.<br>Remove the `Reclaimed` slot, the `app_pid` field and `Refusal::ForeignOwner`. Rewrite CUJ 7 and Goal 7: "the next launch stops the server left by the crash." The pidfile gains `pgid`. |
| design-r1-2 | Major | PROPOSE FIX | The supervisor takes `flock(LOCK_EX \| LOCK_NB)` on `servers/<port>.lock` before probing and keeps it for the slot's life; the lock is released when the slot ends. `ready` passes only on two consecutive passing probes with `child.try_wait() == None` before and after each one. A child that exits during readiness gives `failed to start`, and the listener that answered is reported as foreign, never Owned. |
| design-r1-4 | Major | PROPOSE FIX | The reaper thread is the single owner of `child.wait()` and reports the exit status on a channel. `stop_owned` skips Exited slots, signals only slots whose child has not been reaped, and waits on the reaper's channel within the shared deadline. Signals are sent only while the leader is unreaped, so the pgid cannot have been recycled. |
| M5 | Major | PROPOSE FIX | The map lock is held only for lookup and insert. A `Starting` slot carries a per-key waiter (`Condvar`), and probe, spawn and readiness run outside the global lock. `stop_owned` sets `shutting_down`, kills the Owned and Starting children whose handles it holds, and does not wait for in-flight `ensure` calls. New ISA row: quit during the readiness poll stops the group within the 5 s deadline. |
| design-r1-12 | Major | PROPOSE FIX | `stop_owned` is idempotent (a `shutting_down` flag; slots are drained once). The exit-path Confidence row becomes sand until a test proves ⌘Q from the menu reaches `ExitRequested`. SIGTERM to the app process is added to the stated gap; the next launch's cleanup stops what it left. |
| design-r1-10 | Major | PROPOSE FIX | The spawn cwd is the binary's parent directory, and the environment is inherited from the app. The control room bakes its root in at compile time (`CARGO_MANIFEST_DIR`), so cwd does not change what it serves. Added to the `spawn_server` binding. |
| design-r1-9 | Major | PROPOSE FIX | Cite `portfolio-control-room/control-room/src/server.rs:6-17` (one accept loop, 2 s read/write timeouts). Skip the identity probe when the slot is Owned and its child alive, so the app's own frame cannot make its probe fail. The spike measures a probe while a frame is loaded. |

### Signals and sidebar

| Id | Sev | Suggestion | Proposed edit |
|---|---|---|---|
| M1 | Major | PROPOSE FIX | Add `exe(pid)` (`proc_pidpath`) and `start_time(pid)` (`proc_pidinfo`) to `cos::proc`. Rewrite `is_cos` on `exe(pid)`, which removes the `ps` spawn from `classify`, `count` and `list` for every caller. Drop the recycled-pid caveat at §4, since liveness stays exact. `servers.rs` uses the same helpers. |
| M2, design-r1-1 | Major | PROPOSE FIX | Each tick, the watcher checks `proc::alive` for every business's known non-terminal supervisor pids (one `kill(0)` each), then recomputes rows when a stamp changed or a pid died. It emits `sidebar-changed` only when the rows differ. New CUJ 8: kill a supervisor without touching files, and the row shows lost within one tick. |
| M3 | Major | PROPOSE FIX | Extend `BusinessRow` with `unread` and `scheduled`. Once M1 is in, `businesses()` spawns nothing, so the sidebar and ⌘K read one source. The `sidebar` IPC and `SidebarRow` are removed. The `#other-live` badge is removed. |
| M4 | Major | PROPOSE FIX | Option (a): the report frame adds `brief: "<file>"` to its `cos-report-ready` message (`src/report.html:455`), and `app.js` passes it to `brief_seen(business, file)`. The file is validated as in the ISA. One signature throughout. |
| design-r1-14 | Nit | PROPOSE FIX | Stamp `businesses.toml` and `dashboards.toml`; reload rows and dashboard tabs when they change. |
| design-r1-15 | Nit | PROPOSE FIX | The clock compares canonical registry paths, resolved with `business::location_from` and the plist's environment. A plist with no `COS_BUSINESSES` resolves to the default location. |
| design-r1-17 | Nit | PROPOSE FIX | Performance states the cost as O(runs): one stat per run directory and one `kill(0)` per live run per tick. Full re-reads happen on change only. |
| N3 | Nit | PROPOSE FIX | Use the `plist` crate (already in the dependency graph through tauri) as a direct dependency. It reads binary plists too, so the "binary plist marks none" case is gone. |
| N2, design-r1-13 | Nit | PROPOSE FIX | Q2 is answered: rows are in alphabetical order, because the registry is a `BTreeMap` (`src/business.rs:29`). |

### Embedding and the report scheme

| Id | Sev | Suggestion | Proposed edit |
|---|---|---|---|
| design-r1-7 | Major | PROPOSE FIX | `cosreport` rule:<br>• GET is allowed with no Origin, a `null` Origin (the opaque report frames) or an app origin;<br>• POST requires an Origin that is present and is one of `tauri://localhost`, `http://tauri.localhost`, `cosreport://localhost` or `http://cosreport.localhost`. `null` and an absent Origin are refused.<br>New ISA rows: a POST with `Origin: null` from a frame nested in a dashboard is refused; a POST with no Origin is refused. The spike records which Origin WebKit sends on custom-scheme POSTs. |
| design-r1-8, N6, style-r1-1, style-r1-2 | Major | PROPOSE FIX | Q1 becomes a written go/no-go gate before Build, with three checks:<br>(1) the loopback iframe loads under `tauri://localhost`;<br>(2) `typeof window.__TAURI__ === "undefined"` inside the frame, given `withGlobalTauri: true`;<br>(3) the Origin WebKit sends on `cosreport` POSTs.<br>Fallbacks: if (1) fails, use Alternative F (open in browser) for dashboards; if (2) fails, use Alternative E (child webview). Text that says "spike Q1 confirms" becomes "the design holds only if Q1 passes". |
| design-r1-18 | Nit | PROPOSE FIX | The server log is truncated to its last 1 MiB at each spawn. Validation requires a probe marker of at least 12 characters that is not a bare tag such as `<title>`. Drop `allow-popups` from the dashboard sandbox. |
| comp-12 | Nit | PROPOSE FIX | Line 29 lists both existing frame sources: `cosreport:` and `http://cosreport.localhost`. |
| comp-16 | Nit | PROPOSE FIX | One sentence: `cosreport` serves the reports and accepts writes such as `/docs/organize`; those are scoped by the `x-cos-business` header, which any page that knows the business name can set. |

### Threat model and binary check

| Id | Sev | Suggestion | Proposed edit |
|---|---|---|---|
| design-r1-6, N5 (`out/` part), comp-15 | Major | PROPOSE FIX | State the threat model plainly: whoever can write the declared binary, or `dashboards.toml`, can run code as the owner the next time a dashboard opens. The design accepts this for a binary the owner builds in their own repo. The `out/` check is dropped as protection it does not give. The check stays: path absolute, regular file, executable bit set. Alternative C's rejection is reworded to the real difference: `profile.json` is the business's own record, read by agents and reports, and mixing launcher configuration into it couples two owners. |

### Scope

| Id | Sev | Suggestion | Proposed edit |
|---|---|---|---|
| structural-r1-3, N5 (guard part), comp-8 | Major | PROPOSE FIX | Keep the guard: embedding the gazette is what makes the 8790 collision matter.<br>• Add a Problem bullet: `cos dash` is the local report server on 8790, the same port as the Longhand gazette.<br>• Add Goal 9: with the gazette running, `task schedule` loads `cos.brief`, skips `cos.dash` and names the holder. |
| structural-r1-1 | Major | PROPOSE FIX | Add `## Risks & Mitigations` after Alternatives, with these rows:<br>• the server is unsupervised between a crash and the next launch;<br>• SIGTERM to the app;<br>• pid or pgid reuse;<br>• Q1 fails, and the fallback it triggers;<br>• the probe markers drift;<br>• the `127.0.0.1:*` frame source;<br>• tick cost O(runs);<br>• realestate's home is nested in founding-bets';<br>• workspace write access means code execution. |
| design-r1-11, structural-r1-2, style-r1-8 | Major | PROPOSE FIX | Link the brief's decision table and give each rejection a trade-off.<br>• A: launchd keeps the server always on and outside the app's view of its lifecycle, traded against supervisor code and the crash gap.<br>• F: the dashboards stay outside the business switch, and nothing ties a tab to the business it belongs to.<br>Write "Rejected (owner decision U4, brief §Decisions)" with a link, instead of "Rejected by author". |

### Terms and comprehension

| Id | Sev | Suggestion | Proposed edit |
|---|---|---|---|
| style-r1-3, comp-5 | Major | PROPOSE FIX | Terms:<br>• **managed server**: `dashboards.toml` gives it a binary;<br>• **unmanaged server**: no binary;<br>• **Owned**: a runtime slot only, meaning this instance spawned it and holds its lock;<br>• **run supervisor**: the agent run's `cos agents supervise` process, as distinct from the **server supervisor** (`servers::Supervisor`). |
| comp-6, comp-7, comp-17, comp-21 | Major | PROPOSE FIX | Add one line each to Terms, with a link to the base design:<br>• `cos`;<br>• the registry (`businesses.toml`, its home / workspace / projects triple);<br>• the app's business (per window, set by ⌘K or the sidebar) vs the active business (CLI, stored in the registry);<br>• brief;<br>• profile;<br>• live, lost and starting runs;<br>• pinned by env;<br>• the unsaved-Hire guard;<br>• the Today, Agents and Docs views. |
| comp-9 | Major | PROPOSE FIX | Add a two-line intro to the Semantic ISA:<br>• each side effect becomes an instruction in one of five cores;<br>• a sink is an instruction with an effect outside the app;<br>• every sink accepts input only after a named deterministic VERIFY clears it.<br>Link `docs/ritual-zero-to-hero.md` if present, else the semantic-isa skill name. |
| comp-4 | Major | PROPOSE FIX | `ensure` gets a branch for an unmanaged dashboard whose port is free: refuse with `not running` and the hint "its owner (for example launchd) is not running it". An Exited slot respawns on the next open. Goal 8's states become: starting, running (Owned), running (external), refused (identity), not running, binary missing, failed to start. "Stopped" is removed (comp-18). |
| comp-1 | Major | PROPOSE FIX | `ensure` lists its steps in one place, and the file section points to them by name, not by count. |
| comp-10 | Major | PROPOSE FIX | Not contested: the reader could not know this. The fix states it: `portfolio-control-room` is a cargo workspace root (`portfolio-control-room/Cargo.toml:4`), so the release binary lands in `portfolio-control-room/target/release/`. |
| comp-11, structural-r1-10 | Nit | PROPOSE FIX | `RENDER` is cleared by `VERIFY(identity)` for an external server, or by the readiness check for a server this instance spawned. The test-matrix column becomes "Sink / checkpoint". |
| comp-13, comp-14, comp-20, comp-22, structural-r1-9 | Nit | PROPOSE FIX | `location()` is "…joined with `dashboards.toml`". The brief is linked where IDs appear. The kronos (business) vs `~/Projects/chronos` (directory) spelling is noted as deliberate. "Runbook" points at its section. Rock, sand and water get one line above the Confidence table. `pr-proof` is glossed. The base-design citation becomes `:58-59`. |

### Style

| Id | Sev | Suggestion | Proposed edit |
|---|---|---|---|
| style-r1-4 | Major | PROPOSE FIX | "Four businesses are not registered (the registry holds `acme` and `default`), and Kronos has no workspace yet." |
| style-r1-5 | Major | PROPOSE FIX | "A separate file outside every workspace is not rewritten by any `cos` version (U6)." |
| style-r1-6 | Major | PROPOSE FIX | "The app makes no non-loopback request; embedded pages keep their own network behaviour." |
| style-r1-7, style-r1-9, style-r1-10, style-r1-11, style-r1-13, style-r1-14, structural-r1-12 | Nit | PROPOSE FIX | Apply each as written in `critic_report.md`. |
| style-r1-15 | Nit | PROPOSE FIX | Covered by comp-22. |

### Structure

| Id | Sev | Suggestion | Proposed edit |
|---|---|---|---|
| structural-r1-6, structural-r1-7 | Nit | PROPOSE FIX | Number every Technical Plan subsection. Move the registration runbook to its own `## Setup Runbook (not code)` section. |
| structural-r1-11 | Nit | PROPOSE FIX | CUJ 8 covers the lost warning (from M2). The clock, binary-missing and failed-to-start states are named as covered by tests in `tests/signals.rs` and `apps/desktop/tests/servers.rs`. |
| structural-r1-4 | Nit | DECLINE | The headings follow the EGM design template this document was drafted from (`The Problem`, `Technical Plan (Architecture)`). Renaming them to another standard's names adds no information. |
| structural-r1-5 | Nit | DECLINE | The ritual's design layout puts the Decision first (`ritual/references/spec.md`: "Decision first, in one paragraph"). Line 13 gets split into bullets under the comp-6 rewrite. |
| structural-r1-8 | Nit | DECLINE | Mixed markers come from the template's metadata block. They carry no meaning and are left as they are. |
| N4 | Nit | DECLINE | Numbers such as timeouts and sizes stay where the behaviour is defined, because the ISA bindings and tests reference them. |

## Totals

- **PROPOSE FIX:** every Major (33 after merging), and 24 Nits.
- **CONTEST:** 0.
- **ESCALATE:** 0. The three behaviour changes are listed at the top for the owner.
- **DECLINE:** 4 Nits.
