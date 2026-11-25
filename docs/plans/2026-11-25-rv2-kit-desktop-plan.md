# rv2-kit in the desktop app: plan

Design: [2026-11-25-rv2-kit-desktop-design.md](2026-11-25-rv2-kit-desktop-design.md)
(approved, sha256 `b7160f27ca4d936321d09ac86d44fcfeac1ea10c5f50c66aa9136130c64a8cd4`).
Epic: `t-cd6991`. Branch: `feat/desktop-rv2-kit`.

Gate and layer choices were made by TypeSafe Jev: `full-trophy act
confidence=0.90 · runner-up no-e2e p=0.05`.

### Quality gates

#### Epic-level (once, when every story is done)

- `task test`: fmt, clippy `-D warnings`, every test, the design gates.
- `cargo run --example wcag`: no new violations in light or dark on the
  Agents, Today and Flows views.
- `task test-e2e` with `COS_E2E_CHROME` set: the `el()` DOM test and
  `tests/flows_page.rs`.

#### Story-level

- **Backend stories:** their named `tests/*.rs` file, run alone
  (`cargo test --test <name>`).
- **UI stories:** the view opened in `apps/desktop/examples/preview.rs`
  against the stand-in Herdr on the demo home, and the story's own test.

### Testing trophy

#### Unit

- [x] [Logic] the timeline fold produces working, waiting and paused spans;
  hand-offs; marks; totals (`tests/hire_events.rs`)
- [x] [Logic] the paused interval:
  - closes on Said, MarkedDone or Ended;
  - closes on a `working` word in a later turn;
  - does not close on a `working` word in the same turn;
  - `DeadlineStop` opens no pause

  (`tests/hire_events.rs`)
- [x] [Logic] the fold handles `Hired` → `Ended` with no `Started`, and a run
  with no events (`tests/hire_events.rs`)
- [x] [Logic] `summary(&Totals, now)` grows `active_ms` as `now` advances
  with the file unchanged (`tests/hire_events.rs`)
- [x] [Logic] `timeline::word` maps the five Herdr words, and an unknown word
  is `None` (`tests/hire_events.rs`)
- [x] [Logic] identity hashing is stable and in 1..=6 (`tests/hire_events.rs`)
- [x] [Logic] the digest boundary:
  - the newest ok receipt before today in the profile's timezone;
  - `runs[]` `complete` as the fallback;
  - no boundary, no digest

  (`tests/today.rs`)
- [x] [Logic] the digest lanes from ledger, incidents and merged PRs: the
  `merged_at` comparison, and the legacy day rule (`tests/today.rs`)
- [x] [Logic] the KPI point checks: finite value, parseable `evidenceAt`,
  dedup by (name, `evidenceAt`) (`tests/history.rs`)

#### Contract

- [x] [Shape] an `Event` JSON line matches the `STORE(event)` binding, and the
  one with `Said` carries `chars`, not text (`tests/hire_events.rs`)
- [x] [Shape] a ledger `Row` without `title` still deserialises, and a new
  one carries it (`tests/agents.rs`)
- [x] [Shape] `Merged` without `merged_at` still deserialises (`tests/history.rs`)
- [x] [Handler] `agents_list` rows carry `identity`, `active_ms` and
  `paused_now`; `agents_timeline` returns `Timeline`
  (`apps/desktop/tests/ipc.rs`)

#### Integration

- [x] [Concurrency] two threads observing one change append one `Word`
  (`tests/hire_events.rs`)
- [x] [Concurrency] two followers do not lose `ended` or `verdict` under
  `state.lock` (`tests/agents.rs`)
- [x] [Concurrency] two threads recording one KPI point append one line
  (`tests/history.rs`)
- [x] [Flow] pause then say on the stand-in Herdr gives one closed paused
  interval, with `paused_now` true in between (`apps/desktop/tests/ipc.rs`)
- [x] [Flow] the existing approval test passes through the decision stack
  (`apps/desktop/tests/ipc.rs` :2938)
- [x] [Render] the digest sits after the actions, or after `herdr_waiting`
  with no brief; dates read in words; markup in a spec title renders escaped
  (`tests/today.rs`)

#### Frontend and E2E

- [x] [Gate] `window_never_writes_html_from_data` and its fixtures in
  `each_rule_catches_its_violation` (`tests/design.rs`)
