//! The registry at `businesses.toml`: load, add, and the file's location.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use anyhow::Result;
use cos::business::{self, Business, Dirs, Inputs, Layer, Registry, Sources, Triple, resolve};

struct Dir(PathBuf);

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn scratch(name: &str) -> Dir {
    let d = std::env::temp_dir().join(format!("cos-biz-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    Dir(d.canonicalize().unwrap())
}

fn subdir(root: &Dir, name: &str) -> PathBuf {
    let p = root.0.join(name);
    std::fs::create_dir_all(&p).unwrap();
    p.canonicalize().unwrap()
}

fn registry_file(root: &Dir) -> PathBuf {
    root.0.join("cfg").join("businesses.toml")
}

fn one_dir(home: &Path, projects: &Path) -> Business {
    Business {
        home: home.to_owned(),
        workspace: home.to_owned(),
        projects: projects.to_owned(),
    }
}

fn quoted(path: &Path) -> String {
    format!("\"{}\"", path.display())
}

fn write_text(file: &Path, text: &str) {
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, text).unwrap();
}

fn business_block(name: &str, home: &Path, projects: &Path) -> String {
    format!(
        "[business.{name}]\nhome = {}\nworkspace = {}\nprojects = {}\n",
        quoted(home),
        quoted(home),
        quoted(projects)
    )
}

#[test]
fn registry_round_trip() {
    let root = scratch("round-trip");
    let file = registry_file(&root);
    let (acme, beta) = (subdir(&root, "acme"), subdir(&root, "beta"));
    let projects = subdir(&root, "projects");
    let text = format!(
        "active = \"acme\"\n\n{}",
        business_block("acme", &acme, &projects)
    );
    write_text(&file, &text);

    let loaded = business::load(&file).unwrap();
    assert_eq!(loaded.active.as_deref(), Some("acme"));
    assert_eq!(loaded.businesses["acme"], one_dir(&acme, &projects));

    business::add(&file, "beta", one_dir(&beta, &projects), false).unwrap();
    let after = business::load(&file).unwrap();
    assert_eq!(after.active.as_deref(), Some("acme"), "add keeps active");
    assert_eq!(after.businesses.len(), 2);
    assert_eq!(after.businesses["beta"], one_dir(&beta, &projects));
}

#[test]
fn missing_file_is_empty() {
    let root = scratch("missing");
    let loaded = business::load(&registry_file(&root)).unwrap();
    assert_eq!(loaded, Registry::default());
}

#[test]
fn load_errors_name_file_and_key() {
    let root = scratch("load-errors");
    let file = registry_file(&root);
    let projects = subdir(&root, "projects");
    let home = subdir(&root, "home");

    let cases = [
        ("parse", "active = \n".to_owned(), "businesses.toml"),
        ("non-slug", business_block("Acme", &home, &projects), "Acme"),
        (
            "dangling",
            format!(
                "active = \"ghost\"\n\n{}",
                business_block("acme", &home, &projects)
            ),
            "ghost",
        ),
    ];
    for (case, text, key) in cases {
        write_text(&file, &text);
        let before = std::fs::read(&file).unwrap();
        let err = business::load(&file).unwrap_err().to_string();
        assert!(
            err.contains(&file.display().to_string()),
            "{case}: error must name the file, got {err}"
        );
        assert!(
            err.contains(key),
            "{case}: error must name {key}, got {err}"
        );
        assert_eq!(
            std::fs::read(&file).unwrap(),
            before,
            "{case}: file untouched"
        );
    }
}

#[test]
fn active_absent_is_no_active() {
    let root = scratch("no-active");
    let file = registry_file(&root);
    let home = subdir(&root, "home");
    let projects = subdir(&root, "projects");
    write_text(&file, &business_block("acme", &home, &projects));

    let loaded = business::load(&file).unwrap();
    assert_eq!(loaded.active, None);
    assert_eq!(loaded.businesses.len(), 1);
}

#[test]
fn add_writes_entry() {
    let root = scratch("add");
    let file = registry_file(&root);
    let home = subdir(&root, "home");
    let projects = subdir(&root, "projects");

    business::add(&file, "acme", one_dir(&home, &projects), false).unwrap();

    assert!(
        file.is_file(),
        "first write creates the directory and the file"
    );
    let loaded = business::load(&file).unwrap();
    assert_eq!(loaded.businesses["acme"], one_dir(&home, &projects));
}

#[test]
fn add_refuses_bad_names_and_duplicates() {
    let root = scratch("bad-names");
    let file = registry_file(&root);
    let home = subdir(&root, "home");
    let projects = subdir(&root, "projects");
    business::add(&file, "acme", one_dir(&home, &projects), false).unwrap();
    let before = std::fs::read(&file).unwrap();

    let fresh = subdir(&root, "fresh");
    for name in ["../x", "Acme", "", "-acme", "a b"] {
        let err = business::add(&file, name, one_dir(&fresh, &projects), false);
        assert!(err.is_err(), "name {name:?} must be refused");
        assert_eq!(
            std::fs::read(&file).unwrap(),
            before,
            "name {name:?}: file byte-identical"
        );
    }

    let dup = business::add(&file, "acme", one_dir(&home, &projects), false);
    assert!(dup.is_err(), "existing name without --replace is refused");
    assert_eq!(
        std::fs::read(&file).unwrap(),
        before,
        "duplicate: file byte-identical"
    );

    let other = subdir(&root, "other");
    business::add(&file, "acme", one_dir(&other, &projects), true).unwrap();
    let loaded = business::load(&file).unwrap();
    assert_eq!(
        loaded.businesses["acme"],
        one_dir(&other, &projects),
        "--replace overwrites"
    );
}

#[test]
fn add_refuses_bad_or_shared_dirs() {
    let root = scratch("bad-dirs");
    let file = registry_file(&root);
    let home = subdir(&root, "home");
    let projects = subdir(&root, "projects");
    business::add(&file, "acme", one_dir(&home, &projects), false).unwrap();
    let before = std::fs::read(&file).unwrap();

    let a_file = root.0.join("a-file");
    std::fs::write(&a_file, "not a dir").unwrap();
    let missing = root.0.join("missing");

    let relative = one_dir(Path::new("rel/home"), &projects);
    let not_there = one_dir(&missing, &projects);
    let not_dir = one_dir(&a_file, &projects);
    let shared_home = one_dir(&home, &projects);
    let shared_workspace = Business {
        home: subdir(&root, "fresh"),
        workspace: home.clone(),
        projects: projects.clone(),
    };

    for (case, dirs) in [
        ("relative", relative),
        ("missing", not_there),
        ("not a directory", not_dir),
        ("home used by acme", shared_home),
        ("workspace used by acme", shared_workspace),
    ] {
        let err = business::add(&file, "beta", dirs, false);
        assert!(err.is_err(), "{case}: must be refused");
        assert_eq!(
            std::fs::read(&file).unwrap(),
            before,
            "{case}: file byte-identical"
        );
    }
}

struct Restore(PathBuf);

impl Drop for Restore {
    fn drop(&mut self) {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o755));
    }
}

