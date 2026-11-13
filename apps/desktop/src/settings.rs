//! The app's saved settings: `slots` and the `cos` binary. The directories a
//! window used to save are read only for the header's legacy hint and are never
//! written again; the business registry owns them now.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use cos::business::Business;
use serde::Deserialize;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Settings {
    pub home: Option<PathBuf>,
    pub workspace: Option<PathBuf>,
    pub projects: Option<PathBuf>,
    /// Overrides `slots::run_slots()` for agents started here.
    pub slots: Option<usize>,
    /// The `cos` binary that supervises agents, ahead of the search.
    pub cos: Option<PathBuf>,
}

impl Settings {
    /// A missing file is the defaults; a file that does not parse is an error.
    pub fn load(file: &Path) -> Result<Self> {
        match std::fs::read(file) {
            Ok(bytes) => {
                serde_json::from_slice(&bytes).with_context(|| format!("{}", file.display()))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e).with_context(|| format!("read {}", file.display())),
        }
    }

    /// The triple a pre-registry settings file named, filled the way the old
    /// loader filled it. `None` when the file named no directory.
    pub fn legacy(&self, builtin: &Business) -> Option<Business> {
        if self.home.is_none() && self.workspace.is_none() && self.projects.is_none() {
            return None;
        }
        let home = self.home.clone().unwrap_or_else(|| builtin.home.clone());
        Some(Business {
            workspace: self.workspace.clone().unwrap_or_else(|| home.clone()),
            projects: self
                .projects
                .clone()
                .unwrap_or_else(|| builtin.projects.clone()),
            home,
        })
    }
}