- [x] [Gate] `every_page_that_uses_kit_classes_includes_the_kit_sheet`
  (`tests/design.rs`)
- [x] [Gate] the new tokens pass light/dark and AA; `kit.css` passes the
  tokens gate; `WINDOW_JS` includes `kit.js` (`tests/design.rs`)
- [x] [E2E] the `el()` DOM test under the e2e harness:
  - a `javascript:` href is not set;
  - `src`, `formAction` and `innerHTML` throw;
  - markup renders as text;
  - `safeFrameSrc` refuses non-`cosreport`, non-loopback values
- [x] [E2E] fleet, run card, timeline, steps and decision stack render on the
  preview with the demo home (`tests/flows_page.rs` harness pattern, new
  `tests/kit_page.rs`)

**Block merge:** every test above. **Block release:** the wcag run. **Follow-up
allowed:** none.

### Stories

#### US-001: Tokens, kit sheet and its slot [UI]

As the window, I want job-named identity, think and space tokens and one
`kit.css` so that every ported component styles through `tokens.css`.

- [x] Test first, failing (`tests/design.rs`): the new colour tokens have a
  light and a dark value and meet AA; a `kit-` class in a served page without
  `design::KIT_SLOT` fails
  `every_page_that_uses_kit_classes_includes_the_kit_sheet`.
- [x] `tokens.css` gains:
  - `--identity-1..6`, light-dark;
  - `--identity-1..6-soft`, as `color-mix`;
  - `--think`;
  - `--space-1..6`.
- [x] `apps/desktop/ui/kit.css` exists and is linked after `style.css` in
  `index.html`; its animations sit only under `no-preference`.
- [x] `design::KIT_SLOT` inlines `kit.css` into served pages.
- [x] All tests green: `cargo test --test design`.

#### US-002: `el()` allowlist and the window HTML gate [UI]

As the window, I want every builder's props checked at one door so that data
never reaches the DOM as markup, script or a dangerous URL.

- [x] Test first, failing (`tests/design.rs`): `window_never_writes_html_from_data`
  with the fixtures `x.innerHTML = d`, `el("a", {[k]: v})`,
  `setAttribute("onclick", d)`, `setAttribute(k, v)` and `img.src = d`, in
  `each_rule_catches_its_violation`.
- [x] `states.js` `el()` accepts only the design's allowlist, sets `href`
  through `safeUrl()` (http, https, mailto), and throws on any other key or a
  non-function `on*`.
- [x] `frameLoad` sets `frame.src` through `safeFrameSrc()` (`cosreport:` and
  loopback only).
- [x] The gate names exactly three exceptions: `states.js:97` art,
  `states.js:38` Lucide geometry, and the `frame.src` site.
- [x] `WINDOW_JS` (tests/design.rs:1085) includes `kit.js`.
- [x] The e2e `el()` DOM test exists under the `task test-e2e` harness and
  passes with `COS_E2E_CHROME`.
- [x] Every existing `el()` call in the window still works: `task test` green,
  and each tab opens in `preview.rs`.
- [x] All tests green: `cargo test --test design`.

#### US-003: State writes cannot lose an update [Backend]

As cos, I want two followers' state writes serialised so that neither
overwrites `ended` or `verdict` with a stale snapshot.

- [x] Test first, failing (`tests/agents.rs`): two threads run
  `observe`/`close` on one run, and the final state keeps `ended` and
  `verdict`.
- [x] `write_json` writes `<file>.<pid>.<n>.tmp`, then renames.
- [x] `observe`, `close` and `judge` hold `out/agents/<id>/state.lock` (flock)
  across read-modify-write.
- [x] All tests green: `cargo test --test agents`.

#### US-004: The timeline domain [Backend]

As cos, I want a pure fold from hire events to spans and totals so that every
run view reads time from records.

- [x] Tests first, failing (`tests/hire_events.rs`): every Unit row above
  tagged `tests/hire_events.rs`, and the `Event` shape contract.
- [x] `src/agents/timeline.rs` has `Kind`, `By`, `Word`, `Event`,
  `word()`, `fold()`, `totals()` and `summary()`, and no I/O.
- [x] `agents::identity(role)` returns 1..=6.
- [x] All tests green: `cargo test --test hire_events`.

#### US-005: Hire events written and read [Backend]

As cos, I want each run's transitions appended once, under a lock, so that the
timeline has history and two followers never double-log.