#[test]
fn unwritable_dir_leaves_file() {
    let root = scratch("unwritable");
    let file = registry_file(&root);
    let home = subdir(&root, "home");
    let projects = subdir(&root, "projects");
    business::add(&file, "acme", one_dir(&home, &projects), false).unwrap();
    let before = std::fs::read(&file).unwrap();

    use std::os::unix::fs::PermissionsExt;
    let dir = file.parent().unwrap().to_owned();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).unwrap();
    let _restore = Restore(dir);

    let err = business::add(&file, "beta", one_dir(&home, &projects), false);
    assert!(err.is_err(), "a write into an unwritable directory fails");
    assert_eq!(std::fs::read(&file).unwrap(), before, "file byte-identical");
}

#[test]
fn concurrent_adds_both_land() {
    let root = scratch("concurrent");
    let file = registry_file(&root);
    let projects = subdir(&root, "projects");

    for round in 0..50 {
        let (a, b) = (format!("r{round}-a"), format!("r{round}-b"));
        let (home_a, home_b) = (subdir(&root, &a), subdir(&root, &b));
        std::thread::scope(|s| {
            s.spawn(|| business::add(&file, &a, one_dir(&home_a, &projects), false).unwrap());
            s.spawn(|| business::add(&file, &b, one_dir(&home_b, &projects), false).unwrap());
        });
        let loaded = business::load(&file).unwrap();
        assert!(
            loaded.businesses.contains_key(&a),
            "round {round}: {a} lost"
        );
        assert!(
            loaded.businesses.contains_key(&b),
            "round {round}: {b} lost"
        );
    }
}

