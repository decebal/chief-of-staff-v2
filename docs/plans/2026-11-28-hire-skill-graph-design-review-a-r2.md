---
design: docs/plans/2026-11-28-hire-skill-graph-design.md
design-sha256: superseded
verdict: findings
---

## Round 1 findings
1. closed: E7 (`spec.json` `skills`, `record_run`) and E8 (`state.json` `skills_bundled`, `prepare`) are in the inventory (design lines 276-277).
2. closed: VERIFY-U now accepts literal IPs and `localhost` only, no resolver (design lines 305-308).
3. closed: step 1 creates `skills.tmp/skills/`, so `cp -Rc` into `skills.tmp/skills/<name>` has a parent (design lines 207-213). Retry path still broken, see New finding 1.
4. closed: VERIFY-B forbids any symlink below the top directory, and the copy's digest is rechecked (design lines 319-326); S3 row covers an inner symlink (line 355).
5. closed: `harness::argv(spec, bundle: Option<&Path>)` (design line 220); `harness.rs:16` today is `argv(&Spec)`, so the change is named.
6. closed: `SkillsPolicy` added to `Policy` with optional `roots`/`unreviewed` (design lines 179-181); loads with and without key (line 416). `policy.rs:109` is `deny_unknown_fields`, confirmed.
7. closed: fold is VERIFY-R first, then newest-wins (design lines 96-98). No test row for newest-invalid, see Carried.
8. closed: one rule, literal loopback, in §1 and VERIFY-U.
9. open (non-blocking): source id is named (node id from `prime_add_node`, lines 155-158), but target ids ("the graph's newest snapshot ids") have no field: `Fact` (lines 92-94) and `graph.json` carry no id. Moved to Carried.
10. closed: dry run reads `graph.json` only, no Prime call, no write (design lines 152-153).
11. closed: tree digest defined over all regular files (lines 85-91); Acceptance 4 uses the tree digest (line 451).
12. open (non-blocking): repo `.claude/skills` scope stated as out (lines 80-83), but no plan warning is specified. Moved to Carried.
13. closed: "Prime down during a curation write" row, exit 1, nothing queued (design line 346).
14. open: see New finding 1. Step 1 removes `skills.tmp` only; a second `launch` of the same id reruns `prepare`.
15. closed: `main.rs:921` (`AgentCmd::Spawn`), `main.rs:987` (`hire::plan`), `mcp.rs:242`, `mcp.rs:286`, `mcp.rs:295` all match the code.
16. closed: Codex read-outside-worktree is now A6 (design lines 490-492).
17. closed: Prime counts labelled as author measurement in A4 (lines 484-485). The Context line (54-55) still states them unmarked; see Carried.

## New findings
1. blocking §4 Delivery, steps 1 and 4: `rename skills.tmp → skills` fails when `skills/` already exists from an earlier attempt, so a second launch of the same run id fails before Herdr. Evidence: `src/agents/mod.rs:608` `prepare(...)` runs on every `launch`, unconditionally; `mod.rs:624` `harness::argv` is after it; design step 1 (line 207) removes only `skills.tmp`, step 4 (line 217) renames over `skills`. A non-empty target directory makes `rename(2)` fail on macOS. Fix: step 1 also removes `out/agents/<id>/skills` (`remove_dir_all`, ignore NotFound) before step 4, and the test list (line 412-413) gets a row: a second `prepare` for the same id leaves one `skills/` with the digests recorded.

## Carried to Plan
- Edge targets (§2 Write, line 157): `Fact` and `graph.json` hold no node id, so "the graph's newest snapshot ids" cannot be produced. Either add `id` to the stored fold or drop `requires` edges (they are never read back).
- Fold test row (§Testing `tests/skills.rs`): add "newest fact invalid, older valid, skill is picked from the older" as a named case.
- Repo `.claude/skills` (line 80-83): add a plan warning "repo skills load natively" when the worktree has `.claude/skills`, so the scope is visible in the plan as well as the doc.
- Prime-down-on-write (line 346): add an acceptance row (exit 1, URL named, nothing written in `graph.json`).
- Context line 54-55: mark the "37 `execution_role` snapshots" figure as the author's measurement, as A4 does.
- Acceptance 4: the `skills/skills/<name>` path needs the `skills.tmp` → `skills` rename to have happened; state that the plan is checked after `state.json` gets `skills_bundled`.
