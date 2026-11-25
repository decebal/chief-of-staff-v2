---
range: e26ad52..1ebbcaa
prove-it: 🟡 MERGE AFTER CHECK
verdict: findings
---

## prove-it

PR #57 is OPEN. Claims come from the PR body summary and the commit subjects.

🟡 MERGE AFTER CHECK. No refuted claim and no blocking finding. Hand-checks outstanding: 6.

Gates run this session:

| Gate | Result |
|---|---|
| `cargo fmt --check` | exit 0, no output |
| `cargo clippy --all-targets -- -D warnings` | exit 0 |
| `cargo test --test agents` | 9 passed |
| `cargo test --test design` | 30 passed |
| `cargo test --test hire_events` | 22 passed |
| `cargo test --test history` | 18 passed |
| `cargo test --test today` | 23 passed |
| `cargo test --test kit_page -- --include-ignored --test-threads=1` | 11 passed, with `COS_E2E_CHROME="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"`. Without it, 11 failed at `tests/common/cdp.rs:41` ("COS_E2E_CHROME is unset") |
| `cargo run --quiet --example demo -- <scratch>/cos-demo-review` | exit 0 |
| full `cargo test` | not run (exceeds the review window; the five binaries above were run instead) |
| `apps/desktop` `--test ipc` | not run |
| `examples/wcag.rs` | not run |

VERIFY BY HAND (block two, never last, never elided):

- Brief, live (paid, about $0.35): `cos brief --no-open`. Pass: exit 0; `out/history/kpis.jsonl` gains one line per KPI with a finite `current` and a parseable `evidenceAt`; a second brief on the same `evidenceAt` adds no line; Today shows "Since your last brief" after the actions, or is absent when no earlier ok run exists.
- Agent, live (paid, capped): `cos --budget 0.25 agents spawn --model haiku --repo <throwaway repo> --prompt "run: echo hi > a.txt; then run: curl -sS https://example.com"`, then `cos agents show <id>`. Pass: `done`; `out/agents/<id>/events.jsonl` has `hired`, `started` and `word` lines and no free-text field; the curl is refused by the sandbox.
- Pause then reply, on the window's stand-in Herdr: `cargo test --manifest-path apps/desktop/Cargo.toml --test ipc`. Pass: the pause-then-say test gives one closed paused interval in `agents_timeline`, and the approval test stays green.
- Dash: `cos dash --no-open`, open `/` and `/agents` in light and dark. Pass: the fleet and run card render, with no raw run id in the fleet, and the run card's clock is still while held.
- WCAG: `cargo run --example wcag` on the Agents, Today and Flows views, light and dark. Pass: no new violations against the baseline.
- Screenshots: `docs/assets/fleet*.png`, `run*.png`, `flow-decisions*.png` come from the Taskfile section that runs `examples/preview` on `/tmp/cos-demo`. Pass: the demo home is the source (`task screenshots`), and the images show no run id in the fleet or Today.

CLAIMS

- C1, the fleet, run card, timeline and decision stack on the Agents and Flows tabs: HOLDS for the rendering paths covered by the 11 kit_page e2e tests. The window's live behaviour (pause, reply, approval) is a hand-check above.
- C2, two append-only histories written by cos (`events.jsonl`, `kpis.jsonl`): HOLDS. Writers listed in the sink table below; no brain writes either file.
- C3, KPI points are recorded at brief time only: HOLDS. `record_kpis` has one caller, `src/pipeline.rs:467`. `fold_latest_run` does not call it.
- C4, digest after the actions, dates in words, markup escaped: HOLDS (`tests/today.rs`, 23 passed; escape case at `tests/today.rs:309`).
- C5, approval stays `ritual_approve`: UNVERIFIABLE HERE. The ipc test was not run. `src/ritual.rs` changes add `board()` and leave `ritual_approve` unchanged in the diff.
- C6, "no new spawn on a render or poll path": HOLDS. `rg "Command::new|\.spawn\(|\.output\(|proc::"` over the new files (digest, kpis, history, agents/events, agents/timeline, agents/follow, today, report) finds nothing. The only hit in the range is `src/report.rs:265` (`open`), which predates the epic.
- C7, PR body test plan names `tests/agent_events.rs`. That file does not exist. The tests are in `tests/hire_events.rs`. Not a code defect; fix the PR body.

