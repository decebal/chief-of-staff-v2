//! The design gates. Every page a user sees (the desktop window, the Today
//! report, the docs room, the MCP app view) styles through one token file,
//! `apps/desktop/ui/tokens.css`: no colour, font, type size, radius or pixel
//! length of its own. The token file in turn defines every colour for light
//! and dark, and its text colours pass WCAG AA on the surfaces they sit on.

use std::collections::BTreeSet;
use std::path::Path;

const ROOT: &str = env!("CARGO_MANIFEST_DIR");
const TOKEN_FILE: &str = "apps/desktop/ui/tokens.css";
/// Pages `cos` serves itself; each takes the tokens through `design::SLOT`.
const SERVED: &[&str] = &[
    "src/report.html",
    "src/dashboard.html",
    "src/docs.html",
    "src/agents/view.html",
    "src/flows.html",
];
const UI: &str = "apps/desktop/ui";

/// Every UI source the gates read, by path relative to the crate.
fn sources() -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(Path::new(ROOT).join(UI))
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && matches!(
                    p.extension().and_then(|e| e.to_str()),
                    Some("html" | "css" | "js")
                )
        })
        .map(|p| format!("{UI}/{}", p.file_name().unwrap().to_string_lossy()))
        .filter(|rel| rel != TOKEN_FILE)
        .chain(SERVED.iter().map(|s| s.to_string()))
        .chain([ART_FILE.to_string()])
        .collect();
    out.sort();
    out
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(Path::new(ROOT).join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

/// `text` with comments blanked (newlines kept, so offsets stay on their
/// line): `/* */`, `<!-- -->`, and `//` comments that start a line or follow
/// a space.
fn strip_comments(text: &str) -> String {
    let b = text.as_bytes();
    let mut out = b.to_vec();
    let blank = |out: &mut Vec<u8>, from: usize, to: usize| {
        for c in &mut out[from..to] {
            if *c != b'\n' {
                *c = b' ';
            }
        }
    };
    let mut i = 0;
    while i < b.len() {
        if b[i..].starts_with(b"/*") {
            let end = find(b, i + 2, b"*/").map_or(b.len(), |e| e + 2);
            blank(&mut out, i, end);
            i = end;
        } else if b[i..].starts_with(b"<!--") {
            let end = find(b, i + 4, b"-->").map_or(b.len(), |e| e + 3);
            blank(&mut out, i, end);
            i = end;
        } else if b[i..].starts_with(b"//") && (i == 0 || b[i - 1] == b' ' || b[i - 1] == b'\n') {
            let end = find(b, i, b"\n").unwrap_or(b.len());
            blank(&mut out, i, end);
            i = end;
        } else {
            i += 1;
        }
    }
    String::from_utf8(out).unwrap()
}

fn find(hay: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    hay[from..]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + from)
}

fn line_of(text: &str, at: usize) -> usize {
    text[..at].matches('\n').count() + 1
}

fn word(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

const NAMED: &[&str] = &[
    "white",
    "black",
    "red",
    "green",
    "blue",
    "yellow",
    "orange",
    "purple",
    "pink",
    "gray",
    "grey",
    "silver",
    "maroon",
    "navy",
    "teal",
    "olive",
    "lime",
    "aqua",
    "fuchsia",
    "cyan",
    "magenta",
    "brown",
    "gold",
    "indigo",
    "violet",
    "beige",
    "coral",
    "crimson",
    "salmon",
    "tomato",
    "khaki",
    "lavender",
    "ivory",
    "plum",
    "orchid",
    "tan",
    "turquoise",
    "chocolate",
    "firebrick",
    "darkred",
    "darkblue",
    "darkgreen",
    "lightgray",
    "lightgrey",
    "darkgray",
    "darkgrey",
    "whitesmoke",
    "gainsboro",
    "slategray",
    "steelblue",
    "skyblue",
];

const COLOUR_PROPS: &[&str] = &[
    "color",
    "background",
    "background-color",
    "border",
    "border-color",
    "border-top",
    "border-bottom",
    "border-left",
    "border-right",
    "outline",
    "outline-color",
    "fill",
    "stroke",
    "box-shadow",
    "text-decoration",
    "text-decoration-color",
    "caret-color",
    "accent-color",
];

/// Every declaration `prop: value` in `text`, as (offset of prop, prop, value).
/// A prop must start a word that is not a custom property (`--x:`).
fn declarations(text: &str) -> Vec<(usize, String, String)> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    for (colon, _) in text.match_indices(':') {
        let mut s = colon;
        while s > 0 && b[s - 1] == b' ' {
            s -= 1;
        }
        let end = s;
        while s > 0 && (b[s - 1].is_ascii_lowercase() || b[s - 1] == b'-') {
            s -= 1;
        }
        if s == end
            || b[s] == b'-'
            || (s > 0 && (word(b[s - 1]) || b[s - 1] == b'-' || b[s - 1] == b'.'))
        {
            continue;
        }
        let prop = &text[s..end];
        let rest = &text[colon + 1..];
        let stop = rest
            .find([';', '}', '"', '\'', '`', '\n', '{'])
            .unwrap_or(rest.len());
        out.push((s, prop.to_owned(), rest[..stop].trim().to_owned()));
    }
    out
}

