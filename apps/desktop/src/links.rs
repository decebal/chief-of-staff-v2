//! `cos://` links into the window. Any app or web page can open one, so a
//! link is parsed into `Nav` before anything is touched, and it only
//! navigates: it never starts a run, writes state or approves anything. A
//! link that arrives before the window has said it is ready is held and
//! handed over by `link_ready`.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use cos::business;
use cos::runs;
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::{App, chain, switch_target};

/// The three places a link may name, as the `window.navigate` binding has them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "view", rename_all = "kebab-case")]
pub enum Nav {
    Run { business: String, run: String },
    Brief { business: String, day: String },
    SettingsAssistants,
}

/// A calendar day `YYYY-MM-DD` that exists.
fn real_day(s: &str) -> bool {
    runs::is_day(s) && s.parse::<jiff::civil::Date>().is_ok()
}

/// `url` read against the closed grammar: exactly `cos://run/<business>/<run>`,
/// `cos://brief/<business>/<YYYY-MM-DD>` or `cos://settings/assistants`. The
/// refusal quotes the URL cut to 200 chars.
pub fn parse_link(url: &str) -> Result<Nav, String> {
    let quoted: String = url.chars().take(200).collect();
    let refuse = |why: &str| Err(format!("link ignored ({why}): {quoted}"));
    let Some(rest) = url.strip_prefix("cos://") else {
        return refuse("not cos://");
    };
    if rest.contains(['?', '#', '%', '@', ':', '\\'])
        || rest.chars().any(|c| c.is_control() || c.is_whitespace())
    {
        return refuse("a query, fragment, encoding or authority");
    }
    match rest.split('/').collect::<Vec<_>>().as_slice() {
        ["run", b, r] if runs::is_business(b) && runs::is_id(r) => Ok(Nav::Run {
            business: (*b).to_owned(),
            run: (*r).to_owned(),
        }),
        ["brief", b, d] if runs::is_business(b) && real_day(d) => Ok(Nav::Brief {
            business: (*b).to_owned(),
            day: (*d).to_owned(),
        }),
        ["settings", "assistants"] => Ok(Nav::SettingsAssistants),
        _ => refuse("outside the grammar"),
    }
}

#[derive(Default)]
struct Queue {
    ready: bool,
    held: Option<Nav>,
}

/// Links that arrived before the window was ready, and whether it is.
#[derive(Default)]
pub(crate) struct Links(Mutex<Queue>);

impl Links {
    /// The window is about to reload, as a business switch does: links are
    /// held again until the new page calls `link_ready`.
    pub(crate) fn unready(&self) {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).ready = false;
    }
}

/// Routes one URL from the OS: ignored with one stderr line outside the
/// grammar, held while the window is not ready, else applied and sent to it
/// as `link-open`.
pub fn open_link<R: Runtime>(app: &AppHandle<R>, url: &str) {
    let nav = match parse_link(url) {
        Ok(nav) => nav,
        Err(e) => {
            eprintln!("cos://: {e}");
            return;
        }
    };
    let Some(links) = app.try_state::<Links>() else {
        return;
    };
    {
        let mut q = links.0.lock().unwrap_or_else(|e| e.into_inner());
        if !q.ready {
            q.held = Some(nav);
            return;
        }
    }
    let _ = app.emit("link-open", resolve(app, &nav));
}

/// The window has loaded and listens for `link-open`: the link held since
/// launch, applied, or null.
#[tauri::command]
pub(crate) fn link_ready<R: Runtime>(app: AppHandle<R>) -> Value {
    let held = {
        let links = app.state::<Links>();
        let mut q = links.0.lock().unwrap_or_else(|e| e.into_inner());
        q.ready = true;
        q.held.take()
    };
    held.map_or(Value::Null, |nav| resolve(&app, &nav))
}

/// `nav` with `shown`, and the reason when it was `refused`.
fn resolve<R: Runtime>(app: &AppHandle<R>, nav: &Nav) -> Value {
    let mut v = serde_json::to_value(nav).unwrap_or_default();
    match admit(app, nav) {
        Ok(()) => v["shown"] = true.into(),
        Err(e) => {
            v["shown"] = false.into();
            v["refused"] = e.into();
        }
    }
    v
}

/// The business is registered, the run or the brief is there, and the window
/// is on that business or may switch to it. The switch itself is the
/// window's (`switch_business`), so a window that declines it, as with
/// unsaved Hire edits, stays on the business it shows.
fn admit<R: Runtime>(app: &AppHandle<R>, nav: &Nav) -> Result<(), String> {
    let business = match nav {
        Nav::SettingsAssistants => return Ok(()),
        Nav::Run { business, .. } | Nav::Brief { business, .. } => business,
    };
    let state = app.state::<App>();
    let st = state.lock();
    let registry = business::load(&st.registry).map_err(chain)?;
    let found = registry.businesses.get(business).ok_or_else(|| {
        let known: Vec<&str> = registry.businesses.keys().map(String::as_str).collect();
        format!(
            "business {business:?} is not registered; known: {}",
            known.join(", ")
        )
    })?;
    match nav {
        Nav::Run { run, .. } => match runs::read(&found.home, run) {
            Ok(Some(r)) if r.business == *business => {}
            Ok(_) => return Err(format!("no run {run} under business {business}")),
            Err(e) => return Err(chain(e)),
        },
        Nav::Brief { day, .. } => {
            if day_page(&found.home, day).is_none() {
                return Err(format!("no brief for {day} under business {business}"));
            }
        }
        Nav::SettingsAssistants => {}
    }
    if st.triple.label() != business {
        switch_target(&st, business)?;
    }
    Ok(())
}

/// Which page shows a day's brief.
pub(crate) enum DayPage {
    /// The newest brief: the live report.
    Latest,
    /// An older day: its report from the archive.
    Archived(PathBuf),
}

/// The newest `out/briefs/<day>.json` by name.
fn latest_day(home: &Path) -> Option<String> {
    std::fs::read_dir(home.join("out/briefs"))
        .ok()?
        .filter_map(Result::ok)
        .filter_map(|e| {
            let name = e.file_name();
            let day = name.to_str()?.strip_suffix(".json")?.to_owned();
            real_day(&day).then_some(day)
        })
        .max()
}

pub(crate) fn day_page(home: &Path, day: &str) -> Option<DayPage> {
    if !real_day(day) {
        return None;
    }
    if latest_day(home).as_deref() == Some(day) {
        return Some(DayPage::Latest);
    }
    let file = home.join("out/reports").join(format!("{day}.html"));
    file.is_file().then_some(DayPage::Archived(file))
}
