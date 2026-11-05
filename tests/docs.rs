//! The documentation room against real git checkouts in a temp dir: the
//! inventory, the Notion match, the user's organisation, and the routes both
//! `cos dash` and the desktop app serve.

use std::path::{Path, PathBuf};
use std::process::Command;

use cos::docs::{self, Ask, Link, Mark, NotionList, NotionPage, Organize};

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("cos-docs-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d.canonicalize().unwrap()
}

fn git(dir: &Path, args: &[&str]) {
    let st = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .status()
        .unwrap();
    assert!(st.success(), "git {args:?}");
}

fn write(dir: &Path, rel: &str, text: &str) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
}

/// `projects/<dir>` with origin `acme/<slug>` and a few tracked documents.
fn repo(projects: &Path, dir: &str, slug: &str) -> PathBuf {
    let r = projects.join(dir);
    std::fs::create_dir_all(&r).unwrap();
    git(&r, &["init", "-q", "-b", "main"]);
    git(
        &r,
        &[
            "remote",
            "add",
            "origin",
            &format!("git@github.com:acme/{slug}.git"),
        ],
    );
    write(
        &r,
        "README.md",
        "# Demo platform\n\nSee [the design](docs/concepts/design.md).\n",
    );
    write(
        &r,
        "docs/concepts/design.md",
        "# Gateway auth design\n\nBack to [readme](../../README.md) and [ops](../guides/ops.md#run).\n",
    );
    write(&r, "docs/guides/ops.md", "No heading here.\n");
    write(&r, ".github/pull_request_template.md", "# Template\n");
    write(&r, "vendor/lib/README.md", "# Vendored\n");
    git(&r, &["add", "-A"]);
    git(&r, &["commit", "-q", "-m", "docs"]);
    r
}

/// A home and a workspace whose profile monitors `acme/demo`.
fn setup(name: &str) -> (PathBuf, PathBuf, PathBuf) {
    let root = scratch(name);
    let (home, ws, projects) = (root.join("home"), root.join("ws"), root.join("projects"));
    std::fs::create_dir_all(ws.join(".chief-of-staff")).unwrap();
    std::fs::write(
        ws.join(".chief-of-staff/profile.json"),
        r#"{ "monitoredRepos": ["acme/demo", "acme/absent"] }"#,
    )
    .unwrap();
    repo(&projects, "demo", "demo");
    (home, ws, projects)
}

fn get(home: &Path, ws: &Path, projects: &Path, path: &str, query: Option<&str>) -> docs::Reply {
    docs::handle(
        home,
        ws,
        projects,
        &Ask {
            method: "GET",
            path,
            query,
            cos_header: None,
            body: b"",
        },
    )
    .expect("a docs route")
}

#[test]
fn the_inventory_has_titles_folders_and_resolved_links_and_skips_tooling() {
    let (home, ws, projects) = setup("scan");
    let lib = docs::library(&home, &ws, &projects).unwrap();
    let ids: Vec<&str> = lib["docs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids,
        [
            "demo:README.md",
            "demo:docs/concepts/design.md",
            "demo:docs/guides/ops.md"
        ]
    );
    let design = &lib["docs"][1];
    assert_eq!(design["title"], "Gateway auth design");
    assert_eq!(design["folder"], "demo · Concepts");
    assert_eq!(design["suggest"], "move_to_notion");
    assert_eq!(
        design["links"],
        serde_json::json!(["demo:README.md", "demo:docs/guides/ops.md"])
    );
    assert_eq!(
        lib["docs"][2]["title"], "ops",
        "no heading falls back to the file name"
    );
    assert_eq!(lib["docs"][0]["folder"], "demo · Overview");
    assert_eq!(lib["docs"][0]["suggest"], "keep_in_repo");
    assert_eq!(lib["missing"], serde_json::json!(["acme/absent"]));
}

#[test]
fn of_two_checkouts_of_one_repo_the_one_used_last_wins() {
    let root = scratch("checkouts");
    let projects = root.join("projects");
    let older = repo(&projects, "a-review", "demo");
    std::thread::sleep(std::time::Duration::from_millis(1100));
    let newer = repo(&projects, "b-main", "demo");
    let found = docs::checkouts(&projects, &["acme/demo".to_owned()]);
    assert_eq!(found, [("demo".to_owned(), newer.clone())]);
    git(&older, &["status", "--short"]);
    std::fs::write(older.join("touch.txt"), "x").unwrap();
    git(&older, &["add", "touch.txt"]);
    assert_eq!(
        docs::checkouts(&projects, &["acme/demo".to_owned()])[0].1,
        older
    );
}

