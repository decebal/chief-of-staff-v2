# EGM Stage 6: comprehension test

Target: `docs/plans/2026-11-13-business-dashboards-design.md` (sha256 `a853bd18…`), read alone, no document standard. FeatureRange 1-411, DetailedRange 412-613. Result: 0 Blocker, 9 Major, 13 Nit.

## Paraphrase: FeatureRange (lines 1-411)

**What it proposes.** The `cos` desktop app (Tauri) gets a left sidebar with one row per registered business. Each row shows four things:

- the count of live agent runs;
- a dot when there is an unread brief;
- a warning for lost runs;
- a clock on the business named in the installed `cos.brief` launchd plist.

The signals come from file metadata on a 700 ms watcher tick, with no process spawn. Clicking a row runs the same `switch_business` as ⌘K.

Each business can declare dashboards in a hand-edited `dashboards.toml` next to the registry. A dashboard is a loopback URL plus an identity probe.

A server with a declared binary is "owned". The app spawns it on first open, one process per address, and writes a pidfile. On quit the app stops its process group: SIGTERM, then SIGKILL after one shared 5 s deadline. After a crash, the next launch reclaims the server from the pidfile. It checks the pid's start time, the executable path, and that the recorded `app_pid` is dead.

A listener with no binary, or one already on the port, is embedded only if its probe marker matches, and is never killed.

The remaining changes:

- the CSP gains `frame-src http://127.0.0.1:*`;
- the capability file keeps IPC away from frames;
- `cosreport` refuses foreign Origins;
- the four businesses are registered by a runbook;
- `task schedule` skips `cos.dash` when port 8790 is held.

A Semantic ISA section lists the side effects, taint sources and sinks, and the check that runs before each sink.

**Existing system, per the document:**

- ⌘K shows a live count only while it is open.
- The header badge refreshes only on the Agents tab.
- The control room runs by hand via `cargo run`.
- The gazette runs under launchd, in a browser tab.
- The CSP allows frames only from `cosreport:`.
- The registry holds `acme` and `default`.

## Paraphrase: DetailedRange (lines 412-613)

- **New cos modules:**
  - `dashboards.rs`: serde types with `deny_unknown_fields`, `location`, load and validate, a raw-TcpStream HTTP/1.0 probe, and `orphans`.
  - `signals.rs`: live, lost and latest brief from the run walk, plus a plist reader and `stamp`.
- **Other cos changes:**
  - `agents::runs` is exposed for reuse.
  - `business::add` reports nested homes, and the CLI warns.
  - A new `cos business dashboards` subcommand.
- **Desktop `servers.rs`:** a Supervisor with Slot variants Owned, Reclaimed, External and Exited; a Refusal enum; `stop_owned`; and kernel checks via `proc_pidinfo` and `proc_pidpath`.
- **Desktop `lib.rs`:**
  - four new IPC commands: `sidebar`, `dashboards`, `dashboard_open`, `brief_seen`;
  - the watcher extension;
  - an origin check on `cosreport`;
  - `on_run_event`.
- **Rest of the desktop:**
  - `main.rs` switches to `.build().run()`;
  - the CSP edit;
  - UI markup, `sidebar.js`, `dash.js`, and `app.js` and CSS changes;
  - a Taskfile guard for 8790.
- **Tests:**
  - cos: dashboards, signals, business, schedule;
  - desktop: a servers harness in which the test binary acts as the fake server, plus IPC tests;
  - a README section.
- **Verification:**
  - `task build`, `task test` and clippy;
  - the ISA matrix mapped to tests;
  - E2E running CUJ 1-7 with `lsof` and `launchctl` captures.

## Findings

1. **Major: contradiction in the `ensure` step count.** Lines 182-194 list 4 steps; line 464 says "follows the five steps in the Technical Plan". Fix: make the counts agree, or name the fifth step.
2. **Major: another app instance's server is both "external" and a refusal.**
   - Line 187 says treat it as external, meaning embedded and never stopped.
   - Line 465 has a `Refusal::ForeignOwner` variant, meaning not embedded.
   - Goal 8 (line 45) has neither state.

   Fix: pick one behaviour, then make line 187, line 465 and the Goal 8 state list match.
3. **Major: the Owned vs Reclaimed slot model differs between sections.**
   - Lines 114 and 186 say a reclaimed server becomes Owned.
   - Step 1 (line 184) checks "Owned and its child alive", but a reclaimed server has no child handle.
   - Line 198 signals "each Owned slot".
   - Line 463 adds a separate `Reclaimed` variant.

   A reader cannot tell whether Reclaimed slots get stopped by `stop_owned`, or how step 1 handles them. Fix: in section 2, say that step 1 and `stop_owned` cover both Owned and Reclaimed, and that Reclaimed uses a pid liveness check, not a child handle.
4. **Major: undefined behaviour for an external dashboard whose port is free.**
   - The gazette has no `server` entry (lines 158-161).
   - Step 4 (lines 190-194) assumes there is a binary to spawn.
   - `ensure` takes `server: Option<&Server>` (line 464).

   So what does the pane show when the gazette is down? No Goal 8 state covers it, and the same gap applies to an `Exited` slot on the next open. Fix: add a step 4 branch ("no server declared, so refuse with `not running` plus a hint"), add that state to Goal 8, and say whether `Exited` respawns.
