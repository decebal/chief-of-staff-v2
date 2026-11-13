---
design: docs/plans/2026-11-12-multi-business-design.md
design-sha256: superseded
verdict: findings
---

Previous findings (r8):

1. resolved: VERIFY(run_paths) canonicalises both sides; a path that does not canonicalise is refused; symlinked-home case in matrix and Testing. Evidence: design:396-397, design:430, design:470-471; `src/agents/mod.rs:364,388,406`.
2. resolved: clean on a worktree run whose tree is gone deletes the run directory, as today. Evidence: design:397, design:431, design:472-473; `src/agents/mod.rs:578,606`.

New blocking findings:

1. blocking §Taint and sinks vs §ISA test matrix (design:397, design:432): the clean row keys the check on whether the tree still exists, so a worktree run whose `workdir` was edited to a nonexistent path outside `<home>/out/agents/` counts as "tree gone" and its run directory is deleted, while matrix row 432 says that run is skipped with nothing removed. A test cannot satisfy both. Nothing outside `<id>/` is removed either way, so the fault is the spec text. Fix: key the check on the spawn-created `<run dir>/tree`; any recorded workdir that is not that path is refused regardless of existence, and only a passing run whose tree is gone skips `git worktree remove`.

Verdict: findings. Resolved 2 of 2; new blocking 1.
