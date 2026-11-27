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
        "identity-1",
        "identity-2",
        "identity-3",
        "identity-4",
        "identity-5",
        "identity-6",
        "identity-1-soft",
        "identity-2-soft",
        "identity-3-soft",
        "identity-4-soft",
        "identity-5-soft",
        "identity-6-soft",
        "think",
        "space-1",
        "space-2",
        "space-3",
        "space-4",
        "space-5",
        "space-6",
        "motion-fast",
        "motion-slow",
        "motion-loop",
        "motion-stagger",
        "ease-spring",
        "glow-1",
        "glow-2",
        "gradient-from",
        "gradient-to",
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
        // gradient text on the hero: each stop is text
        ("gradient-from", "hero"),
        ("gradient-to", "hero"),
        // verb chips: DO ink, DECIDE amber, DELEGATE and BATCH blue
        ("verb-do-foreground", "verb-do"),
        ("brand-ink", "brand-amber"),
        ("brand-ink", "brand-blue"),
        // identity colours are text-capable: each names a person or agent in words
        ("identity-1", "background"),
        ("identity-1", "card"),
        ("identity-2", "background"),
        ("identity-2", "card"),
        ("identity-3", "background"),
        ("identity-3", "card"),
        ("identity-4", "background"),
        ("identity-4", "card"),
        ("identity-5", "background"),
        ("identity-5", "card"),
        ("identity-6", "background"),
        ("identity-6", "card"),
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

/// `raw` uses a `kit-` class (not inside a comment or a longer name) without
/// `design::KIT_SLOT`, so its page would render without the kit sheet.
fn kit_class_without_sheet(rel: &str, raw: &str) -> Option<String> {
    let text = strip_comments(raw);
    let b = text.as_bytes();
    let uses = text
        .match_indices("kit-")
        .any(|(at, _)| at == 0 || !(word(b[at - 1]) || b[at - 1] == b'-'));
    (uses && !raw.contains(cos::design::KIT_SLOT))
        .then(|| format!("{rel}: kit- class without {}", cos::design::KIT_SLOT))
}

