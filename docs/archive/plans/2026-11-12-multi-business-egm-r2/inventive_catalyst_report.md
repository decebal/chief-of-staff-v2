Mode: REVIEW (Stage 5, strategic). I read the persona, the target design and the brief. The persona names no document standards, and none were supplied. I checked the claims against the code at `/Users/decebaldobrica/Projects/chief-of-staff-v2-wt/multi-business`.

Not findings: making the window disposable with `location.reload()` instead of a cache list resolves the cache contradiction cleanly. Resolving one whole triple from one source removes mixing. The registry lock plus re-read is sound.

**Finding 1**
1. Severity: Major
2. Affected Scope: CLI `cos business`, the last bullet ("`task schedule BUSINESS=<name>` adds `--business <name>`"); the resolver, rule 1.
3. False Compromise: The design keeps the plists unchanged for back-compat and also adds `--business` to them. But both plists always set `COS_HOME` (`launchd/cos.brief.plist:14`, `cos.dash.plist:15`). `Taskfile.yml:194` always substitutes `@COS_HOME@` with `{{.ROOT_DIR}}`. So with `BUSINESS=acme`, every scheduled run hits rule 1 and errors with "--business and COS_HOME both set". `cos.dash` has `KeepAlive` and would crash-loop every 30 s. The log paths (`@COS_HOME@/out/logs`) also stay in the install home, not the business home. The design assumes "unchanged plists" and "the strict no-mix rule" can both hold. Here they cannot.
4. Proposed Resolution: Segmentation of the template. Make two plist variants, or make the `COS_HOME` key conditional. When `BUSINESS` is given, drop `COS_HOME` from `EnvironmentVariables` and point the log paths at the business's resolved home (`cos --business X business show` at schedule time). Add an acceptance row: `task schedule BUSINESS=acme` followed by `launchctl kickstart` exits 0.
5. Secondary Liabilities: The template logic grows, and the log location now depends on the registry at install time. If the business's home changes later, the job needs a reschedule.

**Finding 2**
1. Severity: Major
2. Affected Scope: Resolver rule 1 against the agent supervisor spawn (`src/agents/mod.rs:422-425`). The ISA sink row "app commands on the switched triple".
3. False Compromise: The strict rule "`--business` together with any `COS_*` directory variable or directory flag is an error" is applied to child processes without looking at inheritance. The supervisor is spawned as `cos --home <home> agents supervise <id>` and inherits the parent's env. Example: `COS_BUSINESS=acme cos agents spawn …` from a shell, or an app pinned by `COS_BUSINESS`. The child then sees `COS_BUSINESS` plus `--home`, rule 1 fires, and the supervisor exits before it takes a slot. The agent stays non-terminal until START_GRACE runs out, then becomes `lost`. The design lists `agents_spawn` as an existing sink with "existing checks". It does not cover this new failure path, and no test covers it.
4. Proposed Resolution: Extraction / Intermediary at the spawn boundary. The spawn passes the whole resolved triple (`--home --workspace --projects`) and calls `env_remove` on `COS_BUSINESS`, `COS_HOME`, `COS_WORKSPACE` and `COS_PROJECTS`. The child then gets exactly one source: explicit flags. The same applies to any other place `cos` spawns `cos`. Add test rows: spawn with `COS_BUSINESS` set in the parent, and spawn with `COS_WORKSPACE` set in the parent.
5. Secondary Liabilities: Children no longer inherit env tweaks on purpose. Anything that relied on inherited `COS_*` (`COS_AGENT_SLOTS` is not affected and must not be removed) must be passed explicitly. The argument list gets longer.

**Finding 3**
1. Severity: Major
2. Affected Scope: Desktop, `agents::count_live(home)`; Acceptance 5; Risks, "Badge cost".
3. False Compromise: The design treats accuracy and non-mutation as opposed. To avoid `reconcile`'s kill-and-write, it accepts that "a crashed supervisor in another business is counted until that business is opened" and softens the label to "about N". If a business is never opened, its ghost count stays forever. But `reconcile` already holds a pure liveness test that only reads: `is_cos(pid)` (`agents/mod.rs:668-671`) plus the `START_GRACE` check (`mod.rs:473-476`). Only the repair branch (`kill_group`, `write_state`, `mod.rs:485-492`) mutates.
4. Proposed Resolution: Extraction. Split `reconcile` into `classify(spec, state) -> Live | Lost | Grace`, which only reads, and `repair`. `count_live` uses `classify` and counts only Live and Grace. The badge becomes exact, and "about" can go. The ISA matrix row ("byte-identical, no process killed") still holds.
5. Secondary Liabilities: One `sysctl`/`proc` lookup per non-terminal agent, which adds to the badge cost that has not been measured yet. A reused pid is only partly covered by the `comm == "cos"` check. Two code paths now have to agree on what counts as live.

**Finding 4**
1. Severity: Nit
2. Affected Scope: Risks, "Badge cost"; Confidence, "Agents badge: sand".
3. False Compromise: The scan cost is accepted as "read every `out/agents/*/state.json` per business", and only measuring is planned. Cost grows with all agent history, not with live agents. Finished runs are never pruned, so the cost only rises.
4. Proposed Resolution: Preliminary Action. Use a cheap pre-filter before parsing JSON. For example, the supervisor removes a `running` marker file on a terminal write, or the scan skips directories whose `state.json` mtime is older than the newest terminal write it has cached. Another option: Inversion through the slot locks (`slots.rs:40-64`). Held `run-*.lock` files are the kernel's own live count, but probing them with `flock` races `try_run_slot`. Treat that as rejected unless the probe takes the lock shared and non-blocking, and accept that queued agents are missed.
5. Secondary Liabilities: The marker adds one more write path to keep consistent with `state.json`. An mtime cache can go stale. The slot probe misses queued agents and can briefly push a starting supervisor to the next slot.

**Finding 5**
1. Severity: Nit
2. Affected Scope: Desktop, the env-pin bullet; Risks, "Env pin".
3. False Compromise: "Honour `COS_*` at launch" and "allow switching" are treated as exclusive, so the app refuses all switching. The brief frames env as a one-command override. For the app, the matching scope is the launch, not the whole session.
4. Proposed Resolution: Dynamics. Env seeds the starting triple only. The picker may switch, and the header shows `started from COS_HOME; switched to <name>`. This only works together with the env scrub from Finding 2, or children would mix sources.
5. Secondary Liabilities: A user who launched with env on purpose loses the guarantee. It depends on Finding 2's `env_remove` in every child spawn. The header has more states to render and test.

**Finding 6**
1. Severity: Nit
2. Affected Scope: CLI, "Long-lived processes started with neither bind the active business"; Risks, "App and CLI disagree".
3. False Compromise: `active` does two jobs: the terminal default and the daemon binding. `install-mcp` sets no `COS_HOME` (`Taskfile.yml:65, 74`), so MCP follows `active` at its next restart. `cos.dash` is pinned to `ROOT_DIR` through `COS_HOME`. After `cos business use acme`, Claude Desktop's MCP (on its next restart) serves acme while the dashboard still serves the install home. The Risks section does not list this divergence.
4. Proposed Resolution: Segmentation of the pointer. Either `install-mcp` writes `--business <active at install>` (or `BUSINESS=`) into the MCP args, or the doc states in Risks that MCP follows `active` and dash does not.
5. Secondary Liabilities: Pinning MCP means `use` no longer moves it, so a reinstall is needed to retarget it. Leaving it as is needs a documented warning and a restart habit.

---

Findings: 0 Blocker, 3 Major, 3 Nit
