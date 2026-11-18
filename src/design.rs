//! The design pack for the pages `cos` serves itself: the report, the docs
//! room and the MCP app view. One token file, `apps/desktop/ui/tokens.css`,
//! is the source for these and for the desktop window; here it is inlined
//! with the two Geist fonts as `data:` URLs, so a page needs no network and
//! no file beside it. `tests/design.rs` holds every page to it.

use std::sync::OnceLock;

use anyhow::{Result, ensure};

/// The token file as the desktop window links it.
pub const TOKENS: &str = include_str!("../apps/desktop/ui/tokens.css");
const SANS: &[u8] = include_bytes!("../apps/desktop/ui/vendor/geist/Geist-Variable.woff2");
const MONO: &[u8] = include_bytes!("../apps/desktop/ui/vendor/geist/GeistMono-Variable.woff2");
const SANS_URL: &str = "url(\"vendor/geist/Geist-Variable.woff2\")";
const MONO_URL: &str = "url(\"vendor/geist/GeistMono-Variable.woff2\")";

/// Where a page's first `<style>` takes the tokens.
pub const SLOT: &str = "/*__COS_TOKENS__*/";

/// The cos mark on its ink tile, as the window's header shows it; a
/// page puts `<!--__COS_MARK__-->` where it wants it.
pub const MARK: &str = include_str!("../apps/desktop/ui/mark.svg");

/// The tokens with the fonts embedded.
pub fn css() -> &'static str {
    static CSS: OnceLock<String> = OnceLock::new();
    CSS.get_or_init(|| {
        TOKENS
            .replacen(SANS_URL, &data_url(SANS), 1)
            .replacen(MONO_URL, &data_url(MONO), 1)
    })
}

/// `template` with [`css`] in its [`SLOT`] and [`MARK`] wherever it asks.
pub fn inline(template: &str) -> Result<String> {
    ensure!(template.contains(SLOT), "page template lost its token slot");
    Ok(template
        .replacen(SLOT, css(), 1)
        .replace("<!--__COS_MARK__-->", MARK.trim()))
}

/// The window's icon set (`apps/desktop/ui/vendor/lucide`), read here so a
/// served page draws the same icons without a script.
const LUCIDE: &str = include_str!("../apps/desktop/ui/vendor/lucide/icons-0.556.0.js");

/// Lucide's `name` as inline SVG (class `ico`, stroked in the text's colour,
/// `aria-hidden`: the label beside it says it in words). Empty when the set
/// lacks the name. Values are the vendored file's own; nothing a user wrote
/// reaches this markup.
pub fn icon(name: &str) -> String {
    let key = format!("\n  \"{name}\": ");
    let Some(at) = LUCIDE.find(&key) else {
        return String::new();
    };
    let line = LUCIDE[at + key.len()..]
        .lines()
        .next()
        .unwrap_or_default()
        .replace("[ \"", "[\"");
    let mut out = String::from(r#"<svg class="ico" viewBox="0 0 24 24" aria-hidden="true">"#);
    let mut rest = line.as_str();
    while let Some(i) = rest.find("[\"") {
        rest = &rest[i + 2..];
        let Some(j) = rest.find('"') else { break };
        let tag = &rest[..j];
        let (Some(open), Some(close)) = (rest.find('{'), rest.find('}')) else {
            break;
        };
        out.push('<');
        out.push_str(tag);
        let mut attrs = &rest[open + 1..close];
        while let Some(colon) = attrs.find(':') {
            let name = attrs[..colon].trim().trim_start_matches(',').trim();
            let after = &attrs[colon + 1..];
            let (Some(q1), Some(q2)) = (
                after.find('"'),
                after[after.find('"').unwrap_or(0) + 1..].find('"'),
            ) else {
                break;
            };
            let value = &after[q1 + 1..q1 + 1 + q2];
            out.push_str(&format!(r#" {name}="{value}""#));
            attrs = &after[q1 + q2 + 2..];
        }
        out.push_str("/>");
        rest = &rest[close + 1..];
    }
    out.push_str("</svg>");
    out
}

/// Where a page's one script takes vgpu, the GPU engine's library: the Flows
/// page, with an SVG engine beside it (`tests/design.rs`).
pub const VGPU_SLOT: &str = "/*__COS_VGPU__*/";
const VGPU: &str = include_str!("../apps/desktop/ui/vendor/vgpu/vgpu-0.6.0.iife.js");

/// vgpu, made safe to sit inside a page's one `<script>`.
pub fn vgpu_js() -> &'static str {
    static JS: OnceLock<String> = OnceLock::new();
    JS.get_or_init(|| format!("{}\n;\n", script_safe(VGPU)))
}

/// `js` with its source-map comment dropped and every `<!--`, `<script` and
/// `</script` written as `\x3C…`: inside a script element those sequences
/// change how the HTML parser finds the closing tag. `\x3C` means `<` in a JS
/// string, template or regex alike, which is where the bundles carry them.
pub fn script_safe(js: &str) -> String {
    let mut out = String::with_capacity(js.len());
    for line in js.lines() {
        if line.trim_start().starts_with("//# sourceMappingURL=") {
            continue;
        }
        let mut rest = line;
        while let Some(i) = rest.find('<') {
            out.push_str(&rest[..i]);
            let tail = &rest.as_bytes()[i + 1..];
            let opens = |p: &[u8]| tail.len() >= p.len() && tail[..p.len()].eq_ignore_ascii_case(p);
            if opens(b"!--") || opens(b"script") || opens(b"/script") {
                out.push_str("\\x3C");
            } else {
                out.push('<');
            }
            rest = &rest[i + 1..];
        }
        out.push_str(rest);
        out.push('\n');
    }
    out
}

/// Data as JSON that can sit inside a `<script>`: every `<` escaped, which
/// JSON allows only inside strings, so no value changes.
pub fn script_json<T: serde::Serialize>(data: &T) -> Result<String> {
    Ok(serde_json::to_string(data)?.replace('<', "\\u003c"))
}

fn data_url(font: &[u8]) -> String {
    format!("url(\"data:font/woff2;base64,{}\")", base64(font))
}

pub fn base64(bytes: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, &b)| n | (u32::from(b) << (16 - 8 * i)));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ABC[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}
