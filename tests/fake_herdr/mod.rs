//! A stand-in Herdr for tests: `COS_HC` and `COS_HERDR_APP` point at the
//! fixtures, and `FAKE_HC_STATE` at a directory this test owns. The variables
//! are process-wide, so one test at a time holds them (`fake` takes a lock).
//! No test here can reach the real herdr-core.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

/// cos's `tests/fixtures`, from this crate or the desktop's beneath it.
fn fixture(name: &str) -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .map(|a| a.join("tests/fixtures").join(name))
        .find(|p| p.is_file())
        .unwrap_or_else(|| {
            panic!(
                "no tests/fixtures/{name} above {}",
                env!("CARGO_MANIFEST_DIR")
            )
        })
        .to_string_lossy()
        .into_owned()
}

/// The stand-in agent (`fake-agent.sh`).
pub fn agent() -> String {
    fixture("fake-agent.sh")
}

static ONE: Mutex<()> = Mutex::new(());

pub struct Fake {
    pub state: PathBuf,
    _one: MutexGuard<'static, ()>,
}

/// A fresh stand-in Herdr in `dir`, up, running nothing.
pub fn fake(dir: &Path) -> Fake {
    let one = ONE.lock().unwrap_or_else(|e| e.into_inner());
    let state = dir.join("herdr");
    let _ = std::fs::remove_dir_all(&state);
    std::fs::create_dir_all(&state).unwrap();
    // SAFETY: every test that reads these holds `ONE`.
    unsafe {
        std::env::set_var("COS_HC", fixture("fake-hc.sh"));
        std::env::set_var("COS_HERDR_APP", fixture("fake-herdr-app.sh"));
        std::env::set_var("FAKE_HC_STATE", &state);
        std::env::remove_var("FAKE_HC_EXEC");
        std::env::set_var("COS_HERDR_GRACE_SECS", "0");
    }
    Fake { state, _one: one }
}

impl Fake {
    /// `hc run` runs the agent to its end (`FAKE_HC_EXEC`).
    pub fn exec(&self, on: bool) {
        // SAFETY: `self` holds `ONE`.
        unsafe {
            if on {
                std::env::set_var("FAKE_HC_EXEC", "1");
            } else {
                std::env::remove_var("FAKE_HC_EXEC");
            }
        }
    }

    pub fn down(&self, down: bool) {
        let f = self.state.join("down");
        if down {
            std::fs::write(f, "").unwrap();
        } else {
            let _ = std::fs::remove_file(f);
        }
    }

    pub fn set(&self, address: &str, fact: &str, value: &str) {
        std::fs::write(self.state.join("terms").join(address).join(fact), value).unwrap();
    }

    pub fn fact(&self, address: &str, fact: &str) -> String {
        std::fs::read_to_string(self.state.join("terms").join(address).join(fact))
            .unwrap_or_default()
            .trim()
            .to_owned()
    }

    pub fn exists(&self, address: &str) -> bool {
        self.state.join("terms").join(address).is_dir()
    }

    /// NUL-separated words, as the fake records them.
    fn words(&self, address: &str, fact: &str) -> Vec<String> {
        std::fs::read(self.state.join("terms").join(address).join(fact))
            .unwrap_or_default()
            .split(|b| *b == 0)
            .filter(|w| !w.is_empty())
            .map(|w| String::from_utf8_lossy(w).into_owned())
            .collect()
    }

    pub fn argv(&self, address: &str) -> Vec<String> {
        self.words(address, "argv")
    }

    pub fn sent(&self, address: &str) -> Vec<String> {
        self.words(address, "sent")
    }

    pub fn calls(&self) -> String {
        std::fs::read_to_string(self.state.join("calls.log")).unwrap_or_default()
    }

    pub fn app_log(&self) -> String {
        std::fs::read_to_string(self.state.join("app.log")).unwrap_or_default()
    }

    /// Every terminal the fake holds.
    pub fn terminals(&self) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(self.state.join("terms"))
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    }
}
