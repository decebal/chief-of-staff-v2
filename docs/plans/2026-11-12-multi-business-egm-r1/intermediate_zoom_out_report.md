**Stage 8 zoom-out review of the multi-business design: not ready, 3 blockers**

I did not modify any skill. I am a read-only reviewer with no write tools, and no skill was named. That request goes back to the parent. The report file was not written either, so the parent must save this text.

**Read:** persona, zoom-out-standard, design.md, brief.md, all 6 earlier stage reports, CLAUDE.md. Code checked: launchd/*.plist, Taskfile.yml:48,57-74,194, src/agents/mod.rs:446-484, src/state.rs:548, .claude/agents/tactician.md:21.

**Context:** enough to review. Problem and goal are clear from the brief. The design targets the root problem (one triple, one precedence).

**State of the design:** none of the stage 1-7 findings have been applied. These are all still in the text: design.md:86-87 (circular `state init`), :58 vs :141/:175/:182 (`.bak`), :72-77 (precedence per directory), :198-199 (`serde(default)`), :97-98 ("reconcile left lazy").

**What the earlier reports add (where they agree)**
- 5 of 6 reports flag mixed triples from per-directory precedence (ZO#3, IC F1, CD B1, CS B1, Style#6).
- 4 flag `switch_business` writing the global `active` (IC F2, Comp#8, CD M7/M5).
- 4 flag the hand-listed cache reset plus late async responses (ZO#4, IC F3/F4, CD M4).
- 4 flag the circular `add` / `state init` bootstrap (ZO#1, Comp#5, CD B2, Style#5).
- 3 flag migration vs the dirs check / app settings (ZO#2, CD M3, CS#7).
- 3 flag the badge's reconcile side effects and cost (CD M1, IC F5, ZO#9).

**Mitigations that overshoot (YAGNI)**
- IC F4: an epoch stamp changes the response shape of every read command. A webview reload (IC F3, ZO#4) removes that whole class of bug for less.
- IC F6 / CD M2: a split `active` file plus flock. Not needed once the app stops writing `active` (B2 below). The writers left are `cos business` commands and migration, which are rare and user-driven. A unique tmp name plus rename is enough.
- CD M5: watching the registry file. Not needed if the app owns its current business.
- IC F5: extending the watcher to stamp every home. Too much machinery for a badge.
- CS#2-4 / CD M8: Problem, Goals and Risks sections. Linking brief.md and adding a short Risks list is enough. Copying the brief in is duplication.

**Blocker**
1. **design.md:72-81, 12-13, 95-96. Precedence resolves each directory separately.**
   - launchd/cos.brief.plist:14 and cos.dash.plist:15 set only `COS_HOME`. Home comes from env, while workspace and projects come from `active`. The 08:30 brief then runs one business's home against another's workspace. That is the half-switch bug brief.md:9-11 exists to kill.
   - Fix at altitude: resolve the triple as one unit. If any `COS_*`/dir flag is set, use flags+env plus the `ws()` fallback and ignore the registry. Otherwise use `--business` or `active`. Mixing a dir override with `--business` is an error.
   - Side benefit: existing plists then keep working unchanged. Taskfile.yml:194 needs no new `@BUSINESS@` placeholder, and acceptance 5 (:220) becomes meaningful.
2. **design.md:99, 4, 141. App `switch_business` writes the global `active`.**
   - "What the window shows" and "default for every CLI, launchd and MCP process" become the same state.
   - Proof: Taskfile.yml:65,74 register `cos mcp` with no env and no `--business`. After the change, Claude Desktop/Codex MCP binds to whatever the app last browsed. That silently breaks the design's own "long-lived processes take `--business` at launch" (:11-12).
   - Fix: the app keeps its own current business in `settings.json` (seeded from `active`). Only `cos business use` writes `active`. This one cut also removes the lock (IC F6/CD M2), the file watching (CD M5), and the source of CD M7.
3. **design.md:61-63, 85-87, 166-167. Bootstrap and validation do not hold together.**
   - Migration goes through `STORE`, which `VERIFY(business_dirs)` gates on `profile.json`. A fresh install has none, so migration fails and every command fails.
   - `add` points the user to `cos --business <name> state init` for a name that is not registered yet.
   - Migration reads "app settings", which is the coupling rejected at :120-121.
   - Fix at altitude: STORE checks only that paths are absolute and exist. The profile check moves to use time, with an error naming `cos --workspace <p> state init`. The app migrates its own settings triple. The CLI migrates only its default triple.

**Major**
4. **design.md:104-106, 206-207. `resetForBusiness()` is sand by the design's own rating.**
   - Reload the webview after the switch (keep the tab in the URL hash). That replaces the cache list, the unlisted plan and `ui.deliverables` gaps (CD N4), and the race with late async responses (CD M4/IC F4).
   - The "brief or ask in flight" text at :109-110 attaches to nothing. The app has no such command (CD M4). Drop it.
5. **design.md:97-98. "reconcile left lazy" is false.**
   - `agents::list` → `read` → `reconcile` (src/agents/mod.rs:450, 469-484) runs `ps`, marks agents lost and stops harnesses. The badge would then change other businesses' agents as a side effect, and that side effect is missing from the ISA table (:139-144).
   - The brief requires the badge (brief.md:29-30), so keep it. But count non-terminal `state.json` without reconciling, label it approximate, and drop click-to-filter.
6. **design.md:58-60. A third atomic writer.**
   - Reuse `state::atomic_write` (src/state.rs:548), widened to take bytes.
   - `.bak` exists only in the ISA tables (:141, :175, :182). Drop it (rename is already atomic) or move it into the Design write rule. Choose one.
7. **design.md:11-12 vs Taskfile.yml:48,65,74,194. "Long-lived processes take `--business`" has no delivery path.**
   - No plist, install-mcp or agent-install change is listed.
   - Mostly cured by B1 and B2. Whatever is left (MCP with no env) must be stated: it binds `active` at launch, or install passes `--business`.
8. **design.md:198-199. The test plan rests on a false compile claim.**
   - `serde(default)` does not affect struct literals.
   - apps/desktop/tests/ipc.rs builds `Settings` with home, workspace and projects, and calls the `set_workspace` that is being removed.
   - Where the in-memory triple lives after :94 is not specified (Style#2).

**Nit**
9. design.md:166. The symlink rule is speculative and rejects a symlinked `~/Projects` and `/tmp`. Canonicalize and store.
10. design.md:39, :64. Two-module split for a 4-field file. Pick one toml version now (Cargo.lock has both 0.8.2 and 1.1.8).
11. design.md:13. "Two live bugs" are latent while workspace = home (brief.md:39-40). Keeping them as a first, separate change is right; the wording overstates.
12. .claude/agents/tactician.md:21 + Taskfile.yml:48. The user-global agent has the install home baked in for runs outside cos. Not mentioned. Note it as single-business, or say kit and agent files stay install-scoped (brief.md:50 assumption).
13. design.md:200-201. The JS runner condition is unresolved, and the repo has no package.json. State the hand-check steps.
14. design.md:135-187. The ISA tables carry rules that are missing from Design (`.bak`, profile gate). ritual-full requires the section (CLAUDE.md:70), so keep it, but the rules belong in Design and the ISA should only cite them.

blocker 3, major 5, nit 6
