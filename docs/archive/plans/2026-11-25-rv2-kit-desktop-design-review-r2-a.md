---
design: docs/plans/2026-11-25-rv2-kit-desktop-design.md
design-sha256: superseded (read 67d70e7955b8f2decd2ed05806655f96e7d8121ad67cfa1e16029888b6ee887f)
verdict: findings
---

## Round 1 findings

1. closed — `Event` now types `word: Option<Word>`, `status: Option<Status>`, `outcome: Option<Outcome>` (design :198-208). Matches `agents::Status` snake_case variants (src/agents/mod.rs:131-146), `judge::Outcome` (src/agents/judge.rs:69-76), and the schema enums (design :602-607).
2. closed — popover shows no message (design :371-372, :276); FOLD spans carry word and turn only (:276); privacy section says messages not stored by this change (:516). The Fleet and Run card read Herdr's current message from `state.herdr.msg`, which `Seen` already persists (mod.rs:189; follow.rs:53). Stated as existing, not new.
3. open — gate added (`window_never_writes_html_from_data`, design :175-181). Still missing: the e2e DOM test (no `task test-e2e` test renders a hire message with markup and asserts text nodes), and the ISA matrix row (design :692) names no DOM test. The `ipc.rs` citation is gone. New related defect in B2 below.
4. closed — unique temp name `state.json.<pid>.<n>.tmp` in Concurrency (design :247-250). Current code is fixed-name at mod.rs:1064 (`path.with_extension("json.tmp")`), so the fix is a plan item. Lost-update residue carried below.
5. closed — paused interval now closes on `Word=working` with `turn` greater than the `Paused` turn, plus `Said`/`MarkedDone`/`Ended` (design :254-262). Consistent with Herdr turn semantics (src/agents/mod.rs:190-191 "Goes up each time it stops working"; src/herdr.rs:191). Turn source at pause time carried below.
6. open — read bound is now a cache keyed by (path, length, mtime) (design :287-292). The cache as specified holds now-dependent output (`summary(events, now)` returns `active_ms`, design :211, :265). See new finding B3.
7. closed — refresh path verified: `watch` emits `agents-changed` on `dash::stamp([agents::root])` (lib.rs:1842-1844, poll every 700 ms at :1847). `stamp` reads only entries directly under the root (src/dash.rs:194-209). `WATCHED` (dash.rs:18) does not include `out/agents`, so the agents stamp is the only trigger. Appends do not move a hire dir's mtime. Design :297-304 is correct.
8. closed — inventory rows present: `hc interrupt` (pause; deadline stop) at design :561, `hc kill` (finish) :562, `judge.lock` :567, `events.lock` :554.
9. closed — `Hired` then `Ended` with no `Started`: `launch` on prepare failure calls `follow::end` (mod.rs:598-600), which calls `close` (follow.rs:361-370), which is the design's `Ended` writer. Testing (design :702) and acceptance #3 (:759-761) cover it.
10. closed — `agents_list` at lib.rs:1070, `ritual_approve` at lib.rs:1592, `agents_say` :1087, `agents_pause` :1101, `agents_done` :1110, `agents_scorecard` :1170, `agents-changed` :1844. `generate_handler!` at lib.rs:2043. Design :40-43, :295 correct.
11. open — data-contract fields verified for `finishedAt` (track.rs:100), `evidenceAt` (state.rs:41, :355), `observedAt` (state.rs:47), ledger `Row` fields (ledger.rs:18-42). The `runs[]` status claim is false: see B1.
12. closed — `history::record` keeps its signature (history.rs:64). `record_kpis(home, ws)` is a separate function (design :420-425). Call sites: pipeline.rs:466 and main.rs:1076-1082. The `fold_latest_run` caller at main.rs:445 is not listed (carried).
13. closed — split plan present: `kit.js`, `kit.css`, `timeline.rs` (pure), `events.rs` (adapter), `digest.rs`, `kpis.rs` (design :166-173, :188-218, :379, :434). `app.js` still grows (fleet, run card wiring, decision stack); carried.
14. closed — `tests/hire_events.rs` does not exist (ls); `tests/events.rs` (4.1K) is untouched.
15. closed — `.gitignore:2` is `/out` (grep).
16. closed — `const gates = {` at app.js:1537 (design :111). End line 1755 not checked.

## New findings

1. blocking Data contract / Today, Context (:93-96) and Today digest boundary (:381-384): the design says `state.json` `runs[]` holds status including failed and deferred runs (`RunStatus`, pipeline.rs:73-77), and the boundary is "`runs[]` entry with status `ok`". False. `runs[]` status is `running | complete | partial | blocked | interrupted` (src/state.rs:387-392, written at src/track.rs:102). `RunStatus` (`ok | failed | deferred`) is the receipt enum in `out/runs/<day>.json` (src/pipeline.rs:73-85). The state branch of the boundary never matches, so every digest falls through to receipts. Evidence: src/state.rs:387-392; src/track.rs:102; src/pipeline.rs:73-85. Fix: state the real enum. Boundary = `runs[]` with `status == "complete"` and `finishedAt` (track.rs:100), else receipt `status == ok` with `at` (RunReceipt, pipeline.rs:83-85). Fix the Context line. Fix Acceptance #4 ("no earlier ok run") to name the source it reads.

