//! The registry of named businesses in `businesses.toml`, shared by every `cos` process.
//!
//! Writes hold an exclusive lock on `businesses.toml.lock` and re-read the file inside it,
//! so two `cos business` commands cannot lose each other's entries.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::agents::slots::{Held, lock_exclusive};
use crate::state::atomic_write_bytes;

/// The business a first run writes from the built-in defaults.
const DEFAULT: &str = "default";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Business {
    pub home: PathBuf,
    pub workspace: PathBuf,
    pub projects: PathBuf,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Registry {
    pub active: Option<String>,
    pub businesses: BTreeMap<String, Business>,
}

#[derive(Serialize, Deserialize, Default)]
struct Raw {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    active: Option<String>,
    #[serde(default, rename = "business")]
    businesses: BTreeMap<String, Business>,
}

/// `COS_BUSINESSES`, else `$XDG_CONFIG_HOME/cos/businesses.toml`, else `$HOME/.config/cos/businesses.toml`.
pub fn location() -> Result<PathBuf> {
    location_from(
        std::env::var_os("COS_BUSINESSES"),
        std::env::var_os("XDG_CONFIG_HOME"),
        std::env::var_os("HOME"),
    )
}

pub fn location_from(
    businesses: Option<OsString>,
    xdg_config_home: Option<OsString>,
    home: Option<OsString>,
) -> Result<PathBuf> {
    let set = |v: Option<OsString>| v.filter(|v| !v.is_empty());
    if let Some(path) = set(businesses) {
        return Ok(PathBuf::from(path));
    }
    if let Some(xdg) = set(xdg_config_home) {
        return Ok(PathBuf::from(xdg).join("cos").join("businesses.toml"));
    }
    let home = set(home).context("HOME is unset and COS_BUSINESSES is not set")?;
    Ok(PathBuf::from(home)
        .join(".config")
        .join("cos")
        .join("businesses.toml"))
}

pub fn valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_lowercase() || c.is_ascii_digit())
        && name.len() <= 40
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// A missing file is an empty registry. Nothing here writes the file.
pub fn load(path: &Path) -> Result<Registry> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Registry::default()),
        Err(e) => return Err(e).with_context(|| format!("read {}", path.display())),
    };
    let raw: Raw = toml::from_str(&text).with_context(|| format!("parse {}", path.display()))?;
    let registry = Registry {
        active: raw.active,
        businesses: raw.businesses,
    };
    check(path, &registry)?;
    Ok(registry)
}

fn check(path: &Path, registry: &Registry) -> Result<()> {
    for name in registry.businesses.keys() {
        if !valid_name(name) {
            bail!(
                "{}: business {name:?} is not a slug (a-z, 0-9, '-'; at most 40 characters)",
                path.display()
            );
        }
    }
    if let Some(active) = &registry.active
        && !registry.businesses.contains_key(active)
    {
        bail!("{}: active = {active:?} names no business", path.display());
    }
    Ok(())
}

fn lock(path: &Path) -> Result<Held> {
    let dir = path.parent().context("no parent directory")?;
    std::fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
    let mut lock_path = path.as_os_str().to_owned();
    lock_path.push(".lock");
    lock_exclusive(Path::new(&lock_path))
}

fn write(path: &Path, registry: Registry) -> Result<()> {
    check(path, &registry)?;
    let text = toml::to_string(&Raw {
        active: registry.active,
        businesses: registry.businesses,
    })?;
    atomic_write_bytes(path, text.as_bytes())
}

fn update(path: &Path, change: impl FnOnce(&mut Registry) -> Result<()>) -> Result<()> {
    let _lock = lock(path)?;
    let mut registry = load(path)?;
    change(&mut registry)?;
    write(path, registry)
}

/// Writes `default` as the active business when the file is missing, so the first `cos business`
/// command leaves the same registry whichever runs first. An existing file is loaded (a corrupt
/// one errors) and left as it is. Returns whether the file was written.
pub fn first_run(path: &Path, default: &Business) -> Result<bool> {
    let _lock = lock(path)?;
    if path.exists() {
        load(path)?;
        return Ok(false);
    }
    write(
        path,
        Registry {
            active: Some(DEFAULT.to_owned()),
            businesses: BTreeMap::from([(DEFAULT.to_owned(), default.clone())]),
        },
    )?;
    Ok(true)
}

