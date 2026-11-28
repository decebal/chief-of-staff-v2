---
design: docs/plans/2026-11-25-mcp-client-bridge-design.md
design-sha256: superseded (was dde02984ccb5dd2e1e943bc1c184bb44bf044a75e287acb1019efdb4d921f999)
verdict: findings
---

Review of the design against the live code at `main` (worktree `chief-of-staff-v2-mcp-bridge`). Line numbers are for the code as it stands; design line numbers are `docs/plans/2026-11-25-mcp-client-bridge-design.md`.

1. blocking §A Fallback and §Reconcile: a daemon that dies without SIGTERM (crash, SIGKILL, OOM, launchd restart) leaves each run's `cos brief` process group running with nobody reaping it. Nothing sweeps it on the next start, and the reconcile rule only reads the run file as `lost`. This breaks the process-ownership invariant the brief cites. Acceptance 6 tests only `bootout` (SIGTERM), so it does not catch this. Evidence: design lines 65-66 (SIGTERM only), 98-101 (reconcile reads only); `src/pipeline.rs:703` `spawn_grouped` has no owner beyond the daemon thread. Fix: store `pgid` and its start time in the run file (the repo already does this for hires: `src/agents/mod.rs:263-265`, `:1108`, `:1120-1130`). On daemon start, and on each reconcile of a `running` file, kill any recorded group whose identity still matches (`proc::kill_group`, then reap). Add an acceptance step: `kill -9` the daemon mid-run, restart it, and check `pgrep -x cos` shows no brief child.

2. blocking §A Requests table vs §A "Each run's child": the `start` row spawns `cos --business <name> --no-open brief`, but the next paragraph says the child pins directories "the way `brief_child` already does" (flags `--home`, `--workspace`, `--projects`, env `COS_*`, `COS_BUSINESS` removed). `pipeline::brief_child` (`src/pipeline.rs:683-717`) passes no `--business`. An implementer cannot tell which argv to build. Evidence: design line 73 vs lines 80-82; `src/pipeline.rs:689-700`. Fix: state one argv. Prefer reusing `brief_child` with the directories resolved from the registry, and say how `ask` captures its answer (see item 11).

3. blocking §B Paid tools return at once: the MCP App view calls the `brief` tool and reads `structuredContent.kind` and `.report`. A `brief` that now returns `{run, business, state, link}` leaves the view at "No saved output yet". The output schema also still says `Report`. The design does not mention either. Evidence: `src/dashboard.html:98-104` (reads `sc.kind`, `d.report`), `:166` (`call("brief")` on "Run new brief"), `src/mcp.rs:132` (`output_schema = schema_for_output::<Report>()`). Fix: state the new `brief` output schema, and either make the view call `run_status` with `wait_secs` until `done` and then render the report, or keep the view on `latest_brief` after a run completes. Add a test in `tests/mcp_bridge.rs` for the view's path.

4. blocking Decision and §A "owns every paid run": the scheduled 08:30 brief stays a launchd job that runs `cos --business <name> brief` outside the daemon. Evidence: `launchd/cos.brief.plist:8-11`, `Taskfile.yml:221-233`. Two briefs for one business can then run at once: the scheduled one is not visible to the daemon's "no running brief for that business" guard (design line 73, and the VERIFY row for `run.start`). Fix: either make the scheduled job call `cos daemon` (or `run.start`) so the daemon owns it, or state the exception in Decision and add a cross-process lock (`out/` lock file) that both the daemon and `cos brief` take.

5. blocking §C, §D and §A "Clients": the design says the app and CLI "become clients" of the daemon, but specifies only the fallback path. It does not say what `brief_run` (`apps/desktop/src/lib.rs:1482-1531`) does when the daemon is up (send `start`? keep its own thread?), what `cos brief` / `cos ask` do when it is up, or how the app's `subscribe` connection runs (a thread, reconnect, what it emits). Acceptance 2 ("the app shows the run as running without a reload") depends on that path, and the window's `BriefRun` state is per-window and in memory. Evidence: design lines 9-10, 122-125 (fallback only), 230-232 (subscribe, no thread spec). Fix: add a short client section listing each client's path when the daemon is up, the new Tauri commands (if any), and the subscribe thread's reconnect rule. Add a `tests/ipc.rs` case that a daemon-owned run emits `brief-changed`.

6. should §B `brief`/`ask` annotations: `brief` and `ask` are annotated `read_only_hint = true` (`src/mcp.rs:132-133`, `:152`) although they spend money. Clients may treat read-only tools as safe to run without confirmation. Evidence: `src/mcp.rs:132-133`, `:152`. Fix: set `read_only_hint = false` on `brief`, `ask` and `run_status`-adjacent start paths, and list the annotation change in §B.

7. should §B `run_status`: the wait mechanism is not stated. If `cos mcp` polls the run file, say so; if it polls the daemon, say so. Also say what a second `brief` call returns while one is running: the design says the daemon refuses (design line 73), but the MCP `brief` output is a success shape. Fix: pick "return the running run's id" (idempotent, which clients handle better) or an error, and document it in the tool description.

8. should §C business per call: `resolve` (`src/business.rs:379-418`) keeps one `business` slot for the flags layer and errors when a business name and directory flags are both set. A pinned `cos mcp --business X` plus a per-call `business: Y` is undefined. Design line 146-148 says the flags layer "still wins", which means a per-call argument can never override a pin. State which wins, and say how `Cos` (which holds `cfg: Config`, `src/mcp.rs:40-43`) represents "pinned" versus "per call". Remote stays fixed, as written.

