//! What one business's sidebar row shows, from file metadata and kernel checks
//! only: no process is spawned, so the row can be rebuilt on every tick.

use std::ffi::OsString;
use std::fs::Metadata;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::{agents, business, dash};

/// The newest brief: its file name under `out/briefs` and its mtime in ms.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Brief {
    pub file: String,
    pub mtime: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Signals {
    /// Includes the runs counted in `starting`.
    pub live: usize,
    pub lost: usize,
    pub starting: usize,
    /// Supervisor pids of the Live runs.
    pub supervisors: Vec<i32>,
    pub brief: Option<Brief>,
}

pub fn read(home: &Path) -> Signals {
    let counts = agents::count(home).unwrap_or_default();
    Signals {
        live: counts.live,
        lost: counts.lost,
        starting: counts.starting,
        supervisors: agents::live_supervisors(home),
        brief: latest_brief(home),
    }
}

/// Whole milliseconds since the Unix epoch; 0 when unavailable or before it.
pub fn mtime_ms(meta: &Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .and_then(|d| i64::try_from(d.as_millis()).ok())
        .unwrap_or(0)
}

pub fn stamp(home: &Path) -> String {
    dash::stamp([agents::root(home), briefs(home)])
}

/// The business a `cos.brief` plist runs for, when its `COS_BUSINESSES`
/// resolves to `registry` (already canonical). `home` stands in for a `HOME`
/// the plist does not set; launchd gives the job no `XDG_CONFIG_HOME`.
pub fn scheduled(plist: &Path, registry: &Path, home: Option<&Path>) -> Option<String> {
    let value = plist::Value::from_file(plist).ok()?;
    let job = value.as_dictionary()?;
    let mut args = job.get("ProgramArguments")?.as_array()?.iter();
    args.find(|a| a.as_string() == Some("--business"))?;
    let name = args.next()?.as_string()?.to_owned();

    let env = job
        .get("EnvironmentVariables")
        .and_then(plist::Value::as_dictionary);
    let var = |key: &str| {
        env.and_then(|e| e.get(key))
            .and_then(plist::Value::as_string)
            .map(OsString::from)
    };
    let job_home = var("HOME").or_else(|| home.map(|h| h.as_os_str().to_owned()));
    let theirs = business::location_from(var("COS_BUSINESSES"), var("XDG_CONFIG_HOME"), job_home)
        .ok()?
        .canonicalize()
        .ok()?;
    (theirs == registry).then_some(name)
}

fn briefs(home: &Path) -> PathBuf {
    home.join("out/briefs")
}

fn latest_brief(home: &Path) -> Option<Brief> {
    let dir = briefs(home);
    let file = std::fs::read_dir(&dir)
        .ok()?
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| dated_json(n))
        .max()?;
    let meta = std::fs::metadata(dir.join(&file)).ok()?;
    Some(Brief {
        mtime: mtime_ms(&meta),
        file,
    })
}

/// `YYYY-MM-DD.json`.
fn dated_json(name: &str) -> bool {
    let Some(day) = name.strip_suffix(".json") else {
        return false;
    };
    let b = day.as_bytes();
    b.len() == 10
        && b.iter().enumerate().all(|(i, c)| match i {
            4 | 7 => *c == b'-',
            _ => c.is_ascii_digit(),
        })
}
