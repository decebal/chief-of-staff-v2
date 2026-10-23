# Using `cos`

The full guide; the [README](../README.md) is the overview.

| Brain | Horizon | You get |
|---|---|---|
| **tactician** | today, this week | at most 3 actions that need you (decide / do / delegate / batch) and 2 verified changes; then calendar prep, a backlog with owners, and what each source actually covered |
| **strategist** | this week against your goals | each goal rated rock / sand / water with its KPI and a steer (maintain, sequence, reduce scope, test, delegate, stop); this week's deliverables, proposed; decisions only you can take |
| **chief-of-staff** | routes | ask a question; it picks one or both brains and merges their answers |

The operating model, the private workspace and the safety rules come from the
**Chief of Staff starter kit** in this repo ([START-HERE.md](../START-HERE.md),
`plugins/chief-of-staff-starter/`): goals → KPIs → weekly deliverables →
priorities → delegation. `cos` is the engine that runs it on a schedule.

It reads Google Calendar, Gmail, Jira/Confluence and Notion through your
claude.ai connectors, plus GitHub PRs and chronis tasks on your machine. It is
**read-only**: it recommends, you send.

Measured cost per run: a daily brief about $0.30, a two-brain question about
$1.50. Every run carries a spending cap and a 300 s deadline.

## Requirements

