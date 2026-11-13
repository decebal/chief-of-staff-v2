# Multiple businesses in one cos: plan

Design: [2026-11-12-multi-business-design.md](2026-11-12-multi-business-design.md) (approved,
sha256 c4ee8b6472135f922e7fc4fc79854b5b8cfdb0ad8f8523e0914ad47dcefaa66b). Epic `t-0c18d3`, branch
`feat/multi-business`, PR #24. Plan critique:
[2026-11-12-multi-business-plan-critique.md](2026-11-12-multi-business-plan-critique.md).

### Quality gates

#### Epic-level (once, when every story is done)
- [ ] `task test` (fmt, clippy `-D warnings`, `cargo test`), `task desktop-check`, and
  `cargo test --workspace`
- [ ] Every row of the ISA-row-to-test table below passes
- [ ] Acceptance, run end to end in the CLI and the built app (`task desktop-app`), recorded with
  `pr-proof`. Preconditions: `mkdir -p ~/biz/acme/repos`; `cos --workspace ~/biz/acme state init`;
  acme's `profile.json` approves a `tasks` source so its Flows tab reads (design § Acceptance).
  - [ ] 1. no registry → `cos business list` writes and lists `* default` (US-004)
  - [ ] 2. `cos business add acme …` seeds `context/`, leaves `default` (US-004)
  - [ ] 3. `cos --business acme report --no-open` writes acme's report (US-003)
  - [ ] 4. ⌘K from Agents, then inside the report frame; reload onto Agents showing acme; back to `default`; `cos business list` still marks `default` (US-009a)
  - [ ] 5. agent live in `default`, app on `acme` shows `1 live in other businesses`; picker row `1 live` (US-009b)
  - [ ] 6. `COS_HOME=… cos business list` prints `current: explicit …` from built-in defaults; app started with it shows `pinned by COS_HOME` and refuses a switch (US-003, US-008a)
  - [ ] 7. `--business acme --home /tmp` errors naming both; `COS_HOME=/tmp cos --business acme report` writes acme's (US-003)
  - [ ] 8. `task schedule BUSINESS=acme`, then `launchctl kickstart gui/$UID/cos.brief` writes a brief under `~/biz/acme/out` and `~/biz/acme/out/logs/brief.log` (US-010)
  - [ ] 9. every ISA row (table below)

#### Story-level
- A story that changes a `pub` item of the `cos` crate ends green on `cargo test --workspace`
  (root package and `cos-desktop`), not `cargo test` alone.
- Test first, checkably: before implementing, the agent runs the story's new tests, sees them
  fail, and appends the failing command and its last 20 lines to the bead
  (`cn task edit <id> --append-description`). The reviewer reads that entry.
- UI stories close on their automated tests. Their hand-check steps need a person at the native
  app; they run once, before release, with the epic's acceptance, and are recorded for `pr-proof`.
  A story's bead never waits on them.

### Testing trophy

#### Unit
- [ ] [Logic] registry load, write, lock, add checks (file: `tests/business.rs`)
- [ ] [Logic] resolver layers, conflicts, partial fill, no registry load for explicit triples (file: `tests/business.rs`)
- [ ] [Logic] `run_dir` refusals; `Listing.skipped` (file: `tests/agents.rs`)
- [ ] [Logic] `classify` parity with `reconcile`; `count` writes nothing (file: `tests/agents.rs`)
- [ ] [Logic] home ≠ workspace reads the workspace profile (file: `tests/workspace.rs`)
#### Contract
- [ ] [Handler] `cos business list|add|use|home|remove` output and exit codes (file: `tests/business_cli.rs`)
- [ ] [Handler] `switch_business`, `businesses()`, `paths()` shapes; seven write commands refuse a stale business (file: `apps/desktop/tests/ipc.rs`)
- [ ] [Handler] CLI `cos agents show|kill ../x`, MCP `stop_agent` `../x` refused (file: `tests/agents.rs`)
#### Integration
- [ ] [Flow] spawn under a parent with `COS_BUSINESS`/`COS_WORKSPACE`; the harness's `cos business list` prints the spawning triple (file: `tests/agents.rs`)
- [ ] [Flow] switch, then writes land under the new home; a run spawned before the switch keeps running and lists when its business reopens (file: `apps/desktop/tests/ipc.rs`)
- [ ] [Flow] `task schedule` with a `launchctl` stub and a temp `LaunchAgents` (file: `tests/schedule.rs`)
#### Frontend
- [ ] [Component] picker, ⌘K in frames, reload per tab and pane, Edit menu, Hire unsaved guard, badge text (hand-check, `pr-proof`)

