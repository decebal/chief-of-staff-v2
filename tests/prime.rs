//! The Prime client against a stand-in server: loopback only, one deadline,
//! a size cap, and every way a reply can be wrong (design §1, ISA S1).

#[allow(dead_code)]
#[path = "common/prime.rs"]
mod stand_in;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use cos::agents::policy;
use cos::agents::skill_facts::{self, Decision, Mode as Read};
use cos::agents::skills::{self, GraphState, Root, Verdict};
use cos::prime::Prime;
use serde_json::{Value, json};
use stand_in::Mode;

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("cos-prime-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d.canonicalize().unwrap()
}

fn node(id: &str, name: &str, verdict: &str, roles: &[&str], requires: &[&str], at: &str) -> Value {
    json!({"id": id, "type": "cos_skill", "properties": {
        "name": name, "verdict": verdict, "roles": roles, "requires": requires,
        "curated_at": at, "domain": "cos-skills", "source": "cos-skills"}})
}

fn catalog(base: &Path) -> skills::Catalog {
    for s in ["rust-perf", "tdd", "seo-audit"] {
        let d = base.join("claude").join(s);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("SKILL.md"), "---\ndescription: d\n---\n").unwrap();
    }
    skills::catalog(&[Root {
        label: "claude".into(),
        dir: base.join("claude"),
    }])
}

fn write_cache(home: &Path, v: Value) {
    let p = skill_facts::cache_path(home);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, v.to_string()).unwrap();
}

const OLD: &str = "2026-01-01T00:00:00Z";

#[test]
fn only_literal_loopback_addresses_and_localhost_are_accepted_before_any_socket() {
    let s = stand_in::Prime::start();
    for refused in [
        "http://10.0.0.1:3906/mcp",
        "http://example.com/mcp",
        "https://127.0.0.1:3906/mcp",
        "http://127.0.0.1.evil.test/mcp",
        "ftp://127.0.0.1/mcp",
        "127.0.0.1:3906/mcp",
        "http://127.0.0.1:3906/mcp HTTP/1.1\r\nX: y",
        "http://127.0.0.1:3906/m cp",
    ] {
        let e = Prime::new(refused, Duration::from_secs(1))
            .unwrap_err()
            .to_string();
        assert!(
            e.contains("loopback") || e.contains("http://") || e.contains("control character"),
            "{refused}: {e}"
        );
    }
    assert_eq!(s.connections(), 0);

    let port = s.addr.port();
    for ok in [
        format!("http://127.0.0.1:{port}/mcp"),
        format!("http://[::1]:{port}/mcp"),
        format!("http://localhost:{port}/mcp"),
    ] {
        let p = Prime::new(&ok, Duration::from_secs(2)).unwrap();
        if ok.contains("[::1]") {
            // The stand-in listens on IPv4 only; parsing is what this proves.
            continue;
        }
        p.call("prime_stats", json!({})).unwrap();
    }
    assert_eq!(s.connections(), 2);
}

#[test]
fn a_call_returns_the_tools_json_and_sends_a_tools_call() {
    let s = stand_in::Prime::start();
    s.reply(
        "prime_search",
        json!({"nodes": [{"id": "n1", "type": "cos_skill", "properties": {"name": "tdd"}}]}),
    );
    let p = Prime::new(&s.url(), Duration::from_secs(2)).unwrap();
    let v = p
        .call("prime_search", json!({"type": "cos_skill"}))
        .unwrap();
    assert_eq!(v["nodes"][0]["properties"]["name"], "tdd");
    let r = &s.requests()[0];
    assert_eq!(r["jsonrpc"], "2.0");
    assert_eq!(r["method"], "tools/call");
    assert_eq!(r["params"]["name"], "prime_search");
    assert_eq!(r["params"]["arguments"]["type"], "cos_skill");
}

#[test]
fn one_deadline_bounds_a_stalled_server() {
    let s = stand_in::Prime::start();
    s.mode(Mode::Stall(Duration::from_secs(5)));
    let p = Prime::new(&s.url(), Duration::from_millis(300)).unwrap();
    let t = Instant::now();
    assert!(p.call("prime_search", json!({})).is_err());
    assert!(t.elapsed() < Duration::from_secs(2), "{:?}", t.elapsed());
}

#[test]
fn a_refused_connection_is_an_error_not_a_hang() {
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let p = Prime::new(
        &format!("http://127.0.0.1:{port}/mcp"),
        Duration::from_secs(1),
    )
    .unwrap();
    assert!(p.call("prime_stats", json!({})).is_err());
}

