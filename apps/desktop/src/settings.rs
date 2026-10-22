//! Where the app looks: the `cos` home, the workspace and the projects
//! directory, saved in the app's config directory. Environment first, so the
//! app and the CLI agree: `COS_HOME`, `COS_WORKSPACE`, `COS_PROJECTS`,
//! `COS_AGENT_SLOTS`.

use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, Runtime};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub home: PathBuf,
    pub workspace: PathBuf,
    pub projects: PathBuf,
    /// Overrides `slots::run_slots()` for agents started here.
    #[serde(default)]
    pub slots: Option<usize>,
    /// The `cos` binary that supervises agents, ahead of the search.
    #[serde(default)]
    pub cos: Option<PathBuf>,
}

/// The clone this app was built from; `cos`'s own default home.
fn built_home() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn env_path(var: &str) -> Option<PathBuf> {
    std::env::var_os(var)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

impl Settings {
    fn file<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
        app.path()
            .app_config_dir()
            .ok()
            .map(|d| d.join("settings.json"))
    }

    pub fn load<R: Runtime>(app: &AppHandle<R>) -> Self {
        let saved: Option<Self> = Self::file(app)
            .and_then(|f| std::fs::read(f).ok())
            .and_then(|b| serde_json::from_slice(&b).ok());
        let home = env_path("COS_HOME")
            .or_else(|| saved.as_ref().map(|s| s.home.clone()))
            .unwrap_or_else(built_home);
        let home = std::path::absolute(&home).unwrap_or(home);
        let workspace = env_path("COS_WORKSPACE")
            .or_else(|| saved.as_ref().map(|s| s.workspace.clone()))
            .unwrap_or_else(|| home.clone());
        let projects = env_path("COS_PROJECTS")
            .or_else(|| saved.as_ref().map(|s| s.projects.clone()))
            .or_else(|| env_path("HOME").map(|h| h.join("Projects")))
            .unwrap_or_else(|| home.clone());
        let slots = std::env::var("COS_AGENT_SLOTS")
            .ok()
            .and_then(|v| v.parse().ok())
            .or(saved.as_ref().and_then(|s| s.slots));
        Self {
            home,
            workspace,
            projects,
            slots,
            cos: saved.and_then(|s| s.cos),
        }
    }

    /// The `cos` CLI that supervises agents: `COS_BIN`, then `cos` here, then a
    /// `cos` beside this executable (a workspace build, or a bundled sidecar),
    /// then the home's release and debug builds, then `~/.cargo/bin/cos`.
    pub fn cos_binary(&self) -> Option<PathBuf> {
        let beside = std::env::current_exe()
            .ok()
            .and_then(|e| e.parent().map(|d| d.join("cos")));
        env_path("COS_BIN")
            .into_iter()
            .chain(self.cos.clone())
            .chain(beside)
            .chain([
                self.home.join("target/release/cos"),
                self.home.join("target/debug/cos"),
            ])
            .chain(env_path("HOME").map(|h| h.join(".cargo/bin/cos")))
            .find(|p| p.is_file())
    }

    pub fn save<R: Runtime>(&self, app: &AppHandle<R>) -> Result<()> {
        let file = Self::file(app).ok_or_else(|| anyhow::anyhow!("no app config directory"))?;
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(file, serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }
}
