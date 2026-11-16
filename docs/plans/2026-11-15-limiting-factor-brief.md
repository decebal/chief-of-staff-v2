# Limiting-factor tracker: brief

**Author:** Decebal Dobrica
**Date:** 2026-11-15
**Mode:** THINK (ritual)
**Format reference:** `docs/plans/2026-11-13-business-dashboards-brief.md`
**Spec input:** [`2026-11-15-limiting-factor-spec-notes.md`](2026-11-15-limiting-factor-spec-notes.md) (detection rules, measurement labels, statistics, instrumentation, data sources, full worked example, and the critic findings Spec must resolve)

## Problem

Delivery slows down and cos cannot say where. The only view of the limiting factor is one free-text `bottleneck` line per goal, written by the LLM (`GoalCheck.bottleneck`, `src/reports.rs:210`). Nothing measures it or stores its history, and it has no age and no weight for what it blocks.

This week's cos dashboards build shows the cost. 45 tasks were planned in one batch (measured, cn). The session reports that its first build pass finished 7 of 45 because another session's rustc held the CPU, and a second pass finished 31 (reported, unverified: nothing recorded the CPU wait). One owner decision sat for 8h52m03s and held two dependent tasks for the same time, found only by reading raw timestamps (inferred). The epic's approval wait, 36h53m54s, cannot be classified from the records (inferred).

The consequence: effort goes to whatever looks busy, and improvements are found by accident. The solution: record the wait at each stage, name the constraint from those records under written rules or say it cannot be named, store that answer over time, and judge each improvement against the metric it promised to move.

## Goal

When the epic is done, the owner opens the app or runs `cos constraint` for any registered business and sees, without reading logs: the constraint stage, its share of weighed time over 7 data windows and its top reason (or `unknown: <reason>` with the missing inputs); how long it has held and why it last shifted; its drum KPI with trend; and the suggestions aimed at it with their verdicts. Every value carries a `measured` or `inferred` label and its window count.

**End-to-end job.** A person reads the constraint and its evidence, accepts one suggestion, starts its trial, and later reads a verdict against the KPI it declared, with not-worse guards shown. When no constraint can be named, the page says why and lists what is missing.

## Scope

Three populations are measured: tasks in cn, agent runs under each business home's `out/agents`, and owner gates (decision, approval and person-check requests on tasks and epics).

The unit is the business registered in `~/.config/cos/businesses.toml` (today `acme` and `default`). Work on cos itself is a separate `tool-build` unit and never a business reading. Both epics in cn today (`t-67a718`, `t-0c18d3`) are tool-build. So are 15 of the 19 agent runs in the `default` ledger: they are `ritual-goldfish` reviews of cos's own design documents (measured). Runs are attributed by role or epic link, not by the home they ran in, because `default`'s home is a cos worktree. The build slot and the owner are shared across businesses and reported both per business and machine-wide.

## Constraint model in outline

At each weighed instant a task or epic gate is in exactly one stage, the highest-precedence one that holds:

1. **S1 owner gate**: an owner decision or approval is requested and unanswered, directly or through a blocker.
2. **S6 person check**: a check only a person can run is pending.
3. **S7 review wait**: a PR joined to this task or epic is open.
4. **S2 dependency-blocked**: claimed, with a blocker still open. Never named; the rules follow the blockers.
5. **S4 resource wait**: claimed and waiting for the build slot, CPU or a run slot.
6. **S5 active**: claimed and none of the above. Naming it means the work itself is the limit.
7. **S3 queue for claim**: eligible and unclaimed.

Time before a task is eligible (S0) and epic-gate time the records cannot classify (`undetermined`) are shown but handled separately.

A required stage with no records in the window gives `unknown: <stage> unrecorded`, never a guess and never zero. A constraint is named only from measured time, only after 7 data windows, and only when no other `unknown` holds. A shift needs persistence: a new stage must lead for 3 consecutive daily windows, and an `unknown` window resets the count. That requires a stored history of daily readings.

## KPIs

- **K-D drum throughput.** Resource stage: departures per task and per acceptance criterion (cn, shim, gh). Owner stage: gates answered per task, with gates opened as a guard (owner-gate events).
- **K3 starved share.** Hours the constraint resource sat idle while work waited upstream (shim, run slots). Resource stages only.
- **K5 owner-gate age.** Oldest open owner gate and the open count, per business and cross-business (owner-gate events).
- **K1 constraint share.** Shown, never a verdict basis (rules).
- **K2 queue ahead of the constraint.** Daily count against a band (cn).
- **K4 system lead time.** Median including open tasks' age (cn).
- **K6 waste share.** Non-accepted run cost over known run cost, business runs only, run count beside it (ledger).
- **Throughput and quality guards.** Tasks done per criterion; re-requested gates, unchecked criteria at done, rejected runs (cn, ledger).