- [x] Tests first, failing (`tests/hire_events.rs`):
  - two threads append one `Word`;
  - an early-return `judge` writes no second `Judged`;
  - `say` writes `chars` only;
  - an unknown word writes nothing;
  - a failed append writes an audit line and the action succeeds.
- [x] `src/agents/events.rs`: `append` (flock on `events.lock`, `O_APPEND`),
  `read`, and `tail_summary` (cache keyed by path, length and mtime, holding
  `Totals`).
- [x] Writers wired at the design's sites:
  - `mod.rs`: hire (:568), `launch` (:622);
  - `follow.rs`: `observe` (Word, FirstMessage, DeadlineStop), `say`,
    `pause` (turn from `live()`), `mark_done`, `close`, `judge`.
- [x] All tests green: `cargo test --test hire_events` and
  `cargo test --test agents`.

#### US-006: Run data reaches the window [Backend]

As the Agents tab, I want each row's identity and active time, and a run's
full timeline, so that the fleet and run page draw from records.

- [x] Tests first, failing (`apps/desktop/tests/ipc.rs`):
  - `agents_list` rows carry `identity`, `active_ms` and `paused_now`;
  - pause then say on the stand-in Herdr yields one closed paused interval
    from `agents_timeline`.
- [x] `agents_timeline(business, id)` is added and registered in
  `generate_handler!`.
- [x] All tests green: `cargo test -p cos-desktop --test ipc`.

#### US-007: The fleet [UI]

As the user, I want the Agents tab to open on a fleet of hires with identity,
state, need and live active time so that I see who needs me at a glance.

- [x] Test first, failing (`tests/kit_page.rs`, e2e): the fleet renders one
  card per hire; one role's hires share an identity class; arrow, Home and
  End move focus; Enter opens the run.
- [x] `kit.js` `fleet()`, wired in `app.js`:
  - the Fleet/Table segmented toggle keeps the table;
  - empty and loading states use `stateView`;
  - Reply, Open in Herdr and Open run call the existing commands.
- [x] A live, unpaused clock advances once a second client-side; a held clock
  stays still.
- [x] Verified in `preview.rs` on the demo home, light and dark.
- [x] All tests green: `cargo test --test design` and `task test-e2e`.

#### US-008: Run card and timeline [UI]

As the user, I want a run page that opens on its phase, clock and actions and
a timeline of what happened so that I can see the run's sequence.

- [x] Test first, failing (`tests/kit_page.rs`, e2e):
  - the phase follows the design's mapping;
  - footer actions per phase;
  - spans and hand-offs drawn;
  - arrow keys open span details with kind, start, length, word and turn;
  - a legacy run shows the "starts at this version" note;
  - a prep-failed run reads "ended before it started".
- [x] `kit.js` `runCard()` and `timeline()` sit above the existing verdict,
  contract and diff sections.
- [x] Verified in `preview.rs`, light and dark.
- [x] All tests green: `cargo test --test design` and `task test-e2e`.

#### US-009: Ledger titles and merge times [Backend]

As the digest, I want a title on every ledger row and a merge time on every
archived PR so that items read as names and count against the right boundary.

- [x] Tests first, failing:
  - `tests/agents.rs`: a row without `title` deserialises, and a new one
    carries the spec title;
  - `tests/history.rs`: `Merged` without `merged_at` deserialises, and a new
    one carries `closedAt`.
- [x] `Row.title` is set by `ledger::row`.
- [x] `Merged.merged_at` is filled from `Pr.closed_at` where `history::record`
  folds merged PRs.
- [x] All tests green: `cargo test --test agents` and
  `cargo test --test history`.

#### US-010: Since your last brief [Backend + UI]

As the user, I want Today to show the decisions, errors and finished work that
arrived since my previous brief so that I see what changed without reading
everything.

- [x] Tests first, failing (`tests/today.rs`): every Unit and Render row
  above tagged `tests/today.rs`.
- [x] `src/digest.rs` builds `digest` into `report::data` and renders the
  three lanes, the counts and the cumulative SVG with `esc`/`prose`/`chips`.
- [x] `today.rs` calls `digest::section` after the actions, or after
  `herdr_waiting` with no brief.
- [x] Verified on the demo home report, light and dark, served by `preview.rs`
  at `/cosreport/`: the three lanes, the chart and the cards.