5. **Major: "owned" and "supervisor" each carry several meanings.**
   - "Owned" is used for the config class (line 113: has a binary), for the runtime state (line 114), for "launchd owns" (lines 13 and 52, CUJ 6), and for the other-app case (line 187).
   - "Supervisor" means `servers::Supervisor` (lines 131, 180) and also the agent-run supervisor pid (lines 223, 432: `proc::alive(supervisor)`).

   Fix: in Terms, rename the config class (for example "managed" vs "external"), keep "Owned" for runtime only, and give the agent-run supervisor pid its own name.
6. **Major: core project terms are undefined, and the document defers to a base design the reader doesn't have** (line 111, "as in the base design"). Undefined terms:
   - `cos` itself (line 13);
   - the registry and `businesses.toml` (lines 30, 61);
   - brief (lines 13, 225);
   - live and lost runs, `classify` and `is_cos` (lines 223-224);
   - profile (lines 34, 253);
   - "pinned" (lines 35, 73, 219), which differs from "pinned Codex threads" (line 21);
   - "unsaved-Hire guard" (line 35);
   - "full check" (line 223).

   Fix: extend Terms with one line each, or link the base design section for each.
7. **Major: the app's business vs the registry's active business is never defined.** It appears only indirectly: line 55 says "only the app's business counts", line 75 says "registry's active business unchanged". The distinction drives CUJ 2 and the superseded non-goal (line 17). Fix: add a Terms entry for the app's business (per window, set by ⌘K or the sidebar) vs the active business (CLI, stored in the registry).
8. **Major: section 6 and `cos.dash` are unexplained.** `cos dash`, `cos.dash` and `cos::dash::stamp` appear at lines 53, 237, 258-260, 326 and 531, but nothing says what `cos dash` is. Section 6 does not explain how it relates to this feature, and no Goal covers it. Fix: one sentence on what `cos.dash` serves and why it collides with the gazette on 8790, plus a Goal or a note in Problem.
9. **Major: the Semantic ISA framework is undefined.** Lines 312-390 use these terms without definition or link:
   - "Core" (Memory, Execution, Normative);
   - "Governance property";
   - "Sink?";
   - "Binding (closed schemas)";
   - the instruction verbs LOAD, PROBE, VERIFY, TOOL_CALL, STORE and RENDER.

   Fix: add a two-line intro to the section, or link to the framework doc.
10. **Major: possible wrong binary path.**
    - Line 21 says the repo is `.../portfolio-control-room/control-room`.
    - Line 255 says to build in `.../control-room`.
    - Line 142 places the binary at `.../portfolio-control-room/target/release/portfolio-control-room`.

    That path holds only if `portfolio-control-room` is a cargo workspace root. Fix: state the workspace layout, or correct the path.

    *Author check: `portfolio-control-room/Cargo.toml:4` declares `[workspace]`, so the path at line 142 is right. The design should say so.*
11. **Nit: the RENDER sink check is inconsistent.** Lines 343 and 366 say RENDER is cleared by `VERIFY(identity)`. But an owned server the app just spawned is rendered after the `ready` probe, and the identity probe never runs for it (lines 194 and 366: "Owned and ready"). Fix: make line 366's "Cleared by" say `VERIFY(identity)`, or the ready probe for a spawned Owned server.
12. **Nit: CSP text conflicts.** Line 29 says frames come only from `cosreport:`. Line 495's final value also includes `http://cosreport.localhost`. Fix: correct whichever line is wrong.
13. **Nit: `location()` description drifts.** Line 138 says the registry directory joined with `dashboards.toml`; line 421 says only "parent of `business::location()`". Fix: add "joined with `dashboards.toml`" at line 421.
14. **Nit: decision IDs refer to a brief the reader doesn't have.** Lines 15, 268 and 274 cite D5, D6, U4 and U6. Fix: drop the IDs or add a one-line gloss.
15. **Nit: no reason given for refusing binaries under `out/`** (lines 191, 363). The reason, that agents write there, is only implied by lines 279 and 350. Fix: state it inline at line 191.
16. **Nit: the `x-cos-business` header threat is vague** (line 214). Fix: one sentence. `cosreport` serves reports and accepts writes such as `/docs`, scoped by that header, so a dashboard frame could otherwise forge it.
17. **Nit: app UI surfaces are assumed.** The Docs tab (line 209), the Today report and view (lines 75, 227), the Agents tab (line 27), `switchTo` and `cos-report-ready` (line 515) are used without description. Fix: a short UI glossary.
18. **Nit: the "stopped" state** in Goal 8 (line 45) is produced by no flow while the app runs. Fix: define it, for example as `Exited(status)`, or remove it.
19. **Nit: failed readiness after a reclaim is not covered** (line 186). Fix: state the outcome.
20. **Nit: kronos vs chronos spelling** (lines 30, 251, 253). Fix: say the mismatch is deliberate.
21. **Nit: the registration table's columns are undefined** (lines 246-251). Fix: one sentence on what home, workspace and projects mean.
22. **Nit: unexplained references:**
    - "runbook" (line 164);
    - "the plan lists the names" (line 606);
    - `pr-proof` (line 613);
    - the rock and sand tiers (lines 401-410);
    - `COS_HOME` (line 231).

    Fix: point each at its definition.
