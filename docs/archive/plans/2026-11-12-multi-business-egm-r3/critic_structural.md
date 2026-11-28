**Structural Goldfish: REVIEW, Stage 7 Critic. Target `/Users/decebaldobrica/Projects/chief-of-staff-v2-wt/multi-business/docs/plans/2026-11-12-multi-business-design.md`, whole document**

Standard: `/Users/decebaldobrica/.claude/plugins/cache/egm/egm/0.1.1/resources/document_standards/feature-spec-standard.md`. Read in full: persona, standard, design, brief. Spot-checked code: `src/agents/mod.rs`, `apps/desktop/src/settings.rs`, `apps/desktop/src/lib.rs`, `src/main.rs`, `src/state.rs`, `src/agents/slots.rs`, `launchd/*.plist`, `Taskfile.yml`, `apps/desktop/ui/index.html`, `apps/desktop/ui/app.js`.

**Structure map:** H1 title. H2 sections: Problem, Terms, Goals and non-goals, Decision, Departures, Context, Design, Alternatives, Risks, Security and privacy, Semantic ISA, Testing, Confidence, Acceptance. H3 under Design and Semantic ISA only. No skipped levels. All tables are well formed and all code fences are closed. Links resolve: the r1/r2 review files, both `consolidated_review_report.md` files and `../../.claude/skills/semantic-isa/SKILL.md` all exist. Every required area of the standard is present: Problem, Goals/Non-Goals, Architecture (Decision/Design), Alternatives, Risks.

1. **Severity: Major.** **Scope:** Terms, "Run record" (line 29-31).
   The terminal status list is wrong when checked against code. It lists `done`, `failed`, `stopped`, `lost`. `src/agents/mod.rs:110-124` and `:137-147` have `Done`, `Failed`, `Killed` (`killed`), `TimedOut` (`timed_out`) and `Lost`, and there is no `stopped`. `classify`/`count` and the badge are built on this vocabulary, so it has to match the enum.

2. **Severity: Major.** **Scope:** Design, "Long-lived processes and launchd" (line 174-178).
   - The claim "Logs go to the resolved home's `out/logs`, which the job creates" does not match how the plist works. `StandardOutPath` and `StandardErrorPath` are static plist values filled from `@COS_HOME@` (`launchd/cos.brief.plist:24-25`, `cos.dash.plist:20-21`). launchd opens them before exec, so the job cannot resolve the home or create that directory first.
   - With no `BUSINESS=` given, the jobs "follow the active business", but the log path is fixed when `task schedule` runs.
   - The spec should say what goes into the log-path placeholders after "the plist template drops `COS_HOME`".

3. **Severity: Major.** **Scope:** Risks (line 252-266).
   The standard requires each risk to state its mitigation. Several entries state a limit but give no mitigation and do not mark it as an accepted risk:
   - **Badge cost:** "measured before called done", with no fallback if the 200 ms target in Confidence fails.
   - **Env pin.**
   - **MCP and launchd after `use`.**

   Either add a mitigation or label each one "accepted, by design" and cite the brief.

4. **Severity: Nit.** **Scope:** Section order (line 46-95).
   Decision and Departures come before Context, so a fresh reader meets the decision before the current-state map it depends on, such as `set_workspace` and `save()` writing env values. Putting Context right after Terms or Goals would read in order.

5. **Severity: Nit.** **Scope:** Goals and non-goals (line 37-44).
   Goals and non-goals are run-on prose. The rest of the doc uses bullet lists for enumerations, so bullets here would match.

6. **Severity: Nit.** **Scope:** Terms, "Active business" (line 27).
   It is defined as "the registry's default business". "Default" already names the business `default` and resolver layer 4 "defaults". Use "the registry's `active` entry" instead.

7. **Severity: Nit.** **Scope:** Acceptance 5 vs Desktop badge (line 211, 404).
   The format is `N running, M lost in other businesses`, but the acceptance check expects `1 running in other businesses`. The spec does not say whether a zero `M lost` part is left out.

8. **Severity: Nit.** **Scope:** Acceptance 4 (line 400-403) vs brief "The job when this is done".
   - The brief lists "report, plan, flows, docs, agents, staff". The design and acceptance never say where "plan" lives, and there is no plan tab (`ui/index.html:18-21`).
   - Acceptance 4 lists Agents (Runs, Staff) and leaves out the Hire pane that Context names (line 80).

9. **Severity: Nit.** **Scope:** Alternatives (line 245-246, 250).
   - "JSON registry ... the user chose `businesses.toml`" gives the user's choice as the reason without linking where it was decided. The brief's Decisions section is that source and should be cited here, as line 250 cites it.
   - Line 250 rejects the alternative only as "out of scope", with no technical trade-off. That is acceptable under the standard's "scope trade-offs" clause, but it is thin.

10. **Severity: Nit.** **Scope:** Context, line 70.
    The long run of bare line numbers (`main.rs:330-332, 348-349, …, 843`) has no labels, so a fresh reader cannot tell which command each site belongs to.

Findings: 0 Blocker, 3 Major, 7 Nit
