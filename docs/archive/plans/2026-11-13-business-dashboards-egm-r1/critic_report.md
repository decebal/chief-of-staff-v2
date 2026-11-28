# Goldfish Review Report

**Target:** `docs/plans/2026-11-13-business-dashboards-design.md` (lines 1-411, everything before `## Detailed Implementation Plan`)
**Stage:** 7, Critic Review, review-loop round 1
**Standard:** `feature-spec-standard` for egm-design and egm-structural; none for egm-style

## Reviewer Status

| Reviewer | Status |
| --- | --- |
| `egm-design` | completed |
| `egm-structural` | completed |
| `egm-style` | completed |

## Summary

| Severity | Count |
| --- | --- |
| 🔴 Blocker | 0 |
| 🟡 Major | 18 |
| 🟢 Nit | 22 |

Seven duplicate findings were merged into the finding listed under "Also raised by". Merged ids are not listed again.

The synthesizer checked two claims against source: `src/business.rs:29` declares `businesses: BTreeMap<String, Business>`, and `apps/desktop/tauri.conf.json:10` sets `"withGlobalTauri": true`. Both findings that depend on them hold. Reviewers left these unchecked: the control room's single-connection behaviour, which lives in another repository, and the current registry contents.

## Prioritized Findings

### 🟡 Major

#### design-r1-1: Lost and live signals go stale when a supervisor dies
* **Lines:** 223-224, 235-240, 405
* **Problem:** The sidebar recomputes a business only when its `stamp` changes. A dying process writes no file, and `stamp` (`src/dash.rs:181`) reads only the top-level entries of the agents root. A run that passes from Starting to Lost after `START_GRACE` (`agents/mod.rs:546`) also changes no file.
* **Rationale:** The lost-run warning in Goal 3 and CUJ 1 can stay wrong indefinitely, so the "rock" tier for signals is not earned.
* **Fix:** Recompute every business's signals on a slow timer (for example 10 s) with `proc::alive`, which is one `kill(0)` per run. Alternatively, fold supervisor liveness into the per-tick stamp. Add a CUJ: kill a supervisor and the warning appears within a stated time.

#### design-r1-2: A passing readiness check does not prove the app's child serves the port
* **Lines:** 184, 189-194
* **Problem:** Between the failed connect in step 3 and the spawn, another listener can take the port. Two app instances can race, because the mutex only covers one process. The child then fails to bind and exits, but `/health` on the other listener still passes `ready`. The slot is marked Owned, a foreign page is embedded, and the foreign process is signalled on exit.
* **Rationale:** This reaches the irreversible sink S2 with ownership unproven.
* **Fix:** Pass `ready` only when the child is still alive (`try_wait` returns None) and the listening pid is the child or in the child's group. Hold an exclusive lock on `servers/<port>.lock` across probe, spawn and pidfile write.

#### design-r1-3: A reclaimed process is group-killed without a group check
* **Lines:** 185-186, 198, 364
* **Problem:** `stop_owned` sends `kill(-pgid)` to a reclaimed pid. The pidfile has no pgid field, the design never checks `getpgid(pid) == pid`, and start time and exe are checked once at reclaim, not again just before the signal.
* **Rationale:** A SIGKILL to the wrong group cannot be undone.
* **Fix:** Store pgid in the pidfile. Check pgid == pid, start time and exe at reclaim and again immediately before signalling.

#### design-r1-4: Signals can reach a recycled process group, and the Child has two owners
* **Lines:** 193, 198-201
* **Problem:** The reaper marks a slot Exited, but `stop_owned` signals "each Owned slot". Once the leader has been reaped and the group is empty, the pgid can be reused (see the comment at `proc.rs:234-236`). The reaper thread owns `child.wait()`, yet `stop_owned` also "reaps each child".
* **Rationale:** An irreversible signal could go to an unrelated group, and two waiters on one Child is undefined ownership.
* **Fix:** Skip Exited slots. Make `stop_owned` reap through the reaper (a channel or a join) instead of its own wait, and name the single owner of the Child.

