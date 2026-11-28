# EGM round 11 report

- **Round:** 11
- **Document:** `docs/plans/2026-11-13-business-dashboards-design.md` (Status stays In Review)
- **Reviewers:** design, comprehension, structural, style, codebase, readiness
- **Findings:** 34 (major 9, nit 25)
- **Dispositions:** FIX 34, CONTEST 0, DECLINE 0, ESCALATE 0
- **Verdict:** FINDINGS

Several findings repeat across reviewers. Duplicates share one edit and are marked "dup of #n".

| id | reviewer | sev | disposition | edit/evidence |
| --- | --- | --- | --- | --- |
| 1 | design | major | FIX | Verified: `Supervisor::new(data_dir, log_dir, emit, spawn)` had no probe seam, and a refused connect returns at once. Added an injected `connect` (key address to "listener answered") to `Supervisor::new`, with `real_connect` for the app (item 8, item 9 builder, §4 ensure: probe). Matrix row "quit before the port lock is taken" and its item 23 case now block `connect` on a barrier: `stop_owned` returns at its 5 s deadline with nothing signalled (§4 Stop path, closing paragraph), then the released `connect` reports no listener and `ensure` refuses at lock-take with "the app is quitting"; no spawn, no pidfile, lock free. |
| 2 | design | major | FIX | §6 Events already limits the payload to registered businesses. Shared row cache now states that a changed `businesses.toml` replaces the whole cache with the row builder's full output, dropping unregistered names; same in §6 Refresh recompute bullet and item 9 rows bullet. New item 25 case: a business removed from the registry is absent from the next `businesses-changed` payload. |
| 3 | design | nit | FIX | `lsof` exits 1 with no match. §7 step 5: the step does not fail on that exit; non-zero exit with empty output means no listener; `Held` when any LISTEN line has a pid other than the old pid, reporting the first such line. |
| 4 | design | nit | FIX | Folded item 23's "quit between spawn and recording the child stops the new group" into the barrier case and mapped it to matrix row "quit between spawn and recording the child". Matrix row restated to the reachable state: the fake spawn starts its child and waits on a barrier before returning. |
| 5 | design | nit | FIX | Verified: `app.js:4,5,11,23` declare top-level `invoke`, `listen`, `$`, `ui`; `command.js:2-3` uses an IIFE. Item 12 now requires `sidebar.js` and `dash.js` to wrap their code in a function scope, take `invoke`/`listen` from `window.__TAURI__`, touch `ui`/`$` only after startup, and expose only `window.loadDashTabs`, `window.openDash`, `window.hideDash`. Items 13 and 14 restate their own exposure. |
| 6 | comprehension | major | FIX | Verified: identity after readiness gave `Identity{owned:false}`, whose card names "port held by something else" and `lsof`, although the child it stopped may have been the listener (Risks, marker drift). `Identity` gains `spawned`, true only for the post-readiness probe. Edited §4 identity after readiness, `VERIFY(identity)` binding, §Violations (identity row and post-readiness row), matrix `RENDER(iframe src)` and "foreign listener before the child binds", Risks (marker drift, foreign listener), item 8 `Refusal` contract, item 14 card text. New item 23 case: own fake server passes `ready`, page lacks marker, `spawned: true`, child stopped, no pidfile. Refusal kind and the stop behaviour are unchanged; only the card's hint changes. |
| 7 | comprehension | nit | FIX | §4 Slots Exited bullet now shows `exited` only when the slot was Owned, and names Reaper step 2 for a Starting slot. |
| 8 | comprehension | nit | FIX | dup of #5. |
| 9 | comprehension | nit | FIX | Item 15: `brief_seen` is called with `ui.business` and the payload's `brief` and `mtime` (`docs_save` passes `ui.business`, `app.js:178`). |
| 10 | comprehension | nit | FIX | §3 and item 7: the file is loaded and validated first; an invalid file prints key path and rule to stderr and exits 1 before any name lookup. |
| 11 | structural | major | FIX | §4 Launch cleanup says cleanup does not wait for the group. Chose the test-side poll so behaviour stays: item 23 case asserts the pidfile removed at return, then polls `proc::group_alive` and the port every 50 ms for up to 1 s. Matrix row "leftover group member outlives the leader" and Confidence row "Port lock and launch cleanup" say the same. |
| 12 | structural | nit | FIX | Added Terms entry Triple (`src/business.rs:337-343`, `label` at `:347`). |
| 13 | structural | nit | FIX | Item 19 gains: duplicate dashboard `name` in one business refused with key path; a business key that breaks `business::valid_name` (`src/business.rs:68-72`) refused. |
| 14 | structural | nit | FIX | New item 25 case: invalid `dashboards.toml` at setup with a pidfile naming a live test child: setup completes, pidfile removed, child alive. Item 25 scope lists the setup side of the `signal_group` entry. |
| 15 | structural | nit | FIX | dup of #16. |
| 16 | style | major | FIX | Verified: `docs_response` refuses a POST without a matching `x-cos-business` (`apps/desktop/src/lib.rs:479-491`). Matrix `VERIFY(origin_allowed)` and item 25 now send every non-GET case to `cosreport://localhost/docs/organize` with matching `x-cos-business` and a valid body, assert each 403 body is not the header-refusal text (`lib.rs:489`), and use the same request with `Origin: tauri://localhost` for the 200; GET cases target `/`. |
| 17 | style | major | FIX | Verified: `post_organize` calls `docs_response` (`apps/desktop/tests/ipc.rs:1122`); preview calls inner handlers (`apps/desktop/examples/preview.rs:166-168`). Item 25: `post_organize` sends `Origin: tauri://localhost` through `cosreport_response`. Preview not rerouted: it builds requests without an Origin, so the POST rule would refuse its docs saves; §5 now states that preview calls the inner handlers and does not apply the rule, and that GET-only tests calling the handlers directly test the handlers. Item 9 and item 9a restated. |
| 18 | style | major | FIX | Verified: every other desktop start input is read once in `Start::from_env` (`apps/desktop/src/lib.rs:43-67`). `Start` gains `dashboards_file` from `COS_DASHBOARDS`; `location(registry, explicit)` reads no environment; the CLI passes its env value. Edited §3, item 1, item 9 `Start`, item 9a, item 25 `start()` literal. |
| 19 | style | nit | FIX | Added Terms entry Start registry (path fixed at start, contents re-read). Replaced "the app's current registry" and "beside the start registry" with "its location (§3)" in §4 Launch cleanup, §5 Fresh load, item 1, item 9 builder. |
| 20 | style | nit | FIX | dup of #10. |
| 21 | style | nit | FIX | Verified: control room answers `HTTP/1.1` (`control-room/src/server.rs:247`). Item 8 example uses `HTTP/1.1 200 OK`. |
| 22 | style | nit | FIX | dup of #12. |
| 23 | style | nit | FIX | Verified: `run_task` passes `LAUNCHCTL=<stub>` as a CLI arg (`tests/schedule.rs:134`); `LAUNCHCTL` is a Taskfile var (`Taskfile.yml:6`). §7 step 5, item 18, item 22: `DASH_PORT` is a Taskfile var with default 8790, passed by tests as `DASH_PORT=<port>`; the step reads `{{.DASH_PORT}}`. |
| 24 | codebase | major | FIX | Added matrix row `STORE(render_dash_plist)` (`tests/schedule.rs`). Item 22 lists it and adds assertions: Timeout leaves the installed plist byte-identical, Free leaves no `.new`, text scan checks the render writes only `.new` and removes it on failure. |
| 25 | codebase | nit | FIX | dup of #18. |
| 26 | codebase | nit | FIX | Verified: `start()` literal at `apps/desktop/tests/ipc.rs:89-97`. Item 25 lists the three paths plus `dashboards_file`. |
| 27 | codebase | nit | FIX | dup of #14. |
| 28 | codebase | nit | FIX | dup of #16. |
| 29 | readiness | major | FIX | Verified: `> "{{.LAUNCHD}}/cos.{{.ITEM}}.plist.new"` creates `.new` before `sed` can fail (`Taskfile.yml:213-215`). Chose option (a), which makes S9d's existing statement true: §7 step 1 removes `.new` on a failed render, then fails the task. `TOOL_CALL(remove_dash_plist)` binding and VERIFY-before-sink row admit a failed render (`.new` only); matrix `remove_dash_plist` row and item 18 updated; item 22 text scan asserts it. |
| 30 | readiness | nit | FIX | Verified: `spawn_grouped` returns `anyhow::Result<Child>` (`src/proc.rs:11,165`). Item 8: `real_spawn` and the injected `spawn` return `anyhow::Result<std::process::Child>`. |
| 31 | readiness | nit | FIX | dup of #26. |
| 32 | readiness | nit | FIX | dup of #18. |
| 33 | readiness | nit | FIX | Verified: `missing` and `counts: Option<Live>` (`apps/desktop/src/lib.rs:385-410`); ⌘K shows `home is missing` (`apps/desktop/ui/command.js:33-36`). Item 13: a missing row shows no live, unread or lost glyph, the text `home is missing`, and stays clickable. |
| 34 | readiness | nit | FIX | dup of #3. |
