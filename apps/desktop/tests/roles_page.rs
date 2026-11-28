//! The role dialog end to end in headless Chrome, over the window's real
//! commands through the preview server on a temp home: New role, a library
//! entry, Skip, Save; the role's pane shows how it works; Retire moves it to
//! the Retired group and out of the Hire dialog. Run with
//! `COS_E2E_CHROME=<Chrome binary>` and `--include-ignored` (`task test-e2e`),
//! after `cargo build -p cos-desktop --example preview`.

#[allow(dead_code)]
#[path = "../../../tests/common/cdp.rs"]
mod cdp;

use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use cdp::Chrome;
use serde_json::json;

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

struct Preview(std::process::Child);

impl Drop for Preview {
    fn drop(&mut self) {
        cos::proc::kill_and_reap(&mut self.0);
    }
}

fn preview(home: &Path, port: u16) -> Preview {
    let bin = Path::new(ROOT).join("target/debug/examples/preview");
    assert!(
        bin.is_file(),
        "build it first: cargo build -p cos-desktop --example preview"
    );
    let mut cmd = Command::new(bin);
    cmd.arg(home)
        .arg(port.to_string())
        .env_remove("COS_BUSINESS");
    let child = cos::proc::spawn_grouped(&mut cmd).unwrap();
    let at = Instant::now();
    while TcpStream::connect(("127.0.0.1", port)).is_err() {
        assert!(
            at.elapsed() < Duration::from_secs(30),
            "the preview never listened"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    Preview(child)
}

fn home(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("cos-roles-page-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d.canonicalize().unwrap()
}

fn unbase64(s: &str) -> Vec<u8> {
    let val = |c: u8| match c {
        b'A'..=b'Z' => c - b'A',
        b'a'..=b'z' => c - b'a' + 26,
        b'0'..=b'9' => c - b'0' + 52,
        b'+' => 62,
        _ => 63,
    };
    let bytes: Vec<u8> = s.bytes().filter(|b| *b != b'=').map(val).collect();
    bytes
        .chunks(4)
        .flat_map(|q| {
            let n = q.iter().fold(0u32, |n, v| (n << 6) | u32::from(*v)) << (6 * (4 - q.len()));
            let all = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
            all.into_iter().take(q.len().saturating_sub(1))
        })
        .collect()
}

/// The viewport in light and in dark, saved under `target/e2e-shots/` as
/// evidence for the PR.
fn shots(c: &mut Chrome, name: &str) {
    let dir = Path::new(ROOT).join("target/e2e-shots");
    std::fs::create_dir_all(&dir).unwrap();
    for scheme in ["light", "dark"] {
        c.call(
            "Emulation.setEmulatedMedia",
            json!({ "features": [{ "name": "prefers-color-scheme", "value": scheme }] }),
        );
        // The dialog's colours transition; a shot sooner catches them midway.
        c.pause(Duration::from_millis(800));
        let url = c.screenshot();
        let png = unbase64(url.trim_start_matches("data:image/png;base64,"));
        std::fs::write(dir.join(format!("{name}-{scheme}.png")), png).unwrap();
    }
    c.call(
        "Emulation.setEmulatedMedia",
        json!({ "features": [{ "name": "prefers-color-scheme", "value": "light" }] }),
    );
}

fn click(c: &mut Chrome, js_target: &str) {
    let ok = c.eval(&format!(
        "(() => {{ const n = {js_target}; if (!n) return false; n.click(); return true; }})()"
    ));
    assert_eq!(ok, json!(true), "nothing to click: {js_target}");
}

#[test]
#[ignore = "needs COS_E2E_CHROME"]
fn a_library_role_is_added_shown_and_retired_through_the_dialog() {
    cdp::chrome_path();
    let h = home("dialog");
    let port = 8900 + (std::process::id() % 90) as u16;
    let _p = preview(&h, port);
    let dir = cdp::profile("roles-page");
    let mut c = Chrome::start(&dir, &["--disable-gpu"]);
    c.open(&format!("http://127.0.0.1:{port}/#tab=agents&pane=staff"));
    c.wait_for("document.querySelectorAll('#roles .card.role').length > 0");

    click(&mut c, "document.getElementById('role-new')");
    c.wait_for("document.getElementById('role-dialog').open && document.querySelectorAll('#rd-library .rd-entry').length === 12");
    shots(&mut c, "role-dialog-library");
    click(
        &mut c,
        "[...document.querySelectorAll('#rd-library .rd-entry')].find((e) => e.textContent.includes('Application security reviewer'))?.querySelector('button')",
    );
    c.wait_for("!document.getElementById('rd-step-specialise').hidden && document.getElementById('rd-id').value === 'appsec-reviewer'");
    c.wait_for("document.getElementById('rd-sent').textContent.includes('appsec-reviewer@1')");
    click(&mut c, "document.getElementById('rd-skip')");
    c.wait_for("!document.getElementById('rd-form').hidden && document.getElementById('rd-f-principles').value.length > 0");
    assert_eq!(
        c.eval("document.getElementById('rd-confirm').hidden"),
        json!(true),
        "an untouched entry changes no contract"
    );
    shots(&mut c, "role-dialog-review");
    c.eval("document.getElementById('rd-form').requestSubmit(); true");
    c.wait_for("!document.getElementById('role-dialog').open");
    c.wait_for("document.querySelector('#pane-role .playbook') !== null");
    shots(&mut c, "role-pane");
    let pane = c
        .eval("document.getElementById('pane-role').textContent")
        .to_string();
    for want in [
        "How it works",
        "Principles",
        "Method",
        "Never",
        "Hand back",
        "Escalates when",
        "appsec-reviewer@1",
    ] {
        assert!(pane.contains(want), "{want} missing from the role pane");
    }
    let saved = cos::agents::policy::load(&h);
    assert_eq!(
        saved.role("appsec-reviewer").unwrap().from.as_deref(),
        Some("appsec-reviewer@1")
    );

    let retire = "[...document.querySelectorAll('#pane-role .actions button')].find((b) => b.textContent.startsWith('Retire'))";
    click(&mut c, retire);
    click(&mut c, retire);
    c.wait_for("!document.getElementById('retired-roles').hidden && document.getElementById('retired-list').textContent.includes('appsec-reviewer')");
    assert!(
        cos::agents::policy::load(&h)
            .role("appsec-reviewer")
            .unwrap()
            .retired
    );

    click(&mut c, "document.getElementById('hire-open')");
    c.wait_for("document.getElementById('hire-dialog').open");
    assert_eq!(
        c.eval("[...document.getElementById('h-role').options].some((o) => o.value === 'appsec-reviewer')"),
        json!(false),
        "a retired role is not offered for hire"
    );
}
