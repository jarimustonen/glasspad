//! Pure sanitization and text-extraction decisions for artifact metadata.
//!
//! Inputs are already-loaded HTML or producer labels. Returned values are safe
//! display text; HTTP and filesystem handling remain at the CLI edge.

/// Upper bound on resolved artifact titles.
pub const MAX_TITLE_CHARS: usize = 200;
/// Upper bound on generated landing-page descriptions.
pub const MAX_DESC_CHARS: usize = 300;

/// Sanitize a producer-supplied display label (a group label, member title, or
/// member description): strip invisible/bidi spoof chars, collapse whitespace, trim,
/// and length-bound. Returns `None` for an absent or (post-sanitize) empty value, so
/// a manifest/wire label can never smuggle a spoof/zero-width run into the chrome or
/// landing — both of which insert it as text (shell `textContent`, landing escaped).
///
/// Unlike a *resolved artifact title* (which is parsed out of HTML, so
/// [`resolve_title`] entity-decodes it), a group label/title/desc is **plain
/// producer text** from YAML/JSON — it is NOT entity-decoded. Decoding it would (a)
/// be wrong (`R&amp;D` in a label means the literal five characters, not `R&D`) and
/// (b) be **non-idempotent** across the scan→wire→ingest→reload passes (nested
/// entities like `&amp;lt;` would mutate on each hop), so the same label could render
/// differently locally vs. hosted, or drift after a restart. This function is
/// idempotent: `f(f(x)) == f(x)`.
pub fn sanitize_label(raw: Option<&str>, max: usize) -> Option<String> {
    let raw = raw?;
    let cleaned = strip_unsafe_display_chars(&collapse_ws(raw.trim().to_string()));
    bounded_nonempty(&cleaned, max)
}

/// Sanitize producer text that historically uses HTML-title semantics: decode the
/// supported entities, strip spoofing controls, trim, and length-bound it.
pub fn sanitize_html_label(raw: &str, max: usize) -> Option<String> {
    let cleaned = strip_unsafe_display_chars(&decode_entities(raw.trim()));
    bounded_nonempty(&cleaned, max)
}

fn bounded_nonempty(cleaned: &str, max: usize) -> Option<String> {
    let cleaned = cleaned.trim();
    if cleaned.is_empty() {
        None
    } else {
        Some(bound_chars(cleaned, max))
    }
}

// --- title parsing (a tokenizer, not a regex) ------------------------------

/// Resolve an artifact title: `<title>` first, else the first `<h1>`. Parsed with
/// a small tag-aware scanner (case-insensitive, attribute-tolerant), entity-
/// decoded, whitespace-collapsed, and length-bounded. Returned `None` when
/// neither is present. The value is inserted downstream as **text**, never HTML.
pub fn resolve_title(html: &str) -> Option<String> {
    if let Some(t) = extract_element_text(html, "title", MAX_TITLE_CHARS) {
        return Some(t);
    }
    extract_element_text(html, "h1", MAX_TITLE_CHARS)
}

/// Extract a short description for the generated landing page: the text of the
/// first `<p>` in the (already-rendered) artifact body, sanitized + length-bounded
/// exactly like a title. `None` when there is no paragraph (a manifest `desc:`
/// then wins, else no kicker). Inserted into the landing as escaped text.
pub fn extract_description(html: &str) -> Option<String> {
    extract_element_text(html, "p", MAX_DESC_CHARS)
}

/// Extract the text content of the first `<tag>…</tag>`. Not a regex: it walks the
/// byte stream tracking tag boundaries so attributes (incl. a `>` inside a quoted
/// value), whitespace, case, HTML comments, and a look-alike close tag
/// (`</titlebar>`) don't fool it. (It does not yet skip `<script>`/`<style>`
/// raw-text bodies or distinguish SVG/MathML `<title>` — see the module notes;
/// the value is inserted as text, so those are correctness-only gaps.)
fn extract_element_text(html: &str, tag: &str, max: usize) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // Skip HTML comments so `<!-- <title>x</title> -->` never matches. An
        // unterminated comment swallows the rest of the document → no title.
        // Compare on **bytes**, not `lower[i..]` (a str slice): `i` walks
        // byte-by-byte and can land inside a multi-byte char (a leading BOM,
        // an accented/emoji prefix before the first tag), where slicing a `str`
        // at that index panics. Byte-prefix comparison is boundary-safe, and
        // `-->`/tag names are ASCII so the match is identical.
        if bytes[i..].starts_with(b"<!--") {
            let end = lower[i + 4..].find("-->")?;
            i += 4 + end + 3;
            continue;
        }
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        // Try to match `<tag` followed by a delimiter (space, >, /, tab, newline).
        let after = i + 1;
        if lower[after..].starts_with(tag) {
            let j = after + tag.len();
            let delim = bytes.get(j).copied();
            if matches!(
                delim,
                Some(b' ') | Some(b'>') | Some(b'/') | Some(b'\t') | Some(b'\n') | Some(b'\r')
            ) {
                let (content_start, self_closing) = end_of_open_tag(bytes, j)?;
                if self_closing {
                    return None; // `<title/>` has no text content
                }
                let content_end = find_close_tag(&lower, content_start, tag)?;
                let raw = &html[content_start..content_end];
                // Decode entities BEFORE collapsing whitespace, so `&nbsp;` folds;
                // then strip invisible/bidi chars that could reorder or spoof the
                // visible label (it lands in the trusted nav + `document.title`).
                let text =
                    strip_unsafe_display_chars(&collapse_ws(decode_entities(&strip_tags(raw))))
                        .trim()
                        .to_string();
                if text.is_empty() {
                    return None;
                }
                return Some(bound_chars(&text, max));
            }
        }
        i = after;
    }
    None
}

