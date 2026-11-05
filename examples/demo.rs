//! A fully synthetic `COS_HOME` for screenshots and trying `cos` without
//! connecting anything: a fictional person, company, repos and teammates.
//! Nothing here reads your accounts, and every month is pre-marked archived
//! so `cos report` and `cos dash` never call GitHub for it.
//!
//! ```sh
//! cargo run --example demo -- /tmp/cos-demo
//! cos --home /tmp/cos-demo --projects /tmp/cos-demo/projects dash
//! ```

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::Result;
use cos::gather::{Facts, Pr, Task};
use cos::history::{DayRow, Merged};
use cos::reports::Brief;
use cos::store::{self, Kind};
use jiff::{Span, civil::Date};
use serde_json::json;

const ORG: &str = "northwind";
const REPOS: &[&str] = &["api-gateway", "web-app", "mobile", "infra", "design-system"];

/// xorshift64*: deterministic, so screenshots are reproducible.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
}

fn main() -> Result<()> {
    let home = PathBuf::from(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "/tmp/cos-demo".into()),
    );
    anyhow::ensure!(
        !home.join(".git").exists(),
        "{} looks like a repo; point the demo at an empty directory",
        home.display()
    );
    let today = jiff::Zoned::now().date();
    let mut rng = Rng(0x00C0_FFEE_2026);

    write(
        &home.join("context/profile.md"),
        "# Profile\n\n- **Name**: Alex Rivera\n- **Role and team**: staff engineer, Northwind Labs platform team\n",
    )?;
    write(
        &home.join("context/goals.md"),
        "# Goals\n\n## Mobile 2.0 on the new gateway\n## Halve review wait time\n## Design system v3\n",
    )?;

    let facts = facts(today, &mut rng);
    store::save_json(&home, Kind::Facts, &today.to_string(), &facts)?;

    let mut brief_value = brief_json(today);
    brief_value["week"] = week_json(today);
    let brief: Brief = serde_json::from_value(brief_value)?;
    let day = today.to_string();
    // The run's outcome line, as `cos brief` appends it: the report shows its time and cost.
    write(
        &home.join(format!("out/audit/{day}.jsonl")),
        &format!(
            "{}\n",
            json!({ "at": jiff::Timestamp::now().to_string(), "agent": "tactician", "result": "success", "cost_usd": 0.41, "turns": 9, "denied": [] })
        ),
    )?;
    store::save(&home, Kind::Briefs, &day, &brief, &brief.to_markdown(&day))?;

    history(&home, today, &facts, &mut rng)?;
    workspace(&home, today)?;
    docs_room(&home, today, &mut rng)?;
    staff(&home, today)?;

    let report = cos::report::build(&home, &home)?;
    for back in 1..=12 {
        let d = today.checked_sub(Span::new().days(back))?;
        std::fs::copy(&report, home.join(format!("out/reports/{d}.html")))?;
    }
    println!("demo home ready: {}", home.display());
    println!(
        "  cos --home {0} --projects {0}/projects dash",
        home.display()
    );
    Ok(())
}

fn write(path: &Path, text: &str) -> Result<()> {
    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(path, text)?;
    Ok(())
}

fn ts(d: Date, h: u8) -> String {
    format!("{d}T{h:02}:00:00Z")
}

fn facts(today: Date, rng: &mut Rng) -> Facts {
    let titles = [
        "Rate-limit the public token endpoint",
        "Cache layout tokens at build time",
        "Retry uploads on flaky networks",
        "Move preview envs to the new cluster",
        "Dark-mode tokens for charts",
        "Paginate the audit log API",
        "Offline queue for mobile sync",
        "Trace IDs through the gateway",
        "Remove the legacy session cookie",
        "Typed client for the billing API",
        "Speed up cold starts on Android",
        "Canary deploys for the gateway",
    ];
    let mut tasks = Vec::new();
    for (i, t) in titles.iter().enumerate() {
        let repo = REPOS[i % REPOS.len()];
        let pri = ["p0", "p1", "p1", "p2", "p2", "p3"][rng.below(6) as usize];
        tasks.push(Task {
            repo: repo.into(),
            id: format!("t-{:04x}", 0x3a00 + i * 37),
            kind: if i % 5 == 0 {
                "epic".into()
            } else {
                "task".into()
            },
            title: (*t).into(),
            priority: pri.into(),
            blocked_by: if i % 4 == 3 {
                format!("t-{:04x}", 0x3a00 + (i - 1) * 37)
            } else {
                String::new()
            },
            parent: String::new(),
        });
    }
    for extra in 0..30 {
        let repo = REPOS[rng.below(REPOS.len() as u64) as usize];
        tasks.push(Task {
            repo: repo.into(),
            id: format!("t-{:04x}", 0x5000 + extra),
            kind: "task".into(),
            title: format!("Follow-up {extra} for {repo}"),
            priority: ["p1", "p2", "p2", "p3"][rng.below(4) as usize].into(),
            blocked_by: String::new(),
            parent: String::new(),
        });
    }
    tasks.sort_by(|a, b| a.repo.cmp(&b.repo));

    let pr = |repo: &str, n: u64, title: &str, updated: Date, closed: Option<Date>| Pr {
        repo: format!("{ORG}/{repo}"),
        number: n,
        title: title.into(),
        draft: false,
        updated: ts(updated, 10),
        closed: closed.map(|c| ts(c, 15)),
    };
    let ago = |d: i64| today.checked_sub(Span::new().days(d)).unwrap();
    let review_requested = vec![
        pr(
            "api-gateway",
            412,
            "Release 3.4: gateway auth rewrite",
            ago(0),
            None,
        ),
        pr("mobile", 288, "Offline sync queue", ago(1), None),
        pr("web-app", 1931, "Chart dark-mode tokens", ago(2), None),
        pr("design-system", 77, "Button v3 variants", ago(4), None),
        pr("infra", 640, "Preview env autoscaling", ago(6), None),
        pr("web-app", 1902, "Billing page a11y fixes", ago(12), None),
        pr("api-gateway", 398, "Trace propagation", ago(21), None),
        pr("mobile", 251, "Android cold start", ago(45), None),
    ];
    let mine_open = vec![
        pr("api-gateway", 415, "Canary deploy pipeline", ago(0), None),
        pr("web-app", 1936, "Audit log pagination", ago(1), None),
        pr("infra", 644, "Cluster migration runbook", ago(3), None),
    ];
    let mut merged = Vec::new();
    for back in 0..7i64 {
        let n = if back % 7 >= 5 {
            rng.below(2)
        } else {
            2 + rng.below(5)
        };
        for k in 0..n {
            let repo = REPOS[rng.below(REPOS.len() as u64) as usize];
            merged.push(pr(
                repo,
                2000 + (back as u64) * 10 + k,
                "Shipped change",
                ago(back),
                Some(ago(back)),
            ));
        }
    }
    Facts {
        collected_at: jiff::Zoned::now().to_string(),
        merged_since: ago(7).to_string(),
        tasks,
        mine_open,
        review_requested,
        merged,
        gaps: vec![],
        unread: vec![],
    }
}

