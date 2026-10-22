//! The starter kit's private runtime, `<workspace>/.chief-of-staff/`:
//! `profile.json` (preferences and approvals) and `state.json` (goals, KPIs,
//! deliverables, commitments, sources, runs, incidents, and a journal).
//!
//! A port of the kit's `scripts/state.py`: same files, same rules, same
//! messages, so the Python helper and `cos` can share one workspace.
//! `tests/state.rs` runs both validators over the same candidates.

use std::collections::{HashMap, HashSet};
use std::io::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde_json::{Map, Value, json};

pub const DIR: &str = ".chief-of-staff";
pub const COLLECTIONS: [&str; 7] = [
    "goals",
    "kpis",
    "deliverables",
    "commitments",
    "sources",
    "runs",
    "incidents",
];
const TERMINAL: [&str; 2] = ["complete", "cancelled"];
const WORK: [&str; 3] = ["goals", "deliverables", "commitments"];
const WORK_STATUS: [&str; 9] = [
    "proposed",
    "open",
    "active",
    "in_progress",
    "blocked",
    "waiting",
    "paused",
    "complete",
    "cancelled",
];
const DATE_FIELDS: [&str; 3] = ["targetDate", "dueDate", "week"];
const TIME_FIELDS: [&str; 13] = [
    "evidenceAt",
    "checkedAt",
    "reviewedAt",
    "meetingAt",
    "invokedAt",
    "finishedAt",
    "observedAt",
    "retryNotBefore",
    "recoveryClaimedAt",
    "resolvedAt",
    "lastCompleteThrough",
    "windowStart",
    "windowEnd",
];
pub const HEALTH: [&str; 6] = [
    "not_connected",
    "healthy",
    "rate_limited",
    "permission_denied",
    "auth_failed",
    "unavailable",
];
pub const REVIEW: [&str; 3] = ["not_reviewed", "partial", "complete"];

const KIT: &str = "plugins/chief-of-staff-starter/skills/chief-of-staff-starter";
pub const PROFILE_TEMPLATE: &str = include_str!(
    "../plugins/chief-of-staff-starter/skills/chief-of-staff-starter/assets/profile.example.json"
);
pub const DEMO_STATE: &str = include_str!(
    "../plugins/chief-of-staff-starter/skills/chief-of-staff-starter/assets/demo-state.json"
);

/// The kit's skill directory inside a cos checkout.
pub fn kit_dir(home: &Path) -> PathBuf {
    home.join(KIT)
}

fn require(condition: bool, message: &str) -> Result<()> {
    if !condition {
        bail!("{message}");
    }
    Ok(())
}

fn nonempty(v: Option<&Value>) -> bool {
    matches!(v, Some(Value::String(s)) if !s.trim().is_empty())
}

fn absent(v: Option<&Value>) -> bool {
    matches!(v, None | Some(Value::Null))
}

/// Python truthiness, for the helper's `if record.get(...)` checks.
fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64() != Some(0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(o)) => !o.is_empty(),
    }
}

fn is_int(v: Option<&Value>) -> bool {
    matches!(v, Some(Value::Number(n)) if n.is_i64() || n.is_u64())
}

fn one_of(v: Option<&Value>, set: &[&str]) -> bool {
    v.and_then(Value::as_str).is_some_and(|s| set.contains(&s))
}

/// An ISO-8601 instant with an offset; `Z` is accepted.
pub fn timestamp(v: Option<&Value>) -> Result<jiff::Timestamp> {
    let Some(Value::String(s)) = v else {
        bail!("Timestamp must be an ISO string");
    };
    let s = s.replace('Z', "+00:00");
    if let Ok(t) = s.parse::<jiff::Timestamp>() {
        return Ok(t);
    }
    if s.parse::<jiff::civil::DateTime>().is_ok() {
        bail!("Timestamp must include a timezone");
    }
    bail!("Invalid isoformat string: {s:?}")
}

fn date_ok(s: &str) -> bool {
    jiff::civil::Date::strptime("%Y-%m-%d", s).is_ok_and(|d| d.to_string() == s)
}

