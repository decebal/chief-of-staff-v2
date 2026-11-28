---
design: docs/plans/2026-11-28-hire-skill-graph-design.md
design-sha256: superseded
verdict: clean
---

## Round 1 findings

1. closed: a malformed or non-array `$CODEX_HOME/config.toml` now fails the hire before Herdr. `codex_config` returns `Result` and bails on read error, parse error (`{path}: {toml msg}`) and non-array `skills.config` (`src/agents/skill_bundle.rs:44-55`). `prepare` propagates it with `.context("keep the user's Codex skills.config")` (`src/agents/mod.rs:730-731`). `launch` routes the error to `follow::end(Failed)` and bails at `mod.rs:642-646`, which precedes `harness::argv` and `herdr::run` (`mod.rs:652`, `src/herdr.rs:216-226`). A missing file still yields no entries (`skill_bundle.rs:57`). Test: `tests/hiring.rs:912-919` runs `skills = 3\n[[` and `[skills]\nconfig = "nope"`, asserts the error contains `config.toml`, and asserts the `run` call count is unchanged.

2. closed for `skills_hidden`; E11 retention open (minor). The Built-as E8 (extended) row names `skills_hidden` and `skills_codex_config` as written through `write_state` (design line 62). Code writes both (`mod.rs:724-730`, persisted at `mod.rs:744`). E11's location claim is accurate: the Codex value is an argv element after `--` (`src/herdr.rs:226`). Whether Herdr retains argv is not verified (no Herdr read, no live call); the design's "like any hire's" is a baseline claim, not a checked fact.

- carried a (`tests/skills_cli.rs` `CODEX_HOME` pin): open. `cos()` sets `COS_PRIME_URL` (lines 59, 196, 223) but no `CODEX_HOME`.
- carried b (`tests/fake_herdr/mod.rs:52` pins `http://127.0.0.1:9/mcp`): open. Loopback passes VERIFY-U, so a connect is attempted; design Built-as item 6 still says "refused".
- carried c (§2 "per process"): closed. Design lines 221-223 now read "One call in flight per home", matching Built-as item 2. The review-history list (line 609) keeps the old wording as a carried note, which is accurate for history.
- carried d (`tests/hiring.rs` weak assertion): closed. The user entry is `enabled = true` (`hiring.rs:878`), its value is asserted as kept (`:903-907`), and the hidden entry follows it (`:908`).
- carried e (`$HOME/.codex` fallback, read at launch not plan): open. `codex_config` runs in `prepare` at launch (`mod.rs:730`). The value is recorded in `state.json` (`skills_codex_config`), so the read time is recoverable only from the file's mtime. The design does not say when the config is read.

## New findings

None at `blocking`. Checked and clean: hidden names pass `valid_name` before the TOML build (`skill_bundle.rs:59`); repo-held skills are filtered out of both hidden lists (`skill_bundle.rs:70-77`, `:85-92`); the Codex value is built with `toml::Value`, not string joins (`skill_bundle.rs:65`); `prepare` failure leaves no Herdr terminal (see r1 finding 1).

## Carried to Plan

- Security and privacy: `state.json` now holds the user's own Codex `skills.config` entries (`skills_codex_config`, `mod.rs:730`), which can carry `path` strings. Add it to the "Stored" list (design line 496) beside `spec.json`.
- E11: either verify that Herdr does not retain a terminal's argv, or change "Herdr keeps a terminal's command line like any hire's" to "Herdr may keep it" and record that it was not checked.
- Built-as item 6: reword "a refused `COS_PRIME_URL`", or pin a URL VERIFY-U rejects in `tests/fake_herdr/mod.rs:52`.
- `tests/skills_cli.rs` `cos()`: pin `CODEX_HOME` like the other helpers.
- Design Decision or Built-as: state when the Codex config is read (launch, in `prepare`), and that a config edited between plan and launch is picked up.
- `prepare` error path: `follow::end` persists `skills_hidden` without `skills_codex_config` on a config failure (`mod.rs:642-646`). Harmless (the hire is Failed), but say so if the state file is ever read for resume.