fn brief_json(today: Date) -> serde_json::Value {
    let in_days = |d: i64| today.checked_add(Span::new().days(d)).unwrap().to_string();
    let stop = "Stop before anything outward-facing — merging, commenting, sending, approving — and show me the draft.";
    json!({
        "headline": "Release 3.4 ships in three days and its PR waits on your review; Priya is blocked on the mobile sync PR.",
        "needs_you": [
            { "verb": "do", "owner": "Alex", "recommendation": "Review northwind/api-gateway#412 (Release 3.4) before the 16:00 go/no-go",
              "why": "The release cannot ship without a reviewer, and Mobile 2.0's beta depends on it.", "next_step": "Read the auth middleware diff first; it carries the rollout risk.",
              "source": "https://github.com/northwind/api-gateway/pull/412", "deadline": in_days(3), "deadline_basis": "confirmed", "goal_id": "G-1", "deliverable_id": "D-1",
              "agent_prompt": format!("Outcome: a review of northwind/api-gateway#412 for D-1, which unblocks the Mobile 2.0 beta (G-1). Work in ~/Projects/api-gateway. Run `gh pr checkout 412`, `gh pr diff 412` and `gh pr checks 412`. Done when: findings are listed with file and line, plus a verdict. {stop}") },
            { "verb": "delegate", "owner": "Priya", "recommendation": "Hand the mobile#288 retry review to Priya's pair, with the handoff for D-2",
              "why": "Priya is blocked on it and the beta branch (D-2) waits on it.", "next_step": "Raise it in the 14:00 1:1.",
              "source": "https://github.com/northwind/mobile/pull/288", "deadline": "", "deadline_basis": "none", "goal_id": "G-1", "deliverable_id": "D-2",
              "agent_prompt": format!("Outcome: review comments on northwind/mobile#288 (offline sync queue). Work in ~/Projects/mobile. Run `gh pr checkout 288`. Check the queue's retry and ordering logic and its tests. Done when: draft comments with file and line. {stop}") },
            { "verb": "decide", "owner": "Alex", "recommendation": "Choose a weekly review rotation over a 48h SLA",
              "why": "The median review wait is 41h against a 24h target; the queue grew from 5 to 8.", "next_step": "Confirm D-3 so the rotation starts Monday.",
              "source": "cos facts: review queue", "deadline": in_days(4), "deadline_basis": "proposed", "goal_id": "G-2", "deliverable_id": "D-3",
              "agent_prompt": format!("Outcome: a one-page review rotation proposal for G-2. Using `gh search prs --review-requested=@me --state=open`, chart review age by repo and draft the rotation. {stop}") }
        ],
        "what_changed": [
            { "kind": "done", "what": "Gateway auth rewrite merged (api-gateway#405-#411); Release 3.4 is now review-only.", "source": "https://github.com/northwind/api-gateway/pull/411" },
            { "kind": "risk", "what": "infra#640 autoscaling slipped, so the canary pipeline (api-gateway#415) cannot land before the freeze.", "source": "https://github.com/northwind/infra/pull/640" }
        ],
        "calendar": [
            { "date": in_days(0), "start": "09:30", "end": "09:45", "what": "Platform stand-up", "prep": "Raise the gateway canary blocker." },
            { "date": in_days(0), "start": "11:00", "end": "12:00", "what": "Design review: Button v3", "prep": "Read design-system#77 first." },
            { "date": in_days(0), "start": "14:00", "end": "14:30", "what": "1:1 with Priya", "prep": "Unblock the offline sync PR." },
            { "date": in_days(0), "start": "16:00", "end": "17:00", "what": "Release 3.4 go/no-go", "prep": "Have api-gateway#412 reviewed." },
            { "date": in_days(1), "start": "10:00", "end": "11:30", "what": "Gateway canary dry run", "prep": "" },
            { "date": in_days(1), "start": "15:00", "end": "15:30", "what": "Review rotation kickoff", "prep": "Bring the D-3 draft." }
        ],
        "backlog": [
            { "item": "Reply to Sam about the cluster migration window", "owner": "Alex", "source": "mail: Sam Okafor, 'Migration window?'", "next_review": "Before Wednesday's infra freeze" },
            { "item": "web-app#1902 a11y fixes waiting on v3 token names", "owner": "Jordan", "source": "https://github.com/northwind/web-app/pull/1902", "next_review": "After the 11:00 design review" },
            { "item": "mobile#251 idle 45 days", "owner": "author", "source": "https://github.com/northwind/mobile/pull/251", "next_review": "Ask whether it is still wanted at the next triage" }
        ],
        "coverage": [
            { "source": "calendar", "health": "healthy", "review": "complete", "window_start": format!("{today}T00:00:00+00:00"), "window_end": format!("{today}T23:59:59+00:00"), "note": "Today and the rest of the week." },
            { "source": "email", "health": "healthy", "review": "partial", "window_start": format!("{}T00:00:00+00:00", in_days(-2)), "window_end": format!("{today}T08:00:00+00:00"), "note": "Unread human threads, two days; budget reached before older threads." }
        ],
        "gaps": [ "Notion is not an approved source, so it was not read." ],
        "upcoming": [
            { "date": in_days(3), "what": "Release 3.4", "source": "calendar" },
            { "date": in_days(8), "what": "Mobile 2.0 beta", "source": "G-1" },
            { "date": in_days(15), "what": "Q1 planning", "source": "calendar" },
            { "date": in_days(29), "what": "Design system v3 GA", "source": "G-3" }
        ],
        "tickets": [
            { "key": "MOB-412", "summary": "Offline sync for the beta build", "status": "In Progress", "priority": "High", "url": "https://northwind.example/browse/MOB-412", "goal_id": "G-1" },
            { "key": "PLAT-88", "summary": "Canary pipeline for api-gateway", "status": "Blocked", "priority": "Medium", "url": "https://northwind.example/browse/PLAT-88", "goal_id": "" },
            { "key": "DS-31", "summary": "Inventory v2 components still in use", "status": "To Do", "priority": "Low", "url": "https://northwind.example/browse/DS-31", "goal_id": "G-3" }
        ]
    })
}

