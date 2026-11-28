# Goldfish Review Report: Stage 5, Initial Zoom Out

**Target:** `docs/plans/2026-11-13-business-dashboards-design.md` (lines 1-613)
**Standard:** `resources/document_standards/zoom-out-standard.md` (egm 0.1.1)
**GoldfishContext:** none

## Reviewer Status

| Reviewer | Status |
| --- | --- |
| `egm-zoom-out` | completed |
| `egm-inventive-catalyst` | completed |

Both reviewers found the context sufficient (doc:11-31) and agreed the design addresses the root problem.

**Totals after deduplication:** 0 Blocker, 6 Major, 6 Nit.

Line references: `doc:N` is a line in the target document. Other references are repo paths relative to `/Users/decebaldobrica/Projects/chief-of-staff-v2-wt/business-dashboards`.

## Prioritized Findings (Blockers -> Major -> Nits)

No Blockers.

### M1. The plan as written still spawns `ps` on every refresh - Severity: Major
* **Reviewer Lens:** `egm-zoom-out`, `egm-inventive-catalyst`
* **Affected Scope:** Tech Plan §4 Signals (doc:223); file #2 (doc:432); file #3 (doc:439); file list §7 (doc:467)
* **Discrepancy Description:** doc:223 says signals "uses classify but replaces is_cos with proc::alive". `classify` calls `is_cos` directly with no seam (`src/agents/mod.rs:543-545`), and `is_cos` runs `ps -o comm=` through `comm()` (`src/agents/mod.rs:794-807`). File #3 exposes only the run walk and leaves `classify` unchanged, so `signals::read` would still spawn `ps`. That fails goal 3 (doc:36) and the test at doc:550. The fallback at doc:223 then accepts a recycled pid showing a run as live, a trade that is not needed.
* **Rationale & Recommendation:** doc:467 already adds `proc_pidpath` and `proc_pidinfo(PROC_PIDTBSDINFO)`. Put `exe(pid)` and `start_time(pid)` in `cos::proc` (`src/proc.rs`, next to `alive` at :247), not in `apps/desktop/src/servers.rs`. Rewrite `is_cos` on `proc_pidpath`. This removes `ps` from refresh, keeps exact liveness, needs no fork of `classify`, and drops the doc:223 caveat. Optionally move `started_at` (`src/agents/mod.rs:810-817`) onto the same helper. Note the macOS-only FFI in the core crate needs a `cfg` gate if `cos` is ever built off Darwin.

### M2. A supervisor that dies silently is never shown as lost - Severity: Major
* **Reviewer Lens:** `egm-zoom-out`, `egm-inventive-catalyst`
* **Affected Scope:** §4 Refresh (doc:235-240); goal 3 (doc:39); CUJ 1 (doc:65-69)
* **Discrepancy Description:** Rows are recomputed only when a directory stamp changes (doc:238). `dash::stamp` reads newest mtime and entry count only (`src/dash.rs:181-196`). A supervisor crash or `kill -9` writes nothing, so the row keeps `live=1, lost=0` until some unrelated write. The lost-run warning, the reason the signal exists, does not fire. The design treats "metadata-only refresh" as if it meant "no syscalls"; `proc::alive` is `kill(pid,0)` (`src/proc.rs:247-250`).
* **Rationale & Recommendation:** After M1, check liveness of each business's non-terminal run pids on every tick and emit `sidebar-changed` when the computed rows differ, not when a stamp differs. Stamps then gate only the full re-read and the brief read. Add a test: kill the supervisor pid without touching files, assert the row flips to lost within one tick. A kqueue `EVFILT_PROC NOTE_EXIT` push variant is possible but adds a thread and fd lifetime handling.

### M3. A second per-business count source next to the existing one - Severity: Major
* **Reviewer Lens:** `egm-zoom-out`
* **Affected Scope:** §4 (doc:221-240); file #8 `sidebar()` (doc:475)
* **Discrepancy Description:** `businesses()` already returns per-business live/lost rows (`apps/desktop/src/lib.rs:390-440`), and the header "N live in other businesses" badge and the ⌘K picker read from it (`apps/desktop/ui/app.js:1121-1135`). The design adds `SidebarRow` with its own live count beside `BusinessRow`, so the sidebar and ⌘K can disagree on the same screen. The doc does not say what happens to the `#other-live` badge, which the sidebar makes redundant.
* **Rationale & Recommendation:** Extend `BusinessRow` with `unread` and `scheduled` and feed the sidebar from the same function: one row type, one source. State whether `#other-live` is removed.

