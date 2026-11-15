//! `dashboards.toml`: location, parse, and every validation rule.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use cos::business::{Business, Registry};
use cos::dashboards::{self, Dashboards, Invalid};

struct Dir(PathBuf);

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn scratch(name: &str) -> Dir {
    let d = std::env::temp_dir().join(format!("cos-dashboards-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    Dir(d)
}

fn load_text(name: &str, text: &str) -> anyhow::Result<Dashboards> {
    let dir = scratch(name);
    let path = dir.0.join("dashboards.toml");
    std::fs::write(&path, text).unwrap();
    dashboards::load(&path)
}

fn loads(name: &str, text: &str) -> Dashboards {
    load_text(name, text).unwrap_or_else(|e| panic!("{name}: expected to load, got {e:#}"))
}

fn refused(name: &str, text: &str) -> Invalid {
    match load_text(name, text) {
        Ok(d) => panic!("{name}: expected a refusal, loaded {d:?}"),
        Err(e) => match e.downcast::<Invalid>() {
            Ok(invalid) => invalid,
            Err(e) => panic!("{name}: expected Invalid, got {e:#}"),
        },
    }
}

const SERVER: &str = r#"
[server.control-room]
binary = "/opt/control-room/portfolio-control-room"
addr = "127.0.0.1:4777"
ready = { path = "/health", contains = "\"status\":\"ok\"" }
"#;

const CONTROL_ROOM: &str = r#"
[[business.founding-bets]]
name = "Control room"
url = "http://127.0.0.1:4777/"
server = "control-room"
probe = { path = "/", contains = "<title>Wolven portfolio control room" }
"#;

const SALESROOM: &str = r#"
[[business.founding-bets]]
name = "Salesroom"
url = "http://127.0.0.1:4777/sales"
server = "control-room"
probe = { path = "/sales", contains = "<script src=\"/sales.js\"" }
"#;

fn unmanaged(business: &str, url: &str, path: &str, contains: &str) -> String {
    format!(
        "[[business.{business}]]\nname = \"Gazette\"\nurl = {url:?}\nprobe = {{ path = {path:?}, contains = {contains:?} }}\n"
    )
}

fn server(name: &str, binary: &str, addr: &str, ready_path: &str) -> String {
    format!(
        "[server.{name}]\nbinary = {binary:?}\naddr = {addr:?}\nready = {{ path = {ready_path:?}, contains = \"\\\"status\\\":\\\"ok\\\"\" }}\n"
    )
}

const GAZETTE_MARKER: &str = "<h1>Longhand gazette";

#[test]
fn location_is_explicit_else_beside_the_registry() {
    let registry = Path::new("/cfg/cos/businesses.toml");
    assert_eq!(
        dashboards::location(registry, None),
        PathBuf::from("/cfg/cos/dashboards.toml")
    );
    assert_eq!(
        dashboards::location(registry, Some(Path::new("/elsewhere/d.toml"))),
        PathBuf::from("/elsewhere/d.toml")
    );
}

#[test]
fn missing_file_reads_as_empty() {
    let dir = scratch("missing");
    let d = dashboards::load(&dir.0.join("dashboards.toml")).unwrap();
    assert!(d.servers.is_empty());
    assert!(d.businesses.is_empty());
    assert!(d.for_business("founding-bets").is_empty());
}

#[test]
fn server_table_parses() {
    let d = loads("server", &format!("{SERVER}{CONTROL_ROOM}{SALESROOM}"));
    let s = &d.servers["control-room"];
    assert_eq!(
        s.binary,
        PathBuf::from("/opt/control-room/portfolio-control-room")
    );
    assert_eq!(s.addr.to_string(), "127.0.0.1:4777");
    assert_eq!(s.ready.path, "/health");
    assert_eq!(s.ready.contains, "\"status\":\"ok\"");

    let list = d.for_business("founding-bets");
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].name, "Control room");
    assert_eq!(list[0].url, "http://127.0.0.1:4777/");
    assert_eq!(list[0].port, 4777);
    assert_eq!(list[0].server.as_deref(), Some("control-room"));
    assert_eq!(list[0].probe.path, "/");
    assert_eq!(
        list[0].probe.contains,
        "<title>Wolven portfolio control room"
    );
    assert_eq!(list[1].name, "Salesroom");
    assert_eq!(list[1].probe.path, "/sales");
    assert_eq!(list[1].probe.contains, "<script src=\"/sales.js\"");
    assert_eq!(d.server_of(&list[1]), Some(s));
}

