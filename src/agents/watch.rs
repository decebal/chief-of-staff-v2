//! Trouble a running agent shows in its own stream: it has gone quiet, or it
//! keeps making the same call. Computed from lines the supervisor already
//! reads; it starts no process and asks no model. It flags, it never stops.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// When to flag; fixed at spawn from the hiring policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Limits {
    /// No stream line for this long is `stalled`.
    pub stall_secs: u64,
    /// This many identical calls in a row is `looping`.
    pub repeat: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            stall_secs: 600,
            repeat: 5,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Stalled,
    Looping,
}

impl std::fmt::Display for Kind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Stalled => "stalled",
            Self::Looping => "looping",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Flag {
    pub kind: Kind,
    pub detail: String,
}

pub struct Watch {
    limits: Limits,
    last_line: Instant,
    last_call: Option<String>,
    run: usize,
    seen: Vec<Kind>,
}

impl Watch {
    pub fn new(limits: Limits, now: Instant) -> Self {
        Self {
            limits,
            last_line: now,
            last_call: None,
            run: 0,
            seen: Vec::new(),
        }
    }

    /// One stream line arrived; `call` is set when it made a call.
    pub fn line(&mut self, call: Option<&str>, now: Instant) {
        self.last_line = now;
        let Some(call) = call else { return };
        if self.last_call.as_deref() == Some(call) {
            self.run += 1;
        } else {
            self.last_call = Some(call.to_owned());
            self.run = 1;
        }
    }

    /// What is wrong right now. Each kind raised at least once is kept in
    /// [`Watch::seen`] for the ledger, even after it clears.
    pub fn flags(&mut self, now: Instant) -> Vec<Flag> {
        let mut out = Vec::new();
        let quiet = now.saturating_duration_since(self.last_line);
        if quiet >= Duration::from_secs(self.limits.stall_secs) {
            out.push(Flag {
                kind: Kind::Stalled,
                detail: format!("no output for {}s", quiet.as_secs()),
            });
        }
        if self.limits.repeat > 1 && self.run >= self.limits.repeat {
            let call = self.last_call.as_deref().unwrap_or("");
            out.push(Flag {
                kind: Kind::Looping,
                detail: format!("the same call {} times in a row: {}", self.run, clip(call)),
            });
        }
        for f in &out {
            if !self.seen.contains(&f.kind) {
                self.seen.push(f.kind);
            }
        }
        out
    }

    pub fn seen(&self) -> &[Kind] {
        &self.seen
    }
}

fn clip(s: &str) -> String {
    match s.char_indices().nth(100) {
        Some((i, _)) => format!("{}…", &s[..i]),
        None => s.to_owned(),
    }
}
