# Using `cos`

The full guide; the [README](../README.md) is the overview.

One run a day, `cos brief`, by the **tactician**, working down from the goals:

| Part | Horizon | You get |
|---|---|---|
| **Today** | today | at most 3 actions that need you (decide / do / delegate / batch) and 2 verified changes; then calendar prep, a backlog with owners, and what each source actually covered |
| **This week** (`week`) | this week against your goals | each goal rated rock / sand / water with its KPI and a steer (maintain, sequence, reduce scope, test, delegate, stop); this week's deliverables, proposed; decisions only you can take; what to stop doing |

`cos ask "…"` puts a question to the same tactician, on the same sources.

The operating model, the private workspace and the safety rules come from the
**Chief of Staff starter kit** in this repo ([START-HERE.md](../START-HERE.md),
`plugins/chief-of-staff-starter/`): goals → KPIs → weekly deliverables →
priorities → delegation. `cos` is the engine that runs it on a schedule.

It reads Google Calendar, Gmail, Jira/Confluence and Notion through your
claude.ai connectors, plus GitHub PRs and chronis tasks on your machine. It is
**read-only**: it recommends, you send.

Measured cost per run, before the brief carried the week part: a daily brief
about $0.30. The brief with its week part has not been measured yet. Every
run carries a spending cap (brief $3.00, ask $2.00 by default) and a 300 s
deadline.

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
cos state propose                  # the last brief's proposed deliverables → .chief-of-staff/candidate.json
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
- **Tracking**: with `localTracking.approved: true`, every `cos brief`
  commits a run receipt, each profile source's health and the
  window it read, and an incident for a new access failure. A source the
  model reports as read in full is still recorded `partial`, and its
  watermark never moves: completeness is yours to confirm.
- **Dates**: "today" is taken in the profile's timezone.

## Use it

```bash
cos brief                                  # goals → this week → today
cos ask "release review or the bug fix today?"
cos --dry-run brief                        # print the prompt, spend nothing
task schedule                              # weekdays 08:30 brief
```

`task schedule` and `task unschedule` also unload the Friday `cos review`
job an older schedule loaded; the subcommand is gone and its saved
`out/reviews/` are no longer read.

### The report

Every `cos brief` writes `out/report.html` and opens it:

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
- **Steering** (the brief's week part): goals as rock / sand / water with a
  steer, the proposed deliverables, where merged work went, decisions, what
  to stop.
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
Host headers and serves nothing outside `out/reports/<date>.html` and the
documentation room under `/docs`.

### Jira assigned to you

List the Jira projects in the profile (`"jiraProjects": ["WEB", "VM"]`) and
each brief reads every open issue assigned to you there in one search, if Jira
is an approved source. The report shows them in a "Jira assigned to you" card,
grouped by project, each linked, with the bet it serves when a goal's `jira`
list names the key. A project key that is not plain letters and digits is
dropped before it reaches the query.

### The documentation room

```bash
cos docs                 # opens http://127.0.0.1:8790/docs (needs cos dash running)
cos docs notion          # list the Notion pages about your projects (paid, ~$0.3–1.5)
cos docs json            # the inventory as JSON
```

The desktop app has it as the **Docs** tab. Every tracked markdown file in the
profile's `monitoredRepos` (the most recently used local checkout of each
under `~/Projects`) is a document; tooling folders (`.github`, `.claude`,
`vendor`, `node_modules`) are left out. Links between documents come from
their relative markdown links. Notion pages come from `cos docs notion`. A repo
document is "repo + Notion" when a page shares at least 70% of its title's
meaningful words, three or more; short generic titles never match.

- **Bets**: a goal claims documents with a `docs` list in the workspace state,
  `"repo"` or `"repo:path/prefix"` (the longest prefix wins), and its title up
  to the colon names the group (`VM SDK: Workforce ready` → `VM SDK`). Within
  a bet, documents fall into topics by kind from their path: ADRs, Plans,
  Product & specs, Guides, Operations, Reference, Concepts, Code docs,
  Overview, or the `docs/<folder>` name.
- **Graph**, a force-directed view in the style of the Neo4j Browser, laid
  out by the page itself (no library). Nodes are bets, topics, documents
  (coloured by repo only / repo + Notion / move / keep) and Notion pages;
  relationships are `IN_BET`, `IN_TOPIC`, `LINKS_TO`, `IN_NOTION` and your
  own link kinds (`depends on` → `DEPENDS_ON`). Each level starts from its
  own subgraph: all bets and their topics; a bet's topics and documents; a
  topic's documents and their one-hop neighbours. A view starts at 300
  nodes and exploring stops at 1000. Drag pins a node (double-click the
  canvas to release); double-click expands; right-click (long-press) for
  expand by type, collapse, hide, pin, neighbourhood, shortest path, read
  and link. The chips above the graph count labels and types: a label chip
  opens its style (colour, size, caption, kept in this browser), a type chip
  hides it. Filters, Back/Forward/Reset, Table and Text views, SVG/PNG
  export and Copy nodes sit beside it. Keys: arrows, Enter, Delete, Esc,
  `+` `−`, `0` (100%), `F` (fit; Shift+F fits everything), `S` (zoom to the
  selection), `R` (read). The document list below is the same content
  without the canvas.
