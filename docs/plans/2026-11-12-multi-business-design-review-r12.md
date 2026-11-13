---
design: docs/plans/2026-11-12-multi-business-design.md
design-sha256: superseded
verdict: findings
---

Previous findings (r11), under the user's decision that run records are trusted:

1. B2 resolved: VERIFY(run_paths) runs before `read`. Evidence: design:404; `mod.rs:446-450,499,537,571`.
2. B3 resolved for rejudge, stop, clean: `spec.id` equals the directory id; later paths use the id. Evidence: design:404, design:436; `mod.rs:480,492`, `supervise.rs:246-249`, `ledger.rs:48`.
3. B4 resolved for those three: one `Component::Normal`. Evidence: design:404, design:435; `mod.rs:256-262`.
4. B1, B5, B6, B7 declared out of scope; no row still claims a record-field check. Evidence: design:391-395, design:521-524.

New blocking findings:

1. The run-id guard sits in three app handlers, but the same unchecked id reaches `agents::mod` from other entry points: app `agents_record` (`lib.rs:405-420`) and `agents_tail` (`lib.rs:438-441`), the CLI agent subcommands (`main.rs:622-702`, including `clean` with ids from `list`, `main.rs:694-698`), and MCP `stop_agent` (`mcp.rs:298-301`). `../x` escapes `out/agents` through `dir()` (`mod.rs:256-262`), so reconcile can write `state.json` or kill a harness outside it. Fix: one guard in `agents::mod`, first in `read`, `record`, `stop`, `clean` and `rejudge`; extend the sink row, Violations, matrix and Testing to every caller; state that CLI and MCP bind to the resolved triple's home.

Resolution: the design names the invariant: every run id becomes a path through one function, `agents::run_dir(home, id)`, called first in each of the five `agents::mod` functions, so no caller can skip it. Sink row, Violations, matrix and Testing now cover the app, CLI and MCP callers.

Verdict: findings. Resolved 4 of 4; new blocking 1.
