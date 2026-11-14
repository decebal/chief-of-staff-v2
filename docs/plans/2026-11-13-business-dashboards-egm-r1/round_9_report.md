# EGM round 9

- **Document:** [business dashboards design](../2026-11-13-business-dashboards-design.md)
- **Round:** 9
- **Reviewers:** design, comprehension, structural, style, codebase, readiness
- **Findings:** 38 (critical 0, major 15, nit 23)
- **Dispositions:** FIX 38, CONTEST 0, DECLINE 0, ESCALATE 0
- **Verdict:** FINDINGS
- **Status of the design:** In Review (unchanged)

Numbering follows the round's order: F1 to F8 design, F9 to F12 comprehension, F13 to F17 structural, F18 to F24 style, F25 to F30 codebase, F31 to F38 readiness.

## Root-cause rewrites

- **Shared row cache (F1, F13, F18, F31, F38).** `brief_seen` emitted `businesses-changed` while the watcher kept its own last-emitted rows, so the watcher's next emit restored the cleared dot, or a new brief matched the stale cache and was never emitted. One definition now lives in §6 Refresh: managed state `Rows(Mutex<BTreeMap<String, BusinessRow>>)` holds the rows last emitted; `publish(app, rows)` replaces the given rows and, when any changed, emits all cached rows under that mutex; the watcher keeps only stamps and `supervisors` sets. The synchronous builder `rows(registry, current, ui_state_file, only)` is called by the `businesses` command (inside `blocking`), the watcher and `brief_seen`. The §2 diagram, §6 unread, item 9 and item 25 (two new cases) agree. `BusinessRow`, `Present` and `Live` derive `Clone` and `PartialEq` (`apps/desktop/src/lib.rs:363-388` derive only `Serialize` today; `Business` already does, `src/business.rs:19`).
- **Per-dashboard identity on reuse (F2, F20, F25).** ensure: owned returned `Running` for an `Owned` slot, and a waiter built its own `Running`, without probing the requesting dashboard's marker, while the RENDER sink row claimed readiness plus identity. ensure: owned now runs `VERIFY(identity)` with `d.probe` outside `inner` before it returns `Running`; a failure refuses `identity` and leaves the `Owned` slot and its server as they are. A waiter whose slot ends `Owned` runs ensure: owned again. Updated: §4 ensure: owned, the RENDER sink row, the matrix (RENDER row, concurrent-opens row), the Risks foreign-listener row, item 8 `ensure`, item 23 (two new cases, two reworded).
- **Refusal and payload types (F5, F11, F12, F16, F22, F28, F29, F36).** `Refusal` derives `Clone, Debug, Serialize` with `#[serde(tag = "kind")]`, and its JSON shape is given. The ISA refusal outputs map onto variants: `identity` to `Identity`, `binary_admissible` to `BinaryMissing` or `NotAdmissible`, `readiness` and `spawn_result` to `FailedToStart`. `Running` and `DashboardExited` derive `Clone, Serialize`; `Dashboards` derives `Default`.
- **Business check name (F6, F15, F21, F34).** `named()` in §6 and the `STORE(ui-state)` sink row is now `named_st`, as in item 9 and §5 step 1 (`get()` takes the same mutex, `apps/desktop/src/lib.rs:239-244`).
- **No registry business (F24, F26).** §1, §5 and item 9 share one rule: `Triple.business == None` (label `explicit`, or `default` from the defaults layer, `src/business.rs:347-352`) gives an empty `dashboards` list and `UnknownDashboard` from `dashboard_open`. A registered business named `default` keeps its dashboards.
- **Pin text (F27, F33).** No pin handling exists in `apps/desktop/ui/command.js`; §6 Rendering and item 13 now cite the header `#biz-pin` (`apps/desktop/ui/app.js:99-100`) and state that sidebar.js reads `pinned` from `invoke("paths")`.
- **Port 0 (F7, F37).** §3 refuses port 0 in `addr` and URLs; LOAD binding, item 1, matrix `PROBE` row and item 19 case.

## Findings

