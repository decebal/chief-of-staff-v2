//! The documentation room: every tracked markdown document in the monitored
//! repos, the Notion pages last listed by `cos docs notion`, and the user's
//! own organisation of them (topics, links, "move to Notion" marks) in
//! `context/docs.json`. The page lays the graph out itself (a force-directed
//! SVG, no library) and renders documents with the vendored marked and
//! DOMPurify (`apps/desktop/ui/vendor/README.md`); nothing here is written by
//! a model except the Notion listing itself.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::proc;

const TEMPLATE: &str = include_str!("docs.html");
const DATA_SLOT: &str = "\"__DOCS_DATA__\"";
/// Where the page's one script takes the markdown renderer and sanitizer.
pub const MARKDOWN_SLOT: &str = "/*__COS_MARKDOWN__*/";
const MARKED: &str = include_str!("../apps/desktop/ui/vendor/marked/marked-18.1.2.umd.js");
const PURIFY: &str = include_str!("../apps/desktop/ui/vendor/dompurify/purify-3.4.17.min.js");
/// Largest image the reader serves from a checkout.
const MAX_ASSET: u64 = 8 << 20;

/// Paths that are tooling, vendored or generated, not documentation.
const SKIP: [&str; 6] = [
    "node_modules/",
    ".claude/",
    ".github/",
    "vendor/",
    "CHANGELOG",
    ".agents/",
];

/// One budget for scanning every repo.
const SCAN_BUDGET: Duration = Duration::from_secs(60);
/// For reading one checkout's GitHub remote and branch.
const GITHUB_BUDGET: Duration = Duration::from_secs(5);

/// Headers an SVG is served with: opened on its own it is a sandboxed
/// document with no script, no fetch and no frame; shown through `<img>` it
/// never runs script at all.
pub const SVG_HEADERS: [(&str, &str); 2] = [
    (
        "Content-Security-Policy",
        "sandbox; default-src 'none'; style-src 'unsafe-inline'",
    ),
    ("X-Content-Type-Options", "nosniff"),
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Doc {
    /// `repo:path` for a repo document, `notion:<page id or url>` for a page.
    pub id: String,
    pub title: String,
    /// `repo` or `notion`.
    pub place: String,
    pub repo: Option<String>,
    pub path: Option<String>,
    pub url: Option<String>,
    /// Ids of other documents it links to.
    pub links: Vec<String>,
    /// The topic it falls in when the user has not placed it: `group · kind`.
    pub folder: String,
    /// What sort of document its path says it is: `ADRs`, `Plans`, …
    #[serde(default)]
    pub kind: String,
    /// The bet that owns it (a goal's short title), else its repo.
    #[serde(default)]
    pub group: String,
    /// Where documents of its kind usually belong; the user decides.
    #[serde(default)]
    pub suggest: Option<Mark>,
    /// Tracked files and folders (a folder ends in `/`) it links to that are
    /// not documents in the room; the reader sends those to GitHub.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outside: Vec<String>,
}

/// Where a checkout's files live on GitHub, read from its `origin`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GitHub {
    /// `https://github.com/<org>/<repo>`
    pub base: String,
    /// The current branch when it tracks `origin`, else the remote's default
    /// branch, else `HEAD` (which GitHub resolves to the default branch).
    pub branch: String,
}

