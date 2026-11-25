---
range: e26ad52..cdad1cd
prove-it: 🟡 MERGE AFTER CHECK
verdict: findings
---

## Previous findings

Source: `docs/plans/2026-11-25-rv2-kit-desktop-code-review.md` (range e26ad52..1ebbcaa), fixed in cdad1cd.

| # | Finding | Status | Evidence |
|---|---|---|---|
| 1 | `src/history.rs` accepts blank KPI name | closed | `src/history.rs:126` `if name.trim().is_empty() \|\|`; `tests/history.rs:186-187` blank and whitespace names added; `cargo test --test history` 9 passed |
| 2 | fleet card falls back to raw run id (`kit.js:116-117`) | closed | `apps/desktop/ui/kit.js:115-117` label and text use `r.title`; title is `spec.title?.trim() \|\| "Untitled hire"` at `kit.js:82`; id only in `dataset` at `kit.js:117` |
| 3 | run card title `spec.title \|\| spec.id` (`kit.js:82`) | closed | `kit.js:82` now `spec.title?.trim() \|\| "Untitled hire"`; `kit.js:81` `id: spec.id` is data, not rendered |
| 4 | step bead label falls back to tracker id (`kit.js:565`) | closed, deviates | `kit.js:565` `b.title?.trim() \|\| "Untitled bead"`. Finding asked for the status word; a fixed word also keeps the id off screen. Accepted |
| 5 | design doc `states.js` line refs stale; ledger fallback text wrong | OPEN | not touched by cdad1cd. See Drift D1, D2 |
| 6 | PR #57 body names `tests/agent_events.rs` | OPEN | `gh pr view 57` body still lists `tests/agent_events.rs` (file does not exist; tests are in `tests/hire_events.rs`) and omits `tests/kit_page.rs`. Body edit, no commit |

Fix commit also claims "the error goes to logDev": true. `kit.js:584` `logDev("agents_timeline", e)`; `kit.js:585` detail is a fixed sentence.

## prove-it

Frame: `BASE` = `git merge-base origin/main HEAD` = `e26ad529f60147c589eb8e85fac55fb869a581d9`. PR #57 `OPEN`. `git diff -M --name-status` over 61 files, no `.chief-of-staff`, `context` or `out` path (I2 empty, as required).

Verdict line, verbatim from the prior review's format:

🟡 MERGE AFTER CHECK. No refuted claim and no blocking finding. Hand-checks outstanding: 6 as listed below; the ipc and WCAG items have session runs cited, the other four do not.

Note: SKILL.md §5 template words are `CLAIMS HOLD | CLAIM REFUTED | UNPROVEN`. Every claim below holds or is unverifiable here; the MERGE vocabulary is kept from the prior review.

Gates, this session (cdad1cd HEAD):

| Gate | Result |
|---|---|
| `pgrep -x rustc` | empty (exit 1) before every cargo call |
| `cargo fmt --check` | exit 0, no output |
| `cargo test --test history` | 9 passed (includes fixed blank-name case) |
| `COS_E2E_CHROME=... cargo test --test kit_page -- --include-ignored --test-threads=1` | 11 passed (kit.js changed by the fix) |
| `cargo test --test design -- window_never_writes_html_from_data each_rule_catches_its_violation` | 2 passed |
| `cargo test --test today` | 22 passed. Prior review said 23; count at cdad1cd is 22, no failure |
| `task test` (39 binaries, 0 failed) | cited from session facts, not rerun |
| `task test-e2e` (flows_page 9, kit_page 11) | cited from session facts |
| `cargo test -p cos-desktop --test ipc` (48 passed) | cited from session facts |
| WCAG evaluator, 24 states light and dark, 0 violation nodes | cited from session facts |

VERIFY BY HAND (copied verbatim from the prior review, block two, never last, never elided):

