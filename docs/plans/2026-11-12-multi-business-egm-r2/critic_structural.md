**Structural Goldfish: Stage 7 review of `/Users/decebaldobrica/Projects/chief-of-staff-v2-wt/multi-business/docs/plans/2026-11-12-multi-business-design.md`, whole document**

Inputs read: persona `structural.md`, `feature-spec-standard.md`, the design, the brief. Code checked: `src/main.rs`, `apps/desktop/src/settings.rs`, `apps/desktop/src/lib.rs`, `launchd/cos.brief.plist`, `Taskfile.yml`, and grep hits in `agents/mod.rs` and `state.rs`.

**Structure map.** One H1, then these H2 sections in order: Terms, Problem and goals, Decision, Context, Design, Alternatives rejected, Risks, Security and privacy, Semantic ISA, Testing, Confidence, Acceptance. All H3s sit under Design or Semantic ISA. No heading level is skipped. All five sections the standard requires are present in some form.

**Links.** Every internal link resolves:
- brief
- review-r1
- egm-r1/consolidated_review_report.md
- `.claude/skills/semantic-isa/SKILL.md`

**Code references.** Spot-checks are accurate:
- `main.rs:18-26, 42-44, 883`
- `settings.rs:26-28, 44-70, 91-98`
- `lib.rs:26-28, 61-80`
- `agents/mod.rs:446, 469`
- `cos.brief.plist:14`

---

**1. Severity: Major**
- **Scope:** Design > CLI, lines 143-146, against Resolver rule 1, lines 108-109.
- **Problem:** The two sections contradict each other. Line 145 says the plists are unchanged, so they still set `COS_HOME` (`launchd/cos.brief.plist:14`; `Taskfile.yml:194` fills it in with sed). Line 146 says `task schedule BUSINESS=<name>` adds `--business <name>` to the plist arguments. Rule 1 says `--business` plus any `COS_*` directory env is an error. So every scheduled brief and dash started with `BUSINESS=` exits non-zero. The design must either drop `COS_HOME` from the plist when `BUSINESS` is given, or exempt this case from rule 1. Context (line 68-69) also leaves out `Taskfile.yml:187-197`, the code this change touches.

**2. Severity: Major**
- **Scope:** Design > Desktop, lines 177-179, against brief:30-31.
- **Problem:** The brief records the user's decision as "a badge counting agents running in other businesses that switches to them". The design makes the badge "Not clickable". This overrides a user decision, but it is not listed as a deviation and Alternatives rejected gives no reason for it.

**3. Severity: Major**
- **Scope:** CLI `add` (lines 138-140) and Acceptance 1 (lines 332-333), against brief:34-35.
- **Problem:** The brief's migration decision is that "on first run the current triple becomes a business named `default`". The design writes `default` only on the first `add`, and Acceptance 1 states that no file is written. This changes a recorded user decision without saying so or giving a reason.

**4. Severity: Major**
- **Scope:** Design > Desktop (lines 150-164), Risks (lines 209-218), Violations table (lines 273-281).
- **Problem:** No failure mode covers the app starting when its saved `business` is gone or broken. This happens if the business was removed by `cos business remove`, or if its directories or profile were deleted. Line 141 has `remove` refuse only the *active* business, not the app business, so this state is easy to reach. Neither the startup path nor the Violations table says what happens. The standard requires failure modes and how they are mitigated.

**5. Severity: Nit**
- **Scope:** H2 headings at lines 39, 48, 195, 209.
- **Problem:** "Alternatives rejected" and "Risks" do not match the standard's labels "Alternatives Considered" and "Risks & Mitigations". Most Risks bullets name a mitigation, but there is no mitigation column or label for it. "Decision" also comes before "Context", which reverses the usual order of problem, then context, then proposal.

**6. Severity: Nit**
- **Scope:** Problem and goals, lines 28-29.
- **Problem:** The problem statement is one sentence plus a link to the brief. A reader who has only this document has to leave it to understand the motivation. The core principle in the standard asks for a document that stands alone.

**7. Severity: Nit**
- **Scope:** Resolver, lines 124-126, against brief:50.
- **Problem:** The profile check runs before any command reads profile or state, but it does not exempt `state init`. The brief expects `cos --business x state init` to create the profile. The exemption should be stated.

**8. Severity: Nit**
- **Scope:** Resolver rule 1 (line 108-109) against brief:33.
- **Problem:** The brief says "A `COS_*` env var still overrides for one command". The design makes `COS_*` plus `--business` an error instead of an override. The per-directory alternative at line 197 explains part of this, but the change to the brief's wording is never stated.

**9. Severity: Nit**
- **Scope:** Acceptance 2, line 334.
- **Problem:** `add` refuses directories that don't exist (line 135-136). The step never says that `~/biz/acme` and `~/biz/acme/repos` must already exist, so the check cannot be reproduced as written.

**10. Severity: Nit**
- **Scope:** Side-effect inventory table, line 241.
- **Problem:** One cell lists five separate capabilities, each with its own code reference, and shares one "Reversible" and one "Affected" value. That makes them hard to read and check one by one. Give each its own row.

Findings: Blocker 0, Major 4, Nit 6
