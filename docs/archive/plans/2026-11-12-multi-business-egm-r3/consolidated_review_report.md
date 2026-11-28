# Consolidated review: multi-business design, EGM round 3

Target: `docs/plans/2026-11-12-multi-business-design.md`. Design not edited.
Inputs: 7 reports in this directory (zoom_out, inventive_catalyst, comprehension, critic_design, critic_structural, critic_style, intermediate_zoom_out). 78 findings in total, deduplicated to 39 items: 1 Blocker, 16 Major, 22 Nit.

Code citations are as the reports gave them. Spot-check done: `src/claude.rs:120` (`current_dir(home)`) and `:125` (`--setting-sources project`) confirmed. The rest were not re-read here.

Dispositions: where a reviewer rejected another reviewer's suggestion, both are shown. The Traceability Matrix maps every source finding to an item.

---

## 🔴 Blocker (1)

**B1. Home has no agent definitions, CLAUDE.md or context seed. Brief's main job and Acceptance 8 fail.** — Design lines 147-168 (migration, `add`), 393 (preconditions), 411-412 (Acceptance 8).
- Problem: `cos brief` and `ask` run `claude --agent tactician` with `current_dir(home)` and `--setting-sources project`, so `CLAUDE.md` and `.claude/agents/tactician.md` must exist in the home. A new `~/biz/acme` (empty dir plus `state init`) has neither. `context/` is seeded only by `task setup`. `add` checks only that dirs exist and a profile is present.
- Evidence: `src/claude.rs:120, 125`; `src/main.rs:17` (home documented as "Repo holding CLAUDE.md, .claude/agents and context/"); `Taskfile.yml:17` (`cp -Rn templates/context/ context/`).
- Fix: name the install once. `claude` runs with the install as cwd, so agent definitions and `CLAUDE.md` come from there (safe: brains have no file tools, context is inlined, `CLAUDE.md:48`). `add` (or docs) seeds a new home's `context/` from the install's `templates/context`. Rejected: scaffolding full homes (copies install files into every business) and warn-only (leaves brief job broken). Then fix Acceptance 8 and add a `brief` step for acme.
- Sources: intermediate #1, critic_design #1.

---

## 🟡 Major (16)

**M1. Spawn strips `COS_*` triple, so a nested `cos` in an agent run binds the active business, not the spawning one.** — Design lines 215-217 (spawn env), 367-368 (test). Also `--workspace/--projects` on `agents supervise` are dead: `Supervise { id }` takes only `id`.
- Evidence: `supervise.rs:165` (PATH), `seed.rs:115` (COS_SEED_REPO), `main.rs:290-291`, `main.rs:713`.
- Fix: child gets `COS_HOME`, `COS_WORKSPACE`, `COS_PROJECTS` set to the spawning triple; remove only `COS_BUSINESS`. Test: nested `cos` resolves the spawning triple. State what nested `cos` binds to.
- Sources: zoom_out F1, intermediate #2.

**M2. First-run migration source depends on which process runs first.** — Design lines 147-153; Acceptance 1; tests 362-364.
- Problem: CLI-first can write a one-off `--home /tmp` or `COS_HOME` as the permanent `default`. App-first reads legacy `settings.json`, which may hold env-derived transient values (`save()` writes them, `settings.rs:91-98`). If any `cos business` command runs first, the app skips migration and the `set_workspace` choice in `settings.json` is silently lost.
- Fix: migration writes `default` from layer 4 only, never flags or env. At app start, if legacy `settings.json` dirs exist and match no registered business, show once `cos business add <name> …`. Test app-first and CLI-first.
- Sources: zoom_out F2, comprehension #5, critic_design #4, intermediate #4.

