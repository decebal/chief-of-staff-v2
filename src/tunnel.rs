//! `cos tunnel`: the read-only MCP server plus a Cloudflare quick tunnel, so
//! claude.ai can reach it. `cos` owns the `cloudflared` child for its whole
//! life: own process group, killed and reaped on Ctrl-C, on error, or when
//! the server stops.

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

use crate::mcp;
use crate::pipeline::Config;

/// How long cloudflared may take to print its public URL.
const URL_DEADLINE: Duration = Duration::from_secs(60);

pub async fn run(cfg: Config, port: u16) -> Result<()> {
    use tokio::signal::unix::{SignalKind, signal};
    // Registered before the child exists, so no window lets a signal kill
    // `cos` without the tunnel going with it.
    let mut int = signal(SignalKind::interrupt())?;
    let mut term = signal(SignalKind::terminate())?;
    let mut hup = signal(SignalKind::hangup())?;

    let token = token(&cfg.home)?;
    let log = cfg.home.join("out/logs/tunnel.log");
    let pidfile = cfg.home.join("out/tunnel.pid");
    sweep_stale(&pidfile);
    let mut child = spawn_cloudflared(port, &log)?;
    if let Some(pid) = child.id() {
        std::fs::write(&pidfile, pid.to_string())?;
    }

    let work = async {
        let url = match tokio::time::timeout(URL_DEADLINE, public_url(&mut child, &log)).await {
            Ok(r) => r?,
            Err(_) => bail!(
                "cloudflared printed no URL within {}s; see {}",
                URL_DEADLINE.as_secs(),
                log.display()
            ),
        };
        let host = url.trim_start_matches("https://").to_owned();
        println!("Connector URL (treat it like a password):\n\n  {url}/mcp/{token}\n");
        println!("Add it in claude.ai → Settings → Connectors → Add custom connector.");
        println!("The URL changes every time the tunnel restarts. Ctrl-C to stop.");

        let addr = format!("127.0.0.1:{port}");
        tokio::select! {
            r = mcp::http(cfg, &addr, &token, Some(host)) => r,
            status = child.wait() => Err(anyhow::anyhow!("cloudflared exited: {status:?}; see {}", log.display())),
        }
    };
    let outcome = tokio::select! {
        r = work => r,
        _ = int.recv() => Ok(()),
        _ = term.recv() => Ok(()),
        _ = hup.recv() => Ok(()),
    };
    let _ = kill_and_reap(&mut child).await;
    let _ = std::fs::remove_file(&pidfile);
    outcome
}

/// A SIGKILL to `cos` cannot be caught, so its tunnel outlives it. The next
/// start finds the pid it left and stops it, if it is still a cloudflared.
fn sweep_stale(pidfile: &Path) {
    let Some(pid) = std::fs::read_to_string(pidfile)
        .ok()
        .and_then(|s| s.trim().parse::<libc::pid_t>().ok())
    else {
        return;
    };
    let comm = std::process::Command::new("ps")
        .args(["-o", "comm=", "-p", &pid.to_string()])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_default();
    if comm.ends_with("cloudflared") {
        eprintln!("cos: stopping stale cloudflared {pid} from a previous run");
        // SAFETY: signalling the process group we created on a previous run.
        unsafe {
            libc::kill(-pid, libc::SIGTERM);
        }
    }
    let _ = std::fs::remove_file(pidfile);
}

/// The path secret, created once and reused so a restart keeps the same path.
fn token(home: &Path) -> Result<String> {
    let path = home.join("out/.mcp-token");
    if let Ok(t) = std::fs::read_to_string(&path) {
        let t = t.trim().to_owned();
        if t.len() >= 32 {
            return Ok(t);
        }
    }
    let mut bytes = [0u8; 32];
    use std::io::Read;
    std::fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    let t: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    std::fs::create_dir_all(path.parent().context("token path has no parent")?)?;
    std::fs::write(&path, &t)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(t)
}

fn spawn_cloudflared(port: u16, log: &Path) -> Result<Child> {
    if let Some(dir) = log.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut cmd = Command::new("cloudflared");
    cmd.args([
        "tunnel",
        "--no-autoupdate",
        "--url",
        &format!("http://127.0.0.1:{port}"),
    ])
    .stdin(Stdio::null())
    .stdout(Stdio::null())
    .stderr(Stdio::piped())
    .process_group(0)
    .kill_on_drop(true);
    cmd.spawn()
        .context("spawn cloudflared — install it with `brew install cloudflared`")
}

/// Reads cloudflared's stderr until the quick-tunnel URL appears, then keeps
/// draining it to the log so a full pipe never stalls the tunnel.
async fn public_url(child: &mut Child, log: &Path) -> Result<String> {
    let stderr = child
        .stderr
        .take()
        .context("cloudflared stderr not piped")?;
    let mut lines = BufReader::new(stderr).lines();
    let mut out = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log)?;
    use std::io::Write;
    while let Some(line) = lines.next_line().await? {
        writeln!(out, "{line}")?;
        if let Some(url) = find_url(&line) {
            tokio::spawn(async move {
                while let Ok(Some(line)) = lines.next_line().await {
                    let _ = writeln!(out, "{line}");
                }
            });
            return Ok(url);
        }
    }
    bail!("cloudflared exited before printing a URL")
}

/// The first `https://<name>.trycloudflare.com` in a log line.
pub fn find_url(line: &str) -> Option<String> {
    let start = line.find("https://")?;
    let rest = &line[start..];
    let end = rest
        .find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, ':' | '/' | '.' | '-')))
        .unwrap_or(rest.len());
    let url = &rest[..end];
    url.ends_with(".trycloudflare.com").then(|| url.to_owned())
}

async fn kill_and_reap(child: &mut Child) -> Result<()> {
    if let Some(pid) = child.id() {
        // SAFETY: signalling the process group we created; no memory is touched.
        unsafe {
            libc::kill(-(pid as libc::pid_t), libc::SIGTERM);
        }
    }
    if tokio::time::timeout(Duration::from_secs(5), child.wait())
        .await
        .is_err()
    {
        child.kill().await?;
    }
    Ok(())
}