/// `https://github.com/<org>/<repo>` for an `origin` URL on github.com in
/// any of git's spellings; `None` for any other host.
pub fn github_base(url: &str) -> Option<String> {
    let u = url.trim();
    let rest = u
        .strip_prefix("git@github.com:")
        .or_else(|| u.strip_prefix("ssh://git@github.com/"))
        .or_else(|| u.strip_prefix("ssh://git@github.com:22/"))
        .or_else(|| u.strip_prefix("https://github.com/"))
        .or_else(|| u.strip_prefix("http://github.com/"))
        .or_else(|| u.strip_prefix("git://github.com/"))
        .or_else(|| {
            // https://user@github.com/… and https://user:token@github.com/…
            let after = u.strip_prefix("https://")?;
            let (cred, path) = after.split_once('@')?;
            (!cred.contains('/'))
                .then_some(path)?
                .strip_prefix("github.com/")
        })?;
    let rest = rest.trim_end_matches('/');
    let rest = rest.strip_suffix(".git").unwrap_or(rest);
    let mut parts = rest.split('/');
    let (org, repo) = (parts.next()?, parts.next()?);
    let ok = |s: &str| {
        !s.is_empty()
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
    };
    (parts.next().is_none() && ok(org) && ok(repo) && repo != "." && repo != "..")
        .then(|| format!("https://github.com/{org}/{repo}"))
}

/// The GitHub page of `path` (a folder when it ends in `/`) at `gh.branch`.
pub fn github_url(gh: &GitHub, path: &str) -> String {
    let (view, p) = match path.strip_suffix('/') {
        Some(dir) => ("tree", dir),
        None => ("blob", path),
    };
    let enc: Vec<String> = p
        .split('/')
        .map(|seg| {
            seg.bytes()
                .map(|b| match b {
                    b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                        (b as char).to_string()
                    }
                    _ => format!("%{b:02X}"),
                })
                .collect()
        })
        .collect();
    format!("{}/{view}/{}/{}", gh.base, gh.branch, enc.join("/"))
}