fn week_json(today: Date) -> serde_json::Value {
    let in_days = |d: i64| today.checked_add(Span::new().days(d)).unwrap().to_string();
    let stop = "Stop before anything outward-facing and show me the draft.";
    json!({
        "summary": "Mobile 2.0 moved and review wait time did not; the beta now depends on one review.",
        "goals": [
            { "goal_id": "G-1", "goal": "Mobile 2.0 on the new gateway", "confidence": "rock", "kpi": "Beta testers: unknown (enrolment opens with the beta)",
              "evidence": "Auth rewrite merged; 3.4 in review.", "bottleneck": "One reviewer for api-gateway#412.",
              "recommendation": "maintain", "next": "Ship 3.4, then cut the beta branch (D-2).", "would_change": "A blocking finding in #412.",
              "agent_prompt": format!("Work in ~/Projects/mobile. List what the 2.0 beta still needs from the gateway 3.4 API and draft the beta checklist. {stop}") },
            { "goal_id": "G-2", "goal": "Halve review wait time", "confidence": "sand", "kpi": "Median review wait: 41h against 24h (last 7 days)",
              "evidence": "Queue grew from 5 to 8; two PRs older than three weeks.", "bottleneck": "No owner for the queue.",
              "recommendation": "sequence", "next": "Start a weekly rotation before any tooling.", "would_change": "Two weeks of rotation without the median falling.",
              "agent_prompt": format!("Using `gh search prs --review-requested=@me --state=open`, chart review age by repo and propose two options for a review rotation. {stop}") },
            { "goal_id": "G-3", "goal": "Design system v3", "confidence": "water", "kpi": "unknown: no adoption measure yet",
              "evidence": "Tokens landed; component scope still undefined.", "bottleneck": "Scope.",
              "recommendation": "test", "next": "Spike: inventory components used by web and mobile.", "would_change": "The inventory showing fewer than ten shared components.",
              "agent_prompt": format!("Work in ~/Projects/design-system. Inventory components imported by web-app and mobile; output a table of usage counts. {stop}") }
        ],
        "proposed": [
            { "goal_id": "G-2", "title": "Run the first review rotation week", "owner": "Alex", "owner_basis": "proposed", "due_date": in_days(7), "due_basis": "proposed",
              "done_when": "Every PR older than 48h has a named reviewer", "blocker": "" },
            { "goal_id": "G-3", "title": "Define the v3 adoption KPI", "owner": "Jordan", "owner_basis": "proposed", "due_date": "", "due_basis": "unknown",
              "done_when": "A KPI with unit, baseline and source is agreed", "blocker": "Needs the component inventory" }
        ],
        "decisions": [ { "decision": "Review rotation or review SLA?", "options": ["Weekly rotation", "48h SLA", "Both"], "recommendation": "Weekly rotation; measure for two weeks." } ],
        "stop_doing": [ "Two days on preview-env cost work, which serves no goal.", "Ad-hoc reviews outside the rotation once it starts." ]
    })
}

/// A fictional `.chief-of-staff/` workspace, written through the same
/// validated commit `cos` and the kit's helper use.
fn workspace(home: &Path, today: Date) -> Result<()> {
    let ws = std::path::absolute(home)?;
    cos::state::initialize(&ws)?;
    let day = |d: i64| today.checked_add(Span::new().days(d)).unwrap().to_string();
    let at = |d: i64, h: u8| format!("{}T{h:02}:00:00+00:00", day(d));
    let monday = today
        .checked_sub(Span::new().days(i64::from(today.weekday().to_monday_zero_offset())))?
        .to_string();
    let profile = json!({
        "schemaVersion": 1,
        "owner": "Alex Rivera",
        "role": "Staff engineer, Northwind Labs platform team (fictional)",
        "timezone": "America/Chicago",
        "workingHours": "Mon-Fri 09:00-18:00",
        "longTermDirection": "Make the platform the fastest path to ship mobile features.",
        "currentSeason": "Mobile 2.0 beta",
        "guardrails": ["No releases on Fridays", "Reviews before new work"],
        "localTracking": { "approved": true, "approvalRef": "demo: fictional" },
        "sources": [
            { "id": "calendar", "service": "Google Calendar", "accountIdentity": "alex@northwind.example", "scope": "primary calendar", "readsApproved": true, "approvalRef": "demo", "readBudget": "1 list per run" },
            { "id": "email", "service": "Gmail", "accountIdentity": "alex@northwind.example", "scope": "unread human threads, 2 days", "readsApproved": true, "approvalRef": "demo", "readBudget": "2 searches, 10 threads" },
            { "id": "code", "service": "github", "accountIdentity": "alex-northwind", "scope": "my open PRs, reviews requested of me, my merged PRs", "readsApproved": true, "approvalRef": "demo", "readBudget": "3 gh searches" },
            { "id": "tasks", "service": "chronis", "accountIdentity": "local", "scope": "ready tasks in ~/Projects", "readsApproved": true, "approvalRef": "demo", "readBudget": "1 cn ready per repo" }
        ],
        "schedule": { "enabled": false, "automationId": null, "approvalRef": null, "cadence": null },
        "permissions": { "sourceWrites": false, "externalNotifications": false, "dashboardUploads": false },
        "recovery": { "automaticRateLimitProbe": false },
        "setupConfirmedAt": at(-14, 9),
        "monitoredRepos": REPOS.iter().map(|r| format!("{ORG}/{r}")).collect::<Vec<_>>(),
        "jiraProjects": [{ "key": "API", "repos": [format!("{ORG}/api-gateway")] }]
    });
    cos::state::atomic_write(&ws.join(".chief-of-staff/profile.json"), &profile)?;

    let state = json!({
        "schemaVersion": 1,
        "revision": 0,
        "goals": [
            { "id": "G-1", "title": "Mobile 2.0 on the new gateway", "owner": "Alex", "finishLine": "Beta live for 500 testers on gateway 3.4", "targetDate": day(8), "status": "active",
              "whereItLives": "northwind/api-gateway (main); Jira API", "docs": ["api-gateway", "mobile", "web-app"] },
            { "id": "G-2", "title": "Halve review wait time", "owner": "Alex", "finishLine": "Median review wait under 24h for two weeks running", "targetDate": day(40), "status": "active",
              "docs": ["infra:docs/guides", "infra:docs/ops"] },
            { "id": "G-3", "title": "Design system v3", "owner": "Jordan", "finishLine": "v3 components used by web and mobile", "targetDate": day(29), "status": "active",
              "docs": ["design-system"] }
        ],
        "kpis": [
            { "id": "K-1", "goalId": "G-1", "name": "Beta testers enrolled", "unit": "testers", "baseline": 0, "target": 500, "current": null, "evidenceAt": null, "source": null, "reportingWindow": "beta" },
            { "id": "K-2", "goalId": "G-2", "name": "Median review wait", "unit": "hours", "baseline": 52, "target": 24, "current": 41, "evidenceAt": at(0, 8), "source": "gh search, review-requested", "reportingWindow": "last 7 days" }
        ],
        "deliverables": [
            { "id": "D-1", "goalId": "G-1", "title": "Review api-gateway#412 (Release 3.4)", "owner": "Alex", "ownerBasis": "confirmed", "dueDate": day(3), "dueBasis": "confirmed", "week": monday,
              "blocker": null, "dependencies": [], "doneWhen": "Review submitted with a verdict and blocking findings listed", "status": "open",
              "source": "https://github.com/northwind/api-gateway/pull/412", "evidenceAt": at(0, 8), "reviewedAt": null, "meetingAt": at(0, 16), "completionEvidence": null },
            { "id": "D-2", "goalId": "G-1", "title": "Cut the mobile 2.0 beta branch", "owner": "Priya", "ownerBasis": "proposed", "dueDate": day(5), "dueBasis": "proposed", "week": monday,
              "blocker": "Waits on the 3.4 release", "dependencies": ["D-1"], "doneWhen": "Beta branch builds green and TestFlight has a build", "status": "blocked",
              "source": "https://github.com/northwind/mobile/pull/288", "evidenceAt": at(0, 8), "reviewedAt": null, "meetingAt": null, "completionEvidence": null },
            { "id": "D-3", "goalId": "G-2", "title": "Draft the review rotation", "owner": "Alex", "ownerBasis": "proposed", "dueDate": day(4), "dueBasis": "proposed", "week": monday,
              "blocker": null, "dependencies": [], "doneWhen": "A rotation for the next four weeks, agreed at stand-up", "status": "proposed",
              "source": "cos brief", "evidenceAt": at(-1, 16), "reviewedAt": null, "meetingAt": null, "completionEvidence": null },
            { "id": "D-4", "goalId": "G-3", "title": "Land design tokens v3", "owner": "Jordan", "ownerBasis": "confirmed", "dueDate": day(-2), "dueBasis": "confirmed", "week": monday,
              "blocker": null, "dependencies": [], "doneWhen": "Tokens published and consumed by web-app", "status": "complete",
              "source": "https://github.com/northwind/design-system/pull/71", "evidenceAt": at(-2, 15), "reviewedAt": null, "meetingAt": null,
              "completionEvidence": { "reference": "https://github.com/northwind/design-system/pull/71", "verifiedAt": at(-2, 16) } },
            { "id": "D-5", "goalId": "G-1", "title": "API-77: Smoke-test gateway 3.4 against the staging mobile build", "owner": "Alex", "ownerBasis": "confirmed", "dueDate": day(2), "dueBasis": "confirmed", "week": monday,
              "blocker": null, "dependencies": [], "doneWhen": "API-77 resolved in Jira", "status": "open",
              "source": "https://northwind.example/jira/browse/API-77", "evidenceAt": at(0, 8), "reviewedAt": null, "meetingAt": null, "completionEvidence": null }
        ],
        "commitments": [
            { "id": "C-1", "title": "Sam picks the migration window", "owner": "Sam", "status": "waiting", "source": "mail: Sam Okafor, 'Migration window?'", "evidenceAt": at(0, 8),
              "nextReview": "Before Wednesday's infra freeze", "dueDate": null, "meetingAt": at(-1, 15), "reviewedAt": at(0, 9), "completionEvidence": null, "goalId": null }
        ],
        "sources": [
            { "id": "calendar", "health": "healthy", "review": "complete", "checkedAt": at(0, 8), "evidenceAt": at(0, 8), "lastCompleteThrough": at(0, 7), "windowStart": at(-1, 7), "windowEnd": at(0, 7), "note": "Fictional." },
            { "id": "email", "health": "healthy", "review": "partial", "checkedAt": at(0, 8), "evidenceAt": at(0, 8), "lastCompleteThrough": null, "windowStart": at(-2, 0), "windowEnd": at(0, 8), "note": "Budget reached before older threads." }
        ],
        "runs": [
            { "id": "R-brief-demo", "invokedAt": at(-1, 8), "finishedAt": at(-1, 8), "mode": "brief", "status": "partial", "coverage": [], "notificationDecision": "none" }
        ],
        "incidents": [],
        "journal": []
    });
    cos::state::commit(&ws, &state, 0, "Fictional demo workspace")?;
    Ok(())
}

