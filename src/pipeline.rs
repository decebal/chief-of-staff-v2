//! The runs `cos` offers — the daily brief and a question — shared by the CLI
//! and the MCP server. Blocking: each spawns one `claude` child under one
//! deadline.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::claude::{self, Run, Toolset};
use crate::context::{self, Local};
use crate::gather::{self, Facts};
use crate::reports::{self, Brief, Completeness, Coverage, Health, MAX_ACTIONS, MAX_CHANGES};
use crate::store::{self, Kind};
use crate::track::{self, Recorded};
use crate::wake::{self, Wake};

#[derive(Clone)]
pub struct Config {
    /// The clone: `context/`, `out/`, agent files.
    pub home: PathBuf,
    /// Holds `.chief-of-staff/` (profile and state); the kit's `--workspace`.
    pub workspace: PathBuf,
    pub projects: PathBuf,
    /// Overrides each run's default spending cap, in USD.
    pub budget: Option<f64>,
    pub bins: Bins,
}

/// The programs a run starts. `pmset` and `caffeinate` are `None` where
/// there is no dark wake to detect or sleep to hold off.
#[derive(Clone)]
pub struct Bins {
    pub claude: PathBuf,
    pub pmset: Option<PathBuf>,
    pub caffeinate: Option<PathBuf>,
}

impl Default for Bins {
    fn default() -> Self {
        let mac = cfg!(target_os = "macos");
        Self {
            claude: "claude".into(),
            pmset: mac.then(|| "/usr/bin/pmset".into()),
            caffeinate: mac.then(|| "/usr/bin/caffeinate".into()),
        }
    }
}

/// The run did not start: the Mac is in a dark wake and would sleep again
/// mid-run. Nothing was spent.
#[derive(Debug)]
pub struct Deferred(pub &'static str);

impl std::fmt::Display for Deferred {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Mac asleep (dark wake): {} deferred", self.0)
    }
}

impl std::error::Error for Deferred {}

/// A retry needs at least this much of the shared budget left.
const MIN_RETRY: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Ok,
    Failed,
    Deferred,
}

/// What became of a day's brief run, `out/runs/<day>.json`: the Today page
/// says so when today's brief is missing rather than show an older one as
/// current. The last run of the day wins.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunReceipt {
    pub day: String,
    pub at: String,
    pub status: RunStatus,
    #[serde(default)]
    pub reason: String,
    pub attempts: u32,
    pub cost_usd: f64,
}

/// Attempts and spend across one operation's `claude` children.
#[derive(Default)]
struct Tally {
    attempts: u32,
    cost_usd: f64,
}

/// What a run produced, plus what it cost across every `claude` child.
pub struct Done<T> {
    pub value: T,
    pub markdown: String,
    pub cost_usd: f64,
    pub saved: Option<PathBuf>,
    /// Whether the run was written to `.chief-of-staff/state.json`.
    pub tracked: Option<Recorded>,
}

pub struct Dates {
    pub day: String,
    now: String,
    /// `YYYY-MM-DDTHH:MM:00`, local: where a calendar read starts.
    from: String,
    week_ago: String,
    /// Last day a calendar read covers: this Monday-to-Sunday week, plus
    /// tomorrow when it falls outside it (references/review.md).
    pub calendar_end: String,
}

/// Today in the profile's timezone when one is set, else the system's.
fn dates_in(home: &std::path::Path) -> Dates {
    let tz = crate::state::profile(home).and_then(|p| crate::state::timezone(&p));
    dates_at(match tz {
        Some(tz) => jiff::Zoned::now().with_time_zone(tz),
        None => jiff::Zoned::now(),
    })
}

pub fn dates_at(now: jiff::Zoned) -> Dates {
    let today = now.date();
    let to_sunday = 7 - i64::from(today.weekday().to_monday_one_offset());
    let calendar_end = today.saturating_add(jiff::Span::new().days(to_sunday.max(1)));
    Dates {
        day: today.to_string(),
        now: now.strftime("%H:%M %Z").to_string(),
        from: now.strftime("%Y-%m-%dT%H:%M:00").to_string(),
        week_ago: today.saturating_sub(jiff::Span::new().days(7)).to_string(),
        calendar_end: calendar_end.to_string(),
    }
}

