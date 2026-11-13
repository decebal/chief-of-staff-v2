# Multiple businesses in one cos: design

Brief: [2026-11-12-multi-business-brief.md](2026-11-12-multi-business-brief.md). Earlier review
rounds: round 1 [Goldfish](2026-11-12-multi-business-design-review-r1.md) and
[EGM](2026-11-12-multi-business-egm-r1/consolidated_review_report.md); round 2
[Goldfish](2026-11-12-multi-business-design-review-r2.md) and
[EGM](2026-11-12-multi-business-egm-r2/consolidated_review_report.md); round 3
[Goldfish](2026-11-12-multi-business-design-review-r3.md) and
[EGM](2026-11-12-multi-business-egm-r3/consolidated_review_report.md); verification
[Goldfish](2026-11-12-multi-business-design-review-r4.md) and
[Goldfish](2026-11-12-multi-business-design-review-r5.md) and
[Goldfish](2026-11-12-multi-business-design-review-r6.md) and
[Goldfish](2026-11-12-multi-business-design-review-r7.md) and
[Goldfish](2026-11-12-multi-business-design-review-r8.md) and
[Goldfish](2026-11-12-multi-business-design-review-r9.md) and
[Goldfish](2026-11-12-multi-business-design-review-r10.md) and
[Goldfish](2026-11-12-multi-business-design-review-r11.md) and
[Goldfish](2026-11-12-multi-business-design-review-r12.md) and
[Goldfish](2026-11-12-multi-business-design-review-r13.md) and
[Goldfish](2026-11-12-multi-business-design-review-r14.md).

## Problem

cos serves one business at a time. Its three directories (below) come from flags, `COS_*` env
vars or the desktop app's `settings.json`, and the app's only switch, `set_workspace`, changes one
of the three. A person running several businesses edits env vars or JSON to move between them,
and a partial edit leaves cos reading one business's goals against another's repos.

## Terms

- **Home:** holds `context/` (goals, decisions and notes cos reads, "the brain") and `out/`
  (briefs, reports, agent runs). Flag `--home`, env `COS_HOME`.
- **Workspace:** holds `.chief-of-staff/`: `profile.json` (approved sources, settings) and the
  **workspace state** `state.json` (tracked deliverables). Flag `--workspace`, env
  `COS_WORKSPACE`; falls back to home.
- **Projects:** the directory of repos that `cos flows` (ritual flows), the docs room (the app's
  Docs tab) and agent hiring scan. Flag `--projects`, env `COS_PROJECTS`.
