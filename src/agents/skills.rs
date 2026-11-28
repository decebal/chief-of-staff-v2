//! A hire's skill set: the skills on this machine (the catalog, scanned from
//! fixed skill roots), the user's skill facts (which are relevant, which role
//! uses which, which skill requires which), and the set one hire runs with.
//! Facts name skills; paths only ever come from the catalog
//! (`docs/plans/2026-11-28-hire-skill-graph-design.md`).

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::repo_skills::LABEL;

/// The most a skill tree may hold and still be bundled.
pub const TREE_MAX: u64 = 20 * 1024 * 1024;

/// The root Claude reads natively; `skillOverrides` can hide only its skills.
pub const NATIVE: &str = "claude";

/// The roots Codex reads natively; `skills.config` can hide their skills.
pub const NATIVE_CODEX: [&str; 2] = ["agents", "codex"];

/// `^[a-z0-9][a-z0-9._-]{0,63}$`: the only names a fact, an ask or a bundle
/// directory may carry.
pub fn valid_name(s: &str) -> bool {
    let b = s.as_bytes();
    !b.is_empty()
        && b.len() <= 64
        && (b[0].is_ascii_lowercase() || b[0].is_ascii_digit())
        && b.iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"._-".contains(c))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Root {
    pub label: String,
    pub dir: PathBuf,
}

