//! `cos ritual approve` and `cos ritual check`: the ritual's one gate, a
//! person's approval of a design, recorded on the flow's Chronis epic as the
//! design's sha256 plus `cn approve`.
//!
//! `approve` is the one place `cos` writes to a task store, and only on the
//! user's typed command: it is never an MCP tool, and the CLI refuses it
//! without a terminal so an agent session cannot approve for the user.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result, bail};

use crate::flows::{self, Approval, Markers, Task};
use crate::proc;

const CN_CALL: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Check {
    Approved { design: String },
    Unapproved,
    Changed { design: String },
    MissingDesign { design: Option<String> },
}

impl Check {
    pub fn ok(&self) -> bool {
        matches!(self, Check::Approved { .. })
    }

    pub fn describe(&self) -> String {
        match self {
            Check::Approved { design } => format!("approved: {design} is unchanged since approval"),
            Check::Unapproved => "unapproved".into(),
            Check::Changed { design } => format!("changed since approval: {design}"),
            Check::MissingDesign { design: Some(d) } => format!("missing design: {d}"),
            Check::MissingDesign { design: None } => {
                "missing design: no ritual-approved line names one".into()
            }
        }
    }
}

/// The review file's front matter.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Review {
    pub design: String,
    pub design_sha256: String,
    pub verdict: String,
}

impl Review {
    pub fn parse(text: &str) -> Result<Self> {
        let mut lines = text.lines();
        if lines.next().map(str::trim) != Some("---") {
            bail!("the review has no front matter");
        }
        let mut review = Review::default();
        for line in lines {
            let line = line.trim();
            if line == "---" {
                return Ok(review);
            }
            let Some((key, value)) = line.split_once(':') else {
                continue;
            };
            let value = value.trim().to_owned();
            match key.trim() {
                "design" => review.design = value,
                "design-sha256" => review.design_sha256 = value,
                "verdict" => review.verdict = value,
                _ => {}
            }
        }
        bail!("the review's front matter is not closed with ---")
    }
}

fn same_path(a: &str, b: &str) -> bool {
    let norm = |p: &str| p.trim().trim_start_matches("./").to_owned();
    norm(a) == norm(b)
}

fn tasks(cn: &Path, repo: &Path) -> Result<Vec<Task>> {
    flows::read_tasks(cn, repo, CN_CALL)
}

fn epic<'t>(tasks: &'t [Task], id: &str) -> Result<&'t Task> {
    tasks
        .iter()
        .find(|t| t.id == id)
        .with_context(|| format!("no task {id} in this repo's .chronis store"))
}

/// Whether the epic is approved and its design unchanged since.
pub fn check(cn: &Path, repo: &Path, epic_id: &str) -> Result<Check> {
    let tasks = tasks(cn, repo)?;
    let epic = epic(&tasks, epic_id)?;
    Ok(judge(repo, epic))
}

pub fn judge(repo: &Path, epic: &Task) -> Check {
    if !epic.approved.unwrap_or(false) {
        return Check::Unapproved;
    }
    let markers = Markers::parse(epic.description.as_deref().unwrap_or(""));
    let Some(approval) = markers.get("approved").and_then(Approval::parse) else {
        return Check::MissingDesign { design: None };
    };
    match flows::read_design(
        repo,
        markers.get("branch"),
        &approval.design,
        flows::GIT_CALL * 2,
    ) {
        Err(_) => Check::MissingDesign {
            design: Some(approval.design),
        },
        Ok(bytes) if flows::sha256(&bytes) == approval.sha256 => Check::Approved {
            design: approval.design,
        },
        Ok(_) => Check::Changed {
            design: approval.design,
        },
    }
}

/// `--repo`, or the main checkout of the current directory: the flow's
/// `.chronis/` store lives there even when the work runs in a worktree.
pub fn resolve_repo(explicit: Option<&Path>) -> Result<PathBuf> {
    if let Some(repo) = explicit {
        return Ok(repo.to_path_buf());
    }
    let here = std::env::current_dir().context("reading the current directory")?;
    flows::main_checkout(&here)
}

pub struct ApproveArgs<'a> {
    pub epic: &'a str,
    pub design: &'a str,
    pub by: &'a str,
    pub review: Option<PathBuf>,
    pub without_review: bool,
}

/// Validates the review, asks `confirm`, then records the approval. Returns
/// the line appended to the epic.
pub fn approve(
    cn: &Path,
    repo: &Path,
    args: &ApproveArgs,
    confirm: impl FnOnce(&str) -> Result<bool>,
) -> Result<String> {
    if args.by.trim().is_empty() {
        bail!("--by needs the approver's name");
    }
    let tasks = tasks(cn, repo)?;
    let epic = epic(&tasks, args.epic)?;
    let markers = Markers::parse(epic.description.as_deref().unwrap_or(""));
    if let Some(named) = markers.get("design")
        && !same_path(named, args.design)
    {
        bail!(
            "the epic's design is {named} (ritual-design:), not {}",
            args.design
        );
    }
    let digest = flows::sha256(&flows::read_design(
        repo,
        markers.get("branch"),
        args.design,
        flows::GIT_CALL * 2,
    )?);

    let suffix = match (&args.review, args.without_review) {
        (Some(_), true) => bail!("pass --review or --without-review, not both"),
        (None, false) => {
            bail!("pass --review <path> (a clean review of this design) or --without-review")
        }
        (None, true) => " without review".to_owned(),
        (Some(path), false) => {
            let text = std::fs::read_to_string(path)
                .with_context(|| format!("reading the review {}", path.display()))?;
            let review = Review::parse(&text)?;
            if !same_path(&review.design, args.design) {
                bail!("the review is of {}, not {}", review.design, args.design);
            }
            if review.design_sha256 != digest {
                bail!(
                    "the review read a different version of the design (design-sha256 {}, now {digest}): review it again",
                    review.design_sha256
                );
            }
            if review.verdict != "clean" {
                bail!("the review's verdict is {:?}, not clean", review.verdict);
            }
            String::new()
        }
    };

    let line = format!(
        "ritual-approved: {} sha256:{digest} by {}{suffix}",
        args.design.trim_start_matches("./"),
        args.by.trim()
    );
    if !confirm(&line)? {
        bail!("not approved");
    }

    run_cn(
        cn,
        repo,
        &[
            "task",
            "edit",
            args.epic,
            "--append-description",
            &line,
            "--toon",
        ],
    )?;
    run_cn(cn, repo, &["approve", args.epic, "--toon"]).with_context(|| {
        format!(
            "the line was appended to {} but cn approve failed, so it is still unapproved: run `cn approve {}`",
            args.epic, args.epic
        )
    })?;
    Ok(line)
}

fn run_cn(cn: &Path, repo: &Path, args: &[&str]) -> Result<()> {
    let mut cmd = Command::new(cn);
    cmd.current_dir(repo).args(args);
    let out = proc::run(&mut cmd, CN_CALL)?;
    if !out.status.success() {
        bail!("cn {} failed: {}", args.join(" "), out.stderr.trim());
    }
    Ok(())
}
