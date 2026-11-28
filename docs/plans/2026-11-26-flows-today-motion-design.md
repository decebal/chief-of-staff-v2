# Flows and Today, in motion: design

Date: 2026-11-26. Brief: [2026-11-26-flows-today-motion-brief.md](2026-11-26-flows-today-motion-brief.md).
Epic: `t-e89af1`. Branch: `feat/desktop-flows-today-motion`. Tier: lean.

## Decision

Port eight rv2-kit pieces (rust-v2 `7ff3ec5`) as native DOM on `tokens.css`
and `apps/desktop/ui/kit.css`, under one shared motion grammar.

| Surface | Kit source | Becomes |
|---|---|---|
| Flows | `run_process_lanes` | **Lanes** view of one flow |
| Flows | `step_progress_list` | **Steps** of the ritual, in a side column |
| Flows | `expandable_work_item_card` | **Bead ring** on board cards |
| Flows | `task_branch_scene` | **Scene** for the page head and its empty state |
| Flows | `kanban_task_board`'s glide | **Beads** column view |
| Today | `glow_floor_backdrop`, `flowing_gradient_text`, rv2-affect `text_reveal`, `progress_pill_scene`, rv2-affect `thinking_loader` | **Living hero** |
| Today | `background_task_inbox` | **Hires inbox**, replacing "Waiting on you in Herdr" |
| Today | `agent_schedule_board` | **Day timeline**, ported as it is |
| Both | rv2-affect `number_ticker`, `blur_swap_container`, rv2-affect `shimmer_loader` | **Ticking KPIs, crossfades, shimmer loading** |

Every animation, transition and scripted `animate()` runs only under
`prefers-reduced-motion: no-preference`, and a static gate proves it. Under
`reduce`, each component renders its settled end state.

Nothing writes data. The hire reads this change adds never reconcile, signal
or write, and the report stops reconciling on render.

## Context

**Flows page** (`src/flows.html`):
- It is a CSP page in a frame with an opaque origin (`flows::PAGE_CSP`), with
  its scan inlined at `/*__COS_FLOWS__*/` (`flows.rs:940`, `page_html_from`
  :953).
- It builds DOM through `h()` (:268), under
  `flows_page_never_writes_html_from_data` (`tests/design.rs:845`).
- Its own `<style>` holds a breathe keyframe that runs by default
  (`flows.html:128–129`).
- **Data:**
  - `Flow` (`flows.rs:270–289`) carries `step` and the beads.
  - Each bead (`flows.rs:162`) carries `id`, `title`, `status`,
    `blocked_by`, `ready`, `owner` (`claimed_by`), `layer` (its blocker-chain
    column), `open_blockers`, `created_at` and `done_at`.
  - Drift is a flow-level state (`Step::Drift`), not a bead status.
- **`flows::scan`** (`flows.rs:707`) takes the projects directory and a
  deadline, not the business home. Its callers are `ritual.rs:574` and
  `main.rs:760`, plus the desktop's flows commands.

**Hires:**
- A hire started from a bead stores `spec.source = "bead:<repo>/<bead id>"`
  (`agents/mod.rs:569`, from `lib.rs:1763`). `<repo>` is the same string as
  the Flows page's `flow.repo`, because the page sends it in
  `cos-flows-hire`.
- **`agents::list`** (`mod.rs:949–977`) is not a pure read. It reconciles:
  - it writes `state.json` (:963);
  - it can SIGKILL a legacy process group (:954–959);
  - it calls `follow::end` (:969).
- `report::data` calls it on every render (`report.rs:51`). Its
  `herdr_waiting` also feeds `digest::build` (`report.rs:52–60`).
- The hire phase mapping lives only in JS (`kit.js:77–79`, :237–240). "Done
  today" uses the machine's local date (`kit.js:90`).

**Today report** (`src/today.rs` into `src/report.html`):
- `hero` (:120) renders the ink band and run pill. `body` (:243) puts
  `herdr_waiting` (:282) first.
- `ribbon` (:646–767) already draws a 07:00–20:00 axis with a now line and
  past or upcoming slots across the week.
- `report.html` has its own `rise` animation that runs by default
  (:235–241). Its client code renders with `innerHTML` and `esc()` templates
  (:648 onward); no gate covers that page's script.
- The window frames the report with `sandbox="allow-scripts allow-popups"`
  (`index.html:196`), an opaque origin, so the report cannot use
  `sessionStorage` or `localStorage`.
