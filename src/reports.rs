//! The two structured outputs, shaped by the starter kit's review and strategy
//! references: a brief is at most three actions that need the user and two
//! verified changes, with the detail kept behind it; a review steers each goal.
//! Each struct has its JSON Schema beside it; the schema is what
//! `--json-schema` enforces, the struct is what Rust reads back.
//! `tests/reports.rs` keeps the pair in step.
//!
//! Runs saved before this shape still load: each struct deserializes through
//! a wire type that folds the old fields (`must_do`, `celebrations`, `bets`, …)
//! into the new ones.

use std::fmt::Write as _;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// The brief's hard limits, from the kit: three actions, two changes.
pub const MAX_ACTIONS: usize = 3;
pub const MAX_CHANGES: usize = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(from = "BriefWire")]
pub struct Brief {
    pub headline: String,
    /// At most three, each led by the recommendation.
    pub needs_you: Vec<Action>,
    /// At most two material, verified changes; empty when there are none.
    pub what_changed: Vec<Change>,
    pub calendar: Vec<Slot>,
    /// Smaller follow-ups kept with an owner and a review trigger.
    pub backlog: Vec<Followup>,
    /// What each source actually covered. Health and completeness differ.
    pub coverage: Vec<Coverage>,
    /// Anything not read that is not a source: missing context, a skipped step.
    pub gaps: Vec<String>,
    pub upcoming: Vec<Milestone>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Slot {
    /// `YYYY-MM-DD`, local; empty in briefs saved before it existed.
    #[serde(default)]
    pub date: String,
    /// `HH:MM`, 24h, local; empty for all-day.
    #[serde(default)]
    pub start: String,
    #[serde(default)]
    pub end: String,
    pub what: String,
    pub prep: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Verb {
    Decide,
    Do,
    Delegate,
    Batch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Basis {
    Confirmed,
    Proposed,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Action {
    pub verb: Verb,
    /// Who acts: the user, or the proposed delegate for `delegate`.
    pub owner: String,
    pub recommendation: String,
    /// The business consequence.
    pub why: String,
    pub next_step: String,
    pub source: String,
    /// `YYYY-MM-DD`, or empty when there is no real deadline.
    pub deadline: String,
    pub deadline_basis: Basis,
    /// The confirmed goal it serves, or empty for an essential obligation.
    pub goal_id: String,
    /// The tracked deliverable it advances, if any.
    pub deliverable_id: String,
    /// A self-contained handoff a coding agent can run.
    pub agent_prompt: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    Done,
    Unblocked,
    Risk,
    TimeChanged,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Change {
    pub kind: ChangeKind,
    pub what: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Followup {
    pub item: String,
    pub owner: String,
    pub source: String,
    /// What brings it back: a date, a checkpoint, an event.
    pub next_review: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Health {
    NotConnected,
    Healthy,
    RateLimited,
    PermissionDenied,
    AuthFailed,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Completeness {
    NotReviewed,
    Partial,
    Complete,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Coverage {
    /// The profile's source id when there is one (`email`), else the service.
    pub source: String,
    pub health: Health,
    pub review: Completeness,
    /// ISO-8601 with offset, or empty.
    pub window_start: String,
    pub window_end: String,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Milestone {
    /// `YYYY-MM-DD`.
    pub date: String,
    pub what: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(from = "ReviewWire")]
pub struct Review {
    pub summary: String,
    /// One per confirmed goal.
    pub goals: Vec<GoalCheck>,
    /// Deliverables worked back from the goals; proposals, not commitments.
    pub proposed: Vec<Proposal>,
    pub decisions: Vec<Decision>,
    pub stop_doing: Vec<String>,
    pub what_changed: Vec<Change>,
    pub gaps: Vec<String>,
    pub upcoming: Vec<Milestone>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Steer {
    Maintain,
    Sequence,
    ReduceScope,
    Test,
    Delegate,
    Stop,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoalCheck {
    /// The state's goal id, or empty for a goal not yet tracked.
    pub goal_id: String,
    pub goal: String,
    pub confidence: Confidence,
    /// Progress against the KPI, with its window; "unknown" when unmeasured.
    pub kpi: String,
    /// What gained evidence this week.
    pub evidence: String,
    /// The assumption that weakened, if any.
    pub weakened: String,
    pub bottleneck: String,
    pub recommendation: Steer,
    pub next: String,
    /// What new evidence would change the recommendation.
    pub would_change: String,
    pub agent_prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proposal {
    pub goal_id: String,
    pub title: String,
    pub owner: String,
    pub owner_basis: OwnerBasis,
    /// `YYYY-MM-DD`, or empty.
    pub due_date: String,
    pub due_basis: OwnerBasis,
    pub done_when: String,
    pub blocker: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OwnerBasis {
    Proposed,
    Confirmed,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    Rock,
    Sand,
    Water,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decision {
    pub decision: String,
    pub options: Vec<String>,
    pub recommendation: String,
}

#[derive(Deserialize)]
struct BriefWire {
    #[serde(default)]
    headline: String,
    #[serde(default)]
    needs_you: Vec<Action>,
    #[serde(default)]
    what_changed: Vec<Change>,
    #[serde(default)]
    calendar: Vec<Slot>,
    #[serde(default)]
    backlog: Vec<Followup>,
    #[serde(default)]
    coverage: Vec<Coverage>,
    #[serde(default)]
    gaps: Vec<String>,
    #[serde(default)]
    upcoming: Vec<Milestone>,
    #[serde(default)]
    must_do: Vec<OldAction>,
    #[serde(default)]
    reply_to: Vec<OldMail>,
    #[serde(default)]
    blockers: Vec<OldBlocker>,
    #[serde(default)]
    can_drop: Vec<String>,
    #[serde(default)]
    celebrations: Vec<OldWin>,
}

#[derive(Deserialize)]
struct OldAction {
    item: String,
    why: String,
    source: String,
    #[serde(default)]
    agent_prompt: String,
}

#[derive(Deserialize)]
struct OldMail {
    from: String,
    subject: String,
    ask: String,
}

#[derive(Deserialize)]
struct OldBlocker {
    item: String,
    waiting_on: String,
    unblock: String,
}

#[derive(Deserialize)]
struct OldWin {
    win: String,
    why_it_matters: String,
    source: String,
}

impl From<OldWin> for Change {
    fn from(w: OldWin) -> Self {
        Change {
            kind: ChangeKind::Done,
            what: format!("{} — {}", w.win, w.why_it_matters),
            source: w.source,
        }
    }
}

impl From<BriefWire> for Brief {
    fn from(w: BriefWire) -> Self {
        let mut needs_you = w.needs_you;
        needs_you.extend(w.must_do.into_iter().map(|a| Action {
            verb: Verb::Do,
            owner: String::new(),
            recommendation: a.item,
            why: a.why,
            next_step: String::new(),
            source: a.source,
            deadline: String::new(),
            deadline_basis: Basis::None,
            goal_id: String::new(),
            deliverable_id: String::new(),
            agent_prompt: a.agent_prompt,
        }));
        let mut what_changed = w.what_changed;
        what_changed.extend(w.celebrations.into_iter().map(Change::from));
        let mut backlog = w.backlog;
        backlog.extend(w.reply_to.into_iter().map(|m| Followup {
            item: format!("Reply: {}", m.subject),
            owner: m.from,
            source: "mail".into(),
            next_review: m.ask,
        }));
        backlog.extend(w.blockers.into_iter().map(|b| Followup {
            item: b.item,
            owner: b.waiting_on,
            source: String::new(),
            next_review: b.unblock,
        }));
        backlog.extend(w.can_drop.into_iter().map(|d| Followup {
            item: d,
            owner: String::new(),
            source: String::new(),
            next_review: "can wait past this week".into(),
        }));
        Brief {
            headline: w.headline,
            needs_you,
            what_changed,
            calendar: w.calendar,
            backlog,
            coverage: w.coverage,
            gaps: w.gaps,
            upcoming: w.upcoming,
        }
    }
}

#[derive(Deserialize)]
struct ReviewWire {
    #[serde(default)]
    summary: String,
    #[serde(default)]
    goals: Vec<GoalCheck>,
    #[serde(default)]
    proposed: Vec<Proposal>,
    #[serde(default)]
    decisions: Vec<Decision>,
    #[serde(default)]
    stop_doing: Vec<String>,
    #[serde(default)]
    what_changed: Vec<Change>,
    #[serde(default)]
    gaps: Vec<String>,
    #[serde(default)]
    upcoming: Vec<Milestone>,
    #[serde(default)]
    bets: Vec<OldBet>,
    #[serde(default)]
    drift: Vec<String>,
    #[serde(default)]
    celebrations: Vec<OldWin>,
}

#[derive(Deserialize)]
struct OldBet {
    bet: String,
    confidence: Confidence,
    evidence: String,
    next: String,
    #[serde(default)]
    agent_prompt: String,
}

impl From<ReviewWire> for Review {
    fn from(w: ReviewWire) -> Self {
        let mut goals = w.goals;
        goals.extend(w.bets.into_iter().map(|b| GoalCheck {
            goal_id: String::new(),
            goal: b.bet,
            confidence: b.confidence,
            kpi: "unknown".into(),
            evidence: b.evidence,
            weakened: String::new(),
            bottleneck: String::new(),
            recommendation: Steer::Maintain,
            next: b.next,
            would_change: String::new(),
            agent_prompt: b.agent_prompt,
        }));
        let mut stop_doing = w.stop_doing;
        stop_doing.extend(w.drift.into_iter().map(|d| format!("Drift: {d}")));
        let mut what_changed = w.what_changed;
        what_changed.extend(w.celebrations.into_iter().map(Change::from));
        Review {
            summary: w.summary,
            goals,
            proposed: w.proposed,
            decisions: w.decisions,
            stop_doing,
            what_changed,
            gaps: w.gaps,
            upcoming: w.upcoming,
        }
    }
}

fn obj(props: Value) -> Value {
    let required: Vec<_> = props
        .as_object()
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default();
    json!({ "type": "object", "properties": props, "required": required, "additionalProperties": false })
}

fn list(item: Value) -> Value {
    json!({ "type": "array", "items": item })
}

fn capped(item: Value, max: usize) -> Value {
    json!({ "type": "array", "items": item, "maxItems": max })
}

fn str_() -> Value {
    json!({ "type": "string" })
}

fn one_of(values: &[&str]) -> Value {
    json!({ "type": "string", "enum": values })
}

fn changes() -> Value {
    capped(
        obj(json!({
            "kind": one_of(&["done", "unblocked", "risk", "time_changed"]),
            "what": str_(),
            "source": str_(),
        })),
        MAX_CHANGES,
    )
}

fn milestones() -> Value {
    list(obj(
        json!({ "date": str_(), "what": str_(), "source": str_() }),
    ))
}

pub fn brief_schema() -> Value {
    obj(json!({
        "headline": str_(),
        "needs_you": capped(obj(json!({
            "verb": one_of(&["decide", "do", "delegate", "batch"]),
            "owner": str_(),
            "recommendation": str_(),
            "why": str_(),
            "next_step": str_(),
            "source": str_(),
            "deadline": str_(),
            "deadline_basis": one_of(&["confirmed", "proposed", "none"]),
            "goal_id": str_(),
            "deliverable_id": str_(),
            "agent_prompt": str_(),
        })), MAX_ACTIONS),
        "what_changed": changes(),
        "calendar": list(obj(json!({ "date": str_(), "start": str_(), "end": str_(), "what": str_(), "prep": str_() }))),
        "backlog": list(obj(json!({ "item": str_(), "owner": str_(), "source": str_(), "next_review": str_() }))),
        "coverage": list(obj(json!({
            "source": str_(),
            "health": one_of(&crate::state::HEALTH),
            "review": one_of(&crate::state::REVIEW),
            "window_start": str_(),
            "window_end": str_(),
            "note": str_(),
        }))),
        "gaps": list(str_()),
        "upcoming": milestones(),
    }))
}

pub fn review_schema() -> Value {
    let basis = one_of(&["proposed", "confirmed", "unknown"]);
    obj(json!({
        "summary": str_(),
        "goals": list(obj(json!({
            "goal_id": str_(),
            "goal": str_(),
            "confidence": one_of(&["rock", "sand", "water"]),
            "kpi": str_(),
            "evidence": str_(),
            "weakened": str_(),
            "bottleneck": str_(),
            "recommendation": one_of(&["maintain", "sequence", "reduce_scope", "test", "delegate", "stop"]),
            "next": str_(),
            "would_change": str_(),
            "agent_prompt": str_(),
        }))),
        "proposed": list(obj(json!({
            "goal_id": str_(),
            "title": str_(),
            "owner": str_(),
            "owner_basis": basis,
            "due_date": str_(),
            "due_basis": basis,
            "done_when": str_(),
            "blocker": str_(),
        }))),
        "decisions": list(obj(json!({ "decision": str_(), "options": list(str_()), "recommendation": str_() }))),
        "stop_doing": list(str_()),
        "what_changed": changes(),
        "gaps": list(str_()),
        "upcoming": milestones(),
    }))
}

/// The chief's routing decision for `cos ask`.
#[derive(Debug, Serialize, Deserialize)]
pub struct Route {
    pub brains: Vec<BrainName>,
    /// What each brain should answer, restated so it stands alone.
    pub framing: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BrainName {
    Tactician,
    Strategist,
}

impl BrainName {
    pub fn agent(self) -> &'static str {
        match self {
            Self::Tactician => "tactician",
            Self::Strategist => "strategist",
        }
    }

    pub fn model(self) -> &'static str {
        match self {
            Self::Tactician => "sonnet",
            Self::Strategist => "opus",
        }
    }
}

pub fn route_schema() -> Value {
    obj(json!({
        "brains": {
            "type": "array",
            "items": { "type": "string", "enum": ["tactician", "strategist"] },
            "minItems": 1,
            "maxItems": 2,
            "uniqueItems": true,
        },
        "framing": str_(),
    }))
}

impl std::fmt::Display for Verb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Decide => "DECIDE",
            Self::Do => "DO",
            Self::Delegate => "DELEGATE",
            Self::Batch => "BATCH",
        })
    }
}

impl std::fmt::Display for ChangeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Done => "DONE",
            Self::Unblocked => "UNBLOCKED",
            Self::Risk => "RISK",
            Self::TimeChanged => "TIME CHANGED",
        })
    }
}

impl std::fmt::Display for Confidence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Rock => "rock",
            Self::Sand => "sand",
            Self::Water => "water",
        })
    }
}

impl std::fmt::Display for Steer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Maintain => "maintain",
            Self::Sequence => "sequence differently",
            Self::ReduceScope => "reduce scope",
            Self::Test => "test",
            Self::Delegate => "delegate",
            Self::Stop => "stop",
        })
    }
}

