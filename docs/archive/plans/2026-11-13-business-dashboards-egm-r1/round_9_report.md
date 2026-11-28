# EGM round 9

- **Document:** [business dashboards design](../../../plans/2026-11-13-business-dashboards-design.md)
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

## Round 9 follow-up: Q2 and trim

Two owner decisions (2026-11-14), applied after the round. Status stays In Review.

### Q2 answer

"Yes, same rule." After SIGTERM to a leftover whose leader passed all five checks, launch cleanup sends one group SIGKILL with `proc::kill_group(pgid)` (`src/proc.rs:237`) while `proc::group_alive(pgid)` (`src/proc.rs:253`) is true and cleanup still holds `<port>.lock`, before the 5 s per-leftover deadline. Edits:

- Open Questions: Q2 and its Build gate removed; Q1 is the only open question.
- Decision bullet 2, Goal 7 and CUJ 7: cleanup stops any member left in the leftover's group with one SIGKILL.
- §4 Launch cleanup: step 4 splits into the deadline SIGKILL (leader still passes) and the group SIGKILL after the leader exits; `group_alive` runs immediately before that kill; at most one SIGKILL per leftover.
- Risks: the "member of a leftover's group" row is now the check-then-kill window row for launch cleanup. It states that the window is wider than the reaper's: launchd reaps the leftover leader, so the window spans from that reap to the `group_alive` check (at most one 50 ms poll).
- Semantic ISA: new `VERIFY(leftover_group)` checkpoint; S2, the `signal_group` instruction and its sink row name launch cleanup's `kill_group`; the pidfile-removal sink row and the `STORE(pidfile)` matrix row lose "pending Q2"; the `signal_group` matrix row gains an admitted entry; new matrix row "Lifecycle: leftover group member outlives the leader".
- Tests: item 23 gains the `serve-with-member` case after a simulated crash; the leftover is started through an intermediate process that exits, so launchd reaps its leader and the case exercises the new path, and the case asserts cleanup returns before the 5 s deadline.
- NFR Process ownership and the Confidence launch-cleanup row name the member case.

### What was trimmed, by section

- Detailed Implementation Plan: rewritten to the owner's scope (per file: changes, public signatures, IPC and event contracts, behaviour, tests and the matrix rows they cover). Removed: struct and enum field layouts (`Supervisor`, `Inner`, `Slot`, `Starting`, `Owned`, `Spawned`, `Exit`, `Waiter`, `StopTarget`, `ChildExit`, `Dashboards`, `Server`, `Dashboard`, `Probe`, `Signals`), derive lists and serde attributes, `Arc`/`Mutex`/`Condvar` layouts, the `SpawnFn` type, the `reap` parameter list, the `report_response` code snippet, the `blocking` closure's type reasoning, and justifications that concern only internals (why `list` is unusable, why `comm` would fail clippy). Kept: every public signature, the `Running`, `dashboard-exited`, `dashboards` row (`{name, url, managed}`) and `Refusal` contracts (kinds, fields, JSON shape, ISA mapping), the `businesses` row fields, the `Start` fields used by tests and `preview.rs`, and every test case. Items 19, 20, 22, 23 and 25 now name the matrix rows they cover.
- §4 Server supervisor: slot states kept as behaviour (Reserved, Starting, Owned, Exited, Refused) with what each holds; dropped `Arc<Mutex<Inner>>`, `Arc<Waiter>`, `Condvar`, `Arc::ptr_eq`, `Option<Arc<Waiter>>` and the exit-cell struct. The exit cell is now "the reaper's completion notice". Kept: the slot-identity rule, wake-ups, the stop-target rule, reaper order, the signal-under-lock invariant, every ensure step and refusal, the failure epilogue, launch cleanup, the stop path and all deadlines.
- §5: `dashboard_open` steps reduced to the ordering guarantee (business check and registry copy in one App-lock acquisition), the deadlock constraint and the outcome order; dropped `AppHandle` cloning, `Supervisor` not being `Clone` and `anyhow` types.
- §6: the shared row cache and row builder described by behaviour; dropped `Rows(Mutex<BTreeMap<..>>)` and the `rows(..)` signature; dropped the `pub(crate)`/`Held` detail (item 3a keeps the visibility change).
- §1, §3 and the ISA bindings: dropped Rust type names (`Triple.business`, `Option<pid_t>`, `u16`, `i64`, `Refusal::` prefixes) where a plain description carries the same rule.

### Rows and tests retained

Every Semantic ISA VERIFY, sink row, Violations row and test-matrix row from the pre-trim document remains, with wording adjusted only for the removed type names; one VERIFY and one matrix row were added for Q2. Every test case in items 3b, 17b and 19 to 25 remains; item 23 gains one. Every CUJ, Goal, Risk (one replaced per Q2), NFR and Confidence row remains. The doc has no em dashes.
