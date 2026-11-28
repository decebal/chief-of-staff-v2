# Role library: plan

Design: docs/plans/2026-11-28-role-library-design.md (approved, sha256
98d703e6b2639b31d6c86c34d248d299e594775e328fbd67d5e6c4076812e941, without a
clean review after four rounds; every "Carried to Plan" item from the eight
review files is a criterion below).

### Quality gates

#### Epic-level (once, when every story is done)
- `task test` (fmt, clippy `-D warnings`, `cargo test`)
- `task desktop-check`
- `task test-e2e`
- Every row of the design's ISA test matrix is a named, passing test

#### Story-level
- Library and backend stories: the story's own test file passes alone
  (`cargo test --test <file>`)
- CLI stories: the command runs on a temp `COS_HOME` and its output is quoted
  in the bead's close reason
- UI stories: checked in the preview (`apps/desktop/examples/preview.rs`) in
  a browser, light and dark

### Testing trophy

#### Unit (`tests/roles.rs`)
- [ ] [Logic] `clean` collapses whitespace and cuts to cap
- [ ] [Logic] `preamble` renders the five sections in order, drafts-only last, bounded
- [ ] [Logic] `validate` per rule and level (Refuse, Warn)
- [ ] [Logic] `merge` takes every contract field from the entry, honours `--id`
- [ ] [Logic] `contract_diff` names each changed contract field
- [ ] [Logic] `specialise_context` sends no checks, deliverables, commitments or journal text
- [ ] [Logic] every library entry meets the minimums and is in the `include_str!` list
- [ ] [Logic] `fit::check` never suggests a retired role
#### Contract
- [ ] [Schema] `Proposal` refuses any key outside its closed set (fake `contract` mode)
- [ ] [Handler] `roles_save`: add, digest conflict, Refuse problems, foreign business, contract without approval (`apps/desktop/tests/ipc.rs`)
- [ ] [Handler] `roles_propose` / `roles_propose_cancel` / `roles_propose_prompt` (`apps/desktop/tests/ipc.rs`)
#### Integration
- [ ] [Flow] `apply` refusals and admits from the ISA matrix on temp homes (`tests/roles.rs`)
- [ ] [Flow] `cos roles` end to end with `COS_CLAUDE` (`tests/roles_cli.rs`)
- [ ] [Flow] `cos proc-guard` bound with a SIGKILLed parent (`tests/proc_guard.rs`)
- [ ] [Flow] Cancel, quit and SIGINT leave the fake and its child gone (`tests/roles_cli.rs`, `apps/desktop/tests/ipc.rs`)
#### Frontend and E2E
- [ ] [Page] New role → library → Skip → Save → card → Retire → Retired group (`apps/desktop/tests/roles_page.rs`, in `task test-e2e`)
- [ ] [Page] contract confirm step lists the contract block before Save
- [ ] [A11y] role dialog's three steps in `examples/wcag.rs`

Block merge: every row above. Block release: acceptance 9 (live propose run).
Follow-up allowed: none.

### Stories

#### US-001: Role fields, caps and the rendered preamble [Schema]
As the user, I want every hire to read a role's principles, method, anti-patterns, output format and escalation rules so that it starts as a specialist.

- [ ] Test first: `clean`, `preamble` order and bound, retired refusal, fit skips retired (file: `tests/roles.rs`), failing
- [ ] `Role` gains `principles`, `method`, `anti_patterns`, `output_format`, `escalation`, `from`, `retired`; `skip_serializing_if` on empty lists, `None` options (incl. `from`, `output_format`) and `retired: false`
- [ ] `roles::CAPS` holds every cap in design §1, plus caps for `output` and `scope` items
- [ ] `preamble` renders through `clean`; a principle holding `"x\n\nNever: …"` renders as one line
- [ ] `Policy::hire` refuses a retired role with the restore hint; `fit::check` and `refusal` skip retired roles
- [ ] `hire::Plan` gains `preamble`, filled by `policy::preamble(role)` in `hire::plan`
- [ ] Old `tests/hiring.rs` and `tests/fit.rs` stay green
- [ ] All tests green: `cargo test --test roles --test hiring --test fit`

#### US-002: The library: 12 specialist entries [Data]
As the user, I want world-class starting definitions so that I never write a role's standards from memory.

