## What this proposes

cos would get a user-level registry, `~/.config/cos/businesses.toml`, that names several businesses. Each business is a triple of directories (home, workspace, projects), and the registry marks one business as active. A single resolver gives every command its triple from exactly one layer: flags first, then env, then the registry's active business, then built-in defaults. Two sources in the same layer are an error. New `cos business list|add|use|remove` commands manage the registry, and a first-run migration writes a `default` entry. The desktop app keeps its own business in memory and never persists it. A ⌘K picker, wired as a menu accelerator, switches the app's business, then reloads the window and restores the same tab and pane through a URL hash. A badge shows agents running or lost in other businesses. Long-lived processes bind the active business when they start, and launchd plists take an optional `--business`.

## How the existing system works, per this document

Today each directory is resolved on its own from a flag, then a `COS_*` env var, then a default; the app also reads `settings.json`. So a partial override can mix one business's goals with another's repos. The app has one switch, `set_workspace`, which changes only the workspace. It holds `Mutex<Settings>` and saves effective values back to disk, env values included. Agent runs live under `<home>/out/agents/<id>`, each owned by a detached supervisor. `reconcile` both classifies a run (live, lost or starting) and repairs it, killing a dead supervisor's harness and rewriting its record. The launchd plists bake `COS_HOME` in at `task schedule` time. Several functions take a parameter named `home` but actually receive the workspace.

## Findings

1. **Severity:** Major
   **Affected Scope:** Terms, "Run record" (line 29-31)
   **Discrepancy:** The terminal statuses listed are `done, failed, stopped, lost`. The cited code (`src/agents/mod.rs:110-124, 137-147`) has `done, failed, killed, timed_out, lost`. There is no `stopped`, and `killed` and `timed_out` are missing. A reader building the badge or `classify` from this definition gets the vocabulary wrong.

2. **Severity:** Major
   **Affected Scope:** Long-lived processes and launchd (line 174-178)
   **Discrepancy:** The text says "Logs go to the resolved home's `out/logs`, which the job creates." The plists set `StandardOutPath` and `StandardErrorPath` to `@COS_HOME@/out/logs/...` (`launchd/cos.brief.plist:24-25`, `cos.dash.plist:20-21`), and launchd fixes those paths at load time. The doc says the template drops `COS_HOME` but not what replaces `@COS_HOME@` in these keys. That matters most when no `BUSINESS` is given and the job is meant to follow the active business. Two things are unexplained:
   - how logs can follow a business chosen at run time;
   - what "the job creates" means: a cos code path, or launchd itself.

3. **Severity:** Major
   **Affected Scope:** Departures from the brief (line 55-63)
   **Discrepancy:** Each bullet states the new behaviour but not what the brief asked for. "Must name existing directories" implies the brief had `add` create them, but the reader has to guess that. "The badge counts `running` only" appears before "badge" is introduced at line 211. Sign-off needs the before and after side by side; as written, the reader must open the brief to know what is departing from what.

4. **Severity:** Major
   **Affected Scope:** Desktop, the pin rule (line 189-190), and Risks, "Env pin"
   **Discrepancy:** "If any `COS_*` var set the triple at start" leaves two questions open:
   - Does `COS_BUSINESS` alone pin? It is part of the env layer, so presumably yes, but the header text `pinned by <var>` is only shown for `COS_HOME`.
   - `COS_AGENT_SLOTS` is also a `COS_*` var, and nothing explicitly excludes it. The exact pinning set should be listed.

5. **Severity:** Major
   **Affected Scope:** First-run migration (line 149-153), compared with Acceptance 1 and Testing (line 362-364)
   **Discrepancy:** The CLI migrates "from resolver layers 1, 2 or 4". So `cos --home /tmp business list`, or a shell with `COS_HOME` exported, writes `default` from that flag or env value, not from the build clone. Acceptance 1 and the tests describe only the build-clone case. It is unclear whether recording a one-off flag as the permanent `default` is intended. The app path is also unclear: it uses `settings.json` even when `COS_*` pins it. Is that intended?

6. **Severity:** Major
   **Affected Scope:** Desktop badge (line 211) compared with Acceptance 5 (line 404)
   **Discrepancy:** The format is given as `N running, M lost in other businesses`, but the acceptance text is `1 running in other businesses`. Two rules are never stated: whether zero parts are omitted, and whether the badge is hidden when both counts are zero. Departure 3 also says lost runs are "listed separately", which does not match a single combined line.

7. **Severity:** Nit
   **Affected Scope:** Context and Desktop
   **Discrepancy:** Project terms are used without definition:
   - "slots" and `cos` in `Settings`
   - `COS_AGENT_SLOTS`
   - `agents build-slot` ("runs on every compiler call": what is a slot?)
   - "harness" and "harness group"
   - `is_cos`, `START_GRACE`
   - `spec` in `classify(spec, record)`
   - `layer` in `Triple`, whose type is never given
   - "ritual flows"
   - the "ledger" in the inventory

8. **Severity:** Nit
   **Affected Scope:** Non-goals (line 43-44)
   **Discrepancy:** `~/.claude/agents/tactician.md` "keeps the install home written at `task install`" is unexplained. What the file is, and why it matters here, is not said.

9. **Severity:** Nit
   **Affected Scope:** Context, the last bullet, and Renames (line 91-94, 219-223)
   **Discrepancy:** "Correct today; renamed first so a business threaded through later lands on the right directory" is vague. It does not say which later change would misroute if the rename were skipped.

10. **Severity:** Nit
    **Affected Scope:** Desktop, the reload bullet (line 206-209)
    **Discrepancy:** "Rust-side caches are keyed by path" names only one cache, the flows page cache. The claim covers all of them, but the doc gives no inventory showing no others exist. The bullet also does not say what `pane` is for tabs that have no panes.

11. **Severity:** Nit
    **Affected Scope:** Alternatives considered (line 238-240)
    **Discrepancy:** The round-1 and round-2 rejections are too terse to follow without the review files. One example is "broke the existing plists and inherited env in supervisors".

12. **Severity:** Nit
    **Affected Scope:** Semantic ISA, Instructions (line 303-310)
    **Discrepancy:** The vocabulary paragraph defines core, sink and VERIFY, but not the opcodes `LOAD`, `STORE` and `TOOL_CALL`.

13. **Severity:** Nit
    **Affected Scope:** Acceptance 4 (line 400)
    **Discrepancy:** "with focus inside nothing" is unclear. Presumably it means focus on the app body, outside any frame.

Findings: 0 Blocker, 6 Major, 7 Nit
