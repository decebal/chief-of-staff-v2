---
range: 8f68a6a..82ca652
prove-it: UNPROVEN — 2 hand-checks outstanding
verdict: findings
---

# Flows and Today, in motion: code review

Fresh-context review of PR #61 (`feat/desktop-flows-today-motion`, head
`82ca652`) against the approved design
([2026-11-26-flows-today-motion-design.md](2026-11-26-flows-today-motion-design.md))
and plan ([2026-11-26-flows-today-motion-plan.md](2026-11-26-flows-today-motion-plan.md)).

## prove-it

```
UNPROVEN — 2 hand-checks outstanding

VERIFY BY HAND
- Dash: `cos --home <demo> --projects <demo>/projects dash --no-open` on a home from
  `cargo run --example demo`, open `/` and `/flows` in light and dark, then again with
  reduced motion on. Pass: Today shows the hero pills, the hires inbox and the schedule
  board; Flows shows the Steps column, Lanes and Beads; no run id in visible text; with
  reduce on, nothing moves.
- Desktop, live inbox: `task desktop` on the demo home with `COS_HC=tests/fixtures/fake-hc.sh`
  and its `FAKE_HC_STATE`; change a hire's word in the stand-in Herdr so `agents-changed`
  fires. Pass: the card moves column without a report reload, the Hire / Open in Herdr
  labels and Run brief stay as `cos-host` set them, `out/` and the run directories are
  unchanged apart from what the follower itself writes.

CLAIMS
C1 Flows lanes, steps, bead rings, beads glide, scene — HOLDS: tests/flows_page.rs 17 passed (e2e, Chrome).
C2 Today living hero, hires inbox, schedule board, ticking KPIs — HOLDS: tests/kit_page.rs 21 passed, tests/today.rs in `cargo test`.
C3 Every animation off under reduced motion — HOLDS for CSS and `.animate(` (tests/design.rs
   every_animation_respects_reduced_motion, fixtures in each_rule_catches_its_violation); gate weakness below (F6).
C4 Rendering writes nothing — HOLDS: tests/read_only_render.rs (2 tests) walks the demo home before/after
   report::data, today::hero/body, flows::scan(home), agents::snapshot; report.rs:51 no longer calls agents::list.
C5 Steps / schedule board ported as they are from rv2-kit — HOLDS with deviations F1–F3.

FINDINGS              see "Findings" below
ATTACKS TRIED         I1 (no brain tool change), I2 (`git diff --name-only 8f68a6a..HEAD -- .chief-of-staff context out` empty),
                      I3 (no new Command::new/.spawn in src or apps/desktop/src), I4 (no new libc::kill),
                      I6 (dash /flows passes home; no new route), I7 (KPI roll reads data-from from history points, kpis.rs:110),
                      I13 (no new state.json write site; mod.rs hit is a doc comment). No hits.
CONSIDERED AND DROPPED
- events::tail_summary writes? No: in-process cache only (src/agents/events.rs:92-115).
- report kit block HTML sinks? No: report_kit_block_never_writes_html_from_data passes and its fixtures fire.
- kit-rail leftovers? None: `git grep kit-rail` empty; `RAIL`/`rail-needs` in flows.html predate the base (8f68a6a).
- run id visible in Flows? Lane keys `hire:<id>` are Map keys, never text (flows.html:903-906).
```

## Checks

| Check | Result | Evidence |
|---|---|---|
| `cargo fmt --all --check` | clean | no output |
| `cargo clippy --all-targets -- -D warnings` (root, `-p cos-desktop`) | clean | `Finished` both |
| `cargo test` (root) | all `ok`, 0 failed | every `test result: ok.` in the run |
| `cargo test --test flows_page -- --include-ignored --test-threads=1` | 17 passed | `test result: ok. 17 passed` |
| `cargo test --test kit_page -- --include-ignored --test-threads=1` | 21 passed | `test result: ok. 21 passed` |
| `cargo test -p cos-desktop` | 0 failed | suites of 48, 2 and 63 passed |
| `cargo run --example demo` | builds the home | `demo home ready: …` |
| Brains read-only: `agents::snapshot` never reconciles | holds | mod.rs:939-958 reads `run_ids` + `read_raw` only; `tail_summary` caches in memory |
| `tests/read_only_render.rs` real and can fail | holds | walks path, length, mtime of every entry; second test uses a `Starting` run `agents::list` would rewrite |
| `report::data` no longer calls `agents::list` | holds | report.rs:51-64 uses `agents::stored` + `hire_view` |
| Motion gate rewritten, fixtures fire | holds | design.rs:835-885: animation outside, `transition:` no space, `.animate(` outside motionOk (two shapes), unused `@keyframes` |
| No HTML from data: window, flows.html, report kit block | holds | `flows_page_never_writes_html_from_data`, `report_kit_block_never_writes_html_from_data` |
| `cos-hires` checks `event.source`, leaves `cos-host` alone | holds | report.html:789-798 (`e.source !== window.parent`), only reads `canHire` |
| No run id in Today/Flows text | holds | kit_page.rs:891-895 asserts on `innerText`; today.rs puts ids in `data-*` only |
| Steps faithful to `step_progress_list` | mostly | DOM, marks, fold window, pill rule match; F2, F4 |
| Schedule board faithful to `agent_schedule_board` | mostly | CSS line-for-line in rem (kit.css:84-131 vs assets/agent-schedule-board.css); F1 |
| Markers within a marker's width | stacked, not offset | kit.css:97 absolute at `left`, same `top: 50%`; no collision pass in today.rs:1231-1253; the 20:13 needs-you marker sits under the 20:16 running one (sbs/cos-schedule-light.png). rv2-kit does the same (agent_schedule_board.rs:374-397) |
| Scope: dead code, narrating comments | one dead pair (F3); no narrating comments found | `git grep` of added comment words |

## Findings

1. should src/today.rs:1241: schedule markers are `<span role="img">`, not focusable; rv2-kit renders each marker as a `<button>` in the tab order (agent_schedule_board.rs:384-397), and the design says keyboard order matches the gallery. A keyboard user cannot reach a marker. Evidence: today.rs:1241 vs rv2 :384. Fix: render `<button type="button" class="kit-asb__marker" …>` with the same `aria-label` and `data-pick`; the existing click handler already picks on `[data-pick]`. **Fixed in the commit that adds this note:** 1.5rem buttons with the 0.875rem dot drawn centred, markers then rows in tab order (today.rs, kit_page.rs).
2. should apps/desktop/ui/kit.js:709 and src/flows.html:868: the polite live region always says the count. The design (Steps: "one polite live region names the step that needs you") and rv2-kit (step_progress_list.rs:280-291) announce the waiting step. At Approve the region never names it. Evidence: both lines pass `count` unconditionally. Fix: text = `"<step>: Needs you"` when an item is `awaiting_approval` (or `errored` with its reason), else the count; add the assertion to kit_page.rs and flows_page.rs. **Fixed in the commit that adds this note:** "<step>: Needs you" at the gate, as rv2-kit (awaiting only, no errored reason), else the count.
3. should apps/desktop/ui/kit.css:221,224,232,235: `kit-is-arriving`/`kit-arrive` and `kit-is-failed`/`kit-flash` are used by nothing. Evidence: `git grep -E "kit-is-arriving|kit-is-failed" -- src apps` matches only kit.css. The unused-keyframes gate passes because a gated class references them. Fix: delete both classes and keyframes, or wire `kit-is-failed` to the drifted step as the design's state table says. **Fixed in the commit that adds this note:** `kit-arrive`/`kit-is-arriving` deleted; the errored (drifted) step carries `kit-is-failed` in the window and on the Flows page.
4. nit apps/desktop/ui/kit.js:638, src/flows.html:841: a finished step's accessible name is "Think, Done"; rv2-kit's `item_name` drops the empty state word ("Think", step_progress_list.rs:202-209). Fix: join only non-empty parts. **Fixed in the commit that adds this note.**
5. nit src/today.rs:1163: lost, timed-out and stopped hires all read "Failed" on the board. Fix: map them to their words or say why "Failed" stands for all. **Fixed in the commit that adds this note:** Lost and Timed out keep the destructive tone; Stopped (asked for) is a neutral marker and pill.
6. nit tests/design.rs:1736: the `.animate(` rule passes when `motionOk()` appears anywhere earlier in the enclosing function (`if (motionOk()) log(); n.animate(…)` is clean). Fix: require a guard shape (`if (!motionOk()) return` before the call, or the call inside an `if (motionOk())` block) and add that fixture. **Fixed in the commit that adds this note.**
7. nit tests/kit_page.rs:860: the "cos-host state unchanged" check never sends a `cos-host` message first, so a handler resetting to defaults would pass. Fix: post `{type:"cos-host", hire:true, runBrief:true}` before reading `before`. **Fixed in the commit that adds this note.**
8. nit src/flows.rs:872: `page_cached`'s key is (ws, projects) without `home`. Harmless while one dash serves one home. Fix: add `home` to the entry. **Fixed in the commit that adds this note.**
9. nit src/flows.rs:737: `attach_hires` builds views in `TimeZone::system()`, not the profile's zone, so `done_today` on a flow's hires can differ from Today's. Fix: pass the workspace zone (`digest::zone(ws)`) through. **Fixed in the commit that adds this note:** `flows::Hires { home, tz }`; every caller has a workspace and passes its zone.
10. nit src/today.rs:1241, apps/desktop/ui/kit.css:97: markers closer than one marker width (0.875rem) stack at the same height; a needs-you marker can be hidden under a running one. rv2-kit has the same behaviour, so the port is faithful. Fix, if wanted: offset a marker whose position is within 0.875rem of the previous one by one marker height. **Fixed in the commit that adds this note**, because finding 1's buttons needed it: as 24 px buttons, two markers minutes apart covered each other and axe failed target-size (2 nodes, report-left). A marker within one target of another now takes the row above or below (report.html `stack()`, kit.css `data-row`); WCAG back to 0.
11. nit apps/desktop/ui/kit.css:238: `kit-spin 1.2s` is a raw duration beside the motion tokens. Fix: a `--motion-*` token. **Fixed in the commit that adds this note:** `--motion-spin` in tokens.css.

Not checked: the live desktop window with a real `agents-changed`, `cos dash` in a browser, the WCAG evaluator run (`examples/wcag.rs`), `task screenshots`.
