//! `cos connect`: the `cos` entry in Claude Desktop and Codex. HOME and CODEX_HOME
//! point at temp dirs, so no real client config is read or written.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const COS: &str = env!("CARGO_BIN_EXE_cos");

struct Dir(PathBuf);

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn scratch(name: &str) -> Dir {
    let d = std::env::temp_dir().join(format!("cos-connect-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    Dir(d.canonicalize().unwrap())
}

struct Home {
    _dir: Dir,
    home: PathBuf,
    codex: PathBuf,
}

fn home(name: &str) -> Home {
    let dir = scratch(name);
    let home = dir.0.join("home");
    let codex = dir.0.join("codex-home");
    fs::create_dir_all(&home).unwrap();
    Home {
        _dir: dir,
        home,
        codex,
    }
}

impl Home {
    fn desktop(&self) -> PathBuf {
        self.home
            .join("Library/Application Support/Claude/claude_desktop_config.json")
    }

    fn codex_toml(&self) -> PathBuf {
        self.codex.join("config.toml")
    }

    fn tool_path(&self) -> String {
        let home = self.home.display();
        format!(
            "{home}/.cargo/bin:{home}/.local/bin:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin"
        )
    }
}

fn connect(h: &Home, args: &[&str]) -> Output {
    Command::new(COS)
        .arg("connect")
        .args(args)
        .current_dir(&h.home)
        .env_remove("COS_HOME")
        .env_remove("COS_WORKSPACE")
        .env_remove("COS_PROJECTS")
        .env_remove("COS_BUSINESS")
        .env_remove("COS_BUSINESSES")
        .env_remove("XDG_CONFIG_HOME")
        .env("HOME", &h.home)
        .env("CODEX_HOME", &h.codex)
        .env("PATH", "/usr/bin:/bin")
        .output()
        .unwrap()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn status_rows(h: &Home) -> Vec<Value> {
    let out = connect(h, &["status", "--json"]);
    assert!(out.status.success(), "status: {}", stderr(&out));
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    v["clients"].as_array().unwrap().clone()
}

fn row<'a>(rows: &'a [Value], client: &str) -> &'a Value {
    rows.iter().find(|r| r["client"] == client).unwrap()
}

fn cos_bin() -> &'static str {
    COS
}

