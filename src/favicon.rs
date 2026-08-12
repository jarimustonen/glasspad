//! Emoji SVG favicon for the OUTER served/built document (never the sandbox).
//!
//! A configured emoji (`.glasspad.yaml` `favicon:`, resolved by [`crate::config`])
//! is rendered as a zero-dependency inline **SVG favicon** and referenced from the
//! **outer** shell / build `<head>` via `<link rel="icon">`. It lives on the served
//! document only — the artifact sandbox iframe (its `srcdoc`/content route, CSP, and
//! sandbox tokens) is untouched.
//!
//! Two independent defenses keep the emoji from becoming markup (AI-first §1):
//!
//! 1. [`validate`] rejects a configured value that is clearly not a favicon glyph —
//!    empty, overlong, any control character, any Unicode whitespace, the invisible
//!    bidi/zero-width format controls, Unicode noncharacters, the HTML/URI
//!    metacharacters `< > & " ' / \` `` ` ``, or pure ASCII (a favicon needs a
//!    non-ASCII glyph). It is a **heuristic**, not a full Unicode emoji-property check
//!    (that needs a large dependency; glasspad stays zero-dep), so it accepts a short
//!    non-ASCII glyph — a plain emoji or a ZWJ sequence — and does not enforce a single
//!    grapheme. It runs at **every ingress** (the CLI at publish/serve/build time, and
//!    again server-side in the hosted ingest — the untrusted API boundary).
//! 2. [`link_tag`] XML-escapes the emoji into the SVG `<text>` (defense-in-depth on
//!    top of `validate`) and then base64-encodes the whole SVG into the `data:` URI,
//!    so the outer HTML attribute carries only `[A-Za-z0-9+/=]` — no injection surface
//!    even if a value ever reached it unvalidated.

use base64::Engine as _;

/// The built-in default favicon emoji, used when a repo configures none. Glasspad
/// hosts rich data views (dashboards, charts) — a bar chart is the house glyph.
pub const DEFAULT_EMOJI: &str = "📊";

/// Max Unicode scalar values in a configured favicon. Generous: a ZWJ family/flag
/// emoji is ~7–8 scalars (base + ZWJ + variation selectors); this only rejects a
/// pasted blob of text/markup, never a real single emoji sequence.
const MAX_SCALARS: usize = 16;

/// Validate a configured favicon. On success returns the trimmed value; on failure
/// an **informative** message (AI-first §1: strict, never a silent fallback — the
/// caller surfaces it as a hard error). This is a **heuristic**, not a full Unicode
/// emoji-property check (which would need a large dependency; glasspad stays
/// zero-dep): it accepts a short, non-ASCII glyph — a plain emoji or a ZWJ sequence —
/// and rejects the shapes that are clearly *not* a favicon: empty, an overlong value,
/// **any** control character (`char::is_control` — C0/C1/DEL), **any** Unicode
/// whitespace (`char::is_whitespace` — so NBSP / em-space / line & paragraph
/// separators can't slip past), the invisible bidi/zero-width format controls
/// (U+200B/C, U+200E/F, U+202A‑E, U+2060, U+2066‑9, U+FEFF — but **not** ZWJ U+200D,
/// which real emoji sequences need), Unicode **noncharacters** (which would make the
/// SVG XML ill-formed), the HTML/URI metacharacters, and a value that is pure ASCII
/// (a favicon needs a non-ASCII glyph). It does **not** enforce a single grapheme, so
/// e.g. two emoji still pass — see the module docs.
pub fn validate(raw: &str) -> Result<String, String> {
    let s = raw.trim();
    if s.is_empty() {
        return Err(
            "favicon is empty: set an emoji (e.g. `favicon: 🚀`) or omit the key to use the default"
                .to_string(),
        );
    }
    let count = s.chars().count();
    if count > MAX_SCALARS {
        return Err(format!(
            "favicon {s:?} is too long ({count} code points, max {MAX_SCALARS}): use a single emoji"
        ));
    }
    let mut has_non_ascii = false;
    for c in s.chars() {
        // `is_control` covers C0, C1, and DEL (not just the ASCII range); `is_whitespace`
        // covers NBSP and the Unicode separators an `is_ascii_whitespace` check misses.
        if c.is_control() {
            return Err(format!(
                "favicon {s:?} contains a control character: a favicon must be a plain glyph"
            ));
        }
        if c.is_whitespace() {
            return Err(format!(
                "favicon {s:?} contains whitespace: use a single emoji with no spaces"
            ));
        }
        if is_disallowed_format(c) {
            return Err(format!(
                "favicon {s:?} contains an invisible bidi/zero-width control ({c:?}): a favicon \
                 must be a visible glyph"
            ));
        }
        if is_noncharacter(c) {
            return Err(format!(
                "favicon {s:?} contains a Unicode noncharacter ({c:?}): not a renderable glyph"
            ));
        }
        if matches!(c, '<' | '>' | '&' | '"' | '\'' | '/' | '\\' | '`') {
            return Err(format!(
                "favicon {s:?} contains the markup/URI metacharacter {c:?}: a favicon must be a \
                 glyph, not markup"
            ));
        }
        if !c.is_ascii() {
            has_non_ascii = true;
        }
    }
    if !has_non_ascii {
        return Err(format!(
            "favicon {s:?} is plain ASCII: a favicon must be a non-ASCII glyph (typically an emoji)"
        ));
    }
    Ok(s.to_string())
}

