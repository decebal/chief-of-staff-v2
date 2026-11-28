use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::json;

use super::*;
use crate::proc::Group;

const ID: &str = "r-20261125-091502-k3x9";

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("cos-runs-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn full() -> serde_json::Value {
    json!({"id":ID,"kind":"brief","business":"acme",
     "state":"running","reason":null,"owner":"daemon",
     "pid":4242,"start":"1764061702.481233",
     "groups":[{"pgid":4250,"start":"1764061703.002110"}],
     "owner_pid":4100,"owner_start":"1764058801.120004",
     "started":1764061702,"ended":null,"said":null,"cost_usd":null,
     "day":"2026-11-25","answer":null,"client_req":null})
}

fn running(business: &str) -> Record {
    Record {
        pid: Some(4242),
        start: Some("1764061702.481233".into()),
        state: State::Running,
        ..Record::new(ID, Kind::Brief, business, Owner::Itself)
    }
}

#[test]
fn run_ids_match_the_pattern_and_parse_back() {
    let id = new_id();
    assert!(is_id(&id), "{id}");
    assert!(is_id(ID));
    let at: jiff::Timestamp = "2026-11-25T09:15:02Z".parse().unwrap();
    assert_eq!(id_at(at, "k3x9"), ID);
    for bad in [
        "",
        "r-20261125-091502-K3X9",
        "r-20261125-091502-k3x",
        "r-20261125-091502-k3x9-",
        "r-2026112-091502-k3x9",
        "../r-20261125-091502-k3x9",
        "r-20261125-091502-k3x9.json",
        "x-20261125-091502-k3x9",
    ] {
        assert!(!is_id(bad), "{bad:?} accepted");
    }
    assert_ne!(new_id()[18..], new_id()[18..], "the suffix is random");
}

#[test]
fn a_full_record_parses_and_round_trips() {
    let text = full().to_string();
    let rec = Record::parse(&text).unwrap();
    assert_eq!(rec.state, State::Running);
    assert_eq!(rec.owner, Owner::Daemon);
    assert_eq!(
        rec.groups,
        vec![Group {
            pgid: 4250,
            start: "1764061703.002110".into()
        }]
    );
    let back: serde_json::Value = serde_json::to_value(&rec).unwrap();
    assert_eq!(back, full());
}

#[test]
fn the_record_binding_is_closed_and_checked() {
    let cases: Vec<(&str, serde_json::Value)> = vec![
        ("extra field", {
            let mut v = full();
            v["extra"] = json!(1);
            v
        }),
        ("extra group field", {
            let mut v = full();
            v["groups"][0]["pid"] = json!(1);
            v
        }),
        ("missing state", {
            let mut v = full();
            v.as_object_mut().unwrap().remove("state");
            v
        }),
        ("bad id", {
            let mut v = full();
            v["id"] = json!("r-1");
            v
        }),
        ("bad business", {
            let mut v = full();
            v["business"] = json!("Acme Ltd");
            v
        }),
        ("bad state", {
            let mut v = full();
            v["state"] = json!("paused");
            v
        }),
        ("bad owner", {
            let mut v = full();
            v["owner"] = json!("cron");
            v
        }),
        ("pgid 1", {
            let mut v = full();
            v["groups"][0]["pgid"] = json!(1);
            v
        }),
        ("bad start", {
            let mut v = full();
            v["start"] = json!("yesterday");
            v
        }),
        ("bad day", {
            let mut v = full();
            v["day"] = json!("25/11/2026");
            v
        }),
        ("bad client_req", {
            let mut v = full();
            v["client_req"] = json!("req-7");
            v
        }),
        ("long reason", {
            let mut v = full();
            v["reason"] = json!("x".repeat(201));
            v
        }),
        ("too many groups", {
            let mut v = full();
            v["groups"] = json!(vec![json!({"pgid": 4250, "start": "1.000001"}); 65]);
            v
        }),
    ];
    for (what, v) in cases {
        assert!(Record::parse(&v.to_string()).is_err(), "{what} accepted");
    }
}

#[test]
fn owner_specs_parse_as_pid_and_start() {
    assert_eq!(
        parse_owner("4100/1764058801.120004").unwrap(),
        (4100, "1764058801.120004".to_owned())
    );
    for bad in [
        "",
        "4100",
        "/1.1",
        "x/1.1",
        "4100/yesterday",
        "0/1.1",
        "-5/1.1",
    ] {
        assert!(parse_owner(bad).is_err(), "{bad:?} accepted");
    }
}

