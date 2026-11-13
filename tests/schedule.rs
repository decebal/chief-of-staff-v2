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
        let stub = bin.join("launchctl");
        std::fs::write(
            &stub,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\n",
                stub_log.display()
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
    let mut cmd = Command::new("task");
    cmd.current_dir(&sb.root.0)
        .arg("--taskfile")
        .arg(Path::new(REPO).join("Taskfile.yml"))
        .arg(task);
    if let Some(b) = business {
        cmd.arg(format!("BUSINESS={b}"));
    }
    cmd.arg(format!("LAUNCHCTL={}", sb.stub().display()))
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