/// Invisible bidi / zero-width **format** controls that must not ride in a favicon —
/// they render nothing and (bidi overrides especially) are a spoofing vector. ZWJ
/// (U+200D) is deliberately **excluded**: real emoji ZWJ sequences (families, flags)
/// need it. `char::is_control` does not catch these (they are category `Cf`, not `Cc`).
fn is_disallowed_format(c: char) -> bool {
    matches!(c,
        '\u{200B}' | '\u{200C}'          // zero-width space, zero-width non-joiner
        | '\u{200E}' | '\u{200F}'        // LTR / RTL marks
        | '\u{202A}'..='\u{202E}'        // bidi embeddings + override
        | '\u{2060}'                     // word joiner
        | '\u{2066}'..='\u{2069}'        // bidi isolates
        | '\u{FEFF}'                     // BOM / zero-width no-break space
    )
}

/// Unicode noncharacters (U+FDD0..U+FDEF and the last two code points of every plane,
/// `xFFFE`/`xFFFF`). They are permanently unassigned and would make the SVG XML
/// ill-formed, so a favicon must not contain one.
fn is_noncharacter(c: char) -> bool {
    let u = c as u32;
    (0xFDD0..=0xFDEF).contains(&u) || (u & 0xFFFE) == 0xFFFE
}

/// The `<link rel="icon">` tag for `emoji` (a value that has already passed
/// [`validate`]), or for the [`DEFAULT_EMOJI`] when `None`. Infallible: the emoji is
/// XML-escaped into the SVG (defense-in-depth on top of `validate`) and the SVG is
/// base64-encoded into the `data:` URI, so the emitted attribute value can never
/// carry markup. This is the single string the outer `<head>` embeds.
pub fn link_tag(emoji: Option<&str>) -> String {
    let emoji = emoji.unwrap_or(DEFAULT_EMOJI);
    let svg = svg_document(emoji);
    let b64 = base64::engine::general_purpose::STANDARD.encode(svg.as_bytes());
    format!(r#"<link rel="icon" type="image/svg+xml" href="data:image/svg+xml;base64,{b64}">"#)
}

/// The inline SVG document carrying the emoji as centered `<text>`. The emoji is
/// XML-escaped so the SVG is well-formed even for a value that slipped past
/// `validate` (belt-and-suspenders — `validate` already rejects `<`/`>`/`&`).
fn svg_document(emoji: &str) -> String {
    // Name an explicit emoji-font stack: without it, Linux/Windows often render an
    // emoji in an SVG `<text>` node as a tofu box or a monochrome outline. The stack
    // is a static constant (no injection surface).
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><text x="50%" y="50%" dominant-baseline="central" text-anchor="middle" font-size="52" font-family="'Apple Color Emoji','Segoe UI Emoji','Noto Color Emoji','Segoe UI Symbol',system-ui,sans-serif">{}</text></svg>"#,
        xml_escape(emoji)
    )
}

