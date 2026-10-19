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
| `references/strategy.md`: weekly steering, outcomes not outputs | `Review`: a `GoalCheck` per goal with KPI, weakened assumption, bottleneck, steer and what would change it; `proposed` deliverables |
| `references/delegation.md`: nine-part handoff | `src/handoff.rs`, from the state, no model; `cos handoff`, MCP `handoff`, the Plan tab, `cos agents spawn --deliverable` |
| profile `sources[].readsApproved` | `context::tools`: only approved sources' connectors reach a brain; `context::local_reads`: `gh` / `cn` run only for an approved `code` / `tasks` source; `pipeline::cover_every_source`: each approved source gets a coverage row |
| `references/review.md`: calendar is this week plus tomorrow when outside it | `pipeline::dates_at` computes the window, the prompt carries exact bounds, `hold_to_window` drops entries dated outside it |
| "save only approved local tracking" | `src/track.rs`: run receipts and source health committed only when `localTracking.approved`; coverage recorded `partial` at most, watermark never advanced |
| `guides/OPTIONAL-DASHBOARD.md` | `cos dash` / the report: Today (priorities), Plan (goals, KPIs, this week, sources), Steering, History; loopback only, no remote scripts, 320 px without sideways scroll |
| Codex plugin + marketplace | unchanged, plus `.claude-plugin/` manifests so Claude Code installs the same skill directory |

Runs saved before the kit's shape still load: `Brief` and `Review`
deserialize through wire types that fold `must_do`, `celebrations`, `bets`,
`drift` and the rest into the new fields.

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
| `.claude/agents/financial-analyst.md` | `.claude/agents/{tactician,strategist}.md` |
| main agent with "delegate, don't answer" | `.claude/agents/chief-of-staff.md` |
| `scripts/*.py` called through Bash | `src/gather.rs` — `cn` and `gh`, run by `cos`, not by the model |
| `/budget-impact` slash command | `cos brief`, `cos review`, `cos ask` |
| PostToolUse `report-tracker.py` hook | `out/audit/YYYY-MM-DD.jsonl`, written from the event stream |
| `executive` output style | the `Answering` rules in the chief's prompt + typed reports |
| `send_query()` | `claude::execute` |

## `cos ask`: orchestration in Rust

The Agent tool runs subagents in the background, and `claude -p` emits its
result before they report: the first live run returned "both brains are still
running" for $0.85. So `cos ask` orchestrates outside the model:

1. **Route** — the chief, no tools, returns `{brains, framing}` (`--json-schema`).
2. **Brains** — `cos` runs each chosen brain as its own `claude -p --agent …`,
   in parallel threads, sharing one deadline.
3. **Merge** — the chief, no tools, merges under its Answering rules. A failed
   brain arrives as `FAILED: …`, which the chief reports as a gap.

Interactive sessions (agents installed in `~/.claude/agents`) still delegate
in-session; there background agents report back.

## Cost

Every headless run pays to cache its context. Measured on 2026-10-15 for a
one-word reply (Sonnet):

| Config | Cache-write tokens | Cost |
|---|---|---|
| default (user rules, skills, all connectors) | 125,695 | $0.50 |
| `--tools ""` (drops ToolSearch, so MCP tools load eagerly) | 209,597 | $0.84 |
| `--setting-sources project`, unused servers denied, ToolSearch kept | 42,465 | $0.17 |

`cos` uses the last for brains and `--tools "" --strict-mcp-config` for route
and merge. Which servers are "unused" is per person, so the deny list is data:
`context/deny-mcp.txt`, seeded empty from `templates/context/`. Live runs:
`cos brief` $0.32 / 9 turns; `cos ask` (both brains) $1.45 including the route
step. Every run carries `--max-budget-usd`.

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
| HTTP (`Remote`) | `latest_brief`, `latest_review` only | a leaked URL must not spend money or start runs |

The remote filter sits in both `list_tools` and `call_tool`, so a client that
calls an unlisted tool by name gets `-32602 tool … is local-only`. HTTP is
served at `/mcp/<token>` (≥ 32 chars); every other path is a 404. Verified
2026-10-16 with curl: wrong path 404, two tools listed, `brief` refused.

Saved runs are stored as JSON next to the markdown (`src/store.rs`), so the
free tools return typed `structuredContent`. Brief and review tools carry
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

A brief hands out one `agent_prompt` per action and a review one per goal;
every tracked deliverable has a handoff. `cos agents spawn --from brief --item 2`
or `--deliverable D-1` runs it instead of leaving it to be pasted
(`src/agents/`).

| Piece | Where | Does |
|---|---|---|
| spawn | `agents::spawn` | cuts a detached worktree from the repo the prompt names (`~/Projects/<repo>`), writes `spec.json`, starts the supervisor in its own session |
| supervisor | `agents::supervise` (`cos agents supervise <id>`, hidden) | owns the harness for its whole life: own process group, one deadline (default 1800 s, ceiling 7200 s), streams `events.jsonl`, rewrites `state.json`, kills the group and reaps on exit, stop or deadline |
| harness | `agents::harness` | Claude Code `-p --output-format stream-json`, `acceptEdits`, `--permission-prompts none`, Seatbelt sandbox with a strict network allowlist and no unsandboxed retry, `--max-budget-usd`; Codex `exec --json --sandbox workspace-write` |
| monitor | `cos agents ls / watch / show / tail -f`, `cos dash` → `/agents`, MCP `agents` | status, tool calls, cost or tokens, latest activity, the session id to take over with |
| stop | `cos agents kill`, MCP `stop_agent` | a `stop` file the supervisor polls every 250 ms; a supervisor that does not answer in 10 s is SIGKILLed and its harness group with it |
| reconcile | every read | a running state whose supervisor is gone becomes `lost`, and its harness group is killed, but only if the group leader's recorded start time still matches, so a recycled pid is never signalled |
| clean | `cos agents clean` | removes finished worktrees; refuses one with uncommitted changes or commits past its base unless `--force` |

The agents are the one place `cos` writes outside `out/`: in their own
worktree. Shell commands get no network in either harness, so a push, a PR
comment or a mail is refused by the sandbox, not by the prompt's request to
stop. Agents run past the 5-minute command ceiling by design; they are
background work with their own deadline and spend cap, and no CLI call waits
on one. Codex has no spend flag, so its cap is the deadline alone.

`tests/agents.rs` drives the real supervisor binary against stand-in
harnesses (`tests/fixtures/`): done, failed, killed, timed out, lost, and
clean refusing work.

## Not built

- An `--act` mode (drafting replies, moving tickets). Add per tool to
  `READ_ONLY`'s sibling list when wanted, never by widening to a whole server.
- Multi-turn sessions (`--resume`). Every run is stateless; memory is the
  workspace state and the `context/` files.
- From the kit's optional dashboard: **Mark reviewed** and any other write
  from the page. The page is read-only; acknowledgments go through
  `cos state commit` or the skill.
- A scheduler that follows the kit's scheduling reference. `task schedule`
  runs `cos brief` and `cos review` under launchd, with tracking gated on the
  profile; there is no quiet-run notification path.
- Automatic rate-limit recovery. Off in the kit, and off here.
