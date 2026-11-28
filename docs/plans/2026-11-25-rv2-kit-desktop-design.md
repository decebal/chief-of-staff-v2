# rv2-kit in the desktop app: design

Date: 2026-11-25. Brief: [2026-11-25-rv2-kit-desktop-brief.md](2026-11-25-rv2-kit-desktop-brief.md).
Epic: `t-cd6991`. Branch: `feat/desktop-rv2-kit`.

## Decision

Port eight rv2-kit components into cos as native DOM, written against
`apps/desktop/ui/tokens.css`:

| Component | Where it goes |
|---|---|
| worker fleet | Agents roster |
| run status card | run page |
| run timeline rows | run page |
| since-last-brief digest | Today |
| metric tile | Today |
| metric trend | Today |
| step progress list | Flows |
| pending decision stack | Flows |

cos keeps two new append-only histories so the components show facts rather
than gaps:

- `out/agents/<id>/events.jsonl`, written by cos for each run;
- `out/history/kpis.jsonl`, written after each brief.

Both hold enums, numbers and timestamps only, and no free text. No brain writes
either. No component adds a process spawn on a render or poll path. Approval
stays `ritual_approve`.

## Context

**Agents tab**

- `apps/desktop/ui/app.js`: `refreshAgents` (:349), `runRow` (:665),
  `renderRuns` (:701), `renderRun` (:720), `renderScores` (:1045). The file is
  1,799 lines.
- Data comes from the Tauri commands in `apps/desktop/src/lib.rs`:
  - `agents_list` (:1070, returning `Row{spec, state, needs}`, :1061);
  - `agents_say` (:1087), `agents_pause` (:1101), `agents_done` (:1110);
  - `agents_scorecard` (:1170);
  - all registered in `generate_handler!` (:2043).
- **Refresh:** the watcher (`watch`, lib.rs:1781) emits `agents-changed` when
  `dash::stamp(out/agents)` changes (lib.rs:1842). `stamp` (`src/dash.rs:194`)
  reads the mtime and count of the entries directly under `out/agents/`: the
  per-hire directories and `ledger.jsonl`. A hire's directory mtime moves when
  an entry inside it is created or renamed (the `state.json` rename), not
  when a file inside it is appended to.

**Hire state**

