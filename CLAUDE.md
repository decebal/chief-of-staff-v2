# chief-of-staff

A personal chief of staff: a Rust binary, `cos`, that drives headless Claude
Code (`claude -p --output-format stream-json`) for one daily brief, and
serves it over MCP to Claude Desktop, Codex and claude.ai. User guide:
`README.md`.

| Agent | Horizon | Invoked by |
|---|---|---|
| `tactician` | today, set against this week and the goals | `cos brief`, `cos ask "…"` |

The brief works down from the goals: `week` (each goal's steer, the
deliverables the week should hold, decisions, stop doing), then today's
actions and changes.

Design and measurements: `docs/design.md`. Slack copy for sharing the idea:
`docs/slack-chief-of-staff.html`.

## The starter kit is the authority

`START-HERE.md`, `plugins/chief-of-staff-starter/` (skill, references, the
`state.py` helper and its tests), `guides/` and `AGENTS.md` are the Chief of
Staff starter kit. Its rules win over anything `cos` did before: goals → KPIs
→ weekly deliverables → priorities → delegation, read-only sources, drafts
only, honest coverage, handoffs that grant nothing. When changing the kit,
keep fixtures fictional, update `release-files.json`, and keep both plugin
manifests at its version (`tests/kit.rs`).

## What the brains read

- `<workspace>/.chief-of-staff/` (the active business's workspace; `--business`
  / `COS_BUSINESS` pick another, `--workspace` / `COS_WORKSPACE` override it,
  default the home) — `profile.json` (approved sources, timezone,
  tracking approval) and `state.json` (goals, KPIs, deliverables,
  commitments, sources, runs, incidents, journal). Written only through
  `state::commit`, `state::set_source_not_needed` (profile.json, same lock)
  or the kit's `state.py`. Gitignored.
- `context/`, gitignored and seeded from `templates/context/`:
  `principles.md`, `decisions/`, and `profile.md` / `goals.md`, which matter
  only when there is no workspace.

The brains are read-only. They recommend; the user commits. Never commit
anything from `.chief-of-staff/`, `context/` or `out/`.

## Layout