Block merge: every unit, contract and integration test above; every ISA row. Block release: the
frontend hand-check and acceptance 1-9. Follow-up allowed: none.

### ISA row to test

Each row of design § ISA test matrix, with the test that proves it. Test function names are fixed
here so the merge gate is decidable.

| ISA row | Story | Test |
|---|---|---|
| `add` with a slug and existing directories | US-002 | `tests/business.rs::add_writes_entry` |
| `add` with `../x`, uppercase, existing name without `--replace` (and `--replace` overwrites) | US-002 | `tests/business.rs::add_refuses_bad_names_and_duplicates` |
| `add` with relative or missing path, or a home or workspace another business uses | US-002 | `tests/business.rs::add_refuses_bad_or_shared_dirs` |
| `switch_business` to unknown name, deleted workspace, workspace without profile, while pinned | US-008a | `apps/desktop/tests/ipc.rs::switch_refusals_keep_triple` |
| corrupt registry | US-004 | `tests/business_cli.rs::corrupt_registry_errors_but_explicit_and_internal_run` |
| `cos business use nope` | US-004 | `tests/business_cli.rs::use_unknown_refused` |
| `businesses()` with a dead supervisor in another home | US-008a | `apps/desktop/tests/ipc.rs::businesses_leaves_other_homes_untouched` |
| after a switch: `docs_save`, docs organize, `agents_spawn`, `agents_record` | US-008b | `apps/desktop/tests/ipc.rs::writes_land_in_new_business` |
| each of the seven write commands naming the previous business | US-008b | `apps/desktop/tests/ipc.rs::stale_business_writes_refused` |
| `agents::read/record/stop/clean/rejudge` with `""`, `.`, `..`, `../x`, `a/b`, other home's id | US-006 | `tests/agents.rs::run_dir_refuses_bad_ids` |
| app `agents_tail`/`agents_record`, CLI `show`/`kill`, MCP `stop_agent` with `../x` | US-006, US-008b | `tests/agents.rs::cli_and_mcp_refuse_escaping_ids`, `apps/desktop/tests/ipc.rs::app_run_commands_refuse_bad_ids` |
| `list` with a symlinked dir and a stale `spec.id` beside two good runs | US-006 | `tests/agents.rs::list_skips_refused_runs` |
| the same three on a `spec.json` naming another run | US-006 | `tests/agents.rs::spec_id_mismatch_refused` |
| `rejudge` on unedited worktree, scratch and in-place runs | US-006 | `tests/agents.rs::rejudge_each_run_kind` |
| `clean` on a worktree run whose tree was deleted | US-006 | `tests/agents.rs::clean_with_tree_gone` |
| `task schedule BUSINESS=nope` | US-010 | `tests/schedule.rs::unknown_business_changes_nothing` |

### Stories

Dependency graph (an arrow reads "needs"): roots US-000, US-001, US-002, US-006.
US-003 → US-001, US-002. US-004 → US-003. US-005 → US-003. US-007 → US-004, US-006.
US-010 → US-004. US-008a → US-004, US-005, US-007. US-008b → US-006, US-008a. US-009a → US-008b.
US-009b → US-009a. US-011 → US-009b, US-010.

File ownership for the parallel roots, so they never edit the same file: US-000 owns
`tests/flows.rs` and `.claude/skills/`; US-001 owns `src/pipeline.rs`, `src/track.rs`,
`src/context.rs`, `tests/workspace.rs` and the rename hunk of `src/main.rs:775-778`; US-002 owns
`src/business.rs`, `src/state.rs`, `src/agents/slots.rs`, `src/lib.rs` (module line),
`Cargo.toml`, `tests/business.rs`; US-006 owns `src/agents/mod.rs`, `src/agents/supervise.rs`,
`src/agents/judge.rs`, `src/dash.rs`, `src/mcp.rs`, the agents hunk of `src/main.rs:600-720`,
`apps/desktop/src/lib.rs` (agents handlers only), `tests/agents.rs`, `tests/fit.rs`,
`tests/hiring.rs`. US-001 and US-006 both touch `src/main.rs` in disjoint hunks; whichever lands
second rebases.

