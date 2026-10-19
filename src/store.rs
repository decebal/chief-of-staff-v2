//! Saved outputs under `out/<kind>/YYYY-MM-DD.{md,json}`. The JSON is what the
//! MCP server hands to dashboards; the markdown is for people.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Serialize;
use serde::de::DeserializeOwned;

#[derive(Debug, Clone, Copy)]
pub enum Kind {
    Briefs,
    Reviews,
    /// The gathered local facts behind a run; JSON only.
    Facts,
}

impl Kind {
    fn dir(self, home: &Path) -> PathBuf {
        home.join("out").join(match self {
            Self::Briefs => "briefs",
            Self::Reviews => "reviews",
            Self::Facts => "facts",
        })
    }
}

pub fn save_json<T: Serialize>(home: &Path, kind: Kind, day: &str, value: &T) -> Result<()> {
    let dir = kind.dir(home);
    std::fs::create_dir_all(&dir)?;
    std::fs::write(
        dir.join(format!("{day}.json")),
        serde_json::to_vec_pretty(value)?,
    )?;
    Ok(())
}

pub struct Saved<T> {
    pub day: String,
    pub value: T,
    pub markdown: String,
}

pub fn save<T: Serialize>(
    home: &Path,
    kind: Kind,
    day: &str,
    value: &T,
    markdown: &str,
) -> Result<PathBuf> {
    let dir = kind.dir(home);
    std::fs::create_dir_all(&dir)?;
    std::fs::write(
        dir.join(format!("{day}.json")),
        serde_json::to_vec_pretty(value)?,
    )?;
    let md = dir.join(format!("{day}.md"));
    std::fs::write(&md, markdown)?;
    Ok(md)
}

/// The newest saved output with a JSON twin, by date in the file name.
pub fn latest<T: DeserializeOwned>(home: &Path, kind: Kind) -> Result<Option<Saved<T>>> {
    let dir = kind.dir(home);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Ok(None);
    };
    let Some(day) = entries
        .filter_map(Result::ok)
        .filter_map(|e| {
            e.file_name()
                .to_str()?
                .strip_suffix(".json")
                .map(str::to_owned)
        })
        .max()
    else {
        return Ok(None);
    };
    let json = dir.join(format!("{day}.json"));
    let value = serde_json::from_slice(&std::fs::read(&json)?)
        .with_context(|| format!("parse {}", json.display()))?;
    let markdown = std::fs::read_to_string(dir.join(format!("{day}.md"))).unwrap_or_default();
    Ok(Some(Saved {
        day,
        value,
        markdown,
    }))
}
