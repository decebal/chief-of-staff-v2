# Style Goldfish review: multi-business design (Mode: REVIEW)

**Target:** `/Users/decebaldobrica/Projects/chief-of-staff-v2-wt/multi-business/docs/plans/2026-11-12-multi-business-design.md`, whole document.

**Context read:** the brief (`2026-11-12-multi-business-brief.md`) and the persona `/Users/decebaldobrica/.claude/plugins/cache/egm/egm/0.1.1/agents/style.md`. The persona names no style standard, so I audited against the persona's own checks only.

**Code checked, matches the design:**
- `src/main.rs:16-45`
- `apps/desktop/src/settings.rs:26-28, 44-70, 91-98`
- `CLAUDE.md:31`, `docs/usage.md:458`, `apps/desktop/README.md:8`
- the r1 review links
- the semantic-isa skill link

**Overall:** The writing is dry, direct and free of fluff. There is no hyperbole, no buzzwords and no praise of code. The findings are about claims that say more than the evidence supports, plus a few undefined terms.

---

**1. Severity: Major**
- **Where:** lines 28 and 35-37 (the design says the brief's decisions are its source), against lines 138-140, 177-179, 125-126, 277 and Acceptance 1 (line 332-333).
- **Problem:** The design says it follows the user's decisions in the brief, but it changes three of them without saying so:
  - **Agents badge.** Brief:30-31 asks for a badge "that switches to them". Design:178-179 says "Not clickable."
  - **Migration.** Brief:34-35 says that on first run the current triple becomes `default`. The design writes `default` only on the first `cos business add` (138-140). Acceptance 1 says no file is written.
  - **Profile setup command.** Brief:50 names `cos --business x state init`. The design names `cos --workspace <dir> state init`.
- **Action: CALIBRATE.** List each change in one place as a change to a user decision, with its reason. Otherwise the sentence pointing to the brief overstates how closely the design follows it.

**2. Severity: Major**
- **Where:** lines 173-176 ("every in-memory cache starts from the new triple") and line 326 ("The reload removes the cache list").
- **Problem:** `location.reload()` only clears state in the window's JavaScript. Rust-side caches live in the app process and survive a reload. One exists today: the static `LAST` in `flows::page_cached` (`src/flows.rs:673-694`). It is safe only because it is keyed by `ws` and `projects` (lines 681-682), not because of the reload. The sentence credits the wrong mechanism and covers more than it checked. Line 326 also means nothing to a new reader: "the cache list" refers to the rejected alternative at line 204, which line 326 does not name.
- **Action: CALIBRATE.** Write "every view and JS-side cache restarts; Rust-side caches are keyed by path (`flows.rs:673`)". Rewrite line 326 so it stands on its own.

**3. Severity: Nit**
- **Where:** line 32, "**coherent** triple".
- **Problem:** "Coherent" is a vague word, and bolding it adds emphasis but no meaning. The exact rule is stated at lines 43-44: one source per triple, sources never mixed.
- **Fix:** Use "single-source triple" or point to that rule.

**4. Severity: Nit**
- **Where:** lines 72-73, "renamed first so a business key cannot be threaded into the wrong one".
- **Problem:** "Business key" is never defined; the Terms section uses business, triple, home and workspace. "Cannot" is too strong: renaming a parameter lowers the chance of a mix-up but does not prevent one, since both are `PathBuf`.
- **Action: CALIBRATE.** For example: "renamed first so the workspace path is not passed where the home is expected."

**5. Severity: Nit**
- **Where:** line 112, "keeps the existing plists (`COS_HOME` only) and scripts working unchanged".
- **Problem:** The plists and `task install-mcp` are cited (lines 68-69). "Scripts" is not; no list of scripts or checks backs it.
- **Action: CALIBRATE.** Limit the claim to the cited plists and `Taskfile.yml:57-74`, or drop "scripts".

**6. Severity: Nit**
- **Where:** lines 98-99 ("the version already in `Cargo.lock`") and line 203 ("costs no new crate").
- **Problem:**
  - `Cargo.lock` contains two `toml` versions, 0.8.2 (line 3733) and 1.1.8 (line 3745), so "the version" is ambiguous.
  - Neither version is a direct dependency in `Cargo.toml` (no match there); 0.8.2 is only pulled in by another dependency.
  - So "no new crate" holds only for downloads. The design still adds a direct dependency to `Cargo.toml`.
- **Action: CALIBRATE.** For example: "`toml` 0.8.2, already in the lockfile as an indirect dependency; becomes a direct dependency."

**7. Severity: Nit**
- **Where:** line 325, "Resolver: **rock** once its precedence tests pass".
- **Problem:** Line 322 defines rock as "understood well enough to commit to". A rating that depends on tests that have not run is not rock yet. Line 327-328 handles the same situation correctly: the badge is sand, with a stated condition for becoming rock.
- **Action: CALIBRATE.** Rate the resolver rock without the condition, or sand with "moves to rock when the precedence tests pass".

**8. Severity: Nit**
- **Where:** line 211, "held off by the lock and the re-read".
- **Problem:** "Held off" is vague. It does not say whether the risk is prevented or only made less likely.
- **Fix:** "prevented by the exclusive lock and the re-read inside it" (the mechanism is at lines 95-97).

**9. Severity: Nit**
- **Where:** line 201, "**Registry in the app's `settings.json`:** the CLI would read a Tauri config path."
- **Problem:** The line states what would happen but not why that is a reason to reject it. Every other rejected alternative states its cost.
- **Fix:** Add the cost, for example "couples the CLI to the app's per-platform config directory."

Findings: 0 Blocker, 2 Major, 7 Nit
