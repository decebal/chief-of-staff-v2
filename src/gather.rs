//! Local facts `cos` collects itself, so the model spends no turns on them:
//! ready chronis tasks per repo, and PRs. Typed, because the report charts
//! count them; each source fails soft into `gaps`, never a silent absence.

use std::fmt::Write as _;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::proc;

const PER_CALL: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Facts {
    pub collected_at: String,
    pub merged_since: String,
    pub tasks: Vec<Task>,
    pub mine_open: Vec<Pr>,
    pub review_requested: Vec<Pr>,
    pub merged: Vec<Pr>,
    pub gaps: Vec<String>,
    /// `code` and/or `tasks` when the profile did not approve reading them.
    #[serde(default)]
    pub unread: Vec<String>,
}

/// Which local facts may be collected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reads {
    /// PRs through `gh`.
    pub code: bool,
    /// Ready tasks through `cn`.
    pub tasks: bool,
}

impl Reads {
    pub const ALL: Self = Self {
        code: true,
        tasks: true,
    };
}

const NOT_APPROVED: &str = "not read: no approved source for it in the profile";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub repo: String,
    pub id: String,
    pub kind: String,
    pub title: String,
    pub priority: String,
    pub blocked_by: String,
    pub parent: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pr {
    pub repo: String,
    pub number: u64,
    pub title: String,
    pub draft: bool,
    pub updated: String,
    pub closed: Option<String>,
}

impl Pr {
    fn line(&self) -> String {
        let draft = if self.draft { "[draft] " } else { "" };
        format!(
            "{}#{} {draft}{} (updated {})",
            self.repo,
            self.number,
            self.title,
            self.updated.get(..10).unwrap_or(&self.updated)
        )
    }
}

impl Facts {
    /// The text block the brains read.
    pub fn render(&self) -> String {
        let mut s = String::from("### chronis ready (per repo)\n");
        if self.unread.iter().any(|u| u == "tasks") {
            let _ = writeln!(s, "{NOT_APPROVED}");
        }
        let mut repo = "";
        for t in &self.tasks {
            if t.repo != repo {
                repo = &t.repo;
                let _ = writeln!(s, "#### {repo}");
            }
            let blocked = if t.blocked_by.is_empty() {
                String::new()
            } else {
                format!(" blocked_by={}", t.blocked_by)
            };
            let _ = writeln!(s, "{} {} {} {}{blocked}", t.id, t.priority, t.kind, t.title);
        }
        for (title, prs) in [
            ("my open PRs", &self.mine_open),
            ("PRs awaiting my review", &self.review_requested),
            ("my PRs merged since", &self.merged),
        ] {
            let since = if title.ends_with("since") {
                format!(" {}", self.merged_since)
            } else {
                String::new()
            };
            let _ = writeln!(s, "\n### {title}{since}");
            if self.unread.iter().any(|u| u == "code") {
                let _ = writeln!(s, "{NOT_APPROVED}");
            } else if prs.is_empty() {
                let _ = writeln!(s, "none");
            }
            for p in prs {
                let _ = writeln!(s, "{}", p.line());
            }
        }
        if !self.gaps.is_empty() {
            let _ = writeln!(s, "\n### not collected");
            for g in &self.gaps {
                let _ = writeln!(s, "GAP: {g}");
            }
        }
        s
    }
}

/// PRs of mine merged between `from` and `to` inclusive (`YYYY-MM-DD`), up to
/// GitHub search's 1,000-result ceiling, so callers query a month at a time.
/// A month measured 11s and 463 PRs; the deadline leaves room for that.
pub fn merged_between(from: &str, to: &str, gaps: &mut Vec<String>) -> Vec<Pr> {
    let merged_at = format!("--merged-at={from}..{to}");
    gh_query(
        &["--author=@me", "--merged", &merged_at],
        &format!("merged PRs {from}..{to}"),
        "1000",
        Duration::from_secs(60),
        gaps,
    )
}