#[test]
fn every_page_that_uses_kit_classes_includes_the_kit_sheet() {
    let missing: Vec<String> = SERVED
        .iter()
        .filter_map(|rel| kit_class_without_sheet(rel, &read(rel)))
        .collect();
    assert!(missing.is_empty(), "{missing:#?}");
    assert!(
        kit_class_without_sheet("sample.html", r#"<p class="kit-card">x</p>"#).is_some(),
        "a kit- class without the slot was not flagged"
    );
    let with_slot = format!(
        "<style>{}</style><p class=\"kit-card\">x</p>",
        cos::design::KIT_SLOT
    );
    assert!(kit_class_without_sheet("sample.html", &with_slot).is_none());
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

    for (sample, rule) in [
        ("x.innerHTML = d;", "markup sink"),
        ("el(\"a\", {[k]: v});", "computed key"),
        ("n.setAttribute(\"onclick\", d);", "on* handler"),
        ("n.setAttribute(k, v);", "not a literal"),
        ("img.src = d;", "URL property .src"),
    ] {
        let found = window_js_violations("sample.js", sample);
        assert!(
            found.iter().any(|v| v.contains(rule)),
            "{rule} missed in {sample:?}: {found:?}"
        );
    }
    assert_eq!(
        window_js_violations(
            "sample.js",
            "el(\"a\", {href: u, textContent: d});\nn.setAttribute(\"role\", \"img\");\nif (a == b) x.src == y;"
        ),
        Vec::<String>::new()
    );

    let motion = |css: &str, js: &str| {
        motion_violations(
            &[("sample.css", css.to_owned())],
            &[("sample.js", js.to_owned())],
        )
    };
    let frames = "@keyframes kit-settle { to { opacity: 1; } }\n";
    for (css, js, rule) in [
        (
            format!("{frames}.a {{ animation: kit-settle 1s; }}"),
            "",
            "animation outside the reduced-motion gate",
        ),
        (
            format!(
                "{frames}@media (prefers-reduced-motion: no-preference) {{ .b {{ animation: kit-settle 1s; }} }}\n.a{{transition:transform 1s}}"
            ),
            "",
            "transition outside the reduced-motion gate",
        ),
        (
            "@keyframes spare { to { opacity: 0; } }".to_owned(),
            "",
            "unused @keyframes spare",
        ),
        (
            String::new(),
            "function f(n) { n.animate([], 1); }",
            ".animate( outside motionOk()",
        ),
        (
            String::new(),
            "function f(n) { if (motionOk()) {} }\nfunction g(n) { for (const m of n) { m.animate([], 1); } }",
            ".animate( outside motionOk()",
        ),
        (
            String::new(),
            "function f(n) { if (motionOk()) log(); n.animate([], 1); }",
            ".animate( outside motionOk()",
        ),
        (
            String::new(),
            "function f(n) { if (!motionOk()) log(); n.animate([], 1); }",
            ".animate( outside motionOk()",
        ),
        (
            String::new(),
            "function f(n) { if (motionOk()) { log(); } n.animate([], 1); }",
            ".animate( outside motionOk()",
        ),
        (
            String::new(),
            "function f(n) { if (!motionOk() && n) return; n.animate([], 1); }",
            ".animate( outside motionOk()",
        ),
        (
            String::new(),
            "function f(n) { if (n) { if (!motionOk()) return; } n.animate([], 1); }",
            ".animate( outside motionOk()",
        ),
    ] {
        let found = motion(&css, js);
        assert!(
            found.iter().any(|v| v.contains(rule)),
            "{rule} missed in {css:?} / {js:?}: {found:?}"
        );
    }
    assert_eq!(
        motion(
            &format!(
                "{frames}@media (prefers-reduced-motion:no-preference){{.a{{animation:kit-settle 1s}}}}\n.b {{ animation: none; transition: color var(--motion-fast), opacity .08s; }}"
            ),
            "function g(n) {\n  if (!motionOk()) return;\n  for (const m of n) { m.animate([], 1); }\n}"
        ),
        Vec::<String>::new()
    );
    assert_eq!(
        motion(
            "",
            "function a(n) {\n  if (!motionOk() || !n.length) {\n    n.x();\n    return;\n  }\n  n.animate([], 1);\n}\nfunction b(n) { if (motionOk()) { n.animate([], 1); } }"
        ),
        Vec::<String>::new(),
        "an early return block and an enclosing if both guard"
    );
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

/// The report's `/* kit */ … /* end kit */` block: the one place its live
/// Today code (inbox, pills, timeline, ticker) lives.
fn report_kit_block(page: &str) -> &str {
    let (open, close) = ("/* kit */", "/* end kit */");
    assert_eq!(page.matches(open).count(), 1, "one {open} marker");
    assert_eq!(page.matches(close).count(), 1, "one {close} marker");
    let from = page.find(open).unwrap() + open.len();
    let to = page.find(close).unwrap();
    assert!(from < to, "{close} before {open}");
    &page[from..to]
}

/// The report's kit block builds every node with DOM methods and text, never
/// markup, with the window's own rules; the page's older `esc()` templates
/// sit outside it and are out of scope.
#[test]
fn report_kit_block_never_writes_html_from_data() {
    let page = read("src/report.html");
    let found = window_js_violations("src/report.html kit block", report_kit_block(&page));
    assert!(
        found.is_empty(),
        "the report's kit block writes markup or a URL:\n{found:#?}"
    );
    let caught = |js: &str| {
        let page = format!("x /* kit */\n{js}\n/* end kit */ y");
        !window_js_violations("fixture", report_kit_block(&page)).is_empty()
    };
    assert!(caught("box.innerHTML = `<b>${t}</b>`;"));
    assert!(caught("box.insertAdjacentHTML(\"beforeend\", t);"));
    assert!(caught("a.href = url;"));
    assert!(caught("n.setAttribute(name, v);"));
    assert!(!caught("n.textContent = t;"));
}

const WINDOW_MARKUP_SINKS: &[&str] = &[
    "innerHTML",
    "outerHTML",
    "insertAdjacentHTML",
    "document.write",
    "createContextualFragment",
    "DOMParser",
];
const WINDOW_URL_PROPS: &[&str] = &[
    "src",
    "href",
    "srcset",
    "action",
    "formAction",
    "poster",
    "data",
];

/// The window's sinks that take no data, each named with its reason. A hit
/// must match its snippet, and each must be hit exactly once.
const WINDOW_EXCEPTIONS: &[(&str, &str, &str)] = &[
    (
        "states.js",
        "t.innerHTML = window.ART",
        "static art from art/art.js, no data",
    ),
    (
        "states.js",
        "n.setAttribute(k, v)",
        "static Lucide geometry from window.LUCIDE, no data",
    ),
    (
        "states.js",
        "frame.src = safeFrameSrc(src)",
        "the app's cosreport: page or a loopback dashboard, through safeFrameSrc",
    ),
];

/// The window-script sinks in `raw`: markup writes, `setAttribute` with a
/// name that is not a literal or an `on*` literal, URL property assignments,
/// and computed keys in an `el(` props object. Returns `file:line: what`.
fn window_js_violations(rel: &str, raw: &str) -> Vec<String> {
    let text = strip_comments(raw);
    let mut out = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let at = |what: &str| format!("{rel}:{}: {what}: {}", n + 1, line.trim());
        for sink in WINDOW_MARKUP_SINKS {
            if line.contains(sink) {
                out.push(at(&format!("markup sink {sink}")));
            }
        }
        for (i, _) in line.match_indices("setAttribute(") {
            let arg = line[i + "setAttribute(".len()..].trim_start();
            match arg.strip_prefix('"') {
                None => out.push(at("setAttribute with a name that is not a literal")),
                Some(name) if name.to_ascii_lowercase().starts_with("on") => {
                    out.push(at("setAttribute of an on* handler"))
                }
                Some(_) => {}
            }
        }
        for prop in WINDOW_URL_PROPS {
            let hit = line.match_indices(&format!(".{prop}")).any(|(i, _)| {
                let rest = line[i + prop.len() + 1..].trim_start();
                rest.starts_with('=') && !rest.starts_with("==")
            });
            if hit {
                out.push(at(&format!("URL property .{prop} assigned")));
            }
        }
    }
    let b = text.as_bytes();
    for (at, _) in text.match_indices("el(") {
        if at > 0 && (word(b[at - 1]) || b[at - 1] == b'.' || b[at - 1] == b'$') {
            continue;
        }
        let mut depth = 0usize;
        let mut end = text.len();
        for (j, &c) in b.iter().enumerate().skip(at + 2) {
            match c {
                b'(' => depth += 1,
                b')' if depth == 0 => {
                    end = j;
                    break;
                }
                b')' => depth -= 1,
                _ => {}
            }
        }
        let span = &text[at..end];
        for (k, _) in span.match_indices('[') {
            let before = span[..k].trim_end().bytes().last();
            let key_close = span[k..].find(']').map(|c| k + c);
            let is_key = matches!(before, Some(b'{' | b','))
                && key_close.is_some_and(|c| span[c + 1..].trim_start().starts_with(':'));
            if is_key {
                let line_no = text[..at + k].matches('\n').count() + 1;
                out.push(format!("{rel}:{line_no}: computed key in el() props"));
            }
        }
    }
    out
}

#[test]
fn window_never_writes_html_from_data() {
    let dir = Path::new(ROOT).join(UI);
    let mut files: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|e| e == "js"))
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    files.sort();
    assert!(
        files.contains(&"kit.js".to_string()),
        "kit.js is not in {UI}"
    );
    let mut hits = vec![0usize; WINDOW_EXCEPTIONS.len()];
    let mut found = Vec::new();
    for file in &files {
        for v in window_js_violations(&format!("{UI}/{file}"), &read(&format!("{UI}/{file}"))) {
            match WINDOW_EXCEPTIONS
                .iter()
                .position(|(f, snip, _)| file == f && v.contains(snip))
            {
                Some(i) => hits[i] += 1,
                None => found.push(v),
            }
        }
    }
    assert!(
        found.is_empty(),
        "window writes markup or a URL from data:\n{found:#?}"
    );
    for ((file, snip, why), n) in WINDOW_EXCEPTIONS.iter().zip(&hits) {
        assert_eq!(*n, 1, "exception {file}: `{snip}` ({why}) hit {n} times");
    }
    assert_eq!(WINDOW_EXCEPTIONS.len(), 3);
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
const WINDOW_JS: &[&str] = &["states.js", "kit.js", "command.js", "dash.js", "app.js"];

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

