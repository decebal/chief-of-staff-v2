//! A proposal's `claude` dies with its owner: the stop flag ends it from the
//! spawning thread, and `cos proc-guard` ends its group at the limit even
//! when the process that started the guard is SIGKILLed.

use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const COS: &str = env!("CARGO_BIN_EXE_cos");

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("cos-guard-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d.canonicalize().unwrap()
}

/// A program that never writes to stdout, starts a `sleep` child, records
/// both pids in `dir`, and waits.
fn silent(dir: &Path) -> PathBuf {
    let p = dir.join("silent.sh");
    std::fs::write(
        &p,
        format!(
            "#!/bin/bash\nsleep 600 &\necho $! > {d}/child\necho $$ > {d}/self\nwait\n",
            d = dir.display()
        ),
    )
    .unwrap();
    let mut perm = std::fs::metadata(&p).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut perm, 0o755);
    std::fs::set_permissions(&p, perm).unwrap();
    p
}

fn pid(dir: &Path, name: &str) -> libc::pid_t {
    let at = Instant::now();
    loop {
        if let Ok(s) = std::fs::read_to_string(dir.join(name))
            && let Ok(n) = s.trim().parse()
        {
            return n;
        }
        assert!(at.elapsed() < Duration::from_secs(10), "no {name} pid");
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn gone_within(p: libc::pid_t, limit: Duration) -> bool {
    let at = Instant::now();
    while at.elapsed() < limit {
        if !cos::proc::alive(p) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

#[test]
fn the_guard_ends_its_group_at_the_limit_after_its_parent_is_killed() {
    let dir = scratch("orphan");
    let prog = silent(&dir);
    // The parent stands in for an owner that dies without running any code:
    // it starts the guard as a group leader, then is SIGKILLed.
    let mut parent = Command::new("/bin/bash")
        .arg("-c")
        .arg(format!(
            "{COS} proc-guard --secs 2 -- {} & sleep 600",
            prog.display()
        ))
        .process_group(0)
        .spawn()
        .unwrap();
    let (me, child) = (pid(&dir, "self"), pid(&dir, "child"));
    // SAFETY: the test's own child.
    unsafe {
        libc::kill(parent.id() as libc::pid_t, libc::SIGKILL);
    }
    let _ = parent.wait();
    assert!(cos::proc::alive(me), "still running right after the kill");
    assert!(
        gone_within(me, Duration::from_secs(6)),
        "guarded program outlived the limit"
    );
    assert!(
        gone_within(child, Duration::from_secs(6)),
        "its child outlived the limit"
    );
}

#[test]
fn the_guard_passes_through_the_exit_code_and_times_out_with_124() {
    let ok = Command::new(COS)
        .args(["proc-guard", "--secs", "5", "--", "/bin/sh", "-c", "exit 3"])
        .status()
        .unwrap();
    assert_eq!(ok.code(), Some(3));
    let late = Command::new(COS)
        .args(["proc-guard", "--secs", "1", "--", "/bin/sleep", "30"])
        .status()
        .unwrap();
    assert_eq!(late.code(), Some(cos::proc::GUARD_TIMEOUT));
}

#[test]
fn a_child_that_ignores_sigterm_still_ends_with_the_guard() {
    let dir = scratch("stubborn");
    let p = dir.join("stubborn.sh");
    std::fs::write(
        &p,
        format!(
            "#!/bin/bash\n(trap '' TERM; exec /bin/sh -c 'echo $$ > {d}/child; exec sleep 600') &\nwait\n",
            d = dir.display()
        ),
    )
    .unwrap();
    let mut perm = std::fs::metadata(&p).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut perm, 0o755);
    std::fs::set_permissions(&p, perm).unwrap();
    let mut guard = Command::new(COS)
        .args(["proc-guard", "--secs", "1", "--"])
        .arg(&p)
        .spawn()
        .unwrap();
    let child = pid(&dir, "child");
    let status = guard.wait().unwrap();
    assert_eq!(status.code(), Some(cos::proc::GUARD_TIMEOUT));
    assert!(
        gone_within(child, Duration::from_secs(3)),
        "a member that ignores SIGTERM outlived the guard"
    );
}

#[test]
fn the_stop_flag_kills_and_reaps_from_the_spawning_thread() {
    let dir = scratch("stop");
    let prog = silent(&dir);
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    let run = std::thread::spawn(move || {
        let mut cmd = Command::new(&prog);
        cos::proc::stream_lines_until(&mut cmd, Duration::from_secs(60), &flag, |_| Ok(true))
    });
    let (me, child) = (pid(&dir, "self"), pid(&dir, "child"));
    stop.store(true, Ordering::SeqCst);
    let err = run.join().unwrap().unwrap_err();
    assert!(err.is::<cos::proc::Cancelled>(), "{err:#}");
    assert!(gone_within(me, Duration::from_secs(3)));
    assert!(gone_within(child, Duration::from_secs(3)));
}
