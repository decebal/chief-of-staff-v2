Could not write `/Users/decebaldobrica/Projects/chief-of-staff-v2-wt/multi-business/docs/plans/2026-11-12-multi-business-egm/critic_structural.md`: no write tool, persona read-only. Full report below; Elephant must persist it. Did not modify any skill: outside reviewer step.

Mode REVIEW. Target `/Users/decebaldobrica/Projects/chief-of-staff-v2-wt/multi-business/docs/plans/2026-11-12-multi-business-design.md`, 1..End. Standard feature-spec-standard.md. Baseline brief read.

Structure map: H1 > Decision, Context, Design (Registry, One resolver, CLI, Desktop, Fixes taken first), Alternatives rejected, Security and privacy, Semantic ISA (5 H3), Testing, Confidence, Acceptance. No skipped levels. Tables well-formed. Code block closed. Link at line 3 resolves.

**Blocker**
1. design:72-78 vs `launchd/cos.brief.plist:14`, `launchd/cos.dash.plist:15`. Precedence resolves each dir separately. launchd sets only `COS_HOME`, so home comes from env (rank 2) and workspace/projects from registry `active` (rank 4). After `cos business use acme`, the 08:30 brief and dash run default home + acme workspace. That is the mixed-triple bug brief:9-11 exists to kill. Decision:12-13 says launchd takes `--business`, but Design has no launchd/Taskfile section (`Taskfile.yml:194`). Same hole for app under env pin (design:95-96): switch_business under `COS_HOME` gives a mixed triple, behaviour unstated. Spec must say: env/flag pins the whole triple, or partial pin + business is an error.

**Major**
2. Missing required section: Problem Statement. Doc opens on solution (design:5-14); problem lives only in brief. Standard core principle wants self-contained spec.
3. Missing required section: Goals & Non-Goals. Out-of-scope only in brief:57-58. Design hints at it at :123 only.
4. Missing required section: Risks & Mitigations. Security (:127-133) and Confidence (:203-209) are partial. Unaddressed failure modes:
   - CLI `use` and app `switch_business` both write `businesses.toml`, no lock: lost update.
   - Running app not told when CLI changes `active`; brief:19 promises "active for both CLI and app".
   - Per-home agents scan cost has no bound or mitigation.
5. Alternatives (:118-125) too thin vs Alternatives Rule.
   - No alternative given for registry format: TOML adds a dep (:64); serde_json is already in tree.
   - No alternative for per-dir vs whole-triple precedence, picker vs dropdown, or migration strategy.
   - :123 "out of scope by the user's decision": rejection pinned on a person, no link. Should link brief:25-28.
6. Contradiction on `.bak`: write procedure :58-60 is tmp+fsync+rename, no backup. ISA :141, :175, :182 promise `businesses.toml.bak` on each write. Testing :191-193 never checks `.bak`.
7. Contradiction on migration source: :61-62 migrates from "flags/env/app settings/defaults". If CLI runs first, it must read Tauri `settings.json`, the exact coupling :120-121 rejects. If it doesn't, the app's saved workspace is lost. Pick one and say so.
8. Validation rules disagree across :85-88, :166, :167, :61:
   - CLI `add` "prints the state init to run" when profile.json missing (refuse or accept?). :166 STORE requires profile.json.
   - Migrated `default` is not validated. :167 re-verifies on switch, so switching back to `default` without profile.json is refused, breaking Acceptance 3 (:217-218).
   - "not a symlink to elsewhere once canonicalized" (:166) undefined. macOS `/tmp`, symlinked `~/Projects` would be refused.
9. Resolver output (:69-70) "name or None when flags alone define it" says nothing on partial override (`--business acme --home /x`). Which name? Is the triple valid? Tie to finding 1.
10. Testing :200-201 conditional "if the repo has a JS test runner". Repo has none: no package.json under apps/desktop. Resolve it now and name the hand-check steps.

**Nit**
11. :60 cites `settings.rs:45-47` as "silently replaced". Those lines silently ignore a parse failure; the overwrite is `settings.rs:96` save.
12. :64 "0.8.2 or 1.1.8, whichever..." undecided. APIs differ. Lock has `1.1.8+spec-1.1.0` (Cargo.lock:3746).
13. :29 decision (picker hand-written) placed inside Context.
14. :39 H3 too long, carries two code paths. Split or shorten.
15. :98 "reconcile left lazy" and :205-209 rock/sand: jargon undefined for a fresh reader.
16. :148-157 ISA Core/Governance terms and `additionalProperties: false` (JSON-schema wording on CLI args) unexplained.
17. :220 Acceptance 5 "still works" not verifiable. State expected output.
18. :32 `main.rs:775` correct, but the `state::profile(home)` call is at :778. Cite :775-778.

Counts: blocker 1, major 9, nit 8.
