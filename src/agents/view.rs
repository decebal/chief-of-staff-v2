//! Agents rendered for the terminal (`cos agents ls/show`) and for
//! `cos dash` (`/agents`).

use std::fmt::Write as _;

use super::{Harness, Spec, State, ledger};

pub fn table(agents: &[(Spec, State)]) -> String {
    if agents.is_empty() {
        return "no agents yet — `cos agents spawn --from brief --item 1`\n".into();
    }
    let mut s = format!(
        "{:<21} {:<9} {:<22} {:<9} {:<6}  {}\n",
        "ID", "STATUS", "IN HERDR", "VERDICT", "HARN", "TITLE / NOTE"
    );
    for (spec, st) in agents {
        let _ = writeln!(
            s,
            "{:<21} {:<9} {:<22} {:<9} {:<6}  {}\n{:>71}{}",
            spec.id,
            st.status.to_string(),
            herdr_word(spec, st),
            verdict_word(st),
            harness_name(spec.harness),
            spec.title,
            "",
            st.error.as_deref().unwrap_or("—"),
        );
    }
    s
}

/// What Herdr last said about the run, in its own words.
pub fn herdr_word(spec: &Spec, st: &State) -> String {
    if spec.legacy() {
        return "not in Herdr".into();
    }
    match &st.herdr {
        None => "not yet".into(),
        Some(h) if !h.live => format!("ended ({})", h.reason.as_deref().unwrap_or("exited")),
        Some(h) => match (h.word.as_deref(), h.kind.as_deref()) {
            (Some("blocked"), Some(k)) => format!("blocked: {k}"),
            (Some(w), _) => format!("{w} · turn {}", h.turn),
            (None, _) => "no status yet".into(),
        },
    }
}

/// The hires Herdr says wait on you, for Today: `{ id, title, kind,
/// terminal, address }`, oldest first.
pub fn waiting_in_herdr(agents: &[(Spec, State)]) -> Vec<serde_json::Value> {
    let mut out: Vec<serde_json::Value> = agents
        .iter()
        .filter_map(|(spec, st)| {
            let h = st.herdr.as_ref()?;
            let kind = h.blocked()?;
            Some(serde_json::json!({
                "id": spec.id,
                "title": spec.title,
                "kind": kind,
                "terminal": spec.terminal,
                "address": h.address,
            }))
        })
        .collect();
    out.reverse();
    out
}

