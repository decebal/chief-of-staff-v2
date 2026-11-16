# Limiting-factor tracker: spec notes

**Status:** input to the Spec step, not an approved design.
**Author:** Decebal Dobrica
**Date:** 2026-11-15
**Brief:** [`2026-11-15-limiting-factor-brief.md`](2026-11-15-limiting-factor-brief.md)

This file holds the design-level detail removed from the brief so the brief stays owner-facing. The sections below are carried over from the Think draft (`constraint-brief.md`) unchanged in wording and reorganised by topic. Where the last critic round found a section wrong or incomplete, the finding is listed under "Open findings Spec must resolve" at the end; the section text itself has not been corrected, so read the two together. Two brief decisions already override the draft: the cheapest instrumentation (owner-gate emitter, shim and CPU sampler records, task id and run kind in `spec.json`, I7 size snapshots) moves into the first slice, and agent runs are attributed to `tool-build` or a business by role or epic link, so `default`'s K6 48% is not business output.

Paths: source paths are under `~/Projects/chief-of-staff-v2-wt/business-dashboards`. Run data for the `default` business is under `~/Projects/chief-of-staff-v2-wt/multi-business/out/agents` (run directories, `ledger.jsonl`, and `events.jsonl` inside each run directory); `acme` (`~/biz/acme/out`) has no `out/agents`. cn data is read with `cn show <id> --toon` and `cn list --all` from `~/Projects/chief-of-staff-v2`. Timestamps are WET (UTC+0 in November). Related: `docs/design.md:21` (binds `Brief.week` to strategy), `plugins/chief-of-staff-starter/skills/chief-of-staff-starter/references/strategy.md:24` (free-text bottleneck prompt).

## Contents

1. Data sources
2. Instrumentation
3. Scope, shared resources and attribution
4. Constraint model: weighed time, stages, measurement labels, detection rules, episodes
5. KPIs
6. Improvement-suggestion loop and verdict statistics
7. Surfaces and reuse candidates
8. Worked example, full tables
9. Draft staging (superseded by the brief)
10. Open findings Spec must resolve

## Data sources

Data span at the snapshot: the first cn event is 2026-11-12 15:56:23 (`t-0c18d3` created), the first ledger row is judged 2026-11-12 19:53:26, the last 2026-11-15 08:49:14. The span covers 4 calendar days (2026-11-12 to 2026-11-15) but **3 windows with data**: every timeline in the cn store (both epics and all 61 children) was read, and none has an event dated 2026-11-14; the ledger has 8 rows judged 2026-11-12, 10 on 2026-11-13, none on 2026-11-14 and 1 on 2026-11-15.

| # | Source | Timestamped fields | Supports | Gap |
| --- | --- | --- | --- | --- |
| 1 | `cn show <id> --toon`; `cn list --all` (filter by parent) | `task.created`, `task.updated`, `task.dependency.added`, `workflow.claimed`, `workflow.step.completed`, `workflow.approval.granted`; fields `claimed_by`, `blocked_by`, `status` | K-D, K2, K4, K5, all task stages | 1-second timestamps. `task.updated` has no payload. No blocked, unblocked or done events, so I1, I2, I3 and I5 apply. No business field. No description history (I7). |
| 2 | `multi-business/out/agents/<id>/spec.json` | `created` (`src/agents/mod.rs:96`), `deadline_secs`, `budget_usd`, `role`, `title` | run queue wait (`started − created`) | No task id. `title` truncated. |
| 3 | `multi-business/out/agents/<id>/state.json` | `started` (`src/agents/mod.rs:164`), `ended`, `status`, `cost_usd`, `tool_calls`, `shimmed`, `error`, `verdict` | run wall time, `lost` runs, whether the run had the build shim | `tokens` null throughout. `cost_usd` null for timed-out and lost runs. |
| 4 | `multi-business/out/agents/ledger.jsonl` | `judged` | K6, quality guard, R0c | 19 rows: fields include `status`, `outcome`, `cost_usd`, `secs`, `repo`, `role`. `secs` is run wall time and includes any build-slot wait inside the run (for example `c125` ran with `shimmed: ["cargo"]`). Four rows have no run directory (pruned); run directory `20261113-132343-c125` (`lost`) has no row. Absent in `acme`. |
| 5 | `multi-business/out/agents/<id>/events.jsonl` | `timestamp` on some lines (93 of 317 lines in `20261113-225320-f142`) | stall detection (gaps between timestamped lines) | Not every line is timestamped. |
| 6 | `multi-business/out/agents/<id>/harness.stderr.log` | none | stall and error context | Often empty (0 bytes in `c125`; 82 bytes in `f142`, one MCP policy warning). Shim wait messages land here with no timestamp. |
| 7 | `~/.claude/projects/<project-dir>/<session>/subagents/workflows/wf_*/journal.jsonl` | `launched`, `started`, `result` | fan-out per workflow | No timestamps and no status. |
| 8 | `gh pr list --repo decebal/chief-of-staff-v2 --state all --json number,state,createdAt,mergedAt,headRefName` | `createdAt`, `mergedAt` | S7, E-post | No first-review time, no ready-for-review time. No task id; join by branch (I6). |
| 9 | `gh pr view <n> --json statusCheckRollup,commits` | check `startedAt`, `completedAt` | CI part of S7 | Snapshot only. |
| 10 | `git -C <worktree> log --format=%cI` | commit time | rework indicator | None. |
| 11 | `~/.config/cos/businesses.toml` | none | business dimension | Lists `acme` and `default` only; the epic names four businesses (founding-bets, longhand, realestate, kronos). |
| 12 | `~/biz/acme/out/` | file mtimes; `runs/<day>.json` `at` | acme brief and history only | No `out/agents`; `out/runs` holds brief-job records, excluded from the run population. |

## Instrumentation required before the rules can be trusted (draft stage 2; first slice in the brief)