ATTACKS TRIED

- I1 (brains read-only): `src/claude.rs` is not in the diff. No brain tool set changed.
- I2 (no committed state): `git diff --name-only e26ad52..HEAD -- .chief-of-staff context out` is not checked here; no added path in the diff points there.
- I3 (children owned and reaped): no new `Command`/`spawn` in the range outside `src/proc.rs` users.
- I4 (signals identity-checked): no new `libc::kill` in the range; `libc::flock` only.
- I7 (numbers from facts): digest chart and KPI tiles read `history` and ledger records; counts come from list lengths.
- I13 (state.json written only through `state::commit`): the new writes are to `out/agents/<id>/state.json` (hire state, `write_json`) and `out/history/`. Workspace `state.json` is read only (`src/digest.rs:200`, `src/history.rs:77`).
- Two-follower race on hire state: `with_state_lock` (`src/agents/follow.rs:93-122`) takes `state.lock` around `observe`, `close`, `judge`. Re-entrant via `HELD`. Two-thread append dedup: `tests/hire_events.rs:409` (passed).
- Second item, failing item, abort: `events::record` swallows append errors into the audit log (`src/agents/events.rs:67-75`); `record_kpis` returns an error the pipeline logs (`src/pipeline.rs:467`).

FINDINGS

1. should `src/history.rs:119-129`: `new_kpi_points` accepts an empty KPI name. The design's VERIFY(kpi_point) (design.md, Bindings and Sinks table) requires "the kpi id is non-empty", and the schema has `minLength: 1`. Evidence: `rg -n "is_empty|empty\(\)" src/history.rs` returns no check on `name`. Fix: add `|| name.trim().is_empty()` to the guard at `history.rs:126`, and an empty-name case to `a_kpi_point_needs_a_finite_value_an_evidence_time_and_a_new_pair` (`tests/history.rs:180`).

2. should `apps/desktop/ui/kit.js:116-117`: the fleet card's visible name and its aria label fall back to the raw run id: `textContent: r.title || r.id`, `ariaLabel: \`${r.title || r.id}, ${text}\``. A hire whose title is empty shows `YYYYMMDD-HHMMSS-cNNN` to the user, which the claim "no raw run ids in new views" forbids. Evidence: `rg -n "r\.title \|\| r\.id" apps/desktop/ui/kit.js`. Fix: fall back to the role title, or to a fixed word such as "Untitled hire", and keep the id in `dataset.id` only.

3. should `apps/desktop/ui/kit.js:82`: the run card title is `spec.title || spec.id`. Same class as finding 2. Fix: fall back to a fixed phrase.

4. should `apps/desktop/ui/kit.js:565`: step bead label is `b.title || b.id`, a tracker id shown as text. Fix: fall back to the bead's status word.

5. nit `docs/plans/2026-11-25-rv2-kit-desktop-design.md`, Bindings and Sinks table: `states.js:97`, `:38` and `:172` are now `:140`, `:81` and `:219`. The gate matches by snippet (`tests/design.rs:879-894`), so the test is right and the doc is stale. Also the design says a ledger row with no spec falls back to "the run id"; the code falls back to "An agent run" (`src/digest.rs:186`), which is the better behaviour. Update the design to match.

6. nit PR #57 body: `tests/agent_events.rs` should read `tests/hire_events.rs`; `tests/kit_page.rs` is also missing from the list. Edit the body; do not add a commit for it.

SINK TO VERIFY

