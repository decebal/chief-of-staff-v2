# MCP client bridge: run process cleanup spike

Story US-001 of the [plan](2026-11-25-mcp-client-bridge-plan.md). Design:
[2026-11-25-mcp-client-bridge-design.md](2026-11-25-mcp-client-bridge-design.md)
(sha256 `c1a65677958bcc6f2773c25fe0f1d96b0933bd363442c0b02c8a3b0c711c0476`).
Findings resolved: review a New 1 to 3 and review b New 1, against `src/proc.rs`
on `feat/mcp-client-bridge` at `a05a730`.

## Result

The design's group list stands as the mechanism. The design text does not stand
as written: the normal-exit path, the definition of "gone", and the
spawn-to-record window each need a change (below). Those changes alter the run
process, reaping, sweep and record text, so the flow returns to Spec. No
implementation is committed. Owner approval is needed for the changes in
"Design text changes".

## Question

Two mechanisms can make "every group a run created is gone before its final
state is written" hold.

- **A. Grandchildren in the run's own group.** No `process_group(0)` for
  children a run spawns. The run's pgid is its pid, which the owner already
  knows. Owners kill `-pid` and reach every grandchild, recorded or not.
- **B. Groups list (the design).** Each grandchild keeps its own group. The
  record lists `{pgid, start}` for each; owners and sweeps kill each listed group.

## Code facts (current `src/proc.rs`)

The review cites lines from an earlier revision (`proc.rs:166-170`,
`:229-233`, `:343-346`, `:235-237`). The current locations:

| Item | Line |
|---|---|
| `PIPE_GRACE` = 2 s | 96 |
| `run`: spawn, drain, wait | 105-117 |
| `run_input`: `process_group(0)`, group kill on pipe timeout | 120-161 |
| `stream_lines_on`: `kill_and_reap` on deadline or error; returns on stdout EOF without killing the group | 174-206 |
| `spawn_grouped`: `process_group(0)`, stdin null | 210-214 |
| `Drain::finish`: kills the group only when a pipe is still open after 2 s | 269-275 |
| `wait_until` | 278-289 |
| `kill_and_reap`: kill group, kill child, wait | 291-295 |
| `kill_group`: `kill(-pgid, SIGKILL)`, guarded on `pgid > 0` | 300-307 |
| `start_time` (macOS `proc_pidinfo`; `None` elsewhere) | 348-372 |
| `group_alive`: `kill(-pgid, 0)`, true while any member lives | 405-408 |

Facts the choice depends on:

1. `stream_lines_on` returns once stdout closes and `wait_until` returns. It does
   not touch members that hold neither stdout nor stderr (`:204-205`). A member
   of `claude` that is not a pipe holder survives the return.
2. `Drain::finish` kills the group only when a pipe stays open past
   `PIPE_GRACE`. A member that does not hold a pipe is never killed here.
3. `kill_group(pgid)` sends SIGKILL to the whole group, including the caller
   when the caller is in that group.

## Measurements

Throwaway shell, in the scratchpad, not in the repo. The leader is a job-control
shell (`set -m`, own group, like a run process). It starts one grandchild that
gets its own group, and exits 0 at once while the grandchild lives. The wrapper
is the leader's parent and is not part of the tree being measured.

The two throwaway scripts (`leader.sh`, `wrapper.sh`) are not committed. The
wrapper runs the leader under `sh`, waits for it, then probes. Output:

```
leader pid=68689 exited with status 0
grandchild pid=68690
ps after leader exit:
  PID  PPID  PGID STAT COMMAND
68690     1 68690 R    sleep 300
pgrep -P LP (children of exited leader):
exit=1
pgrep -g GP (group members):
68690
exit=0
/bin/kill -0 -- -GP:
exit=0
SIGKILL -- -GP exit=0
pgrep -g GP after kill:
exit=1
```

What this shows:

- The grandchild outlives its leader. Its ppid is 1 and its pgid is its own pid.
- A pid-tree walk from the exited leader finds nothing (`pgrep -P`, exit 1).
  The grandchild is still a member of its group.
- `kill -0 -- -pgid` reports the group alive while a member lives. The group
  probe finds what the pid walk misses.
- `kill -KILL -- -pgid` clears the group.

Earlier runs of a single-leader script were not clean. The grandchild was
still alive when probed, then gone at the next tool call, and the cause is not
known. The wrapper run above does all probes inside one process, so its output
is the one relied on.

Not measured: a grandchild that calls `setsid` or `setpgid` leaves the group and
escapes both A and B. A zombie leader's `start_time` on macOS (`proc.rs:348-367`)
is still unverified.

## Decision: B, with five design text changes

Why not A.