### M4. The `brief_seen` input is unspecified and has two signatures - Severity: Major
* **Reviewer Lens:** `egm-zoom-out`, `egm-inventive-catalyst`
* **Affected Scope:** doc:227, file #8 (doc:478), file #14 (doc:515)
* **Discrepancy Description:** doc:227 has `brief_seen(business)`; doc:478 has `brief_seen(business, file)`. doc:515 says `app.js` passes "the brief file the report rendered" on `cos-report-ready`, but that message carries only `type` (`src/report.html:455`; handler `apps/desktop/ui/app.js:193`). If the host instead reads the newest brief at call time, a brief that lands between render and mark clears a dot the owner never saw.
* **Rationale & Recommendation:** Pick one. (a) The report frame adds the filename to the message; it is tainted input already validated at doc:367. (b) `brief_seen(business)` with the host reading the newest `out/briefs/*.json`; accept the race and drop the filename check (doc:367) and ISA rows doc:389 and doc:588. (c) Keep `lastSeenBrief` in webview `localStorage` and compare in JS against the row's `latest_brief`; this removes `brief_seen`, `ui-state.json` and the `ui_state` helper (doc:485), if loss across app updates is acceptable. Use one signature throughout.

### M5. One global mutex held across slow probes and readiness polls - Severity: Major
* **Reviewer Lens:** `egm-inventive-catalyst` (Major), `egm-zoom-out` (Nit)
* **Affected Scope:** §2 Supervisor (doc:180-201); file list §7 (doc:462)
* **Discrepancy Description:** `ensure` is "single-flight under the mutex" (doc:182). Inside it are two 3 s probes (doc:189) and a readiness poll of up to 10 s (doc:194). `stop_owned` on `ExitRequested` (doc:196) takes the same `Mutex<HashMap>`, so ⌘Q during a start blocks for up to about 16 s before the 5 s shutdown deadline starts. A gazette open waits behind a control-room start although the keys differ.
* **Rationale & Recommendation:** Hold the map lock only for lookup and insert. Add a `Starting` slot with a per-key waiter; probe, spawn and readiness run outside the global lock. `stop_owned` sets a `shutting_down` flag, kills `Owned` and `Starting` children from handles it already holds, and does not wait on in-flight `ensure` calls. Add an ISA row: quit during the readiness poll stops the group within the 5 s deadline.

### M6. Crash-reclaim machinery is larger than needed and has a pid-reuse hole - Severity: Major
* **Reviewer Lens:** `egm-zoom-out`, `egm-inventive-catalyst`
* **Affected Scope:** §2 (doc:185-188); Slot (doc:463); `Refusal::ForeignOwner` (doc:465); ISA (doc:364, doc:386); Confidence (doc:407); CUJ 7 (doc:105); goal 7 (doc:44)
* **Discrepancy Description:** Three pidfile branches (reclaim, foreign-instance external, stale), an `app_pid` field and a `Reclaimed` slot variant. The "another app instance" branch (doc:187) covers a case nothing in the design creates. It also has a hole: if the dead app's pid is recycled to any live process, the branch treats the server as owned by another instance, marks it External, and never stops or reclaims it. That breaks goal 7 (doc:44).
* **Rationale & Recommendation:** On launch, if the pidfile's pid is alive with matching start time and exe (the M1 helpers), stop its group and remove the pidfile; otherwise remove the pidfile. The next open spawns fresh. This drops `app_pid`, `Reclaimed`, the reclaim-then-ready re-probe and `ForeignOwner`. CUJ 7 becomes "stops it" instead of "reclaims it". If the reclaim branch is kept, record `app_start_time` and compare it, with "missing field = stale" for old pidfiles, and add a matrix row.

### N1. Step count and refusal variant do not match the Technical Plan - Severity: Nit
* **Reviewer Lens:** `egm-inventive-catalyst`
* **Affected Scope:** doc:464-465; Tech Plan steps (doc:184-194); goal 8 (doc:45)
* **Discrepancy Description:** doc:464 says `ensure` "follows the five steps"; the Tech Plan lists four. `ForeignOwner` has no pane state in goal 8, and doc:187 maps that case to External, not a refusal.
* **Rationale & Recommendation:** Align the count. Drop `ForeignOwner` (see M6) or add it to goal 8.

