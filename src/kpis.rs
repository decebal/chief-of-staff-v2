//! KPI tiles and trend cards: the workspace's KPIs with the points the brief
//! recorded (`history::kpi_points`). The Today week section shows them all;
//! the Plan tab shows each goal's own, through `by_goal`.

use std::fmt::Write as _;

use serde_json::{Map, Value};

use crate::today::{self, Ctx};

/// The KPIs a tile shows: not retired, not proposed, and in the active score.
fn active(kpis: &Value) -> Vec<&Value> {
    kpis.as_array()
        .into_iter()
        .flatten()
        .filter(|k| {
            !matches!(k["status"].as_str(), Some("retired" | "proposed"))
                && !matches!(k["targetBasis"].as_str(), Some("retired" | "proposed"))
                && k["excludeFromActiveScore"] != true
        })
        .collect()
}

/// The Today section: one tile per active KPI; empty when there are none.
pub fn section(data: &Value) -> String {
    let kpis = active(&data["workspace"]["kpis"]);
    if kpis.is_empty() {
        return String::new();
    }
    let cx = Ctx::new(data);
    let points = points(data);
    let mut s = String::from(
        r#"<section class="kit-tile-group" data-section="kpis" aria-labelledby="kpis-h"><header class="kit-tile-group__head"><h3 class="kit-tile-group__label" id="kpis-h">KPIs</h3></header><div class="kit-tile-group__grid">"#,
    );
    for (i, k) in kpis.iter().enumerate() {
        tile(&cx, k, &points, &format!("kpi-{i}"), &mut s);
    }
    s.push_str("</div></section>");
    s
}

/// Each goal's tiles, keyed by goal id, for the Plan tab's goal rows.
pub fn by_goal(data: &Value) -> Value {
    let cx = Ctx::new(data);
    let points = points(data);
    let mut out = Map::new();
    for (i, k) in active(&data["workspace"]["kpis"]).into_iter().enumerate() {
        let Some(goal) = k["goalId"].as_str() else {
            continue;
        };
        let html = out
            .entry(goal.to_owned())
            .or_insert_with(|| Value::String(String::new()));
        if let Value::String(s) = html {
            tile(&cx, k, &points, &format!("plan-{i}"), s);
        }
    }
    Value::Object(out)
}

/// The recorded points, oldest evidence time first.
fn points(data: &Value) -> Vec<Value> {
    let mut v: Vec<Value> = data["kpi_points"].as_array().cloned().unwrap_or_default();
    v.sort_by(|a, b| a["evidenceAt"].as_str().cmp(&b["evidenceAt"].as_str()));
    v
}

/// A number as a reader writes it: no decimals when it is whole.
fn num(v: f64) -> String {
    if v.fract() == 0.0 {
        format!("{v:.0}")
    } else {
        format!("{v:.1}")
    }
}

