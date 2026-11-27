---
design: docs/plans/2026-11-26-flows-today-motion-design.md
design-sha256: superseded (read 25020671396ca88104b8cc0407194a61043515d1cfdeb2ad6b0b4ff879a0f4ae)
verdict: findings
---

1. blocking Data (Design §1 "Data", §Security "Reads"): the hire reads named as directory reads with no writes or spawns are not. `agents::list` calls `read` → `reconcile` per run. `reconcile` writes `state.json` (`src/agents/mod.rs:963`), SIGKILLs a legacy supervisor and its process group (`mod.rs:954-959`), and calls `follow::end` for lost runs (`mod.rs:966-976`). The proposed `flows::scan` hires and `report::data` `hires` both read through it. Today's `report.rs:51` already does this on every render, so the existing report path writes and signals on render. `open_runs` (`mod.rs:918-943`) is the non-reconciling read, but it drops terminal runs, and the inbox's "Done today" needs them. Fix: add a read-only listing of all runs (`read_raw` per id, `mod.rs:801`, plus `events::tail_summary` for active time, `lib.rs:1106`). Use it in `flows::scan` and `report::data`. Switch `report.rs:51` too. Add a test that a list call leaves run mtimes unchanged.

2. should Data: `flows::scan` takes `(projects, cn, budget)` with no home (`src/flows.rs:707`, `:712`). The design says "from `agents::list` in the business's home" but does not thread `home`. Callers are `src/ritual.rs:574` and `src/main.rs:760`, plus the desktop and dash flows paths. Fix: name the signature change and the callers.

3. should Data: `report::data` feeds `herdr_waiting` into `digest::build` (`src/report.rs:52-60`). Replacing `herdr_waiting` with `hires` changes the digest's input, and the design does not mention it. Fix: say what the digest receives after the change.

4. should Motion (§0 "Reduced motion"): existing animations are not under `no-preference`. `src/report.html:235-241` runs `rise` on `.act` and `.today > section` by default, opting out under `reduce`. `src/flows.html:128-129` declares its own `breathe` keyframes, against "none brings its own". Fix: convert both to opt-in and reuse the kit set, or list them as migrations.

5. should Gates (§Testing "Design gates"): `every_animation_respects_reduced_motion` (`tests/design.rs:1452`) reads only `style.css` (`:1453`). Its `animation:` check is shorthand-only and misses `animation-name` and `transition`. It matches the literal `prefers-reduced-motion: no-preference` with a space, so `src/flows.html:128` (no space) would fail once extended. The design also needs transitions longer than `--motion-fast` gated, which no test checks. Fix: normalise whitespace in the gate, add `animation-*` and `transition` checks, and name the extension to `kit.css`, `report.html` and `flows.html`.

6. should Motion (§0 "Glide"): `element.animate` in `glide` and in the flows inline copy is in no CSS or static gate. Only e2e `getAnimations()` covers it, and only if the e2e test runs. Fix: a static gate that each `element.animate` call sits behind a `REDUCED()` or `matchMedia` check, plus the three-copy body match, in `tests/design.rs`.

7. should Window (§2 "Hires inbox", §Acceptance 3 and 5): cards keep last boxes per id in `sessionStorage`, so they glide across reloads. The report iframe is `sandbox="allow-scripts allow-popups"` (`apps/desktop/ui/index.html:196`), an opaque origin, and `app.js:232` says it has no storage. In the desktop app `sessionStorage` throws, and `report.html:1129` swallows it. The glide-across-reloads acceptance is therefore inert in the window. Fix: hand the last positions over through `cos-host` messages, or scope the acceptance to `cos dash`.

8. should Data (§1 "Data" phase): "same phase mapping as the fleet" has no Rust side. The fleet phase is client JS only (`apps/desktop/ui/kit.js:77-79`, `FLEET_PHASE`, `runPhase` at `kit.js:237-240`). `today::hires` and `flows::scan` would need a Rust port. Fix: state the port (or move the mapping into Rust and reuse it), with a test pinning the two.

