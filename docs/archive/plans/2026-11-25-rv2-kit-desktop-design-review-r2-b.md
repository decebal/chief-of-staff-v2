---
design: docs/plans/2026-11-25-rv2-kit-desktop-design.md
design-sha256: superseded (read 67d70e7955b8f2decd2ed05806655f96e7d8121ad67cfa1e16029888b6ee887f)
verdict: findings
---

## Round 1 findings

1. closed — Inventory now has a row per Herdr call: run (design:559), send (:560), interrupt for pause and deadline stop (:561), kill (:562), rm (:563), focus (:564), read/get (:565). Matches follow.rs calls at :162, :254, :279, :288, :340.
2. closed — `screen.txt` is inventoried (design:566) and its reply-text content is named (:566, :517-520) and accepted. Harm 4 is scoped to new persistence (:548), so the accepted existing sink does not contradict it.
3. closed — KPI write moved to `history::record_kpis(home, ws)`, a separate no-network function (design:420-425). `gh` reads and `backfilled.json` are inventoried (design:568). `record` itself unchanged (history.rs:63-64).
4. closed — `agents_list` cost stated and cached: `tail_summary` keyed by (path, length, mtime) (design:289-292). Residual: see Carried to Plan, active_ms ticking.
5. open — boundary rule is false against code. Design says "newest `state.json` `runs[]` entry with status `ok`" (design:381-382). `runs[]` entries get `status: "complete" | "partial"` (track.rs:97-102), and are written only when tracking is approved (track.rs:66-71). `ok` exists only in the receipt `RunStatus::Ok` (pipeline.rs:73-77, 418-427, saved at :431). Primary path never matches. See New finding 1.
6. closed — merged PRs carry day only; design compares by day and states the limit (design:396-400). Residual gap: see New finding 2.
7. closed — `Row.title` added with `serde(default)`, set in `ledger::row` from `spec.title` (design:401-405). `ledger::row(spec, state)` already receives the spec (ledger.rs:48), and `Spec.title: String` exists (mod.rs:96).
8. closed — claim corrected: window and `cos dash /` only (design:521-528). `report::data` carries `history`, `last_run`, `incidents` (via `workspace`), `herdr_waiting` (report.rs:55-80). MCP not checked again this round (R1 covered mcp.rs:105-140).
9. closed — `word()` is an exhaustive match with `_ => None` (design:209); unknown word not stored (:226, :679); ISA test for a Herdr word `"<script>"` (:689). `Seen.word` is `Option<String>` (mod.rs:186), so the match is the only gate.
10. closed — `Judged` written only on a new verdict, never on the early return (design:233); dedup by `judged` timestamp (:241-242). Early return confirmed at follow.rs:411-413.
11. closed — `kpis.lock` flock around read-dedup-append (design:430-431), two-thread test (:690).
12. closed — signature stated (design:420-425). `record` is `(home, day, facts, brief)` (history.rs:63-64). `fold_latest_run` gains `ws`; its only caller `Cmd::Report` already has `ws` in scope (main.rs:445-446).
13. closed — `timeline.rs` (pure: `Kind`, `Event`, `word`, `fold`, `summary`) and `events.rs` (adapter: `append`, `read`, `tail_summary`) split and named (design:192-215).
14. closed — no-brief placement "after `herdr_waiting`" (design:416), against today.rs no-brief branch (R1 cited :245-251).
15. closed — word check compares in memory with `state.herdr`; log read only on change (design:226).
16. closed — lifetime stated: `clean` deletes the run dir with its `events.jsonl`; digest and scorecard read the ledger only (design:306-308). Ledger path root/`ledger.jsonl` survives (ledger.rs:44-46 per R1).
17. closed — `DeadlineStop` does not open a paused interval (design:263-264) and has a fold test (:701).
18. closed — `agents_list` at lib.rs:1070, `Row` at :1061, `ritual_approve` at :1592 (design:40-43, :112).
19. closed — `finishedAt` cited at `src/track.rs:100` (design:94).
20. closed — `apps/desktop/examples/preview.rs` (design:728-729).
21. closed — `sources()` picks up `kit.css` with no change (design:124-125, :158-159); `KIT_SLOT` gate added (:162-164).