- **Mouse and trackpad**: zoom stays anchored at the pointer, eased for a
  notched wheel and direct for a trackpad, between 5% and 400% (the readout
  turns amber at a limit; click it for 100%). The **Mouse** menu chooses
  "wheel zooms" (default) or "wheel pans, Ctrl/⌘ + wheel zooms"; a pinch
  always zooms, and so do two touch points. Pan by dragging the background,
  Space + drag, or the middle button; "Glide after a pan" adds inertia (never
  under reduced motion). Fit stops at 75% so captions stay readable (turn it
  off in the same menu); a caption under 5 px on screen is hidden and shows
  in full on hover. Shift + drag draws a box and Shift + click adds a node:
  the inspector then expands, hides, pins, unpins or zooms to all of them.
  Settings are kept in this browser.
- **Suggestions**: Plans, Product & specs, Concepts and Guides suggest "move to
  Notion"; ADRs, Overview, Code docs, Operations and Reference suggest "keep in
  repo". Nothing is marked until you accept: one at a time, or "Accept N
  suggestions" for the bet or topic in view.
- **Topics of yours**: add one and drag documents in; search filters titles,
  paths and topics.
- **Link**: your own edges (`depends on`, `supersedes`, …) beside the found ones.
- **Read**: a repo document opens rendered (GitHub-flavoured markdown through
  the vendored marked, then DOMPurify; tables, task lists, fenced code,
  autolinks, heading anchors, front matter folded away). The reader takes the
  main area beside a narrow graph, with a handle to resize the split (drag,
  arrow keys, double-click resets) and "Hide graph", or goes **Full screen**
  (Esc comes back). It shows the bet › topic › document path, contents from
  the headings, previous/next in the topic (`[` `]`), Show in graph and Open
  in Notion. A link to another known document opens it and selects it in the
  graph; a link to anything else is greyed out with the reason. Images inside
  the checkout show (PNG, JPEG, GIF, WebP, AVIF); SVG and remote images show a
  placeholder; Mermaid fences show as source. The last mode is remembered,
  and `#doc=<id>` deep-links a document. A Notion page opens in Notion.

Your topics, links and marks are saved to `context/docs.json`. In the desktop
app the room asks the window to save it (`docs_save`), which honours only
messages from the Docs frame. Through `cos dash` a save must carry an
`X-Cos: docs` header, so another website cannot write the file. A broken file
is reported, never overwritten.

### In Claude Desktop and Codex

```bash
task install-mcp      # registers `cos mcp` with both (backs up the Desktop config)
```

Restart Claude Desktop. Both apps then have these tools:

| Tool | Cost | Notes |
|---|---|---|
| `latest_brief` | free, instant | read the last saved brief, week part included |
| `plan`, `handoff` | free, instant | the workspace's open work; a deliverable's handoff |
| `brief` | paid, minutes | fresh run |
| `ask` | paid, minutes | the tactician answers a question |

