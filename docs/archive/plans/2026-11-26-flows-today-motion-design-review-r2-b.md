---
design: docs/plans/2026-11-26-flows-today-motion-design.md
design-sha256: superseded (read 5b3e5e99f7a37e4aab99a6bc72ef2c88b023f854f6f324595cdc339e3b65349b)
verdict: findings
---

## Round 1 findings

1. **Closed.** Storage removed. Design §3 "Hires inbox" updates by live message, "needs no storage"; Alternatives rejects storage. Frame sandbox unchanged (`src/report.html`, `apps/desktop/ui/index.html:196`). No cross-reload glide claimed.
2. **Closed.** Design §3 "Brief loader" reuses `cos-brief-state` and states "No new message type". Live: `apps/desktop/ui/app.js:1360` posts it, `src/report.html:526` handles it. `cos-host` is still used for `hire`/`runBrief` (`app.js:242`, `report.html:521`) plus the new `hires` push.
3. **Partly closed.** Design §3 "Day timeline" says "rework of `today::ribbon`", names the 07:00–20:00 axis and now line, and lists the delta. Not yet stated: the fixed-now test reads `Ctx.now` (`src/today.rs:35`, `:46` `data["now_ts"]`, `:695` `now_m`). Carried.
4. **Partly closed.** Design §2 "Data" gives `flows::scan(…, home: Option<&Path>)` and callers `ritual.rs:574`, `main.rs:760`. Open parts:
   - Context (design line 47–48) says "plus the desktop's flows commands". Grep of `apps/desktop/src` finds no `flows::scan` call. The real internal caller is `src/flows.rs:918` (`scan_with`), which the Data section omits.
   - Test call sites `tests/flows.rs:281`, `:377` are not listed.
   Carried, not blocking: the plan can enumerate them.
5. **Closed.** `repo` is `flow.repo`: the page posts `repo: f.repo` (`src/flows.html:492`, `:477`, `:481`), `app.js:1500` passes it to `flowsHost.hire`, and `lib.rs:1763` writes `format!("bead:{repo}/{}", b.id)`. Non-matching `source` values are ignored by the `==` match. Test named in §Testing.
6. **Closed.** `Bead.status` is not used as a lane key. Design §2 table uses done / in progress / ready / blocked. `blocked` derives from `open_blockers` (`src/flows.rs:569`). Drift is on the rail (§2 Rail). Derivation of `ready` is not spelled out; carried as a wording nit.
7. **Closed.** §0 "The gate" covers `animation`, `animation-name`, `transition` declarations outside `no-preference`, the `<style>` blocks of report/flows/docs, textual `.animate(` outside a `motionOk()` function, and unused `@keyframes`. Fixtures in `each_rule_catches_its_violation` are listed in §Testing.
8. **Closed.** §0 keyframe table: `color-mix(in srgb, var(--warning) 18%, transparent)`.
9. **Closed.** §4 "Shimmer" is a variant, no new kind. `STATE` has `reading`, `refreshing`, `starting` (`apps/desktop/ui/states.js:117-124`), so the icon and art gate is unchanged.
10. **Closed.** §2 Ring: SVG `aria-hidden`, counts go on the card button's accessible name.
11. **Partly closed.** §3 KPIs: first-render rule added (no prior point, no roll). "Reads the last two points for the same kpi" is not stated; `src/kpis.rs:63,85` not referenced. Carried.
12. **Partly closed.** `task test-e2e`, `task screenshots`, `examples/wcag.rs` (acceptance 10) now named. Acceptance 9 cites a harness that does not exist; see New finding N1.

## New findings

**N1 (blocking). Acceptance 9 cites a harness that is not in the code.**
- Design line 446: "the e2e harness compares mtimes". Acceptance 9 is the only proof of "no new writes".
- Grep for `mtime` in `tests/`, `examples/`, `apps/desktop/tests/` finds only brief-file mtime fixtures (`tests/signals.rs`, `tests/report.rs`, `apps/desktop/tests/ipc.rs:1092-2682`). None walks the business home before and after rendering Today or Flows.
- §Testing E2E list (lines 389–401) does not add one.
- Result: acceptance 9 cannot be checked as written, and its stated basis is false.
- Fix: add to §Testing a named test (e.g. `tests/kit_page.rs` or `apps/desktop/tests/ipc.rs`) that snapshots `(path, mtime, len)` under the business home, renders Today and Flows (and `agents_list`), and asserts equality. Remove "the e2e harness" wording.

**N2 (verified clean, recorded so the plan does not re-check).** The claim "`snapshot` never reconciles, signals or writes" holds against the code:
- `read_raw` (`src/agents/mod.rs:801`) is `read_json` on `spec.json`/`state.json` only; `read` (`:795-798`) is the one that calls `reconcile`.
- `events::tail_summary` (`src/agents/events.rs:92`) reads through the local `read` (`events.rs:78`, returns `Vec<Event>`), not `agents::read`, and caches in a process-static `Mutex`. No disk write.
- Consequence: `snapshot` must be built from `read_raw` + `tail_summary`, not from `agents::read`, or the claim breaks. The design says this (§1), so this is a note for the plan, not a finding.

**N3 (carried). Hire pill bar has no deadline field.**
- §3 "Hire pills": bar of `active_ms` "against the hire's deadline". `HireView` (§1) lists no deadline.
- The deadline is `Spec.deadline_secs` (`src/agents/mod.rs:102`), read in `follow.rs:214-262`.
- Add `deadline_secs` to `HireView`. Not blocking: the plan adds one field.

**N4 (carried). Stored "working" phase shown as live after a lost run.**
- §1 "Lost runs": a run lost while neither runs is "shown as last stored" with "as last read" text.
- The pill "shimmers while working" and the inbox puts it in Running. Shimmer is a liveness claim (`ui-remote-states` rule 3).
- The "as last read" text must reach the pill's shimmer and column, not only the footer. Carried, since the design already names the text.

**N5 (carried). `cos-host` now carries data, not only host capabilities.**
- Round-1 finding 2 said keep `cos-host` for host capabilities. §3 now sends `{type:"cos-host", hires}` on `agents-changed`.
- Either document it as a deliberate second use or add a `cos-hires` type. The report handler must also check `event.source === window.parent` before re-rendering (the design's `postMessage(…, "*")` is already the existing pattern).

**N6 (carried). Gate scope for the report script.**
- §3 "Report script gate" covers only the `/* kit */` block. The existing report script (`src/report.html:648+`, `innerHTML` with `esc()`) stays ungated. Stated in the design, so acceptable; the plan should keep the block boundary explicit.

## Carried to Plan

- Round-1 3: name `Ctx.now` / `now_ts` (`src/today.rs:35,46,695`) as the fixed-now source for the timeline test.
- Round-1 4: fix Context line 48 (no desktop `flows::scan` caller); list `flows.rs:918` and `tests/flows.rs:281,377` in Data.
- Round-1 6: state that `ready` and `blocked` derive from `Bead.ready` / `open_blockers` (`src/flows.rs:569`).
- Round-1 11: state that the ticker reads the last two points of the same kpi only.
- N3: add `deadline_secs` to `HireView`.
- N4: "as last read" must reach the pill's shimmer and its inbox column.
- N5: decide `cos-host` vs `cos-hires`; add `event.source` check to the report handler.
- N6: keep `/* kit */` block boundaries explicit in the plan.
- Ribbon: the lead-phrase split test and the "no run id in visible text" test need fixtures named.