- **Build slot (S4, machine-level).** Today the shim prints only the wait start, untimestamped, to stderr (`exec_holding_build_slot`, `src/agents/slots.rs:95`, message at `:100`). It does not record when the wait ends or when the lock is released, and it cannot: it replaces itself with cargo (`Command::exec`, `:114`), and the lock is released when cargo and its compilers exit. Its arguments are only `(lock, program, args)` (`:95`), with no home, business or task. Required: the spawner sets `COS_BUSINESS` and `COS_TASK` in the run environment; the shim runs cargo as a child it waits on and reaps, and appends `wait_start`, `acquired` and `released` records with business and task to one machine-level file (open question 2). This gives wait intervals per business and busy intervals per machine for K3 and R3. Route to measured S4 for interactive work: a `cargo` wrapper on the Claude Code session's `PATH` that goes through the same shim, with `COS_TASK` set from the session's current claim, so those compiles stop being unshimmed.
- **CPU sampler (S4, unshimmed compiles).** The shim sees only cos agent runs. The epic's tasks were all claimed by one Claude Code session (`claimed_by: claude:b6e88a80`), whose compiles never go through the shim, and the 7-of-45 contention came from another session's rustc. Required: a per-minute sample of load average over cores, the `rustc` process count, and whether the build lock is held, read from the kernel without spawning a process (the approach of `src/signals.rs`). A `rustc` count above zero while the lock is free marks an unshimmed compile. **Owner and liveness:** a launchd job `cos.sampler`, installed by `task schedule` beside `cos.dash`, with `StartInterval` 60: each run takes one sample, appends one line to the machine-level file and exits, so launchd owns and reaps it and no long-lived process exists. Its liveness is the sample itself: R0 requires records for 95% of a window's minutes, and `cos constraint` prints `sampler: last record <age>` on every run. Claimed time overlapping an unshimmed minute is inferred (I8); the sampler shows contention, it does not make S4 measured.
- **Owner gates (S1, S6), with a named emitter.** Producer: the session that writes an owner request. At the moment it appends a block with a phrase from the owner-phrase list, it also runs `cn block <id> --reason owner:decision|owner:acceptance`, and runs `cn unblock <id>` when the answer or the person check is recorded. The ritual and verification instructions that produce those blocks today gain these calls; cos itself emits nothing. Written to: the cn event store, as new typed events `blocked` and `unblocked` with `reason` and task id (business comes from the task's tag). Heartbeat: R0's conformance check, since a missing emit cannot otherwise be told from no gate: every phrase-matching block without a typed event within 60 s is an `unknown`. This retires I2 and I3. Fallback if the cn change is declined (open question 3): `cos gate open|close <task> --reason <r>` appending to `<home>/out/constraints/gates.jsonl`, same conformance check.
- **cn done.** A typed `done` event. Existing events stay as they are. This retires I1.
- **Run to task.** The spawner writes the cn task id into `spec.json`, so run-slot waits join task stages.
- **Workflow journal.** Timestamps on `started` and `result`.

Until these exist, R0 returns `unknown: S4 unrecorded; owner gates unrecorded` for every unit with claimed work.

## Scope: what is measured

Three populations, each a source of waiting:

1. **Tasks in cn**, attributed to a business. Each runs from created to done; the part the rules weigh is defined under "Weighed time".
2. **Agent runs** under each business home's `out/agents`. These cover queue (status `queued`, waiting for a run slot, `src/agents/mod.rs:118`), run wall time, budget-cap failures and other non-accepted outcomes. Runs cannot be joined to cn tasks today: `spec.json` carries `id, harness, program, model, repo, base, workdir, worktree, title, source, prompt, deadline_secs, budget_usd, created, slots, contract, limits, role` and no task id. Until the spawner writes one, run KPIs stand alone per business and do not enter task stage shares.
3. **Owner gates**: decision, approval and person-check requests waiting on the owner, on tasks and on epics.

### Shared resources

Two things are shared across businesses, and both are reported twice: per business (waits, attributed to the waiting task's business) and machine-wide (utilization).

- **The owner.** K5 is per business and cross-business.
- **The build slot.** It is one lock per user, shared by every cos home (`build_lock`, `src/agents/slots.rs:81-82`, `temp_dir()/cos-build.lock`). Its busy intervals, K3 and R3 are computed once per machine. Compiles that bypass the shim (the owner's terminal, Claude Code sessions, any non-shim cargo) also take CPU and are not seen by the lock; see Instrumentation.

### Business attribution

The unit is the business, as registered in `~/.config/cos/businesses.toml` (today: `acme` with home `~/biz/acme`, and `default` with home `~/Projects/chief-of-staff-v2-wt/multi-business`).

- **Agent runs** belong to the business whose registry `home` contains the run's `out/agents` directory. The ledger's `repo` field (`multi-business`, `saved-ledger-free-monetization`, `lean-model-context`, `business-dashboards`) names a repository, not a business, and is never used for attribution. Because the `default` home is itself a cos worktree, its runs mix cos's own build with other repositories; the card says so.
- **cn tasks** belong to the business named by a business tag (open question 1). Until tags exist, the backfill accepts an explicit epic map in which each entry declares a kind: `--epic t-xxxx=<business>` for business delivery, or `--epic t-xxxx=tool-build` for work on cos itself. A task inherits its epic's entry. The map is recorded as the attribution source. A task with neither is `unattributed`, never guessed.
- **Tool-build work is never a business reading.** The only two epics in cn, `t-67a718` ("Business sidebar and per-business dashboards", 45 children) and `t-0c18d3` ("Multiple businesses in one cos", 16 children), are both builds of cos itself. They are reported under a separate `tool-build` unit and are excluded from every registered business's constraint output. Today no business-owned epic exists in cn, so every registered business reads `unknown: no attributed business tasks`.
- **Both homes surveyed.** `default` has `out/agents` with 16 run directories, a `.slots` directory and `out/agents/ledger.jsonl` (19 rows). `acme` has `out/{audit,briefs,facts,history,logs,reports,runs}` and no `out/agents`. Its `out/runs/2026-11-13.json` is a daily brief-job record (`day, at, status, reason, attempts, cost_usd`), not an agent run; it has no queue or active interval and is excluded from the run population. acme's run KPIs read `null`, not zero.

## Constraint model

### Weighed time

- **Eligibility** is the earlier of: the instant the task's last blocker is done, or the task's first `workflow.claimed`. A task with no blockers is eligible at creation.
- **Owner gates apply from creation.** S1 and S6 conditions are evaluated from the moment a task or epic exists, not only after eligibility, so a gate on a blocked task is never hidden.
- **Owner gates held by blockers (`via`).** Before eligibility, a task is in S1 or S6 with reason `<reason> (via <blocker>)` at every instant when any of its open blockers is itself in S1 or S6 (directly or via its own blockers, transitively). A gate therefore weighs once for its own task and once for each task it holds, which is the "weight for what it blocks" the problem asks for.
- **S0 pre-eligibility** is the rest of created-to-eligibility: blockers open, none of them in an owner gate. It is a planning artifact (tasks created in one batch with blockers still open), is shown, and is excluded from R1.
- **Epic gates** are the waits on an epic itself: `E-pre` from epic creation to the last `workflow.approval.granted` before the first child is created (I4), and `E-post` from the later of the last child done and the joined PR being marked ready, to the epic done. Each instant of an epic gate is classified into S1 (owner decision or approval requested and not answered), S6 (person check pending), S7 (joined PR open for review) or `undetermined` (the timestamps cannot say whether the owner was waiting). Epic-gate time enters R1 with reason suffix `(epic)`, weighted once per epic.
- **Weighed time** for a unit is the sum of: each task's time from eligibility to done (or window end), each task's pre-eligibility time spent in S1 or S6 `via`, and each epic's gate time, including `undetermined`. R1 shares use weighed time, so every second counted belongs to exactly one state, and shares including `undetermined` sum to 1.
- **Lead time** is done minus created. K4 uses lead time, so the owner still sees the full wait.

### Stages and single-precedence state

At every weighed instant, each task or epic gate is in exactly one state: the highest-precedence condition that holds. States are disjoint.

| Precedence | Stage | Condition that holds | Stage type |
| --- | --- | --- | --- |
| 1 | S1 owner-gate | an owner decision or approval is requested on this task (or its epic, for epic gates) and not answered, or an open blocker is in S1 (`via`) | owner |
| 2 | S6 person-check gate | a check only a person can run is recorded as pending on this task or epic (typed `owner:acceptance` request, or I3 text), or an open blocker is in S6 (`via`) | owner |
| 3 | S7 review-wait | a PR joined to this task is open, or the epic's joined PR is open inside `E-post` | resource |
| 4 | S2 dependency-blocked | after eligibility, a blocking task is not done | structural |
| 5 | S4 resource-wait | claimed, and an open wait for a shared resource (build slot, CPU, run slot) | resource |
| 6 | S5 active | claimed, and none of the above | resource |
| 7 | S3 queue-for-claim | not claimed, and none of the above | resource |
| — | `undetermined` | an epic-gate instant whose direction the records cannot settle | none |

Consequences of the precedence:

- An owner gate overrides a claim. A claimed task waiting on an owner decision is in S1, not S5.
- An owner decision is never modelled as a blocking task. If a cn blocker is an owner-decision item, the waiting task is in S1, so S1 and S2 never count the same wait.
- A person-check gate overrides an open blocker. `t-f7769f` is claimed, blocked by open `t-873db6`, and records a person check not run; it is in S6. The blocker's own wait is visible on `t-873db6`.
- S2 arises only when a claimed task still has an open blocker, or a blocker is added after eligibility. S2 is never named as the constraint; R8 (stage 3) follows the blockers. In the thin slice, S2 with the top share reads `unknown: S2 top, R8 not built`.
- S5 can be the constraint. That means the work itself is the limit, not a wait.

### Measurement labels

Every interval and every number in an output carries one label: `measured` (both endpoints read from typed, timestamped events, and the meaning of each endpoint is the event's own type) or `inferred: I<n>` (one or more endpoints, or the stage assignment, derived under a rule below). An interval with any inferred endpoint is inferred. A share, median or count is measured only if every interval it sums is measured; otherwise it is labelled with the rules it used, and the output prints `<x> measured of <y>` beside it.

**Fixed owner-phrase list** (case-insensitive substring match, used by I2 and I3 only; adding a phrase is a brief change, not a code default):

| Phrase | Reason |
| --- | --- |
| `Owner choice needed` | `owner:decision` |
| `needs a design decision` | `owner:decision` |
| `needs a person` | `owner:acceptance` |
| `need a person` | `owner:acceptance` |
| `not run (owner may be typing` | `owner:acceptance` |

- **I1 Done time.** cn has no typed done event. Done is the timestamp of `workflow.step.completed` when present, else the last `task.updated` before the task is seen with `status: done`. Applied literally, never adjusted to a nearby update.
- **I2 Owner-decision interval.** cn has no blocked or unblocked events, and `task.updated` carries no payload. Appended description blocks are mapped to the `task.updated` events after the claim in order, and only when the counts are equal: the number of blocks appended after the claim must equal the number of `task.updated` events after the claim. If they differ, every interval on that task that depends on the mapping is `unknown: block mapping`, not mapped. The interval starts at the update that carried the first block containing an `owner:decision` phrase, and ends at the first `task.updated` after the answer is recorded (a `workflow.approval.granted` on the task or its epic, or a block naming the owner decision).
- **I3 Person-check interval.** Starts at the update that carried a block containing an `owner:acceptance` phrase (same mapping and count check as I2). Ends only at the update carrying a block that records that same check as run (its checklist item marked `[x]`), or at the task's done (I1), or at the window end while open. An unrelated update does not end it. A task done with the check never recorded as run also counts in the quality guard.
- **I4 Approval mapping.** A `workflow.approval.granted` timestamp is measured; which gate it closed is inferred from the epic description's `ritual-*` lines.
- **I5 Gap start from a payloadless update.** When an interval starts at a `task.updated` with no typed partner event, the start is inferred.
- **I6 PR join.** A PR is joined to an epic when its `headRefName` equals the epic's `ritual-branch` line, and to a task only when the task text names the PR or the branch. The PR timestamps are measured; the join is inferred.
- **I7 Size proxy.** The acceptance-criteria count is the number of lines under `## Acceptance criteria`. cn keeps no description history, and descriptions are edited after the fact (`t-3612fa` appends "Criterion 5 replaced"), so the count is never read retroactively. From stage 1, the daily brief job writes each task's count the first time it sees the task and again at done, to `<home>/out/constraints/sizes.jsonl`; verdicts use those snapshots. Labelled inferred.
- **I8 Unshimmed overlap.** Once the CPU sampler exists, any claimed interval that overlaps a sampler minute with an unshimmed compile is inferred for its stage assignment, even when its endpoints are typed: the task may have been waiting on CPU (S4) or working (S5), and nothing says which.

Typed events (`task.created`, `workflow.claimed`, `workflow.approval.granted`, `task.dependency.added`, run `spec.created`, `state.started`, `state.ended`, gh `createdAt`, `mergedAt`) are measured.

### Detection rules

Windows are daily per unit. The headline uses a rolling 7-day window. Thresholds are starting values, not sourced, and stay labelled `untuned` until the backfill has been checked against the worked example. Every output line prints the window count behind it. Rules run in the order listed; the first `unknown` or `insufficient-data` result is printed with every other one that also holds, and no constraint is named.

- **R0 Completeness.** Attribution must be resolved: every task under a mapped epic has a business or `tool-build`. Then each required stage must have a **heartbeat inside the window** from its source:
  - S3 and S5: at least one cn event in the window on a task or epic attributed to this unit (its mapped epics, or its tag once tags exist). A store-wide event does not count, so a unit whose tasks go silent fails.
  - S4: on any window where a task of this unit was claimed, a CPU-sampler record covering at least 95% of that window's minutes. Shim records are required for each acquire; a build slot that is idle all day is shown by the sampler's lock-held field.
  - S1 and S6: an owner-gate source heartbeat (see Instrumentation), and no conformance miss in the window: a description block in the unit matching the owner-phrase list with no typed owner-gate event within 60 s of its update is a miss, and gives `unknown: owner gate untyped on <task>`. None of this exists today.
  - S7: required only for tasks or epics with a joined PR.
  Zero wait is valid only with a heartbeat present. A source that stops writing reads as unrecorded, not as zero. Any required stage without a heartbeat gives `unknown: <stage> unrecorded` (for S1 and S6, `unknown: owner gates unrecorded`), names no constraint, and lists the missing source. Measured and inferred waits are still shown as evidence.
- **R0b Measured coverage.** A stage can be named only if its share computed from measured intervals alone is at least the entry threshold (`0.30`). Inferred time is shown as evidence and never counts toward naming. Otherwise: `unknown: <stage> share is inferred (<x> measured of <y>)`. If any window in the headline has an unshimmed compile overlapping claimed time (I8), the output is also `unshimmed compile observed in <n> windows`.
- **R0c Window count.** A daily window **has data** for a unit when it contains at least one cn event on a task or epic attributed to that unit, or one ledger row or run `state.json` timestamp in that unit's home. An interval that merely spans the day does not make it a data window. Until 7 windows with data exist in the headline, the output is `insufficient-data: <n> of 7 windows`. Shares are shown; no constraint is named.
- **R0d Undetermined time.** If `undetermined` share is at least the gap between the top stage's share and the runner-up's, the ranking could flip if that time were resolved, and the output is `unknown: <h> undetermined epic-gate time could change the ranking`.
- **R1 Share.** For each stage `s`, `share(s) = Σ time in s / Σ weighed time`, over the window, finished and unfinished items alike, clipped to the window. A gate that stays open keeps adding to its stage every day it is open. The share over finished tasks only is shown beside it as a diagnostic. A stage is candidate when its share is at least `0.30` and highest. In stage 3, a named constraint keeps its name until another stage beats it by at least `0.05` on 3 consecutive daily windows, or its own share falls below `0.25` on 3 consecutive windows.
- **R1a Diffuse.** If no stage reaches `0.30`, the output is `no constraint: diffuse (top <s> <x>)`, and no suggestions are proposed.
- **R4 Reason by time.** Group S1, S3, S4, S6 and S7 intervals by reason code (`owner:decision`, `owner:acceptance`, `resource:cpu`, `resource:build-slot`, `resource:run-slot`, `review:pr`, `unrecorded`). The top reason is the one with the largest summed wait time, not the most occurrences. S3 reasons are not instrumented today, so S3 time carries `unrecorded`. That every claim in a unit carries one `claimed_by` value is shown as an observation (`single claimant: claude:b6e88a80`), not a reason code, and never ranked: it is an absence of variation, not a measured wait.

**Stage 3 rules** (deferred, each with its own fixture check):

- **R2 Queue growth.** Queue depth for a stage is the count of tasks in it at each hour, daily max kept. In the backfill, depth is reconstructed from event timestamps; a live sampler replaces it after instrumentation. A stage is growing when its daily max rises on 3 consecutive days. R2 confirms R1; it does not replace it.
- **R3 Resource saturation.** For a resource with recorded busy intervals, `utilization = busy / available` over the window. The build slot is computed machine-wide from shim `acquired` and `released` records. At `≥ 0.9` with a nonzero S3 or S4 queue, the resource is the constraint of that stage. If the CPU sampler saw an unshimmed compile in the window, R3 for the build slot is `unknown: unshimmed compile observed`.
- **R5 Tie.** If two stages are within `0.05` of each other and both pass R0b, both are reported as co-constraints, neither ranked above the other. Suggestions may target either. Starting a trial on one writes `constraint_elevated` for that stage with `co: <other>`, and shift detection runs against that stage.
- **R6 Queue source.** If arrivals into the candidate stage `C` exceed its departures on 3 consecutive daily windows, three separate tests run, in this order, and the first that holds decides:
  1. **Upstream burst.** Arrivals into `C` in the window are more than twice the median daily arrivals of the prior 14 windows, while `C`'s departures are at or above their own prior median. Name the releasing step, and do not declare `C`. Needs 14 prior windows.
  2. **Downstream block.** `C`'s resource is idle (K3 for `C` above `0.25`) while items in `C` cannot leave because the next stage `D` refuses or is at a work-in-progress limit. Name `D`. With no WIP limit configured on `D`, this test does not hold.
  3. **Own capacity.** `C`'s utilization is at least `0.9` (R3), or K3 for `C` is at or below `0.05`. `C` is the constraint.
  A test that cannot be evaluated gives `unknown: R6 <test> needs <input>`. If all three are evaluated and none holds, `unknown: queue source undetermined for <C>`.
- **R7 Spike.** One window above threshold is a spike and does not change the constraint.
- **R8 Follow S2.** When S2 has the top share, compute stage shares over the blockers' own weighed time in the same window, and name their top stage as `<stage> (via S2 blockers)`, subject to R0 to R7. Its stage type is the blockers' stage's type, which sets the allowed KPI set for suggestions. If a blocker is unattributed or outside the unit, `unknown: S2 blockers unattributed`.

### Episodes and shift detection (stage 3)

An **episode** is the interval between two consecutive constraint changes for a unit (`constraint_elevated`, `constraint_shifted` or `constraint_moved`). Every series that depends on which stage is the constraint is split at episode boundaries.

- **Elevated.** Written at **trial start** (not at accept) for a suggestion aimed at the current constraint. The pre-window for that trial is therefore the end of the episode the suggestion was proposed in, and the post-window is the start of the episode the elevation opens. A shift is never assumed.
- **Shifted.** After an elevation, a shift is declared when a different stage beats the old constraint's pre-window share by at least `0.05` on 3 consecutive daily windows, and the old constraint's share has fallen below its pre-window share by at least `0.05` over the same windows. If the old share did not fall, the result is R5 or R6, not a shift.
- **Moved.** If R1 with its hysteresis names a different stage with no elevation event, record `constraint_moved` with `unexplained: true` and show it to the owner.
- **Hysteresis on both.** A shift or move is declared only if the old and new readings pass R0, R0b, R0c and R0d on each of the 3 windows. A window that reads `unknown` or `insufficient-data` resets the count.
- **No flapping.** After any change, the new name holds for at least 3 daily windows before another change can be declared.

## KPIs

Each KPI uses null for unknown. That matches the KPI validator in `src/state.rs:338-356`: `baseline`, `target` and `current` must each be absent, JSON null or a number (`:343-349`, with `absent` treating null as absent, `:89-91`); a KPI is treated as measured when `current` is present, and only then must it carry non-empty `source` and `reportingWindow` and a valid `evidenceAt` timestamp (`:350-356`). The validator says nothing about `baseline` or `target` provenance, so the tracker's own `measured`/`inferred` label is written beside each value. Each KPI has one source and one formula, and carries its label and window count.

**Per stage type.** The drum and starvation KPIs depend on what kind of stage the constraint is.

| KPI | Resource stage (S3, S4, S5, S7) | Owner stage (S1, S6) | Structural (S2), S0, `undetermined` |
| --- | --- | --- | --- |
| K-D drum throughput, per 7 days | departures: S3 claims, S4 grants, S5 tasks done, S7 merges; reported per task and per acceptance criterion (I7) | gates answered | n/a (S2 is never the constraint; R8 names the blockers' stage, whose type applies) |
| K3 starved share | hours the resource has free capacity and zero items in the stage while at least one item waits upstream for it, over window hours. Capacity: S3 and S5 for cos agent runs use run slots (`run_slots`, `src/agents/slots.rs:23`); S4 uses the machine-wide build slot (`build_lock`, `src/agents/slots.rs:81`); work done in interactive sessions has no observed capacity (n/a); S7 is n/a until reviewer capacity is known | n/a (owner availability is not observed) | n/a |
| K5 owner-gate age | n/a | age in hours of the oldest open owner gate, and the count of open gates (task and epic, `via` excluded so a gate is counted once), per business and cross-business | n/a |

**Supporting flow KPIs.**

- **K1 Constraint share.** R1's share for the current constraint, as a percentage. Shown on every card; never a verdict basis, because it can fall by relabelling.
- **K2 Queue ahead of constraint.** Count of tasks queued upstream of the constraint at 09:00 local each day. Target band, not "never zero": floor is `max(1, ⌈K-D ÷ 7⌉)` (one day of drum departures), ceiling is three times the floor. With K-D at zero or n/a the band is `n/a: no departures`. Below the floor the constraint risks starving; above the ceiling is a prompt to read R6, not a trigger. Band is `untuned`.
- **K4 System lead time.** Median over two sets: lead time of tasks done in the window, and age at window end of tasks open at window end (right-censored). Labelled `lower bound` when any open task's age is above the median, so a change that delays work shows up as a rise instead of dropping out.
- **K6 Waste share.** Cost of runs whose `outcome` is not `accepted`, divided by total known run cost, from `<home>/out/agents/ledger.jsonl` (`cost_usd` per row). Ledger statuses are `done`, `failed`, `killed` and `timed_out`:
  - `failed`: the harness ended with an error (every failed row this week is the $5 budget cap, `error_max_budget_usd`);
  - `killed`: stopped by a kill signal before finishing; who sent it is not recorded;
  - `timed_out`: stopped at its `deadline_secs`;
  - `done` with `outcome: rejected`: finished, rejected by the judge.
  A `lost` status (supervisor died without an outcome, `src/agents/mod.rs:127`) exists only in `state.json` and never reaches the ledger, so K6 also counts run directories with no ledger row. K6 is a lower bound while `cost_usd` is null for `killed`, `timed_out` and `lost` runs.
- **System throughput.** Tasks done per 7 days across the unit, regardless of stage, per task and per acceptance criterion. Used as a guard on verdicts.
- **Quality guard.** Three counts per window: owner gates answered and then re-requested on the same task or epic; tasks marked done with unchecked acceptance criteria or an I3 check never recorded as run; ledger rows with `outcome: rejected`. Used as a guard on verdicts.

Little's law and flow efficiency (`S5 time / weighed time`) stay as diagnostics inside the rules, not as dashboard KPIs.

## Improvement-suggestion loop (stage 4)

1. **Propose.** Once a day, cos reads the current constraint and its top reason, and proposes at most three suggestions aimed at that stage (or at either co-constraint under R5). Each records the stage, the reason it targets, the **declared KPI** and its direction, the change proposed, and the observation that would show it did not work. The declared KPI must come from the set for the constraint's stage type: K-D or K3 for a resource stage; K-D or K5 for an owner stage. K1 may be shown beside it but is never declared. No suggestions are proposed while the output is `unknown`, `insufficient-data` or diffuse.
2. **Store.** Suggestions go to `<home>/out/constraints/suggestions.jsonl`, append-only. This is a new file, not a registry field, so the registry writer cannot strip it (see the C1 risk in the dashboards brief).
3. **Lifecycle.** `proposed`, then `accepted` or `rejected` (`cos constraint accept|reject <n>`). Accepting creates a cn task for the change (open question 5). `accepted` becomes `in-trial` on the first of exactly two triggers: the owner runs `cos constraint start <n>`, or that cn task records `workflow.claimed`. The trial start is that timestamp, labelled measured, and the AC-count snapshot (I7) and every pre-window value are written to the suggestion's row at that moment. `in-trial` becomes `measured` when the post window closes or its episode ends, and `retired` after a `no-movement` verdict or an owner reject.
4. **Elevate.** At trial start, if the suggestion is aimed at the current constraint, write `constraint_elevated`. If the constraint changed between accept and start, the trial still runs, no elevation is written, and the verdict carries `constraint-changed-before-start`. A suggestion aimed elsewhere writes nothing about the constraint.
5. **Measure.** The verdict is computed on the suggestion's declared KPI.
   - **Windows.** Pre: up to 7 daily windows ending at the trial start, inside the episode the suggestion was proposed in (cut at that episode's start). Post: up to 7 daily windows from the trial start, inside the episode the elevation opened (cut at its end). The accept-to-start gap is in neither.
   - **Minimum sample.** At least 5 daily windows on each side, and for count KPIs at least 30 events (departures, gates, tasks) on each side. Otherwise the verdict is `insufficient-data: <n> windows, <m> events`.
   - **Interval.** Count KPIs: a 90% exact conditional binomial interval on the post/pre rate ratio, with exposure in window-days. Duration and share KPIs (K3, K5, K4, stage shares): a 90% bootstrap interval on the difference of daily medians, 2,000 resamples with a fixed seed. "Beyond its band" means the interval excludes no-change. The interval is printed in the verdict. Both stay `untuned`.
   - **Size.** Count KPIs are compared per acceptance criterion as well as per task, using the I7 snapshots stored at trial start and at verdict. If the two disagree in direction, the verdict uses the per-criterion value and shows both, so splitting work into more tasks does not count as movement.
   - **Guards.** Over the same windows, each with its own interval: K4 must not rise; system throughput (per criterion) must not fall; no quality-guard count may rise; and **displacement**: the next-largest stage's share, S0 pre-eligibility time, epic-gate time and `undetermined` time must not rise. A change that moves wait to another stage, before eligibility, or onto the epic breaks this guard.
   - **Verdicts.** `moved`: the declared KPI's interval lies wholly in the declared direction, and every guard holds. `regression`: the declared KPI's interval lies wholly in the wrong direction, or a guard broke. `no-movement`: neither. `insufficient-data`: sample too small. A trial whose post window was cut by a shift or move also carries `episode-ended`.
   - The verdict is one before/after comparison with no control. It does not show cause, and the card says so.
6. **Retire.** A `no-movement` suggestion is retired, and cos does not re-propose it unchanged within the same episode.

cos never applies a change to a repo from this loop. The owner decides every acceptance.

## Where it surfaces

- **CLI.** Slice 1: `cos constraint` (current) and `cos constraint backfill`. Later: `history`, `suggest`, `accept|reject <n>`, `start <n>`, `verdict`. All take `--business <name>`; `--business tool-build` reads the cos-build unit.
- **Brief.** Slice 1: a "Limiting factor" section after Steering. Values come from the rules, with their labels and window counts. The LLM may comment on them but does not set them.
- **Today.** A card per business: constraint stage or `unknown: <reason>`, K1, K-D with its trend, and the top suggestion with its status. The existing LLM bottleneck `<details>` stays, with one line under it showing the measured constraint, so the owner sees whether prose and measurement agree.
- **Sidebar.** One small label per business. Today the dashboard stamp watches only `out/briefs`, `out/facts` and `out/history` (`WATCHED`, `src/dash.rs:18`, stamped by `stamp`, `src/dash.rs:181`). Watching `out/agents`, `out/constraints` and the cn store is new work. The refresh path does not spawn.
- **Dashboard.** A Flow page with the K-D series split at episodes, daily stage-share bars, the waits list by reason, and the suggestion log with verdicts.

**Reuse candidates** (verified by grep in the business-dashboards worktree): scorecard pattern in `src/agents/ledger.rs` (`Score` 107, `scorecard` 123, `RoleScore` 167, `by_role` 181); day series in `src/history.rs` (`DayRow` 15; `Merged` 32 is the PR merge log, filled by `merge_prs` 203); dependency layering in `src/flows.rs:549` (`layers`); KPI validation in `src/state.rs:338-356`; live signals in `src/signals.rs:19` (`Signals`) and `src/signals.rs:49` (`stamp`); launchd job rendering for `cos.dash` in `Taskfile.yml` (`task schedule`), the pattern for the sampler job.

## Worked example: the epic, snapshot 2026-11-15 21:32:57 WET

Epic `t-67a718` is cos's own build, so it is mapped `--epic t-67a718=tool-build` and read under the `tool-build` unit. It checks the mechanics; it is not a business reading. Sibling epic `t-0c18d3` is also tool-build. At the snapshot, cn showed 31 done, 3 in progress and 11 open, summing to 45. The two task tables are read from current cn, since both tasks finished after the snapshot.

**One task decomposed under single precedence.** `t-3612fa` (done 21:34:44). Its timeline after the claim has four `task.updated` events (12:39:04, 12:42:45, 21:31:07, 21:34:40) and its description has four appended blocks after the claim, so the I2 count check passes (4 = 4). Mapped in order: the first verification round ending "needs a design decision", the re-verification ending "Owner choice needed", the "owner decision 2026-11-15, design re-approved" block, and verification round 3.

| Interval | Stage | Duration | Label |
| --- | --- | --- | --- |
| 09:31:50 created to 12:27:59 blocker `t-192a4b` `workflow.step.completed` | S0 | 2h56m09s | inferred (I1) |
| 12:27:59 to 12:31:35 `workflow.claimed` | S3 `unrecorded` | 3m36s | inferred (I1) |
| 12:31:35 to 12:39:04 | S5 | 7m29s | inferred (I2, I5) |
| 12:39:04 to 21:31:07 | S1 `owner:decision` | 8h52m03s | inferred (I2, I4) |
| 21:31:07 to 21:34:44 `workflow.step.completed` | S5 | 3m37s | inferred (I2, I1) |

- The S0 end follows I1 literally: `t-192a4b` has `task.updated` at 12:27:55 and `workflow.step.completed` at 12:27:59, and I1 takes the latter. `t-192a4b` was never in an owner gate, so none of the S0 time is `via`.
- The S1 answer is anchored to a measured event: the epic records `workflow.approval.granted` at 21:30:51 alongside a second `ritual-approved` line for the design (sha256 `7088aeb1…`). Which gate that closed is I4; the S1 end is the next task update, 21:31:07.
- The 12:42:45 update falls inside S1. It carried the re-verification block restating the same request, so it does not change stage. The 21:34:40 update falls inside the last S5.
- Lead time 12h02m54s, the sum of all five intervals. Weighed time (eligibility at 12:27:59 to done) 9h06m45s, the sum of the last four. Shares of weighed time: S1 97.3%, S5 2.0%, S3 0.7%. Measured share: 0 of 9h06m45s. The two S5 intervals may contain unrecorded S4 time.

**A dependent held by that gate (`via`).** `t-600b1d` (blocked by `t-3612fa` only; done 22:02:04).

| Interval | Stage | Duration | Label |
| --- | --- | --- | --- |
| 09:31:51 created to 12:39:04 | S0 | 3h07m13s | inferred (I2) |
| 12:39:04 to 21:31:07 | S1 `owner:decision (via t-3612fa)` | 8h52m03s | inferred (I2, I4) |
| 21:31:07 to 21:34:44 blocker done | S0 | 3m37s | inferred (I2, I1) |
| 21:34:44 to 21:36:51 `workflow.claimed` | S3 `unrecorded` | 2m07s | inferred (I1) |
| 21:36:51 to 22:02:04 `workflow.step.completed` | S5 | 25m13s | inferred (I1) |

Lead time 12h30m13s; weighed time 9h19m23s, of which S1 `via` is 95.1%. Without the `via` rule the same task would show 27m20s of weighed time and no owner wait at all. `t-25ee51` (created 09:31:52, blocked by `t-3612fa` only, still open) carries the same 8h52m03s of S1 `via`. Tasks blocked by those two (`t-40aae6`, `t-bf2525`, `t-efdf2c`, `t-70c532`) are `via` transitively; the backfill computes their totals and they are not hand-counted here.

**Other waits.**

- **S3, fully measured.** `t-f54b25` has no blockers, so it was eligible at creation; created 09:31:46 to claimed 09:47:08 is 15m22s, both endpoints typed.
- **Pre-eligibility.** All 45 children were created in one batch, the earliest at 09:31:46 (`t-f54b25`, `t-ce4e1f`, `t-690e48`, `t-22c435`) and the last dependency link at 09:32:08 (`t-f54f0b`), all measured. Created-to-claimed spans such as `t-f7769f` 5h54m57s and `t-192a4b` 2h47m13s are mostly S0, not S3.
- **S6 person-check, open at the snapshot.** `t-873db6`: from its 15:23:09 update, 6h09m48s. `t-f7769f`: from its 15:30:06 update, 6h02m51s; it is also blocked by open `t-873db6`, and precedence puts it in S6. Their blocks match `not run (owner may be typing` (I3), and neither has a later block recording the check run, so both stay open. Under R1 these open waits count toward S6 in the window; a finished-tasks-only share would miss them. `t-f54f0b`, blocked by `t-f7769f`, is S6 `via` from 15:30:06.
- **Claimants.** Every claim carries `claimed_by: claude:b6e88a80`; shown as an observation, not a reason code.
- **S4.** Never instrumented, and the session's compiles bypass the shim in any case.

**Epic-level waits (in weighed time, weighted once per epic).**

- **E-pre, `t-67a718`.** Epic created 2026-11-13 20:12:51; `task.updated` 22:52:45 (I5); `workflow.approval.granted` 2026-11-15 09:06:45 (measured); first child created 09:31:46. The first `ritual-approved` line ("by Decebal Dobrica without review") reads as the owner's answer, not a request; if it was written at 22:52:45, the 34h14m00s that follows is recording lag. No owner phrase or typed gate marks a request, so created to approval, 36h53m54s, is `undetermined` (I4, I5). E-pre ends at that approval; the 25m01s to the first child is the session writing children and is outside every gate.
- **E-pre, `t-0c18d3`.** Created 2026-11-12 15:56:23 to `workflow.approval.granted` 19:21:13, 3h24m50s, `undetermined` for the same reason (I4).
- **E-post, `t-0c18d3`.** Every child is done. The epic's last block records acceptance items 4, 5 and 6 (app half) as "need a person at the built app" (I3); the last event is the 16:34:41 update, taken as the I3 start (I5). S6 `owner:acceptance (epic)`, open 52h58m16s at the snapshot.
- **PR #27, `t-67a718`.** `headRefName: feat/business-dashboards` matches the epic's `ritual-branch` line (I6). It was created 2026-11-13 20:13:58, before any child existed, and is the epic's working PR. E-post has not started (11 children open), so its open time is context, not S7. The children say "No PR for this bead alone", so no task has S7. Merged PRs #17 to #25 were created 2026-11-05 to 2026-11-13 01:10, before this epic existed, and are not evidence for it.

**Agent runs and waste (`default`).** Ledger, 19 rows: 10 `done` (9 accepted, 1 rejected), 5 `failed` (all `error_max_budget_usd`, $5 cap), 3 `killed`, 1 `timed_out` (240 s deadline). Accepted run wall time from `secs` (measured, includes any build-slot wait): median 385 s (6m25s), min 162 s (2m42s), max 489 s (8m09s). Known cost $59.22: accepted $30.76, failed $25.49, done-rejected $2.97. K6 = $28.46 / $59.22 = 48%, lower bound, since the 3 killed and 1 timed-out rows have null cost. The run directory `20261113-132343-c125` is `lost` in `state.json` with null cost and no ledger row; it adds nothing to the known cost but counts as one more non-accepted run. These runs are not joined to any cn task. The rows fall on 2026-11-12, 2026-11-13 and 2026-11-15, so K6 prints `3 of 7 windows`.

**What the rules output.** These four lines are the thin slice's acceptance output.

- **Pass 1 (the CPU wait), `tool-build`, a resource stage with no events.** Tasks were claimed on 2026-11-15, and there is no shim record (the workers were Claude Code subagents, not cos runs) and no CPU sampler record. Without R0, the CPU wait would fall into S5 by precedence and R1 would read "the work is the limit". With R0: `unknown: S4 unrecorded; owner gates unrecorded; insufficient-data: 3 of 7 windows`. No constraint is named, and no S5 reading is offered. Missing sources listed: CPU sampler, build-slot shim records, owner-gate events. The pass counts (7 of 45, then 31) stay unverified.
- **Today, `tool-build`.** `unknown: S4 unrecorded; owner gates unrecorded; 40h18m44s undetermined epic-gate time could change the ranking; insufficient-data: 3 of 7 windows`. The 40h18m44s is the two E-pre intervals (36h53m54s and 3h24m50s), both inside the 7-day window. Even with every source present, R0b would block a name: no interval is measured. The card shows the evidence ranked by summed time, each with its label: E-post S6 `owner:acceptance (epic)` 52h58m16s (inferred), undetermined E-pre 40h18m44s, S1 `owner:decision` 8h52m03s on `t-3612fa` plus the same time `via` on `t-600b1d` and `t-25ee51` (inferred), S6 open waits 6h09m48s and 6h02m51s (inferred), S3 `unrecorded`, and S0 pre-eligibility shown separately.
- **After instrumentation (stage 2 acceptance).** For a pass like pass 1, with shim, sampler and owner-gate records present and the compiles still unshimmed: `unknown: S4 share is inferred (0 measured of <y>); unshimmed compile observed in <n> windows`. Naming S4 from measured time needs those compiles to go through the shim (the `PATH` wrapper above); until then the honest reading is this `unknown`, with the sampler's contention minutes as evidence.
- **`default` and `acme`.** `unknown: no attributed business tasks`. `default` also shows its run KPIs (K6 48%, lower bound, `3 of 7 windows`); acme's read `null`.

**Design rounds.** Commit `e0f438f` shows major findings of 35, 32, 27 and 15 across rounds, then the round cap. This is a rework signal, not yet a KPI.

## Draft staging (superseded by the brief)

Kept for the rule-to-stage assignment and the per-stage checks. The brief folds the draft's stage 2 instrumentation into stage 1 and renumbers the rest; stage numbers elsewhere in this file refer to this draft list.

The epic lands in stages. Each stage is usable on its own and has its own check.

1. **Thin slice (bet first).** `cos constraint` (current only, `--business <name>`), a "Limiting factor" section in the daily brief, and `cos constraint backfill`, which rebuilds stage intervals from existing cn timelines and `out/agents` data. Rules in this slice: R0, R0b, R0c, R0d, R1, R1a, R4, and the evidence list. Every interval carries a `measured` or `inferred: I<n>` label. Output includes every `unknown` and `insufficient-data` case. No new events, no UI beyond the brief, no history. On today's data the slice names no constraint for any unit; its value is the evidence list and the list of missing inputs. **Check:** the backfill reproduces the `t-3612fa` and `t-600b1d` tables below to the second, and prints the four output lines under "What the rules output" verbatim, window counts included.
2. **Instrumentation.** Build-slot records from the shim, a machine CPU sampler, owner-gate events with a named emitter, task id on agent runs, typed cn done. This is what turns `unknown: S4 unrecorded` and `unknown: owner gates unrecorded` into readings. **Check:** one full build pass with shim and sampler records, and one owner gate opened and answered through the emitter, read back by `cos constraint` with R0 passing for that window.
3. **History, shifts and the deferred rules.** `cos constraint history`, episodes, shift and moved detection with hysteresis, and R2, R3, R5, R6, R7, R8. None of these can fire on today's data (no build-slot records, no queue history, no S2 blockers after eligibility, one episode), so they are deferred rather than shipped untested. **Check:** a fixture set of synthetic timelines, one per rule outcome (each R6 step holding, R5 tie, R7 spike, R8 via blockers, a shift, a move, a reset by an `unknown` window), each with its expected output written before the code.
4. **Suggestion loop.** `suggest`, `accept|reject`, `start`, `verdict`. **Check:** fixture trials for each verdict, including a displacement-guard break and an interval that straddles no-change.
5. **Surfaces.** Today card, sidebar label, Flow page.

## Open findings Spec must resolve

From the last critic round on the Think draft: 2 blocking, 10 major, 3 minor. Line references (`L<n>`) point into `constraint-brief.md`; the sections above carry that text unchanged. Facts marked verified were checked by the critic against cn, gh or the ledger.

### Blocking

**B1. PR join puts every epic child in S7.** (Draft L121 I6, L87 S7 condition, L297.)
Finding: I6 joins a PR to a task when the task text names the branch. All 45 children of `t-67a718` say "branch feat/business-dashboards" (verified in `cn show t-3612fa`, `t-600b1d`, `t-f54f0b`). PR #27 is open, `headRefName: feat/business-dashboards`, created 2026-11-13T20:13:58Z (gh verified). Applied literally, every claimed or open child is in S7 (precedence 3, above S2, S4 and S5) while PR #27 is open, and the unit reads review-wait. L297 says "no task has S7", which contradicts the rule.
Fix: join a PR to a task only on an explicit PR number or URL, or on a branch the task owns; never on the epic's shared branch. Fixture: two tasks naming the shared branch with the PR open; expected output, no S7.

**B2. Tool-build runs reported as business output.** (Draft L62, L64, L299, L306.)
Finding: the draft says tool-build work is never a business reading and the ledger `repo` is never used for attribution, yet reports `default`'s K6 48% and run KPIs as business output. In `multi-business/out/agents/ledger.jsonl`, 15 of 19 rows have role `ritual-goldfish` (reviews of cos's own design documents), including failed $5.06 row `20261115-084512-9084` and done row `20261113-225320-f142` (`spec.json` title: "Goldfish review of docs/plans/2026-11-13-business-dashboards-design.md"). The `default` home is cos's own worktree. Only one non-goldfish cost row exists ($2.97, `20261112-194827-21cf`).
Fix: attribute runs to `tool-build` by role or epic link, or by a run-level kind written by the spawner; recompute K6 on business runs only; print the run count beside K6. Never state K6 48% as `default` business delivery. (The brief adopts this.)

### Major

**M1. Owner-gate classification by phrase match.** (Draft L118 I3, L134 R0 conformance, L288, L90.)
Finding: S6 is triggered by phrase match only (`not run (owner may be typing`). The `t-873db6` and `t-f7769f` blocks are the agent's own declined check ("task desktop-app: NOT run (owner may be typing; ... installs over ~/Applications app)"), not an owner request, and owner availability is stated as unobserved (L178). R0 conformance fires only on phrase matches, so a deferral worded outside the list gets no owner event and no miss, and its wait falls to S5 active, where the reading becomes "the work is the limit". Gameable by wording.
Fix: a separate reason code for agent-deferred checks, outside the owner stage; require a typed `blocked` event from the named emitter before any owner stage can be named; extend the conformance check to flag any `NOT run`, `skipped` or `deferred` text on claimed tasks, not only the phrase list.

**M2. Owner-stage K-D is gameable.** (Draft L175, L191.)
Finding: owner-stage K-D is "gates answered". Opening more gates raises departures, and the quality guard counts only re-requests of the same gate, so splitting or spawning gates reads as movement.
Fix: normalise owner K-D per task or per acceptance criterion, as for resource stages, and add a guard on gates opened per window. (The brief adopts per-task normalisation and the gates-opened guard; Spec fixes the formula.)

**M3. Displacement guard covers one stage only.** (Draft L206, L48, L207.)
Finding: the guard checks only "the next-largest stage's share", so a move from S4 into a third stage (for example S5 under an unshimmed compile, I8) is not checked. Run-queue wait (agent runs, not joined to tasks) is in no stage share, so moving wait into run slots passes. K6 is not in the guard list.
Fix: guard every other stage's share, plus run-queue wait and K6 cost; an unrecorded stage is a guard failure, not a pass.

**M4. Sampler coverage and machine sleep.** (Draft L245, L133.)
Finding: the sampler is a launchd job with `StartInterval` 60 and R0 requires records for 95% of a window's minutes. launchd does not fire `StartInterval` jobs while the Mac sleeps (missed runs coalesce on wake; verify on this machine), so any day with more than about 1.2 h asleep reads `unknown`.
Fix: decide before the slice-1 check whether sleep minutes leave the denominator (the sampler can record its own gap on wake), or sample from a long-lived process, which contradicts the draft's no-long-lived-process design.

**M5. Verdict statistics on 5 windows.** (Draft L201-L204.)
Finding: minimum 5 daily windows per side, bootstrap of the difference of daily medians, 2,000 resamples, 90% interval. Five values per side give about 126 distinct resample multisets; the interval is unstable and the days are autocorrelated. Up to 3 suggestions a day, each at 90%, with no multiplicity control, so the spurious `moved` rate is unbounded.
Fix: a rank test, or an exact binomial on counts, for every KPI; a minimum window count the chosen test can support; a stated false-positive rate per episode; keep the `untuned` label.

**M6. Stage 1 usable on its own.** (Draft L35-L37, L304, L306.)
Finding: on today's data stage 1 names no constraint for any unit; its only output is the evidence list, so "usable on its own" is not met.
Fix: state the user and the decision stage 1 supports, or fold the cheapest instrumentation into stage 1 so one unit can be named. (The brief does both: owner gates by age for the owner's daily answer-first decision, and the instrumentation in slice 1. Spec writes the acceptance strings.)

**M7. Expected output omits a result that holds.** (Draft L304, L137.)
Finding: R0b says a stage can be named only from measured share, and the rules print every result that holds alongside the first `unknown`. For tool-build, S1 is top (97% on `t-3612fa`), 0 measured of 9h06m45s, so `unknown: S1 share is inferred (0 measured of 9h06m45s)` holds but is missing from the "verbatim" line. The stage-1 check is only as good as that string.
Fix: add the line to the expected output (using the unit-level total, not one task's), or amend R0b to print only the first result.

**M8. Worked-example precedence errors.** (Draft L288, L282.)
Finding: (a) `t-f7769f`: its open blocker `t-873db6` is in S6 from 15:23:09; precedence 2 and pre-eligibility `via` put `t-f7769f` in S6 from 15:23:09, not 15:30:06, so the span is 6h09m48s, not 6h02m51s. (b) `t-f54f0b`: the chain `t-600b1d` → `t-70c532` → `t-f54f0b` keeps it in S1 `via` from 12:39:04 to 21:31:07 (S1 outranks S6), then S6 `via` from 21:31:07 (1m50s at the snapshot); "S6 via from 15:30:06" is wrong. (c) L282 "tasks blocked by those two" omits `t-f54f0b`.
Fix: recompute every worked-example row with the rules before using it as the backfill acceptance; the "to the second" check fails on these lines as written. (The brief's table already uses 6h09m48s for `t-f7769f`.)

**M9. E-pre classified as undetermined despite run activity.** (Draft L294.)
Finding: the draft says nothing in the timestamps settles the direction of E-pre. Measured run records fall inside it: run `20261113-225320-f142`, `spec.json` created 2026-11-13T22:53:20Z (a goldfish review of the design revision, 35 s after the 22:52:45 update), and run `20261115-084512-9084` (business-dashboards goldfish, started 08:45:12, 21 minutes before the 09:06:45 approval). Live activity at 22:53 contradicts the "recording lag" reading.
Fix: use run and session activity as an input to E-pre classification (active work is S5 or S4, not `undetermined`), or at least show it as evidence; do not present the undetermined share as unresolvable.

**M10. No real-data end-to-end acceptance.** (Draft L212, L40, L337.)
Finding: the suggestion-loop check is fixture trials only. "Whole epic to rock" needs a business-tagged epic with R0 passing, which R0 blocks today. So the suggest → start → verdict job is never run on real data in the epic's own acceptance, while the epic could be declared done. The definition of done requires end-to-end.
Fix: an acceptance step that runs the full job on one real unit (a tool-build unit is acceptable) and records the verdict or the named `unknown`; state which real-data gate blocks it. (The brief adds this to stage 3.)

**M11. I7 history in a "no history" stage.** (Draft L37, L122.)
Finding: stage 1 says "no new events, no UI beyond the brief, no history", but I7 has the daily brief job write `sizes.jsonl` from stage 1, a per-task history file that verdicts depend on.
Fix: move the I7 writer to the instrumentation stage, or state it as an explicit stage-1 exception. (The brief lists task-size snapshots in slice 1 explicitly.)

### Minor

**m1. I1 fallback cannot be evaluated.** (Draft L116.) "Last `task.updated` before the task is seen with `status: done`" needs an observation log that no source names, since `task.updated` has no payload. Sampled done tasks (`t-3612fa`, `t-600b1d`, `t-192a4b`, `t-f54b25`, `t-7cc623`) all have `workflow.step.completed`. Fix: name the fallback's source, or read done as `unknown` when `workflow.step.completed` is absent.

**m2. E-post start not computed by its own rule.** (Draft L296.) E-post starts at the later of last child done and PR ready (L75). PR #24 for `t-0c18d3` merged 2026-11-13T16:29:25Z (gh verified, joined by `ritual-branch feat/multi-business`). The draft uses the 16:34:41 update (I3, I5) instead; the difference is 5m16s of S6 against `undetermined`. Fix: compute E-post start by the rule and show the I3 start separately.

**m3. R0c and sporadic units.** (Draft L138.) A window has data only if it contains an event, so a sporadic unit may never reach 7 data windows in the headline and stays `insufficient-data` indefinitely. Fix: define the headline as 7 calendar days or 7 data days, and state the behaviour for gaps.