/// The checkout's GitHub home, or why it has none. Three bounded `git` calls
/// at most, sharing `left`.
pub fn github(dir: &Path, left: Duration) -> std::result::Result<GitHub, String> {
    let until = Instant::now() + left;
    let git = |args: &[&str]| -> Option<String> {
        let left = until.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return None;
        }
        let out = proc::run(Command::new("git").arg("-C").arg(dir).args(args), left).ok()?;
        let s = out.stdout.trim().to_owned();
        (out.status.success() && !s.is_empty()).then_some(s)
    };
    let url = git(&["remote", "get-url", "origin"]).ok_or("the checkout has no origin remote")?;
    let base = github_base(&url).ok_or_else(|| format!("origin is not on GitHub: {url}"))?;
    let branch = git(&[
        "rev-parse",
        "--abbrev-ref",
        "--symbolic-full-name",
        "@{upstream}",
    ])
    .and_then(|u| u.strip_prefix("origin/").map(str::to_owned))
    .or_else(|| {
        git(&["symbolic-ref", "--short", "refs/remotes/origin/HEAD"])
            .and_then(|u| u.strip_prefix("origin/").map(str::to_owned))
    })
    .filter(|b| !b.is_empty())
    .unwrap_or_else(|| "HEAD".into());
    Ok(GitHub { base, branch })
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Link {
    pub from: String,
    pub to: String,
    /// The user's word for it: `depends on`, `supersedes`, …
    pub kind: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mark {
    MoveToNotion,
    KeepInRepo,
}

/// The user's organisation, `context/docs.json`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Organize {
    /// Topic name to the document ids placed in it, in order.
    #[serde(default)]
    pub topics: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub links: Vec<Link>,
    #[serde(default)]
    pub marks: BTreeMap<String, Mark>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NotionPage {
    pub title: String,
    pub url: String,
    #[serde(default)]
    pub parent: String,
    /// Which monitored project it belongs to, as the listing judged it.
    #[serde(default)]
    pub project: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct NotionList {
    pub pages: Vec<NotionPage>,
    /// When the listing ran; empty before the first.
    #[serde(default)]
    pub listed_at: String,
}

pub fn organize_path(home: &Path) -> PathBuf {
    home.join("context/docs.json")
}

pub fn notion_path(home: &Path) -> PathBuf {
    home.join("out/docs/notion.json")
}

/// A missing file is an empty organisation; a broken one is an error, so a
/// save never silently replaces work the user did by hand.
pub fn load_organize(home: &Path) -> Result<Organize> {
    match std::fs::read(organize_path(home)) {
        Ok(bytes) => serde_json::from_slice(&bytes).with_context(|| {
            format!(
                "{} is not a docs organisation",
                organize_path(home).display()
            )
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Organize::default()),
        Err(e) => Err(e.into()),
    }
}

pub fn save_organize(home: &Path, org: &Organize) -> Result<()> {
    let p = organize_path(home);
    std::fs::create_dir_all(p.parent().context("no context dir")?)?;
    crate::state::atomic_write(&p, &serde_json::to_value(org)?)
}

pub fn load_notion(home: &Path) -> NotionList {
    std::fs::read(notion_path(home))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub fn save_notion(home: &Path, list: &NotionList) -> Result<()> {
    let p = notion_path(home);
    std::fs::create_dir_all(p.parent().context("no out dir")?)?;
    crate::state::atomic_write(&p, &serde_json::to_value(list)?)
}

/// `owner/name` slugs from the profile's `monitoredRepos`.
pub fn monitored(workspace: &Path) -> Vec<String> {
    crate::state::profile(workspace)
        .and_then(|p| p["monitoredRepos"].as_array().cloned())
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect()
}

/// The local checkout of each slug under `projects`: the first directory
/// whose `.git/config` names it as `origin`. Reads files; starts no process.
pub fn checkouts(projects: &Path, slugs: &[String]) -> Vec<(String, PathBuf)> {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(projects)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.join(".git/config").is_file())
        .collect();
    dirs.sort();
    slugs
        .iter()
        .filter_map(|slug| {
            let needle = slug.to_ascii_lowercase();
            // Several checkouts can share one origin; the one whose index was
            // written last is the one in use.
            dirs.iter()
                .filter(|d| {
                    std::fs::read_to_string(d.join(".git/config")).is_ok_and(|c| {
                        origin_url(&c).is_some_and(|u| {
                            let u = u.to_ascii_lowercase();
                            u.trim_end_matches(".git").ends_with(&format!("/{needle}"))
                                || u.trim_end_matches(".git").ends_with(&format!(":{needle}"))
                        })
                    })
                })
                .max_by_key(|d| {
                    std::fs::metadata(d.join(".git/index"))
                        .and_then(|m| m.modified())
                        .ok()
                })
                .map(|d| (repo_name(slug), d.clone()))
        })
        .collect()
}

fn origin_url(config: &str) -> Option<String> {
    let mut in_origin = false;
    for line in config.lines().map(str::trim) {
        if line.starts_with('[') {
            in_origin = line == "[remote \"origin\"]";
        } else if in_origin && let Some(v) = line.strip_prefix("url") {
            return Some(v.trim_start().trim_start_matches('=').trim().to_owned());
        }
    }
    None
}

fn repo_name(slug: &str) -> String {
    slug.rsplit('/').next().unwrap_or(slug).to_owned()
}

/// Every tracked markdown document in each checkout, with its title and the
/// links it makes to other documents in the same repo.
pub fn scan(repos: &[(String, PathBuf)]) -> Vec<Doc> {
    let until = Instant::now() + SCAN_BUDGET;
    let mut docs = Vec::new();
    for (name, dir) in repos {
        let left = until.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        let Ok(out) = proc::run(
            Command::new("git")
                .arg("-C")
                .arg(dir)
                .args(["ls-files", "-z"]),
            left,
        ) else {
            continue;
        };
        let tracked: BTreeSet<String> = out
            .stdout
            .split('\0')
            .filter(|f| !f.is_empty())
            .map(str::to_owned)
            .collect();
        let files: BTreeSet<String> = tracked
            .iter()
            .filter(|f| f.ends_with(".md") && !SKIP.iter().any(|s| f.contains(s)))
            .cloned()
            .collect();
        for f in &files {
            let text = std::fs::read_to_string(dir.join(f)).unwrap_or_default();
            let kind = kind(f);
            docs.push(Doc {
                id: format!("{name}:{f}"),
                title: title(&text, f),
                place: "repo".into(),
                repo: Some(name.clone()),
                path: Some(f.clone()),
                url: None,
                links: links(&text, f, &files)
                    .into_iter()
                    .map(|t| format!("{name}:{t}"))
                    .collect(),
                folder: format!("{name} · {kind}"),
                suggest: suggest(&kind),
                kind,
                group: name.clone(),
                outside: outside(&text, f, &tracked, &files),
            });
        }
    }
    docs
}

/// The relative link targets of a document, resolved from its folder; a
/// target that climbs out of the repo is dropped.
fn targets(text: &str, from: &str) -> Vec<String> {
    let base = Path::new(from).parent().unwrap_or(Path::new(""));
    let mut out = Vec::new();
    for chunk in text.split("](").skip(1) {
        let Some(raw) = chunk.split([')', ' ', '#', '?']).next() else {
            continue;
        };
        let raw = raw.trim_start_matches('<').trim_end_matches('>');
        if raw.is_empty() || raw.starts_with("//") || raw.contains(':') {
            continue;
        }
        let decoded = percent_decode(raw);
        let joined = if let Some(abs) = decoded.strip_prefix('/') {
            Path::new(abs).to_path_buf()
        } else {
            base.join(&decoded)
        };
        let Some(mut p) = normalize_inside(&joined) else {
            continue;
        };
        if decoded.ends_with('/') && !p.is_empty() {
            p.push('/');
        }
        out.push(p);
    }
    out
}

/// Tracked files, and folders holding tracked files, that a document links
/// to and that are not documents in the room.
fn outside(
    text: &str,
    from: &str,
    tracked: &BTreeSet<String>,
    docs: &BTreeSet<String>,
) -> Vec<String> {
    let mut out = BTreeSet::new();
    for t in targets(text, from) {
        let bare = t.trim_end_matches('/');
        if bare.is_empty() || docs.contains(bare) {
            continue;
        }
        let in_room = [format!("{bare}.md"), format!("{bare}/README.md")]
            .iter()
            .any(|x| docs.contains(x));
        if in_room {
            continue;
        }
        if !t.ends_with('/') && tracked.contains(bare) {
            out.insert(bare.to_owned());
        } else if tracked
            .range(format!("{bare}/")..)
            .next()
            .is_some_and(|f| f.starts_with(&format!("{bare}/")))
        {
            out.insert(format!("{bare}/"));
        }
    }
    out.into_iter().collect()
}

/// Like [`normalize`], but `None` once a `..` climbs above the root.
fn normalize_inside(p: &Path) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    for c in p.components() {
        match c {
            std::path::Component::ParentDir => {
                parts.pop()?;
            }
            std::path::Component::Normal(s) => parts.push(s.to_string_lossy().into_owned()),
            _ => {}
        }
    }
    Some(parts.join("/"))
}

/// The first `# ` heading, else the file name.
fn title(text: &str, path: &str) -> String {
    text.lines()
        .take(40)
        .find_map(|l| l.strip_prefix("# ").map(|t| t.trim().to_owned()))
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| {
            Path::new(path)
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.to_owned())
        })
}