- **Triple:** one home, workspace and projects directory, used together.
- **Business:** a named triple in the registry.
- **Active business:** the registry's default business.
- **App business:** the business the desktop window shows, held in the app's memory.
- **Run record:** `out/agents/<id>/state.json`, one per agent run; its status is live
  (`starting`, `queued`, `seeding`, `running`) or terminal (`done`, `failed`, `killed`,
  `timed_out`, `lost`) (`src/agents/mod.rs:108-133`; the app's `LIVE`, `app.js:48`).
- **Install:** the cos source checkout the binary was built from. It holds `CLAUDE.md`,
  `.claude/agents/` and `templates/context/`, which every business shares.
- **Built-in defaults:** home = the install, workspace = home, projects = `$HOME/Projects`.
- **Supervisor:** the detached `cos agents supervise` process that owns one agent run.
- **Long-lived processes:** `cos dash` (dashboard, :8790), `cos mcp` (MCP server for Claude Desktop
  and Codex), `cos serve` / `cos tunnel` (remote MCP, :8787), and the launchd jobs `cos.brief`
  (weekday brief) and `cos.dash` (keeps the dashboard up). Each binds one triple at start.

## Goals and non-goals

Goals: a registry of named businesses; one resolver that gives every command a triple taken from
a single source; a command-select picker that switches the app; `cos business` commands.

Non-goals (brief, "Out of scope"): per-business ports or launchd labels, a dashboard or MCP server
that follows the app's switch, per-business credentials, and `~/.claude/agents/tactician.md`,
which keeps the install home written at `task install`.

## Decision

A registry, `~/.config/cos/businesses.toml`, names each business's triple and the active business.
One resolver returns a triple from the highest of three layers that names anything (flags, then
env, then the registry): either one named business, or explicit directories completed from the
built-in defaults. A registry business is never combined with flags or env. The app keeps its business in memory,
starting from the resolver; only `cos business use` changes the active business. The picker
(⌘K, bound as an app menu accelerator so it reaches the report and docs frames; confirmed by the
hand-check) switches the app business and reloads the window.

User decision (2026-11-12): when flags or env set only some directories, the rest come from the
built-in defaults, exactly as cos behaves today (`COS_HOME` alone gives workspace = that home and
projects = `$HOME/Projects`). Such an explicit triple is never completed from a registry business.

## Departures from the brief (for sign-off)

- A business added with `cos business add` must name existing directories; the profile hint is
  `cos --workspace <dir> state init`, because the new name is not registered until `add`
  succeeds.
- Inside one layer, two sources conflict and stop the command (`--business` with `--home`;
  `COS_BUSINESS` with `COS_HOME`). Across layers the higher one wins, as the brief's "env still
  overrides" asks.
- The badge counts live runs; lost runs show in the picker rows instead.
- First-run migration writes `default` from the built-in defaults, not from whichever process
  starts first; an app whose legacy settings point elsewhere shows the command that adds them.
- Claude runs with the install as its working directory for every business (its `CLAUDE.md` and
  agents are cos's instructions), and `cos business add` seeds a new home's `context/`.
- The launchd jobs are baked to one business at `task schedule`; moving them is a reschedule.

## Context

- CLI (`src/main.rs:16-45`): `home` defaults to `default_home()`, the build's clone
  (`main.rs:883-885`); `Cli::ws()` falls back to home (`main.rs:41-44`); `projects` defaults to
  `home_dir().join("Projects")` (`main.rs:25`). Each reads its `COS_*` env. The triple is read at
  `main.rs:330-332, 348-349, 370-371, 383-385, 392, 396, 401, 428, 512, 567-571, 843`.
- Internal commands `agents supervise` and `agents build-slot` (`main.rs:290-294, 713-718`) take
  their inputs as arguments; `build-slot` runs on every compiler call inside an agent
  (`slots.rs:114-116`).
- App (`apps/desktop/src/settings.rs`): one `Settings { home, workspace, projects, slots, cos }`,
  env first, then `settings.json`, then defaults; the app default home is
  `CARGO_MANIFEST_DIR/../..` (`settings.rs:26-28`). `save()` writes the effective values, env
  included (`settings.rs:91-98`); a file that fails to deserialise is dropped (`settings.rs:45-47`).
  `set_workspace` (`lib.rs:59-80`) is the only switch. `App(Mutex<Settings>)`; `get()` clones one
  snapshot per command (`lib.rs:23-29`).
- UI: top-level tabs Today (the report), Agents (with Runs, Staff and Hire panes), Flows, Docs
  (`ui/index.html:18-21, 179`). Report, docs and flows render in sandboxed iframes; key events
  inside a frame do not reach `app.js`. No startup code reads the URL hash (`app.js:186-206`).
- Agents: `agents::list` and `read` call `reconcile` (`src/agents/mod.rs:446-494`), whose
  read-only part decides live, lost or still starting (`is_cos`, START_GRACE, `mod.rs:468-476,
  668-671`) and whose repair part kills a dead supervisor's harness group and rewrites the run
  record (`mod.rs:485-492`). Spawn runs `cos --home <home> agents supervise <id>`
  (`mod.rs:422-425`) and the child inherits the parent's env.
- launchd: `launchd/cos.brief.plist:14` and `cos.dash.plist:15` set `COS_HOME`; `task schedule`
  fills it from the install (`Taskfile.yml:187-197`). `task install-mcp` passes no triple
  (`Taskfile.yml:57-74`).
- Parameters named `home` that receive the workspace: `pipeline.rs:119-120, 183-184` (callers pass
  `self.workspace`, `pipeline.rs:344, 360, 388, 408, 459`), `track.rs:58-72, 192`,
  `context.rs:72-74, 203-248`, `main.rs:775-778`. Correct today; renamed first so a business
  threaded through later lands on the right directory.

## Design

### Registry (`src/business.rs`)

```toml
active = "default"

[business.default]
home = "/Users/me/Projects/chief-of-staff-v2"
workspace = "/Users/me/Projects/chief-of-staff-v2"
projects = "/Users/me/Projects"
```

- `Registry { active: Option<String>, businesses: BTreeMap<String, Business> }`,
  `Business { home, workspace, projects }`.
- Location: `COS_BUSINESSES`, else `$XDG_CONFIG_HOME/cos/businesses.toml`, else
  `$HOME/.config/cos/businesses.toml`. The first write creates the directory.
- Load: a missing file is an empty registry. A parse error, a name outside
  `^[a-z0-9][a-z0-9-]{0,39}$`, or an `active` naming no business is an error naming the file and
  the problem; nothing rewrites the file. `active` absent with businesses present means no active
  business (resolver layer 3 is skipped).
- Write: the writers are `cos business add | use | remove` and first-run migration (below). Each
  takes an exclusive `flock` on `businesses.toml.lock` (the lock helper in `src/agents/slots.rs`),
  re-reads inside the lock, applies one change, and writes through `state::atomic_write`, widened
  to take bytes (it already uses a unique temp name, `state.rs:548-564`).
- `toml` 1.1.8 (in `Cargo.lock` through Tauri) becomes a direct dependency of `cos`.

### Resolver (`cos::business::resolve`)

`resolve(inputs) -> Result<Triple>`, where inputs are the parsed flags (`--business`, `--home`,
`--workspace`, `--projects`), the env values (`COS_BUSINESS`, `COS_HOME`, `COS_WORKSPACE`,
`COS_PROJECTS`) and a registry loader, all passed as values so tests need no process env. The
loader runs only when resolution reaches layer 1 or 2 with a business name, or layer 3; a command
given directories by flags or env never loads the registry.
`Triple { home, workspace, projects, business: Option<String>, layer }`.

1. **Flags layer** (anything given on the command line): `--business` alone gives that business's
   triple; directory flags alone give an explicit triple (each missing directory from the
   built-in defaults, workspace falling back to the given home); `--business` with a directory
   flag is an error.
2. **Env layer**, if no flag was given: `COS_BUSINESS` alone, or the `COS_*` directory vars alone,
   the same way; `COS_BUSINESS` with a directory var is an error.
3. **Registry layer**, if neither: the active business.
4. Else the built-in defaults (the install, canonicalised; workspace = home; `$HOME/Projects`);
   an unset `HOME` is an error.

The app passes no defaults of its own (`built_home()` is deleted), so the CLI and the app agree by
construction. `Cli`'s directory fields lose `default_value_os_t` and become `Option`s. Commands
that consume a triple call `resolve` once and take the `Triple`; `agents supervise`,
`agents build-slot` do not resolve (they take their paths as arguments); `state init` resolves
like any command but never requires a profile, since creating one is its job.
Long-lived processes print `business: <name>` (or `explicit` / `default`) and the triple when they
start.

### First-run migration

When the registry file does not exist, the first `cos business` command, or the first desktop app
start, writes `default` (active) from the built-in defaults, so whichever runs first writes the
same entry. It prints, or shows once in the app, that it did so. If the app's legacy
`settings.json` holds a different triple, the app also shows the exact
`cos business add <name> <home> <workspace> <projects>` that registers it. Other commands and
long-lived processes never write the registry. Tests point `COS_BUSINESSES` at a temp path.

### Claude runs and a new business's home

Claude runs today with home as its working directory so `CLAUDE.md` and `.claude/agents` load
(`claude.rs:111-121`); cos inlines `context/` into the prompt itself (`context.rs:26-49`). Both
files are the install's instructions, not a business's, so the working directory becomes the
install for every business; the business home still supplies `context/` and `out/`.
`cos business add` seeds `<home>/context/` from the install's `templates/context/` when it does
not exist, copying no file over an existing one, as `task setup` does today.

### CLI: `cos business`

- `list`: one line per business with `*` on the active one and whether its directories exist and
  its workspace has a profile; then `current: <name|explicit|default> <triple>` for this
  invocation.
- `add <name> <home> <workspace> <projects> [--replace]`: positional directories, so they are not
  the global flags. The name must be a slug; each directory absolute after `std::path::absolute`,
  existing and a directory, stored canonicalised (a symlinked `.chief-of-staff` is still refused
  later by `state::runtime_dir`, as today). An existing name is refused unless `--replace`. A home
  or a workspace already used by another business is refused. A missing profile is reported with
  `cos --workspace <workspace> state init`, not refused.
- `use <name>`: sets `active`; the business must exist; no profile check.
- `home [name]`: prints the home of the named, else current, business and exits 0 only when the
  name exists and its home, workspace and projects directories exist (used by `task schedule`).
- `remove <name>`: refuses the active business; deletes no directory.
- `--business <name>` and `COS_BUSINESS` work on every command that consumes a triple.

### Long-lived processes and launchd

`cos dash`, `mcp`, `serve` and `tunnel` resolve once at start. With no flag and no env they bind
the active business, so `cos mcp` (registered with no triple, `Taskfile.yml:65, 74`) follows
`cos business use` on its next start. `task schedule` bakes one business into the plists:
`BUSINESS` defaults to the active business; the task's first step runs
`cos --business <name> business home` and stops on a non-zero exit, before any other step (the
existing `mkdir -p out/logs` and `unload-retired`, `Taskfile.yml:189-191`, move after it). Then it
creates `<home>/out/logs` and substitutes `@BUSINESS@` into `ProgramArguments`
(`--business <name>`) and `@HOME_DIR@` into `StandardOutPath` / `StandardErrorPath`. The template
drops `COS_HOME`. Rescheduling is how the jobs move to another business. An existing install's
plists, still setting `COS_HOME`, keep working as an explicit env triple until `task schedule` is
rerun.

### Desktop

- The app business lives in memory. At start the app runs first-run migration if needed, then
  resolves (env layer, else the registry's active business, else defaults). `settings.json` keeps
  `slots` and `cos`; `home`, `workspace` and `projects` become `Option` fields read only by
  migration and never written again. A `settings.json` that fails to load is reported in the
  header, not silently dropped. A registry that fails to load, or whose `active` names no
  business, is shown as an error banner; the app then runs on the env layer or the built-in
  defaults and the picker shows the error instead of businesses.
- App state is one `Mutex<AppState { settings, triple }>`; every command takes one snapshot, so no
  command sees half a switch.
- If any of `COS_BUSINESS`, `COS_HOME`, `COS_WORKSPACE`, `COS_PROJECTS` set the triple at start
  (`COS_AGENT_SLOTS` does not count), the triple is pinned: the header says
  `pinned by <var>`; the picker lists the businesses and refuses to switch, naming the variable.
  The env values are read once at start and passed to the app builder, so tests inject them.
- `cos_binary()` is resolved once at start from the install, not from a business's home.
- `businesses()` re-reads the registry; per business it returns name, triple, active or not, app
  business or not, whether each directory exists, and `agents::count(home)` as
  `{ live, lost, unreadable }`. A business whose home is missing is marked `missing` with no count,
  not shown as zero.
- `agents::count(home)` uses the read-only part of `reconcile`, split out as
  `classify(spec, record) -> Live | Lost | Starting`; `reconcile` becomes `classify` then repair.
  `live` counts records with a live status that `classify` calls `Live` or `Starting`; `lost`
  counts live-status records it calls `Lost` (a supervisor that died). A run record that cannot be
  read counts as `unreadable` instead of failing the call. `count` never kills or writes.
- `switch_business(name)` re-reads the registry, checks the name exists, its directories exist and
  its workspace has a profile, and no pin is set; then swaps the triple in `AppState`. Nothing is
  written to disk. `set_workspace` and its dialog are removed.
- UI: `ui/command.js`, a command-select dialog. An app menu item "Switch business…" with the
  accelerator ⌘K / Ctrl+K opens it from anywhere, including inside the frames; the app menu keeps
  the platform's predefined Edit and Window items, so copy, paste and window commands still work; the header button,
  which shows the app business's name, opens it too. Typing filters by name, then home; arrows
  move; Enter picks the highlighted business and does nothing on an empty list; Esc closes and
  returns focus. Names render with `textContent`.
- After `switch_business`, the UI writes `#tab=<tab>&pane=<pane>` and calls `location.reload()`.
  The tab click handler (`app.js:134-152`, anonymous today) is extracted into a named
  `openTab(tab, pane)` that sets the tab, loads its frame and calls the existing `showPane`
  (`app.js:195`); the click handler and the startup hash reader both call it. Every view and every
  JS-side cache restarts on the new triple. Rust-side caches are keyed by path (the flows page
  cache, `flows.rs:673-694`), so they need nothing. If the Hire dialog holds unsaved edits, the
  picker asks before reloading.
- Every write command carries the app business it was issued for, so a request sent before a
  switch never acts on the new business: `docs_save`, `agents_spawn`, `agents_record`,
  `agents_rejudge`, `agents_stop` and `agents_clean` take a `business` argument, which `app.js`
  reads once from `paths()` at page load; the docs POST sends an `X-Cos-Business` header, which
  the docs frame reads from its URL, set when the frame loads. Each handler refuses the call when
  the name differs from the current app business.
- The Agents tab header shows `N live in other businesses` when N is non-zero, computed on picker
  open and Agents tab open; clicking it opens the picker. Each picker row shows its business's
  `live` and, when non-zero, `lost` counts.
- An agent spawned before a switch keeps running under its own home and shows when its business
  is opened.
- Spawn removes `COS_BUSINESS` from the supervisor's env and sets `COS_HOME`, `COS_WORKSPACE` and
  `COS_PROJECTS` to the spawning triple (`COS_AGENT_SLOTS` stays). The supervisor and the harness
  it starts inherit that env, so a `cos` run inside the agent resolves the spawning business
  through the env layer, not whatever is active.

### Renames taken first

`pipeline.rs:119-120, 183-184`, `track.rs:58-72, 192`, `context.rs:72-74, 203-248` and
`main.rs:775-778` rename `home` to `workspace`. A characterisation test drives `Config` with home ≠
workspace and asserts the workspace profile is read, before and after.

### Error messages

The existing missing-state errors (`main.rs:731, 777`, `track.rs:195`, `mcp.rs:185`) name
`cos --workspace <dir> state init`. The report keeps its onboarding view for a workspace without a
profile (`report.rs:77-80`).

### Docs updated in the same change

`CLAUDE.md:31`, `docs/usage.md:458`, `apps/desktop/README.md:8`, the README's usage section and
`Taskfile.yml`'s `schedule` description.

## Alternatives considered

- **Per-directory precedence** (round 1): partial overrides mixed businesses.
- **One flat precedence with an error on any `--business` + env mix** (round 2): broke the
  existing plists and inherited env in supervisors.
- **The app writes `active`, or persists its business:** the picker would retarget terminals and
  launchd, or a removed business would dangle at the next start.
- **Registry inside the app's `settings.json`:** couples the CLI to a per-platform Tauri config
  directory.
- **JSON registry:** no new dependency, but the user chose `businesses.toml`; `toml` is already
  compiled for the app.
- **Hand-listed cache reset, or a generation counter on every response:** both leave a list to
  keep complete.
- **⌘K as a window keydown listener:** misses keys typed inside the frames.
- **One dashboard or MCP server following the app's switch:** out of scope (brief, Decisions).

## Risks

- **Lost update between two `cos business` writes:** prevented by the exclusive lock and the
  re-read inside it.
- **App and CLI show different businesses:** by design; the header names the app business, and
  `cos business list` marks the active one.
- **Badge cost:** one run-record read per run per business, and finished runs are never pruned;
  measured before the badge is called done.
- **Orphaned harness in an unopened business:** only `reconcile` on an opened home kills a dead
  supervisor's harness. The picker row shows `lost` so the user can open that business, where the
  existing reconcile cleans up.
- **Env pin:** an app started from a shell with `COS_*` exported cannot switch; the header says
  which variable.
- **After `cos business use`:** `cos mcp`, `cos serve`, `cos tunnel` and a hand-started `cos dash`
  follow the new active business on their next start; the launchd jobs stay on the business baked
  at `task schedule` until it is rerun.

## Security and privacy

Local files only. Reads the registry, each business's directories and run records. Writes the
registry (`cos business` and first-run migration), `settings.json` (`slots`, `cos`), and,
unchanged, each business's own `out/` and `.chief-of-staff/`. Nothing new leaves the machine. The
keychain token for Claude stays shared.

## Semantic ISA

Vocabulary ([semantic-isa skill](../../.claude/skills/semantic-isa/SKILL.md)): an instruction is
one typed step; its core is Memory (reads state), Normative (checks) or Execution (changes the
world: files, processes); a sink is an Execution step that unchecked input must not reach; a
VERIFY is the deterministic check that clears it. Writing the registry is Execution because it
changes what every later process binds to.

### Side-effect inventory

| Capability | System | Reversible? | Affected if it misfires |
|---|---|---|---|
| `cos business add/use/remove`, first-run migration: write `businesses.toml` and create `businesses.toml.lock` beside it | local files | yes: inverse command or edit | CLI runs and newly started processes |
| `switch_business`: swap the app's triple | app memory | yes: switch back | every later app command |
| `docs_save` writes `<home>/context` (`lib.rs:151-158`) | local file | as today | the app business |
| docs POST `/docs/organize` writes `<home>/context` (`lib.rs:114-134`, `docs.rs:854-860`) | local file | as today | the app business |
| `agents_spawn` creates a worktree and a supervisor under `<home>` (`lib.rs:363-391`); new: the supervisor's env gets `COS_HOME`, `COS_WORKSPACE`, `COS_PROJECTS` set to the spawning triple and `COS_BUSINESS` removed (today it gets only `--home`, `mod.rs:422-425`) | files, process, child env | as today | the app business; any `cos` the agent runs |
| `agents_record` writes a run record (`lib.rs:404-420`, `mod.rs:515-531`) | local file | as today | the app business |
| `agents_rejudge` runs the run's contract checks through `sh -c` in its workdir (worktree, scratch or in-place repo), then writes a run record and appends the ledger (`lib.rs:212-216`, `mod.rs:498-511`, `judge.rs:281-301`) | process, local files | the checks: as written; the record: as today | the app business |
| `agents_stop` kills a process group (`lib.rs:423-426`) | process | no | the app business |
| `agents_clean` runs `git worktree remove --force` on finished runs' worktrees, then deletes each run's directory with `remove_dir_all` (`lib.rs:429-431`, `mod.rs:578-606`) | files, git metadata | no | the app business |
| Claude runs (brief, ask) with the install as working directory | process | as today | the business whose `context/` is inlined |
| `cos business add` seeds `<home>/context/` from `templates/context/` | local files | yes: delete the copies | the new business |
| `agents_list`, `agents_tail` reconcile: may kill a dead supervisor's harness and rewrite its run record (`lib.rs:167-176, 437-448`, `mod.rs:485-492`) | process, file | no | the app business |
| `businesses()` counts runs in every business: reads run records and runs `ps` once per live record to check its supervisor (`is_cos`, `mod.rs:659-671`); writes nothing, kills nothing | process (`ps`), reads | n/a | nothing |
| `task schedule`: checks the business first, then removes the retired `cos.review` job, creates `<home>/out/logs`, writes `~/Library/LaunchAgents/cos.{brief,dash}.plist`, reloads them with `launchctl` | local files, launchd | yes: `task unschedule` | the scheduled brief and dashboard |

The report handler (`lib.rs:85-99`) only reads.

### Instructions

| Instruction | Core | Sink? |
|---|---|---|
| `LOAD(registry)` | Memory | no |
| `VERIFY(business_name)` | Normative | no |
| `VERIFY(business_dirs)` | Normative | no |
| `VERIFY(write_business)` | Normative | no |
| `VERIFY(run_paths)` | Normative | no |
| `STORE(registry)` | Execution | **yes** |
| `TOOL_CALL(switch_business)` | Execution | **yes** |
| the app commands in the inventory | Execution | **yes** (existing) |

Arguments: `add { name, home, workspace, projects, replace }`, `use | remove { name }` (clap
rejects unknown arguments); `switch_business` takes a `{ name }` struct with
`#[serde(deny_unknown_fields)]`.

### Taint and sinks

Sources: CLI arguments, the picker's selected name, the registry and `settings.json` (both
user-editable), `COS_*` env, the run id a command names, and commands an agent runs: an
agent can call `cos business add | use | remove` like any user, and those calls pass through the
same `STORE(registry)` checks; the change shows in `cos business list`. `settings.json`'s `cos` names the
supervisor binary; it is read once
at start, as today, and a switch does not change it.

Run records (`out/agents/<id>/spec.json`, `state.json`) are trusted, as today: cos writes them at
spawn, they sit in the user's own home, and a hand edit to one already steers today's rejudge and
clean. This design does not add tamper checks on their fields (`workdir`, `repo`, `worktree`, the
contract commands); a record check that reads one field to clear another clears nothing. What it
adds is binding: a run command acts only on a run that belongs to the app business.

| Sink | Cleared by | Check |
|---|---|---|
| `STORE(registry)` | `VERIFY(business_name)` + `VERIFY(business_dirs)` | slug; for `add`, absent unless `--replace`, each directory absolute, existing, a directory, stored canonicalised, home and workspace unique; for `use` and `remove`, the name exists; `remove` refuses the active business |
| `add` seeds `<home>/context/` | `VERIFY(business_dirs)` | runs only after the entry passed the checks above; copies only files under the install's `templates/context/`, creates `context/` only when absent, and never overwrites an existing file |
| `TOOL_CALL(switch_business)` | `VERIFY(business_name)` + `VERIFY(business_dirs)` | name in the registry read during this call; directories exist; workspace has `profile.json`; no pin |
| app commands in the inventory | the switch's VERIFY, plus the rows below | each receives a triple that passed `switch_business` or the startup resolution, in one snapshot |
| every write command (`docs_save`, docs POST, `agents_spawn`, `agents_record`, `agents_rejudge`, `agents_stop`, `agents_clean`) | `VERIFY(write_business)` | the business named by the call equals the current app business |
| every function in `agents::mod` that takes a run id: `read`, `record`, `stop`, `clean`, `rejudge` (`read` reconciles: rewrites `state.json` and may kill a harness group, `mod.rs:450,469-493`; rejudge then runs the contract checks, `judge.rs:281-301`; clean runs `git worktree remove --force` and `remove_dir_all`, `mod.rs:578-606`). Their callers: the app's `agents_record`, `agents_tail`, `agents_rejudge`, `agents_stop`, `agents_clean`; the CLI agent subcommands (`main.rs:622-702`, including `clean` with no id, which takes ids from `list`); MCP `stop_agent` (`mcp.rs:298-301`) | `VERIFY(run_paths)` | Invariant: every run id becomes a path through one function, `agents::run_dir(home, id)`, called first in each of those five functions, so no caller can skip it. It checks, before anything is read through reconcile: the id is exactly one `Component::Normal` (not empty, `.` or `..`, no separator); `<home>/out/agents/<id>` is a directory (not a symlink) and a direct child of `out/agents`; `spec.json`, read directly, has `spec.id` equal to `<id>`; every later path and reconcile uses `<id>`, never `spec.id`. `home` is the app business's home in the app and the resolved triple's home in the CLI and MCP. Otherwise refused with nothing read, written, run or killed. Past the check, the run proceeds exactly as today. An unparseable `spec.json` is refused. `run_dir` is public and returns a `RunDir` handle whose `path()` and `events_path()` replace the raw-id helpers `dir` and `events_path` (`mod.rs:260,264`), which are removed; their outside callers, the app's `agents_tail` (`lib.rs:443-444`) and the CLI's `show` (`main.rs:628`), call `agents::run_dir(home, id)?` first. `write_state` (`mod.rs:610`, already `pub(crate)`) takes a `&RunDir`. `supervise` (`supervise.rs:34`) reaches the id first through `read` (`supervise.rs:38`). `list` (`mod.rs:455-464`) returns `Listing { runs: Vec<(Spec, State)>, skipped: Vec<String> }`: an entry whose `run_dir` or `read` errs is skipped and its id recorded, so one bad directory never aborts `cos agents list`/`watch`, `clean` with no id, the dashboard, `agents_list` or MCP list. Each of those uses `runs` and drops `skipped`, except the CLI, which prints `skipped: <ids>` on stderr; `clean` with no id does not clean a skipped id. `businesses()` uses `agents::count`, not `list` |
| `task schedule` writes the plists and loads them | `VERIFY(business_name)` + `VERIFY(business_dirs)` | its first step, `cos --business <name> business home`, exits 0 (the name exists and its three directories exist); otherwise the task stops before any other step, with no directory created and no plist written or removed |

### Violations

| Violation | Reported | Recovery |
|---|---|---|
| unknown or non-slug name | the name and the known names | nothing changes |
| directory missing, relative or not a directory | field and path | nothing changes; message names `cos business add` |
| workspace without a profile at switch | the workspace | nothing changes; message names `cos --workspace <dir> state init` |
| registry unparseable, bad name, dangling `active` | file and key | commands that reach the registry layer stop; a command given directories by flags or env (for example `cos --workspace <dir> state init`) never reads the registry and still runs, as do `agents supervise` and `agents build-slot`; file untouched |
| two sources in one layer | both names | the command stops |
| remove the active business | the name | `use` another first |
| switch while pinned | the variable | the triple stays |
| a write command naming a business other than the app business | both names | nothing written, run or killed |
| a run id, from any caller (app, CLI, MCP), that is not one plain segment, has no directory under the business's `out/agents/` (or a symlink there), or whose record's `spec.id` differs from it | the run id and the path | nothing read, written, run or killed; `list` leaves it out and reports its id, so clean with no id and every listing go on with the rest |
| `task schedule` with an unknown business or missing directories | the name or path | nothing created, written or removed |

### ISA test matrix

| Case | Expectation |
|---|---|
| `add` with a slug and existing directories | entry written |
| `add` with `../x`, uppercase, or an existing name without `--replace` | refused; file byte-identical |
| `add` with a relative or missing path, or a home or workspace another business uses | refused; file byte-identical |
| `switch_business` to an unknown name, a deleted workspace, or while pinned | refused; triple unchanged |
| corrupt registry | commands reaching the registry error with file and key; `cos --workspace <dir> state init` and `agents build-slot` still run |
| `cos business use nope` | refused; file byte-identical |
| `businesses()` with a dead supervisor in another home | its run record byte-identical; no process killed |
| after a switch: `docs_save`, docs organize, `agents_spawn`, `agents_record` | each writes under the new business's home |
| a write command (each of the seven) naming the previous business, arriving after a switch | refused; nothing written, run or killed in either business |
| `agents::read`, `record`, `stop`, `clean`, `rejudge` called directly with run id `""`, `.`, `..`, `../x`, `a/b`, or an id that exists only in another business | refused; every `state.json` byte-identical, no process killed, nothing removed |
| app `agents_tail` and `agents_record`, CLI `cos agents show <id>` and `kill <id>`, MCP `stop_agent` with id `../x` | refused through the same check; nothing written or killed |
| `list` with one symlinked run directory and one whose `spec.id` is stale, beside two good runs | the two good runs listed, the two bad ids reported; `cos agents clean` with no id cleans the good finished run and skips the two |
| the same three on a run whose `spec.json` says `"id": "<other run>"` | refused; the other run's `state.json` byte-identical and its harness alive |
| `agents_rejudge` on an unedited worktree, scratch and in-place run of the app business | each re-judged, as today |
| `agents_clean` on a worktree run whose tree was deleted by hand | no `git worktree remove`; the run directory is deleted, as today |
| `task schedule BUSINESS=nope` | stops at its first step; `out/logs` not created; `cos.brief`, `cos.dash` and `cos.review` plists byte-identical |

## Testing

- `tests/business.rs`, inputs passed as values, each test with its own registry temp path:
  - registry round trip and load errors;
  - resolver: each layer alone; the higher layer winning (`--business` over `COS_HOME`;
    `--home` over `COS_BUSINESS`); each in-layer conflict an error; `COS_HOME=/h` with an active
    business gives `(/h, /h, $HOME/Projects)`, and `COS_PROJECTS=/p` alone gives
    `(<install>, <install>, /p)`: partial overrides fill from the built-in defaults, never from the
    active business (the user's decision); defaults equal the literal build-clone path and
    `$HOME/Projects`;
  - writes: an unwritable temp directory leaves the file byte-identical; 50 rounds of two
    concurrent `add` calls with different names end with both present every time.
- `tests/business_cli.rs` through the binary: `add`, `use`, `list`, `remove`; first-run migration
  writes `default` from the build clone, never from `add`'s arguments (`add acme …` on an empty
  registry writes `default` and `acme` with different homes); `agents build-slot` runs with a
  corrupt registry.
- Agents: `classify` matches today's `reconcile` decisions on the existing fixtures; `count` leaves
  a dead supervisor's run record byte-identical and counts an unreadable record as `unreadable`;
  spawn with `COS_BUSINESS` or `COS_WORKSPACE` set in the parent reaches `running`, and a
  `cos business list` run by the harness prints the spawning triple as `current: explicit …`.
- Claude: `claude::command` for a business whose home has no `CLAUDE.md` sets the working
  directory to the install; `cos business add` on a home without `context/` seeds the template
  files and leaves an existing `context/goals.md` unchanged.
- Schedule: `task schedule BUSINESS=acme` writes plists whose arguments carry `--business acme`,
  whose log paths are under acme's home, and which set no `COS_HOME`; `task schedule BUSINESS=nope`
  creates nothing and leaves every plist byte-identical.
- CLI: `cos business home acme` exits non-zero after acme's projects directory is removed;
  `cos --workspace <dir> state init` succeeds with a corrupt registry.
- Characterisation test for the renamed profile reads.
- `apps/desktop/tests/ipc.rs`: the `Settings` literals (`ipc.rs:107-113, 295-301, 441-447`) are
  edited to the new fields; the builder takes a registry path and the env values; the
  `set_workspace` test (`ipc.rs:195`) becomes `switch_business`. Cases: switch changes `paths()`,
  the report and flows; unknown name and pin refused; after a switch, `docs_save`, docs organize,
  `agents_spawn` and `agents_record` write under the new home; each of the seven write commands
  naming the previous business is refused; `agents_rejudge`, `agents_stop`, `agents_clean`,
  `agents_record` and `agents_tail` refuse run ids `""`, `.`, `..`, `../x`, `a/b` and another
  business's id, and a record whose `spec.id` names another run, leaving every `state.json`
  byte-identical and the other run's harness alive. `tests/agents.rs` calls `read`, `record`,
  `stop`, `clean` and `rejudge` directly with the same ids, and drives `cos agents show ../x`,
  `cos agents kill ../x` and the MCP `stop_agent` tool with `../x`: each refused, nothing written
  or killed; `list` over a symlinked run directory, a stale-`spec.id` one and two good runs returns
  the two good runs and reports the other two ids, and `cos agents clean` with no id goes on past
  them; `tests/fit.rs:260` and `tests/hiring.rs:415` move to `Listing.runs`;
  `agents_rejudge` re-judges an unedited worktree, scratch and in-place run; `agents_clean` deletes
  the run directory of a worktree run whose tree is gone; a business with a missing home is
  reported `missing`;
  `businesses()` leaves another home's dead run record untouched; first app
  start writes `default` from the built-in defaults and, with a different legacy triple, returns
  the `cos business add` hint.
- UI hand-check in the real app, recorded with `pr-proof` (the PR evidence report): ⌘K with focus
  in the app, then inside the report frame, then inside the docs frame; type, arrows, Enter (reload
  onto the same tab and pane, including Flows and Docs, whose frames load), Esc (focus returns),
  Enter on an empty list (nothing), badge click, copy and paste in the Docs editor and the Hire
  dialog after the menu change, and a start with `COS_HOME` set (pin shown, switch refused).

## Confidence

Rock: understood well enough to commit to. Sand: likely to shift once built.

- Registry, resolver, CLI, migration: **rock**.
- Desktop switch and the write-business guard: **rock**.
- Reload with `openTab` hash restore: **sand** until the hand-check reloads onto each tab.
- ⌘K as a menu accelerator inside sandboxed frames: **sand** until the hand-check passes.
- Badge: **sand**. Picker open under 200 ms with every business's runs counted, measured on this
  machine's registry, moves it to rock.

## Accepted trade-offs

Review round 3 was the last full EGM re-audit (EGM allows one re-audit per stage), followed by
three Goldfish verification rounds. Every blocking and should-fix finding from them is fixed
above; the partial-override fill is the user's decision (Decision). These remaining ones are
accepted, each with its reason:

- **Badge latency** (EGM M8): the 200 ms gate is measured during Build; if it fails, the badge
  moves to Agents tab open only. Measuring first was the intermediate reviewer's call.
- **Path newtypes for home, workspace and projects** (EGM M16): the `Triple` struct keeps the three
  together; separate types add churn across every call site for a mix-up the resolver tests
  already catch.
- **Orphaned harnesses in unopened businesses** (Risks): cleaned when that business is opened, as
  today for one business; the picker shows the `lost` count that tells the user to open it.
- **Shared workspace or home in hand-edited registries**: `add` refuses them; a hand edit that
  creates one is the user's choice and is not re-checked on load.
- **Existing agent behaviour this change does not touch:** `agents_stop` and `reconcile` kill the
  process group a run record names, guarded today by `harness_group` and `is_cos`
  (`mod.rs:668-671, 685-689`); `agents_spawn` accepts any absolute repo the hire resolves
  (`hire.rs:419-424`); the contract checks `agents_rejudge` runs are the commands stored at hire
  time. Run records are trusted as today (Taint and sinks): a hand edit to a record's `workdir`,
  `repo` or commands still steers rejudge and clean. This design neither adds nor widens any of
  them; it only routes them to the app business and adds the business and run-id guards above.
  Hardening record fields is separate work (the user's decision, 2026-11-12).
- **Wording and line-reference nits** across all rounds: corrected where they changed
  meaning; the rest are left to the implementation PR, where the code replaces the line numbers.

## Acceptance

Preconditions: `mkdir -p ~/biz/acme/repos`, then `cos --workspace ~/biz/acme state init`, and
acme's `profile.json` approves a `tasks` source so its Flows tab reads (`flows.rs:698-699`).

1. With no registry, `cos business list` writes `default` (active) from the build clone, prints
   that it did, and lists `* default`.
2. `cos business add acme ~/biz/acme ~/biz/acme ~/biz/acme/repos` adds `acme`, seeds
   `~/biz/acme/context/` from the templates, and leaves `default`'s home unchanged.
3. `cos --business acme report --no-open` writes `~/biz/acme/out/report.html`.
4. In the app, from the Agents tab with focus inside nothing, then inside the report frame: ⌘K,
   type `ac`, Enter. The window reloads on Agents; the header shows `acme`; Today, Agents (Runs,
   Staff), Flows and Docs show acme's data. ⌘K, `de`, Enter restores `default`.
   `cos business list` still marks `default` active.
5. With an agent running in `default`, the app on `acme` shows `1 live in other businesses`;
   clicking it opens the picker, whose `default` row shows `1 live`.
6. With `acme` active, `COS_HOME=$HOME/Projects/chief-of-staff-v2 cos business list` prints
   `current: explicit $HOME/Projects/chief-of-staff-v2 $HOME/Projects/chief-of-staff-v2
   $HOME/Projects` (workspace and projects from the built-in defaults, not from acme); the app
   started with that variable shows `pinned by COS_HOME` and refuses a switch.
7. `cos --business acme --home /tmp report` exits non-zero naming both; `COS_HOME=/tmp cos
   --business acme report --no-open` writes acme's report (flag beats env).
8. `task schedule BUSINESS=acme` creates `~/biz/acme/out/logs`; then
   `launchctl kickstart gui/$UID/cos.brief` writes a brief under `~/biz/acme/out` and its log to
   `~/biz/acme/out/logs/brief.log`.
9. Every row of the ISA test matrix passes.
