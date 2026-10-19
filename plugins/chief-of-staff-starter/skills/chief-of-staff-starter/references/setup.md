# Guided setup

Ask one useful question at a time. Reuse explicit answers already provided. Do not make the user complete a long questionnaire before showing value.

1. Confirm the private workspace and whether the user wants local tracking. Explain that setup is not approval to connect services or perform external actions. The helper needs Python 3.10+; if it is unavailable, draft the setup in chat without silently installing software.
2. Ask: “What is your role, and what result would make the next 90 days successful?” Capture the user's wording and who benefits.
3. Identify one monthly goal, its measurable finish line, target date, and owner. Ask what matters beyond the current quarter and what must not be sacrificed. Treat tensions between goals explicitly; do not invent a company strategy.
4. Define a KPI with units, a real baseline or `null`, target, measurement source, and evidence date. Distinguish outcomes from activity metrics. A missing value is not zero. If measurement is not yet available, create a proposed deliverable to establish it.
5. Work backward into this week's deliverables. Each has a linked goal, owner plus proposed/confirmed basis, date plus basis, blockers/dependencies, and definition of done. Ask which choices the user confirms; do not fabricate commitments.
6. Confirm timezone, working days/hours, capacity constraints, and communication preferences. Do not infer timezone from the package author. Store an IANA timezone before enabling a schedule.
7. Offer at most three initial sources. For each approved source, record service, account/workspace identity, read scope, and approval reference. Use supported account connections and verify identity before reading business content. A connected account is not authority to review every mailbox, channel, base, or repository. Connector availability varies by user and organization. Never copy another user's tokens or private source IDs.
8. Show the proposed profile, first goal, and first weekly plan. Save only confirmed facts and clearly labeled proposals. Run a manual review if requested. Leave scheduling and all source writes disabled.

Use `scripts/state.py init --workspace ABSOLUTE_PRIVATE_WORKSPACE` to create empty runtime files, then edit profile.json only with confirmed preferences. Do not initialize inside the installed plugin or overwrite existing runtime files. Use `state.py commit` for state changes as described in the state reference.

The `assets/profile.example.json` file is a blank starting template. `assets/demo-state.json` is wholly fictional; never import it into a real user's tracking state without an explicit demo request. Example links are placeholders, not live evidence.

For each private profile `sources` entry, use `id`, `service`, `accountIdentity`, `scope`, `readsApproved`, `approvalRef`, and `readBudget`. Record only a non-secret account/workspace identifier and the user's actual scope. Choose a conservative read budget with the user; do not imply it is a provider quota. Leave unapproved entries disabled. The profile is a user-maintained preference record; the v0.1 helper validates state, not this profile's truth or source permissions.

Before recurring use, verify: the user can explain the goal; each priority has a reason; unknown evidence is visible; a handoff has acceptance criteria; reviewed does not mean done; and no external write occurred. Ask about cadence only after the first brief is useful.
