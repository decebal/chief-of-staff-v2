---
design: docs/plans/2026-11-25-mcp-client-bridge-design.md
design-sha256: superseded (was dde02984ccb5dd2e1e943bc1c184bb44bf044a75e287acb1019efdb4d921f999)
verdict: findings
---

Counts: 5 blocking, 20 should, 8 nit.

1. blocking §A.5 and §Alternatives: children outlive their owner. A daemon killed with SIGKILL or crashing, or a `cos mcp` or app process killed, leaves the `cos brief` child in its own process group, reparented to PID 1, with nothing reaping it. `spawn_grouped` sets stdin to null (`src/proc.rs:166-167`), so the child gets no death signal, and the run file reads `lost` while the process runs. Acceptance 6 only covers `launchctl bootout` (SIGTERM). Evidence: `src/proc.rs:166-170`, `src/pipeline.rs:703`. Fix: give each owned child a watchdog that ends it when its owner dies (a pipe whose write end the owner holds, or a kqueue EVFILT_PROC NOTE_EXIT watch on the owner pid, run by a supervisor thread). On daemon start, kill and reap the groups of every run file still `running` before reconcile. Add a SIGKILL-the-daemon case to `tests/daemon.rs` and to acceptance.

2. blocking §A run file (design lines 87-91, 98-101): the schema has no `pid` or `pgid`. Reconcile for `owner: mcp|app` reads "the recorded pid", and `run.stop` (S5) needs the group after a daemon restart, but neither is stored. A bare pid also reads alive after reuse. Evidence: design lines 87-91; `src/proc.rs:287` (`start_time`) exists for this. Fix: add `pid`, `pgid`, `pid_start` to the run file; liveness = pid alive with the same start time.

3. blocking §Semantic ISA, side-effect inventory: incomplete. §C gives `spin_off`, `stop_agent` and `agents` a `business` argument, so they reach any registered business, yet S1-S11 cover only paid brief/ask. Missing: hire spawn (paid Claude session, worktree cut and seed, `src/agents/seed.rs`); `judge` running model-supplied shell `checks` in the daemon (`src/agents/judge.rs:292` `Command::new("sh")`; checks come from `spin_off` args, `src/mcp.rs:73-76`); follower writes `herdr::send` (`src/agents/follow.rs:246`), `focus` (`:288`), `rm` (`:340`); `git -C` writes in the judge (`src/agents/judge.rs:264`); the run child's own writes to `out/briefs` and receipts in `state.json`. S11 lists only read, interrupt, kill. Fix: one row per side effect, each with a VERIFY. Give `spin_off` and `stop_agent` a business check and an arg schema; decide whether `checks` run in the daemon at all, and bound them.

4. blocking §A (`brief_child` reuse): the design's assumptions about `pipeline::brief_child` do not hold. (a) §A argv is `cos --business <name> --no-open brief`, but `brief_child` pins `--home --workspace --projects` and env (`src/pipeline.rs:689-700`); §A also says it pins "the way brief_child already does". (b) `brief_child` sends stdout to null (`src/pipeline.rs:701`), so `ask` cannot capture `answer`. (c) `brief_child` owns the `Child` inside the function (`src/pipeline.rs:703-716`), so `run.stop` (S5) and the deadline thread cannot reach the group. Fix: split into a spawn that returns a handle (pid, pgid) plus wait and kill on that handle. State the exact argv for `brief` and for `ask`.

5. blocking §C, §F, §Constraints: `Cos` is one type for both modes (`src/mcp.rs:40-43`), and `get_info` is shared (`src/mcp.rs:380-397`). §F adds `resources.subscribe` and a daemon subscription to `cos mcp`, but nothing gates it on `Mode::Local`. A remote session subscribing to `ui://cos/dashboard` would then hold a daemon connection, against "never reaches the daemon socket". Fix: construct the daemon client only when mode is Local; do not advertise subscribe in remote `get_info`; add a test that `Mode::Remote` builds no socket client.

