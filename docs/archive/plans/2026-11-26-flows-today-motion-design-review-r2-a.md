---
design: docs/plans/2026-11-26-flows-today-motion-design.md
design-sha256: superseded (read 5b3e5e99f7a37e4aab99a6bc72ef2c88b023f854f6f324595cdc339e3b65349b)
verdict: findings
---

## Round 1 findings

1. **Open, blocking (now extended).** Hire reads through `agents::list`. Design §1 now adds `agents::snapshot` for `report::data`, which closes the report path. The window's live path is not closed: §3 says the window posts `hires` "using HireView from the same snapshot through `agents_list`". `agents_list` is `apps/desktop/src/lib.rs:1101`, which calls `agents::list` and so reconciles. See New finding N1.
2. **Partly open, carried.** `flows::scan` home parameter is named for `ritual.rs:574` and `main.rs:760`. The desktop callers are not named: `Pages::scan` and `Pages::try_scan` at `flows.rs:870`/`:885`, reached from `apps/desktop/src/lib.rs:862` and `:907`. `Pages::scan` already takes `ws`, so the plan should say where `home` comes from there.
3. **Closed.** `herdr_waiting` becomes the `needs you` subset of `hires`; `digest::build` input is derived from it (§1 "report::data"). Current call at `src/report.rs:51-60`.
4. **Closed.** §0 "Fixed under the rewritten gate" moves `report.html` rise (:235-241) and `flows.html` breathe (:128-129) inside `no-preference`.
5. **Closed.** §0 gate rewrite covers `style.css`, `kit.css` and the three `<style>` blocks; matches with or without a space after the colon; covers `transition`.
6. **Closed.** §0 gate bullet 2 forbids `.animate(` outside a `motionOk()` function in `apps/desktop/ui/*.js`, `report.html`, `flows.html`.
7. **Closed.** No `sessionStorage` in the design. Glide is in-memory FLIP; live updates arrive by `postMessage` (§3). Acceptance 5 is the live-update check. The message itself is wrong; see N2.
8. **Closed.** §1 `agents::phase` in Rust is the one mapping; `kit.js` `fleetRow` reads `phase` and drops its own map.
9. **Partly open, carried.** The new report code is gated by `report_kit_block_never_writes_html_from_data` inside a `/* kit */ … /* end kit */` block (§3). The existing `innerHTML` sites (`report.html:648`, `:689`, `:735`, `:807` as cited in r1) are left unchanged and ungated. That is a stated scope choice, not an error. The gate matches a comment delimiter, not a function, so it can be bypassed by moving code outside the markers. Carried to Plan.
10. **Closed.** Each gradient stop is added to the contrast test as text on `--hero` (§0 Tokens). The test is not yet written, so it is a commitment, not a measurement.
11. **Closed.** §2 Rail: `flows.html` carries `design::KIT_SLOT`; gate `every_page_that_uses_kit_classes_includes_the_kit_sheet` named.
12. **Closed.** §0 keyframes table adds `kit-flow` with `stroke-dasharray: 0.08 0.12`; §2 Ring names `kit-draw` on `pathLength="1"`.
13. **Closed.** §1 `done_today` is computed in the profile timezone (`agents_list` rows gain `done_today`).
14. **Closed.** §Context cites `agents/mod.rs:569` as where the request `source` is stored, and `lib.rs:1763` as where `bead:{repo}/{id}` is built. Verified: `apps/desktop/src/lib.rs:1763` builds the string; `mod.rs:569` is `source: req.source`.
15. **Open, nit, carried.** §Security still says "the window builds through `el()`" with no owner. `el` is `const el` in `apps/desktop/ui/states.js:36`, not a shared kit helper. Name it or add a `kit.js` helper.
16. **Closed.** §3 Hero: each word is `aria-hidden`, `<h1 aria-label="<full text>">`; now line has the name "Now, 14:05".

## New findings

**N1. Blocking. Live hire updates reconcile.** Design §3 "Live updates in the window" says the window posts `{type: "cos-host", hires}` "using `HireView` from the same snapshot through `agents_list`". `agents_list` (`apps/desktop/src/lib.rs:1101`) calls `agents::list`, which calls `read` → `reconcile` (`src/agents/mod.rs:820-829`, `:794-797`, `:949-977`). Reconcile writes `state.json` and signals or kills process groups on the `agents-changed` path (`app.js:1639`). That contradicts §1 "never reconciles, signals or writes" and Acceptance 9. Fix: name a new read-only command (for example `agents_snapshot`) that calls `agents::snapshot`, and say that `agents_list` keeps its current behaviour.

**N2. Blocking. The posted message resets the report's host state.** Design §3 posts `{type: "cos-host", hires: [...]}`. The existing `cos-host` handler at `src/report.html:521-525` sets `canHire = Boolean(d.hire)` on every `cos-host` message. It also relabels `[data-hire]` and `[data-herdr-open]` buttons and sets `#host-run` hidden unless `d.runBrief === true`. So each `agents-changed` would flip the Hire buttons to "Copy prompt" and hide "Run brief". Fix: use a new type (for example `cos-hires`), or have the window send `hire` and `runBrief` with every `cos-host` post. Pick one in the design and add a test that a `cos-host` with only `hires` leaves the host state unchanged.

**N3. Not blocking, carried.** Acceptance 9 ("no file under the business home changed after rendering Today and Flows") depends on `report::data` callees `history::load`, `history::backfilled`, `kpis::by_goal`, `delivery::view` and `constraints::view` being write-free on render. I did not read them. Carried to Plan as a check to run before the gate is written.

## Carried to Plan

- Round 1 item 2: name the desktop `Pages::scan`/`try_scan` callers and where `home` comes from (`flows.rs:870`, `:885`; `apps/desktop/src/lib.rs:862`, `:907`).
- Round 1 item 9: the `/* kit */` marker gate is bypassable; gate on function scope or on the whole `report.html` script, or state the residual risk.
- Round 1 item 15: name the owner of `el()` in the window.
- N3: verify the `report::data` callees are write-free before writing Acceptance 9's e2e.
- `scan` callers: `Pages::scan(ws, …)` has `ws`; state how `home` is derived from it.
- Contrast gate for gradient stops is a commitment; state the measured ratios once computed.