fn obj(v: &Value) -> &Map<String, Value> {
    static EMPTY: std::sync::OnceLock<Map<String, Value>> = std::sync::OnceLock::new();
    v.as_object().unwrap_or_else(|| EMPTY.get_or_init(Map::new))
}

pub fn empty_state() -> Value {
    let mut m = Map::new();
    m.insert("schemaVersion".into(), json!(1));
    m.insert("revision".into(), json!(0));
    for c in COLLECTIONS {
        m.insert(c.into(), json!([]));
    }
    m.insert("journal".into(), json!([]));
    Value::Object(m)
}

/// Checks one state snapshot. Returns the first rule it breaks.
pub fn validate(state: &Value) -> Result<()> {
    let Value::Object(top) = state else {
        bail!("State must be an object");
    };
    let mut want: HashSet<&str> = COLLECTIONS.into_iter().collect();
    want.extend(["schemaVersion", "revision", "journal"]);
    let have: HashSet<&str> = top.keys().map(String::as_str).collect();
    require(have == want, "Unexpected or missing top-level state fields")?;
    require(
        is_int(top.get("schemaVersion")) && top["schemaVersion"].as_i64() == Some(1),
        "Unsupported schema version",
    )?;
    require(
        is_int(top.get("revision")) && top["revision"].as_i64().is_some_and(|r| r >= 0),
        "Invalid revision",
    )?;

    let mut ids: HashMap<&str, HashSet<String>> = HashMap::new();
    for collection in COLLECTIONS {
        let Value::Array(records) = &top[collection] else {
            bail!("{collection} must be a list");
        };
        let seen = ids.entry(collection).or_default();
        for record in records {
            require(
                record.is_object() && nonempty(record.get("id")),
                &format!("Invalid {collection} record ID"),
            )?;
            let id = record["id"].as_str().unwrap_or_default().to_owned();
            require(
                !seen.contains(&id),
                &format!("Duplicate ID in {collection}"),
            )?;
            seen.insert(id);
            check_record(collection, record)?;
        }
    }

    let goal_ids = &ids["goals"];
    for record in list(state, "deliverables")
        .iter()
        .chain(list(state, "kpis"))
    {
        let goal = record.get("goalId").and_then(Value::as_str);
        require(
            goal.is_some_and(|g| goal_ids.contains(g)),
            "Missing linked goal",
        )?;
    }
    let deliverable_ids = &ids["deliverables"];
    let mut graph: HashMap<String, Vec<String>> = HashMap::new();
    for record in list(state, "deliverables") {
        let id = record["id"].as_str().unwrap_or_default();
        let mut children = Vec::new();
        for dep in dependencies(record) {
            let d = dep.as_str();
            require(
                d.is_some_and(|d| deliverable_ids.contains(d) && d != id),
                "Invalid deliverable dependency",
            )?;
            children.push(d.unwrap_or_default().to_owned());
        }
        graph.insert(id.to_owned(), children);
    }
    acyclic(&graph)?;
    for record in list(state, "commitments") {
        let g = record.get("goalId");
        require(
            absent(g)
                || g.and_then(Value::as_str)
                    .is_some_and(|g| goal_ids.contains(g)),
            "Missing commitment goal",
        )?;
    }
    for record in list(state, "incidents") {
        let s = record.get("sourceId").and_then(Value::as_str);
        require(
            s.is_some_and(|s| ids["sources"].contains(s)),
            "Incident requires a known source",
        )?;
    }

    let revision = top["revision"].as_i64().unwrap_or(-1);
    let journal = top["journal"].as_array();
    require(
        journal.is_some_and(|j| j.len() as i64 == revision),
        "Journal must match revision",
    )?;
    for (n, entry) in journal.into_iter().flatten().enumerate() {
        require(
            entry.is_object()
                && entry.get("revision").and_then(Value::as_f64) == Some((n + 1) as f64)
                && nonempty(entry.get("reason")),
            "Invalid journal entry",
        )?;
        timestamp(entry.get("at"))?;
    }
    Ok(())
}

