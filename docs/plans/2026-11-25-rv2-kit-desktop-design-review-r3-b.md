---
design: docs/plans/2026-11-25-rv2-kit-desktop-design.md
design-sha256: superseded (read 4867a4706290c3780922fcdbb6f22814ee9167d96240b4453bd821b9aad47e75)
verdict: findings
---

## Round 2 findings

1. closed — r2-b items 1-4, 6-21 (r1 closures) were not re-derived this round; design text still matches the r2-b evidence (design:9-29 in r2-b numbering retained). Not re-checked: r2-b's verdicts stand.
5. closed — boundary now uses the run receipt. `RunReceipt{day, at, status}` at pipeline.rs:81-90, `RunStatus {Ok, Failed, Deferred}` at pipeline.rs:73-77, receipt built with `at: state::now()` at pipeline.rs:~418-426, saved by `store::save_json(Kind::Runs)` at pipeline.rs:~428. Design fallback uses `runs[]` `complete` (track.rs:102 writes `complete|partial`), matching the code. design:417-428.
New-1 (boundary, r2-b) closed — same evidence as 5. `ok` now comes only from receipts. design:417-423.
New-2 (merged-day gap, r2-b) closed — `Merged` gains `merged_at` from `closedAt` (gather.rs:287 already parsed; gather.rs:256 fetches it). `merge_prs` (history.rs:211) dedups by repo+number, so a PR merged after brief D is folded by brief D+1 (gather.rs:171 `--merged-at=>=merged_since`) with its `merged_at`. design:442-448. Residual: depends on D+1's `merged_since` lower bound reaching the merge day (see Carried).
New-3 (audit sink, r2-b) closed — audit row at design:621 names `out/audit/<day>.jsonl`, the path `claude::audit_path` returns (claude.rs:402-404). Violations row at design:733-734.

## New findings

1. blocking §0 Tokens/Kit builders (design:187-196, ISA row design:726, test matrix design:745): `window_never_writes_html_from_data` as specified only matches the literal `setAttribute("on…")` and `innerHTML`-family names. The window already calls `setAttribute` with a computed name: `states.js:38` `for (const [k, v] of Object.entries(attrs)) n.setAttribute(k, v);`. Any data-fed call of that shape passes the gate. The named exception (design:195-196) covers only `states.js:97`. Evidence: apps/desktop/ui/states.js:38; design:192, :198. Fix: the gate fails every `setAttribute(` whose name argument is not a string literal, with `states.js:38` named as the exception (vendored Lucide data, no user input). Add the fixture `n.setAttribute(k, d)` to `each_rule_catches_its_violation`.

2. blocking §0 `el()` (design:180-186, ISA row design:726): the "one door" claim covers `href` and `src` only. `el()` is `Object.assign(document.createElement(tag), rest)` (states.js:8-14), so every other property is written unchecked. URL-bearing properties outside the list include `action`, `formAction`, `poster`, `data` and `srcset`. A data-fed `{formAction: "javascript:…"}` or `{action: …}` reaches the DOM with no VERIFY, and the design's ISA says the sink is cleared. Evidence: states.js:8-14; design:183-186, :726. Fix: either an allowlist of props `el()` accepts, or route every URL-valued property (`href`, `src`, `action`, `formAction`, `poster`, `data`, `srcset`, `cite`, `background`) through `safeUrl()`, and add a fixture for `{formAction: "javascript:alert(1)"}` to the e2e `el()` test (design:198-201).

## Carried to Plan

- Boundary and `day` basis: receipt `day` comes from `dates_in` in the profile timezone (pipeline.rs:122-128), not system local time. design:419 and :425-428 say "local time". State "profile timezone" and use the same basis for both sides.
- Boundary fallback when a day's receipt is overwritten: receipts are one file per day, and the last run of the day wins (pipeline.rs:70-71 doc). A failed re-run on day D-1 makes the boundary fall back to D-2, which widens the digest. State this or key the boundary on the last ok run per day.
- Merged-day residual: a PR merged after brief D is picked up by brief D+1 only if D+1's `merged_since` (gather.rs:171) is on or before the merge day. Say what sets `merged_since`, or the late-merge guarantee in design:442-448 is only as good as that lower bound.
- `fold_latest_run` call sites (design:510-512 in Carried, "called at main.rs:445 as well as main.rs:1082"): main.rs:1082 is the `history::record` call inside `fold_latest_run`, not a caller. The only caller is main.rs:445. Fix the citation.
- `cos agents clean` ordering (design:867-870): the live-run refusal is `state.status.terminal()` at mod.rs:1019-1022. follow.rs:334-339 refuses only when the Herdr terminal is still live. Cite both, and state the failure mode when a follower holds `events.lock` on a finished run.
- `Judged` written only on a new verdict: design:254 and the test at :760 are consistent. No change needed.
- Carried "Paused.turn from `live()`": `live()` is follow.rs:221. Cite it there.
- ISA Status and Outcome schema enums (design:659-660) match agents::Status snake_case (mod.rs:131-145, 171-173) and judge::Outcome (judge.rs:69-75). No change needed.
- Herdr word vocabulary (`idle, working, done, blocked, error`) matches `Seen.word` comment (mod.rs:186-188) and osc.rs:15. The `Word` enum and `word()` are consistent.
- `Seen.turn: u64` (mod.rs:191) and `Seen.msg` (mod.rs:189) match design:223 and Carried text.
- Anchors checked and correct: agents_list :1070 and Row :1062 (design says :1061, off by one), agents_scorecard :1170, ritual_approve :1592, watch :1781, stamp use :1842, generate_handler :2043, dash::stamp src/dash.rs:194, `KIT_SLOT`-era gate names in tests/design.rs (flows gate :762, sources :23, WINDOW_JS :1085 - the design's `tests/design.rs` cite at :1085 is correct).
- `Row` line: design:1061 vs actual :1062. Nit.
- Nit: design:120-125 and 1-line `tests/design.rs` `sources()` cite are correct (:23).
