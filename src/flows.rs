//! `cos flows`: every planned piece of work on this machine, one Chronis epic
//! per flow, with the ritual step it is in (`.claude/skills/ritual`,
//! `docs/ritual.md`).
//!
//! Read-only. Each repo under the projects root with `.chronis/` is read once
//! with `cn list --format json --all`, all under one deadline. Descriptions are
//! parsed for `ritual-*` markers and dropped.

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
            Flow {
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
    flows.sort_by(|a, b| a.id.cmp(&b.id));
    flows
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
            let blocked = child
                .blocked_by
                .iter()
                .any(|b| status.get(b.as_str()).is_none_or(|s| !is_done(s)));
            out.push(Bead {
                id: child.id.clone(),
                title: child.title.clone(),
                status: child.status.clone(),
                parent: child.parent.clone(),
                blocked_by: child.blocked_by.clone(),
                ready: child.status == "open" && !blocked,
            });
            queue.push(child.id.as_str());
        }
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

/// Every flow under `projects`, read with `cn` under one `budget`.
pub fn scan(projects: &Path, cn: &Path, budget: Duration) -> Scan {
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
    for repo in repos {
        let name = repo
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
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

const STEPS: [Step; 9] = [
    Step::Think,
    Step::Spec,
    Step::Plan,
    Step::Build,
    Step::Review,
    Step::Ship,
    Step::Reflect,
    Step::Drift,
    Step::Done,
];

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
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

/// The flow's beads as a layered SVG: blockers on the left, ready beads
/// outlined. Unitless coordinates; the page's tokens style it.
pub fn graph_svg(beads: &[Bead]) -> String {
    const COL: usize = 230;
    const ROW: usize = 58;
    const W: usize = 200;
    const H: usize = 42;
    if beads.is_empty() {
        return "<p class=\"empty\">No beads yet.</p>".into();
    }
    let layer = layers(beads);
    let mut rows: BTreeMap<usize, Vec<&Bead>> = BTreeMap::new();
    for b in beads {
        rows.entry(layer[b.id.as_str()]).or_default().push(b);
    }
    let mut at: HashMap<&str, (usize, usize)> = HashMap::new();
    for (col, list) in &rows {
        for (row, b) in list.iter().enumerate() {
            at.insert(b.id.as_str(), (col * COL + 10, row * ROW + 10));
        }
    }
    let width = rows.len() * COL;
    let height = rows.values().map(Vec::len).max().unwrap_or(1) * ROW + 10;
    let mut svg = format!(
        "<svg class=\"graph\" viewBox=\"0 0 {width} {height}\" role=\"img\" aria-label=\"bead dependency graph\">"
    );
    for b in beads {
        let (x, y) = at[b.id.as_str()];
        for blocker in &b.blocked_by {
            if let Some((bx, by)) = at.get(blocker.as_str()) {
                svg.push_str(&format!(
                    "<path class=\"edge\" d=\"M{} {} C{} {} {} {} {} {}\"/>",
                    bx + W,
                    by + H / 2,
                    bx + W + 15,
                    by + H / 2,
                    x - 15,
                    y + H / 2,
                    x,
                    y + H / 2
                ));
            }
        }
    }
    for b in beads {
        let (x, y) = at[b.id.as_str()];
        let class = format!(
            "node {}{}",
            esc(&b.status),
            if b.ready { " ready" } else { "" }
        );
        svg.push_str(&format!(
            "<g class=\"{class}\"><title>{} · {} · {}</title><rect x=\"{x}\" y=\"{y}\" width=\"{W}\" height=\"{H}\" rx=\"6\"/><text x=\"{}\" y=\"{}\">{}</text><text class=\"sub\" x=\"{}\" y=\"{}\">{}</text></g>",
            esc(&b.id),
            esc(&b.status),
            esc(&b.title),
            x + 8,
            y + 17,
            esc(&b.id),
            x + 8,
            y + 33,
            esc(&truncate(&b.title, 30)),
        ));
    }
    svg.push_str("</svg>");
    svg
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
/// `PAGE_TTL` is served without scanning.
pub fn page_cached(ws: &Path, projects: &Path) -> String {
    type Entry = (std::time::Instant, PathBuf, PathBuf, String);
    static LAST: std::sync::Mutex<Option<Entry>> = std::sync::Mutex::new(None);
    let mut last = LAST
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some((at, w, p, html)) = last.as_ref()
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

/// `/flows` for `cos dash` and the desktop app: read only when the profile
/// approves the `tasks` source, the gate the brief's `cn` facts use.
pub fn page(ws: &Path, projects: &Path) -> String {
    if !crate::context::local_reads(ws).tasks {
        return html(&Scan {
            gaps: vec![
                "the profile does not approve a tasks source, so no task store was read".into(),
            ],
            ..Scan::default()
        });
    }
    html(&scan(projects, &cn_program(), SCAN_BUDGET))
}

/// The page `cos dash` serves at `/flows` and the desktop app shows in its
/// Flows tab.
pub fn html(scan: &Scan) -> String {
    let page = crate::design::inline(include_str!("flows.html"))
        .expect("flows.html carries the token slot");
    let summary = format!("{} flows in {} repos", scan.flows.len(), scan.repos);
    let steps: String = STEPS
        .iter()
        .filter_map(|s| {
            let n = scan.flows.iter().filter(|f| f.step == *s).count();
            (n > 0).then(|| format!("<li><b>{n}</b> {}</li>", s.as_str()))
        })
        .collect();
    let flows: String = if scan.flows.is_empty() {
        "<p class=\"empty\">No flows yet. The ritual skill starts one with a Chronis epic.</p>"
            .into()
    } else {
        // The first flow being built opens with its graph showing; the rest stay folded.
        let open = scan
            .flows
            .iter()
            .position(|f| f.step == Step::Build)
            .unwrap_or(0);
        scan.flows
            .iter()
            .enumerate()
            .map(|(i, f)| flow_card(f, i == open))
            .collect()
    };
    let gaps = if scan.gaps.is_empty() {
        String::new()
    } else {
        format!(
            "<div class=\"gaps\"><b>Not read</b><ul>{}</ul></div>",
            scan.gaps
                .iter()
                .map(|g| format!("<li>{}</li>", esc(g)))
                .collect::<String>()
        )
    };
    page.replace("<!--__SUMMARY__-->", &esc(&summary))
        .replace(
            "<!--__STEPS__-->",
            &format!("<ul class=\"steps\">{steps}</ul>"),
        )
        .replace("<!--__FLOWS__-->", &flows)
        .replace("<!--__GAPS__-->", &gaps)
}

fn flow_card(f: &Flow, open: bool) -> String {
    let c = &f.counts;
    let ready = f.beads.iter().filter(|b| b.ready).count();
    let mut meta = vec![format!(
        "cn approve: <code>{}</code>",
        if f.approved { "yes" } else { "no" }
    )];
    if f.legacy {
        meta.push("legacy flow: no <code>ritual-*</code> lines".into());
    }
    for (key, value) in &f.markers.0 {
        meta.push(format!("{}: <code>{}</code>", esc(key), esc(value)));
    }
    format!(
        "<details class=\"flow\"{unfolded}><summary><span class=\"step {step}\">{step}</span><span class=\"repo\">{repo}</span><span class=\"title\">{title} <small>{id}</small></span><span class=\"counts\">{open} open · {prog} in progress · {done} done · {ready} ready{blocked}</span></summary><div class=\"body\"><ul class=\"meta\">{meta}</ul>{graph}<p class=\"legend\">Left to right: blockers first. Outlined in green: ready now.</p></div></details>",
        unfolded = if open { " open" } else { "" },
        step = f.step.as_str(),
        repo = esc(&f.repo),
        title = esc(&f.title),
        id = esc(&f.id),
        open = c.open,
        prog = c.in_progress,
        done = c.done,
        blocked = if c.blocked > 0 {
            format!(" · {} blocked", c.blocked)
        } else {
            String::new()
        },
        meta = meta
            .iter()
            .map(|m| format!("<li>{m}</li>"))
            .collect::<String>(),
        graph = graph_svg(&f.beads),
    )
}

/// One line per flow, for the terminal.
pub fn render(scan: &Scan) -> String {
    let mut lines = Vec::new();
    for f in &scan.flows {
        let c = &f.counts;
        lines.push(format!(
            "{:<8} {:<24} {:<10} {}{}  [{} open, {} in progress, {} done{}]",
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
