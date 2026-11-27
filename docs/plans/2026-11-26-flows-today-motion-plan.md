# Flows and Today, in motion: plan

Design: [2026-11-26-flows-today-motion-design.md](2026-11-26-flows-today-motion-design.md)
(approved, sha256 `e69ea6e8a096c5376ea10c4d37febdf432c1bc0dec0594f5b1c79d81cd13751f`).
Epic `t-e89af1`, branch `feat/desktop-flows-today-motion`, PR #61.

Gates and layers follow the previous flow's decision, the same repo and the
same kind of change (`full-trophy act confidence=0.90`).

### Quality gates

#### Epic-level (once, when every story is done)

- `task test`
- `task test-e2e`
- WCAG evaluator (`examples/wcag.rs`, axe-core 4.13.0): 0 violation nodes,
  light and dark
- `task screenshots`, looked at

#### Story-level

- **Backend stories:** their named test file, run alone.
- **UI stories:** their e2e test, and the view seen in
  `apps/desktop/examples/preview.rs` on the demo home, light and dark.
- **Reduced motion:** with reduced motion emulated, the view has no running
  animation (`document.getAnimations()` is empty once settled).

### Testing trophy

#### Unit

- [x] `agents::snapshot` leaves every file and mtime unchanged
  (`tests/read_only_render.rs`)
- [x] `agents::phase` gives the fleet phase for each status and word, and
  `done_today` in the profile's timezone (`tests/hire_events.rs`)
- [ ] `flows::scan` with a home: hires are matched by `bead:{repo}/{id}`;
  lanes and ring counts (`tests/flows.rs`)
- [ ] Today (`tests/today.rs`):
  - inbox columns;
  - timeline slots, outcomes and blips against a fixed now;
  - the lead-phrase split;
  - no run id in visible text;
  - escaped hire titles;
  - the digest unchanged in meaning
- [x] the `motionOk`, `glide` and `swap` copies are identical
  (`tests/design.rs`)

#### Contract

- [x] `agents_snapshot` returns `HireView`s; `agents_list` rows carry
  `phase` and `done_today` (`apps/desktop/tests/ipc.rs`)

#### Integration

- [x] rendering Today, Flows and the snapshot writes nothing under the demo
  home (`tests/read_only_render.rs`)

#### Frontend and E2E

- [ ] the rewritten reduced-motion gate and its fixtures;
  `report_kit_block_never_writes_html_from_data`; the new tokens and the
  gradient contrast (`tests/design.rs`)
- [x] Lanes, Rail, Ring and Beads (`tests/flows_page.rs`)
- [ ] the brief loader, live inbox, KPI roll, and the window's rail and
  crossfade (`tests/kit_page.rs`)
- [ ] reduced motion on every page (`tests/kit_page.rs`,
  `tests/flows_page.rs`)

**Block merge:** all of the above.

### Stories

#### US-001: Motion grammar and the gate that proves it [UI]

- [x] Tests first, failing: the rewritten
  `every_animation_respects_reduced_motion` over `style.css`, `kit.css` and
  the `<style>` of `report.html`, `flows.html` and `docs.html`. Its fixtures
  in `each_rule_catches_its_violation`:
  - an animation outside the block;
  - `transition:` with no space;
  - `.animate(` outside `motionOk()`;
  - an unused `@keyframes`.
- [x] Tokens: motion, `--glow-1`/`--glow-2`, and
  `--gradient-from`/`--gradient-to`. The gradient stops are added to the
  contrast test as text on `--hero`, and their values are measured.
- [x] `kit.css` keyframes: `kit-shimmer`, `kit-breathe` (valid `color-mix`),
  `kit-settle`, `kit-arrive`, `kit-draw`, `kit-flow`, `kit-flash`,
  `kit-tick`. All are under `no-preference`.
- [x] `motionOk`, `glide` and `swap` in `kit.js`, `report.html` and
  `flows.html`, with a test that their bodies are identical.
- [x] `report.html` rise and `flows.html` breathe move under
  `no-preference`.