#[test]
fn only_unmanaged_dashboards_parse_with_no_servers() {
    let d = loads(
        "unmanaged-only",
        &unmanaged(
            "longhand",
            "http://127.0.0.1:8790/#staff",
            "/",
            GAZETTE_MARKER,
        ),
    );
    assert!(d.servers.is_empty());
    let list = d.for_business("longhand");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].server, None);
    assert_eq!(list[0].port, 8790);
    assert_eq!(d.server_of(&list[0]), None);
}

#[test]
fn only_servers_parse() {
    let d = loads("servers-only", SERVER);
    assert_eq!(d.servers.len(), 1);
    assert!(d.businesses.is_empty());
}

#[test]
fn orphan_keys_are_reported() {
    let text = format!(
        "{SERVER}{CONTROL_ROOM}{}",
        unmanaged("longhand", "http://127.0.0.1:8790/", "/", GAZETTE_MARKER)
    );
    let d = loads("orphans", &text);
    let dir = Path::new("/tmp/x");
    let mut businesses = BTreeMap::new();
    businesses.insert(
        "founding-bets".to_owned(),
        Business {
            home: dir.into(),
            workspace: dir.into(),
            projects: dir.into(),
        },
    );
    let registry = Registry {
        active: None,
        businesses,
    };
    assert_eq!(d.orphans(&registry), vec!["longhand".to_owned()]);
    assert_eq!(
        d.orphans(&Registry::default()),
        vec!["founding-bets".to_owned(), "longhand".to_owned()]
    );
}

#[test]
fn non_loopback_url_refused() {
    for url in [
        "https://127.0.0.1:4777/",
        "http://localhost:8790/",
        "http://0.0.0.0:8790/",
        "http://example.com:8790/",
        "http://127.0.0.1:8790@example.com/",
        "http://127.0.0.1/",
        "http://127.0.0.1:8790",
    ] {
        let e = refused(
            "non-loopback-url",
            &unmanaged("longhand", url, "/", GAZETTE_MARKER),
        );
        assert_eq!(e.key, "business.longhand[0].url", "{url}: {e}");
    }
}

#[test]
fn unknown_server_refused() {
    let e = refused(
        "unknown-server",
        &format!(
            "{SERVER}{}",
            CONTROL_ROOM.replace("server = \"control-room\"", "server = \"nope\"")
        ),
    );
    assert_eq!(e.key, "business.founding-bets[0].server");
    assert!(e.rule.contains("nope"), "{e}");
}

#[test]
fn port_mismatch_refused() {
    let e = refused(
        "port-mismatch",
        &format!("{SERVER}{}", CONTROL_ROOM.replace("4777/", "4778/")),
    );
    assert_eq!(e.key, "business.founding-bets[0].url");
    assert!(e.rule.contains("4777"), "{e}");
}

#[test]
fn relative_binary_refused() {
    let e = refused(
        "relative-binary",
        &server(
            "cr",
            "target/release/portfolio-control-room",
            "127.0.0.1:4777",
            "/health",
        ),
    );
    assert_eq!(e.key, "server.cr.binary");
}