- macOS (the scheduler uses launchd; everything else runs anywhere Rust does)
- [Claude Code](https://claude.com/claude-code) CLI, logged in (`claude` on PATH)
- Connectors enabled on your claude.ai account: Google Calendar, Gmail,
  Atlassian, Notion — whichever you use. Check with `/mcp` in Claude Code.
- `gh` logged in; optional `cn` (chronis) for tasks
- Rust 1.99.1 (pinned in `rust-toolchain.toml`; `rustup toolchain install` in the repo fetches it), [Task](https://taskfile.dev), `jq`

## Quickstart

```bash
git clone <this repo> ~/Projects/chief-of-staff
cd ~/Projects/chief-of-staff
task setup            # seeds context/ and .chief-of-staff/, builds, installs cos + agents
```

Then set up the workspace with the starter skill, in Claude Code or Codex,
from this folder:

```text
Read plugins/chief-of-staff-starter/skills/chief-of-staff-starter/SKILL.md completely and use Setup mode for this project.
```

It asks one question at a time: your role and the 90-day outcome, one
monthly goal with a measurable finish line, a KPI (unknown stays unknown),
this week's deliverables with owners and dates and what counts as done, your
timezone, and at most three sources to approve. Then:

```bash
cos brief             # first brief → out/briefs/YYYY-MM-DD.{md,json}
```

`.chief-of-staff/`, `context/` and `out/` are gitignored: your profile,
goals, state and briefs stay on your machine. `context/principles.md` and
`context/decisions/` still reach the brains; `context/goals.md` is used only
when there is no workspace.

## The workspace

`.chief-of-staff/profile.json` holds your preferences and approvals;
`.chief-of-staff/state.json` holds goals, KPIs, deliverables, commitments,
sources, runs and incidents, with a journal. `cos` and the kit's Python helper
read and write the same files under the same rules, and take the same lock.

```bash
cos state init                     # create it (never overwrites)
cos state read                     # print the validated state
cos state check --file cand.json   # validate a candidate
cos state commit --candidate cand.json --expected-revision 3 --reason "Mark D-1 reviewed"
cos state propose                  # the last review's proposals → .chief-of-staff/candidate.json
cos handoff D-1                    # "Delegate this": the handoff text; starts nothing
```

A commit refuses a stale revision, a deleted record, reopened or rewritten
completed work, a moved journal, a watermark moving backward or past a gap,
and a held lock (never stolen).

What changes once a profile exists:

- **Connectors**: the brains get only the tools of the sources marked
  `readsApproved: true` (mail, calendar, Jira/Confluence, Notion). No approved
  source means no connector at all. Without a profile, every `READ_ONLY` tool
  is available, as before.
- **PRs and tasks**: `cos` runs `gh` only for an approved source with
  `service: "github"` (or `"code"`) and `cn` only for one with
  `service: "chronis"` (or `"tasks"`); otherwise the brief says they were not
  read. Without a profile both run, as before.
- **Coverage**: every approved source gets a row. One the brief left out is
  added by `cos`: `partial` when the run called its tools, "Not attempted"
  when it never did.
- **Calendar window**: the rest of this Monday-to-Sunday week, plus tomorrow
  on a Sunday. `cos` gives the model exact start and end times and drops
  entries dated outside them.
- **Tracking**: with `localTracking.approved: true`, every `cos brief` and
  `cos review` commits a run receipt, each profile source's health and the
  window it read, and an incident for a new access failure. A source the
  model reports as read in full is still recorded `partial`, and its
  watermark never moves: completeness is yours to confirm.
- **Dates**: "today" is taken in the profile's timezone.

## Use it

```bash
cos brief                                  # tactician
cos review                                 # strategist (weekly)
cos ask "release review or the bug fix today?"
cos --dry-run brief                        # print the prompt, spend nothing
task schedule                              # weekdays 08:30 brief, Fridays 16:00 review
```

### The report

Every `cos brief` and `cos review` writes `out/report.html` and opens it:

- **What changed** and **Coming up** across the top: verified deltas (done,
  unblocked, risk, time changed), dated milestones with countdowns, and a
  merged-PRs-per-day chart.
- **Today** (tactician): the actions that need you, each with a **copyable
  handoff** (paste into Claude Code or Codex; each stops before anything
  outward-facing), a day timeline, the backlog, review-queue age, ready tasks
  by repo and priority, and per-source coverage.
- **Plan** (the workspace): goals with finish lines and KPIs (null shows as
  unknown, not zero), this week's deliverables with owner and date and their
  basis, a **Delegate this** handoff on each, commitments, and sources with
  connection health kept apart from review coverage.
- **Steering** (strategist): goals as rock / sand / water with a steer, the
  proposed deliverables, where merged work went, decisions, what to stop.
- **History** builds up every run: a year of merged PRs as a calendar
  heatmap, trend lines for review queue, ready tasks, actions per brief and
  meeting time, a diagram of how it works, and every archived daily report.

The page reads at 320 px without sideways scrolling and remembers the last
tab.
- **Diagrams** — the roadmap timeline, the goals flywheel and the
  architecture — are [Markdy](https://markdy.com) 1.5.7, vendored as one
  inline bundle (`vendor/markdy/`). Their source is generated from data, not
  written by the model.

Chart numbers come from the facts `cos` collects itself (`out/facts/`,
`out/history/`), not from the model. `cos report` rebuilds and opens it for
free; `--no-open` skips opening.

The archive seeds itself with a year of merged PRs, two months per run, so
GitHub's search rate limit is never hit; months not yet archived show as
outlined cells, not as zero.

### Live

```bash
cos dash            # or: task dash — http://127.0.0.1:8790
```

Serves the same page on localhost and reloads an open tab whenever a run
lands (scheduled ones included). It answers only to `127.0.0.1` / `localhost`
Host headers and serves nothing outside `out/reports/<date>.html`.

### In Claude Desktop and Codex

```bash
task install-mcp      # registers `cos mcp` with both (backs up the Desktop config)
```

Restart Claude Desktop. Both apps then have these tools:

| Tool | Cost | Notes |
|---|---|---|
| `latest_brief`, `latest_review` | free, instant | read the last saved run |
| `plan`, `handoff` | free, instant | the workspace's open work; a deliverable's handoff |
| `brief`, `review` | paid, minutes | fresh run |
| `ask` | paid, minutes | routes and merges |

In Claude Desktop the brief and review tools render a **dashboard** (an
[MCP App](https://modelcontextprotocol.io/docs/extensions/apps)): Today / Week
tabs, reload, and a two-click "run new". For Codex, the paid tools can outlast
the default 60 s tool timeout; add `tool_timeout_sec = 320` under
`[mcp_servers.cos]` in `~/.codex/config.toml`.

In Claude Code, `task setup` also installs the three agents into
`~/.claude/agents/`, so any session can delegate to `tactician` or `strategist`.

### Spin off agents

Every action in a brief and every goal in a review carries an agent prompt,
and every tracked deliverable has a handoff. `cos agents` runs them:

```bash
cos agents spawn --from brief                 # list the items that have a prompt
cos agents spawn --from brief --item 2        # Claude Code on action #2
cos agents spawn --from review --item 1 --harness codex
cos agents spawn --from brief --all           # one agent per action
cos agents spawn --deliverable D-1            # the handoff for D-1, built from the state
cos agents spawn --prompt "…" --repo ~/Projects/my-repo
cos agents spawn --deliverable D-1 --check "task test" --scope src/   # hired against a contract
cos agents watch                              # the table, redrawn until all finish
cos agents tail -f <id>                       # one line per tool call or message
cos agents show <id>                          # verdict, checks, diff, cost, how to take it over
cos agents judge <id>                         # judge again, e.g. after finishing its work by hand
cos agents scorecard                          # how agents did, by harness, model and repo
cos agents kill <id>
cos agents clean                              # remove finished worktrees (keeps any holding work)
```

- **Where it works**: a detached worktree under `out/agents/<id>/tree`, cut
  from the HEAD of the repo the prompt names (`~/Projects/<repo>`), or
  `--repo`. `--in-place` uses the checkout itself. Without a repo it gets a
  scratch directory.
- **What it may do**: edit and run commands inside that worktree. Shell
  commands have no network (Claude Code's sandbox with a strict allowlist;
  Codex's `workspace-write`), so it cannot push, comment or send; it stops
  and leaves the draft for you, as every agent prompt asks.
- **Limits**: `--deadline` seconds (default 1800, ceiling 7200) and
  `--budget` dollars for Claude (default $5). Codex has no spend flag.
- **The hiring policy**: `context/agents.json` (seeded by `task setup`)
  fills what a spawn leaves out, per repo directory name over `defaults`:
  `checks`, `scope`, `deadline_secs`, `budget_usd`, and the watch limits
  `stall_secs` (default 600) and `repeat` (default 5). Checks or scope given
  on the spawn win whole; the policy never adds to them. An unknown field
  makes `cos` ignore the file and say so, rather than hire on a typo.
- **Trouble while it runs**: `stalled` (no output for `stall_secs`) and
  `looping` (the same call, arguments and all, `repeat` times in a row) show
  in the verdict column of `ls` and `/agents`, and in `show`. They flag; they
  never stop an agent. Spend is not watched live: Claude reports its cost
  only when it ends, and `--budget` already caps it.
- **The scorecard**: every verdict is a line in `out/agents/ledger.jsonl`,
  which `clean` leaves alone. `cos agents scorecard`, `/agents` and the MCP
  `agents` tool group it by harness, model and repo: runs, accepted, needs
  you, rejected, overclaimed, median cost and time.
- **The verdict**: when an agent ends, `cos` judges it from what it left,
  never from its summary. It lists the files that differ from the commit the
  agent started from, re-runs each `--check` in the workdir itself (one 300 s
  budget for all of them), and compares the diff with `--scope`. `accepted`:
  every check passed and the diff is in scope. `needs_you`: nothing failed,
  but there were no checks, no changes, or changes outside scope.
  `rejected`: the agent did not finish, or a check failed; `rejected!` means
  it claimed success anyway. `--deliverable` brings its `done_when`.
- **Take over**: `cos agents show <id>` prints `claude --resume <session>` or
  `codex resume <session>` for the worktree.
- **From Claude Desktop or Codex**: the `spin_off`, `agents` and `stop_agent`
  tools (local server only; the remote one never offers them).
- **In the browser**: `cos dash`, then `/agents`.

`cos agents clean` refuses a worktree with uncommitted changes or new commits;
copy or commit the work first, or pass `--force`.

### Remote, for claude.ai (Claude Dashboards)

```bash
brew install cloudflared
cos tunnel            # or: task tunnel — prints the connector URL; Ctrl-C stops it
```

`cos tunnel` starts the read-only server and a Cloudflare quick tunnel, and
prints `https://<name>.trycloudflare.com/mcp/<token>`. Add that in claude.ai →
Settings → Connectors → Add custom connector.

- The remote server exposes **only** `latest_brief` and `latest_review`: a
  leaked URL can read your last brief, never trigger a paid run.
- The URL path is the credential (`out/.mcp-token`, mode 600). Treat the URL
  like a password; `task rotate-token` invalidates it.
- Quick-tunnel hostnames change on every restart, so the connector has to be
  re-added after one. A stable hostname needs a named Cloudflare tunnel on
  your own domain.
- `cos` owns the `cloudflared` process: Ctrl-C, SIGTERM or SIGHUP stop both;
  after a SIGKILL the next `cos tunnel` stops the leftover (`out/tunnel.pid`).

Briefs quote your mail and calendar; decide deliberately before exposing them.
`cos serve --public-host <host>` runs the server alone, behind a proxy of your
choosing.

## Customise

- **What the brains may touch**: `READ_ONLY` in `src/lib.rs`, narrowed to
  your approved sources by `context::tools` when a profile exists. Anything
  not listed is denied. Add individual tools, never a whole server, and never
  a file tool: the workspace and `context/` reach the brains inlined in the
  prompt.
- **Sharing the kit**: `task kit OUT=/abs/path/chief-of-staff-starter.zip`
  packages only the files in `release-files.json`, with per-file SHA256s.
  Never share your working folder.
- **Cheaper runs**: list connectors the brains never use in
  `context/deny-mcp.txt`; their tool definitions then stay out of every run's
  cached context.
- **Prompts**: `.claude/agents/*.md`. Re-run `task install` after editing.
- **Models and budgets**: `src/pipeline.rs` (`brain(...)` calls, defaults
  `--budget`).
- **Where your repos live**: `COS_PROJECTS` (default `~/Projects`).

## Troubleshooting

- *"needs to be connected in claude.ai"* in a brief's gaps: reconnect that
  connector with `/mcp` in Claude Code. The brief lists it as not read rather
  than pretending the inbox is empty.
- Every tool call and failure is logged to `out/audit/YYYY-MM-DD.jsonl`.
- Scheduled runs log to `out/logs/`.

Design and measurements: [docs/design.md](docs/design.md).
