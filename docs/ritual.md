# Ritual flows

The ritual is one flow from an idea to a shipped change: Think, Spec, Plan, Build, Review, Ship,
Reflect, with a file handed between each step and a Chronis epic as the flow's anchor. The skill
that runs it is `.claude/skills/ritual/`; copy it to `~/.claude/skills/ritual` to use it in every
repo. `cos` is the harness: it reads every flow on the machine, records your approval of a design
when you ask it to, and offers roles to hire for the steps a separate process can do.

## Where a flow lives

- **The epic** lives in the repo's main checkout. Chronis data is untracked, so every linked
  worktree has its own empty store that dies with it. A repo without `.chronis/` runs `cn init`
  first.
- **The files** live on the flow's branch, `<type>/<slug>`, cut from the trunk at the start of Spec
  in its own worktree and named on the epic as `ritual-branch:`. The design and its reviews are
  committed there, Build continues there, and they merge with the feature's one PR.
- **Reading the design:** while the branch exists, `cos` reads the design as committed on it
  (`git show <branch>:<path>`); after it merges and is deleted, from the main checkout's working
  tree.

## The seven steps

| Step | Hands over | Advances when |
|---|---|---|
| Think | `docs/plans/YYYY-MM-DD-<slug>-brief.md` in the main checkout; the epic | the brief is in the repo's docs index |
| Spec | `…-design.md`, `…-design-review.md`, committed on the branch | you run `cos ritual approve` |
| Plan | `…-plan.md` on the branch (PRD, gates, testing trophy); beads under the epic | `cos ritual check` passes before the first bead |
| Build | commits on the branch | every bead done; tests ran |
| Review | `…-code-review.md`, or a review on the PR | a clean review newer than the last change |
| Ship | one commit, the PR, evidence | merged; `ritual-shipped:` written |
| Reflect | `docs/reports/YYYY-MM-DD-<slug>.md` | epic closed |

The PRD sits in its own file so that planning never changes the approved design's bytes. A bug fix
skips Think and Spec. A parentless `bug` with children is a flow; one with none is a task.

## Markers on the epic

One line per handed-over file, appended with `cn task edit <epic> --append-description`. The newest
line of each key wins.

| Marker | Written at | Value |
|---|---|---|
| `ritual-brief:` | Think | brief path |
| `ritual-branch:` | Spec | branch name |
| `ritual-design:` | Spec | design path |
| `ritual-approved:` | Spec, by `cos ritual approve` | `<path> sha256:<hex> by <name>`, optionally `without review` |
| `ritual-plan:` | Plan | plan path |
| `ritual-review:` | Review | `<path or PR URL> clean` or `<path or PR URL> findings` |
| `ritual-pr:` | Ship | PR URL |
| `ritual-shipped:` | Ship, after the merge | merge commit |
| `ritual-report:` | Reflect | report path |

A flow with no marker is a legacy flow: an epic from before the ritual. A flow is "designed" when it
carries `ritual-brief:` or `ritual-design:` and is not a bug.

## The review file

```markdown
---
design: docs/plans/YYYY-MM-DD-<slug>-design.md
design-sha256: <sha256 of the design file the reviewer read>
verdict: clean | findings
---

1. <blocking | should | nit> <section>: <problem>. Evidence: <file:line or command>. Fix: <fix>.
```

A later review supersedes an earlier one: keep the earlier file with `design-sha256: superseded`.

## Approval

`cos ritual approve <epic> <design> --by <name> (--review <path> | --without-review) [--repo <dir>]`:

1. Refuses a design other than the one the epic's `ritual-design:` names, and refuses unless the
   review's front matter names this design, its `design-sha256` equals the design's sha256, and its
   `verdict` is `clean`.
2. Shows the line it will write and asks for `yes` on the terminal; refuses when stdin is not a
   terminal.
3. Appends `ritual-approved: <path> sha256:<hex> by <name>`, then runs `cn approve <epic>`.

`cos ritual check <epic> [--repo <dir>]` exits 0 only when the epic is approved and the newest
`ritual-approved:` digest matches the design as the flow holds it. Otherwise it prints
`unapproved`, `changed since approval`, or `missing design`, and exits 1. `--repo` defaults to the
main checkout of the current directory, so both commands work from the flow's worktree.

### What approval does and does not stop

The terminal prompt stops accidents: a piped or scripted call fails. It does not authenticate
anyone: `--by` is a name, a pseudo-terminal wrapper gets past the check, and an agent with shell
access can write the same line and run `cn approve` itself, which `check` cannot tell apart. The
skill tells agents never to approve. A Claude Code PreToolUse hook that denies `cos ritual approve`
and `cn approve` in agent Bash calls turns that into a gate for direct and chained calls; a call
hidden behind a variable, `eval` or a script still gets through.

`approve` is a CLI command only, never an MCP tool. It is the one place `cos` writes to a task
store, and only on your typed command; the starter kit's read-only rule holds everywhere else.

## Reading flows

`cos flows [--json]`, `/flows` in `cos dash`, and the desktop Flows tab read the same model:

- **Gate:** with a workspace profile, only when it approves the `tasks` source, the gate the brief's
  `cn` facts use. With no profile at all, every local read is allowed, as for the brief.
- **Where:** each main checkout directly under `--projects` with `.chronis/`. A directory whose
  `.git` is a file is a linked worktree and is skipped.
- **How:** `cn list --format json --all` per repo through `proc::run`, under one 90 s deadline for
  the whole scan; each call gets the smaller of 20 s and what is left. Repos left when it runs out
  are reported as gaps, and so is an approved flow the drift check had no time for. The `cn`
  binary is `--cn` / `COS_CN`.
- **Serving:** `cos dash` and the desktop app serve one scan at a time and reuse a page younger
  than 10 s.
- **What is kept:** id, title, type, status, parent, `blocked_by`, approval and the `ritual-*`
  markers. Descriptions are parsed for markers and dropped. Nothing is exposed over MCP.

### Inferring the step

First match wins:

| # | Step | When |
|---|---|---|
| 1 | done | epic done, and a report is named or the flow is legacy |
| 2 | reflect | epic done, or `ritual-shipped:` named |
| 3 | drift | approved, and the design's sha256 differs from the `ritual-approved:` digest |
| 4 | think | designed, not approved, no design named |
| 5 | spec | designed, not approved, design named |
| 6 | plan | no beads |
| 7 | build | any bead not done |
| 8 | review | not legacy, and no review, or the newest `ritual-review:` says findings |
| 9 | ship | every bead done |

A legacy flow skips rows 4, 5 and 8, so existing epics read as plan, build, ship or done. A bug flow
skips rows 4 and 5.

Each flow's card opens to its beads drawn as layers by longest `blocked_by` chain, coloured by
status, the ready set outlined; a cycle is broken where it is found, so a bad graph still draws.

## Roles to hire

Two roles in `templates/context/agents.json`. Hire them with `--repo` pointing at the flow's
worktree, so the agent's worktree is cut at the flow branch with the design committed.

| Role | Work kind | Output | Checks |
|---|---|---|---|
| `ritual-shaper` | `design` | `DESIGN.md`; you move it to the dated path | `test -s DESIGN.md`, an `## Acceptance` heading |
| `ritual-goldfish` | `spec-review` | `REVIEW.md` with the review front matter | front matter opens the file, a `verdict: clean\|findings` line, a 64-hex `design-sha256:` line |

`builder`, `reviewer`, `release-clerk` and `docs-keeper` cover Build, Review, Ship and Reflect. A
hired role is a separate process, so a hired Goldfish has no author context.
