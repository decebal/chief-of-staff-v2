**Design Critic Goldfish: REVIEW, Stage 7**

Target: `/Users/decebaldobrica/Projects/chief-of-staff-v2-wt/multi-business/docs/plans/2026-11-12-multi-business-design.md`, whole doc.
Standard: `/Users/decebaldobrica/.claude/plugins/cache/egm/egm/0.1.1/resources/document_standards/feature-spec-standard.md`.
Context: brief `/Users/decebaldobrica/Projects/chief-of-staff-v2-wt/multi-business/docs/plans/2026-11-12-multi-business-brief.md`.

Standard coverage: Problem, Goals and non-goals, Architecture, Alternatives, Risks and Security sections are all present. Cited code refs checked: `main.rs`, `settings.rs`, `lib.rs`, `agents/mod.rs`, `slots.rs`, `state.rs`, plists, `Taskfile.yml`. All accurate.

---

**1. Severity: Blocker**
Scope: design lines 147-168 (migration, `add`), Acceptance 8 (lines 411-412), Acceptance preconditions (line 393). Code: `src/claude.rs:111-125`, `src/main.rs:17`.

The design never says what a business **home** must contain. `cos brief` and `ask` run `claude --agent tactician` with `current_dir(home)` and `--setting-sources project` (`claude.rs:120-125`). So the home must hold `CLAUDE.md` and `.claude/agents/tactician.md`. Today only the cos checkout has them. `main.rs:17` documents home as "Repo holding CLAUDE.md, .claude/agents and context/".

In the preconditions, `~/biz/acme` is an empty dir plus `state init`. That means:
- `cos --business acme brief`, the brief's headline job, fails or runs with no agent definition.
- Acceptance 8 (kickstart `cos.brief` for acme) cannot pass.
- Acceptance 3 dodges this by using the free `report`.

`add` checks only that the dirs exist and that the workspace has a profile. The brief assumes "Kit templates … resolve from the install, not per business". Agent definitions and `CLAUDE.md` are not covered by that.

Recommendation: decide one of these and state it:
- (a) Agent definitions and `CLAUDE.md` load from the install (for example `--add-dir`, or passing agents explicitly) while `context/` and `out/` come from the business home.
- (b) `cos business add` or a `cos business init` scaffolds the home.
- (c) `add` refuses or warns on a home without them.

Then fix Acceptance 8 and add a `brief --dry-run` or a real brief step for acme.

**2. Severity: Major**
Scope: "Long-lived processes and launchd" (lines 170-178). Code: `launchd/cos.brief.plist:24-25`, `cos.dash.plist:20-21`, `Taskfile.yml:190-195`.

"Logs go to the resolved home's `out/logs`, which the job creates" cannot be built as written. `StandardOutPath` and `StandardErrorPath` are fixed plist strings, filled from `@COS_HOME@` at `task schedule` time, and launchd opens them before `cos` runs. When the plist follows the active business, these paths still point at the install home or a stale home, and a missing directory stops the job from starting. Note too that `main.rs:318-322` relies on stderr being a file.

Recommendation: either keep fixed log paths under a stable location such as `~/Library/Logs/cos/`, or have `cos` redirect its own output to `<resolved home>/out/logs`. Say which. Also say what happens to `mkdir -p out/logs` in `task schedule`.

**3. Severity: Major**
Scope: Risks, "MCP and launchd after `cos business use`" (lines 265-266), and line 176. Code: `cos.dash.plist:17-19` (`RunAtLoad`, `KeepAlive`).

`cos.dash` has KeepAlive and never restarts by itself, so "follows the new active business on its next start" means effectively never. Meanwhile `cos.brief` launches fresh every weekday and does follow `use`. After `cos business use acme`, the 08:30 brief writes acme's out, and the dashboard on :8790 keeps serving default's briefs indefinitely. Nothing tells the user. This is a silent split between the two scheduled jobs, which the design's own "a triple never mixes" goal is meant to prevent.

Recommendation: have `cos business use` kickstart or restart `cos.dash` when it is loaded with no `--business`, or at least print that dash and MCP still serve the old business. Also bind both plists to the same `--business` by default (the brief says they are "chosen at launch with `--business`"). Add an acceptance step for this.

**4. Severity: Major**
Scope: First-run migration (lines 147-153). Code: `apps/desktop/src/settings.rs:44-70, 91-98`.

What becomes `default` depends on which process runs first:
- App first: the legacy `settings.json` triple. The design's own Context (line 76) notes that `save()` wrote env-derived values into it, so it may hold transient values such as `/tmp/cos-demo` from a `COS_HOME` launch.
- CLI first: layers 1, 2 or 4. A one-off `cos --home /tmp/x business list` or `COS_HOME=… cos business list` makes a temporary dir the permanent default.

The test "migration writes `default` from the build clone" covers only the no-flag case.

Recommendation: pick one deterministic source. One option is layer 4 plus legacy `settings.json` only if its directories exist and differ, with a confirmation. Otherwise refuse to migrate while a flag or env layer is active. Say which, and test app-first versus CLI-first.

