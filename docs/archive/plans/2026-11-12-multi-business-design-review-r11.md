---
design: docs/plans/2026-11-12-multi-business-design.md
design-sha256: superseded
verdict: findings
---

Previous findings (r10):

1. resolved for partial edits: rejudge derives the expected workdir from the recorded shape. Evidence: design:398, design:414, design:430-432.

Whole-check audit of VERIFY(run_paths), new blocking findings:

1. B1 self-referential clearance: the check compares `workdir` to a path derived from `repo` and `worktree` of the same editable record, so one consistent edit passes and `sh -c` runs in any directory; contract commands reach the sink with no clearance. Evidence: design:383-385, design:398; `mod.rs:388,404`, `judge.rs:142,281-293`.
2. B2 ordering: rejudge and clean call `read` first (`mod.rs:499,571`), which reconciles: writes `state.json` and may kill harness groups (`mod.rs:450,469-493`), before any check. The design says "nothing run/written".
3. B3 `spec.id` not bound to the directory: code builds paths from `spec.id` (`mod.rs:480,492`; `supervise.rs:246-249`; `ledger.rs:48`), so an edited id reconciles and kills another run's harness.
4. B4 ids `.` and `""` pass the literal "one segment, no `/`, no `..`" rule; `.` resolves to `out/agents` itself (`mod.rs:260,606`).
5. B5 Violations design:414 checks clean's in-place and scratch workdirs; the clean row design:399 and code `mod.rs:578` do not.
6. B6 spawn stores `repo` relative as given (`mod.rs:388,404`); canonicalising in the app's cwd can match the wrong directory.
7. B7 a symlinked run dir or `tree` reaches `git worktree remove --force` (`mod.rs:596-604`).

Resolution (the user's decision, 2026-11-12, "Trusted, bind business"): run records are trusted as today; the record-field rules are dropped (B1, B5, B6, B7 become out of scope, stated in Taint and sinks and Accepted trade-offs). VERIFY(run_paths) binds the run to the app business before `read`: one `Component::Normal` id, a real directory that is a direct child of `out/agents`, `spec.id` equal to the id, with every later path built from the id (B2, B3, B4).

Verdict: findings. Resolved 1 of 1; new blocking 7.
