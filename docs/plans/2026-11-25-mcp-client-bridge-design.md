# MCP client bridge: design

Date: 2026-11-25. Brief: [2026-11-25-mcp-client-bridge-brief.md](2026-11-25-mcp-client-bridge-brief.md).

## Decision

A new `cos daemon`, loaded by launchd, owns the paid runs (`brief`, `ask`)
that MCP clients and the app start, for every registered business, and
follows every business's hires. It listens on a unix socket only the user can
open. `cos mcp` and the desktop app become its clients. A paid MCP tool
returns a run id at once, with a `cos://` link when the run's business is a
registered one, and `run_status` reports the run,
waiting up to 50 s per call. Every run, whoever starts it, is a **run record**
under its business's `out/run-records/`, written by the run process itself, so
any client reads it without the daemon. Every brief process, whoever starts it
(daemon, app, CLI, the scheduled launchd job), holds its business's
**brief lock** for its whole life, so one business never has two paid briefs at
once. Every run child watches its owner and ends its own process groups if the
owner dies; the daemon also sweeps leftover groups when it starts. When the
daemon is not running, clients do what they do today. Every local MCP tool
takes an optional `business`. A new `cos connect` command, and a Settings card
in the app over the same code, registers, checks and repairs `cos` in Claude
Desktop and Codex. The app registers `cos://` and opens on the run or brief a
link names.

Out of this design: MCP resource subscriptions (`resources/subscribe`; rmcp
3.11 marks `subscribe` and `unsubscribe` legacy, `handler/server.rs:562-580`)
and links from the app into Claude Desktop or Codex. Neither is needed for the
goal.

## Context

| Piece | Today | File |
|---|---|---|
| MCP server | stdio child of the client. `Config` resolved once at spawn (`resolve_triple`). `brief` and `ask` block on `spawn_blocking` for the whole run and are annotated `read_only_hint = true`. `follow_while_serving` starts lazily, only after a `spin_off` | `src/mcp.rs:126-163`, `:247`, `:331`; `src/main.rs:495`, `:1166` |
| Remote MCP | one `Cos` type for both modes (`Cos { cfg, mode }`); `allowed()` filters to `latest_brief`; `get_info` shared | `src/mcp.rs:31`, `:40-43`, `:375-397`, `:470` |
| MCP App view | `src/dashboard.html` calls `brief` on "Run new brief" and reads `structuredContent.kind`, `.report`, `.cost_usd` | `src/dashboard.html:97-114`, `:164-167` |
| App brief | `brief_run` runs `pipeline::brief_child` on a thread: `cos --home --workspace --projects --no-open brief`, env pinned, stdout to null, the `Child` private to the function, one 420 s deadline. A window mutex allows one at a time per window | `apps/desktop/src/lib.rs:1482-1531`; `src/pipeline.rs:665`, `:683-717` |
| Scheduled brief | `cos.brief` launchd job, weekday 08:30, one label per machine, for the business `task schedule` names | `launchd/cos.brief.plist`, `Taskfile.yml:201-268` |
| Run receipts | `out/runs/<day>.json` holds `RunReceipt`s; `store::latest` skips names that are not `YYYY-MM-DD` | `src/store.rs:16-27`, `:72-80`; `src/pipeline.rs:79-91` |
| App follower | every 2 s, `agents::tick` per registered home with live hires; helpers `registered_homes`, `check_dirs` private to the app | `apps/desktop/src/lib.rs:346`, `:1760`, `:1860` |
| App freshness | `watch` emits `report-changed` on `out/` and agents-dir changes | `apps/desktop/src/lib.rs:1781` |
| Business | registry `active`; `cos business use`. `resolve` has one flags slot: a name plus directory flags is an error | `src/business.rs:233`, `:379-418` |
| Registration | `task install-mcp`: `jq` into the Claude Desktop config, `codex mcp add`. On 2026-11-25 neither client had `cos` registered | `Taskfile.yml:63-90` |
| Processes | `spawn_grouped` (own group, stdin null), `wait_until`, `kill_group`, `start_time`, `group_alive`. `proc::run` puts each grandchild (`claude`, `git`, `sh`) in its own group too | `src/proc.rs:105-345` |
| Hire records | already store `pgid` and `pgid_started` and kill a group only when its leader's start matches | `src/agents/mod.rs:263-265`, `:1116-1131` |
| Judge | runs the hire's `checks` with `sh -c` in its worktree, and `git -C`, each through `proc::run` inside `judge::BUDGET` (300 s) | `src/agents/judge.rs:16`, `:263`, `:292-301` |

Hires are owned by Herdr; `cos` polls them, and `judge.lock` keeps each verdict
single.

## Design

### Terms

- **run**: one paid invocation, `brief` or `ask`, for one business. Its id
  matches `^r-[0-9]{8}-[0-9]{6}-[a-z0-9]{4}$`: UTC start time plus four random
  characters.
- **run record**: `<home>/out/run-records/<id>.json`. A new directory, so the
  day receipts in `out/runs/` are untouched.
- **owner**: the process that spawned the run child and reaps it: `daemon`,
  `app` or `mcp`. A run started from a terminal or by launchd has owner
  `self`.
- **brief lock**: `<home>/out/run-records/brief.lock`, an exclusive `flock`
  the brief process takes before it writes anything and holds until it exits.
- **start**: a process's start time as `proc::start_time` returns it, written
  `"<sec>.<usec>"` (`"1764061702.481233"`). A pid or a pgid leader is the same
  process only when its start matches.
- **groups**: every process group a run owns: its own (the run process is the
  leader) and one per grandchild that `proc` put in a group of its own
  (`claude`, `git`, `sh`; `src/claude.rs:198` → `src/proc.rs:166`).
- **socket**: `<user temp dir>/cos/daemon.sock`, where the user temp dir is
  `confstr(_CS_DARWIN_USER_TEMP_DIR)`. It is the same for every process of the
  user whatever its environment, and short enough for `sun_path` (104 bytes);
  a longer path is refused at bind with the length named. The directory is
  created or `chmod`ed to 0700, and the socket is 0600.
- **default business**: the registry's `active`, read at the moment of a call.
- **business label**: what a record's `business` holds. The registered name
  when the triple came from the registry; `default` for the built-in defaults
  (no registry, no active business); `explicit` for directories given by flag
  or environment with no name. Only a registered name produces a link.
- **platform**: runs are macOS-only. A run needs process start times
  (`proc::start_time`); where the platform gives none, the run process, the
  daemon and every owner refuse to begin, naming why.

### The run process

`cos brief` and `cos ask` gain, behind no new flag:

1. **Brief lock first.** `cos brief` takes the brief lock without blocking
   before anything else. If it is held, the process writes nothing, prints
   the holder's run id (the lock file's content), and exits 73. Holding the
   lock, it writes its own run id into the lock file. `ask` takes no lock, so
   several asks can run.
   **Then the sweep.** Every `cos brief` and `cos ask` start, after the lock
   and before its own work, sweeps its business's records that are not final
   and whose process is gone (`pid` dead, or alive with another `start`). For
   each, it kills the groups that pass the identity check, waits for them to
   be **gone**, and writes `lost` (reason `previous run died`) under the
   record lock. The sweep takes no brief lock, so a terminal `cos ask` killed
   with SIGKILL is cleaned up by the next run for its business.
2. **Run record.** The id comes from `COS_RUN_RECORD` when the owner set it,
   otherwise the process makes one. (`COS_RUN_ID` is a hired agent's hire id,
   which its terminal carries, so a `cos` started there ignores it.) The run
   strips `COS_RUN_RECORD` and `COS_OWNER` from every child it spawns. The process writes `running` with its
   `pid`, `start` and its own group, then its final state at exit (`done`,
   `failed`, `deferred`). `cos ask` writes from `src/main.rs:491-494`, where
   it has the markdown.
3. **Groups recorded.** `proc::spawn_grouped` and `proc::run` add each
   child's `{pgid, start}` to a process-wide set as soon as `spawn` returns.
   An entry leaves the set only once `proc::group_alive(pgid)` is false, not
   when its leader is reaped, because a reaped leader can leave members
   behind. Each change rewrites the record's `groups`. The record lists the
   grandchild groups only. The run's own group is its `pid`, which the owner
   kills.
   **The spawn gate.** A run spawns each grandchild through
   `proc::spawn_gated`. In `pre_exec` the child closes the pipe's write end
   and blocks reading its read end; only `close` and `read`, both
   async-signal-safe, run there. The run process writes the child's `groups`
   entry, then writes one byte and closes the write end, and the child execs.
   If the record write fails, the run process kills the group instead of
   releasing it. If the run process dies first, the write end closes, the
   child reads EOF and exits before `exec`. No grandchild does work outside a
   record. Gated spawns take one process-wide mutex, so no other `fork` can
   inherit the write end. `Command::spawn` returns only once the child has
   exec'd, and the child execs only once released, so the spawn runs on a
   helper thread while the calling thread reads the child's pid from a second
   pipe, writes the `groups` entry and releases the gate. `COS_GATE_HOLD_MS` holds the newest gated child that
   long before its group is recorded, naming it in `<home>/gate.pid`, so
   tests can kill a run inside the window.