#[test]
fn every_malformed_reply_is_an_error_naming_what_was_wrong() {
    let cases = [
        (Mode::Status(503), "503"),
        (Mode::NoLength, "Content-Length"),
        (Mode::Huge, "MiB"),
        (Mode::RpcError, "unknown tool"),
        (Mode::Raw("not json".into()), "JSON"),
    ];
    for (mode, expect) in cases {
        let s = stand_in::Prime::start();
        s.mode(mode.clone());
        let p = Prime::new(&s.url(), Duration::from_secs(2)).unwrap();
        let e = format!("{:#}", p.call("prime_stats", json!({})).unwrap_err());
        assert!(e.contains(expect), "{mode:?}: {e}");
    }
}

#[test]
fn a_read_folds_the_newest_valid_fact_per_skill_and_caches_it() {
    let home = scratch("fold");
    let s = stand_in::Prime::start();
    s.reply("prime_search", json!({"nodes": [
        node("a1", "rust-perf", "irrelevant", &[], &[], "2026-11-01T00:00:00Z"),
        node("a2", "rust-perf", "relevant", &["builder", "nobody"], &["tdd"], "2026-11-02T00:00:00Z"),
        node("b1", "tdd", "relevant", &[], &[], "2026-11-01T00:00:00Z"),
        node("b2", "tdd", "irrelevant", &[], &["../evil"], "2026-11-03T00:00:00Z"),
        node("c1", "Bad Name", "relevant", &[], &[], "2026-11-01T00:00:00Z"),
    ]}));
    let p = Prime::new(&s.url(), Duration::from_secs(2)).unwrap();
    let g = skill_facts::read_graph(&home, Ok(&p), &policy::seed(), Read::Live);
    assert_eq!(g.state, GraphState::Fresh);
    let rp = &g.facts["rust-perf"];
    assert_eq!(rp.verdict, Verdict::Relevant);
    assert_eq!(rp.roles, ["builder"], "an unknown role is dropped");
    assert_eq!(rp.node.as_deref(), Some("a2"));
    assert_eq!(
        g.facts["tdd"].verdict,
        Verdict::Relevant,
        "the invalid newer tdd does not shadow the older"
    );
    assert!(!g.facts.contains_key("Bad Name"));
    for expect in ["nobody", "\"tdd\"", "Bad Name"] {
        assert!(
            g.warnings.iter().any(|w| w.contains(expect)),
            "{expect}: {:?}",
            g.warnings
        );
    }
    let cached = skill_facts::load_cache(&home).unwrap();
    assert!(cached.read_at.is_some() && cached.failed_at.is_none());
    assert_eq!(cached.facts.len(), 2);
}

#[test]
fn prime_is_not_asked_again_within_thirty_seconds_of_a_success_or_a_failure() {
    let home = scratch("reuse");
    let s = stand_in::Prime::start();
    s.reply(
        "prime_search",
        json!({"nodes": [node("a", "tdd", "relevant", &[], &[], OLD)]}),
    );
    let p = Prime::new(&s.url(), Duration::from_secs(2)).unwrap();
    let pol = policy::seed();
    skill_facts::read_graph(&home, Ok(&p), &pol, Read::Live);
    let again = skill_facts::read_graph(&home, Ok(&p), &pol, Read::Live);
    assert_eq!(again.state, GraphState::Fresh);
    assert_eq!(s.connections(), 1);

    let home = scratch("reuse-fail");
    let s = stand_in::Prime::start();
    s.mode(Mode::Status(503));
    let p = Prime::new(&s.url(), Duration::from_secs(2)).unwrap();
    let first = skill_facts::read_graph(&home, Ok(&p), &pol, Read::Live);
    assert_eq!(first.state, GraphState::None);
    assert!(
        first.warnings.iter().any(|w| w.contains("503")),
        "{:?}",
        first.warnings
    );
    let second = skill_facts::read_graph(&home, Ok(&p), &pol, Read::Live);
    assert_eq!(second.state, GraphState::None);
    assert_eq!(s.connections(), 1, "a failure is remembered too");
}

#[test]
fn a_failed_read_keeps_the_last_graph_and_marks_it_stale() {
    let home = scratch("stale");
    write_cache(
        &home,
        json!({"facts": {"tdd": {"name": "tdd", "verdict": "relevant",
        "curated_at": OLD}}, "read_at": OLD}),
    );
    let s = stand_in::Prime::start();
    s.mode(Mode::Raw("garbage".into()));
    let p = Prime::new(&s.url(), Duration::from_secs(2)).unwrap();
    let g = skill_facts::read_graph(&home, Ok(&p), &policy::seed(), Read::Live);
    assert_eq!(
        g.state,
        GraphState::Stale {
            read_at: Some(OLD.into())
        }
    );
    assert!(g.facts.contains_key("tdd"));
    let c = skill_facts::load_cache(&home).unwrap();
    assert_eq!(c.read_at.as_deref(), Some(OLD), "the previous graph stays");
    assert!(c.failed_at.is_some());
}

