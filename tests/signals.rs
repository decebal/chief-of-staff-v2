//! `cos::signals`: sidebar row signals from file metadata and kernel checks.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use cos::agents::{Seen, State, Status};
use cos::signals::{self, Brief};

const COS: &str = env!("CARGO_BIN_EXE_cos");
/// Set only on the child run of a case, never in this process's own environment.
const CHILD_HOME: &str = "COS_SIGNALS_CHILD_HOME";
const CHILD_PID: &str = "COS_SIGNALS_CHILD_PID";

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("cos-signals-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d.canonicalize().unwrap()
}

/// A long-lived `cos` process, killed and reaped on drop.
struct CosProc(Child);

impl CosProc {
    fn start(root: &Path) -> Self {
        let home = root.join("cos-home");
        std::fs::create_dir_all(&home).unwrap();
        let child = Command::new(COS)
            .arg("--home")
            .arg(&home)
            .arg("--workspace")
            .arg(&home)
            .arg("--projects")
            .arg(&home)
            .arg("mcp")
            .env_remove("COS_HOME")
            .env_remove("COS_WORKSPACE")
            .env_remove("COS_PROJECTS")
            .env_remove("COS_BUSINESS")
            .env_remove("XDG_CONFIG_HOME")
            .env("HOME", root.join("user-home"))
            .env("COS_BUSINESSES", root.join("none.toml"))
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut p = CosProc(child);
        let pid = p.pid();
        let until = Instant::now() + Duration::from_secs(10);
        while !cos::proc::exe(pid).is_some_and(|e| e.file_name().is_some_and(|f| f == "cos")) {
            assert!(Instant::now() < until, "cos {pid} never ran");
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(p.0.try_wait().unwrap().is_none(), "cos mcp exited early");
        p
    }

    fn pid(&self) -> i32 {
        self.0.id() as i32
    }
}

impl Drop for CosProc {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// A pid that no longer names a live process.
fn dead_pid() -> i32 {
    let mut child = Command::new("true").spawn().unwrap();
    let pid = child.id() as i32;
    child.wait().unwrap();
    pid
}

/// How a run that has not ended is held.
enum Held {
    /// Herdr has it.
    InHerdr,
    /// This process prepares it.
    Launcher(i32),
    /// Nothing does.
    Nothing,
}

fn write_run(home: &Path, id: &str, created: &str, held: Held) {
    let dir = home.join("out/agents").join(id);
    std::fs::create_dir_all(&dir).unwrap();
    let spec = serde_json::json!({
        "id": id,
        "harness": "claude",
        "program": "claude",
        "workdir": home,
        "worktree": false,
        "title": "t",
        "source": "test",
        "prompt": "p",
        "deadline_secs": 60,
        "created": created,
        "terminal": format!("cos-{id}"),
    });
    std::fs::write(dir.join("spec.json"), serde_json::to_vec(&spec).unwrap()).unwrap();
    let mut state = State::default();
    match held {
        Held::InHerdr => {
            state.status = Status::Running;
            state.herdr = Some(Seen {
                address: "t1".into(),
                live: true,
                ..Seen::default()
            });
        }
        Held::Launcher(pid) => {
            state.launcher = Some(pid);
            state.launcher_started = cos::agents::started_at(pid);
        }
        Held::Nothing => {}
    }
    std::fs::write(dir.join("state.json"), serde_json::to_vec(&state).unwrap()).unwrap();
}

fn now() -> String {
    jiff::Timestamp::now().to_string()
}

fn long_ago() -> String {
    "2020-01-01T00:00:00Z".to_owned()
}

fn run_records(home: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut out = Vec::new();
    for run in std::fs::read_dir(home.join("out/agents"))
        .unwrap()
        .flatten()
    {
        for file in std::fs::read_dir(run.path()).unwrap().flatten() {
            out.push((file.path(), std::fs::read(file.path()).unwrap()));
        }
    }
    out.sort();
    out
}

#[cfg(target_os = "macos")]
#[test]
fn live_is_counted_with_no_ps_spawned() {
    if let Some(home) = std::env::var_os(CHILD_HOME) {
        let pid: i32 = std::env::var(CHILD_PID).unwrap().parse().unwrap();
        let s = signals::read(Path::new(&home));
        assert_eq!((s.live, s.lost, s.starting), (2, 0, 1));
        assert_eq!(s.launchers, vec![pid]);
        return;
    }

    let root = scratch("no-ps");
    let home = root.join("home");
    let launcher = CosProc::start(&root);
    write_run(&home, "live", &long_ago(), Held::InHerdr);
    write_run(
        &home,
        "preparing",
        &long_ago(),
        Held::Launcher(launcher.pid()),
    );

    let bin = root.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let marker = root.join("ps-ran");
    let ps = bin.join("ps");
    std::fs::write(
        &ps,
        format!("#!/bin/sh\ntouch '{}'\nexit 1\n", marker.display()),
    )
    .unwrap();
    std::fs::set_permissions(&ps, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut path = std::ffi::OsString::from(&bin);
    if let Some(rest) = std::env::var_os("PATH") {
        path.push(":");
        path.push(rest);
    }

    let out = Command::new(std::env::current_exe().unwrap())
        .args([
            "live_is_counted_with_no_ps_spawned",
            "--exact",
            "--test-threads=1",
            "--nocapture",
        ])
        .env("PATH", &path)
        .env(CHILD_HOME, &home)
        .env(CHILD_PID, launcher.pid().to_string())
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.status.success(), "child run failed: {text}");
    assert!(text.contains("1 passed"), "child ran no case: {text}");
    assert!(!marker.exists(), "signals::read spawned ps");
}

#[test]
fn lost_and_starting_are_counted_and_launchers_hold_live_pids_only() {
    let root = scratch("counts");
    let home = root.join("home");
    let launcher = CosProc::start(&root);
    let dead = dead_pid();
    write_run(&home, "live", &long_ago(), Held::InHerdr);
    write_run(
        &home,
        "preparing",
        &long_ago(),
        Held::Launcher(launcher.pid()),
    );
    write_run(&home, "lost", &long_ago(), Held::Launcher(dead));
    write_run(&home, "starting", &now(), Held::Nothing);
    write_run(&home, "stale", &long_ago(), Held::Nothing);

    let before = run_records(&home);
    let s = signals::read(&home);
    assert_eq!(s.live, 3, "the run in Herdr and the two being prepared");
    assert_eq!(s.starting, 2);
    assert_eq!(s.lost, 2, "the dead launcher and the stale start");
    assert_eq!(s.launchers, vec![launcher.pid()]);
    assert!(!s.launchers.contains(&dead));
    assert_eq!(run_records(&home), before, "signals::read wrote a record");
}

#[test]
fn latest_brief_is_the_greatest_dated_filename() {
    let root = scratch("brief");
    let home = root.join("home");
    assert_eq!(signals::read(&home).brief, None, "no briefs directory");

    let briefs = home.join("out/briefs");
    std::fs::create_dir_all(&briefs).unwrap();
    for name in [
        "2026-11-13.json",
        "2026-11-14.json",
        "2026-11-15.md",
        "2026-11-15.json.bak",
        "2026-11-15-x.json",
        "2026-1-150.json",
        "2026-11-1a.json",
        "latest.json",
        "zzzz-zz-zz.json",
    ] {
        std::fs::write(briefs.join(name), b"{}").unwrap();
    }
    std::fs::create_dir_all(briefs.join("2027-01-01.json.d")).unwrap();

    let meta = std::fs::metadata(briefs.join("2026-11-14.json")).unwrap();
    let mtime = signals::mtime_ms(&meta);
    assert!(mtime > 0);
    assert_eq!(
        signals::read(&home).brief,
        Some(Brief {
            file: "2026-11-14.json".into(),
            mtime,
        })
    );
}

#[test]
fn store_latest_keeps_dated_names_and_carries_the_file_mtime() {
    let root = scratch("store-latest");
    let home = root.join("home");
    let briefs = home.join("out/briefs");
    std::fs::create_dir_all(&briefs).unwrap();
    let brief = serde_json::json!({ "headline": "Dated.", "needs_you": [] }).to_string();
    for name in [
        "2026-11-13.json",
        "2026-11-14.json",
        "2026-11-15-x.json",
        "2026-1-150.json",
        "latest.json",
        "zzzz-zz-zz.json",
    ] {
        std::fs::write(briefs.join(name), &brief).unwrap();
    }

    let saved = cos::store::latest::<serde_json::Value>(&home, cos::store::Kind::Briefs)
        .unwrap()
        .unwrap();
    let mtime = signals::mtime_ms(&std::fs::metadata(briefs.join("2026-11-14.json")).unwrap());
    assert_eq!(saved.day, "2026-11-14");
    assert_eq!(saved.mtime_ms, mtime);

    let data = cos::report::data(&home, &home, false).unwrap();
    assert_eq!(
        data["briefRef"],
        serde_json::json!({ "file": "2026-11-14.json", "mtime": mtime })
    );
}

#[test]
fn store_latest_never_pairs_content_with_another_writes_mtime() {
    let root = scratch("store-race");
    let home = root.join("home");
    let briefs = home.join("out/briefs");
    std::fs::create_dir_all(&briefs).unwrap();
    let file = briefs.join("2026-11-14.json");
    let pad = "x".repeat(16 * 1024);
    let body = move |seq: usize| serde_json::json!({ "seq": seq, "pad": pad }).to_string();
    std::fs::write(&file, body(0)).unwrap();
    let first = signals::mtime_ms(&std::fs::metadata(&file).unwrap());

    let writer = {
        let file = file.clone();
        std::thread::spawn(move || {
            let mut written = vec![first];
            for seq in 1..5000 {
                std::fs::write(&file, body(seq)).unwrap();
                written.push(signals::mtime_ms(&std::fs::metadata(&file).unwrap()));
            }
            written
        })
    };

    let mut reads = Vec::new();
    // is_finished also turns true when the writer panics, so a failed write ends the loop.
    while !writer.is_finished() {
        reads.push(
            cos::store::latest::<serde_json::Value>(&home, cos::store::Kind::Briefs)
                .map(|s| s.unwrap())
                .map(|s| (s.value["seq"].as_u64().unwrap() as usize, s.mtime_ms))
                .map_err(|e| format!("{e:#}")),
        );
    }
    let written = writer.join().unwrap();

    assert!(!reads.is_empty());
    for read in reads {
        match read {
            Ok((seq, mtime)) => assert_eq!(
                written[seq], mtime,
                "content of write {seq} paired with another write's mtime"
            ),
            Err(e) => assert!(e.starts_with("parse "), "not a parse failure: {e}"),
        }
    }
}

#[test]
fn mtime_ms_is_whole_milliseconds_since_the_epoch() {
    let root = scratch("mtime");
    let file = root.join("f");
    std::fs::write(&file, b"x").unwrap();
    let meta = std::fs::metadata(&file).unwrap();
    let want = meta
        .modified()
        .unwrap()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    assert_eq!(signals::mtime_ms(&meta), want);
}

fn brief_plist(path: &Path, args: &[&str], env: &[(&str, &Path)]) {
    let mut xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\"><dict>\n<key>Label</key><string>cos.brief</string>\n",
    );
    xml.push_str("<key>ProgramArguments</key><array>\n");
    for a in args {
        xml.push_str(&format!("<string>{a}</string>\n"));
    }
    xml.push_str("</array>\n<key>EnvironmentVariables</key><dict>\n");
    for (k, v) in env {
        xml.push_str(&format!("<key>{k}</key><string>{}</string>\n", v.display()));
    }
    xml.push_str("</dict>\n</dict></plist>\n");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, xml).unwrap();
}

