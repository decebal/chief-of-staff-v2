---
design: docs/plans/2026-11-28-role-library-design.md
design-sha256: 55a0b8fb441f61c3a27d4175255115af93e80389fc256cbadb35d0137a6dec61
verdict: findings
---

## Round 1 findings
- r1-b #1: closed. Design §3 "Load never drops a role" (L207-214); `policy.rs:146-148` fallback still only when `roles` is empty.
- r1-b #2: closed. Step 4 clears retired keys before serialise (L248-260); step 6 compares `serialise(parse(out)) == out` (L263-265). `policy.rs:44-49`, `:100-105` skip on write, so the bytes match. Nit carried: step 4 names roles and repos, not `defaults` (same `Terms` struct).
- r1-b #3: closed. Specialise run tier is `sand`, acceptance 9 live, `Toolset::Brain(vec![])` fallback (L293-300, L621). `grep Toolset::None` still only `claude.rs:140`.
- r1-b #4: open. `roles_propose_cancel` and `on_run_event` now exist on paper (L353-360), but the killed group is the `cos roles propose` child's. Its `claude` is a separate group leader: `claude.rs:193-198` → `proc.rs:174-181` → `proc.rs:210-211` (`process_group(0)`). See finding 1.
- r1-b #5: closed. Step 1 refuses with path, pid, running|not running; never taken over (L236-244). `state.rs:597-600` shape is the same, with no takeover.
- r1-b #6: closed. CAPS table covers every proposal string (L95-108); `\n`/`\r` refused in title, mandate, list items (L196); "any count 0 to max" defines present (L113-114). `output` and `scope` have no cap: carried.
- r1-b #7: closed. Checks-in-proposal row moved to `STORE(proposal_file)`, fake `contract` mode (L544). `Role` with checks written only after approval is the `apply` admit row (L545).
- r1-b #8: closed. `merge` takes `--id` else entry id (L309-313); `new <library-id> [--id]` kept (L325). Wording nit carried.
- r1-b #9: closed. `roles_save` named in the command table (L355) and ipc foreign-name case (L605-607). `named` at `lib.rs:279-281`.
- r1-b #10: closed. Materialisation and its one-time note stated (L250-256); `fit` and `defaults` not materialised.
- r1-b #11: closed. Repo `checks` excluded from `SpecialiseContext` (L286-290); exact text shown before Propose (L352, L582-584).
- r1-b #12: closed. Arithmetic checks: 8×240 + 10×240 + 8×240 + 6×240 + 1,200 + 600 = 9,480 (L111-113). `serves`/`sandbox`/`judged`/`done_when` are not rendered (`policy.rs:268-284`).
- r1-b #13: closed. `apps/desktop/examples/preview.rs` exists; `Taskfile.yml:38-46` change listed (L608-611).
- r1-b #14: closed. `ledger.rs:51-54` row id is the spec id; `by_role` at `ledger.rs:186-189` groups by role field. Design L35 wording now matches.
- r1-b #15: closed. `judge.rs:57` `#[serde(rename = "none")]` on `Nothing`; design uses `none` (L153-166).
- r1-b #16: closed. `unreachable` used (L378, L386); `states.js:117-129` has no `error` kind.
- r1-b #17: closed. `specialise_context` and `specialise_prompt` defined with signatures (L273-284, L281-283).
- r1-b #18: closed. Dialog loaders added to `tests/design.rs` loader list (L395-396; list at `tests/design.rs:1531-1543`).
- r1-b #19: closed. `create_dir_all(home/context)` before lock (L236). Inventory gap is finding 2.
- r1-b #20: partly closed. Lists and `retired` skipped on write (L81-83). `Option` fields and `Policy.fit` still serialise as `null`: carried.
- r1-b #21: closed. Acceptance 6 requires `policy::preamble(role)` in the plan (L644-647). Plan `hire.rs:33-52` has no preamble field yet.
- r1-b #22: closed. `COS_CLAUDE` read as `COS_HC` is (`herdr.rs:31-32`), stated L362-363.
- r1-b #23: closed. `ROLE_ICON` gains 12 icons (L175-177). Vendored set not checked: carried.

## Findings
1. blocking §7 Desktop (`roles_propose`, `roles_propose_cancel`, quit hook; §Semantic ISA Taint sink `TOOL_CALL(kill_group)`): the owner chain breaks, so Cancel and quit leave `claude` running. `roles_propose` runs `cos roles propose` as a group (`spawn_grouped`). That child runs `claude` through `claude::execute` → `proc::stream_lines` → `stream_lines_on` → `spawn_grouped` (`src/claude.rs:193-198`, `src/proc.rs:174-181`, `:210-211` `process_group(0)`). `claude` leads its own group. `kill_group(pgid)` on the held `cos` pgid (`src/proc.rs:300-306`) kills `cos` and not `claude`. The `claude` deadline lives in the dead `cos`, so it runs to completion or its `--max-budget-usd` (`claude.rs:127`). Acceptance 7 (`pgrep -f` count 0 after Cancel) fails for the same reason. Fix: the `Proposing` slot must hold the pgid of the process that runs `claude`. Either the app runs `claude` through an owned call that returns its pgid, reaped on the slot's owner thread, with the CLI `propose` path unchanged; or `cos` exposes its `claude` pgid to the parent. Restate the invariant as "the pgid in `Proposing` is the `claude` child's group" and assert in tests that the fake's pid is gone after Cancel and after quit. Closes r1-b #4.

