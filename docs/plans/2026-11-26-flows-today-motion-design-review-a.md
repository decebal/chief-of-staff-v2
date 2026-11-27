---
design: docs/plans/2026-11-26-flows-today-motion-design.md
design-sha256: e69ea6e8a096c5376ea10c4d37febdf432c1bc0dec0594f5b1c79d81cd13751f
verdict: clean
---

## Round 2 findings

- **N1 (blocking) closed.** Design §3 "Live updates in the window" now calls a new read-only `agents_snapshot` command that calls `agents::snapshot` only. Verified the old path: `apps/desktop/src/lib.rs:1097-1101` `agents_list` calls `agents::list`; `src/agents/mod.rs:794-797` `read` calls `reconcile`. `agents_list` stays unchanged and is out of scope, as the design says.
- **N2 (blocking) closed.** Live hires travel as their own `{type: "cos-hires", hires}` message. The `cos-host` handler at `src/report.html:521-525` (sets `canHire`, relabels `[data-hire]` and `[data-herdr-open]`, sets `#host-run` hidden) is untouched by the design.
- **N3 (not blocking) carried.** Acceptance 9 depends on the `report::data` callees being write-free. Checked now, not yet a test:
  - `history::load` and `history::backfilled` (`src/history.rs:143`, `:201`) only read. The writes are in `record` (:157) and `backfill` (:210–246), which `report.rs` does not call.
  - `kpis::by_goal` (`src/kpis.rs:43`), `delivery::view` (`src/delivery.rs:118`) and `constraints::view` (`src/constraints.rs:83`) contain no `fs::write`, `create_dir` or `commit(` calls.
  - `record_kpis` (`src/history.rs:76`, which creates a lock file and writes) is not called from `report.rs`. The design's Carried to Plan entry still stands as the check to run before the test lands.
- **Round 1 item 1** (hire read via `agents::list`) closed by the N1 fix.
- **Round 1 item 2** (desktop `Pages::scan` and `try_scan` callers, `home` source) carried in the design's Carried to Plan (`flows.rs:870`, `:885`; `apps/desktop/src/lib.rs:862`, `:907`). Counts as closed.
- **Round 1 item 9** (`/* kit */` gate bypassable by moving code outside the markers) carried in Carried to Plan as "covers the new code only". Counts as closed.
- **Round 1 item 15** (`el()` owner) carried as the fact, `const el` at `apps/desktop/ui/states.js:36`. Counts as closed.
- **Scan callers** (`flows::scan` home plumbing) carried. Closed by Carried to Plan.
- **Contrast gate for gradient stops** carried as a commitment. Closed by Carried to Plan.

## New findings

None blocking.

Checked and held:
- `Flow.repo` exists (`src/flows.rs:272`). `bead:{repo}/{id}` is built at `apps/desktop/src/lib.rs:1763` from the `cos-flows-hire` `repo` (`src/flows.html:492` posts `f.repo`). The source match in §2 is consistent.
- `agents::tick` and `cos agents watch` still reconcile, as §1 states.
- `tail_summary` (`src/agents/events.rs:92`) writes nothing; its only side effect is the in-memory cache.

## Carried to Plan

- **KPI roll "on first render":** the report has no storage in its opaque-origin frame, so a report reload re-rolls every tile. Define "first render" as first render per document load. Acceptance 7 cannot check "later renders do not roll" across reloads otherwise.
- **Acceptance 9 and `cn`:** `flows::scan` with a home spawns `cn` child processes. Confirm `cn` read commands write nothing under the home, or scope the walk to exclude the Chronis store, before `tests/read_only_render.rs` is written.
- **"As last read" heuristic:** "no `agents-changed` within 10 s of load" needs a test with a fixed clock, as the timeline already does.
- **Report::data write-freedom:** finish the N3 check above and record the result in the test's doc comment, not in the design.
