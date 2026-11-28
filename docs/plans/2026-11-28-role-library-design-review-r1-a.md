---
design: docs/plans/2026-11-28-role-library-design.md
design-sha256: bbfbeb7025ce8b4abe606e867fb4b53f66aba7ddc7b2a032b1705ec4ef304a6f
verdict: findings
---

1. blocking §4 step 6 (round-trip): `Terms` and `Role` carry `retired_budget`, `retired_stall`, `retired_repeat` with `skip_serializing`. A file with legacy `budget_usd`/`stall_secs`/`repeat` parses to `Some(..)`, serialises without them, and parses back to `None`. The "result equals the policy" check then fails on every write to such a home. Evidence: `src/agents/policy.rs:44-49`, `policy.rs:100-105`, `policy.rs:33-34` (PartialEq). Fix: state that `apply` drops the retired keys on both sides before the comparison (or clears them on the policy first), and say so in the file-write notes.

2. blocking §5, §7 (Specialise Cancel): the dialog has a Cancel button during `roles_specialise`, but §7 defines no cancel command and no kill handle. `claude::execute` blocks in `proc::stream_lines` with no cancel path (`src/claude.rs:193-198`). A cancelled or quit app leaves `claude` running until its deadline, or past it once the owning thread dies. This breaks the process-ownership invariant. Fix: add `roles_cancel(run)` that holds the child handle, kills the group and reaps. Make app exit kill the group. Add both to the `ipc.rs` test list.

3. should §4 step 5 (effective vocab): `policy.fit` is `Option<Vocab>` and `load` fills the seed `fit` only in `load` (`src/agents/policy.rs:121-122`, `policy.rs:149-151`). `apply` reads raw bytes (step 2). A file without `fit` gives `unwrap_or_default()` (empty kinds), so every `work` value fails `validate`, and every write is refused. Fix: state that the write path applies the same seed fallback as `load`.

4. should §3, §4 (signature gap): `roles::specialise_context(home, workspace)` is named as the VERIFY for the `claude.specialise` sink (ISA §Sinks table), but §5 defines only the `SpecialiseContext` struct and never its builder. Evidence: design lines 176-185 and 361. Fix: define the builder: its inputs (`home`, `workspace`, `entry`, `brief`), the sources it reads (`state::read`, `policy.repos`, `policy.roles`) and the truncation or refusal rule for goal titles over 200 chars (`Bindings` schema maxLength).

5. should §5, §ISA (approval gate): `cos roles new X --specialise --yes` generates the proposal and writes it in one invocation (acceptance 3). The diff is printed only after the write, so `VERIFY(user_approval)` approves text nobody read. Also, §ISA names "desktop Save from the main window" as the approval, but no origin check is designed. `ritual_approve` relies on `named()` only (`apps/desktop/src/lib.rs:279`, `:1698-1708`). Fix: require a separate confirm (or a re-run without `--yes`) for any specialised write. Add an origin check to `roles_save` and `roles_specialise` and state it in the ISA table.

6. should ISA §Side-effect inventory (incomplete): missing rows for (a) `out/audit/<day>.jsonl` appends: `AuditLog::open` runs on every `execute` (`src/claude.rs:195`, `:313-327`), and "field and value" on a schema failure is written there through `audit_note` (`src/claude.rs:263`); (b) the `-p <prompt>` argv, which carries goal titles, repo names and the brief and is visible to `ps` for up to 180 s (`src/claude.rs:121`); (c) every hire's preamble, which now carries role text to the model provider on each spawn (`src/agents/policy.rs:250-252`), so the "Yes, reversible" row is wrong for the outbound copy; (d) library source links that open in the browser (§7). Fix: add each row with its reversibility and who is affected.

7. should ISA §Taint (sources): goal titles from `state.json` are used as prompt input (§5, `SpecialiseContext.goals`) but are not listed as tainted sources, although a goal title can be copied from an external source (mail subject, ticket title). Fix: list goal titles and repo names as tainted input to `GENERATE(proposal)`.

8. should §Security and privacy: "No secrets are involved" is unverified. `SpecialiseContext` sends each repo's `checks` (free-form shell strings from `context/agents.json`, `src/agents/policy.rs:38`, `:95`) to Anthropic. A check can embed a token (`TOKEN=… task test`). The recruiter does not need checks, since contract fields come from the library. Fix: send repo name and `scope` only, and drop `checks` from the prompt. Or state the redaction rule.

