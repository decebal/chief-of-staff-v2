# Flows and Today, in motion: brief

Date: 2026-11-26.

## Problem

The Flows page (`src/flows.html`) and the Today report (`src/today.rs` into
`src/report.html`) are the two pages the user opens most. Both show state as
static cards and lists.

**Flows:**
- A flow's beads sit in a graph of cards with no sense of who holds each one.
- A step change, a bead finishing or a gate opening looks the same as nothing
  happening.

**Today:**
- The hero is a still ink band.
- Hires working now appear only when they wait on the user.
- The calendar ribbon is a static week.
- KPI values and section changes jump.

PR #57 ported nine rv2-kit components; rv2-kit has since lifted 149 more to
parity (rust-v2 `7ff3ec5`).

## Goal

**Flows:**
- **Process lanes:** a flow reads as lanes, one per person or hire. Each bead
  sits in its owner's lane in dependency order, with connectors drawing in, and
  the approval gate breathes with a light running into it.
- **Checkpoint rail:** the ritual's steps run down a rail that draws to the
  current step.
- **Board:** each flow card on the board carries a bead ring.
- **Bead board:** a column view whose beads glide between columns when their
  state changes.
- **Scene:** an epic splitting into beads and hires heads the page and fills
  its empty state.

**Today:**
- **Hero:** lit by a slow glow floor, with the headline revealed and
  gradient-swept, and a progress pill for each hire working now. A loader
  shows while a brief runs.
- **Hires inbox:** hires sit in needs-you, running and done columns, gliding
  as they move.
- **Day timeline:** the calendar becomes a day around a now line: past slots
  with outcomes, upcoming slots, and hire runs as blips.
- **KPIs:** values tick.

**Both surfaces:** loading uses a shimmer, and changes crossfade.

Motion level, chosen by the user: showcase, ambient motion included. Every
animation is off under `prefers-reduced-motion`.

## Approaches

1. **Native port onto `tokens.css` and `kit.css`, as in #57 (chosen).** The
   catalogue (2026-11-26) found that each kit component:
   - animates by default and turns motion off under `reduce`;
   - uses px throughout;
   - calls `document::eval` in places.

   A port therefore rewrites three things:
   - the CSS, against tokens and rem, with every animation inside
     `prefers-reduced-motion: no-preference`;
   - the DOM, through `el()` in the window and `h()` in `flows.html`;
   - the `document::eval` calls, as plain listeners.

   This is the only route through `tests/design.rs`.
2. **Vendoring kit CSS.** Rejected for the reasons recorded in the #57 brief:
   a second token system, px and raw colours, and motion that defaults on.
3. **A canvas or WebGPU scene for the lanes.** `flows.html` already has a GPU
   map with an SVG fallback, but a lane diagram needs readable text, focus
   order and ARIA. DOM plus SVG connectors keeps all three.

TypeSafe Jev rated the ritual tier `lean act confidence=0.88`: no new writes,
one repository, reversible.

## Constraints

- **Read-only:**
  - no new file, network or data write;
  - the brains stay read-only;
  - no spawn on a render or poll path.
- **Every number from records:**
  - lanes from bead owners and `blocked_by`;
  - rings from bead states;
  - the timeline from calendar slots and hire timestamps;
  - pills from hire events.
- **Gates:**
  - `tests/design.rs`: tokens only, no px above 2, animations only under
    `no-preference`, every state through `stateView`;
  - `window_never_writes_html_from_data` and
    `flows_page_never_writes_html_from_data`;
  - `every_page_that_uses_kit_classes_includes_the_kit_sheet`.
- **Report and surfaces:**
  - `tests/today.rs`: actions first, dates in words, escaped text;
  - the report is also served by `cos dash`.
- **Flows page:** stays a CSP page, a frame with an opaque origin, and keeps
  its GPU map with the SVG fallback.
- **Accessibility:**
  - each component keeps the kit's keyboard model and live regions;
  - the WCAG evaluator stays at 0 violation nodes, light and dark.
- **Screenshots:** from the demo home only.

## Unknowns

- **Lane assignment:** a bead's lane is its owner (`claimed_by`), and
  unowned beads go to a "not yet claimed" lane. Hires that work a bead are not
  linked to it in data today. Sand: reading `src/flows.rs` and the hire spec
  `source` makes it rock or names the gap.
- **Hire runs on the timeline:** start and end come from `state.json` and hire
  events. Rock.
- **Brief-running state in the report:** the window knows it from
  `brief_run`; the report's client script must be told. Rock: the window
  already posts `cos-host` messages to the report frame.
- **Process lanes geometry:** the kit lays out in a px coordinate space. The
  port lays out in rem units and percentages with SVG `viewBox` connectors.
  Sand until one flow with real beads renders.
- **Glide (FLIP) for bead columns and hire columns:** small client JS,
  reduced-motion gated. Rock.
