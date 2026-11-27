---
range: 82ca652..1b196c1
prove-it: CLAIM REFUTED — C5 (no marker covers another) fails after a minute on another tab; 2 hand-checks outstanding
verdict: clean
resolution: N1 and N2 fixed in the commit that adds this line (stacking waits for a visible axis and a ResizeObserver restacks on show; three rows inside a 5.25rem axis above the hour labels; kit_page asserts both). N3 and N4 stay nits: no current page hits them.
---

# Flows and Today, in motion: code review 2

Fresh-context review of the fix commit `1b196c1` on PR #61
(`feat/desktop-flows-today-motion`) against the first review
([2026-11-26-flows-today-motion-code-review.md](2026-11-26-flows-today-motion-code-review.md))
and the approved design
([2026-11-26-flows-today-motion-design.md](2026-11-26-flows-today-motion-design.md)).

## Checks run

| Command | Result |
|---|---|
| `gtimeout -k 15 300 cargo test --test design --test today --test flows` | design 21, flows 29, today 33 passed, 0 failed |
| `COS_E2E_CHROME=… gtimeout -k 15 300 cargo test --test kit_page --test flows_page -- --include-ignored --test-threads=1` | flows_page 17 passed, kit_page 21 passed |
| `cargo fmt --all --check` | no output |
| `cargo clippy --all-targets -- -D warnings` (root and `apps/desktop`) | `Finished` both |
| `cargo test` (root) | 43 suites `test result: ok`, 0 failed |
| `cargo run --quiet --example demo -- <tmp>/cos-demo` | `demo home ready: …` |
| `cargo test` (`apps/desktop`) | suites of 48, 2 and 63 passed, 0 failed |

## First review findings

| # | Status | Evidence |
|---|---|---|
| 1 marker not focusable | closed | `src/today.rs:1244` renders `<button type="button" class="kit-asb__marker" …>`; `apps/desktop/ui/kit.css:98` 1.5rem target, dot in `::before` (`:102`). Tests: `today.rs` `the_schedule_board_ports_agent_schedule_board_centred_on_now` (8 buttons, no `role="img"`); `kit_page.rs` `the_schedule_board_moves_under_the_now_line_each_minute_and_selects_a_row` (markers then rows in tab order, no `tabindex`, Enter on a marker selects its row and panel). |
| 2 live region always the count | closed | `apps/desktop/ui/kit.js:635-636,711`, `src/flows.html:826-827,870`: `"<step>: Needs you"` when a step is `awaiting_approval`, else the count. Tests: `kit_page.rs` `steps_mark_before_at_and_after_the_current_step_and_a_drift` (`"Approve: Needs you"`, drift gives the count); `flows_page.rs` `the_flow_views_steps_column_ports_step_progress_list` (`"Spec: Needs you"`). |
| 3 unused `kit-is-arriving` / `kit-is-failed` | closed | `git grep -n -E "kit-is-arriving\|kit-arrive\|kit-is-failed" -- src apps`: no `kit-arrive` left; `kit-is-failed` defined at `kit.css:239`, used at `kit.js:659` and `flows.html:857` on the errored step. Tests: both e2e above assert exactly one `li.kit-is-failed`, the drifted step. |
| 4 "Think, Done" | closed | `kit.js:640`, `flows.html:843` join non-empty parts. Tests: same two e2e assert `["Think", "Spec", …]`. |
| 5 lost / timed out / stopped read "Failed" | closed | `src/today.rs:1163-1165`; `kit.css:108` stopped dot. Test: `today.rs` `a_lost_timed_out_or_stopped_hire_says_so_on_the_schedule_board`. |
| 6 `.animate(` gate passes on any earlier `motionOk()` | closed | `tests/design.rs` `motion_guarded` (`:1666`), called at `:1854`; five new failing fixtures and two passing shapes in `each_rule_catches_its_violation`. Residual bypasses: new finding N3. |
| 7 cos-host check never set the host state | closed | `tests/kit_page.rs` `a_hires_message_moves_cards_between_columns_and_leaves_the_host_state` posts `cos-host` and asserts `[false, "Hire"]` before the hires message, plus the Open in Herdr labels after it. |
| 8 `page_cached` key without `home` | closed | `src/flows.rs` `page_cached` entry carries `Option<PathBuf>` and compares `h.as_deref() == home`. No test (one dash serves one home). |
| 9 flows hires in the system zone | closed | `flows::Hires { home, tz }`; every caller passes `digest::zone(ws)`: `src/main.rs:761`, `src/flows.rs` `page`, `apps/desktop/src/lib.rs:1667,1742`. Test: `flows.rs` `a_flows_hire_is_done_today_in_the_zone_the_scan_is_given` (mid-afternoon zone true, 1 am zone false). |
| 10 markers stack at one height | closed for the visible case; see N1, N2 | `src/report.html:866-878` `stack()`, `kit.css:100-101` `data-row`. Test: `kit_page.rs` schedule test, `overlaps` = 0 at load and after a 31-minute shift. |
| 11 raw `1.2s` | closed | `tokens.css:156` `--motion-spin`, used at `kit.css:242`. |

