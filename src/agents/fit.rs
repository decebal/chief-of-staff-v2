//! The pre-hire fit check: can this role do this work from its sandbox?
//!
//! The vocabulary is data (`fit` in `context/agents.json`, the template's
//! when the file has none): which words mark a kind of work, which mark a
//! need no sandboxed agent has (network, a live system), which finish lines
//! are outward-facing, and which tasks are operations on `cos` itself. Roles
//! declare the kinds they do (`work`) and the needs they can draft for
//! (`drafts`). The code only matches words against that data.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::judge::{Contract, Expects};
use super::policy::Role;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Vocab {
    /// Lines of a prompt holding any of these are boilerplate (the permission
    /// boundary every handoff ends with), not the work, and are not read.
    #[serde(default)]
    pub ignore_lines: Vec<String>,
    /// Kind of work → the words that mark it, matched in the title.
    #[serde(default)]
    pub kinds: BTreeMap<String, Vec<String>>,
    /// Need → what it is and the words that mark it, matched in the work.
    #[serde(default)]
    pub needs: BTreeMap<String, Need>,
    /// Finish lines that happen outside the sandbox.
    #[serde(default)]
    pub outward_done: Vec<Outward>,
    /// Tasks that write outside any worktree, with the command that does them.
    #[serde(default)]
    pub ops: Vec<Op>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Need {
    /// In words, for the dialog: "network access".
    pub say: String,
    /// Whether a role that cannot draft for it is refused; `false` only notes it.
    #[serde(default)]
    pub blocks: bool,
    pub terms: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Outward {
    /// The words in a finish line, e.g. "resolved in jira".
    pub term: String,
    /// What you do to finish it, e.g. "resolve".
    pub verb: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Op {
    pub terms: Vec<String>,
    /// The command that does it, offered instead of a hire.
    pub command: String,
    /// Why no agent can: where it writes.
    pub why: String,
}

/// The work as the check reads it.
#[derive(Debug, Clone, Default)]
pub struct Work {
    /// Read for the kind of work.
    pub title: String,
    /// Read for needs and operations: the title and the work's own words.
    pub text: String,
    pub done_when: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Call {
    /// The role can do it from its sandbox.
    Fit,
    /// Another role can; `suggest` names it.
    Misfit,
    /// No sandboxed role can; it needs you.
    NeedsYou,
    /// An operation on `cos` itself; `command` does it.
    RunYourself,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fit {
    pub call: Call,
    /// Why it is not a fit, in plain words.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// The role to hire instead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggest: Option<String>,
    /// The command to run instead of hiring.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    /// What stays with you even when it fits.
    #[serde(default)]
    pub notes: Vec<String>,
    pub kinds: Vec<String>,
    pub needs: Vec<String>,
}

impl Fit {
    pub fn ok(&self) -> bool {
        self.call == Call::Fit
    }

    /// The refusal in one sentence, with what to do instead.
    pub fn refusal(&self, roles: &[Role]) -> Option<String> {
        let reason = self.reason.clone().unwrap_or_default();
        match self.call {
            Call::Fit => None,
            Call::Misfit => Some(match self.suggest.as_deref() {
                Some(id) => format!("{reason}; hire the {} instead", title(roles, id)),
                None => reason,
            }),
            Call::NeedsYou => Some(format!("this needs you: {reason}")),
            Call::RunYourself => Some(format!(
                "{reason}; run it yourself: {}",
                self.command.as_deref().unwrap_or("")
            )),
        }
    }
}

fn title<'a>(roles: &'a [Role], id: &'a str) -> &'a str {
    roles
        .iter()
        .find(|r| r.id == id)
        .map_or(id, |r| r.title.as_str())
}

/// `text` with every boilerplate line dropped.
pub fn strip(vocab: &Vocab, text: &str) -> String {
    text.lines()
        .filter(|l| {
            let l = l.to_lowercase();
            !vocab
                .ignore_lines
                .iter()
                .any(|i| l.contains(&i.to_lowercase()))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Whether `term` appears in `hay` as whole words, ignoring case.
pub fn has(hay: &str, term: &str) -> bool {
    find(hay, term).is_some()
}

/// Byte offset of the first whole-word, case-insensitive `term` in `hay`.
fn find(hay: &str, term: &str) -> Option<usize> {
    let (h, t) = (hay.to_lowercase(), term.trim().to_lowercase());
    if t.is_empty() || h.len() != hay.len() {
        return None;
    }
    let edge = |c: Option<char>| !c.is_some_and(char::is_alphanumeric);
    h.match_indices(&t)
        .map(|(i, _)| i)
        .find(|&i| edge(h[..i].chars().next_back()) && edge(h[i + t.len()..].chars().next()))
}

fn any(hay: &str, terms: &[String]) -> bool {
    terms.iter().any(|t| has(hay, t))
}

/// The verdict for `role` on `work`. `roles` is the whole staff, for a
/// suggestion; `None` checks the work alone (a spawn with no role).
pub fn check(vocab: &Vocab, roles: &[Role], role: Option<&Role>, work: &Work) -> Fit {
    let kinds: Vec<String> = vocab
        .kinds
        .iter()
        .filter(|(_, terms)| any(&work.title, terms))
        .map(|(k, _)| k.clone())
        .collect();
    let needs: Vec<String> = vocab
        .needs
        .iter()
        .filter(|(_, n)| any(&work.text, &n.terms))
        .map(|(k, _)| k.clone())
        .collect();
    let mut fit = Fit {
        call: Call::Fit,
        reason: None,
        suggest: None,
        command: None,
        notes: Vec::new(),
        kinds: kinds.clone(),
        needs: needs.clone(),
    };
    if let Some(op) = vocab.ops.iter().find(|o| any(&work.text, &o.terms)) {
        fit.call = Call::RunYourself;
        fit.reason = Some(format!(
            "this needs writes outside the worktree: {}",
            op.why
        ));
        fit.command = Some(op.command.clone());
        return fit;
    }
    let say = |k: &str| vocab.needs.get(k).map_or(k, |n| n.say.as_str()).to_owned();
    let blocking = |r: Option<&Role>| -> Vec<String> {
        needs
            .iter()
            .filter(|k| vocab.needs.get(*k).is_some_and(|n| n.blocks))
            .filter(|k| !r.is_some_and(|r| r.drafts.contains(k)))
            .cloned()
            .collect()
    };
    let does = |r: &Role| {
        kinds.is_empty() || r.work.is_empty() || kinds.iter().any(|k| r.work.contains(k))
    };
    let can = |r: &Role| does(r) && blocking(Some(r)).is_empty();

    let fits = match role {
        Some(r) => can(r),
        None => blocking(None).is_empty(),
    };
    if fits {
        if let Some(r) = role {
            for k in needs.iter().filter(|k| r.drafts.contains(k)) {
                fit.notes.push(format!(
                    "{} drafts it; {} stays with you: you run it",
                    r.title,
                    say(k)
                ));
            }
        }
        for k in needs
            .iter()
            .filter(|k| vocab.needs.get(*k).is_some_and(|n| !n.blocks))
        {
            fit.notes
                .push(format!("{} stays with you: it returns a draft", say(k)));
        }
        return fit;
    }
    let suggest = roles.iter().filter(|r| !r.head).find(|r| can(r));
    fit.reason = Some(match role {
        Some(r) if !does(r) => format!(
            "the {} does {} work; this is {} work",
            r.title,
            r.work.join(", "),
            kinds.join(" and ")
        ),
        Some(r) => format!(
            "the {} can't reach {} from its sandbox",
            r.title,
            blocking(Some(r))
                .iter()
                .map(|k| say(k))
                .collect::<Vec<_>>()
                .join(" or ")
        ),
        None => format!(
            "it needs {}, which no sandboxed agent has",
            blocking(None)
                .iter()
                .map(|k| say(k))
                .collect::<Vec<_>>()
                .join(" and ")
        ),
    });
    match suggest {
        Some(s) => {
            fit.call = Call::Misfit;
            fit.suggest = Some(s.id.clone());
        }
        None => fit.call = Call::NeedsYou,
    }
    fit
}

/// The finish line an agent can reach, when `original` is outward-facing:
/// the draft it returns for you to finish. `None` when it is not.
pub fn draft_done_when(vocab: &Vocab, original: &str, role: Option<&Role>) -> Option<String> {
    let (at, o) = vocab
        .outward_done
        .iter()
        .filter_map(|o| find(original, &o.term).map(|i| (i, o)))
        .min_by_key(|(i, _)| *i)?;
    let subject = original[..at].trim().trim_end_matches([',', ':']).trim();
    let subject = ["is", "are", "gets", "get", "be", "been", "was"]
        .iter()
        .find_map(|w| subject.strip_suffix(&format!(" {w}")))
        .unwrap_or(subject)
        .trim();
    let act = if subject.is_empty() {
        format!("finish \"{}\"", original.trim())
    } else {
        format!("{} {subject}", o.verb)
    };
    let what = match role.and_then(|r| r.output.as_deref()) {
        Some(out) => format!("`{out}`"),
        None if role.is_some_and(|r| r.expects == Expects::Nothing) => {
            "its answer, with nothing else changed".into()
        }
        None => "the change in its worktree".into(),
    };
    Some(format!(
        "A draft ready for you to {act}: {what}, with its evidence. The outward step stays with you."
    ))
}

/// Whether the judge has anything to run against: a check, a scope, or a
/// file the contract names.
pub fn judgeable(c: &Contract) -> bool {
    !c.checks.is_empty()
        || !c.scope.is_empty()
        || (c.expects != Expects::Diff && c.output.as_deref().is_some_and(|o| !o.trim().is_empty()))
}
