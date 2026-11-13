# Consolidated review: multi-business design (EGM round 2)

Target: `docs/plans/2026-11-12-multi-business-design.md`
Inputs: zoom_out, inventive_catalyst, comprehension, critic_design, critic_structural, critic_style, intermediate_zoom_out (all in this directory).
Verdict: needs revision. 2 Blocker, 11 Major, 20 Nit.

Line numbers are the ones the source reports cite. This pass did not re-verify them against the design or the code.

Not adopted (from the intermediate stage, kept out on purpose):
- Running-marker files or mtime cache for the badge (catalyst #4). Measure the badge cost first.
- Env seeds the app start, then switching is allowed (catalyst #5). Keep the env pin.
- Blanket `env_remove` on every child (catalyst #2). Layered precedence (B1) covers it; passing the full triple at spawn stays as hygiene.

---

## 🔴 Blockers

### B1. `--business` plus inherited `COS_HOME` is a hard error in the launchd jobs
- **Section:** CLI last bullet (design l.143-146); Context launchd bullet (l.68-69); Resolver rule 1 (l.108-109); Acceptance 7.
- **Problem:** Both plists always set `COS_HOME` (`launchd/cos.brief.plist:14`, `launchd/cos.dash.plist:15`). `Taskfile.yml:194` fills it with `{{.ROOT_DIR}}`. `task schedule BUSINESS=<name>` adds `--business <name>`, so rule 1 fires on every run. The brief job fails every weekday. `cos.dash` (`KeepAlive`, `ThrottleInterval 30`) crash-loops every 30 s. Without BUSINESS, the jobs stay pinned to the install home and never follow `cos business use`, which contradicts l.143. The log paths (`@COS_HOME@/out/logs`) also stay in the install home. The doc says the plists are "unchanged", which is false.
- **Fix:** Pick one of two designs and write it down.
  - Layered precedence (intermediate F1): take the whole triple from the highest layer that names anything. Flags beat env, env beats the registry. Error only on a conflict inside one layer (`--business` with `--home`, or `COS_BUSINESS` with `COS_HOME`). Under this, the plists work unchanged and `--business` beats `COS_HOME`.
  - Or drop the `COS_HOME` key when `BUSINESS` is given, and route logs to the business's home (`cos --business X business show` at schedule time).
  - Either way: state which jobs follow `use` and which stay pinned, say where logs go, and add the acceptance row `task schedule BUSINESS=acme` followed by `launchctl kickstart` exits 0.
- **Evidence:** zoom_out #1; catalyst F1 and #1; critic_design #2; structural #1; intermediate F1; comprehension #5.

### B2. Agent supervisor inherits `COS_BUSINESS` and fails rule 1
- **Section:** Design, agents spawn (l.120-122, 180-181); ISA sink "app commands on the switched triple"; Testing.
- **Problem:** `agents::spawn` runs `cos --home <home> agents supervise <id>` (`src/agents/mod.rs:422-425`). The child inherits the parent's env. If the parent has `COS_BUSINESS` set (shell export, app pinned by env, or `cos mcp` / `cos dash` started with it), the child sees `COS_BUSINESS` plus `--home` and exits at rule 1 before taking a slot. The agent stays non-terminal until START_GRACE runs out, then becomes `lost`. The design lists this sink with "existing checks" and has no test for it.
- **Fix:** Layered precedence (B1) makes the explicit `--home` win over inherited env, so the child starts. As hygiene, pass the full triple at spawn and `env_remove` the `COS_*` directory vars. Add tests: spawn with `COS_BUSINESS` set in the parent reaches `running`, and spawn with `COS_WORKSPACE` set in the parent reaches `running`. Do not remove `COS_AGENT_SLOTS`.
- **Evidence:** critic_design #1; catalyst F2; intermediate F1.

---

## 🟡 Majors

### M1. App-business startup source is contradictory and unspecified
- **Section:** Terms "App business" (l.17-20); Desktop bullets (l.150-164); Violations (l.271-281); Risks "Legacy settings".
- **Problem:** Three sources compete: `settings.business`, the legacy `home/workspace/projects` fields, and `active`. Terms says the app starts on `active`. The Desktop section persists `business`, so it restarts on its last pick. `save()` writes effective values (`apps/desktop/src/settings.rs:91-98`), so every existing install carries legacy fields and resolves as explicit (step 2). Those installs ignore `cos business use` until the user acts. `remove` refuses only the active business, so a saved app business can dangle. No fallback is specified. A `settings.json` missing fields fails the non-`Option` deserialize, and `load()` drops the whole file with `.ok()` (`settings.rs:45-47`), silently losing `cos` and `slots`. The legacy path also differs from the CLI default (the seeded `default` may not match what the app showed). The legacy path with no `business` and no legacy fields is unspecified.
- **Fix:** Pick one startup rule. Recommended: keep the app business in memory and start it from `resolve()`. Persist nothing. Read the legacy fields only to seed `default` on the first `add`. Then `switch_business` writes nothing, the dangling-name case disappears, and the Terms line becomes true. If persisting is kept, add a fallback (active, then default, plus a visible error banner) and make the legacy fields `Option` with `#[serde(default)]`. Add an acceptance line and ipc tests for: saved business removed, directories gone, profile gone, registry corrupt at launch.
- **Evidence:** zoom_out #2; critic_design #4, #5; structural #4; intermediate F3; comprehension #7 (nit, folded here).

### M2. Departures from the brief are not recorded
- **Section:** Problem and goals (l.26-37); Desktop badge (l.177-179); CLI `add` (l.138-140); Acceptance 1 (l.332-333); Resolver profile check; Violations (l.277).
- **Problem:** The design says it follows the brief's decisions, then changes four without saying so:
  - The badge "switches to them" becomes "Not clickable". No reason is given under Alternatives rejected.
  - Migration "on first run" becomes migration on first `add`, and Acceptance 1 writes no file.
  - "A `COS_*` env var still overrides for one command" becomes a hard error with `--business` (B1). A user with `COS_HOME` exported, as current docs suggest, cannot use `--business` at all.
  - `cos --business x state init` becomes `cos --workspace <dir> state init`.
  Several of these are good YAGNI cuts, but they are unrecorded.
- **Fix:** Add a "Departures from the brief" section with one line per change and its reason, flagged for the user's sign-off. Add the shell-exported `COS_*` case to Risks. Name the migration deviation (including the desktop legacy mismatch from M1).
- **Evidence:** zoom_out #4; critic_design #7; structural #2, #3, #8; critic_style #1; intermediate F4 (and F1 for the override).

### M3. `add` directory flags collide with the global resolver flags
- **Section:** CLI `cos business add --home --workspace --projects` (l.134-140); Resolver step 2 (l.110); Acceptance 2 (l.334-335).
- **Problem:** The doc does not say whether `add`'s directory args are subcommand args or the global flags. If global: on an empty registry, `default` takes acme's directories from step 2, and the shared-home check then refuses `acme`. Acceptance 2 fails. `cos --business x business add …` also hits rule 1.
- **Fix:** Give `add` positional directory arguments (or rename them). Seed `default` from the resolver without them, or from the legacy settings (M1). State this in the doc and add the test.
- **Evidence:** comprehension #1; critic_design #3; intermediate F6.

### M4. Global profile pre-check breaks onboarding and conflicts with `use`
- **Section:** Resolver "Before a command reads profile or state…" (l.124-126); Violations "workspace without a profile at switch or use" (l.277); CLI `use` (l.141).
- **Problem:** The blanket check turns `report`, `dash` and the app's report view into hard errors for a business without a profile. `report::workspace` returns `Null` on purpose (`src/report.rs:77-80`), which drives onboarding ("cos state init"). The check also blocks `state init`, which is how the profile gets created. Separately, the doc does not say whether `use` checks the profile: the CLI section says no, and the Violations table says `use` is refused.
- **Fix:** Drop the blanket check. Keep the profile check only at `switch_business` and `business show`. Reword the existing missing-state errors (`main.rs:731`, `main.rs:777`, `track.rs:195`, `mcp.rs:185`) to name `--workspace <dir>`. Exempt `state init` explicitly. State whether `use` checks the profile, and make the Violations row match.
- **Evidence:** zoom_out #3; comprehension #2; structural #7; intermediate F5.

### M5. Badge count is not exact, and the status set is not stated
- **Section:** `agents::count_live` (l.165-168); Desktop badge (l.165-168); Acceptance 5.
- **Problem:** The read-only liveness test already exists: `is_cos(pid)` (`src/agents/mod.rs:668-671`) plus the START_GRACE check (`mod.rs:473-476`). Only the repair branch (`kill_group`, `write_state`, `mod.rs:485-492`) mutates. The design instead accepts a ghost count and hedges with "about N". The label counts `starting`, `queued` and `seeding` too (statuses at `mod.rs:127-131`), and the doc never lists the terminal statuses.
- **Fix:** Split `reconcile` into `classify(spec, state) -> Live | Lost | Grace` (read-only) and `repair` (mutating). `count_live` uses `classify`. Count `running` only, or label the count "live", and list the terminal statuses in the doc. Drop "about". The ISA row "no process killed" still holds.
- **Evidence:** zoom_out #4; catalyst F3 (and #3); critic_design #14; intermediate F7 (and comprehension #9 for the status list).

### M6. Orphaned harness in an unopened business is not in Risks
- **Section:** Risks; `count_live` (l.165-167); design text "counted until that business is opened".
- **Problem:** Today `reconcile` is the only thing that kills a dead supervisor's harness group, and it runs only on the opened home. With several businesses, a dead supervisor in an unopened business leaves its harness running with no deadline or budget enforcement. The doc accepts a ghost count but does not name this cost.
- **Fix:** Add it to Risks. Pick a mitigation and state it: reconcile other businesses on picker open with the read-only liveness test and an explicit opt-in kill; or surface "N lost" separately from running; or make the harness group exit when its supervisor dies.
- **Evidence:** critic_design #6; intermediate F7.

### M7. ⌘K does not fire from inside the report iframe
- **Section:** Desktop UI (l.171-172); Acceptance 4; `apps/desktop/ui/app.js:134-189`; `src/docs.html:1336-1344`.
- **Problem:** The window is mostly sandboxed iframes (report, docs, flows). Keydown events inside an iframe do not reach the parent's listener. After any click in the report, ⌘K does nothing, so Acceptance 4 is unreliable.
- **Fix:** Register ⌘K as a Tauri menu accelerator or global shortcut. The alternative is `postMessage` forwarding from each frame; the report already has a `cos-report-state` channel. Add a hand-check with focus inside each iframe.
- **Evidence:** critic_design #8; intermediate F8.

### M8. Reload and tab-restore claims are wrong or unfinished
- **Section:** Desktop (l.173-176); Confidence (l.326); Acceptance 4.
- **Problem:** `location.reload()` clears JS state only. Rust-side caches survive it. The `flows` page cache (`src/flows.rs:673-694`, static `LAST`) is safe because it is keyed by `ws` and `projects` (l.681-682), not because of the reload. Line 326 ("The reload removes the cache list") refers to a rejected alternative the line does not name. The design writes the current tab to the URL hash before reloading, but `app.js` has no boot-time reader; the default is `ui.pane = "staff"`, so "same tab" fails. The reload also discards an open hire dialog's unsaved edits.
- **Fix:** Rewrite the cache claim: "every view and JS-side cache restarts; Rust-side caches are keyed by path (`flows.rs:673`)". Rewrite l.326 to stand alone. Specify the boot-time hash reader. Warn on or block a tab switch while a dialog has unsaved edits.
- **Evidence:** critic_style #2; critic_design #9 (nit here, since the reader gap breaks Acceptance 4); intermediate F13.

### M9. Internal plumbing commands run through the resolver
- **Section:** Resolver "`main` calls `resolve` once" (l.120-122); Load errors (l.92-94); Violations "registry unparseable".
- **Problem:** Every subcommand resolves, including `agents supervise` and `agents build-slot` (`src/main.rs:290-294`, `713-718`). `build-slot` is the shim that runs on every compiler or package-tool call inside a sandboxed agent (`src/agents/slots.rs:114-116`). The harness inherits the supervisor's env. So every `cargo` call loads the registry, and a corrupt registry, a rule-1 conflict or an unset HOME breaks builds in every running agent of every business.
- **Fix:** Scope resolution to commands that consume a triple. Internal commands skip the resolver and take their inputs as arguments (`--home`, `--lock`), which they already do.
- **Evidence:** intermediate F2.

### M10. `state.json` names two different files
- **Section:** Terms (l.11-12); Context (l.67); Desktop (l.165); Security (l.222-223); ISA matrix (l.294).
- **Problem:** Terms defines `state.json` as the workspace's tracked deliverables (`.chief-of-staff/state.json`). Elsewhere it means the per-run `out/agents/<id>/state.json`, which is never defined. Context l.67 ("`reconcile` may … rewrite `state.json`") does not say which.
- **Fix:** Define both names in Terms. Use the workspace-file name for deliverables and a distinct name (for example "run record") for the per-run file, or qualify each use.
- **Evidence:** comprehension #3.

### M11. Default home derivation is built two ways; "same strings" has no mechanism
- **Section:** Resolver defaults (l.116-118); Context (l.50-57); Testing (l.303).
- **Problem:** The CLI uses `default_home()` (`src/main.rs:883`). The app uses `CARGO_MANIFEST_DIR/../..`. The doc says both "produce the same strings" but does not say how one function gets one compile-time path for both, or what "canonicalised" does when the checkout moves. The cross-crate test in `tests/business.rs` cannot call the app's resolution as written.
- **Fix:** Make the app call `cos::business::resolve` with no default of its own, and delete `built_home()` (`apps/desktop/src/settings.rs:26-28`). The guarantee then holds by construction. Drop the cross-crate test claim or move it to `apps/desktop/tests`.
- **Evidence:** comprehension #4; critic_design #12; intermediate F12.

---

## 🟢 Nits

- **N1.** Registry lock is close to speculative. `atomic_write` already uses a unique temp name (`src/state.rs:548-554`), so "with a unique temp name" is no change. If the lock stays, reuse `flock` from `src/agents/slots.rs:44`, do not write a second one. (zoom_out #5; intermediate F9)
- **N2.** `toml` version and alternative rejection. `Cargo.lock` holds 0.8.2 (transitive, line 3733) and 1.1.8 (line 3745). Pick the current major or give a reason for 0.8. Drop the feature list from the design. Note that a direct dependency is still added. The JSON-registry rejection needs its cost. The settings.json rejection needs its cost ("couples the CLI to the app's per-platform config directory"). (zoom_out #6; critic_style #6, #9; intermediate F10)
- **N3.** `cos business show` overlaps `list`. Fold its existence and profile flags into `list`, or justify it. (zoom_out #7; intermediate F11)
- **N4.** Altitude. Line-number inventories, picker keystroke behaviour and the slug regex belong in the plan or tests. (zoom_out #9; intermediate F15)
- **N5.** `serde deny_unknown_fields` does not apply to a bare Tauri command argument. Wrap it in a struct or drop the claim. (critic_design #11; intermediate F15)
- **N6.** Undefined terms: "the brain", "`cos flows`", "the docs room", "agent hiring", "supervisor", "run slots", "build lock", "tactician", "Goldfish", "EGM", "business key" (l.73). (comprehension #8; critic_style #4)
- **N7.** Wording that outruns the evidence. "coherent triple" (l.32): use "single-source triple". "cannot be threaded into the wrong one" (l.72-73): "cannot" is too strong for two `PathBuf`s. "scripts working unchanged" (l.112): cite only the plists and `Taskfile.yml:57-74`. "held off by the lock" (l.211): say "prevented by the exclusive lock and the re-read". "rock once its precedence tests pass" (l.325): rock without a condition, or sand with the condition. (critic_style #3, #4, #5, #7, #8)
- **N8.** Terminal statuses not listed. Covered by M5's fix; listed here so it is not lost. (comprehension #9)
- **N9.** ISA classification: `STORE(registry)` writes a state file but is classed Execution, while the Memory core "reads and writes state" (l.230-231 vs l.251). Add a one-line justification. (comprehension #10)
- **N10.** Taint section (l.261-263): "`cos` field ... stays as today" with no statement of what it does or how `cos_binary()` uses it at startup. (comprehension #11)
- **N11.** Acceptance 2 (l.334): `~/biz/acme` and its `repos` directory must already exist, because `add` refuses missing directories (l.135-136). Say so. (structural #9)
- **N12.** Side-effect inventory row (l.241) packs five capabilities into one cell with shared Reversible and Affected values. Give each its own row. (structural #10)
- **N13.** Headings: "Alternatives rejected" and "Risks" do not match the standard's labels. "Decision" precedes "Context". (structural #5)
- **N14.** Problem and goals is one sentence plus a link. A reader needs the brief to get the motivation. State the problem in the doc. (structural #6)
- **N15.** Shared workspace across businesses is not refused or warned on (only shared home is). Say whether it is intended. Name who creates `~/.config/cos/` and the `.lock` file on first `add`. (critic_design #13)
- **N16.** Site inventory misses `src/main.rs:428`, `567-571` and dash at `383-385`. The compiler will catch them after the fields become `Option`, but the list is incomplete. (critic_design #10)
- **N17.** After `cos business use`, MCP follows `active` on its next restart, while `cos.dash` stays pinned through `COS_HOME`. Add this to Risks. Pinning MCP is the alternative, but it needs a reinstall to retarget. (catalyst #6; intermediate F14; `Taskfile.yml:65, 74`)
- **N18.** Env pin refuses all switching. The pin is simpler, which is why it is kept; state the trade in the doc. Rejected alternative: env seeds the start and switching is allowed. (catalyst #5)
- **N19.** Badge scan cost is unmeasured and grows with all agent history (finished runs are never pruned). Say how it will be measured before the badge is called sand-to-rock. (catalyst #4)
- **N20.** `add` sets `active` on an empty registry, but the decision line says only `use` changes it (l.44). Say so. (comprehension #6)

---

## Traceability Matrix

Each source item to its consolidated ID. Source reports: Z = zoom_out, C = inventive_catalyst, P = comprehension, CD = critic_design, ST = critic_structural, S = critic_style, I = intermediate_zoom_out.

| Source | Item | Consolidated |
|---|---|---|
| Z | 1 | B1 |
| Z | 2 | M1 |
| Z | 3 | M4 |
| Z | 4 | M2, M5 |
| Z | 5 | N1 |
| Z | 6 | N2 |
| Z | 7 | N3 |
| Z | 8 | M1, M2 |
| Z | 9 | N4 |
| C | 1 | B1 |
| C | 2 | B2 |
| C | 3 | M5, M6 |
| C | 4 | N19 |
| C | 5 | N18 |
| C | 6 | N17 |
| P | 1 | M3 |
| P | 2 | M4 |
| P | 3 | M10 |
| P | 4 | M11 |
| P | 5 | B1 |
| P | 6 | N20 |
| P | 7 | M1 |
| P | 8 | N6 |
| P | 9 | N8 |
| P | 10 | N9 |
| P | 11 | N10 |
| CD | 1 | B2 |
| CD | 2 | B1 |
| CD | 3 | M3 |
| CD | 4 | M1 |
| CD | 5 | M1 |
| CD | 6 | M6 |
| CD | 7 | M2 |
| CD | 8 | M7 |
| CD | 9 | M8 |
| CD | 10 | N16 |
| CD | 11 | N5 |
| CD | 12 | M11 |
| CD | 13 | N15 |
| CD | 14 | M5, N8 |
| ST | 1 | B1 |
| ST | 2 | M2 |
| ST | 3 | M2 |
| ST | 4 | M1 |
| ST | 5 | N13 |
| ST | 6 | N14 |
| ST | 7 | M4 |
| ST | 8 | M2 |
| ST | 9 | N11 |
| ST | 10 | N12 |
| S | 1 | M2 |
| S | 2 | M8 |
| S | 3 | N7 |
| S | 4 | N6, N7 |
| S | 5 | N7 |
| S | 6 | N2 |
| S | 7 | N7 |
| S | 8 | N7 |
| S | 9 | N2 |
| I | F1 | B1, B2 |
| I | F2 | M9 |
| I | F3 | M1 |
| I | F4 | M2 |
| I | F5 | M4 |
| I | F6 | M3 |
| I | F7 | M5, M6 |
| I | F8 | M7 |
| I | F9 | N1 |
| I | F10 | N2 |
| I | F11 | N3 |
| I | F12 | M11 |
| I | F13 | M8 |
| I | F14 | N17 |
| I | F15 | N4, N5 |

Totals: 2 Blocker, 11 Major, 20 Nit.

Findings: 2 Blocker, 11 Major, 20 Nit