| Path | What |
|---|---|
| `src/claude.rs` | stream-json driver, audit log |
| `src/context.rs` | the workspace and `context/` inlined into prompts (brains have no file tools); connectors narrowed to approved sources; `deny-mcp.txt` |
| `src/state.rs` | the kit's `state.py` in Rust: validate, transition rules, lock, atomic journaled commit; `set_source_not_needed` (the Today row's Not needed) under the same lock |
| `src/track.rs` | what `cos` commits: run receipts and source coverage (tracking approved only); the brief's proposed deliverables as a candidate |
| `src/handoff.rs` | the kit's nine-part handoff, from a tracked deliverable |
| `src/proc/` | bounded child processes: one deadline, kill the group, reap; process start times and the group identity check; the process-wide group set a run records, and `spawn_gated`, which holds a child before `exec` until its group is recorded |
| `src/runs/` | run records (`out/run-records/<id>.json`, transitions under a per-record lock), the brief lock (a second brief exits 73), the start sweep, the owner watchdog and self-cleanup: every group a run created is gone before its final state is written. Runs are macOS-only (process start times) |
| `src/daemon/` | `cos daemon`: owns paid runs on `<user temp dir>/cos/daemon.sock` (0700 dir, 0600 socket, peer uid checked); requests `hello`, `start` (caps 2 asks per business, 4 runs total, `over_cap`), `stop`, `subscribe` (changes only), `drain` (refuses every later `start`, replies with the pid, holds off the exe-change exit; fails, changing nothing, while a run is live), `undrain`; writes its stderr to `<daemon dir>/daemon.log` (0600, appended); sweeps dead runs and its own groups (`daemon.groups.json`) at start; follows every business's hires (`agents::Follow`); SIGTERM stops its runs. `client.rs` the probe, `start`, `stop`; `install.rs` `cos daemon install` / `uninstall`, the `cos.daemon` launchd job: drains first, exit 75 while a run is live unless `--force` (stops the runs drain named, then drains), records checked only when nothing listens, fails when the drained pid outlives `bootout` and sends `undrain` on any failure after a drain, `COS_LAUNCHCTL` / `COS_LAUNCH_AGENTS`. Tests: `tests/daemon.rs`, `tests/daemon_install.rs` with `COS_DAEMON_DIR`, `COS_RUN_BIN`, `COS_DAEMON_DEADLINE_SECS`, `COS_DAEMON_UID` and the `cos-stand-in` binary |
| `src/bridge.rs` + `src/daemon/client.rs` | the paid MCP tools' runs: `brief` / `ask` return a run id at once, started on the daemon when its probe answers (`ENOENT`/`ECONNREFUSED` within 200 ms = down; any other error is an error; a socket directory or socket not the user's alone is never sent anything), else owned by `cos mcp` (owner `mcp`) under the daemon's caps until stdin closes; a `cos --budget` server owns its runs and passes the budget on; `RunView` for `run_status` and `runs`, read from records only, with a `cos://run/…` link for a registered business only. Tests: `tests/mcp_bridge.rs` (`--features stand-in`) |
| `src/connect/` | `cos connect status` / `claude-desktop` / `codex` / `remove`: only the `cos` entry in each client's config, `<file>.cos.orig` kept for a file that existed, a file changed mid-write refused, the configured binary's `--version` a bounded cached child; the desktop Settings → Assistants card calls the same code. Tests: `tests/connect.rs` |
| `src/herdr.rs` | herdr-core (`hc`) and the Herdr window (`herdr-app`) as cos uses them, every call a bounded child: server status (cached 3 s, stale on timeout) and start, `run`, `get`, `send`, `interrupt`, `kill`, `rm`, `read`, focus; `COS_HC` / `COS_HERDR_APP` name stand-ins |
| `src/agents/` | `cos agents`: hire Claude Code / Codex agents into Herdr terminals (`cos-<id>`, label `hire=<id>`) in seeded worktrees, each command line pinned to the business that hired it, then follow and judge them; Herdr owns every agent process, cos none. `follow.rs` reads each live terminal (`hc get`), stops it at its deadline (interrupt, then kill), judges it once (`judge.lock`; on `done` after the first message, an exit, the deadline or Mark done), say/pause/mark done/open/screen, and touches only terminals labelled with its hire; `harness.rs` the interactive command line; `osc.rs` the OSC 7501 records `cos agents watch --osc` writes, a live hire's word mirrored from its terminal; `slots.rs` run slots (live hires) and the build lock, `seed.rs` worktree seeding, `judge.rs` the verdict from diff, checks and scope (never the agent's summary), `policy.rs` hiring defaults from `context/agents.json` (an old `budget_usd`, `stall_secs` or `repeat` is ignored), `hire.rs` the plan before any spawn (repo resolution, refusal), `fit.rs` the pre-hire fit check over the policy's `fit` vocabulary, `ledger.rs` the verdict ledger and scorecard (no cost for runs in Herdr). Runs from before Herdr read, never resume. Tests: `tests/fixtures/fake-hc.sh`, never the real core |
| `apps/desktop/` | Tauri 2 app over the `cos` lib: a header with the brand, the view tabs, the dashboards, the business picker, the one way to switch business (its rows carry each business's live, lost, unread and schedule marks, its footer the window's directories, and a dot on the button flags another business's unread brief or lost hires); a settings gear whose Assistants card (`src/assistants.rs`, `ui/assistants.js`) shows Claude Desktop and Codex green or red through `connect_status`, registers, repairs or removes the `cos` entry through `connect_apply`, and names a version skew against the app's version or the daemon's `hello`; the report on `cosreport://` in a sandboxed iframe (hires waiting on you in Herdr first on Today, with Open in Herdr), whose Run brief (on the report's tab bar, shown only when the window announces itself) asks the window to run `cos brief` for the app business as a child it owns (`brief_run`); the Agents tab (staff roster by role, run verdicts, hire dialog that refuses without Herdr and offers Start Herdr; the run page keeps cos's verdict, checks, diff, contract and worktree, sends into Herdr with reply, Pause and Mark done, and folds the last screen); a follower thread that polls every registered business's live hires every 2 s (`agents::tick`); `tests/ipc.rs` drives its commands on the mock runtime against the stand-in Herdr; `examples/preview.rs` serves the window's UI to a browser over the same commands, against the stand-in Herdr unless `COS_HC` names another; `task desktop-app` installs `~/Applications/Chief of Staff.app`. The main window is built in setup (`"create": false`) with navigation and new-window hooks: http, https and mailto links from the window or any frame open in the default browser, the app's pages and its loopback dashboards stay in place, other schemes are refused (`tests/links.rs`). `cos://run/<business>/<run>`, `cos://brief/<business>/<day>` and `cos://settings/assistants` (`src/links.rs`; the deep-link and single-instance plugins in `with_links`, real app only) only navigate: held until the window calls `link_ready`, refused with the reason in the window, a switch window-only; a run opens the Today run panel over `run_get` and `run_stop` (daemon or this app's own runs only) |
| UI states (`apps/desktop/ui/states.js`, `ui/art/`, `ui/vendor/lucide/`) | every loading, empty, slow and error state in the window (its views, the business switcher, the dashboards) renders through `stateView(kind, …)` in `states.js`: an icon always (the vendored Lucide set, `icon()`), and at region level (a tab, a panel, a frame) the kind's state art from `ui/art/art.js` with a headline and the next action; a retry re-fetches, never reloads the window (a business switch is the one reload). Today, Flows and Docs frames show a state until their page loads, and a slow state with Try again after 6 s. Gate: `tests/design.rs` `every_state_renders_through_state_view_with_an_icon_and_art` and `every_animation_respects_reduced_motion` |
| `apps/desktop/ui/tokens.css` + `src/design.rs` | the one design token file (colour light/dark, Geist type scale, radii) over vendored Basecoat and Geist (`ui/vendor/README.md`); `design.rs` inlines it with the fonts into every page `cos` serves; `tests/design.rs` fails any page styling outside it |
| `src/gather.rs` | `cn` / `gh` facts collected without the model |
| `src/pipeline.rs` | brief and ask, shared by CLI and MCP |
| `src/reports.rs` | the brief's type + its JSON Schema |
| `src/store.rs` | `out/<kind>/<day>.{md,json}` |
| `src/mcp.rs` + `src/dashboard.html` | MCP server (stdio = all tools, HTTP = read-only) and the MCP App view |
| `src/report.rs` + `src/report.html` | the HTML report; `vendor/markdy/` is inlined into it |
| `src/today.rs` | the report's hero and Brief tab, rendered server-side (`tests/today.rs`: actions first, no raw URL text, dates in words) |
| `src/delivery.rs` | the Today page's delivery board: each Jira ticket with its PRs, code state, what the dev cluster runs and the next step, from `context/delivery.json` (strict, versioned); `cos` only renders it, the user or an agent writes it (`tests/delivery.rs`) |
| `src/constraints.rs` | the Today page's limiting factors: what currently stops a release, each with evidence, owner, confidence and the move that loosens it, from `context/constraints.json` (strict, versioned), limiting first; the brief reads the open factors and may only propose changes (`tests/constraints.rs`) |
| `src/prime.rs` | the Prime client: JSON-RPC `tools/call` to Prime's HTTP MCP endpoint (`COS_PRIME_URL`), literal loopback or `localhost` only, one deadline per call, replies capped at 8 MiB |
| `src/agents/skills.rs`, `skill_facts.rs`, `skill_bundle.rs`, `skill_view.rs` | a hire's skill set (`docs/plans/2026-11-28-hire-skill-graph-design.md`): the catalog of the skill roots (`COS_SKILLS_HOME`, else `$HOME`), whole-tree digests, the resolver; skill facts read from Prime (checked, folded, cached in `out/skills/graph.json`, asked once per 30 s and once at a time per home) and written by the user only (`cos skills set|curate`, the Skills pane), never by a hire (`COS_RUN_ID`); the session plugin under `out/agents/<id>/skills` and `skillOverrides` for Claude, `-c skills.config` for Codex (the user's own entries kept), the repo's own skills winning. Tests: `tests/common/prime.rs`, a stand-in Prime; `tests/fake_herdr` pins `COS_SKILLS_HOME` and a refused `COS_PRIME_URL` |
| `src/staff.rs` | `cos staff-sync`: refresh a configured staff role from an enrolled AllStack coordinator (bounded reads, revision checks, owner parking kept); the Plan tab's Staff card reads it (`docs/managed-staff.md`) |
| `src/history.rs` | the archive under `out/history/`, backfilled two months per run |
| `src/dash.rs` | `cos dash`, the live localhost page |
| `src/docs.rs` + `src/docs.html` | the documentation room: repo docs from `monitoredRepos`, Notion pages from `cos docs notion`, the user's topics, links and marks in `context/docs.json`, each repo document's lifecycle (front matter and folder, never age), `superseded_by` and broken links; served at `/docs` by `cos dash` and the desktop app |
| `src/flows.rs` + `src/flows.html` | `cos flows` and `/flows`: every ritual flow (a Chronis epic) in the main checkouts under the business's `--projects`, its step inferred from its beads and `ritual-*` markers; each bead's owner, layer and acceptance criteria; read-only, gated on an approved `tasks` source, one deadline for the scan (`tests/flows.rs`). The page is scripted with the scan inlined (CSP `flows::PAGE_CSP`, a frame with an opaque origin; `cos dash` adds `frame-ancestors 'none'`): Board, Flow graph of bead cards with a drawer, and a Map on SVG or vgpu (`tests/flows_page.rs`, `task test-e2e`); Refresh (or `r`) scans again. Inside the app it asks the window by message to open a `ritual-*` file (`flow_open`, checked by `open_target` and `contained`), to show an approval card, or to hire for a bead (`cos-flows-hire`; `flow_bead` reads it again from its store, `bead_to_hire` refuses one done or waiting on an open bead, and the Hire dialog opens on a Flow bead) |
| `src/ritual.rs` | `cos ritual approve` / `check`: the design's sha256 on the epic plus `cn approve`, the one place `cos` writes a task store (`docs/ritual.md`); one `approve` behind both the CLI (prompts on a terminal; `--yes` for an agent the user told to approve) and the desktop Flows tab's Approve button (`ritual_approve`, reachable only from the app's window and naming its business); `gates`, the flows waiting at the gate with their design digest and reviews, which the Flows tab draws as cards (`tests/ritual.rs`) |
| `.claude/skills/ritual/` | the ritual skill: Think, Spec, Plan, Build, Review, Ship, Reflect, and the `ritual-*` markers `cos flows` reads |
| `.claude/skills/ritual-full/`, `.claude/skills/semantic-isa/` | ritual with every tool on (EGM, a Semantic ISA section per design, two reviewers, `cos` hires); planned work in this repo runs as a `ritual-full` flow. All three skills are vendored byte-for-byte from decebal-claude-skills |
| `examples/demo.rs` | the synthetic home behind every screenshot (`task demo`), with fictional Chronis stores and a stand-in `cn` for `/flows` |
| `examples/wcag.rs` | the WCAG measurement as an autoresearch JSONL evaluator: axe-core over every view, dialog and dashboard of the window and its pages, light and dark, each theme on a fresh page, Chrome on a mock keychain (`docs/wcag.md`) |

Screenshots in `docs/assets/` come only from `task screenshots` on the demo
home. Never capture a real `COS_HOME`: briefs quote mail, calendar and tickets.

## Commands

```bash
task test               # fmt, clippy -D warnings, tests (the kit's Python tests run inside)
task desktop            # build cos, run the desktop app; `task desktop-check` lints it
task kit OUT=/abs.zip   # package the starter kit from release-files.json
task setup              # first run: seed context/, build, install
task install-mcp        # register with Claude Desktop + Codex
task schedule BUSINESS=acme  # launchd: weekday brief, the dashboard, for one business (default: the active one)
cos business list       # registry: each business, the active one, its directories
cos --dry-run brief     # print the prompt, call nothing
```

Agent files carry `@COS_HOME@`; `task install` substitutes the clone path.

## Review

`/prove-it` (`.claude/skills/prove-it/`) asks whether a change does what it
claims in this repo: the brains stay read-only, every child is owned and
reaped, every number comes from facts. Copilot code review reads the same
invariants from `.github/skills/prove-it-review/` and
`.github/instructions/review.instructions.md`.