- **Messages from the window:** `cos-host` (`app.js:242`) and
  `cos-brief-state` (`app.js:1360`, `tellBrief`), which already carries the
  brief's running state and text.

**Gates:**
- `tests/design.rs` `every_animation_respects_reduced_motion` (:1452) reads
  only `style.css`, and only the `animation:` shorthand with a space.
- Neither transitions nor scripted `animate()` are gated.
- `every_state_renders_through_state_view_with_an_icon_and_art` (:1325)
  requires an icon and art per state kind.
- The contrast test checks solid token pairs only (:593).

## Design

### 0. Motion grammar, tokens, and the gate that proves it

**Tokens.** `tokens.css` gains:

- `--motion-fast: 140ms`, `--motion-slow: 320ms`, `--motion-loop: 1.8s`,
  `--motion-stagger: 90ms`;
- `--ease-spring: cubic-bezier(0.2, 0.8, 0.2, 1)`;
- `--glow-1`, `--glow-2` (light-dark) for the hero floor;
- `--gradient-from`, `--gradient-to` (light-dark) for gradient text. Each
  stop is added to the contrast test as text on `--hero`, so gradient text
  meets AA at every stop.

**Keyframes.** One shared set in `kit.css`, named for states. Components
compose them; none brings its own.

| State | Keyframe | Effect |
|---|---|---|
| running | `kit-shimmer` | background-position sweep over one `--motion-loop` |
| waiting / needs you | `kit-breathe` | box-shadow ring to `0 0 0 0.25rem color-mix(in srgb, var(--warning) 18%, transparent)` and back |
| appearing | `kit-settle` | opacity 0→1 and translateY `0.375rem`→0, delayed `calc(var(--i) * var(--motion-stagger))` |
| hand-off | `kit-arrive` | one scale pulse, once per transfer id |
| connecting | `kit-draw` | stroke-dashoffset 1→0 on `pathLength="1"` paths |
| travelling | `kit-flow` | stroke-dashoffset loop over a `stroke-dasharray: 0.08 0.12` path |
| failed / drift | `kit-flash` | one fading tint of `--destructive` |
| done | `kit-tick` | a check path drawn once |

**Scripted motion.** All scripted motion goes through one helper per
document, `motionOk()`:

```js
const motionOk = () => !matchMedia("(prefers-reduced-motion: reduce)").matches;
```

The three documents are the window (`kit.js`), `report.html` and
`flows.html`. `glide(container, render)` (FLIP) and `swap(container, next)`
(crossfade) call `animate()` only when `motionOk()` is true.

**The gate.** `every_animation_respects_reduced_motion` is rewritten. Over
`style.css`, `kit.css` and the `<style>` blocks of `report.html`,
`flows.html` and `docs.html`, it fails on:

- any `animation`, `animation-name` or `transition` declaration outside
  `@media (prefers-reduced-motion: no-preference)`, matched with or without
  a space after the colon;
- any `.animate(` call in `apps/desktop/ui/*.js`, `report.html` or
  `flows.html` outside a function whose body tests `motionOk()`;
- any `@keyframes` that no rule under `no-preference` uses.

A `transition` of `--motion-fast` or less on a colour or opacity (hover
feedback) is allowed outside the block, named in the test with its reason.

**Fixed under the rewritten gate** (discovered work): the existing
`report.html` rise (:235–241) and `flows.html` breathe (:128–129) move inside
`no-preference`.

### 1. Hire reads without side effects

- **`agents::snapshot(home) -> Vec<HireView>`** (new):
  - reads each run directory's `spec.json` and `state.json` through
    `read_raw`, plus `events::tail_summary`;
  - never reconciles, signals or writes;
  - its doc comment says so, and a test asserts the run directory's files
    and mtimes are unchanged after a call.
- **`agents::phase(&State, &Summary) -> Phase`** (new) is the one mapping
  from state to the fleet's phases: starting, working, needs you, paused,
  done, failed, lost, timed out, stopped.
  - `agents_list` rows gain `phase` and `done_today`, the latter computed in
    the business profile's timezone.
  - `kit.js` `fleetRow` reads `phase` and drops its own mapping and its
    local-date `isToday`.
- **`HireView`** holds:
  - `id`, `title`, `role`, `identity`, `phase`;
  - `active_ms`, `paused_now`, `started`, `ended`, `done_today`, `deadline_secs`;
  - `word`, `msg`, `source`.