fn history(home: &Path, today: Date, facts: &Facts, rng: &mut Rng) -> Result<()> {
    let dir = home.join("out/history");
    std::fs::create_dir_all(&dir)?;
    let mut days = Vec::new();
    for back in (0..30i64).rev() {
        let d = today.checked_sub(Span::new().days(back))?;
        if d.weekday().to_monday_one_offset() > 5 {
            continue;
        }
        let ready = BTreeMap::from([
            ("p0".to_owned(), 2 + rng.below(3) as usize),
            ("p1".to_owned(), 10 + rng.below(6) as usize),
            ("p2".to_owned(), 14 + rng.below(8) as usize),
            ("p3".to_owned(), 3 + rng.below(4) as usize),
        ]);
        days.push(DayRow {
            date: d.to_string(),
            review_queue: (4 + (30 - back as usize) / 7 + rng.below(3) as usize).min(12),
            mine_open: 2 + rng.below(4) as usize,
            ready,
            must_do: Some(1 + rng.below(3) as usize),
            meeting_minutes: Some(60 + 15 * rng.below(10) as u32),
            wins: Some(rng.below(4) as usize),
        });
    }
    if let Some(last) = days.last_mut() {
        *last = cos::history::row(&today.to_string(), facts, None);
        last.must_do = Some(3);
        last.meeting_minutes = Some(165);
        last.wins = Some(1);
    }

    let mut merged = Vec::new();
    for back in 0..365i64 {
        let d = today.checked_sub(Span::new().days(back))?;
        let weekday = d.weekday().to_monday_one_offset() <= 5;
        let busy = (back / 30) % 4 != 1;
        let n = if !weekday {
            rng.below(2)
        } else if busy {
            rng.below(6)
        } else {
            rng.below(3)
        };
        for k in 0..n {
            merged.push(Merged {
                repo: format!("{ORG}/{}", REPOS[rng.below(REPOS.len() as u64) as usize]),
                number: 10_000 + (back as u64) * 10 + k,
                title: "Shipped change".into(),
                day: d.to_string(),
            });
        }
    }
    merged.sort_by(|a, b| a.day.cmp(&b.day));
    let backfilled: Vec<String> = cos::history::months_to(&today.to_string())
        .into_iter()
        .map(|(from, _)| from)
        .collect();

    std::fs::write(dir.join("daily.json"), serde_json::to_vec_pretty(&days)?)?;
    std::fs::write(dir.join("merged.json"), serde_json::to_vec_pretty(&merged)?)?;
    std::fs::write(
        dir.join("backfilled.json"),
        serde_json::to_vec_pretty(&backfilled)?,
    )?;
    Ok(())
}