2. blocking Semantic ISA / Sinks, `RENDER(window_dom)` (design :673; Design :175-181, :529-531): the sink claim "strings reach the DOM only as text nodes via `el()`" is not enforced. `el()` does `Object.assign(document.createElement(tag), rest)` (apps/desktop/ui/states.js:8-14), so any data-keyed prop (`href`, `src`, `value`, `innerHTML`) is assigned as a property. The planned gate scans only for named HTML APIs by text (design :175-179), and cannot see `{[key]: data}` or `{href: data}`. `states.js:4` already says values reach the DOM "through textContent or a property". A kit builder that sets `href` from a hire or brief string is a `javascript:` URL sink with no VERIFY. Evidence: apps/desktop/ui/states.js:8-14, :4; design :673. Fix: `el()` forwards only an allowlist of props (`className`, `dataset`, `aria-*`, `hidden`, `tabIndex`); no data-keyed props. Any `href` built from data passes an http/https check before `el()`. Add a gate test with fixtures `{href: "javascript:…"}` and `{[k]: v}` that must fail, and record the allowlist in the ISA row.

3. blocking Timeline / Read cost (design :287-292, :211, :265; Acceptance #2 :756-758): the cache is keyed by (path, length, mtime) and "refolds only when the file changed". If the cached value is `Summary` (which carries `active_ms = (ended or now) − started − paused_ms`, :265), a live run's `active_ms` freezes between events. The run card clock (:345) and fleet "active time" (:323) would not advance. Evidence: design :211, :265, :289-292. Fix: the cache stores a now-independent fold (`started`, `paused_ms`, open pause start, `ended`). `active_ms` is computed from `now` on every read. Add a plan test: unchanged file, `now` advanced, `active_ms` advances.

## Carried to Plan

- Round 1 #3 remains: plan the e2e DOM test (`task test-e2e` harness, as `tests/flows_page.rs`), name it in the ISA matrix row for `RENDER(window_dom)`.
- Round 1 #6: once B3 is fixed, state the read bound: one `metadata` per run per list, one fold per change, fold cost O(events), with events bounded by Word changes per run.
- Lost update: `observe` (follow.rs:91-160) reads the whole state, mutates, and rewrites it. Two followers can overwrite `ended` or `verdict` with a stale snapshot (`write_json` rename is atomic but last-writer-wins on stale content). The unique temp name does not fix this. Either take a state lock in `observe`/`end`/`judge` or state the risk in Concurrency. Harm #1 (:545) is not met by the temp-name change alone.
- Paused interval turn source: `pause` (follow.rs:252-256) reads the terminal fresh in `live()`. Specify `Paused.turn = t.turn` from that read, not from `state.herdr.turn`, to avoid a stale turn closing the interval early.
- KPI identity: state KPIs have `name` and no `id` (state.rs:340). The design's `kpi` / "kpi id" (:427, :622, :670) must be `name`. Uniqueness of `name` is not validated; say what happens on duplicates.
- Harm #4 (:548) excludes spec titles, but `STORE(ledger_title)` persists them (:637, :515). Reword harm #4 to cover persisted titles, or drop the title from the ledger.
- `tests/design.rs:1085` `WINDOW_JS` omits `kit.js`. Add it so `every_state_renders_through_state_view_with_an_icon_and_art` covers the kit builders. `sources()` (tests/design.rs:23-37) already picks `kit.js` and `kit.css` up for the HTML-sink gate.
- `fold_latest_run` is also called at main.rs:445 (`fold_latest_run(&t.home)`). List it with the `ws` parameter change (:424-425).
- `RunReceipt.at` is pipeline.rs:85 (design cites :83 in the r1 text); cite the field line.
- `report.html` client script: the trend card's arrow-key stepping (:445-447) lives in a page whose renderers use `innerHTML` with `esc()` templates (report.html:647-1073). Any readout string added there must go through `esc`.
- `app.js` (1,799 lines) still grows with fleet, run card wiring, timeline wiring and decision-stack wiring. No size gate exists in `tests/`; the 1,500-line rule is convention. Name the target size for `app.js` after the split.
- Herdr message persistence: `Seen.msg` is already in `state.json` today (mod.rs:189, follow.rs:53). Say so in the privacy section, so the Fleet card's text source is not read as new storage.

Counts: round 1: 13 closed, 3 open (#3, #6, #11). New: blocking 3, should 0, nit 0 (rest carried). Verdict: findings.
