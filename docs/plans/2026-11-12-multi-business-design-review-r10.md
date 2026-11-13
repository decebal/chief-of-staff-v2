---
design: docs/plans/2026-11-12-multi-business-design.md
design-sha256: superseded
verdict: findings
---

Previous findings (r9):

1. resolved: clean keys its worktree check on the spawn-created `<run dir>/tree`; any other recorded workdir is refused whether or not it exists; a passing run whose tree is gone deletes its run directory. Evidence: design:398, design:432-433, design:473-474; `src/agents/mod.rs:578,606`.

New blocking findings:

1. blocking §Taint and sinks (`agents_rejudge`, design:397): the row admits any of the three kinds for any run. A worktree run records `spec.repo = Some(repo)` (`mod.rs:403-404`), so a worktree run whose workdir was edited to that repo passes and runs `sh -c` in the user's repo, and a worktree run edited to `scratch/` passes too. Matrix design:429 and Violations design:413 say such runs are refused. Fix: tie each kind to the run's recorded shape (worktree → `tree/` only; no repo → `scratch/` only; in-place → `spec.repo` only) and refuse any other workdir, existing or missing.

Verdict: findings. Resolved 1 of 1; new blocking 1.