/// The documentation room: one git checkout per repo under `<home>/projects`
/// (files staged, nothing committed), a Notion listing, and an organisation
/// with topics, links and marks. Several hundred documents, so the room's
/// node cap and exploration have something to show.
fn docs_room(home: &Path, today: Date, rng: &mut Rng) -> Result<()> {
    const AREAS: [[&str; 6]; 5] = [
        [
            "rate limit",
            "auth token",
            "routing",
            "edge cache",
            "request log",
            "quota",
        ],
        [
            "checkout",
            "search",
            "onboarding",
            "feature flag",
            "session",
            "settings",
        ],
        [
            "push",
            "offline sync",
            "deep link",
            "crash report",
            "app start",
            "beta build",
        ],
        ["cluster", "backup", "network", "secrets", "deploy", "cost"],
        [
            "button",
            "colour token",
            "typography",
            "spacing",
            "icon",
            "theme",
        ],
    ];
    const NOUNS: [&str; 8] = [
        "rollout",
        "budget",
        "schema",
        "retry policy",
        "migration",
        "alerting",
        "ownership",
        "fallback",
    ];
    type Title = fn(&str, usize) -> String;
    let kinds: [(&str, usize, Title); 8] = [
        ("docs/adr", 20, |s, i| {
            format!("ADR {:04}: adopt {s}", i + 1)
        }),
        ("docs/plans", 14, |s, i| {
            format!("Q{} plan for {s}", i % 4 + 1)
        }),
        ("docs/specs", 14, |s, i| format!("Spec: {s} v{}", i % 3 + 1)),
        ("docs/guides", 18, |s, _| format!("How to run {s}")),
        ("docs/ops", 12, |s, _| format!("Operating {s}")),
        ("docs/reference", 16, |s, _| format!("{s} reference")),
        ("docs/architecture", 12, |s, _| format!("{s} architecture")),
        ("apps", 10, |s, _| format!("{s} service")),
    ];
    let slug = |s: &str| s.replace(' ', "-");
    // (repo, path, title) of every document written.
    let mut all: Vec<(String, String, String)> = Vec::new();
    for (r, repo) in REPOS.iter().enumerate() {
        let dir = home.join("projects").join(repo);
        std::fs::create_dir_all(&dir)?;
        git(&dir, &["-c", "init.defaultBranch=main", "init", "-q"])?;
        git(
            &dir,
            &[
                "remote",
                "add",
                "origin",
                &format!("https://github.com/{ORG}/{repo}.git"),
            ],
        )?;
        let mut files: Vec<(String, String)> = vec![
            ("README.md".into(), format!("{repo} (fictional)")),
            ("docs/README.md".into(), format!("{repo} documentation map")),
        ];
        for (k, (base, count, title)) in kinds.iter().enumerate() {
            for i in 0..*count {
                let n = (i * 7 + k * 5) % 48;
                let subject = format!("{} {}", AREAS[r][n / 8], NOUNS[n % 8]);
                let path = match *base {
                    "docs/adr" => format!("{base}/{:04}-{}.md", i + 1, slug(&subject)),
                    "apps" => format!("apps/{}/README.md", slug(&subject)),
                    _ => format!("{base}/{}.md", slug(&subject)),
                };
                files.push((path, title(&subject, i)));
            }
        }
        for (i, (path, title)) in files.iter().enumerate() {
            let mut body =
                format!("# {title}\n\nFictional demo document for the Northwind {repo} repo.\n\n");
            let picks = if i < 2 { 10 } else { 1 + rng.below(3) as usize };
            for _ in 0..picks {
                let (to, to_title) = &files[rng.below(files.len() as u64) as usize];
                if to != path {
                    body.push_str(&format!("- See [{to_title}]({}).\n", relative(path, to)));
                }
            }
            body.push_str(&rich(path, &files[1].0));
            write(&dir.join(path), &body)?;
            all.push(((*repo).to_owned(), path.clone(), title.clone()));
        }
        std::fs::create_dir_all(dir.join("docs/assets"))?;
        std::fs::write(
            dir.join("docs/assets/app-icon.png"),
            include_bytes!("../apps/desktop/icons/128x128.png"),
        )?;
        // The script is a probe: served sandboxed and shown through <img>, it never runs.
        std::fs::write(
            dir.join("docs/assets/flow.svg"),
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"320\" height=\"80\" viewBox=\"0 0 320 80\">\
             <script>alert('svg script ran')</script>\
             <rect x=\"4\" y=\"20\" width=\"120\" height=\"40\" rx=\"8\" fill=\"#171717\"/>\
             <rect x=\"196\" y=\"20\" width=\"120\" height=\"40\" rx=\"8\" fill=\"#ffb224\"/>\
             <path d=\"M128 40h62\" stroke=\"#52a8ff\" stroke-width=\"4\"/>\
             <text x=\"64\" y=\"45\" fill=\"#ffffff\" font-family=\"sans-serif\" font-size=\"14\" text-anchor=\"middle\">Map</text>\
             <text x=\"256\" y=\"45\" fill=\"#171717\" font-family=\"sans-serif\" font-size=\"14\" text-anchor=\"middle\">Guides</text></svg>\n",
        )?;
        write(&dir.join("Taskfile.yml"), "version: '3'\n")?;
        write(&dir.join("src/main.rs"), "fn main() {}\n")?;
        git(&dir, &["add", "-A"])?;
        git(
            &dir,
            &[
                "-c",
                "user.name=Demo",
                "-c",
                "user.email=demo@northwind.example",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-q",
                "-m",
                "Fictional demo repo",
            ],
        )?;
    }

    let page = |title: &str, rng: &mut Rng| {
        format!(
            "https://www.notion.so/northwind/{}-{:016x}",
            slug(&title.to_lowercase()).replace(':', ""),
            rng.next()
        )
    };
    let mut pages = Vec::new();
    for (repo, path, title) in &all {
        let narrative = [
            "docs/plans/",
            "docs/specs/",
            "docs/architecture/",
            "docs/guides/",
        ]
        .iter()
        .any(|p| path.starts_with(p));
        if narrative && rng.below(3) == 0 {
            pages.push(cos::docs::NotionPage {
                url: page(title, rng),
                title: title.clone(),
                parent: "Engineering wiki".into(),
                project: repo.clone(),
            });
        }
    }
    for (title, project) in [
        ("Platform team charter", ""),
        ("Offsite agenda and outcomes", ""),
        ("Hiring loop for platform engineers", ""),
        ("Quarterly business review outline", ""),
        ("Incident review: gateway timeouts in March", "api-gateway"),
        ("Partner onboarding checklist for mobile", "mobile"),
        ("Beta tester recruitment plan", "mobile"),
        ("Design critique schedule and rules", "design-system"),
        ("Accessibility audit findings summary", "design-system"),
        ("Web performance budget proposal", "web-app"),
        ("Cloud spend monthly review", "infra"),
        ("On-call handbook for platform", "infra"),
        ("Release train calendar 2026", ""),
        ("Engineering principles and values", ""),
    ] {
        pages.push(cos::docs::NotionPage {
            url: page(title, rng),
            title: title.into(),
            parent: "Team space".into(),
            project: project.into(),
        });
    }
    cos::docs::save_notion(
        home,
        &cos::docs::NotionList {
            pages,
            listed_at: format!("{today} 08:00"),
        },
    )?;

    let id = |i: usize| format!("{}:{}", all[i].0, all[i].1);
    let pick = |n: usize, keep: &dyn Fn(&(String, String, String)) -> bool, rng: &mut Rng| {
        let pool: Vec<usize> = (0..all.len()).filter(|&i| keep(&all[i])).collect();
        let ids: std::collections::BTreeSet<String> = (0..n)
            .map(|_| id(pool[rng.below(pool.len() as u64) as usize]))
            .collect();
        ids.into_iter().collect::<Vec<_>>()
    };
    let mut org = cos::docs::Organize::default();
    org.topics.insert(
        "Launch readiness".into(),
        pick(9, &|d| d.0 == "api-gateway" || d.0 == "mobile", rng),
    );
    org.topics.insert(
        "New joiner reading".into(),
        pick(7, &|d| d.1.starts_with("docs/guides/"), rng),
    );
    for _ in 0..36 {
        let a = rng.below(all.len() as u64) as usize;
        let b = rng.below(all.len() as u64) as usize;
        if a != b {
            let kind =
                ["depends on", "supersedes", "explains", "same topic"][rng.below(4) as usize];
            org.links.push(cos::docs::Link {
                from: id(a),
                to: id(b),
                kind: kind.into(),
            });
        }
    }
    for _ in 0..30 {
        let mark = if rng.below(2) == 0 {
            cos::docs::Mark::MoveToNotion
        } else {
            cos::docs::Mark::KeepInRepo
        };
        org.marks
            .insert(id(rng.below(all.len() as u64) as usize), mark);
    }
    cos::docs::save_organize(home, &org)?;
    Ok(())
}

