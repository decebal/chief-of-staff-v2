# MCP client bridge: brief

Date: 2026-11-25.

## Problem

The desktop app, Claude Desktop and Codex each run their own `cos`, and they
meet only in files on disk. On 2026-11-25:

- Neither client had `cos` registered: `.mcpServers.cos` is absent from
  `claude_desktop_config.json`, and `~/.codex/config.toml` has no
  `[mcp_servers.cos]`. Registration lives in `Taskfile.yml:63-90` (`jq`,
  `codex mcp add`), and nothing in the app shows or repairs it.
- `cos mcp` resolves its business once, at spawn (`src/main.rs:1166`). The
  app's `switch_business` (`apps/desktop/src/lib.rs:364`) changes only the
  window and writes nothing, so the two can disagree with no sign of it.
- `brief` and `ask` block for the whole run (`src/mcp.rs:134`, `:153`), which
  runs for minutes. Codex's tool timeout is 60 s unless `tool_timeout_sec` is
  raised. The run is a thread of the stdio server, so it dies when the client
  closes or restarts the server.
- Hires are followed twice: the app polls every 2 s (`follow_herdr`,
  `apps/desktop/src/lib.rs:1860`), and `cos mcp` polls while it serves
  (`follow_while_serving`, `src/mcp.rs:331`). `judge.lock` keeps verdicts
  single. A hire started from Claude Desktop is followed only while that
  server or the app is alive.
- Nothing in a tool result opens the app, and the app registers no URL scheme.
- `~/.cargo/bin/cos` (what the clients spawn) and the cos lib linked into the
  app can be different versions. Nothing compares them.

## Goal

The user switches to a business in the app, asks Claude Desktop or Codex for a
brief, and gets a run id back at once. The app shows that run live. Clicking
the run's link in Claude Desktop opens the app on that run. Settings shows
both clients registered and healthy, and repairs one that is not.

## Approaches

Decisions taken with TypeSafe Jev (`ritual-feedback decide`):

| Question | Chosen | Runner-up | Decide line |
|---|---|---|---|
| Who owns a paid run | a `cos daemon` under launchd owns every run as a grouped child with one deadline; app and `cos mcp` are clients; in-process fallback when it is down | the app owns runs (p=0.05) | `daemon act confidence=0.92` |
| Business after an app switch | the switch stays window-only; MCP defaults to the registry's active business, read per call; the app shows which business MCP clients default to and offers Make default | the switch also sets the active business (p=0.03) | `separate act confidence=0.96` |
| Daemon transport | unix domain socket in the user's cos runtime dir, mode 0600 | loopback HTTP with a token (p=0.00) | `unix act confidence=1.00` |

A detached run (spawned into its own session, recorded on disk, nobody
waiting) was rejected outright: it reparents to PID 1 and breaks the
process-ownership invariant. Leaving the app as the owner fails whenever the
window is closed, which is when Claude Desktop is most likely to be used.

The six pieces:

- **A. `cos daemon`** under launchd (KeepAlive), on a unix socket. It owns
  runs (brief, ask) and the hire follower for every registered business.
  Clients: the app, `cos mcp`, the CLI. When the daemon is down, clients run
  as they do today.
- **B. Paid tools return at once.** `brief` and `ask` return a `run_id` and a
  link. `run_status` reads progress and the result. Progress notifications
  go out where the client sent a progress token.
- **C. Business per call.** Every MCP tool takes an optional `business`.
  `businesses` lists them with the default. With no argument, the server reads
  the registry's active business on each call.
- **D. `cos connect`** in Rust replaces the Taskfile's `jq` and
  `codex mcp add`. It does status, register and repair for Claude Desktop and
  Codex: binary path, the binary runs, its version matches, and Codex's
  `tool_timeout_sec`. Writes keep a backup, and installs copy then rename. A
  Settings card in the app shows the same status and runs the same code.
- **E. `cos://` links.** The Tauri deep-link plugin registers the scheme, and
  tool results carry `cos://run/<id>` and `cos://brief/<business>/<day>`. For
  the reverse direction, the user's installed apps already register `claude:`
  (`/Applications/Claude.app`) and `codex:` (`/Applications/ChatGPT.app`). An
  "Open in Claude" or "Open in Codex" action is possible if those schemes
  accept a prompt. That is unverified.
- **F. Push freshness.** The MCP server sends
  `notifications/resources/updated` for the dashboard resource when a run
  ends. The app already watches `out/` and the agents dir (`watch`,
  `apps/desktop/src/lib.rs:1781`), so it only needs the daemon's run events.

## Constraints

- The brains stay read-only. The daemon writes only where `cos` writes today:
  `out/`, the agents dir and run receipts.
- Every child is owned and reaped by the daemon, with one deadline per run
  (`proc::spawn_grouped`, `wait_until`, `kill_group`). Stopping the daemon
  kills and reaps its runs.
- The remote HTTP server (`cos serve`, `cos tunnel`) stays read-only and never
  reaches the daemon socket.
- Writing client configs is a side effect on files `cos` does not own. Each
  write keeps a `.bak`, preserves every other key, and is refused when the
  file does not parse.
- Tests drive stand-ins only: no real Claude Desktop config, no real
  `~/.codex`, no real launchd, no real Herdr (`tests/fixtures/fake-hc.sh`).
- No step runs longer than 5 minutes.

## Unknowns

| Unknown | Tier | What moves it up |
|---|---|---|
| Daemon protocol and lifecycle (launchd plist, socket path, upgrade while runs are live, fallback rules) | water | a spike: daemon skeleton, one `run brief` round trip from `cos mcp`, kill and restart with a run in flight |
| Whether Claude Desktop renders `cos://` links in tool results as clickable | sand | test with the registered server in a plain chat |
| Whether `claude:` / `codex:` URLs accept a prompt or a new-chat target | water | read their documented URL handlers; without documentation, drop reverse links |
| Progress notifications in Claude Desktop and Codex | sand | log the `progressToken` each client sends |
| Tauri deep-link plugin with the app's `"create": false` window setup and link hooks (`tests/links.rs`) | sand | add the plugin and drive it in `tests/ipc.rs` on the mock runtime |
| Version check between the app and `~/.cargo/bin/cos` | rock | `cos --version` vs `env!("CARGO_PKG_VERSION")` in the app |
