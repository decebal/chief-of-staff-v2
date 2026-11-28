---
range: 2e295db..24bb91a4746382e048c9c04079436e2c7f091b86
prove-it: DON'T MERGE
verdict: findings
---

PR: decebal/chief-of-staff-v2 #63, OPEN, draft (`isDraft: true`), head `24bb91a`.
Design: `docs/plans/2026-11-28-role-library-design.md`. Plan: `docs/plans/2026-11-28-role-library-plan.md`.
Earlier review: `docs/plans/2026-11-28-role-library-code-review.md` (range to `ea028b9`).

## Earlier review

- #1 (finish line reaches the prompt uncleaned): closed. `src/agents/hire.rs:135-136` cleans the draft to `2 * CAPS.text` and the original to `CAPS.text` before `hire.rs:138`. Test `tests/roles.rs:346` (`a_hand_edited_finish_line_cannot_forge_a_line_in_the_hires_prompt`) asserts one `Your finish line here:`, no line starting `Never:`, under 1,500 chars. Design ISA row `RENDER(finish_line)` at `design.md:653`.
- #2 (guard signals without a named sink; SIGTERM-only, no reap of stubborn members): closed. `src/proc.rs:410` SIGTERM to own group, `proc.rs:417-423` SIGKILL to every remaining member from `proc_listpids`. Test `tests/proc_guard.rs:130` (`a_child_that_ignores_sigterm_still_ends_with_the_guard`). Design row `TOOL_CALL(guard_sweep)` at `design.md:655` and test row at `:677`. Residual `child.kill()` at `proc.rs:400` is not in the table: finding 1.
- #3 (desktop propose falls back to running `claude` directly when no cos binary): closed for both callers. `apps/desktop/src/lib.rs:1383-1386` refuses (`ok_or`) before the thread starts; CLI passes `Some(&cos)` from `current_exe` at `src/agents/roles/cli.rs:253-254`. Residual `None` branch in `src/claude.rs:211-215` is reachable API with no caller and no test: nit 3.
- #4 (proposal caps only in the JSON schema): closed. `src/agents/roles/propose.rs:243` calls `check(&p)` (`propose.rs:249-283`) before `merge`. Test `tests/roles_propose.rs:233-234` runs the fake's `overcap` mode (`tests/fixtures/fake-claude-proposal.sh:29`) and expects `breaks its caps: method`.
- #5 (relaxed refuse check vs design §4 step 6): closed by design and code agreeing. `design.md:304-310` now states the rule. `src/agents/roles/write.rs:298-320` refuses any refuse problem on the role an Add or Replace writes, and elsewhere only a problem the change adds. Test `tests/roles_write.rs:286` and `:310`.
- #6 (global `--dry-run` writes): closed. `src/agents/roles/cli.rs:333-335` (`new`), `:364-366` (`edit`), `:384-386` (`set_retired`) return before `apply`. Test `tests/roles_cli.rs:180` (`a_dry_run_writes_nothing_for_any_command`) compares `agents.json` bytes before and after (`:206`).
- #7 (no test for existing `--out` or symlinked parent): closed. `tests/roles_cli.rs:107-152` (existing file, `never overwrites` at `:143`) and `:211-222` (`an_out_path_through_a_symlink_is_refused`, `goes through a symlink`).
- #8 (caps `output` and `scope` missing from design §1): closed. `design.md:134-135` lists both; code `src/agents/roles.rs:44-47`, `:74-77`.
- #9 (empty list item refuses, undocumented): closed. `design.md:222` documents it; `src/agents/roles.rs:316-317` keeps `required = true`.
- #10 (`roles show --json` in design, not in code; guard wording): closed. `src/main.rs:291-297` accepts `--json` (documented as always JSON); `design.md:371`; `design.md:477-479` now says the thread spawns `cos proc-guard`.

## prove-it