/// Stylesheets the motion gate reads whole.
const MOTION_CSS: &[&str] = &["apps/desktop/ui/style.css", "apps/desktop/ui/kit.css"];
/// Pages whose `<style>` blocks the motion gate reads.
const MOTION_PAGES: &[&str] = &["src/report.html", "src/flows.html", "src/docs.html"];
/// Pages whose script the `.animate(` rule reads, beside `apps/desktop/ui/*.js`.
const MOTION_SCRIPTS: &[&str] = &["src/report.html", "src/flows.html"];
/// Properties a hover may fade outside the gate: colour and opacity changes
/// move nothing, so they are feedback, not motion.
const HOVER_PROPS: &[&str] = &[
    "color",
    "background",
    "background-color",
    "border-color",
    "outline-color",
    "fill",
    "stroke",
    "opacity",
];

/// Only the `<style>` blocks of `html`, everything else blanked (newlines
/// kept, so lines still match the file).
fn style_blocks(html: &str) -> String {
    let mut out: Vec<u8> = html
        .bytes()
        .map(|c| if c == b'\n' { b'\n' } else { b' ' })
        .collect();
    let mut from = 0;
    while let Some(open) = html[from..].find("<style") {
        let open = from + open;
        let body = open + html[open..].find('>').map_or(0, |e| e + 1);
        let close = html[body..]
            .find("</style>")
            .map_or(html.len(), |c| body + c);
        out[body..close].copy_from_slice(&html.as_bytes()[body..close]);
        from = close;
    }
    String::from_utf8(out).unwrap()
}

