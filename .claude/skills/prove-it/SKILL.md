---
name: prove-it
description: "Adversarially check whether a cos change does what it claims: the brains stay read-only, every child process is owned and reaped, every chart number comes from facts, the remote MCP server stays free, agents stay sandboxed. Reproduces each finding with a command. Use for \"is this PR ready\", \"prove this works\", \"did this actually ship\" in this repo. Claude Code only: it runs cargo and the cos binary."
---

# prove-it (cos)

The unit of review is a **claim** ("this ships X"), not a line. For each claim
ask: *is there a path where X does not happen?* A finding needs a command
whose output is pasted, or a `file:line` read this session. A kill needs the
same. Nobody gets to assert.

## 0. Frame

```bash
git fetch origin main
BASE=$(git merge-base origin/main HEAD)
git diff -M --name-status "$BASE"..HEAD
gh pr list --search "$(git rev-parse HEAD)" --state all --json number,state,title,body
```

- `MERGED` → stop: nothing reviewed here can land.
- Claims come from the PR body's summary bullets, then commit subjects, then
  the diff. Print each with an id. Print every changed hunk that maps to no
  claim as `unclaimed`.
- "Improves reliability" is `NOT A CLAIM`; say so instead of hunting it.

## 1. The gates, and what they cannot see

Run them, paste the summary line of each:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo run --quiet --example demo -- "$(mktemp -d)/cos-demo"
```

CI runs the same four on `macos-latest`. They do not see: a live `claude` or
`codex` run, the MCP server over a real client, `cos dash` in a browser,
launchd. Those become hand-checks (§4), never assumptions.

`tests/agents.rs` and `tests/safety.rs` start real processes. Inside the
Claude Code Bash sandbox, orphans are reaped at once and worktrees may be
refused. If one fails there, re-run it outside the sandbox before calling it
a defect.

## 2. The invariants: attack each one the diff touches

| # | Invariant | Pinned by | Attack |
|---|---|---|---|
| I1 | A brain has no file, shell or write tool. `BRAIN_TOOLS` is `ToolSearch`; `READ_ONLY` names single connector tools, never a whole server. | `tests/safety.rs` | grep the diff for `Read`, `Glob`, `Grep`, `Bash`, `Write`, `Edit`, `mcp__…` without a tool suffix, `--permission-mode` other than `dontAsk` in `src/claude.rs` |
| I2 | The workspace and `context/` reach a brain inlined (`src/context.rs`), capped at `LIMIT`; nothing in `.chief-of-staff/`, `context/` or `out/` is committed; kit fixtures stay fictional. | `tests/safety.rs`, `tests/kit.rs`, `.gitignore` | `git diff --name-only $BASE..HEAD -- .chief-of-staff context out` must print nothing |
| I3 | Every child `cos` starts has one owner, one deadline (≤ `proc::CEILING`; agents ≤ `agents::CEILING`), is killed with its group and reaped. No `Command::spawn`/`output` outside `src/proc.rs` except short `ps`/`git` probes. | `tests/safety.rs`, `tests/agents.rs` | `git diff $BASE..HEAD -G 'Command::new\|\.spawn\(' -- src` — for each new spawn: who waits, what deadline, what kills the group |
| I4 | A signal to a pid or group is preceded by an identity check: `is_cos`, the recorded `pgid_started`, or `comm` for the tunnel pidfile. | `tests/agents.rs` (lost) | every new `libc::kill` — what stops it hitting a recycled pid? |
| I5 | The remote MCP server (`Mode::Remote`) offers only `REMOTE_TOOLS`, filtered in `list_tools` **and** `call_tool`; HTTP answers only `/mcp/<token>`. | `src/mcp.rs` | a new `#[tool]` added to `REMOTE_TOOLS`, or a filter removed from one of the two methods |
| I6 | `cos dash` answers only localhost `Host` headers and serves no path outside `out/reports/<date>.html` and its fixed routes. | `src/dash.rs` | a new route reading a path from the URL |
| I7 | Chart and history numbers come from `Facts`, never from model output. | `src/report.rs`, `src/history.rs` | a number in the report traced back to a `Brief`/`Review` field |
| I8 | An agent's shell has no network and cannot leave its worktree: Claude with `CLAUDE_SANDBOX` (`strictAllowlist`, no unsandboxed retry) and `--permission-prompts none`; Codex `--sandbox workspace-write`. No `bypassPermissions`, no `danger-full-access`. | `tests/harness.rs` | any flag added to `agents::harness::command` |
| I9 | `cos agents clean` never removes a worktree holding uncommitted changes or commits past its base without `--force`. | `tests/agents.rs` | a new removal path that skips `clean` |
| I10 | A connector the brain could not read is listed in `gaps`, never rendered as empty. | `.claude/agents/*.md` | prompt edits that drop the gaps rule |
| I11 | Schemas and structs move together: a new `Brief`/`Review` field is in its schema and in its wire type with `#[serde(default)]`; a brief holds ≤ 3 actions and ≤ 2 changes. | `tests/reports.rs` | a field added to one and not the other; a cap removed from schema or `hold_to_limits` |
| I12 | `src/state.rs` and the kit's `scripts/state.py` accept and refuse the same states, and each reads what the other commits. | `tests/state.rs` | a rule changed in one helper only: add the case to `parity_cases` and run it |
| I13 | `cos` writes `.chief-of-staff/state.json` only through `state::commit`, only when `localTracking.approved`, never advances a watermark, never steals `write.lock`. | `tests/plan.rs` | `git diff $BASE..HEAD -G 'atomic_write\|state.json' -- src` — every write site |
| I14 | With a profile, a brain gets only the connectors of `readsApproved` sources; the remote server offers no workspace tool (`plan`, `handoff`). | `tests/plan.rs`, `src/mcp.rs` | `context::tools` widened; a workspace tool added to `REMOTE_TOOLS` |

