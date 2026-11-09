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
