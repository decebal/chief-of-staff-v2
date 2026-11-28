//! The only writer of `context/agents.json`. Every change goes through
//! [`apply`]: lock, digest check, contract approval, validation, a byte-equal
//! round trip, one level of undo, atomic replace. A refusal leaves the file
//! as it was.

use std::collections::BTreeSet;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{Level, entry, validate, vocab};
use crate::agents::policy::{self, Policy, Role};

/// One change to the staff.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Change {
    /// A role whose id is new.
    Add(Role),
    /// The role `id`, replaced whole; `role.id` must equal `id`.
    Replace {
        id: String,
        role: Role,
    },
    Retire(String),
    Restore(String),
}

/// Whether a person approved changing a role's contract: a typed answer on
/// a terminal, or the window's confirm step. No flag stands in for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContractApproval {
    None,
    Human,
}

#[derive(Debug, Clone, Serialize)]
pub struct Written {
    /// The file's sha256 after the write; the next write passes it back.
    pub digest: String,
    pub role: String,
    pub notes: Vec<String>,
}

/// One field that differs between two roles.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct FieldDiff {
    pub field: String,
    pub before: Value,
    pub after: Value,
}

/// What a hire runs, may touch and is judged on. `checks` run under `sh -c`
/// when a hire ends (`judge.rs`), so changing any of these needs a person.
pub const CONTRACT: &[&str] = &[
    "checks",
    "scope",
    "harness",
    "model",
    "repo",
    "expects",
    "output",
    "deadline_secs",
    "work",
    "drafts",
];

/// Every field in `fields` whose value differs between `before` and `after`.
pub fn diff(before: &Role, after: &Role, fields: &[&str]) -> Vec<FieldDiff> {
    let (b, a) = (
        serde_json::to_value(before).unwrap_or_default(),
        serde_json::to_value(after).unwrap_or_default(),
    );
    fields
        .iter()
        .filter_map(|f| {
            let (bv, av) = (b.get(f).cloned(), a.get(f).cloned());
            (bv != av).then(|| FieldDiff {
                field: (*f).to_owned(),
                before: bv.unwrap_or(Value::Null),
                after: av.unwrap_or(Value::Null),
            })
        })
        .collect()
}

pub fn contract_diff(before: &Role, after: &Role) -> Vec<FieldDiff> {
    diff(before, after, CONTRACT)
}

/// The policy as `load` gives it and the digest of the bytes it came from
/// (`absent` when there is no file). Every write passes that digest back.
pub fn read(home: &Path) -> Result<(Policy, String)> {
    let digest = match std::fs::read(policy::path(home)) {
        Ok(b) => crate::flows::sha256(&b),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => "absent".to_owned(),
        Err(e) => return Err(e).context("read context/agents.json"),
    };
    Ok((policy::load(home), digest))
}

struct Lock(PathBuf);

impl Drop for Lock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn lock(path: PathBuf) -> Result<Lock> {
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
    {
        Ok(mut f) => {
            f.write_all(std::process::id().to_string().as_bytes())?;
            Ok(Lock(path))
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            let pid = std::fs::read_to_string(&path).unwrap_or_default();
            let pid = pid.trim();
            let state = match pid.parse::<libc::pid_t>() {
                Ok(n) if crate::proc::alive(n) => "running",
                Ok(_) => "not running",
                Err(_) => "unknown",
            };
            bail!(
                "another edit holds {} (pid {pid}, {state}); if no edit is in progress, delete {}",
                path.display(),
                path.display()
            )
        }
        Err(e) => Err(e).with_context(|| format!("create {}", path.display())),
    }
}

fn find(p: &Policy, id: &str) -> Result<usize> {
    p.roles
        .iter()
        .position(|r| r.id == id)
        .ok_or_else(|| anyhow::anyhow!("no role {id} in context/agents.json"))
}

fn refuse_keys(p: &Policy) -> BTreeSet<(Option<String>, String, String)> {
    validate(p, &vocab(p))
        .into_iter()
        .filter(|x| x.level == Level::Refuse)
        .map(|x| (x.role, x.field, x.message))
        .collect()
}

/// Clears the keys from before hires ran in Herdr; how many there were.
fn drop_retired(p: &mut Policy) -> usize {
    let mut n = 0;
    let mut clear = |v: &mut Option<Value>| {
        if v.take().is_some() {
            n += 1;
        }
    };
    let terms = std::iter::once(&mut p.defaults).chain(p.repos.values_mut());
    for t in terms {
        clear(&mut t.retired_budget);
        clear(&mut t.retired_stall);
        clear(&mut t.retired_repeat);
    }
    for r in &mut p.roles {
        clear(&mut r.retired_budget);
        clear(&mut r.retired_stall);
        clear(&mut r.retired_repeat);
    }
    n
}

