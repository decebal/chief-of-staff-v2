**What this proposes**

A TOML registry at `~/.config/cos/businesses.toml` names "businesses". Each business is a triple of home, workspace and projects directories, and the registry marks one business as active. A single resolver, `cos::business::resolve`, takes the whole triple from exactly one source. In priority order: a named business (`--business`/`COS_BUSINESS`), then explicit directory flags or env vars, then the registry's active business, then built-in defaults. It never mixes sources. New `cos business list|show|add|use|remove` commands manage the registry, and only these commands write it. The desktop app keeps its own "app business" in `settings.json` and switches it through a ⌘K picker, which reloads the window. `COS_*` env vars set at app start pin the app to one triple. Long-lived processes (dash, mcp, serve, launchd jobs) still bind one triple at start, now through the resolver. A read-only `count_live` badge shows agents running in other businesses.

**How the existing system works, per this document**

Today cos holds one triple. The CLI has independent `--home`/`--workspace`/`--projects` flags with `COS_*` env fallbacks and per-flag defaults; workspace falls back to home. The desktop app has one `Settings` struct (env, then `settings.json`, then defaults). `save()` writes the effective values back, env included. `set_workspace` is the only runtime switch, and it changes only the workspace. Agent runs live under `<home>/out/agents/`, with run slots per home. Listing agents runs `reconcile`, which can kill process groups and rewrite state. The launchd plists and `install-mcp` pass `COS_HOME` or nothing. Several functions take a parameter named `home` that actually receives the workspace.

---

1. **Severity:** Major
   **Affected Scope:** CLI `cos business add` (l.134-140); Acceptance 2 (l.334-335); Resolver step 2 (l.110)
   **Discrepancy:** `add` takes `--home/--workspace/--projects`, and the same names are the resolver's global directory flags. The doc never says whether these are subcommand arguments or the global flags. If they are global, then on an empty registry the "invocation's resolved triple (step 2)" is acme's own directories. `default` would then get acme's home, and the next check ("home already used by another business") would refuse `acme`. That contradicts Acceptance 2. I could not paraphrase how `default`'s triple is chosen in Acceptance 2.

2. **Severity:** Major
   **Affected Scope:** Violations "workspace without a profile at switch or use" (l.277) vs `cos business use` (l.141) and `add` (l.139-140)
   **Discrepancy:** The CLI section says `use` only sets `active` and never mentions a profile check. `add` reports a missing profile but does not refuse. The Violations table says `use` is refused with "nothing changes". A reader cannot tell whether `use` checks the profile.

3. **Severity:** Major
   **Affected Scope:** Term "state.json" (Terms l.11-12; Context l.67; Desktop l.165; Security l.222-223; ISA matrix l.294)
   **Discrepancy:** Terms defines `state.json` as the workspace's tracked deliverables (`.chief-of-staff/state.json`). Elsewhere it means a per-agent-run `out/agents/<id>/state.json`. Context l.67 ("`reconcile` may ... rewrite `state.json`") does not say which one. The agent-run file is never defined.

4. **Severity:** Major
   **Affected Scope:** Resolver defaults (l.116-118); Context (l.50-51, l.56-57)
   **Discrepancy:** The doc says home defaults to "the cos source checkout the binary was built from" and that the CLI and app "produce the same strings". The two are built separately: the CLI uses `default_home()`, the app uses `CARGO_MANIFEST_DIR/../..`. The doc does not say how a shared resolver function gets one compile-time path for both, or what "canonicalised" does when the checkout has moved. "The app passes no default of its own" does not explain the mechanism.

5. **Severity:** Major
   **Affected Scope:** CLI section l.143-146 vs Context l.68-69
   **Discrepancy:** The doc says long-lived processes "started with neither bind the active business (step 3)" and that the plists are unchanged. But the plists pass `COS_HOME`, which triggers step 2, so the registry is never read. "Pass `COS_HOME` only, or nothing" does not say which plist does which. A reader cannot tell which jobs follow `cos business use` and which stay pinned. It is also unclear whether `task schedule BUSINESS=` removes `COS_HOME`; if it does not, step 1 errors with "--business and COS_HOME both set".

6. **Severity:** Nit
   **Affected Scope:** Decision l.44 ("only `cos business use` changes the active business") vs `add` l.138-139
   **Discrepancy:** On an empty registry, `add` also sets `active` (to `default`). The statement is too absolute.

7. **Severity:** Nit
   **Affected Scope:** Desktop legacy path (l.150-153) vs Term "App business" (l.20-21)
   **Discrepancy:** The term says the app business "starts as the active business". The legacy path starts the app on explicit directories that may match no business, and the doc does not say what the header and picker then show. It also does not say what happens when `business` is unset and `settings.json` has no legacy directories (presumably step 3 or 4).

8. **Severity:** Nit
   **Affected Scope:** Terms (l.14), Context (l.64-67), Non-goals (l.37), header (l.3-5)
   **Discrepancy:** Several project-specific terms are not defined: "the brain", "`cos flows`", "the docs room", "agent hiring", "supervisor", "run slots", "build lock", "tactician", "Goldfish", "EGM". "Business key" (l.73) is also never defined.

9. **Severity:** Nit
   **Affected Scope:** `agents::count_live` (l.165-167)
   **Discrepancy:** The doc counts statuses "that are not terminal" but never lists the terminal statuses, so a reader cannot tell what is counted.

10. **Severity:** Nit
    **Affected Scope:** Semantic ISA definitions (l.230-231) vs Instructions table (l.251)
    **Discrepancy:** The vocabulary says the Memory core "reads and writes state". `STORE(registry)` writes a state file but is classed as Execution. Either the definition or the classification needs a one-line justification.

11. **Severity:** Nit
    **Affected Scope:** Taint and sinks (l.261-263)
    **Discrepancy:** The doc says "`cos` field ... stays as today" but never explains what it does today or how `cos_binary()` uses it at startup. A reader must already know this to judge the "resolved once" claim.

Spot-check of code citations: `main.rs:18,25` (`default_value_os_t`), `main.rs:42` (`ws`), `main.rs:883` (`default_home`), `lib.rs:61` (`set_workspace`) and `agents/mod.rs:446/469` (`read`/`reconcile`) match what the doc says.

Findings: 0 Blocker, 5 Major, 6 Nit
