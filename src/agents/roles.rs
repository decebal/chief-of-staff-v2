//! The role library: the caps every role field is held to, the shipped
//! library entries, validation, and (in `write`) the only writer of
//! `context/agents.json`.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use super::fit::Vocab;
use super::judge::Expects;
use super::policy::{self, Policy, Role};

mod propose;
mod write;
pub use propose::{
    Proposal, ProposalOut, RepoScope, SpecialiseContext, merge, proposal_schema, propose,
    sent_text, specialise_context, specialise_prompt,
};
pub use write::{
    CONTRACT, Change, ContractApproval, FieldDiff, Written, apply, contract_diff, diff, read,
};

/// A list's bound: at most `items`, each at most `chars` characters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListCap {
    pub items: usize,
    pub chars: usize,
}

/// One table for the validator, the proposal schema and the renderer, so
/// the three never disagree on a bound.
#[derive(Debug, Clone, Copy)]
pub struct Caps {
    pub title: usize,
    pub mandate: usize,
    /// `serves`, `sandbox`, `judged`, `done_when`.
    pub text: usize,
    pub principles: ListCap,
    pub method: ListCap,
    pub anti_patterns: ListCap,
    pub escalation: ListCap,
    pub output_format: usize,
    /// The file a role leaves, relative to its workdir.
    pub output: usize,
    pub scope: ListCap,
    pub rationale: usize,
    pub brief: usize,
}

pub const CAPS: Caps = Caps {
    title: 60,
    mandate: 600,
    text: 300,
    principles: ListCap {
        items: 8,
        chars: 240,
    },
    method: ListCap {
        items: 10,
        chars: 240,
    },
    anti_patterns: ListCap {
        items: 8,
        chars: 240,
    },
    escalation: ListCap {
        items: 6,
        chars: 240,
    },
    output_format: 1200,
    output: 200,
    scope: ListCap {
        items: 40,
        chars: 200,
    },
    rationale: 800,
    brief: 1000,
};

/// `s` on one line: every run of whitespace, newlines included, becomes one
/// space, trimmed, then cut to `cap` characters. The preamble renders every
/// role value through this, so a hand-edited role cannot open a section of
/// its own or outgrow the bound.
pub fn clean(s: &str, cap: usize) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(cap)
        .collect()
}

/// `items` cleaned and cut to `cap`, empty ones dropped.
pub fn clean_list(items: &[String], cap: ListCap) -> Vec<String> {
    items
        .iter()
        .map(|i| clean(i, cap.chars))
        .filter(|i| !i.is_empty())
        .take(cap.items)
        .collect()
}

/// A public source a library entry draws on.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Cite {
    pub title: String,
    pub url: String,
}

/// One shipped starting definition: a role plus where its standards come
/// from and the work kinds it adds to the fit vocabulary.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LibraryEntry {
    pub id: String,
    pub version: u32,
    pub summary: String,
    pub sources: Vec<Cite>,
    #[serde(default)]
    pub fit_kinds: BTreeMap<String, Vec<String>>,
    pub role: Role,
}

/// Every file under `templates/roles/`, compiled in. `tests/roles.rs` fails
/// when a file there is missing from this list.
pub const LIBRARY_FILES: &[(&str, &str)] = &[
    (
        "staff-engineer.json",
        include_str!("../../templates/roles/staff-engineer.json"),
    ),
    (
        "principal-reviewer.json",
        include_str!("../../templates/roles/principal-reviewer.json"),
    ),
    (
        "appsec-reviewer.json",
        include_str!("../../templates/roles/appsec-reviewer.json"),
    ),
    ("sre.json", include_str!("../../templates/roles/sre.json")),
    (
        "test-engineer.json",
        include_str!("../../templates/roles/test-engineer.json"),
    ),
    (
        "performance-engineer.json",
        include_str!("../../templates/roles/performance-engineer.json"),
    ),
    (
        "technical-writer.json",
        include_str!("../../templates/roles/technical-writer.json"),
    ),
    (
        "release-manager.json",
        include_str!("../../templates/roles/release-manager.json"),
    ),
    (
        "accessibility-auditor.json",
        include_str!("../../templates/roles/accessibility-auditor.json"),
    ),
    (
        "data-analyst.json",
        include_str!("../../templates/roles/data-analyst.json"),
    ),
    (
        "product-researcher.json",
        include_str!("../../templates/roles/product-researcher.json"),
    ),
    (
        "debugger.json",
        include_str!("../../templates/roles/debugger.json"),
    ),
];

/// The library, parsed once. An entry that does not parse is left out here
/// and fails `tests/roles.rs`.
pub fn library() -> &'static [LibraryEntry] {
    static LIB: OnceLock<Vec<LibraryEntry>> = OnceLock::new();
    LIB.get_or_init(|| {
        LIBRARY_FILES
            .iter()
            .filter_map(|(_, text)| serde_json::from_str(text).ok())
            .collect()
    })
}

pub fn entry(id: &str) -> Option<&'static LibraryEntry> {
    library().iter().find(|e| e.id == id)
}

