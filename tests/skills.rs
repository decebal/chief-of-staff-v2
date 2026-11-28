//! The skill catalog, tree digests and resolving a hire's skill set
//! (`docs/plans/2026-11-28-hire-skill-graph-design.md` §2). No Prime here:
//! graphs are built in the test.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use cos::agents::skills::{
    self, Asked, Catalog, Fact, Graph, GraphState, Root, SkillSet, Unreviewed, Verdict, Why,
};

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("cos-skills-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d.canonicalize().unwrap()
}

fn skill(dir: &Path, description: &str) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(
        dir.join("SKILL.md"),
        format!("---\nname: x\ndescription: {description}\n---\n\nBody.\n"),
    )
    .unwrap();
}

/// Three roots: `claude` holds rust-perf, seo-audit, ritual and tdd;
/// `agents` holds a symlinked review and a second tdd; `codex` holds
/// broken (no SKILL.md) and `Bad_Name`.
fn machine(name: &str) -> (PathBuf, Vec<Root>) {
    let base = scratch(name);
    let claude = base.join("claude");
    let agents = base.join("agents");
    let codex = base.join("codex");
    for s in ["rust-perf", "seo-audit", "ritual", "tdd"] {
        skill(&claude.join(s), &format!("{s} skill"));
    }
    skill(&base.join("elsewhere/review"), "review skill");
    std::fs::create_dir_all(&agents).unwrap();
    std::os::unix::fs::symlink(base.join("elsewhere/review"), agents.join("review")).unwrap();
    skill(&agents.join("tdd"), "second tdd");
    std::fs::create_dir_all(codex.join("broken")).unwrap();
    skill(&codex.join("Bad_Name"), "bad");
    let roots = vec![
        Root {
            label: "claude".into(),
            dir: claude,
        },
        Root {
            label: "agents".into(),
            dir: agents,
        },
        Root {
            label: "codex".into(),
            dir: codex,
        },
    ];
    (base, roots)
}

fn fact(name: &str, verdict: Verdict, roles: &[&str], requires: &[&str]) -> Fact {
    Fact {
        name: name.into(),
        verdict,
        roles: roles.iter().map(|s| s.to_string()).collect(),
        requires: requires.iter().map(|s| s.to_string()).collect(),
        note: None,
        curated_at: "2026-11-28T10:00:00Z".into(),
        source: Some("cos-skills".into()),
        node: None,
    }
}

fn graph(facts: Vec<Fact>) -> Graph {
    Graph {
        facts: facts.into_iter().map(|f| (f.name.clone(), f)).collect(),
        state: GraphState::Fresh,
        warnings: Vec::new(),
    }
}

fn names(set: &SkillSet) -> Vec<(String, Why)> {
    set.picked.iter().map(|p| (p.name.clone(), p.why)).collect()
}

fn resolve(g: &Graph, c: &Catalog, role: Option<&str>, asked: Asked) -> SkillSet {
    skills::resolve(g, c, role, &asked, Unreviewed::On, &BTreeSet::new(), &[])
}

/// A `repo` root at `dir` holding `names`, ahead of `roots`.
fn with_repo(dir: &Path, names: &[&str], roots: Vec<Root>) -> Vec<Root> {
    for n in names {
        skill(&dir.join(n), &format!("{n} from the repo"));
    }
    let mut all = vec![Root {
        label: "repo".into(),
        dir: dir.to_path_buf(),
    }];
    all.extend(roots);
    all
}

#[test]
fn the_catalog_reads_every_root_follows_top_symlinks_and_skips_what_is_not_a_skill() {
    let (_b, roots) = machine("catalog");
    let c = skills::catalog(&roots);
    let found: Vec<&str> = c.skills.keys().map(String::as_str).collect();
    assert_eq!(found, ["review", "ritual", "rust-perf", "seo-audit", "tdd"]);
    let review = &c.skills["review"][0];
    assert_eq!(review.root, "agents");
    assert!(review.canonical.ends_with("elsewhere/review"));
    assert_eq!(review.description, "review skill");
    assert_eq!(
        c.skills["tdd"].len(),
        2,
        "both roots' tdd are kept, claude first"
    );
    assert_eq!(c.skills["tdd"][0].root, "claude");
    assert!(
        c.skipped.iter().any(|s| s.contains("broken")),
        "{:?}",
        c.skipped
    );
    assert!(
        c.skipped.iter().any(|s| s.contains("Bad_Name")),
        "{:?}",
        c.skipped
    );
}