Sizes: every story below touches at most about a dozen files, under the one-agent threshold;
US-003, US-006, US-008 and US-009 list their files so a builder sees the whole change.

#### US-000: carry the stranded test fix and re-vendor the ritual skills [Chore]
As a maintainer, I want `tests/flows.rs` to pass on a machine with `commit.gpgsign=true`, and the
vendored ritual skills to match the installed copies, so the suite is green and the flow's own
instructions are current.

- [ ] Take only the `tests/flows.rs` git-helper hunk from `55e5129` (`.args(["-c", "commit.gpgsign=false"])` in `git()`, `tests/flows.rs:311-316`), not the commit; `src/ritual.rs` and `src/main.rs` are already on main
- [ ] Test: with a temp file `[commit]\n gpgsign = true` as `GIT_CONFIG_GLOBAL`, `cargo test --test flows the_design_is_read_from_the_flow_branch_while_it_exists` fails without the hunk and passes with it; both outputs appended to the bead
- [ ] `diff -r .claude/skills/<name> ~/.claude/skills/<name>` prints nothing for `ritual`, `ritual-full`, `semantic-isa`
- [ ] All tests green: `cargo test --test flows`

#### US-001: rename `home` parameters that receive the workspace [Backend]
As a maintainer, I want every parameter that receives the workspace named `workspace`, so a
business threaded through later lands on the right directory.

- [ ] Test: `tests/workspace.rs::config_reads_workspace_profile` drives `Config` with home ≠ workspace and asserts the workspace profile is read; passes before and after the rename
- [ ] The parameters at `pipeline.rs:119-120, 183-184`, `track.rs:58-72, 192`, `context.rs:72-74, 203-248`, `main.rs:775-778` are named `workspace`; `rg -n '\bhome\b' src/pipeline.rs src/track.rs src/context.rs` shows none of those lines except `denied_servers(home)` (`context.rs:247-248`), which reads `<home>/context/deny-mcp.txt` and keeps its name. The test is a characterisation test: it passes before and after the rename, so test-first means it is written and green before the rename starts
- [ ] All tests green: `cargo test --workspace`

#### US-002: registry (`src/business.rs`) — sink `STORE(registry)` [Backend]
As a person with several businesses, I want them named in `~/.config/cos/businesses.toml`, so every
cos process reads the same list.

