//! `cos flows`: every planned piece of work on this machine, one Chronis epic
//! per flow, with the ritual step it is in (`.claude/skills/ritual`,
//! `docs/ritual.md`).
//!
//! Read-only. Each repo under the projects root with `.chronis/` is read once
//! with `cn list --format json --all`, all under one deadline. Descriptions are
//! parsed for `ritual-*` markers and acceptance criteria, then dropped.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::proc::{self, Deadline};

/// The whole scan, every repo: comfortably under the five-minute gate ceiling.
pub const SCAN_BUDGET: Duration = Duration::from_secs(90);
/// One repo's `cn list`, taken from what the scan has left.
const PER_REPO: Duration = Duration::from_secs(20);

/// One task as `cn list --format json --all` prints it (field names observed
/// on chronis 2026-11-09). `status` and `task_type` are required: a renamed
/// field fails the parse and becomes a gap rather than zero flows.
#[derive(Debug, Clone, Deserialize)]
pub struct Task {
    pub id: String,
    #[serde(default)]
    pub title: String,
    pub status: String,
    pub task_type: String,
    #[serde(default)]
    pub parent: Option<String>,
    #[serde(default)]
    pub blocked_by: Vec<String>,
    #[serde(default)]
    pub approved: Option<bool>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub archived: bool,
    #[serde(default, deserialize_with = "lenient")]
    pub priority: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    pub claimed_by: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    pub created_at: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    pub done_at: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    pub done_reason: Option<String>,
}