impl Dates {
    fn calendar_rule(&self) -> String {
        let after = jiff::civil::Date::strptime("%Y-%m-%d", &self.calendar_end)
            .map(|d| d.saturating_add(jiff::Span::new().days(1)).to_string())
            .unwrap_or_default();
        format!(
            "Calendar window: from now to the end of {end}. Call list_events with startTime \
             {from} and endTime {after}T00:00:00, and give every calendar entry its date. \
             Leave anything outside the window out of the brief.",
            end = self.calendar_end,
            from = self.from,
        )
    }
}

/// Drops calendar entries dated outside today..=`calendar_end`, naming them in
/// gaps. Undated entries stay: briefs saved before `date` existed have none.
pub fn hold_to_window(brief: &mut Brief, d: &Dates) {
    let (kept, out): (Vec<_>, Vec<_>) =
        std::mem::take(&mut brief.calendar)
            .into_iter()
            .partition(|s| {
                s.date.is_empty() || (s.date.as_str() >= d.day.as_str() && s.date <= d.calendar_end)
            });
    brief.calendar = kept;
    if !out.is_empty() {
        brief.gaps.push(format!(
            "cos dropped {} calendar entr{} outside the window {}..{}: {}",
            out.len(),
            if out.len() == 1 { "y" } else { "ies" },
            d.day,
            d.calendar_end,
            out.iter()
                .map(|s| format!("{} {}", s.date, s.what))
                .collect::<Vec<_>>()
                .join("; ")
        ));
    }
}

/// Gives every approved source a coverage row. One the brief left out is
/// `partial` when the run called its tools and `not_reviewed` when it never
/// tried; `cos`'s own reads report what `cos` collected.
pub fn cover_every_source(brief: &mut Brief, home: &Path, facts: &Facts, called: &[String]) {
    let Some(profile) = crate::state::profile(home) else {
        return;
    };
    for src in crate::state::approved_sources(&profile) {
        if brief.coverage.iter().any(|c| c.source == src.id) {
            continue;
        }
        let row = |health, review, note: String| Coverage {
            source: src.id.clone(),
            health,
            review,
            window_start: String::new(),
            window_end: String::new(),
            note,
        };
        let failed = |what: &str| facts.gaps.iter().find(|g| g.contains(what)).cloned();
        let coverage = match context::local_kind(&src.service) {
            Some(Local::Code) => match failed("gh ") {
                Some(e) => row(Health::Unavailable, Completeness::NotReviewed, e),
                None => row(
                    Health::Healthy,
                    Completeness::Partial,
                    format!(
                        "Collected by cos: {} open, {} awaiting review, {} merged since {}.",
                        facts.mine_open.len(),
                        facts.review_requested.len(),
                        facts.merged.len(),
                        facts.merged_since
                    ),
                ),
            },
            Some(Local::Tasks) => match failed("cn ready") {
                Some(e) => row(Health::Unavailable, Completeness::NotReviewed, e),
                None => row(
                    Health::Healthy,
                    Completeness::Partial,
                    format!("Collected by cos: {} ready tasks.", facts.tasks.len()),
                ),
            },
            None => {
                let tools = context::service_tools(&src.service);
                let used: Vec<&str> = called
                    .iter()
                    .map(String::as_str)
                    .filter(|c| tools.contains(c))
                    .collect();
                if used.is_empty() {
                    row(
                        Health::Unavailable,
                        Completeness::NotReviewed,
                        "Not attempted: the run made no call to this source, and the brief did \
                         not report it."
                            .into(),
                    )
                } else {
                    row(
                        Health::Healthy,
                        Completeness::Partial,
                        format!(
                            "The run called {} but the brief did not report coverage; treat as \
                             partial.",
                            used.join(", ")
                        ),
                    )
                }
            }
        };
        brief.gaps.push(format!(
            "The brief left approved source `{}` out of coverage; cos added it as {}.",
            src.id,
            crate::reports::snake(&coverage.review)
        ));
        brief.coverage.push(coverage);
    }
}

/// Holds a brief to the kit's limits. Anything cut is named in gaps, so the
/// limit is visible rather than silent.
fn hold_to_limits(brief: &mut Brief) {
    if brief.needs_you.len() > MAX_ACTIONS {
        let cut = brief.needs_you.split_off(MAX_ACTIONS);
        brief.gaps.push(format!(
            "cos cut {} action(s) over the limit of {MAX_ACTIONS}: {}",
            cut.len(),
            cut.iter()
                .map(|a| a.recommendation.as_str())
                .collect::<Vec<_>>()
                .join("; ")
        ));
    }
    if brief.what_changed.len() > MAX_CHANGES {
        brief.what_changed.truncate(MAX_CHANGES);
    }
}

