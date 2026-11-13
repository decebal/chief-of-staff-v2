Zoom Out (Stage 8, intermediate) review of the multi-business design. Verdict: needs revision. 1 Blocker, 7 Major, 9 Nit.

Target: /Users/decebaldobrica/Projects/chief-of-staff-v2-wt/multi-business/docs/plans/2026-11-12-multi-business-design.md (whole doc).

Read: persona zoom_out.md, zoom-out-standard.md, brief, design (full), the six earlier reports. Code checked:
- src/agents/mod.rs:410-494
- src/agents/slots.rs:80-141
- src/main.rs:16-45 and 285-300, 690-730
- src/report.rs:70-84
- src/staff.rs:70-99
- Taskfile.yml:48-199
- harness/sandbox sites in src/agents/

Context sufficiency passes. The resolver idea is the right root fix. The earlier stages agree on most defects. This pass merges them, ranks them by root cause, and adds one new structural finding (F2). What the earlier stages rightly propose not to adopt is listed at the end.

---

**F1. Severity: Blocker**
Affected Scope: Resolver rule 1 (l.108-109); CLI last bullet (l.143-146); Context launchd bullet (l.68-69); Acceptance 7.
Simplicity Rationale: Every reviewer found a crash path, and all of them come from one rule. Rule 1 turns "`--business` and any `COS_*` dir set" into a hard error. That breaks every place where env is inherited or baked in:
- launchd: both plists always set `COS_HOME=@COS_HOME@` (cos.brief.plist:14, cos.dash.plist:15; sed at Taskfile.yml:194). `task schedule BUSINESS=x` adds `--business x`, so brief fails every weekday and dash (KeepAlive) crash-loops.
- Supervisor: spawned as `cos --home <home> agents supervise <id>` (agents/mod.rs:422-425) and inherits env. A parent with `COS_BUSINESS` makes the child hit rule 1, and the agent ends up `lost` after START_GRACE.
- A user's shell with `COS_HOME` exported (what current docs teach) can never use `--business`. That contradicts the brief's "A `COS_*` env var still overrides for one command" (brief:33).

Patching each site (env_remove at spawn, conditional plist key, documenting the shell case) treats symptoms. Structural fix: layered precedence, so flags beat env and env beats the registry. Take the whole triple from the highest layer that names anything, and raise an error only for a conflict inside one layer (`--business` with `--home`, or `COS_BUSINESS` with `COS_HOME`). This keeps the no-mix property and matches the brief's "flag > env > default" (brief:44). With it:
- plists work unchanged (flag `--business` beats env `COS_HOME`);
- the supervisor works (flag `--home` beats inherited `COS_BUSINESS`);
- shell exports stop blocking.

Still to state separately: plist log paths stay at `@COS_HOME@/out/logs` (install home). Say so, or route them by business.

**F2. Severity: Major** (new)
Affected Scope: Resolver, "`main` calls `resolve` once" (l.120-122); Load errors (l.92-94); Violations row "registry unparseable … every command that needs the registry stops".
Simplicity Rationale: As written, every subcommand resolves, including the internal plumbing commands `agents supervise` and `agents build-slot` (main.rs:290-294, 713-718).
- `build-slot` is the build-tool shim. It runs on every compiler or package-tool call inside each sandboxed agent (`exec cos agents build-slot --lock … -- <tool>`, slots.rs:114-116).
- The harness inherits the supervisor's env (no env_clear in src/agents/). So each `cargo` call would load the registry, and could fail on a corrupt registry, a rule-1 conflict or an unset HOME.
- One bad hand-edit of `businesses.toml` would break builds in every running agent of every business.

These commands already take everything they need as arguments (`--home`, `--lock`). Scope resolution to the commands that consume a triple, and have internal commands skip the resolver. That means less code on the hot path and a smaller blast radius.

**F3. Severity: Major**
Affected Scope: Terms "App business" (l.20-21); Desktop bullets 1, 5 (l.150-154, 162-164); Risks "Legacy settings"; brief:19-20.
Simplicity Rationale: Four reports flag the startup rule. It is still unresolved, and the doc has three competing sources: `settings.business`, the legacy fields, and `active`, plus the env pin.
- Every existing install has home/workspace/projects in `settings.json`, because `save()` writes effective values (settings.rs:91-98). So the "legacy, one release" path is in fact the default path for every current user, and it ignores `cos business use` until they act.
- Persisting `business` also creates a dangling-name case: `remove` only refuses the *active* business. That case has no specified fallback.

