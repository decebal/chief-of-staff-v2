//! Skill facts in Prime: reading them into a [`Graph`] (checked, folded,
//! cached in `out/skills/graph.json`) and writing one when the user curates.
//! Prime is asked at most once per [`REUSE`] per home, and never twice at
//! once for one home in a process, so a slow Prime costs a hire one
//! deadline, not one per keystroke.
//! Kept apart from `skills.rs`, which never touches the network.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::policy::Policy;
use super::skills::{Catalog, Fact, Graph, GraphState, Verdict, valid_name};
use crate::prime::Prime;

pub const NODE_TYPE: &str = "cos_skill";
pub const SOURCE: &str = "cos-skills";
pub const REUSE: Duration = Duration::from_secs(30);
pub const NOTE_MAX: usize = 500;

/// Homes with a Prime read in flight in this process.
static IN_FLIGHT: Mutex<BTreeSet<PathBuf>> = Mutex::new(BTreeSet::new());

struct Flight(PathBuf);

impl Flight {
    fn take(home: &Path) -> Option<Self> {
        let mut set = IN_FLIGHT.lock().unwrap_or_else(|e| e.into_inner());
        set.insert(home.to_path_buf())
            .then(|| Self(home.to_path_buf()))
    }
}

impl Drop for Flight {
    fn drop(&mut self) {
        IN_FLIGHT
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.0);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Live,
    /// `cos --dry-run`: the cache only; Prime is not asked and nothing is written.
    Dry,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Cache {
    pub facts: BTreeMap<String, Fact>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failed_at: Option<String>,
}

pub fn cache_path(home: &Path) -> PathBuf {
    home.join("out/skills/graph.json")
}

pub fn load_cache(home: &Path) -> Option<Cache> {
    serde_json::from_slice(&std::fs::read(cache_path(home)).ok()?).ok()
}

/// Writes `graph.json` through a temp file of its own and a rename, so two
/// writers at once each leave a whole file.
pub fn store_cache(home: &Path, c: &Cache) -> Result<()> {
    let p = cache_path(home);
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d)?;
    }
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp = p.with_file_name(format!("graph.json.{}.{seq}.tmp", std::process::id()));
    std::fs::write(&tmp, serde_json::to_vec_pretty(c)?)?;
    std::fs::rename(&tmp, &p)?;
    Ok(())
}

fn now() -> String {
    jiff::Timestamp::now().to_string()
}

fn younger_than(at: Option<&str>, d: Duration) -> bool {
    at.and_then(|a| a.parse::<jiff::Timestamp>().ok())
        .is_some_and(|t| {
            jiff::Timestamp::now()
                .duration_since(t)
                .unsigned_abs()
                .le(&d)
        })
}