/// The model's own reason when a run ended in an error result, else the
/// whole chain.
fn failure_reason(e: &anyhow::Error) -> String {
    match e.chain().find_map(|c| c.downcast_ref::<claude::Failed>()) {
        Some(f) => f
            .reason
            .strip_prefix("claude run failed: ")
            .unwrap_or(&f.reason)
            .to_owned(),
        None => format!("{e:#}"),
    }
}

fn report_tracking(r: Result<Recorded>) -> Option<Recorded> {
    match r {
        Ok(r) => Some(r),
        Err(e) => {
            eprintln!("cos: run not tracked: {e:#}");
            None
        }
    }
}

fn facts_block(facts: &Facts) -> String {
    format!(
        "Local facts gathered by cos (already read, do not re-fetch):\n\n{}",
        facts.render()
    )
}

/// Everything a brain reads that is not a connector: `context/`, then facts.
/// The profile's `jiraProjects` turned into one bounded Jira read for the
/// brief's `tickets`; nothing when the profile names none.
pub fn tickets_rule(workspace: &Path) -> String {
    let projects: Vec<String> = crate::state::profile(workspace)
        .and_then(|p| p["jiraProjects"].as_array().cloned())
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .filter(|k| !k.is_empty() && k.chars().all(|c| c.is_ascii_alphanumeric()))
        .collect();
    if projects.is_empty() {
        return String::new();
    }
    format!(
        " Fill `tickets` from one Jira search, `assignee = currentUser() AND project in ({}) \
         AND statusCategory != Done`, if Jira is an approved source: every result with its key, \
         summary, status, priority and URL, and goal_id when a workspace goal's `jira` list \
         names the key. When Jira was not read, leave `tickets` empty and say so in coverage.",
        projects.join(", ")
    )
}

fn grounding(home: &Path, workspace: &Path, facts: &Facts) -> String {
    format!(
        "{}\n\n{}",
        crate::context::block(home, workspace),
        facts_block(facts)
    )
}

impl Config {
    /// Collects local facts once and keeps them, so the report charts the same
    /// numbers the brain was shown.
    pub fn gather(&self) -> Result<Facts> {
        let d = dates_in(&self.workspace);
        let facts = gather::collect_for(
            &self.projects,
            &d.week_ago,
            context::local_reads(&self.workspace),
            &context::work_orgs(&self.workspace),
        );
        store::save_json(&self.home, Kind::Facts, &d.day, &facts)?;
        Ok(facts)
    }

