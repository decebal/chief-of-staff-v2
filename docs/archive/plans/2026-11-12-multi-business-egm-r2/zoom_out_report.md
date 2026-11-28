Zoom Out review: multi-business design. Verdict: needs revision (0 Blocker, 4 Major, 5 Nit)

Target: /Users/decebaldobrica/Projects/chief-of-staff-v2-wt/multi-business/docs/plans/2026-11-12-multi-business-design.md
Read: persona zoom_out.md, zoom-out-standard.md, brief, design (full), plus code: Cargo.toml, Cargo.lock, src/main.rs, src/state.rs, src/agents/mod.rs, src/agents/slots.rs, src/report.rs, apps/desktop/src/{lib.rs,settings.rs}, apps/desktop/Cargo.toml, launchd/*.plist, Taskfile.yml.

Context sufficiency: passes. Objective and motivation are clear from Terms, Problem and goals, and the brief. Root problem is addressed: one resolver for the whole triple replaces the workspace-only `set_workspace`. Reuse claims check out: `toml` 0.8.2 is in Cargo.lock (line 3733), `state::atomic_write` exists (state.rs:548), `Status::terminal()` exists (agents/mod.rs:127), and the desktop app already depends on the `cos` crate.

---

**1. Severity: Major**
Affected Scope: CLI `cos business` last bullet (lines 143-146); Context launchd bullet (lines 68-69); Risks.
Simplicity Rationale: The launchd design contradicts itself. Both plists always set `COS_HOME` in `EnvironmentVariables` (launchd/cos.brief.plist:14, cos.dash.plist:15), and `task schedule` fills it with ROOT_DIR (Taskfile.yml:194). This breaks two things:
- `task schedule BUSINESS=<name>` adds `--business <name>`, which hits resolver rule 1: "--business and COS_HOME both set". The brief job would fail every weekday. The dash job has `KeepAlive`, so it would crash-loop.
- Without BUSINESS, rule 2 pins the jobs to the install home. They never follow `cos business use`. That contradicts line 143, "Long-lived processes started with neither bind the active business", and the brief's "newly started processes" use the default.

Log paths are also baked to `@COS_HOME@/out/logs`. Simplest fix: drop the `COS_HOME` env from the plists, or emit it only when BUSINESS is unset, and decide where logs go. Then state that the plists change, not that they are "unchanged".

**2. Severity: Major**
Affected Scope: Terms "App business" (lines 18-19); Desktop bullets 1 and 5 (lines 150-154, 162-164); Side-effect inventory row 2.
Simplicity Rationale: The doc disagrees with itself and with the brief. Terms says the app business "starts as the active business". The brief says "the app starts on that default and then switches on its own". The Desktop section instead persists `business` in `settings.json`, so the app restarts on its last pick, not on the active business.

This adds state the goals do not need: a persisted field, a `settings.json` write in `switch_business`, an extra sink row, and extra test cases. YAGNI: keep the app business in memory only and start from `resolve()` (the active business). Then `switch_business` writes nothing, and the "App and CLI disagree" risk shrinks to one session. If persisting it is wanted, record that as a decision and fix the Terms line.

**3. Severity: Major**
Affected Scope: Design "Before a command reads profile or state..." (lines 124-126).
Simplicity Rationale: This adds a new global gate that duplicates existing checks and breaks an intended degraded path:
- Missing-state errors already exist at main.rs:731, main.rs:777, track.rs:195 and mcp.rs:185.
- `report::workspace` deliberately returns `Null` when there is no state (report.rs:77-80). That drives the onboarding prompt "cos state init" (report.html:787).
- A blanket pre-check before any profile or state read would turn `report`, `dash` and the app's report view into hard errors for a business without a profile.

Structural fix: update the existing messages to name `--workspace <dir>`. Keep the profile check only where the design already places it (`switch_business`, `business show`).

**4. Severity: Major**
Affected Scope: Desktop badge bullets (lines 165-168, 177-179); Alternatives rejected.
Simplicity Rationale: The brief's user decision says the badge counts agents in other businesses "that switches to them". The design makes it "Not clickable" and does not record why under Alternatives rejected. Either list it as a deliberate cut (a fine YAGNI cut: the picker already switches) or honour it.

Small reuse point: `count_live` could also call the read-only `is_cos(pid)` check that `reconcile` uses (agents/mod.rs:474). That drops dead supervisors without writing anything, so the label would not need to hedge with "about".

**5. Severity: Nit**
Affected Scope: Registry "Write" bullet (lines 95-97); Risks "Lost update".
Simplicity Rationale: A lock file plus re-read for a single-user config that changes only through hand-run `cos business` commands is close to speculative. `atomic_write` (pid+nanos temp name, already unique at state.rs:554) prevents corruption on its own. If the lock stays, reuse the existing `flock` helper in src/agents/slots.rs:44 by making it `pub(crate)` or moving it to a shared module, rather than writing a second one. "With a unique temp name" is already true today, so it is not a change.

**6. Severity: Nit**
Affected Scope: Registry "Format" bullet (line 98); Alternatives "JSON registry".
Simplicity Rationale: Cargo.lock holds both `toml` 0.8.2 (pulls toml_edit 0.20.2) and `toml` 1.1.8. Either one costs no new crate. Pinning the cos crate to the superseded 0.8 line, which is probably only a transitive Tauri build dependency, is a small dead-end. Prefer the current major, or say why 0.8. The version and feature list is also implementation detail for a design doc.

**7. Severity: Nit**
Affected Scope: CLI `cos business show` (lines 132-133).
Simplicity Rationale: The brief asks for `list | add | use | remove`. `show` partly duplicates `list`'s `current:` line and `add`'s missing-profile report. Fold the per-directory existence and profile flags into `list`, or justify `show` as needed now.

**8. Severity: Nit**
Affected Scope: Acceptance 1-2 (lines 332-335) vs brief "Migration".
Simplicity Rationale: The brief says "on first run the current triple becomes a business named `default`". The design is lazier: there is no file until the first `add`, and `default` stays implicit through resolver step 4. That is simpler and fine, but the deviation is not stated as one.

There is also a gap. The `default` seeded by `add` comes from the CLI's resolved triple. A desktop install whose legacy `settings.json` triple differs from it ends up with a `default` business that does not match what the app was showing. The doc should name that case.

**9. Severity: Nit**
Affected Scope: Context (lines 50-73); Desktop UI bullet (lines 169-172).
Simplicity Rationale: Some sections drop in altitude. The dense line-number inventories and keystroke-level picker behaviour belong in the implementation plan or tests, and the slug regex belongs with the tests. They are not wrong, but they crowd out the data-flow picture.

---

Findings: 0 Blocker, 4 Major, 5 Nit
