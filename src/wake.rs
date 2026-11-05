//! Whether the Mac is awake enough for a model run, and keeping it awake
//! for one.
//!
//! A launchd job whose time passed during sleep fires on the next wake, and
//! that is often a dark wake: a maintenance or Power Nap wake with no
//! display that lasts seconds before the system sleeps again. A run started
//! there stalls until the user opens the lid. `pmset -g systemstate` reports
//! the kernel's current capabilities: a full wake includes `Graphics`, a dark
//! wake has `CPU` without it. That is the definition IOKit uses for a dark
//! wake, unlike display sleep or HID idle time, which are also true of a Mac
//! left awake on a desk.

use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use crate::proc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Wake {
    Full,
    /// Awake without graphics: the system is about to sleep again.
    Dark,
    /// The probe could not tell; a run goes ahead.
    Unknown(String),
}

/// Reads `pmset -g systemstate` output.
pub fn parse(systemstate: &str) -> Wake {
    let Some(caps) = systemstate.lines().find_map(|l| {
        l.trim()
            .strip_prefix("Current System Capabilities are:")
            .map(str::to_owned)
    }) else {
        return Wake::Unknown("pmset printed no capabilities line".into());
    };
    let caps: Vec<&str> = caps.split_whitespace().collect();
    if caps.contains(&"Graphics") {
        Wake::Full
    } else if caps.contains(&"CPU") {
        Wake::Dark
    } else {
        Wake::Unknown(format!("capabilities: {}", caps.join(" ")))
    }
}

/// The machine's wake state through `pmset` (5 s, owned and reaped). `None`
/// for platforms with no dark wake: always `Full`.
pub fn probe(pmset: Option<&Path>) -> Wake {
    let Some(pmset) = pmset else {
        return Wake::Full;
    };
    let mut cmd = Command::new(pmset);
    cmd.args(["-g", "systemstate"]);
    match proc::run(&mut cmd, Duration::from_secs(5)) {
        Ok(o) if o.status.success() => parse(&o.stdout),
        Ok(o) => Wake::Unknown(format!("pmset exited {}", o.status)),
        Err(e) => Wake::Unknown(format!("{e:#}")),
    }
}

/// An idle-sleep assertion for as long as this lives: a `caffeinate -i -w
/// <cos pid>` child. `-w` ends it if `cos` dies without dropping it; drop
/// kills and reaps it. Closing the lid, or a low battery, still sleeps the
/// machine.
pub struct Hold(Option<Child>);

impl Hold {
    pub fn start(caffeinate: Option<&Path>) -> Self {
        let Some(program) = caffeinate else {
            return Self(None);
        };
        let mut cmd = Command::new(program);
        cmd.args(["-i", "-w", &std::process::id().to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        match proc::spawn_grouped(&mut cmd) {
            Ok(child) => Self(Some(child)),
            Err(e) => {
                eprintln!("cos: no sleep assertion, the run may be cut short by sleep: {e:#}");
                Self(None)
            }
        }
    }

    pub fn pid(&self) -> Option<libc::pid_t> {
        self.0.as_ref().map(proc::group_of)
    }
}

impl Drop for Hold {
    fn drop(&mut self) {
        if let Some(child) = &mut self.0 {
            proc::kill_and_reap(child);
        }
    }
}