9. should Accessibility and Rendering (§2 "Hires inbox", "Hero"): `src/report.html` builds client DOM with `innerHTML` (e.g. `:648`, `:689`, `:735`, `:807`). `tests/design.rs:845` covers only `flows.html`, and `:964` only the window. The design's escaping (`esc`/`prose`) is server-side in `today.rs` only. Client-rendered hire titles, the sessionStorage glide, and the now-line need `textContent`, and a gate. Fix: add a `report.html` gate for client code and state the rule.

10. should Contrast (§2 "Hero", §0 tokens): `--gradient-accent` is "AA where they are text". `tests/design.rs:593` checks only solid colour-token pairs. A gradient sweep over the ink band has no check. Fix: state the measured contrast of the gradient stops against the ink band, in light and dark, and gate it.

11. should Pages (§1 "Rail", "Ring"): `steps()` markup parity needs `kit.css` on `flows.html`. The gate `every_page_that_uses_kit_classes_includes_the_kit_sheet` (`tests/design.rs:744`) will fail otherwise, and the design does not say `flows.html` inlines the kit sheet. Fix: state it, and state the CSP impact (none, since it is inline).

12. nit Motion (§0 "Keyframes"): `kit-draw` is one-shot (dashoffset 1→0). The lane "light runs along the connector" needs a looped dash-offset keyframe, which is not in the set. `kit-draw` on `pathLength="1"` also needs `stroke-dasharray: 1`. Fix: add `kit-flow` to the table and state the dasharray.

13. nit Data (§2 "Hires inbox"): "Done today" uses the browser-local `isToday` (`kit.js:90`). The Rust side uses the business day from `today.rs` `Ctx::today` (`today.rs:65`). Fix: say the day comes from the business timezone.

14. nit Data (§1 "Data"): "a hire started from a bead records `spec.source` (`apps/desktop/src/lib.rs:1763`)". Line 1763 only builds the Hire dialog's prep JSON. The stored field is `Spec.source` (`src/agents/mod.rs:100`), set from the request (`mod.rs:569`, `src/agents/hire.rs:36`). Fix: cite the correct path.

15. nit Window (§0 "Glide", §2): "window builds through `el()`". `el` is `const el` in `apps/desktop/ui/states.js:36`, not a shared kit helper. Fix: name which `el`, or state that `kit.js` gets one.

16. nit Accessibility (§2 "Hero"): `text_reveal` splits words into spans and the gradient sweep recolours a lead phrase. The full text stays plain in the DOM (§2), but the split spans need `aria-hidden` or an `aria-label`, and the design should say so. The now-line needs an accessible name too.

Counts: blocking 1, should 10, nit 6.
Blocking: (1) hire reads via `agents::list` write state and signal processes (`src/agents/mod.rs:954-976`). Existing `report.rs:51` already does it.

Checked and correct: flows.rs `Bead` :162, `Flow` :270-289, `FLOWS_SLOT` :940, `page_html_from` :953; flows.html `h()` :268; today.rs `hero` :120, `body` :243, `herdr_waiting` :282, `ribbon` :646 (07:00-20:00 at :647-648); report.rs `data` :36, `agents::list` :51; app.js:242 `cos-host` posts; report.html:521-525 `cos-host` handler; kpis.rs `points` :62-66 (prior KPI point exists); kit.js `decisions`, `steps` present; no `glide`/`swap` yet; tokens `--warning`, `--destructive`, `--muted`, `--ring`, `--identity-N-soft` present; no `--motion-fast`/`--motion-slow`/`--ease-spring`/`--glow-*`/`--gradient-accent` yet (new, as designed); `kit.css` px values only 1px/2px (passes `tests/design.rs:289-308`); `task test-e2e` (Taskfile.yml:38) and `task screenshots` (:155) exist; `examples/wcag.rs`, `examples/demo.rs`, `tests/flows_page.rs`, `tests/kit_page.rs`, `tests/hire_events.rs`, `tests/today.rs` exist; hire `State.started`/`ended` `mod.rs:230-231`, `Spec.deadline_secs` `mod.rs:102`, `Spec.role` `mod.rs:112`; `timeline.rs:131-133` `active_ms`/`paused_now`; MCP does not call `report::data` (only `dash.rs:140` and `apps/desktop/src/lib.rs:818`).

Not checked: whether the three `glide` copies can share a test without a build step; the WCAG evaluator's current baseline; the demo home's hires and beads (needed for the screenshots and the lanes acceptance).
