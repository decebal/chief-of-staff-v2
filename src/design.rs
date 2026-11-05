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