In Claude Desktop the brief tools render a **dashboard** (an
[MCP App](https://modelcontextprotocol.io/docs/extensions/apps)): Today / Week
tabs over the same brief, reload, and a two-click "run new". For Codex, the paid tools can outlast
the default 60 s tool timeout; add `tool_timeout_sec = 320` under
`[mcp_servers.cos]` in `~/.codex/config.toml`.

In Claude Code, `task setup` also installs the `tactician` agent into
`~/.claude/agents/`, so any session can delegate to it.

### Spin off agents

Every action in a brief and every goal in its week carries an agent prompt,
and every tracked deliverable has a handoff. `cos agents` runs them:

```bash
cos agents spawn --from brief                 # list the items that have a prompt
cos agents spawn --from brief --item 2        # Claude Code on action #2
cos agents spawn --from goals --item 1 --harness codex   # goal #1 of the week
cos agents spawn --from brief --all           # one agent per action
cos agents spawn --deliverable D-1            # the handoff for D-1, built from the state
cos agents spawn --prompt "…" --repo ~/Projects/my-repo
cos agents spawn --deliverable D-1 --check "task test" --scope src/   # hired against a contract
cos agents watch                              # the table, redrawn until all finish
cos agents tail -f <id>                       # each call with its command or path, its result, each message
cos agents show <id>                          # verdict, checks, diff, cost, how to take it over
cos agents judge <id>                         # judge again, e.g. after finishing its work by hand
cos agents spawn --deliverable D-1 --role reviewer   # hired as a reviewer: judged on REVIEW.md
cos --dry-run agents spawn --deliverable D-1 --role tester   # the plan: repo, fit, contract; starts nothing
cos agents record <id> --outcome passed --notes "…" --evidence <link>   # what you saw running its draft
cos agents scorecard                          # how agents did, by role, harness, model and repo
cos agents kill <id>
cos agents clean                              # remove finished worktrees (keeps any holding work)
```

- **Where it works**: a detached worktree under `out/agents/<id>/tree`, cut
  from the HEAD of the repo the work belongs in, or `--repo`. `--in-place`
  uses the checkout itself. The repo is found in this order: the
  deliverable's own `repo`/`repos`; its goal's `repos`, `whereItLives`
  (`owner/name` slugs) and `docs`; the ticket's project in the profile's
  `jiraProjects` (`{ "key": "VM", "repos": ["owner/name"] }`); then a path
  or a `monitoredRepos` name the work mentions. Each must be a checkout
  under `--projects` (`COS_PROJECTS`). When none is found the hire is
  refused and the window asks for the repo; only a role with `repo: false`
  that is judged on a file (the researcher) runs in a scratch directory.
- **Fit before hiring**: `cos` checks the role against the work before it
  starts anything, from the `fit` vocabulary in the same file: the kinds of
  work a title names (`test`, `docs`, `code`, …) against the role's `work`,
  and the needs the work names (`network`, `live`) against what the role
  `drafts`. A docs keeper on a test deliverable is refused with "hire the
  Tester instead"; work that needs a live system and no role can draft is
  "this needs you"; an operation on `cos` itself (refresh the dashboard,
  rebuild the report) is refused with the command to run, `cos report`.
  The desktop's hire dialog shows the verdict as you pick; `cos agents
  spawn` and the MCP `spin_off` tool refuse the same way.
- **Finish lines outside the sandbox**: when a deliverable's `doneWhen` is
  outward-facing ("resolved in Jira", "merged", "deployed", "sent"), the
  agent is held to the draft it can return ("A draft ready for you to
  resolve VM-311: `TEST-PLAN.md`, …") and the original stays as your finish
  line (`yours` in `show` and the window). A contract with no check, no
  scope and no output file is warned about before hiring: the judge would
  have nothing to run.
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
- **Roles**: the staff under the chief of staff, in the same file's `roles`
  (the template's eight when the file has none): builder, reviewer,
  researcher, investigator, docs keeper, release clerk (drafts only) and
  tester, under the chief of staff, which hires and is never hired. The
  tester drafts and you run: in a worktree with no network it writes
  `TEST-PLAN.md` (the plan, a runnable script naming the environment and
  the package version, an evidence checklist) and is judged on that file.
  Run the script yourself, then record what happened on the run (the run's
  "Record result" in the window, or `cos agents record`); it is stored in
  the run's `state.json` and sent nowhere. `--role reviewer` adds
  the role's mandate to the prompt and fills its contract: `expects` (`diff`,
  `file` with an `output` such as `REVIEW.md`, or `none`), checks, scope and
  `done_when`. The role's contract wins over the repo's; the repo's deadline
  and budget win over the role's. The desktop's Agents tab hires by role.
- **Trouble while it runs**: `stalled` (no output for `stall_secs`) and
  `looping` (the same call, arguments and all, `repeat` times in a row) show
  in the verdict column of `ls` and `/agents`, and in `show`. They flag; they
  never stop an agent. Spend is not watched live: Claude reports its cost
  only when it ends, and `--budget` already caps it.
- **The scorecard**: every verdict is a line in `out/agents/ledger.jsonl`,
  which `clean` leaves alone. `cos agents scorecard`, `/agents` and the MCP
  `agents` tool group it by role, harness, model and repo: runs, accepted, needs
  you, rejected, overclaimed, median cost and time.
- **The verdict**: when an agent ends, `cos` judges it from what it left,
  never from its summary. It lists the files that differ from the commit the
  agent started from, re-runs each `--check` in the workdir itself (one 300 s
  budget for all of them), and compares the diff with `--scope`. `accepted`:
  every check passed and the diff is in scope. `needs_you`: nothing failed,
  but there were no checks, no changes (for `expects: diff`), or changes
  outside scope. `rejected`: the agent did not finish, a check failed, its
  `expects: file` output is missing or empty, or it changed files under
  `expects: none`; `rejected!` means it claimed success anyway.
  `--deliverable` brings its `doneWhen`.
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

- The remote server exposes **only** `latest_brief`: a
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