9. should §1 / Confidence (preamble size): the caps sum to 3,000 (principles, 10×300) + 3,600 (method, 12×300) + 3,000 (anti-patterns, 10×300) + 1,500 (output_format) + 2,400 (escalation, 8×300) = 13,500 characters, plus a mandate of up to 600. That is about 2,300 to 2,500 words, roughly 3,000 to 3,500 tokens per hire. The design says "about 1,500 words", and the brief's "preamble length (rock)" rests on that figure. Evidence: design lines 51-66; brief lines 74-75. Fix: correct the figure and the token claim, and reconsider the caps or mark the preamble length `sand`.

10. should §ISA "Hire names a retired role" / §1: `fit::check` builds its suggestions from every non-head role. `refusal` says "hire the X instead" (`src/agents/fit.rs:116-121`), and `suggest` is `roles.iter().filter(|r| !r.head).find(...)` (`src/agents/fit.rs:245`). A retired role gets suggested and then refused. Fix: state that `fit::check` and `refusal` skip `retired` roles, and add a test.

11. should §4 step 4 / §7 (signature mismatch): `apply(home, change, read_digest, fit_kinds)` needs `fit_kinds` to merge a library entry's kinds (§4, line 146). `roles_save(business, change, digest)` in §7 has no `fit_kinds`, and the CLI's `Edit` path never sets it. Fix: either pass `fit_kinds` from the caller (the entry id as `from`, looked up in `roles::library()`) or have `apply` look up the library from `change.from` itself, and state which.

12. should §Testing, §Acceptance 3 (fixture cannot produce a proposal): `tests/fixtures/fake-claude.sh:33` always returns `structured_output: {"headline":"Fake brief"}`, which is not a proposal, and it has no schema-failure mode. Acceptance 3 (`cos roles new sre --specialise --brief …` with the fake) cannot pass as written. Fix: add a proposal mode (and a schema-failure mode) to the fixture, and name it in §Testing.

13. should §Acceptance 6, §6 (dry-run premise is false): the design says "today dry-run prints the plan without the prompt". `hire::Plan` already has `prompt` (`src/agents/hire.rs:33-37`), and dry-run prints the whole plan as JSON (`src/main.rs:988-989`). The preamble is not in `plan` at all. It is added only in `record_run` through `policy::load(&home).apply(...)` (`src/agents/mod.rs:513-515`). Fix: say the plan must call `policy::preamble(role)` (or `Policy::apply` on a copy) to show it, and correct the premise.

14. should §Acceptance 5 (wrong string): the ledger row's `title` is the task title (`src/agents/ledger.rs:78` from `spec.title`; `src/agents/mod.rs:568` from `req.title`; `src/agents/hire.rs:153` `Plan.title = task.title`). The role title is not stored in the row, and the ledger groups by role id (`ledger.rs:41`, `:138`). "Still show Reviewer" will not hold. Fix: name the view that shows the role title (the Staff pane's policy lookup), or check the role id `reviewer`.

15. should §Testing, §Acceptance 7 (no e2e target): the design says `task test-e2e` runs a role-dialog pass. `Taskfile.yml:38-46` runs only `flows_page` and `kit_page`. Fix: add the new browser pass to a named task, and list it in acceptance.

16. should §3 / §1 (load-time validation): `Policy.note` is one `Option<String>` (`src/agents/policy.rs:124-125`), so it cannot name several invalid roles. Dropping an invalid head at load also leaves `Policy::hire` with no head. Fix: make the note a list (or a per-role problem list), and state what happens when the head is invalid.

17. nit §4 step 6: `atomic_write_bytes` names its temp file `.state-<pid>-<nanos>.tmp` (`src/state.rs:561`). Fix: say it is reused and that the temp name is a `.state-` prefix, or pick a neutral prefix.

18. nit §4 step 1: `Lock` in `state.rs` is private (`src/state.rs:597`), so "the same shape as state::commit" means copying the struct. A lock left by a crash is never removed, and the error should name the path and the recovery step. Fix: say both, and name the lock path in the refusal text.

19. nit §4 STORE binding (ISA §Bindings, line 334): `$ref: "Role"` is not defined in the schema. Fix: inline the Role schema or name its definition.

20. nit §2 library entry table: the JSON value for `expects` is `"none"`, not `"nothing"` (`src/agents/judge.rs:57`). The `debugger` entry's `work` kind is not in the list of new `fit.kinds` (§2). Fix: use `"none"` and add the kind or name the existing one.

21. nit §2 library JSON: the entry carries `id` both at the top and inside `role`. Nothing says they must match. Fix: add the equality check to `validate`.

22. nit ISA §Side-effect inventory: `.bak` holds only the previous bytes and is overwritten on each write, so undo is one level. Fix: say so.
