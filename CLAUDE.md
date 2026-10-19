# chief-of-staff

A personal chief of staff: a Rust binary, `cos`, that drives headless Claude
Code (`claude -p --output-format stream-json`) with two brains, and serves
them over MCP to Claude Desktop, Codex and claude.ai. User guide: `README.md`.

| Agent | Horizon | Invoked by |
|---|---|---|
| `chief-of-staff` | routes, never answers domain questions | `cos ask "…"` |
| `tactician` | today, this week | `cos brief` |
| `strategist` | this week against the quarter | `cos review` |

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

- `.chief-of-staff/` — `profile.json` (approved sources, timezone,
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
| `src/track.rs` | what `cos` commits: run receipts and source coverage (tracking approved only); review proposals as a candidate |
| `src/handoff.rs` | the kit's nine-part handoff, from a tracked deliverable |
| `src/proc.rs` | bounded child processes: one deadline, kill the group, reap |
| `src/agents/` | `cos agents`: spawn, supervise, monitor and stop Claude Code / Codex agents in worktrees |
| `src/gather.rs` | `cn` / `gh` facts collected without the model |
| `src/pipeline.rs` | brief, review, ask — shared by CLI and MCP |
| `src/reports.rs` | typed outputs + their JSON Schemas |
| `src/store.rs` | `out/<kind>/<day>.{md,json}` |
| `src/mcp.rs` + `src/dashboard.html` | MCP server (stdio = all tools, HTTP = read-only) and the MCP App view |
| `src/report.rs` + `src/report.html` | the HTML report; `vendor/markdy/` is inlined into it |
| `src/history.rs` | the archive under `out/history/`, backfilled two months per run |
| `src/dash.rs` | `cos dash`, the live localhost page |
| `examples/demo.rs` | the synthetic home behind every screenshot (`task demo`) |

Screenshots in `docs/assets/` come only from `task screenshots` on the demo
home. Never capture a real `COS_HOME`: briefs quote mail, calendar and tickets.

## Commands

```bash
task test               # fmt, clippy -D warnings, tests (the kit's Python tests run inside)
task kit OUT=/abs.zip   # package the starter kit from release-files.json
task setup              # first run: seed context/, build, install
task install-mcp        # register with Claude Desktop + Codex
task schedule           # launchd: weekday brief, Friday review
cos --dry-run brief     # print the prompt, call nothing
```

Agent files carry `@COS_HOME@`; `task install` substitutes the clone path.

## Review

`/prove-it` (`.claude/skills/prove-it/`) asks whether a change does what it
claims in this repo: the brains stay read-only, every child is owned and
reaped, every number comes from facts. Copilot code review reads the same
invariants from `.github/skills/prove-it-review/` and
`.github/instructions/review.instructions.md`.