4. **Owner watchdog.** When `COS_OWNER=<pid>/<start>` is set, a thread checks
   every second that the pid is alive with that start. When the owner is
   gone, the run runs **Self-cleanup** with `lost` and reason `owner gone`,
   and exits 76.
5. **Signals.** The run process handles SIGTERM, SIGINT and SIGHUP (as
   `src/tunnel.rs` handles them) by running **Self-cleanup** with the ending
   the owner wrote to the record's `stopping` before signalling, else
   `stopped`, and exits 143. A terminal Ctrl-C therefore ends `claude` too.
6. **Self-cleanup** runs before every final write of the run process: normal
   exit (`done`, `deferred`), error (`failed`), signals and the watchdog. It
   kills every group in the set (never the run's own group), waits up to 2 s
   for each to be **gone**, and only then writes the final state under the
   record lock and exits. If a group is still not gone at the bound, it
   writes no final state, logs the group, and exits; the record stays
   non-final for the owner's reaping or the next sweep to retry.
7. **Answer.** `cos ask` writes its markdown into `answer`, capped at
   262,144 bytes and cut at a char boundary, saying so. Stdout behaves as
   today.

Record shape:

```json
{"id":"r-20261125-091502-k3x9","kind":"brief","business":"acme",
 "state":"queued|running|done|failed|deferred|stopped|lost",
 "reason":null,"owner":"daemon|app|mcp|self",
 "pid":4242,"start":"1764061702.481233",
 "groups":[{"pgid":4250,"start":"1764061703.002110"}],
 "owner_pid":4100,"owner_start":"1764058801.120004",
 "started":1764061702,"ended":null,"said":null,"cost_usd":null,
 "day":"2026-11-25","answer":null,"client_req":null}
```

`said` holds the last 12 stderr lines when the run fails. `reason` holds the
cause (`deadline`, `stopped`, `owner gone`, `daemon restarted`,
`brief already running: <id>`, `exit <n>`).

**Writes.** Every write takes an exclusive `flock` on `<id>.lock` beside the
record, re-reads the record, applies the change only if the transition is
allowed, writes a temp file and renames it into place. Allowed transitions:
`queued → running | failed | lost | stopped` and
`running → done | failed | deferred | stopped | lost`. Final states never
change. The owner writes `queued` before spawning, with `owner_pid` and
`owner_start`; `pid`, `start` and `groups` come from the run process.

**The invariant.** Every group that has run work for a run is in its record
(the spawn gate), and every recorded group is **gone** before anyone writes
the run's final state, whatever ended the run. The run's own group is the
exception: only the run process is in it, and the owner kills it. Every path
below holds it.

**Group identity.** A group may be killed when it is alive and either its
leader is alive with the recorded start, or its leader is gone. A pgid stays
reserved while any member lives (`src/proc.rs:235`). A live group whose leader
is gone is therefore still the one recorded, unless the pgid was freed and a
new process with that exact pid led a new group whose leader has since died.
That case is accepted and stated. A live leader with another start is never
killed.

**Gone.** `proc::group_gone(g)` is true when `!group_alive(g.pgid)`, or when
the pgid's leader is alive with a start other than `g.start` (the recorded
group ended and its pgid was reused). Every writer of a final state (Reaping,
Self-cleanup, the sweeps) waits up to 2 s for every recorded group to be gone.
If one is not gone at the bound, that writer writes no final state, logs the
group, and leaves the record for the next reap or sweep to retry.

**Reaping a run** (the owner, on every exit of the run child: normal, error,
signal, SIGKILL): once `wait` returns, the owner kills every group in the
record that passes the identity check, plus the run's own group (`pid`),
and waits up to 2 s for each to be gone. Then it writes the final
state under the record lock if the record is not final yet: `failed` with
reason `exit <n>` or `signal <s>`. A run that wrote `done` before exiting
keeps `done`, and its groups are already gone.

**Stopping a run** (stop, deadline, owner SIGTERM): the owner writes the
ending it wants (`stopped`, `failed` with reason `deadline`, `stopped` with
reason `daemon stopped`) to the record's `stopping` under the record lock,
then sends SIGTERM to the run process and waits up to 5 s. If it is still alive, the owner sends
SIGKILL. Either way it then reaps it as **Reaping a run** says, writing
`stopped`, or `failed` with reason `deadline`, when the run did not write its
own final state.

**Starting a run sweeps dead runs.** The start sweep (The run process, item
1) covers the brief lock's previous holder: a terminal brief killed with
SIGKILL, or one whose owner died too, is swept before the next brief starts.
A brief therefore never runs beside the processes of a dead brief for the
same business, and a dead `ask` is swept by the next run of either kind.

**Readers never write.** `run_status`, `runs`, `run_get` and the subscribe
stream may show a `queued` or `running` record as `lost` when its process is
gone (for `queued` with no pid, when the owner is gone). Before showing
`lost`, a reader re-reads the record. If the record is now final, it shows
that state. Readers never write `lost`. Only the run's self-cleanup, the brief
lock's sweep and the daemon's sweep write it.

### A. The daemon

`cos daemon` runs in the foreground; launchd keeps it alive.

0. **Log.** It creates the socket directory (0700), then reopens its own
   stderr onto `<daemon dir>/daemon.log`: appended to, created 0600 if
   missing, before it takes the lock or binds the socket. The log is
   therefore beside the socket whoever started the daemon (launchd, a shell,
   a test on a temp `COS_DAEMON_DIR`); the plist names no log path. Stdout
   stays where it was ("already running" goes there).
1. Takes an exclusive `flock` on `daemon.lock` beside the socket, retrying
   once after 100 ms (a client never takes this lock; it probes by
   connecting). If the lock is still held, it exits 0 with "already running".
2. **Sweep.** For every registered business, every record in `queued` or
   `running` whose owner is `daemon` and whose `owner_pid`/`owner_start` is
   not this daemon is swept, and so is every other record that is not final
   and whose process is gone, as the start sweep defines it. The sweep kills
   the run's own group and each group in `groups` that passes the identity
   check, waits for each to be gone, then writes the record `lost` (reason
   `daemon restarted`, or `previous run died` for another owner) under its
   lock. The watchdog usually
   ends such runs first; the sweep covers a run whose watchdog did not. The
   daemon's own children outside runs (the judge's `sh -c` checks and `git`,
   the follower's `hc` calls) are in the daemon's `proc` set. That set is
   mirrored to `daemon.groups.json` beside the socket on every change, and
   the sweep kills those groups under the same identity check.
3. Binds the socket, removing a stale socket file only while holding the lock.
4. Serves connections. Each request is one JSON line. A connection gets one
   JSON line in reply, except `subscribe`, which streams lines until the
   client closes. Requests are capped at 64 KiB per line, and a connection
   with no complete line within 5 s is closed. At most 16 connections are
   open at once.
5. Runs the hire follower: `agents::follow_all(registry)`, the loop body of
   `follow_herdr` moved into the lib, every 2 s. `registered_homes` and
   `check_dirs` move into `cos::business` with the app calling them.
6. On SIGTERM (handled as `src/tunnel.rs` handles it), stops accepting,
   stops every run it owns as **Stopping a run** says (reason
   `daemon stopped`), closes its group tracking (no new child of its own
   starts), kills its own groups, and exits 0. Known limit: a judge still
   running when tracking closes cannot start its next check, and its verdict
   is dropped.
7. Each tick it compares its executable's size and mtime with what it started
   with. When they differ, no run it owns is live and it is not drained, it
   exits 1 so launchd starts the new binary. A drained daemon stays up until
   the `bootout` of the install that drained it (or an `undrain`), since
   launchd would restart a retired one undrained before that `bootout`.

Requests:

| Request | Does |
|---|---|
| `hello` | `{"version","pid"}` |
| `start` brief | under a per-business mutex: if the brief lock file is locked, replies with the run id it holds and `"existing": true`. Otherwise writes `queued`, spawns the run (below), and replies `{"run","existing":false}`. If the child then loses the lock race (exit 73), the daemon writes the record `failed` with reason `brief already running: <id>`. A repeated `client_req` returns the first reply while the daemon lives (in-memory map, cleared on restart) |
| `start` ask | writes `queued`, spawns, replies `{"run","existing":false}`. Capped at 2 live asks per business and 4 live runs in total (`ASKS_PER_BUSINESS`, `RUNS_TOTAL`; `daemon::over_cap` words the refusal: `2 asks are live for <business>, the most per business`, `4 runs are live, the most run at once`) |
| `stop` | stops a run it owns as **Stopping a run** says; replies with the final state. Replies with an error, not a state, when one of the run's groups is still alive at the **gone** bound: the record stays non-final for the next sweep |
| `drain` | under the runs lock: with no run live, sets the daemon draining and replies `{"drained":true,"pid"}`; from then on every `start` is refused (`the daemon is draining for a reinstall; …`) and every other request is served. With a run live, changes nothing and replies `{"error":"runs are live: <id> (<business>), …","live":[{"run","business"}]}`. A drained daemon stays drained until `undrain` or its exit, and does not retire on an executable change meanwhile. Draining again is allowed |
| `undrain` | clears the drain and replies `{"drained":false}`; `start` is admitted again. Install sends it when it fails after a drain |
| `subscribe` | streams `{"run","business","kind","state"}` for every record that changes under any registered home, including runs the daemon does not own (CLI, launchd, app fallback). It polls each `out/run-records/` directory every second with `dash::stamp` (no new crate) and reads the changed records. It sends changes only: the records present when the client connects are its baseline and are not sent, so a client seeds itself from the files |