#[test]
fn a_missing_root_is_an_empty_catalog_not_an_error() {
    let c = skills::catalog(&[Root {
        label: "claude".into(),
        dir: "/nonexistent/cos".into(),
    }]);
    assert!(c.skills.is_empty());
}

#[test]
fn names_follow_the_skill_name_rule() {
    for ok in ["a", "rust-perf", "v1.2_x", &"a".repeat(64)] {
        assert!(skills::valid_name(ok), "{ok}");
    }
    for bad in [
        "",
        "-a",
        "A",
        "a/b",
        "../x",
        "a b",
        &"a".repeat(65),
        "x\",\"y",
    ] {
        assert!(!skills::valid_name(bad), "{bad}");
    }
}

#[test]
fn the_tree_digest_covers_every_file_and_refuses_inner_symlinks_and_size() {
    let base = scratch("digest");
    let d = base.join("s");
    skill(&d, "one");
    std::fs::create_dir_all(d.join("scripts")).unwrap();
    std::fs::write(d.join("scripts/run.sh"), "echo hi\n").unwrap();
    let first = skills::tree_digest(&d).unwrap();
    assert_eq!(first.len(), 64);
    assert_eq!(first, skills::tree_digest(&d).unwrap(), "stable");
    std::fs::write(d.join("scripts/run.sh"), "echo bye\n").unwrap();
    assert_ne!(
        first,
        skills::tree_digest(&d).unwrap(),
        "a file below SKILL.md counts"
    );

    std::os::unix::fs::symlink("/etc/hosts", d.join("hosts")).unwrap();
    let e = skills::tree_digest(&d).unwrap_err().to_string();
    assert!(e.contains("symlink"), "{e}");
    std::fs::remove_file(d.join("hosts")).unwrap();

    let f = std::fs::File::create(d.join("big.bin")).unwrap();
    f.set_len(skills::TREE_MAX + 1).unwrap();
    let e = skills::tree_digest(&d).unwrap_err().to_string();
    assert!(e.contains("MiB"), "{e}");
}

#[test]
fn a_role_picks_its_relevant_skills_and_their_requires() {
    let (_b, roots) = machine("role");
    let c = skills::catalog(&roots);
    let g = graph(vec![
        fact("rust-perf", Verdict::Relevant, &["builder"], &["tdd"]),
        fact("review", Verdict::Relevant, &["reviewer"], &[]),
        fact("seo-audit", Verdict::Irrelevant, &[], &[]),
    ]);
    let set = resolve(&g, &c, Some("builder"), Asked::default());
    assert_eq!(
        names(&set),
        [
            ("rust-perf".into(), Why::Role),
            ("tdd".into(), Why::Requires)
        ]
    );
    let rp = &set.picked[0];
    assert_eq!(rp.root.as_deref(), Some("claude"));
    assert_eq!(rp.digest.as_ref().map(String::len), Some(64));
    assert!(
        set.warnings
            .iter()
            .any(|w| w.contains("tdd") && w.contains("agents")),
        "the duplicate tdd is named: {:?}",
        set.warnings
    );
}

