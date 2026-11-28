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
task schedule                              # weekdays 08:30 brief, the active business (BUSINESS=<name> for another)
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
- **Lifecycle**: each repo document is `archived` (under an `archive/`
  folder), `superseded` (front matter `status: superseded` with
  `superseded_by: <relative path>`), `needs review` (`status:
  needs-update`, `deprecated`, `obsolete`, or `superseded` with no
  replacement), `dated evidence` (under `reports/`, `audits/` or
  `investigations/`), or `guidance`. A superseded document "can archive"
  only when its replacement is tracked; age alone never archives anything.
  Relative links to no tracked file or folder count as broken. The list
  and the inspector show all three, and the reader says which links are
  broken. Nothing is moved or rewritten.
- **Topics of yours**: add one and drag documents in; search filters titles,
  paths, topics and lifecycle (`needs review`, `broken link`).
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

### Flows

Every piece of planned work you have going, across every repo under
`--projects`: one Chronis epic per flow, with the `ritual` step it is in (think,
spec, plan, build, review, ship, reflect, or drift when a design changed after
approval) and its beads drawn as a dependency graph, ready beads outlined.

```bash
cos flows                 # one line per flow
cos flows --json          # the same, for scripts
```

The desktop app's Flows tab and `cos dash` at `/flows` show the same page. It
reads `cn list` once per repo under one deadline, skips linked worktrees, and
keeps only ids, titles, statuses, edges and the `ritual-*` lines; it reads
nothing unless the profile approves a `tasks` source.

The ritual's one gate is your approval of a design, recorded on the flow's epic:

```bash
cos ritual approve <epic> docs/plans/<date>-<slug>-design.md --by "<your name>" \
  --review docs/plans/<date>-<slug>-design-review.md
cos ritual check <epic>   # exit 0: approved, and the design unchanged since
```

