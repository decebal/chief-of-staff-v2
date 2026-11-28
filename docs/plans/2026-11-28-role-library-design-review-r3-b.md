---
design: docs/plans/2026-11-28-role-library-design.md
design-sha256: c99f344ab936c72683c2932c001a09533cc3fd95178ff49019860de444f2d8a1
verdict: findings
---

## Round 2 findings
- r2-b #1: closed. Design §6 L346-354 (only the thread that spawned `claude` signals it) and §7 L372-380 (`roles_propose` runs `roles::propose` in-process on an owned thread; slot holds `stop` and `done` only, no pid). Paper fix; `claude::execute_until` and `stream_lines_until` do not exist yet (`src/claude.rs:193` has `execute` only).
- r2-b #2: closed. Inventory adds "Create `context/`" (L461). The spawned `cos roles propose` child row is unnecessary because propose is in-process (L380). Owner-death gap is finding 1.
- r1-b #4 (open in r2-b): closed. Cancel and quit now reach the owning thread's `claude` child (L353-354, L377-380). Kill path: `src/proc.rs:291` `kill_and_reap`, `src/proc.rs:210-211` `spawn_grouped`.

## Findings
1. blocking §6 / §Semantic ISA side-effect inventory (L462, "Ends at deadline, stop flag or exit"): owner death is not covered. If `cos` or the desktop app dies (SIGKILL, crash, force quit, terminal hang-up), the thread that holds the deadline dies with it. `claude` (own group, `src/proc.rs:210-211`) then runs with no deadline and no stop flag. Breaks prove-it I3 (`.claude/skills/prove-it/SKILL.md:54`: one owner, one deadline, killed and reaped). Evidence: design L349-353, L462; `src/claude.rs:193-198` (deadline passed to `stream_lines` in the owner). Fix: add an owner-death row to the inventory and to §6, naming what ends `claude` (the closed stdout pipe fails its next write; `--max-budget-usd` at `src/claude.rs:127` caps spend). Add a test: SIGKILL the owner mid-run and assert the `claude` pid is gone within a stated bound. If it survives, add a parent-death watchdog and name it in §6.

2. blocking §4 `roles::apply` / §Semantic ISA `STORE(agents.json)` / §5 `new --file`: `checks` reach a shell unverified. `judge.rs:292-293` runs every contract check with `sh -c` in the hire's worktree, as the user, outside the agent sandbox. `cos roles new --file <role.json> --yes` (design L332, L335) and `roles_save`'s `change` argument (L375) admit any `checks`, `scope`, `harness`, `output`, `expects` or `model` value. Nothing checks them against `from`'s library entry or the existing role. `policy_valid` (L196-204) has no `checks` rule. The taint list names `--file` role JSON as a source (L532-534), and harm #1 (model text becomes a check command) is rank 1 (L442). The `--yes` gate is an invocation flag, not a human check. An agent that writes a role file and runs `new --file --yes` is admitted. Evidence: `src/agents/judge.rs:292-293`, design L332, L375, L548. Fix: in `apply`, when the change touches a contract field (`checks`, `scope`, `harness`, `output`, `expects`, `model`, `repo`, `deadline_secs`, `work`, `drafts`), refuse unless the value equals the `from` entry's or the role's current value. A changed contract value requires an interactive TTY confirmation that prints the exact `checks` text. `roles_save` applies the same rule server-side and does not trust the webview's read-only display.

## Carried to Plan
- §4/§8: `--yes` is not a human checkpoint in the VERIFY table (L548); say so, or require a TTY for proposal-derived files. Partly superseded by finding 2.
- §5 fallback `Toolset::Brain(vec![])` (acceptance 9): `src/claude.rs:130-137` sets no `--strict-mcp-config`, so connector tool schemas load into the run. "No tool calls are possible" (L607) holds only via `dontAsk`; state it or add strict-mcp to the fallback.
- §Security "What is sent" (L455, L605-608): `--agent recruiter` body and install-dir CLAUDE.md load (`src/claude.rs:113-125`, cwd `install`, `--setting-sources project`). They are not in the disclosure, so "the user sees the exact text first" is false for the system part.
- §3/§8: `apply` validates the whole `Policy` (L267-268). One Refuse-level role blocks every add, retire and restore, including retiring that role. Say so, and allow a retire of the offending role.
