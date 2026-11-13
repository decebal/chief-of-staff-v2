I couldn't write `/Users/decebaldobrica/Projects/chief-of-staff-v2-wt/multi-business/docs/plans/2026-11-12-multi-business-egm/critic_design.md`. No write tool here, and the persona is read-only. Findings below for the caller to save. Your "modify the skill" request is outside this review, so I made no edits.

REVIEW, feature-spec-standard, design.md 1..End. All target, brief, persona and standard files read.

**BLOCKER**
- B1 design.md:72-81, 12-13, 95-96. Precedence is per directory, so one env/flag override plus registry `active` builds a mixed triple. launchd/cos.brief.plist:14 and launchd/cos.dash.plist:15 set only `COS_HOME` and no `--business`. The design lists no plist or Taskfile change. Result: home comes from env, workspace and projects come from `active`, so they follow the app's last switch. The morning brief then runs on business A's home with business B's profile. The `ws()` fallback "for a triple built from flags" (design.md:80-81) is not defined when only one directory is overridden. Same mix happens when the app is env-pinned and `switch_business` runs. Fix: `--business`/`active` picks the whole triple; a partial override with `--business` is an error, or the per-directory fallback is spelled out; plists pass `--business`; add a test row for the mixed case.
- B2 design.md:85-87 vs :166. `add` needs `profile.json`. The suggested fix `cos --business <name> state init` fails because the name is not in the registry yet. Fix: suggest `cos --workspace <path> state init`, or add an `add --init`.

**MAJOR**
- M1 design.md:97-98. "agents::list, reconcile left lazy" is false. `agents::list` calls `read`, which calls `reconcile` (src/agents/mod.rs:446-465, 469+). That runs `ps` per live agent, marks agents lost and stops harnesses. The badge would change other businesses' agents, and this is missing from the side-effect table at :139-144. lib.rs:478 already says list is too costly per tick. Needs a counter that does not reconcile, with a defined staleness.
- M2 design.md:58 vs :141, :175, :182. `.bak` appears in the ISA tables but not in the write path. No lock on the registry: CLI `use`, app `switch_business` and migration race. A fixed `businesses.toml.tmp` name collides between writers, and read-modify-write can lose an update. Fix: flock, a unique tmp name, then rename.
- M3 design.md:61-63. Migration runs in whichever process comes first: launchd, the supervisor (mod.rs:423 `--home`), or a test binary. The CLI can't read the app's settings.json (rejected alternative at :120-121). So `default` can be saved from a one-off env/flag triple and the app's saved workspace is lost. Existing CLI tests would also write the real ~/.config/cos/businesses.toml. Fix: migrate only from the app or an explicit `cos business` command, never from env/flag values; set `COS_BUSINESSES` for all tests.
- M4 design.md:109-110. The app has no brief or ask command (lib.rs:528-546; app.js invokes none) and no status line, so the claim has nothing to attach to. The real hazard is not covered: async invokes (app.js:249, 271-274, 677, 1087) hold the old triple and resolve after `resetForBusiness`. Old data then renders in the new business, and `agents_stop`/`agents_clean` act on the wrong home. Needs a generation token that drops late responses.
- M5 brief.md:18-19 vs design. The running app never re-reads the registry. A CLI `use` while the app is open leaves CLI and app silently out of sync, and the app's next switch overwrites it. Specify file watching, or state that the app reads `active` at start only.
- M6 design.md:198-199. "`#[serde(default)]` so the existing tests compile" is wrong: serde attributes don't affect Rust struct literals. tests/ipc.rs:107, 295, 441 build `Settings` with home/workspace/projects, and ipc.rs:195 calls `set_workspace`, which is being removed. The plan doesn't cover either. Also unspecified: how a test-injected `Settings` (lib.rs:503-505) carries a triple once those fields are gone.
- M7 design.md:99, :141. An app switch rewrites the global `active`. That moves every later CLI command, every new `cos mcp` that Claude Desktop starts, and launchd runs without `--business`. The side-effect row ("the user: the wrong business becomes active") understates this.
- M8 Standard gaps. No Risks & Mitigations section; no Goals/Non-goals (left to the brief); Confidence (:203-209) covers only 3 items. Alternatives (:118-125) skip the key choices: whole-triple vs per-directory precedence, a shared `active` vs one local to the app, and registry location.

**NIT**
- N1 design.md:166. "not a symlink to elsewhere once canonicalized" is undefined. `runtime_dir` already canonicalizes (state.rs:519-531), and macOS symlinks like /tmp and /var would trip the check. Define the rule.
- N2 design.md:64-65. Cargo.lock has both toml 0.8.2 (:3733) and 1.1.8 (:3745), and the lock is shared across the workspace (Cargo.toml:10-11). Pick one, and name the features, which might pull in new crates.
- N3 design.md:150. Names read from a hand-edited registry are not slug-checked on LOAD. Check them on LOAD and use textContent in the picker.
- N4 design.md:104-106. The reload list leaves out the plan view, which Acceptance :217 requires. `ui.deliverables` (app.js:1053) and the `brief_summary` data are missing from the reset list.
- N5 Two businesses can share a home. Nothing refuses it, and they would then share out/agents and the slot pool (slots.rs:40-41), so the badge double-counts.
- N6 Unspecified CLI behaviour for `--business X` when X's directories are missing; the dirs check only runs at the switch and STORE sinks.

Checked and accurate: pipeline.rs:120, 184; main.rs:775; track.rs:66; context.rs:73, 204, 213, 230; settings.rs:45-47; lib.rs:59-80; app.js:93-132.

Counts: blocker 2, major 8, nit 6
