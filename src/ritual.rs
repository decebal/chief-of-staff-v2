//! `cos ritual approve` and `cos ritual check`: the ritual's one gate, a
//! person's approval of a design, recorded on the flow's Chronis epic as the
//! design's sha256 plus `cn approve`.
//!
//! `approve` is the one place `cos` writes to a task store, and only on a
//! person's act: it is never an MCP tool. Two callers share it:
//!
//! - the CLI, which asks on a terminal and refuses without one, because an
//!   agent's shell can run `cos` but has no terminal to type `yes` at. With
//!   `--yes` it skips the prompt: the ritual skill lets an agent pass it only
//!   when the user told it, in the conversation, to approve the design. No
//!   hook checks that; the skill's instruction is the only gate on it;
//! - the desktop app's Approve button on the Flows tab. That path needs no
//!   terminal: the command behind it is reachable only from the app's own
//!   window (its capability names only `main`), never from a shell, an MCP
//!   client or the sandboxed report, flows and docs frames, and the click
//!   plus the inline confirm are the person's act.
//!
//! [`gates`] is what the button's card reads: each flow waiting at the gate,
//! its design's digest and its reviews.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::Serialize;

use crate::flows::{self, Approval, Flow, Markers, Step, Task};
use crate::proc::{self, Deadline};

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

    /// What the person does next, when it is theirs to do.
    pub fn next(&self) -> Option<&'static str> {
        match self {
            Check::Unapproved | Check::Changed { .. } => Some(APPROVE_WHERE),
            _ => None,
        }
    }
}

/// Where a person approves a design.
pub const APPROVE_WHERE: &str =
    "approve it from the Flows tab in Chief of Staff, or tell your agent to approve it";

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

/// Where the review an approval rests on is read from.
#[derive(Debug, Clone)]
pub enum ReviewAt {
    /// A path as the CLI was given it.
    File(PathBuf),
    /// A path in the repo, read like the design: committed on the flow's
    /// branch while it exists, else the repo's working tree.
    InRepo(String),
}

pub struct ApproveArgs<'a> {
    pub epic: &'a str,
    pub design: &'a str,
    pub by: &'a str,
    pub review: Option<ReviewAt>,
    pub without_review: bool,
    /// Records without calling `confirm`.
    pub yes: bool,
}

/// Validates the review, asks `confirm` unless `yes`, then records the
/// approval. Returns the line appended to the epic.
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
        (Some(at), false) => {
            let text = match at {
                ReviewAt::File(path) => std::fs::read_to_string(path)
                    .with_context(|| format!("reading the review {}", path.display()))?,
                ReviewAt::InRepo(path) => String::from_utf8(flows::read_design(
                    repo,
                    markers.get("branch"),
                    path,
                    flows::GIT_CALL * 2,
                )?)
                .with_context(|| format!("the review {path} is not UTF-8 text"))?,
            };
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
    if !args.yes && !confirm(&line)? {
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

/// The approval behind the Flows tab's button: the same [`approve`], with the
/// review read from the repo and the confirmation already given in the
/// window. Returns the line written and the gate read back afterwards.
pub fn approve_in_app(
    cn: &Path,
    repo: &Path,
    epic: &str,
    design: &str,
    by: &str,
    review: Option<&str>,
    without_review: bool,
) -> Result<(String, Check)> {
    let args = ApproveArgs {
        epic,
        design,
        by,
        review: review.map(|r| ReviewAt::InRepo(r.to_owned())),
        without_review,
        yes: false,
    };
    let line = approve(cn, repo, &args, |_| Ok(true))?;
    Ok((line, check(cn, repo, epic)?))
}

/// The approver's name: the workspace profile's `owner`, else the repo's
/// `git config user.name`.
pub fn approver(workspace: &Path, repo: &Path) -> Option<String> {
    let owner = crate::state::profile(workspace)
        .and_then(|p| p["owner"].as_str().map(str::trim).map(str::to_owned))
        .filter(|o| !o.is_empty());
    owner.or_else(|| {
        let mut cmd = Command::new("git");
        cmd.arg("-C").arg(repo).args(["config", "user.name"]);
        proc::run(&mut cmd, flows::GIT_CALL)
            .ok()
            .filter(|o| o.status.success())
            .map(|o| o.stdout.trim().to_owned())
            .filter(|n| !n.is_empty())
    })
}

/// The ritual's steps as the approval card draws them, the gate included.
pub const STRIP: [&str; 8] = [
    "think", "spec", "approve", "plan", "build", "review", "ship", "reflect",
];

/// One design review file, as the card shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReviewFile {
    pub path: String,
    pub verdict: String,
    /// It read the design as the flow holds it now.
    pub current: bool,
    pub blocking: usize,
    pub should: usize,
    pub nit: usize,
}

impl ReviewFile {
    pub fn clean(&self) -> bool {
        self.current && self.verdict == "clean"
    }
}

/// A flow waiting at the approval gate: what its card shows.
#[derive(Debug, Clone, Serialize)]
pub struct Gate {
    pub repo: String,
    pub epic: String,
    pub title: String,
    /// Always `approve`: the card's place on [`STRIP`].
    pub step: &'static str,
    pub brief: Option<String>,
    pub design: String,
    /// `None` when the design could not be read; `design_error` says why.
    pub design_sha256: Option<String>,
    pub design_error: Option<String>,
    /// The newest review, `<design stem>-review.md`.
    pub review: Option<ReviewFile>,
    /// Earlier rounds, `<design stem>-review-<n>.md`, newest first.
    pub history: Vec<ReviewFile>,
}

impl Gate {
    /// The newest review read this design and found it clean.
    pub fn clean(&self) -> bool {
        self.review.as_ref().is_some_and(ReviewFile::clean)
    }
}

/// Counts `<n>. blocking|should|nit …` lines below the front matter.
pub fn findings(text: &str) -> (usize, usize, usize) {
    let body = text
        .strip_prefix("---")
        .and_then(|rest| rest.split_once("\n---").map(|(_, b)| b))
        .unwrap_or(text);
    let (mut blocking, mut should, mut nit) = (0, 0, 0);
    for line in body.lines() {
        let line = line.trim_start();
        let digits = line.chars().take_while(char::is_ascii_digit).count();
        let Some(rest) = line[digits..]
            .strip_prefix('.')
            .filter(|r| digits > 0 && r.starts_with(char::is_whitespace))
        else {
            continue;
        };
        match rest
            .split_whitespace()
            .next()
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("blocking" | "must") => blocking += 1,
            Some("should") => should += 1,
            Some("nit") => nit += 1,
            _ => {}
        }
    }
    (blocking, should, nit)
}

