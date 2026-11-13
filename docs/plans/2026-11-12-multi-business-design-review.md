---
design: docs/plans/2026-11-12-multi-business-design.md
design-sha256: c4ee8b6472135f922e7fc4fc79854b5b8cfdb0ad8f8523e0914ad47dcefaa66b
verdict: clean
---

Previous findings (r14):

1. resolved: `run_dir` returns a public `RunDir` with `path()` and `events_path()`; the raw-id `dir` and `events_path` are removed from the public API; the cross-crate callers `lib.rs:443-444` and `main.rs:628` call `run_dir` first; `write_state` takes `&RunDir`. Evidence: design:407.

Caller coverage checked by grep over `src/`, `apps/` and `tests/`: `events_path` (`lib.rs:443`, `main.rs:628`), `dir` outside `agents` (`lib.rs:444`), `list` (`dash.rs:132`, `mcp.rs:217`, `lib.rs:170`, `main.rs:610,614,694`, `tests/fit.rs:260`, `tests/hiring.rs:415`), `write_state` (`supervise.rs:53,98,136,153,246,249`; `mod.rs:419,492,506,529,563`), raw-id `read` callers (`main.rs:622,627,645,676,682`; `lib.rs:441`; `mcp.rs:298-301`). No raw `out/agents/<id>` construction outside the helpers.

Non-blocking, for Build:

- Spawn (`mod.rs:368`, before `spec.json` exists), reconcile (`mod.rs:480`) and `supervise.rs:42,150,247` still need a run path. Keep a private path helper inside `agents`, or thread `RunDir` through prepare and finish.
- `RunDir` keeps private fields so `run_dir` is its only constructor.
- `cos agents watch` (`main.rs:614-616`) exits when every listed run is terminal; a skipped live run is not counted. Treat a skipped id as non-terminal, or document it.
- Line reference drift: clean is `mod.rs:570-606`.

New blocking findings: none.

Verdict: clean. Resolved 1 of 1; new blocking 0.