/// Relative markdown links `](x.md)` that resolve to another tracked file.
fn links(text: &str, from: &str, files: &BTreeSet<String>) -> Vec<String> {
    let base = Path::new(from).parent().unwrap_or(Path::new(""));
    let mut out = BTreeSet::new();
    for chunk in text.split("](").skip(1) {
        let Some(target) = chunk.split([')', ' ', '#']).next() else {
            continue;
        };
        if !target.ends_with(".md") || target.contains("://") {
            continue;
        }
        let joined = normalize(&base.join(target));
        if joined != from && files.contains(&joined) {
            out.insert(joined);
        }
    }
    out.into_iter().collect()
}

fn normalize(p: &Path) -> String {
    let mut parts: Vec<String> = Vec::new();
    for c in p.components() {
        match c {
            std::path::Component::ParentDir => {
                parts.pop();
            }
            std::path::Component::Normal(s) => parts.push(s.to_string_lossy().into_owned()),
            _ => {}
        }
    }
    parts.join("/")
}

/// What a document is, from the words in its path; first match wins, so an
/// ADR under `docs/concepts/adr/` is an ADR, not a concept. A `docs/<dir>/`
/// no rule names keeps its own folder name (`vm-sdk`), not `Other`.
pub fn kind(path: &str) -> String {
    let lower = path.to_ascii_lowercase();
    let segs: Vec<&str> = lower.split('/').collect();
    let readme_at_top = segs.last().is_some_and(|f| *f == "readme.md")
        && (segs.len() == 1 || (segs.len() == 2 && segs[0] == "docs"));
    if readme_at_top {
        return "Overview".into();
    }
    match known_kind(path) {
        "Other" if segs.len() >= 3 && segs[0] == "docs" => segs[1].to_owned(),
        k => k.to_owned(),
    }
}

