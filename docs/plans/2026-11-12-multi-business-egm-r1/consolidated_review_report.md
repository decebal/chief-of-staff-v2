# Consolidated EGM review: multi-business design

Target: `docs/plans/2026-11-12-multi-business-design.md` (brief: `2026-11-12-multi-business-brief.md`).
Reports: zoom_out (ZO), inventive_catalyst (IC), comprehension (CP), critic_design (CD), critic_structural (CS), critic_style (Style), intermediate_zoom_out (S8, Stage 8 zoom-out).
Design not edited.

Result: 3 blockers, 8 majors, 8 nits. Raw findings: 110, folded into these 19 items.

Verdict: not ready. The three blockers are one design fault (per-directory precedence and a shared `active`) plus a bootstrap contradiction. Fix those and the majors are mostly text edits.

Note on the Stage 8 report: it says no write tools. This reviewer has Write, and the two files were written here. S8's text is saved verbatim at `intermediate_zoom_out_report.md` in the same folder.

## 🔴 Blockers

### B-1. Precedence resolves each directory separately, so the triple can mix businesses
- **Design section:** Design > One resolver (design.md:67-81); Desktop (:94-96); Decision (:12-13).
- **Problem:** Each of home, workspace and projects is resolved alone: flag > env > `--business` > `active` > default. A partial override therefore mixes two sources. Example: `COS_HOME` set, `active` = acme. Home is default's, workspace and projects are acme's. This is the mixed-triple bug the brief exists to remove (brief.md:9-11). The design does not say what happens for `--home /x` plus an active entry (:70, :80-81 cover only the all-flags case).
- **Evidence:** launchd/cos.brief.plist:14 and launchd/cos.dash.plist:15 set only `COS_HOME` (per S8, CD B1, CS B1, IC F1; not re-checked here). Design has no plist or Taskfile change (Taskfile.yml:194). App under an env pin plus `switch_business` gives the same mix (:95-96).
- **Fix:** Resolve the triple as one unit.
  - If any directory flag or `COS_HOME`/`COS_WORKSPACE`/`COS_PROJECTS` is set, and no `--business`: build all three from flags and env, with the `Cli::ws()` fallback, and ignore the registry.
  - Otherwise: `--business` or `active` sets all three.
  - `--business` together with any directory flag or env is an error.
  - App with an env pin: the picker is disabled or refused with a message that says so. Write this down; the current text (:95-96) only covers `COS_HOME`.
  - Side effect: the existing plists keep working unchanged. No `@BUSINESS@` placeholder is needed.
  - Test row: `COS_HOME` set, `active` ≠ that home, result is one coherent triple.

### B-2. App `switch_business` writes the global `active`
- **Design section:** Desktop (design.md:99); Decision (:4); Design > Registry (:18-19 in the description).
- **Problem:** The window's current business and the default for every CLI, launchd job and MCP process become the same state. Browsing in the picker changes what terminals and scheduled jobs use. It also contradicts the design's own rule that long-lived processes pin `--business` at launch (:11-12), because `cos mcp` is registered with no env and no `--business`.
- **Evidence:** design.md:99; Taskfile.yml:65,74 (`cos mcp`, no env, no `--business`); IC F2; CP#8; CD M7; S8 blocker 2. The running app never re-reads the registry (CD M5, brief.md:18-19), so a CLI `use` leaves the app stale and the next app switch overwrites it (CS risks bullet).
- **Fix:** The app keeps its own current business in `settings.json`, seeded from `active` at startup. Only `cos business use` writes `active`. The app shows "view differs from default" in its header.
  - This removes the need for a lock (IC F6, CD M2), file watching (CD M5), and a split `active` file (IC F6). Rare writers plus a unique tmp name and rename are enough.
  - Decide and state: `cos mcp` with no env binds `active` at launch. Or `install-mcp` passes `--business`.