#[test]
fn transitions_follow_the_table_and_final_states_never_change() {
    use State::*;
    let all = [Queued, Running, Done, Failed, Deferred, Stopped, Lost];
    for from in all {
        for to in all {
            let allowed = match from {
                Queued => matches!(to, Running | Failed | Lost | Stopped),
                Running => matches!(to, Done | Failed | Deferred | Stopped | Lost),
                _ => false,
            };
            assert_eq!(from.may_become(to), allowed, "{from:?} -> {to:?}");
        }
        assert_eq!(from.is_final(), !matches!(from, Queued | Running));
    }
}

#[test]
fn writes_apply_allowed_transitions_and_leave_final_records_alone() {
    let home = scratch("writes");
    let w = update(&home, ID, |cur| {
        assert!(cur.is_none());
        Some(running("acme"))
    })
    .unwrap();
    assert!(matches!(w, Write::Written(_)));

    let w = update(&home, ID, |cur| {
        let mut r = cur.unwrap().clone();
        r.state = State::Queued;
        Some(r)
    })
    .unwrap();
    assert!(matches!(w, Write::Unchanged(_)), "running -> queued");

    update(&home, ID, |cur| {
        let mut r = cur.unwrap().clone();
        r.state = State::Done;
        r.ended = Some(1);
        Some(r)
    })
    .unwrap();
    let before = std::fs::read(path(&home, ID)).unwrap();
    let w = update(&home, ID, |cur| {
        let mut r = cur.unwrap().clone();
        r.state = State::Stopped;
        Some(r)
    })
    .unwrap();
    assert!(matches!(w, Write::Unchanged(_)));
    assert_eq!(std::fs::read(path(&home, ID)).unwrap(), before);
    assert_eq!(read(&home, ID).unwrap().unwrap().state, State::Done);
}

#[test]
fn a_new_record_starts_queued_or_running_and_must_be_valid() {
    let home = scratch("create");
    let w = update(&home, ID, |_| {
        Some(Record {
            state: State::Done,
            ..running("acme")
        })
    })
    .unwrap();
    assert!(matches!(w, Write::Unchanged(None)));
    assert!(update(&home, ID, |_| Some(running("Not Valid"))).is_err());
    assert!(read(&home, ID).unwrap().is_none());
    assert!(update(&home, "../escape", |_| Some(running("acme"))).is_err());
}

#[test]
fn racing_final_writes_under_the_lock_keep_exactly_one() {
    let home = scratch("race");
    update(&home, ID, |_| Some(running("acme"))).unwrap();
    let threads: Vec<_> = [State::Done, State::Stopped, State::Lost, State::Failed]
        .into_iter()
        .map(|s| {
            let home = home.clone();
            std::thread::spawn(move || {
                matches!(
                    update(&home, ID, |cur| {
                        let mut r = cur.unwrap().clone();
                        std::thread::sleep(Duration::from_millis(20));
                        r.state = s;
                        Some(r)
                    })
                    .unwrap(),
                    Write::Written(_)
                )
            })
        })
        .collect();
    let written = threads
        .into_iter()
        .map(|t| t.join().unwrap())
        .filter(|w| *w)
        .count();
    assert_eq!(written, 1);
}

#[test]
fn a_reader_re_reads_before_showing_lost_and_writes_nothing() {
    let dead = running("acme");
    let done = Record {
        state: State::Done,
        ..dead.clone()
    };
    let mut reads = vec![Some(done.clone()), Some(dead.clone())];
    assert_eq!(
        shown_from(|| reads.pop().flatten(), |_| true),
        Some(State::Done),
        "the re-read found the run's own final write"
    );
    let mut reads = vec![Some(dead.clone()), Some(dead.clone())];
    assert_eq!(
        shown_from(|| reads.pop().flatten(), |_| true),
        Some(State::Lost)
    );
    let mut reads = vec![Some(dead.clone())];
    assert_eq!(
        shown_from(|| reads.pop().flatten(), |_| false),
        Some(State::Running)
    );

    let home = scratch("reader");
    update(&home, ID, |_| {
        Some(Record {
            pid: Some(-1),
            ..running("acme")
        })
    })
    .unwrap();
    let before = std::fs::read(path(&home, ID)).unwrap();
    assert_eq!(shown(&home, ID).unwrap(), Some(State::Lost));
    assert_eq!(std::fs::read(path(&home, ID)).unwrap(), before);
}

struct Stuck;

