---
design: docs/plans/2026-11-28-hire-skill-graph-design.md
design-sha256: superseded
verdict: findings
---

## Earlier findings

Round 1 (all closed in r2, re-checked):
1. closed: §2 policy `skills` field (design lines 189-192); `tests/hiring.rs` row (line 430-440).
2. closed: §4 Codex block in `Plan::request` (line 239-243).
3. closed: §4 one 300 s bundle budget (line 212-215).
4. closed: Terms digest and inner-symlink refusal (lines 87-91); VERIFY-B (337-344); S3 row (373).
5. closed: `failed_at` and 30 s window (line 139-150); Testing `tests/prime.rs` row (line 424-426) now covers "no second call within 30 s of a success or a failure".
6. closed: temp-and-rename (line 151-154); S6 row (377).
7. closed: VERIFY-U literal IP or `localhost` mapped to 127.0.0.1, no resolver (line 323-326, 297). Carried item on `localhost` is closed by this wording.
8. open (partial): Decision section (lines 6-21) still does not state day-one behaviour. Behaviour is in §2 line 188-195 and Acceptance 10. Carried.
9. closed: callers named (line 25-31).

Round 2 (r2 "New findings"):
1. closed: project-skill name collision. §2 step 5 (line 181-187) never hides a name the repo holds in `.claude/skills/`; the picked copy is recorded `project`, not bundled, not hidden. ISA S4 (line 375) and Testing (line 435-437) cover it.
2. closed: concurrency. §2 line 144-150 adds a per-process `try_lock` Mutex and 30 s throttle via `graph.json`; Testing line 425-429 tests ten concurrent reads. Wording caveat carried below (the CLI "adds at most one more" is per invocation, not total).
3. open: hire can still reach `cos skills set`. See New finding 1.

## New findings

1. blocking §5 Not over MCP (lines 272-280) and VERIFY-W (line 332-336): the first guard is not a boundary, and the second rests on an unverified sandbox behaviour the code describes two ways.
   - Guard 1 is `COS_RUN_ID` refusal. `environment()` pins `COS_RUN_ID={spec.id}` at `src/agents/mod.rs:731`, but it is an env var in the hire's shell. A hire's Bash runs `/usr/bin/env -u COS_RUN_ID cos skills set <name> --relevant`; `env` is not sandboxed. The design's claim "two checks that do not depend on each other" holds only if guard 2 holds.
   - Guard 2 is the sandbox. `src/agents/harness.rs:4-5` says a network call "asks you in the terminal", but `harness.rs:12` says "network refused without a prompt". `CLAUDE_SANDBOX` (`harness.rs:13`) sets `network.strictAllowlist: true`; the design does not say whether `127.0.0.1:3906` is on the allowlist. If loopback is allowed, the write goes through with no prompt. The design (line 277-280) picks the prompt reading without a test.
   - Evidence: `src/agents/mod.rs:708-733`, `src/agents/harness.rs:4-5,12-13`, design lines 275-280.
   - Fix: pick one. (a) Make the write path not reachable from a hire's shell: `cos skills set` requires a secret outside the run directory (e.g. a user-held token in `context/`, not readable by hires) or an interactive TTY for `set` too, and hires have no route to either. (b) Or keep the CLI open and add an ISA row plus a hand-check: a `cos skills set` run from a Claude hire's Bash, with `env -u COS_RUN_ID`, writes nothing to the stand-in Prime. Either way state which behaviour `strictAllowlist` gives loopback, and drop the claim that guard 1 is independent.

## Carried to Plan

- Decision section (lines 6-21) does not state day-one behaviour (unreviewed stays on until curated; plan prints the count). Add one sentence.
- Confidence row "Prime client and fold: rock — protocol checked against the live server" (line 453) overstates; only `prime_stats` was checked (line 52-56). Downgrade to sand or check the `prime_search` argument shape, `content[0].text` envelope and `Accept` header before Build.
- `skills.roots` (line 79-81, 179) takes absolute paths from `context/agents.json`; VERIFY-R/B say nothing. Add: roots must be absolute existing directories, else dropped with a warning.
- Catalog directory names are not VERIFY-checked. VERIFY-B mentions "the validated name" (line 342) but defines no regex. Add the name regex to the catalog scan; names are printed in plan output and used as `skillOverrides` keys.
- Concurrency wording (line 148-150): "the CLI, a separate process, adds at most one more" holds per CLI invocation. Scripted concurrent `cos agents spawn` calls each add one. Reword as "each CLI invocation adds at most one call in flight".
- `record_run` writes `spec.json` with `skills` (`mod.rs:580`) before `prepare` runs the bundle (`launch`, `mod.rs:602-608`). VERIFY-S (line 345-348) says `skills` holds only names through VERIFY-B, which is false at that point. Write `spec.json` after the bundle, or say in S7 that a failed bundle leaves `skills` naming the failed skill.
- Whole `prepare` is not bounded: sum of seed `STEP_DEADLINE`s (`seed.rs:22`) plus the 300 s bundle budget. `timeouts.md` makes 5 min a ceiling; a 300 s bundle budget sits on it. Lower it (the copies are APFS clones) and state a bound for `prepare`.
- Project-skill check (§2 step 5, line 181-187) reads `.claude/skills/` from the plan-time repo. The hire's worktree is seeded from its own ref (`seed.rs`). A project skill present in the worktree but not in the plan-time checkout is not checked. Read the names from the seeded worktree, or state that the plan repo is the seed source.
- Claude reads the bundle from `out/agents/<id>/skills`, outside the worktree (Acceptance 4, line 473-478). Seatbelt read policy for that path is not checked. Add it to the hand-check (line 446-447).
- A1 (`--plugin-dir` outside the working directory) and A2 (Codex per-invocation switch) stay unverified; Confidence rows cover them.
- A6 (Codex workspace-write read outside the workspace) stays open until the spike.
- Repo `CLAUDE.md` invariants checked again: brains read-only (MCP spin_off is `Local` only per r2 `src/mcp.rs:240-242`); every child via `proc::run` (`cp -Rc`, line 222-225); judge reads worktree only (bundle is outside it, judge.rs:247-254 per r2). No violation found.
