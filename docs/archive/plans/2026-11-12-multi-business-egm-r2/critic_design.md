**Design Critic Goldfish: REVIEW**

Target: /Users/decebaldobrica/Projects/chief-of-staff-v2-wt/multi-business/docs/plans/2026-11-12-multi-business-design.md, whole doc. Standard: feature-spec-standard.md. Context: the brief.

Standard compliance: problem, goals and non-goals, architecture, alternatives, risks, and security are all present. The Alternatives section passes. The gaps are below.

---

1. **Severity**: Blocker
   **Affected Scope**: design.md:120-122 and 180-181; src/agents/mod.rs:422-428; Testing section
   **Finding & Recommendation**: `agents::spawn` starts the supervisor as `cos --home <home> agents supervise <id>`, and the child inherits the parent's environment. Resolver rule 1 (design.md:108-109) makes `--business` or `COS_BUSINESS` combined with any directory flag a hard error. So if the spawner has `COS_BUSINESS` set, every supervisor dies at startup with "--business and --home both set" and the agent ends up `lost`. That covers `COS_BUSINESS=acme cos agents spawn …`, an app launched pinned by `COS_BUSINESS` (design.md:155-157), and `cos mcp` or `cos dash` started with that env. The design never mentions this spawn site.
   Fix: have the spawn site `env_remove` `COS_BUSINESS` and the other `COS_*` directory vars, or pass the whole triple and clear the env. Add a test where an agent spawned under `COS_BUSINESS` reaches `running`.

2. **Severity**: Blocker
   **Affected Scope**: design.md:143-146 ("the plists are unchanged, and `task schedule BUSINESS=<name>` adds `--business <name>`"); launchd/cos.brief.plist:14, launchd/cos.dash.plist:15; Taskfile.yml:187-197
   **Finding & Recommendation**: Both plists always set `COS_HOME=@COS_HOME@` in EnvironmentVariables. Adding `--business <name>` to ProgramArguments therefore trips rule 1 on every run. The scheduled brief fails every weekday. `cos.dash` (`KeepAlive`, `ThrottleInterval 30`) crash-loops every 30 s. Logs also still go to `{{.ROOT_DIR}}/out/logs`, not the business's home.
   Fix: when `BUSINESS` is given, drop the `COS_HOME` key and point the log paths at the business's home (resolve it with `cos business show`). Add an acceptance item for `task schedule BUSINESS=acme`.

3. **Severity**: Major
   **Affected Scope**: design.md:134-140 (`cos business add … --home --workspace --projects`) vs the global directory flags (design.md:103-114, 143)
   **Finding & Recommendation**: The design does not say how `add`'s `--home/--workspace/--projects` relate to the global `--home/--workspace/--projects` that feed the resolver. If clap or the resolver treats them as the same flags, three things break:
   - On an empty registry, `default` is written from step 2 using acme's own directories.
   - The "home already used by another business" check then refuses `acme`, which contradicts Acceptance 2.
   - `cos --business x business add …` hits rule 1.

   Fix: either rename `add`'s arguments (positional, or `--set-home`), or state that `add` resolves `default` while ignoring its own directory arguments, and add a test for it.

4. **Severity**: Major
   **Affected Scope**: design.md:17-20 (Terms: app business "starts as the active business") vs 150-154 (`settings.json` stores `business`); brief "the app starts on that default"
   **Finding & Recommendation**: The startup rule contradicts itself. If the app restores `settings.business`, it never starts on `active` again after the first pick. Also, every existing install has a `settings.json` with home, workspace and projects, because `save()` writes effective values (settings.rs:91-98). Those installs resolve as explicit (legacy step 2) and ignore `cos business use` indefinitely.
   Fix: state one startup rule, with precedence among `settings.business`, legacy fields and `active`, and add an acceptance line for it.

5. **Severity**: Major
   **Affected Scope**: Desktop startup, design.md:148-181; Violations table design.md:271-281
   **Finding & Recommendation**: App startup failure modes are unspecified:
   - `settings.business` names a business that `cos business remove` deleted. `remove` only refuses the *active* business, not the app business.
   - The business's directories or profile are gone.
   - The registry is corrupt at launch ("every command that needs the registry stops"). What does the window show?

   Also, a `settings.json` written without `home/workspace/projects` fails today's non-`Option` deserialize. `load()` swallows that error with `.ok()` (settings.rs:45-47), silently dropping `cos` and `slots`.
   Fix: specify a fallback (for example, active then default plus a visible error banner), make the legacy fields `Option` with `#[serde(default)]`, and add ipc tests for each case.