/// A display field `cn` may print in another type one day: a string is kept,
/// a number becomes its text, anything else is dropped, so one odd field
/// never turns the whole repo into a gap.
fn lenient<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> std::result::Result<Option<String>, D::Error> {
    Ok(match serde_json::Value::deserialize(d)? {
        serde_json::Value::String(s) => Some(s),
        serde_json::Value::Number(n) => Some(n.to_string()),
        _ => None,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Step {
    Think,
    Spec,
    Plan,
    Build,
    Review,
    Ship,
    Reflect,
    Drift,
    Done,
}

impl Step {
    pub fn as_str(self) -> &'static str {
        match self {
            Step::Think => "think",
            Step::Spec => "spec",
            Step::Plan => "plan",
            Step::Build => "build",
            Step::Review => "review",
            Step::Ship => "ship",
            Step::Reflect => "reflect",
            Step::Drift => "drift",
            Step::Done => "done",
        }
    }
}

/// The `ritual-<key>: <value>` lines on an epic. The newest line of each key
/// wins.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Markers(pub BTreeMap<String, String>);

impl Markers {
    pub fn parse(description: &str) -> Self {
        let mut found = BTreeMap::new();
        for line in description.lines() {
            let Some(rest) = line.trim().strip_prefix("ritual-") else {
                continue;
            };
            let Some((key, value)) = rest.split_once(':') else {
                continue;
            };
            if key.is_empty() || key.contains(char::is_whitespace) {
                continue;
            }
            found.insert(key.to_owned(), value.trim().to_owned());
        }
        Markers(found)
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(String::as_str)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Planned through the ritual's Think or Spec: it carries a brief or a
    /// design. A bug flow, or a legacy epic, does not.
    pub fn designed(&self) -> bool {
        self.get("brief").is_some() || self.get("design").is_some()
    }

    /// The newest review's verdict: `ritual-review: <path or URL> clean|findings`.
    pub fn review_clean(&self) -> bool {
        self.get("review")
            .and_then(|v| v.split_whitespace().last())
            .is_some_and(|verdict| verdict == "clean")
    }
}

/// `ritual-approved: <path> sha256:<hex> by <name>` split into its parts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Approval {
    pub design: String,
    pub sha256: String,
}

impl Approval {
    pub fn parse(value: &str) -> Option<Self> {
        let mut words = value.split_whitespace();
        let design = words.next()?.to_owned();
        let sha256 = words.find_map(|w| w.strip_prefix("sha256:"))?.to_owned();
        Some(Approval { design, sha256 })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Bead {
    pub id: String,
    pub title: String,
    pub status: String,
    pub parent: Option<String>,
    pub blocked_by: Vec<String>,
    pub ready: bool,
    pub priority: Option<String>,
    /// `claimed_by`, as the store prints it.
    pub owner: Option<String>,
    pub created_at: Option<String>,
    pub done_at: Option<String>,
    pub done_reason: Option<String>,
    /// The longest chain of in-flow blockers under the bead: its column.
    pub layer: usize,
    pub criteria: Vec<Criterion>,
    /// Criteria past the cap, left in the task store.
    pub criteria_more: usize,
    /// The beads it waits on that are not done yet; a hire waits with it.
    pub open_blockers: Vec<String>,
    /// What a hire or a copied prompt carries: where, what, done when, and
    /// what it waits on and unblocks (`handoff`).
    pub handoff: String,
}

/// One checkbox line under a bead's `## Acceptance criteria`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Criterion {
    pub done: bool,
    pub text: String,
}

/// At most this many criteria per bead reach the page, each at most
/// `CRITERION_CHARS` characters.
pub const CRITERIA_CAP: usize = 20;
pub const CRITERION_CHARS: usize = 300;

/// The `- [ ]` and `- [x]` lines under `## Acceptance criteria`, up to the
/// next heading; the rest of the description is not kept.
pub fn criteria(description: &str) -> (Vec<Criterion>, usize) {
    let mut out = Vec::new();
    let mut more = 0;
    let mut inside = false;
    for line in description.lines() {
        let t = line.trim();
        if t.starts_with('#') {
            inside = t
                .trim_start_matches('#')
                .trim()
                .eq_ignore_ascii_case("acceptance criteria");
            continue;
        }
        if !inside {
            continue;
        }
        let item = t
            .strip_prefix("- [ ]")
            .map(|r| (false, r))
            .or_else(|| t.strip_prefix("- [x]").map(|r| (true, r)))
            .or_else(|| t.strip_prefix("- [X]").map(|r| (true, r)));
        let Some((done, text)) = item else {
            continue;
        };
        if out.len() == CRITERIA_CAP {
            more += 1;
            continue;
        }
        out.push(Criterion {
            done,
            text: truncate(text.trim(), CRITERION_CHARS),
        });
    }
    (out, more)
}

/// Why a flow's next move is the user's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NeedsYou {
    /// A design waits for approval.
    Gate,
    /// The design changed after it was approved.
    Drift,
    /// The newest review found problems.
    Findings,
}

fn needs_you(step: Step, approved: bool, markers: &Markers) -> Option<NeedsYou> {
    let findings = markers
        .get("review")
        .and_then(|v| v.split_whitespace().last())
        == Some("findings");
    match step {
        Step::Spec if !approved && markers.get("design").is_some() => Some(NeedsYou::Gate),
        Step::Drift => Some(NeedsYou::Drift),
        Step::Review if findings => Some(NeedsYou::Findings),
        _ => None,
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Counts {
    pub open: usize,
    pub in_progress: usize,
    pub done: usize,
    pub blocked: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct Flow {
    pub repo: String,
    pub id: String,
    pub title: String,
    pub kind: String,
    pub status: String,
    pub step: Step,
    pub legacy: bool,
    pub approved: bool,
    /// The scan ran out of time before it could compare the approved design
    /// with its digest, so `drift` was not ruled out.
    pub drift_unchecked: bool,
    pub markers: Markers,
    pub counts: Counts,
    pub beads: Vec<Bead>,
    pub needs_you: Option<NeedsYou>,
    /// The latest `created_at` or `done_at` of its beads.
    pub touched: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Scan {
    pub flows: Vec<Flow>,
    /// Repos that could not be read, and why.
    pub gaps: Vec<String>,
    pub repos: usize,
}

/// sha256 of a file, as `shasum -a 256` prints it.
pub fn sha256_file(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    Ok(sha256(&bytes))
}

pub fn sha256(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

pub const GIT_CALL: Duration = Duration::from_secs(10);
/// Below this, a `git` call could be killed mid-answer and a timeout read as
/// "no such branch".
const GIT_FLOOR: Duration = Duration::from_secs(1);

fn git(repo: &Path, args: &[&str], deadline: Duration) -> Result<String> {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(repo).args(args);
    let out = proc::run(&mut cmd, deadline)?;
    if !out.status.success() {
        anyhow::bail!("git {} failed: {}", args.join(" "), out.stderr.trim());
    }
    Ok(out.stdout)
}

/// The design as the flow holds it: committed on `branch` while that branch
/// exists (Spec and Build work there), else the file in `repo`'s working tree
/// (after the branch merged and was deleted). Both `git` calls share `budget`.
pub fn read_design(
    repo: &Path,
    branch: Option<&str>,
    path: &str,
    budget: Duration,
) -> Result<Vec<u8>> {
    if let Some(branch) = branch {
        let until = Deadline::after(budget);
        let call = || {
            let left = until.left().min(GIT_CALL);
            anyhow::ensure!(left >= GIT_FLOOR, "out of time reading {path} on {branch}");
            Ok(left)
        };
        let reference = format!("refs/heads/{branch}");
        if git(
            repo,
            &["rev-parse", "--verify", "--quiet", &reference],
            call()?,
        )
        .is_ok()
        {
            let object = format!("{branch}:{}", path.trim_start_matches("./"));
            return git(repo, &["show", &object], call()?)
                .map(String::into_bytes)
                .with_context(|| format!("{path} is not committed on {branch}"));
        }
    }
    let file = repo.join(path);
    std::fs::read(&file).with_context(|| format!("reading {}", file.display()))
}

/// The main checkout of the repository holding `dir`: the directory whose
/// `.chronis/` store a flow lives in, even when called from a linked worktree.
pub fn main_checkout(dir: &Path) -> Result<PathBuf> {
    let common = git(
        dir,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        GIT_CALL,
    )?;
    let common = PathBuf::from(common.trim());
    common
        .parent()
        .map(Path::to_path_buf)
        .context("the git common dir has no parent")
}

/// A linked worktree keeps a `.git` file, not a directory. Its `.chronis/`
/// store is separate from the main checkout's and dies with the worktree.
fn is_linked_worktree(dir: &Path) -> bool {
    dir.join(".git").is_file()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn is_flow_root(task: &Task, has_children: bool) -> bool {
    if task.archived || task.parent.as_deref().is_some_and(|p| !p.is_empty()) {
        return false;
    }
    match task.task_type.as_str() {
        "epic" | "feature" => true,
        "bug" => has_children,
        _ => false,
    }
}

fn is_done(status: &str) -> bool {
    status == "done"
}

/// Where and until when the drift check may run: the repo's main checkout,
/// and the scan's deadline.
pub struct DriftCheck<'a> {
    pub dir: &'a Path,
    pub until: &'a Deadline<'a>,
}

/// Every flow in one repo's task list. `drift` resolves design paths for the
/// drift check; `None` skips it.
pub fn flows_in(repo: &str, drift: Option<&DriftCheck>, tasks: &[Task]) -> Vec<Flow> {
    let mut children: HashMap<&str, Vec<&Task>> = HashMap::new();
    for t in tasks.iter().filter(|t| !t.archived) {
        if let Some(parent) = t.parent.as_deref().filter(|p| !p.is_empty()) {
            children.entry(parent).or_default().push(t);
        }
    }
    let status: HashMap<&str, &str> = tasks
        .iter()
        .map(|t| (t.id.as_str(), t.status.as_str()))
        .collect();

    let mut flows: Vec<Flow> = tasks
        .iter()
        .filter(|t| is_flow_root(t, children.contains_key(t.id.as_str())))
        .map(|root| {
            let beads = descendants(root, &children, &status);
            let markers = Markers::parse(root.description.as_deref().unwrap_or(""));
            let approved = root.approved.unwrap_or(false);
            let checked = if approved {
                drifted(&markers, drift)
            } else {
                Some(false)
            };
            let drifted = checked.unwrap_or(false);
            let step = infer(&Facts {
                kind: &root.task_type,
                epic_done: is_done(&root.status),
                markers: &markers,
                approved,
                drifted,
                beads: &beads,
            });
            let touched = beads
                .iter()
                .flat_map(|b| [b.created_at.as_deref(), b.done_at.as_deref()])
                .flatten()
                .max()
                .map(str::to_owned);
            Flow {
                needs_you: needs_you(step, approved, &markers),
                touched,
                repo: repo.to_owned(),
                id: root.id.clone(),
                title: root.title.clone(),
                kind: root.task_type.clone(),
                status: root.status.clone(),
                step,
                legacy: markers.is_empty(),
                approved,
                drift_unchecked: checked.is_none(),
                counts: count(&beads),
                markers,
                beads,
            }
        })
        .collect();
    for f in &mut flows {
        let hands: Vec<String> = f.beads.iter().map(|b| handoff(f, b)).collect();
        for (b, h) in f.beads.iter_mut().zip(hands) {
            b.handoff = h;
        }
    }
    flows.sort_by(|a, b| a.id.cmp(&b.id));
    flows
}

/// One bead as work for an agent: the repo and flow it sits in, the bead,
/// its acceptance criteria as the finish line, what it waits on and what it
/// unblocks, and where the full record is. Built here so the copied prompt
/// and a hire from the desktop app carry the same words.
pub fn handoff(flow: &Flow, bead: &Bead) -> String {
    let title_of = |id: &str| {
        flow.beads
            .iter()
            .find(|b| b.id == id)
            .map(|b| format!("{id} {}", b.title))
            .unwrap_or_else(|| format!("{id} (another flow)"))
    };
    let mut s = format!(
        "Work one bead of a ritual flow in the {repo} repo.\n\n\
         Flow: {fid} {ftitle} (step: {step})\n\
         Bead: {id} {title}\n",
        repo = flow.repo,
        fid = flow.id,
        ftitle = flow.title,
        step = flow.step.as_str(),
        id = bead.id,
        title = bead.title,
    );
    if let Some(p) = &bead.priority {
        s.push_str(&format!("Priority: {p}\n"));
    }
    s.push_str("\nDone when:\n");
    if bead.criteria.is_empty() {
        s.push_str(
            "- (no acceptance criteria on this bead: agree the finish line before changing code)\n",
        );
    }
    for c in &bead.criteria {
        s.push_str(&format!(
            "- [{}] {}\n",
            if c.done { "x" } else { " " },
            c.text
        ));
    }
    if bead.criteria_more > 0 {
        s.push_str(&format!(
            "- and {} more in the task store\n",
            bead.criteria_more
        ));
    }
    if !bead.blocked_by.is_empty() {
        s.push_str("\nWaits on:\n");
        for id in &bead.blocked_by {
            let open = if bead.open_blockers.contains(id) {
                " (not done yet)"
            } else {
                ""
            };
            s.push_str(&format!("- {}{open}\n", title_of(id)));
        }
    }
    let unblocks: Vec<&Bead> = flow
        .beads
        .iter()
        .filter(|b| b.blocked_by.contains(&bead.id))
        .collect();
    if !unblocks.is_empty() {
        s.push_str("\nUnblocks:\n");
        for b in unblocks {
            s.push_str(&format!("- {} {}\n", b.id, b.title));
        }
    }
    s.push_str(&format!(
        "\nRead the full record first: cn show {} --toon\n\
         Claim it before starting (cn claim {} --toon) and close it with cn done when every criterion holds.\n",
        bead.id, bead.id
    ));
    s
}

/// All descendants through `parent`, nearest first.
fn descendants(
    root: &Task,
    children: &HashMap<&str, Vec<&Task>>,
    status: &HashMap<&str, &str>,
) -> Vec<Bead> {
    let mut out = Vec::new();
    let mut seen = HashSet::from([root.id.as_str()]);
    let mut queue = vec![root.id.as_str()];
    while let Some(id) = queue.pop() {
        for child in children.get(id).into_iter().flatten() {
            if !seen.insert(child.id.as_str()) {
                continue;
            }
            let open_blockers: Vec<String> = child
                .blocked_by
                .iter()
                .filter(|b| status.get(b.as_str()).is_none_or(|s| !is_done(s)))
                .cloned()
                .collect();
            let blocked = !open_blockers.is_empty();
            let (criteria, criteria_more) = criteria(child.description.as_deref().unwrap_or(""));
            out.push(Bead {
                id: child.id.clone(),
                title: child.title.clone(),
                status: child.status.clone(),
                parent: child.parent.clone(),
                blocked_by: child.blocked_by.clone(),
                ready: child.status == "open" && !blocked,
                priority: child.priority.clone(),
                owner: child.claimed_by.clone(),
                created_at: child.created_at.clone(),
                done_at: child.done_at.clone(),
                done_reason: child.done_reason.clone(),
                layer: 0,
                criteria,
                criteria_more,
                open_blockers,
                handoff: String::new(),
            });
            queue.push(child.id.as_str());
        }
    }
    let layer: HashMap<String, usize> = layers(&out)
        .into_iter()
        .map(|(id, d)| (id.to_owned(), d))
        .collect();
    for b in &mut out {
        b.layer = layer.get(&b.id).copied().unwrap_or(0);
    }
    out
}

fn count(beads: &[Bead]) -> Counts {
    let mut c = Counts::default();
    for b in beads {
        match b.status.as_str() {
            "done" => c.done += 1,
            "in-progress" => c.in_progress += 1,
            _ => c.open += 1,
        }
        if !is_done(&b.status) && !b.ready && b.status == "open" {
            c.blocked += 1;
        }
    }
    c
}

/// Whether the design changed since it was approved; `None` when the scan's
/// deadline left no time to look. A design that cannot be read is not drift;
/// `cos ritual check` reports it as missing.
fn drifted(markers: &Markers, drift: Option<&DriftCheck>) -> Option<bool> {
    let (Some(check), Some(approval)) = (drift, markers.get("approved").and_then(Approval::parse))
    else {
        return Some(false);
    };
    let left = check.until.left();
    if left < GIT_FLOOR {
        return None;
    }
    Some(
        read_design(check.dir, markers.get("branch"), &approval.design, left)
            .is_ok_and(|bytes| sha256(&bytes) != approval.sha256),
    )
}

/// What the step is inferred from.
pub struct Facts<'a> {
    pub kind: &'a str,
    pub epic_done: bool,
    pub markers: &'a Markers,
    pub approved: bool,
    pub drifted: bool,
    pub beads: &'a [Bead],
}

/// The ritual step, first match wins (the design's table).
pub fn infer(f: &Facts) -> Step {
    let legacy = f.markers.is_empty();
    let designed = f.markers.designed() && f.kind != "bug";
    if f.epic_done && (legacy || f.markers.get("report").is_some()) {
        return Step::Done;
    }
    if f.epic_done || f.markers.get("shipped").is_some() {
        return Step::Reflect;
    }
    if f.drifted {
        return Step::Drift;
    }
    if designed && !f.approved {
        return if f.markers.get("design").is_some() {
            Step::Spec
        } else {
            Step::Think
        };
    }
    if f.beads.is_empty() {
        return Step::Plan;
    }
    if f.beads.iter().any(|b| !is_done(&b.status)) {
        return Step::Build;
    }
    if !legacy && !f.markers.review_clean() {
        return Step::Review;
    }
    Step::Ship
}

/// `cn list --format json --all` in `repo`, killed at `deadline`.
pub fn read_tasks(cn: &Path, repo: &Path, deadline: Duration) -> Result<Vec<Task>> {
    let mut cmd = Command::new(cn);
    cmd.current_dir(repo)
        .args(["list", "--format", "json", "--all"]);
    let out = proc::run(&mut cmd, deadline)?;
    if !out.status.success() {
        anyhow::bail!("cn list failed: {}", out.stderr.trim());
    }
    serde_json::from_str(&out.stdout)
        .context("cn list printed something other than a JSON task list")
}

/// The main checkouts directly under `projects` that keep a `.chronis/`
/// store. Linked worktrees are left out: their store is not the flow's.
pub fn repos(projects: &Path) -> Result<Vec<PathBuf>> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(projects)
        .with_context(|| format!("reading {}", projects.display()))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.join(".chronis").is_dir() && !is_linked_worktree(p))
        .collect();
    found.sort();
    Ok(found)
}

/// Told before each repo is read: (repos done, repos in all, the next one).
pub type Progress<'a> = &'a (dyn Fn(usize, usize, &str) + Sync);

/// Every flow under `projects`, read with `cn` under one `budget`.
pub fn scan(projects: &Path, cn: &Path, budget: Duration) -> Scan {
    scan_with(projects, cn, budget, &|_, _, _| {})
}

/// `scan`, telling `progress` as it moves from repo to repo.
pub fn scan_with(projects: &Path, cn: &Path, budget: Duration, progress: Progress) -> Scan {
    let mut out = Scan::default();
    let repos = match repos(projects) {
        Ok(r) => r,
        Err(e) => {
            out.gaps.push(format!("{e:#}"));
            return out;
        }
    };
    out.repos = repos.len();
    let deadline = Deadline::after(budget);
    for (done, repo) in repos.into_iter().enumerate() {
        let name = repo
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        progress(done, out.repos, &name);
        if deadline.expired() {
            out.gaps
                .push(format!("{name}: not read, the scan ran out of time"));
            continue;
        }
        match read_tasks(cn, &repo, deadline.left().min(PER_REPO)) {
            Ok(tasks) => {
                let check = DriftCheck {
                    dir: &repo,
                    until: &deadline,
                };
                let found = flows_in(&name, Some(&check), &tasks);
                let unchecked = found.iter().filter(|f| f.drift_unchecked).count();
                if unchecked > 0 {
                    out.gaps.push(format!(
                        "{name}: {unchecked} approved flow(s) not checked for drift, the scan ran out of time"
                    ));
                }
                out.flows.extend(found);
            }
            Err(e) => out.gaps.push(format!("{name}: {e:#}")),
        }
    }
    out
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_owned();
    }
    let cut: String = s.chars().take(max.saturating_sub(1)).collect();
    format!("{cut}…")
}

/// Layer of each bead: the longest chain of in-flow blockers under it. A cycle
/// is broken where it is found, so a bad graph still draws.
fn layers(beads: &[Bead]) -> HashMap<&str, usize> {
    let ids: HashSet<&str> = beads.iter().map(|b| b.id.as_str()).collect();
    let by_id: HashMap<&str, &Bead> = beads.iter().map(|b| (b.id.as_str(), b)).collect();
    fn depth<'a>(
        id: &'a str,
        by_id: &HashMap<&'a str, &'a Bead>,
        ids: &HashSet<&str>,
        memo: &mut HashMap<&'a str, usize>,
        visiting: &mut HashSet<&'a str>,
    ) -> usize {
        if let Some(d) = memo.get(id) {
            return *d;
        }
        if !visiting.insert(id) {
            return 0;
        }
        let d = by_id[id]
            .blocked_by
            .iter()
            .filter(|b| ids.contains(b.as_str()))
            .map(|b| {
                1 + depth(
                    by_id.get_key_value(b.as_str()).unwrap().0,
                    by_id,
                    ids,
                    memo,
                    visiting,
                )
            })
            .max()
            .unwrap_or(0);
        visiting.remove(id);
        memo.insert(id, d);
        d
    }
    let mut memo = HashMap::new();
    let mut visiting = HashSet::new();
    for b in beads {
        depth(b.id.as_str(), &by_id, &ids, &mut memo, &mut visiting);
    }
    memo
}

/// The chronis binary: `COS_CN`, else `cn` on `PATH`.
pub fn cn_program() -> PathBuf {
    std::env::var_os("COS_CN")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("cn"))
}

/// How long a rendered `/flows` page is served again before a new scan.
const PAGE_TTL: Duration = Duration::from_secs(10);

/// `page`, at most one scan at a time: a reload or a second window waits for
/// the scan already running and gets its result, and a page younger than
/// `PAGE_TTL` is served without scanning unless `fresh` (the page's Rescan).
/// The page carries the time of its own scan, so a cached one says its age.
pub fn page_cached(ws: &Path, projects: &Path, fresh: bool) -> String {
    type Entry = (std::time::Instant, PathBuf, PathBuf, String);
    static LAST: std::sync::Mutex<Option<Entry>> = std::sync::Mutex::new(None);
    let mut last = LAST
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some((at, w, p, html)) = last.as_ref()
        && !fresh
        && at.elapsed() < PAGE_TTL
        && w == ws
        && p == projects
    {
        return html.clone();
    }
    let html = page(ws, projects);
    *last = Some((
        std::time::Instant::now(),
        ws.to_path_buf(),
        projects.to_path_buf(),
        html.clone(),
    ));
    html
}

/// The desktop's `/flows`: the last page it rendered, kept so a tab open
/// shows it at once while a scan runs behind it (2.1 to 2.8 s of `cn` over
/// 46 flows on the machine this was measured on). One scan at a time.
#[derive(Default)]
pub struct Pages {
    last: std::sync::Mutex<Option<(std::time::Instant, PathBuf, PathBuf, String)>>,
    scanning: std::sync::Mutex<()>,
}

impl Pages {
    /// The page held for this workspace and projects directory, with its age.
    pub fn cached(&self, ws: &Path, projects: &Path) -> Option<(Duration, String)> {
        let last = self
            .last
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        last.as_ref()
            .filter(|(_, w, p, _)| w == ws && p == projects)
            .map(|(at, _, _, html)| (at.elapsed(), html.clone()))
    }

    /// Scans and keeps the page. A caller that finds a scan running waits for
    /// it and, unless `fresh`, takes its page instead of scanning again.
    pub fn scan(&self, ws: &Path, projects: &Path, fresh: bool, progress: Progress) -> String {
        let _one = self
            .scanning
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !fresh
            && let Some((age, html)) = self.cached(ws, projects)
            && age < PAGE_TTL
        {
            return html;
        }
        self.scan_held(ws, projects, progress)
    }

    /// Scans only when no scan is running; `None` when one is.
    pub fn try_scan(&self, ws: &Path, projects: &Path, progress: Progress) -> Option<String> {
        let _one = self.scanning.try_lock().ok()?;
        Some(self.scan_held(ws, projects, progress))
    }

    fn scan_held(&self, ws: &Path, projects: &Path, progress: Progress) -> String {
        let html = page_with(ws, projects, progress);
        *self
            .last
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some((
            std::time::Instant::now(),
            ws.to_path_buf(),
            projects.to_path_buf(),
            html.clone(),
        ));
        html
    }
}

/// A held page older than this is shown, and scanned again behind it.
pub const STALE: Duration = PAGE_TTL;

/// `/flows` for `cos dash` and the desktop app: read only when the profile
/// approves the `tasks` source, the gate the brief's `cn` facts use.
pub fn page(ws: &Path, projects: &Path) -> String {
    page_with(ws, projects, &|_, _, _| {})
}

/// `page`, telling `progress` as the scan moves from repo to repo.
pub fn page_with(ws: &Path, projects: &Path, progress: Progress) -> String {
    let approved = crate::context::local_reads(ws).tasks;
    let scan = if approved {
        scan_with(projects, &cn_program(), SCAN_BUDGET, progress)
    } else {
        Scan {
            gaps: vec![
                "the profile does not approve a tasks source, so no task store was read".into(),
            ],
            ..Scan::default()
        }
    };
    page_html(&scan, approved).unwrap_or_else(|e| format!("flows page: {e:#}"))
}

/// The Content-Security-Policy `/flows` is served with: its one inline
/// script and style, the inlined fonts and images, and nothing to fetch.
pub const PAGE_CSP: &str = "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; font-src data:; img-src data:";
/// `PAGE_CSP` for `cos dash`, where the page is top-level: no other site may
/// frame it, because a framing page would receive the messages it posts to
/// its parent. The desktop frames it itself, so its copy cannot carry this.
pub const DASH_CSP: &str = "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; font-src data:; img-src data:; frame-ancestors 'none'";

const TEMPLATE: &str = include_str!("flows.html");
/// Where the page's one script takes the scan.
pub const FLOWS_SLOT: &str = "/*__COS_FLOWS__*/";

/// The page `cos dash` serves at `/flows` and the desktop app shows in its
/// Flows tab: the scan inlined as JSON, drawn by the page's own script.
/// `tasks_approved` is false when the profile approves no tasks source.
pub fn page_html(scan: &Scan, tasks_approved: bool) -> Result<String> {
    page_html_from(TEMPLATE, scan, tasks_approved)
}

/// `page_html` over another template, so a test can hand it one with a slot
/// missing. The slots are filled before the data, so a title that reads like
/// a slot stays a title.
#[doc(hidden)]
pub fn page_html_from(template: &str, scan: &Scan, tasks_approved: bool) -> Result<String> {
    anyhow::ensure!(
        template.contains(FLOWS_SLOT),
        "flows template lost its data slot"
    );
    anyhow::ensure!(
        template.contains(crate::design::VGPU_SLOT),
        "flows template lost its vgpu slot"
    );
    let scanned_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let data = serde_json::json!({
        "flows": scan.flows,
        "gaps": scan.gaps,
        "repos": scan.repos,
        "scanned_at": scanned_at,
        "tasks_approved": tasks_approved,
    });
    let json = crate::design::script_json(&data)?;
    Ok(crate::design::inline(template)?
        .replacen(crate::design::VGPU_SLOT, crate::design::vgpu_js(), 1)
        .replacen(FLOWS_SLOT, &json, 1))
}

/// The `ritual-*` files a flow's page may ask the window to open.
pub const OPEN_KEYS: [&str; 6] = ["brief", "design", "plan", "review", "report", "pr"];

/// What a `ritual-<key>:` line names, once checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// A path relative to the flow's repo, to be located on its branch.
    Path(String),
    /// A GitHub URL.
    Url(String),
}

