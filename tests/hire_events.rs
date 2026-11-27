//! The hire timeline: the pure fold (`agents::timeline`), the event log's append
//! and read (`agents::events`), and the writers that follow a hire. The fold
//! and the shape tests need no Herdr; the writer tests run against the
//! stand-in Herdr (`tests/fixtures/fake-hc.sh`), never the real core.

#[allow(dead_code)]
mod fake_herdr;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use cos::agents::timeline::{self, By, Event, Kind, Mode, Word};
use cos::agents::{self, Contract, Harness, Request, Spec, State, Status, events};
use cos::business::{Layer, Triple};
use fake_herdr::{Fake, agent, fake};
use jiff::Timestamp;

const COS: &str = env!("CARGO_BIN_EXE_cos");
const BASE: i64 = 1_800_000_000;

fn at(s: i64) -> String {
    now_at(s).to_string()
}

fn now_at(s: i64) -> Timestamp {
    Timestamp::from_second(BASE + s).unwrap()
}

fn spec() -> Spec {
    Spec {
        id: "r1".into(),
        harness: Harness::Claude,
        program: "claude".into(),
        model: None,
        repo: None,
        base: None,
        workdir: PathBuf::from("scratch"),
        worktree: false,
        title: "a title".into(),
        source: "test".into(),
        prompt: "do it".into(),
        deadline_secs: 1800,
        created: at(0),
        slots: None,
        contract: Contract::default(),
        role: None,
        terminal: Some("cos-r1".into()),
        workspace: None,
        projects: None,
    }
}