fn known_kind(path: &str) -> &'static str {
    let lower = path.to_ascii_lowercase();
    let segs: Vec<&str> = lower.split('/').collect();
    let dirs = &segs[..segs.len().saturating_sub(1)];
    let file = segs.last().copied().unwrap_or_default();
    let has = |words: &[&str]| {
        dirs.iter().any(|d| words.iter().any(|w| d.contains(w)))
            || words.iter().any(|w| file.starts_with(w))
    };
    const CODE: [&str; 9] = [
        "apps", "packages", "crates", "services", "src", "tooling", "scripts", "test", "tests",
    ];
    if has(&["adr", "decision"]) {
        "ADRs"
    } else if segs.len() == 1 {
        "Overview"
    } else if dirs.first().is_some_and(|d| CODE.contains(d)) {
        "Code docs"
    } else if has(&["plan", "roadmap"]) {
        "Plans"
    } else if has(&["proposal", "product", "spec", "rfc", "prd"]) {
        "Product & specs"
    } else if has(&[
        "guide",
        "getting-started",
        "how-to",
        "tutorial",
        "runbook",
        "troubleshoot",
    ]) {
        "Guides"
    } else if has(&["operation", "ops", "infra", "deploy", "security"]) {
        "Operations"
    } else if has(&["reference", "api"]) {
        "Reference"
    } else if has(&["concept", "architecture", "design", "diagram", "c4"]) {
        "Concepts"
    } else {
        "Other"
    }
}

/// Narrative documents read by people outside the code usually belong in
/// Notion; decisions, code-adjacent and operational ones in the repo. Only a
/// suggestion: the room shows it, the user accepts or overrides it.
fn suggest(kind: &str) -> Option<Mark> {
    match kind {
        "Plans" | "Product & specs" | "Concepts" | "Guides" => Some(Mark::MoveToNotion),
        "ADRs" | "Overview" | "Code docs" | "Operations" | "Reference" => Some(Mark::KeepInRepo),
        _ => None,
    }
}

/// A goal's claim on documents: `repo` or `repo:path/prefix`.
struct Claim {
    group: String,
    repo: String,
    prefix: String,
}

/// From the workspace goals' `docs` lists; the group is the goal title up to
/// its first colon (`VM SDK: Workforce ready` → `VM SDK`).
fn claims(workspace: &Path) -> Vec<Claim> {
    let Ok(Some(st)) = crate::state::read(workspace) else {
        return Vec::new();
    };
    crate::state::open(&st, "goals")
        .flat_map(|g| {
            let title = g["title"].as_str().unwrap_or_default();
            let group = title.split(':').next().unwrap_or(title).trim().to_owned();
            g["docs"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(move |c| {
                    let (repo, prefix) = c.split_once(':').unwrap_or((c, ""));
                    Claim {
                        group: group.clone(),
                        repo: repo.to_owned(),
                        prefix: prefix.trim_matches('/').to_owned(),
                    }
                })
                .collect::<Vec<_>>()
        })
        .filter(|c| !c.group.is_empty())
        .collect()
}

/// The bet owning `repo:path`: the claim with the longest matching prefix.
fn owner<'a>(claims: &'a [Claim], repo: &str, path: &str) -> Option<&'a Claim> {
    claims
        .iter()
        .filter(|c| {
            c.repo == repo
                && (c.prefix.is_empty()
                    || path == c.prefix
                    || path.starts_with(&format!("{}/", c.prefix)))
        })
        .max_by_key(|c| c.prefix.len())
}

