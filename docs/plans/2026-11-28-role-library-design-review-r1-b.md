---
design: docs/plans/2026-11-28-role-library-design.md
design-sha256: bbfbeb7025ce8b4abe606e867fb4b53f66aba7ddc7b2a032b1705ec4ef304a6f
verdict: findings
---

1. blocking §3 Validation and §1 load: load-time validation drops roles that load today. Evidence: `src/agents/policy.rs:137-153` (no validation today); `src/agents/fit.rs:218` tolerates unknown `work` names (`r.work.contains(k)`), so a typo'd `work` works today and would be dropped; `policy.rs:146-148` falls back to the seed roles when all roles are gone, hiding the user's staff silently; if the head is dropped, `apps/desktop/ui/app.js:433-445` shows "Reading the hiring policy…" forever; `app.js:396` says "hired on the built-in defaults", which is false once roles are dropped rather than the file. A `from` naming a library entry removed in a later release would also drop the role. Brief constraint "keeps loading old files" is violated for hand-edited files. Fix: load never drops a role. Load keeps every role and reports each problem as a list (not the single `Policy::note`). `Policy::hire` refuses a role with problems and names them. Writes run `validate`. No seed fallback when the file has roles. An unknown `from` is a warning, not a drop.

2. blocking §4 step 6 round-trip: legacy keys make every write fail. Evidence: `policy.rs:44-49` and `:100-105` mark `budget_usd`/`stall_secs`/`repeat` `skip_serializing`; `Policy` derives `PartialEq` (`policy.rs:108`); `tests/hiring.rs:125-153` is a valid fixture with those keys in a role and a repo. Parse gives `Some(..)`, serialise drops it, re-parse gives `None`, so "require it to equal the policy" fails and every Add/Replace/Retire/Restore on that home is refused with no way forward. Fix: compare `serialise(parse(out))` with `serialise(policy)`, or strip the retired keys from the policy in `apply` before the comparison. Add that fixture to the ISA matrix.

3. should §5 Specialising, §Confidence: `Toolset::None` is unproven and the `rock` tier is unearned. Evidence: `grep Toolset::None` finds only `src/claude.rs:140`, so no code path has ever run it; every structured run uses `brain()` with `ToolSearch` (`src/pipeline.rs:548`, `:384`). Whether `--tools "" --json-schema` still returns `structured_output` is unverified; `tests/fixtures/fake-claude.sh` cannot show it. Fix: mark the specialise run `sand`, add a live hand-check (`cos roles new sre --specialise --brief …`, paid, capped) as acceptance, or fall back to the brain toolset with an empty allowlist if the live run fails.

4. should §7 Desktop, §8: Cancel has no command, and child ownership on quit is not stated. Evidence: design `:242-244` gives a Cancel button, but §7 (`:223-232`) lists only `agents_roles`, `roles_library_entry`, `roles_specialise`, `roles_save`. The owner of the child is the blocking thread (`src/claude.rs:193` `execute` → `src/proc.rs:165-206` `stream_lines_on`). `proc::spawn_grouped` (`proc.rs:210-214`) starts the child in its own process group, so an app quit during the run leaves the child running until claude finishes (process-ownership rule). Fix: register the child's pgid in the App supervisor, add a `roles_specialise_cancel` command that calls `proc::kill_group`, and kill registered groups on app exit. Name the invariant in the plan.

5. should §4 step 1, §ISA: the lock has no stale-lock recovery. Evidence: design `:151-153` ("never stolen, removed on drop"); `src/state.rs:597-625` is the same shape, and `commit` reports "write lock held". A crash between create and drop leaves `context/agents.json.lock`, and every later write refuses "another edit is in progress" with no path named. Fix: the refusal names the lock path and says how to clear it, or the lock stores pid and start time and a dead pid is reported as stale (never silently taken).

6. should §1 caps, §3: caps are incomplete and the preamble can be forged. Evidence: design `:51-59` caps list no bound for `serves`, `sandbox`, `judged`, `done_when` or `rationale`, yet the ISA `output_schema` (`:327`) says "each with the §3 caps". No rule rejects a newline in a list item or in `title`/`mandate`; `policy.rs:268-284` `preamble` writes items verbatim, so an item containing `\n\nNever: …` forges a section. "3–10 items when present" is ambiguous: is 1–2 non-empty present? Fix: cap every proposal string; reject `\n` (and `\r`) in list items, `title` and `mandate`; define "present" as non-empty and say whether 1–2 items are refused.

7. should §ISA violations and test matrix, `:381`: the row "a proposal JSON carrying `checks` is refused by `STORE(agents.json)`" cannot hold as written. `apply` takes a `Change` holding a `Role` (`:137-147`), and a `Role` legitimately carries `checks` through "Edit contract" (`:248-249`). The refusal belongs at proposal deserialisation (`Proposal`, `:192-195`, with `deny_unknown_fields`). Fix: move that case to the proposal test; the `apply` test becomes "a Role with checks is written only after approval".