/// A hover transition allowed outside the gate: each part a [`HOVER_PROPS`]
/// property over `var(--motion-fast)` or at most 140ms.
fn hover_transition(v: &str) -> bool {
    v.split(',').all(|part| {
        let mut words = part.split_whitespace();
        let prop = words.next().unwrap_or("");
        let fast = words.any(|w| {
            w == "var(--motion-fast)"
                || w.strip_suffix("ms")
                    .and_then(|n| n.parse::<f64>().ok())
                    .is_some_and(|n| n <= 140.0)
                || (!w.ends_with("ms")
                    && w.strip_suffix('s')
                        .and_then(|n| n.parse::<f64>().ok())
                        .is_some_and(|n| n <= 0.14))
        });
        HOVER_PROPS.contains(&prop) && fast
    })
}

/// The offset just past the bracket that closes the one at `open`.
fn past_close(js: &str, open: usize) -> Option<usize> {
    let b = js.as_bytes();
    let (o, c) = match b[open] {
        b'(' => (b'(', b')'),
        b'{' => (b'{', b'}'),
        _ => return None,
    };
    let mut depth = 0usize;
    for (i, &ch) in b.iter().enumerate().skip(open) {
        if ch == o {
            depth += 1;
        } else if ch == c {
            depth -= 1;
            if depth == 0 {
                return Some(i + 1);
            }
        }
    }
    None
}

/// Whether a `motionOk()` test in the function body opening at `body`
/// guards the `.animate(` at `at`: an `if (!motionOk() …)` at the body's top
/// level whose statement returns, before the call, or an
/// `if (motionOk() …) { … }` whose block holds the call. Each condition may
/// add terms only with the operator that keeps the guard (`||` after the
/// negated test, `&&` after the plain one).
fn motion_guarded(js: &str, body: usize, at: usize) -> bool {
    js[body..at].match_indices("motionOk()").any(|(k, _)| {
        let p = body + k;
        let head = js[..p].trim_end();
        let (negated, head) = match head.strip_suffix('!') {
            Some(h) => (true, h.trim_end()),
            None => (false, head),
        };
        let Some(before_paren) = head.strip_suffix('(') else {
            return false;
        };
        let kw = before_paren.trim_end();
        if !kw.ends_with("if")
            || kw[..kw.len() - 2]
                .chars()
                .last()
                .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
        {
            return false;
        }
        let open = before_paren.len();
        let Some(close) = past_close(js, open) else {
            return false;
        };
        let cond = &js[open + 1..close - 1];
        let rest_at = close + (js[close..].len() - js[close..].trim_start().len());
        let rest = &js[rest_at..];
        if negated {
            let top =
                js[body + 1..open].matches('{').count() == js[body + 1..open].matches('}').count();
            if cond.contains("&&") || !top {
                return false;
            }
            if rest.starts_with("return") {
                return rest_at < at;
            }
            if !rest.starts_with('{') {
                return false;
            }
            let Some(end) = past_close(js, rest_at) else {
                return false;
            };
            let last = js[rest_at + 1..end - 1]
                .trim_end()
                .trim_end_matches(';')
                .rsplit([';', '{', '}'])
                .next()
                .unwrap_or("")
                .trim();
            end <= at && (last == "return" || last.starts_with("return "))
        } else {
            !cond.contains("||")
                && rest.starts_with('{')
                && past_close(js, rest_at).is_some_and(|end| at < end)
        }
    })
}