## Improvement loop in outline

1. **Propose.** Daily, at most three suggestions aimed at the current constraint, each with a declared KPI from its stage type's set. None while the output is `unknown`.
2. **Accept or reject.** The owner decides; an accepted suggestion becomes a cn task.
3. **Trial.** Starts at `cos constraint start` or the task's claim; pre-window values are frozen then.
4. **Verdict.** `moved`, `no-movement`, `regression` or `insufficient-data` on the declared KPI, with not-worse guards on lead time, throughput, quality, and displacement into any other stage, S0, epic gates, run-queue wait or K6. It is a before/after comparison with no control. cos never applies a change itself.

## Surfaces

`cos constraint --business <name>` (including `tool-build`), a "Limiting factor" section in the daily brief, a Today card beside the LLM bottleneck text, a sidebar label, and a Flow page.

## First slice and staging

1. **Readout plus the cheapest instrumentation.** `cos constraint`, the brief section, and a backfill over cn and `out/agents`, printing labels and every `unknown`. In the same slice: typed owner-gate open and close events with a named emitter (the session that writes an owner request); shim build-slot records and a per-minute CPU sampler; the cn task id and a run kind in each run's `spec.json`; task-size snapshots. Without these no unit can ever be named.
   **Who uses it, for what.** The owner, daily. Before anything can be named, K5 lists open owner gates by age, so the owner decides which to answer first, and the missing-inputs list shows which source is dark. After 7 instrumented data windows, tool-build is the first unit that can be named.
   **Check.** The backfill reproduces the corrected worked example to the second; one owner gate and one build pass are recorded and read back; on real data, tool-build names a constraint from measured time or prints an `unknown` naming the missing source.
2. **History and shifts.** Stored daily readings, shift detection, the rules that need history. Check: one fixture per rule outcome.
3. **Suggestion loop.** Check: fixture trials per verdict, plus one full suggest-to-verdict run on a real unit.
4. **Surfaces.** Today card, sidebar label, Flow page.

## Worked example: today's real waits

Tool-build unit, snapshot 2026-11-15 21:32:57 WET. This checks the mechanics. It is not a business reading.

| Wait | Stage | Duration | Label |
| --- | --- | --- | --- |
| `t-3612fa` owner decision on the design | S1 | 8h52m03s | inferred |
| `t-600b1d`, `t-25ee51` held by that decision | S1 via blocker | 8h52m03s each | inferred |
| `t-0c18d3` app acceptance items need a person | S6 (epic) | 52h58m16s, open | inferred |
| `t-873db6`, `t-f7769f` desktop-app check not run | S6, classification disputed | 6h09m48s each, open | inferred |
| `t-67a718` created to approval | undetermined | 36h53m54s | inferred |
| `t-0c18d3` created to approval | undetermined | 3h24m50s | inferred |
| `t-f54b25` created to claimed | S3 | 15m22s | measured |

On today's data the rules output `unknown: S4 unrecorded; owner gates unrecorded` and `insufficient-data: 3 of 7 windows` for tool-build, and `unknown: no attributed business tasks` for `acme` and `default`. Of the rows above, only the S3 row is measured.

## Open questions

1. **Business tagging.** cn tasks carry no business field. **Recommended:** tag each task at creation with a business or `tool-build`; until then use an explicit epic map; attribute runs by role or epic link.
2. **Where machine-level records go.** **Recommended:** `$XDG_STATE_HOME/cos/build-slot.jsonl` and a sibling `cpu-sampler.jsonl`, one per machine.
3. **New cn event types.** **Recommended:** add typed `blocked`, `unblocked` and `done` as append-only events; fallback `cos gate open|close` writing `<home>/out/constraints/gates.jsonl`.
4. **Thresholds and windows.** **Recommended:** keep the draft values (share 0.30 enter, 0.25 exit, 3-window persistence, 7 data windows) labelled `untuned`, and decide whether machine sleep leaves the sampler's denominator before the slice-1 check.
5. **Suggestions as tasks.** **Recommended:** yes; the task's claim starts the trial, and the suggestion file keeps snapshots and verdicts.

## Non-goals

- Forecasting completion dates or durations.
- Scoring individuals.
- Auto-applying suggestions or reordering tasks.
- Replacing the LLM bottleneck text; the measured constraint sits beside it.
- Changing cn's storage format or existing events, beyond the new event types in question 3.

## Confidence

**Sand.** The stage model and labels are understood, and the readout reads only existing data. Owner-gate classification and PR joins have open findings (spec notes). S4 is water: no records exist, and the costliest compiles bypass the shim. No business-owned epic exists yet.

What moves it up: the backfill reproducing the corrected worked example; owner gates and S4 recorded for one full pass; question 3 decided; and one unit named from measured time on 7 data windows.
