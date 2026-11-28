---
design: docs/plans/2026-11-25-mcp-client-bridge-design.md
design-sha256: superseded (was 8738f2956e1c847742055e4aab88066fc55c84c7e7fa381e7b1cf5d418980825)
verdict: findings
---

Round 3 (final). Checked against the revised design and the live code. Previous review: [r2](2026-11-25-mcp-client-bridge-design-review-a-r2.md). Blocking open: 4 new (B1 to B4). Open earlier: 1 (18, partly). Closed: 19, and the write half of N2.

## Earlier findings

1. open (18, partly): `run.stop` output enum is `stopped|done|failed|deferred` (design:482), but `lost` is a final state of the record enum (design:498). A stop on a run the sweep already marked `lost` cannot produce a valid reply. Closed parts: (a) `run.start` oneOf (design:463-475); (b) `registry.set_active` and `launchd.install` bindings (design:532-545); `hire.spawn` and `hire.follow` now have closed bindings (design:561-585), and `hire.spawn` matches `SpinOffArgs` (src/mcp.rs:63-84: `from`, `item`, `harness`, `checks`, `scope`, `role`). `binary.version` now has checks in its VERIFY row (design:606).
2. closed (19): readers never write (design:141-145); `lost` written only by watchdog and sweep. The read-side race that remains is B3.
3. open (N1, round 2): grandchild groups still unowned after the child's death. Revision covers owner-driven stop, deadline, daemon SIGKILL and bootout (design:134-137, 163-166, 620-629, acceptance 7). Not covered: run child SIGKILL, Ctrl-C of `cos brief`, and groups whose leader was reaped. See B1 and B2.
4. closed (N2, write half): readers no longer write (design:141-145). The view race is B3.

## New findings

1. blocking §Run process 1, 3-5; §Stopping a run; §Violations; acceptance 7: a run child that dies without running its own cleanup leaves `claude` alive and frees the brief lock. SIGKILL of the run child, SIGINT/SIGHUP (Ctrl-C of a terminal `cos brief`, which is owner `self` with no watchdog), and any exit that skips the handler all leave the group running. The brief lock is held by the child's fd, so it is released at exit (design:82-86, 63-64). A second paid brief then starts for the same business. Evidence: design:102-104 (SIGTERM only), design:134-137 (owner acts only on stop/deadline), design:141-145 and 620-629 (no child-exit row), acceptance 7 (only bootout and daemon kill), src/claude.rs:198 (`claude` in its own group), src/pipeline.rs:703-708 (today kills only the brief child's group). Fix: (a) the owner, on reaping the child for any reason, kills every group in the record and writes the final state under the record lock (`lost`, reason `exit <n>` or `killed`), for `daemon`, `app`, `mcp`; (b) the run process treats SIGINT and SIGHUP like SIGTERM; (c) `cos brief` takes the brief lock, then under that lock checks that every group in the previous holder's record is dead, and refuses (exit 73) if not. (c) covers `self` runs and SIGKILL, where no handler runs. Add acceptance rows: SIGKILL of the run child, and Ctrl-C of a terminal brief, then the grandchild is gone and a second brief refuses while it lives.

2. blocking §Run process 3; §Stopping a run; §Semantic ISA `VERIFY(group_identity)`: a group leaves the process-wide set when its leader is reaped (design:92-94 "remove it after reaping"), and `group.kill` requires the leader's start to match (design:610). A group whose leader has exited but whose members live is then unreachable by every owner path. Example: `claude` exits or is killed with an MCP server or tool child still in its group. The design's claim "every other path reaches every group" is false for that case. Evidence: design:92-94, 134-137, 610; src/proc.rs:343-346 (`group_alive` checks any member), src/proc.rs:229-233 (`kill_and_reap` kills the group before it reaps the leader). Fix: remove a group from the set only when `group_alive(pgid)` is false, not on reap. Keep an unreaped leader (zombie) until its group is killed, so its start still matches; state the rule for a leader that is gone (members keep the pgid reserved, so a live group with a dead leader is killable only while its membership is non-empty). Confirm `proc::start_time` reads a zombie on macOS before relying on it.

