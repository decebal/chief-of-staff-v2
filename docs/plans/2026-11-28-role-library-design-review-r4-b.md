---
design: docs/plans/2026-11-28-role-library-design.md
design-sha256: 78321b81150b14effc79d99c85160292ed3e2b0615448f07cb307101ca77ba80
verdict: findings
---

## Round 3 findings
- r3-b #1: open. §6 now names the pipe (L393-401) and the test is on the fake (L615). The fake `slow` mode writes every 0.5 s, so the test cannot reach a silent stall. A real `claude` in a stalled API request writes nothing, and `--max-budget-usd 1` (`src/claude.rs:127`) caps spend, not time. See finding 1.
- r3-b #2: open. `--allow-contract` (L360-362, L617) is an invocation flag with no terminal check. An agent that writes a role file and runs `cos roles new --file <f> --yes --allow-contract` is admitted. Only the typed `contract` path needs a TTY (L361-362). ISA L592 still counts `--yes` as an approval "in an invocation that generated nothing". Fix: CLI contract changes need a typed confirmation on a TTY and no flag; `roles_save`'s `allow_contract` stays set only by the dialog's confirm step.

## Findings
1. blocking §6 "When the owner dies without running any code" (L393-401); ISA "Spawn claude" row (L506) and test row (L615): the stated bound is the next write to the closed pipe. That is not a bound for a silent `claude`. A request stalled mid-generation emits no line, so nothing fails until the request ends or times out, and the API timeout is not in the design. `--max-budget-usd` caps spend, not time. Evidence: design L397-399; `src/claude.rs:127` (budget only); `src/claude.rs:140-141` (`--tools ""`, `--strict-mcp-config`, no time limit); design L615 (fake writes every 0.5 s, so the stall case is never run). Real-claude handling of EPIPE is also unverified. Fix: state a wall-clock bound that does not depend on the owner, either a request-timeout setting for the child (confirm its name and effect on the installed `claude`) or a watchdog that kills the group when the owner pid exits. Name the value in §6 and in L506. Add a SIGKILL test against a fake mode that writes nothing for longer than the bound, asserting the pid is gone within it. Run the same check on real `claude` in acceptance 9.

## Carried to Plan
- §Decision L33 says the desktop "runs `propose` as a child it owns"; §7 L422 says it "spawns no `cos` child". Reword L33 to the in-process thread that owns `claude`.
- §6 `new --blank` (L343) uses an empty `before` (L357), so `expects: diff` makes `--allow-contract` mandatory for blank adds. The table does not say so; state it, and have the dialog's Blank path set it.
- §4 step 4 (L272-274) adds `fit_kinds` to the file's `fit.kinds` without naming them in the approval or in `notes`. Name them in `notes`.
- ISA VERIFY(user_approval) (L592) and Decision: `--yes` is agent-passable for descriptive fields (harm 4, steering a hire). Say it is not a human checkpoint, or require a TTY for file-derived roles (r3-b carry, not addressed).
- §4 step 5 (L277): a Refuse problem on one role blocks `retire` of that role. Carried from r3-b, not addressed.
- §Security L654 "The user sees the exact text first" is still false for the `--agent recruiter` body and the install-dir CLAUDE.md (r2-b carry, L455, L605-608 unchanged).
- Acceptance 9 (L726-729) should include the stalled-owner kill check on real `claude`.

Counts: blocking new 1 (finding 1). r3 blocking open 2 (#1, #2). Verdict: findings.
