# Multiple businesses in one cos: plan critique

Three independent read-only critics (coverage, dependency graph and feasibility, verifiability)
reviewed the first draft of [the plan](2026-11-12-multi-business-plan.md) against the approved
design and the code. 7 blocking and 33 should-fix findings; all are resolved in the plan.

| Finding | Resolution in the plan |
|---|---|
| `add` refusal rows (bad names, `--replace`, shared home or workspace) had no named test | ISA-row-to-test table; US-002 `add_refuses_bad_names_and_duplicates`, `add_refuses_bad_or_shared_dirs` |
| Acceptance 8 (`launchctl kickstart`) and the acceptance preconditions had no owner | Epic gate lists acceptance 1-9 with owning stories and the preconditions |
| Hand-check omitted copy and paste after the menu change, the Hire unsaved guard and the badge text | US-009a and US-009b hand-checks |
| `tests/schedule.rs` would run the real `launchctl` and the CI has no `task` | US-010: `LAUNCHCTL` and `COS` Taskfile vars, a stub, temp `LaunchAgents`, go-task in CI |
| `HOME_DIR` capture and the plist `--business` edit were unspecified | US-010 names the task var and both plist edits |
| Missing edges: US-006 → US-008, US-004 → US-007, US-001 → US-003 | Graph updated; parallel roots get file ownership |
| `cargo test` alone does not build `cos-desktop` | Story gate: `cargo test --workspace` for any `pub` change |
| No reusable blocking lock; `atomic_write` widening touches 12 call sites | `slots.rs` `lock_exclusive`; new `atomic_write_bytes` beside `atomic_write` |
| US-005 offered an inline unit test, against repo convention | Pinned to `tests/business_cli.rs::claude_runs_from_install` |
| US-000 implied cherry-picking all of `55e5129` and editing the global git config | Only the `tests/flows.rs` hunk; `GIT_CONFIG_GLOBAL` temp file |
| "Written first and failing" was uncheckable | Failing output appended to the bead before implementing |
| Judgement-call `rg` checks | Each `rg` names its expected output |
| Unenumerated consumers, start-line and state-init hint untested | US-003 lists the sites and names tests |
| `watch` with a skipped id untested | US-006 `watch_waits_on_skipped` |
| Removal of `set_workspace`/`built_home`, the docs header check, business args from `paths()` untested | US-008a `rg` check, US-008b `docs_post_checks_business_header`, US-009a `rg` check |
| Badge latency criterion passed either way and used a person's registry | US-008a `businesses_under_200ms` on fixtures; US-009b fallback stated |
| Resolver gaps (no active, unset `HOME`), `add` profile hint, first-run notice, bad-dir message | US-003 and US-004 tests |
| `BUSINESS` default, old plists with `COS_HOME`, long-lived processes following `use` | US-010 and US-003 tests |
| Corrupt `settings.json`, triple never written back, workspace without profile at switch | US-008a tests |
| App-level bad-id refusals for rejudge, stop, clean; a run spawned before a switch | US-008b tests |
| US-003, US-006, US-008, US-009 too large | US-008 and US-009 split in two; US-003 and US-006 list their files |
| `cargo test --test kit` was not a doc check | US-011 `rg` gate over the five files |

A second read-only check confirmed 22 of 22 resolved and found 5 new blocking defects, also
resolved:

| Finding | Resolution in the plan |
|---|---|
| US-003's `mcp_follows_active_on_next_start` used `business use`, which US-004 delivers | The test rewrites the fixture registry's `active` directly |
| `cargo tree -i toml` is ambiguous: the lockfile also holds `toml` 0.8.2 | `cargo tree --locked -i toml@1.1.8 --workspace` lists `cos` |
| "Seven write commands" named only six | US-008b enumerates all seven with `lib.rs` lines; the docs POST is checked on its header; US-009a's `rg` covers docs POSTs |
| `task schedule`'s default `BUSINESS` had no machine-readable source | Read from the `current: <name>` line of `cos business list`; `explicit`/`default` or empty stops the task |
| UI hand-checks inside story criteria needed a person | UI stories close on automated tests; hand-checks run once before release with acceptance |
| Acceptance precondition cited unrelated `flows.rs` lines | Cites design § Acceptance |
