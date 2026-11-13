# Multiple businesses in one cos: brief

## Problem

One person runs several businesses, each with its own goals, decisions, approved sources,
state and repos. cos holds exactly one: a home (`context/`, `out/`), a workspace
(`.chief-of-staff/`: profile, state) and a projects directory, set by `--home`/`COS_HOME`,
`--workspace`/`COS_WORKSPACE` and `COS_PROJECTS`, or by the desktop app's `settings.json`.
Switching today means editing env vars or a JSON file, and the app's only switch
(`set_workspace`) changes the workspace alone, leaving home and projects pointing at the
previous business.

## The job when this is done

In the desktop app, press the command select (a searchable picker), type part of a business
name, pick it, and every view in the window (report, plan, flows, docs, agents, staff) shows
that business: its context, its profile and state, its repos. From a terminal,
`cos --business <name> brief` runs against that business, and `cos business use <name>` makes
it the default for CLI commands and newly started processes; the app starts on that default and
then switches on its own.

## Decisions (from the user, 2026-11-12)

- A business is all three directories: its own home, workspace and projects.
- The switcher is a command-select UI component in the desktop app.
- Scope: the app switches everything in its window; the CLI gains `--business <name>` and
  `cos business list | add | use | remove`. `cos dash`, `cos mcp`, `cos serve`/`tunnel` and the
  launchd jobs stay bound to one business per process, chosen at launch with `--business`;
  switching them is a restart.
- Agents: the Agents view lists the active business's agents, with a badge counting agents
  running in other businesses that switches to them. Slot pools stay per business.
- Registry: `~/.config/cos/businesses.toml` (name, home, workspace, projects, which is active),
  read by both the CLI and the app. A `COS_*` env var still overrides for one command.
- Migration: on first run the current triple becomes a business named `default`, active, with
  no files moved.

## Found while mapping (in scope)

- Several functions name their parameter `home` but receive the workspace (`pipeline.rs:119-120`
  and `183-184`, whose callers pass `self.workspace` at `pipeline.rs:344,360,388,408,459`;
  `track.rs:58-72,192`; `context.rs:72-74,203-248`; `main.rs:775`). The reads are correct today;
  the names invite routing the wrong directory once a business is threaded through, so they are
  renamed first.
- The CLI resolves flag > env > default; the app resolves env > saved > default, and `save()`
  writes env-derived values back into `settings.json`. One precedence for both.

## Assumptions (mark wrong ones at review)

- Business names are lowercase slugs; directories are absolute and must exist; a workspace must
  hold `.chief-of-staff/profile.json` (`cos --business x state init` creates it).
- Kit templates and the `cos` supervisor binary resolve from the install, not per business.
- The Claude OAuth token (keychain, user-global) is shared by all businesses.
- A brief or ask already running when the app switches finishes against the business it
  started in.

## Out of scope

Per-business launchd labels and ports, a dashboard that follows the switch live, one MCP or
tunnel serving several businesses, per-business credentials.

Map of every affected site: produced by a five-reader survey of `origin/main` on 2026-11-12,
summarised in the design.
