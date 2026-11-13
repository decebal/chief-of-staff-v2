//! The one resolver as the `cos` binary sees it: flags, env, the registry, and what each
//! long-lived process prints when it starts.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const COS: &str = env!("CARGO_BIN_EXE_cos");

struct Dir(PathBuf);

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn scratch(name: &str) -> Dir {
    let d = std::env::temp_dir().join(format!("cos-biz-cli-{name}-{}", std::process::id()));
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

fn cos(root: &Path, registry: &Path, env: &[(&str, &Path)], args: &[&str]) -> Output {
    let mut cmd = Command::new(COS);
    cmd.args(args)
        .current_dir(root)
        .env_remove("COS_HOME")
        .env_remove("COS_WORKSPACE")
        .env_remove("COS_PROJECTS")
        .env_remove("COS_BUSINESS")
        .env_remove("XDG_CONFIG_HOME")
        .env("HOME", root.join("user-home"))
        .env("COS_BUSINESSES", registry);
    for (key, value) in env {
        cmd.env(key, value);
    }
    cmd.output().unwrap()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn init_state(root: &Path, registry: &Path, ws: &Path) {
    let out = cos(
        root,
        registry,
        &[],
        &["--workspace", ws.to_str().unwrap(), "state", "init"],
    );
    assert!(out.status.success(), "state init: {}", stderr(&out));
}

#[test]
fn business_and_home_flags_conflict() {
    let root = scratch("conflict");
    let registry = root.0.join("cfg/businesses.toml");
    let acme = make_biz(&root.0, "acme");
    write_registry(&registry, "acme", &[("acme", &acme)]);
    let other = root.0.join("other");

    let out = cos(
        &root.0,
        &registry,
        &[],
        &[
            "--business",
            "acme",
            "--home",
            other.to_str().unwrap(),
            "report",
        ],
    );
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("--business"), "must name --business: {err}");
    assert!(err.contains("--home"), "must name --home: {err}");
}

#[test]
fn flag_beats_env() {
    let root = scratch("flag-beats-env");
    let registry = root.0.join("cfg/businesses.toml");
    let acme = make_biz(&root.0, "acme");
    write_registry(&registry, "acme", &[("acme", &acme)]);
    init_state(&root.0, &registry, &acme.workspace);
    let env_home = root.0.join("env-home");
    std::fs::create_dir_all(&env_home).unwrap();

    let out = cos(
        &root.0,
        &registry,
        &[("COS_HOME", &env_home)],
        &["--business", "acme", "report", "--no-open"],
    );
    assert!(out.status.success(), "report: {}", stderr(&out));
    assert!(acme.home.join("out/report.html").is_file());
    assert!(!env_home.join("out/report.html").exists());
}

#[test]
fn state_init_needs_no_profile() {
    let root = scratch("state-init");
    let registry = root.0.join("cfg/businesses.toml");
    std::fs::create_dir_all(registry.parent().unwrap()).unwrap();
    std::fs::write(&registry, "active = \n").unwrap();
    let ws = root.0.join("fresh-ws");
    std::fs::create_dir_all(&ws).unwrap();

    let out = cos(
        &root.0,
        &registry,
        &[],
        &["--workspace", ws.to_str().unwrap(), "state", "init"],
    );
    assert!(out.status.success(), "state init: {}", stderr(&out));
    assert!(ws.join(".chief-of-staff").is_dir());
}

#[test]
fn missing_state_hint_names_state_init() {
    let root = scratch("missing-state");
    let registry = root.0.join("cfg/businesses.toml");
    let ws = root.0.join("empty-ws");
    std::fs::create_dir_all(&ws).unwrap();

    let out = cos(
        &root.0,
        &registry,
        &[],
        &["--workspace", ws.to_str().unwrap(), "state", "read"],
    );
    assert!(!out.status.success());
    let hint = format!("cos --workspace {} state init", ws.display());
    assert!(
        stderr(&out).contains(&hint),
        "hint must read `{hint}`: {}",
        stderr(&out)
    );
}

#[test]
fn long_lived_prints_business() {
    let root = scratch("long-lived");
    let registry = root.0.join("cfg/businesses.toml");
    let acme = make_biz(&root.0, "acme");
    write_registry(&registry, "acme", &[("acme", &acme)]);

    let out = cos(&root.0, &registry, &[], &["mcp"]);
    let err = stderr(&out);
    assert!(
        err.contains("business: acme"),
        "mcp must name its business: {err}"
    );
    assert!(
        err.contains(&acme.home.display().to_string()),
        "and its triple: {err}"
    );
}

#[test]
fn mcp_follows_active_on_next_start() {
    let root = scratch("mcp-follows");
    let registry = root.0.join("cfg/businesses.toml");
    let acme = make_biz(&root.0, "acme");
    let beta = make_biz(&root.0, "beta");
    write_registry(&registry, "acme", &[("acme", &acme), ("beta", &beta)]);

    let first = stderr(&cos(&root.0, &registry, &[], &["mcp"]));
    assert!(first.contains("business: acme"), "first start: {first}");

    write_registry(&registry, "beta", &[("acme", &acme), ("beta", &beta)]);
    let second = stderr(&cos(&root.0, &registry, &[], &["mcp"]));
    assert!(second.contains("business: beta"), "second start: {second}");
    assert!(!second.contains("business: acme"), "second start: {second}");
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .canonicalize()
        .unwrap()
}

fn business(root: &Path, registry: &Path, args: &[&str]) -> Output {
    let mut all = vec!["business"];
    all.extend_from_slice(args);
    cos(root, registry, &[], &all)
}

fn add(root: &Path, registry: &Path, name: &str, biz: &Biz) -> Output {
    business(
        root,
        registry,
        &[
            "add",
            name,
            biz.home.to_str().unwrap(),
            biz.workspace.to_str().unwrap(),
            biz.projects.to_str().unwrap(),
        ],
    )
}

#[test]
fn add_use_list_remove() {
    let root = scratch("add-use-list-remove");
    let registry = root.0.join("cfg/businesses.toml");
    let acme = make_biz(&root.0, "acme");
    let beta = make_biz(&root.0, "beta");

    let out = add(&root.0, &registry, "acme", &acme);
    assert!(out.status.success(), "add acme: {}", stderr(&out));
    let out = add(&root.0, &registry, "beta", &beta);
    assert!(out.status.success(), "add beta: {}", stderr(&out));

    let out = business(&root.0, &registry, &["use", "beta"]);
    assert!(out.status.success(), "use beta: {}", stderr(&out));

    let out = business(&root.0, &registry, &["list"]);
    assert!(out.status.success(), "list: {}", stderr(&out));
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(
        text.lines().any(|l| l.starts_with("* beta ")),
        "active line: {text}"
    );
    assert!(
        text.lines().any(|l| l.starts_with("  acme ")),
        "inactive line: {text}"
    );
    assert!(
        text.lines().any(|l| l.starts_with("  default ")),
        "default kept: {text}"
    );
    assert!(
        text.lines()
            .last()
            .unwrap()
            .starts_with("current: beta home="),
        "current line: {text}"
    );

    let out = business(&root.0, &registry, &["remove", "acme"]);
    assert!(out.status.success(), "remove acme: {}", stderr(&out));
    let text = std::fs::read_to_string(&registry).unwrap();
    assert!(!text.contains("[business.acme]"), "acme removed: {text}");
    assert!(text.contains("[business.beta]"), "beta kept: {text}");
}

#[test]
fn use_unknown_refused() {
    let root = scratch("use-unknown");
    let registry = root.0.join("cfg/businesses.toml");
    let acme = make_biz(&root.0, "acme");
    write_registry(&registry, "acme", &[("acme", &acme)]);
    let before = std::fs::read(&registry).unwrap();

    let out = business(&root.0, &registry, &["use", "ghost"]);
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains("ghost"),
        "must name it: {}",
        stderr(&out)
    );
    assert_eq!(std::fs::read(&registry).unwrap(), before);
}