/// The graph a hire resolves against: Prime's facts when it answers, else
/// the last good read, else none. `prime` is `None` when `COS_PRIME_URL` was
/// refused; the reason is in `refused`.
pub fn read_graph(home: &Path, prime: Result<&Prime, &str>, policy: &Policy, mode: Mode) -> Graph {
    let cache = load_cache(home);
    let from_cache = |state: GraphState, mut warnings: Vec<String>| {
        let facts = match &cache {
            Some(c) => roles_checked(&c.facts, policy, &mut warnings),
            None => BTreeMap::new(),
        };
        let never_read = cache.as_ref().is_none_or(|c| c.read_at.is_none());
        let state = match state {
            GraphState::Stale { .. } if never_read => GraphState::None,
            s => s,
        };
        Graph {
            facts,
            state,
            warnings,
        }
    };
    let stale = GraphState::Stale {
        read_at: cache.as_ref().and_then(|c| c.read_at.clone()),
    };
    if mode == Mode::Dry {
        return from_cache(stale, Vec::new());
    }
    if let Some(c) = &cache {
        let fresh = younger_than(c.read_at.as_deref(), REUSE)
            && c.failed_at.as_deref() <= c.read_at.as_deref();
        if fresh {
            return from_cache(GraphState::Fresh, Vec::new());
        }
        if younger_than(c.failed_at.as_deref(), REUSE) {
            return from_cache(stale, Vec::new());
        }
    }
    let prime = match prime {
        Ok(p) => p,
        Err(why) => return from_cache(stale, vec![format!("skill graph not read: {why}")]),
    };
    let Some(_held) = Flight::take(home) else {
        return from_cache(stale, Vec::new());
    };
    let reply = prime
        .call("prime_search", json!({ "type": NODE_TYPE }))
        .and_then(|v| {
            if v["nodes"].is_array() {
                Ok(v)
            } else {
                Err(anyhow::anyhow!("its prime_search reply has no nodes list"))
            }
        });
    match reply {
        Ok(v) => {
            let mut warnings = Vec::new();
            let facts = fold(&v, &mut warnings);
            let c = Cache {
                facts,
                read_at: Some(now()),
                failed_at: None,
            };
            if let Err(e) = store_cache(home, &c) {
                warnings.push(format!("skill graph cache not written: {e:#}"));
            }
            let facts = roles_checked(&c.facts, policy, &mut warnings);
            Graph {
                facts,
                state: GraphState::Fresh,
                warnings,
            }
        }
        Err(e) => {
            if let Some(mut c) = cache.clone() {
                c.failed_at = Some(now());
                let _ = store_cache(home, &c);
            } else {
                let _ = store_cache(
                    home,
                    &Cache {
                        failed_at: Some(now()),
                        ..Cache::default()
                    },
                );
            }
            from_cache(stale, vec![format!("Prime did not answer: {e:#}")])
        }
    }
}

/// VERIFY-R on every node, then the newest valid fact per name. A newest
/// fact that fails does not hide an older one that passes.
pub fn fold(search: &Value, warnings: &mut Vec<String>) -> BTreeMap<String, Fact> {
    let mut out: BTreeMap<String, (jiff::Timestamp, Fact)> = BTreeMap::new();
    for n in search["nodes"].as_array().into_iter().flatten() {
        let props = &n["properties"];
        let mut fact: Fact = match serde_json::from_value(props.clone()) {
            Ok(f) => f,
            Err(e) => {
                warnings.push(format!(
                    "a skill fact for {} was ignored: {e}",
                    props["name"].as_str().unwrap_or("an unnamed skill")
                ));
                continue;
            }
        };
        let bad = if !valid_name(&fact.name) {
            Some("its name")
        } else if !fact.requires.iter().all(|r| valid_name(r)) {
            Some("a requires name")
        } else if fact
            .note
            .as_ref()
            .is_some_and(|n| n.chars().count() > NOTE_MAX)
        {
            Some("its note")
        } else {
            None
        };
        let at = fact.curated_at.parse::<jiff::Timestamp>();
        if let Some(what) = bad.or(at.is_err().then_some("its curated_at")) {
            warnings.push(format!(
                "a skill fact for {:?} was ignored: {what} is not valid",
                fact.name
            ));
            continue;
        }
        let at = at.expect("checked above");
        fact.node = n["id"]
            .as_str()
            .filter(|id| node_id_ok(id))
            .map(str::to_owned);
        match out.get(&fact.name) {
            Some((t, _)) if *t > at => {}
            _ => {
                out.insert(fact.name.clone(), (at, fact));
            }
        }
    }
    out.into_iter().map(|(k, (_, f))| (k, f)).collect()
}

/// Facts with only the roles this policy knows; an unknown role is dropped
/// with a warning, the fact kept.
fn roles_checked(
    facts: &BTreeMap<String, Fact>,
    policy: &Policy,
    warnings: &mut Vec<String>,
) -> BTreeMap<String, Fact> {
    facts
        .iter()
        .map(|(k, f)| {
            let mut f = f.clone();
            let (known, unknown): (Vec<String>, Vec<String>) =
                f.roles.drain(..).partition(|r| policy.hire(r).is_ok());
            for r in unknown {
                warnings.push(format!("{k}: role {r} is not on this business's staff"));
            }
            f.roles = known;
            (k.clone(), f)
        })
        .collect()
}