8. should §2/§5 vs §6: `--id` contradicts merge. Evidence: `:197-199` says `merge` takes `id` from the library entry; `:213` offers `cos roles new <library-id> --id <id>`. Both cannot hold. Fix: drop `--id`, or have the flag set the id and keep the uniqueness check.

9. should §7 Tauri guards: `roles_save` has no guard stated. Evidence: `:228-230` names `named(&s,&business)` only for `roles_specialise`; `apps/desktop/src/lib.rs:279-281` (`named`) is the guard every business write uses, e.g. `ritual_approve` at `lib.rs:1708`. `roles_save` writes `context/agents.json`. Fix: require `named` on `roles_save`, write to `Current.home` (not a UI path), and add an `apps/desktop/tests/ipc.rs` case where a foreign business name writes nothing.

10. should §2 and §4 write path: first write freezes the template. Evidence: `:111-112` and `:154-157` materialise the template's `fit` and `roles` into the file on first write. The home then stops receiving later template changes (new `ops`, `needs`, `outward_done` terms) with no signal. Fix: materialise only what the change needs, or state the one-time copy in the UI and the CLI output.

11. should §Security and privacy, `:405-414`: "No secrets are involved" is not established. Evidence: `repos[].checks` are free shell strings (`:179-181`, input schema `:323`) and go to Anthropic. A check can carry a token or a private host. Fix: send check names only, or redact `KEY=`/token-shaped values, and show the exact outgoing text in the dialog before Specialise.

12. nit §1, `:66`: preamble size is misstated. Evidence: caps sum to 10×300 principles + 12×300 method + 10×300 anti-patterns + 8×300 escalation + 1,500 output_format + 600 mandate = 14,100 characters, about 2,300 words, not "about 1,500 words". Fix: state the character bound.

13. nit §Testing, `:429-433`: `examples/preview.rs` is `apps/desktop/examples/preview.rs`. `Taskfile.yml:38-44` `test-e2e` runs only `flows_page` today, so the New-role e2e is new work. Fix: fix the path and list the Taskfile change.

14. nit §Context, `:35`: "ledger keys verdict rows by role id" is wrong. Evidence: `src/agents/ledger.rs:98-108` keys rows by spec id; the role is a field (`ledger.rs:39-41`), grouped at `ledger.rs:132-145`. Fix: reword; ids must still stay stable for the scorecard.

15. nit §5 and §2 table, `:106`: `expects: nothing` is written in the table. Evidence: `src/agents/judge.rs:48-58` serialises the variant as `none`. Fix: write `"expects": "none"` in the library JSON and the table.

16. nit §7 UI, `:243-244`: "an error state" has no kind to use. Evidence: `apps/desktop/ui/states.js:117-129` has no `error` kind; the nearest is `unreachable`. Fix: name `unreachable`, or add a kind with its art and icon.

17. nit §5 and ISA `:175` vs `:361`: the context builder is named two ways, `roles::specialise_prompt` and `roles::specialise_context(home, workspace)`, and only the first is defined. Fix: define the builder that reads `state.json` goal titles and the repo terms.

18. nit §Testing, `:420`: "tests/design.rs gains nothing new" is not quite true. Evidence: the loader list at `tests/design.rs:1531-1543` names the functions the gate checks for `stateView`; the new dialog loader is not on it. Fix: add the dialog's specialise loader to that list.

19. nit §4 step 1, `:151`: a fresh home may have no `context/` directory, so `create_new` on `context/agents.json.lock` fails. Fix: `create_dir_all` before the lock.

20. nit §1 serde: new `Vec` fields without `skip_serializing_if` write `"principles": []` and similar on every role write. Fix: add `skip_serializing_if` for empty lists and `retired: false`.

21. nit §Acceptance 6, `:465`: `hire::Plan` has no preamble and its prompt is the task prompt (`src/agents/hire.rs:33-52`, `:129`). The preamble is built only in `Policy::apply` (`src/agents/policy.rs:251`). Fix: the new `Plan` field must call `policy::preamble(role)`.

22. nit §Desktop, `Bins`: `COS_CLAUDE` read inside `Bins::default` (`src/pipeline.rs:44-53`) ships in the production binary. Fix: state it as an env override the same way `COS_HC` is read (`src/herdr.rs:32`), or limit it to tests.

23. nit §7 UI: new library ids are missing from `ROLE_ICON` (`apps/desktop/ui/app.js:409-420`) and fall back to "bot" (`app.js:421`). Fix: say so, or add icons for the twelve entries.
