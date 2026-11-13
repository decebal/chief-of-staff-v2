Style Goldfish review of `/Users/decebaldobrica/Projects/chief-of-staff-v2-wt/multi-business/docs/plans/2026-11-12-multi-business-design.md`. Mode: REVIEW, whole document. Context read: the brief (`/Users/decebaldobrica/Projects/chief-of-staff-v2-wt/multi-business/docs/plans/2026-11-12-multi-business-brief.md`). The persona names no standard file, and the task supplied none, so I checked against the persona's audit vectors only.

Code checks, all consistent with the design:
- `built_home` is at `apps/desktop/src/settings.rs:26-28`.
- The load path drops a bad file at `settings.rs:45-47`, and `save` is at `settings.rs:91-98`.
- `App(Mutex<Settings>)` is at `lib.rs:23`, and `set_workspace` at `lib.rs:61`.
- `reconcile` is at `src/agents/mod.rs:469-494`, `START_GRACE` at `mod.rs:43`, and the kill and write at `mod.rs:485-492`.
- `toml 1.1.8` is in `Cargo.lock`.
- `atomic_write(path, &Value)` is at `state.rs:548`.
- `codex mcp add` at `Taskfile.yml:74` passes no triple.
- The semantic-isa link resolves.

## Findings

1. **Severity: Major.** Scope: Decision (line 52); Design, Desktop (lines 201-202).
   "⌘K, from an app menu accelerator so it works inside the report and docs frames" and "opens it from anywhere, including inside the frames" state as fact something Confidence (line 387) marks as **sand** until the hand-check passes. The claim goes further than the evidence. Defensive Caveats action: **CALIBRATE**, e.g. "expected to work inside the frames; confirmed by the UI hand-check".

2. **Severity: Major.** Scope: Decision (lines 50-51).
   "so a triple never mixes sources" is too absolute. In the flags and env layers, each missing directory comes "from the defaults" (lines 131-132). So a triple can mix flags with defaults, or env with defaults. What is actually true is narrower: no triple mixes flags, env and registry. Action: **CALIBRATE**, e.g. "never mixes layers 1-3; missing directories come from defaults".

3. **Severity: Major.** Scope: Terms (line 27); Resolver (lines 136, 144); CLI `list` (line 158); First-run migration (line 150); Acceptance 1.
   "default" has three meanings:
   - the business named `default`;
   - "the registry's default business" (the definition of Active business);
   - layer 4 "defaults".

   The output label `current: <name|explicit|default>` is ambiguous when a business is named `default`: `current: default` could mean the registry business or the layer-4 fallback. The `business: <name>` start line (line 144) has the same problem. Fix: define Active business without the word "default", and give layer 4 a separate label such as `built-in`.

4. **Severity: Major.** Scope: Desktop badge (line 211) against Acceptance 5 (line 404).
   The spec string is `N running, M lost in other businesses` "when non-zero". Acceptance expects the literal `1 running in other businesses`. The spec does not say whether a zero term is left out, or whether "non-zero" applies to each count or to the sum. Fix: state the format rule for each count.

5. **Severity: Nit.** Scope: Desktop (line 139).
   "so the CLI and the app agree by construction" holds only when both are built from the same checkout, because layer 4 is "the checkout the binary was built from". Action: **CALIBRATE**, adding the condition "when built from the same checkout".

6. **Severity: Nit.** Scope: Risks, Badge cost (lines 258-259).
   "measured before the badge is called done" gives no threshold. The threshold (200 ms on picker open) appears only in Confidence (line 388). Action: **RELOCATE** the threshold here, or cross-reference it.

7. **Severity: Nit.** Scope: Desktop (line 209).
   "so they need nothing" is vague. Suggested wording: "need no reset".

8. **Severity: Nit.** Scope: Terms (line 18).
   The nickname "the brain" adds no meaning. Drop it.

9. **Severity: Nit.** Scope: Terms (lines 30-31).
   "(terminal) (`src/agents/mod.rs:127-131`)" stacks two parentheticals. Merge them, or move the citation into the sentence.

10. **Severity: Nit.** Scope: Desktop (line 189); Risks (line 263); Violations table.
    "pinned" / "pin" is a term used throughout the document but missing from Terms. Add it there.

11. **Severity: Nit.** Scope: Taint and sinks table (line 326).
    "the switch's VERIFY" is imprecise. Name the instructions, `VERIFY(business_name)` and `VERIFY(business_dirs)`, or the startup resolution.

12. **Severity: Nit.** Scope: Semantic ISA (line 279).
    "changes the world: files, processes" is informal. Suggested wording: "has side effects on files or processes".

13. **Severity: Nit.** Scope: CLI `add` (lines 160-165).
    The sentence about directory rules carries a nested parenthetical about the symlinked `.chief-of-staff`. Split it into a separate bullet.

Blocker: 0, Major: 4, Nit: 9