/// The built-in business: the install as home and workspace, `$HOME/Projects` as projects.
pub fn builtin(inputs: &Inputs) -> Result<Business> {
    let triple = explicit(inputs, &Dirs::default(), Layer::Defaults)?;
    Ok(Business {
        home: triple.home,
        workspace: triple.workspace,
        projects: triple.projects,
    })
}

/// The directories as they are on disk, canonicalised; each must exist and be a directory.
pub fn canonical(business: &Business) -> Result<Business> {
    Ok(Business {
        home: canonical_dir("home", &business.home)?,
        workspace: canonical_dir("workspace", &business.workspace)?,
        projects: canonical_dir("projects", &business.projects)?,
    })
}

/// Copies `template` into `<home>/context` when `context/` does not exist yet. Never overwrites.
pub fn seed_context(template: &Path, home: &Path) -> Result<bool> {
    let context = home.join("context");
    if context.exists() {
        return Ok(false);
    }
    // A half copy at `context/` would never be re-seeded, so copy beside it and rename.
    let staging = home.join(format!(".context.seeding-{}", std::process::id()));
    // A crashed run whose pid this process reused left files the template may no longer have.
    let _ = std::fs::remove_dir_all(&staging);
    let copied = copy_tree(template, &staging).and_then(|()| {
        std::fs::rename(&staging, &context)
            .with_context(|| format!("rename {} to {}", staging.display(), context.display()))
    });
    if copied.is_err() {
        let _ = std::fs::remove_dir_all(&staging);
    }
    match copied {
        Ok(()) => Ok(true),
        // A concurrent add of the same home renamed its copy in first.
        Err(_) if context.is_dir() => Ok(false),
        Err(e) => Err(e),
    }
}

fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to).with_context(|| format!("create {}", to.display()))?;
    for entry in std::fs::read_dir(from).with_context(|| format!("read {}", from.display()))? {
        let entry = entry?;
        let dest = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &dest)?;
        } else {
            std::fs::copy(entry.path(), &dest)?;
        }
    }
    Ok(())
}

/// Whether the business's directories exist and its workspace has a profile.
pub fn status(business: &Business) -> String {
    let missing: Vec<&str> = [
        ("home", &business.home),
        ("workspace", &business.workspace),
        ("projects", &business.projects),
    ]
    .into_iter()
    .filter(|(_, dir)| !dir.is_dir())
    .map(|(label, _)| label)
    .collect();
    let dirs = if missing.is_empty() {
        "dirs ok".to_owned()
    } else {
        format!("dirs missing: {}", missing.join(", "))
    };
    let profile = if crate::state::profile(&business.workspace).is_some() {
        "profile ok"
    } else {
        "no profile"
    };
    format!("{dirs}, {profile}")
}

pub fn set_active(path: &Path, name: &str) -> Result<()> {
    update(path, |registry| {
        if !registry.businesses.contains_key(name) {
            bail!("business {name:?} is not registered in {}", path.display());
        }
        registry.active = Some(name.to_owned());
        Ok(())
    })
}

pub fn remove(path: &Path, name: &str) -> Result<()> {
    update(path, |registry| {
        if registry.active.as_deref() == Some(name) {
            bail!("business {name:?} is the active business; run `cos business use <other>` first");
        }
        if registry.businesses.remove(name).is_none() {
            bail!("business {name:?} is not registered in {}", path.display());
        }
        Ok(())
    })
}

fn canonical_dir(label: &str, dir: &Path) -> Result<PathBuf> {
    if !dir.is_absolute() {
        bail!("{label} {} must be an absolute path", dir.display());
    }
    let meta = std::fs::metadata(dir)
        .with_context(|| format!("{label} {} does not exist", dir.display()))?;
    if !meta.is_dir() {
        bail!("{label} {} is not a directory", dir.display());
    }
    Ok(dir.canonicalize()?)
}

/// Adds `name`, or replaces it when `replace` is set. A home or workspace already used by
/// another business is refused, so each directory belongs to one business.
pub fn add(path: &Path, name: &str, business: Business, replace: bool) -> Result<()> {
    let business = canonical(&business)?;
    update(path, |registry| {
        if !replace && registry.businesses.contains_key(name) {
            bail!(
                "business {name:?} already exists in {}; pass --replace to overwrite it",
                path.display()
            );
        }
        for (other, existing) in &registry.businesses {
            if other == name {
                continue;
            }
            let shared = [&existing.home, &existing.workspace]
                .into_iter()
                .any(|d| *d == business.home || *d == business.workspace);
            if shared {
                bail!("business {name:?} shares a home or workspace with {other:?}");
            }
        }
        registry.businesses.insert(name.to_owned(), business);
        Ok(())
    })
}

