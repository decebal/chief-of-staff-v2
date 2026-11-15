use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::Duration;

use cos::proc;

/// Kills the group and reaps the leader on drop, so a failing assertion
/// leaves no process behind.
struct Grouped(Child);

impl Grouped {
    fn pid(&self) -> libc::pid_t {
        proc::group_of(&self.0)
    }
}

impl Drop for Grouped {
    fn drop(&mut self) {
        proc::kill_and_reap(&mut self.0);
    }
}

fn sleeper() -> Grouped {
    Grouped(proc::spawn_grouped(Command::new("/bin/sleep").arg("30")).unwrap())
}

/// A grouped `sh` leader with one background `sleep` in its group; returns
/// the leader and the member's pid.
fn leader_with_member() -> (Grouped, libc::pid_t) {
    let mut cmd = Command::new("/bin/sh");
    cmd.args(["-c", "/bin/sleep 30 & echo $!; wait"])
        .stdout(Stdio::piped());
    let mut child = Grouped(proc::spawn_grouped(&mut cmd).unwrap());
    let out = child.0.stdout.take().unwrap();
    let mut line = String::new();
    BufReader::new(out).read_line(&mut line).unwrap();
    let member: libc::pid_t = line.trim().parse().unwrap();
    assert_eq!(proc::pgid(member), Some(child.pid()), "member not in group");
    (child, member)
}

fn gone_within(pid: libc::pid_t, limit: Duration) -> bool {
    let until = proc::Deadline::after(limit);
    while proc::alive(pid) {
        if until.expired() {
            return false;
        }
        thread::sleep(Duration::from_millis(20));
    }
    true
}

#[cfg(target_os = "macos")]
#[test]
fn exe_is_the_canonical_binary_path() {
    let child = sleeper();
    let want = std::path::Path::new("/bin/sleep").canonicalize().unwrap();
    assert_eq!(proc::exe(child.pid()), Some(want));
}

#[cfg(not(target_os = "macos"))]
#[test]
fn exe_is_the_command_name() {
    let child = sleeper();
    let exe = proc::exe(child.pid()).unwrap();
    assert_eq!(exe.file_name().unwrap(), "sleep");
}

#[cfg(target_os = "macos")]
#[test]
fn start_time_is_some_and_stable() {
    let child = sleeper();
    let first = proc::start_time(child.pid());
    assert!(first.is_some());
    thread::sleep(Duration::from_millis(20));
    assert_eq!(proc::start_time(child.pid()), first);
}

#[test]
fn grouped_child_leads_its_own_group() {
    let child = sleeper();
    assert_eq!(proc::pgid(child.pid()), Some(child.pid()));
}

#[test]
fn signal_group_ends_leader_and_member() {
    let (mut child, member) = leader_with_member();
    proc::signal_group(child.pid(), libc::SIGTERM);
    child.0.wait().unwrap();
    assert!(
        gone_within(member, Duration::from_secs(5)),
        "member outlived SIGTERM"
    );
}

#[test]
fn signal_pid_ends_the_leader_only() {
    let (mut child, member) = leader_with_member();
    let pgid = child.pid();
    proc::signal_pid(pgid, libc::SIGTERM);
    child.0.wait().unwrap();
    assert!(proc::alive(member), "member ended with the leader");
    proc::kill_group(pgid);
    assert!(gone_within(member, Duration::from_secs(5)));
}

#[test]
fn signal_pid_refuses_a_non_positive_pid() {
    let (mut child, member) = leader_with_member();
    proc::signal_pid(-child.pid(), libc::SIGTERM);
    thread::sleep(Duration::from_millis(100));
    assert_eq!(child.0.try_wait().unwrap(), None, "leader was signalled");
    assert!(proc::alive(member), "group was signalled");
}