| id | reviewer | sev | disposition | edit/evidence |
| --- | --- | --- | --- | --- |
| F1 | design | major | FIX | Shared row cache, §6 Refresh; item 25 case: a stamp change in another business keeps `unread: false`. |
| F2 | design | major | FIX | Per-dashboard identity on reuse; RENDER sink row rewritten; matrix and item 23 rows added. |
| F3 | design | major | FIX | §3: `probe.path` and `ready.path` match `^/[!-~]*$`; S5 and the PROBE row state the parse rule clears it; LOAD binding; item 1 `load` and `probe` request line; matrix `PROBE` row; item 19 case. |
| F4 | design | major | FIX | Goal 7: the port lock is taken first; "A pidfile whose port lock another instance holds is left alone"; removal without a signal applies under a free lock only. Matches §4 Launch cleanup step 1. |
| F5 | design | nit | FIX | Refusal and payload types. |
| F6 | design | nit | FIX | Business check name. |
| F7 | design | nit | FIX | Port 0. |
| F8 | design | nit | FIX | §6 unread and `VERIFY(brief_seen)`: an `mtime` greater than the file's current `mtime_ms` is refused (check only, not a value source); sink row, matrix row, item 25 case. |
| F9 | comprehension | major | FIX | Item 4 `signal_pid` and S2: used for a `Leader` target by the failure epilogue and by `stop_owned` (§4 Stop path). |
| F10 | comprehension | nit | FIX | Matrix "stopped server" and item 23: a stopped leader dies from the SIGKILL at 5 s; the assert uses the 5 s + 1 s budget. Stop path unchanged. |
| F11 | comprehension | nit | FIX | Refusal and payload types. |
| F12 | comprehension | nit | FIX | `VERIFY(readiness)` out is `Refusal::FailedToStart { log_tail }`. |
| F13 | structural | major | FIX | Shared row cache; `brief_seen` publishes, and the emit carries all cached rows. |
| F14 | structural | major | FIX | §5 and items 14, 15: `openFromHash` awaits `loadDashTabs()` for every hash; the chain `loadPaths().then(loadRepos).then(openFromHash)` (`apps/desktop/ui/app.js:1154`) sets `ui.business` first (`app.js:97`); E2E step for no hash and `#tab=agents`. |
| F15 | structural | nit | FIX | Business check name. |
| F16 | structural | nit | FIX | Refusal and payload types, including `Dashboards: Default`. |
| F17 | structural | nit | FIX | §5 and item 15: `dash=<d>`, the `data-dash` of the `aria-current` dashboard tab; `switchTo(name)` takes the business (`app.js:159`). |
| F18 | style | major | FIX | Shared row cache; item 25 cases. |
| F19 | style | major | FIX | §6 unread, item 15, item 25, E2E: a message without `brief` is not passed to `brief_seen` and is not kept; the `cos-host {hire: true}` reply (`apps/desktop/ui/app.js:193-194`) is still sent. |
| F20 | style | major | FIX | Per-dashboard identity on reuse. |
| F21 | style | major | FIX | Business check name. |
| F22 | style | nit | FIX | Refusal and payload types (readiness mapping). |
| F23 | style | nit | FIX | §5 `origin_allowed`: every other method follows the POST rule; matrix row; item 25 case. |
| F24 | style | nit | FIX | No registry business. |
| F25 | codebase | major | FIX | Per-dashboard identity on reuse. |
| F26 | codebase | major | FIX | No registry business; `explicit` passes the name rule and unregistered keys are only orphans (§3); `src/business.rs:347-352`. Item 25 case. |
| F27 | codebase | nit | FIX | Pin text; `grep pinned apps/desktop/ui/command.js` has no match. |
| F28 | codebase | nit | FIX | Refusal and payload types. |
| F29 | codebase | nit | FIX | Refusal and payload types (`spawn_result` maps to `FailedToStart`, failing field logged). |
| F30 | codebase | nit | FIX | Item 9 `report_response`: `.and_then(\|mut d\| { d["hostBrief"] = serde_json::Value::Bool(true); cos::report::render(&d) })`, replacing `apps/desktop/src/lib.rs:451-452`. |
| F31 | readiness | major | FIX | Shared row cache; item 25 case: a new brief after `brief_seen` turns the dot back on within one tick. |
| F32 | readiness | major | FIX | Item 3: `comm` (`src/agents/mod.rs:794-801`, only caller `:806`) is deleted; its lookup moves into the off-macOS `proc::exe` fallback (item 4 cross-reference). |
| F33 | readiness | nit | FIX | Pin text. |
| F34 | readiness | nit | FIX | Business check name. |
| F35 | readiness | nit | FIX | Item 9: `brief_seen` copies `registry`, `triple.business`, `triple.home` and `ui_state_file` under the same guard. |
| F36 | readiness | nit | FIX | Refusal and payload types; JSON shape `{"kind": ...}` given for dash.js. |
| F37 | readiness | nit | FIX | Port 0. |
| F38 | readiness | nit | FIX | Shared row cache: synchronous `rows(..)` named in §6 Refresh and item 9. |
