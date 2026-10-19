# Your Chief of Staff for Codex

Version 0.2.0 · Team pilot · October 17, 2026

Turn your goals into a workable week and a short list of what matters today.

**Goals → KPIs → weekly deliverables → priorities → delegation**

This is a starter kit, not somebody else's account or dashboard. It includes a reusable Codex skill, a guided setup, fictional practice data, a local state helper, and an optional dashboard build brief. It does not include a running dashboard, preconnected services, a background worker, or an active schedule.

You need Codex and a private local project folder. The optional local tracking helper needs Python 3.10 or later; no third-party Python packages are required. Without Python, Codex can still help draft the setup in chat, but verified persistent tracking needs the helper or a separately reviewed alternative.

## Start here

1. Unzip this package into a new, private folder on your own computer. Keep it separate from shared company repositories and cloud-synced folders unless your organization approves storing this data there.
2. Open that folder as a project in Codex. Paste the setup prompt below. You can use the skill by its file path before installing the optional plugin.
3. Choose your first outcome and the sources Codex may read. Connect services through Codex's supported connection flow, using your own accounts. Do not paste credentials into chat or this folder.
4. Review one manual brief before deciding whether to add a recurring review.

### Paste this into Codex

```text
Read plugins/chief-of-staff-starter/skills/chief-of-staff-starter/SKILL.md completely and use Setup mode for this project.

Help me create my own Chief of Staff. Ask one useful question at a time, starting with my role and the most important outcome I want over the next 90 days. Then define one monthly goal, a measurable finish line, and this week's deliverables with owners, due dates, blockers, and what counts as done.

Save my confirmed setup and tracking state locally in this project. Start with read-only sources and drafts in this chat. Do not connect an account, install a plugin, create a schedule, deploy a dashboard, send a message, or change a source system without my separate approval. Treat the demo as fictional, never as my goals or work.
```

## Optional: install as a local Codex plugin

The package includes a local marketplace entry and a plugin manifest. After choosing this folder as your project, ask Codex to help install the **Chief of Staff Starter** plugin from this project's local marketplace. Review the files first. If the local marketplace is not visible, restart the desktop app and check the project context; availability can depend on your client and organization policy.

If your client requires a Git repository to discover a repo marketplace, ask Codex to initialize **only this starter folder**, without a remote. The `.gitignore` excludes personal runtime data. Do not initialize or change a parent workspace. A direct path invocation above remains the fallback and requires no plugin installation.

Once installed, use the skill through the skill picker or ask for `$chief-of-staff-starter`. Do not install both a plugin copy and a standalone copy of the same skill. Installing a skill does not connect your accounts or authorize its actions.

Official setup references: [Build skills](https://developers.openai.com/codex/skills) and [Package your plugin](https://developers.openai.com/plugins/build/plugins). Checked October 16, 2026; client behavior may change.

## Optional: install in Claude Code

The same skill directory carries a Claude Code plugin manifest, and this folder a Claude Code marketplace (`.claude-plugin/marketplace.json`). In Claude Code, add this folder as a plugin marketplace and install **chief-of-staff-starter** from it, after reviewing the files. The same rules apply: installing a skill connects nothing and authorizes nothing.

## Optional: run it on a schedule with cos

If you received this kit as part of the `cos` repository, `cos` runs the Priorities review and the weekly steering review against the same `.chief-of-staff/` workspace, reading only the sources your profile approves. See the repository README. It records runs in your state only if you approve local tracking in the profile.

## Useful requests

- **Setup:** “Help me set up my Chief of Staff.”
- **Priorities:** “What are the three most important things for me today? Review only my approved sources.”
- **Weekly plan:** “Work backward from my monthly goals into this week's deliverables. Keep proposed dates and owners clearly marked.”
- **Strategic review:** “Are my goals reinforcing each other? What should we stop, sequence differently, or test next?”
- **Delegate:** “Prepare a handoff for deliverable D-1. Draft-only; include evidence, blockers, and acceptance criteria.”
- **Status:** “Which sources were actually reviewed, how fresh is the evidence, and what's blocked? Don't refresh them.”

## Your first week

Start with one goal and at most three useful sources: typically email, calendar, and a task system. If no connection is available, use information you supply and label coverage accordingly.

Run one manual review, correct its assumptions, and try one handoff. After several useful reviews, choose whether a morning, hourly, or another cadence is worth the read volume and interruptions. A schedule is optional and requires your approval. Its settings must match your timezone and working preferences.

Success means you can explain the goal, measure progress, identify this week's deliverables, and act on today's top priorities. More integrations are not the success measure.

## What's private and what's shareable

- Share the original clean ZIP, not your personalized working folder.
- Your data lives in `.chief-of-staff/` after setup, outside the plugin. The helper creates this directory with restrictive permissions where supported. This is not encryption or protection from other processes on your device.
- Connected-source access, credentials, source content, logs, approval records, and schedule IDs are never part of the starter.
- The supplied packaging script only includes an explicit file allowlist and verifies hashes. It does not certify that arbitrary edited prose is safe to share. Review edits for private information before making a new release.

## Updates, backup, and rollback

One person owns a private runtime workspace; one coordinating chat owns its recurring review. Other chats can prepare handoffs, but should not edit shared state concurrently. The local helper detects competing writes; it is not a distributed task queue or a connector permission system.

Back up `.chief-of-staff/` to an approved private location before an upgrade. Update the plugin and guides without replacing your runtime folder. Test the new version against fictional data first. Keep the prior package for rollback; do not silently downgrade a newer state schema. A credential expiry requires reconnecting through Codex, not copying tokens from another teammate.

The optional dashboard is specified in [guides/OPTIONAL-DASHBOARD.md](guides/OPTIONAL-DASHBOARD.md). Building or hosting it is a separate decision. Nothing in this package publishes your work.