/// Applies `change` to the file `read_digest` was read from. See the module
/// doc for the order; a refusal names why and writes nothing.
pub fn apply(
    home: &Path,
    change: Change,
    read_digest: &str,
    contract: ContractApproval,
) -> Result<Written> {
    let path = policy::path(home);
    let dir = path.parent().context("agents.json has no directory")?;
    std::fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
    let _lock = lock(dir.join("agents.json.lock"))?;

    let previous = match std::fs::read(&path) {
        Ok(b) => Some(b),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e).with_context(|| format!("read {}", path.display())),
    };
    let now = previous
        .as_deref()
        .map_or_else(|| "absent".to_owned(), crate::flows::sha256);
    if now != read_digest {
        bail!("agents.json changed since you read it (read {read_digest}, now {now}); reload");
    }
    let mut p: Policy = match &previous {
        Some(b) => serde_json::from_slice(b).with_context(|| {
            format!(
                "{} does not parse; fix it by hand before changing roles",
                path.display()
            )
        })?,
        None => Policy::default(),
    };

    let mut notes = Vec::new();
    if p.roles.is_empty() {
        p.roles = policy::seed().roles;
        notes.push(format!(
            "agents.json now holds the template's {} roles; later template changes to them will not reach this home",
            p.roles.len()
        ));
    }
    let before_problems = refuse_keys(&p);

    let (id, before, after) = match change {
        Change::Add(role) => {
            if p.role(&role.id).is_some() {
                bail!("a role {} already exists; edit it instead", role.id);
            }
            let before = role
                .from
                .as_deref()
                .and_then(|f| f.split_once('@'))
                .and_then(|(e, _)| entry(e))
                .map(|e| e.role.clone())
                .unwrap_or_default();
            p.roles.push(role.clone());
            (role.id.clone(), before, role)
        }
        Change::Replace { id, role } => {
            let i = find(&p, &id)?;
            if role.id != id {
                bail!("replacing {id} may not change its id (got {})", role.id);
            }
            if p.roles[i].head || role.head {
                bail!("the head is not edited here");
            }
            let before = std::mem::replace(&mut p.roles[i], role.clone());
            (id, before, role)
        }
        Change::Retire(id) | Change::Restore(id) if p.role(&id).is_some_and(|r| r.head) => {
            bail!("the head {id} is never retired or restored")
        }
        Change::Retire(id) => {
            let i = find(&p, &id)?;
            p.roles[i].retired = true;
            let r = p.roles[i].clone();
            (id, r.clone(), r)
        }
        Change::Restore(id) => {
            let i = find(&p, &id)?;
            p.roles[i].retired = false;
            let r = p.roles[i].clone();
            (id, r.clone(), r)
        }
    };

    let changed = contract_diff(&before, &after);
    if !changed.is_empty() && contract != ContractApproval::Human {
        let fields: Vec<&str> = changed.iter().map(|d| d.field.as_str()).collect();
        bail!(
            "contract changes need a person's approval: {}",
            fields.join(", ")
        );
    }

    if let (Some(fit), Some(e)) = (
        p.fit.as_mut(),
        after
            .from
            .as_deref()
            .and_then(|f| f.split_once('@'))
            .and_then(|(e, _)| entry(e)),
    ) {
        for (k, terms) in &e.fit_kinds {
            if !fit.kinds.contains_key(k) {
                fit.kinds.insert(k.clone(), terms.clone());
                notes.push(format!("added the work kind {k} to the fit vocabulary"));
            }
        }
    }

    let dropped = drop_retired(&mut p);
    if dropped > 0 {
        notes.push(format!(
            "dropped {dropped} retired key(s) (budget_usd, stall_secs, repeat)"
        ));
    }

    let problems = validate(&p, &vocab(&p));
    let added: Vec<String> = problems
        .iter()
        .filter(|x| x.level == Level::Refuse)
        .filter(|x| {
            !before_problems.contains(&(x.role.clone(), x.field.clone(), x.message.clone()))
        })
        .map(|x| {
            format!(
                "{}.{}: {}",
                x.role.as_deref().unwrap_or("staff"),
                x.field,
                x.message
            )
        })
        .collect();
    if !added.is_empty() {
        bail!("not written: {}", added.join("; "));
    }
    notes.extend(
        problems
            .iter()
            .filter(|x| x.level == Level::Warn && x.role.as_deref() == Some(id.as_str()))
            .map(|x| format!("{}.{}: {}", id, x.field, x.message)),
    );

    let mut out = serde_json::to_string_pretty(&p)?;
    out.push('\n');
    let back: Policy = serde_json::from_str(&out).context("the new policy does not parse back")?;
    let mut again = serde_json::to_string_pretty(&back)?;
    again.push('\n');
    if again != out {
        bail!("the new policy does not round-trip; nothing written");
    }

    if let Some(b) = &previous {
        crate::state::atomic_write_bytes(&dir.join("agents.json.bak"), b)?;
    }
    crate::state::atomic_write_bytes(&path, out.as_bytes())?;
    Ok(Written {
        digest: crate::flows::sha256(out.as_bytes()),
        role: id,
        notes,
    })
}
