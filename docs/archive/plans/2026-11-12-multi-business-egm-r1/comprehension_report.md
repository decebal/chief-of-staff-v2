The report file was not written: this reviewer is read-only and has no write tools. The parent agent must save the text below to `/Users/decebaldobrica/Projects/chief-of-staff-v2-wt/multi-business/docs/plans/2026-11-12-multi-business-egm/comprehension_report.md`. The user's "modify the skill" request is not something a read-only reviewer can do; it goes back to the parent. Stage 6 reviews the document alone, so no claims were checked against the code.

---

**What this proposes**
A file at `~/.config/cos/businesses.toml` lists named businesses, each with home, workspace and projects paths, plus one active name. One function, `business::resolve`, picks the three paths for every command. For each path the order is flag, then env, then `--business`/`COS_BUSINESS`, then the registry's active entry, then today's default. The CLI gains `cos business list/show/add/use/remove` and a global `--business`. In the desktop app, a ⌘K picker replaces the workspace-path dialog. It switches the active business and resets every per-business cache. A badge counts agents running in other businesses. Long-running processes are tied to one business at launch. Two bugs that read the profile from home instead of workspace are fixed first, and misnamed `home` parameters are renamed.

**How the existing system works, per this document**
The CLI resolves home, workspace and projects separately (flag, then env, then default), and workspace falls back to home. The app keeps its own `settings.json`, where env beats saved values beats defaults. It writes env values back to that file. It can switch only the workspace at runtime, through `set_workspace`, and it reads its settings on every call. Agents and run slots belong to a home. The build lock covers the whole machine. Timezone and the grounding gate read the profile from home, which is wrong.

---

1. Major. Lines 7-14, 18-35. The document never defines home, workspace, projects or "profile". The whole design and both bug fixes depend on the difference between them. A reader can't tell why the profile belongs in the workspace, or why a business would have home ≠ workspace.
2. Major. Lines 104-106, 206. "the map" (as in "every per-business cache the map found") is never defined or linked. The reader can't check whether the cache list is complete.
3. Major. Lines 58 vs 141, 175, 182. The write steps at line 58 (tmp, fsync, rename) say nothing about `.bak`. Lines 141, 175 and 182 say a `.bak` is kept on every write. The two descriptions of the write disagree.
4. Major. Lines 135-187. "Semantic ISA" uses vocabulary that is never defined or linked: Core (Memory/Normative/Execution), "Governance property", "Sink", and `LOAD`/`VERIFY`/`STORE`/`TOOL_CALL`.
5. Major. Lines 86-87. `add` requires `profile.json` and otherwise tells the user to run `cos --business <name> state init`. But that business isn't in the registry yet, so `--business` should fail. Bootstrapping a new business is circular.
6. Major. Lines 61-62 vs 120-121. Migration seeds the default business from "app settings". That means the CLI reads the app's Tauri config path, which is the very reason the settings.json alternative was rejected.
7. Major. Lines 72-81. Precedence is per directory, so `COS_HOME` plus `--business acme` mixes paths from two sources. The document doesn't say whether that is intended or which business name is returned (line 70 covers only the case where flags define everything). Also, `--home` alone used to make workspace = home; now the workspace may come from the active registry entry. That behaviour change is not stated.
8. Major. Lines 99, 11. `switch_business` writes `active` to the shared registry, so switching in the app also changes the CLI's default and the business any newly launched process picks up. The document only says it "reloads every view in the window" and never states this global effect.
9. Major. Line 12. launchd jobs are said to take `--business`, but no section covers how existing jobs or plists are created, updated or migrated.
10. Nit. Lines 61-62. "the first command that needs it" is vague: which commands need the registry when flags already give all three paths?
11. Nit. Line 9. "the triple" is used before it is defined (it can be inferred as home/workspace/projects).
12. Nit. Lines 22, 34, 124. "slots", "run slots" and "slot cap" are undefined.
13. Nit. Line 98. "reconcile left lazy" is undefined.
14. Nit. Line 109. "brief or ask" is undefined.
15. Nit. Line 12. `dash`, `mcp`, `serve` and `tunnel` aren't described.
16. Nit. Line 201. `pr-proof` is undefined.
17. Nit. Lines 205-208. The confidence labels "rock" and "sand" are undefined.
18. Nit. Line 166. "not a symlink to elsewhere once canonicalized" is vague, and what `state::runtime_dir` checks is not stated.
19. Nit. Lines 198-199. `#[serde(default)]` doesn't make Rust struct literals compile, so the reason given is unclear.
20. Nit. Line 64. "0.8.2 or 1.1.8, whichever the lock resolves" is ambiguous.
21. Nit. Line 217 vs 106. Acceptance 3 checks "plan", but the reload list at line 106 doesn't include plan.
22. Nit. Line 34. Why the machine-wide build lock matters to this design is not explained.

blocker 0, major 9, nit 13
