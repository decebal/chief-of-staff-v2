//! A fully synthetic `COS_HOME` for screenshots and trying `cos` without
//! connecting anything: a fictional person, company, repos and teammates.
//! Nothing here reads your accounts, and every month is pre-marked archived
//! so `cos report` and `cos dash` never call GitHub for it.
//!
//! ```sh
//! cargo run --example demo -- /tmp/cos-demo
//! cos --home /tmp/cos-demo dash
//! ```

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::Result;
use cos::gather::{Facts, Pr, Task};
use cos::history::{DayRow, Merged};
use cos::reports::{Brief, Review};
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

    let brief: Brief = serde_json::from_value(brief_json(today))?;
    let day = today.to_string();
    store::save(&home, Kind::Briefs, &day, &brief, &brief.to_markdown(&day))?;
    let review: Review = serde_json::from_value(review_json(today))?;
    store::save(
        &home,
        Kind::Reviews,
        &day,
        &review,
        &review.to_markdown(&day),
    )?;

    history(&home, today, &facts, &mut rng)?;
    workspace(&home, today)?;

    let report = cos::report::build(&home, &home)?;
    for back in 1..=12 {
        let d = today.checked_sub(Span::new().days(back))?;
        std::fs::copy(&report, home.join(format!("out/reports/{d}.html")))?;
    }
    println!("demo home ready: {}", home.display());
    println!("  cos --home {} dash", home.display());
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
            { "date": in_days(0), "start": "16:00", "end": "17:00", "what": "Release 3.4 go/no-go", "prep": "Have api-gateway#412 reviewed." }
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
        ]
    })
}

fn review_json(today: Date) -> serde_json::Value {
    let in_days = |d: i64| today.checked_add(Span::new().days(d)).unwrap().to_string();
    let stop = "Stop before anything outward-facing and show me the draft.";
    json!({
        "summary": "Mobile 2.0 moved and review wait time did not; the beta now depends on one review.",
        "goals": [
            { "goal_id": "G-1", "goal": "Mobile 2.0 on the new gateway", "confidence": "rock", "kpi": "Beta testers: unknown (enrolment opens with the beta)",
              "evidence": "Auth rewrite merged; 3.4 in review.", "weakened": "", "bottleneck": "One reviewer for api-gateway#412.",
              "recommendation": "maintain", "next": "Ship 3.4, then cut the beta branch (D-2).", "would_change": "A blocking finding in #412.",
              "agent_prompt": format!("Work in ~/Projects/mobile. List what the 2.0 beta still needs from the gateway 3.4 API and draft the beta checklist. {stop}") },
            { "goal_id": "G-2", "goal": "Halve review wait time", "confidence": "sand", "kpi": "Median review wait: 41h against 24h (last 7 days)",
              "evidence": "Queue grew from 5 to 8; two PRs older than three weeks.", "weakened": "That ad-hoc reviews would keep up.", "bottleneck": "No owner for the queue.",
              "recommendation": "sequence", "next": "Start a weekly rotation before any tooling.", "would_change": "Two weeks of rotation without the median falling.",
              "agent_prompt": format!("Using `gh search prs --review-requested=@me --state=open`, chart review age by repo and propose two options for a review rotation. {stop}") },
            { "goal_id": "G-3", "goal": "Design system v3", "confidence": "water", "kpi": "unknown: no adoption measure yet",
              "evidence": "Tokens landed; component scope still undefined.", "weakened": "", "bottleneck": "Scope.",
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
        "stop_doing": [ "Drift: two days on preview-env cost work, which serves no goal.", "Ad-hoc reviews outside the rotation once it starts." ],
        "what_changed": [ { "kind": "done", "what": "Auth rewrite done; Mobile 2.0 moved from sand to rock.", "source": "https://github.com/northwind/api-gateway/pull/411" } ],
        "gaps": [ "G-3 has no measurable KPI." ],
        "upcoming": [ { "date": in_days(8), "what": "Mobile 2.0 beta", "source": "G-1" } ]
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
        "setupConfirmedAt": at(-14, 9)
    });
    cos::state::atomic_write(&ws.join(".chief-of-staff/profile.json"), &profile)?;

    let state = json!({
        "schemaVersion": 1,
        "revision": 0,
        "goals": [
            { "id": "G-1", "title": "Mobile 2.0 on the new gateway", "owner": "Alex", "finishLine": "Beta live for 500 testers on gateway 3.4", "targetDate": day(8), "status": "active" },
            { "id": "G-2", "title": "Halve review wait time", "owner": "Alex", "finishLine": "Median review wait under 24h for two weeks running", "targetDate": day(40), "status": "active" },
            { "id": "G-3", "title": "Design system v3", "owner": "Jordan", "finishLine": "v3 components used by web and mobile", "targetDate": day(29), "status": "active" }
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
              "source": "cos review", "evidenceAt": at(-1, 16), "reviewedAt": null, "meetingAt": null, "completionEvidence": null },
            { "id": "D-4", "goalId": "G-3", "title": "Land design tokens v3", "owner": "Jordan", "ownerBasis": "confirmed", "dueDate": day(-2), "dueBasis": "confirmed", "week": monday,
              "blocker": null, "dependencies": [], "doneWhen": "Tokens published and consumed by web-app", "status": "complete",
              "source": "https://github.com/northwind/design-system/pull/71", "evidenceAt": at(-2, 15), "reviewedAt": null, "meetingAt": null,
              "completionEvidence": { "reference": "https://github.com/northwind/design-system/pull/71", "verifiedAt": at(-2, 16) } }
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