- [x] `cargo test --test design` green.

#### US-002: Hire reads without side effects [Backend]

- [x] Tests first, failing: `tests/read_only_render.rs`
  `rendering_today_flows_and_the_hire_snapshot_writes_nothing`.
  - It walks the demo home's paths, lengths and mtimes before and after
    `report::data`, `today::hero`/`body`, `flows::scan` with the home, and
    `agents::snapshot`.
  - It first confirms that `cn` reads write nothing under the home. If they
    do, it scopes the walk to exclude the Chronis store, and says so in its
    doc comment.
  - It records which `report::data` callees were checked write-free.
- [x] `agents::snapshot` and `agents::phase`, and `HireView` with
  `deadline_secs`.
- [x] Tauri `agents_snapshot(business)`, registered. `agents_list` rows gain
  `phase` and `done_today`, and `kit.js` `fleetRow` reads them.
- [x] `report::data` uses `snapshot`. `hires` is added, `herdr_waiting` is
  derived from it, and the digest input is unchanged.
- [x] `cargo test --test read_only_render --test hire_events --test today`
  and `cargo test -p cos-desktop --test ipc` green.

#### US-003: Flows scan knows its hires [Backend]

- [x] Test first, failing (`tests/flows.rs`): with a home, hires attach by
  `bead:{flow.repo}/{bead.id}`.
- [x] `flows::scan`/`try_scan` take `home: Option<&Path>`. Every caller is
  updated: `ritual.rs:574`, `main.rs:760`, `flows.rs:870` and :885,
  `apps/desktop/src/lib.rs:862` and :907, and `tests/flows.rs`.
- [x] `cargo test --test flows --test ritual` green.

#### US-004: Rail [UI]

- [x] Tests first, failing:
  - `tests/flows_page.rs`: the rail shows Think → Reflect with
    `aria-current="step"`, and drift flashes the current step;
  - `tests/kit_page.rs`: `steps()` uses the same markup.
- [x] `flows.html` includes `design::KIT_SLOT`, and the rail sits in the flow
  header. `kit.js` `steps()` switches to the rail markup.
- [x] Seen in preview, light and dark, with reduced motion settled.

#### US-005: Bead ring on board cards [UI]

- [x] Test first, failing (`tests/flows_page.rs`): the card's accessible
  name carries the counts, and the ring is `aria-hidden`.
- [x] The ring draws in once; the done count is centred as text.
- [x] Seen in preview, light and dark.

#### US-006: Scene [UI]

- [x] Test first, failing (`tests/flows_page.rs`): the scene heads the Board
  when a flow waits on the user, and is the empty state's art.
- [x] It draws with `kit-draw` and `kit-settle`. Labels are hidden below
  32rem, and the SVG is `aria-hidden`.
- [x] Seen in preview.

#### US-007: Beads view with glide [UI]

- [x] Tests first, failing (`tests/flows_page.rs`):
  - columns follow status, `ready` and `open_blockers`;
  - a rescan moves a bead's column;
  - under reduce, no animations.
- [x] A Beads tab in the Flow view. `glide` runs on rescan. No drag.
- [x] Seen in preview.

#### US-008: Lanes view [UI]

- [x] Tests first, failing (`tests/flows_page.rs`):
  - lane order is hires, then owners, then "Not yet claimed";
  - beads sit in their lanes;
  - arrow, Home and End keys work, and Enter opens the drawer;
  - the gate diamond posts `cos-flows-gate`;
  - narrow stacks the lanes.
- [x] A Lanes tab. An SVG connector layer with a `viewBox` in column and row
  units. Box states as designed; the gate breathes, with `kit-flow` along its
  connector.
- [x] Seen in preview, light and dark, and on one flow with 30 or more beads.

#### US-009: Living hero [UI]

- [x] Tests first, failing (`tests/today.rs` and `tests/kit_page.rs`):
  - the `h1` `aria-label` holds the full text, and the word spans are
    `aria-hidden`;
  - the lead-phrase split, with the fixture "Release 3.4 ships in three days;
    …";
  - pills for working, needs-you and paused hires, with no run id;
  - `cos-brief-state` running, then done, shows the loader, then "Brief
    ready".