## New findings

1. blocking §3 "Since your last brief", Boundary (design:381-384): the primary source does not exist. `state.json` `runs[]` entries have `status` `complete` or `partial` (track.rs:102), and `runs[]` is written only when tracking is approved (track.rs:66-71). `ok` exists only in the run receipt (`RunStatus::Ok`, pipeline.rs:73-77, 418-421, saved at :431). Evidence: track.rs:97-105; pipeline.rs:418-431. Fix: take the boundary from `out/runs/<day>.json` receipts only (status `ok`, day before today). Delete the `runs[]` clause, or restate it as `mode = "brief"` with `status` in `complete | partial`, and say `runs[]` has no failure entries.

2. blocking §3 "Done", merged-day limit (design:398-400): the stated guarantee is false. "PRs merged on <boundary date> are counted with that day's brief" holds only for PRs merged before that brief ran. `history.merged` is folded at brief time (pipeline.rs:466; history.rs:62-73 merges `facts.merged`), so a PR merged later on the boundary date is in no brief's `merged` list. The next digest then excludes it (day is not after the boundary date). Evidence: pipeline.rs:466; history.rs:62-73; design:396-400. Fix: add `merged_at` (RFC 3339, from the GitHub PR's `mergedAt`) to `Merged` at the point it is pushed (history.rs:211), compare timestamps against `finishedAt`, and delete the day-only limit. If day granularity is kept, state that same-day post-brief merges are dropped.

3. blocking §1 Appending (design:243-245) and Violations (:680): an audit line is written on append failure, but no file is named and no inventory row covers it. The Side-effect inventory (design:552-569) has no audit row. Existing audit log is `out/audit/<day>.jsonl` (claude.rs:402-404). Evidence: design:243-245, :680; design:552-569. Fix: name the sink (reuse `claude::audit_path`, or a new file) and add an inventory row with reversibility and effect.

## Carried to Plan

- should §4 "Kit defects fixed in the port": the kit already focuses `{next}-approve` after a decide (rv2-kit/src/components/pending_decision_stack.rs:228-233, `focus_next` set at :253). State the actual failing mechanism (id mismatch, timing, or unreached path) before calling it fixed. The `data-status` defect the brief names (brief:115) is not addressed in the design at all.
- should §3 KPI points: `kpi` is the KPI's `name` (state.rs:340 requires `name` and `unit`; no `id`). Define the key as `name`, and say how duplicate names are handled. The schema's `kpi` (design:624) and dedup pair (:427) need this.
- should §2/§1 active clock: `agents_list` refreshes only on `agents-changed`, so `active_ms` for a live run does not advance between refreshes (design:287-292). State the client tick (or the per-second recompute), and make `tail_summary` compute `active_ms` from the cached totals at call-time `now`, not at fold time (`summary(events, now)`, design:211).
- should §1 `Word.turn`: `Word.turn` is `Option<u32>` (design:202) but `Seen.turn` is `u64` (mod.rs:191). State the conversion. Also state when `turn` increments (the Seen docstring says "each time it stops working"), since the paused-interval close rule (design:259-262) depends on it.
- should §3 Boundary and Decisions: ledger `judged` is a `String` (ledger.rs:20), and `finishedAt` is a jiff timestamp string. Say the comparison parses both, and that "day before today" is local time.
- should §0 `window_never_writes_html_from_data`: "top-level window scripts" needs an explicit glob. `ui/dompurify/purify-3.4.17.min.js` contains HTML APIs (grep hit at :5). Name the file set, or exclude vendored dirs by path.
- nit §1 Refresh: the first `events.lock` create inside a hire dir moves that dir's mtime once, so `agents-changed` fires once more. Harmless; say so.
- nit §1 `cos agents clean` (design:306) deletes the run dir while a follower may hold `events.lock`. State the ordering or the failure mode.
