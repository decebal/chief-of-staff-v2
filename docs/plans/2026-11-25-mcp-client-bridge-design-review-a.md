---
design: docs/plans/2026-11-25-mcp-client-bridge-design.md
design-sha256: c1a65677958bcc6f2773c25fe0f1d96b0933bd363442c0b02c8a3b0c711c0476
verdict: findings
---

Final check against round 3 (`design-review-a-r3.md`). Scope: its blocking items (B1 to B4 and Earlier 1 and 3). Line numbers are design lines unless marked as code.

## Earlier findings

1. closed: `run.stop` output enum now includes `lost` (design:533). The other parts of R3 Earlier 1 were closed in R3 and not re-read here.
2. open (R3 Earlier 3, N1, grandchild groups unowned): the owner-driven, signal, deadline and watchdog paths now cover the run child's death. Still open for the spawn-to-record window (New 3) and for the normal exit path (New 1).
3. closed with a carry (R3 B1, run child death and Ctrl-C): owner reaps on every exit (design:153-159). SIGINT and SIGHUP run Self-cleanup (design:105-107). The brief lock sweep covers a self-owned SIGKILL on the next brief (design:167-173). Tests at design:693-697. Carry: Acceptance 7 (design:773-777) says groups are gone after `kill -9` of the run process, which a self-owned run does not give until the next brief (see Carried). Gate-level: the design now kills and proceeds instead of refusing with 73 (design:167-173). Acceptable if New 2 is fixed.
4. closed (R3 B2, leader reaped while members live): a group leaves the set only when `group_alive` is false (design:92-96). `group_alive` is true while any member lives (proc.rs:343-346), and the pgid stays reserved (proc.rs:235-237). `kill_and_reap` kills the group before reaping (proc.rs:229-233). Carry: the zombie `start_time` read is still unverified on macOS.
5. closed (R3 B3, reader sees dead pid after completion): readers re-read before showing `lost` (design:175-180). Test at design:692. A run that writes `done` before exiting is final before its pid dies, so the re-read sees it.
6. closed (R3 B4, unresolved `$ref`, `run.stop` enum): `$defs` added (design:508-511). Every `$ref` now reads `#/$defs/...`. The `business` pattern matches `valid_name` (src/business.rs:68-73). Carry: the `$defs` must be merged into each snippet for a validator.

## New findings

1. blocking: normal exit writes the final state while grandchild groups may live. Invariant (design:142-143) is "every group a run created is gone before anyone writes the run's final state, whatever ended the run".
   Evidence: the run writes `done`/`failed`/`deferred` "at exit" (design:87-90). Self-cleanup, the only place a run kills its groups, runs only on watchdog or signal (design:105-112). Reaping (design:157-159) says "a run that wrote `done` before exiting keeps `done`, and its groups are already gone". Nothing in the normal path makes that true. Code: `proc::Drain::finish` kills the group only when the pipe is still open after `PIPE_GRACE` (proc.rs:207-213). `claude::execute` uses `stream_lines`, which drains stderr only (claude.rs:198-199, proc.rs:160-161). A leftover member (an MCP child, `git`, `sh`) that holds neither pipe survives the child's exit. The design's own B2 case shows such members exist. The invariant also cannot hold for the run's own group: the run process writes its final state before it exits (design:110-112, "the final write always happens before its group could be killed"). The wording must exclude that group.
   Fix: on every exit of the run process (normal, error, `deferred`, signal), kill every set group except its own, wait for each to be gone (New 2), then write the final state. State the invariant for the run's own group as an exception: only the run process is in it.

2. blocking: "gone" is undefined, and two writers have no wait or no failure branch. Invariant is the same as New 1.
   Evidence: reaping waits "up to 2 s for `group_alive` to turn false. Then it writes the final state" (design:155-157). It does not say what happens when the group is still alive at 2 s. Self-cleanup has the same gap (design:110-111). The brief-lock sweep kills and writes `lost` with no wait (design:170-171). The daemon sweep also kills and writes with no wait (design:191-194). The identity rule never kills a group whose leader is alive with another start (design:150-151). Such a group's pgid was freed and reused, so `group_alive(pgid)` stays true because of an unrelated group, and the wait cannot turn false for it. Code: proc.rs:343-346 tests any member of the pgid, not the recorded group.
   Fix: define `gone(g)` as `!group_alive(g.pgid)`, or the leader is alive with a different start (the recorded group has ended). Every writer (reaping, self-cleanup, both sweeps) waits up to a stated bound for `gone` on each recorded group. If one is still ours and alive at the bound, the writer does not write a final state. It leaves the record non-final, logs, and retries on its next tick (owner) or next start (sweep).