/// What the reader renders beyond headings and links: guides get steps (a
/// table, code, a task list); each repo's documentation map gets every other
/// construct, an image, and markup the sanitizer must strip.
fn rich(path: &str, map: &str) -> String {
    let back = relative(path, map);
    if path.starts_with("docs/guides/") {
        return format!(
            "\n## Before you start\n\n> Fictional steps. Run them against the demo cluster only.\n\n\
             | Step | Owner | Takes |\n|---|---|:---:|\n| Request access | you | 1 form |\n\
             | Pair with the on-call | platform | 1 session |\n| Ship a change | you | 1 PR |\n\n\
             ## Steps\n\n1. Clone the repo.\n2. Run the checks:\n\n   ```bash\n   task test\n   task lint -- --fix\n   ```\n\n\
             3. Tick these off:\n\n   - [x] Access granted\n   - [ ] First change reviewed\n   - [ ] Added to the rota\n\n\
             ### Configuration\n\n```toml\n[service]\nretries = 3\ntimeout = \"2s\"\n```\n\n\
             Back to the [documentation map]({back}#start-here) or the [setup section](#before-you-start).\n"
        );
    }
    if path == "docs/README.md" {
        return "\n## Start here\n\nThe map links every **document kind** in this repo, with `inline code`, \
             ~~struck~~ text and an autolink: https://example.com/northwind.\n\n\
             ![The app icon](assets/app-icon.png)\n\n\
             ![A diagram kept as SVG](assets/flow.svg) ![Remote image](https://example.com/logo.png)\n\n\
             ```mermaid\ngraph LR\n  Map --> Guides\n  Map --> ADRs\n```\n\n\
             ```rust\nfn main() {\n    println!(\"northwind\");\n}\n```\n\n\
             ## Safety probes\n\n<script>alert('script ran')</script>\n\n\
             <img src=\"x\" onerror=\"alert('onerror ran')\"> <a href=\"javascript:alert('link ran')\">a javascript: link</a> \
             [another javascript: link](javascript:alert(1)) \
             <span style=\"position:fixed;inset:0\" id=\"graph\" class=\"menu\">styled span</span>\n\n\
             A [missing document](../nowhere/missing.md) and an [external page](https://example.com/handbook).\n\n\
             The [Taskfile](../Taskfile.yml) and the [source folder](../src/) are in the repo but not in the room, so they open on GitHub.\n\n\
             Term\n: A definition line, shown as text.\n\n---\n\nLast line.\n"
            .to_owned();
    }
    String::new()
}

/// `to` as a markdown link from the file `from`, both repo-relative.
fn relative(from: &str, to: &str) -> String {
    let f: Vec<&str> = from.split('/').collect();
    let t: Vec<&str> = to.split('/').collect();
    let (fd, td) = (&f[..f.len() - 1], &t[..t.len() - 1]);
    let common = fd.iter().zip(td).take_while(|(a, b)| a == b).count();
    let mut parts = vec![".."; fd.len() - common];
    parts.extend(t[common..].iter().copied());
    parts.join("/")
}

/// Owned and waited on: `status()` reaps the child before returning.
fn git(dir: &Path, args: &[&str]) -> Result<()> {
    let status = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .status()?;
    anyhow::ensure!(status.success(), "git {args:?} failed in {}", dir.display());
    Ok(())
}

