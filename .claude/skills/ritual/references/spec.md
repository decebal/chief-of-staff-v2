# Spec

Write the design, have it reviewed by someone who did not write it, and get a human to approve
it. Output: `docs/plans/YYYY-MM-DD-<slug>-design.md`, its review beside it, both committed on the
flow's branch, and an approved epic.

## The branch

Cut the flow's branch now, in its own worktree, and name it on the epic. The design, its reviews,
the plan and the code all land on it and merge as one PR:

```bash
git fetch origin <trunk>
git worktree add -b <type>/<slug> ../<repo>-<slug> origin/<trunk>
cn task edit <epic> --append-description "ritual-branch: <type>/<slug>" --toon   # from the main checkout
```

Use the repo's branch rule (a ticket key, if the repo uses one). Move the Think brief from the main
checkout onto the branch, and commit the design there as you draft it: `cos` reads the committed
copy while the branch exists, and a hired reviewer's worktree is cut from it.

## Draft (the Elephant)

The author has the full context. Either draft here or ask the user to run `/egm:egm-draft` with
the brief; EGM writes `design_draft.md`, which you move to the dated path.

A design covers:

- **Decision** first, in one paragraph.
- **Context:** the system as it is, with file paths.
- **Design:** components, data flow, interfaces and their shapes, error handling.
- **Alternatives** rejected and why.
- **Security and privacy:** what is read, what is stored, what leaves the machine.
- **Testing:** which layers prove it (see the testing trophy in `plan.md`).
- **Confidence:** each piece as rock, sand or water, and what moves it up. Never a duration.
- **Acceptance:** observable checks a reviewer can run.

Write requirements as what must be true, not as an implementation. Name terms once and use them
consistently. Mark every assumption.

Name the design on the epic:
`cn task edit <epic> --append-description "ritual-design: <path>" --toon`.

## Review (the Goldfish)

The reviewer reads only the draft and the live code, never the author's conversation. A reviewer
that read the chat inherits its blind spots. Pick one:

- the user runs `/egm:egm-review <path>` (EGM starts its own isolated reviewers);
- `cos` hires `ritual-goldfish` on the design;
- a fresh subagent given only the design path and the repo root, told to check claims against the
  code.

If none of these can start a fresh context, stop: do not review in the author's context.

The review is `docs/plans/YYYY-MM-DD-<slug>-design-review.md`:

```markdown
---
design: docs/plans/YYYY-MM-DD-<slug>-design.md
design-sha256: <sha256 of the design file the reviewer read>
verdict: clean | findings
---

1. <blocking | should | nit> <section>: <problem>. Evidence: <file:line or command>. Fix: <fix>.
```

EGM's review writes `goldfish_review_report.md` without a verdict line: record its outcome in this
file, `clean` only when it raised nothing blocking or should-fix.

A hired `ritual-goldfish` works from a worktree cut at a commit, so commit the design on the flow's
branch before hiring it; its review stays in the agent's output, and you point `approve` at it.

Resolve findings in the design (or with `/egm:egm-resolve`), then review again. Each review
records the digest it read; an edit after a clean review needs a new review. Keep earlier reviews
as history and mark their `design-sha256: superseded`.

## Approve

A person approves, never the agent. Show the user the design and the clean review, and give them
the command to type in their own terminal:

```bash
cos ritual approve <epic> docs/plans/YYYY-MM-DD-<slug>-design.md --by "<name>" \
  --review docs/plans/YYYY-MM-DD-<slug>-design-review.md
```

It refuses unless the review names this design, its `design-sha256` matches the design's current
digest and its verdict is `clean`; it asks for confirmation and refuses without a terminal. Then
it appends `ritual-approved: <path> sha256:<digest> by <name>` to the epic and runs
`cn approve <epic>`. `--without-review` replaces `--review` for designs too small to review, and
says so in the line it writes.

Never run `cos ritual approve` or `cn approve` yourself; a PreToolUse hook that denies
both makes this a gate in agent sessions. Without `cos`, the user computes
`shasum -a 256 <design>`, appends the same line, and runs `cn approve <epic>`.

Then the next step is Plan ([plan.md](plan.md)).