/// One bead of one flow, from the repo's own task list, as a hire works from
/// it: refused when it is done or waits on a bead that is not. The page only
/// names it; everything a hire carries is read here.
pub fn bead_to_hire(repo: &str, tasks: &[Task], epic: &str, bead: &str) -> Result<Bead> {
    let flow = flows_in(repo, None, tasks)
        .into_iter()
        .find(|f| f.id == epic)
        .with_context(|| format!("{epic} is not a flow in {repo}"))?;
    let b = flow
        .beads
        .into_iter()
        .find(|b| b.id == bead)
        .with_context(|| format!("{bead} is not a bead of {epic}"))?;
    anyhow::ensure!(!is_done(&b.status), "{bead} is already done");
    anyhow::ensure!(
        b.open_blockers.is_empty(),
        "{bead} waits on {}: finish that first",
        b.open_blockers.join(", ")
    );
    Ok(b)
}

/// The file or link `epic`'s `ritual-<key>:` line names, from the repo's own
/// task list, refused unless it is safe to hand to `open`: a known key, an
/// epic the store printed, a GitHub URL, or a relative `.md` or `.html` path
/// with no `..`. The value never comes from the caller.
pub fn open_target(tasks: &[Task], epic: &str, key: &str) -> Result<Target> {
    anyhow::ensure!(OPEN_KEYS.contains(&key), "{key} is not a file a flow names");
    let task = tasks
        .iter()
        .find(|t| t.id == epic)
        .with_context(|| format!("{epic} is not in this repo's task store"))?;
    let markers = Markers::parse(task.description.as_deref().unwrap_or(""));
    let value = markers
        .get(key)
        .with_context(|| format!("{epic} names no {key}"))?;
    let value = if key == "review" {
        value.split_whitespace().next().unwrap_or("")
    } else {
        value.trim()
    };
    if value.contains(':') || key == "pr" {
        anyhow::ensure!(
            value.starts_with("https://github.com/") && !value.contains(char::is_whitespace),
            "only GitHub links open from Flows: {value}"
        );
        return Ok(Target::Url(value.to_owned()));
    }
    Ok(Target::Path(openable_path(value)?.to_owned()))
}