/// `~/.claude/skills`, `~/.agents/skills`, `~/.codex/skills`, then the
/// policy's extra roots.
pub fn roots(user_home: &Path, extra: &[PathBuf]) -> Vec<Root> {
    let mut r: Vec<Root> = [
        ("claude", ".claude"),
        ("agents", ".agents"),
        ("codex", ".codex"),
    ]
    .into_iter()
    .map(|(label, dot)| Root {
        label: label.into(),
        dir: user_home.join(dot).join("skills"),
    })
    .collect();
    r.extend(extra.iter().map(|d| Root {
        label: "extra".into(),
        dir: d.clone(),
    }));
    r
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Entry {
    pub root: String,
    /// The root's child, which may be a symlink.
    pub path: PathBuf,
    pub canonical: PathBuf,
    pub description: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Catalog {
    /// By name; entries in root order.
    pub skills: BTreeMap<String, Vec<Entry>>,
    /// What a root held that is not a skill, in words.
    pub skipped: Vec<String>,
}

pub fn catalog(roots: &[Root]) -> Catalog {
    let mut c = Catalog::default();
    for root in roots {
        let Ok(dir) = std::fs::read_dir(&root.dir) else {
            continue;
        };
        let mut children: Vec<PathBuf> = dir.flatten().map(|e| e.path()).collect();
        children.sort();
        for path in children {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            if name.starts_with('.') {
                continue;
            }
            if !valid_name(&name) {
                c.skipped
                    .push(format!("{}: not a skill name", path.display()));
                continue;
            }
            let Ok(canonical) = path.canonicalize() else {
                c.skipped
                    .push(format!("{}: does not resolve", path.display()));
                continue;
            };
            let md = canonical.join("SKILL.md");
            if !std::fs::symlink_metadata(&md).is_ok_and(|m| m.is_file()) {
                c.skipped.push(format!("{}: no SKILL.md", path.display()));
                continue;
            }
            let description = std::fs::read_to_string(&md)
                .map(|t| description(&t))
                .unwrap_or_default();
            c.skills.entry(name).or_default().push(Entry {
                root: root.label.clone(),
                path,
                canonical,
                description,
            });
        }
    }
    c
}

/// The `description` from a `SKILL.md`'s front matter; a folded or literal
/// block (`>` or `|`) is joined into one line.
fn description(text: &str) -> String {
    let mut lines = text.lines();
    if lines.next().map(str::trim) != Some("---") {
        return String::new();
    }
    let mut out = String::new();
    let mut block = false;
    for line in lines {
        if line.trim() == "---" {
            break;
        }
        if block {
            if line.starts_with(' ') || line.starts_with('\t') {
                if !out.is_empty() {
                    out.push(' ');
                }
                out.push_str(line.trim());
                continue;
            }
            break;
        }
        if let Some(v) = line.strip_prefix("description:") {
            let v = v.trim();
            if v.starts_with('>') || v.starts_with('|') {
                block = true;
            } else {
                return v.trim_matches(['"', '\'']).to_owned();
            }
        }
    }
    out
}

/// sha256 over every regular file under `dir`, sorted by relative path, each
/// as its path, a NUL, its length and its bytes. A symlink below `dir`, or
/// more than [`TREE_MAX`] bytes, is refused.
pub fn tree_digest(dir: &Path) -> Result<String> {
    let mut files: Vec<(String, PathBuf)> = Vec::new();
    let mut total = 0u64;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d)? {
            let p = e?.path();
            let meta = std::fs::symlink_metadata(&p)?;
            let rel = p.strip_prefix(dir)?.to_string_lossy().into_owned();
            if meta.file_type().is_symlink() {
                bail!("{rel} is a symlink inside the skill");
            }
            if meta.is_dir() {
                stack.push(p);
            } else if meta.is_file() {
                total += meta.len();
                if total > TREE_MAX {
                    bail!("the skill holds more than {} MiB", TREE_MAX / (1024 * 1024));
                }
                files.push((rel, p));
            }
        }
    }
    files.sort();
    let mut h = Sha256::new();
    for (rel, p) in files {
        let bytes = std::fs::read(&p)?;
        h.update(rel.as_bytes());
        h.update([0]);
        h.update((bytes.len() as u64).to_le_bytes());
        h.update(&bytes);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Verdict {
    Relevant,
    Irrelevant,
}

/// One skill fact, as read back from Prime.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fact {
    pub name: String,
    pub verdict: Verdict,
    #[serde(default)]
    pub roles: Vec<String>,
    #[serde(default)]
    pub requires: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub curated_at: String,
    /// `cos-skills` when cos wrote it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Its Prime node id, the target of a later `requires` edge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "lowercase")]
pub enum GraphState {
    Fresh,
    Stale { read_at: Option<String> },
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Graph {
    pub facts: BTreeMap<String, Fact>,
    pub state: GraphState,
    /// Facts ignored and roles dropped while reading, in words.
    #[serde(default)]
    pub warnings: Vec<String>,
}

/// What a set was resolved from, so a launch can resolve it again against
/// the seeded worktree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolveInputs {
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub asked: Asked,
    #[serde(default)]
    pub unreviewed: Unreviewed,
    pub graph: Graph,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Asked {
    #[serde(default)]
    pub add: Vec<String>,
    #[serde(default)]
    pub remove: Vec<String>,
}

/// Whether a global skill with no fact stays on in a hire.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Unreviewed {
    #[default]
    On,
    Off,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Why {
    Role,
    Asked,
    Requires,
    /// The hire's repo holds its own copy; that one is used.
    Project,
    /// The hired repo's own role set, from its export root.
    #[serde(rename = "repo-role")]
    RepoRole,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Picked {
    pub name: String,
    pub why: Why,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillSet {
    pub picked: Vec<Picked>,
    /// Names switched off for a Claude session (`skillOverrides`).
    pub hidden: Vec<String>,
    /// Names switched off for a Codex session (`skills.config`).
    #[serde(default)]
    pub hidden_codex: Vec<String>,
    /// Global skills with no fact, still on.
    pub unreviewed: usize,
    pub graph: GraphState,
    pub warnings: Vec<String>,
    /// The inputs this set resolved from; `None` for a set from before a
    /// launch could resolve again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inputs: Option<ResolveInputs>,
}

impl SkillSet {
    /// The skills copied into the hire's bundle.
    pub fn bundled(&self) -> impl Iterator<Item = &Picked> {
        self.picked.iter().filter(|p| p.why != Why::Project)
    }

    /// The block a Codex prompt carries: each picked skill's name,
    /// description and `SKILL.md`, control characters stripped and each line
    /// capped at 300 characters.
    pub fn codex_block(&self) -> Option<String> {
        let lines: Vec<String> = self
            .bundled()
            .filter_map(|p| {
                let md = codex_path(p)?.join("SKILL.md");
                let line = format!("- {}: {} ({})", p.name, p.description, md.display());
                Some(line.chars().filter(|c| !c.is_control()).take(300).collect())
            })
            .collect();
        (!lines.is_empty()).then(|| format!("{CODEX_HEAD}\n{}", lines.join("\n")))
    }
}

const CODEX_HEAD: &str = "Skills for this work: read a skill's SKILL.md before the step it covers.";

/// `prompt` with its skill index replaced by `set`'s, or without one when
/// `set` has no bundled pick. The index is always the prompt's last part.
pub fn with_codex_block(prompt: &str, set: Option<&SkillSet>) -> String {
    let base = prompt
        .find(&format!("\n\n{CODEX_HEAD}"))
        .map_or(prompt, |i| &prompt[..i]);
    match set.and_then(SkillSet::codex_block) {
        Some(block) => format!("{base}\n\n{block}"),
        None => base.to_owned(),
    }
}

/// The skill names a repo holds in its own `.claude/skills/` or
/// `.agents/skills/`, tracked ones only: names the tool wrote there (its
/// manifest, `.allsource/generated.json`) are the repo's export, not its own.
pub fn project_skills(repo: Option<&Path>) -> BTreeSet<String> {
    let generated = repo.map(generated_names).unwrap_or_default();
    repo.into_iter()
        .flat_map(|r| [r.join(".claude/skills"), r.join(".agents/skills")])
        .filter_map(|d| std::fs::read_dir(d).ok())
        .flatten()
        .flatten()
        .filter(|e| e.path().join("SKILL.md").is_file())
        .filter_map(|e| e.file_name().to_str().map(str::to_owned))
        .filter(|n| valid_name(n) && !generated.contains(n))
        .collect()
}

#[derive(Deserialize)]
struct Manifest {
    files: Vec<ManifestFile>,
}

#[derive(Deserialize)]
struct ManifestFile {
    path: String,
}

/// The skill names `allsource-skills resolve` wrote under `.claude/skills/`
/// or `.agents/skills/` in `repo`, per its manifest. An unreadable manifest
/// names none.
fn generated_names(repo: &Path) -> BTreeSet<String> {
    let Ok(text) = std::fs::read_to_string(repo.join(".allsource/generated.json")) else {
        return BTreeSet::new();
    };
    let Ok(manifest) = serde_json::from_str::<Manifest>(&text) else {
        return BTreeSet::new();
    };
    manifest
        .files
        .iter()
        .filter_map(|f| {
            let parts: Vec<&str> = f.path.split('/').collect();
            match parts.as_slice() {
                [".claude" | ".agents", "skills", name, _, ..] => Some((*name).to_owned()),
                _ => None,
            }
        })
        .collect()
}

/// The file a Codex prompt points at for a pick: a repo pick's `agents`
/// export when the skill has one (Codex reads `.agents/skills`), else its own
/// path.
fn codex_path(p: &Picked) -> Option<PathBuf> {
    let path = p.path.as_ref()?;
    if p.why == Why::RepoRole
        && let Some(export) = path.parent().and_then(Path::parent)
    {
        let agents = export.join("agents").join(&p.name);
        if agents.join("SKILL.md").is_file() {
            return Some(agents);
        }
    }
    Some(path.clone())
}

/// The skill set for one hire: the hired repo's role set (`repo_names`), then
/// the role's relevant skills, plus `asked.add`, minus `asked.remove`, closed
/// over `requires`; the repo's own skills (`project`) are used in place and
/// never hidden. A repo entry is never an unreviewed global skill.
pub fn resolve(
    graph: &Graph,
    catalog: &Catalog,
    role: Option<&str>,
    asked: &Asked,
    unreviewed: Unreviewed,
    project: &BTreeSet<String>,
    repo_names: &[String],
) -> SkillSet {
    let mut warnings = graph.warnings.clone();
    match &graph.state {
        GraphState::Fresh => {}
        GraphState::Stale { read_at } => warnings.push(format!(
            "the skill graph is from {}: Prime did not answer",
            read_at.as_deref().unwrap_or("an earlier read")
        )),
        GraphState::None => warnings
            .push("no skill graph yet: Prime did not answer and nothing was read before".into()),
    }
    let removed: BTreeSet<&str> = asked.remove.iter().map(String::as_str).collect();
    let mut queue: VecDeque<(String, Why)> = VecDeque::new();
    for name in repo_names {
        if valid_name(name) {
            queue.push_back((name.clone(), Why::RepoRole));
        } else {
            warnings.push(format!("{name:?} is not a skill name"));
        }
    }
    if let Some(role) = role {
        for f in graph.facts.values() {
            if f.verdict == Verdict::Relevant && f.roles.iter().any(|r| r == role) {
                queue.push_back((f.name.clone(), Why::Role));
            }
        }
    }
    for a in &asked.add {
        if valid_name(a) {
            queue.push_back((a.clone(), Why::Asked));
        } else {
            warnings.push(format!("{a:?} is not a skill name"));
        }
    }

    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut picked = Vec::new();
    while let Some((name, why)) = queue.pop_front() {
        if removed.contains(name.as_str()) || !seen.insert(name.clone()) {
            continue;
        }
        if let Some(f) = graph.facts.get(&name) {
            for r in &f.requires {
                if removed.contains(r.as_str()) {
                    warnings.push(format!("{name} requires {r}, which this hire removed"));
                } else {
                    queue.push_back((r.clone(), Why::Requires));
                }
            }
        }
        if project.contains(&name) {
            warnings.push(format!(
                "{name}: the repo holds its own copy, which is used"
            ));
            picked.push(Picked {
                name,
                why: Why::Project,
                root: None,
                path: None,
                digest: None,
                description: String::new(),
            });
            continue;
        }
        let Some(entries) = catalog.skills.get(&name) else {
            warnings.push(format!("{name} is not in any skill root on this machine"));
            continue;
        };
        let e = &entries[0];
        if entries.len() > 1 {
            let others: Vec<&str> = entries[1..].iter().map(|o| o.root.as_str()).collect();
            warnings.push(format!(
                "{name} is in {} and {}: the {} copy is used",
                e.root,
                others.join(", "),
                e.root
            ));
        }
        match tree_digest(&e.canonical) {
            Ok(digest) => picked.push(Picked {
                name,
                why,
                root: Some(e.root.clone()),
                path: Some(e.canonical.clone()),
                digest: Some(digest),
                description: e.description.clone(),
            }),
            Err(err) => warnings.push(format!("{name} cannot be bundled: {err}")),
        }
    }

    let bundled: BTreeSet<&str> = picked
        .iter()
        .filter(|p| p.why != Why::Project)
        .map(|p| p.name.as_str())
        .collect();
    let all_picked: BTreeSet<&str> = picked.iter().map(|p| p.name.as_str()).collect();
    let mut hidden = Vec::new();
    let mut hidden_codex = Vec::new();
    let mut unreviewed_on = 0;
    for (name, entries) in &catalog.skills {
        if project.contains(name) || entries.iter().all(|e| e.root == LABEL) {
            continue;
        }
        let fact = graph.facts.get(name);
        if fact.is_none() && !all_picked.contains(name.as_str()) {
            unreviewed_on += 1;
        }
        let unwanted = !all_picked.contains(name.as_str())
            && (removed.contains(name.as_str())
                || match fact {
                    Some(f) => f.verdict == Verdict::Irrelevant,
                    None => unreviewed == Unreviewed::Off,
                });
        if entries.iter().any(|e| e.root == NATIVE) && (unwanted || bundled.contains(name.as_str()))
        {
            hidden.push(name.clone());
        }
        if unwanted
            && entries
                .iter()
                .any(|e| NATIVE_CODEX.contains(&e.root.as_str()))
        {
            hidden_codex.push(name.clone());
        }
    }
    if unreviewed == Unreviewed::Off {
        unreviewed_on = 0;
    }
    if unreviewed_on > 0 {
        warnings.push(format!(
            "{unreviewed_on} global skills are unreviewed and still load: `cos skills curate`"
        ));
    }
    SkillSet {
        picked,
        hidden,
        hidden_codex,
        unreviewed: unreviewed_on,
        graph: graph.state.clone(),
        warnings,
        inputs: None,
    }
}
