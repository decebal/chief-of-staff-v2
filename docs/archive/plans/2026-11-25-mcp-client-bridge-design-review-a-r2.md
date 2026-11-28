---
design: docs/plans/2026-11-25-mcp-client-bridge-design.md
design-sha256: superseded (was e344c79ccb2a0d2b3c26fb27d233ed8fc67b6b2c38f1fd6a8b716482ad6240cf)
verdict: findings
---

Round 2. Earlier findings: 33 (5 blocking, 20 should, 8 nit). Open: 2 (18, 19). New blocking: 2.

## Earlier findings

1. closed: owner watchdog in the run child (§Run process 3), daemon sweep of `daemon`-owned groups (§A.2), SIGTERM stop records `stopped` (§A.6). Child-death gap is new finding N1, not this one.
2. closed: record carries `pid`, `pgid`, `pid_start`, `owner_pid`, `owner_start` (design:98-104); liveness rule at design:116-118. Type mismatch carried.
3. closed: S12 to S14 added; S13 names `hc send/interrupt/kill/rm`, judge `sh -c` and `git -C` (design:381); checks decided to run in daemon, bounded (design:523-528). Binding gap carried under 18.
4. closed: `spawn_run` / `RunChild` split (design:158-161), exact argv for brief and ask (design:162-166), env pinned (design:168-172), ask answer into record (design:91-93). Current `brief_child` (src/pipeline.rs:683-717) is what the split replaces.
5. closed: remote gets `Mode::Remote`, builds no daemon client (design:248); resource subscribe removed (design:25-27). Current shared `Cos` at src/mcp.rs:40-43 is the code to change.
6. closed: remote keeps the Config from start (design:242, 248).
7. closed: remote rejects undeclared arguments in `call_tool` before dispatch, `-32602` (design:242, 548). Not in code yet (src/mcp.rs:412).
8. closed: pin rules per server kind (design:238-241); unpinned resolves by name with no directory flags via `business::resolve` (src/business.rs:379).
9. closed: brief lock is taken by the brief process before any model spend (design:63-64, 80-83); a daemon, app, CLI or launchd start cannot spend without it. Residual holes are N1 (lock released while a grandchild still spends) and N4 (ordering).
10. closed: only `ENOENT` / `ECONNREFUSED` mean down (design:193-195); `client_req` on `start` (design:196). Residual dedupe is in memory (carried).
11. closed: `hello` checked each tick, cached 3 s; `cos mcp` follower starts lazily at first `spin_off` (design:204; src/mcp.rs:247).
12. closed: socket under `confstr(_CS_DARWIN_USER_TEMP_DIR)`, independent of env (design:65-69).
13. closed: `binary.version` is S11 and a binding (design:379, 477-483); 5 s deadline, reaped, cache by path, size, mtime, stale on timeout (design:277-280).
14. closed: `.cos.orig` written only when absent, original mode kept (design:296-298).
15. closed: re-read before rename, refuse on change (design:299-301). Residual window between re-read and rename is carried.
16. closed: `toml_edit` 0.25 named as new direct dep, justified (design:289-291). Cargo.lock:3861 already has `toml_edit 0.25.17`. `serde_json` `preserve_order` is at Cargo.toml:33.
17. closed: `STORE(run_record)` Sink yes, VERIFY row (design:404, 543).
18. open: (a) `run.start` `oneOf` forbids question on brief, requires on ask (design:421-431) closed. (b) `registry.set_active` and `launchd.install` now have bindings (design:486-499) closed. (c) `run.stop` output enum has no `lost` (design:439) closed. Still open: `TOOL_CALL(hire.spawn)` is "keeps SpinOffArgs plus business" (design:515-516) and `TOOL_CALL(hire.follow)` is "keeps today's internal calls" (design:516-517), neither a closed JSON binding. `TOOL_CALL(binary.version)` VERIFY is "none needed beyond its binding" (design:538). Carried.
19. open: the fix was "daemon alone writes its runs' files", but design:116-118 has any reader (`run_status`, `runs`, app) write `lost` on a reconcile read. Regressed into N2.
20. closed: reconcile covers `queued` and `running` (design:116-117).
21. closed: bootout gives `stopped`, SIGKILL gives `lost` (design:141-146, 573-574; acceptance 7 at design:651-654). Acceptance 7 is insufficient for N1.
22. closed: stand-in daemon's subscribe stream drives `brief-changed` and `report-changed` in `apps/desktop/tests/ipc.rs` (design:616).
23. closed: `check_dirs` and `registered_homes` move into `cos::business`, `follow_all` into the lib (design:138-140). Private today at apps/desktop/src/lib.rs:346 and :1760.
24. closed: `task install` and `task setup` run `cos daemon install` (design:188-189); exe-change exit 1 at design:144-146.
25. closed: A is water with a spike recorded under `docs/plans/`; B is sand resting on A (design:630-632).
26. closed: `group.kill` VERIFY under the daemon run mutex immediately before the kill (design:542). Note N1 limits what that group identity covers.
27. closed: subscribe is a stream (design:133-135); 64 KiB line cap, 5 s idle close, 16 connections (design:135-137); `sun_path` length refused at bind (design:67-68); 0700 dir chmod and 0600 socket (design:68-69).
28. closed: clients never take `daemon.lock`; daemon retries once (design:124-126).
29. closed: separate `reason` field (design:107-108).
30. closed: 320 s, reason stated (design:282-284); matches Taskfile.yml:90.
31. closed: CLI with daemon up is owner `self`, foreground, takes the brief lock (design:203).
32. closed: 420 s stated as product budget, not a gate (design:174-176).
33. closed: link switch is window-only, nothing persists, header shows business (design:331-333).