3. blocking §Run record "Readers never write" (design:141-145) and §Clients `run_status` (design:250): a reader reads `running`, then the run child writes `done` and exits, then the reader's pid check finds the pid dead, and the reader reports `lost`. `run_status` treats `lost` as a leaving state (design:250), so it returns `lost` for a completed brief. Acceptance 4 (design:716) fails in that window. Evidence: design:141-143, 250, 716. Fix: when the pid is dead, re-read the record before classifying; report `lost` only if the re-read still shows `queued` or `running`. Apply the same rule to `runs`, `run_get`, and the subscribe stream. Add a stand-in test that finishes the run between the two reads.

4. blocking §Semantic ISA "Bindings": the bindings reference `"$ref":"#business"` and `"$ref":"#run"` (design:467, 470, 480, 497, 540, 542, 551, 553, 564) and the document defines no `$defs` or anchors, so no validator can resolve them and the closed bindings are not machine-checkable. Also `run.stop` output enum lacks `lost` (design:482), see Earlier 1. Fix: add a `$defs` block (`business` pattern `^[a-z0-9][a-z0-9-]{0,39}$`, `run` pattern `^r-[0-9]{8}-[0-9]{6}-[a-z0-9]{4}$`) at the top of the Bindings section, reference it as `#/$defs/...`, and add `lost` to the `run.stop` enum.

## Carried to Plan

- should: `binary.version` `bin` comes from client config contents and is validated only as `^/` (design:524-529, 606). Require a known install location or the path already registered for that client. The design's taint list already names config contents (design:591). Not a new capability, since Claude Desktop runs the same command at launch.
- should: after 5 s without exit, `Stopping a run` does not say SIGKILL to the run child; state it (design:134-136). Also state the stop of a `queued` record that has no pid.
- should: a refused `running` write (record already `lost` after a sweep) must make the run child exit; design:87-90 is silent.
- should: `Violations` "daemon killed" gives reason `daemon restarted` at next start (design:629), but the watchdog writes `owner gone` at once (design:100-101, 628). Pick one reason per path. Acceptance 7 accepts either `lost` value.
- should: `start` format `"<sec>.<usec>"` (design:66, 115): `proc::start_time` returns `Option<(i64, i64)>` (src/proc.rs:287). Name the formatter and zero-pad usec to 6 digits, or the same process can write two strings.
- should: S7 (socket, `daemon.lock`, stale socket unlink) is attributed to `launchd.install` (design:414, 418), but the daemon's bind does it, and no instruction row covers the unlink. Add `TOOL_CALL(daemon.bind)` or attribute S7 to the daemon start.
- should: remote MCP App view. The view's "Run new brief" calls `brief` and `run_status` (design:255-259); remote refuses both (design:271, 616). State what the remote view shows.
- should: the test matrix says `brief` over HTTP is refused with `-32602` (design:647). The code returns `-32602` for any non-remote tool (src/mcp.rs:417-420), so this matches; state it as the expected error text.
- should: `cos brief` in a terminal has no deadline stated (owner `self`, design:61-62, 232). Say whether the 420 s budget applies.
- nit: the `"<sec>.<usec>"` format removes the colon ambiguity from round 2 (`COS_OWNER` uses `/`, design:98, 199). Closed.
- nit: config write re-read to rename window (design:328-330) is a residual race. Say the residual is accepted.

## Remote read-only guarantee

Holds in code and in the design. `allowed()` returns true only for `Mode::Local` or names in `REMOTE_TOOLS` (src/mcp.rs:375-377). `call_tool` checks it before dispatch (src/mcp.rs:417-422). `list_tools` filters the same way (src/mcp.rs:407). The design keeps `REMOTE_TOOLS` as `["latest_brief"]` (design:279, 616) and adds no remote resource for run records. Not blocking.

## Counts

- Earlier open: 1 (18, partly). Earlier closed in this round: 19, N2 (write half). N1 open.
- New blocking: 4 (B1 to B4).
- Carried to Plan: 9 should, 2 nit.