6. **Severity**: Major
   **Affected Scope**: design.md:165-167 (`count_live` never reconciles); src/agents/mod.rs:469-494
   **Finding & Recommendation**: Today, `reconcile` is the only thing that kills the harness group of a dead supervisor. With the supervisor dead, the agent's deadline and budget are no longer enforced. The design accepts "counted until that business is opened", but the real cost is an orphaned harness running and spending without limit in a business nobody opens. Risks does not list this.
   Fix: add it to Risks. Options: reconcile other businesses on picker open (with a liveness check only, `is_cos(pid)`, and an explicit opt-in kill), surface "N lost" separately from "running", or have the supervisor's harness group self-terminate on parent death.

7. **Severity**: Major
   **Affected Scope**: design.md:2-3 and 26-37; brief "Decisions"
   **Finding & Recommendation**: The design departs from decisions the user made in the brief without recording them as departures:
   - The badge "switches to them" (brief) becomes "Not clickable" (design.md:177-179).
   - Migration "on first run" (brief) becomes "on first `add`" (design.md:138-140, Acceptance 1).
   - "A `COS_*` env var still overrides for one command" (brief) becomes a hard error when combined with `--business`. Anyone with `COS_HOME` exported in their shell, as current docs suggest, cannot use `--business` at all.

   Fix: list each departure with its rationale, flag it for the user's sign-off, and add the shell-exported `COS_*` case to Risks.

8. **Severity**: Major
   **Affected Scope**: design.md:171-172 (⌘K / Ctrl+K); apps/desktop/ui/app.js:134-189; src/docs.html:1336-1344
   **Finding & Recommendation**: Most of the window is the report iframe (sandboxed, opaque origin) or the docs and flows iframes. Keydown events inside an iframe do not reach the parent's listener. After any click in the report, ⌘K does nothing, so Acceptance 4 is unreliable.
   Fix: register ⌘K as a Tauri menu accelerator or global shortcut, or forward it from each frame by `postMessage` (the report already uses a `cos-report-state` channel). Add a hand-check with focus inside the iframe.

9. **Severity**: Nit
   **Affected Scope**: design.md:173-176; apps/desktop/ui/app.js
   **Finding & Recommendation**: The design writes the current tab to the URL hash before `location.reload()`, but app.js never reads a hash on boot. The default is `ui.pane = "staff"`, and the top-level `.app-tab` choice is not restored. Without a boot-time reader, "same tab" in Acceptance 4 fails. Also, the reload discards an open hire dialog's unsaved prompt.
   Fix: specify the boot-time hash reader, and either warn on or block a switch while a dialog has edits.

10. **Severity**: Nit
    **Affected Scope**: design.md:50-54 and 120-121 ("every site listed in Context")
    **Finding & Recommendation**: The site list misses main.rs:428 (`flows_cmd(&ws, &cli.projects…)`), 567-571 (`agents_cmd` `cli.ws()` and `cli.projects`) and 383-385 (dash). The compiler will catch these once the fields become `Option`, but the inventory is incomplete.

11. **Severity**: Nit
    **Affected Scope**: design.md:255-257
    **Finding & Recommendation**: `serde deny_unknown_fields` only applies to a struct. `switch_business(name)` as a plain Tauri command argument gets no such check. Either wrap the arguments in a `#[derive(Deserialize)] #[serde(deny_unknown_fields)]` struct or drop the claim.

12. **Severity**: Nit
    **Affected Scope**: design.md:303 (Testing, "the CLI's and the app's empty-environment resolution are the same strings")
    **Finding & Recommendation**: `tests/business.rs` is in the `cos` crate and cannot call the desktop app's resolution, so this test cannot do what it says. Either make the app call `cos::business::resolve` with no default of its own and delete `built_home()` (settings.rs:26-28), so the guarantee holds by construction, or put the test in `apps/desktop/tests`.

13. **Severity**: Nit
    **Affected Scope**: design.md:134-138 (add validation)
    **Finding & Recommendation**: Only a shared *home* is refused. Two businesses can share a workspace, and so share `state.json` and its deliverables, without any warning. Either state that this is intended or warn on it. The design also does not say who creates `~/.config/cos/` and the `.lock` file on first `add`.

14. **Severity**: Nit
    **Affected Scope**: design.md:168
    **Finding & Recommendation**: The label "about N running" also counts `starting`, `queued` and `seeding` (agents/mod.rs:127-131). Either label it "about N live" or count only `running`.

Code claims checked and found to hold: main.rs:16-45, 327-335 and 883-885; settings.rs:26-28, 44-70 and 91-98; lib.rs:26-28, 59-80, 151-158, 363-391, 423-431; slots.rs:40-41 and 68-69; agents/mod.rs:446-494; tauri.conf.json:22; toml 0.8.2 is in Cargo.lock; `state::atomic_write` is at state.rs:548 and takes `&Value`, so widening it is needed as the design says; ipc.rs:107/195/295/441; pipeline workspace callers.

Findings: 2 Blocker, 6 Major, 6 Nit