YAGNI fix: keep the app business in memory and start it from `resolve()` (brief: "the app starts on that default"). Read the legacy fields only to seed `default` on the first `add` (this also closes zoom_out #8's mismatch gap), then ignore them. Effects:
- `switch_business` writes nothing;
- one sink row disappears;
- the dangling-name case is gone;
- the Terms line becomes true.

**F4. Severity: Major**
Affected Scope: Problem and goals (l.28); Desktop badge (l.177-179); CLI add (l.138-140); Acceptance 1; Violations l.277 vs brief:30-35, 50.
Simplicity Rationale: The design departs from four user decisions in the brief without saying so:
- the badge "switches to them" became not clickable;
- migration "on first run" became migration on first `add`;
- the `COS_*` override became a hard error (fixed by F1);
- `cos --business x state init` became `--workspace`.

Several of these are good YAGNI cuts. They still need one "Departures from the brief" section, with a reason for each, for user sign-off. Four of the six reports raised this.

**F5. Severity: Major**
Affected Scope: Resolver, "Before a command reads profile or state…" (l.124-126).
Simplicity Rationale: Still standing from zoom_out #3 and structural #7, and confirmed in code:
- `report::workspace` returns `Null` when there is no state (report.rs:77-80) on purpose, and that drives onboarding.
- A blanket pre-check turns report, dash and the app report view into hard errors.
- As worded, it would also block `state init`.

The existing missing-state errors already cover the real cases. Keep the profile check at `switch_business` and `business show` only, and reword the existing messages.

**F6. Severity: Major**
Affected Scope: `cos business add` (l.134-140); Acceptance 2.
Simplicity Rationale: `add --home/--workspace/--projects` collide with the global resolver flags (comprehension #1, critic_design #3). If they are the same flags, then on an empty registry:
- `default` gets seeded from acme's own directories;
- the shared-home check then refuses acme;
- Acceptance 2 fails.

Simplest fix: give `add` positional directory arguments, and seed `default` from the resolver without them (or from the legacy settings, see F3).

**F7. Severity: Major**
Affected Scope: `agents::count_live` (l.165-168); Risks "Badge cost"; Acceptance 5.
Simplicity Rationale: Two points from the earlier reports combine here.
- The read-only liveness test already exists: `is_cos(pid)` plus the START_GRACE check (agents/mod.rs:473-476). Only the repair branch (l.484-492) mutates. Reuse it so `count_live` stays read-only but exact, and "about" can go.
- The real risk is missing. Before this change the only home was always the one opened, so `reconcile` always ran on it. With several businesses, a dead supervisor in a business nobody opens leaves an orphaned harness with no deadline or budget enforcement. Add this to Risks, and show "N lost" separately from running in the badge.

Count only `running`, or label the count "live" (critic_design #14).

**F8. Severity: Major**
Affected Scope: Desktop UI, ⌘K (l.171-172); Acceptance 4.
Simplicity Rationale: The window is mostly sandboxed iframes, and keydown events inside an iframe never reach the parent's listener (critic_design #8). Use the platform's own mechanism, a Tauri menu accelerator, instead of a JS listener that needs per-frame forwarding.

**F9. Severity: Nit**
Affected Scope: Registry "Write" (l.95-97).
Simplicity Rationale: `atomic_write` already uses a unique temp name (state.rs:554), so "with a unique temp name" describes no change. If the lock stays, reuse `flock` from src/agents/slots.rs:44 rather than writing a second one.

**F10. Severity: Nit**
Affected Scope: Registry "Format" (l.98-99); Alternatives "JSON registry".
Simplicity Rationale: Cargo.lock holds `toml` 0.8.2 (transitive) and 1.1.8. Pinning a new direct dependency to the old major is a small dead-end. Either version costs no download. Pick 1.x or give the reason for 0.8.

**F11. Severity: Nit**
Affected Scope: `cos business show` (l.132-133).
Simplicity Rationale: `show` is not in the brief. Its existence and profile flags fit in `list`.

**F12. Severity: Nit**
Affected Scope: Resolver defaults (l.116-118); Testing l.303.
Simplicity Rationale: Make the app call `cos::business::resolve` with no default of its own and delete `built_home()` (settings.rs:26-28). The two then produce the same strings by construction, and the cross-crate test that cannot work as written can go.

**F13. Severity: Nit**
Affected Scope: Desktop reload (l.173-176); Confidence l.326.
Simplicity Rationale: The reload clears JS state only.
- Rust-side caches (the `flows` page cache) are safe because they are keyed by path, not because of the reload. Say so.
- The URL hash written before the reload has no reader at boot (app.js), so "same tab" needs a boot-time reader.

**F14. Severity: Nit**
Affected Scope: Risks.
Simplicity Rationale: After `cos business use`, MCP follows `active` on its next restart, while `cos.dash` stays pinned through `COS_HOME`. Add this divergence to Risks (catalyst #6), not pinning machinery.

**F15. Severity: Nit**
Affected Scope: Context (l.50-73); Desktop UI bullet (l.169-172); Semantic ISA l.255-257.
Simplicity Rationale: Altitude drops:
- line-number inventories, key-by-key picker behaviour and the slug regex belong in the plan or tests;
- the `deny_unknown_fields` claim does not hold for a bare Tauri command argument; drop it or wrap the argument in a struct.

---

Not adopted from earlier stages (YAGNI):
- catalyst #4 (running-marker files or an mtime cache for the badge): premature optimization before the badge cost has been measured.
- catalyst #5 (env seeds the app's start, then switching is allowed): adds header states and depends on env scrubbing in every child. The pin is simpler.
- catalyst #2's blanket `env_remove`: unnecessary once F1's layered precedence lands. Passing the full triple at spawn is still fine as hygiene.

Findings: 1 Blocker, 7 Major, 7 Nit