6. should §C: "every MCP tool" builds its Config per call, and remote is not excluded. A remote session would then follow Make default, contradicting "its business stays fixed at start" and "a leaked URL reads one business". Fix: state that `Mode::Remote` keeps the Config from start.

7. should §C, remote refusal: `latest_brief` takes no parameters (`src/mcp.rs:126`), so the `#[tool]` macro ignores a `business` argument. The `-32602` refusal needs an explicit check in `call_tool` (`src/mcp.rs:412-425`) that rejects unknown args for remote. Fix: specify that check and test it.

8. should §C, precedence: the flags layer holds one business name (`src/business.rs:310-315`). A per-call `business` placed there collides with the `--business X` pin: under `resolve` it errors when dirs are set (`src/business.rs:411-416`) and silently overrides otherwise (`:380-391`). §C says the pin is kept. Fix: state the rule (pin wins and a different per-call name is refused with a named error, or per-call is refused on a pinned server).

9. should §A, one brief per business: check and insert are not atomic across processes. The daemon checks run files, but the app's `brief_run` guards only its own window mutex (`apps/desktop/src/lib.rs:1493-1496`), and the CLI and mcp fallback do not check at all. Two paid briefs for one business can start. The same goes for the install check (`VERIFY(no_live_runs_or_force)`, design line 368) against a `run.start` that arrives during bootout. Fix: a per-business flock (`out/runs/<biz>.lock`) held across check and `queued` write, taken by every starter (daemon, app, mcp, CLI); the daemon refuses starts while an install is pending.

10. should §A, fallback: only ENOENT and ECONNREFUSED mean "down". A connect that times out, or a `start` whose reply is lost, is not covered, and the client may then run a second paid run. Fix: any other error is an error, never a fallback; `start` takes a client request id so a retry returns the existing run.

11. should §Hire following: the follower choice is made once, at spawn ("hello answered"). If the daemon dies later, `cos mcp` and the app do not resume following. Also, `cos mcp` starts `follow_while_serving` only inside `spin_off` (`src/mcp.rs:247`), not at serve start, so the Context row "Follows hires while serving" overstates. Fix: check `hello` per tick and run the follower whenever the daemon is absent; state the lazy start.

12. should §A socket path: clients read `XDG_RUNTIME_DIR` from their own env (a terminal may set it), while the launchd job has only PATH and COS_BUSINESSES (§A install). Different env gives a different socket path, clients see "down" and fall back, and paid runs duplicate. Fix: one fixed path (the Application Support one), or set the variable identically in the plist and in every client.

13. should §D `cos connect status`: `status` runs the configured `command --version` (5 s) on every call, and the Settings card polls it. That is a spawned child on a poll path (process-ownership hot-path rule) and absent from the inventory. Fix: cache with a TTL, stale on timeout, reap the child; add an S-row for "executes configured binary".

14. should §D writes, step 3: `.cos.bak` is overwritten on every write, so a second repair replaces the pre-cos original with the post-cos file. Acceptance 1 depends on the `.bak`. Fix: write the `.bak` only when absent, or timestamp it.

15. should §D writes: Claude Desktop and Codex rewrite their own configs on settings change. Read, modify and rename can drop a concurrent client write. Fix: re-read before the rename and refuse if the file changed since the snapshot.

16. should §D: `toml_edit` is named as the writer, but `Cargo.toml:36` has only `toml = "1.1.8"`, and `toml_edit` is not a dependency. Fix: name the crate to add (version, features) and justify it against the dependency policy.

17. should §Instructions: `STORE(run_file)` is marked Sink "no", yet S6 writes to disk and §Taint says the file's taint reaches `run_status` and the window. Fix: Sink yes, with a VERIFY (run id regex, owner, business registered).

18. should §Bindings: (a) `run.start` does not forbid `question` on `brief` or require it on `ask` (design lines 305-311); use `oneOf`. (b) `registry.set_active` and `launchd.install` are prose only (lines 347-349), with no JSON binding. (c) `run.stop` output enum includes `lost`, which stop never returns. Fix: complete a closed JSON binding for each sink.

