//! `cos roles`: list, show, propose, new, edit, retire, restore. Every write
//! prints what it will write, then goes through [`super::apply`] on the
//! user's approval: `--yes` or a typed `yes` on a terminal, and for a
//! contract change a typed `contract` on a terminal, which no flag replaces.

use std::io::{BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use super::{
    Change, ContractApproval, FieldDiff, Level, apply, contract_diff, entry, library, read,
};
use crate::agents::policy::Role;

/// Set by SIGINT and SIGTERM while `cos roles propose` runs; the thread that
/// owns `claude` sees it within one poll and kills and reaps it.
static STOP: AtomicBool = AtomicBool::new(false);

extern "C" fn on_signal(_: libc::c_int) {
    STOP.store(true, Ordering::SeqCst);
}

fn trap_signals() {
    // SAFETY: the handler only stores to an atomic, which is signal-safe.
    unsafe {
        libc::signal(libc::SIGINT, on_signal as *const () as libc::sighandler_t);
        libc::signal(libc::SIGTERM, on_signal as *const () as libc::sighandler_t);
    }
}

fn on_terminal() -> bool {
    std::io::stdin().is_terminal() && std::io::stderr().is_terminal()
}

fn typed(prompt: &str, want: &str) -> Result<bool> {
    eprint!("{prompt}\nType {want}: ");
    std::io::stderr().flush()?;
    let mut answer = String::new();
    std::io::stdin().lock().read_line(&mut answer)?;
    Ok(answer.trim() == want)
}

fn show_diff(head: &str, d: &[FieldDiff]) {
    if d.is_empty() {
        return;
    }
    eprintln!("{head}");
    for f in d {
        eprintln!("  {}: {} -> {}", f.field, f.before, f.after);
    }
}

/// The approval a write needs: a contract change needs a person typing
/// `contract` at a terminal; anything else takes `--yes` or a typed `yes`.
fn approve(yes: bool, contract: &[FieldDiff]) -> Result<ContractApproval> {
    if !contract.is_empty() {
        show_diff(
            "Contract changes (what the hire runs, may touch and is judged on):",
            contract,
        );
        let fields: Vec<&str> = contract.iter().map(|d| d.field.as_str()).collect();
        if !on_terminal() {
            bail!(
                "contract changes need a person at a terminal: {}",
                fields.join(", ")
            );
        }
        if !typed("Write these contract changes?", "contract")? {
            bail!("not written");
        }
        return Ok(ContractApproval::Human);
    }
    if yes {
        return Ok(ContractApproval::None);
    }
    if !on_terminal() {
        bail!("this writes context/agents.json: pass --yes, or run it at a terminal");
    }
    if !typed("Write this to context/agents.json?", "yes")? {
        bail!("not written");
    }
    Ok(ContractApproval::None)
}

fn every_field(a: &Role, b: &Role) -> Vec<FieldDiff> {
    let keys: std::collections::BTreeSet<String> = [a, b]
        .iter()
        .filter_map(|r| serde_json::to_value(r).ok())
        .filter_map(|v| v.as_object().map(|o| o.keys().cloned().collect::<Vec<_>>()))
        .flatten()
        .collect();
    let keys: Vec<&str> = keys.iter().map(String::as_str).collect();
    super::diff(a, b, &keys)
}

fn written(w: &super::Written) {
    println!("wrote {} ({})", w.role, &w.digest[..12.min(w.digest.len())]);
    for n in &w.notes {
        eprintln!("note: {n}");
    }
}

/// `cos roles list`.
pub fn list(home: &Path, library_only: bool, retired: bool, as_json: bool) -> Result<()> {
    if library_only {
        let rows: Vec<Value> = library()
            .iter()
            .map(|e| {
                json!({ "id": e.id, "version": e.version, "title": e.role.title,
                        "summary": e.summary, "sources": e.sources })
            })
            .collect();
        if as_json {
            println!("{}", serde_json::to_string_pretty(&rows)?);
        } else {
            for e in library() {
                let cited: Vec<&str> = e.sources.iter().map(|s| s.title.as_str()).collect();
                println!(
                    "{:<24} {}\n{:<24} sources: {}",
                    e.id,
                    e.summary,
                    "",
                    cited.join("; ")
                );
            }
        }
        return Ok(());
    }
    let (p, _) = read(home)?;
    let roles: Vec<&Role> = p.roles.iter().filter(|r| retired || !r.retired).collect();
    if as_json {
        let rows: Vec<Value> = roles
            .iter()
            .map(|r| {
                let problems: Vec<_> = p
                    .problems
                    .iter()
                    .filter(|x| x.role.as_deref() == Some(r.id.as_str()))
                    .collect();
                json!({ "role": r, "problems": problems })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&rows)?);
        return Ok(());
    }
    for r in roles {
        let status = match (r.head, r.retired) {
            (true, _) => "head",
            (_, true) => "retired",
            _ => "hired",
        };
        println!(
            "{:<22} {:<30} {:<8} {}",
            r.id,
            r.title,
            status,
            r.from.as_deref().unwrap_or("")
        );
        for x in p
            .problems
            .iter()
            .filter(|x| x.role.as_deref() == Some(r.id.as_str()))
        {
            let lvl = if x.level == Level::Refuse {
                "problem"
            } else {
                "warning"
            };
            println!("    {lvl}: {}: {}", x.field, x.message);
        }
    }
    if let Some(n) = &p.note {
        eprintln!("note: {n}");
    }
    Ok(())
}

/// `cos roles show`.
pub fn show(home: &Path, id: &str, library_only: bool) -> Result<()> {
    let v = if library_only {
        serde_json::to_value(entry(id).with_context(|| format!("no library entry {id}"))?)?
    } else {
        let (p, _) = read(home)?;
        serde_json::to_value(p.role(id).with_context(|| format!("no role {id}"))?)?
    };
    println!("{}", serde_json::to_string_pretty(&v)?);
    Ok(())
}

/// Refuses an `--out` path that exists, sits under a symlinked directory, or
/// lands inside cos's own state: `context/` and `out/` under the home, and
/// the workspace's `.chief-of-staff/`. The proposal reaches agents.json only
/// through `cos roles new --file`.
pub fn out_path_ok(out: &Path, home: &Path, workspace: &Path) -> Result<PathBuf> {
    let abs = std::path::absolute(out)?;
    if std::fs::symlink_metadata(&abs).is_ok() {
        bail!("{} exists; --out never overwrites", abs.display());
    }
    let parent = abs.parent().context("--out has no directory")?;
    let real = parent
        .canonicalize()
        .with_context(|| format!("{} does not exist", parent.display()))?;
    if real != parent {
        bail!(
            "{} goes through a symlink; name the real directory",
            parent.display()
        );
    }
    let canon = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_owned());
    for protected in [
        canon(&home.join("context")),
        canon(&home.join("out")),
        canon(&workspace.join(".chief-of-staff")),
    ] {
        if real.starts_with(&protected) {
            bail!("--out may not write inside {}", protected.display());
        }
    }
    Ok(abs)
}