`approve` checks that the review names this exact design (its sha256) with
`verdict: clean`, asks you to type `yes`, then writes the digest on the epic and
runs `cn approve`. It refuses without a terminal, so a script cannot approve
for you by accident. An agent you have told to approve a design passes `--yes`
to skip the prompt. It does not authenticate you: see
[ritual.md](ritual.md#what-approval-does-and-does-not-stop).

### In Claude Desktop and Codex

```bash
task install-mcp      # cos connect claude-desktop --repair, then cos connect codex --repair
```

Each client is registered only when it is installed (below, `installed`).
Restart Claude Desktop. Both apps then have these tools:

| Tool | Cost | Notes |
|---|---|---|
| `latest_brief` | free, instant | read the last saved brief, week part included |
| `plan`, `handoff` | free, instant | the workspace's open work; a deliverable's handoff |
| `brief` | paid, minutes | starts a fresh brief and answers at once with its run |
| `ask` | paid, minutes | starts the tactician on a question (1 to 4000 chars) and answers at once with its run |
| `run_status` | free | a run's state; `wait_secs` (0 to 50) waits for it to end. Once done: the brief (`report`, `markdown`, `day`, `cost_usd`) or the `answer` |
| `runs` | free | the business's latest runs, newest first (`limit` 1 to 20, default 5) |
| `businesses` | free | the registered businesses, the default one, and the pin |

`brief`, `ask`, `run_status` and `runs` return a run view: `run` (an id like
`r-20261125-091502-k3x9`), `business`, `kind`, `state` (`queued`, `running`,
`done`, `failed`, `deferred`, `stopped`, `lost`), `reason`, and
`link`, `cos://run/<business>/<run>`, which opens the run in the desktop app
(present for a registered business only; a server on unregistered
directories labels its runs `default` or `explicit` and gives no link).
`brief` while a brief for that business
is running returns that run with `existing: true` and spends nothing. A
client follows a run by calling `run_status` with `wait_secs`; 50 s fits
inside Codex's 60 s tool timeout. The runs are files under
`<home>/out/run-records/`, so `run_status` and `runs` also show runs started
from a terminal, the schedule or the app.

**Business per call.** Every tool takes an optional `business`. Without it,
a `cos mcp` started with `--business`, `COS_BUSINESS`, `COS_HOME` or the
directory flags acts on that business and refuses any other name; one started
without them uses the registry's active business, read on each call, so
`cos business use <name>` takes effect without restarting the client.

In Claude Desktop the brief tools render a **dashboard** (an
[MCP App](https://modelcontextprotocol.io/docs/extensions/apps)): Today / Week
tabs over the same brief, reload, and a two-click "run new" that follows the
run with `run_status` until it ends.

#### `cos daemon`

`task install` loads `cos daemon` under launchd as `cos.daemon`. It owns the
paid runs MCP clients start, for every registered business, and follows
every business's hires, so a run outlives the chat that started it.

```bash
cos daemon install [--force] [--bin <abs>]   # write ~/Library/LaunchAgents/cos.daemon.plist and load it
cos daemon uninstall                         # boot it out and remove the plist
cos daemon                                   # run it in the foreground
```

- It listens on `cos/daemon.sock` under your user temp dir; only you can open
  it. A second daemon exits 0 with `already running`.
- One brief per business at a time, whoever starts it: a brief holds
  `out/run-records/brief.lock`, and a terminal `cos brief` while one runs
  exits 73 naming the running one. At most 2 asks per business and 4 runs in
  total run on the daemon; each has 420 s.
- When the daemon is down, `cos mcp` runs `brief` and `ask` itself, under the
  same caps, and ends them when the client closes.
- Its log is `daemon.log` beside the socket.
- `install` first drains the daemon (`drain`: every later start is refused),
  so reloading it kills no run. It refuses (exit 75) while the daemon owns a
  live run; `--force` stops those runs, then drains. A daemon you started
  from a shell is not stopped by the reload: install fails naming its pid,
  and the daemon takes runs again. `task install` reports
  the refusal and goes on; the running daemon moves to the new binary once
  its runs end.
- On restart it marks the runs a previous daemon left behind `lost`, after
  ending their processes.

#### `cos connect`

```bash
cos connect status [--json]                           # one row per client
cos connect claude-desktop [--repair] [--bin <abs>]   # register; --repair rewrites the entry
cos connect codex [--repair] [--bin <abs>]            # register, with tool_timeout_sec = 320
cos connect remove claude-desktop|codex               # drop only the cos entry
```

`--bin` defaults to the running `cos` and must be absolute, executable, and
print `cos <version>` for `--version`. A write touches only the `cos` entry:
other servers and keys keep their values (Claude Desktop's JSON keeps key
order; Codex's TOML keeps its comments and other tables as text). The first
write keeps the file as it was in `<config>.cos.orig`, with its mode, and later
writes never overwrite that copy. A file that does not parse is left untouched
and named. A file that changes between the read and the write is left as the
other writer left it (`changed while writing; try again`). A symlinked config
stays a link: its target is written, with `.cos.orig` beside the target.

`status --json` prints `{"clients": [row, row]}`, one row per client:

| Key | Meaning |
|---|---|
| `client` | `claude-desktop` or `codex` |
| `config` | the config file: `~/Library/Application Support/Claude/claude_desktop_config.json`, or `$CODEX_HOME/config.toml` (default `~/.codex/config.toml`) |
| `installed` | Claude Desktop: the config's directory exists. Codex: `$CODEX_HOME` exists or `codex` is on `PATH` |
| `registered` | the `cos` entry is present (`.mcpServers.cos`, `[mcp_servers.cos]`) |
| `command` | the entry's `command`, or `null` |
| `command_ok` | `command` is absolute and executable |
| `version` | the first line `command --version` printed, or `null` |
| `version_ok` | `version` is `cos <this cos's version>` |
| `version_stale` | the version check timed out (5 s) and `version` is the last result for that path in this process (the desktop app; a CLI call has none, so it reads `false`) |
| `timeout_sec` | Codex's `tool_timeout_sec`, or `null` |
| `daemon` | `unknown` here; the desktop app's Settings → Assistants card probes the daemon itself |
| `error` | a parse error or failed version check, or `null` |
| `problems` | the failing fields: `config`, `registered`, `command`, `version`, `timeout` (Codex, below 320) |
| `ok` | `problems` is empty |

The version check runs the configured binary once per path, size and mtime
in a process, killed and reaped at 5 s. The cache lives in memory: the
desktop app's Settings card reuses it, while each `cos connect status` is a
new process and checks again. A config error names the file, line and column,
never the file's text: these files hold other servers' secrets.

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
cos agents watch                              # follow them in Herdr: deadlines, one verdict each
cos agents watch --osc                        # the same, as OSC 7501 records for the Herdr pane it runs in
cos agents show <id>                          # verdict, checks, diff, its Herdr terminal
cos agents say <id> "…"                       # a message into its Herdr terminal (hc send)
cos agents pause <id>                         # its stop key (hc interrupt): the turn ends, it stays
cos agents done <id>                          # end its terminal and judge it, once
cos agents open <id>                          # show its terminal in the Herdr window
cos agents screen <id>                        # its screen and recent history (hc read)
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
- **Where it runs**: a [Herdr](https://herdr.dev) terminal (`hc run`)
  named `cos-<id>` and labelled `hire=<id>`, in that worktree, with Claude
  Code or Codex interactive and the work as its first message. Herdr owns
  its process for its whole life; cos owns none. Talk to it, approve its
  prompts and take it over in Herdr. Without herdr-core running, `spawn`,
  the window and `spin_off` refuse with the fix: `hc server start`. Its
  command line pins the business that hired it (`COS_HOME`,
  `COS_WORKSPACE`, `COS_PROJECTS`, with `COS_BUSINESS` cleared), so a `cos`
  it runs resolves the same business.
- **What it may do**: edit and run commands inside that worktree. Shell
  commands have no network (Claude Code's sandbox with a strict allowlist;
  Codex's `workspace-write`, asking on request); anything more asks you in
  Herdr, which shows the run as waiting on you. It cannot push, comment or
  send without you.
- **Limits**: `--deadline` seconds (default 1800, ceiling 7200): at it, cos
  presses its stop key, then ends its terminal 10 s later. Live hires past
  `COS_AGENT_SLOTS` are refused. Herdr measures no spend, so there is no
  spend cap and no cost recorded.
- **Followed, judged once**: the desktop app (every 2 s), `cos agents
  watch` and `cos mcp` while it serves read each live terminal (`hc get`),
  stop it at its deadline, and judge it exactly once (`judge.lock`): when
  Herdr says it is done after its first message, its program ends, the
  deadline passes, or you mark it done. Later turns are judged on request
  (`cos agents judge`). cos only writes to a terminal labelled with its own
  hire.
- **The hiring policy**: `context/agents.json` (seeded by `task setup`)
  fills what a spawn leaves out, per repo directory name over `defaults`:
  `checks`, `scope` and `deadline_secs`. An old file's `budget_usd`,
  `stall_secs` and `repeat` are read and ignored. Checks or scope given on
  the spawn win whole; the policy never adds to them. An unknown field
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
  wins over the role's. The desktop's Agents tab hires by role.
- **The scorecard**: every verdict is a line in `out/agents/ledger.jsonl`,
  which `clean` leaves alone. `cos agents scorecard`, `/agents` and the MCP
  `agents` tool group it by role, harness, model and repo: runs, accepted, needs
  you, rejected, overclaimed and median time; runs in Herdr show "not
  measured in Herdr" for cost, never 0.
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
- **Take over**: it is already yours in Herdr: `hc attach cos-<id>`, or
  Open in Herdr in the window. A run from before cos followed hires in
  Herdr reads, but cannot be resumed.
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
- **Businesses**: each business is a home, a workspace and a projects
  directory, named in `~/.config/cos/businesses.toml` (`COS_BUSINESSES` moves
  it). With no registry, `cos business list` creates a `default` business.

  ```bash
  mkdir -p ~/biz/acme/repos
  cos --workspace ~/biz/acme state init        # the new workspace's profile and state
  cos business add acme ~/biz/acme ~/biz/acme ~/biz/acme/repos
  cos business use acme                        # the active business for cos, the app and the MCP server
  cos --business acme report --no-open         # one run for a named business, without changing the active one
  task schedule BUSINESS=acme                  # launchd jobs bound to acme
  ```

  `COS_HOME` and the other directory variables still set a whole triple by
  hand and win over the registry; `cos business list` prints the `current:`
  line that says which layer answered.

## Troubleshooting

- *"needs to be connected in claude.ai"* in a brief's gaps: reconnect that
  connector with `/mcp` in Claude Code. The brief lists it as not read rather
  than pretending the inbox is empty.
- Every tool call and failure is logged to `out/audit/YYYY-MM-DD.jsonl`.
- Scheduled runs log to `out/logs/`.

Design and measurements: [docs/design.md](design.md).
