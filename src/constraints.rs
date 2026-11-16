//! The limiting factors on the Today view: what currently stops shipping, each
//! with its evidence, its owner and the move that would loosen it. The user, or
//! an agent on their behalf, keeps `context/constraints.json`; `cos` validates
//! and renders it, ordered so the factors still limiting come first.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::design::icon;
use crate::today::{esc, prose, ref_chip};

pub const VERSION: u32 = 1;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Register {
    pub version: u32,
    /// When it was last judged, in words: `Mon 16 Nov`.
    pub as_of: String,
    /// The question the register answers, e.g. "why are we not released to all".
    pub question: String,
    pub factors: Vec<Factor>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Factor {
    pub id: String,
    pub title: String,
    /// `release`, `security`, `review`, `delivery`, `dependency`, `process`.
    pub area: String,
    pub status: Status,
    pub evidence: String,
    pub recommendation: String,
    pub owner: String,
    /// rock, sand or water: how well the way through is understood.
    pub confidence: String,
    #[serde(default)]
    pub tickets: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// Stops a release or a live project now.
    Limiting,
    /// Will limit soon if nothing changes.
    Watch,
    /// No longer limiting; kept so the register shows what moved.
    Cleared,
}

pub fn path(home: &Path) -> PathBuf {
    home.join("context/constraints.json")
}

/// A missing file is no register; a broken one is an error naming the field.
pub fn load(home: &Path) -> Result<Option<Register>> {
    let p = path(home);
    match std::fs::read_to_string(&p) {
        Ok(text) => parse(&text)
            .map(Some)
            .with_context(|| format!("{}", p.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("read {}", p.display())),
    }
}

pub fn parse(text: &str) -> Result<Register> {
    let r: Register = serde_json::from_str(text).context("not a constraints register")?;
    ensure!(
        r.version == VERSION,
        "`version` is {}; this cos reads version {VERSION}",
        r.version
    );
    Ok(r)
}

pub fn view(home: &Path) -> Value {
    match load(home) {
        Ok(Some(r)) => serde_json::to_value(r).unwrap_or(Value::Null),
        Ok(None) => Value::Null,
        Err(e) => json!({ "error": format!("{e:#}") }),
    }
}

/// The register as the week's judgement reads it, with the one thing the
/// brief may do about it: propose a change as a decision. The file stays the
/// user's; the brief never rewrites it.
pub fn prompt_block(home: &Path) -> String {
    let r = match load(home) {
        Ok(Some(r)) => r,
        Ok(None) => return String::new(),
        Err(e) => {
            return format!(
                "\n\n### context/constraints.json\n\n(could not be read: {e:#}; say so in gaps)\n"
            );
        }
    };
    let mut s = format!(
        "\n\n### context/constraints.json (the user's limiting factors, judged {}: {})\n\n",
        r.as_of, r.question
    );
    for f in r.factors.iter().filter(|f| f.status != Status::Cleared) {
        let status = serde_json::to_value(f.status)
            .ok()
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_default();
        s.push_str(&format!(
            "- {} [{status}, {}, owner {}] {}. Evidence: {} Move: {}",
            f.id, f.area, f.owner, f.title, f.evidence, f.recommendation
        ));
        if !f.tickets.is_empty() {
            s.push_str(&format!(" Tickets: {}.", f.tickets.join(", ")));
        }
        s.push('\n');
    }
    s.push_str(
        "\nRe-judge each factor above against this week's facts and tickets. Where its evidence \
         no longer holds (a ticket done, a PR merged, a flag set) or the facts show a new \
         blocker to a release, add one `week.decisions` entry naming the factor id, the change \
         (status, evidence or move) and the fact that shows it. Leave factors that did not move \
         out of the brief. Do not rewrite the file; the user does.\n",
    );
    s
}

fn status_name(s: Status) -> &'static str {
    match s {
        Status::Limiting => "limiting",
        Status::Watch => "to watch",
        Status::Cleared => "cleared",
    }
}

/// One card: the id and the status, the title at full width, the tier and
/// the kind, the owner; then the move that loosens it, the tickets, and the
/// evidence folded behind it.
fn card(f: &Factor) -> String {
    let status = serde_json::to_value(f.status)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default();
    let status_icon = match f.status {
        Status::Limiting => "octagon-x",
        Status::Watch => "eye",
        Status::Cleared => "circle-check",
    };
    let tier_icon = match f.confidence.as_str() {
        "rock" => "mountain",
        "sand" => "hourglass",
        "water" => "waves",
        _ => "circle-dashed",
    };
    let tickets = if f.tickets.is_empty() {
        String::new()
    } else {
        format!(
            r#"<div class="chips lf-refs">{}</div>"#,
            f.tickets.iter().map(|t| ref_chip(t)).collect::<String>()
        )
    };
    format!(
        r#"<article class="lf" data-status="{status}"><header class="lf-head"><span class="lf-id">{id}</span><span class="lf-status" data-status="{status}">{si}{sn}</span><span class="lf-owner" title="owner">{user}{owner}</span></header><h4 class="lf-title">{title}</h4><div class="chips"><span class="lf-chip" title="how well the way through is understood">{ti}{tier}</span><span class="lf-chip">{tag}{area}</span></div><p class="lf-loosen">{key}<span><b>To loosen it:</b> {rec}</span></p>{tickets}<details class="lf-ev"><summary>{ev}Evidence</summary><p>{evidence}</p></details></article>"#,
        id = esc(&f.id),
        si = icon(status_icon),
        sn = status_name(f.status),
        user = icon("user"),
        owner = esc(&f.owner),
        title = esc(&f.title),
        ti = icon(tier_icon),
        tier = esc(&f.confidence),
        tag = icon("tag"),
        area = esc(&f.area),
        key = icon("lock-open"),
        rec = prose(&f.recommendation),
        ev = icon("file-search"),
        evidence = prose(&f.evidence),
    )
}

/// Collapsed like the other secondary parts; its summary counts the factors
/// still limiting, so the count reads without opening it. Inside, a card per
/// factor, limiting first; those to watch and those cleared fold under their
/// count.
pub fn section(data: &Value) -> String {
    let v = &data["constraints"];
    if v.is_null() {
        return String::new();
    }
    if let Some(e) = v["error"].as_str() {
        return format!(
            r#"<p class="quiet" id="constraints" data-section="constraints">The limiting factors could not be read: {}</p>"#,
            esc(e)
        );
    }
    let Ok(mut r) = serde_json::from_value::<Register>(v.clone()) else {
        return String::new();
    };
    r.factors.sort_by_key(|f| f.status);
    let limiting = r
        .factors
        .iter()
        .filter(|f| f.status == Status::Limiting)
        .count();
    let watch = r
        .factors
        .iter()
        .filter(|f| f.status == Status::Watch)
        .count();
    let mut s = String::new();
    let _ = write!(
        s,
        r#"<details class="fold" id="constraints" data-section="constraints"><summary><span class="fold-t">Limiting factors</span><span class="fold-s chips"><span class="lf-count" data-status="limiting">{}{limiting} limiting</span><span class="lf-count" data-status="watch">{}{watch} to watch</span><span class="lf-count">{}judged {}</span></span></summary><div class="fold-body"><p class="quiet">{}</p>"#,
        icon("octagon-x"),
        icon("eye"),
        icon("calendar"),
        esc(&r.as_of),
        esc(&r.question)
    );
    let of = |st: Status| r.factors.iter().filter(move |f| f.status == st);
    s.push_str(r#"<div class="lf-list">"#);
    for f in of(Status::Limiting) {
        s.push_str(&card(f));
    }
    s.push_str("</div>");
    for (st, glyph) in [(Status::Watch, "eye"), (Status::Cleared, "circle-check")] {
        let n = of(st).count();
        if n == 0 {
            continue;
        }
        let _ = write!(
            s,
            r#"<details class="lf-more"><summary>{}{n} {}</summary><div class="lf-list">"#,
            icon(glyph),
            status_name(st)
        );
        for f in of(st) {
            s.push_str(&card(f));
        }
        s.push_str("</div></details>");
    }
    s.push_str("</div></details>");
    s
}
