//! The trouble flags, with time passed in: no sleeping, no process.

use std::time::{Duration, Instant};

use cos::agents::watch::{Kind, Limits, Watch};

fn limits() -> Limits {
    Limits {
        stall_secs: 60,
        repeat: 3,
    }
}

fn kinds(w: &mut Watch, now: Instant) -> Vec<Kind> {
    w.flags(now).into_iter().map(|f| f.kind).collect()
}

#[test]
fn quiet_past_the_limit_is_stalled_and_a_new_line_clears_it() {
    let t0 = Instant::now();
    let mut w = Watch::new(limits(), t0);
    assert!(kinds(&mut w, t0 + Duration::from_secs(59)).is_empty());
    assert_eq!(kinds(&mut w, t0 + Duration::from_secs(60)), [Kind::Stalled]);
    w.line(None, t0 + Duration::from_secs(61));
    assert!(kinds(&mut w, t0 + Duration::from_secs(62)).is_empty());
    assert_eq!(
        w.seen(),
        [Kind::Stalled],
        "a cleared flag is still remembered"
    );
}

#[test]
fn the_same_call_repeated_is_looping_and_a_different_call_clears_it() {
    let t0 = Instant::now();
    let mut w = Watch::new(limits(), t0);
    w.line(Some("Bash {\"command\":\"cargo test\"}"), t0);
    w.line(Some("Bash {\"command\":\"cargo test\"}"), t0);
    assert!(
        kinds(&mut w, t0).is_empty(),
        "two in a row is not a loop yet"
    );
    w.line(Some("Bash {\"command\":\"cargo test\"}"), t0);
    let flags = w.flags(t0);
    assert_eq!(flags[0].kind, Kind::Looping);
    assert!(flags[0].detail.contains("3 times"), "{flags:?}");
    w.line(Some("Read {\"file\":\"x\"}"), t0);
    assert!(kinds(&mut w, t0).is_empty());
}

#[test]
fn a_line_without_a_call_does_not_break_a_run() {
    let t0 = Instant::now();
    let mut w = Watch::new(limits(), t0);
    for _ in 0..2 {
        w.line(Some("Bash {}"), t0);
        w.line(None, t0);
    }
    w.line(Some("Bash {}"), t0);
    assert_eq!(kinds(&mut w, t0), [Kind::Looping]);
}

#[test]
fn calls_that_differ_only_in_arguments_are_not_a_loop() {
    let t0 = Instant::now();
    let mut w = Watch::new(limits(), t0);
    for n in 0..5 {
        w.line(Some(&format!("Read {{\"file\":\"{n}\"}}")), t0);
    }
    assert!(kinds(&mut w, t0).is_empty());
}
