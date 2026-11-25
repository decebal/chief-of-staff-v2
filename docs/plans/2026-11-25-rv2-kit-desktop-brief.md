# rv2-kit in the desktop app: brief

Date: 2026-11-25.

## Problem

The desktop window shows agent runs, the daily brief and ritual flows as tables,
key-value lists and cards built one at a time in `apps/desktop/ui/app.js` and
`src/today.rs`. The questions a user brings to each surface are not answered at a
glance:

- **Agents:** which hires are working, which need me, how long each has run, and
  what happened during a run. Today the run page lists `started`, `ended`,
  `judged on` and the last Herdr word (`app.js` `renderRun`); nothing shows the
  sequence of a run, and time spent paused counts as working time.
- **Today:** what changed since I last looked. The brief lists actions and
  changes, but the user cannot see which decisions, failures and finished work
  arrived since the previous brief, and KPIs show only a current value against
  target with no direction.
- **Flows:** where a flow is and what waits on me. The Flows tab draws approval
  cards (`app.js` `gates`, :1537), but not the step sequence or a stack of the
  decisions in front of the user.

rv2-kit (`rust-v2/crates/rv2-kit`) already specifies these surfaces as
clean-room components with defined states, ARIA and keyboard behaviour:
`worker_fleet_panel`, `single_run_status_card`, `run_timeline_rows`,
`overnight_run_digest` / `absence_digest`, `metric_tile`, `metric_trend_card`,
`step_progress_list` and `pending_decision_stack`.

## Goal

The Agents tab shows the hires as a fleet, each with an identity colour, state,
elapsed time excluding pauses, and what it needs. A run page opens on a status
card and a timeline of what happened, in order. Today opens with what arrived
since the previous brief, sorted into decisions, errors and done, and KPI tiles
show direction over time. The Flows tab shows each flow's steps and stacks the
approvals waiting on the user, with Approve still going through
`ritual_approve`.

## Approaches

1. **Native port (chosen).** Rebuild each component's DOM, ARIA and keyboard
   behaviour in cos: `app.js` DOM builders for the window and `format!` HTML in
   Rust for the report. Write new CSS against `apps/desktop/ui/tokens.css`, and
   add job-named tokens (identity colours, light and dark, AA-checked) to it. This
   is the only route that passes `tests/design.rs`, which forbids raw colours,
   pixel lengths above 2px, type sizes outside `--text-*` and custom properties
   outside the token file. Decided by TypeSafe Jev: `native-port act-and-report
   confidence=0.76 · runner-up vendor-kit p=0.15`.
2. **Vendor kit.css under `ui/vendor/`.** This escapes the gate the way Basecoat
   does, but brings a second token system (`--kit-*`), dark mode by
   `[data-theme]` only, rgb shadows and px sizes into served pages that must
   inline every style. It lost because two token systems is the drift the gate
   exists to stop.
3. **dioxus-ssr.** Rendering the kit to HTML strings adds Dioxus to the `cos`
   lib, and drops every interaction, since the kit's behaviour lives in Dioxus
   signals and `document::eval`. It lost because the window needs the
   interactions.

**History.** The surfaces need data cos does not keep:

- a per-hire event log, since `state.json` is rewritten in place;
- paused time;
- KPI history.

The flow adds append-only history rather than rendering only what exists.
Decided by Jev: `add-history act-and-report confidence=0.50 · runner-up
existing-only p=0.25`. That adds local data writes, so the flow runs at tier
`full`.

## Constraints

- **The brains stay read-only.** New history is written by `cos` (the follower,
  `state::commit`), never by a brain.
- **Every child is owned and reaped.** No new process spawns on a render or
  poll path.
- **Every number comes from facts:**
  - elapsed, paused time and timeline from recorded timestamps;
  - KPI trends from recorded snapshots;
  - nothing inferred by a model.
- **Design gates:**
  - `tests/design.rs`: tokens only; every state through `stateView`; every
    animation under `prefers-reduced-motion: no-preference`;
  - AA contrast in light and dark;
  - `flows.html` never writes HTML from data.
- **Report gates:** `tests/today.rs` (actions first, no raw URLs, dates in
  words, markup escaped).
- **Compatibility:**
  - old `state.json` files without history still render;
  - a hire from before the event log shows the timestamps it has;
  - the ledger stays append-only.
- **Screenshots only from the demo home.** `examples/demo.rs` gains fixture
  history.

## Unknowns

- **Per-hire event log shape and writer** (`follow.rs`): sand. Which transitions
  the follower sees reliably (word changes are polled every 2 s, pause and say
  pass through `agents_pause` / `agents_say`). Reading `follow.rs` and the
  Tauri commands end to end makes it rock.
- **Paused time:** sand. Pause is `hc interrupt` and the hire keeps status
  `running`. A paused interval needs a start (pause) and an end (say, resume or
  a working word). Settling which signal ends a pause makes it rock.
- **KPI snapshots:** sand. Where to append (journal entry vs a separate
  `kpi_history` in `state.json` vs `out/history/`), and how a kit-written
  `state.json` (`state.py`) keeps the field. Reading `state.rs` validation and
  `state.py` makes it rock.
- **Since-last-brief boundary:** rock. `state.json` `runs[]` holds each brief's
  `finishedAt`, and the ledger rows carry `judged` timestamps.
- **Component ports:** rock. Specs, DOM and states are known (`worker_fleet_panel`,
  `single_run_status_card`, `run_timeline_rows`, `overnight_run_digest`,
  `metric_tile`, `metric_trend_card`, `step_progress_list`,
  `pending_decision_stack`). Known kit defects are not ported:
  - `pending_decision_stack` focus id after decide;
  - its card `data-status` that never changes;
  - `metric_trend_card` series tones with no stroke rule.