fn script(path: &Path, body: &str) {
    fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
fn status_reports_both_unregistered() {
    let h = home("unregistered");
    let rows = status_rows(&h);
    assert_eq!(rows.len(), 2);
    for client in ["claude-desktop", "codex"] {
        let r = row(&rows, client);
        assert_eq!(r["registered"], false, "{client}");
        assert_eq!(r["ok"], false, "{client}");
        assert!(r["command"].is_null(), "{client}");
        assert_eq!(r["problems"], json!(["registered"]), "{client}");
        assert_eq!(r["daemon"], "unknown", "{client}");
    }
}

#[test]
fn register_both_then_status_is_green() {
    let h = home("register");
    let out = connect(&h, &["claude-desktop", "--bin", cos_bin()]);
    assert!(out.status.success(), "{}", stderr(&out));
    let out = connect(&h, &["codex", "--bin", cos_bin()]);
    assert!(out.status.success(), "{}", stderr(&out));

    let rows = status_rows(&h);
    for client in ["claude-desktop", "codex"] {
        let r = row(&rows, client);
        assert_eq!(r["ok"], true, "{client}: {r}");
        assert_eq!(r["problems"], json!([]), "{client}");
        assert_eq!(r["command"], cos_bin(), "{client}");
        assert_eq!(r["version_ok"], true, "{client}");
        assert_eq!(r["version_stale"], false, "{client}");
    }

    let desktop: Value = serde_json::from_str(&fs::read_to_string(h.desktop()).unwrap()).unwrap();
    assert_eq!(
        desktop["mcpServers"]["cos"],
        json!({
            "command": cos_bin(),
            "args": ["mcp"],
            "env": {
                "PATH": h.tool_path(),
                "COS_BUSINESSES": h.home.join(".config/cos/businesses.toml"),
            },
        })
    );

    let toml = fs::read_to_string(h.codex_toml()).unwrap();
    let doc: toml_edit::DocumentMut = toml.parse().unwrap();
    assert_eq!(
        doc["mcp_servers"]["cos"]["command"].as_str(),
        Some(cos_bin())
    );
    assert_eq!(
        doc["mcp_servers"]["cos"]["tool_timeout_sec"].as_integer(),
        Some(320)
    );
}

#[test]
fn codex_timeout_below_320_is_red_and_named() {
    let h = home("timeout");
    assert!(connect(&h, &["codex", "--bin", cos_bin()]).status.success());
    let text = fs::read_to_string(h.codex_toml()).unwrap();
    fs::write(
        h.codex_toml(),
        text.replace("tool_timeout_sec = 320", "tool_timeout_sec = 60"),
    )
    .unwrap();

    let rows = status_rows(&h);
    let r = row(&rows, "codex");
    assert_eq!(r["ok"], false);
    assert_eq!(r["problems"], json!(["timeout"]));
    assert_eq!(r["timeout_sec"], 60);
}

#[test]
fn malformed_json_and_toml_untouched_and_named() {
    let h = home("malformed");
    let bad_json = "{ \"mcpServers\": ";
    fs::create_dir_all(h.desktop().parent().unwrap()).unwrap();
    fs::write(h.desktop(), bad_json).unwrap();
    let out = connect(&h, &["claude-desktop", "--bin", cos_bin()]);
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains(&h.desktop().display().to_string()),
        "{}",
        stderr(&out)
    );
    assert!(stderr(&out).contains("not valid JSON"), "{}", stderr(&out));
    assert_eq!(fs::read_to_string(h.desktop()).unwrap(), bad_json);

    let bad_toml = "[mcp_servers\ncos = \"x\"\n";
    fs::create_dir_all(&h.codex).unwrap();
    fs::write(h.codex_toml(), bad_toml).unwrap();
    let out = connect(&h, &["codex", "--bin", cos_bin()]);
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains(&h.codex_toml().display().to_string()),
        "{}",
        stderr(&out)
    );
    assert!(stderr(&out).contains("not valid TOML"), "{}", stderr(&out));
    assert_eq!(fs::read_to_string(h.codex_toml()).unwrap(), bad_toml);
}

#[test]
fn malformed_toml_error_names_the_place_not_the_content() {
    let h = home("malformed-secret");
    let secret = "sk-FICTIONAL-SECRET-1234";
    let bad = format!("[mcp_servers.other.env]\nAPI_KEY = \"{secret}\n");
    fs::create_dir_all(&h.codex).unwrap();
    fs::write(h.codex_toml(), &bad).unwrap();

    let out = connect(&h, &["codex", "--bin", cos_bin()]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("not valid TOML"), "{err}");
    assert!(err.contains("line 2"), "{err}");
    assert!(!err.contains(secret), "{err}");
    assert!(!err.contains("API_KEY"), "{err}");

    let rows = status_rows(&h);
    let e = row(&rows, "codex")["error"].as_str().unwrap().to_owned();
    assert!(e.contains("line 2"), "{e}");
    assert!(!e.contains(secret), "{e}");
    assert!(!e.contains("API_KEY"), "{e}");
    assert_eq!(fs::read_to_string(h.codex_toml()).unwrap(), bad);
}

#[test]
fn symlinked_config_stays_a_link_and_its_target_is_written() {
    let h = home("symlink");
    let dotfiles = h._dir.0.join("dotfiles");
    fs::create_dir_all(&dotfiles).unwrap();
    let target = dotfiles.join("codex.toml");
    let original = "[mcp_servers.other]\ncommand = \"/bin/sh\"\n";
    fs::write(&target, original).unwrap();
    fs::create_dir_all(&h.codex).unwrap();
    std::os::unix::fs::symlink(&target, h.codex_toml()).unwrap();

    let out = connect(&h, &["codex", "--bin", cos_bin()]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(
        fs::symlink_metadata(h.codex_toml())
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(fs::read_link(h.codex_toml()).unwrap(), target);
    let text = fs::read_to_string(&target).unwrap();
    assert!(text.contains("[mcp_servers.other]"), "{text}");
    assert!(text.contains(cos_bin()), "{text}");
    assert_eq!(
        fs::read_to_string(dotfiles.join("codex.toml.cos.orig")).unwrap(),
        original
    );
}

#[test]
fn bin_whose_version_exits_nonzero_refused() {
    let h = home("version-fails");
    let bin = h._dir.0.join("broken-cos");
    script(&bin, "echo 'cos 1.0.0'\nexit 1");
    let out = connect(&h, &["codex", "--bin", bin.to_str().unwrap()]);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("--version"), "{}", stderr(&out));
    assert!(!h.codex_toml().exists());
}

#[test]
fn bin_must_be_absolute() {
    let h = home("relative-bin");
    let out = connect(&h, &["claude-desktop", "--bin", "cos"]);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("absolute"), "{}", stderr(&out));
    assert!(!h.desktop().exists());
}