/// Lowercase words, for matching a repo document to a Notion page by title.
fn words(s: &str) -> BTreeSet<String> {
    const STOP: [&str; 12] = [
        "the", "and", "for", "with", "from", "into", "readme", "docs", "doc", "guide", "page",
        "notes",
    ];
    s.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() >= 3 && !STOP.contains(w))
        .map(str::to_owned)
        .collect()
}

/// The Notion page a repo document most likely is: at least three meaningful
/// title words, and at least 70% of them shared. Short generic titles
/// ("Getting Started") never match, so a match means the same document.
pub fn best_page<'a>(title: &str, pages: &'a [(BTreeSet<String>, String)]) -> Option<&'a str> {
    let mine = words(title);
    if mine.len() < 3 {
        return None;
    }
    pages
        .iter()
        .filter(|(theirs, _)| theirs.len() >= 3)
        .map(|(theirs, id)| {
            let shared = mine.intersection(theirs).count() as f64;
            let union = mine.union(theirs).count() as f64;
            (shared / union, id)
        })
        .filter(|(score, _)| *score >= 0.7)
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, id)| id.as_str())
}

/// Everything the room shows: documents, Notion pages, the user's
/// organisation, and which repo documents already have a Notion page.
pub fn library(home: &Path, workspace: &Path, projects: &Path) -> Result<Value> {
    let slugs = monitored(workspace);
    let repos = checkouts(projects, &slugs);
    let mut docs = scan(&repos);
    let notion = load_notion(home);
    let pages: Vec<(BTreeSet<String>, String)> = notion
        .pages
        .iter()
        .map(|p| (words(&p.title), format!("notion:{}", p.url)))
        .collect();
    let mut in_notion: BTreeMap<String, String> = BTreeMap::new();
    for d in &docs {
        if let Some(page) = best_page(&d.title, &pages) {
            in_notion.insert(d.id.clone(), page.to_owned());
        }
    }
    let claims = claims(workspace);
    for d in &mut docs {
        if let (Some(repo), Some(path)) = (&d.repo, &d.path)
            && let Some(c) = owner(&claims, repo, path)
        {
            d.group = c.group.clone();
            d.folder = format!("{} · {}", d.group, d.kind);
        }
    }
    docs.extend(notion.pages.iter().map(|p| {
        let group = claims
            .iter()
            .find(|c| c.repo == p.project && c.prefix.is_empty())
            .map(|c| c.group.clone())
            .unwrap_or_else(|| {
                if p.project.is_empty() {
                    "Notion".into()
                } else {
                    p.project.clone()
                }
            });
        Doc {
            id: format!("notion:{}", p.url),
            title: p.title.clone(),
            place: "notion".into(),
            repo: None,
            path: None,
            url: Some(p.url.clone()),
            links: Vec::new(),
            folder: format!("{group} · Notion"),
            kind: "Notion".into(),
            group,
            suggest: None,
            outside: Vec::new(),
        }
    }));
    // Bets in goal order first, then any repo no goal claims.
    let mut groups: Vec<String> = Vec::new();
    for g in claims
        .iter()
        .map(|c| c.group.clone())
        .chain(docs.iter().map(|d| d.group.clone()))
    {
        if !groups.contains(&g) {
            groups.push(g);
        }
    }
    Ok(json!({
        "generated_at": jiff::Zoned::now().strftime("%Y-%m-%d %H:%M").to_string(),
        "repos": repos.iter().map(|(n, d)| {
            match github(d, GITHUB_BUDGET) {
                Ok(gh) => json!({ "name": n, "dir": d, "github": gh }),
                Err(why) => json!({ "name": n, "dir": d, "github": null, "github_why": why }),
            }
        }).collect::<Vec<_>>(),
        "missing": slugs.iter().filter(|s| !repos.iter().any(|(n, _)| *n == repo_name(s))).collect::<Vec<_>>(),
        "notion_listed_at": notion.listed_at,
        "groups": groups,
        "docs": docs,
        "in_notion": in_notion,
        "organize": load_organize(home)?,
    }))
}