fn ev(kind: Kind, by: By, s: i64) -> Event {
    Event {
        at: at(s),
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

fn word(s: i64, w: Word, turn: u64) -> Event {
    Event {
        word: Some(w),
        turn: Some(turn),
        ..ev(Kind::Word, By::Herdr, s)
    }
}

fn paused(s: i64, turn: u64) -> Event {
    Event {
        turn: Some(turn),
        ..ev(Kind::Paused, By::You, s)
    }
}

fn ended(s: i64) -> Event {
    Event {
        status: Some(Status::Done),
        ..ev(Kind::Ended, By::Cos, s)
    }
}

fn paused_spans(tl: &timeline::Timeline) -> Vec<&timeline::Span> {
    tl.spans.iter().filter(|s| s.mode == Mode::Paused).collect()
}

#[test]
fn fold_draws_working_and_waiting_spans_from_the_words() {
    let events = vec![
        ev(Kind::Started, By::Cos, 0),
        word(1, Word::Working, 0),
        word(11, Word::Idle, 0),
        word(21, Word::Working, 1),
        word(31, Word::Done, 1),
        ended(40),
    ];
    let tl = timeline::fold(&spec(), &State::default(), &events, now_at(100));
    let got: Vec<(Mode, u64)> = tl.spans.iter().map(|s| (s.mode, s.ms)).collect();
    assert_eq!(
        got,
        vec![
            (Mode::Working, 10_000),
            (Mode::Waiting, 10_000),
            (Mode::Working, 10_000),
            (Mode::Waiting, 9_000),
        ]
    );
    assert_eq!(tl.summary.active_ms, 40_000);
    assert_eq!(tl.summary.paused_ms, 0);
    assert!(!tl.summary.paused_now);
}

#[test]
fn hand_offs_and_marks_come_from_the_messages_and_the_lifecycle() {
    let events = vec![
        ev(Kind::Hired, By::Cos, 0),
        ev(Kind::Started, By::Cos, 1),
        Event {
            turn: Some(0),
            ..ev(Kind::FirstMessage, By::You, 2)
        },
        Event {
            chars: Some(4),
            ..ev(Kind::Said, By::You, 5)
        },
        ev(Kind::MarkedDone, By::You, 6),
        ended(7),
        Event {
            judged: Some(at(8)),
            outcome: Some(cos::agents::Outcome::Accepted),
            ..ev(Kind::Judged, By::Cos, 8)
        },
    ];
    let tl = timeline::fold(&spec(), &State::default(), &events, now_at(10));
    let hand: Vec<Kind> = tl.handoffs.iter().map(|h| h.kind).collect();
    assert_eq!(hand, vec![Kind::FirstMessage, Kind::Said]);
    let marks: Vec<Kind> = tl.marks.iter().map(|m| m.kind).collect();
    assert_eq!(
        marks,
        vec![
            Kind::Hired,
            Kind::Started,
            Kind::MarkedDone,
            Kind::Ended,
            Kind::Judged
        ]
    );
}

#[test]
fn a_paused_interval_closes_on_said() {
    let events = vec![
        ev(Kind::Started, By::Cos, 0),
        word(1, Word::Working, 0),
        paused(10, 0),
        Event {
            chars: Some(3),
            ..ev(Kind::Said, By::You, 20)
        },
        ended(30),
    ];
    let tl = timeline::fold(&spec(), &State::default(), &events, now_at(40));
    let p = paused_spans(&tl);
    assert_eq!(p.len(), 1);
    assert_eq!(p[0].ms, 10_000);
    assert_eq!(tl.summary.paused_ms, 10_000);
    assert_eq!(tl.summary.active_ms, 20_000);
}

#[test]
fn a_paused_interval_closes_on_marked_done() {
    let events = vec![
        ev(Kind::Started, By::Cos, 0),
        paused(10, 0),
        ev(Kind::MarkedDone, By::You, 25),
        ended(26),
    ];
    let tl = timeline::fold(&spec(), &State::default(), &events, now_at(40));
    assert_eq!(paused_spans(&tl)[0].ms, 15_000);
}

#[test]
fn a_paused_interval_closes_on_ended() {
    let events = vec![ev(Kind::Started, By::Cos, 0), paused(10, 0), ended(30)];
    let tl = timeline::fold(&spec(), &State::default(), &events, now_at(40));
    assert_eq!(paused_spans(&tl)[0].ms, 20_000);
}

#[test]
fn a_paused_interval_closes_on_working_in_a_later_turn() {
    let events = vec![
        ev(Kind::Started, By::Cos, 0),
        word(1, Word::Working, 0),
        paused(10, 0),
        word(12, Word::Idle, 0),
        word(20, Word::Working, 1),
        ended(30),
    ];
    let tl = timeline::fold(&spec(), &State::default(), &events, now_at(40));
    let p = paused_spans(&tl);
    assert_eq!(p[0].ms, 10_000);
    assert_eq!(p[0].to, at(20));
}

#[test]
fn a_paused_interval_does_not_close_on_working_in_the_same_turn() {
    let events = vec![
        ev(Kind::Started, By::Cos, 0),
        word(1, Word::Working, 0),
        paused(10, 0),
        word(15, Word::Working, 0),
        word(20, Word::Working, 1),
        ended(30),
    ];
    let tl = timeline::fold(&spec(), &State::default(), &events, now_at(40));
    let p = paused_spans(&tl);
    assert_eq!(
        p[0].to,
        at(20),
        "closed by the later turn, not the same one"
    );
    assert_eq!(p[0].ms, 10_000);
}

#[test]
fn a_deadline_stop_opens_no_paused_interval() {
    let events = vec![
        ev(Kind::Started, By::Cos, 0),
        word(1, Word::Working, 0),
        ev(Kind::DeadlineStop, By::Cos, 10),
        ended(20),
    ];
    let tl = timeline::fold(&spec(), &State::default(), &events, now_at(40));
    assert!(paused_spans(&tl).is_empty());
    assert_eq!(tl.summary.paused_ms, 0);
    assert_eq!(tl.summary.active_ms, 20_000);
}

#[test]
fn a_live_paused_run_reads_paused_now_against_now() {
    let events = vec![ev(Kind::Started, By::Cos, 0), paused(10, 0)];
    let tl = timeline::fold(&spec(), &State::default(), &events, now_at(25));
    assert!(tl.summary.paused_now);
    assert_eq!(tl.summary.paused_ms, 15_000);
    assert_eq!(tl.summary.active_ms, 10_000);
}

#[test]
fn hired_then_ended_with_no_started_gives_no_spans_and_says_so() {
    let events = vec![
        ev(Kind::Hired, By::Cos, 0),
        Event {
            status: Some(Status::Failed),
            ..ev(Kind::Ended, By::Cos, 5)
        },
    ];
    let tl = timeline::fold(&spec(), &State::default(), &events, now_at(10));
    assert!(tl.spans.is_empty());
    assert_eq!(tl.summary.active_ms, 0);
    assert!(
        tl.notes
            .iter()
            .any(|n| n.contains("ended before it started")),
        "{:?}",
        tl.notes
    );
}

#[test]
fn a_run_with_no_events_has_no_spans_and_says_word_history_starts_here() {
    let tl = timeline::fold(&spec(), &State::default(), &[], now_at(10));
    assert!(tl.spans.is_empty());
    assert_eq!(tl.summary.paused_ms, 0);
    assert!(
        tl.notes
            .iter()
            .any(|n| n.contains("word history starts at this version")),
        "{:?}",
        tl.notes
    );
}

#[test]
fn a_run_with_no_events_keeps_its_stored_clock() {
    let running = State {
        started: Some(at(10)),
        ..State::default()
    };
    let tl = timeline::fold(&spec(), &running, &[], now_at(70));
    assert_eq!(tl.summary.active_ms, 60_000, "from its stored start to now");
    assert_eq!(tl.summary.paused_ms, 0);
    assert!(!tl.summary.paused_now);

    let done = State {
        started: Some(at(10)),
        ended: Some(at(40)),
        ..State::default()
    };
    let tl = timeline::fold(&spec(), &done, &[], now_at(90));
    assert_eq!(tl.summary.active_ms, 30_000, "from its start to its end");
}

#[test]
fn a_run_with_an_empty_log_file_keeps_its_stored_clock() {
    let home = home_with_run("empty-log");
    std::fs::write(events::path(&home, "r1"), "").unwrap();
    let state = State {
        started: Some(at(10)),
        ..State::default()
    };
    let s = events::tail_summary(&home, "r1", now_at(70), &state).unwrap();
    assert_eq!(s.active_ms, 60_000);
}

#[test]
fn active_time_grows_with_now_while_the_events_do_not_change() {
    let working = vec![ev(Kind::Started, By::Cos, 0), word(1, Word::Working, 0)];
    let t = timeline::totals(&working);
    assert_eq!(timeline::summary(&t, now_at(20)).active_ms, 20_000);
    assert_eq!(timeline::summary(&t, now_at(30)).active_ms, 30_000);

    let held = vec![ev(Kind::Started, By::Cos, 0), paused(10, 0)];
    let t = timeline::totals(&held);
    assert_eq!(timeline::summary(&t, now_at(20)).active_ms, 10_000);
    assert_eq!(
        timeline::summary(&t, now_at(30)).active_ms,
        10_000,
        "a held clock stays still"
    );
}

#[test]
fn word_maps_the_five_herdr_words_and_drops_any_other() {
    assert_eq!(timeline::word("idle"), Some(Word::Idle));
    assert_eq!(timeline::word("working"), Some(Word::Working));
    assert_eq!(timeline::word("done"), Some(Word::Done));
    assert_eq!(timeline::word("blocked"), Some(Word::Blocked));
    assert_eq!(timeline::word("error"), Some(Word::Error));
    assert_eq!(timeline::word("<script>"), None);
    assert_eq!(timeline::word("working; rm"), None);
    assert_eq!(timeline::word(""), None);
}

#[test]
fn identity_is_stable_and_one_to_six() {
    assert_eq!(agents::identity(None), agents::identity(Some("unassigned")));
    assert_eq!(
        agents::identity(Some("builder")),
        agents::identity(Some("builder"))
    );
    let mut seen = HashSet::new();
    for n in 0..200 {
        let i = agents::identity(Some(&format!("role-{n}")));
        assert!((1..=6).contains(&i), "{i}");
        seen.insert(i);
    }
    assert_eq!(seen.len(), 6, "every identity is used");
}

#[test]
fn an_event_line_matches_the_store_binding() {
    let said = Event {
        chars: Some(11),
        ..ev(Kind::Said, By::You, 5)
    };
    let v = serde_json::to_value(&said).unwrap();
    let obj = v.as_object().unwrap();
    const ALLOWED: [&str; 9] = [
        "at", "kind", "by", "word", "turn", "status", "outcome", "judged", "chars",
    ];
    for k in obj.keys() {
        assert!(ALLOWED.contains(&k.as_str()), "unexpected key {k}");
    }
    assert_eq!(obj["kind"], "said");
    assert_eq!(obj["by"], "you");
    assert_eq!(obj["chars"], 11);
    assert!(!obj.contains_key("text") && !obj.contains_key("word"));

    let w = serde_json::to_value(word(3, Word::Blocked, 2)).unwrap();
    assert_eq!(w["word"], "blocked");
    assert_eq!(w["turn"], 2);
    assert_eq!(
        serde_json::to_value(Kind::FirstMessage).unwrap(),
        "first_message"
    );
}

/// A home with one run directory, for the event log alone.
fn home_with_run(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("cos-events-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    let home = d.canonicalize().unwrap_or_else(|_| {
        std::fs::create_dir_all(&d).unwrap();
        d.canonicalize().unwrap()
    });
    std::fs::create_dir_all(home.join("out/agents/r1")).unwrap();
    home
}

#[test]
fn two_threads_appending_one_word_write_one_line() {
    let home = home_with_run("two-words");
    let w = word(1, Word::Working, 0);
    let (a, b) = (w.clone(), w);
    let (h1, h2) = (home.clone(), home.clone());
    let t1 = std::thread::spawn(move || events::append(&h1, "r1", &a).unwrap());
    let t2 = std::thread::spawn(move || events::append(&h2, "r1", &b).unwrap());
    t1.join().unwrap();
    t2.join().unwrap();
    let words = events::read(&home, "r1")
        .into_iter()
        .filter(|e| e.kind == Kind::Word)
        .count();
    assert_eq!(words, 1);
}

#[test]
fn a_verdict_is_appended_once_per_judged_timestamp() {
    let home = home_with_run("judged-once");
    let j = Event {
        judged: Some(at(8)),
        outcome: Some(cos::agents::Outcome::Rejected),
        ..ev(Kind::Judged, By::Cos, 8)
    };
    events::append(&home, "r1", &j).unwrap();
    events::append(&home, "r1", &j).unwrap();
    let judged = events::read(&home, "r1")
        .into_iter()
        .filter(|e| e.kind == Kind::Judged)
        .count();
    assert_eq!(judged, 1);
}

/// The hire's Herdr fake and a repo, as in `tests/agents.rs`.
fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("cos-hire-events-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d.canonicalize().unwrap()
}

fn at_home(home: &Path) -> Triple {
    Triple {
        home: home.to_owned(),
        workspace: home.to_owned(),
        projects: home.parent().unwrap().join("projects"),
        business: None,
        layer: Layer::Flags,
    }
}

fn git(dir: &Path, args: &[&str]) {
    let st = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .status()
        .unwrap();
    assert!(st.success(), "git {args:?}");
}

fn request(program: &str, repo: Option<PathBuf>, prompt: &str) -> Request {
    Request {
        harness: Harness::Claude,
        program: Some(program.into()),
        model: None,
        repo,
        worktree: true,
        title: "test task".into(),
        source: "test".into(),
        prompt: prompt.into(),
        deadline: Duration::from_secs(1800),
        slots: Some(8),
        contract: Contract {
            checks: vec!["test -f made-by-agent.txt".into()],
            ..Default::default()
        },
        explicit: cos::agents::policy::Explicit { deadline: true },
        role: None,
    }
}

/// A home, a repo and a stand-in Herdr; the hire's agent does not run.
fn setup(name: &str) -> (PathBuf, PathBuf, PathBuf, Fake) {
    let root = scratch(name);
    let f = fake(&root);
    let home = root.join("home");
    let r = root.join("projects/demo-repo");
    std::fs::create_dir_all(&r).unwrap();
    git(&r, &["init", "-q", "-b", "main"]);
    std::fs::write(r.join("README.md"), "demo\n").unwrap();
    git(&r, &["add", "README.md"]);
    git(&r, &["commit", "-q", "-m", "init"]);
    (root, home, r, f)
}

fn hire(home: &Path, repo: &Path, prompt: &str) -> Spec {
    agents::spawn(
        &at_home(home),
        request("claude", Some(repo.to_path_buf()), prompt),
        Path::new(COS),
    )
    .unwrap()
}

fn address(home: &Path, id: &str) -> String {
    agents::read(home, id)
        .unwrap()
        .1
        .herdr
        .expect("a Herdr terminal")
        .address
}

fn kinds(home: &Path, id: &str, kind: Kind) -> usize {
    events::read(home, id)
        .iter()
        .filter(|e| e.kind == kind)
        .count()
}

#[test]
fn say_writes_the_length_of_the_reply_and_never_its_text() {
    let (_, home, repo, _f) = setup("say-chars");
    let spec = hire(&home, &repo, "edit");
    agents::say(&home, &spec.id, "secret text").unwrap();
    let said: Vec<Event> = events::read(&home, &spec.id)
        .into_iter()
        .filter(|e| e.kind == Kind::Said)
        .collect();
    assert_eq!(said.len(), 1);
    assert_eq!(said[0].chars, Some(11));
    let raw = std::fs::read_to_string(agents::dir(&home, &spec.id).join("events.jsonl")).unwrap();
    assert!(!raw.contains("secret"), "{raw}");
}

#[test]
fn an_unknown_herdr_word_writes_no_event() {
    let (_, home, repo, f) = setup("unknown-word");
    let spec = hire(&home, &repo, "edit");
    let t = address(&home, &spec.id);
    f.set(&t, "status", "<script>");
    agents::tick(&home).unwrap();
    let raw = std::fs::read_to_string(agents::dir(&home, &spec.id).join("events.jsonl")).unwrap();
    assert!(!raw.contains("script"), "{raw}");
    assert_eq!(
        kinds(&home, &spec.id, Kind::Word),
        1,
        "only the launch's working word"
    );
}

#[test]
fn an_early_return_judge_writes_no_second_judged() {
    let (_, home, repo, f) = setup("judge-once");
    f.exec(true);
    let spec = agents::spawn(
        &at_home(&home),
        request(&agent(), Some(repo), "edit a file"),
        Path::new(COS),
    )
    .unwrap();
    agents::tick(&home).unwrap();
    assert_eq!(kinds(&home, &spec.id, Kind::Judged), 1);
    agents::mark_done(&home, &spec.id).unwrap();
    assert_eq!(kinds(&home, &spec.id, Kind::Judged), 1);
}

#[test]
fn a_failed_append_is_audited_and_the_reply_still_goes_in() {
    let (_, home, repo, f) = setup("append-fails");
    let spec = hire(&home, &repo, "edit");
    let events_file = agents::dir(&home, &spec.id).join("events.jsonl");
    std::fs::remove_file(&events_file).unwrap();
    std::fs::create_dir(&events_file).unwrap();
    let st = agents::say(&home, &spec.id, "hello").unwrap();
    assert_eq!(f.sent(&address(&home, &spec.id)), vec!["hello".to_owned()]);
    assert_eq!(st.status, Status::Running);
    let audit = std::fs::read_to_string(cos::claude::audit_path(&home)).unwrap();
    assert!(audit.contains(&spec.id), "{audit}");
    assert!(audit.contains("error"), "{audit}");
}

fn clock(paused_now: bool) -> timeline::Summary {
    timeline::Summary {
        active_ms: 0,
        paused_ms: 0,
        paused_now,
    }
}

fn seen(word: &str) -> agents::Seen {
    agents::Seen {
        live: true,
        word: Some(word.into()),
        ..agents::Seen::default()
    }
}

#[test]
fn phase_maps_every_status_and_word_to_the_fleet() {
    use agents::Phase;
    let st = |status: Status| State {
        status,
        ..State::default()
    };
    for (status, want) in [
        (Status::Starting, Phase::Starting),
        (Status::Queued, Phase::Starting),
        (Status::Seeding, Phase::Starting),
        (Status::Running, Phase::Working),
        (Status::Done, Phase::Done),
        (Status::Failed, Phase::Failed),
        (Status::Killed, Phase::Stopped),
        (Status::TimedOut, Phase::TimedOut),
        (Status::Lost, Phase::Lost),
    ] {
        assert_eq!(agents::phase(&st(status), &clock(false)), want, "{status}");
    }
    let running = |herdr: Option<agents::Seen>| State {
        status: Status::Running,
        herdr,
        ..State::default()
    };
    for (word, want) in [
        ("working", Phase::Working),
        ("idle", Phase::Working),
        ("done", Phase::Working),
        ("error", Phase::Working),
        ("blocked", Phase::NeedsYou),
    ] {
        assert_eq!(
            agents::phase(&running(Some(seen(word))), &clock(false)),
            want,
            "{word}"
        );
    }
    assert_eq!(
        agents::phase(&running(Some(seen("blocked"))), &clock(true)),
        Phase::Paused,
        "an open pause wins over the word"
    );
    let mut unsent = seen("idle");
    unsent.first_message = Some(cos::herdr::FirstMessage {
        sent: false,
        reason: Some("no prompt".into()),
    });
    assert_eq!(
        agents::phase(&running(Some(unsent)), &clock(false)),
        Phase::NeedsYou
    );
    let mut judged = running(Some(seen("idle")));
    judged.verdict = Some(agents::Verdict {
        outcome: agents::Outcome::NeedsYou,
        reasons: vec![],
        changed: vec![],
        out_of_scope: vec![],
        checks: vec![],
        overclaimed: false,
        judged: at(5),
    });
    assert_eq!(agents::phase(&judged, &clock(false)), Phase::NeedsYou);
    assert_eq!(
        serde_json::to_value(Phase::NeedsYou).unwrap(),
        "needs-you",
        "the window's state keys"
    );
    assert_eq!(serde_json::to_value(Phase::TimedOut).unwrap(), "timed-out");
}

#[test]
fn done_today_is_the_profile_timezones_day() {
    let tz = jiff::tz::TimeZone::get("America/Chicago").unwrap();
    let done = |ended: &str| State {
        status: Status::Done,
        ended: Some(ended.into()),
        ..State::default()
    };
    // 03:00 UTC on the 29th is still the 28th in Chicago.
    let now: Timestamp = "2026-10-29T03:00:00Z".parse().unwrap();
    assert!(agents::done_today(&done("2026-10-28T15:00:00Z"), &tz, now));
    assert!(!agents::done_today(&done("2026-10-29T06:00:00Z"), &tz, now));
    assert!(!agents::done_today(
        &done("2026-10-28T15:00:00Z"),
        &jiff::tz::TimeZone::UTC,
        now
    ));
    let failed = State {
        status: Status::Failed,
        ended: Some("2026-10-28T15:00:00Z".into()),
        ..State::default()
    };
    assert!(!agents::done_today(&failed, &tz, now));
}

#[test]
fn the_snapshot_matches_the_listing_for_well_formed_runs() {
    let (_, home, repo, _f) = setup("snapshot");
    let spec = hire(&home, &repo, "edit");
    agents::tick(&home).unwrap();
    let listed = agents::list(&home).unwrap().runs;
    let views = agents::snapshot(&home, &jiff::tz::TimeZone::UTC, Timestamp::now());
    assert_eq!(views.len(), listed.len());
    let v = views.iter().find(|v| v.id == spec.id).unwrap();
    let (lspec, lstate) = listed.iter().find(|(s, _)| s.id == spec.id).unwrap();
    assert_eq!(v.title, lspec.title);
    assert_eq!(v.source, lspec.source);
    assert_eq!(v.deadline_secs, lspec.deadline_secs);
    assert_eq!(v.started, lstate.started);
    assert_eq!(v.word, lstate.herdr.as_ref().and_then(|h| h.word.clone()));
}
