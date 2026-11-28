**Zoom Out review, Stage 8 (Intermediate): `/Users/decebaldobrica/Projects/chief-of-staff-v2-wt/multi-business/docs/plans/2026-11-12-multi-business-design.md`**

Mode: REVIEW, whole doc. Standard: `/Users/decebaldobrica/.claude/plugins/cache/egm/egm/0.1.1/resources/document_standards/zoom-out-standard.md`. Read: persona, standard, design, brief, all 6 prior reports. Code checked in `/Users/decebaldobrica/Projects/chief-of-staff-v2-wt/multi-business`.

**Context check: pass.** Goal and why are clear, and the root fix (one resolver, one layer per triple) is right.

The earlier reports agree on these:
- Run-status vocabulary is wrong.
- Migration source depends on which process runs first.
- launchd log paths are unbuildable.
- Badge semantics are undefined.
- `state init` contradicts itself.
- Spawn env strips the business that nested `cos` calls need.

None of these is fixed in the design yet. Below, each item is consolidated and given the simplest fix. Some mitigations from earlier stages add more machinery than needed, and I reject those.

---

**1. Severity: Blocker**
- **Affected Scope:**
  - Terms "Home".
  - CLI `add`.
  - Acceptance preconditions (line 393) and Acceptance 8.
  - This promotes critic_design #1.
