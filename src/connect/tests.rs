use serde_json::json;

use super::*;

#[test]
fn json_set_keeps_key_order_and_other_servers() {
    let mut v = parse_json(
        br#"{"theme":"dark","mcpServers":{"a":{"command":"/bin/a"},"z":{"command":"/bin/z"}},"last":1}"#,
    )
    .unwrap();
    json_set(&mut v, "/opt/cos", "/usr/bin", None).unwrap();
    let keys: Vec<&String> = v.as_object().unwrap().keys().collect();
    assert_eq!(keys, ["theme", "mcpServers", "last"]);
    let servers: Vec<&String> = v["mcpServers"].as_object().unwrap().keys().collect();
    assert_eq!(servers, ["a", "z", "cos"]);
    assert_eq!(v["mcpServers"]["a"], json!({"command": "/bin/a"}));
    assert_eq!(
        json_entry(&v),
        Some(Entry {
            command: Some("/opt/cos".into()),
            timeout_sec: None
        })
    );
}

#[test]
fn json_set_keeps_other_env_on_the_cos_entry() {
    let mut v =
        parse_json(br#"{"mcpServers":{"cos":{"command":"/old","env":{"KEEP":"1"}}}}"#).unwrap();
    json_set(&mut v, "/opt/cos", "/usr/bin", None).unwrap();
    assert_eq!(
        v["mcpServers"]["cos"]["env"],
        json!({"KEEP": "1", "PATH": "/usr/bin"})
    );
    assert!(json_remove(&mut v));
    assert!(!json_remove(&mut v));
    assert_eq!(v, json!({"mcpServers": {}}));
}

#[test]
fn empty_json_file_reads_as_an_empty_object_and_an_array_is_refused() {
    assert_eq!(parse_json(b"  \n").unwrap(), json!({}));
    assert!(parse_json(b"[]").is_err());
}

#[test]
fn toml_set_keeps_comments_and_other_tables_as_text() {
    let before = "# top\nmodel = \"gpt\"\n\n[mcp_servers.other]\n# keep me\ncommand = \"/bin/sh\"\n\n[profiles.x]\nmodel = \"y\"\n";
    let mut d = parse_toml(before.as_bytes()).unwrap();
    toml_set(&mut d, "/opt/cos", "/usr/bin", None).unwrap();
    let after = d.to_string();
    assert!(
        after.starts_with(
            "# top\nmodel = \"gpt\"\n\n[mcp_servers.other]\n# keep me\ncommand = \"/bin/sh\"\n"
        ),
        "{after}"
    );
    assert!(after.ends_with("[profiles.x]\nmodel = \"y\"\n"), "{after}");
    assert_eq!(
        toml_entry(&d),
        Some(Entry {
            command: Some("/opt/cos".into()),
            timeout_sec: Some(CODEX_TIMEOUT_SEC)
        })
    );
    assert!(toml_remove(&mut d));
    assert_eq!(d.to_string(), before);
}

#[test]
fn toml_repair_keeps_the_cos_table_comment_and_its_env() {
    let before = "[mcp_servers.cos]\n# mine\ncommand = \"/old\"\ntool_timeout_sec = 60\n\n[mcp_servers.cos.env]\nKEEP = \"1\"\n";
    let mut d = parse_toml(before.as_bytes()).unwrap();
    toml_set(&mut d, "/opt/cos", "/usr/bin", None).unwrap();
    let after = d.to_string();
    assert!(after.contains("# mine\ncommand = \"/opt/cos\""), "{after}");
    assert!(after.contains("tool_timeout_sec = 320"), "{after}");
    assert!(after.contains("KEEP = \"1\""), "{after}");
    assert!(after.contains("PATH = \"/usr/bin\""), "{after}");
}

#[test]
fn toml_repair_keeps_a_longer_timeout_and_reads_a_float_one() {
    let mut d =
        parse_toml(b"[mcp_servers.cos]\ncommand = \"/old\"\ntool_timeout_sec = 600.0\n").unwrap();
    assert_eq!(toml_entry(&d).unwrap().timeout_sec, Some(600));
    toml_set(&mut d, "/opt/cos", "/usr/bin", None).unwrap();
    assert!(d.to_string().contains("tool_timeout_sec = 600.0"), "{d}");

    let mut d =
        parse_toml(b"[mcp_servers.cos]\ncommand = \"/old\"\ntool_timeout_sec = 900\n").unwrap();
    toml_set(&mut d, "/opt/cos", "/usr/bin", None).unwrap();
    assert!(d.to_string().contains("tool_timeout_sec = 900"), "{d}");
}

#[test]
fn the_entry_pins_the_registry_the_daemon_reads() {
    let mut v = parse_json(b"{}").unwrap();
    json_set(
        &mut v,
        "/opt/cos",
        "/usr/bin",
        Some("/cfg/cos/businesses.toml"),
    )
    .unwrap();
    assert_eq!(
        v["mcpServers"]["cos"]["env"]["COS_BUSINESSES"],
        json!("/cfg/cos/businesses.toml")
    );
    let mut d = parse_toml(b"").unwrap();
    toml_set(
        &mut d,
        "/opt/cos",
        "/usr/bin",
        Some("/cfg/cos/businesses.toml"),
    )
    .unwrap();
    assert!(
        d.to_string()
            .contains("COS_BUSINESSES = \"/cfg/cos/businesses.toml\""),
        "{d}"
    );
}

#[test]
fn cos_version_line() {
    assert!(is_cos_version("cos 0.1.0"));
    assert!(is_cos_version("cos 1.20.3-rc.1"));
    assert!(!is_cos_version("cos 0.1"));
    assert!(!is_cos_version("--version"));
    assert!(!is_cos_version("sh 0.1.0"));
}