/// What the user decided about one skill.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Decision {
    pub name: String,
    pub verdict: Option<Verdict>,
    #[serde(default)]
    pub roles: Vec<String>,
    #[serde(default)]
    pub requires: Vec<String>,
    #[serde(default)]
    pub note: Option<String>,
}

/// VERIFY-W, then one `prime_add_node`, then a `requires` edge to each
/// required skill's newest node. Returns warnings (edges are best effort).
pub fn write_fact(
    home: &Path,
    prime: &Prime,
    catalog: &Catalog,
    policy: &Policy,
    d: &Decision,
) -> Result<Vec<String>> {
    if std::env::var_os("COS_RUN_ID").is_some_and(|v| !v.is_empty()) {
        bail!("skill facts are the user's to write; a hire cannot write them");
    }
    let Some(verdict) = d.verdict else {
        bail!("say whether {} is relevant or irrelevant", d.name);
    };
    if !catalog.skills.contains_key(&d.name) {
        bail!(
            "{} is not a skill in any skill root on this machine",
            d.name
        );
    }
    for r in &d.roles {
        policy.hire(r)?;
    }
    for r in &d.requires {
        if r == &d.name {
            bail!("{} cannot require itself", d.name);
        }
        if !catalog.skills.contains_key(r) {
            bail!(
                "{} requires {r}, which is not a skill on this machine",
                d.name
            );
        }
    }
    if d.note
        .as_ref()
        .is_some_and(|n| n.chars().count() > NOTE_MAX)
    {
        bail!("the note is over {NOTE_MAX} characters");
    }
    let mut props = json!({
        "name": d.name,
        "verdict": verdict,
        "roles": d.roles,
        "requires": d.requires,
        "curated_at": now(),
        "domain": SOURCE,
        "source": SOURCE,
    });
    if let Some(n) = d.note.as_deref().filter(|n| !n.trim().is_empty()) {
        props["note"] = json!(n);
    }
    let graph = read_graph(home, Ok(prime), policy, Mode::Live);
    let added = prime
        .call(
            "prime_add_node",
            json!({ "type": NODE_TYPE, "properties": props }),
        )
        .map_err(|e| anyhow::anyhow!("not written to Prime at {}: {e:#}", prime.url()))?;
    if let Some(mut c) = load_cache(home) {
        c.read_at = None;
        c.failed_at = None;
        let _ = store_cache(home, &c);
    }
    let mut warnings = Vec::new();
    let Some(source) = node_ref(&added) else {
        if !d.requires.is_empty() {
            warnings.push("Prime returned no node id: requires edges not added".into());
        }
        return Ok(warnings);
    };
    for r in &d.requires {
        let Some(target) = graph.facts.get(r).and_then(|f| f.node.as_deref()) else {
            warnings.push(format!("{r} has no fact in Prime yet: no edge to it"));
            continue;
        };
        let edge = json!({
            "source": source,
            "target": as_ref(target),
            "relation": "requires",
        });
        if let Err(e) = prime.call("prime_add_edge", edge) {
            warnings.push(format!("edge {} requires {r} not added: {e:#}", d.name));
        }
    }
    Ok(warnings)
}

/// A Prime node id cos will put in an edge: 1 to 64 ASCII letters, digits
/// and `-` (Prime's ids are UUIDs).
pub fn node_id_ok(id: &str) -> bool {
    (1..=64).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

fn as_ref(id: &str) -> String {
    if id.starts_with("node:") {
        id.to_owned()
    } else {
        format!("node:{NODE_TYPE}:{id}")
    }
}

fn node_ref(added: &Value) -> Option<String> {
    let prefix = format!("node:{NODE_TYPE}:");
    ["entity_id", "id", "node_id"]
        .iter()
        .find_map(|k| added[*k].as_str())
        .filter(|id| node_id_ok(id.strip_prefix(prefix.as_str()).unwrap_or(id)))
        .map(as_ref)
}