#### design-r1-5: A recycled `app_pid` blocks reclaim permanently
* **Lines:** 187
* **Problem:** If the recorded `app_pid` has been recycled to a live process, the server counts as owned by another instance. It is never stopped and runs until reboot.
* **Rationale:** This leaks a process, against the design's own process-ownership requirement (line 396).
* **Fix:** Record `app_start_time` and compare pid plus start time. State whether the app is single-instance (Tauri single-instance plugin) or whether multiple instances are supported.

#### design-r1-6: The binary check does not give the protection the design claims
* **Lines:** 191, 279-280, 350, 363
* **Problem:** Only paths under `out/` are refused. The declared binary is under `~/Projects/founder-mode/.../target/release/`, inside the `founding-bets` home, workspace and projects (line 248), so any agent working there can rebuild or replace it. `dashboards.toml` is agent-writable too (line 350).
* **Rationale:** The reason given for rejecting Alternative C ("lets any agent with workspace write access choose what the app executes") applies just as much to the chosen design.
* **Fix:** State the threat model plainly (workspace write access means code execution as the owner, and this is accepted), or pin the binary by hash, owner and mtime recorded at the hand build and refuse on mismatch. Then remove the security argument from Alternative C or make it true.

#### design-r1-7: A `null` Origin lets a dashboard frame through the `cosreport` check
* **Lines:** 214, 390
* **Problem:** The allowlist admits `null`. A dashboard frame has `allow-scripts`, so it can create a nested sandboxed, `srcdoc` or `data:` frame whose origin is `null`. That frame can POST to `cosreport://localhost/docs/organize` with the guessable `X-Cos-Business` header (`docs.html:366` reads it from the query). The rule "refuse when Origin is present and foreign" also fails open if WebKit omits Origin on custom-scheme requests, which has not been verified.
* **Rationale:** The control written to close the header hole can be bypassed by the frames it targets.
* **Fix:** For POST, require an Origin that is present and in the allowlist, without `null`. Or replace `X-Cos-Business` with a per-launch random token injected only into report frames. Add a `null` Origin from a nested frame and a POST with no Origin to the ISA test matrix, and add the Origin-omission question to spike Q1.

#### design-r1-8: Spike Q1 has no fallback and is written as already passed
* **Lines:** 60, 215, 292, 408
* **Also raised by:** style-r1-1, style-r1-2
* **Problem:** Q1 gates the whole embed design, but no outcome has a fallback. Lines 215 and 292 say "spike Q1 confirms", in the present tense, while lines 60 and 408 mark it open and rated sand. That makes Alternative E's rejection circular. `withGlobalTauri: true` (`tauri.conf.json:10`, confirmed) works directly against the requirement that `window.__TAURI__` be undefined in the frame.
* **Rationale:** A reader cannot tell what happens if the spike fails, and the document claims a result that does not exist yet.
* **Fix:** Make Q1 a written go/no-go gate before Build, with a fallback per outcome: a mixed-content block falls back to Alternative F or a `cosreport` proxy; `__TAURI__` visible in the frame falls back to Alternative E. Rewrite lines 215 and 292 as conditions ("spike Q1 must confirm", "the rejection holds only if Q1 passes"). Address `withGlobalTauri` explicitly.

#### design-r1-9: The single-connection control room is unsourced and unmitigated
* **Lines:** 41, 189
* **Also raised by:** style-r1-12
* **Problem:** "Serves one connection at a time with 2 s timeouts" has no source. WKWebView opens parallel keep-alive connections for the page, `sales.js` and polling, so probes on open can queue or time out while a frame is live, and the app's own frame can make its probe fail.
* **Rationale:** This makes page-load stalls and false identity refusals likely, and the design does not cover them.
* **Fix:** Cite the control-room file and line, measure the behaviour in the spike, and add a mitigation: skip the probe when the slot is Owned and alive, or fix the server's concurrency (noted as cross-repo work).

