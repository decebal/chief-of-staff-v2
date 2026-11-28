# Hire skill graph: plan

Design: docs/plans/2026-11-28-hire-skill-graph-design.md (approved, sha256
fd312716ec6f812084cdea3a7124484810e0ca905f6ed5a8b8f1d8524c55e33c). Epic
`t-6036dd`, branch `feat/hire-skill-graph`. Section numbers (§1 to §5) and
sink names (S1 to S8) are the design's.

### Quality gates

#### Epic-level (once, when every story is done)
- `task test` (fmt, clippy `-D warnings`, every cargo test, the kit's Python
  tests) passes.
- `task desktop-check` passes.
- Every row of the design's ISA test matrix is a named, passing test.

#### Story-level
- Backend stories: the story's own test targets (`cargo test --test <file>`),
  one build at a time, each under five minutes.
- Desktop stories: `cargo test --manifest-path apps/desktop/Cargo.toml --test
  ipc` and `cargo test --test design`.

### Testing trophy

#### Unit
- [ ] [Logic] resolve: role picks, add, remove, `requires` closure, cycles,
  remove-vs-requires warning, missing skill, duplicate roots, hidden set,
  unreviewed on/off, project skills win (file: tests/skills.rs)
- [ ] [Logic] fold: newest valid fact wins, a newest-invalid fact does not
  shadow an older valid one (file: tests/skills.rs)
- [ ] [Logic] tree digest: stable order, inner symlink refused, 20 MiB cap
  (file: tests/skills.rs)
- [ ] [Logic] VERIFY-U parsing: literal loopback IPs and `localhost` only
  (file: tests/prime.rs)

#### Contract
- [ ] [Handler] stand-in Prime: `prime_search`, `prime_add_node`,
  `prime_add_edge` request and reply shapes match the live server's (file:
  tests/common/prime.rs, checked once against the live server in US-011)
- [ ] [Handler] `skills_catalog` and `skills_set` Tauri commands return and
  accept the shapes the panel uses (file: apps/desktop/tests/ipc.rs)
- [ ] [Handler] `Plan` JSON carries `skills` for the dialog and `--dry-run`
  (file: tests/hiring.rs)

#### Integration
- [ ] [Flow] S1, S6: Prime client against the stand-in: refusal, deadline,
  size cap, non-200, JSON-RPC error, 30 s reuse after success and failure,
  single flight, temp-and-rename (file: tests/prime.rs)
- [ ] [Flow] S2: `cos skills set` refusals and the admitted write with
  `source` (file: tests/skills_cli.rs)
- [ ] [Flow] S3, S4, S7: a Claude hire end to end on the fake `hc`: bundle,
  digests, `--plugin-dir`, merged `--settings`, hidden-only set, second
  `prepare`, project skill, `spec.json` and `state.json` (file:
  tests/hiring.rs)
- [ ] [Flow] S5: a Codex hire's prompt block and no `--plugin-dir` (file:
  tests/hiring.rs)
- [ ] [Flow] Prime down: hire continues stale or none; curation write exits 1
  (file: tests/prime.rs, tests/skills_cli.rs)

#### Frontend
- [ ] [Component] Skills panel: groups, counts, Relevant / Irrelevant, roles,
  "written outside cos" mark, unreachable state with re-fetch (file:
  apps/desktop/tests/ipc.rs, tests/design.rs)
- [ ] [Component] S8: markup in a description or note renders as text (file:
  apps/desktop/tests/ipc.rs, the `el()` gate in tests/design.rs)
- [ ] [Component] hire dialog Skills section and run page row (file:
  apps/desktop/tests/ipc.rs)

Block merge: every test above. Block release: the US-011 hand-checks. Follow-up
allowed: none.

### Stories

#### US-001: Policy carries the skills settings [Schema]
As the user, I want `context/agents.json` to hold skill roots and the
unreviewed default, so that an old file and a new one both load with their
roles.

- [ ] Test: a policy with `skills: {roots, unreviewed}` and one without both
  load with roles intact; an unknown key under `skills` drops to defaults with
  a note (file: tests/hiring.rs), written first and failing
- [ ] `Policy.skills: Option<SkillsPolicy>` with `deny_unknown_fields`;
  `roots` must be absolute existing directories (a bad entry is ignored with a
  note); `unreviewed` is `on` | `off`, default `on`
- [ ] All tests green: `cargo test --test hiring`

#### US-002: Prime client on loopback [Backend]
As cos, I want a bounded JSON-RPC client for Prime, so that reading and writing
skill facts cannot hang a hire or reach off the machine.