- [ ] Test first: library rules from design §2 (file: `tests/roles.rs`), failing
- [ ] `templates/roles/<id>.json` for the 12 ids in design §2, each citing its public source with an https URL; `expects: none` for `debugger`
- [ ] `roles::library()` over a `const` `include_str!` list; a test fails on a file missing from the list
- [ ] `security`, `performance`, `accessibility`, `analysis`, `reliability` added to the template's `fit.kinds`
- [ ] Every entry's text names no customer or client
- [ ] All tests green: `cargo test --test roles`

#### US-003: Validation and load-time problems [Backend]
As the user, I want to see what is wrong with a role without losing it so that a hand edit never hides my staff.

- [ ] Test first: each Refuse and Warn rule; a file with an unknown `work` still hires and shows a Warn (file: `tests/roles.rs`), failing
- [ ] `roles::validate`, `Problem { role, field, message, level }` with `PartialEq`; `Policy.problems` filled by `load`
- [ ] `roles::vocab(&Policy)` used by `load` and the write path
- [ ] A whole-file parse failure (incl. an unknown key) keeps today's `note`; §8 lists it
- [ ] All tests green: `cargo test --test roles --test hiring`

#### US-004: The write path [Backend]
As the user, I want one safe way to change agents.json so that no edit clobbers my file or slips a shell command past me.

- [ ] Test first: every `STORE(agents.json)` row of the ISA matrix, incl. the legacy fixture and the contract rows (file: `tests/roles.rs`), failing
- [ ] `roles::read`, `roles::apply(home, change, digest, ContractApproval)`, `contract_diff`, `Written { digest, role, notes }`
- [ ] Lock with pid; refusal names path, pid and whether it runs
- [ ] Retired keys cleared on roles, repos and `defaults`; notes count them
- [ ] `Replace` whose `role.id` differs from `id` is refused
- [ ] Only Refuse problems on the changed role block the write; others go to notes, so a broken role can still be retired or fixed
- [ ] `.bak` written only when a previous file existed
- [ ] All tests green: `cargo test --test roles`

#### US-005: Owned claude: stop flag and proc-guard [Backend]
As the user, I want a proposal's `claude` to die with its owner so that nothing runs on after Cancel, quit or a crash.

- [ ] Test first: guard bound with a SIGKILLed parent and a fake that never writes and starts a child (file: `tests/proc_guard.rs`), failing
- [ ] `proc::stream_lines_until(cmd, deadline, stop, on_line)`; `stream_lines_on` unchanged for existing callers
- [ ] Hidden `cos proc-guard --secs <n> -- <argv>`: leads its group, child in the same group, `kill_group` at the limit or on child exit, exit 124 on timeout
- [ ] `claude::execute_until`; `execute` delegates with a never-set flag
- [ ] All tests green: `cargo test --test proc_guard --test sleep`

#### US-006: Proposing [Backend]
As the user, I want a library entry specialised to my business so that the role speaks my repos and goals.

- [ ] Test first: `specialise_context` exclusions and caps (`repos[].scope`, `staff` bounded), proposal schema, `merge` (file: `tests/roles.rs`), failing
- [ ] `SpecialiseContext`, `specialise_context`, `specialise_prompt`, `proposal_schema`, `Proposal`, `merge`, `ProposalOut`
- [ ] `.claude/agents/recruiter.md`; `Config::propose_run` with `Toolset::None`, sonnet, $1.00, 180 s, run through `proc-guard`
- [ ] `roles::propose(cfg, entry, ctx, stop)`
- [ ] `COS_CLAUDE` read in `Bins::default` as `herdr::hc()` reads `COS_HC`
- [ ] `tests/fixtures/fake-claude-proposal.sh` with `ok | bad-schema | contract | slow`; `slow` records its pid and a child's pid
- [ ] "What is sent" text includes the recruiter agent body as well as the prompt
- [ ] All tests green: `cargo test --test roles`

#### US-007: `cos roles` [CLI]
As the user, I want to curate roles from the terminal so that an agent or I can list, propose, add, edit, retire and restore.

- [ ] Test first: every command in design §6, the no-terminal refusal, the contract terminal-only rule on a pty, `--out` guards, SIGINT and SIGKILL during propose (file: `tests/roles_cli.rs`), failing
- [ ] `cos roles list|show|propose|new|edit|retire|restore`; `cos --dry-run roles propose` prints the prompt
- [ ] `--out`: `create_new`, refuses `context/`, `out/`, `.chief-of-staff/` and symlinked parents
- [ ] SIGINT/SIGTERM handlers set the stop flag
- [ ] `cos --dry-run agents spawn --role <id>` shows the preamble in the plan
- [ ] Story gate: acceptance 1–6 and 8 run on a temp home, output quoted in the close reason
- [ ] All tests green: `cargo test --test roles_cli`