#[test]
fn unknown_key_refused() {
    let cases = [
        (
            format!("{SERVER}args = [\"--x\"]\n"),
            "server.control-room.args",
        ),
        (
            format!("{CONTROL_ROOM}colour = \"red\"\n"),
            "business.founding-bets[0].colour",
        ),
        (
            unmanaged("longhand", "http://127.0.0.1:8790/", "/", GAZETTE_MARKER)
                .replace("\" }", "\", method = \"POST\" }"),
            "business.longhand[0].probe.method",
        ),
        (
            SERVER.replace("\" }", "\", timeout = 3 }"),
            "server.control-room.ready.timeout",
        ),
        ("extra = 1\n".to_owned(), "extra"),
    ];
    for (text, key) in cases {
        let e = refused("unknown-key", &text);
        assert_eq!(e.key, key, "{e}");
    }
}

#[test]
fn duplicate_dashboard_name_refused() {
    let e = refused(
        "duplicate-name",
        &format!(
            "{SERVER}{CONTROL_ROOM}{}",
            SALESROOM.replace("Salesroom", "Control room")
        ),
    );
    assert_eq!(e.key, "business.founding-bets[1].name");
    assert!(e.rule.contains("business.founding-bets[0].name"), "{e}");
}

#[test]
fn same_name_in_two_businesses_parses() {
    let text = format!(
        "{}{}",
        unmanaged("longhand", "http://127.0.0.1:8790/", "/", GAZETTE_MARKER),
        unmanaged("realestate", "http://127.0.0.1:8791/", "/", GAZETTE_MARKER)
    );
    loads("same-name-two-businesses", &text);
}

#[test]
fn invalid_business_key_refused() {
    let e = refused(
        "business-key",
        &unmanaged(
            "Founding_Bets",
            "http://127.0.0.1:8790/",
            "/",
            GAZETTE_MARKER,
        ),
    );
    assert_eq!(e.key, "business.Founding_Bets");
}

#[test]
fn short_and_single_tag_markers_refused() {
    for marker in [
        "status ok",
        "<title>",
        "<div class=x>",
        "<measured in Build>",
        "  <measured in Build>\n",
    ] {
        let e = refused(
            "probe-marker",
            &unmanaged("longhand", "http://127.0.0.1:8790/", "/", marker),
        );
        assert_eq!(
            e.key, "business.longhand[0].probe.contains",
            "{marker:?}: {e}"
        );
    }
    let e = refused(
        "ready-marker",
        &SERVER.replace("\"\\\"status\\\":\\\"ok\\\"\"", "\"<title>\""),
    );
    assert_eq!(e.key, "server.control-room.ready.contains", "{e}");
}

#[test]
fn control_room_markers_admitted() {
    let d = loads("markers", &format!("{SERVER}{CONTROL_ROOM}{SALESROOM}"));
    assert_eq!(d.for_business("founding-bets").len(), 2);
}

#[test]
fn duplicate_addr_port_refused_naming_both_keys() {
    let text = format!(
        "{}{}",
        server("a", "/bin/a", "127.0.0.1:4777", "/health"),
        server("b", "/bin/b", "127.0.0.1:4777", "/health")
    );
    let e = refused("duplicate-addr", &text);
    let msg = e.to_string();
    assert!(msg.contains("server.a.addr"), "{msg}");
    assert!(msg.contains("server.b.addr"), "{msg}");
}

#[test]
fn unmanaged_dashboard_on_a_declared_server_port_refused() {
    let text = format!(
        "{SERVER}{}",
        unmanaged("longhand", "http://127.0.0.1:4777/", "/", GAZETTE_MARKER)
    );
    let e = refused("unmanaged-on-server-port", &text);
    assert_eq!(e.key, "business.longhand[0].url");
    assert!(e.rule.contains("control-room"), "{e}");
}

#[test]
fn probe_and_ready_paths_refused_unless_safe() {
    for path in ["/\r\nHost: evil", "/a b", "sales", "", "/caf\u{e9}"] {
        let e = refused(
            "probe-path",
            &unmanaged("longhand", "http://127.0.0.1:8790/", path, GAZETTE_MARKER),
        );
        assert_eq!(e.key, "business.longhand[0].probe.path", "{path:?}: {e}");
        let e = refused(
            "ready-path",
            &server("cr", "/bin/cr", "127.0.0.1:4777", path),
        );
        assert_eq!(e.key, "server.cr.ready.path", "{path:?}: {e}");
    }
}