pub struct ProposeArgs<'a> {
    pub library_id: &'a str,
    pub id: Option<&'a str>,
    pub brief: Option<&'a str>,
    pub out: Option<&'a Path>,
    pub json: bool,
    pub dry_run: bool,
    pub business: &'a str,
}

/// `cos roles propose`: writes nothing to the policy; only the `--out` file.
pub fn propose(cfg: &crate::pipeline::Config, a: &ProposeArgs) -> Result<()> {
    let e = entry(a.library_id).with_context(|| format!("no library entry {}", a.library_id))?;
    let (p, _) = read(&cfg.home)?;
    let st = crate::state::read(&cfg.workspace).ok().flatten();
    let ctx = super::specialise_context(&p, st.as_ref(), a.business, a.brief)?;
    if a.dry_run {
        println!(
            "{}",
            super::sent_text(&cfg.install, &super::specialise_prompt(e, &ctx))
        );
        return Ok(());
    }
    let out = a
        .out
        .map(|o| out_path_ok(o, &cfg.home, &cfg.workspace))
        .transpose()?;
    trap_signals();
    let cos = std::env::current_exe().context("locate the cos binary")?;
    let got = super::propose(cfg, e, &ctx, a.id, Some(&cos), &STOP)?;
    if let Some(path) = &out {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .with_context(|| format!("create {}", path.display()))?;
        let mut text = serde_json::to_string_pretty(&got.role)?;
        text.push('\n');
        f.write_all(text.as_bytes())?;
    }
    if a.json {
        println!("{}", serde_json::to_string_pretty(&got)?);
    } else {
        println!("{}\n", got.rationale);
        for d in &got.diff {
            println!(
                "{}:\n  library: {}\n  proposed: {}",
                d.field, d.before, d.after
            );
        }
    }
    if let Some(path) = &out {
        eprintln!(
            "saved {}; add it with: cos roles new --file {}",
            path.display(),
            path.display()
        );
    }
    Ok(())
}

