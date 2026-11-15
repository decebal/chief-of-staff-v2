//! `dashboards.toml`: the servers the desktop app may start and the loopback pages each
//! business shows. Nothing here writes the file.
//!
//! Every rule is checked at parse time and the first failure is returned as an [`Invalid`]
//! naming its TOML key path, because a path that reaches the probe is written verbatim into
//! an HTTP request line, and an `addr` names the pidfile, lock and log of a server.

use std::collections::BTreeMap;
use std::fmt;
use std::net::{Ipv4Addr, SocketAddrV4};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use toml::{Table, Value};

use crate::business::{self, Registry};

const FILE: &str = "dashboards.toml";
const MIN_MARKER: usize = 12;
const URL_PREFIX: &str = "http://127.0.0.1:";
const ADDR_PREFIX: &str = "127.0.0.1:";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Probe {
    pub path: String,
    pub contains: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Server {
    pub binary: PathBuf,
    pub addr: SocketAddrV4,
    pub ready: Probe,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dashboard {
    pub name: String,
    pub url: String,
    pub port: u16,
    pub server: Option<String>,
    pub probe: Probe,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Dashboards {
    pub servers: BTreeMap<String, Server>,
    pub businesses: BTreeMap<String, Vec<Dashboard>>,
}

/// A validation failure: the TOML key path and the rule it broke.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invalid {
    pub key: String,
    pub rule: String,
}

impl fmt::Display for Invalid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.key, self.rule)
    }
}

impl std::error::Error for Invalid {}

fn invalid<T>(key: &str, rule: impl Into<String>) -> Result<T, Invalid> {
    Err(Invalid {
        key: key.to_owned(),
        rule: rule.into(),
    })
}

/// `explicit` when given, else `dashboards.toml` beside `registry`.
pub fn location(registry: &Path, explicit: Option<&Path>) -> PathBuf {
    match explicit {
        Some(path) => path.to_path_buf(),
        None => registry.parent().unwrap_or(Path::new("")).join(FILE),
    }
}

/// A missing file is empty.
pub fn load(path: &Path) -> Result<Dashboards> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Dashboards::default()),
        Err(e) => return Err(e).with_context(|| format!("read {}", path.display())),
    };
    let table: Table =
        toml::from_str(&text).with_context(|| format!("parse {}", path.display()))?;
    Ok(validate(&table)?)
}

impl Dashboards {
    pub fn for_business(&self, name: &str) -> &[Dashboard] {
        self.businesses.get(name).map_or(&[], Vec::as_slice)
    }

    pub fn server_of(&self, dashboard: &Dashboard) -> Option<&Server> {
        dashboard
            .server
            .as_deref()
            .and_then(|name| self.servers.get(name))
    }

    /// Business keys the registry does not hold; renaming a business never breaks the file.
    pub fn orphans(&self, registry: &Registry) -> Vec<String> {
        self.businesses
            .keys()
            .filter(|name| !registry.businesses.contains_key(*name))
            .cloned()
            .collect()
    }
}

fn validate(root: &Table) -> Result<Dashboards, Invalid> {
    only_keys(root, "", &["server", "business"])?;
    let mut out = Dashboards::default();

    if let Some(servers) = root.get("server") {
        let servers = table(servers, "server")?;
        let mut ports: BTreeMap<u16, String> = BTreeMap::new();
        for (name, value) in servers {
            let key = format!("server.{name}");
            let server = parse_server(value, &key)?;
            let addr_key = format!("{key}.addr");
            if let Some(other) = ports.insert(server.addr.port(), addr_key.clone()) {
                return invalid(
                    &addr_key,
                    format!("port {} is already used by {other}", server.addr.port()),
                );
            }
            out.servers.insert(name.clone(), server);
        }
    }

    if let Some(businesses) = root.get("business") {
        let businesses = table(businesses, "business")?;
        for (business, value) in businesses {
            let key = format!("business.{business}");
            if !business::valid_name(business) {
                return invalid(
                    &key,
                    "not a business name (a-z, 0-9, '-'; at most 40 characters)",
                );
            }
            let Value::Array(entries) = value else {
                return invalid(&key, "must be an array of tables ([[business.<name>]])");
            };
            let mut names: BTreeMap<String, String> = BTreeMap::new();
            let mut list = Vec::with_capacity(entries.len());
            for (i, entry) in entries.iter().enumerate() {
                let key = format!("{key}[{i}]");
                let dashboard = parse_dashboard(entry, &key, &out.servers)?;
                let name_key = format!("{key}.name");
                if let Some(other) = names.get(dashboard.name.as_str()) {
                    return invalid(
                        &name_key,
                        format!("duplicate name {:?} (also {other})", dashboard.name),
                    );
                }
                names.insert(dashboard.name.clone(), name_key);
                list.push(dashboard);
            }
            out.businesses.insert(business.clone(), list);
        }
    }

    Ok(out)
}

