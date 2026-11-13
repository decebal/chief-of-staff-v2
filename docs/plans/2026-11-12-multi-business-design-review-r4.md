---
design: docs/plans/2026-11-12-multi-business-design.md
design-sha256: superseded
verdict: findings
---

Round-3 blockers (Goldfish r3 items and EGM r3 B1 and the Majors the design claims to fix):

1. Goldfish r3 #1, mixed triple: NOT RESOLVED. Resolver steps 1-2 (design:141-149) still fill missing directories from built-in defaults, so `COS_PROJECTS=/p` alone with an active business gives home = install. Acceptance 6 (design:478-479) and Testing (design:402-404) codify the mix. Accepted trade-offs (design:447-448) claim "blocking findings are fixed above", which is false.
2. Goldfish r3 #2, launchd logs and `out/logs`: RESOLVED. `task schedule` creates `<home>/out/logs` and bakes `@HOME_DIR@` (design:199-201); Acceptance 8 creates it (design:483-485). Taskfile.yml:190 still creates only the install's `out/logs`, which is the current state, not the design.
3. Goldfish r3 #3, ISA sinks for rejudge and clean: NOT RESOLVED (see findings 1 and 2 below).
4. Goldfish r3 #4, migration source: RESOLVED. Migration writes `default` from built-in defaults only (design:161-166).
5. Goldfish r3 #5, corrupt registry at app start: RESOLVED (design:211-214).
6. Goldfish r3 #6, hash restore into Docs: NOT RESOLVED (finding 5).
7. Goldfish r3 #7, badge statuses: RESOLVED (design:31-33, 248). Queued run not added to Acceptance 5 (finding 14).
8. Goldfish r3 #8, Acceptance 4 `tasks` approval: RESOLVED (design:464-465).
9. Goldfish r3 #9, spawn test asserts child triple: RESOLVED (design:412-415). See finding 13.
10. Goldfish r3 #10, harness env: RESOLVED. Design states harness inherits spawn env (design:253-256). Verified: no `env_clear` anywhere in `src/` (grep -F), and `supervise.rs:157-165` only adds PATH to the harness command.
11. Goldfish r3 #11, `businesses()` error per business: NOT RESOLVED for a missing or unmounted home (finding 8). Unreadable records are covered (design:226-228).
12. EGM B1, home has no CLAUDE.md or agents: RESOLVED. Claude cwd is the install (design:170-175; Departures design:72-73).
13. EGM M1, spawn env: RESOLVED (design:253-256).
14. EGM M2, migration source: RESOLVED (design:161-166).
15. EGM M4, launchd logs: RESOLVED (see item 2).
16. EGM M5, run statuses: RESOLVED. Terms match `src/agents/mod.rs:110-124` (enum) and `:126-132` (`terminal()`).
17. EGM M9, menu Edit items: PARTIAL. Predefined Edit and Window kept (design:234-235). Decision still states ⌘K "works inside the report and docs frames" as fact (design:58-59), and the copy/paste Hire-prompt hand-check is not added (design:430-433).
18. EGM M10, in-flight docs saves: PARTIAL. docs_save and docs POST are tagged (design:241-244, 370). Other IPC writes are not (finding 7).

Findings (blocking first):

1. blocking Resolver / Decision / Testing / Acceptance 6 / Accepted trade-offs / Confidence: a flag or env layer that sets some directories still mixes in built-in defaults, the partial-edit mix the Problem names (design:16). Evidence: design:56 ("explicit directories completed from the built-in defaults"), design:143-144, 147-149, design:402-404 (test asserts the mixed env triple), design:478-479; `src/main.rs:18` (`COS_HOME`, `default_value_os_t = default_home()`), `:25` (`COS_PROJECTS`, default `$HOME/Projects`), `:42` (`ws()` falls back to home). Confidence design:439 ("resolver: rock") is premature. Fix: choose one rule and state it in Decision. Either (a) a partial flag or env layer is an error naming the unset directories when a registry business would apply, or (b) unset directories come from the active business, with each directory's source recorded in `Triple`. Add a test for `COS_PROJECTS` alone with an active business, for both CLI and app, and rewrite Acceptance 6 to match. Downgrade the resolver to sand until decided. Remove the "fixed above" claim at design:447-448 for this item.

2. blocking ISA, Side-effect inventory / Sinks / Instructions (`agents_rejudge`): the sink is cleared by "existing (unchanged)", which is not a named VERIFY, and the design's claim that commands "run only inside the run's own worktree" is not enforced. Evidence: `src/agents/mod.rs:498-504` (`rejudge` reads the run from the switched home and calls judge), `src/agents/judge.rs:139-142` (checks run when `Status::Done`), `:293` (`sh -c` with `current_dir(workdir)`); `workdir` comes from `spec.json` (`mod.rs:371, 406, 418`), which is user-editable. Design:332, design:371. Fix: add an instruction `VERIFY(run_workdir)` (workdir resolves under the active home's `out/agents/<id>/`, or under the spec's worktree base) and state it in the sinks table. Add ISA-matrix rows: rejudge after a switch runs checks only under the new home's worktree, and a `spec.json` whose workdir lies outside it is refused with nothing run.