#[test]
fn asked_skills_add_and_remove_and_a_removed_requirement_stays_out_with_a_warning() {
    let (_b, roots) = machine("asked");
    let c = skills::catalog(&roots);
    let g = graph(vec![fact(
        "rust-perf",
        Verdict::Relevant,
        &["builder"],
        &["tdd"],
    )]);
    let set = resolve(
        &g,
        &c,
        Some("builder"),
        Asked {
            add: vec!["review".into()],
            remove: vec!["tdd".into()],
        },
    );
    assert_eq!(
        names(&set),
        [
            ("rust-perf".into(), Why::Role),
            ("review".into(), Why::Asked)
        ]
    );
    assert!(
        set.warnings
            .iter()
            .any(|w| w.contains("rust-perf requires tdd")),
        "{:?}",
        set.warnings
    );
    assert!(
        set.hidden.contains(&"tdd".to_string()),
        "a removed skill the session would load natively is switched off: {:?}",
        set.hidden
    );
    assert!(
        set.hidden_codex.contains(&"tdd".to_string()),
        "tdd is in the agents root too: {:?}",
        set.hidden_codex
    );
}

#[test]
fn requires_cycles_stop_and_unknown_or_invalid_names_are_dropped_with_a_warning() {
    let (_b, roots) = machine("cycle");
    let c = skills::catalog(&roots);
    let g = graph(vec![
        fact("rust-perf", Verdict::Relevant, &["builder"], &["tdd"]),
        fact("tdd", Verdict::Relevant, &[], &["rust-perf", "ghost"]),
    ]);
    let set = resolve(
        &g,
        &c,
        Some("builder"),
        Asked {
            add: vec!["../evil".into()],
            remove: vec![],
        },
    );
    assert_eq!(
        names(&set),
        [
            ("rust-perf".into(), Why::Role),
            ("tdd".into(), Why::Requires)
        ]
    );
    assert!(
        set.warnings.iter().any(|w| w.contains("ghost")),
        "{:?}",
        set.warnings
    );
    assert!(
        set.warnings.iter().any(|w| w.contains("../evil")),
        "{:?}",
        set.warnings
    );
}

#[test]
fn a_skill_whose_tree_cannot_be_digested_is_not_picked() {
    let (_b, roots) = machine("undigestable");
    std::os::unix::fs::symlink("/etc/hosts", roots[0].dir.join("rust-perf/hosts")).unwrap();
    let c = skills::catalog(&roots);
    let g = graph(vec![fact(
        "rust-perf",
        Verdict::Relevant,
        &["builder"],
        &[],
    )]);
    let set = resolve(&g, &c, Some("builder"), Asked::default());
    assert!(set.picked.is_empty());
    assert!(
        set.warnings
            .iter()
            .any(|w| w.contains("rust-perf") && w.contains("symlink")),
        "{:?}",
        set.warnings
    );
}

#[test]
fn hidden_holds_irrelevant_claude_skills_and_the_claude_originals_of_bundled_ones() {
    let (_b, roots) = machine("hidden");
    let c = skills::catalog(&roots);
    let g = graph(vec![
        fact("rust-perf", Verdict::Relevant, &["builder"], &[]),
        fact("seo-audit", Verdict::Irrelevant, &[], &[]),
        fact("review", Verdict::Irrelevant, &[], &[]),
    ]);
    let set = resolve(&g, &c, Some("builder"), Asked::default());
    assert_eq!(
        set.hidden,
        ["rust-perf", "seo-audit"],
        "review lives in agents, not a claude root"
    );
    assert_eq!(set.hidden_codex, ["review"], "Codex reads the agents root");
    assert_eq!(set.unreviewed, 2, "ritual and tdd have no fact");
    assert!(
        set.warnings.iter().any(|w| w.contains("cos skills curate")),
        "{:?}",
        set.warnings
    );

    let off = skills::resolve(
        &g,
        &c,
        Some("builder"),
        &Asked::default(),
        Unreviewed::Off,
        &BTreeSet::new(),
        &[],
    );
    assert_eq!(off.hidden, ["ritual", "rust-perf", "seo-audit", "tdd"]);
}

