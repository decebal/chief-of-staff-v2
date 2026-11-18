# WCAG measurement

`examples/wcag.rs` scores the desktop window and the pages it frames with
axe-core against the WCAG 2.0, 2.1 and 2.2 A/AA tags. It is an
[autoresearch](https://github.com/wolven-tech/autoresearch-rs) JSONL v1
evaluator, so it can drive an improvement loop, and runs by hand as well.

## Result, 2026-11-17

20 states, light and dark, axe-core 4.13.0:

| | before | after |
|---|---|---|
| violation nodes | 42 | **0** |
| rules failing | 6 | 0 |
| colour-contrast nodes axe could not decide | 1,859 | 1,857 |

Both figures are repeat runs that matched exactly. The fixes:

- the report's tooltip is `hidden` while not shown, so an empty
  `role=tooltip` is not exposed (aria-tooltip-name);
- report charts carry a name that points to their table view, and the
  calendar's "now" marker is an image with its time (svg-img-alt,
  aria-prohibited-attr);
- unselected Agents sub-tabs use `--muted-foreground`; Basecoat's
  foreground-at-60% was 4.48:1 on `--muted` (color-contrast);
- the docs room's Style chips pick whichever ink has the higher contrast on
  the chip's fill; a fixed luminance cut gave mid-tone fills white text under
  4.5:1 (color-contrast);
- the hire dialog's work selects are named (select-name).

2026-11-18, with the Flows page, the state views and the settings gear: 24 states, light and
dark, 0 violation nodes and 2,223 colour-contrast nodes axe could not decide, in two runs that
matched exactly. The four Flows states (board, a flow with a bead's drawer open, the Map, the
palette) add 166 of the undecided nodes, 140 of them the Map's SVG labels.

The undecided nodes are mostly text inside SVG (the docs graph's labels,
which carry a `--background` halo), glyphs axe skips by design, and content
behind an open modal. axe decides part of WCAG: keyboard order, reflow at
320 px and what a screen reader is told when a view changes are not in this
number.

## What it measures

1. Copies the candidate tree into the artifact directory and builds `demo`
   and `preview` there (tauri-build writes `apps/desktop/gen/`, and the tree
   under test must stay untouched).
2. Writes the demo home, serves it and an empty home through `preview`.
3. Opens each state in its own headless Chrome per theme, with the colour
   scheme and reduced motion emulated before the page loads; switching the
   scheme under a rendered page leaves some `light-dark()` colours stale.

States: Today, Agents (staff, runs, scorecard, a run, the hire dialog), the
business switcher, the settings gear, Flows, Docs and every dashboard the
window lists; Today and runs over an empty home; the report's tabs; the Flows
page's board, a flow with a bead's drawer open, the Map and the palette; the
docs room's bets, a topic, the reader and focus mode. The frames are opaque-origin
sandboxes axe cannot enter, so their pages are audited top-level.

Chrome runs with `--use-mock-keychain --password-store=basic` and a scratch
HOME holding an empty `Library/Keychains`: without them a fresh profile asks
macOS for the login keychain and raises a dialog per launch. `Math.random` is
seeded in every page so the docs graph lays out the same way each run.

## Running it

```sh
cargo build --example wcag
target/debug/examples/wcag --cargo-home ~/.cargo --rustup-home ~/.rustup \
  --target-dir <a warm build dir> --chrome <Chrome binary> --axe <axe.min.js> < request.json
```

`request.json` is one autoresearch request line; `candidate_worktree` is a
checkout without `target/` and `artifact_directory` is where the build, the
homes, the logs and `wcag-<commit>.json` (every finding, by state and theme)
go. A run takes about a minute with a warm build directory.