3. blocking ISA, Sinks (`agents_clean`): `git worktree remove --force` runs against `spec.repo` and removes `spec.workdir`, with no sink-table row. The blanket row (design:369) says the switch's VERIFY clears it, but that VERIFY checks only the name and directories (design:368). Evidence: `src/agents/mod.rs:570-604` (`clean`: `repo = spec.repo`, `git worktree remove --force <spec.workdir>`); design:334 (inventory, reversibility "no", no VERIFY). Fix: add a sink row for `agents_clean` with `VERIFY(run_repo)` (repo is a projects-directory repo the run was hired on, and workdir lies under that repo's worktree base) and a matrix row: clean with `spec.repo` outside the projects directory is refused.

4. should Resolver, `state init`: the section says `state init` "does not resolve" and also "takes ... the resolved workspace" (design:152-155); violations row says `state init` "still runs" on a corrupt registry (design:380). Evidence: design:152-155, 380. Fix: one rule. Proposed: `state init` resolves like any command and skips the profile check; on a broken registry it accepts only an explicit `--workspace`. State it once.

5. should Testing / Desktop, hash restore: design:238-240 says `app.js` opens the tab and pane "through the same function a tab click uses". No such function exists: the tab handler is an anonymous listener (`apps/desktop/ui/app.js:134-152`), pane clicks are separate (`app.js:204-209`), and `showPane` is `app.js:195-202`. Frames are created only inside the tab handler (`app.js:144-150`), so a restore that sets the hash and never clicks leaves Docs blank. Confidence "hash restore: rock" (design:440) follows from the false claim. Fix: say "dispatch a click on `.app-tab[data-view=<tab>]`, then call `showPane` for the agents pane"; add a hand-check: switch while on Docs, Docs shows the new business.

6. should Desktop, menu and hand-check: Decision (design:58-59) states ⌘K works inside the frames, while Confidence (design:441) says sand. Hand-check (design:430-433) omits copy and paste in the Hire prompt and in the docs room, which EGM M9 required. Fix: calibrate Decision to "expected to work inside the frames; confirmed by the hand-check"; add copy, paste, undo in Hire prompt and docs room to the hand-check.

7. should Desktop, in-flight writes: design:241 says "writes that can arrive after a switch carry the app business", but only `docs_save` and the docs POST carry it (design:241-244, 370). `agents_spawn`, `agents_record`, `agents_rejudge`, `agents_clean` and `agents_stop` take no business tag (design:395-396 test only docs). The source of `business` for `docs_save` is also unstated: the current handler sends only `{ organize }` (`apps/desktop/ui/app.js:156-160`). Fix: either list which IPC writes can arrive after a switch and why the others cannot, or tag them. State where the app's JS gets the business for `docs_save`. Add a matrix row for each tagged write.

8. should Desktop, `businesses()` error rule: `agents::list` reads an absent directory as empty (`src/agents/mod.rs:455-459`, `read_dir(...).into_iter().flatten()`), so a missing or unmounted business home counts as zero silently. Design:223-228 defines `unreadable` only for records. Fix: define `count` returning an error per business for an absent or unreadable home; the picker row shows it, and the other rows still count. Test: a missing home in one business leaves the other rows counted.

9. should Risks, launchd after `use`: design:304-305 says "MCP and launchd after `cos business use` ... follow the new active business on the next start". That contradicts design:198-204 and Departures design:74: launchd jobs are baked at `task schedule` and do not follow `use`. Fix: remove launchd from the Risk entry; keep "rerun `task schedule`" as the mitigation.

10. should ISA inventory, `task schedule`: the design adds BUSINESS to a task that writes `~/Library/LaunchAgents` plists, runs `launchctl bootstrap` (`Taskfile.yml:187-197`), and creates `<home>/out/logs`. That leaves unattended Claude brief runs and a launchd state change, none in the side-effect inventory (design:324-340). Fix: add rows for `task schedule` (write plists, launchd bootstrap, mkdir logs; reversible by the unschedule task) and `task unschedule`.

11. nit Testing, spawn (design:412-415): the test asserts the child's triple, which is correct, but passes trivially when the registry's active business equals the spawning triple. Fix: state that the test's active business differs from the spawning triple.

12. nit Desktop, migration (design:210): "`settings.json` ... `home`, `workspace` and `projects` ... read only by migration". Migration writes from built-in defaults and reads the legacy directories only for the hint (design:163-165). Fix: say "read only for the hint".

13. nit Acceptance 5 (design:476-477): does not exercise a queued run, though `live` includes `queued` (design:31-32, 248). Fix: add a queued run and expect `2 live` or the corresponding count.

14. nit Registry, Write (design:129): "the lock helper in `src/agents/slots.rs`". The `flock` helper there is private and takes an open file (slots.rs:44); there is no path-based lock helper. Fix: "extract a path-based helper from slots.rs:44".

15. nit Terms (design:29) and Resolver (design:153, Acceptance 1): "default" means the active business's name, the built-in layer, and the business named `default`. `current: default` is ambiguous. Fix: label the built-in layer `built-in` in output and Terms.

16. nit Acceptance 4 (design:472): "focus inside nothing" is unclear; Hire pane missing from the list (design:474). Fix: "focus on the app body, outside any frame"; add the Hire pane.

17. nit Desktop, `save()` and `Option` fields (design:210): `Settings::save` (`apps/desktop/src/settings.rs:91-98`) serialises all fields, so `Option` fields write `null` unless `skip_serializing_if` is set. Fix: say whether `save()` remains and which fields it writes.

18. nit Resolver, location of `default_home()` (design:148): it lives in the binary crate (`src/main.rs:883`), and the resolver is in the library. Fix: state that `env!("CARGO_MANIFEST_DIR")` moves to the library, same package root.

Verdict: findings. Blocking 3, should 7, nit 8.
