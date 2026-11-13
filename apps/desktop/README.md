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
| **Today** | the same report as `cos report` / `cos dash`, reloaded when a run or the workspace changes |
| **Agents** | spawn any number of Claude Code or Codex agents, each in its own seeded worktree; past the slot limit they wait as `queued`. Status, tool calls, cost, the live activity tail, stop, clean |

Agents outlive the window: each runs under its own `cos agents supervise`
process. `cos agents ls` in a terminal shows the same list.

The window works on one business at a time: the active one from
`cos business list`. ⌘K switches to another business. A launch with
`COS_BUSINESS`, `COS_HOME`, `COS_WORKSPACE` or `COS_PROJECTS` set is pinned to
that triple: the header says `pinned by <var>` and switching is refused.
Agents started for one business keep running after a switch and show in that
business's list again when it is reopened.