Open earlier: 2 (18, 19).

## New findings

1. blocking §Run process 3, §A.4-6, §Violations "daemon killed", acceptance 7: grandchild process groups are not in any record and survive every owner kill. `proc::stream_lines` and `proc::run` call `spawn_grouped`, which sets `process_group(0)` (src/proc.rs:166-167), so `claude` (src/claude.rs:198) is its own group leader. The brief child's group kill (src/pipeline.rs:708 today; `group.kill` in the design) reaches only the brief child. The record holds one `pgid` (design:101). Stop, deadline, SIGTERM, daemon SIGKILL sweep, and SIGKILL of the run child leave `claude` alive with no owner and no record. The brief lock is released when the child exits, so a second paid brief can start while the first still spends, and acceptance 7 (`ps -g <pgid>` empty) passes with `claude` running. The owner-watchdog kill covers only the case where the child survives. Evidence: src/proc.rs:166-167, 105-108; src/claude.rs:198; src/pipeline.rs:703-708; design:101, 84-90. Fix: a run child keeps its descendants in one group (proc spawns under `COS_RUN_ID` join the child's group, not `process_group(0)`), so one group kill covers the tree; or write each spawned group's pgid and `pid_start` into the run record before the spawn, and have every owner kill and the sweep iterate that list. Add to acceptance: after `run.stop`, after deadline, and after SIGKILL of the run child, the stand-in grandchild is gone. Extend the `run` stand-in test (design:600-601) so the grandchild is checked after the child is SIGKILLed.

2. blocking §Run record "Reconcile on read" (design:116-118) vs "Who writes" (design:110-114): any reader writes `lost` from a read it made earlier. `run_status` polls every 500 ms (design:221-222). The run child can write `done` and exit between the reader's read (`running`) and its pid check (dead), and the reader then overwrites `done` with `lost`. Acceptance 4 (brief returned once done) fails in that window. This also reverses the round-1 fix for finding 19. Evidence: design:110-118, 221-222. Fix: readers never write; they report `lost` in the returned view. Only the daemon sweep or the owner writes `lost`, under the record lock, after re-reading the record and checking it is still `queued` or `running`. Or: compare-and-set on `state` under `flock` on the record.

## Carried to Plan

- should (earlier 18, open): closed JSON binding for `TOOL_CALL(hire.spawn)` (SpinOffArgs plus `business`, with `checks` and `scope` length and charset) and `TOOL_CALL(hire.follow)` (hire record schema), design:515-517. Give `binary.version` a VERIFY row instead of "none" (design:538).
- should (earlier 19, folded into N2): reader-side writes.
- should: brief lock ordering. design:76-83 writes the `running` record (step 1) before the lock (step 2), though design:63-64 says the lock is taken before anything is written. A refused brief leaves a `running` record that reconciles to `lost`, and `runs` lists it. Take the lock first, then write the record.
- should: owner-watchdog exit 75 (design:87) collides with `deferred` (src/pipeline.rs:714 reads 75 as deferred). Use another code. The watchdog writes `lost` (design:87), but "Who writes" (design:110-112) says only reconcile writes `lost`. Make the two agree.
- should: hire following now runs in the daemon with the app closed (design:137-141). Judge `sh -c` of model-supplied `checks` (src/agents/judge.rs:292) then runs unattended at login, not only while the app or `cos mcp` is open. Decide and state: keep, or run `checks` only with an attached client.
- should: `pid_start` is a string in the record (design:101, 457) but `proc::start_time` returns `Option<(i64, i64)>` (src/proc.rs:287). The hire path uses `ps -o lstart=` (src/agents/mod.rs:1126). Pick one format and name it.
- should: the subscribe mechanism for `out/run-records/` is unnamed (design:156). Neither Cargo.toml (root or apps/desktop) has a file-watch crate. Name the mechanism (crate or polling interval) and the app `watch` it reuses (apps/desktop/src/lib.rs:1781).
- should: MCP App view (design:226-230) calls `brief` and `run_status`. Remote refuses both (design:242). State what the remote view shows.
- should: `binary.version` takes `bin` from client config contents (design:521), validated only as `^/` (design:478-480). Require the path to be a known install location or the path already registered for that client.
- nit: `COS_OWNER=<pid>:<start>` (design:84): the start string has colons (`09:15:02`). Parse with `split_once(':')`, and say so.
- nit: `client_req` dedupe lives in daemon memory (design:153-154). After a daemon restart a retried `ask` spawns a second run. Say so, or persist in the record.
- nit: config write re-read to rename window (design:299-301) is a TOCTOU. Say the residual is accepted.
- nit: a terminal `cos brief` that Ctrl-C's leaves `claude` running today (own group, src/claude.rs:198). N1's fix covers it; say so in the design.

## Counts

- Open earlier: 2 (18, 19).
- New blocking: 2 (N1, N2).