2. blocking §Semantic ISA side-effect inventory (L427-440): two visible effects are missing. (a) `apply` step 1 `create_dir_all(home/context)` (L236) creates directories under `COS_HOME`. (b) `roles_propose` spawns a new process, the `cos` binary, as an owned group child (desktop; the same way `brief_run` does at `apps/desktop/src/lib.rs:1606-1607` and `src/pipeline.rs:706`). Neither appears in the table; only the `claude` run and the kill row do. Evidence: design L236, L353; `pipeline.rs:706`. Fix: add rows: "Create `context/` (and `.lock`'s parent) | local directories | `rmdir` by hand | none" and "Spawn `cos roles propose` child | local process, owned by the window | gone on exit or on `kill_group` | the propose run's owner". Re-check the table against `apply` once the owner fix in finding 1 lands.

## Carried to Plan
- §5 `specialise_context` / input schema: `repos[].scope` items and `staff` have no maxItems or maxLength (L277-279, L475-479); bound them.
- §1 CAPS: `output` (rendered at `policy.rs:271-272`) and `scope` items have no cap; add them and say which `clean` cap applies to `output`.
- §4 step 4: retired keys are cleared on roles and repos only; `defaults` (`Terms`, `policy.rs:113`) holds them too. `tests/hiring.rs:130` has them in `defaults`. Say so so the notes count is right.
- §1 serde: `from`, `output_format`, `done_when`, `harness`, `model`, `repo`, `output`, `deadline_secs`, and `Policy.fit` (`policy.rs:121`) serialise as `null` without `skip_serializing_if`. `from: null` fails on an old binary (`deny_unknown_fields`, `policy.rs:55`), and an old binary then falls back to template roles with a note (`policy.rs:141-143`). Add `skip_serializing_if` for `from`, `output_format`, `fit`, and say the downgrade behaviour in §1.
- §7: `roles_propose` must be `async` or spawn a thread and emit an event, as `brief_run` does (`lib.rs:1582-1629`); `agents_*` are `async fn` (`lib.rs:1144-1187`). A sync command would block the window while Cancel waits.
- §7 `Proposing`: store the pgid after spawn (a Cancel in between is a no-op as written); clear the slot and reap under one lock. prove-it I4 asks for an identity check before a group kill; state why slot ownership suffices.
- §7 `roles_propose` child args: pass `--home`, `--workspace`, `--projects` and the `COS_*` env as `brief_child` does (`pipeline.rs:692-705`); `specialise_context` needs the workspace for goal titles.
- §7 UI: say model text (rationale, role fields, problems) renders through `el()`/`textContent`, never `innerHTML`; the webview holds IPC to `roles_save` and `ritual_approve`. `app.js` has no `innerHTML` use today.
- §Semantic ISA taint: `out/audit/<day>.jsonl` holds the proposal text (`claude.rs:385` outcome), listed in the inventory but not as a sink. Name it as a sink with "no reader in the propose path", or add its check.
- §3 errors: a hand-edited typo key (`deny_unknown_fields`, `policy.rs:55`) refuses every write until fixed. Add to §8.
- §6: `cos roles new <library-id>` "adds the entry's role as is" conflicts with `--id`; reword.
- §2: Lucide icons for the 12 ids must exist in `apps/desktop/ui/vendor/lucide/`; if not, vendoring is a step.
- §Context `:35` ledger wording is fine; the `Policy` `PartialEq` derive (`policy.rs:108`) needs `Problem: PartialEq`.
- §5 `hire::Plan` (`hire.rs:33-52`): name the function that fills the new field, since `Plan` is built before `Policy::apply` (`agents/mod.rs:515`).
- Pre-existing, same chain: `brief_child` (`pipeline.rs:706-711`) → `claude::execute` (`claude.rs:193`) leaves `claude` alive on quit; `on_run_event` (`lib.rs:2080-2086`) stops only servers. Fix in the shared helper, not in this design.
- §Testing: `tests/roles_cli.rs` and `apps/desktop/tests/roles_page.rs` need `COS_CLAUDE` set in the test harness; state which test sets it (`tests/fixtures/fake-claude-proposal.sh` is new).