- Brief, live (paid, about $0.35): `cos brief --no-open`. Pass: exit 0; `out/history/kpis.jsonl` gains one line per KPI with a finite `current` and a parseable `evidenceAt`; a second brief on the same `evidenceAt` adds no line; Today shows "Since your last brief" after the actions, or is absent when no earlier ok run exists.
- Agent, live (paid, capped): `cos --budget 0.25 agents spawn --model haiku --repo <throwaway repo> --prompt "run: echo hi > a.txt; then run: curl -sS https://example.com"`, then `cos agents show <id>`. Pass: `done`; `out/agents/<id>/events.jsonl` has `hired`, `started` and `word` lines and no free-text field; the curl is refused by the sandbox.
- Pause then reply, on the window's stand-in Herdr: `cargo test --manifest-path apps/desktop/Cargo.toml --test ipc`. Pass: the pause-then-say test gives one closed paused interval in `agents_timeline`, and the approval test stays green. Status: ipc 48 passed this session; the named test is not individually re-read here.
- Dash: `cos dash --no-open`, open `/` and `/agents` in light and dark. Pass: the fleet and run card render, with no raw run id in the fleet, and the run card's clock is still while held.
- WCAG: `cargo run --example wcag` on the Agents, Today and Flows views, light and dark. Pass: no new violations against the baseline. Status: 0 violation nodes this session.
- Screenshots: `docs/assets/fleet*.png`, `run*.png`, `flow-decisions*.png` come from the Taskfile section that runs `examples/preview` on `/tmp/cos-demo`. Pass: the demo home is the source (`task screenshots`), and the images show no run id in the fleet or Today.

CLAIMS

- C1, fleet, run card, timeline, decision stack: HOLDS for the rendering paths in the 11 kit_page e2e tests (rerun this session). Live window behaviour is the Pause-then-reply hand-check.
- C2, append-only `events.jsonl` and `kpis.jsonl`, written by cos: HOLDS. Writers in the sink table below; no brain code path in the diff.
- C3, KPI points recorded at brief time only: HOLDS. `record_kpis` has one caller, `src/pipeline.rs:467`; `fold_latest_run` (`src/main.rs:1076`) does not call it.
- C4, digest after actions, dates in words, markup escaped: HOLDS (`tests/today.rs`, 22 passed; `view_text_cannot_inject_markup`, `a_digest_item_shows_its_time_in_words_and_escapes_its_title`, `a_kpi_name_with_markup_renders_escaped`).
- C5, approval stays `ritual_approve`: HOLDS on the ipc suite as cited (48 passed); approval test not individually read.
- C6, no new spawn on render or poll path: HOLDS per prior review; not re-grepped this pass.
- C7, PR body test plan names real tests: NOT HELD. `tests/agent_events.rs` does not exist (finding 6, still open).

ATTACKS TRIED

- I1 brains read-only: `src/claude.rs` not in the diff.
- I2 no committed state: `git diff --name-only e26ad52..cdad1cd -- .chief-of-staff context out` prints nothing.
- I13 state.json written only through `state::commit`: new writes are `out/agents/<id>/state.json` via `write_json` (`src/agents/mod.rs:1107-1115`) and `out/history/`. No new workspace-state write in the range.
- Second item, failing item, concurrent run for events: `events::append` takes `events.lock` and dedups last Word and Judged under the lock (`src/agents/events.rs:28-63`); failure goes to audit via `events::record` (`events.rs:67-75`).
- KPI concurrency: `record_kpis` takes `kpis.lock` and dedups the (name, evidenceAt) pair under it (`src/history.rs:81-103`).
- Retry and abort on `write_json`: see finding N1.

## Sink to VERIFY

STORE(event): every call reaches `append` only through `events::record`, `src/agents/events.rs:67`.
- `src/agents/follow.rs:167, 178, 223, 311, 327, 348, 502, 557`: 8 sites, all `events::record`. Checked.
- `src/agents/mod.rs:581, 654, 662`: 3 sites, all `events::record`. Checked.
- `examples/demo.rs:355, 433`: call `events::append` directly. Example only, not production. Type still limits content to enums, numbers and timestamps.
- VERIFY(event_shape) holds: `Event` (`src/agents/timeline.rs:47-67`) has `String` only in `at` (set from `jiff::Timestamp::now().to_string()` at `follow.rs:65`, `mod.rs:585`, `mod.rs:644`) and `judged`. `word` comes from `timeline::word` (`timeline.rs:88-97`, `_ => None`). `say` stores `chars` only (`follow.rs:315`); its text is trimmed and sent to Herdr, never recorded.
- Test: `tests/hire_events.rs:539-551` `say_writes_the_length_of_the_reply_and_never_its_text` (Said chars 11, raw log lacks "secret"). `:554-567` unknown word `<script>` writes no event. Both found by name this session.

STORE(kpi_point): sole caller `src/pipeline.rs:467`. VERIFY(kpi_point) holds: finite value (`history.rs:127`), parseable `evidenceAt` (`history.rs:128`), name non-empty after trim (`history.rs:126`), new pair under `kpis.lock` (`history.rs:81-103`, `:129`). Finding 1 closed.