/// XML-escape text for an SVG element text node: neutralize `&`, `<`, `>` (and the
/// quotes for good measure) so the value cannot break out of the `<text>` element.
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_accepts_common_emoji() {
        for e in ["🚀", "📊", "🌟", "🦀", "👩‍🚀", "🏳️‍🌈", "1️⃣"] {
            assert!(validate(e).is_ok(), "{e:?} should be a valid favicon");
        }
    }

    #[test]
    fn validate_trims_surrounding_whitespace() {
        assert_eq!(validate("  🚀 ").unwrap(), "🚀");
    }

    #[test]
    fn validate_rejects_empty_and_ascii() {
        assert!(validate("").is_err());
        assert!(validate("   ").is_err());
        assert!(validate("x").is_err());
        assert!(validate("ab").is_err());
    }

    #[test]
    fn validate_rejects_markup_and_injection() {
        // The exact injection shapes the SVG/text context must never admit.
        for bad in [
            "<script>",
            "\"></text><script>alert(1)</script>",
            "</text>",
            "&amp;",
            "a<b",
            "'/>",
            "`",
        ] {
            assert!(validate(bad).is_err(), "{bad:?} must be rejected");
        }
    }

    #[test]
    fn validate_rejects_control_and_whitespace_and_overlong() {
        assert!(validate("🚀\u{7f}").is_err()); // DEL
        assert!(validate("🚀\u{85}🚀").is_err()); // internal NEL — a C1 control
        assert!(validate("🚀\n🚀").is_err()); // internal newline (trailing would trim)
        assert!(validate("🚀 🚀").is_err()); // internal space
        assert!(validate("🚀\u{00a0}🚀").is_err()); // no-break space (Unicode whitespace)
        assert!(validate("🚀\u{2003}🚀").is_err()); // internal em space (Unicode whitespace)
        assert!(validate(&"🚀".repeat(17)).is_err()); // over the scalar cap
    }

    #[test]
    fn validate_rejects_invisible_format_and_noncharacters() {
        // Zero-width / bidi format controls are invisible (and a spoofing vector).
        assert!(validate("🚀\u{200b}").is_err()); // zero-width space
        assert!(validate("🚀\u{202e}").is_err()); // RIGHT-TO-LEFT OVERRIDE
        assert!(validate("🚀\u{feff}").is_err()); // BOM / ZWNBSP
        assert!(validate("🚀\u{2066}").is_err()); // bidi isolate
        // Unicode noncharacters would make the SVG XML ill-formed.
        assert!(validate("\u{fffe}").is_err());
        assert!(validate("\u{ffff}").is_err());
        assert!(validate("\u{fdd0}").is_err());
        assert!(validate("\u{1fffe}").is_err());
        // ZWJ (U+200D) is allowed — real emoji sequences need it (already covered by
        // validate_accepts_common_emoji, asserted here explicitly against the denylist).
        assert!(validate("👩\u{200d}🚀").is_ok());
    }

    #[test]
    fn validate_rejects_non_emoji_but_documents_the_heuristic() {
        // The heuristic accepts any short non-ASCII glyph, so a CJK char passes — this
        // pins the DOCUMENTED behavior (validate is not a full emoji-property check).
        assert!(validate("漢").is_ok());
        // Multiple emoji also pass (no single-grapheme enforcement) — documented.
        assert!(validate("🚀🦀").is_ok());
    }

    #[test]
    fn link_tag_default_when_none() {
        let tag = link_tag(None);
        assert!(tag.starts_with(
            r#"<link rel="icon" type="image/svg+xml" href="data:image/svg+xml;base64,"#
        ));
        // The default emoji round-trips through the base64 SVG.
        let b64 = tag.split("base64,").nth(1).unwrap().trim_end_matches("\">");
        let svg = String::from_utf8(
            base64::engine::general_purpose::STANDARD
                .decode(b64)
                .unwrap(),
        )
        .unwrap();
        assert!(svg.contains(DEFAULT_EMOJI));
        assert!(svg.starts_with("<svg"));
    }

    #[test]
    fn link_tag_carries_only_base64_in_the_attribute() {
        // Even a hostile value (were it ever to reach link_tag unvalidated) cannot
        // break out: the attribute holds only base64, and the SVG XML-escapes it.
        let tag = link_tag(Some("\"></text><script>x</script>"));
        // No raw markup metacharacters in the emitted tag beyond our own attribute
        // syntax — the payload's angle brackets/quotes are gone (base64'd away).
        assert!(!tag.contains("<script>"));
        assert!(!tag.contains("</text>"));
        let b64 = tag.split("base64,").nth(1).unwrap().trim_end_matches("\">");
        let svg = String::from_utf8(
            base64::engine::general_purpose::STANDARD
                .decode(b64)
                .unwrap(),
        )
        .unwrap();
        // Inside the SVG the payload is XML-escaped, not live markup.
        assert!(!svg.contains("<script>"));
        assert!(svg.contains("&lt;script&gt;"));
    }
}