/// Finished agents across the staff roles, with the verdicts and ledger rows a
/// real run leaves: the Agents tab's roster, scorecard and run detail. All
/// ended, so nothing claims to be running; their worktrees do not exist.
fn staff(home: &Path, today: Date) -> Result<()> {
    use cos::agents::watch::{Kind as Flag, Limits};
    use cos::agents::{
        Contract, Expects, Harness, Outcome, Spec, State, Status, Verdict, judge, ledger,
    };

    struct Run {
        role: Option<&'static str>,
        title: &'static str,
        repo: Option<&'static str>,
        source: &'static str,
        status: Status,
        outcome: Outcome,
        reasons: &'static [&'static str],
        changed: &'static [&'static str],
        out_of_scope: &'static [&'static str],
        checks: &'static [(&'static str, Option<i32>, &'static str)],
        expects: Expects,
        output: Option<&'static str>,
        done_when: &'static str,
        finish_line: Option<&'static str>,
        cost: f64,
        mins: (u8, u8, u8),
        tools: u32,
        flagged: &'static [Flag],
        model: Option<&'static str>,
    }
    let runs = [
        Run {
            role: Some("builder"),
            title: "Rate-limit the public token endpoint",
            repo: Some("api-gateway"),
            source: "deliverable D-1",
            status: Status::Done,
            outcome: Outcome::Accepted,
            reasons: &[],
            changed: &["src/limits.rs", "tests/limits.rs"],
            out_of_scope: &[],
            checks: &[(
                "task test",
                Some(0),
                "test result: ok. 214 passed; 0 failed",
            )],
            expects: Expects::Diff,
            output: None,
            done_when: "Burst limit enforced per token, with tests",
            finish_line: None,
            cost: 1.84,
            mins: (9, 2, 16),
            tools: 63,
            flagged: &[],
            model: None,
        },
        Run {
            role: Some("reviewer"),
            title: "Review api-gateway#412 (Release 3.4)",
            repo: Some("api-gateway"),
            source: "deliverable D-1",
            status: Status::Done,
            outcome: Outcome::Accepted,
            reasons: &[],
            changed: &["REVIEW.md"],
            out_of_scope: &[],
            checks: &[("test -s REVIEW.md", Some(0), "")],
            expects: Expects::File,
            output: Some("REVIEW.md"),
            done_when: "Review submitted with a verdict and blocking findings listed",
            finish_line: None,
            cost: 0.91,
            mins: (9, 20, 26),
            tools: 31,
            flagged: &[],
            model: Some("sonnet"),
        },
        Run {
            role: Some("researcher"),
            title: "One page: gateway 3.4 migration risks",
            repo: None,
            source: "brief goals #1",
            status: Status::Done,
            outcome: Outcome::Accepted,
            reasons: &[],
            changed: &[],
            out_of_scope: &[],
            checks: &[("test -s DRAFT.md", Some(0), "")],
            expects: Expects::File,
            output: Some("DRAFT.md"),
            done_when: "DRAFT.md is a one-page draft with sources.",
            finish_line: None,
            cost: 0.62,
            mins: (10, 5, 9),
            tools: 18,
            flagged: &[],
            model: None,
        },
        Run {
            role: Some("builder"),
            title: "Cut the mobile 2.0 beta branch",
            repo: Some("mobile"),
            source: "deliverable D-2",
            status: Status::Done,
            outcome: Outcome::Rejected,
            reasons: &["check failed: `task test` (exit 101)"],
            changed: &["app/build.gradle", "app/src/main/Config.kt"],
            out_of_scope: &[],
            checks: &[(
                "task test",
                Some(101),
                "test config::beta_flags ... FAILED\nfailures:\n    config::beta_flags\ntest result: FAILED. 88 passed; 1 failed",
            )],
            expects: Expects::Diff,
            output: None,
            done_when: "Beta branch builds green and TestFlight has a build",
            finish_line: None,
            cost: 2.40,
            mins: (10, 30, 58),
            tools: 112,
            flagged: &[Flag::Looping],
            model: None,
        },
        Run {
            role: Some("investigator"),
            title: "Where does the gateway mint refresh tokens?",
            repo: Some("api-gateway"),
            source: "prompt",
            status: Status::Done,
            outcome: Outcome::Rejected,
            reasons: &["hired to change nothing, changed 1 file(s): src/auth/refresh.rs"],
            changed: &["ANSWER.md", "src/auth/refresh.rs"],
            out_of_scope: &[],
            checks: &[("test -s ANSWER.md", Some(0), "")],
            expects: Expects::Nothing,
            output: Some("ANSWER.md"),
            done_when: "ANSWER.md answers the question with file:line anchors.",
            finish_line: None,
            cost: 0.35,
            mins: (11, 0, 7),
            tools: 22,
            flagged: &[],
            model: Some("haiku"),
        },
        Run {
            role: Some("docs-keeper"),
            title: "Bring the infra runbooks in line with 3.4",
            repo: Some("infra"),
            source: "prompt",
            status: Status::Done,
            outcome: Outcome::NeedsYou,
            reasons: &["1 file(s) changed outside its scope: scripts/rotate.sh"],
            changed: &["docs/ops/rotation.md", "scripts/rotate.sh"],
            out_of_scope: &["scripts/rotate.sh"],
            checks: &[("task docs:check", Some(0), "0 broken links")],
            expects: Expects::Diff,
            output: None,
            done_when: "The docs describe what shipped, and nothing outside docs changed.",
            finish_line: None,
            cost: 0.77,
            mins: (11, 20, 33),
            tools: 40,
            flagged: &[Flag::Stalled],
            model: None,
        },
        Run {
            role: Some("release-clerk"),
            title: "Draft the 3.4 release notes",
            repo: Some("api-gateway"),
            source: "prompt",
            status: Status::Killed,
            outcome: Outcome::Rejected,
            reasons: &["the agent ended killed: stopped from the window"],
            changed: &["CHANGELOG.md"],
            out_of_scope: &[],
            checks: &[],
            expects: Expects::Diff,
            output: None,
            done_when: "The notes and the checklist are drafted for you to read.",
            finish_line: None,
            cost: 0.18,
            mins: (12, 0, 4),
            tools: 9,
            flagged: &[],
            model: None,
        },
        Run {
            role: None,
            title: "Answer Sam on the migration window",
            repo: Some("infra"),
            source: "brief #2",
            status: Status::Done,
            outcome: Outcome::NeedsYou,
            reasons: &["no checks in the contract: nothing proves it is done"],
            changed: &["notes/window.md"],
            out_of_scope: &[],
            checks: &[],
            expects: Expects::Diff,
            output: None,
            done_when: "",
            finish_line: None,
            cost: 0.44,
            mins: (8, 0, 11),
            tools: 14,
            flagged: &[],
            model: None,
        },
        Run {
            role: Some("tester"),
            title: "API-77: Smoke-test gateway 3.4 against the staging mobile build",
            repo: Some("api-gateway"),
            source: "deliverable D-5",
            status: Status::Done,
            outcome: Outcome::Accepted,
            reasons: &[],
            changed: &["TEST-PLAN.md"],
            out_of_scope: &[],
            checks: &[
                ("test -s TEST-PLAN.md", Some(0), ""),
                ("grep -qi evidence TEST-PLAN.md", Some(0), ""),
            ],
            expects: Expects::File,
            output: Some("TEST-PLAN.md"),
            done_when: "A draft ready for you to resolve API-77: `TEST-PLAN.md`, with its evidence. The outward step stays with you.",
            finish_line: Some("API-77 resolved in Jira"),
            cost: 0.71,
            mins: (12, 30, 41),
            tools: 26,
            flagged: &[],
            model: None,
        },
    ];
    let root = cos::agents::root(home);
    let at = |h: u8, m: u8| {
        let t = u32::from(h) * 60 + u32::from(m);
        format!("{today}T{:02}:{:02}:00Z", t / 60, t % 60)
    };
    for (i, r) in runs.iter().enumerate() {
        let (h, m0, m1) = r.mins;
        let id = format!("{}-{:02}{:02}00-d{i:03x}", today.strftime("%Y%m%d"), h, m0);
        let dir = root.join(&id);
        std::fs::create_dir_all(&dir)?;
        let repo = r.repo.map(|n| home.join("projects").join(n));
        let workdir = match r.repo {
            Some(_) => dir.join("tree"),
            None => dir.join("scratch"),
        };
        let spec = Spec {
            id: id.clone(),
            harness: Harness::Claude,
            program: "claude".into(),
            model: r.model.map(Into::into),
            repo,
            base: r
                .repo
                .map(|_| "4f1c2a9e0b7d3c6a5e8f9d0c1b2a3e4f5d6c7b8a".to_owned()),
            workdir,
            worktree: r.repo.is_some(),
            title: r.title.into(),
            source: r.source.into(),
            prompt: format!("Role: {}. {}", r.role.unwrap_or("none"), r.title),
            deadline_secs: 1800,
            budget_usd: 5.0,
            created: at(h, m0),
            slots: Some(3),
            contract: Contract {
                done_when: (!r.done_when.is_empty()).then(|| r.done_when.to_owned()),
                checks: r.checks.iter().map(|c| c.0.to_owned()).collect(),
                scope: Vec::new(),
                expects: r.expects,
                output: r.output.map(Into::into),
                finish_line: r.finish_line.map(Into::into),
            },
            limits: Limits::default(),
            role: r.role.map(Into::into),
        };
        let verdict = Verdict {
            outcome: r.outcome,
            reasons: r.reasons.iter().map(|s| s.to_string()).collect(),
            changed: r.changed.iter().map(|s| s.to_string()).collect(),
            out_of_scope: r.out_of_scope.iter().map(|s| s.to_string()).collect(),
            checks: r
                .checks
                .iter()
                .map(|(cmd, exit, tail)| judge::Check {
                    cmd: cmd.to_string(),
                    passed: *exit == Some(0),
                    exit: *exit,
                    tail: tail.to_string(),
                })
                .collect(),
            overclaimed: r.status == Status::Done && r.outcome == Outcome::Rejected,
            judged: at(h, m1 + 1),
        };
        let state = State {
            status: r.status,
            started: Some(at(h, m0)),
            ended: Some(at(h, m1)),
            exit: Some(if r.status == Status::Done { 0 } else { 143 }),
            cost_usd: Some(r.cost),
            tokens: Some(u64::from(r.tools) * 2_150),
            tool_calls: r.tools,
            session: Some(format!("demo-session-{i}")),
            last: "result: success".into(),
            summary: "Done. Every check passes and the change is ready for review.".into(),
            seeded: vec!["linked target/ from the repo (clonefile)".into()],
            shimmed: vec!["cargo".into()],
            verdict: Some(verdict),
            flagged: r.flagged.to_vec(),
            recorded: (r.role == Some("tester")).then(|| cos::agents::Recorded {
                outcome: cos::agents::Tested::Passed,
                notes: "Ran the script against staging with mobile build 2.0.0-beta.3: 12 of 12 steps passed, token refresh under 200 ms.".into(),
                evidence: vec!["https://northwind.example/ci/runs/4821".into()],
                at: at(h, m1 + 20),
            }),
            ..State::default()
        };
        std::fs::write(dir.join("spec.json"), serde_json::to_vec_pretty(&spec)?)?;
        std::fs::write(dir.join("state.json"), serde_json::to_vec_pretty(&state)?)?;
        let (look, run) = match r.output {
            Some(out) => ("README.md", format!("test -s {out}")),
            None => ("src/lib.rs", "task test".to_owned()),
        };
        let call = |n: u8, name: &str, input: serde_json::Value| json!({"type": "assistant", "message": {"content": [{"type": "tool_use", "id": format!("d{i}-{n}"), "name": name, "input": input}]}});
        let answer = |n: u8, error: bool, text: &str| json!({"type": "user", "message": {"content": [{"type": "tool_result", "tool_use_id": format!("d{i}-{n}"), "is_error": error, "content": text}]}});
        let say = |text: String| json!({"type": "assistant", "message": {"content": [{"type": "text", "text": text}]}});
        let events = [
            json!({"type": "system", "subtype": "init", "session_id": format!("demo-session-{i}"), "model": "claude-demo"}),
            say(format!("Starting on: {}", r.title)),
            call(
                1,
                "Bash",
                json!({"command": "git status --short; git log --oneline -3", "description": "Where the worktree stands"}),
            ),
            answer(1, false, "4f1c2a9 Release 3.4 candidate"),
            call(2, "Read", json!({"file_path": look})),
            answer(2, false, "api-gateway (fictional)"),
            call(
                3,
                "Grep",
                json!({"pattern": "refresh_token", "path": "src/"}),
            ),
            answer(3, false, "src/auth/refresh.rs:42"),
            call(
                4,
                "Bash",
                json!({"command": "curl -sI https://staging.northwind.example/health"}),
            ),
            answer(
                4,
                true,
                "curl: (6) Could not resolve host: the sandbox has no network",
            ),
            call(5, "Bash", json!({"command": run})),
            answer(5, false, ""),
            say("Done. Every check passes and the change is ready for review.".into()),
            json!({"type": "result", "subtype": "success", "is_error": false, "result": "Done.", "total_cost_usd": r.cost, "num_turns": 7}),
        ];
        let lines: Vec<String> = events.iter().map(|e| e.to_string()).collect();
        std::fs::write(dir.join("events.jsonl"), lines.join("\n") + "\n")?;
        if let Some(row) = ledger::row(&spec, &state) {
            ledger::append(home, &row)?;
        }
    }
    // Agents cleaned long ago: in the ledger only, so the scorecard has a history.
    for (k, (role, outcome, cost, secs, model)) in [
        (Some("builder"), Outcome::Accepted, 1.52, 780, None),
        (Some("builder"), Outcome::Accepted, 2.05, 1140, None),
        (Some("builder"), Outcome::NeedsYou, 1.10, 640, Some("opus")),
        (
            Some("reviewer"),
            Outcome::Accepted,
            0.84,
            410,
            Some("sonnet"),
        ),
        (
            Some("reviewer"),
            Outcome::NeedsYou,
            0.97,
            520,
            Some("sonnet"),
        ),
        (Some("researcher"), Outcome::Accepted, 0.58, 300, None),
        (None, Outcome::Accepted, 0.66, 600, None),
        (None, Outcome::Rejected, 0.39, 200, None),
    ]
    .into_iter()
    .enumerate()
    {
        ledger::append(
            home,
            &ledger::Row {
                id: format!(
                    "{}-080000-h{k:03x}",
                    today
                        .checked_sub(Span::new().days(3 + k as i64))?
                        .strftime("%Y%m%d")
                ),
                judged: at(8, 30),
                harness: Harness::Claude,
                model: model.map(Into::into),
                repo: (role != Some("researcher")).then(|| "api-gateway".to_owned()),
                kind: "deliverable".into(),
                status: Status::Done,
                outcome,
                overclaimed: false,
                checks: 1,
                cost_usd: Some(cost),
                secs: Some(secs),
                tool_calls: 40,
                flagged: Vec::new(),
                role: role.map(Into::into),
            },
        )?;
    }
    Ok(())
}