impl Probe for Stuck {
    fn kill(&self, _: &Group) -> bool {
        true
    }
    fn gone(&self, _: &Group) -> bool {
        false
    }
}

struct Clean;

impl Probe for Clean {
    fn kill(&self, _: &Group) -> bool {
        true
    }
    fn gone(&self, _: &Group) -> bool {
        true
    }
}

#[test]
fn a_group_alive_at_the_bound_leaves_the_record_non_final() {
    let home = scratch("stuck");
    update(&home, ID, |_| Some(running("acme"))).unwrap();
    let g = Group {
        pgid: 4250,
        start: "1764061703.002110".into(),
    };
    let wrote = settle_and_write(
        &home,
        ID,
        std::slice::from_ref(&g),
        &Stuck,
        Duration::from_millis(200),
        |_| Some(vec![]),
        |r| r.state = State::Done,
    )
    .unwrap();
    assert!(!wrote);
    assert_eq!(read(&home, ID).unwrap().unwrap().state, State::Running);

    let wrote = settle_and_write(
        &home,
        ID,
        std::slice::from_ref(&g),
        &Clean,
        Duration::from_millis(200),
        |_| Some(vec![]),
        |r| r.state = State::Done,
    )
    .unwrap();
    assert!(wrote);
    assert_eq!(read(&home, ID).unwrap().unwrap().state, State::Done);
}

struct Seen(std::cell::RefCell<Vec<libc::pid_t>>);

impl Probe for Seen {
    fn kill(&self, g: &Group) -> bool {
        self.0.borrow_mut().push(g.pgid);
        true
    }
    fn gone(&self, _: &Group) -> bool {
        true
    }
}

fn recorded_group(home: &Path) -> Group {
    let g = Group {
        pgid: 4250,
        start: "1764061703.002110".into(),
    };
    let rec = Record {
        groups: vec![g.clone()],
        ..running("acme")
    };
    update(home, ID, |_| Some(rec)).unwrap();
    g
}

#[test]
fn a_settle_ends_the_groups_the_record_holds_once_it_has_the_lock() {
    let home = scratch("reread");
    let g = recorded_group(&home);
    let seen = Seen(Default::default());
    let wrote = settle_and_write(
        &home,
        ID,
        &[],
        &seen,
        Duration::from_millis(200),
        |r| {
            Some(vec![Group {
                pgid: 4242,
                start: r.start.clone().unwrap(),
            }])
        },
        |r| r.state = State::Lost,
    )
    .unwrap();
    assert!(wrote);
    assert_eq!(*seen.0.borrow(), vec![g.pgid, 4242]);
    assert_eq!(read(&home, ID).unwrap().unwrap().state, State::Lost);
}

#[test]
fn a_settle_whose_plan_declines_the_re_read_record_ends_and_writes_nothing() {
    let home = scratch("declined");
    recorded_group(&home);
    let seen = Seen(Default::default());
    let wrote = settle_and_write(
        &home,
        ID,
        &[],
        &seen,
        Duration::from_millis(200),
        |r| {
            assert_eq!(r.groups.len(), 1);
            None
        },
        |r| r.state = State::Lost,
    )
    .unwrap();
    assert!(!wrote);
    assert!(seen.0.borrow().is_empty());
    assert_eq!(read(&home, ID).unwrap().unwrap().state, State::Running);
}

#[test]
fn the_owners_reap_ends_its_run_group_even_when_the_record_is_unreadable() {
    use std::os::unix::process::ExitStatusExt;
    use std::process::{Command, Stdio};

    let home = scratch("reap-unreadable");
    std::fs::create_dir_all(home.join(DIR)).unwrap();
    std::fs::write(path(&home, ID), "{not a record").unwrap();
    let mut child = crate::proc::spawn_grouped(
        Command::new("sleep")
            .arg("60")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null()),
    )
    .unwrap();
    let pgid = crate::proc::group_of(&child);
    let own = Group {
        pgid,
        start: crate::proc::start_of(pgid).unwrap(),
    };
    let r = reap(&home, ID, Some(own), ExitStatus::from_raw(0), None);
    let alive = crate::proc::group_alive(pgid);
    crate::proc::kill_and_reap(&mut child);
    assert!(r.is_err(), "{r:?}");
    assert!(!alive, "the reap left the run's group alive");
}

