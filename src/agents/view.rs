//! Agents rendered for the terminal (`cos agents ls/show/tail`) and for
//! `cos dash` (`/agents`).

use std::fmt::Write as _;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::Path;

use super::{Harness, Spec, State, Status, harness, ledger};

pub fn table(agents: &[(Spec, State)]) -> String {
    if agents.is_empty() {
        return "no agents yet — `cos agents spawn --from brief --item 1`\n".into();
    }
    let mut s = format!(
        "{:<21} {:<9} {:<9} {:<6} {:>5} {:>7}  {}\n",
        "ID", "STATUS", "VERDICT", "HARN", "TOOLS", "COST", "TITLE / LAST"
    );
    for (spec, st) in agents {
        let _ = writeln!(
            s,
            "{:<21} {:<9} {:<9} {:<6} {:>5} {:>7}  {}\n{:>63}{}",
            spec.id,
            st.status.to_string(),
            verdict_word(st),
            harness_name(spec.harness),
            st.tool_calls,
            cost(st),
            spec.title,
            "",
            if st.last.is_empty() { "—" } else { &st.last },
        );
    }
    s
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
    let _ = writeln!(
        s,
        "budget   ${:.2}   deadline {}s   tools {}   cost {}",
        spec.budget_usd,
        spec.deadline_secs,
        st.tool_calls,
        cost(st)
    );
    if let Some(t) = st.tokens {
        let _ = writeln!(s, "tokens   {t}");
    }
    for (k, v) in [("started", &st.started), ("ended", &st.ended)] {
        if let Some(v) = v {
            let _ = writeln!(s, "{k:<8} {v}");
        }
    }
    if let Some(e) = &st.error {
        let _ = writeln!(s, "error    {e}");
    }
    if let Some(session) = &st.session {
        let resume = match spec.harness {
            Harness::Claude => format!("claude --resume {session}"),
            Harness::Codex => format!("codex resume {session}"),
        };
        let _ = writeln!(s, "take over: cd {} && {resume}", spec.workdir.display());
    }
    let c = &spec.contract;
    if let Some(d) = &c.done_when {
        let _ = writeln!(s, "done when {d}");
    }
    if !c.scope.is_empty() {
        let _ = writeln!(s, "scope    {}", c.scope.join(", "));
    }
    for f in &st.flags {
        let _ = writeln!(s, "TROUBLE  {}: {}", f.kind, f.detail);
    }
    if st.flags.is_empty() && !st.flagged.is_empty() {
        let kinds: Vec<String> = st.flagged.iter().map(ToString::to_string).collect();
        let _ = writeln!(s, "flagged  {} during the run", kinds.join(", "));
    }
    match &st.verdict {
        Some(v) => {
            let _ = writeln!(
                s,
                "\nverdict  {}{}",
                v.outcome,
                if v.overclaimed {
                    "  (it claimed success; the checks disagree)"
                } else {
                    ""
                }
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
        None => {}
    }
    if !st.summary.is_empty() {
        let _ = writeln!(
            s,
            "\nits own summary (not evidence):\n{}",
            st.summary.trim()
        );
    }
    s
}

/// Activity lines from `events.jsonl` starting at byte `from`; returns them
/// and the offset to continue from.
pub fn activity(events: &Path, harness: Harness, from: u64) -> (Vec<String>, u64) {
    let Ok(mut f) = std::fs::File::open(events) else {
        return (Vec::new(), from);
    };
    if f.seek(SeekFrom::Start(from)).is_err() {
        return (Vec::new(), from);
    }
    let mut r = BufReader::new(f);
    let (mut out, mut at, mut buf) = (Vec::new(), from, String::new());
    while let Ok(n) = r.read_line(&mut buf) {
        // A line without its newline is still being written; read it next time.
        if n == 0 || !buf.ends_with('\n') {
            break;
        }
        at += n as u64;
        let t = harness::read(harness, buf.trim_end());
        if let Some(last) = t.last {
            out.push(last);
        }
        if let Some(f) = t.failed {
            out.push(format!("FAILED {f}"));
        }
        buf.clear();
    }
    (out, at)
}

/// `/agents` for `cos dash`: a self-refreshing table, nothing to install.
pub fn html(agents: &[(Spec, State)], scores: &[ledger::Score]) -> String {
    let mut rows = String::new();
    for (spec, st) in agents {
        let _ = write!(
            rows,
            "<tr class=\"{cls}\"><td><code>{id}</code></td><td>{status}</td><td>{verdict}</td><td>{harn}</td>\
             <td class=n>{tools}</td><td class=n>{cost}</td>\
             <td><b>{title}</b><br><span class=dim>{last}</span></td>\
             <td class=dim><code>{dir}</code></td></tr>",
            cls = if st.status.terminal() { "done" } else { "live" },
            id = esc(&spec.id),
            status = st.status,
            verdict = verdict_word(st),
            harn = harness_name(spec.harness),
            tools = st.tool_calls,
            cost = esc(&cost(st)),
            title = esc(&spec.title),
            last = esc(st.error.as_deref().unwrap_or(&st.last)),
            dir = esc(&spec.workdir.to_string_lossy()),
        );
    }
    let live = agents.iter().filter(|(_, s)| !s.status.terminal()).count();
    format!(
        r#"<!doctype html><html lang=en><meta charset=utf-8>
<meta name=viewport content="width=device-width,initial-scale=1">
<meta http-equiv=refresh content=3><title>cos agents</title>
<style>
:root{{--bg:#fbfaf7;--fg:#1d1c1a;--dim:#6b6862;--line:#e4e1da;--live:#0a7d4f}}
@media (prefers-color-scheme:dark){{:root{{--bg:#151513;--fg:#ecebe6;--dim:#9a978f;--line:#2c2b28;--live:#4cc38a}}}}
body{{margin:0;padding:24px 16px;background:var(--bg);color:var(--fg);font:14px/1.45 ui-sans-serif,system-ui,sans-serif}}
main{{max-width:1100px;margin:0 auto}} h1{{font-size:20px;margin:0 0 4px}}
p{{color:var(--dim);margin:0 0 16px}} .wrap{{overflow-x:auto}}
table{{border-collapse:collapse;width:100%}} td,th{{padding:8px;border-bottom:1px solid var(--line);text-align:left;vertical-align:top}}
th{{font-weight:600;color:var(--dim);font-size:12px}} .n{{text-align:right;font-variant-numeric:tabular-nums}}
.dim{{color:var(--dim)}} tr.live td:nth-child(2){{color:var(--live);font-weight:600}} code{{font-size:12px}}
</style>
<main><h1>Agents</h1><p>{live} running · {total} total · refreshes every 3 s · <a href="/">report</a></p>
<div class=wrap><table><tr><th>ID</th><th>Status</th><th>Verdict</th><th>Harness</th><th class=n>Tools</th><th class=n>Cost</th><th>Task / latest</th><th>Workdir</th></tr>
{rows}</table></div>{score}</main>"#,
        total = agents.len(),
        score = scorecard_html(scores),
    )
}

/// The verdict once judged; while running, the trouble it shows, if any.
fn verdict_word(st: &State) -> String {
    match &st.verdict {
        Some(v) if v.overclaimed => format!("{}!", v.outcome),
        Some(v) => v.outcome.to_string(),
        None if st.status.terminal() => "judging".into(),
        None => match st.flags.first() {
            Some(f) => f.kind.to_string(),
            None => "—".into(),
        },
    }
}

/// `cos agents scorecard`: how agents hired on the same terms did.
pub fn scorecard(scores: &[ledger::Score]) -> String {
    if scores.is_empty() {
        return "no judged agents yet\n".into();
    }
    let mut s = format!(
        "{:<6} {:<18} {:<20} {:>4} {:>8} {:>6} {:>6} {:>6} {:>7} {:>6}\n",
        "HARN", "MODEL", "REPO", "RUNS", "ACCEPTED", "NEEDS", "REJECT", "OVER", "COST", "SECS"
    );
    for sc in scores {
        let _ = writeln!(
            s,
            "{:<6} {:<18} {:<20} {:>4} {:>8} {:>6} {:>6} {:>6} {:>7} {:>6}",
            harness_name(sc.harness),
            sc.model.as_deref().unwrap_or("default"),
            sc.repo.as_deref().unwrap_or("(scratch)"),
            sc.runs,
            sc.accepted,
            sc.needs_you,
            sc.rejected,
            sc.overclaimed,
            sc.median_cost_usd
                .map(|c| format!("${c:.2}"))
                .unwrap_or_else(|| "—".into()),
            sc.median_secs
                .map(|t| t.to_string())
                .unwrap_or_else(|| "—".into()),
        );
    }
    s.push_str("COST and SECS are medians; OVER counts agents that claimed success while a check failed.\n");
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
            cost = sc
                .median_cost_usd
                .map(|c| format!("${c:.2}"))
                .unwrap_or_else(|| "—".into()),
            secs = sc
                .median_secs
                .map(|t| format!("{t}s"))
                .unwrap_or_else(|| "—".into()),
        );
    }
    format!(
        "<h2>Scorecard</h2><p>Every judged agent, from the ledger; cost and time are medians; \
         overclaimed = claimed success while a check failed.</p>\
         <div class=wrap><table><tr><th>Harness</th><th>Model</th><th>Repo</th><th class=n>Runs</th>\
         <th class=n>Accepted</th><th class=n>Needs you</th><th class=n>Rejected</th>\
         <th class=n>Overclaimed</th><th class=n>Cost</th><th class=n>Time</th></tr>{rows}</table></div>"
    )
}

fn harness_name(h: Harness) -> &'static str {
    h.program()
}

fn cost(st: &State) -> String {
    match st.cost_usd {
        Some(c) => format!("${c:.2}"),
        None if st.status == Status::Running => "…".into(),
        None => "—".into(),
    }
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