#[test]
fn probe_and_ready_paths_admitted() {
    for path in ["/", "/sales", "/health"] {
        loads(
            "probe-path-ok",
            &unmanaged("longhand", "http://127.0.0.1:8790/", path, GAZETTE_MARKER),
        );
        loads(
            "ready-path-ok",
            &server("cr", "/bin/cr", "127.0.0.1:4777", path),
        );
    }
}

#[test]
fn port_zero_refused() {
    let e = refused(
        "addr-port-0",
        &server("cr", "/bin/cr", "127.0.0.1:0", "/health"),
    );
    assert_eq!(e.key, "server.cr.addr");
    let e = refused(
        "url-port-0",
        &unmanaged("longhand", "http://127.0.0.1:0/", "/", GAZETTE_MARKER),
    );
    assert_eq!(e.key, "business.longhand[0].url");
    let e = refused(
        "url-port-big",
        &unmanaged("longhand", "http://127.0.0.1:65536/", "/", GAZETTE_MARKER),
    );
    assert_eq!(e.key, "business.longhand[0].url");
}

#[test]
fn non_loopback_addr_refused() {
    for addr in [
        "0.0.0.0:4777",
        "localhost:4777",
        "127.0.0.1",
        "127.0.0.1:+4777",
    ] {
        let e = refused("addr", &server("cr", "/bin/cr", addr, "/health"));
        assert_eq!(e.key, "server.cr.addr", "{addr}: {e}");
    }
}

#[test]
fn missing_required_key_refused() {
    let server_cases = [
        ("binary", "server.cr.binary"),
        ("addr", "server.cr.addr"),
        ("ready", "server.cr.ready"),
    ];
    let full = server("cr", "/bin/cr", "127.0.0.1:4777", "/health");
    for (drop, key) in server_cases {
        let text: String = full
            .lines()
            .filter(|l| !l.starts_with(drop))
            .map(|l| format!("{l}\n"))
            .collect();
        let e = refused("missing-server-key", &text);
        assert_eq!(e.key, key, "{e}");
        assert!(e.rule.contains("missing"), "{e}");
    }

    let full = unmanaged("longhand", "http://127.0.0.1:8790/", "/", GAZETTE_MARKER);
    for (drop, key) in [
        ("name", "business.longhand[0].name"),
        ("url", "business.longhand[0].url"),
        ("probe", "business.longhand[0].probe"),
    ] {
        let text: String = full
            .lines()
            .filter(|l| !l.starts_with(drop))
            .map(|l| format!("{l}\n"))
            .collect();
        let e = refused("missing-dashboard-key", &text);
        assert_eq!(e.key, key, "{e}");
        assert!(e.rule.contains("missing"), "{e}");
    }

    for (inline, key) in [
        ("{ path = \"/\" }", "business.longhand[0].probe.contains"),
        (
            &format!("{{ contains = {GAZETTE_MARKER:?} }}")[..],
            "business.longhand[0].probe.path",
        ),
    ] {
        let text = format!(
            "[[business.longhand]]\nname = \"Gazette\"\nurl = \"http://127.0.0.1:8790/\"\nprobe = {inline}\n"
        );
        let e = refused("missing-probe-key", &text);
        assert_eq!(e.key, key, "{e}");
    }
}

#[test]
fn wrong_types_refused_with_key_path() {
    let e = refused(
        "binary-type",
        &SERVER.replace(
            "binary = \"/opt/control-room/portfolio-control-room\"",
            "binary = 5",
        ),
    );
    assert_eq!(e.key, "server.control-room.binary");
    let e = refused("business-type", "[business.longhand]\nname = \"Gazette\"\n");
    assert_eq!(e.key, "business.longhand");
}

#[test]
fn syntax_error_is_an_error() {
    assert!(load_text("syntax", "[server.cr\n").is_err());
}