- [ ] Test: S1 rows, deadline, 8 MiB cap, non-200, JSON-RPC error, no
  `Content-Length` (file: tests/prime.rs) against the stand-in
  (tests/common/prime.rs), written first and failing
- [ ] `src/prime.rs` `Prime::from_env` (VERIFY-U, `COS_PRIME_URL`, no
  resolver) and `Prime::call`, one deadline per call, `std::net` only
- [ ] All tests green: `cargo test --test prime`

#### US-003: Catalog, tree digest and resolve [Backend]
As cos, I want the local skill catalog and a pure resolver, so that a hire's
skill set comes only from skills on the machine.

- [ ] Test: unit rows for resolve, fold-free catalog scan (three roots,
  symlinked entries, broken `SKILL.md`, names failing the regex skipped), tree
  digest (file: tests/skills.rs), written first and failing
- [ ] `src/agents/skills.rs`: `catalog(roots)`, `tree_digest(dir)`,
  `resolve(graph, catalog, role, asked, unreviewed, project_skills)`
  returning `SkillSet` (picked with why, hidden, unreviewed, warnings)
- [ ] All tests green: `cargo test --test skills`

#### US-004: Skill facts in Prime: read, fold, cache, write [Backend]
As cos, I want to read and write skill facts with VERIFY-R and VERIFY-W, so
that the graph is current when Prime answers and usable when it does not.

- [ ] Test: S2 write shapes (node then edges, `source: "cos-skills"`), S6
  rows, fold with invalid newest, 30 s reuse after success and after failure,
  ten concurrent reads open one connection, `--dry-run` reads the file only
  (file: tests/prime.rs, tests/skills.rs), written first and failing
- [ ] `read_graph(home, prime, mode)`: VERIFY-R per fact, fold, `graph.json`
  with `read_at` / `failed_at` and each fact's node id, unique temp and
  rename, process-wide `try_lock` single flight, `Dry` mode never calls or
  writes
- [ ] `write_fact(prime, catalog, policy, fact)`: VERIFY-W, then edges to the
  newest target node ids; an edge failure is a warning
- [ ] All tests green: `cargo test --test prime --test skills`

#### US-005: Plan, request and spec carry the skill set [Backend]
As the user, I want every entry point to show and pass the skill set, so that
what I see before hiring is what the hire runs with.

- [ ] Test: `Plan.skills` from a role; `--skill` / `--no-skill`; desktop
  `SpawnArgs.skills`; MCP `skills_add` / `skills_remove`; an old `spec.json`
  without `skills` reads; S7 rows; a Codex request's prompt carries the S5
  block, a Claude one does not (file: tests/hiring.rs, tests/fit.rs), written
  first and failing
- [ ] `hire::plan(…, asked_skills)`; its callers (`src/main.rs`,
  `src/mcp.rs`, `apps/desktop/src/lib.rs`) and the three call sites in
  `tests/fit.rs` updated; `Request.skills` and `Spec.skills` (serde default);
  the Codex block built in `Plan::request` with VERIFY-P; one graph read
  serves every ask of `--all`
- [ ] Plan warnings: unreviewed count with `cos skills curate`, stale or
  missing graph, "Codex still sees every global skill", "installed plugins
  still load", a picked skill the repo holds
- [ ] All tests green: `cargo test --test hiring --test fit`

#### US-006: Bundle the set and start Claude with it [Backend]
As the user, I want a Claude hire to load exactly its skill set, so that
irrelevant global skills are out of its context and nothing touches its
worktree.

- [ ] Test: S3 and S4 rows; bundle under one deadline sharing the seed's
  five-minute ceiling; no `skills.tmp` left; second `prepare` rebuilds;
  hidden-only set gets `skillOverrides` without `--plugin-dir`; project skill
  read from the seeded worktree is neither bundled nor hidden; worktree
  `git status --porcelain` empty; digest mismatch fails before `hc run`
  (file: tests/hiring.rs), written first and failing
- [ ] `prepare` bundle step per §4; `seed.rs` `clone_cmd` `pub(crate)` on
  both platforms; `state.skills_bundled`; `harness::argv(spec, bundle)` with
  `skillOverrides` merged when `hidden` is non-empty and `--plugin-dir` when
  bundled
- [ ] All tests green: `cargo test --test hiring --test agents`

#### US-007: `cos skills` on the command line [Backend]
As the user, I want to list, show, set and curate skill facts from a
terminal, so that I can say which global skills matter and for which roles.