/// The innermost function body enclosing `at` in `js`, as its `{` offset.
fn enclosing_function(js: &str, at: usize) -> Option<usize> {
    let b = js.as_bytes();
    let mut depth = 0usize;
    let mut i = at;
    while i > 0 {
        i -= 1;
        match b[i] {
            b'}' => depth += 1,
            b'{' if depth > 0 => depth -= 1,
            b'{' => {
                let head = js[..i].trim_end();
                if head.ends_with("=>") {
                    return Some(i);
                }
                if head.ends_with(')') {
                    let hb = head.as_bytes();
                    let (mut d, mut j) = (0usize, hb.len());
                    while j > 0 {
                        j -= 1;
                        match hb[j] {
                            b')' => d += 1,
                            b'(' => {
                                d -= 1;
                                if d == 0 {
                                    break;
                                }
                            }
                            _ => {}
                        }
                    }
                    let kw: String = head[..j]
                        .trim_end()
                        .chars()
                        .rev()
                        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '$')
                        .collect::<Vec<_>>()
                        .into_iter()
                        .rev()
                        .collect();
                    if !matches!(
                        kw.as_str(),
                        "if" | "for" | "while" | "switch" | "catch" | "with"
                    ) {
                        return Some(i);
                    }
                }
            }
            _ => {}
        }
    }
    None
}

/// Every place motion runs for someone who asked for less, as
/// `file:line: rule: detail`. `css` holds stylesheet text (pages already cut
/// to their `<style>` blocks), `js` script text.
fn motion_violations(css: &[(&str, String)], js: &[(&str, String)]) -> Vec<String> {
    let mut bad = Vec::new();
    let mut defined: Vec<(String, String)> = Vec::new();
    let mut used: BTreeSet<String> = BTreeSet::new();
    for (rel, raw) in css {
        let text = strip_comments(raw);
        let b = text.as_bytes();
        // Each open block: (gated by no-preference, a @keyframes body).
        let mut open: Vec<(bool, bool)> = Vec::new();
        for i in 0..b.len() {
            match b[i] {
                b'{' => {
                    let head = text[..i].rsplit(['}', ';', '{']).next().unwrap_or("");
                    let squeezed: String = head.chars().filter(|c| !c.is_whitespace()).collect();
                    let gate = squeezed.contains("prefers-reduced-motion:no-preference");
                    let frames = head.trim_start().starts_with("@keyframes");
                    if frames {
                        let name = head.trim().trim_start_matches("@keyframes").trim();
                        defined.push((format!("{rel}:{}", line_of(&text, i)), name.to_owned()));
                    }
                    open.push((gate, frames));
                }
                b'}' => {
                    open.pop();
                }
                _ => {}
            }
            if !text.is_char_boundary(i)
                || open.iter().any(|(_, f)| *f)
                || (i > 0 && (word(b[i - 1]) || b[i - 1] == b'-'))
            {
                continue;
            }
            let Some(prop) = ["animation-name", "animation", "transition"]
                .into_iter()
                .find(|p| {
                    text[i..].starts_with(p) && text[i + p.len()..].trim_start().starts_with(':')
                })
            else {
                continue;
            };
            let after = &text[i + prop.len()..];
            let v = after[after.find(':').unwrap() + 1..]
                .split([';', '}'])
                .next()
                .unwrap_or("")
                .trim();
            let gated = open.iter().any(|(g, _)| *g);
            if prop != "transition" && gated {
                used.extend(
                    v.split(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
                        .map(str::to_owned),
                );
            }
            if gated || v.starts_with("none") || (prop == "transition" && hover_transition(v)) {
                continue;
            }
            bad.push(format!(
                "{rel}:{}: {prop} outside the reduced-motion gate: {v}",
                line_of(&text, i)
            ));
        }
    }
    for (at, name) in defined {
        if !used.contains(&name) {
            bad.push(format!(
                "{at}: unused @keyframes {name}: no rule under no-preference runs it"
            ));
        }
    }
    for (rel, raw) in js {
        let text = strip_comments(raw);
        for (at, _) in text.match_indices(".animate(") {
            let ok = enclosing_function(&text, at).is_some_and(|o| motion_guarded(&text, o, at));
            if !ok {
                bad.push(format!(
                    "{rel}:{}: .animate( outside motionOk(): no `if (!motionOk()) return` before it and no `if (motionOk()) {{` around it",
                    line_of(&text, at)
                ));
            }
        }
    }
    bad
}

/// Motion in every page runs only for someone who has not asked for less:
/// every animation and transition sits inside
/// `prefers-reduced-motion: no-preference` (a fast colour or opacity hover
/// fade excepted), every scripted `.animate(` sits in a function that tests
/// `motionOk()`, and no keyframe exists that only ungated rules could run.
#[test]
fn every_animation_respects_reduced_motion() {
    let css: Vec<(&str, String)> = MOTION_CSS
        .iter()
        .map(|rel| (*rel, read(rel)))
        .chain(
            MOTION_PAGES
                .iter()
                .map(|rel| (*rel, style_blocks(&read(rel)))),
        )
        .collect();
    let mut scripts: Vec<String> = std::fs::read_dir(Path::new(ROOT).join(UI))
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("js"))
        .map(|p| format!("{UI}/{}", p.file_name().unwrap().to_string_lossy()))
        .chain(MOTION_SCRIPTS.iter().map(|s| s.to_string()))
        .collect();
    scripts.sort();
    let js: Vec<(&str, String)> = scripts
        .iter()
        .map(|rel| (rel.as_str(), read(rel)))
        .collect();
    let bad = motion_violations(&css, &js);
    assert!(
        bad.is_empty(),
        "motion outside the reduced-motion gate:\n{}",
        bad.join("\n")
    );
}