/// Find the end of an opening tag, starting just after the tag name. Returns
/// `(content_start, self_closing)`. A `>` inside a quoted attribute value does
/// **not** terminate the tag.
fn end_of_open_tag(bytes: &[u8], from: usize) -> Option<(usize, bool)> {
    let mut i = from;
    let mut quote: Option<u8> = None;
    let mut last_non_ws = 0u8;
    while i < bytes.len() {
        let b = bytes[i];
        match quote {
            Some(q) => {
                if b == q {
                    quote = None;
                }
            }
            None => match b {
                b'"' | b'\'' => quote = Some(b),
                b'>' => return Some((i + 1, last_non_ws == b'/')),
                _ => {}
            },
        }
        if !b.is_ascii_whitespace() {
            last_non_ws = b;
        }
        i += 1;
    }
    None
}

/// Find the matching `</tag>` at or after `start`, requiring the tag name be
/// followed by a real delimiter so `</titlebar>` doesn't close `</title>`.
/// Returns the byte index of the `<` of the close tag.
fn find_close_tag(lower: &str, start: usize, tag: &str) -> Option<usize> {
    let needle = format!("</{tag}");
    let bytes = lower.as_bytes();
    let mut from = start;
    while let Some(rel) = lower[from..].find(&needle) {
        let pos = from + rel;
        let after = pos + needle.len();
        match bytes.get(after) {
            Some(b'>') | Some(b' ') | Some(b'\t') | Some(b'\n') | Some(b'\r') | Some(b'/')
            | None => {
                return Some(pos);
            }
            _ => from = after, // e.g. `</titlebar>` — keep looking
        }
    }
    None
}

/// Drop any nested tags inside the extracted content (e.g. `<h1><span>x</span>`).
fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

/// Collapse runs of ASCII whitespace to single spaces.
fn collapse_ws(s: String) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_ws = false;
    for c in s.chars() {
        if c.is_whitespace() {
            if !prev_ws {
                out.push(' ');
            }
            prev_ws = true;
        } else {
            out.push(c);
            prev_ws = false;
        }
    }
    out
}

/// Remove characters that render invisibly or reorder surrounding text. A
/// resolved title is inserted into the **trusted** nav chrome (as `textContent`)
/// and set as `document.title`; it can never execute, but a bidi override
/// (`U+202E`) or zero-width run could reorder/spoof the visible label or the tab
/// title. These are stripped at resolution time so both the `<title>` and the nav
/// see a clean string. Ordinary whitespace controls are already folded to spaces
/// by `collapse_ws`; this targets the *non*-whitespace controls, the bidi
/// embeddings/overrides/isolates, and the zero-width/BOM marks.
fn strip_unsafe_display_chars(s: &str) -> String {
    s.chars().filter(|&c| !is_unsafe_display_char(c)).collect()
}

fn is_unsafe_display_char(c: char) -> bool {
    matches!(c,
        // C0 controls (non-whitespace) + DEL. Whitespace controls (U+0009..U+000D)
        // are left to collapse_ws; the rest have no place in a display label.
        '\u{0000}'..='\u{0008}' | '\u{000e}'..='\u{001f}' | '\u{007f}'
        // C1 controls.
        | '\u{0080}'..='\u{009f}'
        // Bidi embeddings / overrides / isolates (LRE/RLE/PDF/LRO/RLO, LRI..PDI).
        | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'
        // Directional marks, zero-width space/non-joiner/joiner, word joiner, BOM.
        | '\u{200b}'..='\u{200f}' | '\u{2060}' | '\u{feff}'
    )
}

/// Bound a string to `max` chars (not bytes) without splitting a char.
fn bound_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    s.chars().take(max).collect()
}

/// Decode the small set of HTML entities that realistically appear in titles.
/// Unknown entities are left verbatim (the value is rendered as text anyway).
fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'&' {
            // Push the full UTF-8 char starting at i.
            let ch = s[i..].chars().next().unwrap();
            out.push(ch);
            i += ch.len_utf8();
            continue;
        }
        if let Some(semi) = s[i..].find(';').filter(|&off| off <= 10) {
            let ent = &s[i + 1..i + semi];
            let decoded = match ent {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" | "#39" => Some('\''),
                "nbsp" => Some('\u{00a0}'),
                _ => decode_numeric(ent),
            };
            if let Some(c) = decoded {
                out.push(c);
                i += semi + 1;
                continue;
            }
        }
        out.push('&');
        i += 1;
    }
    out
}

fn decode_numeric(ent: &str) -> Option<char> {
    let num = ent.strip_prefix('#')?;
    let code = if let Some(hex) = num.strip_prefix(['x', 'X']) {
        u32::from_str_radix(hex, 16).ok()?
    } else {
        num.parse::<u32>().ok()?
    };
    char::from_u32(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hostile_title_markup_becomes_bounded_inert_text() {
        let title =
            resolve_title("<title>x\"><img src=x onerror=alert(1)>\u{202e}safe</title>").unwrap();
        assert_eq!(title, "x\"safe");
        assert!(!title.contains('<'));
        assert!(!title.contains("onerror"));
    }

    #[test]
    fn producer_label_sanitization_is_idempotent() {
        let once = sanitize_label(Some("R&amp;D  \u{200b}core"), MAX_TITLE_CHARS).unwrap();
        assert_eq!(once, "R&amp;D core");
        assert_eq!(sanitize_label(Some(&once), MAX_TITLE_CHARS), Some(once));
    }
}