pub fn show(spec: &Spec, st: &State) -> String {
    let mut s = String::new();
    let _ = writeln!(
        s,
        "{}  {}  ({})",
        spec.id,
        st.status,
        harness_name(spec.harness)
    );
    let _ = writeln!(s, "title    {}", spec.title);
    let _ = writeln!(s, "source   {}", spec.source);
    let _ = writeln!(s, "workdir  {}", spec.workdir.display());
    if let Some(repo) = &spec.repo {
        let _ = writeln!(
            s,
            "repo     {}{}",
            repo.display(),
            spec.base
                .as_deref()
                .map(|b| format!(" @ {}", &b[..b.len().min(12)]))
                .unwrap_or_default()
        );
    }
    let _ = writeln!(s, "deadline {}s", spec.deadline_secs);
    match (&spec.terminal, &st.herdr) {
        (None, _) => {
            let _ = writeln!(s, "herdr    {}", super::LEGACY);
        }
        (Some(name), Some(h)) => {
            let _ = writeln!(
                s,
                "herdr    {name} ({})  {}",
                h.address,
                herdr_word(spec, st)
            );
            if h.live {
                let _ = writeln!(
                    s,
                    "talk     hc attach {name}   ·   cos agents say {} \"…\"   ·   cos agents done {}",
                    spec.id, spec.id
                );
            }
            if let Some(why) = h.unsent() {
                let _ = writeln!(s, "UNSENT   its first message did not go in: {why}");
            }
        }
        (Some(name), None) => {
            let _ = writeln!(s, "herdr    {name}, not started yet");
        }
    }
    for (k, v) in [("started", &st.started), ("ended", &st.ended)] {
        if let Some(v) = v {
            let _ = writeln!(s, "{k:<8} {v}");
        }
    }
    if let Some(e) = &st.error {
        let _ = writeln!(s, "error    {e}");
    }
    let c = &spec.contract;
    if let Some(d) = &c.done_when {
        let _ = writeln!(s, "done when {d}");
    }
    if let Some(f) = &c.finish_line {
        let _ = writeln!(s, "yours    {f}");
    }
    if let Some(r) = &st.recorded {
        let _ = writeln!(s, "result   {} (recorded by you {})", r.outcome, r.at);
        if !r.notes.trim().is_empty() {
            let _ = writeln!(s, "         {}", r.notes.trim());
        }
        for e in &r.evidence {
            let _ = writeln!(s, "         evidence: {e}");
        }
    }
    if !c.scope.is_empty() {
        let _ = writeln!(s, "scope    {}", c.scope.join(", "));
    }
    match &st.verdict {
        Some(v) => {
            let _ = writeln!(
                s,
                "\nverdict  {}{}{}",
                v.outcome,
                if v.overclaimed {
                    "  (it claimed success; the checks disagree)"
                } else {
                    ""
                },
                st.judged_on
                    .as_deref()
                    .map(|w| format!("  (judged on: {w})"))
                    .unwrap_or_default()
            );
            for r in &v.reasons {
                let _ = writeln!(s, "  - {r}");
            }
            for k in &v.checks {
                let _ = writeln!(s, "  {} {}", if k.passed { "ok  " } else { "FAIL" }, k.cmd);
                if !k.passed && !k.tail.is_empty() {
                    for line in k.tail.lines() {
                        let _ = writeln!(s, "         {line}");
                    }
                }
            }
            let _ = writeln!(s, "  changed  {} file(s)", v.changed.len());
            for f in &v.changed {
                let mark = if v.out_of_scope.contains(f) {
                    "  out of scope"
                } else {
                    ""
                };
                let _ = writeln!(s, "    {f}{mark}");
            }
        }
        None if st.status.terminal() => {
            let _ = writeln!(s, "\nverdict  judging…");
        }
        None => {
            let _ = writeln!(
                s,
                "\nverdict  judged once, when Herdr says it is done or it ends"
            );
        }
    }
    if let Some(summary) = st.summary.as_deref().filter(|x| !x.trim().is_empty()) {
        let _ = writeln!(s, "\nits own summary (not evidence):\n{}", summary.trim());
    }
    s
}

/// `/agents` for `cos dash`: a self-refreshing table, nothing to install.
pub fn html(agents: &[(Spec, State)], scores: &[ledger::Score]) -> String {
    let mut rows = String::new();
    for (spec, st) in agents {
        let _ = write!(
            rows,
            "<tr class=\"{cls}\"><td><code>{id}</code></td><td>{status}</td><td>{verdict}</td>\
             <td>{herdr}</td><td>{harn}</td>\
             <td><b>{title}</b><br><span class=dim>{note}</span></td>\
             <td class=dim><code>{dir}</code></td></tr>",
            cls = if st.status.terminal() { "done" } else { "live" },
            id = esc(&spec.id),
            status = st.status,
            verdict = verdict_word(st),
            herdr = esc(&herdr_word(spec, st)),
            harn = harness_name(spec.harness),
            title = esc(&spec.title),
            note = esc(st.error.as_deref().unwrap_or("")),
            dir = esc(&spec.workdir.to_string_lossy()),
        );
    }
    let live = agents.iter().filter(|(_, s)| !s.status.terminal()).count();
    crate::design::inline(include_str!("view.html"))
        .expect("view.html carries the token slot")
        .replacen("<!--__LIVE__-->", &live.to_string(), 1)
        .replacen("<!--__TOTAL__-->", &agents.len().to_string(), 1)
        .replacen("<!--__ROWS__-->", &rows, 1)
        .replacen("<!--__SCORE__-->", &scorecard_html(scores), 1)
}

