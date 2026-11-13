---
design: docs/plans/2026-11-12-multi-business-design.md
design-sha256: superseded
verdict: findings
---

Previous findings (r5):

1. resolved: Testing and Acceptance 6 now state the partial-override triples. Evidence: design:425-427 (`COS_HOME=/h` gives `(/h, /h, $HOME/Projects)`; `COS_PROJECTS=/p` gives `(<install>, <install>, /p)`), design:515-518 (full triple, no elision).
2. resolved as accepted residual: `agents_stop` and reconcile kill. Evidence: design:490-492 names `harness_group` and `is_cos`; code is as stated (`mod.rs:551-559`, `mod.rs:685-689`). Sink rows still cite only the write guard (design:384); accepted, not re-raised.
3. resolved as accepted residual: `agents_spawn` repo. Evidence: design:492-493. Not re-read in `hire.rs`.
4. resolved: rejudge and clean now check `out/agents/`, not the projects directory. Evidence: design:385-386, design:401. New contradictions in findings 7 and 8.
5. resolved as accepted residual: contract-check commands. Evidence: design:373-375 (taint source), design:492-495 (accepted); `judge.rs:292-293` confirms `sh -c` in `workdir`.
6. resolved: `task schedule` sink row added. Evidence: design:387. Ordering gap in findings 10 and 11.
7. not resolved (nit): row design:383 still overlaps rows design:384-386 for `agents_clean`.
8. not resolved (nit): `use` still absent from the sink table (design:381). Now finding 2.
9. resolved: `state init` rule is consistent across design:159-161, design:396, design:412. Refined in finding 9.

Still open from r4/r5 nits (not re-audited here): design:135 `slots.rs` lock helper, design:161/185/493 `default` term, design:497-499 Acceptance 4, design:213-216 `Settings::save` wording, design:153 `default_home` location, design:438-439 spawn test.

Findings:

1. should Semantic ISA / Taint (design:373-377): agent-authored input is not a taint source, but a `cos` run inside an agent can reach `STORE(registry)`. Evidence: design:264-267 (spawn env makes `cos` inside agents resolve the spawning triple); design:381 (write sink, no agent source listed). Fix: add arguments and output from agent runs to Sources; or state that agent runs cannot invoke `cos business add|use|remove` and enforce it (env flag or harness allowlist), with a matrix case.

2. should ISA sink table (design:381): `use <name>` writes `active` with no existence check in the sink row. A dangling `active` makes every resolving command fail (design:131-132, design:396). Evidence: design:194 states "the business must exist", but design:381 does not list it. Fix: add "`use`: name in registry" to the row.

3. should ISA sink table, missing row: `cos business add` seeds `<home>/context/` from `templates/context/` (design:180-181; inventory design:347). This is an Execution step writing into a user-supplied path with no sink row and no stated order against the registry write. Fix: add a sink row cleared by `VERIFY(business_dirs)`; state that seeding runs only after both verifies pass and before or after `STORE(registry)`.

4. should ISA sink `agents_clean` (design:386): VERIFY covers the worktree path only. The code also runs `remove_dir_all(dir(home,id))` (`mod.rs:606`, `mod.rs:260-262`), where `id` comes from the IPC call and is not validated in `clean`. In-place runs remove only that directory, so the VERIFY passes trivially for them. Evidence: `mod.rs:570-608`. Fix: VERIFY that `id` is the run-id format (`new_id`, `mod.rs:637`) and that `dir(home,id)` canonicalises under `out/agents/` before any removal; add a matrix case with a crafted id.

5. should Inventory (design:349): `businesses()` / `agents::count` is listed as "read-only, nothing affected", but it spawns `ps`. `classify` calls `is_cos` → `comm()` (`mod.rs:659-665`, `mod.rs:668-671`), and `harness_group` calls `started_at()` (`mod.rs:674-688`), once per live record in every business, on picker open and Agents tab open (design:259-260, design:232-236). Evidence: as cited. Fix: correct the row to "process: `ps` per live record"; state the bound (cache with TTL, or confirm it runs only on explicit open) per the process-ownership rule.

6. should Inventory (design:341): row says "files, process | as today" for `agents_spawn`. The change adds new env to the supervisor and harness: `COS_HOME`, `COS_WORKSPACE`, `COS_PROJECTS` set, `COS_BUSINESS` removed (design:264-267). Current spawn sets only `--home` and inherits env (`mod.rs:422-425`). Fix: add that env change to the row and to "affected if it misfires" (the agent's own `cos` calls).

7. should Testing (design:452-453) contradicts ISA row design:386 and matrix design:417. The test says `agents_clean` skips a record whose `repo` is outside projects; the design now checks the worktree path under `out/agents/`. A test written from it asserts the removed guard. Fix: "`agents_clean` skips a record whose worktree path is outside the business's `out/agents/`".

8. should Violations design:401 contradicts ISA row design:385 and matrix design:416. Violations says a workdir outside `out/agents/` means "rejudge runs nothing". Row 385 allows an in-place run whose `workdir` equals its recorded `spec.repo`, which normally lies outside the home. Matrix design:416 does not say which kind of run it edits. Fix: change design:401 to "worktree outside `out/agents/`, or in-place workdir differs from `spec.repo`"; state the run kind in design:416 and design:451-452.

9. should Resolver / Violations (design:142-144, design:159-161, design:396): the Resolver takes "the registry" as an input value, but design:396 says flag- or env-given commands "never read the registry and still run". The matrix (design:412) covers only resolving commands with a corrupt registry, so the `--workspace state init` case is untested. Fix: state that the registry is loaded only when layer 3 or `--business` / `COS_BUSINESS` is consulted; add a test: corrupt registry plus `--workspace <dir> state init` runs.

10. should Sinks, `task schedule` (design:387, design:418) and Acceptance 9 (design:524): row says `cos --business <name> business home` exit 0 proves the name exists and its directories exist. The `home` spec (design:195-196) states no existence check, and the Resolver does not check existence (design:153-154). Fix: either add "`home` fails when a directory is missing" to design:195-196, or narrow row 387 to "the name exists".

11. should Sinks, `task schedule` order (design:387, design:443-444): row says the check runs "before any plist is written", but the current Taskfile runs `mkdir -p out/logs` and `unload-retired` before the plist step (`Taskfile.yml:190-191`). `unload-retired` deletes `cos.review.plist` and bootouts it (`Taskfile.yml:206-211`). That contradicts "existing plists byte-identical" (design:418, design:524). Fix: state that the business check runs first, then `mkdir` and `unload-retired`; say the retired plist is left alone when the check fails.

Nits:

- design:383: row "app commands in the inventory" still overlaps rows 384-386 for `agents_clean`. Name the exclusion.
- design:476-478: "except the partial override fill" reads as an open blocking finding. It is resolved and is the user's decision; drop the exception.
- design:346 and design:515-517: prose says workspace comes from built-in defaults; the Resolver says workspace falls back to the given home (design:147-149). The printed values match, so only the wording is wrong.
- design:337: `businesses.toml.lock` creation is not in the inventory.
- design:315-316: Risks names only `cos mcp` as rebinding on restart; `cos serve` and `cos tunnel` also bind the active business on start (design:202-203).

Verdict: findings. Blocking 0, should 11, nit 5. No blocking defect. The partial-override fill is consistent across Decision, Resolver, Testing and Acceptance. No business mix beyond the recorded user decision was found.
