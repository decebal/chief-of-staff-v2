---
design: docs/plans/2026-11-28-hire-skill-graph-design.md
design-sha256: superseded
verdict: findings
---

## Earlier findings

Round 1 (open items only; the rest closed in r2):
- r1-9: open, carried. Target ids still undefined: `Fact` (design lines 92-94) and `graph.json` hold no node id; §2 Write (lines 157-158) still says "the graph's newest snapshot ids".
- r1-12: open, carried. No plan warning for repo `.claude/skills` (lines 80-83, 183-187).
- r1-14 / r2 new 1: closed. §4 step 1 (lines 217-220) now removes both `skills.tmp` and `skills`; test row at lines 432-433 covers a second `prepare`.
- r1-17: open, carried. Context line 54-55 still states "37 `execution_role` snapshots" unmarked.

Round 2 carried items (all still open, carried):
- r2 carried: edge target ids (above).
- r2 carried: fold test row. §Testing `tests/skills.rs` (lines 420-423) has no "newest fact invalid, older valid, picked from older" case.
- r2 carried: repo `.claude/skills` plan warning (above).
- r2 carried: Prime-down-on-write acceptance row. Acceptance (lines 463-497) has no row for exit 1, URL named, nothing written.
- r2 carried: Context line 54-55 marking (above).
- r2 carried: Acceptance 4 (lines 473-478) does not say the plan digest check runs after `skills_bundled` is written.

## New findings

1. blocking §4 Delivery (lines 212-214, 232-237) with §2 step 4 (lines 178-180) and Acceptance 10 (lines 491-495): hide-only Claude hires get no `skillOverrides`. The bundle step runs only "for `Harness::Claude` with a non-empty `picked`", and the `--settings` merge is listed under "`launch` passes `dir(home, id)/skills` when `state.skills_bundled` is non-empty". A hire with no role and no asked skills has `picked` empty, so no bundle, so no `skillOverrides`, so an irrelevant global skill stays on. Acceptance 10 ("the next Claude hire's `skillOverrides` turns `seo-audit` off") fails for that hire. Evidence: design lines 213-214 and 232-237; `src/agents/mod.rs:602-624` (`launch` passes argv only from `state`). Fix: make the `--settings` merge depend on `hidden` being non-empty, not on the bundle; pass `bundle` only when `picked` is non-empty. Add an acceptance row and an S4 test row for a hire with `picked` empty and `hidden` non-empty.

2. blocking §Semantic ISA, inventory and taint (lines 286-319): a sink is missing. Prime-sourced `note` (VERIFY-W caps length only on write; VERIFY-R does not cover `note`) and catalog `description` (from `SKILL.md` front matter) are rendered in the desktop Skills panel (lines 260-266), and the Hire dialog and run page show picked-skill reasons and descriptions. The sink is the window DOM, not in S1-S7, and no VERIFY is stated. The app's convention is DOM builders with `textContent` (`apps/desktop/ui/app.js:1248`, `el(..., {textContent})`), but the window has one `innerHTML` use (`app.js`, grep count 1). Evidence: design lines 260-266, 311-319; `apps/desktop/ui/app.js:1248`; `apps/desktop/tauri.conf.json` CSP `script-src 'self'`. Fix: add S8 (window render) to the sink list and inventory; VERIFY-D: all Prime and SKILL.md text is set with `textContent` or an `el()` text child, never `innerHTML`; add an ISA row: a `note` of `<img src=x onerror=...>` renders as literal text.

3. blocking §Semantic ISA, side-effect inventory (lines 286-295): E4 omits deletion. §4 step 1 (lines 217-220) and the r2 fix now call `remove` on `out/agents/<id>/skills.tmp` and `out/agents/<id>/skills`; E4 lists only creates, writes, `cp -Rc` and `rename`. Also not listed: `mkdir` of `out/skills/` (E3). Evidence: design line 217-220 vs row E4 line 291. The path is safe: `launch` gets the id through `run_dir` (`src/agents/mod.rs:363-368`, single normal component), so the removal cannot leave `out/agents/`. Fix: add to E4 "delete: `remove_dir_all` of `skills.tmp` and `skills` under the validated run directory" and to E3 "creates `out/skills/` if absent".

4. blocking §5 Curation, the sandbox argument (lines 273-280) and Taint (line 311): the claim that a hire cannot write Prime rests on two checks, and neither is verified. (a) `COS_RUN_ID` is set only through `environment()` (`src/agents/mod.rs:731`); a hire's Bash can run `/usr/bin/env -u COS_RUN_ID cos skills set …`, which the design does not mention. (b) The network check assumes Claude's Seatbelt, under `CLAUDE_SANDBOX` (`src/agents/harness.rs:13`, `network.strictAllowlist: true`), refuses loopback to `127.0.0.1:3906`. Nothing in the repo or design shows that loopback is refused; the harness doc comment (`harness.rs:4-7`) says only "network refused without a prompt". Design line 311 states that "any local process can write Prime", so if loopback is open, a hire can `curl` `prime_add_node` straight into Prime and bypass VERIFY-W entirely. That breaks the "hires sandboxed" invariant and sink S2 with no VERIFY on that path. Fix: before the design is final, a Build hand-check of a Claude hire's Bash `curl http://127.0.0.1:3906/mcp` with `prime_add_node`, recording refused or prompted; if loopback passes silently, deny it in the merged settings or add a Prime-side write control before shipping. Add an ISA row to S2: a raw POST from the hire's sandbox does not reach Prime.

## Carried to Plan

- Edge target ids (r1-9, r2 carried): either add `id` to the stored fold (`graph.json` and the `Fact` read) or drop `requires` edges.
- Fold test row (r2 carried): add "newest fact invalid, older valid, skill picked from the older" to `tests/skills.rs`.
- Repo `.claude/skills` plan warning (r1-12, r2 carried): "repo skills load natively" when the repo holds `.claude/skills`.
- Prime-down-on-write acceptance row (r2 carried): exit 1, URL named, `graph.json` unchanged.
- Context line 54-55 (r1-17, r2 carried): mark the 37-snapshot figure as the author's measurement, as A4 does.
- Acceptance 4 (r2 carried): state the plan digest check runs after `state.json` has `skills_bundled`.
- E7 location (design line 294): the spec write is `write_json(&d.join("spec.json"), …)` inside `record_run` (`src/agents/mod.rs:509-600`); "line 580" points at the state write, not the spec write.
- `plan` per ask (`src/main.rs:987`, `for ask in picked`): `read_graph` runs once per ask. The 30 s cache absorbs it, but the design should say the graph is read once per invocation.
- `seed.rs` `clone_cmd` is defined twice (`src/agents/seed.rs:148` and `:155`, presumably cfg-gated). The `pub(crate)` change must cover both.
- Project-skill check (§2 step 5, lines 183-187) reads the repo the plan resolved (main checkout), not the worktree's HEAD `.claude/skills` (the worktree is detached at `base`). Say which one governs.
- Codex A2 spike and A1 fallback: the A1 fallback (`--add-dir`) loads `.claude/skills` under the added dir, which is a different layout from the plugin bundle; name the layout if the fallback is taken.