- **Simplicity Rationale:**
  - **Root cause:** "home" mixes files the install owns with business data.
    - `claude::command_with` sets `current_dir(home)` and passes `--setting-sources project` (`src/claude.rs:111-128`). So `CLAUDE.md` and `.claude/agents/tactician.md` must exist inside every home.
    - `context/` is seeded only by `task setup` running `cp -Rn templates/context/ context/` (`Taskfile.yml:17`).
    - A new `~/biz/acme` has neither. `cos --business acme brief` (the brief's headline job) and Acceptance 8 cannot work. Acceptance 3 avoids this by using `report`.
  - **Install is already a de facto fourth fixed path.** Layer 4 points at the build clone, `cos_binary()` comes from the install, and the brief says kit templates "resolve from the install".
  - **Simplest structural fix: name the install once.**
    - `claude` runs with the install as its cwd, so agent definitions and `CLAUDE.md` come from there. This is safe because brains have no file tools and context is inlined (`CLAUDE.md:48`).
    - A new home's `context/` is seeded from the install's `templates/context`, reusing the `task setup` copy. Either `add` does it, or the docs say to.
  - Critic_design's options (b) "scaffold a full home" and (c) "warn only" are weaker. (b) copies install files into every business. (c) leaves the brief's main job broken.

**2. Severity: Major**
- **Affected Scope:** Desktop, last bullet (lines 215-217); Testing (lines 367-368). This is still open from zoom_out F1.
- **Simplicity Rationale:**
  - The supervisor's children inherit its env. Only `PATH` (`supervise.rs:165`) and `COS_SEED_REPO` (`seed.rs:115`) are changed.
  - Once every `COS_*` triple var is stripped, any `cos` an agent runs (flows, state) binds the **active** business, not the one it was spawned in. That is the mixing bug again.
  - The proposed `--workspace`/`--projects` flags on `supervise` are dead: `Supervise { id }` takes only `id` (`main.rs:290-291`).
  - **Fix:** one mechanism. Set the child's `COS_HOME`, `COS_WORKSPACE` and `COS_PROJECTS` to the spawning triple, and remove only `COS_BUSINESS`. The test asserts that a nested `cos` resolves the spawning triple.

**3. Severity: Major**
- **Affected Scope:**
  - Long-lived processes and launchd (lines 170-178).
  - Risks "MCP and launchd after `use`".
  - Merges critic_design #2 and #3, comprehension #2 and structural #2.
- **Simplicity Rationale:**
  - The "follow the active business" mode for plists causes all three problems:
    - Log paths are fixed when the plist is generated (`cos.brief.plist:24-25`, `cos.dash.plist:20-21`).
    - `cos.dash` uses KeepAlive and never restarts, while `cos.brief` follows `use`, so the two silently drift apart.
    - "the job creates" its log dir is impossible, because launchd opens those paths before `cos` runs.
  - The brief already says jobs are "chosen at launch with `--business`".
  - **Simplest fix:**
    - `task schedule` always bakes a business into both plists: `BUSINESS=` if given, else the active business at schedule time.
    - It fills the log paths from that business's home and keeps its `mkdir -p`.
    - Switching jobs then means rerunning `task schedule`, as the brief says.
  - This drops a mode, a risk and a log-path redesign. Critic_design's "`use` kickstarts dash" adds a launchctl dependency to a registry write; reject it.

**4. Severity: Major**
- **Affected Scope:**
  - First-run migration (lines 147-153), Acceptance 1.
  - Merges zoom_out F2, comprehension #5 and critic_design #4.
- **Simplicity Rationale:**
  - There are two writers with three possible sources: flags, env, or legacy `settings.json`. The result depends on which runs first, and a one-off `--home /tmp` can become the permanent `default`.
  - **One rule removes the ordering problem:**
    - Migration writes `default` from layer 4 only, never from flags or env.
    - At app start, if the legacy `settings.json` directories exist and match no registered business, the header shows once `cos business add <name> <home> <ws> <projects>`. Nothing is migrated from there automatically.
  - The registry then has one deterministic source. Test it app-first and CLI-first.

**5. Severity: Major**
- **Affected Scope:**
  - Terms "Run record", Departures line 63, Desktop badge (lines 193-197, 211-212), Risks "Badge cost", Acceptance 5.
  - Merges comprehension #1 and #6, catalyst #2 and #3, critic_design #5, structural #1 and #7, style #4.
- **Simplicity Rationale:**
  - The status list is wrong. The actual set is `Done/Failed/Killed/TimedOut/Lost`, with no `stopped` (`mod.rs:110-131`).
  - `reconcile` writes a terminal `Lost` (`mod.rs:489-492`). If the badge counts `lost` *status*, it never clears, because runs are not pruned.
  - **Simplest definition:**
    - `count` considers non-terminal records only.
    - `running` means a live supervisor.
    - `lost` means non-terminal with a dead supervisor, i.e. "needs cleanup; open it".
    - Each part is printed only when non-zero.
  - Reject catalyst #2's asynchronous fill, live-run index and slot probing. They are premature optimisation before the 200 ms measurement exists.
  - Reject catalyst #4's repair-on-count. It breaks the read-only guarantee, which is the cheap part of this design.

**6. Severity: Major**
- **Affected Scope:** Desktop menu accelerator (lines 201-205); UI hand-check (line 376). Endorses critic_design #6.
- **Simplicity Rationale:**
  - The desktop crate has no menu today (grep finds 0 `menu` hits in `apps/desktop/src`).
  - Setting a menu replaces macOS's default. Without the Edit items, ⌘C/⌘V/⌘Z/⌘A die in the Hire prompt and the docs room.
  - **Fix:** use the platform default menu plus one appended item. Do not build a custom menu. Add copy and paste to the hand-check.

**7. Severity: Major**
- **Affected Scope:** Desktop switch (lines 187-188, 206-210). Critic_design #7, with a simpler fix.
- **Simplicity Rationale:**
  - The docs frame forwards `cos-docs-save` through `app.js:158-160` to `docs_save`. A save that lands after `switch_business` writes A's organisation into B's `context/`.
  - Critic_design proposes a business-name guard on every write command. That widens many IPC signatures.
  - **Simpler fix:** the picker sets a `switching` flag in `app.js`, drops or awaits frame saves, and only then calls `switch_business` and reloads. That is one place, and one IPC test.

**8. Severity: Major**
- **Affected Scope:**
  - Resolver lines 141-143; Violations row "internal commands and `state init` still run"; brief Assumption "`cos --business x state init`".
  - Still open from zoom_out F3; also critic_design #10.
- **Simplicity Rationale:**
  - "Does not resolve" and "the resolved workspace" contradict each other.
  - Today every `state` subcommand gets `cli.home` and `ws` (`main.rs:349`).
  - **Fix, one rule:** `state init` resolves like any command but skips the profile check. On a broken registry it accepts only `--workspace`.

**9. Severity: Nit**
- **Affected Scope:** Catalyst #1, #5 and #6; critic_design #13.
- **Simplicity Rationale:** These are YAGNI for this change. Do not adopt them:
  - Newtypes `Home`/`Workspace`/`Projects`: signature churn across 5 modules plus the desktop crate. The renames and the characterisation test already cover the known sites.
  - Pin override "switch anyway".
  - `add --init`.
  - `--business` in every hint.

  If one of them becomes a real need, revisit after shipping.

**10. Severity: Nit**
- **Affected Scope:** Registry write (lines 117-120). Merges zoom_out F4, catalyst #7 and critic_design #12.
- **Simplicity Rationale:**
  - `slots.rs:44` `flock` is private to the agents module, and `atomic_write` takes `&Value` (`state.rs:548`).
  - Move both into one small shared module, rather than having `business.rs` depend on `agents::slots`.
  - Refuse a symlinked registry, as `state::load` does. Do not add symlink-following logic.

**11. Severity: Nit**
- **Affected Scope:** Terms "Active business" (line 27); `list` output (line 158); start line (line 144). Merges style #3 and structural #6.
- **Simplicity Rationale:** "default" means three different things. Label layer 4 `built-in`, and define Active business as "the registry's `active` entry".

**12. Severity: Nit**
- **Affected Scope:** Desktop `AppState { settings, triple }` (line 187); Security line 271. Still open from zoom_out F5.
- **Simplicity Rationale:**
  - Once `set_workspace` goes, nothing writes `settings.json` (`lib.rs:75-77` is the only `.save(`).
  - Keep `settings` immutable, put only `triple` behind the lock, delete `Settings::save`, and correct the Security claim.

**13. Severity: Nit**
- **Affected Scope:** `remove`, `add --replace` (lines 160-167). Critic_design #8.
- **Simplicity Rationale:** Refuse while `count(old home)` is non-zero. That reuses the badge's `count`, so it costs no new mechanism.

**14. Severity: Nit**
- **Affected Scope:** Context line 70; regex, `std::path::absolute`, `default_value_os_t`, `deny_unknown_fields`. Still open from zoom_out F7.
- **Simplicity Rationale:** This is implementation detail inside the architecture sections. Move it to the plan.

---

**Verdict:** revise.
- Fix 1 (install vs home) before planning. It is the only gap that breaks the brief's main job.
- Fixes 2-5 each remove a mode or a moving part rather than adding one.

Findings: 1 Blocker, 7 Major, 6 Nit