/// Each place `rel` styles outside the tokens, as `file:line: rule: detail`.
fn violations(rel: &str, raw: &str) -> Vec<String> {
    let text = strip_comments(raw);
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut flag = |at: usize, rule: &str, detail: &str| {
        out.push(format!("{rel}:{}: {rule}: {detail}", line_of(&text, at)));
    };

    for (at, _) in text.match_indices('#') {
        if at > 0 && (word(b[at - 1]) || b[at - 1] == b'&' || b[at - 1] == b'#') {
            continue;
        }
        let n = b[at + 1..]
            .iter()
            .take_while(|c| c.is_ascii_hexdigit())
            .count();
        let after = b.get(at + 1 + n).copied().unwrap_or(b' ');
        if matches!(n, 3 | 4 | 6 | 8) && !word(after) && after != b'-' {
            flag(at, "raw colour", &text[at..at + 1 + n]);
        }
    }
    let lower = text.to_ascii_lowercase();
    for f in [
        "rgb(", "rgba(", "hsl(", "hsla(", "oklch(", "oklab(", "hwb(", "lab(", "lch(",
    ] {
        for (at, _) in lower.match_indices(f) {
            if at == 0 || !(word(b[at - 1]) || b[at - 1] == b'-') {
                flag(at, "raw colour", f);
            }
        }
    }

    for (at, prop, value) in declarations(&text) {
        let v = value.to_ascii_lowercase();
        if COLOUR_PROPS.contains(&prop.as_str()) {
            for w in v.split(|c: char| !c.is_ascii_alphanumeric() && c != '-') {
                if NAMED.contains(&w) {
                    flag(at, "named colour", &format!("{prop}: {value}"));
                }
            }
        }
        match prop.as_str() {
            "font-family"
                if !matches!(
                    v.as_str(),
                    "var(--font-sans)" | "var(--font-mono)" | "inherit"
                ) =>
            {
                flag(
                    at,
                    "font-family outside the tokens",
                    &format!("{prop}: {value}"),
                )
            }
            "font" if v != "inherit" => flag(at, "font shorthand", &format!("{prop}: {value}")),
            "font-size" if !(v.starts_with("var(--text-") || v == "inherit") => flag(
                at,
                "font-size not a --text-* step",
                &format!("{prop}: {value}"),
            ),
            p if p == "border-radius" || (p.starts_with("border-") && p.ends_with("-radius")) => {
                let ok = v.split_whitespace().all(|part| {
                    part.starts_with("var(--radius") || matches!(part, "0" | "inherit")
                });
                if !ok {
                    flag(
                        at,
                        "border-radius not a radius token",
                        &format!("{prop}: {value}"),
                    );
                }
            }
            _ => {}
        }
    }
    // SVG presentation attributes are colours too.
    for attr in ["fill=\"", "stroke=\""] {
        for (at, _) in text.match_indices(attr) {
            let rest = &text[at + attr.len()..];
            let v = &rest[..rest.find('"').unwrap_or(0)];
            if NAMED.contains(&v.to_ascii_lowercase().as_str()) {
                flag(at, "named colour", &format!("{attr}{v}\""));
            }
        }
    }

    // Pixel lengths: only 1px and 2px (hairlines, focus outlines).
    for (at, _) in text.match_indices("px") {
        let digits = b[..at]
            .iter()
            .rev()
            .take_while(|c| c.is_ascii_digit() || **c == b'.')
            .count();
        if digits == 0 {
            continue;
        }
        let start = at - digits;
        if start > 0 && (b[start - 1].is_ascii_alphabetic() || b[start - 1] == b'_') {
            continue;
        }
        if b.get(at + 2).is_some_and(|c| word(*c)) {
            continue;
        }
        let n = &text[start..at];
        if !matches!(n, "0" | "1" | "2") {
            flag(start, "pixel length", &format!("{n}px"));
        }
    }

    // A page's own custom properties: colours and sizes belong in the token file.
    let mut styles: Vec<(usize, usize)> = Vec::new();
    if rel.ends_with(".css") {
        styles.push((0, text.len()));
    } else {
        let mut from = 0;
        while let Some(open) = text[from..].find("<style") {
            let open = from + open;
            let close = text[open..]
                .find("</style>")
                .map_or(text.len(), |c| open + c);
            styles.push((open, close));
            from = close;
        }
    }
    for (from, to) in styles {
        let s = &text[from..to];
        let sb = s.as_bytes();
        for (i, _) in s.match_indices("--") {
            let before = if i == 0 { b' ' } else { sb[i - 1] };
            if !(before == b'{' || before == b';' || before.is_ascii_whitespace()) {
                continue;
            }
            let name_len = sb[i + 2..]
                .iter()
                .take_while(|c| c.is_ascii_alphanumeric() || **c == b'-')
                .count();
            let after = s[i + 2 + name_len..].trim_start();
            if name_len > 0 && after.starts_with(':') {
                flag(
                    from + i,
                    "custom property outside the token file",
                    &s[i..i + 2 + name_len],
                );
            }
        }
    }
    out
}