pub enum NewFrom<'a> {
    Library {
        id: &'a str,
        new_id: Option<&'a str>,
    },
    File(&'a Path),
    Blank {
        id: &'a str,
        title: &'a str,
        mandate: &'a str,
    },
}

fn role_file(path: &Path) -> Result<Role> {
    let text = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("{} is not a role", path.display()))
}

/// `cos roles new`.
pub fn new(home: &Path, from: NewFrom, yes: bool) -> Result<()> {
    let role = match from {
        NewFrom::Library { id, new_id } => {
            let e = entry(id).with_context(|| format!("no library entry {id}"))?;
            let mut r = e.role.clone();
            if let Some(n) = new_id {
                r.id = n.to_owned();
            }
            r.from = Some(format!("{}@{}", e.id, e.version));
            r
        }
        NewFrom::File(path) => role_file(path)?,
        NewFrom::Blank { id, title, mandate } => Role {
            id: id.to_owned(),
            title: title.to_owned(),
            mandate: mandate.to_owned(),
            ..Role::default()
        },
    };
    let (_, digest) = read(home)?;
    println!("{}", serde_json::to_string_pretty(&role)?);
    let before = role
        .from
        .as_deref()
        .and_then(|f| f.split_once('@'))
        .and_then(|(e, _)| entry(e))
        .map(|e| e.role.clone())
        .unwrap_or_default();
    let approval = approve(yes, &contract_diff(&before, &role))?;
    written(&apply(home, Change::Add(role), &digest, approval)?);
    Ok(())
}

/// `cos roles edit`.
pub fn edit(home: &Path, id: &str, file: &Path, yes: bool) -> Result<()> {
    let role = role_file(file)?;
    let (p, digest) = read(home)?;
    let current = p.role(id).with_context(|| format!("no role {id}"))?;
    let all = every_field(current, &role);
    if all.is_empty() {
        bail!(
            "{} holds the role as it is; nothing to change",
            file.display()
        );
    }
    show_diff(&format!("Changes to {id}:"), &all);
    let approval = approve(yes, &contract_diff(current, &role))?;
    written(&apply(
        home,
        Change::Replace {
            id: id.to_owned(),
            role,
        },
        &digest,
        approval,
    )?);
    Ok(())
}

/// `cos roles retire` / `restore`.
pub fn set_retired(home: &Path, id: &str, retired: bool, yes: bool) -> Result<()> {
    let (_, digest) = read(home)?;
    eprintln!("{} {id}", if retired { "retire" } else { "restore" });
    let approval = approve(yes, &[])?;
    let change = if retired {
        Change::Retire(id.to_owned())
    } else {
        Change::Restore(id.to_owned())
    };
    written(&apply(home, change, &digest, approval)?);
    Ok(())
}
