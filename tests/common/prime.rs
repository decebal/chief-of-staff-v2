//! A stand-in Prime: an HTTP/1.1 listener on loopback that answers JSON-RPC
//! `tools/call` the way `allsource-prime --mode http` does (a JSON body with
//! `Content-Length`, the tool's JSON in `result.content[0].text`), records
//! every request, and can be told to stall, fail or misbehave.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{Value, json};

#[derive(Clone, Debug)]
pub enum Mode {
    /// Answer each tool from `replies`; `prime_add_node` gets a new id.
    Normal,
    /// Sleep this long before answering.
    Stall(Duration),
    /// Answer with this HTTP status and a short body.
    Status(u16),
    /// Answer 200 with this body as it is.
    Raw(String),
    /// Answer 200 with a body and no `Content-Length`.
    NoLength,
    /// Announce a body longer than the client accepts.
    Huge,
    /// A JSON-RPC error member.
    RpcError,
}

#[derive(Default)]
struct Inner {
    mode: Option<Mode>,
    replies: Vec<(String, Value)>,
    requests: Vec<Value>,
    connections: usize,
    next_id: usize,
}

#[derive(Clone)]
pub struct Prime {
    pub addr: SocketAddr,
    inner: Arc<Mutex<Inner>>,
}

impl Prime {
    pub fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let inner = Arc::new(Mutex::new(Inner::default()));
        let shared = inner.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let shared = shared.clone();
                std::thread::spawn(move || serve(stream, &shared));
            }
        });
        Self { addr, inner }
    }

    pub fn url(&self) -> String {
        format!("http://{}/mcp", self.addr)
    }

    pub fn mode(&self, m: Mode) {
        self.inner.lock().unwrap().mode = Some(m);
    }

    /// What `tool` answers in `Normal` mode.
    pub fn reply(&self, tool: &str, v: Value) {
        let mut i = self.inner.lock().unwrap();
        i.replies.retain(|(t, _)| t != tool);
        i.replies.push((tool.into(), v));
    }

    /// Every JSON-RPC request body received, in order.
    pub fn requests(&self) -> Vec<Value> {
        self.inner.lock().unwrap().requests.clone()
    }

    /// The `(tool, arguments)` of every `tools/call` received.
    pub fn calls(&self) -> Vec<(String, Value)> {
        self.requests()
            .iter()
            .map(|r| {
                (
                    r["params"]["name"].as_str().unwrap_or("").to_owned(),
                    r["params"]["arguments"].clone(),
                )
            })
            .collect()
    }

    pub fn connections(&self) -> usize {
        self.inner.lock().unwrap().connections
    }
}

fn serve(mut s: TcpStream, shared: &Mutex<Inner>) {
    shared.lock().unwrap().connections += 1;
    let Some(body) = read_request(&mut s) else {
        return;
    };
    let req: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    let mode = {
        let mut i = shared.lock().unwrap();
        i.requests.push(req.clone());
        i.mode.clone().unwrap_or(Mode::Normal)
    };
    let id = req["id"].clone();
    let ok = |text: &Value| {
        json!({"jsonrpc": "2.0", "id": id, "result": {"content": [{"type": "text", "text": text.to_string()}]}})
            .to_string()
    };
    let response = match mode {
        Mode::Stall(d) => {
            std::thread::sleep(d);
            http(200, &ok(&json!({})), true)
        }
        Mode::Status(code) => http(code, "upstream unavailable", true),
        Mode::Raw(b) => http(200, &b, true),
        Mode::NoLength => http(200, &ok(&json!({})), false),
        Mode::Huge => "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 999999999\r\n\r\n{".to_owned(),
        Mode::RpcError => http(
            200,
            &json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32602, "message": "unknown tool"}}).to_string(),
            true,
        ),
        Mode::Normal => {
            let tool = req["params"]["name"].as_str().unwrap_or("").to_owned();
            let mut i = shared.lock().unwrap();
            let text = if tool == "prime_add_node" {
                i.next_id += 1;
                json!({"id": format!("00000000-0000-0000-0000-{:012}", i.next_id), "type": req["params"]["arguments"]["type"]})
            } else {
                i.replies
                    .iter()
                    .find(|(t, _)| *t == tool)
                    .map(|(_, v)| v.clone())
                    .unwrap_or_else(|| json!({}))
            };
            http(200, &ok(&text), true)
        }
    };
    let _ = s.write_all(response.as_bytes());
}

fn http(code: u16, body: &str, length: bool) -> String {
    let len = if length {
        format!("content-length: {}\r\n", body.len())
    } else {
        String::new()
    };
    format!("HTTP/1.1 {code} X\r\ncontent-type: application/json\r\n{len}\r\n{body}")
}

fn read_request(s: &mut TcpStream) -> Option<Vec<u8>> {
    s.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let head_end = loop {
        let n = s.read(&mut chunk).ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(p) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break p + 4;
        }
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).to_lowercase();
    let len: usize = head
        .lines()
        .find_map(|l| l.strip_prefix("content-length:"))
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(0);
    while buf.len() < head_end + len {
        let n = s.read(&mut chunk).ok()?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    Some(buf[head_end..].to_vec())
}