- [x] All tests green: `cargo test --test today`.

#### US-011: KPI points [Backend]

As the KPI tiles, I want a recorded value per KPI evidence point so that a
trend is drawn from records.

- [x] Tests first, failing (`tests/history.rs`):
  - checks refuse `NaN`, a missing `evidenceAt` and a duplicate;
  - two threads append one line;
  - `kpi_points` reads them back.
- [x] `history::record_kpis(home, ws)` (flock `kpis.lock`) is called from
  pipeline.rs after `history::record` only. `fold_latest_run` is unchanged.
- [x] All tests green: `cargo test --test history`.

#### US-012: KPI tiles and trend [UI]

As the user, I want each KPI as a tile with direction, target and sparkline,
and a trend card on demand, so that I see which way each KPI moves.

- [x] Test first, failing (`tests/today.rs`): with two points, a tile shows
  direction and a sparkline; with one, no sparkline; a KPI name with markup
  renders escaped.
- [x] `src/kpis.rs` renders the tiles inside the week section, replacing
  `report.html` `kpiLine` for each goal's active KPIs. `kpiLine` stays for the
  "Later proposals" rows, which show no tiles.
- [x] The trend card's readout in `report.html` goes through `esc`, and the
  arrow keys step it.
- [x] Verified on the demo home report, light and dark.
- [x] All tests green: `cargo test --test today` and `cargo test --test design`.

#### US-013: Flow steps [UI]

As the user, I want the selected flow's ritual steps as a progress list with
its beads under Build so that I see where the flow stands.

- [x] Test first, failing (`tests/kit_page.rs`, e2e):
  - states before, at and after the current step;
  - `awaiting_approval` at the gate;
  - drift as `errored`;
  - the fold at more than 6 beads.
- [x] `kit.js` `steps()` is wired in the Flows host. No step time is shown.
- [x] Verified in `preview.rs`.
- [x] All tests green: `task test-e2e`.

#### US-014: Decision stack for approvals [UI]

As the user, I want designs waiting on me stacked with Approve, Approve without
review and Open design so that I clear them one by one.

- [x] Test first, failing:
  - `apps/desktop/tests/ipc.rs` :2938 stays green;
  - `tests/kit_page.rs`: peeks rotate by click and arrow keys; focus lands on
    the next Approve after a decision; the decided card's `data-status` shows
    the outcome.
- [x] `kit.js` `decisions()` replaces the `gates` cards. Approve calls
  `ritual_approve` with the same arguments as today.
- [x] All tests green: `cargo test -p cos-desktop --test ipc` and
  `task test-e2e`.

#### US-015: Demo home and screenshots [Integration]

As a reader of the docs, I want screenshots of every new view from the
synthetic home so that the docs show the feature without real data.

- [x] `examples/demo.rs` writes fixture events (one paused-then-resumed run,
  one legacy run, one prep failure), ledger titles, merged PRs with
  `merged_at`, two KPI points per KPI, and a waiting gate.
- [x] `task screenshots` captures the fleet, run card and timeline, digest,
  KPI tiles, steps and decision stack, light and dark, into `docs/assets/`.
  The window views come from `apps/desktop/examples/preview.rs` on the demo
  home, reached by hash (`#tab=agents&pane=runs&run=<id>`, `#tab=flows`).
- [x] `cargo run --example wcag` reports no new violations: 24 states, light
  and dark, 0 violation nodes, after fixing the timeline lanes'
  `aria-prohibited-attr` and the ribbon's past-block contrast.
- [x] All tests green: `task test` (39 test binaries, 0 failed) and
  `task test-e2e` (flows_page 9, kit_page 11).

### Functional requirements

- FR-1: Every hire transition in the design's writer table is appended once to
  `out/agents/<id>/events.jsonl`, with no free text.
- FR-2: Active time excludes paused intervals as the design defines them, and
  advances live while unpaused.
- FR-3: Today's digest counts only items after the previous ok brief.
- FR-4: KPI points are recorded only at brief time, once per (name,
  `evidenceAt`).
- FR-5: No data string reaches the window's DOM as markup, script or a non-safe
  URL.
- FR-6: Approval behaviour is unchanged.

### Non-goals

- Sub-agent trees, act/revert on Today, step timestamps for flows, and storing
  any Herdr message or reply text.

### Open questions

- None. The carried review items are folded into the stories above.