#### design-r1-10: The spawn environment is unspecified
* **Lines:** 192, 255, 340
* **Problem:** The server runs today as `cargo run -- serve` from its repository directory. A GUI-launched app spawns with cwd `/` and launchd's minimal PATH and environment.
* **Rationale:** Static assets, database files and relative paths can break. The result is either "failed to start" or a page that half works.
* **Fix:** Specify `cwd` (the binary's crate directory, or a configurable `cwd` key) and an environment policy (inherit or fixed). Add both to the `spawn_server` binding at line 340.

#### design-r1-11: Rejections fail the Alternatives Rule
* **Lines:** 15, 266-268, 272-274, 294-298
* **Also raised by:** structural-r1-2, style-r1-8
* **Problem:** Alternatives A and F are rejected on owner preference alone ("the owner wants", "the owner asked"), with no technical or scope trade-off. Decision ids U4, U6, D5 and D6 are never linked or explained. Every entry says "Rejected by author", but the reasons name the owner, so it is unclear who rejected what.
* **Rationale:** The standard requires concrete technical or scope trade-offs and forbids attributing a rejection to a person without a link.
* **Fix:** Link the decision rows in `2026-11-13-business-dashboards-brief.md` (structural reports U4 at line 23, U6 at line 25, D5 and D6 at lines 31-32 and 187-188). Give A a trade-off, for example the always-on resource cost and a lifecycle the app cannot see, against supervisor complexity and the crash gap. Give F one too, for example losing in-app context switching and dashboards that follow the business.

#### design-r1-12: The exit path is rated rock without evidence
* **Lines:** 196, 205, 394, 406
* **Also raised by:** style-r1-15 (the line 406 inconsistency)
* **Problem:** The design assumes ⌘Q (the predefined `.quit()` menu item, `lib.rs:925`) and window close both deliver `ExitRequested` or `Exit` on macOS, and that a 5 s blocking wait is allowed there. `stop_owned` runs from both events, so it fires twice, and idempotency is not stated. If the app itself receives SIGTERM (`kill`, some logout paths), servers are orphaned, and that gap is missing from line 394.
* **Rationale:** Goal 5 and CUJ 4 depend on this path, and line 406 rates it rock while still naming a test that would raise it.
* **Fix:** Make `stop_owned` idempotent. Add a test that ⌘Q from the menu reaches the handler, and rate the row sand until it exists. List SIGTERM to the app in the stated gap, or handle it.

#### structural-r1-1: No Risks & Mitigations section
* **Lines:** 392-410 (where the section should sit)
* **Problem:** The standard requires a Risks & Mitigations section. Risks are spread across ISA Violations (369-379), NFR Security (394), the sand rows in Confidence (407-409), Alternative H (310), the recycled-pid note (223) and the nested home (254).
* **Rationale:** A fresh reader cannot find the failure modes, scalability limits and mitigations in one place.
* **Fix:** Add `## Risks & Mitigations` after Alternatives as a table of risk, impact and mitigation. Suggested rows: an unsupervised server after a crash; pid reuse or a wrongly adopted pid; spike Q1 failing, with its fallback; probe markers drifting in the real pages; the `http://127.0.0.1:*` frame wildcard; a recycled pid inflating the live count; tick cost growing with the number of businesses and runs; `realestate`'s home nested inside `founding-bets`.

#### structural-r1-3: The `task schedule` port 8790 guard has no goal
* **Lines:** 258-260, 326, 410
* **Problem:** Section 6 changes `Taskfile.yml` behaviour, but no Problem bullet, Goal or CUJ covers it. Non-Goal line 53 ("moving the `cos dash` port") is related but different.
* **Rationale:** The work is hidden scope and cannot be traced to a requirement.
* **Fix:** Add a Problem bullet (the `cos.dash` default port collides with the gazette) and a Goal 9 with a pass condition, or move the guard to a separate change.

#### style-r1-3: "Owned" has two meanings
* **Lines:** 42, 113-114, 187
* **Problem:** Line 113 defines an owned server as one with a `binary` in config. Line 114 defines "Owned (started)" as spawned or reclaimed by this app instance. Line 187 then treats an owned server as external.
* **Rationale:** The reader cannot tell which meaning applies in Goals 5-7, section 2 or the ISA.
* **Fix:** Use "managed" and "unmanaged" for the config property, and keep "Owned" only for runtime slot state. Apply this throughout.

#### style-r1-4: Problem bullet contradicts itself
* **Lines:** 30
* **Problem:** "Four workspaces are not registered" is followed by "Kronos has no workspace at all". A workspace that does not exist cannot be unregistered.
* **Rationale:** The sentence misstates what is missing.
* **Fix:** "Four businesses are not registered (the registry holds `acme` and `default`). Kronos also has no workspace."

#### style-r1-5: "The only option" overclaims
* **Lines:** 274
* **Problem:** "A separate file is the only option that no binary version can rewrite." Alternative C (line 278) also says `cos` never writes the profile, so "only" is false.
* **Rationale:** The rejection rests on a claim the document itself contradicts.
* **Fix:** "A separate file, outside every workspace, is not rewritten by any `cos` version (U6)."

#### style-r1-6: "No network call leaves the machine" outruns the evidence
* **Lines:** 329
* **Problem:** Validation covers only the URLs the app loads. Embedded pages can fetch remote resources themselves, and `frame-src` does not limit what a framed page loads.
* **Rationale:** The claim is broader than what the design controls.
* **Fix:** "The app makes no non-loopback request; embedded pages keep their own network behaviour."

### 🟢 Nit

#### design-r1-13: Q2 assumes an order the registry cannot keep
* **Lines:** 61, 219
* **Problem:** "Registry order as written" cannot hold: `Registry.businesses` is a `BTreeMap` (`src/business.rs:29`, confirmed), and `write` re-serialises it sorted (`:116-123`).
* **Fix:** State that the order is alphabetical, or add an explicit order field.

#### design-r1-14: Registry and dashboards changes never propagate
* **Lines:** 235-240
* **Problem:** Neither `businesses.toml` nor `dashboards.toml` is stamped, so a business added with `cos business add` does not appear until restart.
* **Fix:** Stamp both files and reload on change.

#### design-r1-15: The clock check compares paths too literally
* **Lines:** 230
* **Problem:** "`COS_BUSINESSES` equals the app's registry path" fails when the plist has no `COS_BUSINESSES` and both sides use the default location.
* **Fix:** Compare resolved, canonicalised paths (`location_from` with the plist's environment).

#### design-r1-16: Reclaim with failed readiness is undefined
* **Lines:** 186
* **Problem:** After reclaim "readiness is probed", but the outcome when that probe fails (stop, refuse, or keep Owned) is not stated.
* **Fix:** Define it.

#### design-r1-17: The performance claim undercounts
* **Lines:** 395
* **Problem:** "Eight directory reads" ignores that `stamp` stats every entry (`dash.rs:184-186`), so cost grows with the number of runs, and a recompute parses spec and state JSON for each run.
* **Fix:** State the cost as O(runs) and give a bound.

#### design-r1-18: Smaller gaps in logging, probes and sandbox
* **Lines:** 161, 192, 211
* **Problem:** The server log is appended with no rotation or size cap. The gazette probe `<title>` matches any HTML page; it is marked as a placeholder, but validation has no minimum-specificity rule. `allow-popups` is never justified.
* **Fix:** Cap or rotate the log. Add a marker-specificity rule. Say what a popup does inside the Tauri window, or drop `allow-popups`.

#### structural-r1-4: Heading names drift from the standard
* **Lines:** 25, 107
* **Fix:** Rename to `## Problem Statement` and `## Proposed Architecture`.

#### structural-r1-5: The Decision comes before the motivation
* **Lines:** 11-13
* **Problem:** The conclusion appears before Overview and Problem, and line 13 is one block of about ten sentences.
* **Fix:** Move Decision after Problem, or split line 13 into bullets.

#### structural-r1-6: Mixed section numbering
* **Lines:** 109, 116, 136-258
* **Problem:** `### Terms` and `### Components` are unnumbered, and `### 1.` to `### 6.` are numbered.
* **Fix:** Number all of them or none.

#### structural-r1-7: Runbook content inside the architecture
* **Lines:** 242-256
* **Fix:** Move section 5 to its own `## Setup Runbook` or into the implementation plan.

#### structural-r1-8: Inconsistent bullet markers
* **Lines:** 3-7, 27-30, 49-56, 67-105, 111-114, 168-172, 266-310
* **Fix:** Use one marker throughout.

#### structural-r1-9: References are code spans, and one line citation is short
* **Lines:** 6-7, 15, 17
* **Problem:** The brief and base-design paths are code spans, not links. Line 17 cites `:58`, but the quoted phrase spans lines 58-59.
* **Fix:** Use relative Markdown links and cite `:58-59`.

#### structural-r1-10: A checkpoint is listed as a sink in the test matrix
* **Lines:** 390
* **Problem:** `VERIFY(origin_allowed)` is a checkpoint and is absent from the Sinks list at line 357.
* **Fix:** Rename the column to "Sink / checkpoint", or move the row to its own table.

#### structural-r1-11: CUJ coverage gaps
* **Lines:** 63-105
* **Problem:** No CUJ covers the lost-run warning or the schedule clock (Goal 3), or the binary-missing and failed-to-start states (Goal 8).
* **Fix:** Add short CUJs, or note that unit tests cover them.

#### structural-r1-12: Repeated non-goal
* **Lines:** 17
* **Problem:** "Per-business ports and launchd labels stay out of scope" repeats Non-Goal line 53.
* **Fix:** Drop it from line 17.

#### style-r1-7: "Four businesses are registered" reads as current state
* **Lines:** 13
* **Problem:** Line 30 and Goal 1 say registration has not happened yet.
* **Fix:** "This design registers four businesses: ..."

#### style-r1-9: "Cheap status signals" is a judgment
* **Lines:** 23
* **Fix:** "Status signals computed from file metadata, with no process spawn."

#### style-r1-10: Redundant wording in CUJ 2
* **Lines:** 75
* **Fix:** "`cos business list` shows the registry's active business unchanged."

#### style-r1-11: "Names the next step" does not say which step
* **Lines:** 93
* **Fix:** State the step, for example suggesting `lsof -nP -iTCP:<port>` and stopping the holder.

#### style-r1-13: Passive voice and a repeated sentence
* **Lines:** 223, 260
* **Problem:** At line 223, "which is accepted" does not say who accepts it. At line 260, "the brief job was loaded alone. The brief job is unaffected." says the same thing twice.
* **Fix:** "The design accepts this for a sidebar count." Drop the last sentence of line 260.

#### style-r1-14: Filler and jargon
* **Lines:** 266, 286, 310, 336, 340
* **Fix:** Line 266: drop "by design". Line 286: "a new port needs only a `dashboards.toml` edit". Line 310: "this gap stays open (see NFR Security)". Lines 336 and 340: "read-only HTTP GET" and "process spawn" in place of "deterministic I/O" and "deterministic action".

#### style-r1-15: Confidence tiers are undefined
* **Lines:** 401-410
* **Problem:** "Rock" and "sand" are never defined. (The line 406 inconsistency is merged into design-r1-12.)
* **Fix:** Define the tiers in one line above the table.