- [ ] Test: `ls` (with "written outside cos" mark) and `ls --json`; `show
  --role`; `set` S2 rows including `COS_RUN_ID`; Prime-down `set` exits 1
  naming the URL; `curate` refuses without a terminal naming `cos skills
  set` (file: tests/skills_cli.rs), written first and failing
- [ ] `cos skills ls|show|set|curate` per §5; `curate` writes each answer
  before showing the next skill
- [ ] All tests green: `cargo test --test skills_cli`

#### US-008: Skills panel in the Agents tab [UI]
As the user, I want to curate skills in the window, so that I can mark
relevance and roles without the terminal.

- [ ] Test: contract for `skills_catalog` and `skills_set` (business check);
  panel groups and counts; marking irrelevant moves the row and the next
  `agents_fit` hides the skill; S8 markup renders as text; Prime unreachable
  shows a retry that re-fetches (file: apps/desktop/tests/ipc.rs), written
  first and failing
- [ ] Tauri commands `skills_catalog`, `skills_set`; panel in
  `apps/desktop/ui/app.js` through `stateView`, `el()` and tokens only
- [ ] All tests green: `cargo test --manifest-path apps/desktop/Cargo.toml
  --test ipc` and `cargo test --test design`

#### US-009: Skills in the hire dialog and on the run page [UI]
As the user, I want to see and change a hire's skills before it starts and
see them afterwards, so that each hire's context is my decision.

- [ ] Test: the dialog's `agents_fit` call carries `skills.add` / `remove`
  and the plan's picked list renders with reasons; removing one re-plans; the
  run page shows names and short digests from `spec.json` (file:
  apps/desktop/tests/ipc.rs), written first and failing
- [ ] Skills section under the contract in the hire dialog; folded Skills row
  on the run page
- [ ] All tests green: `cargo test --manifest-path apps/desktop/Cargo.toml
  --test ipc` and `cargo test --test design`

#### US-010: Codex skill switch spike [Spike]
As cos, I want to know whether Codex can hide a skill for one invocation, so
that Codex hires get the same treatment or an honest warning.

- [ ] In a temp `HOME` with a stand-in skill under `.agents/skills`, run
  `codex exec` with and without `-c 'skills.config=[{name="<x>",enabled=false}]'`
  and record whether the skill is listed; record whether `workspace-write`
  reads a file outside the workspace (A2, A6)
- [ ] Result written to `docs/reports/2026-11-28-codex-skill-switch-spike.md`
  as a classification (works / does not / partial) with the commands and
  output; if it works, a follow-up bead under this epic adds the Codex
  VERIFY-A row and argv
- [ ] Indexed in README

#### US-011: Live checks against real Claude and Prime [Integration]
As the user, I want the documented assumptions checked against the real
tools, so that the tests' stand-ins match what runs.

- [ ] `claude -p` started with `--plugin-dir` on a bundle outside its working
  directory and a `skillOverrides` setting lists `cos-hire:<x>` and not the
  hidden skill; with Claude's sandbox settings the session reads the bundle
  (A1); output in the PR evidence
- [ ] A read-only `prime_search {"type":"cos_skill"}` against the live server
  returns the shape the stand-in serves (A4); a read-only `tools/list` shows
  `prime_add_node` and `prime_add_edge` input schemas and descriptions that
  match what the stand-in accepts and returns (A5); no write is made to the
  live store
- [ ] Results recorded in the PR's evidence report

#### US-012: Docs follow the feature [Docs]
As a reader, I want the README, `docs/usage.md` and `CLAUDE.md` layout to
describe skill graphs, so that the next session finds them.

- [ ] README user guide: `cos skills`, `--skill` / `--no-skill`, the Skills
  panel, the Prime limit stated in the design
- [ ] `CLAUDE.md` layout rows for `src/prime.rs` and `src/agents/skills.rs`
- [ ] `docs/design.md` names the skill graph and links the design

### Functional requirements
- FR-1: A hire's skill set is the role's relevant skills, plus the hire's
  additions, minus its removals, closed over `requires`.
- FR-2: A Claude hire loads its set from a session plugin outside its
  worktree and does not see irrelevant user-level skills.
- FR-3: A hire never waits more than one Prime deadline per 30 s per process
  and starts whether or not Prime answers.
- FR-4: Only names come from Prime; every path comes from the local roots.
- FR-5: The user curates skill facts from the CLI and the window; cos never
  writes them on its own.

### Non-goals
- Hiding skills from installed plugins (warned in the plan).
- Bundling or hiding a repo's own `.claude/skills`.
- Stopping a hire from writing Prime through its own MCP tools (the stated
  limit).

### Open questions
- None blocking Build. The Codex result (US-010) may add one bead.