3. blocking: the spawn-to-record window breaks the invariant. The design admits it (design:97-100).
   Evidence: "a run process SIGKILLed inside it leaves that grandchild out of the record". The invariant says "every path below holds it" (design:142-143). The group is then in no record, so the owner's reap (design:153-156), the brief-lock sweep (design:167-173) and the daemon sweep (design:189-198) never see it, and it lives until something else kills it. Code: `spawn_grouped` returns the `Child` with the group already formed (proc.rs:166-170). The record write comes later, in a separate step.
   Fix, pick one and state it in the design: (a) close the window. Under a run, grandchildren join the run's group (no `process_group(0)`), so the owner's `kill(-pid)` reaches them without a record. This needs a `proc` change, since `kill_group(child.id())` would then miss the child (proc.rs:229-233). (b) Keep the groups model, narrow the invariant to exclude the window, and add the window to Acceptance 7 as a known exception.

## Carried to Plan

- should: Acceptance 7 (design:773-777) says groups are gone after `kill -9` of the run process. For a self-owned run (terminal or launchd), nothing kills them until the next brief (design:167-173, matrix row design:697). Narrow the row to owner `daemon`, `app`, `mcp`, or add a start-time sweep of self-owned records with a dead pid.
- should: the R3 B1 request for "a second brief refuses while it lives" (exit 73) is replaced by kill-and-proceed (design:167-173, matrix row 697). Say which is the rule and keep one acceptance row for it.
- should: record `groups` has `maxItems: 64` (design:552-553). The set's prune trigger is not stated (design:94-96). State when pruning runs (each change) and what a write does when a 65th live group appears. A refused write leaves that group unrecorded, which is New 3.
- should: `start` and `pgid_started` patterns are `^[0-9]+\.[0-9]{1,6}$` (design:539, 551, 555). They accept `1764061702.48`. The code pads usec to 6 (src/agents/mod.rs:1108). Use `{6}`.
- should: reader liveness is by pid alone (design:175-178). Use pid and start, or a reused pid shows `running` forever.
- should: reader shows `lost` during the reap window for an abnormal exit, then the owner writes `failed` (design:157-158). Show `running` until the owner's final write, or state that `lost` is transient.
- should: a run whose `running` write is refused after a sweep must exit (R3 carry, design:87-90 silent).
- should: the identity rule's "accepted" case (design:147-150) kills an unrelated group when a pgid and pid are reused. Check the leader's exe too, or refuse.
- should: existing hire records store `pgid_started` as `ps -o lstart=` (src/agents/mod.rs:1119-1131), not `<sec>.<usec>`. Say whether hire records keep the old format and how a comparison reads it.
- should: `$defs` (design:508-511) must be merged at root for each snippet. Say so.
- should: `proc::start_time` for a zombie leader on macOS (proc.rs:286-305). Unverified. Still open from R3.
- should (R3 carries, not re-checked): `Stopping a run` SIGKILL wording (design:161-165); `daemon killed` reason vs watchdog reason (design:193, 680); S7 attribution (design:463, 485); remote view `brief`/`run_status` (design:293-295); `cos brief` terminal deadline; config re-read rename window (nit).
- Remote read-only holds: `REMOTE_TOOLS = ["latest_brief"]` (src/mcp.rs:31), `allowed()` (src/mcp.rs:375-377), `call_tool` gate (src/mcp.rs:417-422). The revision adds no remote write tool.

## Counts

- Earlier: 6 listed. Closed 4 (1, 5, 6, and B2 at item 4). Closed with carry 1 (B1, item 3). Open 1 (N1, item 2, via New 1 and 3).
- New blocking: 3 (New 1 to 3).
- Carried to Plan: 12 should.

verdict: findings
