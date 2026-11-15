//! `task schedule` as launchd sees it: the plists it writes and the launchctl calls it makes.
//! `HOME` is a sandbox, so plists land in its `Library/LaunchAgents`; `LAUNCHCTL` and `COS` are
//! overridden and `launchctl` is shadowed on PATH, so the real launchd domain is never touched.

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const COS: &str = env!("CARGO_BIN_EXE_cos");
const REPO: &str = env!("CARGO_MANIFEST_DIR");

struct Dir(PathBuf);

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn scratch(name: &str) -> Dir {
    let d = std::env::temp_dir().join(format!("cos-schedule-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    Dir(d.canonicalize().unwrap())
}

struct Biz {
    home: PathBuf,
    workspace: PathBuf,
    projects: PathBuf,
}

fn make_biz(root: &Path, name: &str) -> Biz {
    let base = root.join(name);
    let biz = Biz {
        home: base.join("home"),
        workspace: base.join("ws"),
        projects: base.join("projects"),
    };
    for d in [&biz.home, &biz.workspace, &biz.projects] {
        std::fs::create_dir_all(d).unwrap();
    }
    biz
}

fn write_registry(file: &Path, active: &str, businesses: &[(&str, &Biz)]) {
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    let mut text = format!("active = \"{active}\"\n");
    for (name, biz) in businesses {
        text.push_str(&format!(
            "\n[business.{name}]\nhome = \"{}\"\nworkspace = \"{}\"\nprojects = \"{}\"\n",
            biz.home.display(),
            biz.workspace.display(),
            biz.projects.display()
        ));
    }
    std::fs::write(file, text).unwrap();
}

struct Sandbox {
    root: Dir,
    home: PathBuf,
    registry: PathBuf,
    launchd: PathBuf,
    bin: PathBuf,
    stub_log: PathBuf,
}

impl Sandbox {
    fn new(name: &str) -> Self {
        let root = scratch(name);
        let home = root.0.join("user-home");
        let launchd = home.join("Library/LaunchAgents");
        let bin = root.0.join("bin");
        let stub_log = root.0.join("launchctl.log");
        let registry = root.0.join("cfg/businesses.toml");
        for d in [&home, &launchd, &bin] {
            std::fs::create_dir_all(d).unwrap();
        }
        // Like launchctl, `print` exits non-zero for a job that is not loaded; `cos.dash` is
        // loaded while `dash.pid` exists, and booting it out unloads it without touching the pid.
        // A `cos.dash` bootstrap while the pid in `old.pid` is alive logs `old pid alive`.
        let stub = bin.join("launchctl");
        let state = root.0.join("dash.pid");
        let old = root.0.join("old.pid");
        std::fs::write(
            &stub,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{log}'\ncase \"$*\" in\n  \
                 'print gui/'*/cos.dash)\n    [ -f '{st}' ] || exit 113\n    \
                 printf 'gui/%s/cos.dash = {{\\n\\tstate = running\\n\\tpid = %s\\n}}\\n' \"$(id -u)\" \"$(cat '{st}')\" ;;\n  \
                 'bootout gui/'*/cos.dash) rm -f '{st}' ;;\n  \
                 'bootstrap gui/'*/cos.dash.plist*)\n    \
                 [ -f '{old}' ] && ps -p \"$(cat '{old}')\" >/dev/null 2>&1 && echo 'old pid alive' >> '{log}' ;;\n\
                 esac\nexit 0\n",
                log = stub_log.display(),
                st = state.display(),
                old = old.display(),
            ),
        )
        .unwrap();
        std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();
        Sandbox {
            root,
            home,
            registry,
            launchd,
            bin,
            stub_log,
        }
    }

    fn stub(&self) -> PathBuf {
        self.bin.join("launchctl")
    }

    /// Marks `cos.dash` loaded in the fake launchctl, which then prints `pid = <pid>` for it.
    fn load_dash(&self, pid: u32) {
        std::fs::write(self.root.0.join("dash.pid"), pid.to_string()).unwrap();
        std::fs::write(self.root.0.join("old.pid"), pid.to_string()).unwrap();
    }

    /// An `lsof` ahead of the real one on PATH that logs `lsof <args>` to the launchctl log, then
    /// prints a header and `lines` and exits 0, or, given no lines, prints nothing and exits 1.
    fn fake_lsof(&self, lines: &[&str]) {
        let lsof = self.bin.join("lsof");
        let mut text = format!(
            "#!/bin/sh\nprintf 'lsof %s\\n' \"$*\" >> '{}'\n",
            self.stub_log.display()
        );
        if lines.is_empty() {
            text.push_str("exit 1\n");
        } else {
            text.push_str(
                "cat <<'EOF'\nCOMMAND   PID USER   FD   TYPE DEVICE SIZE/OFF NODE NAME\n",
            );
            for l in lines {
                text.push_str(l);
                text.push('\n');
            }
            text.push_str("EOF\n");
        }
        std::fs::write(&lsof, text).unwrap();
        std::fs::set_permissions(&lsof, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// The stub also copies both `cos.dash` plists into `snap/` when `cos.dash` is booted out,
    /// the last call before the move.
    fn snapshot_dash_at_bootout(&self) -> PathBuf {
        let snap = self.root.0.join("snap");
        std::fs::create_dir_all(&snap).unwrap();
        let (log, dir, s) = (
            self.stub_log.display(),
            self.launchd.display(),
            snap.display(),
        );
        std::fs::write(
            self.stub(),
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{log}'\ncase \"$*\" in bootout*/cos.dash)\n  \
                 cp '{dir}/cos.dash.plist' '{s}/installed' 2>/dev/null\n  \
                 cp '{dir}/cos.dash.plist.new' '{s}/new' 2>/dev/null\n;; esac\nexit 0\n"
            ),
        )
        .unwrap();
        snap
    }

    /// A `sed` ahead of the real one on PATH that fails only the `cos.dash` render.
    fn fail_dash_render(&self) {
        let sed = self.bin.join("sed");
        std::fs::write(
            &sed,
            "#!/bin/sh\ncase \"$*\" in *cos.dash.plist*) echo 'sed: dash render failed' >&2; exit 1 ;; esac\nexec /usr/bin/sed \"$@\"\n",
        )
        .unwrap();
        std::fs::set_permissions(&sed, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    fn plists(&self) -> BTreeMap<String, Vec<u8>> {
        std::fs::read_dir(&self.launchd)
            .unwrap()
            .map(|e| {
                let e = e.unwrap();
                let name = e.file_name().to_string_lossy().into_owned();
                (name, std::fs::read(e.path()).unwrap())
            })
            .collect()
    }

    fn launchctl_log(&self) -> String {
        std::fs::read_to_string(&self.stub_log).unwrap_or_default()
    }
}

fn schedule(sb: &Sandbox, business: Option<&str>) -> Output {
    run_task(sb, "schedule", business)
}

fn run_task(sb: &Sandbox, task: &str, business: Option<&str>) -> Output {
    run_task_on(sb, task, business, free_port())
}

fn run_task_on(sb: &Sandbox, task: &str, business: Option<&str>, dash_port: u16) -> Output {
    let mut cmd = Command::new("task");
    cmd.current_dir(&sb.root.0)
        .arg("--taskfile")
        .arg(Path::new(REPO).join("Taskfile.yml"))
        .arg(task);
    if let Some(b) = business {
        cmd.arg(format!("BUSINESS={b}"));
    }
    cmd.arg(format!("LAUNCHCTL={}", sb.stub().display()))
        .arg(format!("DASH_PORT={dash_port}"))
        .arg(format!("COS={COS}"))
        .env("PATH", path_with(&sb.bin))
        .env("HOME", &sb.home)
        .env("COS_BUSINESSES", &sb.registry)
        .env_remove("COS_HOME")
        .env_remove("COS_BUSINESS")
        .env_remove("COS_WORKSPACE")
        .env_remove("COS_PROJECTS")
        .env_remove("XDG_CONFIG_HOME");
    cmd.output().expect("run task (go-task) from PATH")
}

/// A port with no listener, so the run never meets whatever holds 8790 on this machine.
fn free_port() -> u16 {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    l.local_addr().unwrap().port()
}

fn path_with(first: &Path) -> String {
    let inherited = std::env::var("PATH").unwrap_or_default();
    format!("{}:{inherited}", first.display())
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn assert_bound_to(sb: &Sandbox, out: &Output, biz: &str, home: &Path) {
    assert!(out.status.success(), "task schedule: {}", stderr(out));
    for job in ["brief", "dash"] {
        let text = std::fs::read_to_string(sb.launchd.join(format!("cos.{job}.plist")))
            .unwrap_or_else(|e| panic!("cos.{job}.plist: {e}"));
        assert!(
            text.contains("<string>--business</string>"),
            "{job} plist lacks --business: {text}"
        );
        assert!(
            text.contains(&format!("<string>{biz}</string>")),
            "{job} plist lacks business {biz}: {text}"
        );
        assert!(
            text.contains(&format!("{}/out/logs/", home.display())),
            "{job} log paths not under {}: {text}",
            home.display()
        );
        assert!(
            !text.contains("COS_HOME"),
            "{job} plist sets COS_HOME: {text}"
        );
        assert!(
            !text.contains('@'),
            "{job} plist has unsubstituted tokens: {text}"
        );
        assert!(
            text.contains(&format!(
                "<key>COS_BUSINESSES</key><string>{}</string>",
                sb.registry.display()
            )),
            "{job} plist does not pin the registry: {text}"
        );
    }
    assert!(
        home.join("out/logs").is_dir(),
        "out/logs not created under {}",
        home.display()
    );
    let log = sb.launchctl_log();
    for job in ["brief", "dash"] {
        let bootout = log
            .lines()
            .position(|l| l.starts_with("bootout ") && l.ends_with(&format!("/cos.{job}")));
        let bootstrap = log
            .lines()
            .position(|l| l.starts_with("bootstrap ") && l.contains(&format!("cos.{job}.plist")));
        assert!(
            matches!((bootout, bootstrap), (Some(a), Some(b)) if a < b),
            "cos.{job} not booted out before bootstrap: {log:?}"
        );
    }
}

#[test]
fn acme_plists_carry_business() {
    let sb = Sandbox::new("acme");
    let acme = make_biz(&sb.root.0, "acme");
    let beta = make_biz(&sb.root.0, "beta");
    write_registry(&sb.registry, "beta", &[("acme", &acme), ("beta", &beta)]);

    let out = schedule(&sb, Some("acme"));
    assert_bound_to(&sb, &out, "acme", &acme.home);
}

#[test]
fn default_business_is_active() {
    let sb = Sandbox::new("default");
    let acme = make_biz(&sb.root.0, "acme");
    let beta = make_biz(&sb.root.0, "beta");
    write_registry(&sb.registry, "beta", &[("acme", &acme), ("beta", &beta)]);

    let out = schedule(&sb, None);
    assert_bound_to(&sb, &out, "beta", &beta.home);
}

#[test]
fn builtin_default_business_schedules() {
    let sb = Sandbox::new("builtin-default");
    let default = make_biz(&sb.root.0, "default");
    write_registry(&sb.registry, "default", &[("default", &default)]);

    let out = schedule(&sb, None);
    assert_bound_to(&sb, &out, "default", &default.home);
}

#[test]
fn failed_render_keeps_live_plists() {
    let mut sb = Sandbox::new("render-fail");
    let acme = make_biz(&sb.root.0, "acme");
    write_registry(&sb.registry, "acme", &[("acme", &acme)]);
    assert_bound_to(&sb, &schedule(&sb, Some("acme")), "acme", &acme.home);
    let live = |sb: &Sandbox| {
        let mut p = sb.plists();
        p.retain(|name, _| name.ends_with(".plist"));
        p
    };
    let before = live(&sb);
    let calls = sb.launchctl_log();

    let piped = sb.root.0.join("cfg|x/businesses.toml");
    std::fs::create_dir_all(piped.parent().unwrap()).unwrap();
    std::fs::copy(&sb.registry, &piped).unwrap();
    sb.registry = piped;
    let out = schedule(&sb, Some("acme"));

    assert!(
        !out.status.success(),
        "a `|` in the registry path must fail the render"
    );
    assert_eq!(live(&sb), before, "live plists changed by a failed render");
    let after = sb.launchctl_log();
    let touched: Vec<&str> = after[calls.len()..]
        .lines()
        .filter(|l| l.contains("cos.brief") || l.contains("cos.dash"))
        .collect();
    assert!(
        touched.is_empty(),
        "jobs reloaded after a failed render: {touched:?}"
    );
}

#[test]
fn dash_renders_new_before_replacing_installed() {
    let sb = Sandbox::new("dash-new");
    let acme = make_biz(&sb.root.0, "acme");
    write_registry(&sb.registry, "acme", &[("acme", &acme)]);
    std::fs::write(sb.launchd.join("cos.dash.plist"), "sentinel dash").unwrap();
    let snap = sb.snapshot_dash_at_bootout();

    let out = schedule(&sb, Some("acme"));
    assert_bound_to(&sb, &out, "acme", &acme.home);

    let installed = std::fs::read_to_string(snap.join("installed"))
        .expect("installed cos.dash.plist present when cos.dash is booted out");
    assert_eq!(
        installed, "sentinel dash",
        "installed cos.dash.plist replaced before the move"
    );
    let new = std::fs::read_to_string(snap.join("new"))
        .expect("cos.dash.plist.new written before cos.dash is booted out");
    assert!(
        new.contains("<string>acme</string>"),
        "cos.dash.plist.new not rendered: {new}"
    );
    assert!(
        !sb.launchd.join("cos.dash.plist.new").exists(),
        "cos.dash.plist.new left after the move"
    );
}

#[test]
fn failed_dash_render_removes_new_only() {
    let sb = Sandbox::new("dash-render-fail");
    let acme = make_biz(&sb.root.0, "acme");
    write_registry(&sb.registry, "acme", &[("acme", &acme)]);
    std::fs::write(sb.launchd.join("cos.dash.plist"), "sentinel dash").unwrap();
    sb.fail_dash_render();

    let out = schedule(&sb, Some("acme"));

    assert!(
        !out.status.success(),
        "a failed cos.dash render must fail the task"
    );
    let plists = sb.plists();
    assert!(
        !plists.contains_key("cos.dash.plist.new"),
        "cos.dash.plist.new left after a failed render: {:?}",
        plists.keys()
    );
    assert_eq!(
        plists.get("cos.dash.plist").map(Vec::as_slice),
        Some(&b"sentinel dash"[..]),
        "installed cos.dash.plist changed by a failed render"
    );
    let brief = String::from_utf8_lossy(
        plists
            .get("cos.brief.plist")
            .expect("cos.brief.plist installed"),
    )
    .into_owned();
    assert!(
        brief.contains("<string>acme</string>"),
        "cos.brief.plist not rendered: {brief}"
    );
    let log = sb.launchctl_log();
    assert!(
        log.lines()
            .any(|l| l.starts_with("bootstrap ") && l.contains("cos.brief.plist")),
        "cos.brief not loaded: {log:?}"
    );
    assert!(
        !log.contains("cos.dash"),
        "cos.dash touched after a failed render: {log:?}"
    );
}

/// A live process standing in for an old `cos.dash` that outlives its bootout; killed and reaped on drop.
struct Stuck(std::process::Child);

impl Drop for Stuck {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn old_dash_pid_alive_times_out_keeping_installed_plist() {
    let sb = Sandbox::new("dash-timeout");
    let acme = make_biz(&sb.root.0, "acme");
    write_registry(&sb.registry, "acme", &[("acme", &acme)]);
    let installed = b"sentinel dash\n<installed/>\n".to_vec();
    std::fs::write(sb.launchd.join("cos.dash.plist"), &installed).unwrap();
    let old = Stuck(Command::new("sleep").arg("60").spawn().unwrap());
    sb.load_dash(old.0.id());

    let out = schedule(&sb, Some("acme"));

    assert!(
        !out.status.success(),
        "an old cos.dash pid alive after 10 s must fail the task: {} {:?}",
        stderr(&out),
        sb.launchctl_log()
    );
    let plists = sb.plists();
    assert!(
        !plists.contains_key("cos.dash.plist.new"),
        "cos.dash.plist.new left after a timeout: {:?}",
        plists.keys()
    );
    assert_eq!(
        plists.get("cos.dash.plist"),
        Some(&installed),
        "installed cos.dash.plist changed by a timeout"
    );
    let brief = String::from_utf8_lossy(
        plists
            .get("cos.brief.plist")
            .expect("cos.brief.plist installed"),
    )
    .into_owned();
    assert!(
        brief.contains("<string>acme</string>"),
        "cos.brief.plist not rendered: {brief}"
    );
    let log = sb.launchctl_log();
    assert!(
        log.lines()
            .any(|l| l.starts_with("bootstrap ") && l.contains("cos.brief.plist")),
        "cos.brief not loaded: {log:?}"
    );
    assert!(
        log.lines()
            .any(|l| l.starts_with("bootout ") && l.ends_with("/cos.dash")),
        "cos.dash not booted out: {log:?}"
    );
    assert!(
        !log.lines()
            .any(|l| l.starts_with("bootstrap ") && l.contains("cos.dash.plist")),
        "cos.dash bootstrapped while its old pid is alive: {log:?}"
    );
}

#[test]
fn unloaded_dash_skips_the_wait() {
    let sb = Sandbox::new("dash-unloaded");
    let acme = make_biz(&sb.root.0, "acme");
    write_registry(&sb.registry, "acme", &[("acme", &acme)]);

    let start = std::time::Instant::now();
    let out = schedule(&sb, Some("acme"));
    let took = start.elapsed();

    assert_bound_to(&sb, &out, "acme", &acme.home);
    assert!(
        took < std::time::Duration::from_secs(10),
        "an empty old pid waited: {took:?}"
    );
    let log = sb.launchctl_log();
    let print = log
        .lines()
        .position(|l| l.starts_with("print gui/") && l.ends_with("/cos.dash"));
    let bootout = log
        .lines()
        .position(|l| l.starts_with("bootout ") && l.ends_with("/cos.dash"));
    assert!(
        matches!((print, bootout), (Some(a), Some(b)) if a < b),
        "old cos.dash pid not read before bootout: {log:?}"
    );
}

/// An old `cos.dash` that exits about 1 s after its bootout, reaped by a thread the moment it does,
/// so `ps -p` stops finding it; drop joins that thread.
struct Exiting(Option<std::thread::JoinHandle<()>>);

impl Exiting {
    fn spawn() -> (Self, u32) {
        let mut child = Command::new("sleep").arg("1").spawn().unwrap();
        let pid = child.id();
        let reaper = std::thread::spawn(move || {
            let _ = child.wait();
        });
        (Exiting(Some(reaper)), pid)
    }
}

impl Drop for Exiting {
    fn drop(&mut self) {
        if let Some(h) = self.0.take() {
            let _ = h.join();
        }
    }
}

#[test]
fn free_port_installs_and_bootstraps_dash_after_old_pid_exits() {
    let sb = Sandbox::new("dash-free");
    let acme = make_biz(&sb.root.0, "acme");
    write_registry(&sb.registry, "acme", &[("acme", &acme)]);
    std::fs::write(sb.launchd.join("cos.dash.plist"), "sentinel dash").unwrap();
    let (_old, pid) = Exiting::spawn();
    sb.load_dash(pid);
    sb.fake_lsof(&[]);
    let port = free_port();

    let out = run_task_on(&sb, "schedule", Some("acme"), port);

    assert_bound_to(&sb, &out, "acme", &acme.home);
    let log = sb.launchctl_log();
    let at = |p: &dyn Fn(&str) -> bool| log.lines().position(p);
    let bootout = at(&|l| l.starts_with("bootout ") && l.ends_with("/cos.dash"));
    let check = at(&|l| l == format!("lsof -nP -iTCP:{port} -sTCP:LISTEN"));
    let bootstrap = at(&|l| l.starts_with("bootstrap ") && l.contains("cos.dash.plist"));
    assert!(
        matches!((bootout, check, bootstrap), (Some(a), Some(b), Some(c)) if a < b && b < c),
        "cos.dash not bootstrapped after bootout and a port check: {log:?}"
    );
    assert!(
        !log.contains("old pid alive"),
        "cos.dash bootstrapped before its old pid exited: {log:?}"
    );
    assert!(
        !sb.launchd.join("cos.dash.plist.new").exists(),
        "cos.dash.plist.new left after the move"
    );
}

/// The installed `cos.dash.plist` never holds the rendered acme plist: it was not moved into place.
fn assert_dash_not_installed(sb: &Sandbox, out: &Output) {
    let installed = std::fs::read_to_string(sb.launchd.join("cos.dash.plist")).unwrap_or_default();
    assert!(
        !installed.contains("<string>acme</string>"),
        "cos.dash.plist moved into place while the port is held: {} {installed}",
        stderr(out)
    );
    let log = sb.launchctl_log();
    assert!(
        log.lines()
            .any(|l| l.starts_with("bootstrap ") && l.contains("cos.brief.plist")),
        "cos.brief not loaded: {log:?}"
    );
    assert!(
        !log.lines()
            .any(|l| l.starts_with("bootstrap ") && l.contains("cos.dash.plist")),
        "cos.dash bootstrapped while the port is held: {log:?}"
    );
}

#[test]
fn held_port_neither_moves_nor_bootstraps_dash() {
    let sb = Sandbox::new("dash-held");
    let acme = make_biz(&sb.root.0, "acme");
    write_registry(&sb.registry, "acme", &[("acme", &acme)]);
    std::fs::write(sb.launchd.join("cos.dash.plist"), "sentinel dash").unwrap();
    let holder = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = holder.local_addr().unwrap().port();

    let out = run_task_on(&sb, "schedule", Some("acme"), port);

    assert!(out.status.success(), "task schedule: {}", stderr(&out));
    assert_dash_not_installed(&sb, &out);
    drop(holder);
}

#[test]
fn several_listen_lines_give_one_held_with_first_non_old_pid() {
    let sb = Sandbox::new("dash-held-lines");
    let acme = make_biz(&sb.root.0, "acme");
    write_registry(&sb.registry, "acme", &[("acme", &acme)]);
    let mut gone = Command::new("true").spawn().unwrap();
    let old = gone.id();
    gone.wait().unwrap();
    sb.load_dash(old);
    sb.fake_lsof(&[
        &format!("cos     {old} me    9u  IPv4 0x1      0t0  TCP 127.0.0.1:8790 (LISTEN)"),
        "longhand  4242 me   10u  IPv4 0x2      0t0  TCP 127.0.0.1:8790 (LISTEN)",
        "longhand  4242 me   11u  IPv6 0x3      0t0  TCP [::1]:8790 (LISTEN)",
        "other     5151 me   12u  IPv4 0x4      0t0  TCP 127.0.0.1:8790 (LISTEN)",
    ]);

    let out = schedule(&sb, Some("acme"));

    assert!(out.status.success(), "task schedule: {}", stderr(&out));
    assert_dash_not_installed(&sb, &out);
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let held: Vec<&str> = text.lines().filter(|l| l.contains("held by")).collect();
    assert_eq!(held.len(), 1, "one Held report: {text}");
    assert!(
        held[0].contains("longhand") && held[0].contains("4242"),
        "Held names the first non-old listener: {text}"
    );
}

/// The `schedule` task's steps: a line at the step indent starts one, deeper lines continue it,
/// comment lines are dropped.
fn schedule_steps() -> Vec<String> {
    let text = std::fs::read_to_string(Path::new(REPO).join("Taskfile.yml")).unwrap();
    let start = text.find("\n  schedule:\n").expect("schedule task");
    let body = &text[start + 1..];
    let end = body[1..]
        .find("\n  unschedule:\n")
        .expect("unschedule task")
        + 1;
    let cmds = body[..end]
        .split("\n    cmds:\n")
        .nth(1)
        .expect("schedule cmds");
    let mut steps: Vec<String> = Vec::new();
    for line in cmds.lines() {
        if line.trim_start().starts_with('#') {
            continue;
        }
        if line.starts_with("      - ") {
            steps.push(line.trim().to_string());
        } else if let Some(last) = steps.last_mut() {
            last.push('\n');
            last.push_str(line.trim());
        }
    }
    steps
}

/// Every `cos.`-prefixed name in `step`, up to the first character outside `[a-z.]`.
fn cos_names(step: &str) -> Vec<&str> {
    step.match_indices("cos.")
        .map(|(i, _)| {
            let rest = &step[i..];
            let n = rest
                .find(|c: char| !(c.is_ascii_lowercase() || c == '.'))
                .unwrap_or(rest.len());
            rest[..n].trim_end_matches('.')
        })
        .collect()
}

#[test]
fn taskfile_dash_steps_name_only_dash_targets() {
    let steps = schedule_steps();
    for step in &steps {
        if let Some(line) = step.lines().find(|l| l.starts_with("- for:")) {
            assert_eq!(
                line, "- for: [brief]",
                "a loop still covers cos.dash: {step}"
            );
        }
    }
    let dash: Vec<&String> = steps.iter().filter(|s| s.contains("cos.dash")).collect();
    assert!(!dash.is_empty(), "no cos.dash steps in schedule");
    for step in &dash {
        assert!(
            !step.contains("{{.ITEM}}"),
            "cos.dash step in a loop: {step}"
        );
        for name in cos_names(step) {
            assert!(
                ["cos.dash", "cos.dash.plist", "cos.dash.plist.new"].contains(&name),
                "cos.dash step names {name}: {step}"
            );
        }
        for (i, _) in step.match_indices("{{.LAUNCHD}}/") {
            let path = &step[i..];
            let path = &path[..path.find(['"', ' ', '\n']).unwrap_or(path.len())];
            assert!(
                [
                    "{{.LAUNCHD}}/cos.dash.plist",
                    "{{.LAUNCHD}}/cos.dash.plist.new"
                ]
                .contains(&path),
                "cos.dash step names path {path}: {step}"
            );
        }
        for (i, _) in step.match_indices("gui/$(id -u)/") {
            let label = &step[i + "gui/$(id -u)/".len()..];
            assert!(
                label.starts_with("cos.dash ") || label == "cos.dash",
                "cos.dash step names another label: {step}"
            );
        }
    }

    let render: Vec<&&String> = dash
        .iter()
        .filter(|s| s.contains("launchd/cos.dash.plist"))
        .collect();
    assert_eq!(render.len(), 1, "one cos.dash render step: {render:?}");
    let render = render[0];
    let redirects: Vec<&str> = render
        .match_indices("> ")
        .map(|(i, _)| {
            let t = &render[i + 2..];
            &t[..t.find([' ', '\n']).unwrap_or(t.len())]
        })
        .collect();
    assert_eq!(
        redirects,
        ["\"{{.LAUNCHD}}/cos.dash.plist.new\""],
        "the render writes only cos.dash.plist.new: {render}"
    );
    let fallback = render
        .split_once("||")
        .map(|(_, f)| f)
        .unwrap_or_else(|| panic!("render has no failure branch: {render}"));
    assert!(
        fallback.contains("rm -f \"{{.LAUNCHD}}/cos.dash.plist.new\""),
        "a failed render does not remove .new: {render}"
    );
    assert!(
        !fallback.contains("\"{{.LAUNCHD}}/cos.dash.plist\""),
        "a failed render removes the installed plist: {render}"
    );
    assert!(
        fallback.contains("exit 1"),
        "a failed render does not fail the task: {render}"
    );
}

#[test]
fn unschedule_removes_plists() {
    let sb = Sandbox::new("unschedule");
    let acme = make_biz(&sb.root.0, "acme");
    write_registry(&sb.registry, "acme", &[("acme", &acme)]);
    assert_bound_to(&sb, &schedule(&sb, Some("acme")), "acme", &acme.home);

    let out = run_task(&sb, "unschedule", None);
    assert!(out.status.success(), "task unschedule: {}", stderr(&out));
    let left: Vec<String> = sb.plists().into_keys().collect();
    assert!(left.is_empty(), "plists left to reload at login: {left:?}");
    let log = sb.launchctl_log();
    for job in ["brief", "dash"] {
        assert!(
            log.lines()
                .filter(|l| l.starts_with("bootout ") && l.ends_with(&format!("/cos.{job}")))
                .count()
                >= 2,
            "cos.{job} not booted out by unschedule: {log:?}"
        );
    }
}

#[test]
fn unknown_business_changes_nothing() {
    let sb = Sandbox::new("unknown");
    let acme = make_biz(&sb.root.0, "acme");
    let beta = make_biz(&sb.root.0, "beta");
    write_registry(&sb.registry, "beta", &[("acme", &acme), ("beta", &beta)]);
    for (name, body) in [
        ("cos.brief.plist", "sentinel brief"),
        ("cos.dash.plist", "sentinel dash"),
        ("cos.review.plist", "sentinel review"),
    ] {
        std::fs::write(sb.launchd.join(name), body).unwrap();
    }
    let before = sb.plists();

    let out = schedule(&sb, Some("nope"));
    assert!(
        !out.status.success(),
        "BUSINESS=nope must fail: {}",
        stderr(&out)
    );
    assert_eq!(sb.plists(), before, "launchd dir changed");
    assert_eq!(sb.launchctl_log(), "", "launchctl was called");
    assert!(!acme.home.join("out/logs").exists());
    assert!(!beta.home.join("out/logs").exists());
}

#[test]
fn old_plist_with_cos_home_resolves_explicit() {
    let sb = Sandbox::new("explicit");
    let acme = make_biz(&sb.root.0, "acme");
    write_registry(&sb.registry, "acme", &[("acme", &acme)]);
    let old_home = sb.root.0.join("old-home");
    std::fs::create_dir_all(&old_home).unwrap();

    let out = Command::new(COS)
        .args(["business", "list"])
        .env("HOME", &sb.home)
        .env("COS_BUSINESSES", &sb.registry)
        .env("COS_HOME", &old_home)
        .env_remove("COS_BUSINESS")
        .env_remove("COS_WORKSPACE")
        .env_remove("COS_PROJECTS")
        .env_remove("XDG_CONFIG_HOME")
        .output()
        .unwrap();
    assert!(out.status.success(), "business list: {}", stderr(&out));
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(
        text.lines()
            .last()
            .unwrap()
            .starts_with("current: explicit "),
        "current line: {text}"
    );
}
