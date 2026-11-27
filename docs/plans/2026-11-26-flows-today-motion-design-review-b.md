---
design: docs/plans/2026-11-26-flows-today-motion-design.md
design-sha256: e69ea6e8a096c5376ea10c4d37febdf432c1bc0dec0594f5b1c79d81cd13751f
verdict: clean
---

## Round 2 findings

- **Round-1 3 (timeline fixed now). Closed.** Design Carried "Timeline clock" names the `Ctx` clock the ribbon uses. Live: `src/today.rs:46` reads `now_ts`, `:56` builds `Ctx`.
- **Round-1 4 (scan callers). Closed.** Carried "Callers that pass the home" lists desktop `Pages` scan (`flows.rs:870`, `:885`, `lib.rs:862`, `:907`) and `tests/flows.rs:281,377`. `flows.rs:918` (`scan_with`) is inside "every caller". Residual wording: design Context line 47–48 still says "plus the desktop's flows commands" and Data lists only `ritual.rs:574`, `main.rs:760`. Non-blocking, see Carried.
- **Round-1 6 (ready / blocked derivation). Closed.** Carried "Ready and blocked" states the derivation from `status`, `ready`, `open_blockers`. `Flow.repo` exists at `flows.rs:272`.
- **Round-1 11 (KPI roll previous point). Closed.** Carried "KPI roll" says second-newest by `evidenceAt`. Live: `kpis.rs:64` sorts on `evidenceAt`, `kpis.rs:86` reads it.
- **N1 (acceptance 9 harness). Closed.** Acceptance 9 names `tests/read_only_render.rs` and the walk. The false "e2e harness compares mtimes" wording is gone. Callee write-freedom, which Carried asks to confirm before the test lands, checked now: `report::data` (`report.rs:36–97`) writes nothing; only `report.rs:28–31` writes, outside `data`. `history::load` (`history.rs:143`), `history::backfilled` (`:201`), `kpi_points` (`:65`) are read-only. `history::record` and `backfill` write, but `data` does not call them. `digest::build`, `delivery::view`, `constraints::view`, `kpis::by_goal`: no write calls in their modules. `flows.rs`, `today.rs`: no write calls. `events::tail_summary` writes nothing; its only writer is `events::append` (`events.rs:28`), reached through `record` (`:67`), not from the read path.
- **N2 (snapshot pure read). Still holds.** Design §1 states `read_raw` plus `tail_summary`. Not re-verified beyond the above.
- **N3 (`deadline_secs` on `HireView`). Closed.** Design §1 `HireView` lists `deadline_secs`.
- **N4 (stored "working" shown live after a lost run). Open, narrowed.** Design Carried "As last read" now puts the text on the inbox heading only. The pill shimmer and the inbox Running column, which r2 asked for, are not carried. Design §3 still says the pill "shimmers while working". Non-blocking, see Carried.
- **N5 (`cos-host` vs data). Closed in design.** Design §3 and §Review history move live hires to `{type: "cos-hires"}`, leaving `cos-host` for host state. The `event.source === window.parent` check on the report handler is not in the design. Carried.
- **N6 (report script gate scope). Closed.** Carried "Report script gate scope" keeps the `/* kit */` block and leaves `esc()` templates out.
- **Ribbon fixtures (lead-phrase split, "no run id in visible text"). Open.** Design §Testing names both tests with no fixtures. Not in Carried. Non-blocking.

## New findings

None blocking.

- Checked and not a defect: the window's `agents-changed` event exists (`apps/desktop/ui/app.js`, `apps/desktop/src/lib.rs`), so acceptance 5's live update has a trigger.
- Checked and not a defect: `flow.repo` is the `source` key (`flows.rs:272`), so the `bead:{flow.repo}/{bead.id}` match in §2 Data is well-formed.

## Carried to Plan

- Context line 47–48: replace "plus the desktop's flows commands" with the Carried list of callers (`lib.rs:862`, `:907`, `flows.rs:870`, `:885`, `:918`, `tests/flows.rs:281,377`).
- N4: "as last read" must also reach the pill's shimmer (no shimmer for a stored phase) and the inbox Running column, not only the heading.
- N5: report handler for `cos-hires` checks `event.source === window.parent` before re-rendering.
- Ribbon: name the lead-phrase split fixtures (one with `;`, one with `:`, one with a 9-word lead, one with no separator) and the "no run id in visible text" fixture (a run whose id appears in `spec.source`, `msg` and `word`).