9. should §A Run file and `out/runs/`: `out/runs/<day>.json` already holds `RunReceipt`s (`src/store.rs:16-25`, `src/pipeline.rs:79-91`, `:431`; read by `src/report.rs:61`). The design puts run files at `out/runs/<id>.json` without mentioning this. `store::latest` skips non-`YYYY-MM-DD` names (`src/store.rs:72-80`), so nothing breaks today, but the `runs` tool and any directory scan must filter by the id pattern. Fix: either use `out/run-records/`, or say in §A that `out/runs/` holds both and that `runs` filters on `^r-` names.

10. should §A Install / KeepAlive: `KeepAlive` set to true restarts the job on every exit. A second daemon that exits 0 with "already running" (design line 58) is then relaunched by launchd about every 10 s, forever. Fix: use `KeepAlive` with `SuccessfulExit = false`, and state that the duplicate exit is 0.

11. should §A `ask`: `brief_child` sends stdout to null (`src/pipeline.rs:701`), and `cos ask` prints its markdown to stdout (`src/main.rs:491-493`). The design says `ask` "writes its answer into the run file" but does not say how the child returns it. Fix: capture stdout with the same grouped-child helper and a size cap, or scope `ask` out of this PR (the brief's goal is `brief` only).

12. should §F and scope: the MCP `resources.subscribe` part (design lines 233-236) is not needed for the goal (acceptance 2 is served by the daemon-to-app stream). rmcp 3.11.0 does have `enable_resources_subscribe` (`rmcp-3.11.0/src/model/capabilities.rs:436`) and `notify_resource_updated` (`rmcp-3.11.0/src/service/server.rs:305`), but `subscribe` and `unsubscribe` are `#[deprecated]` "legacy-only" with defaults returning `method_not_found` (`rmcp-3.11.0/src/handler/server.rs:562-580`). Fix: cut §F's MCP part from this PR. If kept, name the negotiated protocol version Claude Desktop and Codex use, and override both methods.

13. should §E cold start: `tauri-plugin-single-instance` routes a link to a running window (design line 211). A link clicked while the app is closed launches it, and the navigation then has to wait for the `create: false` window to load. No mechanism is stated. Evidence: `apps/desktop/tauri.conf.json:14` (`"create": false`). Fix: queue the link until the main window reports ready, and add a test for a cold-start link in `apps/desktop/tests/links.rs`.

14. should §E Info.plist: no `CFBundleURLTypes` exists in `apps/desktop/bundle/Info.plist` (grep finds none). `task desktop-app` copies that file (`Taskfile.yml:120`) and does not re-register with LaunchServices. Fix: add the URL type to `apps/desktop/bundle/Info.plist`, and add a step that re-registers the bundle (`lsregister -f`) after install. Say which step verifies the scheme.

15. should §E reverse links: the design keeps `claude:`/`codex:` in §E (lines 222-226) and in Confidence as water, "it may drop". The goal does not need them. Fix: remove §E's reverse-link paragraph from this design and leave it to a separate spike.

16. should §A Socket path: the path lists `$XDG_RUNTIME_DIR/cos/daemon.sock` first (design line 47-48). A launchd job, Claude Desktop and the app can see different environments, so one side can pick a different path and fall back silently. Fix: use one path, `~/Library/Application Support/cos/daemon.sock`.

17. should §A Upgrade: `task install` replaces the binary but not the running daemon, and only `task setup` calls `cos daemon install` (design line 108). A machine that runs `task install` keeps the old daemon until someone runs the command by hand. Fix: have `task install` call `cos daemon install` (or `cos daemon restart`) after the copy, or make the daemon exit on a version mismatch so launchd restarts it.

18. should §Confidence and brief constraint: the brief says "No step runs longer than 5 minutes" (`docs/plans/2026-11-25-mcp-client-bridge-brief.md:94`). The design keeps the 420 s run deadline (design line 79, `src/pipeline.rs:665`). Fix: state that the run deadline is a product budget, not a gate or step, or lower it. Decide it in writing.

19. should §Terms and §Context "per business": the launchd labels are fixed, not per business. `cos.brief` and `cos.dash` are one pair per machine (`Taskfile.yml:221-233`, `:243-268`). Design line 31 says "per business". Fix: change the wording to "one pair per machine, for the business `task schedule` names".

20. should §Testing stand-ins: `tests/fixtures/fake-cos.sh` (design line 417) and a stand-in `launchctl` are new shell scripts. The brief's and the user's rule is Rust for scripts and tooling. Fix: write the stand-in cos child and the stand-in launchctl as a small Rust test-helper binary, or state why a shell fixture is genuinely impractical. Also list the stand-ins that do not exist yet (`fake-cos`, `launchctl`) in §Testing, since they are only named in the ISA matrix (line 392).

21. nit §D Codex: `tool_timeout_sec = 120` (design line 186) does not match `Taskfile.yml:89-90` (320) or the `runs` check of ≥ 60 (line 180). Pick one number and say why.

22. nit §D TOML: `toml_edit` is not a direct dependency in `Cargo.toml` (`grep` finds none), and `Cargo.lock` holds 0.19, 0.20 and 0.25. Name the version to add.

23. nit §E path: "`tests/links.rs`" (design lines 395, 435) is `apps/desktop/tests/links.rs`. Write the full path.

24. nit §A Install template: `cos daemon install` renders `launchd/cos.daemon.plist`, but an installed `cos` in `~/.cargo/bin` has no repo to read it from. Fix: embed the template with `include_str!`, as the repo does for other templates, or say where it is read from.

25. nit §Acceptance 6: `pgrep -x cos` also matches the brief child (same process name). Fix: count the brief children by argv (`ps -o args= -p <pid>` for the pid in the run file), not by name.

Counts: blocking 5, should 15, nit 5. Verdict: findings.
