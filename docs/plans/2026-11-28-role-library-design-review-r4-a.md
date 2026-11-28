---
design: docs/plans/2026-11-28-role-library-design.md
design-sha256: 78321b81150b14effc79d99c85160292ed3e2b0615448f07cb307101ca77ba80
verdict: findings
---

## Round 3 findings

- r3-a #1 (`--out`): closed. Design L347-352 refuses an existing path (`create_new`), any path under `COS_HOME/context/`, `COS_HOME/out/`, workspace `.chief-of-staff/`, and symlinked parents. Matrix row L616, inventory L502.
- r3-a #2 (fallback toolset): closed. Design L314-316 "There is no fallback toolset". `src/claude.rs:140-141` is the only `--strict-mcp-config` branch and it is `Toolset::None`. Brain branch (L130-137) is no longer referenced by the design.
- r3-a #3 (load contradiction): closed. Design L217-230 restates the invariant as file-level. `apps/desktop/src/lib.rs:1262` already emits `"note": p.note` to the Staff view. Design text does not say the Staff tab renders it: see Carried.
- r3-a #4 (owner-death coverage): closed as to coverage. Inventory L506, §6 L393-401, matrix L615 now name the owner-death path and a bound. The bound itself is unproven for the real binary: see finding 2.
- r2-a #1 (grandchild check, open): closed. Matrix L615 asserts both recorded pids (fake and its `sleep`) gone for Cancel, quit and SIGINT. Acceptance 7 (L723) still names only the recorded pid; narrower than the matrix, carried.
- r1-a #19 (`"$comment": "Role"` placeholder, open nit): open. Design L556-557 still has `{ "$comment": "Role" }` as the `add` and `replace.role` schema, untyped.

## Findings

1. blocking §4 `apply` (L246-247), §ISA sink table STORE(agents.json) (L592), §6 contract approval (L354-367): the sole writer cannot verify contract approval as specified. The sink row says the input is cleared by `VERIFY(contract_approval)`, then `VERIFY(policy_valid)` "in `roles::apply`". The `apply` signature has no `allow_contract` argument, and its 8-step list (L250-285) has no `contract_diff` step. Approval therefore lives in two callers (CLI and `roles_save`), which the design says must "take the same rule". Evidence: design L246-247, L250-285, L592. Fix: `apply(home, change, read_digest, allow_contract: bool)`. `apply` computes `contract_diff` itself: before = library entry named by `from` for `Add`, current role for `Replace`, empty otherwise. A non-empty diff without `allow_contract` refuses with "contract changes need --allow-contract: <fields>" before step 6. Add a matrix row that calls `apply` directly with `allow_contract: false` and a changed `checks`, and asserts the bytes are unchanged.

2. blocking §6 owner death (L393-401), inventory row `claude` (L506), matrix row `kill_and_reap` (L615): the pipe bound is asserted for the real `claude` by a fake that cannot show it. The design says the next write "gets SIGPIPE/EPIPE and ends the process". The fake is bash (`tests/fixtures/fake-claude-proposal.sh`, L619), which dies on SIGPIPE by default. `/opt/homebrew/bin/claude` is a Mach-O arm64 binary (`file`), so its SIGPIPE disposition is unverified; a runtime that ignores SIGPIPE returns EPIPE and continues unless it exits on the error, and the design does not say it does. The bound is also a model-call duration, not a time: "still capped by `--max-budget-usd 1`" caps cost, and nothing caps wall time once the owner is gone. The owner's `proc::lines` reader (`src/proc.rs:184`, `:218-228`) holds the only pipe read end, so the mechanism is real for the fake. Evidence: design L393-401, L615, L619; `file /opt/homebrew/bin/claude`. Fix: either (a) add an acceptance row: live `cos roles propose` against real `claude`, SIGKILL cos, assert `claude`'s pid gone within a stated N s, with N measured; or (b) record the group id and `proc::start_time` (`src/proc.rs:349`) under `out/` at spawn and reap on the next start after the identity check; or (c) record the unbounded-time risk as accepted with the SDK request timeout as its named bound.

## Carried to Plan

- §6 acceptance 7 (L723) names only the fake's recorded pid; matrix L615 names both pids. Align acceptance to the matrix. should.
- §3 L217-230: say that the Staff tab and `cos roles list` render `note` for a whole-file failure (`apps/desktop/src/lib.rs:1262` already emits it). should.
- §6 `--out`: state when `create_new` runs (before the live run or after) so a failed run leaves no empty file. should.
- ISA STORE(agents.json) input schema (L556-557): replace `{ "$comment": "Role" }` with the real Role schema, or point to `roles::role_schema()`. Still r1-a #19. nit.
- §6 `new --file` without a known `from`: `before` is empty, so every contract field counts as a change and needs `--allow-contract`. Say so in the Errors row or accept it as intended friction. nit.

## Counts

- Round 3: 4 blocking, closed 4 (r3-a #1-#4; #4 closed as to coverage, bound reopened as finding 2). Open items: r2-a #1 closed, r1-a #19 open (nit).
- New: 2 blocking (findings 1-2). 5 carried (should 3, nit 2).
- Verdict: findings.