19. should §A, run state writes: `stop` (client side) and the deadline or exit path (daemon thread) both write the same run file. Last writer wins, so `stopped` can flip to `done` or `failed`. Fix: the daemon alone writes its runs' files; `stop` signals the daemon, which records `stopped`; or compare-and-set under the daemon's mutex.

20. should §A reconcile: only `running` is reconciled. A crash between the `queued` write and the spawn leaves a run `queued` forever. Fix: include `queued` in reconcile (owner gone, so `lost`).

21. should §A.5 vs acceptance 6: §A.5 says SIGTERM marks runs `stopped`. Acceptance 6 expects `launchctl bootout` (SIGTERM) to read `lost`. The acceptance cannot pass as written. Fix: bootout gives `stopped`; `lost` only for SIGKILL. Test both.

22. should §Testing, acceptance 2: the subscribe-to-`brief-changed` path has no automated test. The App list names connect, make-default and deep link only, and "without a reload" is manual. Fix: a test where a stand-in daemon's subscribe stream drives `brief-changed` on the mock runtime (`apps/desktop/tests/ipc.rs`).

23. should §A/§E, helper locations: design cites `check_dirs` as existing (§ISA VERIFY `run.start`), but it is private to the app at `apps/desktop/src/lib.rs:346`. `registered_homes` (`apps/desktop/src/lib.rs:1760`) and `follow_herdr` (`:1860`) are also app-private, and §A moves the loop into `cos::agents::follow_all`, which does not exist. Fix: name the move: `check_dirs` and `follow_all` into the cos lib, with the app calling them.

24. should §Upgrade: `task install` replaces the binary (`Taskfile.yml`, install step) but nothing restarts the daemon, and the mismatch shows only on the Settings card. Fix: `task install` runs `cos daemon install`, or the daemon exits when its binary's version or mtime changes.

25. should §Confidence: B ("run tools over run files") is rock, but it sits on A, which is sand, and on the spike the brief asks for before A (brief line 100: daemon protocol and lifecycle is water; spike = skeleton, one round trip, kill and restart mid-run). The spike is not recorded as run. Fix: B sand until A's spike is recorded, or keep A water and run the spike.

26. nit §A stop: after the leader is reaped, `kill(-pgid)` can hit a reused pgid. Check run state under the same lock that reaps. Fix: state the ordering.

27. nit §A socket and protocol: `subscribe` sends many lines, which conflicts with "one JSON response per line". No read timeout or maximum line length on a connection, so a silent client holds a thread. macOS `sun_path` is 104 bytes, and `~/Library/Application Support/cos/daemon.sock` can exceed it with a long home. The 0700 directory must be chmod'd when it already exists. Fix: state each; add the limits.

28. nit §A probe: a client probing `daemon.lock` with a non-blocking flock can make a starting daemon fail with EWOULDBLOCK and exit "already running". Fix: the probe must not take the lock, or the daemon retries once.

29. nit §Violations: `said` is "last 12 stderr lines" (§A), but the deadline row writes the token `deadline` into it. Fix: a separate `reason` field.

30. nit §D Codex: design writes `tool_timeout_sec = 120`; `Taskfile.yml:83-90` says 320. Fix: reconcile the number and say why.

31. nit §A CLI: "The CLI runs as today" is stated only for fallback. With the daemon up, `cos brief` on the CLI is unspecified: wait, attach, or exit. Fix: state it.

32. nit §A deadline: `BRIEF_CHILD` is 420 s (`src/pipeline.rs:665`), above the 5-minute ceiling in `rules/timeouts.md`. A paid run is not a shell step, but the design should say so, or lower the number. Fix: one sentence.

33. nit §E links: `cos://run/<business>/…` from any web page can switch the window's business (§E: "switches the window"), changing which business's data is on screen. Navigation only, no data leaves. Fix: state that link switches do not persist and that the window shows the business name.
