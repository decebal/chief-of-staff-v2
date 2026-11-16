//! The delivery board on the Today view: each Jira ticket beside its pull
//! requests, what the code says, what the dev cluster runs, and the next step.
//! The user, or an agent on their behalf, writes `context/delivery.json` from
//! Jira, `gh` and `kubectl`; `cos` only validates and renders it, so a stale
//! board says when it was read rather than pretending to be live.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::design::icon;
use crate::today::{esc, link_chip, prose};

pub const VERSION: u32 = 1;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Board {
    pub version: u32,
    /// When the sources were read, in words: `Mon 16 Nov, 12:40`.
    pub as_of: String,
    /// What was read, one line each: `Jira: project API`, `kubectl: api-gateway`.
    #[serde(default)]
    pub sources: Vec<String>,
    pub rows: Vec<Row>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Row {
    pub key: String,
    pub summary: String,
    pub url: String,
    pub jira_status: String,
    #[serde(default)]
    pub prs: Vec<Pr>,
    /// Where the change is in code: `merged to develop 13 Nov`, `no PR`.
    pub code: String,
    /// Whether the dev cluster runs it, and which image says so.
    pub cluster: String,
    pub action: Action,
    pub recommendation: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Pr {
    pub number: u32,
    pub url: String,
    /// `merged`, `open, approved`, `open, review required`, `draft`, `closed`.
    pub state: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// The work is ahead of Jira: move or close the ticket.
    UpdateJira,
    /// Nothing in code covers it yet.
    Work,
    /// A PR waits on a reviewer.
    Review,
    /// Approved and green; only the merge is left.
    Merge,
    /// Merged, not yet running on the dev cluster.
    Deploy,
    /// Waiting on someone else.
    Wait,
    /// Nothing to do.
    Done,
}

impl Action {
    fn label(self) -> &'static str {
        match self {
            Action::UpdateJira => "update Jira",
            Action::Work => "work it",
            Action::Review => "get a review",
            Action::Merge => "merge",
            Action::Deploy => "deploy",
            Action::Wait => "wait",
            Action::Done => "done",
        }
    }
}

pub fn path(home: &Path) -> PathBuf {
    home.join("context/delivery.json")
}

/// A missing file is no board; a broken one is an error naming the field.
pub fn load(home: &Path) -> Result<Option<Board>> {
    let p = path(home);
    match std::fs::read_to_string(&p) {
        Ok(text) => parse(&text)
            .map(Some)
            .with_context(|| format!("{}", p.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("read {}", p.display())),
    }
}

pub fn parse(text: &str) -> Result<Board> {
    let board: Board = serde_json::from_str(text).context("not a delivery board")?;
    ensure!(
        board.version == VERSION,
        "`version` is {}; this cos reads version {VERSION}",
        board.version
    );
    Ok(board)
}

/// What `report::data` carries: the board, `null` when there is none, or the
/// error that stopped it being read.
pub fn view(home: &Path) -> Value {
    match load(home) {
        Ok(Some(b)) => serde_json::to_value(b).unwrap_or(Value::Null),
        Ok(None) => Value::Null,
        Err(e) => json!({ "error": format!("{e:#}") }),
    }
}

/// The order cards are grouped in: what moves a ticket soonest first.
fn rank(a: Action) -> u8 {
    match a {
        Action::Deploy => 0,
        Action::Merge => 1,
        Action::UpdateJira => 2,
        Action::Review => 3,
        Action::Work => 4,
        Action::Wait => 5,
        Action::Done => 6,
    }
}

fn slug(a: Action) -> String {
    serde_json::to_value(a)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

fn next_icon(a: Action) -> &'static str {
    match a {
        Action::Deploy => "rocket",
        Action::Merge => "git-merge",
        Action::UpdateJira => "square-pen",
        Action::Review => "eye",
        Action::Work => "hammer",
        Action::Wait => "clock",
        Action::Done => "circle-check",
    }
}

fn pr_icon(state: &str) -> &'static str {
    if state.starts_with("merged") {
        "git-merge"
    } else if state.contains("review required") {
        "eye"
    } else if state.starts_with("closed") {
        "circle-x"
    } else {
        "git-pull-request"
    }
}

/// One card: the key as a link chip, the Jira status and the next action as
/// a coloured chip; the title at full width; the PRs as chips with their
/// state; the code and the dev cluster as two labelled lines; the next step.
fn card(r: &Row) -> String {
    let key = link_chip(
        &icon("ticket"),
        &r.key,
        r.url.starts_with("https://").then_some(r.url.as_str()),
        &r.summary,
    );
    let prs: String = r
        .prs
        .iter()
        .map(|p| {
            let label = format!("#{} · {}", p.number, p.state);
            let chip = link_chip(
                &icon(pr_icon(&p.state)),
                &label,
                p.url.starts_with("https://").then_some(p.url.as_str()),
                &p.state,
            );
            chip.replacen(
                "class=\"src-chip\"",
                &format!(
                    "class=\"src-chip dl-pr\" data-state=\"{}\"",
                    esc(p.state.split(',').next().unwrap_or_default().trim())
                ),
                1,
            )
        })
        .collect();
    let prs = if prs.is_empty() {
        format!(
            r#"<span class="quiet-chip">{}no PR</span>"#,
            icon("git-pull-request")
        )
    } else {
        prs
    };
    format!(
        r#"<article class="dl" data-action="{action}"><header class="dl-head">{key}<span class="dl-jira" title="Jira status">{jira}</span><span class="dl-next" data-action="{action}">{ni}{label}</span></header><h4 class="dl-title">{summary}</h4><div class="chips">{prs}</div><dl class="dl-rows"><dt>{ci}Code</dt><dd>{code}</dd><dt>{si}Dev cluster</dt><dd>{cluster}</dd></dl><p class="dl-step">{ni}<span>{rec}</span></p></article>"#,
        action = slug(r.action),
        jira = esc(&r.jira_status),
        ni = icon(next_icon(r.action)),
        label = r.action.label(),
        summary = prose(&r.summary),
        ci = icon("code"),
        code = prose(&r.code),
        si = icon("server"),
        cluster = prose(&r.cluster),
        rec = prose(&r.recommendation),
    )
}

/// The Today section, collapsed like the other secondary parts. Empty when
/// there is no board.
pub fn section(data: &Value) -> String {
    let v = &data["delivery"];
    if v.is_null() {
        return String::new();
    }
    if let Some(e) = v["error"].as_str() {
        return format!(
            r#"<p class="quiet" id="delivery" data-section="delivery">The delivery board could not be read: {}</p>"#,
            esc(e)
        );
    }
    let Ok(b) = serde_json::from_value::<Board>(v.clone()) else {
        return String::new();
    };
    let mut counts: Vec<(Action, usize)> = Vec::new();
    for r in &b.rows {
        match counts.iter_mut().find(|(a, _)| *a == r.action) {
            Some((_, n)) => *n += 1,
            None => counts.push((r.action, 1)),
        }
    }
    let mut sum = format!(
        "{} ticket{} · read {}",
        b.rows.len(),
        if b.rows.len() == 1 { "" } else { "s" },
        esc(&b.as_of)
    );
    for (a, n) in counts.iter().filter(|(a, _)| *a != Action::Done) {
        let _ = write!(sum, " · {n} {}", a.label());
    }
    let mut s = String::new();
    let _ = write!(
        s,
        r#"<details class="fold" id="delivery" data-section="delivery"><summary><span class="fold-t">Delivery board</span><span class="fold-s">{sum}</span></summary><div class="fold-body">"#
    );
    // The filters: every card, or one next action; the page's script sets
    // `data-show` on the list, and without it every card shows.
    let mut groups: Vec<Action> = b.rows.iter().map(|r| r.action).collect();
    groups.sort_by_key(|a| rank(*a));
    groups.dedup();
    let _ = write!(
        s,
        r#"<div class="chips dl-filters" role="group" aria-label="Show"><button type="button" class="dl-filter" data-show="" aria-pressed="true">All · {}</button>"#,
        b.rows.len()
    );
    for a in &groups {
        let n = b.rows.iter().filter(|r| r.action == *a).count();
        let _ = write!(
            s,
            r#"<button type="button" class="dl-filter" data-show="{}" aria-pressed="false">{}{} · {n}</button>"#,
            slug(*a),
            icon(next_icon(*a)),
            a.label()
        );
    }
    s.push_str(r#"</div><div class="dl-list">"#);
    for a in &groups {
        for r in b.rows.iter().filter(|r| r.action == *a) {
            s.push_str(&card(r));
        }
    }
    s.push_str("</div>");
    if !b.sources.is_empty() {
        let _ = write!(
            s,
            r#"<p class="quiet">Read from {}.</p>"#,
            b.sources
                .iter()
                .map(|x| esc(x))
                .collect::<Vec<_>>()
                .join("; ")
        );
    }
    s.push_str("</div></details>");
    s
}
