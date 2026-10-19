//! The three runs `cos` offers — brief, review, ask — shared by the CLI and the
//! MCP server. Blocking: each spawns `claude` children under one deadline.

use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};
use serde::de::DeserializeOwned;

use crate::claude::{self, Outcome, Run, Toolset};
use crate::context::{self, Local};
use crate::gather::{self, Facts};
use crate::reports::{
    self, Brief, Completeness, Coverage, Health, MAX_ACTIONS, MAX_CHANGES, Review, Route,
};
use crate::store::{self, Kind};
use crate::track::{self, Recorded};

/// Route, then brains in parallel, then merge: all three inside this ceiling.
const ASK_DEADLINE: Duration = Duration::from_secs(300);

#[derive(Clone)]
pub struct Config {
    pub home: PathBuf,
    pub projects: PathBuf,
    /// Overrides each run's default spending cap, in USD.
    pub budget: Option<f64>,
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
fn grounding(home: &std::path::Path, facts: &Facts) -> String {
    format!("{}\n\n{}", crate::context::block(home), facts_block(facts))
}

impl Config {
    /// Collects local facts once and keeps them, so the report charts the same
    /// numbers the brain was shown.
    pub fn gather(&self) -> Result<Facts> {
        let d = dates_in(&self.home);
        let facts = gather::collect(
            &self.projects,
            &d.week_ago,
            context::local_reads(&self.home),
        );
        store::save_json(&self.home, Kind::Facts, &d.day, &facts)?;
        Ok(facts)
    }

    pub fn brief_run(&self, facts: &Facts) -> Run<'static> {
        let d = dates_in(&self.home);
        let prompt = format!(
            "Today is {}, now {}. Write today's brief: at most {MAX_ACTIONS} actions that need \
             the user and at most {MAX_CHANGES} verified changes. Meetings that have already \
             ended go nowhere but a one-line note; prep only for what is still ahead. {}\n\n{}",
            d.day,
            d.now,
            d.calendar_rule(),
            grounding(&self.home, facts)
        );
        Run {
            json_schema: Some(reports::brief_schema()),
            ..self.brain("tactician", "sonnet", prompt, self.budget.unwrap_or(2.0))
        }
    }