STORE(ledger_title): stored tainted by `src/agents/ledger.rs:78` `title: Some(spec.title.clone())`, appended from `src/agents/follow.rs:568` and `examples/demo.rs`. Only reader is `src/digest.rs:183-186` (spec title, then ledger title, else "An agent run"), then `digest.rs:381` `today::prose`. Cleared at render. Tests: `tests/today.rs` `a_ledger_item_takes_its_spec_title_while_the_run_exists`, `a_ledger_item_names_its_role_never_its_run_id`.

RENDER(report_html):
- `src/digest.rs:344-349` lane label and empty text: static strings (`lane.label()`, `lane.empty()`, count is `len`). Clean.
- `src/digest.rs:368-383` `card`: `datetime` `today::esc(at)` (`:373`), label `today::esc(&cx.stamp(at))` (`:374`), title `today::prose(title)` (`:381`), source `today::chips(source)` (`:382`). Clean.
- `src/kpis.rs:112-115, 121, 177, 188-190`: name, value, unit, words, label all through `today::esc`. Clean.
- `src/kpis.rs:187` trend `id` is not escaped, but it is `kpi-{i}` or `plan-{i}` from an enumerate index (`kpis.rs:36, 55`), so it is not a data string. Clean.
- Escapers: `today.rs:16-29` `esc` escapes `& < > " '`. `today.rs:952-957` `url_of` admits only http/https without whitespace, `"`, `<`, `>`. `today.rs:1004-1018` `chip` escapes href, title and label. Clean.

RENDER(window_dom):
- `apps/desktop/ui/states.js:36-57` `el()`: prop allowlist `:9-13`, throws on other keys (`:40`), `on*` must be function (`:42`), `href` only via `safeUrl` (`:15-22`, `:46-48`) allowing http, https, mailto.
- `states.js:140` `t.innerHTML = window.ART?.[kind]` is the only markup write (static art, no data). `states.js:81` `setAttribute(k, v)` runs over `window.LUCIDE` geometry (static).
- `states.js:219` `frame.src = safeFrameSrc(src)`; `safeFrameSrc` (`:25-34`) admits `cosreport:` and loopback http only.
- Rg over `kit.js`, `app.js`, `states.js`: no `innerHTML`, `outerHTML` or `insertAdjacentHTML` outside `states.js:140`. `setAttribute` in `kit.js:516, 517, 547` takes literals. `app.js:291, 687, 688` set aria state from booleans. `app.js:162` reads `$("docs-frame").src`, no write.
- Gate: `window_never_writes_html_from_data` and `each_rule_catches_its_violation` passed this session (2 passed).

## Drift

Design vs code, each item separate.

- D1 (should): design `docs/plans/2026-11-25-rv2-kit-desktop-design.md:478` says a ledger row with no spec "change fall back to the run id". Code `src/digest.rs:186` falls back to "An agent run". Code is correct per finding 5. Fix: update the design line.
- D2 (nit): design `:754` names exceptions `states.js:97` (art), `:38` (Lucide), `:172` (frame.src). Actual: `states.js:140`, `:81`, `:219`. Gate matches by snippet, so the test passes. Other design line refs at `:128, :176, :209-212` also name `states.js:97`, `:38`, `:172`. Fix: update all.
- D3 (nit): design `:751` says the kpi id must be "non-empty". Code trims and refuses whitespace-only names (`history.rs:126`). Stricter than the design. Fix: state "non-blank" in the design.

Matches (no drift): STORE(event) inputs and enums (`design.md:683` vs `timeline.rs:47-67`); `word` exhaustive with `_ => None`; `say` stores `chars` only; `record_kpis` lock and dedup; `write_json` temp name is unique (`mod.rs:1107-1115`, design `:636`); audit lines carry run id, kind or KPI name and error (`events.rs:67-75`); deadline stop after `interrupt` (`follow.rs:223`).

## Findings

No blocking. No should except the two drift items above.

- N1 nit `src/agents/mod.rs:1107-1115` `write_json`: when `fs::write` or `fs::rename` fails, the `.<pid>.<n>.tmp` file is left in the run directory. Fix: remove `tmp` on the error path (`let _ = std::fs::remove_file(&tmp)` before returning the error). Not in the design; the design's "renamed to a unique name" claim still holds.
- F5 nit, carried: design refs (D2 above).
- F6 nit, carried: PR #57 body. Edit the body (`gh pr edit 57 --body ...`), do not add a commit. Replace `tests/agent_events.rs` with `tests/hire_events.rs`, add `tests/kit_page.rs`.
- D1 should, carried as drift: design line 478.

Open at this point: D1 (should), D2/D3, F5, F6, N1 (nits).