## New findings

1. should `src/report.html:866-878,885`: `stack()` measures `offsetLeft`/`offsetWidth`, which are 0 while the Today panel is hidden (`#left` with `hidden`, `display: none`, report.html:131). The minute tick (`setInterval(place, 60000)`, `:899`) calls `stack()` from `place()` whether or not Today is showing, so after a minute on Plan, Steering or History every marker gets row 0, and switching back to Today shows them on one line, covering each other's 1.5rem targets, until the next tick or a resize. `select()` (`:600-612`) re-runs `redraw()`, which does not stack. A resize while on another tab does the same. Reproduced with the same `stack()` body in headless Chrome (three markers 3 px apart in a 600 px axis): `visible=0,1,2 hidden=0,0,0 after-show=0,0,0`. Fix: skip `stack()` when the axis has no box (`!axis.getClientRects().length`), and re-stack when it gets one, for example a `ResizeObserver` on the axis in place of the window `resize` listener (it fires on resize and when the panel is shown). Add an e2e step: switch to Plan, fire the minute, switch back, assert `overlaps` = 0.
2. nit `apps/desktop/ui/kit.css:96,100-101`: a row-2 marker's dot is centred 1.5rem below the axis middle, at 52 px of the 56 px axis, so it spans 45-59 px: 3 px below the band, and across the hour tick labels (`bottom: 2px`, `text-xs`), which sit at about 38-54 px. Ticks are re-appended after the markers each minute (`axis.append(...ticks)`, report.html:896), so a label then paints over a dot that shares its x. A row-1 dot spans -3 to 11 px, 3 px above the band, clear of the trace (which ends 6 px higher). Readable today; collisions with a tick label are possible every two hours of axis. Fix, if wanted: grow the axis to fit three rows plus the tick line, or put row 2 above row 1.
3. nit `src/report.html:871-872`: with more than three markers inside one target width, every extra marker is forced into row 2 on top of the previous row-2 marker, so targets overlap again. At the 20rem width the design names, one target is about 8% of the 10 h axis (48 minutes), so four slots or runs within 48 minutes overlap. Fix, if wanted: a fourth row, or merge a cluster into one marker that selects its first row.
4. nit `tests/design.rs:1666-1722` `motion_guarded`: still passes some unguarded shapes: `if (motionOk() === false) { n.animate(…) }` (the plain branch only refuses `||`), `if (!motionOk()) returned = true; n.animate(…)` (`starts_with("return")` accepts any identifier beginning with it), and braces inside strings throw off the top-level count. `strip_comments` (`:71`) blanks `//` only after a space or newline, so a tab-indented `// if (!motionOk()) return;` above a call would also guard it. No current page uses any of these. Fix, if wanted: require the plain condition to be `motionOk()` or `motionOk() && …` exactly, and `return` followed by a non-identifier byte.

Stacking checks asked for:

- In the axis band: dots stay within 3 px of it (N2); hit areas extend 8 px beyond it, inside the timeline's padding.
- Minute tick and resize: `place()` calls `stack()` (`:885`); `resize` listener at `:901`. Covered by the kit_page `overlaps` assertion after `window.__minute`. Hidden-panel case broken (N1).
- Reduced motion: no transition or animation on `.kit-asb__marker` or `top` (`kit.css:98-109`; `every_animation_respects_reduced_motion` passes); rows change by a jump.
- No HTML from data: `stack()` writes only `dataset.row` with `String(row)`; `report_kit_block_never_writes_html_from_data` passes.