**5. Severity: Major**
Scope: Desktop badge (lines 193-197, 211-212), Departures line 63, Risks lines 258-262. Code: `agents/mod.rs:127-132, 469-494, 668-671`.

The badge semantics are undefined in two places.

- **What counts as "running".** The Departures say the badge counts `running` only. But `count` comes from `classify -> Live | Lost | Starting`, and Live covers `queued`, `seeding` and `running` (`mod.rs:474`). Do queued and seeding runs count?
- **What counts as "lost".** `classify` has no case for terminal records, since `reconcile` returns early on them (`mod.rs:470`).
  - If `lost` means records with status `lost`, then the Risks plan of "open that business, where reconcile cleans up" turns the record into a terminal `lost`. The badge then never clears (runs are never pruned) until `agents clean`.
  - If `lost` means a non-terminal record whose supervisor is dead, say so.

Recommendation: define `count` as "non-terminal records only". Make `lost` mean "needs reconcile" and `running` mean the live statuses. Specify `classify` on terminal input. Add a test that the badge clears after the business is opened.

**6. Severity: Major**
Scope: Desktop menu accelerator (lines 201-205). Code: `apps/desktop/src/lib.rs:503-547`; no menu exists today (grep finds no `menu`).

Adding an app menu in Tauri 2 replaces the default macOS menu. Unless the new menu rebuilds the Edit submenu (Undo, Cut, Copy, Paste, Select All) plus Window and Quit, Cmd+C/V/Z/A stop working in the webview. That affects the Hire dialog's prompt field and the docs room. The design and the UI hand-check (line 376) cover only Cmd+K.

Recommendation: state that the menu keeps the predefined Edit, Window and App items. Add copy and paste in the Hire prompt to the `pr-proof` hand-check.

**7. Severity: Major**
Scope: Desktop switch (lines 187-188, 206-210). Code: `apps/desktop/ui/app.js:156-165`, `lib.rs:151-158`.

"One snapshot per command" does not order two IPC commands that are in flight together. Say a debounced `cos-docs-save` from business A's docs frame calls `docs_save` while `switch_business` is pending. If the switch lands first, A's organisation is written into B's `<home>/context`. The design rejected a generation counter "on every response". A guard only on writes is cheaper.

Recommendation: send the business name with the app's write commands (at least `docs_save`, `agents_spawn` and `agents_record`) and refuse when it differs from the current triple. Alternatively, have the UI stop forwarding frame saves once the picker commits. Add an IPC test for this.

**8. Severity: Nit**
Scope: `remove` and `add --replace` (lines 160-167).

Removing a business, or `--replace` with a new home, while runs are live or lost under the old home makes them invisible. The badge only walks the registry, and nothing will ever reconcile or kill their harnesses.

Recommendation: `remove` and `--replace` should refuse while `count(old home)` is non-zero, or warn.

**9. Severity: Nit**
Scope: Resolver layers 1-2 (lines 130-137).

A lone `COS_PROJECTS` or `COS_WORKSPACE` in a shell rc silently skips the registry: home becomes the build clone and `cos business use` has no effect. The rule is consistent, but only `list` and long-lived processes report it.

Recommendation: when an active business is set and the resolver uses a higher layer, print a one-line stderr note naming the variable.

**10. Severity: Nit**
Scope: lines 139-143 and Violations line 335.

`state init` "does not resolve" yet "takes `--workspace` or the resolved workspace". With a corrupt registry and no `--workspace`, the target is undefined.

Recommendation: `state init` requires `--workspace`, or uses layers 1, 2 and 4 only.

**11. Severity: Nit**
Scope: Resolver (line 127 versus line 137) and Testing (line 354).

"All passed as values so tests need no process env" conflicts with layer 4 reading `$HOME` and the "unset `HOME` is an error" case.

Recommendation: add `home_env: Option<PathBuf>` to the inputs.

**12. Severity: Nit**
Scope: Registry write (lines 117-120). Code: `state.rs:548-573`.

`atomic_write` renames over the target, which replaces a symlinked `businesses.toml` (dotfile managers) with a regular file. Also, `slots.rs`'s `flock` is private (`slots.rs:44`) and would need exporting.

Recommendation: resolve the symlink before writing, or refuse a symlinked registry the way `state::load` does. Note the visibility change.

**13. Severity: Nit**
Scope: user-facing hints. Code: `src/mcp.rs:282`, `src/agents/view.rs:12`.

Hints such as `cos agents tail -f <id>`, when emitted by a process bound to a non-active business, resolve to the active business in the user's terminal and return "no agent".

Recommendation: include `--business <name>` in hints when the triple came from a named business.

**14. Severity: Nit**
Scope: Alternatives, "JSON registry" (lines 245-246).

The rejection rests on "the user chose `businesses.toml`". The standard requires concrete trade-offs and no attribution without a link. The brief's Decisions section is the record, so link it and add the technical reason: TOML is hand-editable and supports comments.

---

Findings: 1 Blocker, 6 Major, 7 Nit
