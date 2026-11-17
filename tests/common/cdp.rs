//! Headless Chrome driven over the DevTools protocol on `--remote-debugging-pipe`:
//! NUL-delimited JSON on two descriptors (Chrome reads fd 3, writes fd 4).
//! Chrome is an owned child in its own process group; the guard's `Drop`
//! kills the group, reaps it and removes the profile directory, so a failed
//! assertion cleans up as the deadline does.

use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::{FromRawFd, RawFd};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

/// Every test's whole budget: 40 s, or `COS_E2E_DEADLINE` seconds, capped at
/// 60 so seven tests stay inside the job's 5-minute step.
pub fn deadline() -> Duration {
    let secs = std::env::var("COS_E2E_DEADLINE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(40u64)
        .min(60);
    Duration::from_secs(secs)
}

/// The flags that give headless Chrome a SwiftShader WebGPU adapter.
pub const SWIFTSHADER: &[&str] = &[
    "--use-angle=swiftshader",
    "--enable-unsafe-swiftshader",
    "--use-webgpu-adapter=swiftshader",
    "--enable-unsafe-webgpu",
];

/// The Chrome binary `COS_E2E_CHROME` names; panics when it is unset or not executable.
pub fn chrome_path() -> PathBuf {
    let p = std::env::var_os("COS_E2E_CHROME")
        .map(PathBuf::from)
        .expect("COS_E2E_CHROME is unset: point it at a Chrome binary");
    use std::os::unix::fs::PermissionsExt;
    let ok =
        std::fs::metadata(&p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0);
    assert!(ok, "COS_E2E_CHROME={} is not an executable", p.display());
    p
}

pub struct Chrome {
    child: Child,
    pgid: libc::pid_t,
    profile: PathBuf,
    to: File,
    from: mpsc::Receiver<Value>,
    next: u64,
    session: String,
    until: Instant,
    /// Console lines and exceptions, printed when a wait fails.
    pub log: Vec<String>,
}

impl Drop for Chrome {
    fn drop(&mut self) {
        cos::proc::kill_group(self.pgid);
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.profile);
    }
}

fn pipe() -> (RawFd, RawFd) {
    let mut fds = [0; 2];
    // SAFETY: `pipe` writes two descriptors into the array we own.
    assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0, "pipe");
    // Above 4, so the child's dup2 onto 3 and 4 never clobbers its own source.
    let high = |fd: RawFd| {
        // SAFETY: duplicating a descriptor we own, close-on-exec, then closing the original.
        let h = unsafe {
            let h = libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 10);
            libc::close(fd);
            h
        };
        assert!(h >= 0, "fcntl F_DUPFD_CLOEXEC");
        h
    };
    (high(fds[0]), high(fds[1]))
}

