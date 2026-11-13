---
design: docs/plans/2026-11-12-multi-business-design.md
design-sha256: superseded
verdict: findings
---

Previous findings (r6):

1. resolved: agent-run `cos` calls listed as taint source and pass STORE(registry) checks. Evidence: design:379-383.
2. resolved: `use | remove` require name exists. Evidence: design:389.
3. resolved: `add` seeds `<home>/context/` only after entry checks, sink row present. Evidence: design:390, design:183-184.
4. resolved: `agents_clean` VERIFY(run_paths) covers id segment and worktree path under `out/agents/`. Evidence: design:395; `src/agents/mod.rs:570-606`.
5. resolved: `businesses()` row corrected to `ps` per live record; runs on picker open and Agents tab open only. Evidence: design:355, design:264, design:235-236.
6. resolved: spawn env change in inventory row and "affected" column. Evidence: design:347; `src/agents/mod.rs:422-425` (base).
7. resolved: Testing agents_clean case matches row 395. Evidence: design:465-466.
8. resolved: rejudge violation and matrix name worktree vs in-place run kinds. Scratch runs not named; see new finding 1. Evidence: design:410, design:426, design:464-465.
9. resolved: loader runs only on layer 1/2 business name or layer 3; corrupt-registry `state init` case tested. Evidence: design:145-147, design:421, design:457.
10. resolved: `home` exits 0 only when name and three directories exist. Evidence: design:199, design:396.
11. resolved: business check runs first in `task schedule`; mkdir and unload-retired move after it; plists byte-identical on failure. Evidence: design:208-211, design:356, design:396, design:428.

New blocking findings:

1. blocking §Taint and sinks / §Violations (`agents_rejudge` VERIFY(run_paths), design:394, design:410): VERIFY admits only a worktree run under `out/agents/` or an in-place run whose workdir equals `spec.repo`. A scratch run (spawned with no repo) gets `workdir = out/agents/<id>/scratch`, `worktree = false`, `repo = None`, so it matches neither and rejudge refuses every scratch run. Judge runs contract checks there (`sh -c` in workdir). Existing rejudge feature breaks for that run kind; the design is silent and the matrix (design:426) and Testing (design:464-465) have no scratch case. Evidence: `src/agents/mod.rs:388-393` (scratch workdir), `src/agents/mod.rs:498-511` (rejudge), `src/agents/judge.rs:292-293` (`current_dir(workdir)`), design:394, design:410. Fix: name the scratch kind in VERIFY(run_paths): workdir canonicalises to `<run dir>/scratch` under the app business's `out/agents/`; add it to the Violations row, one matrix case and one Testing case.

Verdict: findings. Resolved 11 of 11; new blocking 1.