- **`report::data`** uses `snapshot` instead of `agents::list`:
  - `herdr_waiting` becomes the `phase == needs you` subset of the new
    `hires` field;
  - `digest::build` keeps its input, now derived from `hires`.
- **Lost runs.** The desktop's follower thread (`agents::tick`) and
  `cos agents watch` still reconcile. A run lost while neither runs is shown
  as last stored until one does, and the report says "as last read".

### 2. Flows: lanes, rail, ring, beads, scene

**Data.**
- `flows::scan` gains `home: Option<&Path>`, passed by its callers. With a
  home, each `Flow` gains `hires`: every `HireView` from `snapshot(home)`
  whose `source == format!("bead:{}/{}", flow.repo, bead.id)` for a bead of
  this flow.
- `ritual.rs:574` and `main.rs:760` pass the business home; the CLI
  `cos flows` passes it when it resolves a business.
- The scan keeps its single deadline. `snapshot` is a directory read with no
  child process.

**Lanes** (new Flow-view tab, from `run_process_lanes`).

*Layout:*
- **Lanes:** one per hire working a bead of this flow (phase working, needs
  you or paused), then one per distinct `owner`, then "Not yet claimed".
  - A bead with a working hire sits in that hire's lane.
  - Otherwise it sits in its owner's lane, or in "Not yet claimed".
- **Columns:** `layer`.
- **Connectors:** one per `blocked_by` edge, in one `aria-hidden` SVG layer
  sized with a `viewBox` in column and row units. Each box names what it
  waits on in its accessible name.
- **Gate:** at Approve, a diamond sits before column 0. Its button posts the
  existing `cos-flows-gate` message.

*States:*

| Bead or flow state | Box |
|---|---|
| bead done | settles with a tick |
| bead in progress | shimmers |
| bead ready | solid |
| bead blocked | dashed |
| connector into an open blocker | dashed |
| gate | breathes, with a `kit-flow` light running along its connector |
| flow drift | the rail's current step flashes; the beads are unchanged |

*Keyboard:* Tab enters the lanes; arrow keys move by lane and column; Home
and End go to a lane's ends; Enter opens the existing bead drawer.

*Narrow:* below 40rem the lanes stack as one list per lane.

**Steps** (rv2-kit `step_progress_list`, ported as it is; the owner chose it
on 2026-11-27 over a hand-drawn rail).
- The ritual's steps, Think → Reflect, as rv2-kit's step progress list:
  - **Marks:** each step has its state mark: ✓ finished, ◌ in progress,
    ◇ awaiting approval, ✕ errored (drift), ○ queued.
  - **Pills:** each step has its state pill, and no times.
  - **Beads:** the current step unfolds its beads, folding to 5 of more than
    6.
  - **Live region:** one polite live region names the step that needs you.
- **Window:** the window's `kit.js` `steps()` restores the #57 port of this
  component (class `kit-steps`), replacing this branch's `kit-rail`.
- **Flows page:** `flows.html` renders the same markup in a side column of
  the Flow view, beside the Graph, Beads and Lanes tabs. Below 64rem the
  column moves above the tabs.
- **Fidelity:** its DOM, marks, spacing and states match the rv2-kit gallery
  rendering of `StepProgressList`. US-016 checks them side by side, light
  and dark.
- **Kit sheet:** `flows.html` carries `design::KIT_SLOT`.

**Ring** (from `expandable_work_item_card`).
- Each Board card gains an SVG ring of arcs, one per bead, coloured by
  state. The arcs draw in once.
- The ring is `aria-hidden`. The card's button gains the counts in its
  accessible name ("5 of 9 beads done, 1 in progress, 3 blocked"), and the
  done count shows as text in the ring's centre.

**Beads** (new Flow-view tab, from `kanban_task_board`).
- Columns: ready, in progress, blocked, done.
- Each card carries its owner or hire mark.
- A rescan (the page's Refresh, or the window's rescan message) re-renders
  through `glide`, so a bead that changed state moves to its new column.
- No drag: the page is read-only.

**Scene** (from `task_branch_scene`).
- An inline, `aria-hidden` SVG: an epic node branching into bead nodes and
  two role chips, drawn with `kit-draw` and `kit-settle`.
- It heads the Board when a flow waits on the user, and is the art of the
  page's "no flows" state.