#[test]
fn a_project_skill_wins_it_is_neither_bundled_nor_hidden() {
    let (_b, roots) = machine("project");
    let c = skills::catalog(&roots);
    let g = graph(vec![
        fact("ritual", Verdict::Relevant, &["builder"], &[]),
        fact("seo-audit", Verdict::Irrelevant, &[], &[]),
    ]);
    let project: BTreeSet<String> = ["ritual".into(), "seo-audit".into()].into();
    let set = skills::resolve(
        &g,
        &c,
        Some("builder"),
        &Asked::default(),
        Unreviewed::Off,
        &project,
        &[],
    );
    assert_eq!(names(&set), [("ritual".into(), Why::Project)]);
    assert!(set.picked[0].digest.is_none() && set.picked[0].path.is_none());
    assert!(!set.hidden.contains(&"ritual".to_string()));
    assert!(!set.hidden.contains(&"seo-audit".to_string()));
    assert!(
        set.warnings
            .iter()
            .any(|w| w.contains("ritual") && w.contains("repo")),
        "{:?}",
        set.warnings
    );
}

#[test]
fn no_role_and_no_graph_picks_nothing_and_says_the_graph_is_missing() {
    let (_b, roots) = machine("none");
    let c = skills::catalog(&roots);
    let g = Graph {
        facts: BTreeMap::new(),
        state: GraphState::None,
        warnings: Vec::new(),
    };
    let set = resolve(&g, &c, None, Asked::default());
    assert!(set.picked.is_empty());
    assert_eq!(set.graph, GraphState::None);
    assert!(
        set.warnings.iter().any(|w| w.contains("no skill graph")),
        "{:?}",
        set.warnings
    );
}

#[test]
fn the_codex_override_is_toml_built_from_valid_names_only() {
    use cos::agents::skill_bundle::codex_config;
    assert_eq!(codex_config(&[]).unwrap(), None);
    let v = codex_config(&["seo-audit".into(), "x\"],y=[".into()])
        .unwrap()
        .unwrap();
    let parsed: toml::Table = format!("v = {v}").parse().unwrap();
    let ours: Vec<&str> = parsed["v"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["enabled"].as_bool() == Some(false))
        .map(|e| e["name"].as_str().unwrap())
        .collect();
    assert!(ours.contains(&"seo-audit"), "{v}");
    assert!(!v.contains("y=["), "{v}");
}

#[test]
fn the_codex_prompt_block_strips_control_characters_and_caps_each_line() {
    let set = SkillSet {
        picked: vec![cos::agents::skills::Picked {
            name: "rust-perf".into(),
            why: Why::Role,
            root: Some("claude".into()),
            path: Some(PathBuf::from("/skills/rust-perf")),
            digest: Some("0".repeat(64)),
            description: format!("\x1b[2Jwipe {}", "x".repeat(2000)),
        }],
        hidden: Vec::new(),
        hidden_codex: Vec::new(),
        unreviewed: 0,
        graph: GraphState::Fresh,
        warnings: Vec::new(),
        inputs: None,
    };
    let block = set.codex_block().unwrap();
    let line = block.lines().nth(1).unwrap();
    assert!(!block.contains('\x1b'), "{block:?}");
    assert_eq!(line.chars().count(), 300);
    assert!(line.starts_with("- rust-perf: [2Jwipe x"), "{line}");
}

#[test]
fn a_repo_root_copy_wins_over_the_user_copy_and_is_picked_as_repo_role() {
    let (b, roots) = machine("repo-first");
    let all = with_repo(&b.join("export/claude"), &["tdd"], roots);
    let c = skills::catalog(&all);
    assert_eq!(
        c.skills["tdd"][0].root, "repo",
        "the repo root is listed first"
    );
    let g = graph(vec![fact(
        "rust-perf",
        Verdict::Relevant,
        &["builder"],
        &[],
    )]);
    let set = skills::resolve(
        &g,
        &c,
        Some("builder"),
        &Asked::default(),
        Unreviewed::On,
        &BTreeSet::new(),
        &["tdd".into()],
    );
    let tdd = set
        .picked
        .iter()
        .find(|p| p.name == "tdd")
        .expect("tdd picked");
    assert_eq!(tdd.why, Why::RepoRole);
    assert_eq!(tdd.root.as_deref(), Some("repo"));
    assert!(
        tdd.path.as_ref().unwrap().ends_with("export/claude/tdd"),
        "{:?}",
        tdd.path
    );
    assert!(
        set.warnings
            .iter()
            .any(|w| w.contains("tdd") && w.contains("the repo copy is used")),
        "{:?}",
        set.warnings
    );
}