#[test]
fn ten_reads_at_once_open_one_connection_and_the_rest_use_the_file() {
    let home = scratch("single-flight");
    write_cache(&home, json!({"facts": {}, "read_at": OLD}));
    let s = stand_in::Prime::start();
    s.mode(Mode::Stall(Duration::from_millis(800)));
    let p = Prime::new(&s.url(), Duration::from_secs(3)).unwrap();
    let pol = policy::seed();
    let t = Instant::now();
    let states: Vec<(GraphState, Duration)> = std::thread::scope(|sc| {
        let hs: Vec<_> = (0..10)
            .map(|_| {
                sc.spawn(|| {
                    let started = Instant::now();
                    let g = skill_facts::read_graph(&home, Ok(&p), &pol, Read::Live);
                    (g.state, started.elapsed())
                })
            })
            .collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    assert_eq!(s.connections(), 1);
    let quick = states
        .iter()
        .filter(|(st, d)| matches!(st, GraphState::Stale { .. }) && *d < Duration::from_millis(400))
        .count();
    assert!(quick >= 9, "{states:?} in {:?}", t.elapsed());
}

#[test]
fn a_dry_read_neither_asks_prime_nor_writes_the_file() {
    let home = scratch("dry");
    let s = stand_in::Prime::start();
    let p = Prime::new(&s.url(), Duration::from_secs(2)).unwrap();
    let g = skill_facts::read_graph(&home, Ok(&p), &policy::seed(), Read::Dry);
    assert_eq!(g.state, GraphState::None);
    assert_eq!(s.connections(), 0);
    assert!(!skill_facts::cache_path(&home).exists());
}

#[test]
fn a_refused_prime_url_reads_the_cache_and_says_why() {
    let home = scratch("refused");
    let g = skill_facts::read_graph(
        &home,
        Err("10.0.0.1 is not a loopback address"),
        &policy::seed(),
        Read::Live,
    );
    assert_eq!(g.state, GraphState::None);
    assert!(
        g.warnings.iter().any(|w| w.contains("loopback")),
        "{:?}",
        g.warnings
    );
}

#[test]
fn a_write_is_one_node_with_the_closed_property_set_then_one_edge_per_requires() {
    let base = scratch("write");
    let home = base.join("home");
    let c = catalog(&base);
    let s = stand_in::Prime::start();
    s.reply(
        "prime_search",
        json!({"nodes": [node("t1", "tdd", "relevant", &[], &[], OLD)]}),
    );
    let p = Prime::new(&s.url(), Duration::from_secs(2)).unwrap();
    let d = Decision {
        name: "rust-perf".into(),
        verdict: Some(Verdict::Relevant),
        roles: vec!["builder".into()],
        requires: vec!["tdd".into()],
        note: Some("profiling".into()),
    };
    let warnings = skill_facts::write_fact(&home, &p, &c, &policy::seed(), &d).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    let calls = s.calls();
    let tools: Vec<&str> = calls.iter().map(|(t, _)| t.as_str()).collect();
    assert_eq!(tools, ["prime_search", "prime_add_node", "prime_add_edge"]);
    let props = &calls[1].1["properties"];
    let mut keys: Vec<&str> = props
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort();
    assert_eq!(
        keys,
        [
            "curated_at",
            "domain",
            "name",
            "note",
            "requires",
            "roles",
            "source",
            "verdict"
        ]
    );
    assert_eq!(props["source"], "cos-skills");
    assert_eq!(calls[1].1["type"], "cos_skill");
    let edge = &calls[2].1;
    assert_eq!(edge["relation"], "requires");
    assert_eq!(edge["target"], "node:cos_skill:t1");
    assert!(
        edge["source"]
            .as_str()
            .unwrap()
            .starts_with("node:cos_skill:")
    );
}

#[test]
fn a_write_failing_verify_w_reaches_nothing() {
    let base = scratch("verify-w");
    let home = base.join("home");
    let c = catalog(&base);
    let s = stand_in::Prime::start();
    let p = Prime::new(&s.url(), Duration::from_secs(2)).unwrap();
    let pol = policy::seed();
    let ok = Decision {
        name: "rust-perf".into(),
        verdict: Some(Verdict::Relevant),
        ..Decision::default()
    };
    let bad = [
        Decision {
            name: "../x".into(),
            ..ok.clone()
        },
        Decision {
            name: "ghost".into(),
            ..ok.clone()
        },
        Decision {
            roles: vec!["nobody".into()],
            ..ok.clone()
        },
        Decision {
            roles: vec!["chief-of-staff".into()],
            ..ok.clone()
        },
        Decision {
            requires: vec!["rust-perf".into()],
            ..ok.clone()
        },
        Decision {
            requires: vec!["ghost".into()],
            ..ok.clone()
        },
        Decision {
            note: Some("x".repeat(501)),
            ..ok.clone()
        },
        Decision {
            verdict: None,
            ..ok.clone()
        },
    ];
    for d in &bad {
        assert!(
            skill_facts::write_fact(&home, &p, &c, &pol, d).is_err(),
            "{d:?}"
        );
    }
    assert_eq!(s.connections(), 0);
}

#[test]
fn a_write_with_prime_down_fails_and_names_the_url() {
    let base = scratch("write-down");
    let home = base.join("home");
    let c = catalog(&base);
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let p = Prime::new(
        &format!("http://127.0.0.1:{port}/mcp"),
        Duration::from_secs(1),
    )
    .unwrap();
    let d = Decision {
        name: "seo-audit".into(),
        verdict: Some(Verdict::Irrelevant),
        ..Decision::default()
    };
    let e = format!(
        "{:#}",
        skill_facts::write_fact(&home, &p, &c, &policy::seed(), &d).unwrap_err()
    );
    assert!(e.contains(&format!("127.0.0.1:{port}")), "{e}");
}

#[test]
fn a_node_id_that_is_not_an_id_never_reaches_an_edge() {
    let mut warnings = Vec::new();
    let facts = skill_facts::fold(
        &json!({"nodes": [
            node("ok-1234", "tdd", "relevant", &[], &[], OLD),
            node("x\",\"relation\":\"owns", "rust-perf", "relevant", &[], &[], OLD),
        ]}),
        &mut warnings,
    );
    assert_eq!(facts["tdd"].node.as_deref(), Some("ok-1234"));
    assert_eq!(
        facts["rust-perf"].node, None,
        "the fact stays, its id does not"
    );
    assert!(skill_facts::node_id_ok(
        "00000000-0000-0000-0000-000000000001"
    ));
    assert!(!skill_facts::node_id_ok(""));
    assert!(!skill_facts::node_id_ok(&"a".repeat(65)));
}

#[test]
fn a_reply_without_a_nodes_list_keeps_the_last_graph() {
    let home = scratch("no-nodes");
    write_cache(
        &home,
        json!({"facts": {"tdd": {"name": "tdd", "verdict": "relevant",
        "curated_at": OLD}}, "read_at": OLD}),
    );
    let s = stand_in::Prime::start();
    s.reply("prime_search", json!({"unexpected": true}));
    let p = Prime::new(&s.url(), Duration::from_secs(2)).unwrap();
    let g = skill_facts::read_graph(&home, Ok(&p), &policy::seed(), Read::Live);
    assert_eq!(
        g.state,
        GraphState::Stale {
            read_at: Some(OLD.into())
        }
    );
    assert!(g.facts.contains_key("tdd"), "the previous graph stays");
    assert!(
        g.warnings.iter().any(|w| w.contains("no nodes list")),
        "{:?}",
        g.warnings
    );
    let c = skill_facts::load_cache(&home).unwrap();
    assert_eq!(c.read_at.as_deref(), Some(OLD));
    assert!(c.failed_at.is_some());
}

#[test]
fn two_writers_at_once_leave_a_cache_that_parses() {
    let home = scratch("two-writers");
    std::thread::scope(|sc| {
        for i in 0..16 {
            let home = &home;
            sc.spawn(move || {
                let mut facts = std::collections::BTreeMap::new();
                for n in 0..50 {
                    let name = format!("skill-{i}-{n}");
                    facts.insert(
                        name.clone(),
                        serde_json::from_value(
                            json!({"name": name, "verdict": "relevant", "curated_at": OLD}),
                        )
                        .unwrap(),
                    );
                }
                let c = skill_facts::Cache {
                    facts,
                    read_at: Some(OLD.into()),
                    failed_at: None,
                };
                skill_facts::store_cache(home, &c).unwrap();
            });
        }
    });
    let c = skill_facts::load_cache(&home).expect("a whole, parseable graph.json");
    assert_eq!(c.facts.len(), 50, "one writer's whole graph, not a mix");
    let leftovers = std::fs::read_dir(skill_facts::cache_path(&home).parent().unwrap())
        .unwrap()
        .filter(|e| {
            e.as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".tmp")
        })
        .count();
    assert_eq!(leftovers, 0);
}