**Spawning.** `pipeline::brief_child` splits into `spawn_run(kind, triple,
question, env) -> RunChild` (returns pid, pgid and the `Child`) and
`RunChild::wait_until(deadline)` / `RunChild::kill()`. The app's fallback path
and the daemon both use them. argv, the same for both owners:

```
cos --home <h> --workspace <w> --projects <p> --no-open brief
cos --home <h> --workspace <w> --projects <p> ask -- <question>
```

The environment is pinned as `brief_child` pins it today (`COS_HOME`,
`COS_WORKSPACE`, `COS_PROJECTS`, no `COS_BUSINESS`), plus `COS_RUN_RECORD` and
`COS_OWNER=<owner pid>/<owner start>`. The three directories come from the registry entry, which is the
only way a business name reaches a child. `question` goes in as one argv
element after `--`, never through a shell.

**Deadline.** `pipeline::BRIEF_CHILD`, 420 s, for both kinds. This is a
product budget for one paid model run, not a gate or a shell step, so the
5-minute gate rule does not apply. It is the same budget the app uses today.

**Install.** `cos daemon install [--force]` renders the plist from a template
embedded with `include_str!("../launchd/cos.daemon.plist")`. The job sets
`RunAtLoad`, `KeepAlive = { SuccessfulExit = false }` (a duplicate's exit 0 is
not relaunched), `COS_BUSINESSES` pinned as `cos.brief` pins it, and a
`PATH` that names `claude`, `gh`, `cn` and `hc` (the Taskfile's `TOOL_PATH`).
Before it writes anything, install sends `drain` on the socket, so the
`bootout` that follows kills no run and no `start` can land in between:

- `drain` answers `drained`: install proceeds, keeping the pid it named.
- `drain` fails naming live runs: without `--force`, install refuses with
  exit 75 (`EX_TEMPFAIL`, the `Busy` error), naming each run and `--force`;
  nothing is written and `launchctl` is not called. With `--force`, install
  sends `stop` for each run `drain` named, then drains once more, and
  refuses with exit 75 if runs are still live.
- Nothing listens on the socket (`ENOENT` or `ECONNREFUSED`), or the daemon
  answers `unknown op "drain"` (a build from before `drain`): install
  proceeds only when no record names a live daemon run, and otherwise
  refuses with exit 75, `--force` or not, since there is no daemon to send
  `stop` to. A live daemon run is a record under any registered home with
  owner `daemon`, a state that is not final, and a process that is not gone.
- Any other failure (a connect error of another kind, no reply within 5 s,
  a reply that does not parse, another refusal) fails the install with that
  error and no `launchctl` call: a slow daemon is not a gone one.

Just before it writes the plist and runs `bootout`, install drains once
more under the same rules without `--force`: a daemon that came up
undrained in between (launchd relaunching one that retired on the binary
swap before the first drain, or one started while `--force` stopped runs)
is drained too, and one that took a run meanwhile refuses the install with
exit 75. The pid that this second drain names is the one checked after
`bootout`.

After `bootout`, install probes the socket. If the daemon that drained still
answers `hello` with the same pid, launchd did not start it (a daemon run
from a shell), so `bootout` cannot stop it: install fails naming the pid,
without `bootstrap`. Every failure after a drain sends `undrain`, so a
daemon that is still up takes runs again. A shell-started daemon therefore
keeps serving, undrained, on the binary it started from; the user stops it
(the error names `kill <pid>`) and installs again.

`task install` treats 75 as "refused" and carries on. Known limit: an
install killed between its drain and its `undrain` or `bootout` leaves the
daemon drained; the next `cos daemon install` drains it again and reloads it.

The plist is written to `~/Library/LaunchAgents/cos.daemon.plist.new`
(`COS_LAUNCH_AGENTS` overrides the directory), renamed into place, and
loaded with `bootout` then `bootstrap gui/<uid>`. `cos daemon uninstall`
boots the job out and removes the file. `launchctl` is resolved from
`COS_LAUNCHCTL`, defaulting to `/bin/launchctl`, so tests use a stand-in.
`task install` and `task setup` call `cos daemon install` after the binary
copy.

### Clients

The probe: connect to the socket. `ENOENT` or `ECONNREFUSED` within 200 ms
means the daemon is **down**. Any other error is reported as an error and is
never a reason to fall back, so a lost reply cannot cause a second paid run.
`start` carries a fresh `client_req` (a random 16-hex id) for safe retries.

| Client | Daemon up | Daemon down |
|---|---|---|
| `cos mcp` `brief`/`ask` | `start`, for a triple with a registered business name. A `default` or `explicit` triple is always owned by `cos mcp`, since the daemon starts runs only for registered businesses | spawns the run itself through `spawn_run` (owner `mcp`) on a thread with the deadline, returns the id at once, and kills and reaps its runs when stdin closes. It applies the daemon's caps to the runs it owns, from the same constants and with the same refusal (`daemon::over_cap`): 2 live asks per business, 4 live runs in total. The `brief` and `ask` tool descriptions say a run this server starts itself ends when the client closes |
| app `brief_run` | `start`, then sets `BriefRun.running` from the reply; the subscribe thread updates it | today's path, through `spawn_run` (owner `app`), writing records |
| app subscribe thread | one connection. On connect it seeds the window's state from the record files and re-reads the brief the window shows (the stream sends changes only, so a reconnect would otherwise miss what changed while it was down). On each event for the window's business it emits `brief-changed` for both kinds, carrying `kind` and the run id (a brief's carries a `BriefRun` built from the record), and on a final state `report-changed` with payload `{run}`. On disconnect it retries every 2 s | absent; `watch` already emits `report-changed` from the files |
| `cos brief` / `cos ask` in a terminal, the `cos.brief` launchd job | unchanged: they run in the foreground, owner `self`, and take the brief lock, so they cannot overlap a daemon brief for the same business | unchanged |
| hire following (app and `cos mcp`) | skipped while `hello` answers, for registered homes. `hello` is checked each tick, cached 3 s. The app still follows hires in its own home when that home is not registered, since the daemon follows registered homes only | as today: the app every 2 s, `cos mcp` from its first `spin_off` |

### B. Paid tools return at once

| Tool | Input | Output (structured) |
|---|---|---|
| `brief` | `{business?}` | `RunView` |
| `ask` | `{question, business?}` (question 1 to 4000 chars) | `RunView` |
| `run_status` | `{run, business?, wait_secs?}` (`wait_secs` 0 to 50, default 0) | `RunView`, with `report`, `markdown`, `day` and `cost_usd` once a `brief` is `done`, and `answer` once an `ask` is `done` |
| `runs` | `{business?, limit?}` (limit 1 to 20, default 5) | `{runs: [RunView]}`, newest first, run records only |

`RunView` is `{run, business, kind, state, reason?, link?, existing?, day?,
report?, markdown?, cost_usd?, answer?}`, with
`link = cos://run/<business>/<run>` when the serving triple has a registered
business name. For a `default` or `explicit` triple (built-in defaults,
directories given by flag or env) `link` is omitted, since it would name no
business the app can switch to; the record and `business` still carry the
label.

- `run_status` and `runs` take `business` as `RunView.business` gives it: a
  view following a run passes it back unchanged, and a pinned server accepts
  its own label (`default`, `explicit`) as well as its business name.

- `brief` while a brief for that business runs returns that run with
  `existing: true` and does not spend. Its description says so.
- `run_status` waits by re-reading the record every 500 ms until it leaves
  `queued`/`running` or `wait_secs` runs out. It reads files only, so it works
  with the daemon down. 50 s fits inside Codex's 60 s default.
- `brief` and `ask` change to `read_only_hint = false`. `run_status` and
  `runs` are `read_only_hint = true`, `open_world_hint = false`.
- The MCP App view: "Run new brief" calls `brief`, then `run_status` with
  `wait_secs: 45` until the state is final, showing "Running…" with the run
  id meanwhile, and renders the `RunView`'s `report` with the existing
  renderers. The view's `accept` reads `kind` from `RunView`. `latest_brief`
  keeps the `Report` shape.

### C. Business per call

Local `cos mcp` keeps two values: the `Triple` it resolved at start and
whether it was **pinned** (layer `Flags` or `Env`, so `--business`,
`--home`/`--workspace`/`--projects`, `COS_BUSINESS` or `COS_HOME`).

| Server | `business` given | `business` absent |
|---|---|---|
| local, pinned to X | X: runs on X. Another name: refused, `pinned to X: this server was started with <flag/env>` | X |
| local, unpinned | resolved from the registry by name (`business::resolve` with that name in the flags slot and no directory flags); unregistered: refused, naming the known ones | the registry's active business, loaded on each call |
| remote | refused `-32602`: `call_tool` rejects any argument a remote tool does not declare, before dispatch | the business fixed at start |

- Every local tool gains `business?`: `latest_brief`, `plan`, `handoff`,
  `agents`, `spin_off`, `stop_agent`, plus the four in B.
- `businesses` (local only): `{businesses: [{name, home, default}], pinned:
  "<name>" | null}`.
- Remote: `Cos::new(cfg, Mode::Remote)` builds no daemon client and no
  per-call resolver, `get_info` for remote is unchanged, and the remote tool
  list stays `["latest_brief"]` with its schema unchanged.
- App: the business picker marks the registry's active business with an
  "MCP default" badge, and its footer offers **Make default** for the
  highlighted row: a new `business_make_default` command that calls
  `business::set_active`. This is the one registry write the app
  gains. `switch_business` stays window-only.

### D. `cos connect`

```
cos connect status [--json]
cos connect claude-desktop [--repair] [--bin <abs path>]
cos connect codex [--repair] [--bin <abs path>]
cos connect remove claude-desktop|codex
```

`status`, one row per client:

| Field | Claude Desktop | Codex |
|---|---|---|
| config | `~/Library/Application Support/Claude/claude_desktop_config.json` | `$CODEX_HOME/config.toml`, default `~/.codex/config.toml` |
| installed | the config's directory exists | `~/.codex` exists or `codex` is on PATH |
| registered | `.mcpServers.cos` present | `[mcp_servers.cos]` present |
| command | the configured path, absolute and executable | same |
| version | `<command> --version` prints `cos <this version>` | same |
| timeout | — | `tool_timeout_sec` ≥ 320 |
| daemon | in the app's card, `hello` answers with this version; `unknown` in `cos connect status` | same |

The version check spawns the configured binary. That is a bounded child
(5 s deadline, killed and reaped) whose result is cached per path, size and
mtime, so repeated `status` calls and the Settings card spawn it once per
binary. A timeout reads as the last cached result, marked stale.

The timeout is 320 s, the value the Taskfile already recommends: `spin_off`
seeds a worktree before it returns, and `stop_agent` waits up to 10 s, so
paid runs no longer need it, but hires can.

Writes (register, repair, remove) touch only the `cos` entry:

1. Read the file and keep its bytes. Parse it: JSON with `serde_json`
   (`preserve_order` is already enabled), TOML with `toml_edit` 0.25 (new
   direct dependency, already in the lock through another crate; it keeps
   comments and order, which `toml` 1.1 cannot). A parse failure refuses the
   write, naming the file and the error.
2. Set `command` to the absolute `--bin` (default: the running `cos`'s
   resolved path), `args` to `["mcp"]`, and `env.PATH` to the tool path. For
   Codex, also set `tool_timeout_sec = 320`.
3. If the file existed before this write and `<file>.cos.orig` does not
   exist, write the original bytes there with the original's mode. It is
   never overwritten, so it always holds the pre-`cos` file. A file `cos`
   creates gets no `.cos.orig`.
4. Write `<file>.cos.new`. Re-read the file: if its bytes changed since step
   1, refuse ("changed while writing; try again") and remove `.cos.new`.
   Otherwise rename `.cos.new` into place.

`task install-mcp` runs `cos connect <client> --repair` for each client
whose `installed` field is true, and skips a client that is not installed.

**Settings card.** A settings gear in the app header opens the Settings
dialog, whose section is "Assistants". It calls `connect_status` (the same
function), shows each client green or red with the failing field named, and
offers Register, Repair or Remove (`connect_apply`). Register and Repair
write the window's resolved `cos` binary as `command`, and are disabled when
the window has none. After a Claude Desktop write it says "restart Claude
Desktop to load it". The card probes the daemon itself; `cos connect status`
in a terminal leaves its daemon field unknown. A daemon that is down shows
muted, not red: the clients still work without it. States render through
`stateView`, as `tests/design.rs` requires.

**Version skew.** The card compares the app's `env!("CARGO_PKG_VERSION")`
with each configured binary's version and with the daemon's `hello.version`,
and names a mismatch.

### E. `cos://` links

- `tauri-plugin-deep-link` 2.6 and `tauri-plugin-single-instance` 2.4 are new
  app dependencies. `apps/desktop/bundle/Info.plist` gains `CFBundleURLTypes`
  with the scheme `cos`. `task desktop-app` runs
  `lsregister -f "<installed app>"` after the copy, and its last step reads
  the scheme back from LaunchServices (`lsregister -dump` filtered to the
  bundle id, matching the `CFBundleURLName` in any position of its claim
  list) and fails if it is missing.
- Accepted URLs, exactly:
  - `cos://run/<business>/<run-id>`
  - `cos://brief/<business>/<YYYY-MM-DD>`
  - `cos://settings/assistants`

  Anything else, including a query string, a fragment, percent-encoding, or
  an extra or missing segment, is ignored with one stderr line. The
  deep-link plugin hands over URLs already normalised (scheme and host
  lowercased, dot segments resolved); the grammar applies to that form, and
  a normalised link still only navigates.
- **Where a link lands.**
  - A `run` link opens a run panel on the Today tab, above the report. The
    panel shows the run's kind, state, start time, reason and owner. A Stop
    button appears only while the run is live and is owned either by the
    daemon or by this app process (the app keeps its fallback `RunChild`s in
    managed state, keyed by run id). For any other owner, the panel says
    where the run was started (a terminal, the schedule, or Claude Desktop or
    Codex without the daemon) and offers no Stop. Loading, error and lost
    states render through `stateView`. When a brief run is `done`, the panel shows a
    "Brief ready" line and the report below refreshes to it. For an `ask`,
    the panel renders `answer` as markdown with marked and DOMPurify, the
    docs room's renderer.
  - A `brief` link opens Today on `/brief/<day>`, resolved in order: the
    latest brief while that is the day's, else `out/reports/<day>.html` from
    the history archive, else a refusal naming the day.
  - New Tauri commands: `run_get(business, run)` returns the `RunView` (read
    through the same function as MCP `run_status` with `wait_secs: 0`) plus
    `owner`, `started` and `stoppable`, and `run_stop(business, run) ->
    {state}`. `run_stop`
    sends `stop` to the daemon for `owner: daemon`. For a run this app
    owns, it stops its own `RunChild`. For anything else it refuses with
    `not stoppable here: started by <owner>`. The panel updates from
    `brief-changed` events, which carry the run id.
- A link only navigates. It never starts a run, writes state or approves
  anything. A link naming another business switches the window only through
  `switch_business`: window-only, nothing persists, and the header shows the
  business name. The window may decline the switch (a pinned window does,
  and says why), and the link then goes no further.
- **Cold start.** A link that launches the app is held in app state until the
  window calls the `link_ready` command, which it does after its own
  `openFromHash` has run, so the held link lands on top of the window's
  starting view; then it is applied. A link that arrives while running goes
  through `single-instance` to the existing window.
- MCP `RunView.link` carries the link when there is one, in the structured
  content only.

### F. Freshness

The app's subscribe thread (Clients) is F. MCP resource notifications are out
of this design (see Decision).

## Alternatives rejected

- **The app as owner.** Runs started from Claude Desktop would fail whenever
  the window is closed. Jev: `app` p=0.05.
- **Detached runs.** They reparent to PID 1 with nobody reaping them.
- **Routing the scheduled brief through the daemon.** The launchd job would
  depend on the daemon. The brief lock gives the same guarantee, one brief
  per business, with no dependency, and it also covers a terminal run.
- **An app switch that sets the active business.** It would silently retarget
  every MCP client. Jev chose window-only plus Make default (0.96).
- **Loopback HTTP.** Any local process could reach it, and so could a browser
  page through DNS rebinding. Jev 1.00 for the socket.
- **Progress notifications.** No long call remains to report on.
- **MCP resource subscriptions.** The app learns of runs from the daemon, and
  rmcp's subscribe handlers are legacy-only.
- **`codex mcp add`.** It cannot set `tool_timeout_sec`.

## Semantic ISA

### Side-effect inventory

| # | Side effect | System | Reversible? | Affected if it misfires |
|---|---|---|---|---|
| S1 | write `claude_desktop_config.json` and `.cos.orig` | Claude Desktop config | yes: `.cos.orig` | every MCP server in Claude Desktop |
| S2 | write `config.toml` and `.cos.orig` | Codex config | yes: `.cos.orig` | every Codex MCP server and setting |
| S3 | write `cos.daemon.plist`; `launchctl bootout` / `bootstrap` | launchd | yes: `cos daemon uninstall` | the login session |
| S4 | spawn a paid run (`brief`, `ask`) that calls Claude | Anthropic API | no: spend is spent | the user's bill |
| S5 | kill a run's process groups (stop, deadline, SIGTERM, watchdog, sweep) | local processes | no | that run |
| S6 | write run records | `out/run-records/` | yes | views that read them |
| S7 | create the socket, its directory, `daemon.lock` and `daemon.log` (appended, 0600) | user temp dir | yes | other daemons |
| S15 | drain the daemon: every later `start` refused until `undrain` or its exit | the running daemon | yes: `undrain`, or the daemon's restart | paid runs from MCP and the app until the reload |
| S8 | set the registry's active business (Make default) | `businesses.toml` | yes | CLI and MCP defaults |
| S9 | register the `cos:` scheme; `lsregister -f` | LaunchServices | yes: remove the app | links |
| S10 | navigate and switch the window on a link | the app window | yes | the window |
| S11 | spawn the configured binary for `--version` (`connect status`) | local process | n/a (read) | none: bounded and reaped |
| S12 | hire via `spin_off` on a per-call business: worktree cut and seed, Herdr terminal start (paid agent session) | git, Herdr, Anthropic or OpenAI | worktree: yes (`cos agents clean`); spend: no | that business's repos, the bill |
| S13 | follower actions now hosted in the daemon: `hc send`, `interrupt`, `kill`, `rm`, focus; the judge's `sh -c <check>` and `git -C` in the hire's worktree; ledger and verdict writes | Herdr, the hire's worktree, `out/agents` | interrupt/kill/checks: no | that hire |
| S14 | the run process's own writes: `out/briefs`, `out/facts`, `out/runs`, tracking receipts in `state.json` | the business's home and workspace | as today | as today |

S12 to S14 are not new capabilities. S12 gains a per-call business, and S13
moves host (app or `cos mcp` → daemon); both are listed so their checks are
pinned here.

### Instructions

| Instruction | Core | Governance property | Sink? | Side effects |
|---|---|---|---|---|
| `LOAD(client_config)` | Memory | Deterministic I/O | no | read |
| `VERIFY(config_write_safe)` | Normative | Deterministic checkpoint | no | — |
| `TOOL_CALL(config.write)` | Execution | Deterministic action | **yes** | S1, S2 |
| `TOOL_CALL(binary.version)` | Execution | Deterministic action | **yes** | S11 |
| `TOOL_CALL(run.drain)` | Execution | Deterministic action | **yes** | S15 |
| `TOOL_CALL(run.undrain)` | Execution | Deterministic action | **yes** | S15 (reverses it) |
| `VERIFY(no_live_runs_or_force)` | Normative | Deterministic checkpoint | no | — |
| `TOOL_CALL(launchd.install)` | Execution | Deterministic action | **yes** | S3, S7 |
| `VERIFY(peer_is_owner)` | Normative | Deterministic checkpoint | no | — |
| `VERIFY(request_admissible)` | Normative | Deterministic checkpoint | no | — |
| `TOOL_CALL(run.start)` | Execution | Deterministic action | **yes** | S4 (and S14 through the child) |
| `TOOL_CALL(run.stop)` | Execution | Deterministic action | **yes** | S5 |
| `VERIFY(group_identity)` | Normative | Deterministic checkpoint | no | — |
| `TOOL_CALL(group.kill)` | Execution | Deterministic action | **yes** | S5 (watchdog, sweep, deadline, SIGTERM) |
| `STORE(run_record)` | Memory | Deterministic I/O | **yes** | S6 |
| `TOOL_CALL(registry.set_active)` | Execution | Deterministic action | **yes** | S8 |
| `VERIFY(link_grammar)` | Normative | Deterministic checkpoint | no | — |
| `TOOL_CALL(window.navigate)` | Execution | Deterministic action | **yes** | S10 |
| `VERIFY(business_admissible)` | Normative | Deterministic checkpoint | no | — |
| `TOOL_CALL(hire.spawn)` | Execution | Deterministic action | **yes** | S12 |
| `TOOL_CALL(hire.follow)` | Execution | Deterministic action | **yes** | S13 |
| `GENERATE(brief \| answer)` | Cognitive | Probabilistic output | no | inside the child, unchanged |

S9 runs at install with fixed arguments and takes no input.

### Bindings

Every schema below resolves its references against these definitions, and
every object is closed:

```json
{"$defs":{
  "business":{"type":"string","pattern":"^[a-z0-9][a-z0-9-]{0,39}$"},
  "run":{"type":"string","pattern":"^r-[0-9]{8}-[0-9]{6}-[a-z0-9]{4}$"}}}
```

```json
{"instruction":"TOOL_CALL(run.start)",
 "input_schema":{"oneOf":[
  {"type":"object","additionalProperties":false,"required":["op","kind","business","client_req"],
   "properties":{"op":{"const":"start"},"kind":{"const":"brief"},"business":{"$ref":"#/$defs/business"},
    "client_req":{"type":"string","pattern":"^[0-9a-f]{16}$"}}},
  {"type":"object","additionalProperties":false,"required":["op","kind","business","client_req","question"],
   "properties":{"op":{"const":"start"},"kind":{"const":"ask"},"business":{"$ref":"#/$defs/business"},
    "client_req":{"type":"string","pattern":"^[0-9a-f]{16}$"},
    "question":{"type":"string","minLength":1,"maxLength":4000}}}]},
 "output_schema":{"type":"object","additionalProperties":false,"required":["run","existing"],
  "properties":{"run":{"$ref":"#/$defs/run"},"existing":{"type":"boolean"}}}}
```

```json
{"instruction":"TOOL_CALL(run.stop)",
 "input_schema":{"type":"object","additionalProperties":false,"required":["op","run","business"],
  "properties":{"op":{"const":"stop"},"run":{"$ref":"#/$defs/run"},"business":{"$ref":"#/$defs/business"}}},
 "output_schema":{"type":"object","additionalProperties":false,"required":["state"],
  "properties":{"state":{"enum":["stopped","done","failed","deferred","lost"]}}}}
```

```json
{"instruction":"TOOL_CALL(run.drain)",
 "input_schema":{"type":"object","additionalProperties":false,"required":["op"],
  "properties":{"op":{"const":"drain"}}},
 "output_schema":{"oneOf":[
  {"type":"object","additionalProperties":false,"required":["drained","pid"],
   "properties":{"drained":{"const":true},"pid":{"type":"integer","minimum":2}}},
  {"type":"object","additionalProperties":false,"required":["error","live"],
   "properties":{"error":{"type":"string"},"live":{"type":"array","minItems":1,"maxItems":4,
    "items":{"type":"object","additionalProperties":false,"required":["run","business"],
     "properties":{"run":{"$ref":"#/$defs/run"},"business":{"$ref":"#/$defs/business"}}}}}}]}}
```

```json
{"instruction":"TOOL_CALL(run.undrain)",
 "input_schema":{"type":"object","additionalProperties":false,"required":["op"],
  "properties":{"op":{"const":"undrain"}}},
 "output_schema":{"type":"object","additionalProperties":false,"required":["drained"],
  "properties":{"drained":{"const":false}}}}
```

```json
{"instruction":"TOOL_CALL(group.kill)",
 "input_schema":{"type":"object","additionalProperties":false,"required":["pgid","start"],
  "properties":{"pgid":{"type":"integer","minimum":2},"start":{"type":"string","pattern":"^[0-9]+\\.[0-9]{1,6}$"}}},
 "output_schema":{"type":"object","additionalProperties":false,"required":["killed"],
  "properties":{"killed":{"type":"boolean"}}}}
```

```json
{"instruction":"STORE(run_record)",
 "input_schema":{"type":"object","additionalProperties":false,
  "required":["id","kind","business","state","owner","started"],
  "properties":{"id":{"$ref":"#/$defs/run"},"kind":{"enum":["brief","ask"]},"business":{"$ref":"#/$defs/business"},
   "state":{"enum":["queued","running","done","failed","deferred","stopped","lost"]},
   "reason":{"type":["string","null"],"maxLength":200},"owner":{"enum":["daemon","app","mcp","self"]},
   "pid":{"type":["integer","null"]},"start":{"type":["string","null"],"pattern":"^[0-9]+\\.[0-9]{1,6}$"},
   "groups":{"type":"array","maxItems":64,"items":{"type":"object","additionalProperties":false,
    "required":["pgid","start"],"properties":{"pgid":{"type":"integer","minimum":2},
     "start":{"type":"string","pattern":"^[0-9]+\\.[0-9]{1,6}$"}}}},
   "owner_pid":{"type":["integer","null"]},"owner_start":{"type":["string","null"],"pattern":"^[0-9]+\\.[0-9]{1,6}$"},
   "started":{"type":"integer"},"ended":{"type":["integer","null"]},
   "said":{"type":["string","null"],"maxLength":8192},"cost_usd":{"type":["number","null"]},
   "day":{"type":["string","null"],"pattern":"^[0-9]{4}-[0-9]{2}-[0-9]{2}$"},
   "answer":{"type":["string","null"],"maxLength":262144},
   "client_req":{"type":["string","null"],"pattern":"^[0-9a-f]{16}$"}}},
 "output_schema":{"type":"object","additionalProperties":false,"required":["path"],
  "properties":{"path":{"type":"string"}}}}
```

```json
{"instruction":"TOOL_CALL(config.write)",
 "input_schema":{"type":"object","additionalProperties":false,"required":["client","action","bin"],
  "properties":{"client":{"enum":["claude-desktop","codex"]},"action":{"enum":["register","repair","remove"]},
   "bin":{"type":"string","pattern":"^/"}}},
 "output_schema":{"type":"object","additionalProperties":false,"required":["file","orig"],
  "properties":{"file":{"type":"string"},"orig":{"type":"string"}}}}
```

```json
{"instruction":"TOOL_CALL(binary.version)",
 "input_schema":{"type":"object","additionalProperties":false,"required":["bin"],
  "properties":{"bin":{"type":"string","pattern":"^/"}}},
 "output_schema":{"type":"object","additionalProperties":false,"required":["version","stale"],
  "properties":{"version":{"type":["string","null"]},"stale":{"type":"boolean"}}}}
```

```json
{"instruction":"TOOL_CALL(launchd.install)",
 "input_schema":{"type":"object","additionalProperties":false,"required":["bin","force"],
  "properties":{"bin":{"type":"string","pattern":"^/"},"force":{"type":"boolean"}}},
 "output_schema":{"type":"object","additionalProperties":false,"required":["plist","loaded"],
  "properties":{"plist":{"type":"string"},"loaded":{"type":"boolean"}}}}
```

```json
{"instruction":"TOOL_CALL(registry.set_active)",
 "input_schema":{"type":"object","additionalProperties":false,"required":["name"],
  "properties":{"name":{"$ref":"#/$defs/business"}}},
 "output_schema":{"type":"object","additionalProperties":false,"required":["active"],
  "properties":{"active":{"$ref":"#/$defs/business"}}}}
```

```json
{"instruction":"TOOL_CALL(window.navigate)",
 "input_schema":{"oneOf":[
  {"type":"object","additionalProperties":false,"required":["view","business","run"],
   "properties":{"view":{"const":"run"},"business":{"$ref":"#/$defs/business"},"run":{"$ref":"#/$defs/run"}}},
  {"type":"object","additionalProperties":false,"required":["view","business","day"],
   "properties":{"view":{"const":"brief"},"business":{"$ref":"#/$defs/business"},
    "day":{"type":"string","pattern":"^[0-9]{4}-[0-9]{2}-[0-9]{2}$"}}},
  {"type":"object","additionalProperties":false,"required":["view"],
   "properties":{"view":{"const":"settings-assistants"}}}]},
 "output_schema":{"type":"object","additionalProperties":false,"required":["shown"],
  "properties":{"shown":{"type":"boolean"},"refused":{"type":"string"}}}}
```

```json
{"instruction":"TOOL_CALL(hire.spawn)",
 "input_schema":{"type":"object","additionalProperties":false,"required":["from","item"],
  "properties":{"business":{"$ref":"#/$defs/business"},"from":{"enum":["brief","goals"]},
   "item":{"type":"integer","minimum":1,"maximum":50},"harness":{"enum":["claude","codex"]},
   "checks":{"type":"array","maxItems":10,"items":{"type":"string","minLength":1,"maxLength":500}},
   "scope":{"type":"array","maxItems":50,"items":{"type":"string","minLength":1,"maxLength":300,
    "not":{"pattern":"(^/|\\.\\.)"}}},
   "role":{"type":"string","pattern":"^[a-z][a-z0-9-]{0,39}$"}}},
 "output_schema":{"type":"object","additionalProperties":false,"required":["id","terminal","workdir"],
  "properties":{"id":{"type":"string"},"terminal":{"type":"string"},"workdir":{"type":"string"}}}}
```

```json
{"instruction":"TOOL_CALL(hire.follow)",
 "input_schema":{"type":"object","additionalProperties":false,"required":["home","hire"],
  "properties":{"home":{"type":"string","pattern":"^/"},
   "hire":{"type":"string","pattern":"^[a-z0-9][a-z0-9-]{0,63}$"}}},
 "output_schema":{"type":"object","additionalProperties":false,"required":["acted"],
  "properties":{"acted":{"enum":["none","interrupted","killed","judged","removed"]}}}}
```

`hire.spawn`'s bounds on `checks` and `scope` are new: `SpinOffArgs`
(`src/mcp.rs:63-84`) is unbounded today. `hire.follow`'s `home` must be a
registered home, and `hire` must be a hire under it.

### Taint

- **Sources:** MCP tool arguments (model output in Claude Desktop and Codex);
  bytes on the socket; URLs from the OS (any app or web page can open
  `cos://`); client config file contents; `spin_off`'s `checks` and `scope`
  (model output).
- **Propagation:** a record written from a tainted request stays tainted.
  `question` reaches the child as one argv element after `--`. `checks` run
  as today: `sh -c` in the hire's sandboxed worktree, bounded by
  `judge::BUDGET`.
- **Tag-dropping paths:** record → `run_status` → model. The record is data
  the model reads, never an instruction `cos` executes. Link → window: parsed
  into the closed enum before any state is touched.

### VERIFY before every sink

| Sink | Cleared by | Check |
|---|---|---|
| `config.write` | `VERIFY(config_write_safe)` | the file parses; the document minus the `cos` entry is unchanged (JSON: value-equal; TOML: text-equal outside `[mcp_servers.cos]`); `bin` is absolute, executable, and `--version` prints `cos <semver>`; `.cos.orig` exists before the rename; the file's bytes are unchanged since the read |
| `binary.version` | none needed beyond its binding | absolute path; 5 s deadline; killed and reaped; cached by path, size and mtime |
| `launchd.install` | `VERIFY(no_live_runs_or_force)` | the plist comes from the embedded template with only `bin`, `PATH` and `COS_BUSINESSES` substituted, and each is refused if it contains `<`, `&` or a newline; the daemon answered `drained`, or nothing listens (or it does not know `drain`) and no record names a live daemon run; any other socket failure refuses; `force` only adds `stop` for the runs `drain` named, then drains again; a second drain without `force`, under the same rules, just before the plist write and `bootout`; after `bootout` the pid that drain replied with (`{"drained":true,"pid"}`) no longer answers `hello`, else no `bootstrap` and `undrain` |
| `run.drain` | `VERIFY(peer_is_owner)`, `VERIFY(request_admissible)` | the line is exactly `{"op":"drain"}`; no run is live, counted under the runs lock that `start` also holds |
| `run.undrain` | `VERIFY(peer_is_owner)`, `VERIFY(request_admissible)` | the line is exactly `{"op":"undrain"}` |
| `run.start` | `VERIFY(peer_is_owner)`, `VERIFY(request_admissible)` | `getpeereid` uid equals the daemon's; the line parses against the binding; the daemon is not draining or stopping; `business` is registered and `business::check_dirs` passes; caps (2 asks per business, 4 runs in total); a brief while the lock is held returns the holder. A `cos mcp` owning its runs checks the same caps over its own runs |
| `run.stop` | same two | the record exists under that business's home and its owner is `daemon` |
| `group.kill` | `VERIFY(group_identity)` | the group is alive, and its leader is either alive with the recorded `start` or gone (**Group identity**); checked immediately before the kill, under the record lock |
| `run_record` | `VERIFY(request_admissible)` for daemon writes; the binding for every write | the id matches the pattern; the path is `<registered home>/out/run-records/<id>.json`; the file name equals the id |
| `registry.set_active` | `business::set_active`'s check | name registered |
| `window.navigate` | `VERIFY(link_grammar)` | exactly one of the three forms; the business is registered; the record or the brief file exists; a pinned window refuses the switch |
| `hire.spawn` | `VERIFY(business_admissible)` and today's `hire::plan` checks | the per-call business passes the C table and `check_dirs`; `hire::plan` refusals unchanged |
| `hire.follow` | today's checks | the `hire` label read back before every `hc` write; `judge.lock`; checks under `judge::BUDGET` |
| remote MCP | `allowed()` plus the remote argument check | `REMOTE_TOOLS` stays `["latest_brief"]`; undeclared arguments are refused; `Mode::Remote` builds no daemon client |

### Violations

| Violation | Evidence recorded | Recovery |
|---|---|---|
| config parse failure, or file changed mid-write | file, error | refuse; status red with the error; file untouched |
| socket peer uid mismatch | peer uid | close; one `daemon.log` line |
| drain while a run is live | each live run and its business | `drain` fails and changes nothing; install exits 75 naming them |
| start on a drained daemon | — | `{"error"}` naming the drain; nothing spawned |
| `drain` unanswered or refused for another reason | the socket error or the refusal | install fails with it; no plist, no `launchctl` call |
| the drained daemon still answers after `bootout` | its pid | install fails naming it, no `bootstrap`; `undrain` sent |
| request outside the binding | the op, the field | `{"error"}` reply; nothing spawned |
| unregistered or pinned-away business | the name; the known names or the pin | named error |
| link outside the grammar | the URL, truncated to 200 chars | ignored; stderr line |
| run past its deadline | record `failed`, reason `deadline` | groups killed and reaped |
| owner gone | record `lost`, reason `owner gone` | the child kills its groups and exits |
| daemon killed | records `lost`, reason `daemon restarted` at the next start | groups swept |
| group identity mismatch | pgid, recorded start, actual start | not killed |

### ISA test matrix

| Sink | Refused | Admitted |
|---|---|---|
| `config.write` | malformed JSON or TOML untouched, error names it; relative `bin`, or one whose `--version` is not `cos`; a file changed between read and rename (stand-in writer); a second write leaves `.cos.orig` as the first original | a fixture gains `cos`; other entries value-equal (JSON) or text-equal (TOML) |
| `launchd.install` | a live daemon run without `force`: exit 75, no plist, no `launchctl` call, the run still live; a run still in its spawn gate (`COS_GATE_HOLD_MS`) while install runs: exit 75, the run still live; a `start` sent between the drain and the `bootout` (the stand-in `launchctl` sends it at `bootout`), after a plain install and after `--force` on an idle daemon: refused naming the drain, nothing spawned; a daemon that answers `unknown op "drain"` first and names a live run at the second drain: exit 75, no plist, no `launchctl` call; no daemon answering and a record naming a live daemon run: exit 75 with and without `--force`; a daemon that accepts and never answers `drain`: an error, not 75, and no `launchctl` call; a drained daemon that `bootout` leaves running: an error naming its pid, no `bootstrap`, and a later `start` admitted; a `bin` containing `<` | plist rendered into a temp `LaunchAgents`; the stand-in `launchctl` records `bootout`, `bootstrap`; `--force` stops the live run, drains and installs, and a `start` at `bootout` is refused; a daemon answering `unknown op "drain"` with no live record: installed |
| `run.drain` | a live run: error naming it in `live`, the run untouched, a later `start` still admitted; `drain` with an extra field | an idle daemon answers `drained` with its pid; every later `brief` and `ask` start is refused naming the drain and spawns nothing; `hello` still answers; an executable change does not retire it until `undrain`, then it exits 1 |
| `run.undrain` | `undrain` with an extra field | after `drain` then `undrain`, a `start` is admitted |
| `run.start` | unknown op, extra field, `question` on a brief, `ask` without one, unregistered business, over the cap, a drained daemon, a peer uid other than the daemon's (injected uid); with no daemon, a `cos mcp` over the same caps on its own runs (third ask of a business, fifth run) | stand-in child: `queued` → `running` → `done`; a second brief start returns the first run with `existing: true` |
| daemon log | — | a started daemon's `run <id> started` line is in `<daemon dir>/daemon.log` (0600), not on the stderr it inherited |
| `RunView.link` | a `brief` or `ask` on a server pinned to unregistered directories: no `link` in the reply, in `run_status` or in `runs`; the record still says `explicit` | a registered business's run carries `cos://run/<business>/<id>`, through the daemon and when `cos mcp` owns it |
| `run.stop` | a run under another business's home; a run whose owner is not `daemon` (daemon), or not this app (`run_stop`: `not stoppable here`) | every group in the record is gone, the grandchild's included (`group_alive` false), and the record is `stopped`; a stand-in that ignores SIGTERM is SIGKILLed after 5 s and still ends `stopped` |
| `group.kill` | a live group whose live leader's start differs is not killed | a matching group is killed; a group whose leader exited but a member lives is killed |
| record writes | a reader that sees a dead pid re-reads and shows `done` when the run wrote it; a reader writes nothing; `stopped` after `done` leaves `done` | allowed transitions under the lock |
| run child death | — | SIGKILL the stand-in run child with its owner alive: the owner kills the grandchild group and writes `failed` (`signal 9`) |
| terminal Ctrl-C | — | SIGINT to a `cos brief` with `fake-claude.sh` (`tests/run_process.rs`): the fake claude's group is gone, the record is `stopped` |
| owner death | — | SIGKILL the stand-in owner: the child kills its grandchild's group, writes `lost`, exits 76 |
| daemon death | — | a stand-in child started with no `COS_OWNER` (so no watchdog) under a daemon record, plus a judge-style `sh` child of the daemon; SIGKILL the daemon, restart it: the sweep kills the run's groups and the `sh` group, the record is `lost`. SIGTERM instead gives `stopped` |
| brief lock | a CLI brief while a daemon brief holds the lock exits 73 and names the run | after a SIGKILLed self-owned brief whose grandchild lives, the next brief kills that group, writes the old record `lost`, then runs |
| normal exit | — | a run that exits 0 while a grandchild group lives: the group is gone before `done` is written |
| spawn gate | a run SIGKILLed after `spawn` and before its record write: the grandchild never execs | a gated child execs once its `groups` entry is written |
| start sweep | — | after a SIGKILLed self-owned `cos ask` whose grandchild lives, the next run start for that business kills the group and writes the old record `lost` |
| gone | a group still alive at the 2 s bound: no final state is written, the record stays for the next sweep | a group whose pgid's live leader has another start counts as gone |
| `window.navigate` | `cos://run/../x`, unknown host, extra segment, query, fragment, percent-encoding, unregistered business, missing record, a switch on a pinned window | a valid link switches and shows the run (mock runtime); a cold-start link is applied after ready |
| `hire.spawn` | a per-call business on a server pinned elsewhere; an unregistered one | a registered business plans on its own home |
| remote | `brief`, `run_status`, `businesses` over HTTP: `-32602`; `latest_brief` with `business`: `-32602`; `Mode::Remote` has no daemon client | `latest_brief` with no arguments |

## Security and privacy

- Reads: client config files, the registry, run records, briefs.
- Writes: the inventory. Nothing new leaves the machine; paid runs and hires
  call the same services as today.
- The socket is a capability to spend money. The 0700 directory, the 0600
  socket and the peer uid check bound it to the user's own processes, which
  can already run `cos brief`.
- Any web page can open `cos://`. Links only navigate, in a closed grammar.
  A link-triggered switch is window-only and visible in the header.
- Config writes touch files holding other servers' secrets (env values).
  `.cos.orig` keeps the original's mode, and `cos` never prints file
  contents.

## Testing

Stand-ins are Rust, in a test-only binary target `cos-stand-in`
(`tests/bin/stand_in.rs`, outside Cargo's test auto-discovery, behind the
`stand-in` feature so `cargo install` leaves it out), chosen by its first
argument:

- `run`: a fake run child that writes its record (honouring `COS_RUN_RECORD`
  and `COS_OWNER`), optionally sleeps, spawns a grandchild group with null stdio
  (to prove group kills reach it), and exits with a chosen code. SIGTERM,
  SIGINT and SIGHUP run Self-cleanup with the record's `stopping`, else
  `stopped`, and exit 143;
  `COS_IGNORE_TERM=1` makes it ignore SIGTERM, for the 5 s SIGKILL path. Any
  first argument other than a mode name is read as the run child's argv
  (`--home <h> … brief|ask -- <q>`), so it serves as `COS_RUN_BIN` unchanged:
  kind from the subcommand, home from `--home`, business from the owner's
  `queued` record.
- `launchctl`: records its argv to `COS_STAND_IN_LOG`;
  `COS_STAND_IN_FAIL=<verb>` makes that verb fail.
  `COS_STAND_IN_BOOTOUT=<business>` makes `bootout` act as launchd does on
  the daemon in `COS_DAEMON_DIR`: it first sends a `start` of a brief of that
  business and logs the reply as `start <business>: <reply>`, then SIGTERMs
  the daemon and waits until its socket stops answering. Invoked through a link
  whose basename is `launchctl`, it acts as `launchctl` whatever its
  arguments, so `COS_LAUNCHCTL` points at the link.
- `version` (or `--version`): prints a chosen version.

The run mode reads `COS_SLEEP_MS`, `COS_GRANDCHILD_SECS`, `COS_EXIT_CODE`,
`COS_IGNORE_TERM`, `COS_STAND_IN_NO_WATCHDOG`, and `COS_STAND_IN_DAY` and
`COS_STAND_IN_COST` (a done brief's `day` and `cost_usd`).

Environment seams in `cos` itself, each a test stand-in and never set in
production:

| Variable | Replaces |
|---|---|
| `COS_RUN_BIN` | the run program the daemon, `cos mcp` and the app spawn (default: the running `cos`) |
| `COS_CLAUDE` | the `claude` binary a run calls (`tests/run_process.rs` uses `fake-claude.sh`) |
| `COS_PMSET` | the `pmset` a run reads its wake state from (`tests/run_process.rs` uses `pmset-awake.sh`, so a test never defers on the machine's own sleep) |
| `COS_GATE_HOLD_MS` | holds the newest gated child before its group is recorded |
| `COS_DAEMON_DIR` | the socket directory, and so `daemon.log` |
| `COS_DAEMON_DEADLINE_SECS`, `COS_DAEMON_UID` | the daemon's run deadline (only lower) and admitted uid |
| `COS_LAUNCHCTL`, `COS_LAUNCH_AGENTS` | `/bin/launchctl` and `~/Library/LaunchAgents` |

The existing shell fixtures (`fake-hc.sh`, `fake-claude.sh`) stay as they
are. Single-instance forwarding of a link to a running app is not driven by
a test; it is a hand-check row in the PR's verification.

| Layer | Where | What |
|---|---|---|
| Unit | `src/*/tests.rs` siblings | id and link parsers; binding rejection; config editors on fixtures; record transitions and the reader-side lost view; the C table; the socket path length check |
| Integration | `tests/daemon.rs` | daemon on a temp socket and temp registry: start, `existing`, `client_req` retry, stop, deadline, SIGTERM → `stopped`, SIGKILL + restart → sweep and `lost`, a second daemon exits 0, stale socket, exe change → exit 1, peer check, `drain`, `undrain`, no retirement while drained, `daemon.log` |
| Integration | `tests/daemon_install.rs` | install and uninstall on a temp `LaunchAgents` with the stand-in `launchctl`: refusals (exit 75 with a live run, with a live record and no daemon), a start at `bootout` refused, a mute daemon, a daemon without `drain`, a daemon `bootout` leaves running, `--force` |
| Integration | `tests/run_process.rs` | the real `cos brief` with `fake-claude.sh`: the record written, the brief lock refusing a second brief with 73, the owner watchdog |
| Integration | `tests/mcp_bridge.rs` | `cos mcp` over stdio against a temp daemon and with none: `brief` answers within 2 s with a link; `run_status` waits; the caps on runs `cos mcp` owns, and finished ones no longer counting; no link for an `explicit` or `default` triple; the C table; `businesses`; remote refusals; the MCP App view's call path (brief, then run_status, then the rendered report fields) |
| Integration | `tests/connect.rs` | `cos connect` with `HOME` and `CODEX_HOME` on temp dirs: status, register, repair, remove, refusals, `.cos.orig` kept |
| App | `apps/desktop/tests/ipc.rs` | `connect_status`/`connect_apply`; `business_make_default`; a stand-in daemon's subscribe stream emitting `brief-changed` and `report-changed`; `brief_run` with the daemon up sending `start`; `run_get` on a queued, a running, a lost-by-reader and a done record; `run_stop` |
| App | `apps/desktop/tests/links.rs` | accepted and refused links; cold start held until ready |
| Design gate | `tests/design.rs` | the card on `stateView` and tokens |
| End to end | by hand, recorded in the PR | the acceptance below with the real clients |

No test touches the real `~/Library`, `~/.codex`, launchd or Herdr.

## Confidence

| Piece | Tier | What moves it up |
|---|---|---|
| D `cos connect` + card | rock | — |
| C business per call | rock | — |
| Run records, brief lock | rock | — |
| Owner watchdog and the pgid registry in `proc` | sand | the `tests/run_process.rs` owner-death case green |
| A daemon core (socket, start, stop, sweep, deadline) | water | the first Build bead is a spike: the daemon skeleton, one `start` round trip from `cos mcp`, SIGKILL and restart with a run in flight, recorded under `docs/plans/` |
| B run tools | sand (rests on A) | A's spike |
| A launchd install and exe-change restart | sand | the stand-in `launchctl` test, then one real install by hand |
| E links in the app, cold start | sand | the plugins under `"create": false` in `apps/desktop/tests/links.rs` |
| E links clickable in Claude Desktop | sand | one real chat |

## Acceptance

1. After `cos connect claude-desktop` and `cos connect codex`,
   `cos connect status` shows both clients green, and both `.cos.orig` files
   exist.
2. With the daemon loaded, `brief` in Claude Desktop answers within 2 s with a
   run id and a `cos://run/…` link, and the app shows the run as running
   without a reload.
3. Clicking the link opens the app on that run, including when the app was
   closed.
4. `run_status` with `wait_secs: 45` returns the brief once it is done.
5. `cos brief` in a terminal while that run is live exits 73 and names it.
6. Switch the app to business B and choose Make default: `businesses` in
   Codex shows B as default, and `latest_brief` with no argument reads B.
7. `launchctl bootout` of the daemon mid-run gives `stopped`. `kill -9` of the
   daemon mid-run, then a restart, gives `lost`. In both cases, every group in
   the run's `groups`, the `claude` group included, is gone (`ps -g <pgid>`
   prints nothing for each). After `kill -9` of a run process its groups are
   gone at once when its owner outlives it (daemon, app, `cos mcp`), and at
   the next run start for that business when it was self-owned (a terminal
   or launchd). Ctrl-C on a `cos brief` in a terminal leaves no group behind.
8. Over the tunnel, `brief` and `latest_brief` with `business` are refused,
   and `latest_brief` works.
9. `task test` is green.

## Review history

- Round 1 (design sha256 `dde02984…`): reviews
  [a](2026-11-25-mcp-client-bridge-design-review-a-r1.md) and
  [b](2026-11-25-mcp-client-bridge-design-review-b-r1.md), both `findings`
  (10 blocking, 35 should, 13 nit together). This revision answers each.
- Round 2 (sha256 `e344c79c…`): reviews
  [a](2026-11-25-mcp-client-bridge-design-review-a-r2.md) and
  [b](2026-11-25-mcp-client-bridge-design-review-b-r2.md), both `findings`.
  Blocking: grandchild groups unowned (both), readers writing `lost` (a),
  no app view for a run link (b), hire bindings not closed (a). Answered
  here. Their `Carried to Plan` items go to the Plan.
- Round 3 (sha256 `8738f295…`): reviews
  [a](2026-11-25-mcp-client-bridge-design-review-a-r3.md) and
  [b](2026-11-25-mcp-client-bridge-design-review-b-r3.md), both `findings`
  (8 blocking; 6 are the run's groups surviving a run that died without
  cleanup). The owner asked for one more revision. It is built on **The
  invariant**: owner reaping on any exit, signals, self-cleanup order, group
  identity with a dead leader, the brief lock sweeping a dead holder, the
  daemon's own groups, Stop by owner only, readers re-reading, `$defs`.
- Round 4 (sha256 `c1a65677…`): reviews
  [a](2026-11-25-mcp-client-bridge-design-review-a.md) and
  [b](2026-11-25-mcp-client-bridge-design-review-b.md). The owner approved it
  with their open blocking findings carried to the Plan as the cleanup spike.
- Cleanup spike ([report](2026-11-25-mcp-client-bridge-cleanup-spike.md)):
  the group list stands. The owner approved its five text changes: cleanup
  before every final write, **gone** defined with a bounded wait, the spawn
  gate, the start sweep covering a dead `ask`, and Acceptance 7 naming who
  cleans up.
- Round 5: build findings. Owner decisions of 2026-11-27, built and written
  in:
  - The daemon reopens its stderr onto `<daemon dir>/daemon.log`.
  - New `drain` request; `cos daemon install` drains before it writes, so a
    racing start is refused, never killed.
  - `drain` replies with the daemon's pid; install fails, without
    `bootstrap`, when that pid still answers after `bootout`.
  - New `undrain` request, sent by install on any failure after a drain.
  - Install drains a second time, without `--force`, just before the plist
    write and `bootout`.
  - A drained daemon does not retire on an executable change.
  - Install falls back to the records only when nothing listens or the
    daemon does not know `drain`; a timeout or other socket error fails it.
  - `--force` stops only the runs `drain` named; with no daemon answering it
    refuses with exit 75 like a plain install.
  - The stand-in `launchctl` gains `COS_STAND_IN_BOOTOUT`.
  - A `cos mcp` owning its runs applies the daemon's caps.
  - `RunView.link` is omitted for an unregistered triple; tool descriptions
    say the link comes for a registered business; the text content does not
    repeat it.
  - Stated as built: the spawn gate's helper thread; macOS-only runs; `stop`
    erroring past the gone bound; the environment seams and stand-in modes;
    the `default` / `explicit` labels; install's exit 75 and live-run test;
    `install-mcp` through `cos connect --repair`; `run_status` taking
    `RunView.business`; the run-ends note in tool descriptions;
    `brief-changed` for asks, changes-only subscribe, reconnect re-read,
    `report-changed {run}`; Make default and the badge; the app following
    its own unregistered home; the Settings gear and card behaviour;
    `.cos.orig` only for a file that existed; `link_ready`; marked and
    DOMPurify for answers; `run_get`'s extra fields; `/brief/<day>`
    resolution; plugin URL normalisation; single-instance as a hand check;
    the `lsregister` match; links switching only via `switch_business`; the
    judge verdict dropped after tracking closes.