    /// The one daily run: goals, then this week's deliverables, then today.
    /// The default cap carries the goal judgement's old $5.00 opus ceiling at
    /// sonnet's price, about half per token, plus headroom for today's part;
    /// both parts share one set of connector reads.
    pub fn brief_run(&self, facts: &Facts) -> Run<'static> {
        let d = dates_in(&self.workspace);
        let prompt = format!(
            "Today is {}, now {}. Write today's brief, working down from the goals. First the \
             week: each confirmed goal in the workspace (or context/goals.md when there is no \
             workspace) judged on this week's evidence (work merged since {}, this \
             Monday-to-Sunday calendar, tickets, mail), with its steer, the deliverables this \
             week should hold worked back from its finish line, decisions only the user can \
             take, and what to stop doing. Then today, set against that week: at most \
             {MAX_ACTIONS} actions that need the user, each citing the goal and deliverable it \
             serves when there is one, and at most {MAX_CHANGES} verified changes. Meetings that \
             have already ended go nowhere but a one-line note; prep only for what is still \
             ahead. {}{}\n\n{}",
            d.day,
            d.now,
            d.week_ago,
            d.calendar_rule(),
            tickets_rule(&self.workspace),
            grounding(&self.home, &self.workspace, facts)
        );
        Run {
            json_schema: Some(reports::brief_schema()),
            ..self.brain("tactician", "sonnet", prompt, self.budget.unwrap_or(3.0))
        }
    }

    /// A question answered by the tactician on the same grounding as a brief,
    /// with facts read fresh and not saved.
    pub fn ask_run(&self, question: &str) -> Run<'static> {
        let d = dates_in(&self.workspace);
        let facts = gather::collect_for(
            &self.projects,
            &d.week_ago,
            context::local_reads(&self.workspace),
            &context::work_orgs(&self.workspace),
        );
        let prompt = format!(
            "Today is {}, now {}. Answer this question under your Answering rules; do not write \
             a brief.\n\nQuestion: {question}\n\n{}",
            d.day,
            d.now,
            grounding(&self.home, &self.workspace, &facts)
        );
        self.brain("tactician", "sonnet", prompt, self.budget.unwrap_or(2.0))
    }

    /// The daily brief, when the Mac is fully awake, under a sleep assertion.
    /// Every outcome, deferred and failed included, leaves a `RunReceipt`.
    pub fn brief(&self) -> Result<Done<Brief>> {
        let d = dates_in(&self.workspace);
        let invoked = crate::state::now();
        let mut tally = Tally::default();
        let result = self
            .awake("brief")
            .and_then(|_hold| self.brief_awake(&d, &invoked, &mut tally));
        let (status, reason) = match &result {
            Ok(_) => (RunStatus::Ok, String::new()),
            Err(e) if e.is::<Deferred>() => (RunStatus::Deferred, e.to_string()),
            Err(e) => (RunStatus::Failed, failure_reason(e)),
        };
        let receipt = RunReceipt {
            day: d.day.clone(),
            at: crate::state::now(),
            status,
            reason,
            attempts: tally.attempts,
            cost_usd: tally.cost_usd,
        };
        if let Err(e) = store::save_json(&self.home, Kind::Runs, &d.day, &receipt) {
            eprintln!("cos: run receipt not saved: {e:#}");
        }
        if result.is_err() {
            // The page carries the receipt; rebuild it so it says so today.
            if let Err(e) = crate::report::build(&self.home, &self.workspace) {
                eprintln!("cos: report not rebuilt: {e:#}");
            }
        }
        result
    }

    /// `Deferred` in a dark wake; otherwise the sleep assertion to hold for
    /// the run.
    fn awake(&self, what: &'static str) -> Result<wake::Hold> {
        match wake::probe(self.bins.pmset.as_deref()) {
            Wake::Dark => Err(Deferred(what).into()),
            Wake::Full => Ok(wake::Hold::start(self.bins.caffeinate.as_deref())),
            Wake::Unknown(why) => {
                eprintln!("cos: wake state unknown ({why}); running");
                Ok(wake::Hold::start(self.bins.caffeinate.as_deref()))
            }
        }
    }

    fn brief_awake(&self, d: &Dates, invoked: &str, tally: &mut Tally) -> Result<Done<Brief>> {
        let day = d.day.clone();
        let facts = self.gather()?;
        let (mut brief, cost, called): (Brief, f64, _) =
            self.structured(&self.brief_run(&facts), tally)?;
        hold_to_limits(&mut brief);
        hold_to_window(&mut brief, d);
        cover_every_source(&mut brief, &self.workspace, &facts, &called);
        let markdown = brief.to_markdown(&day);
        let saved = store::save(&self.home, Kind::Briefs, &day, &brief, &markdown)?;
        crate::history::record(&self.home, &day, &facts, Some(&brief))?;
        let tracked = report_tracking(track::record_run(
            &self.workspace,
            "brief",
            &brief.coverage,
            brief.gaps.len(),
            invoked,
            &crate::state::now(),
        ));
        crate::report::build(&self.home, &self.workspace)?;
        Ok(Done {
            value: brief,
            markdown,
            cost_usd: cost,
            saved: Some(saved),
            tracked,
        })
    }

    /// A question, under the same wake check and sleep assertion as a brief.
    pub fn ask(&self, question: &str) -> Result<Done<()>> {
        let _hold = self.awake("question")?;
        let out = self.execute(&self.ask_run(question), &mut Tally::default())?;
        Ok(Done {
            value: (),
            markdown: out.result.unwrap_or_default(),
            cost_usd: out.total_cost_usd,
            saved: None,
            tracked: None,
        })
    }

    /// Lists the Notion pages about the monitored projects for the
    /// documentation room, through the user's Notion connector. A listing,
    /// not a judgement: titles and URLs as Notion returns them.
    pub fn notion_pages(&self) -> Result<(crate::docs::NotionList, f64)> {
        let projects: Vec<String> = crate::docs::monitored(&self.workspace)
            .iter()
            .map(|s| s.rsplit('/').next().unwrap_or(s).to_owned())
            .collect();
        anyhow::ensure!(
            !projects.is_empty(),
            "no monitoredRepos in the profile: nothing to list Notion pages for"
        );
        let prompt = format!(
            "Documentation room inventory only — no brief, no advice. Using the Notion search \
             tool (one search per project name, then one for each obvious synonym), list the \
             Notion pages that document these projects: {}. For each page give its exact title, \
             its URL, its parent page or database title (empty when unknown), and which of the \
             projects it belongs to (one of the names above, or empty). At most 150 pages; skip \
             meeting notes older than 90 days. Never invent a page or a URL: list only what a \
             search returned.",
            projects.join(", ")
        );
        let run = Run {
            json_schema: Some(serde_json::json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["pages"],
                "properties": { "pages": { "type": "array", "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["title", "url", "parent", "project"],
                    "properties": {
                        "title": { "type": "string" },
                        "url": { "type": "string" },
                        "parent": { "type": "string" },
                        "project": { "type": "string" }
                    }
                }}}
            })),
            ..self.brain("tactician", "sonnet", prompt, self.budget.unwrap_or(1.5))
        };
        let (mut list, cost, _): (crate::docs::NotionList, f64, _) =
            self.structured(&run, &mut Tally::default())?;
        list.pages
            .retain(|p| p.url.starts_with("https://") && !p.title.trim().is_empty());
        list.listed_at = jiff::Zoned::now().strftime("%Y-%m-%d %H:%M").to_string();
        Ok((list, cost))
    }

    /// The parsed output, its cost, and the tools the run called.
    fn structured<T: DeserializeOwned>(
        &self,
        run: &Run,
        tally: &mut Tally,
    ) -> Result<(T, f64, Vec<String>)> {
        let out = self.execute(run, tally)?;
        let value = out
            .structured_output
            .context("run returned no structured_output")?;
        let parsed =
            serde_json::from_value(value).context("structured_output did not match the schema")?;
        Ok((parsed, out.total_cost_usd, out.called))
    }

    /// One `claude` run, and one more after a transient failure (a timeout,
    /// a dropped connection, a sleep mid-response) when the Mac is awake and
    /// the shared wall-clock budget has `MIN_RETRY` left. Both attempts sit
    /// inside `run.deadline`; the cost returned is their sum.
    fn execute(&self, run: &Run, tally: &mut Tally) -> Result<claude::Outcome> {
        let audit = claude::audit_path(&self.home);
        let started = SystemTime::now();
        tally.attempts += 1;
        let err = match claude::execute(&self.bins.claude, &self.home, &audit, run) {
            Ok(o) => {
                tally.cost_usd += o.total_cost_usd;
                return Ok(o);
            }
            Err(e) => e,
        };
        let Some(first) = err.downcast_ref::<claude::Failed>() else {
            return Err(err);
        };
        tally.cost_usd += first.cost_usd;
        if !first.transient {
            return Err(err);
        }
        let budget = run.deadline.min(crate::proc::CEILING);
        let left = budget.saturating_sub(started.elapsed().unwrap_or(budget));
        if left < MIN_RETRY {
            return Err(err.context(format!(
                "not retried: {}s of the {}s budget left",
                left.as_secs(),
                budget.as_secs()
            )));
        }
        if wake::probe(self.bins.pmset.as_deref()) == Wake::Dark {
            return Err(err.context("not retried: Mac asleep (dark wake)"));
        }
        let first_reason = first.reason.clone();
        let first_cost = first.cost_usd;
        claude::audit_note(
            &audit,
            run.agent,
            serde_json::json!({
                "retry": 2,
                "after": first_reason,
                "deadline_s": left.as_secs(),
            }),
        )?;
        let second = Run {
            deadline: left,
            ..run.clone()
        };
        tally.attempts += 1;
        match claude::execute(&self.bins.claude, &self.home, &audit, &second) {
            Ok(mut o) => {
                tally.cost_usd += o.total_cost_usd;
                o.total_cost_usd += first_cost;
                Ok(o)
            }
            Err(e) => {
                if let Some(f) = e.downcast_ref::<claude::Failed>() {
                    tally.cost_usd += f.cost_usd;
                }
                Err(e.context(format!("retried once after: {first_reason}")))
            }
        }
    }

    fn brain(
        &self,
        agent: &'static str,
        model: &'static str,
        prompt: String,
        budget: f64,
    ) -> Run<'static> {
        Run {
            agent,
            model,
            prompt,
            json_schema: None,
            max_budget_usd: budget,
            tools: Toolset::Brain(crate::context::tools(&self.workspace)),
            deadline: Duration::from_secs(300),
            denied: crate::context::denied_servers(&self.home),
        }
    }
}