/// The room as one self-contained page with its data inlined. Every `<` in
/// the data is escaped, so no document title can close the script or open a
/// `<!--` that would stop it closing.
pub fn render(data: &Value) -> Result<String> {
    if !TEMPLATE.contains(DATA_SLOT) {
        bail!("docs template lost its data slot");
    }
    if !TEMPLATE.contains(MARKDOWN_SLOT) {
        bail!("docs template lost its markdown slot");
    }
    // JSON has `<` only inside strings, so the escape never changes a value.
    let json = serde_json::to_string(data)?.replace('<', "\\u003c");
    Ok(crate::design::inline(TEMPLATE)?
        .replacen(MARKDOWN_SLOT, markdown_js(), 1)
        .replacen(DATA_SLOT, &json, 1))
}

/// marked and DOMPurify, made safe to sit inside the page's one `<script>`.
pub fn markdown_js() -> &'static str {
    static JS: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    JS.get_or_init(|| format!("{}\n;\n{}\n;\n", script_safe(MARKED), script_safe(PURIFY)))
}

/// `js` with its source-map comment dropped and every `<!--`, `<script` and
/// `</script` written as `\x3C…`: inside a script element those sequences
/// change how the HTML parser finds the closing tag. `\x3C` means `<` in a JS
/// string, template or regex alike, which is where the bundles carry them.
pub fn script_safe(js: &str) -> String {
    let mut out = String::with_capacity(js.len());
    for line in js.lines() {
        if line.trim_start().starts_with("//# sourceMappingURL=") {
            continue;
        }
        let mut rest = line;
        while let Some(i) = rest.find('<') {
            out.push_str(&rest[..i]);
            let tail = &rest.as_bytes()[i + 1..];
            let opens = |p: &[u8]| tail.len() >= p.len() && tail[..p.len()].eq_ignore_ascii_case(p);
            if opens(b"!--") || opens(b"script") || opens(b"/script") {
                out.push_str("\\x3C");
            } else {
                out.push('<');
            }
            rest = &rest[i + 1..];
        }
        out.push_str(rest);
        out.push('\n');
    }
    out
}

/// One answer for the room, shared by `cos dash` and the desktop app's
/// `cosreport://` scheme so both serve the same routes under `/docs`.
pub struct Reply {
    pub status: u16,
    pub kind: &'static str,
    pub body: Vec<u8>,
    /// Sent besides `Content-Type` and `Cache-Control`; both servers copy them.
    pub headers: Vec<(&'static str, &'static str)>,
}

fn reply(status: u16, kind: &'static str, body: impl Into<Vec<u8>>) -> Reply {
    Reply {
        status,
        kind,
        body: body.into(),
        headers: Vec::new(),
    }
}

pub struct Ask<'a> {
    pub method: &'a str,
    pub path: &'a str,
    pub query: Option<&'a str>,
    /// The `X-Cos` header.
    pub cos_header: Option<&'a str>,
    pub body: &'a [u8],
}