#[test]
fn location_follows_env_order() {
    let os = |s: &str| Some(OsString::from(s));
    assert_eq!(
        business::location_from(os("/a/b.toml"), os("/x"), os("/h")).unwrap(),
        PathBuf::from("/a/b.toml")
    );
    assert_eq!(
        business::location_from(None, os("/x"), os("/h")).unwrap(),
        PathBuf::from("/x/cos/businesses.toml")
    );
    assert_eq!(
        business::location_from(None, None, os("/h")).unwrap(),
        PathBuf::from("/h/.config/cos/businesses.toml")
    );
    assert!(business::location_from(None, None, None).is_err());
}

fn p(s: &str) -> PathBuf {
    PathBuf::from(s)
}

fn dirs(home: Option<&str>, workspace: Option<&str>, projects: Option<&str>) -> Dirs {
    Dirs {
        home: home.map(p),
        workspace: workspace.map(p),
        projects: projects.map(p),
    }
}

fn sources(business: Option<&str>, dirs: Dirs) -> Sources {
    Sources {
        business: business.map(str::to_owned),
        dirs,
    }
}

fn inputs(flags: Sources, env: Sources) -> Inputs {
    Inputs {
        flags,
        env,
        install: p("/install"),
        user_home: Some(p("/u")),
    }
}

fn none() -> Sources {
    sources(None, dirs(None, None, None))
}

fn biz(name: &str) -> Business {
    let root = p(&format!("/biz/{name}"));
    Business {
        home: root.join("home"),
        workspace: root.join("ws"),
        projects: root.join("projects"),
    }
}

fn registry(active: Option<&str>, names: &[&str]) -> Registry {
    Registry {
        active: active.map(str::to_owned),
        businesses: names.iter().map(|n| (n.to_string(), biz(n))).collect(),
    }
}

fn two_businesses() -> Result<Registry> {
    Ok(registry(Some("acme"), &["acme", "beta"]))
}

fn unset_active() -> Result<Registry> {
    Ok(registry(None, &["acme"]))
}

fn no_registry() -> Result<Registry> {
    panic!("the registry must not be loaded for this resolution")
}

fn triple(
    home: &str,
    workspace: &str,
    projects: &str,
    business: Option<&str>,
    layer: Layer,
) -> Triple {
    Triple {
        home: p(home),
        workspace: p(workspace),
        projects: p(projects),
        business: business.map(str::to_owned),
        layer,
    }
}

fn named(name: &str, layer: Layer) -> Triple {
    let b = biz(name);
    Triple {
        home: b.home,
        workspace: b.workspace,
        projects: b.projects,
        business: Some(name.to_owned()),
        layer,
    }
}

#[test]
fn resolve_layers() {
    let acme = || named("acme", Layer::Flags);
    assert_eq!(
        resolve(
            &inputs(sources(Some("acme"), dirs(None, None, None)), none()),
            two_businesses
        )
        .unwrap(),
        acme()
    );
    assert_eq!(
        resolve(
            &inputs(none(), sources(Some("acme"), dirs(None, None, None))),
            two_businesses
        )
        .unwrap(),
        named("acme", Layer::Env)
    );
    assert_eq!(
        resolve(&inputs(none(), none()), two_businesses).unwrap(),
        named("acme", Layer::Registry)
    );
    assert_eq!(
        resolve(&inputs(none(), none()), unset_active).unwrap(),
        triple("/install", "/install", "/u/Projects", None, Layer::Defaults)
    );
    assert_eq!(
        resolve(
            &inputs(sources(None, dirs(Some("/h"), None, None)), none()),
            no_registry
        )
        .unwrap(),
        triple("/h", "/h", "/u/Projects", None, Layer::Flags)
    );
    assert_eq!(
        resolve(
            &inputs(none(), sources(None, dirs(Some("/h"), None, None))),
            no_registry
        )
        .unwrap(),
        triple("/h", "/h", "/u/Projects", None, Layer::Env)
    );
    assert!(
        resolve(
            &inputs(sources(Some("ghost"), dirs(None, None, None)), none()),
            two_businesses
        )
        .is_err(),
        "a business name the registry does not hold is an error"
    );
}

#[test]
fn higher_layer_wins() {
    let flags_business = sources(Some("acme"), dirs(None, None, None));
    let env_home = sources(None, dirs(Some("/h"), None, None));
    assert_eq!(
        resolve(&inputs(flags_business, env_home), two_businesses).unwrap(),
        named("acme", Layer::Flags)
    );

    let flags_home = sources(None, dirs(Some("/x"), None, None));
    let env_business = sources(Some("acme"), dirs(None, None, None));
    assert_eq!(
        resolve(&inputs(flags_home, env_business), no_registry).unwrap(),
        triple("/x", "/x", "/u/Projects", None, Layer::Flags)
    );
}