### B-3. Bootstrap and validation contradict each other
- **Design section:** Design > Registry migration (design.md:61-63); CLI `add` (:85-87); ISA sink check (:166-167); Alternatives (:120-121).
- **Problem:**
  - Migration writes `default` through `STORE(registry)`, and `VERIFY(business_dirs)` requires `profile.json`. On a fresh install there is none, so migration fails and every command fails.
  - `add` says to run `cos --business <name> state init`, but `<name>` is not registered yet, so `--business` errors.
  - Migration takes its triple from "app settings", which is the coupling rejected at :120-121.
  - The migrated `default` is never validated, yet Acceptance 3 (:217-218) switches back to it.
  - Migration runs in whichever process starts first (launchd, supervisor `--home`, a test binary). Tests would write the real `~/.config/cos/businesses.toml`.
- **Evidence:** design.md:61-63, 85-87, 120-121, 166-167; main.rs:775-778 (`state::profile(home)`); settings.rs:45-47 (parse failure ignored, then overwritten by `save()` at :96); src/main.rs:18-23 (default workspace may lack `profile.json`).
- **Fix:**
  - `STORE` checks only that paths are absolute and exist.
  - The profile check moves to use time. Error text names `cos --workspace <p> state init`.
  - The app migrates its own settings triple. The CLI migrates only the default triple from flags, env or defaults, never from app settings or from one-off env values.
  - Tests set `COS_BUSINESSES` to a temp path.

## 🟡 Majors

### M-1. Cache reset is hand-listed, and late async responses can draw old data into the new business
- **Design section:** Desktop > UI (design.md:101-106); Confidence (:206-207); Acceptance 3 (:217-218).
- **Problem:** `resetForBusiness()` lists every cache by hand, and the design rates it "sand" ("one may be missed"). It also says "every per-business cache the map found", but the map is not in the document (brief.md:60). Requests started before the switch (`refreshAgents`, `refreshStaff`, `loadRepos`, report fetch) resolve after the reset and draw old data. `agents_stop` and `agents_clean` can then act on the wrong home. The reload list omits the plan view and `ui.deliverables`.
- **Evidence:** app.js:249, 271-274, 677, 1087 (async invokes, per CD M4); app.js:1103-1112 (listeners); app.js:1053 (`ui.deliverables`); lib.rs:26-28 (`app.get()` clones per call); lib.rs:528-546 (no brief or ask command, so design.md:109-110 has no target).
- **Fix:** Replace the reset with `location.reload()` after `switch_business`, keeping the tab in the URL hash. This removes the hand-listed cache, the stale-response race and the missing plan and deliverables. Delete the brief/ask sentence (:109-110). Do not add an epoch stamp (IC F4): it changes every read command's response shape.

### M-2. Agents badge reconciles other businesses as a side effect, and its cost is unbounded
- **Design section:** Desktop > Commands and badge (design.md:97-98, 107-108); side-effect table (:139-144); Confidence (:208-209); Acceptance 4 (:219).
- **Problem:** The design says the badge uses `agents::list` with "reconcile left lazy". That is false. `list` calls `read`, which calls `reconcile`. Reconcile runs `ps` per live agent, marks agents lost and stops harnesses. The badge would therefore change other businesses' agents, and that effect is missing from the side-effect table. The cost is per home per call, and the watcher comment already calls this "too dear per tick". Click-to-filter is extra scope.
- **Evidence:** src/agents/mod.rs:446-465, 469-484; lib.rs:478-480 (watcher comment); design.md:97-98, 139-144.
- **Fix:** Count non-terminal entries in each home's `state.json` without reconciling. Label the count "approximate". Refresh it on the existing 700 ms watcher tick or on tab open. State a per-scan time budget and measure it on the real registry before calling the badge done (Confidence). Drop click-to-filter. Acceptance 4 wording becomes "about 1 in other businesses".

