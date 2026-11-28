# Codex skill switch: spike

Date: 2026-11-28. Flow: hire skill graph (epic `t-6036dd`), story US-010.
Design: [2026-11-28-hire-skill-graph-design.md](../plans/2026-11-28-hire-skill-graph-design.md)
(assumptions A2 and A6).

## Classification

**Works.** One Codex invocation can switch a named skill off with
`-c 'skills.config=[{name="<x>",enabled=false}]'`, and Codex's
`workspace-write` sandbox reads files outside its workspace.

## What was run

Codex CLI 0.172.0, `codex exec --ephemeral -s workspace-write`, in a temp git
repo holding one stand-in skill, `.agents/skills/cos-spike/SKILL.md`. Each
run asked Codex, without running a command, for the exact name of every skill
in its instructions.

| Run | Override | Skills listed | `cos-spike` listed |
|---|---|---|---|
| 1 | none | 321 | yes |
| 2 | `-c 'skills.config=[{name="cos-spike",enabled=false}]'` | 320 | no |

Run 3 asked Codex to `cat` a file outside its workspace and reply with its
first line; it replied `OUTSIDE-READ-OK`, the file's content.

## Consequences for the design

- **A2**: a Codex hire's irrelevant skills in `~/.agents/skills` and
  `~/.codex/skills` are switched off with one `-c skills.config=[…]` on its
  command line (`agents::harness::argv`). `-c` replaces the whole array, so
  cos carries over the entries already in `$CODEX_HOME/config.toml` before its
  own (`skill_bundle::codex_config`). Names pass `skills::valid_name` before
  they reach the TOML, so none can close the string.
- **A6**: the skills block in a Codex prompt can name each picked skill's
  `SKILL.md` by its absolute path; Codex reads it there.
- Codex sees 321 skills on this machine without curation, the scale of the
  problem the brief names.

## Not checked

- Whether a disabled skill can still be invoked by name (`$cos-spike`).
- Codex versions other than 0.172.0.