fn tile(cx: &Ctx, k: &Value, points: &[Value], id: &str, s: &mut String) {
    let name = k["name"].as_str().unwrap_or_default();
    let unit = k["unit"].as_str().unwrap_or_default();
    let current = k["current"].as_f64();
    let baseline = k["baseline"].as_f64().unwrap_or(0.0);
    let target = k["target"].as_f64();
    let series: Vec<(&str, f64)> = points
        .iter()
        .filter(|p| p["kpi"].as_str() == Some(name))
        .filter_map(|p| Some((p["evidenceAt"].as_str()?, p["value"].as_f64()?)))
        .collect();
    let lower_better = target.is_some_and(|t| t < baseline);
    let change = (series.len() >= 2).then(|| series[series.len() - 1].1 - series[0].1);
    let (tone, words) = match change {
        None if series.len() == 1 => ("muted", "one point so far".to_owned()),
        None => ("muted", "no points yet".to_owned()),
        Some(0.0) => ("muted", "no change".to_owned()),
        Some(c) => {
            let up = c > 0.0;
            let tone = if up != lower_better { "ok" } else { "warn" };
            let sign = if up { "+" } else { "-" };
            let dir = if up { "up" } else { "down" };
            (tone, format!("{dir} {sign}{}", num(c.abs())))
        }
    };
    let value = current.map_or("not measured".to_owned(), num);
    let pct = match (current, target) {
        (Some(c), Some(t)) if t != baseline => {
            Some(((c - baseline) / (t - baseline) * 100.0).clamp(0.0, 100.0))
        }
        _ => None,
    };
    // The page's script rolls the digits from here to `value` once per load.
    let from = match (current, series.len()) {
        (Some(_), n) if n >= 2 => format!(r#" data-from="{}""#, num(series[n - 2].1)),
        _ => String::new(),
    };
    let _ = write!(
        s,
        r#"<article class="kit-tile" data-tone="{tone}"><header class="kit-tile__top"><span class="kit-tile__label">{}</span></header><div class="kit-tile__body"><p class="kit-tile__value"><span class="num kit-tile__v" aria-live="polite"{from}>{}</span> <span class="kit-tile__unit">{}</span></p><p class="kit-tile__change">{}</p>"#,
        today::esc(name),
        today::esc(&value),
        today::esc(unit),
        today::esc(&words),
    );
    if let Some(pct) = pct {
        let _ = write!(
            s,
            r#"<div class="kit-tile__meter" role="meter" aria-label="{} toward target" aria-valuemin="0" aria-valuemax="100" aria-valuenow="{pct:.0}"><span style="width: {pct:.0}%"></span></div>"#,
            today::esc(name)
        );
    }
    if series.len() >= 2 {
        s.push_str(&spark(&series));
        s.push_str(&trend(cx, name, unit, &series, id));
    }
    s.push_str("</div></article>");
}

fn spark(series: &[(&str, f64)]) -> String {
    let values: Vec<f64> = series.iter().map(|(_, v)| *v).collect();
    let pts = plot(&values, 40.0, 16.0)
        .iter()
        .map(|(x, y)| format!("{x:.1},{y:.1}"))
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        r#"<svg class="kit-tile__spark" viewBox="0 0 40 16" preserveAspectRatio="none" aria-hidden="true"><polyline points="{pts}"/></svg>"#
    )
}

/// Points scaled into a `w` by `h` box, first at the left edge.
fn plot(values: &[f64], w: f64, h: f64) -> Vec<(f64, f64)> {
    let min = values.iter().copied().fold(f64::INFINITY, f64::min);
    let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let span = max - min;
    let n = values.len().saturating_sub(1).max(1) as f64;
    values
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let y = if span > 0.0 {
                h - (v - min) / span * h
            } else {
                h / 2.0
            };
            (i as f64 / n * w, y)
        })
        .collect()
}

/// The trend card: a button opens it in place; its plot steps with the arrow
/// keys, and its readout is a polite live region (report.html's script).
fn trend(cx: &Ctx, name: &str, unit: &str, series: &[(&str, f64)], id: &str) -> String {
    let values: Vec<f64> = series.iter().map(|(_, v)| *v).collect();
    let pts = plot(&values, 600.0, 160.0);
    let last = series.len() - 1;
    let label = |(at, v): &(&str, f64)| format!("{} {} · {}", num(*v), unit, cx.stamp(at));
    let marks: String = series
        .iter()
        .zip(&pts)
        .enumerate()
        .map(|(i, (point, (x, y)))| {
            format!(
                r#"<circle class="kit-metric-card__marker" data-index="{i}" data-label="{}" cx="{x:.1}" cy="{y:.1}" r="4"/>"#,
                today::esc(&label(point))
            )
        })
        .collect();
    let line = pts
        .iter()
        .map(|(x, y)| format!("{x:.1},{y:.1}"))
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        r#"<button type="button" class="kit-tile__open" data-kpi-open="{id}" aria-expanded="false" aria-controls="{id}-trend">Trend</button><div class="kit-metric-card" id="{id}-trend" hidden><p class="kit-metric-card__name">{}</p><div class="kit-metric-card__plot" tabindex="0" role="group" aria-roledescription="chart" aria-label="{} over time" data-index="{last}"><svg viewBox="0 0 600 160" preserveAspectRatio="none" aria-hidden="true"><polyline class="kit-metric-card__line" points="{line}"/>{marks}</svg></div><output class="kit-metric-card__readout" aria-live="polite">{}</output></div>"#,
        today::esc(name),
        today::esc(name),
        today::esc(&label(&series[last])),
    )
}