impl Action {
    /// `DELEGATE: Morgan`, `DO`, …
    pub fn label(&self) -> String {
        match self.verb {
            Verb::Delegate if !self.owner.is_empty() => format!("DELEGATE: {}", self.owner),
            v => v.to_string(),
        }
    }

    /// The deadline with its basis, or nothing when there is none.
    pub fn timing(&self) -> String {
        match (self.deadline.as_str(), self.deadline_basis) {
            ("", _) | (_, Basis::None) => String::new(),
            (d, Basis::Confirmed) => format!("due {d}"),
            (d, Basis::Proposed) => format!("proposed {d}"),
        }
    }
}

impl Coverage {
    pub fn complete(&self) -> bool {
        self.health == Health::Healthy && self.review == Completeness::Complete
    }
}

impl Brief {
    /// The brief as the kit specifies it: Needs you, then What changed, each
    /// only when it has content. The detail (calendar, backlog, coverage)
    /// follows below a rule, for whoever wants it.
    pub fn to_markdown(&self, day: &str) -> String {
        let mut s = format!("# Brief — {day}\n\n{}\n", self.headline);
        section(&mut s, "Needs you", &self.needs_you, |a| {
            let mut line = format!("**{}** {} — {}", a.label(), a.recommendation, a.why);
            if !a.next_step.is_empty() {
                let _ = write!(line, " Next: {}", a.next_step);
            }
            let timing = a.timing();
            if !timing.is_empty() {
                let _ = write!(line, " ({timing})");
            }
            let _ = write!(line, " _{}_", a.source);
            line
        });
        section(&mut s, "What changed", &self.what_changed, |c| {
            format!("**{}** {} _{}_", c.kind, c.what, c.source)
        });
        let partial: Vec<&Coverage> = self.coverage.iter().filter(|c| !c.complete()).collect();
        if self.calendar.is_empty()
            && self.backlog.is_empty()
            && partial.is_empty()
            && self.gaps.is_empty()
            && self.upcoming.is_empty()
        {
            return s;
        }
        s.push_str("\n---\n");
        section(&mut s, "Calendar", &self.calendar, |x| {
            let day = if x.date.is_empty() {
                String::new()
            } else {
                format!("{} ", x.date)
            };
            format!(
                "**{day}{}–{}** {} — prep: {}",
                x.start, x.end, x.what, x.prep
            )
        });
        section(&mut s, "Backlog", &self.backlog, |f| {
            let owner = if f.owner.is_empty() {
                String::new()
            } else {
                format!(" ({})", f.owner)
            };
            format!("{}{owner} — next review: {}", f.item, f.next_review)
        });
        section(&mut s, "Coming up", &self.upcoming, |m| {
            format!("{} {}", m.date, m.what)
        });
        section(&mut s, "Coverage", &partial, |c| {
            format!(
                "{}: {} · {}{}",
                c.source,
                snake(&c.health),
                snake(&c.review),
                if c.note.is_empty() {
                    String::new()
                } else {
                    format!(" — {}", c.note)
                }
            )
        });
        section(&mut s, "Gaps (not read)", &self.gaps, String::clone);
        s
    }
}