### M-3. Registry writer: a third atomic writer, `.bak` contradiction, and an unstated lost-update risk
- **Design section:** Registry write (design.md:58-60); ISA side-effects (:141); violations (:175); test matrix (:182).
- **Problem:** The write rule is tmp, fsync, rename, with no backup. The ISA tables promise `businesses.toml.bak` on every write, and the test matrix checks for it. The write path also reinvents an existing helper. A fixed tmp name collides if two writers run. Read-modify-write can lose an update.
- **Evidence:** design.md:58-60 vs 141, 175, 182; src/state.rs:548-564 (`atomic_write`); src/agents/mod.rs:615-618 (`write_json`); IC F6; CS risks.
- **Fix:** Widen `state::atomic_write` to take bytes and reuse it. Use a unique tmp name. Drop `.bak`, since rename is already atomic. Remove `.bak` from :141, :175, :182. With B-2 the writers are rare, so no flock is needed.

### M-4. No delivery path for `--business` on long-lived processes
- **Design section:** Decision (design.md:11-12); CLI (:89-90).
- **Problem:** The design says `cos dash`, `mcp`, `serve`, `tunnel` and launchd jobs "take `--business` at launch". No plist, `install-mcp` or agent-install change is listed. Under B-1 the existing plists work with no change, so only MCP is left.
- **Evidence:** Taskfile.yml:48, 57-74, 194; launchd/*.plist; CP#9.
- **Fix:** After B-1 and B-2, state the remaining case in the design. `cos mcp` with no env binds `active` at launch. Or `install-mcp` writes `--business <name>`. Name the choice and add an acceptance row for it.

### M-5. Test plan rests on a false compile claim, misses the removed `set_workspace` test, and has no JS runner
- **Design section:** Testing (design.md:191-201).
- **Problem:**
  - `#[serde(default)]` does not make struct literals compile. Removing `home`, `workspace`, `projects` from `Settings` breaks every literal.
  - The design does not say where the in-memory triple lives after the change (:94 keeps `slots` and `cos` only).
  - The `set_workspace` test at ipc.rs:195 is not addressed.
  - The JS test runner condition is open. The repo has no `package.json` under `apps/desktop`.
  - No test row for the mixed-triple case (B-1).
- **Evidence:** apps/desktop/tests/ipc.rs:107-113, 195, 295, 441; design.md:94, 198-201; lib.rs:503-505 (injected `Settings`).
- **Fix:** Edit the literals, and say so. Drop the serde claim. Store the current business name in `settings.json` (B-2) and the resolved triple in memory on the app state. Replace the `set_workspace` test with a `switch_business` test. Add the B-1 mixed-triple test. State the JS hand-check steps: run the app, open the picker, type, Enter, Esc, then record with `pr-proof`.

### M-6. Required standard sections are missing or thin
- **Design section:** Whole document; Alternatives (design.md:118-125); Security (:127-133); Confidence (:203-209).
- **Problem:** No Problem statement, no Goals or Non-goals, no Risks. Alternatives skip the key choices: per-directory vs whole-triple precedence, shared `active` vs app-local current business, and registry format (TOML adds a dependency; `serde_json` is already in the tree). The rejection of per-business dashboard or MCP cites "the user's decision" with no link.
- **Evidence:** CS#2-5; CD M8; brief.md:25-28 (rejection source), brief.md:57-58 (non-goals).
- **Fix:** Keep it short. Add a 2-line Problem and a Goals/Non-goals pair, both linking brief.md. Add a Risks list of 5 items: lost update, app/CLI drift, scan cost, env pin, partial triple. Add the missing alternatives and the link. Do not copy the brief into the design.

### M-7. Core terms and ISA vocabulary are used but never defined
- **Design section:** Decision and Context (design.md:5-26); Semantic ISA (:135-187).
- **Problem:** A reader cannot tell what home, workspace, projects, profile or "the triple" mean, or why they differ. The ISA section uses Core (Memory/Normative/Execution), Governance property, Deterministic checkpoint, Sink, `LOAD`/`VERIFY`/`STORE`/`TOOL_CALL`, and `additionalProperties: false`. The last is JSON Schema wording applied to CLI arguments.
- **Evidence:** design.md:9, 135-157, 160-167; CP#1, CP#4; CS#16; Style#20.
- **Fix:** One glossary sentence per term at first use. Define the ISA vocabulary once or link the semantic-isa skill. Replace `additionalProperties: false` with "unknown arguments are rejected".

### M-8. Acceptance criteria are not measurable, and the reload list disagrees with Acceptance 3
- **Design section:** Acceptance (design.md:211-221); reload list (:104-106).
- **Problem:** Acceptance 5 says "still works" with no expected output, and `/x` does not exist. Acceptance 3 checks the plan view, which the reload list omits. Under M-1's reload fix the plan is covered, but the design still needs to say so.
- **Evidence:** design.md:217-218, 220; Style#11, CS#17, Style#18.
- **Fix:** Acceptance 5 gets the expected stdout (for example, `active` line and `env pin` note text). Acceptance 3 gets "reload shows plan". Use a real path for the env pin.

## 🟢 Nits

### N-1. Symlink rule is speculative and wrong for macOS
- **Design section:** Sink check (design.md:166).
- **Problem:** "Not a symlink to elsewhere once canonicalized" is undefined. It rejects a symlinked `~/Projects`, `/tmp` and `/var`. `runtime_dir` already canonicalizes.
- **Evidence:** src/state.rs:519-531; brief.md:48-49 (only absolute and exists).
- **Fix:** Canonicalize and store. Do not refuse symlinks.

### N-2. TOML version and module split are undecided
- **Design section:** Registry (design.md:39, 64-65).
- **Problem:** `Cargo.lock` has both toml 0.8.2 (:3733) and 1.1.8 (:3745). The design says "whichever the lock resolves". Two modules for a 4-field file.
- **Evidence:** Cargo.lock:3733, 3745-3746; Cargo.toml:10-11 (shared lock).
- **Fix:** Pick one version and name its features. Use one module.

### N-3. "Two live bugs" wording overstates
- **Design section:** Decision (design.md:13).
- **Problem:** The bugs are latent while workspace equals home, which is today's default (main.rs:42-44; brief.md:39-40).
- **Fix:** Say "latent; wrong once workspace differs from home". Keep them as a first, separate change.

### N-4. Document structure, citations and wording
- **Design section:** various.
- **Problem and fix, one line each:**
  - :59-60 garbled `settings.rs` citation. Overwrite is `settings.rs:96` (CS#11, Style#13).
  - :29 hand-written picker decision sits inside Context (CS#13).
  - :39 H3 carries two code paths. Split it (CS#14).
  - :79 `default_value_os_t` becomes `Option` by removing the attribute, not by changing it (Style#15).
  - :132-133 the other-user caveat names no decision. Drop it (Style#19).
  - :171 "Evidence" column holds error text. Rename it (Style#21).
  - :193 "across every combination" needs the count, 5 levels × 3 directories (Style#22).
  - :148-157 and :220 cite-line wording (CS#18: `main.rs:775` vs `:775-778`).
  - Implementation identifiers (`ui.agents`, `hire.tasks`) belong in code, not the design (ZO#10).
  - ISA tables duplicate rules that belong in Design (`.bak`, profile gate). Move them to Design and cite (S8-14).

### N-5. Undefined nit terms
- **Design section:** various.
- **Problem:** `slots`, `run slots`, `slot cap`, `brief or ask`, `reconcile`, `dash`/`mcp`/`serve`/`tunnel`, `pr-proof`, `rock`/`sand`, "the lock" (Why the build lock matters). Also "which commands need the registry" (:61-62) is vague.
- **Evidence:** CP#10-17, CP#22; CS#15; Style#17, Style#24.
- **Fix:** Define on first use or link. Once M-2 and M-6 land, "rock/sand" and "reconcile" are defined by those sections.

### N-6. JS test runner and hand-check steps
- **Design section:** Testing (design.md:200-201).
- **Problem:** Condition is open and the repo has no `package.json` under `apps/desktop`. Also an M-5 sub-point.
- **Fix:** State the hand-check steps. Drop the conditional.

### N-7. User-global tactician agent has the install home baked in
- **Design section:** not mentioned.
- **Problem:** `.claude/agents/tactician.md:21` and Taskfile.yml:48 hard-wire the install home for runs outside cos. The design does not mention it.
- **Evidence:** .claude/agents/tactician.md:21; Taskfile.yml:48; brief.md:50 (assumption).
- **Fix:** Note it as single-business, or state that kit and agent files stay install-scoped.

### N-8. Registry edge cases unspecified
- **Design section:** Registry, resolver, CLI (design.md:39-65, 85-88).
- **Problem:**
  - Names read from a hand-edited registry are not slug-checked on load. The picker must use `textContent` (CD N3).
  - Two businesses can share one home, so they share `out/agents` and the slot pool (slots.rs:40-41) and the badge double-counts (CD N5).
  - Behaviour for `--business X` when X's directories are missing is unspecified (CD N6).
- **Fix:** Slug-check on load. Refuse duplicate homes at `add`. Define the missing-directory error.

## Traceability matrix

| Report | Finding | Consolidated item |
|---|---|---|
| ZO | #1 circular `add`/`state init` | B-3 |
| ZO | #2 migration vs profile gate | B-3 |
| ZO | #3 per-field precedence | B-1 |
| ZO | #4 hand-listed cache reset | M-1 |
| ZO | #5 atomic_write reuse | M-3 |
| ZO | #6 `.bak` nit | M-3 |
| ZO | #7 symlink rule | N-1 |
| ZO | #8 two-module split | N-2 |
| ZO | #9 badge reconcile and cost | M-2 |
| ZO | #10 implementation identifiers in design | N-4 |
| IC | F1 mixed triple, launchd | B-1 |
| IC | F2 `switch_business` writes `active` | B-2 |
| IC | F3 hand-listed cache reset | M-1 |
| IC | F4 in-flight responses, epoch | M-1 |
| IC | F5 watcher extension for badge | M-2 |
| IC | F6 registry lock, split `active` | B-2 (drops), M-3 |
| CP | #1 undefined home/workspace/profile | M-7 |
| CP | #2 undefined "the map" | M-1 |
| CP | #3 `.bak` contradiction | M-3 |
| CP | #4 undefined ISA vocabulary | M-7 |
| CP | #5 circular `add` | B-3 |
| CP | #6 migration reads app settings | B-3 |
| CP | #7 per-directory precedence | B-1 |
| CP | #8 `switch_business` writes `active` | B-2 |
| CP | #9 launchd `--business` with no plan | M-4 |
| CP | #10 which commands need the registry | N-5 |
| CP | #11 "the triple" before defined | N-5 |
| CP | #12 "slots" undefined | N-5 |
| CP | #13 "reconcile left lazy" undefined | N-5 (see M-2) |
| CP | #14 "brief or ask" undefined | N-5 (see M-1) |
| CP | #15 `dash`/`mcp`/`serve`/`tunnel` undefined | N-5 |
| CP | #16 `pr-proof` undefined | N-5 |
| CP | #17 rock/sand undefined | N-5 (see M-2) |
| CP | #18 symlink rule vague | N-1 |
| CP | #19 `serde(default)` reason | M-5 |
| CP | #20 toml version ambiguous | N-2 |
| CP | #21 Acceptance 3 plan vs reload list | M-8 |
| CP | #22 build lock purpose | N-5 |
| CD | B1 precedence, launchd | B-1 |
| CD | B2 circular `add`/`state init` | B-3 |
| CD | M1 badge reconcile | M-2 |
| CD | M2 registry lock, tmp name, `.bak` | M-3 (lock dropped under B-2) |
| CD | M3 migration source and test writes | B-3 |
| CD | M4 in-flight async, no brief/ask command | M-1 |
| CD | M5 app never re-reads registry | B-2 |
| CD | M6 `serde(default)` and ipc.rs literals | M-5 |
| CD | M7 `switch_business` writes `active` | B-2 |
| CD | M8 standard sections, alternatives | M-6 |
| CD | N1 symlink rule | N-1 |
| CD | N2 toml version | N-2 |
| CD | N3 registry names not slug-checked | N-8 |
| CD | N4 reload list misses plan, deliverables | M-1 |
| CD | N5 shared home | N-8 |
| CD | N6 missing-directory behaviour | N-8 |
| CS | B1 precedence, launchd | B-1 |
| CS | #2 Problem section missing | M-6 |
| CS | #3 Goals/Non-goals missing | M-6 |
| CS | #4 Risks missing | M-6 |
| CS | #5 alternatives thin, unlinked rejection | M-6 |
| CS | #6 `.bak` contradiction | M-3 |
| CS | #7 migration source contradiction | B-3 |
| CS | #8 validation rules disagree | B-3 (symlink part N-1) |
| CS | #9 resolver partial override undefined | B-1 |
| CS | #10 JS runner, no package.json | M-5 |
| CS | N11 `settings.rs` citation | N-4 |
| CS | N12 toml version undecided | N-2 |
| CS | N13 decision inside Context | N-4 |
| CS | N14 H3 too long | N-4 |
| CS | N15 "reconcile", rock/sand jargon | N-5 |
| CS | N16 ISA terms and `additionalProperties` | M-7 |
| CS | N17 Acceptance 5 not verifiable | M-8 |
| CS | N18 `main.rs:775` cite range | N-4 |
| S8 | 1 precedence per directory | B-1 |
| S8 | 2 `switch_business` writes `active` | B-2 |
| S8 | 3 bootstrap and validation | B-3 |
| S8 | 4 reset by reload, `resetForBusiness` | M-1 |
| S8 | 5 badge reconcile side effect | M-2 |
| S8 | 6 third atomic writer, `.bak` | M-3 |
| S8 | 7 long-lived processes delivery path | M-4 |
| S8 | 8 test plan, false compile claim | M-5 |
| S8 | 9 symlink rule | N-1 |
| S8 | 10 toml version, module split | N-2 |
| S8 | 11 "two live bugs" overstated | N-3 |
| S8 | 12 tactician agent install home | N-7 |
| S8 | 13 JS runner hand-check | N-6 |
| S8 | 14 ISA rules missing from Design | N-4 |
| Style | #1 `serde(default)` claim | M-5 |
| Style | #2 where the triple lives | M-5 |
| Style | #3 `.bak` contradiction | M-3 |
| Style | #4 migration vs profile gate | B-3 |
| Style | #5 `add` profile refuse or accept | B-3 |
| Style | #6 per-directory precedence | B-1 |
| Style | #7 "two live bugs" latent | N-3 |
| Style | #8 "every per-business cache" vs "may be missed" | M-1 |
| Style | #9 in-flight brief/ask claim | M-1 |
| Style | #10 symlink rule | N-1 |
| Style | #11 Acceptance 5 not measurable | M-8 |
| Style | #12 confidence jargon, badge measurement | M-2 |
| Style | N13 garbled `settings.rs` cite | N-4 |
| Style | N14 toml version | N-2 |
| Style | N15 `default_value_os_t` wording | N-4 |
| Style | N16 env-pin note covers only `COS_HOME` | B-1 |
| Style | N17 "reconcile left lazy" jargon | N-5 |
| Style | N18 reload list vs Acceptance 3 | M-8 |
| Style | N19 other-user caveat | N-4 |
| Style | N20 ISA vocabulary | M-7 |
| Style | N21 "Evidence" column | N-4 |
| Style | N22 "across every combination" | N-4 |
| Style | N23 JS runner | N-6 |
| Style | N24 "triple" before defined | N-5 |

## Overshoot dropped (per S8)

- IC F4 epoch stamp: replaced by webview reload (M-1).
- IC F6 flock and split `active` file: not needed after B-2 (M-3).
- CD M5 registry file watching: not needed after B-2.
- IC F5 watcher extension for every home: too much for a badge (M-2).
- CS#2-4 and CD M8 copying the brief into the design: link plus short Risks list only (M-6).