#[test]
fn remove_active_refused() {
    let root = scratch("remove-active");
    let registry = root.0.join("cfg/businesses.toml");
    let acme = make_biz(&root.0, "acme");
    let beta = make_biz(&root.0, "beta");
    write_registry(&registry, "acme", &[("acme", &acme), ("beta", &beta)]);

    let out = business(&root.0, &registry, &["remove", "acme"]);
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains("active"),
        "must say active: {}",
        stderr(&out)
    );
    let text = std::fs::read_to_string(&registry).unwrap();
    assert!(text.contains("[business.acme]"), "acme kept: {text}");
}

#[test]
fn first_run_writes_default_from_build_clone() {
    let root = scratch("first-run");
    let registry = root.0.join("cfg/businesses.toml");
    let acme = make_biz(&root.0, "acme");

    let out = add(&root.0, &registry, "acme", &acme);
    assert!(out.status.success(), "add acme: {}", stderr(&out));
    assert!(
        stderr(&out).contains("wrote default"),
        "first-run notice: {}",
        stderr(&out)
    );
    let text = std::fs::read_to_string(&registry).unwrap();
    assert!(text.contains("active = \"default\""), "{text}");
    assert!(text.contains("[business.default]"), "{text}");
    assert!(text.contains("[business.acme]"), "{text}");
    let repo = repo_root();
    assert!(
        text.contains(&format!("home = \"{}\"", repo.display())),
        "default is the build clone: {text}"
    );
    assert_eq!(
        text.matches("home = ").count(),
        2,
        "different homes: {text}"
    );
}

#[test]
fn add_without_profile_hints_state_init() {
    let root = scratch("add-no-profile");
    let registry = root.0.join("cfg/businesses.toml");
    let acme = make_biz(&root.0, "acme");

    let out = add(&root.0, &registry, "acme", &acme);
    assert!(out.status.success(), "add acme: {}", stderr(&out));
    let hint = format!(
        "cos --workspace {} state init",
        acme.workspace.canonicalize().unwrap().display()
    );
    assert!(
        stderr(&out).contains(&hint),
        "hint `{hint}`: {}",
        stderr(&out)
    );
}