/// `path` when it is relative, has no `..` and names a `.md` or `.html`
/// file: the shape a ritual file may have before it is located.
pub fn openable_path(path: &str) -> Result<&str> {
    let path = path.trim_start_matches("./");
    let p = Path::new(path);
    anyhow::ensure!(
        !path.is_empty() && p.is_relative(),
        "{path} is not a path inside the repo"
    );
    anyhow::ensure!(
        p.components()
            .all(|c| matches!(c, std::path::Component::Normal(_))),
        "{path} leaves the repo"
    );
    anyhow::ensure!(readable_kind(p), "{path} is not a Markdown or HTML file");
    Ok(path)
}

fn readable_kind(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("md") || e.eq_ignore_ascii_case("html"))
}

/// `file`, canonicalised (symlinks followed), when it sits inside `root` and
/// is still a Markdown or HTML file: the path that may be handed to `open`.
pub fn contained(root: &Path, file: &Path) -> Result<PathBuf> {
    let root = root
        .canonicalize()
        .with_context(|| format!("reading {}", root.display()))?;
    let real = file
        .canonicalize()
        .with_context(|| format!("reading {}", file.display()))?;
    anyhow::ensure!(
        real.starts_with(&root),
        "{} resolves outside {}",
        file.display(),
        root.display()
    );
    anyhow::ensure!(
        readable_kind(&real) && real.is_file(),
        "{} is not a Markdown or HTML file",
        file.display()
    );
    Ok(real)
}

/// One line per flow, for the terminal.
pub fn render(scan: &Scan) -> String {
    let mut lines = Vec::new();
    for f in &scan.flows {
        let c = &f.counts;
        lines.push(format!(
            "{:<8} {:<24} {:<10} {}{}  [{} open, {} in progress, {} done{}]{}",
            f.step.as_str(),
            f.repo,
            f.id,
            f.title,
            if f.legacy { " (legacy)" } else { "" },
            c.open,
            c.in_progress,
            c.done,
            if c.blocked > 0 {
                format!(", {} blocked", c.blocked)
            } else {
                String::new()
            },
            if f.step == Step::Spec && !f.approved && f.markers.get("design").is_some() {
                format!(
                    "\n         waiting for your approval: {}",
                    crate::ritual::APPROVE_WHERE
                )
            } else {
                String::new()
            },
        ));
    }
    if scan.flows.is_empty() {
        lines.push(format!("no flows in {} repos with .chronis/", scan.repos));
    }
    for gap in &scan.gaps {
        lines.push(format!("gap: {gap}"));
    }
    lines.join("\n")
}