**M3. `state init` both "does not resolve" and "takes the resolved workspace".** — Design lines 141-143; Violations line 335; brief assumption `cos --business x state init`.
- Evidence: `main.rs:349` (all `state` subcommands get `cli.home` and `ws`), `main.rs:743` (`Propose` reads briefs from home).
- Fix (one rule): `state init` resolves like any command, skips the profile check; on a broken registry accepts only explicit `--workspace`. Other `state` subcommands resolve normally. State it once. (Severity raised from critic_design #10, Nit.)
- Sources: zoom_out F3, intermediate #8, critic_design #10.

**M4. launchd log paths and plist-follows-active behaviour cannot work as written.** — Design lines 170-178, 265-266.
- Problem: `StandardOutPath`/`StandardErrorPath` are static plist strings filled from `@COS_HOME@` at `task schedule`; launchd opens them before `cos` runs, so "the job creates" `out/logs` is impossible and a missing dir stops the job. A `follow the active business` plist cannot retarget logs. Also `cos.dash` has KeepAlive and never restarts, so after `use` the dashboard serves the old business indefinitely while `cos.brief` (launched fresh) follows the new one: a silent split.
- Evidence: `launchd/cos.brief.plist:24-25`, `launchd/cos.dash.plist:17-21`, `Taskfile.yml:187-197` (critic_design cites 190-195), `main.rs:318-322` (stderr assumed to be a file).
- Fix: `task schedule` always bakes one business into both plists (`BUSINESS=` if given, else active at schedule time), fills log paths from that home, keeps `mkdir -p`. Switching jobs means rerunning `task schedule`, as the brief says. Rejected: `use` kickstarting dash (adds launchctl dependency to a registry write). Alternative if mode is kept: fixed logs under `~/Library/Logs/cos/`, and state which.
- Sources: comprehension #2, critic_design #2, #3, critic_structural #2, intermediate #3.

**M5. Run-status vocabulary is wrong.** — Design lines 29-31 (Terms "Run record").
- Problem: design lists terminal `done, failed, stopped, lost`. Code has no `stopped`; it has `Killed` and `TimedOut`. Badge and `classify` are built on this list.
- Evidence: `src/agents/mod.rs:110-124` (enum: Starting, Queued, Seeding, Running, Done, Failed, Killed, TimedOut, Lost), `:127-131` (terminal check), `:137-147`.
- Fix: correct the list to `done, failed, killed, timed_out, lost` as terminal; name the non-terminal set.
- Sources: comprehension #1, critic_structural #1, catalyst #3 (vocabulary part; Nit in source, raised here).

**M6. Badge count semantics undefined: what is "running", what is "lost", and terminal records.** — Design lines 193-197, 211-212, 255-262 (Risks), Departures line 63.
- Problem: Departures say `running` only, but `count` comes from `classify` where Live covers queued, seeding and running (`mod.rs:474`). `classify` has no terminal case (`reconcile` returns early, `mod.rs:470`). If `lost` means status `lost`, `reconcile` writes a terminal `Lost` (`mod.rs:489-492`), and since runs are never pruned the badge never clears until `agents clean`.
- Fix: `count` over non-terminal records only. `running` = live supervisor; `lost` = non-terminal with dead supervisor ("needs cleanup; open it"). Print each part only when non-zero. Specify `classify` on terminal input. Test: badge clears after the business is opened.
- Sources: critic_design #5, intermediate #5, catalyst #3 (badge part).

**M7. Badge text format and zero-part rule disagree with the acceptance test.** — Design line 211 vs Acceptance 5 (line 404).
- Problem: spec `N running, M lost in other businesses` vs acceptance `1 running in other businesses`. Unstated: whether a zero part is dropped, whether "non-zero" applies per count or to the sum, whether the badge hides when both are zero. Departure 3 says lost is "listed separately", not combined.
- Fix: state the rule per count (print each non-zero part; hide when both zero); make Acceptance 5 match the chosen string exactly.
- Sources: comprehension #6, critic_structural #7, style #4.

**M8. Badge latency grows with run history; picker open may never reach 200 ms.** — Design lines 193-197, 258-259, 388-389; Confidence "Badge: sand".
- Problem: one record read per run per business, runs never pruned, and picker open waits on counts.
- Proposed (rejected by intermediate): async fill per business; live-run marker index; slot-lock probing (`supervise.rs:55-58`, `LOCK_NB`). Index can drift; probing misses `starting` and `lost` runs and can delay a supervisor by one `TICK`.
- Disposition: intermediate says measure first, then decide. Keep the 200 ms gate; adopt the M6 counting rule first, since it removes the dead-record reads. Revisit only if the measured gate fails.
- Sources: catalyst #2 (disputed).

**M9. Menu accelerator replaces the default macOS menu and drops Edit items (copy, paste, undo, select all).** — Design lines 52, 201-205, 376, 387. No menu exists today (`apps/desktop/src`, grep `menu`: 0 hits; `lib.rs:503-547`).
- Problem: Cmd+C/V/Z/A stop working in the Hire prompt and docs room. Claim stated as fact in Decision (line 52) while Confidence marks it sand.
- Fix: keep the predefined App, Edit and Window items; append one item for the switch. Do not build a custom menu. Add copy and paste in the Hire prompt to the `pr-proof` hand-check. Calibrate line 52 to "expected to work inside the frames; confirmed by the hand-check".
- Sources: critic_design #6, intermediate #6, style #1.

**M10. In-flight frame save can land after the switch and write into the wrong business.** — Design lines 187-188, 206-210. `app.js:156-165, 158-160`; `lib.rs:151-158`.
- Problem: debounced `cos-docs-save` from business A's docs frame calls `docs_save` while `switch_business` is pending. If the switch lands first, A's text is written to B's `<home>/context`. Single snapshot per command does not order two IPC calls.
- Fix (intermediate): the picker sets a `switching` flag in `app.js`; frame saves are dropped or awaited; then `switch_business` and reload. One place, one IPC test. Alternative (critic): send the business name with `docs_save`, `agents_spawn`, `agents_record` and refuse on mismatch. Intermediate's option is cheaper; critic's is safer against other unordered paths. Pick one and test it.
- Sources: critic_design #7, intermediate #7.

**M11. Decision overstates: "a triple never mixes sources".** — Design lines 50-51. Lines 131-132 let missing dirs come from defaults.
- Fix: calibrate to "no triple mixes layers 1-3; missing directories come from defaults".
- Sources: style #2.

**M12. "default" has three meanings.** — Design line 27 (Active business = "the registry's default business"), resolver layer 4 (line 136), `current: <name|explicit|default>` (line 158), start line (line 144), Acceptance 1.
- Problem: `current: default` is ambiguous when a business is named `default`.
- Fix: define Active business as "the registry's `active` entry"; label layer 4 `built-in`. Use `built-in` in start and `list` output.
- Sources: style #3, structural #6, intermediate #11.

**M13. Departures from the brief do not show before and after.** — Design lines 55-63.
- Problem: reader must open the brief to see what changed. Badge "switches to them" in brief becomes "opens picker" in design (undisclosed; zoom_out F6). `running` is introduced before "badge".
- Fix: table with columns Brief | Design | Why, one row per departure, including the click-opens-picker change.
- Sources: comprehension #3, zoom_out F6 (part).

**M14. Risks entries give limits but no mitigation.** — Design lines 252-266.
- Problem: Badge cost (no fallback if 200 ms fails), Env pin, MCP and launchd after `use` have no mitigation and are not marked accepted.
- Fix: add a mitigation to each, or mark "accepted, by design" with a brief citation.
- Sources: critic_structural #3.

**M15. Pinning set undefined.** — Design lines 189-190; Risks line 263.
- Problem: unclear if `COS_BUSINESS` alone pins (header only shows `COS_HOME`). `COS_AGENT_SLOTS` is a `COS_*` var; not excluded.
- Fix: list the exact set: `COS_BUSINESS`, `COS_HOME`, `COS_WORKSPACE`, `COS_PROJECTS` pin; `COS_AGENT_SLOTS` does not. Header shows the first set var by name.
- Sources: comprehension #4.

**M16. Raw `PathBuf` for home/workspace/projects allows silent swaps. Proposed newtypes.** — Design lines 128, 219-223 (renames).
- Problem: names do not enforce types; a future call site can pass `triple.home` where a workspace belongs with no compile error.
- Proposed: `Home`, `Workspace`, `Projects` newtypes in `src/business.rs`.
- Disposition: disputed. Intermediate rejects as YAGNI (signature churn across pipeline, track, context, main, desktop). Recommend: keep renames and characterisation test; revisit if a misroute recurs.
- Sources: catalyst #1 (disputed).

---

## 🟢 Nit (22)

**N1. Lock helper and atomic write live in the wrong module; symlinked registry is replaced.** — Design lines 117-120. `slots.rs:44-48` (`flock` private), `state.rs:548-573` (`atomic_write(path, &Value)`, renames over target).
- Fix: move `flock` (path-to-guard) and `atomic_write` (bytes, temp prefix) to one shared module, so `business.rs` does not depend on `agents::slots`. Refuse a symlinked registry the way `state::load` does; no symlink-following logic.
- Sources: zoom_out F4, catalyst #7, critic_design #12, intermediate #10.

**N2. `settings` is mutable only through `set_workspace`; Security claims a write nothing performs.** — Design lines 184, 187, 271. `lib.rs:75-77` (only `.save(`).
- Fix: drop `Settings::save`; keep `settings` immutable; only `triple` behind the mutex; correct Security line.
- Sources: zoom_out F5, intermediate #12.

**N3. Context section altitude and order.** — Design lines 46-95, 67-94, line 70 bare line numbers.
- Fix: move implementation detail (regex, `std::path::absolute`, `default_value_os_t`, `deny_unknown_fields`, line inventories) to the plan. Put Context before Decision, or label the line-number runs by command.
- Sources: zoom_out F7, structural #4, #10, intermediate #14.

**N4. `remove` and `add --replace` make live or lost runs invisible.** — Design lines 160-167.
- Fix: refuse while `count(old home)` is non-zero, reusing M6's `count`.
- Sources: critic_design #8, intermediate #13.

**N5. Lone `COS_WORKSPACE` or `COS_PROJECTS` silently skips the registry.** — Design lines 130-137.
- Fix: one-line stderr note when a higher layer hides an active business, naming the variable.
- Sources: critic_design #9.

**N6. Resolver tests read `$HOME` from ambient env.** — Design lines 127, 137, 354.
- Fix: add `home_env: Option<PathBuf>` to the inputs.
- Sources: critic_design #11.

**N7. Alternatives rejections are terse or unattributed.** — Design lines 238-250.
- Fix: link the brief's Decisions for the TOML choice, and add the technical reason (hand-editable, comments). Give each rejection one concrete trade-off.
- Sources: critic_design #14, critic_structural #9, comprehension #11.

**N8. `--business` in every user-facing hint.** — `src/mcp.rs:282`, `src/agents/view.rs:12`.
- Disposition: disputed. Intermediate rejects as YAGNI. Revisit if the wrong-business hint reaches a user.
- Sources: critic_design #13 (disputed).

**N9. Picker-click departure and `Starting` arm.** — Design lines 55-63 (click), `classify` (`Starting` unused by `count`).
- Fix: confirm `Starting` folds into neither count, or drop it from the return type. Note the click change under M13.
- Sources: zoom_out F6.

**N10. Goals and non-goals are run-on prose.** — Design lines 39-44.
- Fix: bullets.
- Sources: structural #5.

**N11. Acceptance 4 omits the Hire pane and the plan area.** — Design lines 400-403; brief "the job when this is done"; `ui/index.html:18-21`.
- Fix: add Hire pane to Acceptance 4; state where "plan" lives or drop it from the brief job.
- Sources: critic_structural #8.

**N12. Undefined project jargon.** — Design Context and Desktop: `slots`, `cos` (in Settings), `COS_AGENT_SLOTS`, `agents build-slot`, harness and harness group, `is_cos`, `START_GRACE`, `spec` in `classify`, `Triple.layer` type, ritual flows, ledger.
- Fix: define in Terms or drop.
- Sources: comprehension #7.

**N13. `tactician.md` "keeps the install home" unexplained.** — Design lines 43-44.
- Fix: one sentence on what the file is and why it matters (ties to B1).
- Sources: comprehension #8.

**N14. Renames rationale vague.** — Design lines 91-94, 219-223.
- Fix: name the later change that would misroute if the rename were skipped.
- Sources: comprehension #9.

**N15. Reload cache claim lacks an inventory; `pane` undefined for tabs without panes.** — Design lines 206-209; `flows.rs:673-694`.
- Fix: list the JS and Rust caches and show none others exist; define `pane` for paneless tabs.
- Sources: comprehension #10.

**N16. ISA opcodes not defined.** — Design lines 303-310.
- Fix: one line each for `LOAD`, `STORE`, `TOOL_CALL`.
- Sources: comprehension #12.

**N17. "with focus inside nothing" unclear.** — Design line 400.
- Fix: "focus on the app body, outside any frame".
- Sources: comprehension #13.

**N18. "agree by construction" holds only for the same checkout.** — Design line 139.
- Fix: add "when built from the same checkout".
- Sources: style #5.

**N19. Threshold relocated.** — Design lines 258-259 vs 388.
- Fix: cross-reference the 200 ms gate from Risks.
- Sources: style #6.

**N20. Wording batch.** — Design lines 18 ("the brain"), 30-31 (stacked parentheticals), 189 and 263 ("pinned" missing from Terms), 209 ("so they need nothing" → "need no reset"), 279 ("changes the world" → "has side effects on files or processes"), 326 ("the switch's VERIFY" → name `VERIFY(business_name)` and `VERIFY(business_dirs)`), 160-165 (split nested parenthetical on `.chief-of-staff` symlink).
- Sources: style #7, #8, #9, #10, #11, #12, #13.

**N21. Pin override ("switch anyway") rejected.** — Design lines 189-190.
- Disposition: disputed; intermediate rejects as YAGNI. Pin stays. Revisit after shipping.
- Sources: catalyst #5 (disputed).

**N22. Add `--init` and repair-on-count rejected.** — Design lines 57-59 (add), 260-262 (orphaned harness).
- Disposition: `add --init` and repair-on-count both disputed. Intermediate rejects repair-on-count because it breaks the read-only guarantee of `businesses()` (line 297 in the ISA matrix). Keep the read-only design; the orphan case is covered by opening the business.
- Sources: catalyst #4, #6 (disputed).

---

## Traceability Matrix

Format: source report, finding number, arrow, consolidated item. Findings numbered as in each report.

| Report | Finding → Item |
|---|---|
| zoom_out_report | F1→M1; F2→M2; F3→M3; F4→N1; F5→N2; F6→N9, M13; F7→N3 |
| inventive_catalyst_report | #1→M16; #2→M8; #3→M5, M6, N-wording (Starting, vocab); #4→N22; #5→N21; #6→N22; #7→N1 |
| comprehension_report | #1→M5; #2→M4; #3→M13; #4→M15; #5→M2; #6→M7; #7→N12; #8→N13; #9→N14; #10→N15; #11→N7; #12→N16; #13→N17 |
| critic_design | #1→B1; #2→M4; #3→M4; #4→M2; #5→M6; #6→M9; #7→M10; #8→N4; #9→N5; #10→M3; #11→N6; #12→N1; #13→N8; #14→N7 |
| critic_structural | #1→M5; #2→M4; #3→M14; #4→N3; #5→N10; #6→M12; #7→M7; #8→N11; #9→N7; #10→N3 |
| critic_style | #1→M9; #2→M11; #3→M12; #4→M7; #5→N18; #6→N19; #7→N20; #8→N20; #9→N20; #10→N20; #11→N20; #12→N20; #13→N20 |
| intermediate_zoom_out_report | #1→B1; #2→M1; #3→M4; #4→M2; #5→M6; #6→M9; #7→M10; #8→M3; #9→N8, M16, N21, N22 (rejections); #10→N1; #11→M12; #12→N2; #13→N4; #14→N3 |

Count check: 7 + 7 + 13 + 14 + 10 + 13 + 14 = 78 source findings, all mapped.

Notes on dispositions: M8, M16, N8, N21 and N22 were rejected or flagged as YAGNI by the intermediate review. They stay listed so the decision is visible, not silently dropped.
