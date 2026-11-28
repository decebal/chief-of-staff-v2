Review is complete, but `critic_style.md` was not written. I am read-only and have no write tool, so the findings are below for the caller to save. The relayed request "modify the skill" is outside this reviewer's scope, so no skill or file was edited. Mode: REVIEW, style, stage 7. Standards: the persona names none, so none were applied.

**Major**
1. design.md:198-199. Claim is false. `#[serde(default)]` does not affect Rust struct literals. If `Settings` drops `home/workspace/projects` (line 94), `apps/desktop/tests/ipc.rs:107-113` will not compile whatever the attributes say. CALIBRATE: say the literals are edited.
2. design.md:94 vs 99-100. Unclear whether `Settings` keeps the triple. Line 94 says "keeps `slots` and `cos`", but `switch_business` "swaps the in-memory triple". Name where the triple lives.
3. design.md:58 vs 141, 175, 182. Write procedure is tmp+fsync+rename with no `.bak`. The side-effect table, violations table and test matrix all rely on `.bak` on every write. Contradiction.
4. design.md:61-63 vs 166. Migration writes `default` without passing `VERIFY(business_dirs)`. The default workspace (`src/main.rs:18-23`, the build's clone) may not hold `profile.json`, yet line 166 says every `STORE(registry)` needs it. Contradiction.
5. design.md:86 vs 166. It is unclear whether `add` succeeds or is refused when `profile.json` is missing. Line 86 says it "prints the `state init` to run"; the table says the store is blocked. Pick one.
6. design.md:72-77, 70, 80-81. Precedence is per directory, so `--home /x` with an active registry entry gives a mixed triple. That outcome is not stated. "`None` when flags alone define it" leaves the partial case undefined.
7. design.md:13-14 "Two live bugs". Overstated: the brief (brief.md:39-40) says they are harmless while workspace equals home, which is today's default (`Cli::ws()`, main.rs:42-44). CALIBRATE: "latent; wrong once workspace ≠ home".
8. design.md:104-105 "every per-business cache the map found". The "map" is not in the document (brief.md:60 says only that it was summarised), so the reader cannot check the list. "Every" contradicts Confidence at 206-207 ("one may be missed"). CALIBRATE.
9. design.md:109-110. In-flight brief/ask behaviour is stated as fact, but brief.md:52-53 lists it as an assumption and the design cites no evidence. CALIBRATE, or cite the code path that captures paths at spawn.
10. design.md:166 "not a symlink to elsewhere once canonicalized". Vague. Say whether symlinks are refused or resolved and stored canonical.
11. design.md:220 Acceptance 5 "still works". Not measurable: no expected output, and `/x` is a path that does not exist. Name the expected stdout.
12. design.md:203-209. "rock/sand", "Small, local, fully testable" and "moves it up" are undefined jargon and uncalibrated. The agents badge has no measurement threshold. CALIBRATE: give a time budget per scan.

**Nit**
13. design.md:59-60 "(the app's `settings.json` is today, `settings.rs:45-47`)". Garbled. The parse is silently ignored at 45-47 and overwritten by `save()` at `settings.rs:96`. Rewrite.
14. design.md:64-65 "0.8.2 or 1.1.8, whichever…". Defers the choice; both are in `Cargo.lock:3733-3746`. Pick one.
15. design.md:79. The `default_value_os_t` attribute does not "become `Option`". The field becomes `Option` and the attribute is removed.
16. design.md:95-96. Only `COS_HOME` gets the env-pinned note. Also cover `COS_WORKSPACE` and `COS_PROJECTS`, or say why not.
17. design.md:98 "reconcile left lazy". Jargon; define it.
18. design.md:106 vs 217-218. The reload list leaves out "plan", which Acceptance 3 checks. Align them.
19. design.md:132-133. The other-user-permissions caveat names no reader decision. DROP.
20. design.md:148-157. "Normative", "Governance property", "Deterministic checkpoint" and `additionalProperties: false` are undefined, and that last term is JSON Schema vocabulary applied to CLI arguments.
21. design.md:171 "Evidence" column. It actually holds error-message contents; rename it.
22. design.md:193 "across every combination". Uncalibrated; state the count (5 levels × 3 directories).
23. design.md:200-201 "if the repo has a JS test runner". It has none (only `apps/desktop/tests/ipc.rs`, no `package.json`). State the hand-check directly.
24. design.md:9 "triple" is used before it is defined. Define it on first use.

Verified, no finding: `main.rs:16-45`, `main.rs:327-335`, `main.rs:775`, `pipeline.rs:120,184`, `lib.rs:59-80`, `app.js:93-132`, `slots.rs:40-41`, `agents::root` = `home/out/agents`, CSP `script-src 'self'`, and the `track.rs:192` / `context.rs:72,203-247` params named `home`.

blocker 0, major 12, nit 12
