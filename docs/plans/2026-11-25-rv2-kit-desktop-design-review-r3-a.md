---
design: docs/plans/2026-11-25-rv2-kit-desktop-design.md
design-sha256: superseded (read 4867a4706290c3780922fcdbb6f22814ee9167d96240b4453bd821b9aad47e75)
verdict: findings
---

## Round 2 findings

1. open — B1 (boundary statuses). Boundary fixed: design :417-423 uses receipt `status == ok` (`RunStatus`, pipeline.rs:73-77, serde snake_case) with `at` (pipeline.rs:85), then `runs[]` `status == complete` with `finishedAt` (track.rs:100, :102; state.rs:391-396). Still open: Context :94-96 says `runs[]` holds "status, including failed and deferred runs (`RunStatus`, pipeline.rs:73–77)". False (track.rs:102 writes only `complete` or `partial`). Acceptance #4 (:815-818) still says "no earlier ok run" without naming the source.
2. open — B2 (`el()` prop sink). Partly fixed: `el()` throws on `innerHTML`/`outerHTML`/`srcdoc`/string `on*`, and routes `href`/`src` through `safeUrl` (design :175-186). Open: `formAction`, `action`, `poster`, `data` are not in the URL list; the gate does not cover the non-`el()` doors (see New finding 2).
3. closed — Round 1 #3. E2E `el()` test named in design :199-201 and the matrix row :745; carried :831.
4. closed — B3 (now-dependent cache). Cache holds `Totals` only (design :310-317). `active_ms` is computed at read time (:318-321). Test that advances `now` with the file unchanged is at :326.
5. closed — Round 1 #6 read bound. Stated at :327-328 (one `metadata` per run per list, one fold per change, linear in events).
6. closed — Round 1 #11 data contract. `finishedAt` (track.rs:100), `evidenceAt` and `observedAt` (state.rs:41, :355), ledger `Row` (ledger.rs:18-42). `runs[]` status claim: see item 1 (open).

## New findings

1. blocking Context, Today (:93-96): Context still states `runs[]` status includes failed and deferred runs. Evidence: track.rs:102 (`"complete"` or `"partial"`); state.rs:391. Fix: replace with the real set `running | complete | partial | blocked | interrupted` and say only `complete` counts for the fallback. Name the source in Acceptance #4 (:815-818).

2. blocking Design, Semantic ISA `RENDER(window_dom)` (:175-186, :726): "`el()` is the one door every window builder uses" is not enforced. `window_never_writes_html_from_data` names only `innerHTML`, `outerHTML`, `insertAdjacentHTML`, `document.write`, `createContextualFragment`, `DOMParser`, `setAttribute("on…")` and computed `el()` keys (:187-193). Doors outside `el()` that the gate misses: `states.js:38` `n.setAttribute(k, v)` over `Object.entries(attrs)`, so a data-keyed `setAttribute("href", d)` bypasses `safeUrl`. `states.js:172` `frame.src = src`, a direct property write. A builder writing `x.href = d` or `x.action = d` also passes. Evidence: apps/desktop/ui/states.js:38, :172; design :185-186, :726. Fix: extend the gate to fail `setAttribute(` with any name outside a fixed allowlist, and direct `.href|.src|.action|.formAction|.srcdoc|.style|.data|.poster =` assignments in `apps/desktop/ui/*.js` (with `frame.src` and the `attrs` helper either routed through `safeUrl` or listed with their reason). Add fixtures `x.href = d`, `setAttribute("href", d)`, `el("form", {action: d})` to `each_rule_catches_its_violation`. Add `formAction`, `action`, `poster`, `data` to the `safeUrl` list.

3. blocking Design, KPI points (:471-473) vs Alternatives (:549-551): Alternatives reject "Recording KPI points on report render: a write on a read path." But :472-473 wires `record_kpis` into `fold_latest_run`, which `cos report` calls (main.rs:445-446 → main.rs:1076, `history::record` at main.rs:1082). So `cos report` writes KPI points. The claim and the wiring contradict. Fix: either state that `cos report` writes KPI points (dedup makes it idempotent) and delete the rejection, or call `record_kpis` only from the brief path (pipeline.rs:466) and keep `fold_latest_run` out of it. The plan must pick one.

4. blocking Semantic ISA, side-effect inventory (:607-608): the temp-name change is listed only for `state.json`. `write_json` (agents/mod.rs:1063) is also the writer for `spec.json` (mod.rs:568), so `spec.json` gets the same temp-name change. Evidence: src/agents/mod.rs:568, :1059, :1063. Fix: add a row for `spec.json` under `out/agents/<id>/` (changed, reversible, affected: hire spec on record).

## Carried to Plan

- Carried items in design :831-870 are accepted as carried. Two mis-cites need fixing in the plan:
  - `fold_latest_run` (:850-851) says it is called at main.rs:445 and main.rs:1082. Only main.rs:445 calls it (main.rs:1076 is the definition). main.rs:1082 is the `history::record` call inside it.
  - `KPI key is the KPI's name` (:845-847) is carried, but the body still says "kpi id" at :723 and :734. Replace with `kpi name` when the plan is written.
- `frame.src = src` (states.js:172) and `attrs` helper (states.js:38): plan verifies every `src` value is a constant or a `safeUrl` result, and records it.
- `Paused.turn` source (:839-843) and the lost-update lock (:834-838) remain carried; no change.
- `tests/design.rs:1085` `WINDOW_JS` lacks `kit.js` (carried :848). `sources()` already covers `.js` at the top of `ui/` for the new HTML-sink gate.
- Refresh claim (:335-340): "every transition except Said, Paused and FirstMessage coincides with a state.json rename" is not checked for `MarkedDone` (follow.rs:259). Plan verifies it or drops the claim.
- `el()` `style` prop from data (cssText) is not gated; carry as a should.

Counts: round 2: 4 closed (Round 1 #3, #6, #11 with the Context line open under B1, B3), 2 open (B1 Context line, B2 gate gap). New: blocking 4, should 0, nit 0 (rest carried). Verdict: findings.
