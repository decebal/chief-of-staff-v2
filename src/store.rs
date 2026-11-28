//! Saved outputs under `out/<kind>/YYYY-MM-DD.{md,json}`. The JSON is what the
//! MCP server hands to dashboards; the markdown is for people.

use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Serialize;
use serde::de::DeserializeOwned;

#[derive(Debug, Clone, Copy)]
pub enum Kind {
    Briefs,
    /// The gathered local facts behind a run; JSON only.
    Facts,
    /// What became of each day's brief run (`pipeline::RunReceipt`); JSON only.
    Runs,
}

impl Kind {
    fn dir(self, home: &Path) -> PathBuf {
        home.join("out").join(match self {
            Self::Briefs => "briefs",
            Self::Facts => "facts",
            Self::Runs => "runs",
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
    /// The JSON file's mtime (`signals::mtime_ms`) when `value` was read from it.
    pub mtime_ms: i64,
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

/// The newest saved `YYYY-MM-DD.json`, by date in the file name.
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
                .filter(|d| is_day(d))
                .map(str::to_owned)
        })
        .max()
    else {
        return Ok(None);
    };
    on_day(home, kind, &day).map(Some)
}

/// The saved `day`.
pub fn on_day<T: DeserializeOwned>(home: &Path, kind: Kind, day: &str) -> Result<Saved<T>> {
    anyhow::ensure!(is_day(day), "day {day:?}");
    let dir = kind.dir(home);
    let json = dir.join(format!("{day}.json"));
    let (bytes, mtime_ms) = read_settled(&json)?;
    let value =
        serde_json::from_slice(&bytes).with_context(|| format!("parse {}", json.display()))?;
    let markdown = std::fs::read_to_string(dir.join(format!("{day}.md"))).unwrap_or_default();
    Ok(Saved {
        day: day.to_owned(),
        value,
        markdown,
        mtime_ms,
    })
}

/// `YYYY-MM-DD`, by shape only.
pub(crate) fn is_day(day: &str) -> bool {
    let b = day.as_bytes();
    b.len() == 10
        && b.iter().enumerate().all(|(i, c)| match i {
            4 | 7 => *c == b'-',
            _ => c.is_ascii_digit(),
        })
}

/// The file's bytes with the mtime they were written at. `save` rewrites in
/// place, so the full-resolution mtime is taken from the open handle before
/// and after the read; a read it changed across is retried from a new handle.
fn read_settled(path: &Path) -> Result<(Vec<u8>, i64)> {
    for _ in 0..3 {
        let mut file =
            std::fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
        let before = file.metadata()?.modified()?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        let after = file.metadata()?;
        if after.modified()? == before {
            return Ok((bytes, crate::signals::mtime_ms(&after)));
        }
    }
    anyhow::bail!(
        "parse {}: rewritten during each of three reads",
        path.display()
    )
}