    pub fn review_run(&self, facts: &Facts) -> Run<'static> {
        let d = dates_in(&self.home);
        let prompt = format!(
            "Today is {}. Run the weekly steering review: each confirmed goal in the workspace \
             (or context/goals.md when there is no workspace), worked back into this week's \
             proposed deliverables.\n\n{}",
            d.day,
            grounding(&self.home, facts)
        );
        Run {
            json_schema: Some(reports::review_schema()),
            ..self.brain("strategist", "opus", prompt, self.budget.unwrap_or(5.0))
        }
    }

    pub fn brief(&self) -> Result<Done<Brief>> {
        let d = dates_in(&self.home);
        let day = d.day.clone();
        let invoked = crate::state::now();
        let facts = self.gather()?;
        let (mut brief, cost, called): (Brief, f64, _) =
            self.structured(&self.brief_run(&facts))?;
        hold_to_limits(&mut brief);
        hold_to_window(&mut brief, &d);
        cover_every_source(&mut brief, &self.home, &facts, &called);
        let markdown = brief.to_markdown(&day);
        let saved = store::save(&self.home, Kind::Briefs, &day, &brief, &markdown)?;
        crate::history::record(&self.home, &day, &facts, Some(&brief))?;
        let tracked = report_tracking(track::record_run(
            &self.home,
            "brief",
            &brief.coverage,
            brief.gaps.len(),
            &invoked,
            &crate::state::now(),
        ));
        crate::report::build(&self.home)?;
        Ok(Done {
            value: brief,
            markdown,
            cost_usd: cost,
            saved: Some(saved),
            tracked,
        })
    }

    pub fn review(&self) -> Result<Done<Review>> {
        let day = dates_in(&self.home).day;
        let invoked = crate::state::now();
        let facts = self.gather()?;
        let (mut review, cost, _): (Review, f64, _) = self.structured(&self.review_run(&facts))?;
        review.what_changed.truncate(MAX_CHANGES);
        let markdown = review.to_markdown(&day);
        let saved = store::save(&self.home, Kind::Reviews, &day, &review, &markdown)?;
        crate::history::record(&self.home, &day, &facts, None)?;
        let tracked = report_tracking(track::record_run(
            &self.home,
            "review",
            &[],
            review.gaps.len(),
            &invoked,
            &crate::state::now(),
        ));
        crate::report::build(&self.home)?;
        Ok(Done {
            value: review,
            markdown,
            cost_usd: cost,
            saved: Some(saved),
            tracked,
        })
    }

    pub fn route_run(question: &str) -> Run<'static> {
        Run {
            deadline: Duration::from_secs(60),
            json_schema: Some(reports::route_schema()),
            ..bare(format!(
                "Today is {}. Routing step only — do not answer. Pick the brain(s) for \
                 this question and restate it so each brain can answer it alone.\n\n\
                 Question: {question}",
                dates_at(jiff::Zoned::now()).day
            ))
        }
    }

    /// Route, run the chosen brains in parallel, merge. `value` is the routing.
    pub fn ask(&self, question: &str) -> Result<Done<Route>> {
        let started = Instant::now();
        let left = || ASK_DEADLINE.saturating_sub(started.elapsed());
        let audit = claude::audit_path(&self.home);
        let d = dates_in(&self.home);

        let (route, mut cost, _): (Route, f64, _) = self.structured(&Self::route_run(question))?;
        let prompt = format!(
            "Today is {}. {}\n\nOriginal question: {question}\n\n{}",
            d.day,
            route.framing,
            grounding(
                &self.home,
                &gather::collect(
                    &self.projects,
                    &d.week_ago,
                    context::local_reads(&self.home)
                )
            )
        );
        let budget = self.budget.unwrap_or(2.0);
        let answers: Vec<(&str, Result<Outcome>)> = thread::scope(|s| {
            let handles: Vec<_> = route
                .brains
                .iter()
                .map(|b| {
                    let run = Run {
                        deadline: left(),
                        ..self.brain(b.agent(), b.model(), prompt.clone(), budget)
                    };
                    let (home, audit) = (&self.home, &audit);
                    (
                        b.agent(),
                        s.spawn(move || claude::execute(home, audit, &run)),
                    )
                })
                .collect();
            handles
                .into_iter()
                .map(|(name, h)| {
                    let r = h
                        .join()
                        .unwrap_or_else(|_| Err(anyhow!("{name} thread panicked")));
                    (name, r)
                })
                .collect()
        });

        let mut merged_input = String::new();
        for (name, r) in &answers {
            let body = match r {
                Ok(o) => {
                    cost += o.total_cost_usd;
                    o.result.clone().unwrap_or_default()
                }
                Err(e) => format!("FAILED: {e:#}"),
            };
            merged_input.push_str(&format!("## {name}\n\n{body}\n\n"));
        }

        let merged = claude::execute(
            &self.home,
            &audit,
            &Run {
                deadline: left(),
                ..bare(format!(
                    "Merge step. Question: {question}\n\n\
                     Answers from the brains (a FAILED entry is a gap, not an empty answer):\n\n\
                     {merged_input}Write the final answer under your Answering rules."
                ))
            },
        )?;
        cost += merged.total_cost_usd;
        Ok(Done {
            value: route,
            markdown: merged.result.unwrap_or_default(),
            cost_usd: cost,
            saved: None,
            tracked: None,
        })
    }

    /// The parsed output, its cost, and the tools the run called.
    fn structured<T: DeserializeOwned>(&self, run: &Run) -> Result<(T, f64, Vec<String>)> {
        let out = claude::execute(&self.home, &claude::audit_path(&self.home), run)?;
        let value = out
            .structured_output
            .context("run returned no structured_output")?;
        let parsed =
            serde_json::from_value(value).context("structured_output did not match the schema")?;
        Ok((parsed, out.total_cost_usd, out.called))
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
            tools: Toolset::Brain(crate::context::tools(&self.home)),
            deadline: Duration::from_secs(300),
            denied: crate::context::denied_servers(&self.home),
        }
    }
}

/// The chief with no tools: routing and merging read only the prompt.
fn bare(prompt: String) -> Run<'static> {
    Run {
        agent: "chief-of-staff",
        model: "sonnet",
        prompt,
        json_schema: None,
        max_budget_usd: 0.5,
        tools: Toolset::None,
        deadline: Duration::from_secs(120),
        denied: Vec::new(),
    }
}