- Labels are hidden below 32rem.

### 3. Today: hero, hires inbox, day timeline, KPIs

**Living hero** (`today::hero`).
- **Floor:** two `aria-hidden` SVG layers of blurred shapes from
  `--glow-1`/`--glow-2`, drifting at different speeds behind the ink band.
- **Headline:** the server renders `<h1 aria-label="<full text>">`. Inside
  it, each word is an `aria-hidden` span for the reveal (blur `0.25rem` and
  rise, staggered).
  - The lead phrase carries one sweep from `--gradient-from` to
    `--gradient-to`, then holds. It is the text up to the first `;`, `:` or
    `,`, capped at eight words.
  - Under reduced motion the words are plain and the gradient is static.
- **Hire pills:** one per `hires` entry with phase working, needs you or
  paused. Each holds:
  - the title and the role's identity mark;
  - the Herdr word and the active time;
  - a thin bar of `active_ms` against the hire's deadline, labelled "of its
    deadline".

  The bar shimmers while working and breathes while it needs the user. Only
  the pill's word line is a polite live region.
- **Brief loader:** when `cos-brief-state` says `state: "running"`, the run
  pill area shows the `thinking_loader` rays and the message's `text` with a
  sweeping band.
  - `state: "done"` turns it into a tick and "Brief ready" before the
    report reloads, as now.
  - `state: "failed"` shows the message's text as a fixed sentence.
  - No new message type.

**Hires inbox** (`today::hires`, replacing `herdr_waiting`, from
`background_task_inbox`).
- The server renders three columns, Needs you, Running and Done today, from
  `data.hires`. Below 45rem it is one list with filter tabs.
- **Live updates in the window:** on `agents-changed` the window calls a
  new read-only Tauri command, `agents_snapshot(business) -> Vec<HireView>`.
  - It calls `agents::snapshot` and nothing else: no `agents::list`, no
    reconcile.
  - The window then posts `{type: "cos-hires", hires: [...]}` to the report
    frame.
  - This is its own message type, so the report's `cos-host` handler, which
    sets the Hire buttons and Run brief (`report.html:521–525`), is
    untouched.
  - The fleet keeps using `agents_list`, whose reconciling is unchanged and
    out of scope.
  - The report's script re-renders the inbox and the hero pills in place
    with `glide`, so a card moves to its new column. This needs no storage.
  - Outside the window (`cos dash`), the inbox is the server's render.
- Open in Herdr and Open run keep their existing messages. The run id
  travels only in those messages, never in visible text.
- **No hire:** the section is not rendered, as `herdr_waiting` is not today.

**Day timeline** (rv2-kit `agent_schedule_board`, ported as it is; the owner
asked on 2026-11-27 for an existing component rather than a rework of the
ribbon).
- Today, as rv2-kit's schedule board: a timeline centred on now (±5 h), with
  past markers carrying their outcome in words and upcoming markers to the
  right.
  - **Markers:** today's calendar slots and the hire runs that started
    today.
  - **Outcomes:** done, needs you or failed, from `hires`.
  - **Trace:** the activity trace SVG.
- **Dropped:** the board's Run now, skip, restore and pause controls, since
  Today is read-only. Everything else, including keyboard order, live region
  and no horizontal scroll at 20rem, matches the gallery.
- **Week:** the existing week ribbon stays below, unchanged.
- **Fidelity:** the port matches the rv2-kit gallery rendering of
  `AgentScheduleBoard`. US-011 checks them side by side, light and dark.

**KPIs.**
- On first render, a tile whose KPI has a previous point rolls its digits
  from that value to the current one. Without one, it shows the value
  statically. Later renders do not roll.
- The full value is plain text with `aria-live="polite"`; the rolling digits
  are `aria-hidden`.

**Report script gate.** The inbox, pills, timeline and ticker client code in
`report.html` lives in one `/* kit */ … /* end kit */` block that builds with
DOM methods. A new gate, `report_kit_block_never_writes_html_from_data`, fails
any HTML-writing call inside that block. The page's existing `esc()`
templates are unchanged.

### 4. Shared: crossfades and shimmer

**Crossfade.**
- `kit.js` `swap(container, next)` (from `blur_swap_container`) crossfades
  with a small blur and glides the height. The outgoing layer is `inert`.
- It is used for window pane changes and the run page's loading → ready.
- `flows.html` carries the same function inline for its view changes.

