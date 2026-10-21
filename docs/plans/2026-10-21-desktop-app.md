# Desktop app, workspace split, agent governor

2026-10-21. Status: implemented on `feat/desktop-app`.

## Problems this fixes

| # | Problem | Evidence |
|---|---|---|
| 1 | `cos` read `.chief-of-staff/` from `COS_HOME` only, and `COS_HOME` defaults to the clone (`CARGO_MANIFEST_DIR`, fixed at compile time). A workspace set up elsewhere with the kit's `state.py --workspace` was invisible to `cos`. | Setup wrote goal G-1 to `~/Projects/.chief-of-staff/state.json` (revision 1); `cos state read` returned the clone's revision 0. |
| 4 | `agents::spawn` started every agent at once and cut each worktree bare. The harness sandbox refuses network, so an agent could not install dependencies, and N agents meant N cold builds on the same cores. | `spawn` had no cap; `harness.rs` sets `network.strictAllowlist`. |

Not addressed here: the projects scan stays one level deep for `.chronis/`
(14 of 42 repos under `~/Projects` have one), and `gh search prs --author=@me`
still covers the whole GitHub account rather than the repos on disk.

## 1. Workspace split

`--workspace` / `COS_WORKSPACE` names the directory holding `.chief-of-staff/`,
the same meaning as the kit's `state.py --workspace`. It defaults to `--home`,
so existing setups behave as before.

| Follows `home` | Follows `workspace` |
|---|---|
| `context/`, `out/` (briefs, reviews, facts, history, reports, agents), agent definitions, the audit log | `profile.json`, `state.json`, run receipts, approved sources, the connector allowlist, the timezone, handoffs, `cos state …` |

`pipeline::Config` carries both; `context::block`, `report::{build,data}` and
`dash::serve` take both. `tests/workspace.rs` proves the brains, the report's
plan view and the dashboard's change detection all follow the workspace.

## 2. Governor

- **Run slots.** `COS_AGENT_SLOTS`, default `cores / 4` (3 on 12 cores). A
  supervisor takes `out/agents/.slots/run-<i>.lock` with `flock` and holds it
  for its life; past the limit an agent waits as `queued`. The kernel frees a
  slot when its holder exits, so a crashed supervisor cannot leak one. A
  queued agent honours `stop`. Queue time does not count against the deadline.
- **Build slot.** The supervisor writes a `cargo` shim into
  `out/agents/<id>/bin/` and puts it first on the harness's PATH. The shim runs
  `cos agents build-slot`, which takes one machine-wide `flock`
  (`$TMPDIR/cos-build.lock`, opened read-only so a sandboxed agent can take it),
  clears `FD_CLOEXEC` and execs the real `cargo`. The lock is held until cargo
  and every compiler it started have exited. Only agents coordinate through it;
  a build you start by hand does not.

## 3. Seeding

Before the harness starts, outside the sandbox, the supervisor seeds a worktree
agent (`Status::Seeding`), logs to `out/agents/<id>/seed.log`, and verifies each
step on disk. Any failure fails the agent before the harness runs.

1. An executable `.cos/seed` committed in the repo runs alone, in the worktree,
   with `COS_SEED_REPO` set to the main checkout. Use it for anything specific:
   a prebuilt `dist/`, fetched binaries, an env file.
2. Otherwise: `Cargo.toml` → clonefile `target/` from the checkout
   (`cp -Rc`; `cp --reflink=auto` off macOS).
3. `package.json` → clonefile `node_modules/` if the checkout has it, else a
   frozen lockfile install (bun, pnpm, npm, yarn), which needs `node_modules/`
   to exist afterwards.

Each step runs under the 300 s ceiling.

## 4. Desktop app (`apps/desktop`)

Tauri 2.12 (stable; 3.0 is alpha), a workspace member depending on the `cos`
lib. `task desktop` builds `cos` first, then runs the app.

- **Ownership.** The app launches and watches; it owns no agent. Agents are
  setsid'd `cos agents supervise` processes. Closing the window stops nothing;
  reopening reconciles from `out/agents/*/state.json`.
- **The supervisor binary.** `COS_BIN`, else a `cos` beside the app executable
  (the shared workspace `target/`), else the home's release or debug build,
  else `~/.cargo/bin/cos`.
- **Report.** Served on a custom `cosreport://` scheme into
  `<iframe sandbox="allow-scripts">`: its own opaque origin, no access to the
  app's commands, and its inline scripts run without loosening the main
  window's `script-src 'self'`. A brief quotes mail and tickets; nothing in it
  can reach `agents_spawn`.
- **Live updates.** A watcher thread compares metadata stamps every 700 ms and
  emits `report-changed` (runs, workspace) and `agents-changed` (agent state
  writes). It spawns nothing per tick; the UI coalesces refreshes so only one
  `agents_list` is in flight.
- **Agents view.** A spawn form (Claude Code or Codex, repo under
  `~/Projects`, own seeded worktree or in place, title, prompt, copies 1–32,
  deadline, budget, model), one-click spawn from the latest brief's actions or
  review's goals, a table with status / tool calls / cost / latest line, and a
  detail pane with the activity tail, seed log, resume command, Stop and Clean.
  Clean refuses a worktree holding work unless you confirm.
- **Workspace picker.** Header link; accepts a directory whose
  `.chief-of-staff/profile.json` exists; saved in the app config directory.
  `COS_HOME`, `COS_WORKSPACE`, `COS_PROJECTS`, `COS_AGENT_SLOTS` win over it.

## Not done

- **A bundled `.app`.** Needs `tauri-cli` (`cargo install tauri-cli --locked`),
  then `cargo tauri build` in `apps/desktop`. Shipping `cos` inside the bundle
  is `bundle.externalBin` with a `cos-<target-triple>` copy; until then the app
  finds `cos` as above.
- **The real-sandbox shim run.** The tests drive a stand-in harness. Whether
  Claude Code's Seatbelt profile lets a sandboxed Bash exec the shim and
  `flock` the read-only lock has not been run against a real `claude`.
- **Non-cargo builds** (`bun run build`, `tsc`) do not take the build slot.
