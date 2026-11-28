//! `cos skills` end to end: the real binary against a stand-in Prime and a
//! skill home in a temp directory (design §5, ISA S2).

#[allow(dead_code)]
#[path = "common/prime.rs"]
mod stand_in;

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use serde_json::{Value, json};

const COS: &str = env!("CARGO_BIN_EXE_cos");

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("cos-skills-cli-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d.canonicalize().unwrap()
}

/// A home, a user home holding three claude skills, and a stand-in Prime
/// with rust-perf (builder, requires tdd) and seo-audit written by another
/// tool (no source).
fn machine(name: &str) -> (PathBuf, stand_in::Prime) {
    let root = scratch(name);
    for (s, d) in [
        ("rust-perf", "Profile hot paths"),
        ("tdd", "Test first"),
        ("seo-audit", "SEO"),
    ] {
        let dir = root.join("user/.claude/skills").join(s);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("SKILL.md"),
            format!("---\ndescription: {d}\n---\n"),
        )
        .unwrap();
    }
    let p = stand_in::Prime::start();
    p.reply(
        "prime_search",
        json!({"nodes": [
            {"id": "a", "type": "cos_skill", "properties": {"name": "rust-perf", "verdict": "relevant",
              "roles": ["builder"], "requires": ["tdd"], "curated_at": "2026-11-28T10:00:00Z", "source": "cos-skills"}},
            {"id": "b", "type": "cos_skill", "properties": {"name": "seo-audit", "verdict": "irrelevant",
              "curated_at": "2026-11-28T10:00:00Z"}}
        ]}),
    );
    (root, p)
}

fn cos(root: &Path, p: &stand_in::Prime, args: &[&str]) -> Output {
    Command::new(COS)
        .arg("--home")
        .arg(root.join("home"))
        .args(args)
        .env("COS_SKILLS_HOME", root.join("user"))
        .env("COS_PRIME_URL", p.url())
        .env_remove("COS_RUN_ID")
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

fn text(o: &Output) -> (String, String) {
    (
        String::from_utf8_lossy(&o.stdout).into_owned(),
        String::from_utf8_lossy(&o.stderr).into_owned(),
    )
}

#[test]
fn ls_lists_every_skill_with_its_verdict_and_marks_facts_written_outside_cos() {
    let (root, p) = machine("ls");
    let o = cos(&root, &p, &["skills", "ls"]);
    let (out, err) = text(&o);
    assert!(o.status.success(), "{err}");
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 3, "{out}");
    assert!(
        lines[0].starts_with("rust-perf")
            && lines[0].contains("relevant")
            && lines[0].contains("roles: builder")
            && lines[0].contains("requires: tdd"),
        "{out}"
    );
    assert!(
        lines[1].starts_with("seo-audit") && lines[1].contains("(written outside cos)"),
        "{out}"
    );
    assert!(
        lines[2].starts_with("tdd") && lines[2].contains("unreviewed"),
        "{out}"
    );

    let o = cos(&root, &p, &["skills", "ls", "--json", "--unreviewed"]);
    let rows: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 1);
    assert_eq!(rows[0]["name"], "tdd");
    assert_eq!(rows[0]["description"], "Test first");
}

#[test]
fn show_prints_the_skill_set_a_role_would_get() {
    let (root, p) = machine("show");
    let o = cos(
        &root,
        &p,
        &["skills", "show", "--role", "builder", "--no-skill", "tdd"],
    );
    let (out, err) = text(&o);
    assert!(o.status.success(), "{err}");
    let set: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(set["picked"][0]["name"], "rust-perf");
    assert_eq!(set["picked"].as_array().unwrap().len(), 1);
    assert!(
        set["warnings"]
            .to_string()
            .contains("rust-perf requires tdd"),
        "{set}"
    );

    let o = cos(&root, &p, &["skills", "show", "--role", "chief-of-staff"]);
    assert!(!o.status.success());
}

#[test]
fn set_writes_one_fact_with_its_edges() {
    let (root, p) = machine("set");
    let o = cos(
        &root,
        &p,
        &[
            "skills",
            "set",
            "tdd",
            "--relevant",
            "--role",
            "builder",
            "--requires",
            "rust-perf",
            "--note",
            "always",
        ],
    );
    let (_, err) = text(&o);
    assert!(o.status.success(), "{err}");
    let tools: Vec<String> = p.calls().into_iter().map(|(t, _)| t).collect();
    assert_eq!(
        tools,
        ["prime_search", "prime_add_node", "prime_add_edge"],
        "the write reuses the fresh read"
    );
    let node = &p.calls()[1].1;
    assert_eq!(node["properties"]["name"], "tdd");
    assert_eq!(node["properties"]["source"], "cos-skills");
    assert_eq!(node["properties"]["note"], "always");
    assert_eq!(p.calls()[2].1["target"], "node:cos_skill:a");
}

#[test]
fn set_refuses_what_verify_w_refuses_and_writes_nothing() {
    let (root, p) = machine("set-refused");
    let long = "x".repeat(501);
    let refused: [&[&str]; 5] = [
        &["skills", "set", "../x", "--relevant"],
        &[
            "skills",
            "set",
            "rust-perf",
            "--relevant",
            "--role",
            "nobody",
        ],
        &[
            "skills",
            "set",
            "rust-perf",
            "--relevant",
            "--requires",
            "rust-perf",
        ],
        &["skills", "set", "rust-perf", "--relevant", "--note", &long],
        &["skills", "set", "rust-perf"],
    ];
    for args in refused {
        let o = cos(&root, &p, args);
        assert!(!o.status.success(), "{args:?}");
    }
    let mut c = Command::new(COS);
    c.arg("--home")
        .arg(root.join("home"))
        .args(["skills", "set", "tdd", "--irrelevant"])
        .env("COS_SKILLS_HOME", root.join("user"))
        .env("COS_PRIME_URL", p.url())
        .env("COS_RUN_ID", "20261128-abc")
        .stdin(Stdio::null());
    let o = c.output().unwrap();
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("a hire cannot"));
    assert!(
        !p.calls().iter().any(|(t, _)| t == "prime_add_node"),
        "{:?}",
        p.calls()
    );
}

#[test]
fn set_with_prime_down_exits_one_naming_the_url() {
    let (root, _p) = machine("set-down");
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let url = format!("http://127.0.0.1:{port}/mcp");
    let o = Command::new(COS)
        .arg("--home")
        .arg(root.join("home"))
        .args(["skills", "set", "tdd", "--irrelevant"])
        .env("COS_SKILLS_HOME", root.join("user"))
        .env("COS_PRIME_URL", &url)
        .env_remove("COS_RUN_ID")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains(&format!("127.0.0.1:{port}")));
}

#[test]
fn curate_needs_a_terminal_and_names_set_instead() {
    let (root, p) = machine("curate");
    let o = cos(&root, &p, &["skills", "curate"]);
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("cos skills set"));
    assert!(!p.calls().iter().any(|(t, _)| t == "prime_add_node"));
}

#[test]
fn a_dry_run_set_writes_nothing() {
    let (root, p) = machine("dry");
    let o = cos(
        &root,
        &p,
        &["--dry-run", "skills", "set", "tdd", "--irrelevant"],
    );
    assert!(o.status.success(), "{}", text(&o).1);
    assert_eq!(p.connections(), 0);
}