fn list<'a>(state: &'a Value, collection: &str) -> &'a [Value] {
    state[collection].as_array().map_or(&[], Vec::as_slice)
}

fn dependencies(record: &Value) -> &[Value] {
    record
        .get("dependencies")
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

fn acyclic(graph: &HashMap<String, Vec<String>>) -> Result<()> {
    fn visit<'a>(
        node: &'a str,
        graph: &'a HashMap<String, Vec<String>>,
        visiting: &mut HashSet<&'a str>,
        visited: &mut HashSet<&'a str>,
    ) -> Result<()> {
        require(!visiting.contains(node), "Cyclic deliverable dependencies")?;
        if !visited.contains(node) {
            visiting.insert(node);
            for child in graph.get(node).into_iter().flatten() {
                visit(child, graph, visiting, visited)?;
            }
            visiting.remove(node);
            visited.insert(node);
        }
        Ok(())
    }
    let (mut visiting, mut visited) = (HashSet::new(), HashSet::new());
    for node in graph.keys() {
        visit(node, graph, &mut visiting, &mut visited)?;
    }
    Ok(())
}

fn check_record(collection: &str, r: &Value) -> Result<()> {
    for field in DATE_FIELDS {
        if !absent(r.get(field)) {
            let s = r[field].as_str();
            require(s.is_some(), &format!("{field} must be a date string"))?;
            require(date_ok(s.unwrap_or_default()), &format!("Invalid {field}"))?;
        }
    }
    for field in TIME_FIELDS {
        if !absent(r.get(field)) {
            timestamp(r.get(field))?;
        }
    }
    if WORK.contains(&collection) {
        require(nonempty(r.get("title")), "Tracked work needs a title")?;
        require(
            absent(r.get("owner")) || nonempty(r.get("owner")),
            "Invalid owner",
        )?;
        require(one_of(r.get("status"), &WORK_STATUS), "Invalid work status")?;
        if one_of(r.get("status"), &TERMINAL) {
            let evidence = r.get("completionEvidence");
            require(
                evidence.is_some_and(Value::is_object)
                    && nonempty(evidence.and_then(|e| e.get("reference"))),
                "Terminal work requires evidence",
            )?;
            timestamp(evidence.and_then(|e| e.get("verifiedAt")))?;
        }
    }
    match collection {
        "goals" => require(nonempty(r.get("finishLine")), "Goal requires a finish line")?,
        "deliverables" => {
            require(
                nonempty(r.get("doneWhen")),
                "Deliverable requires acceptance criteria",
            )?;
            let basis = ["proposed", "confirmed", "unknown"];
            require(one_of(r.get("ownerBasis"), &basis), "Invalid owner basis")?;
            require(one_of(r.get("dueBasis"), &basis), "Invalid due-date basis")?;
            require(
                absent(r.get("dueDate")) || r["dueBasis"].as_str() != Some("unknown"),
                "Date needs a stated basis",
            )?;
            require(
                r.get("dependencies").is_none_or(Value::is_array),
                "Dependencies must be a list",
            )?;
        }
        "commitments" => require(
            one_of(r.get("status"), &TERMINAL) || nonempty(r.get("nextReview")),
            "Open commitment requires a review trigger",
        )?,
        "kpis" => {
            require(
                nonempty(r.get("name")) && nonempty(r.get("unit")),
                "KPI needs a name and unit",
            )?;
            for field in ["baseline", "target", "current"] {
                let v = r.get(field);
                require(
                    absent(v) || v.is_some_and(Value::is_number),
                    "KPI values must be finite numbers or null",
                )?;
            }
            if !absent(r.get("current")) {
                require(
                    nonempty(r.get("source")) && nonempty(r.get("reportingWindow")),
                    "Measured KPI requires source and window",
                )?;
                timestamp(r.get("evidenceAt"))?;
            }
        }
        "sources" => {
            require(one_of(r.get("health"), &HEALTH), "Invalid source health")?;
            require(
                one_of(r.get("review"), &REVIEW),
                "Invalid review completeness",
            )?;
            if truthy(r.get("windowStart")) && truthy(r.get("windowEnd")) {
                require(
                    timestamp(r.get("windowStart"))? <= timestamp(r.get("windowEnd"))?,
                    "Invalid source window",
                )?;
            }
            if r["review"].as_str() == Some("complete") {
                require(
                    truthy(r.get("lastCompleteThrough"))
                        && truthy(r.get("windowStart"))
                        && truthy(r.get("windowEnd")),
                    "Complete review needs a bounded window and watermark",
                )?;
                require(
                    timestamp(r.get("lastCompleteThrough"))? == timestamp(r.get("windowEnd"))?,
                    "Complete watermark must match the reviewed window end",
                )?;
                require(
                    timestamp(r.get("checkedAt"))? >= timestamp(r.get("windowEnd"))?,
                    "Review cannot cover a future window",
                )?;
            }
        }
        "runs" => {
            require(
                one_of(
                    r.get("status"),
                    &["running", "complete", "partial", "blocked", "interrupted"],
                ),
                "Invalid run status",
            )?;
            timestamp(r.get("invokedAt"))?;
            if r["status"].as_str() == Some("complete") {
                require(
                    timestamp(r.get("finishedAt"))? >= timestamp(r.get("invokedAt"))?,
                    "Invalid completion time",
                )?;
            }
        }
        "incidents" => {
            require(
                one_of(
                    r.get("kind"),
                    &["rate_limit", "permission_denied", "auth_failed", "unknown"],
                ),
                "Invalid incident kind",
            )?;
            timestamp(r.get("observedAt"))?;
            if truthy(r.get("resolvedAt")) {
                require(
                    nonempty(r.get("resolutionRef")),
                    "Resolved incident needs a resolution reference",
                )?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// What a routine commit may change: nothing deleted, completed work and
/// recorded history unchanged, watermarks only forward and never past a gap.
pub fn validate_transition(old: &Value, new: &Value) -> Result<()> {
    validate(old)?;
    validate(new)?;
    require(
        new["journal"] == old["journal"],
        "Prior journal is immutable",
    )?;
    require(
        new["revision"] == old["revision"],
        "Candidate must preserve current revision",
    )?;
    for collection in COLLECTIONS {
        let after: HashMap<&str, &Value> = list(new, collection)
            .iter()
            .map(|r| (r["id"].as_str().unwrap_or_default(), r))
            .collect();
        for prior in list(old, collection) {
            let id = prior["id"].as_str().unwrap_or_default();
            let Some(updated) = after.get(id) else {
                bail!("Existing records cannot be deleted by a routine commit");
            };
            let (p, u) = (obj(prior), obj(updated));
            if WORK.contains(&collection) && one_of(p.get("status"), &TERMINAL) {
                require(
                    u.get("status") == p.get("status")
                        && norm(u.get("completionEvidence")) == norm(p.get("completionEvidence")),
                    "Completed history cannot be reopened or rewritten",
                )?;
            }
            if collection == "incidents" {
                for field in ["sourceId", "kind", "observedAt"] {
                    require(
                        norm(p.get(field)) == norm(u.get(field)),
                        "Original incident identity is immutable",
                    )?;
                }
                for field in [
                    "recoveryClaimedAt",
                    "recoveryOutcome",
                    "resolvedAt",
                    "resolutionRef",
                ] {
                    if !absent(p.get(field)) {
                        require(
                            p.get(field) == u.get(field),
                            "Existing recovery history is immutable",
                        )?;
                    }
                }
            }
            if collection == "runs" {
                require(
                    p.get("invokedAt") == u.get("invokedAt"),
                    "Original invocation time is immutable",
                )?;
                if p.get("status").and_then(Value::as_str) == Some("complete") {
                    require(prior == *updated, "Completed run receipt is immutable")?;
                }
            }
            if collection == "sources" {
                let (previous, current) =
                    (p.get("lastCompleteThrough"), u.get("lastCompleteThrough"));
                if truthy(previous) {
                    require(
                        truthy(current) && timestamp(current)? >= timestamp(previous)?,
                        "Review watermark cannot move backward",
                    )?;
                }
                if norm(current) != norm(previous) && truthy(current) {
                    require(
                        u.get("review").and_then(Value::as_str) == Some("complete"),
                        "Partial review cannot advance the watermark",
                    )?;
                    if truthy(previous) {
                        require(
                            timestamp(u.get("windowStart"))? <= timestamp(previous)?,
                            "Review cannot skip a coverage gap",
                        )?;
                    }
                }
            }
        }
    }
    Ok(())
}

/// `record.get(field)` in Python returns None for a missing key and for null alike.
fn norm(v: Option<&Value>) -> Option<&Value> {
    v.filter(|v| !v.is_null())
}

/// `<workspace>/.chief-of-staff`, refusing a relative or missing workspace,
/// one inside an installed plugin, and a symlinked runtime directory.
pub fn runtime_dir(workspace: &Path) -> Result<PathBuf> {
    require(
        workspace.is_absolute() && workspace.is_dir(),
        "Choose an existing absolute workspace directory",
    )?;
    let root = workspace.canonicalize()?;
    let in_plugin = root
        .ancestors()
        .any(|a| a.join("plugin.json").is_file() && a.join("skills").is_dir());
    require(!in_plugin, "Runtime must be outside the installed plugin")?;
    let result = root.join(DIR);
    require(
        !std::fs::symlink_metadata(&result).is_ok_and(|m| m.file_type().is_symlink()),
        "Refusing a symlinked runtime directory",
    )?;
    Ok(result)
}

/// Reads JSON, refusing a symlinked file.
pub fn load(path: &Path) -> Result<Value> {
    require(
        !std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()),
        "Refusing a symlinked data file",
    )?;
    let bytes = std::fs::read(path).with_context(|| format!("read {}", path.display()))?;
    serde_json::from_slice(&bytes).with_context(|| format!("parse {}", path.display()))
}

/// Temp file in the same directory, fsync, rename, fsync the directory.
pub fn atomic_write(path: &Path, value: &Value) -> Result<()> {
    let dir = path.parent().context("no parent directory")?;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp = dir.join(format!(".state-{}-{nanos}.tmp", std::process::id()));
    let write = || -> Result<()> {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)?;
        let mut text = serde_json::to_string_pretty(value)?;
        text.push('\n');
        f.write_all(text.as_bytes())?;
        f.sync_all()?;
        std::fs::rename(&tmp, path)?;
        std::fs::File::open(dir)?.sync_all()?;
        Ok(())
    };
    let result = write();
    if tmp.exists() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// Creates `.chief-of-staff/` with the blank profile and an empty state.
/// Never overwrites: an existing directory, even a partial one, stops it.
pub fn initialize(workspace: &Path) -> Result<Value> {
    let folder = runtime_dir(workspace)?;
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder
        .create(&folder)
        .with_context(|| format!("{} already exists", folder.display()))?;
    let profile: Value = serde_json::from_str(PROFILE_TEMPLATE)?;
    atomic_write(&folder.join("profile.json"), &profile)?;
    atomic_write(&folder.join("state.json"), &empty_state())?;
    Ok(json!({ "initialized": true, "revision": 0, "scheduleEnabled": false }))
}

/// Removes the lock it created, and only that.
struct Lock(PathBuf);

impl Drop for Lock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// The one write path: exclusive lock, revision check, transition rules,
/// journal entry, atomic replace. A held lock is never stolen.
pub fn commit(
    workspace: &Path,
    candidate: &Value,
    expected_revision: i64,
    reason: &str,
) -> Result<Value> {
    require(
        !reason.trim().is_empty() && reason.chars().count() <= 300,
        "Use a short nonempty commit reason",
    )?;
    let folder = runtime_dir(workspace)?;
    let lock_path = folder.join("write.lock");
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut opts, 0o600);
    let mut file = opts
        .open(&lock_path)
        .with_context(|| format!("write lock held: {}", lock_path.display()))?;
    let _lock = Lock(lock_path);
    file.write_all(std::process::id().to_string().as_bytes())?;
    drop(file);

    let path = folder.join("state.json");
    let old = load(&path)?;
    validate(&old)?;
    require(
        old["revision"].as_i64() == Some(expected_revision),
        "Revision conflict: reread and reconcile",
    )?;
    let mut new = candidate.clone();
    validate_transition(&old, &new)?;
    require(new != old, "No state change to commit")?;
    let revision = old["revision"].as_i64().unwrap_or(0) + 1;
    new["revision"] = json!(revision);
    if let Some(journal) = new["journal"].as_array_mut() {
        journal.push(json!({ "revision": revision, "at": now(), "reason": reason }));
    }
    validate(&new)?;
    atomic_write(&path, &new)?;
    Ok(json!({ "committed": true, "revision": revision }))
}

/// UTC, microseconds, `+00:00`: the form Python's `isoformat()` writes.
pub fn now() -> String {
    jiff::Timestamp::now()
        .to_zoned(jiff::tz::TimeZone::UTC)
        .strftime("%Y-%m-%dT%H:%M:%S%.6f+00:00")
        .to_string()
}

/// The saved state, validated. `None` when the workspace has none.
pub fn read(workspace: &Path) -> Result<Option<Value>> {
    let path = runtime_dir(&std::path::absolute(workspace)?)?.join("state.json");
    if !path.exists() {
        return Ok(None);
    }
    let state = load(&path)?;
    validate(&state)?;
    Ok(Some(state))
}

/// The private profile, if setup created one. Not validated: the kit treats it
/// as a user-maintained preference record.
pub fn profile(workspace: &Path) -> Option<Value> {
    let path = runtime_dir(&std::path::absolute(workspace).ok()?)
        .ok()?
        .join("profile.json");
    load(&path).ok()
}

pub fn tracking_approved(profile: &Value) -> bool {
    profile["localTracking"]["approved"] == json!(true)
}

/// An approved read source from the profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub id: String,
    pub service: String,
    pub scope: String,
    pub read_budget: String,
}

/// Sources with `readsApproved: true`. Everything else stays unread.
pub fn approved_sources(profile: &Value) -> Vec<Source> {
    let text = |v: &Value| match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    };
    profile["sources"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|s| s["readsApproved"] == json!(true))
        .map(|s| Source {
            id: text(&s["id"]),
            service: text(&s["service"]).to_lowercase(),
            scope: text(&s["scope"]),
            read_budget: text(&s["readBudget"]),
        })
        .collect()
}