/// File names in `dir` as the flow holds them: on `branch` while it exists,
/// else in the repo's working tree.
fn names_in(repo: &Path, branch: Option<&str>, dir: &str, budget: Duration) -> Vec<String> {
    if let Some(branch) = branch {
        let reference = format!("refs/heads/{branch}");
        let mut probe = Command::new("git");
        probe
            .arg("-C")
            .arg(repo)
            .args(["rev-parse", "--verify", "--quiet", &reference]);
        if proc::run(&mut probe, budget).is_ok_and(|o| o.status.success()) {
            let mut ls = Command::new("git");
            ls.arg("-C")
                .arg(repo)
                .args(["ls-tree", "--name-only", &reference, "--"])
                .arg(format!("{dir}/"));
            return proc::run(&mut ls, budget)
                .ok()
                .filter(|o| o.status.success())
                .map(|o| {
                    o.stdout
                        .lines()
                        .filter_map(|l| l.rsplit('/').next().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default();
        }
    }
    std::fs::read_dir(repo.join(dir))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect()
}

/// The design's reviews: the newest, then earlier rounds newest first.
fn reviews(
    repo: &Path,
    branch: Option<&str>,
    design: &str,
    digest: Option<&str>,
    until: &Deadline,
) -> (Option<ReviewFile>, Vec<ReviewFile>) {
    let design = design.trim_start_matches("./");
    let (dir, file) = design.rsplit_once('/').unwrap_or(("", design));
    let stem = file.strip_suffix(".md").unwrap_or(file);
    let latest = format!("{stem}-review.md");
    let round = |name: &str| -> Option<u32> {
        name.strip_prefix(&format!("{stem}-review-"))?
            .strip_suffix(".md")?
            .parse()
            .ok()
    };
    let names = names_in(repo, branch, dir, until.left().min(flows::GIT_CALL));
    let mut rounds: Vec<(u32, String)> = names
        .iter()
        .filter_map(|n| round(n).map(|r| (r, n.clone())))
        .collect();
    rounds.sort_by_key(|r| std::cmp::Reverse(r.0));
    let read = |name: &str| -> Option<ReviewFile> {
        let path = if dir.is_empty() {
            name.to_owned()
        } else {
            format!("{dir}/{name}")
        };
        let left = until.left().min(flows::GIT_CALL);
        let bytes = flows::read_design(repo, branch, &path, left).ok()?;
        let text = String::from_utf8_lossy(&bytes);
        let front = Review::parse(&text).unwrap_or_default();
        let (blocking, should, nit) = findings(&text);
        Some(ReviewFile {
            current: digest.is_some_and(|d| d == front.design_sha256)
                && (front.design.is_empty() || same_path(&front.design, design)),
            verdict: if front.verdict.is_empty() {
                "unknown".into()
            } else {
                front.verdict
            },
            path,
            blocking,
            should,
            nit,
        })
    };
    let newest = if names.contains(&latest) {
        read(&latest)
    } else {
        None
    };
    let history = rounds.iter().filter_map(|(_, n)| read(n)).collect();
    (newest, history)
}

/// What the card says when the design cannot be read: the next move, never
/// git's own error text.
pub fn unreadable_design(design: &str, branch: Option<&str>, error: &str) -> String {
    match branch {
        Some(b) if error.contains("not committed on") => format!(
            "The design is not committed on {b} yet. Commit {design} on that branch, then refresh this tab to approve it."
        ),
        _ if error.contains("does not exist") || error.contains("No such file") => format!(
            "{design} is not in the repo. Write and commit the design, or fix the epic's ritual-design line."
        ),
        _ => format!(
            "{design} could not be read from the repo. Check that it is committed, then refresh this tab."
        ),
    }
}

/// The card for `flow` when it waits at the gate: designed, not approved.
/// `repo` is its main checkout.
pub fn gate(repo: &Path, flow: &Flow, budget: Duration) -> Option<Gate> {
    if flow.approved || flow.step != Step::Spec {
        return None;
    }
    let design = flow.markers.get("design")?.to_owned();
    let branch = flow.markers.get("branch");
    let until = Deadline::after(budget);
    let read = flows::read_design(repo, branch, &design, until.left().min(flows::GIT_CALL * 2));
    let (digest, design_error) = match read {
        Ok(bytes) => (Some(flows::sha256(&bytes)), None),
        Err(e) => (
            None,
            Some(unreadable_design(&design, branch, &format!("{e:#}"))),
        ),
    };
    let (review, history) = reviews(repo, branch, &design, digest.as_deref(), &until);
    Some(Gate {
        repo: flow.repo.clone(),
        epic: flow.id.clone(),
        title: flow.title.clone(),
        step: "approve",
        brief: flow.markers.get("brief").map(str::to_owned),
        design,
        design_sha256: digest,
        design_error,
        review,
        history,
    })
}

/// Every flow under `projects` waiting at the gate, and what could not be read.
pub fn gates(projects: &Path, cn: &Path, budget: Duration) -> (Vec<Gate>, Vec<String>) {
    let until = Deadline::after(budget);
    let scan = flows::scan(projects, cn, budget);
    let mut gaps = scan.gaps;
    let mut found = Vec::new();
    for flow in &scan.flows {
        if flow.approved || flow.step != Step::Spec {
            continue;
        }
        if until.expired() {
            gaps.push(format!("{}: {} not read, out of time", flow.repo, flow.id));
            continue;
        }
        found.extend(gate(&projects.join(&flow.repo), flow, until.left()));
    }
    (found, gaps)
}

/// Where [`locate`] found a file, and the directory it resolved it under: the
/// worktree with the flow's branch, the repo, or the export directory. A
/// caller checks the file against `root` (`flows::contained`) before opening.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Located {
    pub root: PathBuf,
    pub file: PathBuf,
}

/// A file a card links to, as a path to open: the copy in a worktree that has
/// the flow's branch checked out, else the committed copy written under
/// `export`, else (no branch) the repo's working tree.
pub fn locate(repo: &Path, branch: Option<&str>, path: &str, export: &Path) -> Result<Located> {
    let path = path.trim_start_matches("./");
    if let Some(branch) = branch {
        let mut cmd = Command::new("git");
        cmd.arg("-C")
            .arg(repo)
            .args(["worktree", "list", "--porcelain"]);
        let out = proc::run(&mut cmd, flows::GIT_CALL)?;
        let want = format!("branch refs/heads/{branch}");
        let mut here: Option<PathBuf> = None;
        for line in out.stdout.lines() {
            if let Some(dir) = line.strip_prefix("worktree ") {
                here = Some(PathBuf::from(dir));
            } else if line == want
                && let Some(dir) = &here
                && dir.join(path).is_file()
            {
                return Ok(Located {
                    root: dir.clone(),
                    file: dir.join(path),
                });
            }
        }
        let bytes = flows::read_design(repo, Some(branch), path, flows::GIT_CALL * 2)?;
        let file = export.join(path.rsplit('/').next().unwrap_or(path));
        std::fs::create_dir_all(export)?;
        std::fs::write(&file, bytes).with_context(|| format!("writing {}", file.display()))?;
        return Ok(Located {
            root: export.to_path_buf(),
            file,
        });
    }
    let file = repo.join(path);
    anyhow::ensure!(file.is_file(), "{path} is not in {}", repo.display());
    Ok(Located {
        root: repo.to_path_buf(),
        file,
    })
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
