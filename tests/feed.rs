//! The activity feed from a stream shaped like a real Claude Code run (hooks,
//! thinking, parallel calls answered out of order, a sandbox refusal), with
//! fictional content: what each call ran, how it came back, no duplicates.

use std::path::Path;

use cos::agents::Harness;
use cos::agents::harness::EntryKind;
use cos::agents::view::{feed, feed_line};

const EVENTS: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/claude-events.jsonl"
);

#[test]
fn each_call_shows_what_it_ran_and_how_it_came_back() {
    let (entries, next) = feed(Path::new(EVENTS), Harness::Claude, 0);
    let kinds: Vec<EntryKind> = entries.iter().map(|e| e.kind).collect();
    use EntryKind::*;
    assert_eq!(
        kinds,
        [
            Start, Call, Result, Call, Call, Call, Result, Denied, Result, Say, End
        ],
        "hooks, thinking and rate-limit lines show nothing"
    );
    let calls: Vec<(&str, &str)> = entries
        .iter()
        .filter(|e| e.kind == Call)
        .map(|e| (e.tool.as_deref().unwrap(), e.detail.as_str()))
        .collect();
    assert_eq!(
        calls,
        [
            ("Bash", "pwd; git status --short | head; ls -la"),
            ("Read", "/tmp/worktree/README.md"),
            ("Grep", "<script>alert(1)</script> in src/"),
            ("Bash", "cos report"),
        ],
        "the command, the path, the pattern: kept raw, the window escapes it"
    );
    let failed = entries
        .iter()
        .find(|e| e.call.as_deref() == Some("toolu_a") && e.kind == Result)
        .unwrap();
    assert_eq!(
        (failed.ok, failed.detail.as_str()),
        (Some(false), "fatal: not a git repository")
    );
    let grep = entries
        .iter()
        .find(|e| e.call.as_deref() == Some("toolu_c") && e.kind == Result)
        .unwrap();
    assert_eq!(
        grep.detail, "src/a.rs:3: match",
        "array content reads as text"
    );
    let denied = entries.iter().find(|e| e.kind == Denied).unwrap();
    assert_eq!(denied.call.as_deref(), Some("toolu_d"));
    assert!(denied.detail.contains("Operation not permitted"));
    assert_eq!(entries.last().unwrap().detail, "ended success, $0.21");
    assert_eq!(next, std::fs::metadata(EVENTS).unwrap().len());
}

#[test]
fn a_range_read_twice_keys_the_same_and_the_end_reads_nothing() {
    let (a, next) = feed(Path::new(EVENTS), Harness::Claude, 0);
    let (b, _) = feed(Path::new(EVENTS), Harness::Claude, 0);
    assert_eq!(a, b);
    let mut keys: Vec<&str> = a.iter().map(|e| e.key.as_str()).collect();
    let n = keys.len();
    keys.sort_unstable();
    keys.dedup();
    assert_eq!(keys.len(), n, "every entry has its own key");
    let (rest, same) = feed(Path::new(EVENTS), Harness::Claude, next);
    assert!(rest.is_empty());
    assert_eq!(same, next);
}

#[test]
fn a_line_still_being_written_waits_and_the_terminal_reads_it_plainly() {
    let dir = std::env::temp_dir().join(format!("cos-feed-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let f = dir.join("events.jsonl");
    let first = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"x1","name":"Bash","input":{"command":"task test"}}]}}"#;
    std::fs::write(&f, format!("{first}\n{{\"type\":\"assi")).unwrap();
    let (entries, next) = feed(&f, Harness::Claude, 0);
    assert_eq!(entries.len(), 1);
    assert_eq!(
        next as usize,
        first.len() + 1,
        "the partial line is read next time"
    );
    assert_eq!(feed_line(&entries[0], None), "call Bash  task test");
    let (all, _) = feed(Path::new(EVENTS), Harness::Claude, 0);
    let err = all
        .iter()
        .find(|e| e.ok == Some(false) && e.kind == cos::agents::harness::EntryKind::Result)
        .unwrap();
    assert_eq!(
        feed_line(err, Some("pwd; git status --short | head; ls -la")),
        "  ERR [pwd; git status --short | head; ls -la] fatal: not a git repository"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