**Shimmer.**
- Region-level `stateView("reading" | "refreshing" | "starting")` keeps its
  icon and art, and gains three shimmer skeleton rows under the headline
  (from `shimmer_loader`). The fill is `--muted` with a `--background` band.
- No new state kind is added, so the icon-and-art gate is unchanged.

**Copies.** `motionOk`, `glide` and `swap` exist in three documents. A test
asserts their bodies are identical.

## Alternatives

- **Vendoring kit CSS:** rejected (two token systems, px, motion on by
  default).
- **Lanes on the GPU map engine:** text, focus and ARIA would have to be
  rebuilt.
- **Glide across report reloads through storage:** impossible in the
  opaque-origin frame. Live updates by message keep the glide with no storage.
- **Keeping `agents::list` in the report:** rendering would keep writing
  state and signalling processes.

## Security and privacy

- **Reads:** hire spec and state, and event summaries, through `snapshot`.
  It is a pure read, and replaces a reconciling read on the render path.
- **Writes:** none new.
- **Messages:** the window posts `hires` to the report frame with
  `postMessage(…, "*")`, as it already does with `cos-host` and
  `cos-brief-state`. The payload is the same data the report already renders,
  and the frame's sandbox is unchanged.
- **Rendering:**
  - the window builds through `el()`;
  - the Flows page through `h()`;
  - the report's new code through DOM methods inside its gated block;
  - the server escapes through `esc`/`prose`;
  - hire titles and Herdr messages are text.

## Testing

**Unit:**
- **`snapshot`:** leaves files and mtimes unchanged; matches `list` for
  well-formed runs.
- **`phase`:** every status and word combination, and `done_today` in the
  profile's timezone.
- **`flows::scan`** with a home: hires matched by
  `bead:{flow.repo}/{bead.id}`; lane assignment order; ring counts.
- **Today:**
  - inbox columns from phases;
  - timeline slots, outcomes and blips against a fixed now;
  - the lead phrase split;
  - no run id in visible text;
  - a hire title with markup renders escaped;
  - `herdr_waiting` and the digest unchanged in meaning.
- **Copies:** the `motionOk`, `glide` and `swap` bodies are identical.

**Design gates:**
- the rewritten `every_animation_respects_reduced_motion`, with its own
  fixtures in `each_rule_catches_its_violation`:
  - an `animation` outside the block;
  - a `transition:` with no space after the colon;
  - an `.animate(` outside a `motionOk()` check;
  - an unused `@keyframes`;
- `report_kit_block_never_writes_html_from_data`;
- the new tokens, light and dark, with the gradient stops at AA on `--hero`.

**E2E (`tests/kit_page.rs`, `tests/flows_page.rs`):**
- **Lanes:** placement, arrow keys, Enter opens the drawer, the gate posts
  `cos-flows-gate`.
- **Rail:** `aria-current`.
- **Ring:** the label matches the counts.
- **Beads:** a rescan moves a bead's column.
- **Today:**
  - a `cos-brief-state` running → done sequence shows the loader, then
    "Brief ready";
  - a `cos-host` `hires` message moves a card between inbox columns;
  - the KPI value text equals the record.
- **Reduced motion:** with `reduce` emulated, `document.getAnimations()` is
  empty on every page once settled.

**WCAG:** `examples/wcag.rs`, light and dark, at 0 violation nodes.

**Seen:** `task screenshots` gains the Lanes, Beads and Board-with-rings
views and the Today hero with pills, the inbox and the timeline, light and
dark. `examples/demo.rs` gains a flow at the gate with owned, hired and
blocked beads, and hires sourced from its beads.

## Confidence

| Piece | Tier |
|---|---|
| tokens, grammar, rewritten gate | rock |
| snapshot and phase | rock |
| rail | rock |
| ring | rock |
| scene | rock |
| beads + glide | rock |
| hero | rock |
| inbox with live updates | rock |
| KPI ticker | rock |
| crossfade and shimmer | rock |
| day timeline | sand: how overlap outcomes read on a busy day; the demo screenshot settles it |
| lanes geometry | sand: connector routing past 12 columns; one 30-bead flow seen makes it rock |

## Acceptance

1. **Lanes.** On the demo home, a flow at Approve shows lanes per hire and
   owner, beads in dependency columns with connectors, and a breathing gate
   diamond. Arrow keys move between beads; Enter opens one.
