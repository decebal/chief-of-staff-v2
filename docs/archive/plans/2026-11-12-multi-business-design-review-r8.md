---
design: docs/plans/2026-11-12-multi-business-design.md
design-sha256: superseded
verdict: findings
---

Previous findings (r7):

1. resolved: `agents_rejudge` VERIFY(run_paths) admits worktree `tree/`, scratch `scratch/` and in-place `spec.repo`; Violations, matrix and Testing match. Evidence: design:395, design:411, design:428, design:466-468; `src/agents/mod.rs:371-394`, `src/agents/judge.rs:292-293`.

Non-blocking: inventory row design:350 said "runs ... in its worktree"; it now names all three workdir kinds.

New blocking findings:

1. blocking §Taint and sinks (VERIFY(run_paths), design:395-396): the workdir is canonicalised but the expected paths are not. Spawn stores raw paths (`mod.rs:364` `std::path::absolute(home)`, `mod.rs:260-262` raw join, `mod.rs:388` `repo.clone()`), so a home or repo behind a symlink (macOS `/tmp` -> `/private/tmp`, explicit `COS_HOME`/`--home`) makes valid runs fail the comparison: rejudge refuses, clean skips. Fix: canonicalise both sides; add a symlinked-home case to the matrix and Testing.
2. blocking §Taint and sinks (`agents_clean`, design:396): canonicalising a worktree path that no longer exists fails, so a run whose tree was removed by hand or by `git worktree prune` can no longer be cleaned. Today's code tolerates it (`mod.rs:578` `if spec.worktree && spec.workdir.exists()`, then `mod.rs:606` `remove_dir_all`). Fix: when the tree is gone, skip `git worktree remove` and delete the run directory after the id-segment and root checks; add a matrix and Testing case.

Verdict: findings. Resolved 1 of 1; new blocking 2.