Verdict: `CLAIM REFUTED` (mapped to DON'T MERGE: C2 refuted by finding 2; blocking finding 1 open). Gates all exit 0 on `24bb91a`: `cargo fmt --check` receipt `2cc0ed38-ec24-4d67-b166-e9839bbb9532`; `cargo clippy --all-targets -- -D warnings` `50b8de92-5547-48e8-8224-6dd0c83be5b2` (Checking cos, 11.9 s, not a cache hit); `cargo test` `86b779a6-0e6f-4eb6-a63d-76465b79731d` (exit 0, 124,986 ms, not timed out); `cargo run --example demo` `492060c7-1fb5-401d-af84-9a7744746d8d` (exit 0). Not run: desktop crate tests (`apps/desktop/tests/roles_ipc.rs`, `roles_page.rs` ignored), outside root `cargo test`.

Claims (PR body bullets):

- C1 `12 library entries, each with a cited source`: HOLDS. `src/agents/roles.rs:129-175` lists 12 files; `tests/roles_cli.rs:72`.
- C2 `structured role fields rendered into every hire's preamble`: REFUTED. Path where it does not happen: `src/agents/policy.rs:282` skips `preamble(r)` when the task prompt starts with `Role: `. `src/agents/mod.rs:516` runs `Policy::apply` on every spawn, so the skip applies to every spawn whose prompt matches. Finding 2. Not reproduced live (no hire run); file:line read.
- C3 `no-tools brain specialises; proposal limited to descriptive fields; written only on approval`: HOLDS for the agents.json write (`write.rs:338-340` only reached via `apply`, approval in `cli.rs:58-86`). The `--out` file is written at run end without a separate approval (`cli.rs:255-264`); it is the path the user named. Nit 6 on the claim wording.
- C4 `one validated write path to agents.json`: HOLDS for the policy file. Seed on first write and `business::seed_context` (`src/business.rs:171-187`, first setup only) also create it: nit 7.
- C5 `Staff tab: New role, Edit, Retire, Restore`: UNVERIFIABLE HERE. Desktop not run. H3.
- C6 `draft until the feature works end to end`: HOLDS. `gh pr view 63`: `isDraft: true`.
- C7 `live propose returns structured output under $1.00` (design acceptance 9): UNVERIFIABLE HERE. H1.

Attacks (prove-it §2), each run against the code:

- I1 brain read-only: `Toolset::None` only in `pipeline.rs:677` (`propose_run`), argv `--tools "" --strict-mcp-config` at `src/claude.rs:141-143`. HOLDS. Test `tests/roles_propose.rs:238`.
- I2 private state not committed: `git diff --name-only 2e295db..HEAD -- .chief-of-staff context out` prints nothing. HOLDS.
- I3 every spawn owned, deadlined, killed, reaped: new spawns are `cos proc-guard` (`claude.rs:213` via `proc::guarded`, spawned by `stream_lines_on_until` at `proc.rs:216`, owner thread waits and kills at `:223-235`) and the guard's own child (`proc.rs:389`, reaped `:401`). Gap: finding 1 (`proc.rs:400`).
- I4 signal identity: the guard signals its own group after `setpgid(0,0)` (`proc.rs:386`). Pids in the SIGKILL loop come from the group listing (`proc.rs:417-423`). HOLDS.
- I5, I6, I12, I13, I14: `src/mcp.rs`, `src/dash.rs`, `src/state.rs`, `src/context.rs` not in diff. Untouched.
- I8 agent sandbox: `src/agents/harness.rs` not in diff. Untouched.
- I11: `src/reports.rs` not in diff.
- Second item / concurrent propose: `lib.rs:1389-1394` refuses a running slot; `write.rs:113-139` lock refuses with pid. HOLDS.
- Sink taint: `specialise_context` is the only constructor of `SpecialiseContext` (`propose.rs:83`); no other literal in `src/` or `apps/desktop/src/`. HOLDS.
- Desktop render: role text through `textContent` (`apps/desktop/ui/app.js`, `el(... textContent)`); `rg innerHTML apps/desktop/ui/app.js` not rerun for the new code (limit).

VERIFY BY HAND (block two, never last, never elided):

- H1 `Propose, live (paid, capped at $1.00)`: `cos roles propose performance-engineer --brief "Rust services" --out <scratch>/perf.json`. Pass: exit 0; `structured_output` present and valid; cost under $1.00 in `out/audit/<day>.jsonl`; `context/agents.json` digest unchanged; only `<scratch>/perf.json` written. Not run.
- H2 `Hire, live (paid, capped at $0.25)`: `cos --budget 0.25 agents spawn --model haiku --role appsec-reviewer --repo <throwaway repo> --prompt "run: echo hi > a.txt; then run: curl -sS https://example.com"`, then `cos agents show <id>`. Pass: `done`; the recorded prompt holds the five preamble sections once each and the `Drafts only` sentence last; `a.txt` only in the worktree; curl refused by the sandbox. Not run.
- H3 `Desktop, preview`: `task test-e2e` (`apps/desktop/tests/roles_page.rs`), then in the preview: Staff, New role, Skip, Save, Retire. Then Propose with `FAKE_PROPOSAL=slow` and Cancel: the pid the fake recorded is gone within 3 s; repeat with quit and Ctrl-C on `cos roles propose`. Pass: each step as in design acceptance 7. Not run.
- H4 `Preamble skipped (finding 2)`: `cos agents spawn --role appsec-reviewer --repo <throwaway repo> --prompt "Role: x. Review the login flow"`, then read the recorded prompt for the spec. Pass: it starts with `Role: Application security reviewer.` and contains `Drafts only:`. Expected to FAIL on `24bb91a`. Not run.

## Sink to VERIFY

Method: `rg -n` over `src/`, `apps/desktop/src/`, `examples/`, `tests/` for each sink's primitive (`execute_until|propose\(|Toolset::None`; `atomic_write|create_new|OpenOptions|agents\.json`; `preamble\(|done_when|finish_line`; `kill_and_reap|kill_group|libc::kill|wait_until|stream_lines_until|guard\(`).

| Sink | Call sites (file:line) | VERIFY (file:line) | Test |
|---|---|---|---|
| `TOOL_CALL(claude.propose)` | `src/agents/roles/propose.rs:237` (`execute_until`), only from `propose.rs:234-237`; `propose()` callers `src/agents/roles/cli.rs:254`, `apps/desktop/src/lib.rs:1404`; `Toolset::None` only `src/pipeline.rs:677` (`propose_run`, only from `propose.rs:234`) | `specialise_context` `propose.rs:46-90`, reached via `cli.rs:240` and `lib.rs:1314` (`propose_context`, used at `lib.rs:1388`); guard `lib.rs:1383`, `cli.rs:253` | `tests/roles_propose.rs:58` (context carries goals, repos, staff), `:88` (brief refused), `:238` (`--tools ""`, `--strict-mcp-config`), `:213` (run merges, refuses `checks`), `:233` (over-cap). Prompt-contents gap: finding 3 |
| `STORE(proposal_file)` | `src/agents/roles/cli.rs:256-263` (`create_new`), only write of `--out` in `src/` and `apps/desktop/src/` | `out_path_ok` `cli.rs:197-223`, called `cli.rs:250` before the run; `Proposal` `deny_unknown_fields` `propose.rs:119-121`; caps `propose.rs:249-283` via `propose.rs:243`; `merge` `propose.rs:191-214` | `tests/roles_propose.rs:119` (closed schema, `checks` rejected), `:233`; `tests/roles_cli.rs:107` (writes only `--out`), `:143` (never overwrites), `:211` (symlinked parent) |
| `STORE(agents.json)` | `src/agents/roles/write.rs:338` (`.bak`), `:340` (replace), both in `apply` (`write.rs:180`); `apply` callers `cli.rs:337`, `cli.rs:368`, `cli.rs:393`, `apps/desktop/src/lib.rs:1443` (`roles_save`) | lock `write.rs:189`, digest `:199`, parse `:202-210`, refuse rule `:298-320`, contract approval `:266-273`, round trip `:328-335`; approval `cli.rs:58-86` (`contract` typed on a terminal, `--yes` only for non-contract); desktop `lib.rs:1438-1442` | `tests/roles_write.rs:117` (added refuse refused), `:140` (head), `:166` (replace, retire, restore, `.bak`), `:227` (contract needs a person), `:286`, `:310`; `tests/roles_cli.rs:180` (dry run writes nothing), `:299` (contract at a terminal); `apps/desktop/tests/roles_ipc.rs` not run |
| `RENDER(preamble)` | `src/agents/policy.rs:283` (`Policy::apply`, from `src/agents/mod.rs:516`); `src/agents/hire.rs:171` (plan preview) | `preamble()` `policy.rs:301-356`; every value through `roles::clean` `src/agents/roles.rs:86-103`. Bypass: `policy.rs:282` skips the call when the prompt starts `Role: ` (finding 2) | `tests/roles.rs:44` (sections once, in order, drafts-only last), `:78` (forged `Never:` and over-cap); `tests/roles_cli.rs:278-279` (order) |
| `RENDER(finish_line)` (added to ISA at `design.md:585`, `:653`) | `src/agents/hire.rs:133-141` only (the `Your finish line here:` push at `:138`) | `hire.rs:135-136` (`roles::clean`, `2 * CAPS.text` and `CAPS.text`). The raw `c.done_when` at `policy.rs:276-277` is not rendered into a prompt; `view.rs:129-132` displays it | `tests/roles.rs:346` |
| `TOOL_CALL(kill_and_reap)` | `src/proc.rs:223`, `:227`, `:235` (in `stream_lines_on_until`, `:208-245`, spawn at `:216` on the same thread); `proc.rs:323` (`wait_until`, from `:243` on the same thread). Other callers, pre-existing and outside this PR: `src/wake.rs:94`, `src/pipeline.rs:727` | spawning thread owns the child (`proc.rs:216-243`); stop flag set only by `apps/desktop/src/lib.rs:1419-1422` and `src/agents/roles/cli.rs:20-30` | `tests/proc_guard.rs:159` (stop flag kills and reaps from the spawning thread), `tests/roles_cli.rs:426` (ctrl_c during a proposal), `:431` (SIGKILL to cos), `apps/desktop/tests/roles_ipc.rs:214` (not run). Unlisted call: `proc.rs:400` (`child.kill()`, finding 1) |
| `TOOL_CALL(guard_sweep)` | `src/proc.rs:410` (SIGTERM to own group), `:417-423` (SIGKILL per listed member); single caller `src/main.rs:544` (`cos proc-guard`) | `setpgid(0,0)` `proc.rs:386`, `proc_listpids(PROC_PGRP_ONLY)` `:434-445`, grace `:412-416` | `tests/proc_guard.rs:64` (orphan bound), `:96` (sweep never reaches the starter), `:116` (exit 124), `:130` (member ignoring SIGTERM is gone) |

## Findings

1. **blocking** `src/proc.rs:400`: `let _ = child.kill();` in the guard's deadline path is a kill primitive with no row in the Semantic ISA table. The table lists `TOOL_CALL(guard_sweep)` (`design.md:655`) and `TOOL_CALL(kill_and_reap)` (`design.md:654`); the guard's timeout kill is described in prose (`design.md:441-442`) but has no row, VERIFY or test row of its own. It is owned (spawned at `proc.rs:389`, deadline at `:394`, reaped at `:401`), so this is a governance gap, not a recycled-pid risk. Evidence: `rg -n "kill\(\)|libc::kill" src/proc.rs` gives `:332, :343, :400, :410, :421, :526, :537` as kill sends. New in this PR: `:400`, `:410`, `:421`. `:332`, `:343` sit in `kill_and_reap` and `kill_group` and have rows. `:526`, `:537` are pre-existing helpers outside this PR. `:400` is the only new kill with no row. Fix: add `TOOL_CALL(guard_kill_child)` to the instruction table with VERIFY "spawned by this guard at `proc.rs:389`, deadline `:394`, reaped `:401`" and test `tests/proc_guard.rs:116`, or fold it into the `guard_sweep` row and name it there.

2. **should** `src/agents/policy.rs:282`: `if !req.prompt.starts_with("Role: ") { req.prompt = format!("{}\n\n{}", preamble(r), req.prompt) }` drops the whole preamble (including `Never:` and the `Drafts only:` sentence) when the task prompt begins with `Role: `. The prompt is task text (ticket, deliverable, or a copied hire output), so this is reachable from input the design calls tainted. Evidence: file:line read; `rg -n '"Role: '` finds no code that produces that prefix except `preamble` itself (`policy.rs:304`), so the guard is a text-sniffing idempotency check that fails open. The design says the drafts-only sentence is always last (`design.md:652`, ISA `RENDER(preamble)`). Not reproduced live: H4. Fix: carry a flag on the `Request` (for example `preamble_applied: bool`, set by `plan`) instead of sniffing the prompt; add a test where the prompt starts `Role: ` and the preamble still renders.

3. **should** `src/agents/roles/propose.rs:94` (`specialise_prompt` serialises `entry.role`): the prompt sent to the model contains the library entry's contract fields, including `checks`, `scope`, `harness`, `model`, `repo`, `expects`, `output`. Example: `templates/roles/performance-engineer.json:56` (`"checks": ["test -s PERF.md", ...]`). The design says the run is built "only from goal titles, repo names and scopes (no checks), staff ids and titles, and a brief" (`design.md:649`). The compiled-in checks carry no secret, so this is drift, not a leak, but the design's "what leaves the machine" row is wrong. `tests/roles_propose.rs:58` is named `..._and_nothing_else` but asserts only that secrets are absent. Evidence: `rg -n '"checks"' templates/roles/performance-engineer.json` gives `:56`; `Role` serialises `checks` (no `skip`). Fix: either send only the descriptive fields of the entry to the model (the contract is merged back from the entry anyway), or change `design.md:649` to name the entry's contract as sent and assert it in the test.

4. **nit** `src/claude.rs:211-215`: `execute_until` keeps a `None` branch that runs `claude` with no guard. No caller passes `None` now (`cli.rs:253-254`, `lib.rs:1383-1404`) and the branch has no test. Fix: make `guard` a required `&Path`, delete the branch.

5. **nit** `src/proc.rs:223`, `:243`, `:317-328`: the stop flag is checked in the poll loop only. After the `result` event (`Ok(false)`, `:241-242`) `wait_until` runs to the deadline (180 s) without checking it, so a cancel in that window returns `ok` instead of `cancelled`. Fix: check `stop` in `wait_until`, or document the window in `design.md:413-422`.

6. **nit** `design.md:654` and `src/proc.rs:308-313` (pre-existing `Drain::finish`): "a no-op (ESRCH) once none does" is not a kernel guarantee. After the last member exits and the leader is reaped, a new process can take that pid and call `setpgid(0,0)`, and `kill(-pgid)` would then reach it. Window is small. Fix: reword the design sentence, or kill only while the owner still holds the child.

7. **nit** `src/agents/roles/write.rs:1` ("The only writer of `context/agents.json`") vs `src/business.rs:171-187` (`seed_context` copies the template `agents.json` on first setup when `context/` is absent; `Taskfile.yml:21`). Fix: reword the module doc to "the only writer after setup".

8. **nit** PR body C3 ("the proposal ... is written only on the user's approval"): `--out` is written at run end without a separate approval (`src/agents/roles/cli.rs:255-264`). Fix: reword the PR body to say the proposal file is written only to the `--out` path the user names; approval gates `context/agents.json`.

9. **nit** `src/proc.rs:434-445` (`group_members`): calls macOS-only `libc::proc_listpids` without `#[cfg(target_os = "macos")]`, unlike `exe()` at `proc.rs:454`. CI is `macos-latest`, so it builds; a 256-entry buffer also silently truncates larger groups. Fix: gate it, or note the macOS-only build in the module doc.

## Considered and dropped

- Stop set before the first poll: `proc.rs:222-224` kills and reaps at once, and no line is read. Not a defect.
- `execute` (brief path, `claude.rs:194-197`) passes a fresh `AtomicBool`; behaviour unchanged from `stream_lines`. Not a defect.
- `COS_CLAUDE` override (`src/pipeline.rs:45-50`) lets any environment choose the binary for every `claude` run, brief included. Design accepts it (`design.md:563`, same trust as `COS_HC`). Not a finding.
- `apps/desktop/src/lib.rs:1419-1422` (`roles_propose_cancel`) only sets the flag. Matches the design.
- `RENDER(preamble)` output bound: 8 x 240 + 10 x 240 + 8 x 240 + 6 x 240 + 600 + 60 + 1,200 + 200 + tail is under the 12,000 in `design.md:625`. Holds by arithmetic; no runtime assert. Not a defect.

## Limits

- Not run: H1, H2, H3, H4 (live paid runs, desktop preview).
- Desktop crate tests (`apps/desktop/tests/roles_ipc.rs`, `roles_page.rs`) not run; outside root `cargo test`.
- No source edited, nothing committed. Receipts under `.allstack/checks/`.