- `src/agents/mod.rs` `State` (:225–266) is written by `write_state` (:1058).
  It writes `state.json.tmp`, then renames it (:1063–1066). The temp name is
  fixed, so two writers (the window's follower and `cos agents watch`) can
  interleave on it.
- `src/agents/follow.rs`:
  - `tick` (:69) calls `observe` (:91) for every live run every 2 s.
  - `observe` writes state only when the Herdr `Seen` changes (:155), comparing
    against the previous `Seen` (`state.herdr`, `Seen.word` a free string,
    mod.rs:186).
  - `say` (:240) and `pause` (:252) send `hc send` / `hc interrupt` and leave
    no record. `mark_done` (:259) ends and judges.
  - At the deadline, `observe` sends `hc interrupt` (:162) and records
    `herdr.interrupted`, then `finish` kills after the grace (:304).
  - `close` (:372) sets `ended` and judges. `judge` (:396) holds `judge.lock`,
    returns the existing verdict early unless asked again (:411–413), writes
    the verdict and appends `out/agents/ledger.jsonl` (:429).
  - `save_screen` (:343) writes up to 200 lines of the terminal to
    `screen.txt` when a run ends (:138, :152, :303). That includes the replies
    the user sent with `hc send`.
- `spec.json` is written when a hire is recorded (mod.rs:568), before
  `prepare` and `launch` (:581), which can fail. `launch` sets
  `status = Running` and `started` (:622).
- `cos agents clean` deletes `out/agents/<id>/` (mod.rs:1054). The ledger
  (`out/agents/ledger.jsonl`, ledger.rs:44) survives it.
- **Ledger rows** (`Row`, `src/agents/ledger.rs:18–42`) carry `id`, `judged`,
  `kind`, `repo`, `status`, `outcome`, `role`, and no title.

**Today**

- `src/report.rs` `data` (:36) assembles the report JSON. It is served to the
  window's report iframe and by `cos dash` at `/` (`src/dash.rs:140`). The MCP
  server serves brief and saved-report objects, not `report::data`
  (`src/mcp.rs:105–140`).
- `src/today.rs` `body` (:243) renders the sections server-side. With a brief,
  the order is actions, constraints, delivery, week, ribbon, changes,
  coverage, detail. With no brief, it renders `herdr_waiting`, a fallback,
  constraints and delivery (:245–251). The file is 1,112 lines.
- KPIs are drawn client-side by `report.html` `kpiLine` (:888) from
  `workspace.kpis` (report.rs:100). `state.json` KPIs carry `current` and
  `evidenceAt` (`src/state.rs:41`, validated at :355).
- **Runs:**
  - `state.json` `runs[]` holds each run's `finishedAt` (`src/track.rs:100`)
    and status, `complete` or `partial` only (track.rs:102); the run
    receipts in `out/runs/` carry `ok`, failed and deferred (`RunStatus`,
    pipeline.rs:73–77).
  - `history::record` and `track::record_run` run before the report renders
    (pipeline.rs:466–467), so after today's brief the newest `runs[]` entry is
    today's.
- **History:**
  - `src/history.rs` `record(home, day, facts, brief)` (:64) runs once per
    brief (pipeline.rs:466) and from `fold_latest_run(home)` (main.rs:1076).
  - It also calls `gh` through `backfill` (:117–156) and writes
    `backfilled.json`.
  - Merged PRs (`Merged`, :31–38) carry a `day` only.

**Flows**

- `src/flows.rs` infers a step per flow (:646).
- The window draws approval cards from the `gates` object in `app.js`
  (`const gates = {`, :1537–1755). Approve calls `ritual_approve`
  (lib.rs:1592).
- `apps/desktop/tests/ipc.rs`
  `the_flows_tab_approves_a_waiting_design_only_on_a_clean_review_or_by_choice`
  (:2938) pins that path.

**Gates**

- `tests/design.rs`:
  - tokens only; light and dark AA;
  - every state through `stateView`;
  - animations only under `prefers-reduced-motion: no-preference`;
  - `flows.html` builds no HTML from data (:762);
  - `sources()` (:23) already picks up every `.css`, `.js` and `.html` at the
    top of `apps/desktop/ui/`.
- Today the window's scripts have one `innerHTML` assignment:
  `states.js:97`, which writes the static art table `window.ART`. No gate
  forbids `innerHTML` in the window's scripts.
- `tests/today.rs`: actions first, no raw URLs, dates in words, escaped text
  (`view_text_cannot_inject_markup`, :250).
- `out/` is gitignored (`.gitignore:2`).

**Source.** rv2-kit is clean-room (`rust-v2/docs/specs/rv2-kit/README.md`),
with no licence header to carry.

## Design

### 0. Tokens, the kit sheet, the kit builders

**Tokens.** `tokens.css` gains job-named tokens, each `light-dark(…)`:

- `--identity-1` … `--identity-6`, from the kit's identity palette. There is
  no orange or red, because those mean warn and fault.
- `--identity-1-soft` … `--identity-6-soft`, as `color-mix` with
  `--background`.
- `--think` for reasoning spans.
- `--space-1` … `--space-6` (0.25–1.5rem). The gate allows no px above 2.

`every_colour_token_has_a_light_and_a_dark_value` and
`text_tokens_meet_aa_contrast_in_light_and_dark` cover the new colours. An
identity colour used as text gets its AA pair added to the contrast list.

**Kit sheet.** `apps/desktop/ui/kit.css` holds every ported component's rules:

- classes are prefixed `kit-`;
- sizes use `rem`, `--space-*`, `--text-*` and `--radius-*`;
- animations sit only under `@media (prefers-reduced-motion: no-preference)`;
- the window links it after `style.css`, and `sources()` picks it up with no
  change.

Served pages that render a component (`report.html`) take the rules through a
new `design::KIT_SLOT`, inlined like `design::SLOT`. A new gate,
`every_page_that_uses_kit_classes_includes_the_kit_sheet`, fails a served page
that uses a `kit-` class without the slot.

**Kit builders.** `apps/desktop/ui/kit.js` (new, loaded before `app.js`)
exports the window components as DOM builders over `states.js` `el()`:

- `fleet`, `runCard`, `timeline`, `steps`, `decisions`, `tile`, `trend`;
- each takes plain data and callbacks, and returns a node;
- `app.js` only fetches and wires.

This keeps `app.js` from growing.

**No HTML or script from data in the window.** `el()` (states.js:8–14)
copies every prop onto the element with `Object.assign`. A data-carrying
prop is therefore a sink of its own, not only the text children. Three
changes close it:

1. **`el()` takes an allowlist, not a denylist.**
   - **Props:** it accepts only `className`, `id`, `title`, `textContent`,
     `type`, `value`, `checked`, `disabled`, `hidden`, `tabIndex`, `role`,
     `name`, `placeholder`, `htmlFor`, `colSpan`, `open`, `href`, `target`,
     `rel`, `ariaLabel` and `aria*` properties, `dataset` (an object of string
     values), `tone`, and `on*` handlers whose value is a function.
   - **URLs:** `href` is the only URL prop it accepts, and it is set only
     through `safeUrl()`, a new `states.js` function. `safeUrl()` returns the
     value when it parses as `http:`, `https:` or `mailto:`, and `null`
     otherwise, in which case nothing is set.
   - **Refused:** any other key throws, including `innerHTML`, `outerHTML`,
     `srcdoc`, `src`, `srcset`, `action`, `formAction`, `poster`, `data` and
     `style`, and any `on*` key with a non-function value.
   - **Reach:** `el()` is the one door every window builder uses, so the rule
     holds for `app.js`, `kit.js` and the rest without a per-call check.
2. **A new gate, `window_never_writes_html_from_data`,** modelled on
   `flows_page_never_writes_html_from_data` (tests/design.rs:762).
   - **Files:** the explicit set `apps/desktop/ui/*.js`, top level only.
     `ui/vendor/` and `ui/dompurify/` are vendored and excluded by path.
   - **It fails on:**
     - `innerHTML`, `outerHTML`, `insertAdjacentHTML`, `document.write`,
       `createContextualFragment` and `DOMParser`;
     - any `setAttribute(` whose first argument is not a string literal, and
       any literal `setAttribute("on…")`;
     - any assignment to a URL property (`.src =`, `.href =`, `.srcset =`,
       `.action =`, `.formAction =`, `.poster =`, `.data =`);
     - a computed key (`[expr]:`) inside an `el(` props object.
   - **Exceptions,** each named in the test with its reason:
     - `states.js` writes `window.ART` (static art, no data);
     - `states.js` sets attributes from the vendored Lucide node table
       `window.LUCIDE` (static icon geometry, no data);
     - `states.js` `frameLoad` sets `frame.src` for the report and dashboard frames.
       Its value is the app's own `cosreport://` page or a loopback dashboard
       URL, and `frameLoad` passes it through a new `safeFrameSrc()` that
       admits only `cosreport:` and `http://127.0.0.1`/`localhost` origins.
3. **Fixtures.** Each rule gets its case in `each_rule_catches_its_violation`:
   - `x.innerHTML = d`;
   - `el("a", {[k]: v})`;
   - `setAttribute("onclick", d)`;
   - `setAttribute(k, v)`;
   - `img.src = d`.

   A unit test of `el()` runs under the e2e harness:
   - `{href: "javascript:alert(1)"}` sets no `href`;
   - `{src: d}`, `{formAction: d}` and `{innerHTML: d}` each throw;
   - a markup string as a child renders as text.

**Identity.** A hire's identity index is
`1 + fnv1a(role or "unassigned") % 6`. It is computed in Rust
(`agents::identity`) and shipped on each row, so every hire of one role has
the same colour everywhere.

### 1. Hire events (new history)

**Two modules, so the fold has no I/O:**

- **`src/agents/timeline.rs`** (pure, no I/O):

  ```rust
  pub enum Kind { Hired, Started, Word, FirstMessage, Said, Paused, DeadlineStop, MarkedDone, Ended, Judged }
  pub enum By { Cos, You, Herdr }
  pub enum Word { Idle, Working, Done, Blocked, Error }
  pub struct Event {
      pub at: String,                // RFC 3339
      pub kind: Kind,
      pub by: By,
      pub word: Option<Word>,        // Word only
      pub turn: Option<u64>,         // Word, Paused, FirstMessage; Seen.turn is u64
      pub status: Option<Status>,    // Ended: the run's agents::Status
      pub outcome: Option<Outcome>,  // Judged: judge::Outcome
      pub judged: Option<String>,    // Judged: the verdict's own timestamp
      pub chars: Option<usize>,      // Said: length only
  }
  pub fn word(herdr: &str) -> Option<Word>;   // exhaustive match, `_ => None`
  pub fn fold(spec: &Spec, state: &State, events: &[Event], now: Timestamp) -> Timeline;
  pub fn totals(events: &[Event]) -> Totals;                  // now-independent, cacheable
  pub fn summary(t: &Totals, now: Timestamp) -> Summary;      // active_ms, paused_ms, paused_now
  ```

- **`src/agents/events.rs`** (file adapter): `append(home, id, &Event)`,
  `read(home, id)`, `tail_summary(home, id, now)`.

Every field is an enum, a number or a timestamp, so free text cannot reach
the log through this type.

**Writers.** Each is one call after the action it records has succeeded:

| Kind | Written in | Condition |
|---|---|---|
| `Hired` | `mod.rs`, after `spec.json` (:568) | always |
| `Started` | `launch`, where `status` becomes `Running` (:622) | always |
| `Word` | `follow::observe` | `timeline::word(new)` differs from `timeline::word(prev.word)`, compared in memory against `state.herdr`. The log is read only on a change. An unknown word maps to `None` and is not written. |
| `FirstMessage` | `follow::observe` | when `Seen.first_message` goes from none to some |
| `Said` | `follow::say` | after `hc send` succeeds: `chars = text.chars().count()` |
| `Paused` | `follow::pause` | after `hc interrupt` succeeds, with the current `turn` |
| `DeadlineStop` | `follow::observe` | when it sets `herdr.interrupted` (:165) |
| `MarkedDone` | `follow::mark_done` | before `finish` |
| `Ended` | `follow::close` | with the final `Status` |
| `Judged` | `follow::judge` | only when a new verdict is written, never on the early return (:411–413), and with the verdict's `judged` timestamp |

**Appending.**

- `append` takes `flock` on `out/agents/<id>/events.lock` and writes one JSON
  line with `O_APPEND`.
- A `Word` append also re-reads the log's last `Word` under the lock, so two
  followers that both saw the change append it once.
- A `Judged` append is skipped when the last `Judged` carries the same
  `judged` timestamp.
- A failed append writes an audit line (run id, kind, error) and does not fail
  the action. The run's `state.json` stays the source of truth for status, and
  the log is history.

**State write race (existing, fixed here).** `write_state` (mod.rs:1058) and
`write_json` take a unique temp name, `state.json.<pid>.<n>.tmp`, then rename.
The two followers can then no longer interleave on one temp file. Rename stays
atomic, and the last writer wins, as now.

**Paused time.**

- A `Paused` event opens a paused interval and records `turn`.
- The interval closes at the first of:
  - `Said`;
  - `MarkedDone`;
  - `Ended`;
  - a `Word = working` whose `turn` is greater than the `turn` recorded on the
    `Paused` event.
- A `working` word in the same turn, polled while the interrupt is still
  landing, does not close it.
- `DeadlineStop` does not open a paused interval: the grace before the kill
  counts as active time.
- `active_ms` = (ended or now) − started − `paused_ms`.

**Fold.** `fold` returns a `Timeline`:

- **spans:** `working`, `waiting`, `paused`. Waiting is a word in `idle`,
  `blocked` or `done` before judgement.
- **hand-offs:** each `Said`, and the `FirstMessage`, from you to the hire.
- **marks:** `Hired`, `Started`, `DeadlineStop`, `MarkedDone`, `Ended`,
  `Judged`.
- **totals:** `active_ms`, `paused_ms`, `paused_now`.

Each span carries its word and turn, and no message.

- **`Hired` then `Ended` with no `Started`** (a hire that failed in
  preparation) gives no spans and one note: "ended before it started".
- **A run with no events** (hired before this change) gets marks from its
  `created`, `started`, `herdr.interrupted`, `ended` and `verdict.judged`. It
  gets no spans and `paused_ms = 0`, and a note that word history starts at
  this version.

**Read cost.**

- `agents_list` rows gain `identity`, `active_ms` and `paused_now` through
  `events::tail_summary`.
- **The cache holds only what does not depend on `now`.**
  `timeline::totals(events) -> Totals` returns:
  - `started` and `ended` (if any);
  - closed `paused_ms`;
  - `open_pause_since` (the start of an unclosed paused interval, if any).

  `events::tail_summary` caches `Totals` per run, keyed by (path, file length,
  mtime), and refolds only when the file changed.
- **`active_ms` is computed at read time:**
  `summary(&Totals, now)` gives
  `active_ms = (ended or now) − started − paused_ms − (now − open_pause_since if paused)`.
  `paused_now` is `open_pause_since.is_some()`.
- **Live clocks:**
  - `agents_list` returns `Totals` with `active_ms` at call time;
  - the fleet and the run card advance a live, unpaused clock client-side once
    a second from that value, and a held clock stays still;
  - a test advances `now` with the file unchanged and sees `active_ms` grow.
- **Read bound:** one `metadata` call per run per list, one fold per file
  change, and fold cost linear in the number of events.
- The full `Timeline` is read only by the new read-only command
  `agents_timeline(business, id)`, for the open run page. That command is
  added to `generate_handler!` (lib.rs:2043).

**Refresh.**

- Appends do not move the hire directory's mtime, so they do not by
  themselves emit `agents-changed`.
- Every recorded transition except `Said`, `Paused` and `FirstMessage`
  coincides with a `state.json` rename, which does move it.
- `say` and `pause` already end in `observe`, and the window refreshes the run
  after its own command returns.

**Lifetime.** `cos agents clean` deletes the run's directory, and its
`events.jsonl` with it. The digest and the scorecard read only the ledger,
which survives.

### 2. Agents tab components

**Fleet (from `worker_fleet_panel`)** replaces the runs table as the first
view.

- **Head:** count of hires, live, needs you, done today.
- **Column:** an `ol` of hire cards. Each card shows:
  - an identity mark with the role's initial;
  - the title;
  - a state pill: `starting`, `working`, `needs you`, `paused`, `done`,
    `failed`, `lost`, `timed out`, `stopped`;
  - the role;
  - Herdr's latest message, from current state, as text;
  - active time.
- **Keys:** arrow keys, Home and End move focus. Enter opens the run page.
- **Respond buttons:**
  - needs you: Reply, Open in Herdr;
  - failed or lost: Open run.

  They call the existing commands.
- **Dropped:** the kit's sub-agent tree, because cos hires have no
  sub-agents.
- **Kept:** the existing table, behind a Fleet/Table segmented toggle, which
  the ipc tests address. Empty and loading states stay `stateView`.

**Run card (from `single_run_status_card`)** heads the run page.

- **Phase:**
  - `standby`: starting, queued, seeding;
  - `active`: running and working;
  - `needs_you`: running with an idle or blocked word after turn 1;
  - `held`: `paused_now`;
  - `complete`: done;
  - `errored`: failed, lost, timed out;
  - `stopped`: killed.
- **Clock:** `active_ms`, which freezes while held.
- **Strip:** one segment per timeline span, coloured by mode.
- **Body:** the current line is Herdr's message from current state, as text.
  The verdict summary shows when judged.
- **Footer actions:**

  | Phase | Actions |
  |---|---|
  | active | Pause, Mark done |
  | held | Reply, Mark done |
  | needs_you | Reply, Open in Herdr |
  | errored | Open screen |
  | stopped | Open screen |

  These are the existing commands.
- **Below the card:** the verdict, contract and diff sections stay as they
  are.

**Timeline (from `run_timeline_rows`)** sits under the card.

- **Rows:** two, You and the hire, on one time axis.
- **Spans:** working as tool fill, waiting as a dashed warn outline, paused as
  a muted fill.
- **Hand-offs:** each Said and the first message.
- **Live runs:** a playhead at now.
- **Keys:** arrow keys move between spans. Each span opens a detail popover
  with its kind, start, length, word and turn. The log stores no message, so
  the popover shows none.
- **Narrow:** under 30rem the name column narrows.

### 3. Today

**Since your last brief (from `overnight_run_digest`, `absence_digest`).**

`src/digest.rs` (new) builds `digest` into report data and renders it.

- **Boundary:** `at` (pipeline.rs:85) of the newest `out/runs/<day>.json`
  receipt with status `ok` (`RunStatus`, pipeline.rs:73–77) whose day is
  before today in local time.
  - Failing that, `finishedAt` of the newest `state.json` `runs[]` entry with
    status `complete` (track.rs:102; the statuses are `running`, `complete`,
    `partial`, `blocked`, `interrupted`, state.rs:387–392) and a local day
    before today.
  - Without either, there is no digest, and the section is omitted.
  - Every comparison parses both sides as `jiff::Timestamp`. Ledger
    `judged`, incident `observedAt`, `finishedAt` and `at` are all stored as
    strings. A string that does not parse leaves its item out and writes a
    `logDev`-style audit line.
- **Decisions:**
  - the brief's `needs_you` actions, current and not time-filtered;
  - hires waiting in Herdr (`herdr_waiting`, current);
  - ledger rows with outcome `needs_you` and `judged` after the boundary.
- **Errors:**
  - ledger rows with outcome `rejected`, or status `failed`, `lost` or
    `timed_out`, judged after the boundary;
  - workspace incidents with `observedAt` after the boundary;
  - a last run that failed.
- **Done:**
  - ledger rows with outcome `accepted` judged after the boundary;
  - merged PRs from `history.merged` merged after the boundary.

  `Merged` (history.rs:31–38) gains `#[serde(default)] merged_at:
  Option<String>`, filled from the `closedAt` that `gather` already fetches
  for every PR (gather.rs:256; a merged PR closes when it merges). A PR merged
  after a brief on the same day is then folded by the next brief with its
  `merged_at`, and counted when that is after the boundary. Rows archived
  before this change have no `merged_at`. They count when their `day` is after
  the boundary's local date, and never on the boundary's own date.
- **Item text:** a ledger item's title is the run's `spec.json` title while
  the run directory exists, else the ledger row's new `title` field. `Row`
  gains `#[serde(default)] title: Option<String>`, set by `ledger::row` from
  the spec, so a cleaned run still has a title. Rows written before this
  change, with no title, read "An agent run". The item's source names the
  run's role ("builder agent", or "agent run" without one), never its run id.
- **Item fields:** each item carries its source and, where one exists, `at`
  (rendered as words) and a link chip from `today::chips`.
- **Rendering:** three lanes, three items each with "and N more", counts in
  the heading, and a cumulative SVG chart of the timed items. Every string
  goes through `today::esc` / `today::prose`.
- **Dropped:** the kit's act, revert and reapply. The report recommends; the
  user acts in the tools the chips open.
- **Placement:**
  - with a brief: immediately after the actions, so
    `the_actions_come_before_every_other_section` holds;
  - with no brief: after `herdr_waiting`.

**KPI points (new history).**

- **Writer:** `history::record_kpis(home, ws)` is a function of its own, with
  no network calls, separate from `record` and its `gh` backfill.
  - It is called only from the brief pipeline, right after
    `history::record` (pipeline.rs:466), with the workspace the pipeline
    already holds.
  - `fold_latest_run` (main.rs:1076, called by `cos report` at main.rs:445)
    is unchanged and does not record KPI points. The report path stays free
    of new writes.
- **What it records:** it reads `state.json` `kpis[]`. For each KPI with a
  finite `current` and a parseable `evidenceAt`, whose (`name`, `evidenceAt`)
  pair is not yet in `out/history/kpis.jsonl`, it appends
  `{kpi: name, value, evidenceAt, recordedAt}`. KPIs have a `name` and no id
  (state.rs:340).
- **Locking:** the read-dedup-append runs under `flock` on
  `out/history/kpis.lock`.
- **Reading:** `history::kpi_points(home)` reads the file back.

**KPI tiles (from `metric_tile`, `metric_trend_card`).** `src/kpis.rs` (new)
renders one tile per KPI, server-side, inside the week section. It replaces
`report.html` `kpiLine`.

- **Tile:**
  - name;
  - current value with unit;
  - change since the first point in the reporting window, as a direction
    badge, favourable by target direction;
  - a target meter;
  - a sparkline with two or more points.
- **Trend card:** opens in place, with the full series and a value readout in
  a polite live region. The report's existing client script steps it with the
  arrow keys.

`today.rs` calls `digest::section` and `kpis::section`, and grows by only
those two calls.

### 4. Flows tab

**Steps (from `step_progress_list`)** renders through `kit.js` `steps`, in the
Flows host.

- **States:** for the selected flow, the ritual strip (`think` … `reflect`)
  renders as an ordered list:
  - steps before the current: `finished`;
  - the current step: `in_progress`, or `awaiting_approval` at the gate;
  - drift: `errored`, with the drift reason;
  - later steps: `queued`.
- **Build sub-items:** the epic's beads, with done and open marks, folding to
  5 when there are more than 6.
- **No step times:** step markers carry no timestamps. The bead counts stand
  in, and no time is invented.

**Decisions (from `pending_decision_stack`)** renders through `kit.js`
`decisions` and replaces the gate cards drawn by `gates`.

- **Stack:** the top gate is a card with the design path, digest, both review
  verdicts, findings counts and history. Up to two more gates show as peeks;
  clicking a peek, or the arrow keys on the stack, brings it to the top.
- **Actions:**
  - Approve: enabled on a clean current review;
  - Approve without review: behind the existing confirm;
  - Open design.

  Both approvals call `ritual_approve` with the same arguments as today.
- **History:** a list of the flows approved this session.
- **Kit defects fixed in the port:**
  - focus after a decision lands on the next card's Approve;
  - `data-status` reflects the outcome.

## Alternatives

- **Vendoring kit.css:** two token systems, and served pages would need it
  inlined too. Rejected in the brief.
- **Recording events inside `state.json`:** it is rewritten on each change,
  and every `agents_list` reads it whole. A separate log keeps the state read
  fixed-size; the log is summarised through a cache keyed by file length and
  mtime.
- **Storing Herdr messages in the log:** this would give the timeline popover
  text, but it puts free agent text on disk in a second place. Current state
  already shows the latest message, and `screen.txt` keeps the end of the
  terminal.
- **KPI history inside `state.json`:** the kit's `state.py` owns that schema.
  History in `out/history/` stays cos's.
- **Adding KPI capture inside `history::record`:** that function calls `gh`.
  A separate function keeps the KPI write local and testable without a
  network.
- **Recording KPI points on report render:** a write on a read path. Brief
  time is the write point.
- **The kit's act and revert on Today:** the report is read-only and
  recommends.

## Security and privacy

- **Read:** hire state and spec, the ledger, `state.json` (runs, incidents,
  KPIs), `out/runs/`, `out/history/`.
- **Stored, under `out/` (gitignored, `.gitignore:2`), local:**
  - event kinds, timestamps, Herdr words as an enum, turn counts, the length
    of each reply;
  - KPI values that `state.json` already holds;
  - each ledger row's spec title.
- **Not stored by this change:** reply text or Herdr messages.
- **Already stored, unchanged:** `screen.txt` (follow.rs:343) keeps the last
  200 lines of a finished hire's terminal, which includes replies the user
  typed. It is local, under `out/`, read back by Open screen, and deleted by
  `cos agents clean`. This change neither adds to it nor reads it.
- **Served:**
  - the digest and KPI tiles reach the window's report iframe and `cos dash`
    `/` (loopback, `report::data`);
  - the MCP server does not serve `report::data` (`src/mcp.rs:105–140`), so
    nothing new reaches claude.ai;
  - the digest's sources are the ledger, incidents, run receipts, merged PRs,
    `herdr_waiting` and the brief, all already in `report::data` or the ledger
    on disk.
- **Rendering:** `kit.js` and `app.js` build DOM through `el()` text nodes,
  pinned by `window_never_writes_html_from_data`. `digest.rs` and `kpis.rs`
  escape through `today::esc` / `prose`. `flows.html` is unchanged.

## Semantic ISA

### Scope

- **Agent:** none new. This change adds rendering, two cos-owned histories, a
  ledger field and a unique temp name for state writes.
- **Environment:**
  - the local `out/` tree;
  - Herdr through `hc`;
  - `gh` (existing, via `history::backfill`);
  - `cn` via `ritual_approve`.
- **Harms, ranked:**
  1. A history write that corrupts or blocks a run's state or verdict.
  2. Text from a hire, a brief or a ledger row rendered as markup.
  3. A number on screen not backed by a record.
  4. New free text from a hire or a reply persisted by this change. The one
     new persisted string is the spec title copied into the ledger. It is
     text the user or the brief wrote when hiring, already in `spec.json`, and
     it is stored tainted and escaped at render.

### Side-effect inventory

| Capability | System | New or existing | Reversible? | Affected if it misfires |
|---|---|---|---|---|
| append `events.jsonl`, create `events.lock` | `out/agents/<id>/` | new | yes (delete; nothing decides from it) | the run's timeline view |
| append `kpis.jsonl`, create `kpis.lock` | `out/history/` | new | yes (delete lines) | the KPI trend |
| ledger row gains `title` | `out/agents/ledger.jsonl` | new field on an existing append | yes (field ignored by old readers) | the digest's item text |
| `state.json` and `spec.json` temp files renamed to a unique name (`write_json`, mod.rs:1063, used for `spec.json` at :568 and by `write_state`) | `out/agents/<id>/` | changed | yes | run status or spec, if the rename failed |
| render digest, tiles, fleet, run card, timeline, steps, decisions | report HTML, window DOM | new | yes | the user, through misleading or injected markup |
| `hc run` (launch) | Herdr | existing, unchanged | no (a process starts) | the hire |
| `hc send` (say) | Herdr | existing; now followed by a `Said` append | no (keys sent) | the hire |
| `hc interrupt` (pause; deadline stop) | Herdr | existing; now followed by `Paused` / `DeadlineStop` | no | the hire |
| `hc kill` (finish) | Herdr | existing, unchanged | no | the hire |
| `hc rm` (clean) | Herdr | existing, unchanged | no | the terminal |
| `hc focus` (open in Herdr) | Herdr window | existing, unchanged | yes | the user's focus |
| `hc read` / `hc get` (screen, observe) | Herdr | existing, read-only | n/a | nothing |
| write `screen.txt` | `out/agents/<id>/` | existing, unchanged | yes | holds reply text, locally |
| `judge.lock`, verdict write, ledger append | `out/agents/` | existing; now followed by a `Judged` append | append-only | the verdict |
| `gh` merged-PR reads, `backfilled.json` write (`history::record`) | GitHub API, `out/history/` | existing, unchanged | read / yes | the archive |
| `Merged` gains `merged_at` from the fetched `closedAt` | `out/history/merged.json` | new field on an existing write | yes (field ignored by old readers) | the digest's Done lane |
| append an audit line on a failed event append, an unparseable timestamp or a refused KPI point | `out/audit/<day>.jsonl` (claude.rs:402–404) | new lines in an existing log | append-only, local | nobody; it carries run id, kind or KPI name, and the error, with no hire or reply text |
| `ritual_approve` → `cn approve` | `cn` task store | existing, unchanged | approval line appended; superseded by a new digest | the flow |

### Instructions

| Instruction | Core | Governance property | Sink? | Replaces capability |
|---|---|---|---|---|
| `LOAD(run_state)` | Memory | deterministic I/O | no | `agents::read_raw` |
| `LOAD(events)` | Memory | deterministic I/O | no | `events::read`, `tail_summary` |
| `FOLD(timeline)` | Normative | pure function of (spec, state, events, now) | no | `timeline::fold` |
| `MAP(word)` | Normative | exhaustive match, unknown → none | no | `timeline::word` |
| `STORE(event)` | Memory | deterministic append under lock | **yes** | `events::append` |
| `STORE(kpi_point)` | Memory | deterministic append under lock | **yes** | `history::record_kpis` |
| `STORE(ledger_title)` | Memory | deterministic append | **yes** | `ledger::append` |
| `RENDER(window_dom)` | Execution | deterministic | **yes** | `kit.js`, `app.js` |
| `RENDER(report_html)` | Execution | deterministic | **yes** | `digest.rs`, `kpis.rs` |
| `VERIFY(event_shape)` | Normative | type check | no | — |
| `VERIFY(kpi_point)` | Normative | deterministic checkpoint | no | — |
| `VERIFY(escaped)` | Normative | deterministic checkpoint | no | — |

The Herdr, `gh` and `cn` calls are existing capabilities this change does not
alter. Each is listed in the inventory above, and no new path reaches them.

### Bindings

```json
{
  "instruction": "STORE(event)",
  "input_schema": {
    "type": "object",
    "additionalProperties": false,
    "required": ["at", "kind", "by"],
    "properties": {
      "at": { "type": "string", "format": "date-time" },
      "kind": { "enum": ["hired", "started", "word", "first_message", "said", "paused", "deadline_stop", "marked_done", "ended", "judged"] },
      "by": { "enum": ["cos", "you", "herdr"] },
      "word": { "enum": ["idle", "working", "done", "blocked", "error"] },
      "turn": { "type": "integer", "minimum": 0 },
      "status": { "enum": ["starting", "queued", "seeding", "running", "done", "failed", "killed", "timed_out", "lost"] },
      "outcome": { "enum": ["accepted", "needs_you", "rejected"] },
      "judged": { "type": "string", "format": "date-time" },
      "chars": { "type": "integer", "minimum": 0 }
    }
  },
  "output_schema": { "type": "null" }
}
```

```json
{
  "instruction": "STORE(kpi_point)",
  "input_schema": {
    "type": "object",
    "additionalProperties": false,
    "required": ["kpi", "value", "evidenceAt", "recordedAt"],
    "properties": {
      "kpi": { "type": "string", "minLength": 1 },
      "value": { "type": "number" },
      "evidenceAt": { "type": "string", "format": "date-time" },
      "recordedAt": { "type": "string", "format": "date-time" }
    }
  },
  "output_schema": { "type": "null" }
}
```

```json
{
  "instruction": "STORE(ledger_title)",
  "input_schema": { "type": ["string", "null"], "description": "the hire's spec title, tainted (user or brain authored)" },
  "output_schema": { "type": "null" }
}
```

```json
{
  "instruction": "RENDER(report_html) / RENDER(window_dom)",
  "input_schema": { "type": "object", "description": "report data or a Tauri command result; every string field is tainted" },
  "output_schema": { "type": "string", "description": "HTML whose data strings passed esc/prose, or DOM text nodes" }
}
```

### Taint and sinks

- **Taint sources:**
  - Herdr word, message and terminal text (written by the hire);
  - brief JSON (model output);
  - spec titles (user or brain);
  - ledger rows;
  - `state.json` (the kit's `state.py` and the user).
- **Propagation:**
  - **`MAP(word)`:** drops the string and keeps an enum, so its output is
    untainted by construction.
  - **`STORE` → `LOAD`:** the event log round-trip carries enums and numbers
    only.
  - **The ledger round-trip:** carries `title`, which stays tainted. Every
    renderer treats a loaded string as tainted.
- **Sinks and clearing:**

| Sink | Accepted input must be cleared by | Check |
|---|---|---|
| `STORE(event)` | `VERIFY(event_shape)` | `Event` has no string field except `at` and `judged`, both produced by `jiff::Timestamp::to_string`. `word` comes only from `timeline::word`, an exhaustive `match` whose `_` arm returns `None`, and a `None` word is not appended. `Said` carries `chars`. |
| `STORE(kpi_point)` | `VERIFY(kpi_point)` | the value is a finite number; `evidenceAt` parses as a timestamp; the KPI name is not blank; the (name, `evidenceAt`) pair is not in the file, checked under `kpis.lock` |
| `STORE(ledger_title)` | — (stored tainted) | stored as data; cleared at render by `VERIFY(escaped)` |
| `RENDER(report_html)` | `VERIFY(escaped)` | every data string goes through `today::esc` or `today::prose`, and links through `today::chips` (http/https only) |
| `RENDER(window_dom)` | `VERIFY(escaped)` | strings reach the DOM as text children of `el()`. `el()` accepts only its prop allowlist and throws on any other key. `href` is its one URL prop, set only through `safeUrl()` (http, https, mailto), and `on*` must be a function. `window_never_writes_html_from_data` fails any HTML-writing call, non-literal or `on*` `setAttribute`, URL-property assignment or computed `el()` key in `apps/desktop/ui/*.js`. The exceptions are named: `states.js` writing `window.ART` (art), setting attributes from `window.LUCIDE` (Lucide geometry) and setting `frame.src` in `frameLoad` (`frame.src` through `safeFrameSrc()`: `cosreport:` and loopback only). |

### Violations

| Violation | Evidence recorded | Recovery |
|---|---|---|
| an unknown Herdr word | none needed: it is not stored | the run continues, and the next known word is logged |
| an event append fails (disk, lock) | audit line: run id, kind, error | the action still succeeds; the timeline shows a gap |
| a KPI point fails its check | audit line: KPI name, rule | the point is skipped; the brief continues |
| markup in a rendered string | — (it renders as text) | none needed; the tests below prove it |
| an HTML-writing call in a window script | test failure naming file:line | rewrite through `el()` |

### ISA test matrix

| Sink | Tainted input refused | VERIFY-cleared input admitted |
|---|---|---|
| `STORE(event)` | a Herdr word `"<script>"` or `"working; rm"` writes no line; `say("secret text")` writes a `Said` with `chars = 11` and no text (`tests/hire_events.rs`) | `idle` then `working` appends one `Word`, even from two threads observing at once (`tests/hire_events.rs`) |
| `STORE(kpi_point)` | `NaN`, a missing `evidenceAt` and a duplicate pair, including from two threads at once, write nothing (`tests/history.rs`) | a new `evidenceAt` appends one line |
| `STORE(ledger_title)` + `RENDER(report_html)` | a spec title `<img src=x onerror=…>` renders escaped in the digest (`tests/today.rs`) | a plain title renders as text with its chip |
| `RENDER(window_dom)` | the gate fails on the fixtures `x.innerHTML = d`, `el("a", {[k]: v})`, `setAttribute("onclick", d)`, `setAttribute(k, v)` and `img.src = d` (`each_rule_catches_its_violation`); in the e2e `el()` test, `{href: "javascript:alert(1)"}` sets no href and `{src: d}`, `{formAction: d}`, `{innerHTML: d}` each throw; `safeFrameSrc("javascript:…")` returns `null` | an `https:` href is set; a markup string child renders as text; the window's scripts pass the gate as written |

## Testing

- **Unit (`tests/hire_events.rs`):**
  - spans from words;
  - a paused interval closed by `Said`;
  - a same-turn `working` word that does not close it;
  - a later-turn `working` that does;
  - `DeadlineStop` not counted as paused;
  - `Hired` → `Ended` with no `Started`;
  - a run with no events;
  - a live run against `now`;
  - the unknown-word drop;
  - two-thread append dedup;
  - `Judged` written once on an early-return judge;
  - identity hashing;
  - the `tail_summary` cache refolding only on a length or mtime change.
- **Unit (`tests/history.rs`, `tests/today.rs`):**
  - KPI point checks and the two-thread lock;
  - the digest boundary: none, failed runs skipped, today's run skipped,
    receipt fallback;
  - lanes;
  - the merged-day limit;
  - placement with and without a brief;
  - escaping.
- **Integration (`apps/desktop/tests/ipc.rs`):**
  - pause then say on the stand-in Herdr gives one closed paused interval in
    `agents_timeline`, and `agents_list` reports `paused_now` in between;
  - the existing approval test stays green through the decision stack.
- **Design gates (`tests/design.rs`):**
  - `kit.css` and the new tokens;
  - `window_never_writes_html_from_data`;
  - `every_page_that_uses_kit_classes_includes_the_kit_sheet`.
- **WCAG:** `examples/wcag.rs` over the Agents, Today and Flows views, light
  and dark.
- **Seen, not assumed:** `apps/desktop/examples/preview.rs` and
  `task screenshots` on the demo home. `examples/demo.rs` gains fixture
  events, ledger titles and KPI points.

## Confidence

| Piece | Tier |
|---|---|
| tokens, `kit.css`, `kit.js` | rock |
| timeline fold | rock |
| events adapter and lock | rock |
| paused intervals | rock |
| unique temp name | rock |
| fleet, run card | rock |
| timeline view | rock |
| digest | rock |
| ledger title | rock |
| KPI points | rock |
| KPI tiles and trend | rock |
| steps | rock |
| decision stack | rock |
| HTML gate | rock |
| wcag pass on the new views | sand: the run of `examples/wcag.rs` settles it |

## Acceptance

1. On the demo home, the Agents tab opens on the fleet. Each hire of one role
   has the same identity colour in the fleet, the table and the run page.
2. Pause a live hire on the stand-in Herdr, then reply. `agents_timeline`
   shows one closed paused interval, and the run card's clock did not advance
   while held.
3. A run hired before this change opens with marks from its stored timestamps
   and a note that word history starts at this version. A hire that failed in
   preparation reads "ended before it started".
4. Today shows "Since your last brief" after the actions, with Decisions,
   Errors and Done from the ledger, incidents, merged PRs and the brief. A
   spec title with markup renders as text. With no earlier ok run, the section
   is absent.
5. After two briefs with a changed KPI `current` and `evidenceAt`, the KPI
   tile shows a direction and a two-point sparkline, and `kpis.jsonl` has one
   line per (kpi, `evidenceAt`).
6. The Flows tab shows the selected flow's steps, and gates as a decision
   stack. Approve on a clean review approves as before (ipc test green).
7. `task test` passes: fmt, clippy, every test, the design gates.
   `examples/wcag.rs` reports no new violations in light or dark.

## Carried to Plan

Should- and nit-level items from review round 2. Plan turns each into a task.

- **E2E DOM test** for `el()` and the kit builders, on the `task test-e2e`
  harness (as `tests/flows_page.rs`). It is named in the `RENDER(window_dom)`
  matrix row.
- **Lost update in `observe`:** follow.rs:91–160 reads, mutates and rewrites
  state, so two followers can write a stale snapshot over `ended` or
  `verdict`. Plan adds a per-run `state.lock` flock around the
  read-modify-write in `observe`, `close`, `judge` and `write_state` callers,
  with a two-follower test. The unique temp name alone does not close it.
- **`Paused.turn`** comes from the terminal read in `live()` inside `pause`,
  not from `state.herdr.turn`. `Seen.turn` is `u64` (mod.rs:191), and
  `Event.turn` is `Option<u64>` to match. Herdr increments it each time the
  agent stops working, so a `working` word in a later turn means the agent
  started a new turn.
- **KPI key is the KPI's `name`** (state.rs:340; KPIs have no `id`). Points
  are keyed and deduplicated by (name, `evidenceAt`). Duplicate names in
  `state.json` share one series, and the tile shows the newest point; the
  audit log notes the duplicate.
- **`WINDOW_JS` in tests/design.rs:1085 gains `kit.js`**, so the state-view
  gate covers the kit builders.
- **`fold_latest_run`** (only caller main.rs:445) does not change. KPI points
  are recorded by the brief pipeline alone.
- **Boundary day basis** is the business profile's timezone (pipeline.rs:122–128),
  the same day the receipts are named by, not the machine's local time.
- **The trend card's readout in `report.html`** goes through that page's
  `esc` (report.html renders with `esc()` templates, :647–1073).
- **`app.js` size:** the kit builders move to `kit.js`, and the gate and
  fleet wiring stay in `app.js`. Target: `app.js` no larger after this change
  than its 1,799 lines.
- **Privacy wording:** `Seen.msg` is already stored in `state.json`
  (mod.rs:189). The fleet card reads it from there, and this change adds no
  storage for it.
- **Decision stack:** the kit already focuses the next card's Approve after a
  decision (pending_decision_stack.rs:228–233). Plan verifies the port
  behaves the same rather than claiming a fix. `data-status` is set to the
  outcome (approved / approved-without-review) on the decided card in the
  session history.
- **Refresh nits:** the first `events.lock` create moves a hire directory's
  mtime once, which emits one extra `agents-changed`; that is harmless.
  `cos agents clean` refuses a live run (mod.rs:1019–1022), so it never
  deletes a directory whose lock a follower of a live run holds. A follower
  that loses the race on a finished run fails its append, and that failure
  goes to the audit log.

## Review history

- Round 1: [r1-a](../archive/plans/2026-11-25-rv2-kit-desktop-design-review-r1-a.md) (3
  blocking, 8 should, 4 nit) and
  [r1-b](../archive/plans/2026-11-25-rv2-kit-desktop-design-review-r1-b.md) (3 blocking, 14
  should, 4 nit), both against sha256 `951469c8…`. All closed in revision 2.
- Round 2: [r2-a](../archive/plans/2026-11-25-rv2-kit-desktop-design-review-r2-a.md) (3 new
  blocking: boundary statuses, `el()` prop sink, now-dependent cache) and
  [r2-b](../archive/plans/2026-11-25-rv2-kit-desktop-design-review-r2-b.md) (3 new blocking:
  boundary statuses, merged-day gap, audit sink), both against sha256
  `67d70e79…`. The blocking findings are closed in revision 3, and should and
  nit items are under Carried to Plan.
- Round 3: [r3-a](2026-11-25-rv2-kit-desktop-design-review-r3-a.md) (4
  blocking: the window gate missed computed `setAttribute` names and direct
  URL-prop assignments, KPI write wired into `cos report`, `spec.json` temp
  name missing from the inventory, a false `runs[]` context line) and
  [r3-b](2026-11-25-rv2-kit-desktop-design-review-r3-b.md) (2 blocking: the
  `setAttribute` bypass and `el()` URL props beyond `href`/`src`), both
  against sha256 `4867a470…`.
  - After round 3 the owner chose "Fix, then approve" over a fourth round.
  - Revision 4 closes all five distinct findings: an `el()` prop allowlist,
    a gate over `setAttribute`, URL assignments and computed keys with three
    named exceptions, KPI points recorded by the brief pipeline only, the
    `spec.json` inventory row, and the corrected context line.
  - Approved without a further review, at the owner's instruction.
