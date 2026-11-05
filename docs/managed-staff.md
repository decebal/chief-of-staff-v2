# Managed staff dashboard

The Staff card follows configured mandates and source-backed AllStack delivery records. It does not launch models, assign customers or authorize publication. Staff data lives under the existing dashboard commitment, with private roster and handoff material outside Git.

The report's Plan tab shows a Staff card when the dashboard commitment carries a `staff` block: one row per role with its status, its first follow-up and its execution proof, and a summary of configured roles, open managed runs and follow-ups that need attention. Several roles can refer to one run; the open-run count deduplicates its identity. A run reference without a receipt and readback date shows as "Run reference; evidence missing", never as a verified run.

## Refresh existing evidence

```text
cos --home <workspace> --workspace <workspace> staff-sync \
  --coordinator <enrolled-worktree> --run <recorded-run> --role <configured-role>
```

The coordinator and engine executable must match saved enrollment configuration. The command reads AllStack evidence and history, checks coherent revisions, and uses resume to verify current Chronis ownership when no unresolved operation exists. One bounded process deadline covers those reads; children are killed and reaped on expiry. No model or delivery mutation command is issued.

The projection retains the host, epoch, workflow stage, checkpoint, task, handoffs and gate receipts. A stale engine revision, changed coordinator, mismatched history or ownership failure preserves the previous cos state. Successful updates use the existing revision-checked state commit and journal.

## Parking and proof boundaries

Owner-requested parking survives evidence refresh. An engine can retain its planning stage while the Staff view remains parked; refresh does not reopen execution or grant model access. Run evidence does not establish a completed worker report, passed candidate gates, publication, installed application behavior or customer acceptance.

The dashboard keeps unknown measurements and existing acceptance gates unchanged. Live state, private transcripts and customer data are excluded from this change. Screenshots of real workspaces are not captured.