#[test]
fn bad_dir_message_names_business_add() {
    let root = scratch("bad-dir");
    let registry = root.0.join("cfg/businesses.toml");
    let acme = make_biz(&root.0, "acme");
    let missing = root.0.join("nowhere");

    let out = business(
        &root.0,
        &registry,
        &[
            "add",
            "acme",
            missing.to_str().unwrap(),
            acme.workspace.to_str().unwrap(),
            acme.projects.to_str().unwrap(),
        ],
    );
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains("cos business add"),
        "{}",
        stderr(&out)
    );
    assert!(!registry.exists(), "a refused add writes no registry");
}

#[test]
fn home_fails_when_projects_missing() {
    let root = scratch("home-projects");
    let registry = root.0.join("cfg/businesses.toml");
    let acme = make_biz(&root.0, "acme");
    write_registry(&registry, "acme", &[("acme", &acme)]);

    let out = business(&root.0, &registry, &["home", "acme"]);
    assert!(out.status.success(), "home: {}", stderr(&out));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        acme.home.display().to_string()
    );

    std::fs::remove_dir_all(&acme.projects).unwrap();
    let out = business(&root.0, &registry, &["home", "acme"]);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("projects"), "{}", stderr(&out));
}

#[test]
fn corrupt_registry_errors_but_explicit_and_internal_run() {
    let root = scratch("corrupt");
    let registry = root.0.join("cfg/businesses.toml");
    let acme = make_biz(&root.0, "acme");
    write_registry(&registry, "ghost", &[("acme", &acme)]);

    let out = business(&root.0, &registry, &["list"]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(
        err.contains(&registry.display().to_string()),
        "names file: {err}"
    );
    assert!(err.contains("active"), "names key: {err}");

    let ws = root.0.join("explicit-ws");
    std::fs::create_dir_all(&ws).unwrap();
    let out = cos(
        &root.0,
        &registry,
        &[],
        &["--workspace", ws.to_str().unwrap(), "state", "init"],
    );
    assert!(out.status.success(), "state init: {}", stderr(&out));

    let lock = root.0.join("build.lock");
    std::fs::write(&lock, "").unwrap();
    let out = cos(
        &root.0,
        &registry,
        &[],
        &[
            "agents",
            "build-slot",
            "--lock",
            lock.to_str().unwrap(),
            "/usr/bin/true",
        ],
    );
    assert!(out.status.success(), "build-slot: {}", stderr(&out));
}

#[test]
fn add_seeds_context() {
    let root = scratch("seed-context");
    let registry = root.0.join("cfg/businesses.toml");
    let acme = make_biz(&root.0, "acme");

    let out = add(&root.0, &registry, "acme", &acme);
    assert!(out.status.success(), "add acme: {}", stderr(&out));
    let template = repo_root().join("templates/context");
    let entries: Vec<_> = std::fs::read_dir(&template)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert!(!entries.is_empty(), "template has entries");
    for name in entries {
        assert!(
            acme.home.join("context").join(&name).exists(),
            "{name:?} seeded into home/context"
        );
    }
}

#[test]
fn add_keeps_existing_context() {
    let root = scratch("keep-context");
    let registry = root.0.join("cfg/businesses.toml");
    let acme = make_biz(&root.0, "acme");
    std::fs::create_dir_all(acme.home.join("context")).unwrap();
    std::fs::write(acme.home.join("context/goals.md"), "mine\n").unwrap();

    let out = add(&root.0, &registry, "acme", &acme);
    assert!(out.status.success(), "add acme: {}", stderr(&out));
    assert_eq!(
        std::fs::read_to_string(acme.home.join("context/goals.md")).unwrap(),
        "mine\n"
    );
}

#[test]
fn claude_runs_from_install() {
    use std::os::unix::fs::PermissionsExt;

    let root = scratch("claude-install");
    let install = root.0.join("install");
    std::fs::create_dir_all(&install).unwrap();
    std::fs::write(install.join("CLAUDE.md"), "install rules\n").unwrap();
    let acme = make_biz(&root.0, "acme");
    assert!(!acme.home.join("CLAUDE.md").exists());

    let record = root.0.join("cwd.txt");
    let stub = root.0.join("claude-stub");
    std::fs::write(
        &stub,
        format!("#!/bin/sh\npwd -P > '{}'\n", record.display()),
    )
    .unwrap();
    std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();

    let cfg = cos::pipeline::Config {
        home: acme.home.clone(),
        install: install.clone(),
        workspace: acme.workspace.clone(),
        projects: acme.projects.clone(),
        budget: None,
        bins: cos::pipeline::Bins {
            claude: stub,
            pmset: None,
            caffeinate: None,
        },
    };
    // The stub prints no result, so the question fails after it has run.
    let _ = cfg.ask("where do you run");
    assert_eq!(
        std::fs::read_to_string(&record).unwrap().trim(),
        install.canonicalize().unwrap().display().to_string()
    );
}