- The run process must kill its own grandchildren before it writes `done`
  (review a New 1). Under A every grandchild shares the run's pgid, so the run
  process cannot send `kill(-pgid)` without killing itself. It would have to walk
  the pid tree. Measurement above: the pid tree misses an orphaned member. Under
  A the run process's normal-exit kill is therefore unsound.
- Owner-side `kill(-pid)` works under A for owners that outlive the run
  (`daemon`, `app`, `mcp`). A self-owned run (terminal, launchd) has no owner
  that outlives it, so A gives no cleanup for it either.

B keeps the group kill that reaches orphans. Its two defects are fixable in the
design text and in code (below). A keeps one defect that the group kill cannot
reach: the run process killing members from inside.

## Design text changes

Each change alters the design text, so this is the change set the owner must
approve. Line numbers refer to the design at the sha above.

**C1. Self-cleanup runs on every exit of the run process** (design 105-112, "Run
process" items 5 and 6). Today it runs on watchdog and signals only. New: the run
process runs Self-cleanup before every final write, on normal exit, `deferred`,
error, signal and watchdog. Self-cleanup kills every group in the set except its
own, waits for each to be `gone` (C2), and only then writes the final state.
Replaces "The process exits by itself, so the final write always happens before
its group could be killed" with the ordering above. Reason: review a New 1,
review b R3 item 1.

**C2. "Gone" is defined, and every writer waits with a bound** (design 142-143,
155-157, 170-171, 191-194). Define:

`gone(g) = !group_alive(g.pgid) || (leader_start(g.pgid) is Some and != g.start)`

The second clause covers a freed pgid reused by an unrelated group. A pgid stays
reserved while any member lives (`proc.rs:297-299`), so a new leader with that pid
cannot appear while members of the recorded group remain. Every writer (Reaping,
Self-cleanup, the brief-lock sweep, the daemon sweep) waits up to 2 s for `gone`
on each recorded group. If a group is still not gone at the bound, the writer
does not write a final state. It logs, leaves the record non-final, and the next
owner tick, brief start or daemon start retries.

**C3. Spawn-to-record window closed by a gate** (design 97-100). Replaces "a run
process SIGKILLed inside it leaves that grandchild out of the record" with:

- A grandchild spawned by a run goes through a new `proc::spawn_gated`. The
  child closes the write end of a pipe in `pre_exec` and blocks reading the read
  end before `exec`.
- The run process writes the `groups` entry for the child, then writes one byte
  to the pipe and closes its write end. The child then execs.
- If the record write fails, the run process kills the group instead of releasing
  it.
- If the run process dies first, the write end closes, the child's read returns
  EOF, and the child exits before exec. No grandchild does work outside a record.

Invariant (design 142-143) then reads: every group that has run work is in a
record, and every recorded group is gone before a final state is written. The
run's own group is excepted: only the run process is in it, and it writes its
final state before it exits.

Risk: `pre_exec` is `unsafe` and runs after fork. Only `close` and `read` are
called there, both async-signal-safe. Another thread's `fork` between `pipe` and
`FD_CLOEXEC` could leak the write end into that child. The run process spawns from
one thread, which has to be confirmed in code before Build.

**C4. Sweep covers self-owned `ask`** (design 85-86, 189-190, 270; review b New 1,
the recommended fix). Every `cos brief` and `cos ask` start, and every daemon
start, sweeps the business's non-final records whose `pid`/`start` is dead:
kills their groups under C2 and writes `lost` (reason `previous run died`) under
the record lock. `ask` then does its own work. Without this, a `kill -9` of a
terminal `cos ask` leaves a `running` record and a live `claude` group that no
writer reaches. Also fixes the "ask takes no lock" versus "take the brief lock"
contradiction at design 85-86 and 270: the sweep is per business and does not
take the brief lock.

**C5. Acceptance 7 names owners** (design 773-777, 697). After `kill -9` of a run
process, its groups are gone at the next sweep for the business (C4), or at once
for an owner that outlives it. The row no longer claims "gone" for a self-owned
run without a next start.

Plan impact: US-001's four tests stay as written. The window test asserts that a
SIGKILL before the record write leaves no grandchild exec. The orphan `ask` test
asserts the sweep under C4.

## Not decided here

- Whether `proc::run` should kill its own leftover members on normal return. C1
  covers the run process. `proc::run` callers that hold no record are out of this
  change.
- The daemon's judge groups after SIGTERM and `bootout` (review b carry, daemon
  item 6). Separate change to the daemon section.
- Records written by existing hires (`pgid_started` as `lstart`, review a carry).

## Outcome

The owner approved C1 to C5 on 2026-11-26. The design holds them at sha256
`7f9316ae2ff6b6886f028aae983ac3c44c9aa71ea97428fa07a5719fe087b2b9`, and US-001
is built against that text.
