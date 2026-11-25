//! A hire's events folded into spans, hand-offs and marks. Pure: no I/O, so
//! the fold is a function of (spec, state, events, now).

use jiff::Timestamp;
use serde::{Deserialize, Serialize};

use super::judge::Outcome;
use super::{Spec, State, Status};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Hired,
    Started,
    Word,
    FirstMessage,
    Said,
    Paused,
    DeadlineStop,
    MarkedDone,
    Ended,
    Judged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum By {
    Cos,
    You,
    Herdr,
}

/// Herdr's words for a terminal, as an enum: a word cos does not know is not
/// stored at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Word {
    Idle,
    Working,
    Done,
    Blocked,
    Error,
}

/// One line of `events.jsonl`. Every field is an enum, a number or a
/// timestamp, so no free text can reach the log through this type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    /// RFC 3339.
    pub at: String,
    pub kind: Kind,
    pub by: By,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub word: Option<Word>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<Status>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<Outcome>,
    /// The verdict's own timestamp, on `Judged`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub judged: Option<String>,
    /// The length of a reply, on `Said`; its text is never kept.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chars: Option<usize>,
}

impl Event {
    /// An event of `kind` by `by` at `at`, with no detail set.
    pub fn new(at: String, kind: Kind, by: By) -> Self {
        Self {
            at,
            kind,
            by,
            word: None,
            turn: None,
            status: None,
            outcome: None,
            judged: None,
            chars: None,
        }
    }
}

