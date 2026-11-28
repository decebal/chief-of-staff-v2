---
design: docs/plans/2026-11-28-role-library-design.md
design-sha256: 55a0b8fb441f61c3a27d4175255115af93e80389fc256cbadb35d0137a6dec61
verdict: findings
---

## Round 1 findings
- r1-a #1: closed. §4 step 4 clears `retired_*` keys on every role and repo before the step 6 round trip; notes count the drops. Evidence: `src/agents/policy.rs:44-49`, `:100-105` (skip_serializing).
- r1-a #2: open. `roles_propose_cancel` and quit kill only the cos child's group. `claude` runs in its own group: `src/claude.rs:198` → `src/proc.rs:181`, `:210-211` (`process_group(0)`). `kill_group` (`proc.rs:300-307`) does not reach it. See finding 1.
- r1-a #3: closed. §3 L216-217: `roles::vocab(&Policy)` shared by load and write, same fallback as `policy.rs:149-151`.
- r1-a #4: closed. §5 defines `specialise_context(policy, state, business, brief)`, sources, 200-char cut, 1,000-char refusal. `state::read` returns `Result<Option<Value>>` (`src/state.rs:659`).
- r1-a #5: closed. `propose` writes `--out` only; `new` writes an approved role; no combined generate-and-write (§6 L331-336). Desktop commands are `main`-window only: `apps/desktop/capabilities/default.json:5` `"windows": ["main"]`, the boundary `ritual_approve` uses.
- r1-a #6: closed. Inventory rows added for audit log, `-p` argv, outbound preamble, library links (design L434-439). Missing `COS_CLAUDE` exec row: see finding 2.
- r1-a #7: closed. Goal titles, repo names and scopes listed as tainted sources (§Taint).
- r1-a #8: closed. Repo `checks` excluded (§5 L288-289); ISA test row asserts no check string in prompt.
- r1-a #9: closed. Caps table (§1 L95-113) sums to 8,880 + 600 mandate = 9,480 chars, "about 2,400 tokens".
- r1-a #10: closed. §1 L116-119: `fit::check` and `refusal` skip retired. Current `src/agents/fit.rs:245` filters head only, so the rule is new and needs its test (§Testing L597).
- r1-a #11: closed. §4 step 4: `apply` takes fit_kinds from the library entry named by `from`.
- r1-a #12: closed. `tests/fixtures/fake-claude-proposal.sh` named (§Testing L549). Existing `tests/fixtures/fake-claude.sh:33` still returns `{"headline":"Fake brief"}`; fixture not yet written, not a design fault.
- r1-a #13: closed. Premise corrected: `hire::Plan` has `prompt` (`src/agents/hire.rs:33-37`); preamble added in `Policy::apply` (`policy.rs:250-251`) during `record_run` (`src/agents/mod.rs:515`). Plan change itself not in §6: see carried.
- r1-a #14: closed. Acceptance 5 checks the scorecard under role id `reviewer` (`ledger.rs:41`), not the title.
- r1-a #15: closed. `roles_page.rs` added to `task test-e2e` (§Testing L607-611). `Taskfile.yml:38-47` runs only flows and kit pages today, so the edit is needed and is stated.
- r1-a #16: closed. `Policy.problems: Vec<Problem>` (§3 L207); `note` kept for whole-file failures (`policy.rs:125`). Head validity: load keeps it, `hire` refuses via head check.
- r1-a #17: closed. §4 step 7 names the `.state-*.tmp` temp file (`src/state.rs:561`).
- r1-a #18: closed. Lock copied from `state.rs:597` shape; refusal names path, pid and liveness; stale lock named, never taken over (§4 step 1).
- r1-a #19: open (nit). §ISA Bindings still `"$comment": "Role"` placeholder (design L488-489); no definition inlined.
- r1-a #20: closed. `"none"` used; `locate` kind exists in the seed (`templates/context/agents.json:158`); `judge.rs:57` renames `none`.
- r1-a #21: closed. id = role.id check moved to `tests/roles.rs` (§2 L169). Library is compiled in, so the test is the gate. Not in `validate`; accepted.
- r1-a #22: closed. One-level undo stated (§4 step 7, ISA row L432).