#[test]
fn a_busy_brief_lock_names_a_holder_that_has_not_written_its_id_yet() {
    let home = scratch("lock-early");
    std::fs::create_dir_all(home.join(DIR)).unwrap();
    let p = home.join(DIR).join(LOCK);
    let held = File::create(&p).unwrap();
    // SAFETY: flock on a descriptor this test owns.
    assert_eq!(unsafe { libc::flock(held.as_raw_fd(), libc::LOCK_EX) }, 0);
    let writer = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(150));
        std::fs::write(&p, ID).unwrap();
    });
    let taken = take_brief_lock(&home, "r-20261125-091503-bbbb").unwrap();
    writer.join().unwrap();
    drop(held);
    match taken {
        Taken::Busy(h) => assert_eq!(h, ID),
        Taken::Held(_) => panic!("a held lock was taken twice"),
    }
}

/// `home`'s brief lock held by a descriptor of its own, naming a run whose
/// record is `done`.
fn held_by_a_finished_run(name: &str) -> (PathBuf, File) {
    let home = scratch(name);
    std::fs::create_dir_all(home.join(DIR)).unwrap();
    let done = Record {
        state: State::Done,
        ..running("acme")
    };
    std::fs::write(path(&home, ID), serde_json::to_vec(&done).unwrap()).unwrap();
    let p = home.join(DIR).join(LOCK);
    std::fs::write(&p, ID).unwrap();
    let held = File::open(&p).unwrap();
    // SAFETY: flock on a descriptor this test owns.
    assert_eq!(unsafe { libc::flock(held.as_raw_fd(), libc::LOCK_EX) }, 0);
    (home, held)
}

#[test]
fn a_lock_held_a_moment_under_a_finished_runs_id_is_taken() {
    let (home, held) = held_by_a_finished_run("lock-probe");
    let probe = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(150));
        drop(held);
    });
    let taken = take_brief_lock(&home, "r-20261125-091503-bbbb").unwrap();
    probe.join().unwrap();
    match taken {
        Taken::Held(_) => {}
        Taken::Busy(h) => panic!("busy under the finished run {h}"),
    }
}

#[test]
fn a_probe_waits_past_a_finished_runs_id_for_the_new_holder() {
    let (home, held) = held_by_a_finished_run("lock-stale");
    let p = home.join(DIR).join(LOCK);
    let next = "r-20261125-091503-cccc";
    let writer = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(150));
        std::fs::write(&p, next).unwrap();
        held
    });
    let holder = brief_held(&home).unwrap();
    drop(writer.join().unwrap());
    assert_eq!(holder.as_deref(), Some(next));
}

#[test]
fn the_brief_lock_is_exclusive_and_names_its_holder() {
    let home = scratch("lock");
    let held = match take_brief_lock(&home, ID).unwrap() {
        Taken::Held(l) => l,
        Taken::Busy(h) => panic!("busy: {h}"),
    };
    let text = std::fs::read_to_string(home.join(DIR).join("brief.lock")).unwrap();
    assert_eq!(text, ID);
    match take_brief_lock(&home, "r-20261125-091503-bbbb").unwrap() {
        Taken::Busy(h) => assert_eq!(h, ID),
        Taken::Held(_) => panic!("a held lock was taken twice"),
    }
    drop(held);
    let other = "r-20261125-091503-aaaa";
    // A child another test forks holds a copy of the descriptor until it
    // execs; a gated one holds it for as long as its gate.
    let until = std::time::Instant::now() + Duration::from_secs(5);
    let _again = loop {
        match take_brief_lock(&home, other).unwrap() {
            Taken::Held(l) => break l,
            Taken::Busy(_) => {
                assert!(
                    std::time::Instant::now() < until,
                    "a released lock stayed held"
                );
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    };
    assert_eq!(
        std::fs::read_to_string(home.join(DIR).join("brief.lock")).unwrap(),
        other
    );
}

#[test]
fn an_answer_is_capped_at_a_char_boundary_and_says_so() {
    assert_eq!(cap_answer("short".into()), "short");
    let long = "é".repeat(ANSWER_MAX);
    let capped = cap_answer(long);
    assert!(capped.len() <= ANSWER_MAX);
    assert!(capped.ends_with(CUT_NOTE));
}

#[test]
fn a_record_whose_pid_is_dead_or_reused_has_no_process() {
    let me = std::process::id() as libc::pid_t;
    let mut r = running("acme");
    r.pid = Some(me);
    r.start = crate::proc::start_of(me);
    assert!(!process_gone(&r));
    r.start = Some("1.000001".into());
    assert!(process_gone(&r), "alive with another start");
    r.pid = Some(-1);
    assert!(process_gone(&r));
}