### N2. "Registry order as written" is not available - Severity: Nit
* **Reviewer Lens:** `egm-inventive-catalyst`
* **Affected Scope:** Q2 (doc:61); Rendering (doc:219)
* **Discrepancy Description:** The registry is a `BTreeMap<String, Business>` (`src/business.rs:29`, `:37`) and `write` re-serialises it sorted (`src/business.rs:116-123`). File order is lost on load.
* **Rationale & Recommendation:** Say "alphabetical (registry is a BTreeMap)", or note that write order needs a format change, which conflicts with the non-goal at doc:397.

### N3. Hand-rolled XML plist reader where a crate is already in the graph - Severity: Nit
* **Reviewer Lens:** `egm-zoom-out`
* **Affected Scope:** file #2 (doc:433); doc:232; doc:553
* **Discrepancy Description:** `plist` 1.10.2 is already built through tauri (`Cargo.lock:2437-2438`).
* **Rationale & Recommendation:** Add it as a direct dependency. It also reads binary plists, which removes the "binary plist marks none" case (doc:232, doc:553).

### N4. Implementation detail in the architecture sections - Severity: Nit
* **Reviewer Lens:** `egm-zoom-out`
* **Affected Scope:** doc:189, doc:194, doc:367, doc:424, doc:467, doc:521
* **Discrepancy Description:** `pbi_start_tvsec`/`tvusec`, 250 ms / 10 s / 200 bytes / 20 lines, 64 KiB, the filename regex and 200 px sit in the design narrative.
* **Rationale & Recommendation:** Move them to the implementation plan; keep interfaces and ownership rules in the Technical Plan.

### N5. Scope items that are not needed by dashboards - Severity: Nit
* **Reviewer Lens:** `egm-zoom-out`
* **Affected Scope:** §6 and files #16, #20 (doc:258-260, doc:528-531, doc:560-563); `out/` binary check (doc:191, doc:363); §5 runbook and goal 1 (doc:34, doc:242-256)
* **Discrepancy Description:** The `task schedule` 8790 guard fixes a `cos.dash` versus gazette port collision (`Taskfile.yml:211-222`) that dashboards do not depend on. The `out/` binary check is weak because `dashboards.toml` is itself agent-writable (taint, doc:350). The runbook is operations, not design.
* **Rationale & Recommendation:** Move the schedule guard to its own change. Drop the `out/` check or state the threat it stops. Label §5 as non-code.

### N6. Q1 spike omits `withGlobalTauri` - Severity: Nit
* **Reviewer Lens:** `egm-zoom-out`
* **Affected Scope:** Q1 (doc:60)
* **Discrepancy Description:** `withGlobalTauri: true` is set (`apps/desktop/tauri.conf.json:10`). It decides whether `window.__TAURI__` appears inside the dashboard frame.
* **Rationale & Recommendation:** Name the flag in the Q1 spike and its pass condition.

## Checked with no finding

* Alt H crash gap is stated as an owner choice; launchd socket activation needs server support the control room lacks.
* Separate `dashboards.toml` (Alt B): reasoning matches `src/business.rs:116-123` and `:162-168`.
* CSP wildcard plus capability isolation (Alt D/E), gated by the Q1 spike.
* Cited references verified by at least one reviewer: `src/business.rs:41`, `:116-123`, `:162-168`, `:282-287`; `src/agents/mod.rs:561-591`; `src/dash.rs:181`; `src/proc.rs:41`, `:165`, `:237`, `:247`; `src/state.rs:555`; `src/report.rs:240`; `apps/desktop/src/lib.rs:264`, `:479-484`, `:891-909`, `:963-974`; `apps/desktop/ui/app.js:141`, `:159`; `apps/desktop/ui/tokens.css:90-97`; `apps/desktop/tauri.conf.json:22`, `:29`; `capabilities/default.json` (main window only); desktop `Cargo.toml` lacks `libc` (doc:580 correct to add it).
* The synthesizer re-checked M1 (`src/agents/mod.rs:543-545`, `:794`, `:804`), M4 (`src/report.html:455`, `app.js:193`) and N1 (doc:464-465).

## Not checked

* WKWebView mixed-content behaviour and `__TAURI__` exposure in the frame (Q1).
* portfolio-control-room and gazette source, outside this repo: `/health` body, the 2 s / one-connection claim (doc:189), page markers, CSP.
* The brief (D5/D6), the rest of the base design, README and `context.md` alignment.
* `tests/schedule.rs` internals, the `profile.json` `dashboardUrl` claim (doc:279), the on-disk plist format.
* Whether webview `localStorage` survives app updates (M4 option c).