- [ ] Test: `tests/business.rs::registry_round_trip`, `::missing_file_is_empty`, `::load_errors_name_file_and_key` (parse error, non-slug name, dangling `active`; file untouched), `::active_absent_is_no_active`
- [ ] Test: `::add_writes_entry`, `::add_refuses_bad_names_and_duplicates` (`../x`, uppercase, existing name without `--replace` refused, file byte-identical; `--replace` overwrites), `::add_refuses_bad_or_shared_dirs` (relative, missing, not a directory, home or workspace used by another business; file byte-identical)
- [ ] Test: `::unwritable_dir_leaves_file`, `::concurrent_adds_both_land` (50 rounds of two threads)
- [ ] `Registry`, `Business`; location `COS_BUSINESSES` → `$XDG_CONFIG_HOME/cos/businesses.toml` → `$HOME/.config/cos/businesses.toml`; the private `flock(file, nonblocking)` in `src/agents/slots.rs:44-47` becomes `pub(crate) fn lock_exclusive(path) -> Result<Held>`; writes lock `businesses.toml.lock`, re-read inside the lock, and write through a new `state::atomic_write_bytes` beside `atomic_write` (`state.rs:548-564`), which keeps its signature and callers
- [ ] `toml` 1.1.8 a direct dependency of `cos`: `cargo tree --locked -i toml@1.1.8 --workspace` lists `cos` as a direct dependent (the lockfile's other `toml`, 0.8.2 through `system-deps`, stays)
- [ ] All tests green: `cargo test --workspace`

#### US-003: one resolver for every command [Backend]
As a CLI user, I want `--business`, `COS_BUSINESS` or the active business to pick one whole triple,
so goals and repos never come from two businesses.

Files: `src/business.rs` (resolve), `src/main.rs` (Cli, consumers), `src/track.rs`, `src/mcp.rs`,
`src/dash.rs`, `src/serve.rs`/tunnel entry, `tests/business.rs`, `tests/business_cli.rs`.

- [ ] Test: `tests/business.rs::resolve_layers` (each layer alone), `::higher_layer_wins` (`--business` over `COS_HOME`, `--home` over `COS_BUSINESS`), `::in_layer_conflicts_error`, `::partial_env_fills_from_defaults` (`COS_HOME=/h` with an active business gives `(/h, /h, $HOME/Projects)`; `COS_PROJECTS=/p` gives `(<install>, <install>, /p)`), `::defaults_are_build_clone_and_projects`, `::explicit_triple_never_loads_registry` (loader panics if called), `::no_active_skips_layer_three`, `::unset_home_errors`
- [ ] Test: `tests/business_cli.rs::business_and_home_flags_conflict` (`cos --business acme --home /tmp report` exits non-zero naming both), `::flag_beats_env` (`COS_HOME=/tmp cos --business acme report --no-open` writes acme's report), `::state_init_needs_no_profile`, `::missing_state_hint_names_state_init`, `::long_lived_prints_business` (`cos dash --help`-free start: `cos mcp` prints `business: <name>` and the triple on stderr, then exits on closed stdin), `::mcp_follows_active_on_next_start` (start; rewrite the fixture registry's `active` directly; start again: the printed business changes)
- [ ] `resolve(inputs) -> Result<Triple>`; `Cli` directory fields `Option` without `default_value_os_t`; every triple read at `main.rs:330-332, 348-349, 370-371, 383-385, 392, 396, 401, 428, 512, 567-571, 843` goes through the one `Triple`; `agents supervise` and `agents build-slot` do not resolve; `rg -n 'default_value_os_t' src/main.rs` shows no home, workspace or projects field
- [ ] Missing-state errors at `main.rs:731, 777`, `track.rs:195`, `mcp.rs:185` name `cos --workspace <dir> state init`
- [ ] All tests green: `cargo test --workspace`

#### US-004: `cos business` commands and first-run migration — sinks `STORE(registry)`, `add` seeds `context/` [Backend]
As a CLI user, I want `cos business list | add | use | home | remove`, so I manage businesses
without editing TOML.

- [ ] Test: `tests/business_cli.rs::add_use_list_remove`, `::use_unknown_refused` (file byte-identical), `::remove_active_refused`, `::first_run_writes_default_from_build_clone` (`add acme …` on an empty registry writes `default` and `acme` with different homes, and prints that `default` was written), `::add_without_profile_hints_state_init` (succeeds, prints `cos --workspace <dir> state init`), `::bad_dir_message_names_business_add`
- [ ] Test: `::home_fails_when_projects_missing`, `::corrupt_registry_errors_but_explicit_and_internal_run` (`cos business list` errors with file and key; `cos --workspace <dir> state init` and `agents build-slot` succeed)
- [ ] Test: `::add_seeds_context` (`templates/context/` copied into a home without `context/`), `::add_keeps_existing_context` (`context/goals.md` unchanged)
- [ ] `list` prints `*` on the active business, directory and profile status, then `current: <name|explicit|default> <triple>`
- [ ] All tests green: `cargo test --workspace`

#### US-005: Claude runs from the install [Backend]
As a person running a second business, I want Claude to load cos's own `CLAUDE.md` and agents
whatever business is active, so a business home needs only `context/` and `out/`.

- [ ] Test: `tests/business_cli.rs::claude_runs_from_install` asserts `claude::command` for a home with no `CLAUDE.md` sets the working directory to the install
- [ ] `claude.rs:111-121` takes the install path; `context/` still inlined from the business home
- [ ] All tests green: `cargo test --workspace`

#### US-006: one run-id check for every agent command — sink `VERIFY(run_paths)` [Backend]
As a person whose agents run in several businesses, I want every run id to resolve through one
check, so no caller reaches a path outside the business's `out/agents`.

Files: `src/agents/mod.rs`, `src/agents/supervise.rs`, `src/agents/judge.rs`, `src/main.rs`
(agents subcommands), `src/dash.rs`, `src/mcp.rs`, `apps/desktop/src/lib.rs` (agents handlers),
`tests/agents.rs`, `tests/fit.rs`, `tests/hiring.rs`.

- [ ] Test: `tests/agents.rs::run_dir_refuses_bad_ids` (`read`, `record`, `stop`, `clean`, `rejudge` with `""`, `.`, `..`, `../x`, `a/b`, an id only in another home, a symlinked run dir, an unparseable `spec.json`: refused, every `state.json` byte-identical, no process killed, nothing removed), `::spec_id_mismatch_refused` (the other run's `state.json` byte-identical, its harness alive)
- [ ] Test: `::list_skips_refused_runs` (two good runs in `runs`, two ids in `skipped`; `cos agents clean` with no id cleans the good finished run, skips both, prints `skipped:` on stderr), `::watch_waits_on_skipped` (`cos agents watch` with one skipped id does not exit while the other runs are terminal; bounded by a 5 s test timeout)
- [ ] Test: `::cli_and_mcp_refuse_escaping_ids` (`cos agents show ../x`, `cos agents kill ../x`, MCP `stop_agent` with `../x`), `::rejudge_each_run_kind`, `::clean_with_tree_gone`
- [ ] `agents::run_dir(home, id) -> Result<RunDir>`, private fields, `path()`, `events_path()`; spawn and supervise keep a private path helper; `write_state` takes `&RunDir`; `list` returns `Listing { runs, skipped }`; callers at `apps/desktop/src/lib.rs:170, 441-444`, `main.rs:610-702`, `dash.rs:132`, `mcp.rs:217, 298-301`, `tests/fit.rs:260`, `tests/hiring.rs:415` updated
- [ ] `rg -n 'out/agents|join\("agents"\)' src apps --glob '!src/agents/**' --glob '!**/*.md'` prints nothing
- [ ] All tests green: `cargo test --workspace`

#### US-007: count runs without touching them, and spawn under the spawning triple [Backend]
As a person switching businesses, I want other businesses' runs counted read-only and every agent
to keep its own business, so the count never kills anything and `cos` inside an agent resolves
the business that spawned it.

- [ ] Test: `tests/agents.rs::classify_matches_reconcile` (existing fixtures), `::count_is_read_only` (a dead supervisor's run record byte-identical; an unreadable record counted `unreadable`)
- [ ] Test: `::spawn_pins_triple_in_env` (parent with `COS_BUSINESS` or `COS_WORKSPACE` set; the run reaches `running`; the harness's `cos business list` prints `current: explicit <spawning triple>`)
- [ ] `reconcile` = `classify` then repair; `count(home) -> { live, lost, unreadable }`; spawn removes `COS_BUSINESS`, sets `COS_HOME`, `COS_WORKSPACE`, `COS_PROJECTS`, keeps `COS_AGENT_SLOTS`
- [ ] All tests green: `cargo test --workspace`

#### US-008a: desktop backend: app business, startup, switch — sink `TOOL_CALL(switch_business)` [Backend]
As an app user, I want the window to hold one business in memory and switch it safely, so a
switch never leaves half a business behind.

Files: `apps/desktop/src/settings.rs`, `apps/desktop/src/lib.rs` (state, startup, switch,
businesses, paths), `apps/desktop/tests/ipc.rs`.

- [ ] Test: `apps/desktop/tests/ipc.rs` `Settings` literals (`ipc.rs:107-113, 295-301, 441-447`) moved to the new fields; the builder takes a registry path and env values; `::switch_changes_paths_report_flows`, `::switch_refusals_keep_triple` (unknown name, deleted workspace, workspace without `profile.json` with the `state init` message, pinned by `COS_HOME`)
- [ ] Test: `::businesses_leaves_other_homes_untouched`, `::businesses_marks_missing_home`, `::first_start_writes_default_and_hints_legacy` (different legacy triple returns the `cos business add` hint), `::corrupt_registry_runs_on_env_or_defaults` (error surfaced, picker gets the error), `::corrupt_settings_reported` (header error, not dropped), `::settings_never_writes_triple` (after a switch `settings.json` has no `home`, `workspace`, `projects`)
- [ ] Test: `::businesses_under_200ms` (10 fixture businesses × 50 run records each; `businesses()` under 200 ms; result recorded in the bead)
- [ ] `Mutex<AppState { settings, triple }>`; one snapshot per command; `rg -n 'set_workspace|built_home' apps/desktop` prints nothing; `cos_binary()` resolved once from the install
- [ ] All tests green: `cargo test --workspace`

#### US-008b: desktop backend: write-business guard on every write — sink `VERIFY(write_business)` [Backend]
As an app user, I want every write to name the business it was issued for, so a request sent
before a switch never lands in the new business.

- [ ] Test: `::writes_land_in_new_business` (`docs_save`, docs organize, `agents_spawn`, `agents_record` after a switch), `::stale_business_writes_refused` (each of the seven write commands naming the previous business: nothing written, run or killed in either business), `::docs_post_checks_business_header` (missing or mismatched `X-Cos-Business` refused)
- [ ] Test: `::app_run_commands_refuse_bad_ids` (`agents_rejudge`, `agents_stop`, `agents_clean`, `agents_record`, `agents_tail` with `""`, `.`, `..`, `../x`, `a/b`, another business's id, and a `spec.json` naming another run), `::run_spawned_before_switch_survives` (spawn in `default`, switch to `acme`, the run keeps running, and `agents_list` after switching back lists it)
- [ ] The seven write commands: `docs_save` (`lib.rs:151`), the docs POST through `cos::docs::handle` (`lib.rs:114-127`, which includes organize), `agents_spawn` (`:363`), `agents_record` (`:405`), `agents_rejudge` (`:213`), `agents_stop` (`:423`), `agents_clean` (`:429`). The six invoke commands take `business`; the docs POST is checked on its `X-Cos-Business` header before `cos::docs::handle` runs; `paths()` returns `business`
- [ ] All tests green: `cargo test --workspace`

#### US-009a: desktop UI: picker, ⌘K and reload [UI]
As an app user, I want ⌘K to open a searchable list of businesses from anywhere in the window, so
switching is one keystroke, a few letters and Enter.

Files: `apps/desktop/ui/command.js` (new), `apps/desktop/ui/app.js`, `apps/desktop/ui/index.html`,
`apps/desktop/ui/style.css`, `apps/desktop/src/lib.rs` (menu), the docs frame's script.

- [ ] Test: `::paths_and_businesses_shape` (contract: `businesses()` rows `{ name, triple, active, app, dirs, live, lost, unreadable }` or `missing`; `paths()` has `business`)
- [ ] `command.js`: filter by name then home, arrows, Enter (no-op on empty), Esc returns focus; names via `textContent`; header button shows the app business
- [ ] App menu "Switch business…" ⌘K / Ctrl+K; predefined Edit and Window items kept
- [ ] `openTab(tab, pane)` extracted from `app.js:134-152`; click handler and startup `#tab=&pane=` reader both call it; switch writes the hash and reloads; Hire dialog with unsaved edits asks first
- [ ] Every write call passes `business` read once from `paths()`; the docs frame sends `X-Cos-Business` from its URL on every POST; `rg -n "invoke\('(docs_save|agents_spawn|agents_record|agents_rejudge|agents_stop|agents_clean)'" apps/desktop/ui` shows `business` on every match, and `rg -n "method: *'POST'" apps/desktop/ui` and the docs frame's script show `X-Cos-Business` on every docs POST
- [ ] Hand-check (recorded for `pr-proof`): ⌘K with focus in the app, inside the report frame, inside the docs frame; type, arrows, Enter reloads onto the same tab and pane for Today, Agents (Runs, Staff), Flows, Docs; Esc returns focus; Enter on an empty list does nothing; copy and paste in the Docs editor and the Hire dialog; Hire with unsaved edits asks before reload; app started with `COS_HOME` shows `pinned by COS_HOME` and refuses
- [ ] All tests green: `cargo test --workspace`

#### US-009b: desktop UI: other-business badge [UI]
As an app user, I want to see when agents run in other businesses, so I can switch to them.

- [ ] Agents header shows `N live in other businesses` when N > 0, computed on picker open and Agents tab open; click opens the picker; picker rows show `live` and non-zero `lost`
- [ ] Hand-check (recorded for `pr-proof`): with one run live in `default` and the app on `acme`, the header reads `1 live in other businesses`, the click opens the picker, and the `default` row reads `1 live`
- [ ] If `::businesses_under_200ms` failed in US-008a, the badge computes on Agents tab open only (design § Accepted trade-offs) and the PR body says so
- [ ] All tests green: `cargo test --workspace`

#### US-010: launchd jobs bound to one business — sink `task schedule` [Integration]
As a person running several businesses, I want `task schedule BUSINESS=<name>` to bake that
business into the brief and dashboard jobs, so the scheduled brief runs where I expect.

- [ ] `Taskfile.yml`: new vars `LAUNCHCTL` (default `launchctl`) and `COS` (default `cos`); `schedule` and `unload-retired` call `{{.LAUNCHCTL}}` and `{{.COS}}`; `BUSINESS` defaults to the active business, read from the `current: <name> …` line `cos business list` prints (design § CLI; with no flag or env that line names the active business), and an empty or `explicit`/`default` name stops the task asking for `BUSINESS=`; first step `{{.COS}} --business {{.BUSINESS}} business home` stops the task on non-zero; `HOME_DIR` is a task var from `sh: {{.COS}} --business {{.BUSINESS}} business home`; `mkdir -p {{.HOME_DIR}}/out/logs` and `unload-retired` run after the check
- [ ] `launchd/cos.brief.plist`, `launchd/cos.dash.plist`: `ProgramArguments` gain `--business @BUSINESS@`; log paths use `@HOME_DIR@`; `COS_HOME` removed
- [ ] `.github/workflows/ci.yml` installs go-task (`arduino/setup-task@v2`, pinned version) before `cargo test`
- [ ] Test: `tests/schedule.rs` runs `task schedule` with `LAUNCHD=<tmp>`, `LAUNCHCTL=<tmp stub that appends its args to a log>`, `COS=<CARGO_BIN_EXE_cos>`, `COS_BUSINESSES=<tmp>`; it never touches the real launchd domain. `::acme_plists_carry_business` (`--business acme`, logs under acme's home, no `COS_HOME`, stub log shows `bootstrap`), `::default_business_is_active`, `::unknown_business_changes_nothing` (`BUSINESS=nope`: `out/logs` absent, every plist in the temp dir byte-identical, stub log empty), `::old_plist_with_cos_home_resolves_explicit` (a plist still setting `COS_HOME`: its command resolves `current: explicit`)
- [ ] All tests green: `cargo test --workspace`

#### US-011: docs [Docs]
As a reader, I want the docs to describe businesses, so a new install sets one up without reading
the design.

- [ ] `CLAUDE.md:31`, `docs/usage.md:458`, `apps/desktop/README.md:8`, the README's usage section, `Taskfile.yml`'s `schedule` description updated
- [ ] `rg -n -- '--business|cos business|BUSINESS=' CLAUDE.md docs/usage.md apps/desktop/README.md README.md Taskfile.yml` matches in each of the five files
- [ ] The docs index links the brief, design, plan and final review
- [ ] All tests green: `task test`

### Functional requirements
- FR-1: The registry names businesses and the active one; only `cos business` and first-run migration write it.
- FR-2: Every command's triple comes whole from one layer: flags, env, registry, built-in defaults.
- FR-3: The app holds its business in memory, switches through ⌘K, and refuses writes meant for another business.
- FR-4: Every run id reaches the filesystem only through `agents::run_dir`.
- FR-5: Each agent run keeps the triple that spawned it.
- FR-6: `task schedule` binds the launchd jobs to one named business, testable without touching launchd.

### Non-goals
- Per-business ports or launchd labels; a dashboard or MCP server that follows the app's switch; per-business credentials (design § Goals and non-goals).
- Tamper checks on run-record fields (design § Accepted trade-offs, the user's decision 2026-11-12).

### Open questions
- None. Design line references drift as the code moves; Build uses the current file.