/// The fit vocabulary a policy is checked against: its own, else the
/// template's. `load` and the write path both use this.
pub fn vocab(p: &Policy) -> Vocab {
    p.fit
        .clone()
        .or_else(|| policy::seed().fit)
        .unwrap_or_default()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    /// A write carrying it is refused.
    Refuse,
    /// Reported; never blocks.
    Warn,
}

/// What is wrong with one field of one role (or of the staff as a whole).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Problem {
    pub role: Option<String>,
    pub field: String,
    pub message: String,
    pub level: Level,
}

fn valid_id(id: &str) -> bool {
    let mut c = id.chars();
    (2..=40).contains(&id.len())
        && c.next().is_some_and(|f| f.is_ascii_lowercase())
        && c.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
}

/// A path that stays inside the workdir: relative, no `..` segment.
fn contained(p: &str) -> bool {
    let path = std::path::Path::new(p);
    !p.is_empty()
        && !path.is_absolute()
        && !path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
}

/// Every problem in `p`'s roles, checked against `vocab`. Deterministic, no
/// I/O beyond the compiled-in library.
pub fn validate(p: &Policy, vocab: &Vocab) -> Vec<Problem> {
    let mut out = Vec::new();
    let heads = p.roles.iter().filter(|r| r.head).count();
    if heads != 1 {
        out.push(Problem {
            role: None,
            field: "head".into(),
            message: format!("the staff needs exactly one head; it has {heads}"),
            level: Level::Refuse,
        });
    }
    let mut seen = std::collections::BTreeSet::new();
    for r in &p.roles {
        let mut push = |field: &str, message: String, level: Level| {
            out.push(Problem {
                role: Some(r.id.clone()),
                field: field.into(),
                message,
                level,
            })
        };
        let refuse = Level::Refuse;
        if !valid_id(&r.id) {
            push(
                "id",
                "lowercase letters, digits and dashes, 2 to 40, starting with a letter".into(),
                refuse,
            );
        } else if !seen.insert(r.id.as_str()) {
            push("id", format!("{} appears more than once", r.id), refuse);
        }
        let text = |push: &mut dyn FnMut(&str, String, Level),
                    field: &str,
                    v: &str,
                    cap: usize,
                    required: bool,
                    one_line: bool| {
            if required && v.trim().is_empty() {
                push(field, "is empty".into(), Level::Refuse);
            }
            if v.chars().count() > cap {
                push(field, format!("is over {cap} characters"), Level::Refuse);
            }
            if one_line && v.contains(['\n', '\r']) {
                push(field, "must be one line".into(), Level::Refuse);
            }
        };
        let c = &CAPS;
        text(&mut push, "title", &r.title, c.title, true, true);
        text(&mut push, "mandate", &r.mandate, c.mandate, true, true);
        for (field, v) in [
            ("serves", &r.serves),
            ("sandbox", &r.sandbox),
            ("judged", &r.judged),
        ] {
            text(&mut push, field, v, c.text, false, false);
        }
        if let Some(d) = &r.done_when {
            text(&mut push, "done_when", d, c.text, false, false);
        }
        if let Some(f) = &r.output_format {
            text(&mut push, "output_format", f, c.output_format, false, false);
        }
        for (field, items, cap) in [
            ("principles", &r.principles, c.principles),
            ("method", &r.method, c.method),
            ("anti_patterns", &r.anti_patterns, c.anti_patterns),
            ("escalation", &r.escalation, c.escalation),
            ("scope", &r.scope, c.scope),
        ] {
            if items.len() > cap.items {
                push(
                    field,
                    format!("holds more than {} items", cap.items),
                    refuse,
                );
            }
            for i in items {
                text(&mut push, field, i, cap.chars, true, true);
            }
        }
        if r.expects == Expects::File && r.output.is_none() {
            push(
                "output",
                "a role judged on a file names that file".into(),
                refuse,
            );
        }
        if let Some(o) = &r.output {
            text(&mut push, "output", o, c.output, true, true);
            if !contained(o) {
                push("output", "must be relative, with no `..`".into(), refuse);
            }
        }
        if r.scope.iter().any(|s| !contained(s)) {
            push(
                "scope",
                "every entry must be relative, with no `..`".into(),
                refuse,
            );
        }
        for w in r.work.iter().filter(|w| !vocab.kinds.contains_key(*w)) {
            push(
                "work",
                format!("{w} is not a kind in the fit vocabulary"),
                Level::Warn,
            );
        }
        for d in r.drafts.iter().filter(|d| !vocab.needs.contains_key(*d)) {
            push(
                "drafts",
                format!("{d} is not a need in the fit vocabulary"),
                Level::Warn,
            );
        }
        if let Some(from) = &r.from {
            let known = from.split_once('@').and_then(|(id, v)| {
                let e = entry(id)?;
                v.parse::<u32>().ok().filter(|v| *v <= e.version)
            });
            if known.is_none() {
                push(
                    "from",
                    format!("{from} names no library entry this cos ships"),
                    Level::Warn,
                );
            }
        }
    }
    out
}