#[test]
fn the_repo_role_reason_is_written_as_repo_role() {
    assert_eq!(
        serde_json::to_string(&Why::RepoRole).unwrap(),
        "\"repo-role\""
    );
}

#[test]
fn names_the_tool_generated_are_not_the_repos_own_project_skills() {
    let base = scratch("generated");
    std::fs::create_dir_all(base.join(".allsource")).unwrap();
    skill(&base.join(".claude/skills/gen"), "generated");
    skill(&base.join(".claude/skills/mine"), "tracked");
    std::fs::write(
        base.join(".allsource/generated.json"),
        r#"{"version":1,"files":[{"path":".claude/skills/gen/SKILL.md","sha256":"00","mode":420}]}"#,
    )
    .unwrap();
    assert_eq!(
        skills::project_skills(Some(&base)),
        BTreeSet::from(["mine".to_string()])
    );
}

#[test]
fn repo_root_skills_are_not_counted_as_unreviewed() {
    let (b, roots) = machine("repo-unreviewed");
    let all = with_repo(&b.join("export/claude"), &["gen", "extra"], roots);
    let c = skills::catalog(&all);
    let g = graph(vec![
        fact("rust-perf", Verdict::Relevant, &["builder"], &[]),
        fact("seo-audit", Verdict::Irrelevant, &[], &[]),
        fact("review", Verdict::Relevant, &["reviewer"], &[]),
    ]);
    let set = skills::resolve(
        &g,
        &c,
        Some("builder"),
        &Asked::default(),
        Unreviewed::On,
        &BTreeSet::new(),
        &["gen".into()],
    );
    assert_eq!(
        set.unreviewed, 2,
        "ritual and tdd; gen and extra are the repo's"
    );
}

#[test]
fn a_role_less_hire_with_no_repo_names_says_nothing_about_repo_skills() {
    let c = skills::catalog(&[]);
    let g = Graph {
        facts: BTreeMap::new(),
        state: GraphState::Fresh,
        warnings: Vec::new(),
    };
    let set = skills::resolve(
        &g,
        &c,
        None,
        &Asked::default(),
        Unreviewed::Off,
        &BTreeSet::new(),
        &[],
    );
    assert!(set.picked.is_empty());
    assert!(set.warnings.is_empty(), "{:?}", set.warnings);
}

#[test]
fn the_codex_index_reads_the_agents_export_for_a_skill_that_targets_agents() {
    let base = scratch("codex-index");
    let claude = base.join("export/claude");
    skill(&claude.join("tdd"), "test first");
    skill(&claude.join("ritual"), "ritual");
    skill(&base.join("export/agents/tdd"), "test first");
    let pick = |name: &str| skills::Picked {
        name: name.into(),
        why: Why::RepoRole,
        root: Some("repo".into()),
        path: Some(claude.join(name)),
        digest: Some("0".repeat(64)),
        description: "d".into(),
    };
    let set = SkillSet {
        picked: vec![pick("tdd"), pick("ritual")],
        hidden: Vec::new(),
        hidden_codex: Vec::new(),
        unreviewed: 0,
        graph: GraphState::Fresh,
        warnings: Vec::new(),
        inputs: None,
    };
    let block = set.codex_block().unwrap();
    let agents_tdd = base.join("export/agents/tdd/SKILL.md");
    assert!(block.contains(&agents_tdd.display().to_string()), "{block}");
    let claude_ritual = claude.join("ritual/SKILL.md");
    assert!(
        block.contains(&claude_ritual.display().to_string()),
        "{block}"
    );
}