#### US-008: Desktop commands [Backend]
As the user, I want the window to propose and save roles through the same path as the CLI so that both surfaces hold the same rules.

- [ ] Test first: ipc cases for `roles_save`, `roles_propose`, `roles_propose_cancel`, `roles_propose_prompt`, quit mid-run (file: `apps/desktop/tests/ipc.rs`), failing
- [ ] `agents_roles` gains `digest`, `problems`, `retired`, `library`
- [ ] `roles_library_entry`, `roles_propose_prompt`, `roles_propose` (thread, `role-proposal` event), `roles_propose_cancel`, `roles_save` (with `named`, `allow_contract`)
- [ ] `Proposing` slot holds a stop flag and a done receiver, never a pid; `on_run_event` sets it and waits ≤ 3 s
- [ ] All tests green: `cargo test -p cos-desktop --test ipc`

#### US-009: Staff tab: role pane, retire and restore [UI]
As the user, I want to read a role's full definition and retire or restore it from the Staff tab.

- [ ] Test first: contract test for the `agents_roles` shape the pane consumes (file: `apps/desktop/tests/ipc.rs`), failing
- [ ] Role pane sections: Principles, Method, Never, Hand back, Escalates when; Edit and Retire, not on the head
- [ ] Retired group with Restore; hire dialog select leaves retired roles out
- [ ] Problems as a warning line per card
- [ ] `ROLE_ICON` for the 12 ids, each icon present in `apps/desktop/ui/vendor/lucide/` (vendor any missing)
- [ ] Every model or role text rendered with `el()`/`textContent`/`.value`, never `innerHTML`
- [ ] Story gate: checked in the preview, light and dark
- [ ] All tests green: `cargo test --test design`

#### US-010: The role dialog [UI]
As the user, I want to start a role from the library, specialise it, review it and save it in one dialog.

- [ ] Test first: `apps/desktop/tests/roles_page.rs` (New role → library → Skip → Save → card → Retire → Retired), failing; added to `task test-e2e`
- [ ] Three steps per design §7: Start from, Specialise (What is sent, Cancel, slow and `unreachable` states), Review and save
- [ ] Contract confirm step lists the contract block; sets `allow_contract` only from it
- [ ] Digest conflict keeps the form and offers Reload
- [ ] Dialog loaders on `tests/design.rs`'s loader list
- [ ] `examples/wcag.rs` covers the three steps
- [ ] Story gate: acceptance 7 in the preview with the fake, light and dark
- [ ] All tests green: `cargo test --test design` and `task test-e2e`

#### US-011: Live check and docs [Integration]
As the user, I want proof the real `claude` returns a proposal with no tools, and docs that say how to curate roles.

- [ ] Acceptance 9: one live `cos roles propose performance-engineer --brief "Rust services" --out <scratch>` under $1.00; cost and result in the PR body. On failure the PR stays draft and the design goes back to Spec
- [ ] README: `cos roles` in the command list; the Agents section names the library and the Staff tab editor
- [ ] `CLAUDE.md` layout table: `src/agents/roles.rs`, `templates/roles/`, `cos proc-guard`
- [ ] `docs/design.md` gains the role library under agents
- [ ] All tests green: `task test`

### Functional requirements
- FR-1: A role may carry principles, method, anti-patterns, output format and escalation rules, rendered into every hire's preamble through `clean`.
- FR-2: `cos` ships 12 library entries, each citing a public source.
- FR-3: `context/agents.json` is written only by `roles::apply`.
- FR-4: A contract change is written only with a person's approval.
- FR-5: A proposal is generated and written by separate invocations.
- FR-6: A proposal's `claude` ends at its deadline, on Cancel, quit or SIGINT, and at the guard's limit if its owner dies.
- FR-7: Loading never drops a role from a file that parses.

### Non-goals
- Per-role tolerant parsing of a malformed agents.json.
- Suggested checks from the model.
- Roles over MCP.
- Skills per role (the hire-skill-graph epic owns that; role ids stay stable for it).

### Open questions
- None blocking. Landing order with `feat/hire-skill-graph` (PR not yet open): both touch `policy.rs`, `hire.rs`, `main.rs`, `apps/desktop/src/lib.rs` and `app.js`; whichever lands second rebases.