/// Herdr's word as an enum. The `_` arm is the only one that maps to `None`,
/// so a word added upstream is dropped until it is named here.
pub fn word(herdr: &str) -> Option<Word> {
    match herdr {
        "idle" => Some(Word::Idle),
        "working" => Some(Word::Working),
        "done" => Some(Word::Done),
        "blocked" => Some(Word::Blocked),
        "error" => Some(Word::Error),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Working,
    Waiting,
    Paused,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Span {
    pub mode: Mode,
    pub word: Option<Word>,
    pub turn: Option<u64>,
    pub from: String,
    pub to: String,
    pub ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Handoff {
    pub at: String,
    pub kind: Kind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Mark {
    pub at: String,
    pub kind: Kind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Summary {
    pub active_ms: u64,
    pub paused_ms: u64,
    pub paused_now: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Timeline {
    pub spans: Vec<Span>,
    pub handoffs: Vec<Handoff>,
    pub marks: Vec<Mark>,
    pub summary: Summary,
    pub notes: Vec<String>,
}

/// What does not depend on `now`, so it can be cached per event log.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Totals {
    pub started: Option<Timestamp>,
    pub ended: Option<Timestamp>,
    pub paused_ms: u64,
    /// Start of a paused interval still open.
    pub open_pause_since: Option<Timestamp>,
}

fn stamp(at: &str) -> Option<Timestamp> {
    at.parse().ok()
}

fn millis(from: Timestamp, to: Timestamp) -> u64 {
    (to.as_millisecond() - from.as_millisecond()).max(0) as u64
}

/// The paused interval a `Paused` event opens: where it began and the turn
/// the hire was in when it was stopped.
#[derive(Clone, Copy)]
struct Hold {
    from: Timestamp,
    turn: u64,
}

/// True when `e` closes a paused interval begun in `turn`.
fn closes(e: &Event, hold: Hold) -> bool {
    match e.kind {
        Kind::Said | Kind::MarkedDone | Kind::Ended => true,
        Kind::Word => e.word == Some(Word::Working) && e.turn.is_some_and(|t| t > hold.turn),
        _ => false,
    }
}

pub fn totals(events: &[Event]) -> Totals {
    let mut t = Totals::default();
    let mut hold: Option<Hold> = None;
    for e in events {
        let Some(at) = stamp(&e.at) else { continue };
        if let Some(h) = hold
            && closes(e, h)
        {
            t.paused_ms += millis(h.from, at);
            hold = None;
        }
        match e.kind {
            Kind::Started => {
                t.started.get_or_insert(at);
            }
            Kind::Ended => t.ended = Some(at),
            Kind::Paused if hold.is_none() => {
                hold = Some(Hold {
                    from: at,
                    turn: e.turn.unwrap_or(0),
                });
            }
            _ => {}
        }
    }
    t.open_pause_since = hold.map(|h| h.from);
    t
}

/// The clock at `now`: active time runs from start to end (or now), less the
/// closed pauses and any pause still open.
pub fn summary(t: &Totals, now: Timestamp) -> Summary {
    let open = t.open_pause_since.map_or(0, |s| millis(s, now));
    let paused_ms = t.paused_ms + open;
    let active_ms = match t.started {
        Some(start) => millis(start, t.ended.unwrap_or(now)).saturating_sub(paused_ms),
        None => 0,
    };
    Summary {
        active_ms,
        paused_ms,
        paused_now: t.open_pause_since.is_some(),
    }
}

/// The clock for a run with no event log (hired before events were written):
/// from its stored start to its stored end or now, with no pauses.
pub fn stored_summary(state: &State, now: Timestamp) -> Summary {
    let start = state.started.as_deref().and_then(stamp);
    let end = state.ended.as_deref().and_then(stamp);
    Summary {
        active_ms: start.map_or(0, |s| millis(s, end.unwrap_or(now))),
        paused_ms: 0,
        paused_now: false,
    }
}

fn mode_of(w: Word) -> Option<Mode> {
    match w {
        Word::Working => Some(Mode::Working),
        Word::Idle | Word::Blocked | Word::Done => Some(Mode::Waiting),
        Word::Error => None,
    }
}

fn span(mode: Mode, word: Option<Word>, turn: Option<u64>, from: Timestamp, to: Timestamp) -> Span {
    Span {
        mode,
        word,
        turn,
        from: from.to_string(),
        to: to.to_string(),
        ms: millis(from, to),
    }
}

/// The run's timeline at `now`. Spans come from the words and the paused
/// intervals; hand-offs from what you sent; marks from the lifecycle.
pub fn fold(spec: &Spec, state: &State, events: &[Event], now: Timestamp) -> Timeline {
    let mut tl = Timeline {
        spans: Vec::new(),
        handoffs: Vec::new(),
        marks: Vec::new(),
        summary: if events.is_empty() {
            stored_summary(state, now)
        } else {
            summary(&totals(events), now)
        },
        notes: Vec::new(),
    };
    if events.is_empty() {
        tl.notes.push("word history starts at this version".into());
        mark_from_state(spec, state, &mut tl);
        return tl;
    }
    let has_started = events.iter().any(|e| e.kind == Kind::Started);
    if !has_started && events.iter().any(|e| e.kind == Kind::Ended) {
        tl.notes.push("ended before it started".into());
    }

    let mut cur: Option<(Word, Option<u64>, Timestamp)> = None;
    let mut hold: Option<Hold> = None;
    let mut ended_at: Option<Timestamp> = None;
    for e in events {
        let Some(at) = stamp(&e.at) else { continue };
        match e.kind {
            Kind::FirstMessage | Kind::Said => tl.handoffs.push(Handoff {
                at: e.at.clone(),
                kind: e.kind,
            }),
            Kind::Hired
            | Kind::Started
            | Kind::DeadlineStop
            | Kind::MarkedDone
            | Kind::Ended
            | Kind::Judged => tl.marks.push(Mark {
                at: e.at.clone(),
                kind: e.kind,
            }),
            _ => {}
        }
        if let Some(h) = hold
            && closes(e, h)
        {
            let p = span(Mode::Paused, None, Some(h.turn), h.from, at);
            tl.spans.push(p);
            hold = None;
            cur = None;
        }
        match e.kind {
            Kind::Word => {
                let Some(w) = e.word else { continue };
                if hold.is_some() {
                    continue;
                }
                if let Some((pw, pt, from)) = cur.take()
                    && let Some(m) = mode_of(pw)
                {
                    tl.spans.push(span(m, Some(pw), pt, from, at));
                }
                cur = Some((w, e.turn, at));
            }
            Kind::Paused if hold.is_none() => {
                if let Some((pw, pt, from)) = cur.take()
                    && let Some(m) = mode_of(pw)
                {
                    tl.spans.push(span(m, Some(pw), pt, from, at));
                }
                hold = Some(Hold {
                    from: at,
                    turn: e.turn.unwrap_or(0),
                });
            }
            Kind::Ended => ended_at = Some(at),
            _ => {}
        }
    }
    let end = ended_at.unwrap_or(now);
    if let Some(h) = hold {
        tl.spans
            .push(span(Mode::Paused, None, Some(h.turn), h.from, end));
    }
    if let Some((w, t, from)) = cur
        && let Some(m) = mode_of(w)
    {
        tl.spans.push(span(m, Some(w), t, from, end));
    }
    tl
}

/// Marks for a run that predates the event log: from its stored times.
fn mark_from_state(spec: &Spec, state: &State, tl: &mut Timeline) {
    let mut push = |at: Option<&str>, kind: Kind| {
        if let Some(at) = at {
            tl.marks.push(Mark {
                at: at.to_owned(),
                kind,
            });
        }
    };
    push(Some(&spec.created), Kind::Hired);
    push(state.started.as_deref(), Kind::Started);
    push(
        state.herdr.as_ref().and_then(|h| h.interrupted.as_deref()),
        Kind::DeadlineStop,
    );
    push(state.ended.as_deref(), Kind::Ended);
    push(
        state.verdict.as_ref().map(|v| v.judged.as_str()),
        Kind::Judged,
    );
}