/// `merged_since` is a `YYYY-MM-DD` lower bound for merged PRs. Runs nothing
/// that `reads` does not allow.
pub fn collect(projects: &Path, merged_since: &str, reads: Reads) -> Facts {
    let mut f = Facts {
        collected_at: jiff::Zoned::now().to_string(),
        merged_since: merged_since.to_owned(),
        ..Facts::default()
    };
    if reads.tasks {
        chronis_ready(projects, &mut f);
    } else {
        f.unread.push("tasks".into());
    }
    if !reads.code {
        f.unread.push("code".into());
        return f;
    }
    let merged_at = format!("--merged-at=>={merged_since}");
    f.mine_open = gh(
        &["--author=@me", "--state=open"],
        "my open PRs",
        &mut f.gaps,
    );
    f.review_requested = gh(
        &["--review-requested=@me", "--state=open"],
        "PRs awaiting my review",
        &mut f.gaps,
    );
    f.merged = gh(
        &["--author=@me", "--merged", &merged_at],
        "merged PRs",
        &mut f.gaps,
    );
    f
}

fn chronis_ready(projects: &Path, f: &mut Facts) {
    let Ok(entries) = std::fs::read_dir(projects) else {
        f.gaps.push(format!("cannot read {}", projects.display()));
        return;
    };
    let mut repos: Vec<_> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.join(".chronis").is_dir())
        .collect();
    repos.sort();
    for repo in repos {
        let name = repo
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let mut cmd = Command::new("cn");
        cmd.current_dir(&repo).args(["ready", "--toon"]);
        match proc::run(&mut cmd, PER_CALL) {
            Ok(o) if o.status.success() => f.tasks.extend(parse_toon(&name, &o.stdout)),
            Ok(o) => f
                .gaps
                .push(format!("cn ready in {name}: {}", o.stderr.trim())),
            Err(e) => f.gaps.push(format!("cn ready in {name}: {e}")),
        }
    }
}

/// `cn ready --toon` rows: `id|type|title|pri|status|claimed|blocked_by|parent|archived`.
/// A title may itself contain `|`, so fixed columns are taken from both ends.
pub fn parse_toon(repo: &str, out: &str) -> Vec<Task> {
    out.lines()
        .filter(|l| !l.starts_with('[') && !l.trim().is_empty())
        .filter_map(|l| {
            let cols: Vec<&str> = l.split('|').collect();
            if cols.len() < 9 {
                return None;
            }
            let n = cols.len();
            Some(Task {
                repo: repo.to_owned(),
                id: cols[0].to_owned(),
                kind: cols[1].to_owned(),
                title: cols[2..n - 6].join("|"),
                priority: cols[n - 6].to_owned(),
                blocked_by: cols[n - 3].to_owned(),
                parent: cols[n - 2].to_owned(),
            })
        })
        .collect()
}

fn gh(filter: &[&str], what: &str, gaps: &mut Vec<String>) -> Vec<Pr> {
    gh_query(filter, what, "100", PER_CALL, gaps)
}

fn gh_query(
    filter: &[&str],
    what: &str,
    limit: &str,
    deadline: Duration,
    gaps: &mut Vec<String>,
) -> Vec<Pr> {
    let mut cmd = Command::new("gh");
    cmd.args(["search", "prs"]).args(filter).args([
        "--json",
        "repository,number,title,isDraft,updatedAt,closedAt",
        "--limit",
        limit,
    ]);
    let out = match proc::run(&mut cmd, deadline) {
        Ok(o) if o.status.success() => o.stdout,
        Ok(o) => {
            gaps.push(format!("gh {what}: {}", o.stderr.trim()));
            return vec![];
        }
        Err(e) => {
            gaps.push(format!("gh {what}: {e}"));
            return vec![];
        }
    };
    match serde_json::from_str::<Vec<Value>>(&out) {
        Ok(rows) => rows.iter().filter_map(parse_pr).collect(),
        Err(e) => {
            gaps.push(format!("gh {what}: unparseable output: {e}"));
            vec![]
        }
    }
}

fn parse_pr(v: &Value) -> Option<Pr> {
    Some(Pr {
        repo: v["repository"]["nameWithOwner"].as_str()?.to_owned(),
        number: v["number"].as_u64()?,
        title: v["title"].as_str().unwrap_or_default().to_owned(),
        draft: v["isDraft"].as_bool().unwrap_or(false),
        updated: v["updatedAt"].as_str().unwrap_or_default().to_owned(),
        closed: v["closedAt"].as_str().map(str::to_owned),
    })
}