- [x] The floor, the reveal, the gradient sweep, the pills with the
  deadline-share bar, and the loader.
- [x] Seen in preview, light and dark, with reduced motion settled.

#### US-010: Hires inbox with live updates [UI]

- [x] Tests first, failing:
  - `tests/today.rs`: the columns;
  - `tests/kit_page.rs`: a `cos-hires` message moves a card between
    columns; a message whose `event.source` is not `window.parent` is
    ignored; the `cos-host` state is unchanged;
  - `tests/design.rs`: `report_kit_block_never_writes_html_from_data`.
- [x] `today::hires` replaces `herdr_waiting`. The window posts `cos-hires`
  on `agents-changed` from `agents_snapshot`. "As last read" shows on the
  heading, the pills and the Running column when no `agents-changed` arrives
  within 10 s of load, tested with a fixed clock.
- [x] Seen in preview.

#### US-011: Day timeline [UI]

Ported as it is from rv2-kit `agent_schedule_board`, read-only.

- [x] Tests first, failing: `tests/today.rs` (with today's `Ctx` clock fixed,
  markers from today's slots and today's hires with outcomes in words, the
  centred now line, two-hourly ticks in the profile's timezone, the status
  line and the trace; no switch, Run now, Skip or Restore; an empty day) and
  `tests/kit_page.rs` (the minute timer moves the markers under the fixed now
  line; a marker or a row selects its detail; arrow keys move the selection).
- [x] `today::schedule` renders the board above the week ribbon, which stays
  as it was; `report.html`'s kit block moves it once a minute with DOM
  methods; `kit.css` `.kit-asb*` ports the component.
- [x] Checked side by side with the rv2-kit gallery's AgentScheduleBoard on a
  busy demo day, light and dark.

#### US-012: KPI ticker [UI]

- [x] Test first, failing (`tests/kit_page.rs`): the value text equals the
  record. The roll happens once per document load, from the second-newest
  point by `evidenceAt`.
- [x] The digit roll is `aria-hidden`, and the value is a polite live
  region.
- [x] Seen in preview.

#### US-013: Crossfades and shimmer [UI]

- [x] Tests first, failing (`tests/kit_page.rs`):
  - a pane change and the run page's loading → ready use `swap`, and the
    outgoing layer is `inert`;
  - reading states show shimmer rows;
  - under reduce, no animations.
- [x] `swap` is wired in the window and in `flows.html`'s view changes.
  `stateView` gains its shimmer rows.

#### US-016: Steps from StepProgressList [UI]

Replaces US-004's rail with rv2-kit `step_progress_list`, ported as it is.

- [x] Tests first, failing: `tests/kit_page.rs` (`steps()` marks, pills, the
  24 px / 40 px / 17 px rhythm, unfolding beads folded to five around the one
  in progress, one polite live region, no times) and `tests/flows_page.rs`
  (the Flow view's Steps column beside Graph, Beads and Lanes, above them
  below 64rem, drift as an errored step).
- [x] `kit.js` `steps()` and `kit.css` `.kit-steps*` port the component;
  `flows.html` renders the same markup in `#flowsteps`. The rail markup and
  CSS are gone.
- [x] Checked side by side with the rv2-kit gallery's StepProgressList, light
  and dark.

#### US-014: Demo, screenshots, evidence [Integration]

- [x] `examples/demo.rs`: a flow at the gate with owned, hired and blocked
  beads; hires sourced from its beads; a busy calendar day.
- [x] `task screenshots` gains Lanes, Beads, Board with rings, the Flow view
  with its Steps column, and the Today hero, inbox, schedule board and KPI
  tiles, light and dark.
- [x] WCAG at 0 violation nodes; `task test` and `task test-e2e` green.

### Non-goals

- Drag in the Beads view.
- Changing `agents_list`'s reconciling.
- Step timestamps.
- Gating the report's existing `esc()` templates.
