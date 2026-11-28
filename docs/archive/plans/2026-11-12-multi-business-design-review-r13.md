---
design: docs/plans/2026-11-12-multi-business-design.md
design-sha256: superseded
verdict: findings
---

Previous findings (r12):

1. resolved: one `agents::run_dir(home, id)` first in `read`, `record`, `stop`, `clean`, `rejudge`; sink row, Violations, matrix and Testing name every caller (app `lib.rs:405-441`, CLI `main.rs:622-702`, MCP `mcp.rs:298-301`). Evidence: design:374, design:405, design:420, design:435-437, design:474-482.

Non-blocking: `supervise` (`supervise.rs:34`) takes an id and is guarded through `read` (`supervise.rs:38`); `dir`, `events_path`, `write_state` (`mod.rs:260,264,611`) are public and take a raw id. The design now names `supervise` and makes the three helpers private.

New blocking findings:

1. `list` (`mod.rs:455-464`) collects `read` over every id into one `Result<Vec>`, so a single refused run (symlinked directory, stale `spec.id`) aborts the whole list: `cos agents list`, `clean` with no id (`main.rs:610,614,694-712`), the dashboard (`dash.rs:132`), `agents_list` (`lib.rs:167-170`), MCP list (`mcp.rs:217`) and `businesses()`. Violations design:420 ("clean skips that run") is unimplementable as written. Fix: `list` checks each entry and skips refused ones, reporting their ids; add a matrix row and a test.

Verdict: findings. Resolved 1 of 1; new blocking 1.
