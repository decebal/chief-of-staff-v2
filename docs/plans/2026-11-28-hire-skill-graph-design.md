# Hire skill graph: design

Date: 2026-11-28. Brief: [2026-11-28-hire-skill-graph-brief.md](2026-11-28-hire-skill-graph-brief.md).
Epic: `t-6036dd`. Branch: `feat/hire-skill-graph`.

## Decision

Every hire runs with a resolved **skill set**: the skills its role uses, plus
the ones the hire asked for, minus the ones it removed, closed over `requires`.
The facts that decide it, which skills are relevant, which role uses which,
which skill requires which, are **skill facts** held in Prime as `cos_skill`
nodes, written only by the user through `cos skills` or the desktop Skills
panel. Prime names skills; it never supplies a path. Paths come from a local
scan of fixed **skill roots**. A Claude hire gets its set as a session plugin
copied into `out/agents/<id>/skills/` and loaded with `--plugin-dir`, and every
user-level skill outside the set that the user marked irrelevant is switched
off for that session with `skillOverrides` in the `--settings` JSON cos
already passes. A Codex hire gets the set as an index in its prompt; whether
Codex can also switch skills off per session is settled by a spike before its
bead is cut. A hire never waits on Prime: a bounded read, then the last graph
read, then no graph, each said in the plan.

## Built as

What Build changed from the design approved at sha256 `fd312716…`; the
sections below are otherwise as approved.

1. **Codex hides skills too.** The spike
   ([report](../reports/2026-11-28-codex-skill-switch-spike.md)) found that
   `-c skills.config=[{name=…,enabled=false}]` switches a skill off for one
   Codex invocation. `SkillSet` carries `hidden_codex`: skills in the `agents`
   and `codex` roots that are irrelevant (or unreviewed under
   `unreviewed: off`) and not picked. A Codex hire's command line gets one
   `-c skills.config=[…]` holding the entries already in
   `$CODEX_HOME/config.toml`, then a disable per hidden name
   (`skill_bundle::codex_config`); names pass `valid_name` first, so none can
   close a TOML string. Picked skills still reach Codex only as the prompt
   block. New sink S9 below.
2. **Single flight per home**, not per process: one Prime read in flight per
   home in a process. The guarantee per business is the same; parallel tests
   no longer share one lock.
3. **Modules.** The Prime read and write are `src/agents/skill_facts.rs`, the
   bundle and the Codex override `src/agents/skill_bundle.rs`, the rows `cos
   skills ls` and the Skills pane share `src/agents/skill_view.rs`;
   `src/agents/skills.rs` keeps the catalog, digests and the resolver. Each
   stays under 500 lines.
4. **`harness::argv(spec, &Delivery)`**: `Delivery { bundle, hidden,
   codex_config }`, built by `launch` from `state.json`, in place of a bundle
   path argument.
5. **Project skills** are the repo's `.claude/skills/` and `.agents/skills/`,
   so a Codex hire keeps the repo's copy too.
6. **`COS_SKILLS_HOME`** replaces `$HOME` for the skill roots;
   `tests/fake_herdr` pins it, `CODEX_HOME` and a refused `COS_PRIME_URL`, so
   no test reads the machine's skills or reaches the live Prime.

Added to the Semantic ISA:

| # | Effect | Where | Reversible |
|---|---|---|---|
| E10 | File read: `$CODEX_HOME/config.toml` (else `$HOME/.codex/config.toml`), its `skills.config` | `skill_bundle::codex_config`, from `prepare` | read only |
| E11 | Harness configuration: `-c skills.config=[…]` on a Codex hire's command line. It carries the user's own `skills.config` entries (skill names and flags), and Herdr keeps a terminal's command line like any hire's | `harness::argv` | per session only |
| E8 (extended) | `state.json` also gains `skills_hidden` (Claude or Codex) and `skills_codex_config` (the S9 value, built in `prepare`), which `launch` turns into the command line | `prepare`, through `write_state` | yes, removed with the run |

`$CODEX_HOME/config.toml` is a new taint source (a file the user owns).

- **S9** the `-c skills.config` value. **VERIFY-C (S9):** built in `prepare`
  with the `toml` crate's `Value`, never by string joins; hidden names pass
  `valid_name`; the user's own entries are carried as parsed TOML values. A
  config file that exists but cannot be read or parsed, or whose
  `skills.config` is not an array, fails the hire before Herdr starts it,
  naming the file: running on would re-enable skills the user switched off.
  A missing file means no entries of the user's.
- VERIFY-S covers `skills_hidden` and `skills_codex_config` as it covers
  `skills_bundled`: written with serde through `write_state`, from names that
  passed VERIFY-R and the catalog.
