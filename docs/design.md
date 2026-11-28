# Design

Adapted from the Agent SDK cookbook
[The chief of staff agent](https://platform.claude.com/cookbook/claude-agent-sdk-01-the-chief-of-staff-agent),
in Rust.

## The starter kit is the authority

The Chief of Staff starter kit 0.1.0 (`START-HERE.md`, `plugins/chief-of-staff-starter/`,
`guides/`, its design at `docs/plans/2026-10-16-chief-of-staff-starter-design.md`)
defines what a chief of staff does here: goals → KPIs → weekly deliverables →
priorities → delegation, a private journaled workspace, read-only sources
and drafts by default, coverage that is never overstated, and handoffs that
transfer context, not authority. Where `cos` and the kit disagreed, the kit
won; `cos` is the engine that runs it.

| Kit | In `cos` |
|---|---|
| `scripts/state.py` | `src/state.rs`: same files, rules and messages, same `write.lock`. `tests/state.rs` ports all 21 kit tests and runs both validators over 25 candidates; each reads what the other commits |
| `references/review.md`: ≤ 3 actions, ≤ 2 changes, DECIDE / DO / DELEGATE / BATCH, coverage kept honest | `Brief` (`src/reports.rs`): `needs_you` (schema `maxItems` 3, cut in code too), `what_changed` (2), `backlog`, `coverage` with health and completeness apart |
| `references/strategy.md`: weekly steering, outcomes not outputs | `Brief.week` (`Week`): a `GoalCheck` per goal with KPI, this week's evidence, bottleneck, steer and what would change it; `proposed` deliverables, `decisions`, `stop_doing`. Judged in the daily brief, before today's actions, so today is ranked against it |
| `references/delegation.md`: nine-part handoff | `src/handoff.rs`, from the state, no model; `cos handoff`, MCP `handoff`, the Plan tab, `cos agents spawn --deliverable` |
| profile `sources[].readsApproved` | `context::tools`: only approved sources' connectors reach a brain; `context::local_reads`: `gh` / `cn` run only for an approved `code` / `tasks` source; `pipeline::cover_every_source`: each approved source gets a coverage row |
| `references/review.md`: calendar is this week plus tomorrow when outside it | `pipeline::dates_at` computes the window, the prompt carries exact bounds, `hold_to_window` drops entries dated outside it |
| "save only approved local tracking" | `src/track.rs`: run receipts and source health committed only when `localTracking.approved`; coverage recorded `partial` at most, watermark never advanced |
| `guides/OPTIONAL-DASHBOARD.md` | `cos dash` / the report: Today (priorities), Plan (goals, KPIs, this week, sources), Steering, History; loopback only, no remote scripts, 320 px without sideways scroll |
| Codex plugin + marketplace | unchanged, plus `.claude-plugin/` manifests so Claude Code installs the same skill directory |

Briefs saved before the kit's shape still load: `Brief` deserializes through
a wire type that folds `must_do`, `celebrations` and the rest into the new
fields, and a brief with no `week` loads with an empty one. One run a day
does both horizons: a separate weekly review left the report showing a
Friday steer beside today's actions, and the brief never read it. Saved
`out/reviews/` are no longer read by anything.

## Transport

There is no Rust Agent SDK. The Python and TypeScript SDKs spawn the `claude`
CLI and speak `stream-json` over its stdio; `cos` does the same
(`src/claude.rs`). That keeps everything the CLI already has: the claude.ai
connectors (Gmail, Calendar, Atlassian, Notion), `.claude/agents`, and
`--json-schema` structured output, which arrives on the `result` event as
`structured_output`.

A direct-API loop (`rig` + `rmcp`) was rejected: it would mean re-wiring OAuth
for four MCP servers and rebuilding subagents by hand.

| Cookbook piece | Here |
|---|---|
| `CLAUDE.md` company context | `CLAUDE.md` + `context/goals.md`, `principles.md`, `decisions/` |
| `.claude/agents/financial-analyst.md` | `.claude/agents/tactician.md` |
| `scripts/*.py` called through Bash | `src/gather.rs` — `cn` and `gh`, run by `cos`, not by the model |
| `/budget-impact` slash command | `cos brief`, `cos ask` |
| PostToolUse `report-tracker.py` hook | `out/audit/YYYY-MM-DD.jsonl`, written from the event stream |
| `executive` output style | the `Answering a question` rules in the tactician's prompt + the typed brief |
| `send_query()` | `claude::execute` |

## `cos ask`: one run

The Agent tool runs subagents in the background, and `claude -p` emits its
result before they report: the first live run returned "both brains are still
running" for $0.85. With one brain there is nothing to route or merge, so
`cos ask` is a single `claude -p --agent tactician` run on the same grounding
as a brief (workspace, `context/`, local facts read fresh), under one deadline.

## Cost

Every headless run pays to cache its context. Measured on 2026-10-15 for a
one-word reply (Sonnet):

| Config | Cache-write tokens | Cost |
|---|---|---|
| default (user rules, skills, all connectors) | 125,695 | $0.50 |
| `--tools ""` (drops ToolSearch, so MCP tools load eagerly) | 209,597 | $0.84 |
| `--setting-sources project`, unused servers denied, ToolSearch kept | 42,465 | $0.17 |

`cos` uses the last for every run. Which servers are "unused" is per person,
so the deny list is data: `context/deny-mcp.txt`, seeded empty from
`templates/context/`. Live run, before the brief carried the week part:
`cos brief` $0.32 / 9 turns; the brief with its week part is not yet
measured. Every run carries `--max-budget-usd`: the brief defaults to $3.00,
the weekly review's $5.00 opus cap at sonnet's price (about half per token)
plus headroom for today's part, which shares the same connector reads; `ask`
defaults to $2.00.

## Safety

- Read-only: `--permission-mode dontAsk` plus an explicit allowlist
  (`READ_ONLY` in `src/lib.rs`). Send, draft, trash, Jira edits and file
  writes are denied, not merely discouraged.
- No file tools. A brain's only built-in tool is `ToolSearch`
  (`BRAIN_TOOLS` in `src/claude.rs`); `context/` is inlined into the prompt
  by `src/context.rs`, capped at 64 KiB, newest decision first. A brain reads
  mail and tickets other people wrote, and a brain that can `Read` any path
  can be told by one of them to quote `~/.ssh` into a brief that `cos tunnel`
  then serves. The agent prompts also treat instructions found in that data
  as suspicious items, never as instructions. `tests/safety.rs` pins both.
- Every child runs in its own process group under one deadline (≤ 300 s);
  on expiry the group is killed and reaped (`src/proc.rs`). A pipe still
  open 2 s after its child exited belongs to a leftover in the group, which
  is killed, so a grandchild cannot hold a run open past its deadline.
- With a workspace profile, a brain's connectors are only those of the
  sources the user approved; none approved means none. The state a brain
  sees is the validated open work; a state that fails validation is not
  inlined at all.
- `cos` writes the workspace state only through `state::commit`, only when
  local tracking is approved, and never advances a review watermark on the
  model's say-so (`tests/plan.rs`).
- `out/`, `context/` and `.chief-of-staff/` are gitignored: briefs quote mail
  and calendar, and the other two hold personal goals and state.

## MCP: Claude Desktop, Codex, claude.ai

`cos mcp` (stdio) and `cos serve` (streamable HTTP) expose the same server,
built on `rmcp` 3.5 (`src/mcp.rs`).

| Mode | Tools | Why |
|---|---|---|
| stdio (`Local`) | all of them | runs as you, on your machine |
| HTTP (`Remote`) | `latest_brief` only | a leaked URL must not spend money or start runs |

The remote filter sits in both `list_tools` and `call_tool`, so a client that
calls an unlisted tool by name gets `-32602 tool … is local-only`. HTTP is
served at `/mcp/<token>` (≥ 32 chars); every other path is a 404. Verified
2026-10-16 with curl, when two read tools existed: wrong path 404, two tools
listed, `brief` refused. Since then the remote list is `latest_brief` alone
(not re-verified over HTTP). The remote server takes no `business` argument
(`-32602`), builds no daemon client, and serves the business it started on.

Full design: [MCP client bridge](plans/2026-11-25-mcp-client-bridge-design.md).

### Paid tools return at once

`brief` and `ask` start a run and answer with a `RunView` straight away;
nothing blocks for the minutes a model run takes (`src/bridge.rs`).

| Tool | Input | Returns |
|---|---|---|
| `brief` | `{business?}` | `RunView`; while a brief for that business runs, that run with `existing: true`, and nothing is spent |
| `ask` | `{question, business?}`, question 1 to 4000 chars | `RunView` |
| `run_status` | `{run, business?, wait_secs?}`, wait 0 to 50 s | `RunView`, plus `report`, `markdown`, `day`, `cost_usd` once a brief is done, `answer` once an ask is done |
| `runs` | `{business?, limit?}`, limit 1 to 20, default 5 | `{runs: [RunView]}`, newest first |
| `businesses` | none | `{businesses: [{name, home, default}], pinned}` |

`RunView` is `{run, business, kind, state, reason?, link?, existing?, day?,
report?, markdown?, cost_usd?, answer?}`. `link` is
`cos://run/<business>/<run>` for a registered business, and absent when the
server runs on unregistered directories (`business` then reads `default` or
`explicit`). `run_status` and `runs` read run records only,
so they answer with the daemon down and for runs any process started. 50 s
of waiting fits inside Codex's 60 s default tool timeout. The MCP App view's
"Run new brief" calls `brief`, then `run_status` with `wait_secs: 45` until
the run ends.

### Runs and the brief lock

Every `cos brief` and `cos ask`, whoever starts it (the daemon, `cos mcp`,
the app, a terminal, the `cos.brief` launchd job), writes a run record at
`<home>/out/run-records/<id>.json` (`src/runs/`). Ids look like
`r-20261125-091502-k3x9`. States: `queued`, `running`, then one of `done`,
`failed`, `deferred`, `stopped`, `lost`; a final state never changes. Each
write takes `<id>.lock` and re-reads the record first.

A brief takes `out/run-records/brief.lock` before it writes anything and
holds it until it exits; a second brief for the same business exits 73,
naming the holder. Asks take no lock. Each grandchild (`claude`, `git`, `sh`)
is spawned through a gate (`proc::spawn_gated`): it execs only after its
process group is in the record. Before any final state is written, every
recorded group is killed and waited on (2 s); a group still alive at that
bound leaves the record non-final for the next sweep. A run started with
`COS_OWNER=<pid>/<start>` ends itself (exit 76, `lost`, `owner gone`) when
that owner dies. Every run start sweeps its business's dead runs. Run
records need process start times; where the platform gives none, a run
refuses to begin.

### The daemon

`cos daemon` (`src/daemon/`) owns the paid runs MCP clients start, for every
registered business, and follows every business's hires every 2 s. `task
install` loads it under launchd as `cos.daemon` (`cos daemon install`;
`KeepAlive` unless it exits 0).

- **Socket**: `cos/daemon.sock` under the user temp dir
  (`confstr(_CS_DARWIN_USER_TEMP_DIR)`), directory 0700, socket 0600.
  `daemon.lock` beside it keeps one daemon; a second exits 0, "already
  running".
- **Log**: it writes its stderr to `daemon.log` beside the socket (appended,
  0600), whoever started it.
- **Requests**, one JSON line each: `hello` (`{version, pid}`), `start`,
  `stop`, `subscribe` (a stream of record changes under every registered
  home, changes only), `drain` (refuses every later `start` and replies with
  the pid; fails, changing nothing, while a run is live), `undrain`. Lines are capped at 64 KiB, an idle connection closes after 5 s, at
  most 16 connections are open. A `start` carries a `client_req`; a repeat
  gets the first reply.
- **Limits**: one brief per business (the brief lock), 2 live asks per
  business, 4 live runs in total; each run's deadline is 420 s.
- **Sweep** at start: records of runs whose process or owning daemon is gone
  become `lost` once their groups are gone. The daemon's own children (the
  judge's checks, `git`, `hc`) are mirrored to `daemon.groups.json` and swept
  the same way.
- **Stop**: SIGTERM stops every run it owns, then exits 0. A `stop` whose run
  keeps a live group past the bound is an error naming the run; the next
  sweep ends it. At shutdown the daemon kills its own children and refuses
  new ones, so a hire's verdict that needs a check after that point is
  dropped.
- **Upgrade**: when its executable changes, no run it owns is live and it is
  not drained, it exits 1 and launchd starts the new binary. `cos daemon
  install` sends `drain` first, so the reload kills no run and a racing
  `start` is refused; it refuses (exit 75) while a run is live, unless
  `--force`, which stops the runs `drain` named and drains again. With no
  daemon listening it checks the records instead (exit 75 on a live daemon
  run, `--force` or not); a daemon that does not answer fails the install.
  If the drained daemon still answers after `bootout` (one started from a
  shell), install fails naming its pid; every failure after a drain sends
  `undrain`.

**Clients.** A client connects to the socket: `ENOENT` or `ECONNREFUSED`
within 200 ms means the daemon is down; any other error is reported and
never falls back, so a lost reply cannot start a second paid run. With the
daemon down, `cos mcp` spawns the run itself (owner `mcp`) under the
daemon's caps and kills and reaps it when its client closes stdin.

**The app.** The desktop window is a daemon client too. Its subscribe thread
follows runs from any client: on connect it seeds from the record files, then
emits `brief-changed` (both kinds, with the run id) and `report-changed`
(`{run}`) on each change, reconnecting every 2 s. `Run brief` sends `start`
when the daemon answers and otherwise owns the run itself (owner `app`). A
`cos://run/<business>/<run>` or `cos://brief/<business>/<day>` link opens the
window on a run panel (state, owner, Stop where this window or the daemon
owns the run, an ask's answer as markdown) or on that day's brief; a link
only navigates, and switches business only through `switch_business`.
`cos://settings/assistants` opens Settings → Assistants, a card over
`cos connect` that shows each client's health, offers Register, Repair and
Remove, and probes the daemon itself. The business picker marks the MCP
default and offers Make default. `cos mcp` follows the hires it
started only while the daemon does not answer `hello` for that business.

### Business per call

Every local tool takes an optional `business`. A `cos mcp` started with
`--business`, `--home`/`--workspace`/`--projects`, `COS_BUSINESS` or
`COS_HOME` is pinned: it acts on that business and refuses another name
(`pinned to X: this server was started with …`). An unpinned server reads
the registry on each call: the named business, or the registry's active one;
an unregistered name is refused with the known ones.

### Registration: `cos connect`

`cos connect claude-desktop|codex [--repair] [--bin <abs>]` writes only the
`cos` entry (`command`, `args = ["mcp"]`, `env.PATH`; Codex also
`tool_timeout_sec = 320`), keeps the pre-`cos` file as `<config>.cos.orig`,
and refuses a file that changed between read and write (`src/connect/`).
`cos connect status` checks each client's entry, its command and its
`--version`. `task install-mcp` runs `cos connect claude-desktop --repair`
and `cos connect codex --repair`, each only when that client is installed.

Saved runs are stored as JSON next to the markdown (`src/store.rs`), so the
free tools return typed `structuredContent`. The brief tools carry
`_meta.ui.resourceUri = ui://cos/dashboard`; that resource is
`src/dashboard.html` with mime `text/html;profile=mcp-app`, speaking the MCP
Apps postMessage protocol (2026-01-26) without the JS SDK.

`cos tunnel` (`src/tunnel.rs`) adds a Cloudflare quick tunnel. Verified
2026-10-16 through the public URL: wrong path 404, two tools listed,
`latest_brief` returned the day's brief. `cos` owns `cloudflared` in its own
process group: SIGINT/SIGTERM/SIGHUP kill and reap it (tested: no process
left, pidfile removed); a SIGKILLed run leaves it orphaned to PPID 1, and the
next start stops it via `out/tunnel.pid` (tested).

Claude Desktop, 2026-10-16: `latest_brief` returned text in a task-view chat,
and the server log shows Desktop fetched `ui://cos/dashboard`
(`resources/read`), but no dashboard rendered. Not yet checked in a plain chat.

Not verified: Claude Dashboards over the remote server. Codex Spaces hold
pages, sites and images; MCP servers do not appear there, and no documented
way to populate a Space from a server was found.

## Agents: from prompt to running work

A brief hands out one `agent_prompt` per action and one per goal of its week
(`--from goals`); every tracked deliverable has a handoff. `cos agents spawn --from brief --item 2`
or `--deliverable D-1` runs it instead of leaving it to be pasted
(`src/agents/`).

| Piece | Where | Does |
|---|---|---|
| spawn | `agents::spawn` | refused without herdr-core (`hc server status`) or a free slot; cuts a detached worktree from the repo the prompt names (`~/Projects/<repo>`), writes `spec.json`, seeds the worktree, then `hc run --bg --no-parent` a terminal `cos-<id>` labelled `hire=<id>` |
| harness | `agents::harness` | interactive, the work as its first message: Claude Code `acceptEdits` with the Seatbelt sandbox (strict network allowlist, no unsandboxed retry); Codex `--sandbox workspace-write --ask-for-approval on-request`. The command line pins the business that hired it |
| follow | `agents::follow`: `cos daemon` for every registered business and the desktop app, each every 2 s; `cos agents watch`; `cos mcp` while the daemon does not answer | one bounded `hc get` per live terminal; at the deadline (default 1800 s, ceiling 7200 s) `hc interrupt`, then `hc kill` 10 s later; judges once under `judge.lock` on `done` after the first message, an exit, the deadline or Mark done |
| talk | `cos agents say / pause / done / open / screen`, the run page | `hc send`, `hc interrupt`, `hc kill` then the judge, focus in the Herdr window, `hc read`; each reads the terminal back and checks its `hire` label first |
| monitor | `cos agents ls / watch / show`, `cos dash` → `/agents`, MCP `agents` | status, what Herdr says (blocked shows as waiting on you in Herdr), the verdict |
| stop | `cos agents kill`, MCP `stop_agent` | `hc kill` for a live terminal, its last screen kept; a hire still being prepared is told to stop by a `stop` file |
| reconcile | every read | a hire whose preparing process is gone before Herdr had it becomes `lost`; a run from before Herdr reads, never resumes |
| clean | `cos agents clean` | removes finished worktrees and their Herdr terminals; refuses one with uncommitted changes or commits past its base unless `--force` |
| skills | `agents::skills`, `skill_facts`, `skill_bundle`; `cos skills`, the Skills pane | the plan resolves a skill set from skill facts in Prime and a local catalog of the skill roots; a Claude hire gets it as a session plugin under its run directory with the rest switched off ([design](plans/2026-11-28-hire-skill-graph-design.md)) |

The agents are the one place `cos` writes outside `out/`: in their own
worktree. Herdr owns every agent process; cos owns none, and no CLI call
waits on one. Shell commands get no network in either harness, so a push, a
PR comment or a mail asks you in Herdr. Herdr measures no spend, so a run in
Herdr has no cost recorded and no spend cap; its deadline is the cap.

`tests/agents.rs` drives a stand-in herdr-core (`tests/fixtures/fake-hc.sh`)
and a stand-in agent (`fake-agent.sh`): the terminal's name, label and argv,
judged once on done, the deadline's interrupt then kill, blocked as waiting
on you, reply, pause, mark done, exits, removals, slots and clean refusing
work. No test reaches the real core.

## Not built

- An `--act` mode (drafting replies, moving tickets). Add per tool to
  `READ_ONLY`'s sibling list when wanted, never by widening to a whole server.
- Multi-turn sessions (`--resume`). Every run is stateless; memory is the
  workspace state and the `context/` files.
- From the kit's optional dashboard: **Mark reviewed** and any other write
  from the page. The page is read-only; acknowledgments go through
  `cos state commit` or the skill.
- A scheduler that follows the kit's scheduling reference. `task schedule`
  runs `cos brief` on weekdays under launchd, with tracking gated on the
  profile; there is no quiet-run notification path.
- Automatic rate-limit recovery. Off in the kit, and off here.