#[test]
fn in_layer_conflicts_error() {
    let flag_both = sources(Some("acme"), dirs(Some("/h"), None, None));
    assert!(resolve(&inputs(flag_both, none()), no_registry).is_err());

    let env_both = sources(Some("acme"), dirs(Some("/h"), None, None));
    assert!(resolve(&inputs(none(), env_both), no_registry).is_err());
}

#[test]
fn partial_env_fills_from_defaults() {
    assert_eq!(
        resolve(
            &inputs(none(), sources(None, dirs(Some("/h"), None, None))),
            no_registry
        )
        .unwrap(),
        triple("/h", "/h", "/u/Projects", None, Layer::Env),
        "COS_HOME alone is never completed from the active business"
    );
    assert_eq!(
        resolve(
            &inputs(none(), sources(None, dirs(None, None, Some("/p")))),
            no_registry
        )
        .unwrap(),
        triple("/install", "/install", "/p", None, Layer::Env)
    );
}

#[test]
fn defaults_are_build_clone_and_projects() {
    assert_eq!(
        resolve(&inputs(none(), none()), || Ok(Registry::default())).unwrap(),
        triple("/install", "/install", "/u/Projects", None, Layer::Defaults)
    );
}

#[test]
fn explicit_triple_never_loads_registry() {
    let flags_home = sources(None, dirs(Some("/h"), None, None));
    let flags_projects = sources(None, dirs(None, None, Some("/p")));
    let env_dirs = sources(None, dirs(Some("/h"), Some("/w"), Some("/p")));
    assert!(resolve(&inputs(flags_home, none()), no_registry).is_ok());
    assert!(resolve(&inputs(flags_projects, none()), no_registry).is_ok());
    assert!(resolve(&inputs(none(), env_dirs), no_registry).is_ok());
}

#[test]
fn no_active_skips_layer_three() {
    assert_eq!(
        resolve(&inputs(none(), none()), unset_active).unwrap(),
        triple("/install", "/install", "/u/Projects", None, Layer::Defaults)
    );
}

#[test]
fn unset_home_errors() {
    let mut no_home = inputs(none(), none());
    no_home.user_home = None;
    assert!(resolve(&no_home, || Ok(Registry::default())).is_err());

    let mut no_home = inputs(sources(None, dirs(Some("/h"), None, None)), none());
    no_home.user_home = None;
    assert!(
        resolve(&no_home, no_registry).is_err(),
        "projects falls back to $HOME/Projects"
    );
}

#[test]
fn failed_seed_leaves_no_context() {
    let root = scratch("seed-fail");
    let template = subdir(&root, "template");
    let home = subdir(&root, "home");
    std::fs::write(template.join("goals.md"), "goals").unwrap();
    let nested = template.join("nested");
    std::fs::create_dir_all(&nested).unwrap();
    // A dangling link fails the copy for any user, root included.
    let broken = nested.join("x.md");
    std::os::unix::fs::symlink(root.0.join("missing"), &broken).unwrap();

    let seeded = business::seed_context(&template, &home);

    assert!(seeded.is_err(), "an uncopyable template must fail the seed");
    let left: Vec<_> = std::fs::read_dir(&home).unwrap().collect();
    assert!(
        left.is_empty(),
        "home must hold nothing after a failed seed: {left:?}"
    );
    std::fs::remove_file(&broken).unwrap();
    std::fs::write(&broken, "x").unwrap();
    assert!(business::seed_context(&template, &home).unwrap());
    assert_eq!(
        std::fs::read_to_string(home.join("context/nested/x.md")).unwrap(),
        "x"
    );
}

#[test]
fn seed_ignores_a_stale_staging_dir() {
    let root = scratch("seed-stale");
    let template = subdir(&root, "template");
    let home = subdir(&root, "home");
    std::fs::write(template.join("goals.md"), "goals").unwrap();
    let stale = home.join(format!(".context.seeding-{}", std::process::id()));
    std::fs::create_dir_all(&stale).unwrap();
    std::fs::write(stale.join("removed-from-template.md"), "old").unwrap();

    assert!(business::seed_context(&template, &home).unwrap());

    let mut seeded: Vec<_> = std::fs::read_dir(home.join("context"))
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    seeded.sort();
    assert_eq!(seeded, ["goals.md"]);
    assert!(!stale.exists(), "staging dir left behind");
}
