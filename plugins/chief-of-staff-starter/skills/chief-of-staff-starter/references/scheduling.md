# Optional scheduled review

No scheduler is bundled or enabled. Use the host's supported automation tooling only after the user requests a recurring review and confirms its scope. Do not implement a hidden cron job or a second scheduler as a workaround.

Before enabling, confirm the workspace, timezone, cadence, working/quiet hours, approved source scopes, local-state writes, notification threshold, and what happens on failures. Review existing automations to avoid duplication; preserve settings unless the user asks to change them. Record the resulting automation identity and approval reference in the private profile. An `enabled` flag is a record, not proof that the host has a working schedule.

Use one coordinating chat/schedule per personal workspace. Optional specialist modules own their procedure; the Chief owns cadence, synthesis, and notifications. Keep specialists off until individually configured. Do not dispatch a specialist merely because its skill is installed.

## Suggested prompt, to personalize before use

Use the installed Chief of Staff Starter skill in Scheduled review mode for the confirmed private workspace. Read its scheduled-mode references, profile, source scopes, prior run, and unresolved commitments. Review only approved sources. Respect persistent source blocks and budgets. Keep source reads read-only and drafts in this chat. Update approved local tracking with source-linked evidence; preserve original invocation identity and reconcile interrupted runs before starting new work. Do not revive completed commitments or repeat unchanged items. Return at most three consequential actions and two material verified changes, or use the host's required quiet result. Do not send, post, assign, schedule, deploy, spend, or change access. No dashboard uploads unless separately configured and approved. Keep permission and authentication failures stopped until resolved with current authority.

This text is a proposal, not an instruction to create a schedule now. Do not insert fabricated workspace paths or schedule IDs.

## Runtime behavior

Persist the actual invocation ID, trigger time, source coverage, disposition, and notification decision. If a run is interrupted, reconcile its evidence and any uncertain actions before recovery. Do not replay an external write after a timeout; verify whether it happened first.

Use at most one in-flight review for this workspace. In v0.1 the state helper protects commits, not entire runs or provider requests. A second concurrent review must stop/defer rather than assume the file lock coordinates its connector calls. Cross-chat/provider request locking must be implemented and tested before multiple collectors are enabled.

Hourly review does not mean a full mailbox rescan every hour. Use bounded incremental reads, retain a daily backlog check, and honor source cooldowns. Read frequency and available automation behavior depend on the user's client, plan, organization policies, and connected services.

Follow the actual host heartbeat output contract. When that host requires XML, use its real automation ID and NOTIFY/DONT_NOTIFY decision; do not print placeholder XML or send quiet statuses to another app.