/// GitHub orgs that count as this user's work, from the `orgs` list on
/// approved `code` sources. Empty means every org counts.
pub fn code_orgs(profile: &Value) -> Vec<String> {
    profile["sources"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|s| s["readsApproved"] == json!(true))
        .filter(|s| {
            s["service"].as_str().and_then(crate::context::local_kind)
                == Some(crate::context::Local::Code)
        })
        .flat_map(|s| s["orgs"].as_array().into_iter().flatten())
        .filter_map(|o| o.as_str().map(str::trim).filter(|o| !o.is_empty()))
        .map(str::to_owned)
        .collect()
}

/// The profile's IANA timezone, when set and known.
pub fn timezone(profile: &Value) -> Option<jiff::tz::TimeZone> {
    profile["timezone"]
        .as_str()
        .and_then(|tz| jiff::tz::TimeZone::get(tz).ok())
}

/// A record by id from one collection.
pub fn find<'a>(state: &'a Value, collection: &str, id: &str) -> Option<&'a Value> {
    list(state, collection)
        .iter()
        .find(|r| r["id"].as_str() == Some(id))
}

/// Work that is not complete or cancelled.
pub fn open<'a>(state: &'a Value, collection: &str) -> impl Iterator<Item = &'a Value> {
    list(state, collection)
        .iter()
        .filter(|r| !one_of(r.get("status"), &TERMINAL))
}