impl Review {
    pub fn to_markdown(&self, day: &str) -> String {
        let mut s = format!("# Weekly review — {day}\n\n{}\n", self.summary);
        section(&mut s, "Goals", &self.goals, |g| {
            let mut line = format!(
                "`{}` **{}** — KPI: {}. {} → {}: {}",
                g.confidence, g.goal, g.kpi, g.evidence, g.recommendation, g.next
            );
            if !g.weakened.is_empty() {
                let _ = write!(line, " Weakened: {}.", g.weakened);
            }
            if !g.bottleneck.is_empty() {
                let _ = write!(line, " Bottleneck: {}.", g.bottleneck);
            }
            if !g.would_change.is_empty() {
                let _ = write!(line, " Would change if: {}.", g.would_change);
            }
            line
        });
        section(&mut s, "Proposed this week", &self.proposed, |p| {
            let due = if p.due_date.is_empty() {
                "no date".to_owned()
            } else {
                format!("{} ({})", p.due_date, snake(&p.due_basis))
            };
            format!(
                "{} — owner {} ({}), {due}; done when {}",
                p.title,
                p.owner,
                snake(&p.owner_basis),
                p.done_when
            )
        });
        section(&mut s, "Decisions needed", &self.decisions, |d| {
            format!(
                "{} — options: {} → recommend: {}",
                d.decision,
                d.options.join(" / "),
                d.recommendation
            )
        });
        section(&mut s, "Stop doing", &self.stop_doing, String::clone);
        section(&mut s, "What changed", &self.what_changed, |c| {
            format!("**{}** {} _{}_", c.kind, c.what, c.source)
        });
        section(&mut s, "Coming up", &self.upcoming, |m| {
            format!("{} {}", m.date, m.what)
        });
        section(&mut s, "Gaps (not read)", &self.gaps, String::clone);
        s
    }
}

/// The serde name of a unit enum value: `rate_limited`.
pub fn snake<T: Serialize>(v: &T) -> String {
    serde_json::to_value(v)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

fn section<T>(s: &mut String, title: &str, items: &[T], line: impl Fn(&T) -> String) {
    if items.is_empty() {
        return;
    }
    let _ = writeln!(s, "\n## {title}\n");
    for it in items {
        let _ = writeln!(s, "- {}", line(it));
    }
}