#[test]
fn every_page_styles_only_through_the_tokens() {
    let found: Vec<String> = sources()
        .iter()
        .flat_map(|rel| violations(rel, &read(rel)))
        .collect();
    assert!(
        found.is_empty(),
        "{} design violation(s); use the tokens in {TOKEN_FILE}:\n{}",
        found.len(),
        found.join("\n")
    );
}

#[test]
fn every_page_includes_the_shared_tokens() {
    let mut missing = Vec::new();
    for rel in sources().iter().filter(|r| r.ends_with(".html")) {
        let text = read(rel);
        let ok = if rel.starts_with(UI) {
            text.contains(r#"<link rel="stylesheet" href="tokens.css">"#)
        } else {
            let style = text.find("<style>").map(|s| &text[s..]).unwrap_or("");
            style
                .trim_start_matches("<style>")
                .trim_start()
                .starts_with(cos::design::SLOT)
        };
        if !ok {
            missing.push(rel.clone());
        }
    }
    assert!(
        missing.is_empty(),
        "pages without the shared tokens (the window links tokens.css; a served page opens its first <style> with {}): {missing:?}",
        cos::design::SLOT
    );
}

/// The `:root` block's custom properties, in order, as (name, value).
fn root_tokens() -> Vec<(String, String)> {
    let css = strip_comments(cos::design::TOKENS);
    let start = css.find(":root {").expect("tokens.css has a :root block");
    let body = &css[start + 7..start + css[start..].find('}').unwrap()];
    body.split(';')
        .filter_map(|d| {
            let (k, v) = d.split_once(':')?;
            let k = k.trim();
            k.starts_with("--")
                .then(|| (k.trim_start_matches("--").to_owned(), v.trim().to_owned()))
        })
        .collect()
}

/// `light-dark(a, b)` → (a, b), splitting at the top-level comma.
fn light_dark(v: &str) -> Option<(String, String)> {
    let inner = v.strip_prefix("light-dark(")?.strip_suffix(')')?;
    let mut depth = 0;
    for (i, c) in inner.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                return Some((
                    inner[..i].trim().to_owned(),
                    inner[i + 1..].trim().to_owned(),
                ));
            }
            _ => {}
        }
    }
    None
}

fn is_colour(v: &str) -> bool {
    v.contains('#') || v.contains("rgb(") || v.contains("light-dark(")
}

#[test]
fn every_colour_token_has_a_light_and_a_dark_value() {
    let tokens = root_tokens();
    let mut bad = Vec::new();
    for (name, v) in &tokens {
        if !is_colour(v) {
            continue;
        }
        // A shadow wraps its colour: find the light-dark() inside.
        let ld = v.find("light-dark(").map(|i| {
            let s = &v[i..];
            let mut depth = 0;
            let end = s
                .char_indices()
                .find_map(|(j, c)| {
                    match c {
                        '(' => depth += 1,
                        ')' => {
                            depth -= 1;
                            if depth == 0 {
                                return Some(j + 1);
                            }
                        }
                        _ => {}
                    }
                    None
                })
                .unwrap_or(s.len());
            &s[..end]
        });
        match ld.and_then(light_dark) {
            Some((l, d)) if !l.is_empty() && !d.is_empty() => {}
            _ => bad.push(format!("--{name}: {v}")),
        }
    }
    assert!(
        bad.is_empty(),
        "colour tokens without light-dark(light, dark): {bad:#?}"
    );

    let names: BTreeSet<&str> = tokens.iter().map(|(n, _)| n.as_str()).collect();
    let required = [
        "background",
        "foreground",
        "card",
        "card-foreground",
        "popover",
        "popover-foreground",
        "primary",
        "primary-foreground",
        "secondary",
        "secondary-foreground",
        "muted",
        "muted-foreground",
        "accent",
        "accent-foreground",
        "destructive",
        "destructive-foreground",
        "border",
        "input",
        "ring",
        "chart-1",
        "chart-2",
        "chart-3",
        "chart-4",
        "chart-5",
        "sidebar",
        "sidebar-foreground",
        "sidebar-primary",
        "sidebar-primary-foreground",
        "sidebar-accent",
        "sidebar-accent-foreground",
        "sidebar-border",
        "sidebar-ring",
        "brand-ink",
        "brand-amber",
        "brand-blue",
        "font-sans",
        "font-mono",
        "text-xs",
        "text-sm",
        "text-base",
        "text-lg",
        "text-xl",
        "text-2xl",
        "text-xs--line-height",
        "text-sm--line-height",
        "text-base--line-height",
        "text-lg--line-height",
        "text-xl--line-height",
        "text-2xl--line-height",
        "text-3xl",
        "text-display",
        "hero",
        "hero-foreground",
        "radius",
        "radius-sm",
        "radius-md",
        "radius-lg",
        "radius-xl",
        "radius-full",
    ];
    let missing: Vec<&str> = required
        .into_iter()
        .filter(|r| !names.contains(r))
        .collect();
    assert!(missing.is_empty(), "tokens.css lacks {missing:?}");
    let dupes: Vec<&String> = tokens
        .iter()
        .map(|(n, _)| n)
        .filter(|n| tokens.iter().filter(|(m, _)| m == *n).count() > 1)
        .collect();
    assert!(dupes.is_empty(), "defined twice: {dupes:?}");
}

