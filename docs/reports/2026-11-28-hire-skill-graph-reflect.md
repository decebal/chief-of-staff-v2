# Hire skill graph: what shipped

Date: 2026-11-28. Epic: t-6036dd. PR: https://github.com/decebal/chief-of-staff-v2/pull/66,
merged as `4c53826`.

## Shipped

Each `cos` hire runs with the skills its work needs: its role's relevant skills, plus
`--skill`, minus `--no-skill`, closed over `requires`. The decisions live in Prime as
`cos_skill` facts; every path comes from the local catalog of skill roots.

- Curation: `cos skills ls|show|set|curate`, the Skills pane in the Agents tab, the hire
  dialog's Skills section, the run page's Skills fold, `skills_add` / `skills_remove` on MCP
  `spin_off`.
- Claude hires load the set as a session plugin from `out/agents/<id>/skills` and get
  `skillOverrides` for skills switched off. Codex hires get the set in their prompt and
  `-c skills.config=[…]` for skills switched off, with the user's own entries kept.
- Evidence: [hire skill graph evidence](2026-11-28-hire-skill-graph.html). `task test`
  passed 60 test binaries with 0 failures on the merged commit's parent `0ff5822`;
  `task desktop-check` passed. Live runs: real Claude listed `cos-hire:hello-cos` and not the
  hidden skill; Codex 0.172.0 went from 321 to 320 skills with one disabled
  ([spike](2026-11-28-codex-skill-switch-spike.md)).

## Graph

12 beads planned (US-001 to US-012), 12 done, none added during Build. The Codex switch
spike (US-010) changed the design: Codex gained a hide path (sink S9), which the plan had
as prompt-only.

## Gates

| Gate | Caught |
|---|---|
| Design review, 3 rounds x 2 | round 3 left blocking items; the user chose to carry them to Plan |
| Built-as review, 3 rounds x 2 | r1: a malformed Codex config dropped the user's own entries. r2: S9 missing from the sink list. r3: the bundle trusted the path in `spec.json`; `exists()` hid a permission error; `--no-skill` left a natively loaded skill on |
| Code review r1 (prove-it) | 7 blocking: a reply with no `nodes` list read as fresh; `skills_hidden` missing from the ISA; no tests for S5 and S6; control characters allowed in `COS_PRIME_URL`; project skills counted as unreviewed; `SKILL.md` read through a symlink; temp-file name collision |
| Code review r2 | an unchecked Prime node id reached `prime_add_edge` (fixed with `node_id_ok`) |
| clippy `-D warnings` | `large_enum_variant` on `AgentCmd`; a slice clone |
| `task test` | the process-wide single-flight lock serialised parallel tests (made per home) |

`task test` also failed twice on `No space left on device` from other sessions' builds,
and once on the existing `probe_closed_port` race in the dashboards tests, which passed
when run alone. Neither was this change.

## Drift

The design's Built as section records every departure, and was approved again at sha256
`b37c18b5…` before merge: the Codex hide path (S9, VERIFY-C), single flight per home,
the module split, `harness::argv(spec, &Delivery)`, `.agents/skills` as project skills,
`COS_SKILLS_HOME`, VERIFY-B rescanning the roots at bundle time.

New-sink check against the merged code: Prime calls go only through `src/prime.rs`
(`Prime::call`), reached from `skill_facts::read_graph` and `skill_facts::write_fact`;
file writes are `graph.json` (E3), the bundle (E4), `spec.json` and `state.json` (E7,
E8); command-line effects are E5 and E11. No sink outside the inventory.

This change edits the design's Review history to point at the archived rounds, so
`cos ritual check` now reports it changed since approval; the change is links only.

## Next

- t-8fcec0: demo home skill roots and a stand-in Prime, so `task screenshots` captures the
  Skills pane, the hire dialog's Skills section and the run page fold.
- t-72ab77: read `prime_add_node`'s real reply once and pin the id field used for
  `requires` edges (assumption A5).
- t-0a94df: whether Codex can still invoke a switched-off skill by name.
- t-9211c4: validate `skills.roots` entries; fix stale line references in the design.