#[test]
fn a_repo_document_with_a_notion_page_of_the_same_title_is_matched() {
    let (home, ws, projects) = setup("notion");
    docs::save_notion(
        &home,
        &NotionList {
            pages: vec![NotionPage {
                title: "Gateway Auth Design".into(),
                url: "https://www.notion.so/acme/Gateway-Design-1".into(),
                parent: "Platform".into(),
                project: "demo".into(),
            }],
            listed_at: "2030-04-09 09:00".into(),
        },
    )
    .unwrap();
    let lib = docs::library(&home, &ws, &projects).unwrap();
    assert_eq!(
        lib["in_notion"]["demo:docs/concepts/design.md"],
        "notion:https://www.notion.so/acme/Gateway-Design-1"
    );
    let page = lib["docs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["place"] == "notion")
        .unwrap();
    assert_eq!(page["folder"], "demo · Notion");
}

#[test]
fn a_save_needs_the_header_and_round_trips_and_a_broken_file_is_never_overwritten() {
    let (home, ws, projects) = setup("organize");
    assert_eq!(docs::load_organize(&home).unwrap(), Organize::default());
    let mut org = Organize::default();
    org.topics
        .insert("VM SDK".into(), vec!["demo:README.md".into()]);
    org.links.push(Link {
        from: "demo:README.md".into(),
        to: "demo:docs/guides/ops.md".into(),
        kind: "depends on".into(),
    });
    org.marks
        .insert("demo:docs/guides/ops.md".into(), Mark::MoveToNotion);
    let body = serde_json::to_vec(&org).unwrap();
    let post = |header: Option<&str>| {
        docs::handle(
            &home,
            &ws,
            &projects,
            &Ask {
                method: "POST",
                path: "/docs/organize",
                query: None,
                cos_header: header,
                body: &body,
            },
        )
        .unwrap()
    };
    assert_eq!(post(None).status, 403, "a cross-site form cannot save");
    assert_eq!(post(Some("docs")).status, 200);
    assert_eq!(docs::load_organize(&home).unwrap(), org);

    std::fs::write(docs::organize_path(&home), "{ \"topics\": 3 }").unwrap();
    assert!(docs::load_organize(&home).is_err());
    let r = get(&home, &ws, &projects, "/docs", None);
    assert_eq!(r.status, 500, "the room refuses to show over a broken file");
    let bad = docs::handle(
        &home,
        &ws,
        &projects,
        &Ask {
            method: "POST",
            path: "/docs/organize",
            query: None,
            cos_header: Some("docs"),
            body: b"{\"tpoics\":{}}",
        },
    )
    .unwrap();
    assert_eq!(bad.status, 400, "an unknown field is refused, not dropped");
}

#[test]
fn reading_stays_inside_the_monitored_checkouts() {
    let (home, ws, projects) = setup("read");
    let ok = get(
        &home,
        &ws,
        &projects,
        "/docs/read",
        Some("id=demo%3Adocs%2Fguides%2Fops.md"),
    );
    assert_eq!(
        (ok.status, String::from_utf8(ok.body).unwrap()),
        (200, "No heading here.\n".to_owned())
    );
    for id in [
        "demo:../outside.md",
        "demo:docs/../../x.md",
        "demo:README",
        "absent:README.md",
        "nonsense",
    ] {
        let r = get(
            &home,
            &ws,
            &projects,
            "/docs/read",
            Some(&format!("id={id}")),
        );
        assert_eq!(r.status, 404, "{id}");
    }
}

#[test]
fn the_page_inlines_its_data_without_letting_a_title_close_the_script() {
    let (home, ws, projects) = setup("render");
    let r = projects.join("demo");
    write(&r, "docs/x.md", "# </script><script>alert(1)</script>\n");
    write(&r, "docs/y.md", "# <!--<script>\n");
    git(&r, &["add", "-A"]);
    git(&r, &["commit", "-q", "-m", "x"]);
    let page = String::from_utf8(get(&home, &ws, &projects, "/docs", None).body).unwrap();
    assert!(!page.contains("__DOCS_DATA__") && !page.contains(docs::MARKDOWN_SLOT));
    assert!(
        !page.contains("Markdy"),
        "the room draws its own graph; no diagram bundle is inlined"
    );
    for title in ["</script><script>alert(1)</script>", "<!--<script>"] {
        assert!(page.contains(&title.replace('<', "\\u003c")), "{title}");
    }
    assert_eq!(
        page.matches("</script>").count(),
        1,
        "only the page's one script tag closes"
    );
    let script = &page[page.find("<script>").unwrap() + 8..page.find("</script>").unwrap()];
    for bad in ["<!--", "<script", "sourceMappingURL"] {
        assert!(
            !script.to_ascii_lowercase().contains(bad),
            "{bad} inside the script"
        );
    }
    assert!(
        docs::handle(
            &home,
            &ws,
            &projects,
            &Ask {
                method: "GET",
                path: "/elsewhere",
                query: None,
                cos_header: None,
                body: b""
            }
        )
        .is_none()
    );
}

#[test]
fn a_notion_match_needs_three_real_words_mostly_shared() {
    let page = |t: &str, id: &str| {
        (
            t.to_lowercase()
                .split(|c: char| !c.is_alphanumeric())
                .filter(|w| {
                    w.len() >= 3 && !["the", "and", "for", "readme", "docs", "guide"].contains(w)
                })
                .map(str::to_owned)
                .collect::<std::collections::BTreeSet<_>>(),
            id.to_owned(),
        )
    };
    let pages = [
        page("Verified Meetings (VM) Architecture", "notion:a"),
        page("Getting Started", "notion:b"),
        page("Gateway auth design", "notion:c"),
    ];
    assert_eq!(
        docs::best_page("Verified Meetings Architecture", &pages),
        Some("notion:a")
    );
    assert_eq!(
        docs::best_page("Getting Started", &pages),
        None,
        "too generic to be the same page"
    );
    assert_eq!(
        docs::best_page("Gateway auth design and rollout plan", &pages),
        None,
        "too little shared"
    );
}

#[test]
fn a_path_says_what_kind_of_document_it_is() {
    for (path, kind) in [
        ("README.md", "Overview"),
        ("docs/README.md", "Overview"),
        ("docs/concepts/adr/005-image.md", "ADRs"),
        ("docs/adr/0001-x.md", "ADRs"),
        ("apps/plugin/README.md", "Code docs"),
        ("docs/plans/merge.md", "Plans"),
        ("docs/specs/staging-clone.md", "Product & specs"),
        ("docs/guides/setup.md", "Guides"),
        ("docs/operations/deploy.md", "Operations"),
        ("docs/reference/api.md", "Reference"),
        ("docs/concepts/c4-model.md", "Concepts"),
        ("docs/vm-sdk/publishing.md", "vm-sdk"),
        ("notes/scratch.md", "Other"),
    ] {
        assert_eq!(docs::kind(path), kind, "{path}");
    }
}

#[test]
fn a_goal_that_claims_a_repo_or_a_path_groups_its_documents_as_the_bet() {
    let (home, ws, projects) = setup("claims");
    let r = projects.join("demo");
    write(&r, "apps/plugin/README.md", "# Plugin\n");
    git(&r, &["add", "-A"]);
    git(&r, &["commit", "-q", "-m", "plugin"]);
    let profile = std::fs::read(ws.join(".chief-of-staff/profile.json")).unwrap();
    std::fs::remove_dir_all(ws.join(".chief-of-staff")).unwrap();
    cos::state::initialize(&ws).unwrap();
    std::fs::write(ws.join(".chief-of-staff/profile.json"), profile).unwrap();
    let st = serde_json::json!({
        "schemaVersion": 1, "revision": 0,
        "goals": [
            { "id": "G-1", "title": "Platform: ship it", "status": "active", "finishLine": "shipped", "docs": ["demo"] },
            { "id": "G-2", "title": "Plugin: in production", "status": "active", "finishLine": "live", "docs": ["demo:apps/plugin"] }
        ],
        "kpis": [], "deliverables": [], "commitments": [], "sources": [], "runs": [], "incidents": [], "journal": []
    });
    cos::state::commit(&ws, &st, 0, "Fictional goals").unwrap();
    let lib = docs::library(&home, &ws, &projects).unwrap();
    let folder = |id: &str| {
        lib["docs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["id"] == id)
            .unwrap()["folder"]
            .clone()
    };
    assert_eq!(
        folder("demo:docs/concepts/design.md"),
        "Platform · Concepts"
    );
    assert_eq!(
        folder("demo:apps/plugin/README.md"),
        "Plugin · Code docs",
        "the longest claim wins"
    );
    assert_eq!(lib["groups"], serde_json::json!(["Platform", "Plugin"]));
}

#[test]
fn images_are_served_from_the_checkout_and_nothing_else_is() {
    let (home, ws, projects) = setup("asset");
    let r = projects.join("demo");
    let png = b"\x89PNG\r\n\x1a\nfake";
    write(&r, "docs/img/a.png", "");
    std::fs::write(r.join("docs/img/a.png"), png).unwrap();
    write(&r, "docs/img/b.svg", "<svg onload=\"alert(1)\"/>");
    let outside = projects.join("secret.png");
    std::fs::write(&outside, png).unwrap();
    std::os::unix::fs::symlink(&outside, r.join("docs/img/link.png")).unwrap();
    std::os::unix::fs::symlink(projects.join("secret.md"), r.join("docs/link.md")).unwrap();
    std::fs::write(projects.join("secret.md"), "secret\n").unwrap();

    let ok = get(
        &home,
        &ws,
        &projects,
        "/docs/asset",
        Some("id=demo%3Adocs%2Fimg%2Fa.png"),
    );
    assert_eq!(
        (ok.status, ok.kind, ok.body.as_slice()),
        (200, "image/png", &png[..])
    );
    assert_eq!(ok.headers, [("X-Content-Type-Options", "nosniff")]);
    for id in [
        "demo:docs/img/link.png",
        "demo:docs/img/../../README.md",
        "demo:./docs/img/a.png",
        "demo:docs/img/missing.png",
        "absent:docs/img/a.png",
    ] {
        let r = get(
            &home,
            &ws,
            &projects,
            "/docs/asset",
            Some(&format!("id={id}")),
        );
        assert_eq!(r.status, 404, "{id}");
    }
    let r = get(
        &home,
        &ws,
        &projects,
        "/docs/read",
        Some("id=demo:docs/link.md"),
    );
    assert_eq!(r.status, 404, "a symlink out of the checkout is not read");
}

const SVG_CSP: &str = "sandbox; default-src 'none'; style-src 'unsafe-inline'";

/// `demo:docs/img/b.svg` (a script inside it) and a symlinked SVG leaving the checkout.
fn svg_setup(name: &str) -> (PathBuf, PathBuf, PathBuf) {
    let (home, ws, projects) = setup(name);
    let r = projects.join("demo");
    write(
        &r,
        "docs/img/b.svg",
        "<svg xmlns=\"http://www.w3.org/2000/svg\"><script>alert(1)</script></svg>",
    );
    std::fs::write(projects.join("secret.svg"), "<svg/>").unwrap();
    std::os::unix::fs::symlink(projects.join("secret.svg"), r.join("docs/img/out.svg")).unwrap();
    (home, ws, projects)
}

#[test]
fn an_svg_is_served_sandboxed_and_only_from_the_checkout() {
    let (home, ws, projects) = svg_setup("svg");
    let ok = get(
        &home,
        &ws,
        &projects,
        "/docs/asset",
        Some("id=demo%3Adocs%2Fimg%2Fb.svg"),
    );
    assert_eq!((ok.status, ok.kind), (200, "image/svg+xml"));
    assert!(ok.body.starts_with(b"<svg"));
    assert_eq!(
        ok.headers,
        [
            ("Content-Security-Policy", SVG_CSP),
            ("X-Content-Type-Options", "nosniff")
        ]
    );
    for id in ["demo:docs/img/out.svg", "demo:docs/../../secret.svg"] {
        let r = get(
            &home,
            &ws,
            &projects,
            "/docs/asset",
            Some(&format!("id={id}")),
        );
        assert_eq!(r.status, 404, "{id}");
    }
}

#[test]
fn cos_dash_sends_the_svg_headers() {
    let (home, ws, projects) = svg_setup("svg-dash");
    let r = get(
        &home,
        &ws,
        &projects,
        "/docs/asset",
        Some("id=demo:docs/img/b.svg"),
    );
    let res = cos::dash::docs_http(r);
    let h = res.headers();
    assert_eq!(res.status(), 200);
    assert_eq!(h["content-type"], "image/svg+xml");
    assert_eq!(h["content-security-policy"], SVG_CSP);
    assert_eq!(h["x-content-type-options"], "nosniff");
    assert_eq!(h["cache-control"], "no-store");
}

#[test]
fn an_origin_on_github_in_any_spelling_gives_the_repo_home() {
    for url in [
        "git@github.com:acme/demo.git",
        "git@github.com:acme/demo",
        "https://github.com/acme/demo.git",
        "https://github.com/acme/demo",
        "https://github.com/acme/demo/",
        "ssh://git@github.com/acme/demo.git",
        "https://ci-bot@github.com/acme/demo.git",
    ] {
        assert_eq!(
            docs::github_base(url).as_deref(),
            Some("https://github.com/acme/demo"),
            "{url}"
        );
    }
    for url in [
        "git@gitlab.com:acme/demo.git",
        "https://github.com.evil.example/acme/demo",
        "https://github.com/acme",
        "https://github.com/acme/demo/extra",
        "https://github.com/acme/<x>",
        "/srv/git/demo.git",
    ] {
        assert_eq!(docs::github_base(url), None, "{url}");
    }
    let gh = docs::GitHub {
        base: "https://github.com/acme/demo".into(),
        branch: "main".into(),
    };
    assert_eq!(
        docs::github_url(&gh, "docs/a b.md"),
        "https://github.com/acme/demo/blob/main/docs/a%20b.md"
    );
    assert_eq!(
        docs::github_url(&gh, "src/"),
        "https://github.com/acme/demo/tree/main/src"
    );
}

#[test]
fn the_room_knows_each_checkouts_github_branch_and_its_links_outside_the_room() {
    let (home, ws, projects) = setup("github");
    let r = projects.join("demo");
    write(&r, "Taskfile.yml", "version: '3'\n");
    write(&r, "src/main.rs", "fn main() {}\n");
    write(
        &r,
        "docs/guides/run.md",
        "# Run\n\n[tasks](../../Taskfile.yml), [code](../../src/), [design](../concepts/design.md), \
         [gone](../../missing.txt), [up](../../../x.txt), [web](https://example.com/a.txt).\n",
    );
    git(&r, &["add", "-A"]);
    git(&r, &["commit", "-q", "-m", "run"]);
    let lib = docs::library(&home, &ws, &projects).unwrap();
    assert_eq!(
        lib["repos"][0]["github"],
        serde_json::json!({ "base": "https://github.com/acme/demo", "branch": "HEAD" }),
        "no upstream and no origin/HEAD: GitHub's default branch"
    );
    let run = lib["docs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["id"] == "demo:docs/guides/run.md")
        .unwrap();
    assert_eq!(run["outside"], serde_json::json!(["Taskfile.yml", "src/"]));

    // A branch that tracks origin is the one linked.
    git(&r, &["update-ref", "refs/remotes/origin/feat/x", "HEAD"]);
    git(
        &r,
        &["switch", "-q", "-c", "feat/x", "--track", "origin/feat/x"],
    );
    let lib = docs::library(&home, &ws, &projects).unwrap();
    assert_eq!(lib["repos"][0]["github"]["branch"], "feat/x");
    git(&r, &["switch", "-q", "main"]);
    git(&r, &["update-ref", "refs/remotes/origin/trunk", "HEAD"]);
    git(
        &r,
        &[
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/trunk",
        ],
    );
    let lib = docs::library(&home, &ws, &projects).unwrap();
    assert_eq!(
        lib["repos"][0]["github"]["branch"], "trunk",
        "the default branch"
    );

    git(
        &r,
        &[
            "remote",
            "set-url",
            "origin",
            "https://gitlab.com/acme/demo.git",
        ],
    );
    let lib = docs::library(&home, &ws, &projects).unwrap();
    assert_eq!(lib["repos"][0]["github"], serde_json::Value::Null);
    assert!(
        lib["repos"][0]["github_why"]
            .as_str()
            .unwrap()
            .contains("not on GitHub"),
        "{}",
        lib["repos"][0]
    );
}

#[test]
fn the_page_carries_one_renderer_and_one_sanitizer_and_renders_only_through_both() {
    let (home, ws, projects) = setup("renderer");
    let page = String::from_utf8(get(&home, &ws, &projects, "/docs", None).body).unwrap();
    assert_eq!(
        page.matches("marked v18.1.2 - a markdown parser").count(),
        1
    );
    assert_eq!(page.matches("@license DOMPurify 3.4.17").count(), 1);
    let html = include_str!("../src/docs.html");
    assert_eq!(
        html.matches("MD.parse(").count(),
        1,
        "markdown is parsed in one place"
    );
    assert!(
        html.contains("window.DOMPurify.sanitize(MD.parse(src), PURIFY)"),
        "and that place sanitizes what it parsed"
    );
    for forbid in ["\"style\"", "\"iframe\"", "\"form\"", "\"svg\""] {
        assert!(
            html.contains(forbid),
            "the sanitizer config forbids {forbid}"
        );
    }
}

#[test]
fn script_safe_hides_what_would_end_or_derail_a_script_element() {
    let js = "var a = \"</SCRIPT>\", b = /<!--/, c = `<script>`, d = 1 < 2;\n//# sourceMappingURL=x.map\n";
    let out = docs::script_safe(js);
    assert_eq!(
        out,
        "var a = \"\\x3C/SCRIPT>\", b = /\\x3C!--/, c = `\\x3Cscript>`, d = 1 < 2;\n"
    );
    assert!(!docs::markdown_js().to_ascii_lowercase().contains("<!--"));
}
