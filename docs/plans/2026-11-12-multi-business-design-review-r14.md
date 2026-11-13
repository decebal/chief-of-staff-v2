---
design: docs/plans/2026-11-12-multi-business-design.md
design-sha256: superseded
verdict: findings
---

Previous findings (r13):

1. resolved: `list` applies `run_dir` per entry, skips refused ones and returns the skipped ids. Evidence: design:406, design:421, design:439, design:484-487.

Non-blocking (folded into design:406 in the same change):

- `businesses()` uses `agents::count`, not `list` (design:240-244); dropped from the list-caller enumeration.
- `list`'s return type unspecified; `tests/fit.rs:260` and `tests/hiring.rs:415` call `.is_empty()` on it. Now `Listing { runs, skipped }`; the tests move to `runs`.
- Where skipped ids surface per caller: now named (CLI prints on stderr; dashboard, app and MCP drop them).
- `list` skipped only `run_dir` refusals; a `read` error on a passing directory (`mod.rs:448,492`) still aborted. Now any per-entry error skips.
- An unparseable `spec.json` is refused: now stated.

New blocking findings:

1. `dir` and `events_path` "become private" (design:406) cannot compile: the app (`lib.rs:443-444`) and the CLI (`main.rs:628`) are separate crates that call them. Fix: name the public replacement. Now `run_dir` returns a public `RunDir` with `path()` and `events_path()`; the raw-id helpers are removed and those three call sites call `run_dir` first.

Verdict: findings. Resolved 1 of 1; new blocking 1.