impl Chrome {
    /// Starts Chrome with `flags` on a fresh profile in `profile` and opens
    /// one blank page.
    pub fn start(profile: &Path, flags: &[&str]) -> Chrome {
        let (cmd_r, cmd_w) = pipe();
        let (resp_r, resp_w) = pipe();
        std::fs::create_dir_all(profile).unwrap();
        let mut cmd = Command::new(chrome_path());
        cmd.args([
            "--headless=new",
            "--remote-debugging-pipe",
            "--window-size=1400,1000",
            "--no-first-run",
            "--no-default-browser-check",
            "--disable-background-timer-throttling",
            "--disable-renderer-backgrounding",
        ])
        .arg(format!("--user-data-dir={}", profile.display()))
        .args(flags)
        .arg("about:blank")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
        // SAFETY: only async-signal-safe calls between fork and exec.
        unsafe {
            cmd.pre_exec(move || {
                if libc::dup2(cmd_r, 3) < 0 || libc::dup2(resp_w, 4) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let child = cos::proc::spawn_grouped(&mut cmd).expect("start Chrome");
        let pgid = cos::proc::group_of(&child);
        // SAFETY: the child holds its own copies; these ends are ours alone.
        let (to, mut reader) = unsafe {
            libc::close(cmd_r);
            libc::close(resp_w);
            (File::from_raw_fd(cmd_w), File::from_raw_fd(resp_r))
        };
        let (tx, from) = mpsc::channel();
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let mut chunk = [0u8; 65536];
            while let Ok(n) = reader.read(&mut chunk) {
                if n == 0 {
                    break;
                }
                buf.extend_from_slice(&chunk[..n]);
                while let Some(end) = buf.iter().position(|&b| b == 0) {
                    let msg: Vec<u8> = buf.drain(..=end).collect();
                    if let Ok(v) = serde_json::from_slice::<Value>(&msg[..msg.len() - 1])
                        && tx.send(v).is_err()
                    {
                        return;
                    }
                }
            }
        });
        let mut c = Chrome {
            child,
            pgid,
            profile: profile.to_owned(),
            to,
            from,
            next: 0,
            session: String::new(),
            until: Instant::now() + deadline(),
            log: Vec::new(),
        };
        let target = c.send(
            "Target.createTarget",
            json!({ "url": "about:blank" }),
            false,
        );
        let id = target["targetId"].as_str().unwrap().to_owned();
        let attached = c.send(
            "Target.attachToTarget",
            json!({ "targetId": id, "flatten": true }),
            false,
        );
        c.session = attached["sessionId"].as_str().unwrap().to_owned();
        c.call("Runtime.enable", json!({}));
        c.call("Page.enable", json!({}));
        c
    }

    fn left(&self) -> Duration {
        self.until.saturating_duration_since(Instant::now())
    }

    fn fail(&self, what: &str) -> ! {
        panic!("{what}\n--- page console ---\n{}", self.log.join("\n"));
    }

    fn record(&mut self, ev: &Value) {
        match ev["method"].as_str() {
            Some("Runtime.consoleAPICalled") => {
                let args: Vec<String> = ev["params"]["args"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|a| {
                        a["value"]
                            .as_str()
                            .map(str::to_owned)
                            .unwrap_or_else(|| a["description"].to_string())
                    })
                    .collect();
                self.log.push(format!(
                    "{}: {}",
                    ev["params"]["type"].as_str().unwrap_or(""),
                    args.join(" ")
                ));
            }
            Some("Runtime.exceptionThrown") => {
                let d = &ev["params"]["exceptionDetails"];
                self.log.push(format!(
                    "exception: {} {}",
                    d["text"].as_str().unwrap_or(""),
                    d["exception"]["description"].as_str().unwrap_or("")
                ));
            }
            _ => {}
        }
    }

    fn send(&mut self, method: &str, params: Value, in_session: bool) -> Value {
        self.next += 1;
        let id = self.next;
        let mut msg = json!({ "id": id, "method": method, "params": params });
        if in_session {
            msg["sessionId"] = json!(self.session);
        }
        let mut bytes = serde_json::to_vec(&msg).unwrap();
        bytes.push(0);
        if self.to.write_all(&bytes).is_err() {
            self.fail(&format!("Chrome closed its pipe before {method}"));
        }
        loop {
            let left = self.left();
            if left.is_zero() {
                self.fail(&format!("deadline passed waiting for {method}"));
            }
            match self.from.recv_timeout(left) {
                Ok(v) if v["id"].as_u64() == Some(id) => {
                    if let Some(e) = v.get("error") {
                        self.fail(&format!("{method}: {e}"));
                    }
                    return v["result"].clone();
                }
                Ok(v) => self.record(&v),
                Err(_) => self.fail(&format!("no answer to {method}")),
            }
        }
    }

    /// One DevTools command in the page's session.
    pub fn call(&mut self, method: &str, params: Value) -> Value {
        self.send(method, params, true)
    }

    /// Navigates and waits for the load event.
    pub fn open(&mut self, url: &str) {
        self.call("Page.navigate", json!({ "url": url }));
        self.wait_for("document.readyState === 'complete'");
    }

    /// Reloads and waits for the load event.
    pub fn reload(&mut self) {
        self.call("Page.reload", json!({}));
        self.pause(Duration::from_millis(200));
        self.wait_for("document.readyState === 'complete'");
    }

    /// `expr`'s value, awaiting a promise; panics on an exception.
    pub fn eval(&mut self, expr: &str) -> Value {
        let r = self.call(
            "Runtime.evaluate",
            json!({ "expression": expr, "awaitPromise": true, "returnByValue": true }),
        );
        if let Some(d) = r.get("exceptionDetails") {
            self.fail(&format!(
                "{expr}: {}",
                d["exception"]["description"]
                    .as_str()
                    .unwrap_or(&d.to_string())
            ));
        }
        r["result"]["value"].clone()
    }

    /// Polls `expr` until it is truthy.
    pub fn wait_for(&mut self, expr: &str) {
        let probe = format!("!!({expr})");
        loop {
            if self.eval(&probe) == json!(true) {
                return;
            }
            if self.left() < Duration::from_millis(150) {
                self.fail(&format!("deadline passed waiting for {expr}"));
            }
            self.pause(Duration::from_millis(100));
        }
    }

    /// Lets time pass while still draining events.
    pub fn pause(&mut self, d: Duration) {
        let end = Instant::now() + d.min(self.left());
        while let Some(left) = end.checked_duration_since(Instant::now()) {
            match self.from.recv_timeout(left) {
                Ok(v) => self.record(&v),
                Err(_) => break,
            }
        }
    }

    /// A `body.dataset` value as text.
    pub fn data(&mut self, key: &str) -> String {
        self.eval(&format!("document.body.dataset[{key:?}] ?? ''"))
            .as_str()
            .unwrap_or("")
            .to_owned()
    }

    /// A mouse event at CSS pixel (`x`, `y`) of the viewport.
    pub fn mouse(&mut self, kind: &str, x: f64, y: f64, buttons: u32) {
        self.call(
            "Input.dispatchMouseEvent",
            json!({ "type": kind, "x": x, "y": y, "button": if kind == "mouseMoved" && buttons == 0 { "none" } else { "left" }, "buttons": buttons, "clickCount": 1 }),
        );
    }

    /// The viewport as a PNG data URL.
    pub fn screenshot(&mut self) -> String {
        let r = self.call("Page.captureScreenshot", json!({ "format": "png" }));
        format!("data:image/png;base64,{}", r["data"].as_str().unwrap())
    }
}

/// A profile directory unique to this test and process.
pub fn profile(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("cos-e2e-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}