#[test]
fn scheduled_names_the_business_only_for_the_same_registry() {
    let root = scratch("scheduled");
    let user_home = root.join("user-home");
    let plist = user_home.join("Library/LaunchAgents/cos.brief.plist");
    let registry = root.join("cfg/businesses.toml");
    std::fs::create_dir_all(registry.parent().unwrap()).unwrap();
    std::fs::write(&registry, "").unwrap();
    let app_registry = registry.canonicalize().unwrap();
    let args = ["/usr/local/bin/cos", "--business", "acme", "brief"];

    brief_plist(&plist, &args, &[("COS_BUSINESSES", &registry)]);
    assert_eq!(
        signals::scheduled(&plist, &app_registry, Some(&user_home)),
        Some("acme".into())
    );

    let link = root.join("link");
    std::os::unix::fs::symlink(root.join("cfg"), &link).unwrap();
    brief_plist(
        &plist,
        &args,
        &[("COS_BUSINESSES", &link.join("businesses.toml"))],
    );
    assert_eq!(
        signals::scheduled(&plist, &app_registry, Some(&user_home)),
        Some("acme".into()),
        "both paths are canonicalised"
    );

    let fallback = user_home.join(".config/cos/businesses.toml");
    std::fs::create_dir_all(fallback.parent().unwrap()).unwrap();
    std::fs::write(&fallback, "").unwrap();
    brief_plist(&plist, &args, &[]);
    assert_eq!(
        signals::scheduled(&plist, &fallback.canonicalize().unwrap(), Some(&user_home)),
        Some("acme".into()),
        "no COS_BUSINESSES falls back to HOME from the user home"
    );
    assert_eq!(
        signals::scheduled(&plist, &fallback.canonicalize().unwrap(), None),
        None,
        "no key and no user home leaves HOME unset"
    );

    let other = root.join("other/businesses.toml");
    std::fs::create_dir_all(other.parent().unwrap()).unwrap();
    std::fs::write(&other, "").unwrap();
    brief_plist(&plist, &args, &[("COS_BUSINESSES", &other)]);
    assert_eq!(
        signals::scheduled(&plist, &app_registry, Some(&user_home)),
        None,
        "mismatched registry"
    );

    brief_plist(
        &plist,
        &["/usr/local/bin/cos", "brief"],
        &[("COS_BUSINESSES", &registry)],
    );
    assert_eq!(
        signals::scheduled(&plist, &app_registry, Some(&user_home)),
        None,
        "legacy plist"
    );

    std::fs::write(&plist, b"<plist><dict><key>").unwrap();
    assert_eq!(
        signals::scheduled(&plist, &app_registry, Some(&user_home)),
        None,
        "parse error"
    );

    std::fs::remove_file(&plist).unwrap();
    assert_eq!(
        signals::scheduled(&plist, &app_registry, Some(&user_home)),
        None,
        "no plist"
    );
}