## Findings
1. blocking §5, §7, §ISA Side-effect inventory (process ownership): cancel and quit kill the cos child's group, but `claude` is not in it. `claude::execute` → `proc::stream_lines` → `spawn_grouped` with `process_group(0)` (`src/proc.rs:181`, `:210-211`; `src/claude.rs:198`). `kill_group` on the cos pgid (`proc.rs:300-307`) SIGKILLs cos only. `claude` keeps running with no owner and no cos deadline, bounded only by its own budget. Terminal Ctrl-C on `cos roles propose` has the same gap, because `claude` is outside the foreground group. Acceptance 7 (`pgrep -f` count 0 after Cancel) would fail. Evidence: `src/proc.rs:210-211`, `src/claude.rs:198`, design L353-360. Fix: the owner of the proposal must hold claude's own pgid. Either the desktop spawns `claude` itself (`command_with` + `proc::spawn_grouped`) and `Proposing` stores that pgid, or `cos roles propose` traps SIGINT/SIGTERM, calls `kill_group(group_of(claude))` and reaps, and Cancel sends SIGTERM before any SIGKILL. Name which, and add a grandchild check to `ipc.rs` and to acceptance 7. Note `pipeline::brief_child` (`src/pipeline.rs:706-711`) has the same shape; it is not a model for this path.
2. blocking §6 `COS_CLAUDE`, §ISA Side-effect inventory: production code would take the executable to run from an env var (`Command::new(program)`, `src/claude.rs:119`). The inventory (L429-440) has no row for it. Evidence: design L362-363 ("read the way `herdr::hc()` reads `COS_HC`"). Fix: add an inventory row (who sets it, what runs, who is affected), or read it only in test builds (`cfg(test)` or a test-only constructor) so the production exec path is always `claude` on PATH.

## Carried to Plan
- §6 table: add the plan `preamble` field that acceptance 6 needs (`hire::plan` at `src/agents/hire.rs:63` does not carry it). should.
- §6 `new --file` / `edit --file`: a Role file can carry `checks`, `scope`, `harness` not from a library entry; the CLI prints them but names no contract-specific confirm. Print the contract block under its own heading and require `--yes` on it, or reject contract drift on `--file`. should.
- §ISA sinks: proposal and role text reach the DOM (review form, rationale, Staff cards). Add `RENDER(form)`: textContent or `.value` only, per the rule at `apps/desktop/ui/app.js:1`. should.
- §Bindings: `Role` described "with deny_unknown_fields", but `policy.rs:55-56` has none and §1 says every old file loads and load never drops a role. State that `Role` stays lenient on load; deny_unknown_fields applies only to proposal and Change input. should.
- §3 / §4: `validate` runs over the whole policy, so one Refuse problem in an unrelated role blocks add, retire and restore, and the broken role cannot be retired from CLI or UI. Say so, or scope the write check to the changed role. should.
- §5 / §ISA: "the slot is cleared when the owning thread reaps it" leaves a window where the pgid is reusable after reap (`proc.rs:297-299`). Clear the slot before reap, or check start time (I4). should.
- §4 step 1, `src/agents/mod.rs:513` is `:515` (`record_run` at `:509`). App ref `roleCard` is `app.js:490`, `renderStaff` `:430`, not `L431-535`. nit.
- §ISA Bindings `"$comment": "Role"` (r1-a #19): inline the Role schema or name its definition. nit.

## Counts
- Round 1: 22 findings. 20 closed, 2 open (r1-a #2 by finding 1; r1-a #19 nit).
- New: 2 blocking (findings 1, 2). 8 carried (6 should, 2 nit).
- Verdict: findings.