fn verdict_word(st: &State) -> String {
    match &st.verdict {
        Some(v) if v.overclaimed => format!("{}!", v.outcome),
        Some(v) => v.outcome.to_string(),
        None if st.status.terminal() => "judging".into(),
        None => "—".into(),
    }
}

/// A median cost, or why there is none.
pub fn cost_word(median: Option<f64>, unmeasured: usize) -> String {
    match median {
        Some(c) => format!("${c:.2}"),
        None if unmeasured > 0 => "not measured in Herdr".into(),
        None => "—".into(),
    }
}

/// `cos agents scorecard`: how agents hired on the same terms did.
pub fn scorecard(scores: &[ledger::Score]) -> String {
    if scores.is_empty() {
        return "no judged agents yet\n".into();
    }
    let mut s = format!(
        "{:<13} {:<6} {:<18} {:<20} {:>4} {:>8} {:>6} {:>6} {:>6} {:>21} {:>6}\n",
        "ROLE",
        "HARN",
        "MODEL",
        "REPO",
        "RUNS",
        "ACCEPTED",
        "NEEDS",
        "REJECT",
        "OVER",
        "COST",
        "SECS"
    );
    for sc in scores {
        let _ = writeln!(
            s,
            "{:<13} {:<6} {:<18} {:<20} {:>4} {:>8} {:>6} {:>6} {:>6} {:>21} {:>6}",
            sc.role.as_deref().unwrap_or("unassigned"),
            harness_name(sc.harness),
            sc.model.as_deref().unwrap_or("default"),
            sc.repo.as_deref().unwrap_or("(scratch)"),
            sc.runs,
            sc.accepted,
            sc.needs_you,
            sc.rejected,
            sc.overclaimed,
            cost_word(sc.median_cost_usd, sc.unmeasured),
            sc.median_secs
                .map(|t| t.to_string())
                .unwrap_or_else(|| "—".into()),
        );
    }
    s.push_str("COST and SECS are medians; OVER counts agents that claimed success while a check failed; runs in Herdr have no cost measured.\n");
    s
}

fn scorecard_html(scores: &[ledger::Score]) -> String {
    if scores.is_empty() {
        return String::new();
    }
    let mut rows = String::new();
    for sc in scores {
        let _ = write!(
            rows,
            "<tr><td>{harn}</td><td>{model}</td><td>{repo}</td><td class=n>{runs}</td>\
             <td class=n>{acc}</td><td class=n>{needs}</td><td class=n>{rej}</td>\
             <td class=n>{over}</td><td class=n>{cost}</td><td class=n>{secs}</td></tr>",
            harn = harness_name(sc.harness),
            model = esc(sc.model.as_deref().unwrap_or("default")),
            repo = esc(sc.repo.as_deref().unwrap_or("(scratch)")),
            runs = sc.runs,
            acc = sc.accepted,
            needs = sc.needs_you,
            rej = sc.rejected,
            over = sc.overclaimed,
            cost = esc(&cost_word(sc.median_cost_usd, sc.unmeasured)),
            secs = sc
                .median_secs
                .map(|t| format!("{t}s"))
                .unwrap_or_else(|| "—".into()),
        );
    }
    format!(
        "<h2>Scorecard</h2><p>Every judged agent, from the ledger; cost and time are medians; \
         overclaimed = claimed success while a check failed; runs in Herdr have no cost measured.</p>\
         <div class=wrap><table><tr><th>Harness</th><th>Model</th><th>Repo</th><th class=n>Runs</th>\
         <th class=n>Accepted</th><th class=n>Needs you</th><th class=n>Rejected</th>\
         <th class=n>Overclaimed</th><th class=n>Cost</th><th class=n>Time</th></tr>{rows}</table></div>"
    )
}

fn harness_name(h: Harness) -> &'static str {
    h.program()
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