For a `fixes-defect` claim, also ask whether the **sibling call sites** have
the same bug (`git grep` the pattern at `HEAD`), and whether the fixed
invariant is asserted where it can be broken again.

For anything with a loop, a retry, a poll or a second agent: what happens on
the **second** item, the failing one, the concurrent run, the abort? The
reconcile race in `agents::reconcile` (stale `Running` read, then a dead
supervisor, overwriting `Done`) was exactly this shape.

## 3. Reproduce or drop

| Outcome | Result |
|---|---|
| reproduced with a pasted command | defect |
| killed with a pasted command or a `file:line` read this session | `CONSIDERED AND DROPPED`, one line |
| neither | hand-check, with its pass criterion |

## 4. Hand-checks the gates cannot do

Print only those the diff makes relevant, each as exact commands and a pass
line. None of them spends money unless it says so.

- **Brief, live** (paid, about $0.35): `cos brief --no-open`. Pass: exit 0,
  `out/audit/<day>.jsonl` shows no `Read`/`Glob`/`Grep` tool call, the brief's
  gaps name every connector that failed.
- **Agent, live** (paid, capped): in a throwaway repo,
  `cos --budget 0.25 agents spawn --model haiku --repo <repo> --prompt "run: echo hi > a.txt; then run: curl -sS https://example.com"`,
  then `cos agents show <id>`. Pass: `done`, `a.txt` exists in the worktree
  only, the curl is refused by the sandbox.
- **Kill**: spawn with a long prompt, `cos agents kill <id>`, then
  `ps -g <pgid>` prints only the header.
- **Remote MCP**: `cos serve --token $(openssl rand -hex 32)`; a wrong path is
  404, `tools/list` names only `latest_brief` and `latest_review`, a call to
  `brief` is `-32602`.
- **Dash**: `cos dash --no-open`, open `/` and `/agents` in light and dark.

## 5. Output

```
CLAIMS HOLD | CLAIM REFUTED | UNPROVEN — n hand-checks outstanding

VERIFY BY HAND        (block two, never last, never elided)
CLAIMS                C1 HOLDS / REFUTED / UNVERIFIABLE HERE / NOT A CLAIM, each with its evidence
FINDINGS              file:line · what fails · the command that shows it · fix
ATTACKS TRIED         invariant ids run, and hits
CONSIDERED AND DROPPED  always printed, even empty
```

Post the result as one PR review when a PR is open, inline at each finding's
line; otherwise print it. Never edit, commit or push from this skill. No
remark on the size of the change, no time estimate, no numeric confidence.
