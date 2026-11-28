# Hire skill graph: brief

Date: 2026-11-28.

## Problem

A hire started by `cos agents` (`src/agents/harness.rs` `argv`) gets its
role preamble and the task prompt. Its skills are whatever the harness finds
on the machine: today 60 entries under `~/.claude/skills`, 70 under
`~/.codex/skills` and 100 under `~/.agents/skills`, plus installed plugins.
Most are irrelevant to any one hire (SEO, property scouting and gstack skills
sit beside the Rust and review skills a builder needs), and the user cannot
say which ones a hire should use:

- Every skill's name and description sits in the hire's context, relevant or
  not, and the model picks among all of them.
- A skill the work needs, kept in a repo or a project directory rather than a
  global one, is never offered.
- Nothing records which skills a hire had, so a verdict cannot be traced back
  to the context it ran with.

## Goal

Each hire pulls in the skills its work needs and no others, chosen from a
knowledge graph of skills held in Prime. The graph says which skills exist
(discovered from the global skill directories), which the user has reviewed
as relevant, which role uses which skill, and which skill requires another.
At hire time `cos` resolves the role's skills, the hire's own additions and
removals, and every skill they require; the hire dialog, `cos agents spawn`
and the MCP `spin_off` tool show that set before anything starts; the hire
runs with exactly that set; and the run records it with a digest per skill. A
curation utility (`cos skills curate`, and the same list in the desktop
Agents tab) walks the user through unreviewed global skills to mark each
relevant or not and attach it to roles.

## Approaches

1. **Prime graph, per-hire plugin bundle (recommended).** Skill, role and
   `requires` facts live in Prime (`127.0.0.1:3906/mcp`, the user's choice).
   At hire time `cos` copies the resolved skill directories into
   `out/agents/<id>/skills/` as a session plugin and starts Claude with
   `--plugin-dir` on it, so the skills load with progressive disclosure
   (name and description until invoked), sit outside the worktree and never
   reach the diff the judge reads. The global skills the user marked
   irrelevant are kept out of the session through the harness's own settings.
2. **Copy into the worktree's `.claude/skills/`.** Native discovery, but the
   copies show up in the diff: a reviewer whose scope is `REVIEW.md` fails its
   scope check on every hire, and a scratch-directory hire has no repo to copy
   into. Lost on the judge.
3. **Inline each `SKILL.md` into the prompt.** Simplest, but every skill's
   full text costs context up front, scripts and references beside the
   `SKILL.md` cannot come along, and progressive disclosure is gone. Lost on
   context cost.

A graph in a repo file (`context/skills.json`) or one built by graphify were
offered; the user chose Prime.

## Constraints

- A hire never waits on Prime. Reads are bounded; when Prime is down or slow
  the last graph `cos` read is used and the plan says it is stale. No graph at
  all means the role's skills are unknown and the plan says so; the hire still
  starts.
- Prime content is untrusted input: anything on the machine can write to it.
  A skill path from Prime is used only when it resolves inside a discovered
  skill root and holds a `SKILL.md`.
- Loopback only: the Prime client refuses any host but `127.0.0.1` / `::1`.
  No new network dependency for the brains or the remote MCP server.
- Prime sync is off on this machine (`prime_stats`: `LOCAL-ONLY`); curation
  writes stay local and do not appear on the AllSource dashboard.
- The brains stay read-only. Curation is a user action; a hire reads the
  graph and never writes it.
- Every child process `cos` starts is owned and reaped (`proc`); the bundle
  copy runs under the seed step's deadline.
- The judge reads diff, checks and scope, never the agent's summary; the
  bundle must not change any of the three.
- Overlap: the role-library flow (epic `t-b0b659`) adds fields to `Role` and
  the preamble. Role-to-skill links live in Prime, so this flow leaves `Role`
  alone.
- Desktop UI: every state through `stateView`, tokens only
  (`tests/design.rs`).

## Unknowns

- **Hiding a global skill from one session** (`sand`). Claude's
  `--plugin-dir` adds skills; whether `--settings` can hide named user-level
  skills (a `Skill(<name>)` deny rule, a plugin toggle) while keeping user
  hooks and `CLAUDE.md` needs confirming. Moves to rock: a documented setting,
  tried against a stand-in skill in a test home.
- **Codex skill loading** (`sand`). Codex reads `~/.codex/skills` and
  `~/.agents/skills`; a per-session skills directory or disable switch is not
  confirmed. Moves to rock: the documented mechanism, or a `CODEX_HOME`
  overlay that keeps auth and config.
- **Prime has no update** (`sand`). `prime_add_node` appends; the store holds
  several snapshots per key (37 `execution_role` nodes for a handful of
  roles). Moves to rock: the skill node carries its `requires` and `roles`
  as properties, read by folding the newest snapshot per key, with edges
  added for exploration only.
- **Graph size** (`rock`). About 230 skills; one `prime_search` by type
  returns them all in one response.