fn parse_server(value: &Value, key: &str) -> Result<Server, Invalid> {
    let t = table(value, key)?;
    only_keys(t, key, &["binary", "addr", "ready"])?;
    let binary_key = format!("{key}.binary");
    let binary = PathBuf::from(string(required(t, key, "binary")?, &binary_key)?);
    if !binary.is_absolute() {
        return invalid(&binary_key, "must be an absolute path");
    }
    let addr_key = format!("{key}.addr");
    let addr = string(required(t, key, "addr")?, &addr_key)?;
    let Some(port) = addr.strip_prefix(ADDR_PREFIX).and_then(port) else {
        return invalid(&addr_key, "must be 127.0.0.1:<port>, port 1 to 65535");
    };
    let ready_key = format!("{key}.ready");
    let ready = parse_probe(required(t, key, "ready")?, &ready_key)?;
    Ok(Server {
        binary,
        addr: SocketAddrV4::new(Ipv4Addr::LOCALHOST, port),
        ready,
    })
}

fn parse_dashboard(
    value: &Value,
    key: &str,
    servers: &BTreeMap<String, Server>,
) -> Result<Dashboard, Invalid> {
    let t = table(value, key)?;
    only_keys(t, key, &["name", "url", "server", "probe"])?;
    let name = string(required(t, key, "name")?, &format!("{key}.name"))?.to_owned();
    let url_key = format!("{key}.url");
    let url = string(required(t, key, "url")?, &url_key)?.to_owned();
    let Some(port) = url_port(&url) else {
        return invalid(
            &url_key,
            "must be http://127.0.0.1:<port>/..., port 1 to 65535",
        );
    };
    let probe = parse_probe(required(t, key, "probe")?, &format!("{key}.probe"))?;

    let server = match t.get("server") {
        Some(value) => {
            let server_key = format!("{key}.server");
            let name = string(value, &server_key)?;
            let Some(declared) = servers.get(name) else {
                return invalid(&server_key, format!("no [server.{name}] is declared"));
            };
            if declared.addr.port() != port {
                return invalid(
                    &url_key,
                    format!(
                        "port {port} differs from server.{name}.addr port {}",
                        declared.addr.port()
                    ),
                );
            }
            Some(name.to_owned())
        }
        None => {
            if let Some((owner, _)) = servers.iter().find(|(_, s)| s.addr.port() == port) {
                return invalid(
                    &url_key,
                    format!("port {port} belongs to server.{owner}; add server = {owner:?}"),
                );
            }
            None
        }
    };

    Ok(Dashboard {
        name,
        url,
        port,
        server,
        probe,
    })
}

fn parse_probe(value: &Value, key: &str) -> Result<Probe, Invalid> {
    let t = table(value, key)?;
    only_keys(t, key, &["path", "contains"])?;
    let path_key = format!("{key}.path");
    let path = string(required(t, key, "path")?, &path_key)?;
    if !safe_path(path) {
        return invalid(
            &path_key,
            "must start with '/' and hold only printable ASCII with no space",
        );
    }
    let contains_key = format!("{key}.contains");
    let contains = string(required(t, key, "contains")?, &contains_key)?;
    if contains.chars().count() < MIN_MARKER {
        return invalid(
            &contains_key,
            format!("marker must have at least {MIN_MARKER} characters"),
        );
    }
    if single_tag(contains.trim()) {
        return invalid(
            &contains_key,
            "marker must not be a single tag with nothing outside it",
        );
    }
    Ok(Probe {
        path: path.to_owned(),
        contains: contains.to_owned(),
    })
}

/// `^/[!-~]*$`
fn safe_path(path: &str) -> bool {
    path.starts_with('/') && path.bytes().all(|b| (b'!'..=b'~').contains(&b))
}

/// `^<[^<>]*>$`
fn single_tag(marker: &str) -> bool {
    marker
        .strip_prefix('<')
        .and_then(|rest| rest.strip_suffix('>'))
        .is_some_and(|inner| !inner.contains(['<', '>']))
}

fn port(digits: &str) -> Option<u16> {
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse::<u16>().ok().filter(|&p| p != 0)
}

fn url_port(url: &str) -> Option<u16> {
    let rest = url.strip_prefix(URL_PREFIX)?;
    let end = rest.find('/')?;
    port(&rest[..end])
}

fn table<'a>(value: &'a Value, key: &str) -> Result<&'a Table, Invalid> {
    match value {
        Value::Table(t) => Ok(t),
        _ => invalid(key, "must be a table"),
    }
}

fn string<'a>(value: &'a Value, key: &str) -> Result<&'a str, Invalid> {
    match value {
        Value::String(s) => Ok(s),
        _ => invalid(key, "must be a string"),
    }
}

fn required<'a>(t: &'a Table, key: &str, field: &str) -> Result<&'a Value, Invalid> {
    match t.get(field) {
        Some(v) => Ok(v),
        None => invalid(&join(key, field), "missing required key"),
    }
}

fn only_keys(t: &Table, key: &str, allowed: &[&str]) -> Result<(), Invalid> {
    match t.keys().find(|k| !allowed.contains(&k.as_str())) {
        Some(unknown) => invalid(&join(key, unknown), "unknown key"),
        None => Ok(()),
    }
}

fn join(key: &str, field: &str) -> String {
    if key.is_empty() {
        field.to_owned()
    } else {
        format!("{key}.{field}")
    }
}