/// Directory values, each optional.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Dirs {
    pub home: Option<PathBuf>,
    pub workspace: Option<PathBuf>,
    pub projects: Option<PathBuf>,
}

/// One source of inputs (the flags, or the env): a business name and/or directories.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Sources {
    pub business: Option<String>,
    pub dirs: Dirs,
}

impl Sources {
    fn is_set(&self) -> bool {
        self.business.is_some()
            || self.dirs.home.is_some()
            || self.dirs.workspace.is_some()
            || self.dirs.projects.is_some()
    }
}

/// Everything one resolution reads, passed as values so tests need no process env.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inputs {
    pub flags: Sources,
    pub env: Sources,
    /// The clone this binary was built from; the built-in home and workspace.
    pub install: PathBuf,
    pub user_home: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    Flags,
    Env,
    Registry,
    Defaults,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Triple {
    pub home: PathBuf,
    pub workspace: PathBuf,
    pub projects: PathBuf,
    pub business: Option<String>,
    pub layer: Layer,
}

impl Triple {
    /// The business name, else `default` for the built-in defaults, else `explicit`.
    pub fn label(&self) -> &str {
        match (&self.business, self.layer) {
            (Some(name), _) => name,
            (None, Layer::Defaults) => "default",
            (None, _) => "explicit",
        }
    }
}

impl std::fmt::Display for Triple {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "home={} workspace={} projects={}",
            self.home.display(),
            self.workspace.display(),
            self.projects.display()
        )
    }
}

/// The triple for one command: the flags layer, else the env layer, else the registry's active
/// business, else the built-in defaults. `load` runs only when a business name or the active
/// business is needed, so a command given directories never reads the registry.
pub fn resolve(inputs: &Inputs, load: impl FnOnce() -> Result<Registry>) -> Result<Triple> {
    let (layer, source) = if inputs.flags.is_set() {
        (Layer::Flags, &inputs.flags)
    } else if inputs.env.is_set() {
        (Layer::Env, &inputs.env)
    } else {
        let registry = load()?;
        return match &registry.active {
            Some(name) => named(&registry, name, Layer::Registry),
            None => explicit(inputs, &Dirs::default(), Layer::Defaults),
        };
    };
    let Some(name) = &source.business else {
        return explicit(inputs, &source.dirs, layer);
    };
    let (business_key, dir_keys) = match layer {
        Layer::Flags => ("--business", ["--home", "--workspace", "--projects"]),
        _ => (
            "COS_BUSINESS",
            ["COS_HOME", "COS_WORKSPACE", "COS_PROJECTS"],
        ),
    };
    let dirs = [
        (source.dirs.home.is_some(), dir_keys[0]),
        (source.dirs.workspace.is_some(), dir_keys[1]),
        (source.dirs.projects.is_some(), dir_keys[2]),
    ];
    let set: Vec<&str> = dirs
        .into_iter()
        .filter(|(on, _)| *on)
        .map(|(_, key)| key)
        .collect();
    if !set.is_empty() {
        bail!(
            "{business_key} {name} conflicts with {}; use one",
            set.join(" and ")
        );
    }
    named(&load()?, name, layer)
}

fn named(registry: &Registry, name: &str, layer: Layer) -> Result<Triple> {
    let business = registry
        .businesses
        .get(name)
        .with_context(|| format!("business {name:?} is not in the registry"))?;
    Ok(Triple {
        home: business.home.clone(),
        workspace: business.workspace.clone(),
        projects: business.projects.clone(),
        business: Some(name.to_owned()),
        layer,
    })
}

/// Each directory not given comes from the built-in defaults; the workspace falls back to the
/// home actually used, so `--home` alone also moves the workspace.
fn explicit(inputs: &Inputs, dirs: &Dirs, layer: Layer) -> Result<Triple> {
    let home = dirs.home.clone().unwrap_or_else(|| inputs.install.clone());
    let workspace = dirs.workspace.clone().unwrap_or_else(|| home.clone());
    let projects = match &dirs.projects {
        Some(projects) => projects.clone(),
        None => inputs
            .user_home
            .as_ref()
            .context("HOME is unset, so the default projects directory is unknown; set HOME or pass --projects")?
            .join("Projects"),
    };
    Ok(Triple {
        home,
        workspace,
        projects,
        business: None,
        layer,
    })
}