/// `motionOk`, `glide` and `swap` from `text`, each line trimmed.
fn motion_helpers(rel: &str, text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let ok = text
        .lines()
        .find(|l| l.trim_start().starts_with("const motionOk = "))
        .unwrap_or_else(|| panic!("{rel}: no motionOk"));
    out.push(ok.trim().to_owned());
    for head in ["function glide(", "function swap("] {
        let at = text
            .find(head)
            .unwrap_or_else(|| panic!("{rel}: no {head}"));
        let open = at + text[at..].find('{').unwrap();
        let mut depth = 0;
        let mut end = text.len();
        for (j, c) in text[open..].char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = open + j + 1;
                        break;
                    }
                }
                _ => {}
            }
        }
        out.extend(text[at..end].lines().map(|l| l.trim().to_owned()));
    }
    out
}

/// `swapPane` from `text`, each line trimmed.
fn swap_pane(rel: &str, text: &str) -> Vec<String> {
    let at = text
        .find("function swapPane(")
        .unwrap_or_else(|| panic!("{rel}: no swapPane"));
    let open = at + text[at..].find('{').unwrap();
    let mut depth = 0;
    for (j, c) in text[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return text[at..open + j + 1]
                        .lines()
                        .map(|l| l.trim().to_owned())
                        .collect();
                }
            }
            _ => {}
        }
    }
    panic!("{rel}: swapPane never closes")
}

/// The window's panes and the Flows page's views cross-fade with the same
/// `swapPane`; the report has no panes.
#[test]
fn swap_pane_is_identical_in_the_window_and_the_flows_page() {
    assert_eq!(
        swap_pane("src/flows.html", &read("src/flows.html")),
        swap_pane("kit.js", &read("apps/desktop/ui/kit.js")),
        "src/flows.html's swapPane differs from kit.js"
    );
}

/// The window, the report and the Flows page each carry their own copy of the
/// motion helpers (separate documents, no shared script); the copies match.
#[test]
fn motion_helpers_are_identical_in_every_document() {
    let window = motion_helpers("kit.js", &read("apps/desktop/ui/kit.js"));
    for rel in ["src/report.html", "src/flows.html"] {
        assert_eq!(
            motion_helpers(rel, &read(rel)),
            window,
            "{rel}'s motionOk/glide/swap differ from kit.js"
        );
    }
}
