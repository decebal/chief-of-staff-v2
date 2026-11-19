# cos desktop

The `cos` report and an agent launcher in one window. Tauri 2, over the `cos`
library. Design: [docs/plans/2026-10-21-desktop-app.md](../../docs/plans/2026-10-21-desktop-app.md).

```sh
task desktop                                   # builds cos, then runs the app
COS_BUSINESS=acme task desktop                 # pinned to acme for this launch
COS_WORKSPACE=~/Projects task desktop          # a workspace outside the clone
COS_AGENT_SLOTS=6 task desktop                 # more agents at once
```

| Tab | What |
|---|---|
| **Today** | the same report as `cos report` / `cos dash`, reloaded when a run or the workspace changes; hires waiting on you in Herdr first, with Open in Herdr |
| **Agents** | hire Claude Code or Codex agents into [Herdr](https://herdr.dev) terminals, each in its own seeded worktree; past the slot limit a hire is refused, and without Herdr the dialog offers Start Herdr. Each run's page keeps cos's verdict, checks, diff, contract and worktree, sends your message into its terminal, Pause and Mark done, and folds its last screen |

Agents outlive the window: each runs in its Herdr terminal, which owns it.
While the window is open, a thread follows every registered business's
hires (`agents::tick`), stops one at its deadline and judges each once.
`cos agents ls` in a terminal shows the same list. The preview
(`examples/preview.rs`) answers to the stand-in Herdr unless `COS_HC` names
another.

The window works on one business at a time: the active one from
`cos business list`. ⌘K switches to another business. A launch with
`COS_BUSINESS`, `COS_HOME`, `COS_WORKSPACE` or `COS_PROJECTS` set is pinned to
that triple: the header says `pinned by <var>` and switching is refused.
Agents started for one business keep running after a switch and show in that
business's list again when it is reopened.
