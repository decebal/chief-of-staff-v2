---
design: docs/plans/2026-11-28-hire-skill-graph-design.md
design-sha256: superseded
verdict: findings
---

## Round 1 findings

1. closed: §2 "Policy gains an optional, deny_unknown_fields `skills`" (design lines 179-182) and Testing `tests/hiring.rs` row (line 415-416). Code confirms the failure mode: `src/agents/policy.rs:109` `deny_unknown_fields`, `policy.rs:141-144` fallback to `Policy { note, ..default }` drops roles.
2. closed: §4 (design lines 227-231) puts the Codex block in `Plan::request`, which has `run.harness` (`src/agents/hire.rs:168-169`, `:181`). E6 no longer names `hire::plan`.
3. closed: §4 (design lines 203-205) sets one 300 s budget for the whole bundle, and `clone_cmd` is named `pub(crate)` (line 213). Code: `seed.rs:22` and `:126` show `STEP_DEADLINE` applied per `proc::run`; `seed.rs:147-159` shows `clone_cmd` is private (cfg-split). `proc::Deadline` exists (`src/proc.rs:42`) if the implementer wants it named.
4. closed: §Terms (design lines 88-91) digest covers every regular file, refuses inner symlinks and trees over 20 MiB; VERIFY-B (lines 319-326) checks the source and the copy; ISA S3 (line 355) includes an inner symlink case.
5. closed: §2 (design lines 143-147) records `failed_at` and uses the cached graph for 30 s after a failure. Remaining concurrency gap is raised below as new finding 2.
6. closed: §2 (design lines 148-151) uses `graph.json.<pid>.<nanos>.tmp` and rename; ISA S6 (line 359) has the two-writer case.
7. closed: VERIFY-U (design lines 306-308) accepts only literal loopback IPs or `localhost` with no resolver; ISA S1 (line 353) covers `example.com` as refused before any lookup. Remaining: see Carried to Plan on `localhost`.
8. closed: step 4 (design lines 172-177) and Acceptance 10 (lines 467-471) state the day-one behaviour (`unreviewed: on` keeps unreviewed skills on, plan warns with the count). The Decision section still does not state it; see Carried to Plan.
9. closed: §3 names the callers (design lines 25-31 Context: `main.rs:921`/`:987`, `mcp.rs:295`, `lib.rs:1382`/`:1414`) and the three test sites (`tests/fit.rs:125`, `:212`, `:348`, confirmed by rg).

## New findings

1. blocking §4 Delivery and §2 Resolve step 4: `skillOverrides` is keyed by name, and the design's own cited docs say it hides "a user or project skill" (design line 64-65). Hidden names come from the claude root (line 173-174), so a repo's own skill with the same name is switched off for every hire in that repo. This contradicts design line 82-83 ("this design neither bundles nor hides [project skills]"). Evidence: repo `.claude/skills/` holds `ritual`, `ritual-full`, `semantic-isa`, and `~/.claude/skills/` holds the same three names (rg/ls, 2026-11-28). Once any of them is marked irrelevant, the worktree's own copy is hidden. Fix: never add to `hidden` a name that exists under the plan's repo `.claude/skills/` (or the worktree's); warn naming it. Add a Testing and ISA row: a worktree with `.claude/skills/x` and an irrelevant user-level `x` keeps `x` visible in argv.

2. blocking §2 Read ("Asking Prime at most every 30 s"): the bound claimed ("blocks for at most one 2 s deadline per 30 s, not one per keystroke") holds only for calls that start after a result is recorded. Desktop fit checks are debounced at 150 ms per input (`apps/desktop/ui/app.js:1217`), each runs `plan` in a `blocking` task (`apps/desktop/src/lib.rs:1433-1435`), and nothing makes concurrent calls share one Prime read. While Prime is slow, every keystroke after the first 2 s starts another blocking read, so the number of in-flight Prime calls is unbounded by the design. This is the hot-path pile-up the process-ownership rule names. Fix: single-flight. An in-process mutex plus a try-lock on a `graph.json.lock` file: the caller that loses the lock reads the file and makes no call. State it in §2 and add an Acceptance row: ten concurrent plans with Prime stopped make one request.

3. blocking §Semantic ISA / Not-over-MCP claim: the design says hires never write skill facts, and that "Claude's sandbox refuses network" (design lines 261-262). VERIFY-W (lines 314-318) admits `cos skills set` from any caller with no check of hire origin. The only gate is the Claude Seatbelt network setting, `CLAUDE_SANDBOX` `strictAllowlist` (`src/agents/harness.rs:13`). The module doc of the same file says a network call "asks you in the terminal" (`harness.rs:4-5`), and the constant's own comment says "refused without a prompt" (`harness.rs:12`). The two readings give different outcomes for a hire's `cos skills set --relevant`, and the design picks neither. Evidence: `harness.rs:4-5`, `:12-13`; design lines 314-318, 261-262. Fix: state which behaviour the sandbox has (test it once in the hand-check), and either refuse `set`/`curate` when run inside a hire (an env marker set in `environment()`, `src/agents/mod.rs` near line 624) or add an ISA row: a `cos skills set` run from a Claude hire's Bash writes nothing.

## Carried to Plan

- Decision section (design lines 8-21) does not state the day-one behaviour; the Goal in the brief does. Add one sentence: unreviewed skills stay on until curated, and the plan says how many.
- Acceptance 2 (design lines 444-446) does not test the 30 s `failed_at` window; add it.
- Confidence "Prime client and fold: rock, protocol checked against the live server" (design line 429) overstates. Only `prime_stats` was checked (design line 54-55). The `prime_search` argument shape, the `content[0].text` envelope and the need for `Accept` headers are unverified. Downgrade to sand or check before Build.
- VERIFY-U accepts `localhost`, which the design maps to 127.0.0.1 without a resolver. Confirm the Prime client connects to the literal address, not the name, so no resolver runs in the connect call.
- `skills.roots` (design lines 179-181) takes absolute paths from `context/agents.json`, and VERIFY-R/B say nothing about them. Add: roots must be absolute and an existing directory, else dropped with a warning.
- Catalog directory names are not VERIFY-checked (only facts are, VERIFY-R). Their names are printed in plan output and used as `skillOverrides` keys. serde keeps argv safe, but plan output is not stripped (VERIFY-P covers descriptions only). Add a name regex to the catalog scan.
- `record_run` writes `spec.json` (`src/agents/mod.rs:580`, design E7) before the bundle step (§4). A hire that fails at bundle time leaves a `spec.json` whose `skills` never passed VERIFY-B. Say so in S7, or write `spec.json` after the bundle.
- Claude reads the bundle from `out/agents/<id>/skills`, outside the worktree (Acceptance 4). The Seatbelt read policy for that path is not checked. Add it to the hand-check.
- Total `prepare` time is the sum of seed steps (each `STEP_DEADLINE`, `seed.rs:22`) plus the bundle budget. The design bounds the bundle but not the whole `prepare`; state it.
- A1 and A2 remain unverified by design (Confidence rows); nothing new.
- Repo `CLAUDE.md` invariants checked: brains read-only (MCP spin_off is `Local` only, `src/mcp.rs:240-242`, remote mode gets no skill tool) holds; every child via `proc::run` holds for `cp`; judge reads worktree only (`judge.rs:247-254`), bundle is outside it. No violation found there.