2. **Ring.** Board cards show rings whose labels match the flow's counts.
3. **Beads.** Columns match bead states, and a rescan that finishes a bead
   glides it to Done.
4. **Today hero.** It shows the floor, the revealed headline and one pill
   per working hire. A brief run shows the loader, then "Brief ready".
5. **Hires inbox.** It shows Needs you, Running and Done today. A hire's
   phase change in the stand-in Herdr moves its card without a page reload.
   No run id is visible.
6. **Day timeline.** Today's lane places slots around a named now line,
   with hire runs as blips and outcomes in words.
7. **KPI tiles.** A tile with a previous point rolls to the recorded value.
8. **Reduced motion.** With reduced motion on, no page has a running
   animation.
9. **No new writes.** A new test, `tests/read_only_render.rs`
   `rendering_today_flows_and_the_hire_snapshot_writes_nothing`, walks the demo
   home and records every file's path, length and mtime. It then calls:

   - `report::data` and `today::hero` / `today::body`;
   - `flows::scan` with the home;
   - `agents::snapshot`.

   It walks again and asserts the two listings are equal.
10. **Gates.** `task test` and `task test-e2e` pass, and the WCAG evaluator
    reports 0 violation nodes in light and dark.

## Review history

- Round 1: [review-a](../archive/plans/2026-11-26-flows-today-motion-design-review-r1-a.md) (1
  blocking, 10 should, 6 nit) and
  [review-b](../archive/plans/2026-11-26-flows-today-motion-design-review-r1-b.md) (1 blocking, 8
  should, 3 nit), against sha256 `25020671…`. Closed in revision 2:
  - `agents::list` reconciles on render, so the change adds `snapshot` and
    `phase`;
  - storage is impossible in an opaque-origin frame, so the inbox updates by
    live message instead;
  - the reduced-motion gate is rewritten;
  - plus scan plumbing, the brief-state message, ribbon reuse, drift scope,
    `color-mix` syntax, shimmer as a variant, the report script gate,
    gradient contrast, the kit sheet in `flows.html`, and the ARIA fixes.
- Round 2: [r2-a](../archive/plans/2026-11-26-flows-today-motion-design-review-r2-a.md)
  (2 blocking) and [r2-b](../archive/plans/2026-11-26-flows-today-motion-design-review-r2-b.md)
  (1 blocking), against sha256 `5b3e5e99…`. Closed in revision 3:
  - live hires come from a read-only `agents_snapshot` command, not
    `agents_list`;
  - they travel in their own `cos-hires` message, so `cos-host` state is
    untouched;
  - the no-writes acceptance names its test, `tests/read_only_render.rs`.
- Round 3: both clean against sha256 `e69ea6e8…`; approved by the owner.
- 2026-11-27, owner decision after seeing the built rail: the hand-drawn rail
  is replaced by rv2-kit's `step_progress_list`, ported as it is, and the day
  timeline is rv2-kit's `agent_schedule_board`, ported as it is. Both are
  checked side by side with the rv2-kit gallery.

## Carried to Plan

- **Callers that pass the home:** every `flows::scan`/`try_scan` caller
  passes the home, including the desktop's `Pages` scan (`flows.rs:870`,
  :885, `apps/desktop/src/lib.rs:862`, :907) and the tests (`tests/flows.rs`
  :281, :377). Where only a workspace is known, the home is the business's
  resolved home.
- **`report::data` callees:** before the no-writes test lands, read each one
  to confirm it is write-free: `history::load`, `history::backfilled`,
  `kpis::by_goal`, `delivery::view`, `constraints::view`.
- **Report script gate scope:** the `/* kit */` block gate covers the new
  code only. The page's existing `esc()` templates are out of scope and stay
  as they are.
- **Gradient contrast:** measure the gradient stop values against `--hero`
  when the tokens land.
- **Timeline clock:** the timeline takes "now" from the same `Ctx` clock the
  ribbon uses, so tests can fix it.
- **"As last read":** shown on the inbox heading when the window's follower
  is not running (no `agents-changed` within 10 s of load).
- **Ready and blocked:** a bead's column in Beads follows its `status`,
  `ready` and `open_blockers`:
  - in progress when its status is in progress;
  - ready when `ready`;
  - blocked otherwise, until done.
- **KPI roll:** the previous value is the second-newest point by
  `evidenceAt`.
- **`el()`:** it is `const el` in `states.js:36`.
