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
  `state::commit` or the kit's `state.py`. Gitignored.
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
| `src/state.rs` | the kit's `state.py` in Rust: validate, transition rules, lock, atomic journaled commit |
| `src/track.rs` | what `cos` commits: run receipts and source coverage (tracking approved only); the brief's proposed deliverables as a candidate |
| `src/handoff.rs` | the kit's nine-part handoff, from a tracked deliverable |
| `src/proc.rs` | bounded child processes: one deadline, kill the group, reap |
| `src/agents/` | `cos agents`: spawn, supervise, monitor and stop Claude Code / Codex agents in worktrees; `slots.rs` run slots and the build lock, `seed.rs` worktree seeding, `judge.rs` the verdict from diff, checks and scope (never the agent's summary), `policy.rs` hiring defaults from `context/agents.json`, `hire.rs` the plan before any spawn (repo resolution, refusal), `fit.rs` the pre-hire fit check over the policy's `fit` vocabulary, `watch.rs` stalled/looping flags from the stream, `ledger.rs` the verdict ledger and scorecard |
| `apps/desktop/` | Tauri 2 app over the `cos` lib: the report on `cosreport://` in a sandboxed iframe, the Agents tab (staff roster by role, run verdicts, hire dialog); `tests/ipc.rs` drives its commands on the mock runtime; `examples/preview.rs` serves the window's UI to a browser over the same commands; `task desktop-app` installs `~/Applications/Chief of Staff.app` |
| `apps/desktop/ui/tokens.css` + `src/design.rs` | the one design token file (colour light/dark, Geist type scale, radii) over vendored Basecoat and Geist (`ui/vendor/README.md`); `design.rs` inlines it with the fonts into every page `cos` serves; `tests/design.rs` fails any page styling outside it |
| `src/gather.rs` | `cn` / `gh` facts collected without the model |
| `src/pipeline.rs` | brief and ask, shared by CLI and MCP |
| `src/reports.rs` | the brief's type + its JSON Schema |
| `src/store.rs` | `out/<kind>/<day>.{md,json}` |
| `src/mcp.rs` + `src/dashboard.html` | MCP server (stdio = all tools, HTTP = read-only) and the MCP App view |
| `src/report.rs` + `src/report.html` | the HTML report; `vendor/markdy/` is inlined into it |
| `src/today.rs` | the report's hero and Brief tab, rendered server-side (`tests/today.rs`: actions first, no raw URL text, dates in words) |
| `src/staff.rs` | `cos staff-sync`: refresh a configured staff role from an enrolled AllStack coordinator (bounded reads, revision checks, owner parking kept); the Plan tab's Staff card reads it (`docs/managed-staff.md`) |
| `src/history.rs` | the archive under `out/history/`, backfilled two months per run |
| `src/dash.rs` | `cos dash`, the live localhost page |
| `src/docs.rs` + `src/docs.html` | the documentation room: repo docs from `monitoredRepos`, Notion pages from `cos docs notion`, the user's topics, links and marks in `context/docs.json`; served at `/docs` by `cos dash` and the desktop app |
| `src/flows.rs` + `src/flows.html` | `cos flows` and `/flows`: every ritual flow (a Chronis epic) in the main checkouts under `--projects`, its step inferred from its beads and `ritual-*` markers, its bead graph; read-only, gated on an approved `tasks` source, one deadline for the scan (`tests/flows.rs`) |
| `src/ritual.rs` | `cos ritual approve` / `check`: the design's sha256 on the epic plus `cn approve`; approve is CLI-only and prompts on a terminal (`--yes` for an agent the user told to approve), the one place `cos` writes a task store (`docs/ritual.md`) |
| `.claude/skills/ritual/` | the ritual skill: Think, Spec, Plan, Build, Review, Ship, Reflect, and the `ritual-*` markers `cos flows` reads |
| `.claude/skills/ritual-full/`, `.claude/skills/semantic-isa/` | ritual with every tool on (EGM, a Semantic ISA section per design, two reviewers, `cos` hires); planned work in this repo runs as a `ritual-full` flow. All three skills are vendored byte-for-byte from decebal-claude-skills |
| `examples/demo.rs` | the synthetic home behind every screenshot (`task demo`), with fictional Chronis stores and a stand-in `cn` for `/flows` |

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