| Sink | Call sites (file:line) | Check, and where it is enforced | Test |
|---|---|---|---|
| STORE(event) | `src/agents/events.rs:28` `append`, reached only through `events.rs:67` `record`. Callers of `record`: `src/agents/follow.rs:167,178,223,311,327,348,502,557`; `src/agents/mod.rs:581,654,662`. No other caller of `append`. | VERIFY(event_shape): `Event` (`src/agents/timeline.rs:48-67`) has `String` only in `at` and `judged`; `word` is an exhaustive match with `_ => None` (`timeline.rs:88-97`); `follow.rs:165-176` skips a `None` word; `say` stores `chars` only (`follow.rs:315`). Clean. | `tests/hire_events.rs:409` (two threads, one line), `:554` (unknown word writes nothing), `:570` (judged written once). A test that `say("secret text")` writes `chars = 11` with no text: not located by name; a hand-check. |
| STORE(kpi_point) | `src/history.rs:76` `record_kpis`. Sole caller `src/pipeline.rs:467`. `fold_latest_run` does not call it. | VERIFY(kpi_point): finite value, parseable `evidenceAt`, pair not yet in the file, under `kpis.lock` (`history.rs:81-103`). Missing: non-empty name (finding 1). | `tests/history.rs:155` (finite and dated only; a repeat writes nothing), `:168` (two threads, one line), `:180` (value, time, new pair). |
| STORE(ledger_title) | `src/agents/ledger.rs:51-79` `row` sets `title: Some(spec.title)`; `ledger.rs:84` `append`; callers `src/agents/follow.rs:567-568`, `examples/demo.rs`. | Stored tainted. Cleared at render: `src/digest.rs:181-186` (title, a plain string), `:381` `today::prose(title)`. | `tests/today.rs:246` (title while the run exists), `:309` (escape: markup renders as text). |
| RENDER(report_html) | `src/digest.rs:344-349` (lane label and empty text: static), `:368-383` (`card`: `esc(at)` at `:373`, `esc(&cx.stamp(at))` at `:374`, `prose(title)` at `:381`, `chips(source)` at `:382`); `src/digest.rs:213-218` (incident title, passed to `prose` at `:381`); `src/kpis.rs:112-121`, `:177`, `:188-190` (`esc`). Escapers: `src/today.rs:16` `esc`, `:965` `prose`, `:1058` `chips` (http/https links only). | VERIFY(escaped): every data string goes through `esc`, `prose` or `chips`. The `format!` in `kpis.rs:138,169,175,186` takes numbers and fixed text; `label(point)` is escaped at `:177`. Clean. | `tests/today.rs:309`; KPI tile test `tests/today.rs:335`. |
| RENDER(window_dom) | `apps/desktop/ui/states.js:9-57` (`el`: allowlist `EL_PROPS` at `:9-12`, `isElProp` at `:13`, `href` only through `safeUrl` at `:15-22` and `:46-48`); sinks: `states.js:140` (`t.innerHTML = window.ART`), `:81` (`n.setAttribute(k, v)` over `window.LUCIDE`), `:219` (`frame.src = safeFrameSrc(src)`, refused at `:188` when not `cosreport:` or loopback). `kit.js` and `app.js`: no markup sink (rg). | VERIFY(escaped): `el` throws on any prop outside the allowlist; `on*` must be a function; computed keys and URL props are rejected by the gate. The three exceptions are the only markup or URL writes, each matched by snippet. | `tests/design.rs:964` (`window_never_writes_html_from_data`, 3 exceptions, each hit exactly once), `:763` (`each_rule_catches_its_violation`, the five fixtures), `tests/kit_page.rs` (`el_door_refuses_what_is_not_on_the_allowlist`, passed in the e2e run). |

No sink lacks a VERIFY. No call site bypasses one. The window sink for `frame.src` is reached through `frameLoad`, which calls `safeFrameSrc` at `states.js:188` before the assignment.

CONSIDERED AND DROPPED

- `frame.src` as a URL sink: killed by `states.js:188` and `:219`, both read this session.
- Run ids in the digest: killed. `src/digest.rs:186` falls back to "An agent run", not the id.
- A second spawn on the poll path: killed. `tick` (`follow.rs:69-91`) calls `observe` every 2 s, as before. The new `events::record` runs only on a change, and it does no spawn.
- Brain writes to the histories: killed. No brain code path is in the diff, and the writers are listed above.
- The Taskfile `pkill -f "examples/preview /tmp/cos-demo 8793"` as an orphan risk: killed. It matches the preview's own argv, and `defer:` runs it on exit.