#[test]
fn bin_whose_version_is_not_cos_refused() {
    let h = home("not-cos");
    let out = connect(&h, &["codex", "--bin", "/bin/echo"]);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("--version"), "{}", stderr(&out));
    assert!(!h.codex_toml().exists());
}

#[test]
fn changed_between_read_and_rename_refused() {
    let dir = scratch("race");
    let path = dir.0.join("config.json");
    fs::write(&path, "old").unwrap();
    let err = cos::connect::write_checked(&path, Some(b"old"), b"new", || {
        fs::write(&path, "theirs").unwrap();
    })
    .unwrap_err();
    assert!(
        format!("{err:#}").contains("changed while writing; try again"),
        "{err:#}"
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), "theirs");
    assert!(!dir.0.join("config.json.cos.new").exists());
}

#[test]
fn second_write_keeps_first_cos_orig() {
    let h = home("orig");
    let original =
        "{\n  \"theme\": \"dark\",\n  \"mcpServers\": {\"other\": {\"command\": \"/bin/sh\"}}\n}\n";
    fs::create_dir_all(h.desktop().parent().unwrap()).unwrap();
    fs::write(h.desktop(), original).unwrap();
    fs::set_permissions(h.desktop(), fs::Permissions::from_mode(0o600)).unwrap();

    assert!(
        connect(&h, &["claude-desktop", "--bin", cos_bin()])
            .status
            .success()
    );
    let orig = h
        .desktop()
        .with_file_name("claude_desktop_config.json.cos.orig");
    assert_eq!(fs::read_to_string(&orig).unwrap(), original);
    assert_eq!(
        fs::metadata(&orig).unwrap().permissions().mode() & 0o777,
        0o600
    );

    let other = h._dir.0.join("other-cos");
    script(&other, "echo \"cos 9.9.9\"");
    let other = other.to_str().unwrap();
    let out = connect(&h, &["claude-desktop", "--repair", "--bin", other]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(fs::read_to_string(&orig).unwrap(), original);

    let desktop: Value = serde_json::from_str(&fs::read_to_string(h.desktop()).unwrap()).unwrap();
    assert_eq!(desktop["theme"], "dark");
    assert_eq!(
        desktop["mcpServers"]["other"],
        json!({"command": "/bin/sh"})
    );
    assert_eq!(desktop["mcpServers"]["cos"]["command"], other);
}

#[test]
fn codex_keeps_other_tables_and_comments_as_text() {
    let h = home("codex-text");
    let original = "# my codex config\nmodel = \"gpt\"\n\n[mcp_servers.other]\n# keep me\ncommand = \"/bin/sh\"\n";
    fs::create_dir_all(&h.codex).unwrap();
    fs::write(h.codex_toml(), original).unwrap();

    assert!(connect(&h, &["codex", "--bin", cos_bin()]).status.success());
    let text = fs::read_to_string(h.codex_toml()).unwrap();
    assert!(
        text.contains("# my codex config\nmodel = \"gpt\"\n"),
        "{text}"
    );
    assert!(
        text.contains("[mcp_servers.other]\n# keep me\ncommand = \"/bin/sh\"\n"),
        "{text}"
    );
    assert_eq!(
        fs::read_to_string(h.codex.join("config.toml.cos.orig")).unwrap(),
        original
    );
}

#[test]
fn register_is_idempotent_and_repair_fixes_a_stale_command() {
    let h = home("repair");
    assert!(
        connect(&h, &["claude-desktop", "--bin", cos_bin()])
            .status
            .success()
    );
    let text = fs::read_to_string(h.desktop()).unwrap();
    fs::write(h.desktop(), text.replace(cos_bin(), "/gone/cos")).unwrap();

    let rows = status_rows(&h);
    let r = row(&rows, "claude-desktop");
    assert_eq!(r["problems"], json!(["command", "version"]));

    let out = connect(&h, &["claude-desktop", "--bin", cos_bin()]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(
        stdout(&out).contains("already registered"),
        "{}",
        stdout(&out)
    );
    let rows = status_rows(&h);
    assert_eq!(row(&rows, "claude-desktop")["ok"], false);

    let out = connect(&h, &["claude-desktop", "--repair", "--bin", cos_bin()]);
    assert!(out.status.success(), "{}", stderr(&out));
    let rows = status_rows(&h);
    assert_eq!(row(&rows, "claude-desktop")["ok"], true);
}

#[test]
fn remove_drops_only_the_cos_entry() {
    let h = home("remove");
    let original = "[mcp_servers.other]\ncommand = \"/bin/sh\"\n";
    fs::create_dir_all(&h.codex).unwrap();
    fs::write(h.codex_toml(), original).unwrap();
    assert!(connect(&h, &["codex", "--bin", cos_bin()]).status.success());
    assert!(connect(&h, &["remove", "codex"]).status.success());
    let text = fs::read_to_string(h.codex_toml()).unwrap();
    assert!(
        text.contains("[mcp_servers.other]\ncommand = \"/bin/sh\"\n"),
        "{text}"
    );
    assert!(!text.contains("cos"), "{text}");

    assert!(
        connect(&h, &["claude-desktop", "--bin", cos_bin()])
            .status
            .success()
    );
    assert!(connect(&h, &["remove", "claude-desktop"]).status.success());
    let rows = status_rows(&h);
    assert_eq!(row(&rows, "claude-desktop")["registered"], false);
}

#[test]
fn version_probe_runs_once_per_path_size_and_mtime() {
    let dir = scratch("probe-cache");
    let bin = dir.0.join("cos");
    let runs = dir.0.join("runs");
    script(
        &bin,
        &format!("echo x >> {}\necho \"cos 0.1.0\"", runs.display()),
    );

    let first = cos::connect::probe(&bin, Duration::from_secs(5));
    assert_eq!(
        first.version.as_deref(),
        Some("cos 0.1.0"),
        "{:?}",
        first.error
    );
    assert!(!first.stale);
    let again = cos::connect::probe(&bin, Duration::from_secs(5));
    assert_eq!(again.version.as_deref(), Some("cos 0.1.0"));
    assert_eq!(fs::read_to_string(&runs).unwrap().lines().count(), 1);

    script(
        &bin,
        &format!("echo x >> {}\necho \"cos 0.1.0 changed\"", runs.display()),
    );
    let changed = cos::connect::probe(&bin, Duration::from_secs(5));
    assert_eq!(changed.version.as_deref(), Some("cos 0.1.0 changed"));
    assert_eq!(fs::read_to_string(&runs).unwrap().lines().count(), 2);
}

#[test]
fn version_timeout_reads_last_result_as_stale_and_reaps_the_child() {
    let dir = scratch("probe-stale");
    let bin = dir.0.join("cos");
    script(&bin, "echo \"cos 0.1.0\"");
    let good = cos::connect::probe(&bin, Duration::from_secs(5));
    assert_eq!(good.version.as_deref(), Some("cos 0.1.0"));

    let pid_file = dir.0.join("sleep.pid");
    script(
        &bin,
        &format!("sleep 30 &\necho $! > {}\nwait", pid_file.display()),
    );
    let started = Instant::now();
    let slow = cos::connect::probe(&bin, Duration::from_millis(300));
    assert!(started.elapsed() < Duration::from_secs(10));
    assert_eq!(slow.version.as_deref(), Some("cos 0.1.0"));
    assert!(slow.stale);
    assert!(slow.error.is_some());

    let pid: libc::pid_t = fs::read_to_string(&pid_file)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    // The killed grandchild is reparented to init, which reaps it shortly after.
    let gone = (0..50).any(|_| {
        // SAFETY: signal 0 only checks that the pid exists.
        if unsafe { libc::kill(pid, 0) } != 0 {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
        false
    });
    assert!(gone, "sleep {pid} survived the probe deadline");
}