/// `GET /docs`, `GET /docs/read?id=…`, `GET /docs/asset?id=…`,
/// `POST /docs/organize`; `None` for any
/// other path. A save must carry `X-Cos: docs`: a page on another site cannot
/// set that header without a preflight nothing here answers.
pub fn handle(home: &Path, workspace: &Path, projects: &Path, ask: &Ask) -> Option<Reply> {
    let (query, body) = (ask.query, ask.body);
    let text = "text/plain; charset=utf-8";
    Some(match (ask.method, ask.path) {
        ("GET", "/docs" | "/docs/") => {
            match library(home, workspace, projects).and_then(|d| render(&d)) {
                Ok(html) => reply(200, "text/html; charset=utf-8", html),
                Err(e) => reply(500, text, format!("{e:#}")),
            }
        }
        ("GET", "/docs/read") => match read(workspace, projects, &query_id(query)) {
            Ok(t) => reply(200, text, t),
            Err(e) => reply(404, text, format!("{e:#}")),
        },
        ("GET", "/docs/asset") => match asset(workspace, projects, &query_id(query)) {
            Ok((kind, bytes)) => Reply {
                headers: if kind == SVG {
                    SVG_HEADERS.to_vec()
                } else {
                    vec![SVG_HEADERS[1]]
                },
                ..reply(200, kind, bytes)
            },
            Err(e) => reply(404, text, format!("{e:#}")),
        },
        ("POST", "/docs/organize") => {
            if ask.cos_header != Some("docs") {
                return Some(reply(403, text, "missing X-Cos header"));
            }
            match serde_json::from_slice::<Organize>(body)
                .map_err(anyhow::Error::from)
                .and_then(|o| save_organize(home, &o))
            {
                Ok(()) => reply(200, text, "saved"),
                Err(e) => reply(400, text, format!("{e:#}")),
            }
        }
        _ => return None,
    })
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' if i + 2 < b.len() => match u8::from_str_radix(&s[i + 1..i + 3], 16) {
                Ok(v) => {
                    out.push(v);
                    i += 3;
                }
                Err(_) => {
                    out.push(b'%');
                    i += 1;
                }
            },
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn query_id(query: Option<&str>) -> String {
    query
        .into_iter()
        .flat_map(|q| q.split('&'))
        .find_map(|kv| kv.strip_prefix("id="))
        .map(percent_decode)
        .unwrap_or_default()
}

/// `repo:path` as a file inside that monitored checkout, after symlinks.
fn resolve(workspace: &Path, projects: &Path, id: &str) -> Result<PathBuf> {
    let (repo, path) = id.split_once(':').context("not a document id")?;
    if path
        .split('/')
        .any(|p| p == ".." || p == "." || p.is_empty())
        || path.contains('\\')
    {
        bail!("not a repo-relative path: {path}");
    }
    let repos = checkouts(projects, &monitored(workspace));
    let dir = repos
        .iter()
        .find(|(n, _)| n == repo)
        .map(|(_, d)| d)
        .with_context(|| format!("{repo} is not a monitored checkout"))?;
    let root = dir.canonicalize()?;
    let file = dir.join(path).canonicalize()?;
    if !file.starts_with(&root) {
        bail!("{path} leaves the checkout");
    }
    Ok(file)
}

/// A repo document's text, by id; refuses anything outside the checkouts.
pub fn read(workspace: &Path, projects: &Path, id: &str) -> Result<String> {
    if !id.ends_with(".md") {
        bail!("not a markdown document: {id}");
    }
    Ok(std::fs::read_to_string(resolve(workspace, projects, id)?)?)
}

const SVG: &str = "image/svg+xml";

/// An image a repo document shows, by `repo:path`, with its content type.
/// An SVG is safe only with [`SVG_HEADERS`]: without the sandbox, one opened
/// on its own runs its scripts in the page's origin, which can save the
/// user's organisation.
pub fn asset(workspace: &Path, projects: &Path, id: &str) -> Result<(&'static str, Vec<u8>)> {
    let ext = id.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase());
    let kind = match ext.as_deref() {
        Some("svg") => SVG,
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("avif") => "image/avif",
        _ => bail!("not an image the reader shows: {id}"),
    };
    let file = resolve(workspace, projects, id)?;
    let len = std::fs::metadata(&file)?.len();
    if len > MAX_ASSET {
        bail!("{id} is {len} bytes; the reader shows images up to {MAX_ASSET}");
    }
    Ok((kind, std::fs::read(file)?))
}
