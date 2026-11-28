//! A small client for Prime, the user's memory server, over its HTTP MCP
//! endpoint (`allsource-prime --mode http`). Loopback only, one deadline per
//! call, a capped reply: reading skill facts must never hang a hire or reach
//! off the machine. Prime's data directory belongs to the running server, so
//! this talks to it rather than opening the store.

use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};

pub const DEFAULT_URL: &str = "http://127.0.0.1:3906/mcp";
pub const DEADLINE: Duration = Duration::from_secs(2);
pub const BODY_MAX: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct Prime {
    url: String,
    addr: SocketAddr,
    host: String,
    path: String,
    deadline: Duration,
}

impl Prime {
    /// `COS_PRIME_URL`, else [`DEFAULT_URL`], with [`DEADLINE`].
    pub fn from_env() -> Result<Self> {
        let url = std::env::var("COS_PRIME_URL")
            .ok()
            .filter(|u| !u.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_URL.to_owned());
        Self::new(&url, DEADLINE)
    }

    /// Refuses, before any lookup or socket, a URL that is not `http://` to a
    /// literal loopback address or `localhost`.
    pub fn new(url: &str, deadline: Duration) -> Result<Self> {
        let rest = url
            .strip_prefix("http://")
            .ok_or_else(|| anyhow!("{url}: Prime is reached over http:// on loopback only"))?;
        if rest.bytes().any(|b| b <= 0x20 || b == 0x7f) {
            anyhow::bail!("{url:?}: a space or control character in the URL");
        }
        let (authority, path) = match rest.find('/') {
            Some(i) => (&rest[..i], &rest[i..]),
            None => (rest, "/"),
        };
        let (host, port) = match authority.strip_prefix('[') {
            Some(v6) => {
                let (h, p) = v6
                    .split_once(']')
                    .ok_or_else(|| anyhow!("{url}: unclosed IPv6 address"))?;
                (h, p.strip_prefix(':'))
            }
            None => match authority.rsplit_once(':') {
                Some((h, p)) => (h, Some(p)),
                None => (authority, None),
            },
        };
        let port: u16 = match port {
            Some(p) => p.parse().with_context(|| format!("{url}: bad port"))?,
            None => 80,
        };
        let ip: IpAddr = if host == "localhost" {
            IpAddr::V4(Ipv4Addr::LOCALHOST)
        } else {
            host.parse()
                .ok()
                .filter(IpAddr::is_loopback)
                .ok_or_else(|| anyhow!("{url}: {host} is not a loopback address"))?
        };
        Ok(Self {
            url: url.to_owned(),
            addr: SocketAddr::new(ip, port),
            host: authority.to_owned(),
            path: path.to_owned(),
            deadline,
        })
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    /// One `tools/call`; returns the JSON the tool wrote in
    /// `result.content[0].text`.
    pub fn call(&self, tool: &str, arguments: Value) -> Result<Value> {
        let until = Instant::now() + self.deadline;
        let left = || {
            let d = until.saturating_duration_since(Instant::now());
            if d.is_zero() {
                Err(anyhow!("Prime at {} did not answer in time", self.url))
            } else {
                Ok(d)
            }
        };
        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {"name": tool, "arguments": arguments},
        })
        .to_string();
        let mut s = TcpStream::connect_timeout(&self.addr, left()?)
            .with_context(|| format!("connect to Prime at {}", self.url))?;
        s.set_write_timeout(Some(left()?))?;
        write!(
            s,
            "POST {} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nAccept: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            self.path,
            self.host,
            body.len()
        )?;

        let mut buf = Vec::new();
        let mut chunk = [0u8; 16 * 1024];
        let head_end = loop {
            s.set_read_timeout(Some(left()?))?;
            let n = s
                .read(&mut chunk)
                .with_context(|| format!("read Prime's reply from {}", self.url))?;
            if n == 0 {
                bail!(
                    "Prime at {} closed the connection before replying",
                    self.url
                );
            }
            buf.extend_from_slice(&chunk[..n]);
            if let Some(p) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                break p + 4;
            }
            if buf.len() > 64 * 1024 {
                bail!("Prime's reply headers are too long");
            }
        };
        let head = String::from_utf8_lossy(&buf[..head_end]).to_ascii_lowercase();
        let status: u16 = head
            .split_whitespace()
            .nth(1)
            .and_then(|c| c.parse().ok())
            .ok_or_else(|| anyhow!("Prime sent no HTTP status"))?;
        if head.contains("transfer-encoding:") {
            bail!("Prime sent a chunked reply; cos reads Content-Length replies only");
        }
        let len: usize = head
            .lines()
            .find_map(|l| l.strip_prefix("content-length:"))
            .and_then(|v| v.trim().parse().ok())
            .ok_or_else(|| anyhow!("Prime's reply has no Content-Length"))?;
        if len > BODY_MAX {
            bail!(
                "Prime's reply is over {} MiB ({len} bytes)",
                BODY_MAX / (1024 * 1024)
            );
        }
        while buf.len() < head_end + len {
            s.set_read_timeout(Some(left()?))?;
            let n = s.read(&mut chunk)?;
            if n == 0 {
                bail!("Prime's reply ended early");
            }
            buf.extend_from_slice(&chunk[..n]);
        }
        let body = &buf[head_end..head_end + len];
        if status != 200 {
            let start = String::from_utf8_lossy(&body[..body.len().min(200)]).into_owned();
            bail!("Prime answered HTTP {status}: {start}");
        }
        let reply: Value =
            serde_json::from_slice(body).context("Prime's reply is not JSON-RPC JSON")?;
        if let Some(e) = reply.get("error") {
            bail!(
                "Prime refused {tool}: {}",
                e["message"].as_str().unwrap_or("no message")
            );
        }
        let result = &reply["result"];
        let text = result["content"][0]["text"]
            .as_str()
            .ok_or_else(|| anyhow!("Prime's {tool} reply has no text content"))?;
        if result["isError"].as_bool() == Some(true) {
            bail!(
                "Prime's {tool} failed: {}",
                text.chars().take(200).collect::<String>()
            );
        }
        serde_json::from_str(text).with_context(|| format!("Prime's {tool} text is not JSON"))
    }
}
