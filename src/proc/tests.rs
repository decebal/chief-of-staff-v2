use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use super::*;

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("cos-proc-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A `sleep` leading its own group, owned by the test and reaped on drop.
struct Sleeper(std::process::Child);

impl Sleeper {
    fn start() -> Self {
        let mut cmd = Command::new("sleep");
        cmd.arg("60").stdout(Stdio::null()).stderr(Stdio::null());
        Sleeper(spawn_grouped(&mut cmd).unwrap())
    }

    fn group(&self) -> Group {
        let pgid = group_of(&self.0);
        Group {
            pgid,
            start: start_of(pgid).unwrap(),
        }
    }
}

impl Drop for Sleeper {
    fn drop(&mut self) {
        kill_and_reap(&mut self.0);
    }
}

fn until_gone(g: &Group) -> bool {
    let until = Instant::now() + Duration::from_secs(5);
    while !group_gone(g) {
        if Instant::now() > until {
            return false;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    true
}

#[test]
fn a_live_leader_with_the_recorded_start_is_killed() {
    let mut s = Sleeper::start();
    let g = s.group();
    assert!(may_kill(&g));
    assert!(kill_recorded(&g));
    let _ = s.0.wait();
    assert!(until_gone(&g));
}

#[test]
fn a_live_leader_with_another_start_is_never_killed() {
    let s = Sleeper::start();
    let real = s.group();
    let other = Group {
        pgid: real.pgid,
        start: "1.000001".into(),
    };
    assert!(!may_kill(&other));
    assert!(!kill_recorded(&other));
    std::thread::sleep(Duration::from_millis(100));
    assert!(
        group_alive(real.pgid),
        "a group with another start was killed"
    );
    assert!(
        group_gone(&other),
        "its pgid was reused: the recorded group is gone"
    );
    assert!(!group_gone(&real));
}

#[test]
fn a_group_whose_leader_exited_while_a_member_lives_is_killed() {
    let dir = scratch("orphan");
    let member = dir.join("member.pid");
    let mut cmd = Command::new("/bin/sh");
    cmd.arg("-c")
        .arg(format!(
            "sleep 60 </dev/null >/dev/null 2>&1 & echo $! > '{}'",
            member.display()
        ))
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut leader = spawn_grouped(&mut cmd).unwrap();
    let pgid = group_of(&leader);
    let g = Group {
        pgid,
        start: start_of(pgid).unwrap(),
    };
    wait_until(&mut leader, &Deadline::after(Duration::from_secs(10))).unwrap();
    assert!(group_alive(pgid), "the member outlives its leader");
    assert!(start_of(pgid).is_none(), "the leader is gone");
    assert!(!group_gone(&g));
    assert!(may_kill(&g));
    assert!(kill_recorded(&g));
    assert!(until_gone(&g), "the member survived its group kill");
}

#[test]
fn an_empty_group_is_gone_and_not_killed() {
    let mut s = Sleeper::start();
    let g = s.group();
    kill_and_reap(&mut s.0);
    assert!(group_gone(&g));
    assert!(!may_kill(&g));
}

#[test]
fn wait_gone_returns_the_groups_still_alive_at_the_bound() {
    let s = Sleeper::start();
    let g = s.group();
    let started = Instant::now();
    let left = wait_gone(std::slice::from_ref(&g), Duration::from_millis(300));
    assert_eq!(left, vec![g]);
    assert!(started.elapsed() >= Duration::from_millis(300));
    assert!(started.elapsed() < Duration::from_secs(3));
}

#[test]
fn start_strings_have_microseconds() {
    let s = start_of(std::process::id() as libc::pid_t).unwrap();
    let (sec, usec) = s.split_once('.').unwrap();
    assert!(sec.parse::<u64>().unwrap() > 0);
    assert_eq!(usec.len(), 6);
    assert!(start_of(-1).is_none());
}

fn marker_cmd(mark: &Path) -> Command {
    let mut cmd = Command::new("/bin/sh");
    cmd.arg("-c")
        .arg(format!("touch '{}'", mark.display()))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    cmd
}

#[test]
fn a_gated_child_execs_only_once_its_group_is_registered() {
    let dir = scratch("gate-open");
    let mark = dir.join("execd");
    let mut seen = None;
    let mut child = spawn_gated(&mut marker_cmd(&mark), |g| {
        std::thread::sleep(Duration::from_millis(300));
        assert!(
            !mark.exists(),
            "the child ran before its group was recorded"
        );
        assert_eq!(pgid(g.pgid), Some(g.pgid), "the held child leads its group");
        assert!(group_alive(g.pgid));
        seen = Some(g.clone());
        Ok(())
    })
    .unwrap();
    let g = seen.expect("the hook saw the group");
    assert_eq!(g.pgid, group_of(&child));
    let status = wait_until(&mut child, &Deadline::after(Duration::from_secs(10))).unwrap();
    assert!(status.success());
    assert!(mark.exists(), "the released child never ran");
}

#[test]
fn a_gated_child_whose_registration_fails_never_execs() {
    let dir = scratch("gate-refused");
    let mark = dir.join("execd");
    let mut pgid = 0;
    let err = spawn_gated(&mut marker_cmd(&mark), |g| {
        pgid = g.pgid;
        anyhow::bail!("record write failed")
    })
    .expect_err("a failed registration fails the spawn");
    assert!(
        format!("{err:#}").contains("record write failed"),
        "{err:#}"
    );
    assert!(pgid > 0);
    std::thread::sleep(Duration::from_millis(200));
    assert!(!mark.exists(), "the child ran without a record");
    assert!(!alive(pgid), "the refused child was not reaped");
}

#[test]
fn a_gated_spawn_of_a_missing_program_fails_without_a_live_group() {
    let mut pgid = 0;
    let r = spawn_gated(
        Command::new("/nonexistent/cos-no-such-program").stdin(Stdio::null()),
        |g| {
            pgid = g.pgid;
            Ok(())
        },
    );
    assert!(r.is_err());
    assert!(pgid > 0, "the gate never registered the group: {r:?}");
    assert!(!group_alive(pgid));
}