fn hex(v: &str) -> Option<[f64; 3]> {
    let h = v.trim().strip_prefix('#')?;
    let h: String = match h.len() {
        3 => h.chars().flat_map(|c| [c, c]).collect(),
        6 => h.to_owned(),
        _ => return None,
    };
    let p = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok().map(f64::from);
    Some([p(0)?, p(2)?, p(4)?])
}

fn luminance([r, g, b]: [f64; 3]) -> f64 {
    let lin = |c: f64| {
        let c = c / 255.0;
        if c <= 0.039_28 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b)
}

fn contrast(a: [f64; 3], b: [f64; 3]) -> f64 {
    let (x, y) = (luminance(a), luminance(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

#[test]
fn text_tokens_meet_aa_contrast_in_light_and_dark() {
    let tokens = root_tokens();
    let get = |name: &str| -> (String, String) {
        let v = &tokens
            .iter()
            .find(|(n, _)| n == name)
            .unwrap_or_else(|| panic!("no --{name}"))
            .1;
        light_dark(v).unwrap_or_else(|| panic!("--{name} is not light-dark()"))
    };
    // (text, surface): every pair a page puts text on.
    let pairs = [
        ("foreground", "background"),
        ("card-foreground", "card"),
        ("popover-foreground", "popover"),
        ("primary-foreground", "primary"),
        ("secondary-foreground", "secondary"),
        ("muted-foreground", "muted"),
        ("muted-foreground", "background"),
        ("muted-foreground", "card"),
        ("accent-foreground", "accent"),
        ("destructive-foreground", "destructive"),
        ("destructive", "background"),
        ("destructive", "card"),
        ("success-foreground", "success"),
        ("success", "background"),
        ("success", "card"),
        ("warning-foreground", "warning"),
        ("warning", "background"),
        ("warning", "card"),
        ("info-foreground", "info"),
        ("info", "card"),
        ("brand-text", "background"),
        ("brand-text", "card"),
        ("brand-accent-text", "card"),
        ("sidebar-foreground", "sidebar"),
        ("sidebar-primary-foreground", "sidebar-primary"),
        ("sidebar-accent-foreground", "sidebar-accent"),
        ("foreground", "accent"),
        ("foreground", "secondary"),
        ("hero-foreground", "hero"),
        ("hero-muted", "hero"),
        ("brand-amber", "hero"),
        // verb chips: DO ink, DECIDE amber, DELEGATE and BATCH blue
        ("verb-do-foreground", "verb-do"),
        ("brand-ink", "brand-amber"),
        ("brand-ink", "brand-blue"),
    ];
    let mut low = Vec::new();
    for (text, surface) in pairs {
        let (tl, td) = get(text);
        let (sl, sd) = get(surface);
        for (mode, t, s) in [("light", tl, sl), ("dark", td, sd)] {
            let (a, b) = (hex(&t).expect(text), hex(&s).expect(surface));
            let r = contrast(a, b);
            if r < 4.5 {
                low.push(format!(
                    "{mode}: --{text} {t} on --{surface} {s} = {r:.2}:1"
                ));
            }
        }
    }
    assert!(low.is_empty(), "below WCAG AA 4.5:1:\n{}", low.join("\n"));
}

#[test]
fn every_token_a_page_uses_is_defined() {
    let defined: BTreeSet<String> = root_tokens().into_iter().map(|(n, _)| n).collect();
    let mut unknown = Vec::new();
    for rel in sources() {
        let text = strip_comments(&read(&rel));
        for (at, _) in text.match_indices("var(--") {
            let name: String = text[at + 6..]
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
                .collect();
            let next = text[at + 6 + name.len()..].chars().next();
            if name.is_empty() || next == Some('$') || defined.contains(&name) {
                continue;
            }
            unknown.push(format!("{rel}:{}: --{name}", line_of(&text, at)));
        }
    }
    assert!(
        unknown.is_empty(),
        "tokens not in {TOKEN_FILE}:\n{}",
        unknown.join("\n")
    );
}

#[test]
fn the_served_pages_embed_the_tokens_and_both_fonts() {
    let css = cos::design::css();
    assert!(!css.contains("vendor/geist/"), "a font is still a file URL");
    assert_eq!(css.matches("url(\"data:font/woff2;base64,").count(), 2);
    let page = cos::design::inline(&format!(
        "<style>{}</style><!--__COS_MARK__-->",
        cos::design::SLOT
    ))
    .unwrap();
    assert!(page.contains("--primary:") && page.contains(r#"aria-label="Chief of Staff""#));
    let agents = cos::agents::view::html(&[], &[]);
    assert!(agents.contains("--primary:") && !agents.contains(cos::design::SLOT));
    assert!(agents.contains("0 running · 0 total") && !agents.contains("<!--__"));
    assert!(
        cos::design::inline("<style></style>").is_err(),
        "a page without the slot renders"
    );
}

#[test]
fn base64_matches_the_rfc_4648_vectors() {
    for (raw, enc) in [
        ("", ""),
        ("f", "Zg=="),
        ("fo", "Zm8="),
        ("foo", "Zm9v"),
        ("foob", "Zm9vYg=="),
        ("fooba", "Zm9vYmE="),
        ("foobar", "Zm9vYmFy"),
    ] {
        assert_eq!(cos::design::base64(raw.as_bytes()), enc);
    }
}

/// Each rule fires on a sample, so a gate that silently matches nothing fails here.
#[test]
fn each_rule_catches_its_violation() {
    let cases = [
        (".a { color: #fff; }", "raw colour"),
        (".a { background: rgba(0, 0, 0, .1); }", "raw colour"),
        (".a { border: 1px solid white; }", "named colour"),
        (r#"<rect fill="red"/>"#, "named colour"),
        (
            ".a { font-family: Menlo, monospace; }",
            "font-family outside the tokens",
        ),
        (".a { font: 14px/1.4 system-ui; }", "font shorthand"),
        (".a { font-size: 13px; }", "font-size not a --text-* step"),
        (".a { font-size: 0.8rem; }", "font-size not a --text-* step"),
        (
            ".a { border-radius: 6px; }",
            "border-radius not a radius token",
        ),
        (".a { padding: 12px; }", "pixel length"),
        (".a { margin: -4px; }", "pixel length"),
        (
            "<style>:root { --ink: var(--foreground); }</style>",
            "custom property outside the token file",
        ),
    ];
    for (sample, rule) in cases {
        let rel = if sample.starts_with('<') {
            "sample.html"
        } else {
            "sample.css"
        };
        let found = violations(rel, sample);
        assert!(
            found.iter().any(|v| v.contains(rule)),
            "{rule} missed in {sample:?}: {found:?}"
        );
    }
    let clean = ".a { color: var(--foreground); border: 1px solid var(--border); font-size: var(--text-sm); border-radius: var(--radius-md); padding: 0.5rem; }\n/* #fff 13px */\n<b>&#39;</b>";
    assert_eq!(violations("sample.css", clean), Vec::<String>::new());
}

/// The served pages' icons come from the window's set: every icon in it
/// draws every one of its shapes, whichever way the file spaces them.
#[test]
fn every_vendored_icon_draws_all_its_shapes_in_a_served_page() {
    let file = read("apps/desktop/ui/vendor/lucide/icons-0.556.0.js");
    let mut n = 0;
    for line in file.lines().filter(|l| l.starts_with("  \"")) {
        let name = line.split('"').nth(1).unwrap();
        let shapes = line.matches("\", {").count();
        let svg = cos::design::icon(name);
        assert_eq!(svg.matches("/>").count(), shapes, "{name}: {svg}");
        assert!(svg.contains(r#"aria-hidden="true""#));
        n += 1;
    }
    assert!(n >= 80, "{n} icons");
    assert_eq!(cos::design::icon("no-such-icon"), "");
}

/// The Flows page draws data a task store holds, so it builds every node
/// with `textContent` and attributes, never by parsing markup.
#[test]
fn flows_page_never_writes_html_from_data() {
    let page = strip_comments(&read("src/flows.html"));
    for sink in [
        "innerHTML",
        "outerHTML",
        "insertAdjacentHTML",
        "document.write",
        "createContextualFragment",
        "DOMParser",
    ] {
        assert!(!page.contains(sink), "src/flows.html uses {sink}");
    }
}

/// A `title` on an iframe is a native tooltip that floats over the page it
/// frames wherever the pointer rests; the name goes in `aria-label`.
#[test]
fn no_iframe_carries_a_hover_tooltip() {
    let html = read("apps/desktop/ui/index.html");
    let mut frames = 0;
    for tag in html.split("<iframe").skip(1) {
        let tag = &tag[..tag.find('>').unwrap()];
        frames += 1;
        assert!(!tag.contains(" title="), "<iframe{tag}>");
        assert!(tag.contains("aria-label="), "<iframe{tag}> has no name");
    }
    assert!(frames >= 4, "found {frames} iframes");
    for file in WINDOW_JS {
        let js = read(&format!("{UI}/{file}"));
        for line in js.lines().filter(|l| l.contains("\"iframe\"")) {
            assert!(!line.contains("title"), "{file}: {line}");
        }
    }
}

// ---- the Flows Map's GPU engine

const VGPU_FILE: &str = "apps/desktop/ui/vendor/vgpu/vgpu-0.6.0.iife.js";
/// The pages that take vgpu, each with an SVG engine beside it.
const GPU_PAGES: &[&str] = &["src/flows.html"];
const VGPU_CEILING: usize = 220_000;

/// The marked WGSL literals in `page`: (the name it is bound to, its text).
fn wgsl_literals(page: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for (at, m) in page.match_indices("/*wgsl*/`") {
        let body_start = at + m.len();
        let end = page[body_start..]
            .find('`')
            .map_or(page.len(), |e| body_start + e);
        let name = page[..at]
            .trim_end()
            .strip_suffix('=')
            .map(|h| {
                h.trim_end()
                    .rsplit(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                    .next()
                    .unwrap_or("")
                    .to_owned()
            })
            .unwrap_or_default();
        out.push((name, page[body_start..end].to_owned()));
    }
    out
}

/// A WGSL numeric literal: `1`, `-1.0`, `.9`, `1e3`, `0.5f`, `2u`.
fn numeric(arg: &str) -> Option<f64> {
    let a = arg.trim().trim_end_matches(['f', 'h', 'u', 'i']);
    if !a.chars().any(|c| c.is_ascii_digit()) {
        return None;
    }
    a.parse::<f64>().ok()
}

/// The top-level arguments of the call whose `(` is at `open`, and the
/// offset just past its `)`.
fn call_args(text: &str, open: usize) -> (Vec<String>, usize) {
    let b = text.as_bytes();
    let (mut depth, mut start, mut args) = (0usize, open + 1, Vec::new());
    for (i, c) in b.iter().enumerate().skip(open) {
        match c {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    let last = text[start..i].trim();
                    if !last.is_empty() {
                        args.push(last.to_owned());
                    }
                    return (args, i + 1);
                }
            }
            b',' if depth == 1 => {
                args.push(text[start..i].trim().to_owned());
                start = i + 1;
            }
            _ => {}
        }
    }
    (args, b.len())
}

/// A vector constructor in any spelling at the start of `s`: its width and
/// the offset of its `(`.
fn vector_ctor(s: &str) -> Option<(usize, usize)> {
    let rest = s.strip_prefix("vec")?;
    let width = rest.bytes().next()?;
    if !(b'2'..=b'4').contains(&width) {
        return None;
    }
    let tail = &rest[1..];
    for suffix in ["<f32>", "<f16>", "<i32>", "<u32>", "f", "h", "i", "u", ""] {
        if let Some(after) = tail.strip_prefix(suffix) {
            let ws = after.len() - after.trim_start().len();
            if after.trim_start().starts_with('(') {
                return Some(((width - b'0') as usize, 4 + suffix.len() + ws));
            }
        }
    }
    None
}

/// The components of `arg` when it is a numeric literal or a vector built
/// only from them; `None` for anything that reads a value.
fn literal_components(arg: &str) -> Option<Vec<f64>> {
    let a = arg.trim();
    if let Some(n) = numeric(a) {
        return Some(vec![n]);
    }
    let (width, open) = vector_ctor(a)?;
    let (args, end) = call_args(a, open);
    if end != a.len() {
        return None;
    }
    let mut out = Vec::new();
    for x in &args {
        out.extend(literal_components(x)?);
    }
    if out.len() == 1 {
        out = vec![out[0]; width];
    }
    Some(out)
}

/// Each way `page` could carry a colour outside the uniform struct.
fn wgsl_violations(page: &str) -> Vec<String> {
    let mut out = Vec::new();
    let lits = wgsl_literals(page);
    for (name, body) in &lits {
        if body.contains("${") {
            out.push(format!("{name}: interpolation inside a /*wgsl*/ literal"));
        }
        let b = body.as_bytes();
        for (at, _) in body.match_indices("vec") {
            if at > 0 && word(b[at - 1]) {
                continue;
            }
            let Some((width, open)) = vector_ctor(&body[at..]) else {
                continue;
            };
            if width < 3 {
                continue;
            }
            let (args, end) = call_args(body, at + open);
            let parts: Option<Vec<f64>> = args.iter().try_fold(Vec::new(), |mut acc, a| {
                acc.extend(literal_components(a)?);
                Some(acc)
            });
            if parts.is_some_and(|p| p.iter().any(|v| *v != 0.0)) {
                out.push(format!("{name}: literal colour vector {}", &body[at..end]));
            }
        }
    }
    let bound: BTreeSet<&str> = lits.iter().map(|(n, _)| n.as_str()).collect();
    for (at, m) in page.match_indices("prepareShader(") {
        let (args, _) = call_args(page, at + m.len() - 1);
        let first = args.first().map(String::as_str).unwrap_or("");
        let bare = first.starts_with("WGSL_")
            && first
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'_');
        if !bare || !bound.contains(first) {
            out.push(format!(
                "prepareShader({first}) does not name a /*wgsl*/ literal"
            ));
        }
    }
    out
}

#[test]
fn wgsl_takes_colours_only_from_uniforms() {
    for file in GPU_PAGES {
        let page = read(file);
        assert!(
            !wgsl_literals(&page).is_empty(),
            "no /*wgsl*/ literal found in {file}"
        );
        let found = wgsl_violations(&page);
        assert!(
            found.is_empty(),
            "{file}: shader colours come from the uniform struct (design tokens):\n{}",
            found.join("\n")
        );
    }

    let wrap = |src: &str| format!("const WGSL_T = /*wgsl*/`{src}`;\nprepareShader(WGSL_T);");
    for spelling in [
        "vec3",
        "vec3f",
        "vec3h",
        "vec3<f32>",
        "vec3<f16>",
        "vec4",
        "vec4f",
        "vec4h",
        "vec4<f32>",
        "vec4<f16>",
    ] {
        let args = if spelling.starts_with("vec3") {
            "(1.0, 0.0, 0.0)"
        } else {
            "(1.0, 0.0, 0.0, 1.0)"
        };
        let sample = wrap(&format!(
            "@fragment fn fs_main() -> @location(0) vec4f {{ return {spelling}{args}; }}"
        ));
        assert!(!wgsl_violations(&sample).is_empty(), "{spelling} missed");
    }
    for bad in [
        "struct O { @builtin(position) p: vec4f, @location(0) c: vec4f };\n@vertex fn vs_main() -> O { var o: O; o.c = vec4f(1.0, 0.0, 0.0, 1.0); return o; }\n@fragment fn fs_main(i: O) -> @location(0) vec4f { return i.c; }",
        "@fragment fn fs_main() -> @location(0) vec4f { return vec4f(vec2f(.9, .3), vec2f(.1, 1.0)); }",
        "@fragment fn fs_main() -> @location(0) vec4f { return ${COLOUR}; }",
    ] {
        assert!(!wgsl_violations(&wrap(bad)).is_empty(), "missed {bad}");
    }
    for good in [
        "var a = vec4f(0.0);",
        "let c = vec2f(-1.0, 1.0);",
        "let q = vec4f(p.xy, 0.0, 1.0);",
    ] {
        assert_eq!(wgsl_violations(&wrap(good)), Vec::<String>::new(), "{good}");
    }
    for call in [
        "const WGSL_T = /*wgsl*/`let a = 1.0;`;\nprepareShader(\"@fragment fn f() {}\");",
        "const WGSL_T = /*wgsl*/`let a = 1.0;`;\nprepareShader(A + B);",
        "const WGSL_T = /*wgsl*/`let a = 1.0;`;\nprepareShader(WGSL_OTHER);",
    ] {
        assert!(!wgsl_violations(call).is_empty(), "missed {call}");
    }
}

#[test]
fn every_gpu_page_keeps_its_svg_fallback() {
    let with_slot: Vec<&str> = SERVED
        .iter()
        .copied()
        .filter(|rel| read(rel).contains("__COS_VGPU__"))
        .collect();
    assert_eq!(
        with_slot, GPU_PAGES,
        "vgpu is inlined into the Flows page only"
    );
    for file in GPU_PAGES {
        let page = read(file);
        for f in ["svgEngine(", "gpuEngine(", "pickEngine("] {
            assert!(page.contains(f), "{file} defines no {f}");
        }
    }
}

#[test]
fn the_vendored_vgpu_is_pinned_and_within_its_ceiling() {
    use sha2::{Digest, Sha256};
    let bytes = std::fs::read(Path::new(ROOT).join(VGPU_FILE)).unwrap();
    let readme = read("apps/desktop/ui/vendor/README.md");
    let row = readme
        .lines()
        .find(|l| l.starts_with("| `vgpu/vgpu-0.6.0.iife.js`"))
        .expect("the vendor README has a row for the vgpu file");
    let pinned = row
        .rsplit('`')
        .nth(1)
        .expect("the row ends with the sha256 in backticks");
    let actual: String = Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    assert_eq!(actual, pinned, "{VGPU_FILE} differs from its README row");
    assert!(
        bytes.len() <= VGPU_CEILING,
        "{VGPU_FILE} is {} B, over the {VGPU_CEILING} B ceiling",
        bytes.len()
    );
    let text = String::from_utf8_lossy(&bytes);
    for api in [
        "eval(",
        "new Function",
        "Worker(",
        "createObjectURL",
        "fetch(",
        "XMLHttpRequest",
        "WebSocket",
        "EventSource",
        "sendBeacon",
        "importScripts",
        "import(",
        "WebAssembly",
    ] {
        assert!(!text.contains(api), "{VGPU_FILE} contains {api}");
    }
}

// ---- states: every loading, empty, slow and error state in the window is
// built by `stateView` in states.js, with an icon always and the state's art
// at region level.

const ART_FILE: &str = "apps/desktop/ui/art/art.js";
const ICON_FILE: &str = "apps/desktop/ui/vendor/lucide/icons-0.556.0.js";
/// Each picture's share of art/art.js: the README's "under ~6 KB each".
const ART_PER_KIND: usize = 6 * 1024;
/// The window's scripts, each of which may show a state.
const WINDOW_JS: &[&str] = &["states.js", "command.js", "dash.js", "app.js"];

/// From `head` to the first brace closing a top-level function or a method.
fn body<'a>(js: &'a str, head: &str) -> &'a str {
    let at = js.find(head).unwrap_or_else(|| panic!("{head} not found"));
    let rest = &js[at..];
    let end = ["\n}\n", "\n};\n", "\n  },\n"]
        .iter()
        .filter_map(|e| rest.find(e))
        .min()
        .unwrap_or(rest.len());
    &rest[..end]
}

#[test]
fn every_state_renders_through_state_view_with_an_icon_and_art() {
    let js: String = WINDOW_JS
        .iter()
        .map(|f| strip_comments(&read(&format!("{UI}/{f}"))))
        .collect::<Vec<_>>()
        .join("\n");
    let html = read("apps/desktop/ui/index.html");
    let art = read(ART_FILE);
    let icons = read(ICON_FILE);

    // Every kind has an icon in the vendored set and a picture in ART.
    let state = body(&js, "const STATE = {");
    let kinds: Vec<(&str, &str)> = state
        .lines()
        .skip(1)
        .filter_map(|l| {
            let (k, v) = l.trim().split_once(": [")?;
            Some((k, v.split('"').nth(1)?))
        })
        .collect();
    assert!(kinds.len() >= 9, "{kinds:?}");
    for (kind, glyph) in &kinds {
        assert!(
            icons.contains(&format!("\"{glyph}\":")),
            "{kind}: no icon `{glyph}` in {ICON_FILE}"
        );
        assert!(
            art.contains(&format!("{kind}: svg(\"{kind}\""))
                || art.contains(&format!("{kind}: svg(\n      \"{kind}\"")),
            "{kind}: no picture in {ART_FILE}"
        );
    }
    assert!(
        art.len() <= kinds.len() * ART_PER_KIND,
        "art.js is {} B for {} pictures",
        art.len(),
        kinds.len()
    );
    assert!(
        art.contains(r#"aria-hidden="true""#),
        "the art is decorative"
    );

    // stateView draws the icon on both paths and the art on the region path.
    let view = body(&js, "function stateView(");
    assert_eq!(
        view.matches("stateIcon(kind)").count(),
        2,
        "an icon in a line and in a region"
    );
    assert!(view.contains("stateArt(kind)"), "a region gets its picture");
    assert!(view.contains(r#"setAttribute("aria-live", "polite")"#));

    // Nothing else builds a state container, and no ad-hoc ones remain. A
    // business switch is the one reload: it drops every view of the old one.
    let outside = js
        .replace(view, "")
        .replace(body(&js, "async function switchTo("), "");
    for made in [
        r#"className: "state-line""#,
        "className: `state",
        r#"className: "state""#,
        r#"className: "empty"#,
        "emptyNote(",
        "location.reload",
    ] {
        assert!(!outside.contains(made), "`{made}` outside stateView");
    }
    assert!(
        !html.contains(r#"class="empty""#),
        "index.html has an ad-hoc empty state"
    );

    // Every view that fetches says so through stateView, or through
    // frameLoad, which puts one over its frame.
    for loader in [
        "async function refreshAgents(",
        "function renderStaff(",
        "function renderRuns(",
        "function renderScores(",
        "  async refresh() {",
        "function frameLoad(",
    ] {
        assert!(
            body(&js, loader).contains("stateView("),
            "{loader} renders a state without stateView"
        );
    }
    for frame in ["function reloadReport(", "  load(fresh, quiet = false) {"] {
        assert!(
            body(&js, frame).contains("frameLoad("),
            "{frame} loads a frame with no state over it"
        );
    }
    assert!(js.contains(r#"frameLoad("docs-frame""#));
    for file in ["command.js", "dash.js"] {
        assert!(
            read(&format!("{UI}/{file}")).contains("stateView(\"unreachable\""),
            "{file} reads the backend with no state for a failed read"
        );
    }

    // An icon-only button always has a name.
    assert!(
        body(&js, "function iconButton(")
            .contains(r#"if (iconOnly) b.setAttribute("aria-label", label);"#)
    );
    for tag in html.split("<button").skip(1) {
        let tag = &tag[..tag.find("</button>").unwrap()];
        let mut text = tag[tag.find('>').unwrap() + 1..].to_owned();
        while let (Some(a), Some(b)) = (text.find('<'), text.find('>')) {
            if b < a {
                break;
            }
            text.replace_range(a..=b, "");
        }
        assert!(
            !tag.contains("data-icon") || !text.trim().is_empty() || tag.contains("aria-label="),
            "a button with only an icon and no aria-label: <button{tag}"
        );
    }
}

/// Motion in the window (state art, spinners) runs only for someone who has
/// not asked for less: every animation sits inside
/// `prefers-reduced-motion: no-preference`.
#[test]
fn every_animation_respects_reduced_motion() {
    let css = strip_comments(&read("apps/desktop/ui/style.css"));
    let b = css.as_bytes();
    let (mut open, mut gated, mut bad) = (Vec::new(), 0, Vec::new());
    for i in 0..b.len() {
        match b[i] {
            b'{' => {
                let head = css[..i].rsplit(['}', ';', '{']).next().unwrap_or("");
                let gate = head.contains("prefers-reduced-motion: no-preference");
                gated += usize::from(gate);
                open.push(gate);
            }
            b'}' if open.pop() == Some(true) => gated -= 1,
            _ => {}
        }
        if css[i..].starts_with("animation:") && (i == 0 || !(word(b[i - 1]) || b[i - 1] == b'-')) {
            let v = css[i + 10..].split([';', '}']).next().unwrap_or("").trim();
            if v != "none" && gated == 0 {
                bad.push(format!("line {}: animation: {v}", line_of(&css, i)));
            }
        }
    }
    assert!(
        bad.is_empty(),
        "animations outside the reduced-motion gate:\n{}",
        bad.join("\n")
    );
}