- Test matrix S9: a name `x"],y=["` never reaches the override (dropped by
  VERIFY-R, and again by `codex_config`: `tests/skills.rs`
  `the_codex_override_is_toml_built_from_valid_names_only`); a user
  `skills.config` entry survives and the hidden names
  follow it, each `enabled = false`, in a value that parses back
  (`tests/hiring.rs`
  `a_codex_hire_switches_off_irrelevant_agents_skills_and_keeps_the_users_own_entries`,
  where the user's entry is `enabled = true` and survives as it was); a
  config that does not parse, and one whose `skills.config` is a string,
  each fail the hire with no `hc run` (same test).

Review fixes, also built:

- A `prime_search` reply with no `nodes` list is a failed read (the previous
  graph stays, `failed_at` set), not an empty fresh graph.
- `COS_PRIME_URL` with a space or control character is refused before any
  socket, so nothing can reach the HTTP request line (VERIFY-U).
- The repo's own skills do not count as unreviewed global skills.
- `graph.json` temp files are named by pid and a per-process counter, so two
  threads cannot share one; the two-writers row of S6 has its test.
- **VERIFY-B, at bundle time:** `skill_bundle::bundle` scans the skill roots
  again (`hire::skill_roots`) and copies a skill only when that catalog holds
  an entry of its name at the recorded canonical path; a path in `spec.json`
  is never trusted on its own (`tests/hiring.rs`
  `bundling_again_rebuilds_and_a_repo_skill_is_neither_copied_nor_hidden`
  refuses a forged path whose tree digest matches).
- **VERIFY-C, unreadable config:** whether `$CODEX_HOME/config.toml` exists
  is asked with `std::fs::exists`, so a permission error stops the hire like
  a parse error does instead of reading as "no config" (same Codex test, with
  the directory at mode 000).
- **`--no-skill` switches a skill off**, not only out of the set: a removed
  skill in a root the harness reads natively joins `hidden` (Claude) or
  `hidden_codex` (Codex), so it does not load anyway (`tests/skills.rs`
  `asked_skills_add_and_remove_and_a_removed_requirement_stays_out_with_a_warning`).

## Context

- `src/agents/hire.rs` `plan()` (line 63) is the one planner behind the
  desktop hire dialog (`apps/desktop/src/lib.rs` `plan`, line 1382, and
  `agents_fit`, line 1433), `cos agents spawn` (`src/main.rs`, the `Spawn`
  arm at line 921, its `plan` call at line 987) and the MCP `spin_off` tool
  (`src/mcp.rs` `spin_off` line 242, `spin_off_hire` line 286, its `plan`
  call at line 295). Its other callers are tests (`tests/fit.rs` lines 125,
  212, 348). `Plan` (line 33) is what
  the dialog renders (`apps/desktop/ui/app.js` `renderFit`, line 1240) and
  `--dry-run` prints.
- `Plan::request` (line 181) builds the `Request`; `record_run`
  (`src/agents/mod.rs` line 509) writes `Spec` to `out/agents/<id>/spec.json`;
  `launch` (line 602) calls `prepare` (line 683: seeds the worktree, writes
  shims) and then `harness::argv(&spec)` (`src/agents/harness.rs` line 16).
- `harness::argv` passes Claude `--settings` with `CLAUDE_SANDBOX` (line 13),
  a JSON string; Codex gets `--sandbox workspace-write` and its prompt after
  `--`.
- The judge counts tracked changes and untracked files git does not ignore
  (`src/agents/judge.rs` line 246). Anything written inside the worktree is
  judged; `out/agents/<id>/` outside it is not.
- `context/agents.json` (`src/agents/policy.rs`) holds roles; the role-library
  flow (`t-b0b659`) adds fields to `Role`. This design does not touch `Role`.
  `Policy` is `deny_unknown_fields` (line 109) and a parse error drops the
  whole file to defaults (`load`, line 137), so a new top-level key must be a
  `Policy` field before any file carries it.
- `cos --dry-run` prints and calls nothing (`CLAUDE.md`, Commands).
- Prime: `allsource-prime` serves MCP over HTTP at `127.0.0.1:3906/mcp`
  (`claude mcp get prime`). A plain `POST` of a JSON-RPC `tools/call` with no
  session returns `200` and `application/json` with a `Content-Length`
  (checked with `prime_stats`). Nodes are append-only: the store holds 37
  `execution_role` snapshots for about eight role keys. Sync is off
  (`LOCAL-ONLY`). The data directory is owned by the running server, so cos
  cannot open the store directly.
- Skill directories on this machine: `~/.claude/skills` (about 60 entries),
  `~/.agents/skills` (about 100, many symlinks into project checkouts),
  `~/.codex/skills` (about 70).
- Claude Code (docs, fetched 2026-11-28): `--plugin-dir <path>` loads a
  plugin for one session; skills live at `skills/<name>/SKILL.md`, are
  namespaced `<plugin>:<skill>`, and load with progressive disclosure
  ([plugins/loading](https://code.claude.com/docs/en/plugins/loading.md)).
  `skillOverrides: {"<name>": "off"}` in settings hides a user or project
  skill from the listing and refuses it by name; plugin skills are not covered
  ([skills](https://code.claude.com/docs/en/skills.md#override-skill-visibility-from-settings)).
  `--settings` sits above user settings and below managed ones, so user hooks
  and `~/.claude/CLAUDE.md` stay. Whether `--plugin-dir` accepts a path
  outside the working directory is not documented (assumption A1).
- Codex (docs): skills come from `.agents/skills` up to the repo root,
  `~/.agents/skills`, `/etc/codex/skills` and bundled ones;
  `[[skills.config]] name = … enabled = false` in `config.toml` disables one.
  Whether `-c` can set it for one invocation is not documented (A2).

## Terms

- **Skill root:** a directory whose direct children are skills. Fixed list:
  `~/.claude/skills` (label `claude`), `~/.agents/skills` (`agents`),
  `~/.codex/skills` (`codex`). Extra roots may be named in
  `context/agents.json` under a new optional top-level `skills.roots` array of
  absolute paths. A repo's own `.claude/skills` and `.agents/skills` are not
  roots: project skills load natively in the worktree and this design
  neither bundles nor hides them.
- **Catalog:** the result of scanning the roots: per skill name, every entry
  found (root label, entry path, canonical path, `description` from the
  `SKILL.md` front matter). Rebuilt on every plan; never stored in Prime.
- **Tree digest:** sha256 over the skill directory's regular files, sorted by
  relative path, each contributing its relative path, a NUL, its length and
  its bytes. Computed for picked skills only. A skill tree holding a symlink
  below its top directory, or more than 20 MiB of files, has no digest and
  cannot be picked.
- **Skill fact:** one `cos_skill` node: `name`, `verdict`
  (`relevant` | `irrelevant`), `roles` (role ids), `requires` (skill names),
  `note`, `curated_at` (RFC 3339), `domain: "cos-skills"`, and
  `source: "cos-skills"` on every fact cos writes.
- **Graph:** the newest skill fact per name among facts that pass VERIFY-R
  (each fact is checked first, then folded by `curated_at`, ties by node
  order; an invalid newer fact does not shadow a valid older one). A name with
  no valid fact is **unreviewed**.
- **Skill set:** what one hire runs with: `picked` (name, why: `role` |
  `asked` | `requires`, root label, canonical path, tree digest), `hidden` (names
  switched off), `unreviewed` (count still on), `graph` (`fresh` |
  `stale <read_at>` | `none`), `warnings`.

## Design

### 1. Prime client: `src/prime.rs`

A small blocking JSON-RPC client over `std::net::TcpStream`; no new crate.

```rust
pub struct Prime { addr: SocketAddr, path: String, deadline: Duration }
impl Prime {
    /// `COS_PRIME_URL`, default `http://127.0.0.1:3906/mcp`. VERIFY-U: no
    /// resolver is consulted.
    pub fn from_env() -> Result<Self>;
    /// `tools/call`; returns the JSON in `result.content[0].text`.
    pub fn call(&self, tool: &str, args: Value) -> Result<Value>;
}
```

- One deadline per call (default 2 s): connect timeout, then read and write
  timeouts set to what is left. Response body capped at 8 MiB; over the cap is
  an error. `Content-Length` required; chunked or SSE replies are an error
  (the server returns neither today).
- A JSON-RPC `error` member, a non-200 status or a body that does not parse is
  an error carrying the status and at most 200 bytes of the body.

### 2. Skill facts: `src/agents/skills.rs`

```rust
pub fn catalog(roots: &[Root]) -> Catalog;                 // local scan
pub fn read_graph(home: &Path, prime: Option<&Prime>) -> Graph;  // fresh, stale or none
pub fn write_fact(prime: &Prime, catalog: &Catalog, policy: &Policy,
                  fact: Fact) -> Result<()>;               // curation only
pub fn resolve(graph: &Graph, catalog: &Catalog, role: Option<&str>,
               asked: &Asked, unreviewed: Unreviewed) -> SkillSet;  // pure
```

- **Read:** `prime_search {"type": "cos_skill"}`, check each fact against
  VERIFY-R (below), drop and warn on the failures, fold the rest. On success
  write the fold to `<home>/out/skills/graph.json` with `read_at`. On any
  error record `failed_at` in that file (keeping its graph), use its graph and
  mark it `stale`; no file marks it `none`.
- **Asking Prime at most every 30 s:** `read_graph` does not call Prime when
  `graph.json` has a `read_at` or a `failed_at` younger than 30 s; it uses the
  file. **One call in flight per home:** a process-wide set records the
  homes with a Prime read open; a caller whose home is in it does not wait
  and uses `graph.json` as `stale`. So while Prime is slow the window has at
  most one Prime call open per business and every other fit check returns
  from the file at once; the CLI, a separate process, adds at most one more.
- **`graph.json` writes** go to a unique temp file beside it
  (`graph.json.<pid>.<nanos>.tmp`) and `rename` over it, so the desktop app
  and a CLI writing at once each leave a whole file; the last rename wins,
  and either is a valid graph.
- **Dry run:** `cos --dry-run` reads `graph.json` only. It neither calls Prime
  nor writes the file.
- **Write:** `prime_add_node` with type `cos_skill` and the fact's
  properties, after VERIFY-W. Then, for exploration only, `prime_add_edge`
  `requires` from the new node to the newest snapshot of each required skill,
  both named `node:cos_skill:<id>` with the ids `prime_add_node` returned and
  the graph's newest snapshot ids (A5). Edges are never read back; `requires`
  and `roles` on the node are the truth. An edge failure is a warning, not a
  failed write.
- **Prime down when writing:** the write fails and says so (`cos skills`
  exits 1 naming Prime's URL and the error; the panel shows the error beside
  the row). Nothing is queued or retried; the user repeats the action.
- **Resolve** (pure, unit-tested):
  1. `picked` = relevant facts whose `roles` holds the role, plus
     `asked.add`, minus `asked.remove`.
  2. Close over `requires` breadth first; a required skill in `asked.remove`
     stays out and adds a warning naming who required it. Cycles stop at the
     first repeat.
  3. A picked name absent from the catalog is dropped with a warning. A name
     in several roots takes the first root in list order; the others are
     named in the warning.
  4. `hidden` = names in the `claude` root (and extra roots Claude reads
     natively, none today) that are not picked and are `irrelevant`, plus,
     when `unreviewed` is `Off`, those with no fact. Every picked skill whose
     entry is in the `claude` root is hidden too, so the bundled copy is the
     only one the session sees.
  5. **Project skills win.** `skillOverrides` matches by name and also
     covers project skills, so no name in the hire's repo `.claude/skills/`
     (listed at plan time from the repo the plan resolved) is ever hidden. A
     picked skill the repo also holds is not bundled: it is recorded with why
     `project`, the repo's copy is what the session sees, and the plan says
     so. This repo holds `ritual`, `ritual-full` and `semantic-isa`, which
     `~/.claude/skills` also holds; a hire here keeps the repo's.
  6. `unreviewed` count goes in the set; the plan warns while it is above 0.
- `Unreviewed` comes from `context/agents.json` `skills.unreviewed`
  (`"on"` default, `"off"`). `Policy` gains an optional, `deny_unknown_fields`
  `skills: SkillsPolicy { roots: Vec<PathBuf>, unreviewed: Unreviewed }`, so
  a file with or without the key loads (`tests/hiring.rs`). The default is
  `on` because the user asked that global skills stay considered until
  curated; the plan names the unreviewed count and `cos skills curate` until
  it is 0.

### 3. Planning and the request

- `hire::plan` gains `asked_skills: &Asked` and fills `Plan.skills:
  SkillSet`; its three callers and the three test call sites change with it.
  The graph read happens here, bounded as §2 says.
- `Request` and `Spec` gain `skills: Option<SkillSet>` (serde default, so
  older `spec.json` files still read). `Plan::request` moves it across.
- Entry points:
  - CLI: `cos agents spawn --skill <name>` and `--no-skill <name>`,
    repeatable. `--dry-run` prints the set in the plan.
  - Desktop: `SpawnArgs.skills: { add: [], remove: [] }`.
  - MCP `spin_off`: `skills_add`, `skills_remove` string arrays.

### 4. Delivery

`prepare` (`mod.rs` line 683) gains one step after seeding, for
`Harness::Claude` with a non-empty `picked` (a set with only `hidden` names
skips the bundle and still gets its `skillOverrides`, below), under one 300 s
deadline for the
whole bundle (each copy gets what is left, as `staff.rs` does with its
`until`):

1. Remove `out/agents/<id>/skills.tmp` and `out/agents/<id>/skills` if a
   previous `launch` left either (both live only under this run's directory,
   and `launch` runs `prepare` each time it is called, `mod.rs` line 608),
   then create `skills.tmp/.claude-plugin/` and `skills.tmp/skills/`, and write
   `skills.tmp/.claude-plugin/plugin.json` as `{"name": "cos-hire"}`.
2. For each picked skill, VERIFY-B, then `cp -Rc <canonical path>
   skills.tmp/skills/<name>` through `proc::run`. The destination does not
   exist (fresh temp directory), so `cp` creates it rather than nesting into
   it. `seed.rs` `clone_cmd` becomes `pub(crate)` and is reused.
3. Recompute the tree digest of each copy; it must equal the set's. A
   mismatch, a symlink in the copy or a missing `SKILL.md` fails the hire
   before Herdr, as a seed failure does.
4. `rename` `skills.tmp` to `skills`, then record `skills_bundled:
   Vec<String>` in `state.json`.

`harness::argv(spec, &Delivery)` (Built as item 4); `launch` sets its bundle
to `dir(home, id)/skills` when `state.skills_bundled` is non-empty:

- Claude `--settings` becomes `CLAUDE_SANDBOX` merged (as
  `serde_json::Value`) with `{"skillOverrides": {<hidden name>: "off", …}}`
  whenever `spec.skills.hidden` is non-empty, whether or not anything was
  bundled.
- Claude `--plugin-dir <bundle>` when `bundle` is `Some`.

Codex: `Plan::request`, which knows `run.harness`, appends a "Skills for this
work" block to a Codex prompt: each picked skill's name, description and
canonical `SKILL.md` path, to read there (A6). Nothing is bundled for Codex;
its irrelevant skills are switched off as Built as item 1 describes.

### 5. Curation

- `cos skills ls [--unreviewed] [--role <id>] [--json]`: the catalog joined
  with the graph: name, roots, verdict, roles, requires.
- `cos skills curate [--all]`: needs a terminal (refuses otherwise, naming
  `cos skills set`). Walks unreviewed skills (every skill with `--all`) one at
  a time: name, roots, description; keys `r` relevant, `i` irrelevant, `s`
  skip, `q` quit. After `r` it asks for roles (comma list, offered from the
  policy's hireable roles; empty for none) and requires (names, tab-free
  comma list, checked against the catalog). Each answer is one `write_fact`
  call before the next skill shows, so quitting loses nothing.
- `cos skills set <name> (--relevant | --irrelevant) [--role <id>]...
  [--requires <name>]... [--note <text>]`: the same write, scripted.
- `cos skills show --role <id> [--skill …] [--no-skill …]`: the skill set a
  hire would get, without hiring.
- Desktop Agents tab, a **Skills** panel: three groups (Unreviewed, Relevant,
  Irrelevant) with counts; each row shows name, root labels, description, and
  for relevant skills its role chips and requires; actions Relevant,
  Irrelevant, Edit roles. Tauri commands `skills_catalog` (read) and
  `skills_set` (write, refused unless it names the window's business, as
  `agents_spawn` does). Loading, empty (no skill roots), Prime unreachable
  (retry re-fetches) and stale states all render through `stateView`.
- Hire dialog: a **Skills** section under the contract: the picked list with
  each skill's reason and an × to remove, an Add picker over the catalog,
  and the plan's skill warnings in the existing notes list.
- Run page: a folded **Skills** row listing the picked names and short
  digests from `spec.json`.
- Not over cos's MCP: the brains never write skill facts (they run with no
  tools), and cos's own MCP server offers no skill-writing tool.
- **Limit, stated (the user's decision, 2026-11-28):** Prime is writable by
  any process on the machine, hires included. A hire loads the user's MCP
  config, which registers `prime`, and an MCP call is not a Bash command, so
  Seatbelt does not stand between a hire and `prime_add_node`. cos does not
  deny it. What cos does:
  - `cos skills curate` and `cos skills set` refuse when `COS_RUN_ID` is set
    (`environment`, `mod.rs` line 708, pins it on every hire). This stops a
    hire calling cos by accident, not a hire set on writing Prime.
  - Every fact cos writes carries `source: "cos-skills"`. The Skills panel and
    `cos skills ls` mark a fact without it as "written outside cos", so a
    fact a hire or another tool wrote is visible, and one click supersedes it.
  - VERIFY-R bounds what any fact can do: it names skills already on the
    machine, picks among them, or hides them. It cannot add a path, a
    command or a skill that is not in a root.

## Semantic ISA

### Side-effect inventory

| # | Effect | Where | Reversible |
|---|---|---|---|
| E1 | Network read: `POST 127.0.0.1:3906/mcp` `prime_search` | `prime.rs`, from `read_graph` | yes, read only |
| E2 | Data write: `prime_add_node`, `prime_add_edge` into Prime | `write_fact`, from `cos skills curate|set` and `skills_set` | append-only; a later fact supersedes; `prime_forget` removes |
| E3 | File write: `out/skills/graph.json` (temp file and rename; `read_at`, `failed_at`) | `read_graph` | yes, a cache |
| E4 | File writes, deletes and child processes: `remove_dir_all` of a leftover `skills.tmp/` and `skills/`, `skills.tmp/` created, `plugin.json` written, `cp -Rc` per skill, `rename` to `skills/`, all under `out/agents/<id>/` (a path reached only through `run_dir`, `mod.rs` line 363) | `prepare` | yes, removed with the run (`cos agents clean`) |
| E9 | Window rendering: skill descriptions (from `SKILL.md`) and fact notes (from Prime) shown in the Skills panel, the hire dialog and the run page | `apps/desktop/ui/app.js` | n/a, display |
| E5 | Harness configuration: `skillOverrides` and `--plugin-dir` on the agent's command line | `harness::argv` | per session only |
| E6 | Prompt content: the Codex skills block | `Plan::request` | per session only |
| E7 | File write: `out/agents/<id>/spec.json` gains `skills` | `record_run` (`mod.rs` line 580) | yes, removed with the run |
| E8 | File write: `out/agents/<id>/state.json` gains `skills_bundled` | `prepare`, through `write_state` | yes, removed with the run |

No DNS lookup: VERIFY-U accepts literal addresses and `localhost` only.

### Instructions

| Core | Instruction | Governance | input_schema → output_schema |
|---|---|---|---|
| read | `PrimeSearch` | loopback only, one deadline, size cap | `{type: "cos_skill"}` → `[Fact]` (closed: name, verdict, roles, requires, note, curated_at) |
| compute | `Resolve` | pure | `(Graph, Catalog, role?, Asked{add,remove}, Unreviewed)` → `SkillSet` |
| write | `WriteFact` | user action only (TTY, `set`, window command naming its business) | `Fact` → `()` |
| write | `Bundle` | after VERIFY-B, under the seed deadline | `SkillSet.picked` → `skills_bundled: [name]` |
| config | `HarnessArgs` | serde-built JSON, no string joins | `(Spec)` → `argv: [String]` |

### Taint

Sources: Prime responses (any local process can write Prime), `SKILL.md`
front matter and directory names under the roots (files), the user's `--skill`
/ dialog / MCP `skills_add` names (input), `context/agents.json` (file),
`$CODEX_HOME/config.toml` (file), the URL in `COS_PRIME_URL` (environment).

Sinks: S1 the TCP connect and the HTTP request line (E1, E2), S2
`prime_add_node` and `prime_add_edge`, including the node ids an edge names
(E2), S3 the `cp -Rc` source and destination paths (E4), S4 the `--settings`
JSON and `--plugin-dir` path (E5), S5 the Codex prompt block (E6), S6
`graph.json` (E3), S7 the `skills` written into `spec.json` and
`skills_bundled`, `skills_hidden` and `skills_codex_config` into
`state.json` (E7, E8), S8 the window's DOM (E9), S9 the Codex
`-c skills.config=[…]` value on the command line (E10, E11).

### VERIFY before every sink

- **VERIFY-U (S1):** the URL parses as `http://<host>:<port>/<path>`; `host`
  is a literal IPv4 or IPv6 address for which `IpAddr::is_loopback` holds, or
  exactly `localhost`, which maps to `127.0.0.1` without a resolver. A URL
  holding a space or control character is refused too, so nothing reaches the
  request line. Anything else is refused before any lookup or socket.
- **VERIFY-R (S2, S4, S5, S6, S9 via the graph):** every fact from Prime:
  `name` and each `requires` entry match `^[a-z0-9][a-z0-9._-]{0,63}$`;
  `verdict` is one of the two words; `roles` entries are role ids present in
  the loaded policy (others dropped with a warning); `curated_at` parses. A
  failing fact is dropped and named in the set's warnings. A node id is kept
  for a later edge only when it is 1 to 64 ASCII letters, digits and `-`
  (`node_id_ok`); otherwise the fact stays and no edge names it. The id
  `prime_add_node` returns passes the same check.
- **VERIFY-W (S2):** the name is in the local catalog; `verdict` is one of
  two; every role is a hireable role in the policy; every `requires` name is in
  the catalog and is not the skill itself; `note` at most 500 chars. The call
  comes from a TTY session, `cos skills set`, or `skills_set` with the
  window's own business, and the CLI paths refuse when `COS_RUN_ID` is set.
- **VERIFY-B (S3):** the source path comes only from the catalog entry for a
  VERIFY-R name: a direct child of a configured root, found by listing that
  root (the entry itself may be a symlink, as `~/.agents/skills` entries
  are; its canonical path is used). Below that top directory the tree holds
  no symlink, at most 20 MiB, and a regular `SKILL.md`, and its tree digest
  equals the set's. The destination is `skills.tmp/skills/<name>` with the
  validated name (no separator possible) and does not exist. After the copy
  the tree digest is recomputed on the copy. Prime never supplies a path.
- **VERIFY-D (S8):** descriptions, notes, names and role ids reach the DOM
  only as text: through `el()` with `textContent` or `badge()`, never
  `innerHTML` or an attribute that takes a URL. The existing `el()`
  allowlist gate (`tests/design.rs`) covers the new code.
- **VERIFY-S (S7):** `skills` is the `SkillSet` `Resolve` returned, every
  name and path already through VERIFY-R and VERIFY-B; `skills_bundled`
  lists only names whose copy passed step 3 of §4. Both are written with
  serde through the existing `write_json` and `write_state`.
- **VERIFY-A (S4, S9):** hidden names and the bundle path enter argv only
  through `serde_json` values (Claude), `toml::Value`s (Codex, VERIFY-C in
  Built as) and `Path` arguments, and only names that passed VERIFY-R or come
  from the catalog. Each is one argv element; no shell parses it.
- **VERIFY-P (S5):** the Codex block prints only catalog descriptions with
  control characters stripped and each line capped at 300 chars.

### Violations

| Violation | Evidence recorded | Recovery |
|---|---|---|
| Non-loopback `COS_PRIME_URL` | plan warning names the URL's host | graph from `graph.json` (stale) or none |
| Prime slow, down or malformed | warning with status or error, `graph: stale` | hire continues on the cached graph |
| Fact fails VERIFY-R | warning names the skill and the field | fact ignored; the skill reads as unreviewed |
| Write fails VERIFY-W | `cos skills` exits 1 naming the field; the panel shows it beside the row | nothing written |
| Bundle digest mismatch, symlink in the tree, tree over 20 MiB or missing `SKILL.md` | at plan: the skill is dropped with a warning naming why; at bundle: the run fails before Herdr with the skill named, as a seed failure | hire again after fixing or removing the skill |
| Prime down during a curation write | `cos skills` exits 1 naming the URL and error; the panel shows it beside the row | repeat the action once Prime answers |
| Removed skill required by a picked one | plan warning: who required it | stays removed; the user may add it back |

### ISA test matrix

| Sink | Tainted input refused | VERIFY-cleared input admitted |
|---|---|---|
| S1 | `COS_PRIME_URL=http://10.0.0.1:3906/mcp`, `http://example.com/mcp` and `https://127.0.0.1/mcp` are refused by `from_env` before any lookup or socket (stand-in listener sees no connection) | `http://127.0.0.1:<port>/mcp`, `http://[::1]:<port>/mcp` and `http://localhost:<port>/mcp` reach the stand-in |
| S2 | `set` with an unknown name, an unknown role, a self `requires`, a 501-char note, or with `COS_RUN_ID` set: nothing reaches the stand-in | a valid fact arrives as one `prime_add_node` with the closed property set, then one `prime_add_edge` per `requires` |
| S3 | a Prime fact named `../evil` or `a/b`; a skill tree holding an inner symlink; a tree over 20 MiB; a tree changed between plan and bundle: none is copied | a valid picked skill (its top entry a symlink into another directory) is copied and the copy's tree digest matches |
| S8 | a description `<img src=x onerror=alert(1)>` and a Prime note with markup render as literal text in the Skills panel and hire dialog (`apps/desktop/tests/ipc.rs` drives the data; the `el()` gate fails any `innerHTML`) | a plain description renders as is |
| S7 | a set built from a Prime fact that failed VERIFY-R does not name that skill in `spec.json`; a skill whose copy failed is not in `skills_bundled` | a resolved set reads back from `spec.json` equal to the plan's |
| S4 | a fact named `x","permissions":{` is dropped before argv; an `irrelevant` fact whose name the hire's repo holds in `.claude/skills/` is not in `skillOverrides` | hidden names appear as keys under `skillOverrides` in a JSON that parses back |
| S5 | a description with `\x1b[2J` and a 2,000-char line prints stripped and capped | a normal description prints as is |
| S6 | a malformed Prime reply leaves the previous graph in `graph.json` and adds only `failed_at`; two writers at once leave a file that parses | a good reply replaces it through the temp-and-rename path |

## Alternatives

- **Copy into the worktree's `.claude/skills/`**: the judge counts untracked
  files (`judge.rs` line 246), so every reviewer and goldfish would fail its
  scope; scratch hires have no repo. Rejected.
- **Inline `SKILL.md` text in the prompt** for Claude: costs every skill's full
  text up front and drops the scripts and references beside it. Kept only as
  the Codex fallback, as an index, not inline text.
- **`--setting-sources project,local`** to drop user skills wholesale: also
  drops user hooks (`claude-guard`), user permissions and
  `~/.claude/CLAUDE.md`. Rejected.
- **`permissions.deny Skill(<name>)`** instead of `skillOverrides`: whether a
  denied skill leaves the listing is not documented; `skillOverrides: off`
  is documented to hide it. Rejected.
- **Paths in Prime**: would make any local writer of Prime able to point a
  hire at an arbitrary directory. Rejected for name-only facts and a local
  catalog.
- **A graph file in the repo or from graphify**: offered in Think; the user
  chose Prime.

## Security and privacy

- Read: the skill roots (names, front matter, `SKILL.md` digests) and Prime
  over loopback. Nothing leaves the machine; Prime sync is off here, and if
  the user turns it on, skill facts (names, roles, notes) sync with the rest of
  their Prime store.
- Stored: skill facts in Prime; `out/skills/graph.json`; each run's bundle and
  its `spec.json` set, under `out/`, gitignored.
- No credentials are read or written. Skill contents are the user's own files,
  copied for one session; `cos agents clean` removes them with the run.
- The remote read-only MCP server (`cos mcp` over HTTP) gains nothing: no
  skill tool, no Prime access.

## Testing

Trophy weight on integration tests that drive the real planner and argv
against stand-ins; no test reaches the real Prime or real skill roots.

- **Stand-in Prime** (`tests/common/prime.rs`): a `TcpListener` thread that
  answers JSON-RPC from a scripted table, records requests, and can delay,
  close or return malformed bodies.
- `tests/skills.rs`: catalog scan over a temp home with three roots, symlinked
  entries and a broken `SKILL.md`; fold newest-wins; resolve (role, add,
  remove, requires closure, cycles, remove-vs-requires, missing skill,
  duplicates across roots, hidden set, unreviewed on/off); VERIFY-R cases.
- `tests/prime.rs`: S1, S6 rows; deadline (stand-in sleeps past it); size cap;
  non-200; JSON-RPC error; no second call within 30 s of a success or a
  failure; with the stand-in holding a reply, ten concurrent `read_graph`
  calls in one process open one connection and the other nine return the
  file's graph as `stale` without waiting; `--dry-run` makes no call and
  writes no file.
- `tests/hiring.rs`: a hire with a role plan carries `skills` into
  `spec.json` (S7); Claude argv holds `--plugin-dir` and the merged settings
  (parsed back, `sandbox` intact); the bundle exists with digests and no
  `skills.tmp` is left; a leftover `skills.tmp` from a failed attempt is
  replaced, not nested into; a second `prepare` on the same run, with
  `skills/` already present, rebuilds it and succeeds; a picked skill the
  repo also holds in `.claude/skills/` is recorded `project`, not bundled and
  not hidden; a digest mismatch fails before Herdr (fake `hc`
  never sees `run`); Codex prompt block (S5) and no `--plugin-dir` for Codex;
  old `spec.json` without `skills` still reads; `context/agents.json` with and
  without `skills` loads with its roles.
- `tests/skills_cli.rs`: `cos skills set` S2 rows; `curate` refuses without a
  TTY; `ls --json` and `show` output.
- `apps/desktop/tests/ipc.rs`: `skills_catalog`, `skills_set` (business
  check), `agents_fit` with `skills` add/remove.
- `tests/design.rs` gates cover the new panel's states and tokens unchanged.
- Hand-check (real Claude, once, in the PR's evidence): a hire with one bundled
  skill shows `/cos-hire:<name>` in its skill list and not a hidden global.

## Confidence

| Piece | Tier | What moves it up |
|---|---|---|
| Prime client and fold | rock | — (protocol checked against the live server) |
| Resolve and catalog | rock | — |
| Claude `skillOverrides` hiding | rock | documented; the hand-check confirms |
| `--plugin-dir` outside the working directory (A1) | sand | the hand-check; fallback: pass `--add-dir out/agents/<id>/skills` with the bundle as `.claude/skills/` there, which the docs say loads |
| Codex per-session disable (A2) | rock | settled by the spike: it works (see Built as). Not checked: whether a disabled skill can still be called by name |
| Desktop Skills panel and dialog section | rock | same patterns as the Staff tab and hire dialog |
| Plugin skills from installed plugins | out of scope | named in the plan warning: "installed plugins still load"; a later flow can toggle them with `enabledPlugins` per plugin |

## Acceptance

1. With the stand-in Prime holding `rust-perf` relevant for `builder` and
   `requiring` `tdd`, `cos --dry-run agents spawn --role builder --prompt …`
   after one non-dry plan has cached the graph, prints a plan whose
   `skills.picked` names `rust-perf` (role) and `tdd` (requires) with tree
   digests, and the stand-in records no request from the dry run.
2. A non-dry plan with the stand-in stopped: `graph: stale <time>` and the
   same set; with no `graph.json`, `graph: none`, an empty set and a warning;
   the hire still starts in both.
3. `cos agents spawn … --no-skill tdd` keeps `tdd` out and warns that
   `rust-perf` requires it.
4. A spawned Claude hire's `spec.json` holds the set; its run directory holds
   `skills/.claude-plugin/plugin.json` and `skills/skills/rust-perf/` whose
   tree digest equals the recorded one; the command line Herdr received has
   `--plugin-dir` on that directory and a `--settings` JSON whose
   `skillOverrides` turns off every irrelevant `claude`-root skill and the
   `claude`-root originals of bundled skills.
5. The worktree's `git status --porcelain` is empty after the bundle step;
   a reviewer hire with scope `REVIEW.md` is not judged out of scope for it.
6. `cos skills set ../x --relevant` and `cos skills set rust-perf --relevant
   --role nobody` exit 1 and the stand-in records no write.
7. `cos skills curate` in a terminal walks unreviewed skills and each `r`/`i`
   lands as one `prime_add_node` before the next skill shows; without a
   terminal it exits 1 naming `cos skills set`.
8. The desktop Agents tab shows the Skills panel with the three groups; marking
   a skill irrelevant moves it and the next fit check hides it; with Prime
   down the panel shows the unreachable state with a retry that re-fetches.
9. The hire dialog lists the picked skills with reasons; removing one updates
   the plan; the run page shows the set.
10. Day one, before any curation: the plan's warnings name the unreviewed
    count and `cos skills curate`. After `cos skills set seo-audit
    --irrelevant`, the next Claude hire's `skillOverrides` turns `seo-audit`
    off, also for a hire whose role picks no skill (nothing bundled, no
    `--plugin-dir`, `skillOverrides` present); with `skills.unreviewed:
    "off"` every unreviewed `claude`-root skill is off too. A fact written
    to the stand-in without `source` shows as "written outside cos" in
    `cos skills ls`.
11. Every row of the ISA test matrix is a passing test.
12. `task test` passes.

## Review history

Three rounds, two fresh-context reviewers per round: rounds 1 and 2 in
[`docs/archive/plans/`](../archive/plans/README.md), round 3 beside this
design ([a](2026-11-28-hire-skill-graph-design-review-a.md),
[b](2026-11-28-hire-skill-graph-design-review-b.md)). The Built as section
had three more rounds, the last beside this design
([a](2026-11-28-hire-skill-graph-design-review-drift-a-r3.md),
[b](2026-11-28-hire-skill-graph-design-review-drift-b-r3.md)). Round 3 left blocking items open; on 2026-11-28 the user chose, in
conversation, "Honest limit only" for hire writes to Prime and "Approve,
carry to Plan". The round-3 mechanical items (skillOverrides for a
hidden-only set, VERIFY-D/S8, the E4 delete row) and the stated Prime-write
limit are applied above. Carried to Plan, each a bead or a line in one:

- Decision paragraph: say the day-one behaviour (unreviewed skills on, count
  and `cos skills curate` in the plan).
- Confidence: the Prime client row is sand until the `prime_search` and
  `prime_add_node` reply shapes are read once from the live server (A5).
- Validate `skills.roots` entries as absolute existing directories; regex-check
  catalog names at scan, not only Prime names.
- Concurrency wording: "one Prime call in flight per process".
- `spec.json` is written at `record_run`, before the bundle: `skills_bundled`
  in `state.json` is what was delivered; acceptance 4 reads both in that
  order.
- `prepare` total: seed (300 s) and bundle share the five-minute ceiling;
  give the bundle what the seed left, capped at 300 s.
- Read the repo's project skills from the seeded worktree at bundle time, not
  only from the checkout at plan time.
- Hand-checks at Build: Claude's Seatbelt can read `out/agents/<id>/skills`
  (A1 and its fallback layout), Codex `-c skills.config` (A2), Codex reads
  outside the workspace (A6).
- Edge target ids: keep each fact's Prime node id in the fold and
  `graph.json` so `prime_add_edge` has its target.
- Tests: fold with a newest-invalid and an older-valid fact; acceptance rows
  for a Prime-down write and for the repo `.claude/skills` plan warning.
- Context: mark the Prime counts as measured on 2026-11-28; fix stale line
  references (E7 `record_run`) at Plan time.
- `hire::plan` is called once per ask in `cos agents spawn --all`: one
  graph read serves every ask (the 30 s reuse covers it); say so.
- `seed.rs` `clone_cmd`: make it `pub(crate)` for both platform variants.

## Assumptions

- A1: `--plugin-dir` accepts an absolute path outside the working directory
  (fallback in Confidence).
- A2: Codex has no documented per-invocation skill switch; the spike decides.
- A3: skill relevance and role links are per user, not per business: one
  Prime store serves every business on the machine. Role ids that one
  business lacks are dropped for it with a warning (VERIFY-R).
- A4: Prime's `prime_search` returns every node of a type in one response
  (the 37 `execution_role` snapshots came back in one response on
  2026-11-28; about 230 skill facts at most per curation pass).
- A5: `prime_add_node` returns the new node's id, and `prime_add_edge` takes
  `node:<type>:<id>` (its tool schema says so). Read at Build against the
  live server's response once; the stand-in mirrors what it returns. Edges are
  best effort, so a wrong shape costs exploration only.
- A6: Codex's `workspace-write` sandbox lets it read files outside the
  workspace. The Codex spike checks it; if reads are refused, the Codex block
  names each skill's description only.