## prove-it

Run on `8f68a6a..1b196c1` (`git merge-base origin/main HEAD` = `8f68a6a`, after
`git fetch origin main`). PR #61 is `OPEN`. Printed here, not posted to the PR.

```
CLAIM REFUTED — C5 (no marker covers another) fails after a minute on another tab; 2 hand-checks outstanding

VERIFY BY HAND
- Dash: `cos --home <demo> --projects <demo>/projects dash --no-open` on a home from
  `cargo run --example demo`, open `/` and `/flows` in light and dark, then again with
  reduced motion on. Pass: Today shows the hero pills, the hires inbox and the schedule
  board with no marker covering another (also after a minute on the Plan tab and back);
  Flows shows the Steps column, Lanes and Beads, and "<step>: Needs you" at a gate; no run
  id in visible text; with reduce on, nothing moves.
- Desktop, live inbox: `task desktop` on the demo home with `COS_HC=tests/fixtures/fake-hc.sh`
  and its `FAKE_HC_STATE`; change a hire's word in the stand-in Herdr so `agents-changed`
  fires. Pass: the card moves column without a report reload, the Hire / Open in Herdr
  labels and Run brief stay as `cos-host` set them, `out/` and the run directories are
  unchanged apart from what the follower itself writes.

CLAIMS
C1 Flows lanes, steps, bead rings, beads glide, scene — HOLDS: flows_page 17 passed (e2e, Chrome).
C2 Today living hero, hires inbox, schedule board, ticking KPIs — HOLDS: kit_page 21 passed, today 33 passed.
C3 Every animation off under reduced motion — HOLDS: every_animation_respects_reduced_motion and
   each_rule_catches_its_violation pass; the stacking adds no transition (kit.css:98-109).
C4 Rendering writes nothing — HOLDS: read_only_render 2 passed in `cargo test`; the fix only threads a zone.
C5 Fix commit: first-review findings 1–11 closed; "a marker within one target of another takes the
   row above or below, so no button covers another" (docs/wcag.md) — REFUTED for the hidden-panel
   path (N1); every other finding HOLDS (table above).

FINDINGS
- src/report.html:866-878,885,899 · stack() reads offsetLeft/offsetWidth = 0 while #left is hidden;
  the minute tick resets every marker to row 0, and showing Today does not re-stack ·
  headless Chrome on the same stack() body: `visible=0,1,2 hidden=0,0,0 after-show=0,0,0` ·
  skip when the axis has no box; ResizeObserver on the axis in place of the window resize listener.
- nits N2–N4 in "New findings".

ATTACKS TRIED         I2 (`git diff --name-only 8f68a6a..HEAD -- .chief-of-staff context out` empty),
                      I3/I4/I13 (`git diff -G "Command::new|\.spawn\(|libc::kill|atomic_write|state\.json"
                      -- src apps/desktop/src`: one hit, a doc comment in src/agents/mod.rs `snapshot`),
                      I6 (dash.rs passes home to page_cached; no new route), I7 (kpis.rs data-from from
                      history points), I1/I5/I8/I14 (claude.rs, context.rs, mcp.rs, harness untouched). No hits.
CONSIDERED AND DROPPED
- Stacking data as HTML? No: stack() writes only dataset.row = String(row); report_kit_block_never_writes_html_from_data passes.
- Stacking unsorted markers overlaps? No: runs sort by at_ms (today.rs:1185), and a row accepts a marker only
  at least one width past its last, so rows 0–1 never overlap; only the forced row 2 can (N3).
- Comment-guard bypass of motion_guarded? Mostly closed: strip_comments runs first (design.rs:1852); tab-indented `//` survives (N4).
- flows hires zone wrong for desktop? No: lib.rs:1667,1742 pass digest::zone(&s.workspace).
```

Gates: `cargo fmt --all --check` no output; `cargo clippy --all-targets -- -D warnings`
`Finished` (root and `apps/desktop`); `cargo test` 43 suites `ok`, 0 failed;
`cargo run --quiet --example demo -- <scratch>/cos-demo` printed `demo home ready: …`.

Not checked: the live desktop window, `cos dash` in a browser, the WCAG evaluator
(`examples/wcag.rs`), `task screenshots`.
