**Zoom Out review: `/Users/decebaldobrica/Projects/chief-of-staff-v2-wt/multi-business/docs/plans/2026-11-12-multi-business-design.md`**

Mode: REVIEW. Range: whole document. Standard: `/Users/decebaldobrica/.claude/plugins/cache/egm/egm/0.1.1/resources/document_standards/zoom-out-standard.md`.

**Context check: passes.** Problem, Terms, Goals and the brief together make objective and motivation clear. Root problem addressed: one resolver, one source per triple. Mixed directories fixed structurally, not patched.

Code claims checked, hold:
- `Cli` defaults `main.rs:16-45`, `default_home` `main.rs:883`.
- `settings.rs` env-then-saved-then-built, silent drop `:45-47`, `save` `:91-98`.
- `set_workspace` sole switch `lib.rs:59-80`; `App(Mutex<Settings>)` `lib.rs:23-29`.
- `reconcile` `mod.rs:469-494` (read-only part `:470-477`, repair `:484-492`).
- Spawn passes only `--home` (`mod.rs:422-425`).
- `atomic_write` unique temp name `state.rs:548-564`.
- `toml 1.1.8` in `Cargo.lock:3745`.

---

**Finding 1**
- **Severity:** Major
- **Affected scope:** Desktop, last bullet (lines 215-217); Testing "spawn with `COS_BUSINESS` or `COS_WORKSPACE` set" (lines 367-368).
- **Simplicity rationale:**
  - The supervisor's harness inherits the supervisor's env. The only env edits are `PATH` (`supervise.rs:165`) and `COS_SEED_REPO` (`seed.rs:115`).
  - Spawn removes every `COS_*` triple var, so any `cos` the agent runs inside its run (`cos flows` per ritual skill, `cos state …`) resolves layer 3, the **active** business, not the business it was spawned in.
  - Example: an agent spawned from the app on `acme` while `default` is active silently acts on `default`. That is the brief's mixing bug again, moved one process down.
  - Passing `--workspace`/`--projects` to `agents supervise` is also dead weight. `AgentCmd::Supervise` uses only `home` (`main.rs:713`), and the design says supervise does not resolve.
  - Simpler, one mechanism: spawn sets `COS_HOME`/`COS_WORKSPACE`/`COS_PROJECTS` to the spawning triple and removes only `COS_BUSINESS`. The supervisor and every nested `cos` then get an explicit env triple (layer 2), with no extra flags. The design should state what nested `cos` binds to.

**Finding 2**
- **Severity:** Major
- **Affected scope:** First-run migration (lines 147-153); Acceptance 1.
- **Simplicity rationale:**
  - There are two migration sources, and whichever runs first wins.
  - If the user runs any `cos business` command before the next app start, `default` is written from the build clone or env. The app then sees a registry, skips migration and never reads `settings.json`'s `home/workspace/projects` again.
  - So a workspace chosen earlier through `set_workspace`, saved in `settings.json`, is silently lost. Acceptance 1 encodes exactly this order.
  - The design rejects coupling the CLI to Tauri's config directory (fine), but leaves the gap open.
  - Minimal fix, no new machinery: at app start, if the registry exists and legacy fields in `settings.json` differ from every registered business, say so once in the header with the `cos business add` line. Or state explicitly that this loss is accepted.

**Finding 3**
- **Severity:** Major
- **Affected scope:** Resolver, lines 141-143 ("`state init` do not resolve … takes `--workspace` or the resolved workspace"); brief Assumption "`cos --business x state init` creates it"; Violations row "internal commands and `state init` still run".
- **Simplicity rationale:**
  - These statements contradict each other. "Does not resolve" and "the resolved workspace" cannot both hold.
  - Today `state_cmd` gets `cli.home` and `ws` for every `state` subcommand (`main.rs:349`). `Propose` reads briefs from home (`main.rs:743`).
  - The design does not say whether `state read/commit/propose/check` resolve, or whether `--business x state init` (the brief's stated path) works.
  - Pick one rule: `state init` resolves like any command, but skips the profile check and, on a broken registry, accepts only explicit `--workspace`. The other `state` subcommands resolve normally. Then state it in one place.

**Finding 4**
- **Severity:** Nit
- **Affected scope:** Registry, Write (lines 117-120).
- **Simplicity rationale:**
  - Reuse is the right call, but "the lock helper in `src/agents/slots.rs`" is a private `fn flock(file, nonblocking) -> bool` (`slots.rs:44-48`), tied to the slot module.
  - Say it gets lifted to a shared place (`state` or a small `lock` module) next to `atomic_write`, rather than having `business.rs` depend on `agents::slots`. That would be a dependency-direction smell: config depending on the agents subsystem.

**Finding 5**
- **Severity:** Nit
- **Affected scope:** Desktop, `AppState { settings, triple }` (line 187); `settings.json keeps slots and cos` (line 184); Security, "Writes … `settings.json` (`slots`, `cos`)" (line 271).
- **Simplicity rationale:**
  - `set_workspace` is the only writer of settings (`lib.rs:75-77`, the only `.save(` call). Once it is removed, nothing writes `settings.json` and nothing mutates `settings`.
  - Two consequences:
    - The Security section claims a write that no path performs.
    - `settings` does not need to sit behind the mutex; only `triple` changes.
  - Fix: drop `Settings::save`, keep `settings` immutable app state, and put only the triple behind the lock. Correct the Security line.

**Finding 6**
- **Severity:** Nit
- **Affected scope:** Desktop, badge bullets (lines 193-197, 211-212); Departures (lines 55-63).
- **Simplicity rationale:**
  - The brief says the badge "switches to them". The design makes clicking it open the picker. That is reasonable, but it is an undisclosed departure; add it to "Departures from the brief".
  - The `Starting` arm of `classify` is not used by `count` (`{ running, lost }` only). Confirm that `Starting` folds into neither count, or drop it from the returned type.

**Finding 7**
- **Severity:** Nit
- **Affected scope:** Context (lines 67-94); Registry and CLI bullets.
- **Simplicity rationale:**
  - There are altitude drops in the architectural sections: the exact regex `^[a-z0-9][a-z0-9-]{0,39}$`, `std::path::absolute`, `default_value_os_t`, `#[serde(deny_unknown_fields)]`, and the long `main.rs` line list at line 70.
  - These are implementation-plan detail. They are acceptable as evidence, but they crowd the design. Move the line inventories to the plan and keep the design at interfaces (registry schema, resolver contract, app state, spawn env).

---

**No YAGNI blockers.** Scope matches the brief:
- The registry is one file, and the resolver is a pure function.
- No speculative per-business ports or credentials.
- The badge measurement gate is proportionate.
- The Semantic ISA section is heavy, but it follows the repo's skill convention.

**Verdict:** revise. Address Findings 1-3 before planning.

Findings: 0 Blocker, 3 Major, 4 Nit
