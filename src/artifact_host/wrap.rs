//! Fragment wrapping + the bridge/theme injection point (Wave 3b / Phase 4-bridge).
//!
//! An artifact file is served on the content route either **verbatim** (a full
//! HTML document — the author owns the whole page) or **wrapped** (a fragment —
//! the author wrote only body content). Wrapping is where the first-party scaffold
//! is injected:
//!
//! * `data-theme` is inlined on `<html>` at wrap time so there is **no FOUC** —
//!   the bridge only handles *later* toggles (design.md §6). `auto` (the default)
//!   follows `prefers-color-scheme` purely in CSS, so it never flashes either.
//! * `base.css` (the `--gp-*` design system) is linked once.
//! * `bridge.js` — the child side of the parent↔iframe channel — is injected
//!   **only here**, so a full-document artifact never gets it silently (it opts in
//!   itself and falls back to `target="_top"`).
//!
//! The wrapped document is still served under the frozen Wave-1 artifact CSP
//! (`headers::artifact_csp_from_origins`); nothing here widens it. `base.css`/`bridge.js` load
//! as classic same-host subresources, which `script-src`/`style-src` already
//! permit. The fragment body is inserted verbatim — the artifact is already
//! untrusted script inside the null-origin sandbox, so wrapping adds no new trust
//! boundary (it is NOT sanitization; the sandbox/CSP is the boundary, design.md §7).
//!
//! **Detection scope.** `is_fragment` is BOM/whitespace/comment-tolerant (not a
//! naive prefix check). Wave 3a's CLI formalizes the full detection contract; this
//! is the host-side hook that classifies at serve time.

/// The theme inlined into a wrapped fragment. `Auto` follows `prefers-color-scheme`
/// (handled in `base.css`), so it is both the safe default and FOUC-free.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Theme {
    Auto,
    Light,
    Dark,
}

impl Theme {
    /// Parse a request-supplied theme token. Anything outside the fixed allowlist
    /// (including `None`) resolves to `Auto` — a query string can never inject an
    /// arbitrary attribute value, only pick one of three.
    pub fn from_query(value: Option<&str>) -> Theme {
        match value {
            Some("light") => Theme::Light,
            Some("dark") => Theme::Dark,
            _ => Theme::Auto,
        }
    }

    /// The `data-theme` attribute value.
    fn as_attr(self) -> &'static str {
        match self {
            Theme::Auto => "auto",
            Theme::Light => "light",
            Theme::Dark => "dark",
        }
    }
}

/// Does `html` look like a full HTML document (served verbatim) rather than a
/// fragment (wrapped)? Tolerant of a leading UTF-8 BOM, arbitrary leading
/// whitespace, and any number of leading HTML comments — so a document that opens
/// with `<!-- license -->\n<!doctype html>` is still recognized as full. The
/// signal is the first *real* markup token being `<!doctype …>` or an `<html …>`
/// tag; a bare `<h1>`/`<div>`/text start is a fragment.
pub fn is_full_document(html: &str) -> bool {
    let rest = skip_prelude(html);
    let lower_head: String = rest
        .chars()
        .take(16)
        .collect::<String>()
        .to_ascii_lowercase();
    starts_with_doctype(&lower_head) || starts_with_html_tag(&lower_head)
}

/// A fragment is anything that is not a full document.
pub fn is_fragment(html: &str) -> bool {
    !is_full_document(html)
}

/// A real tag/name delimiter — the byte that must follow `doctype`/`html` for it
/// to be that token rather than a longer name (`<!doctypeevil>`, `<htmlx>`).
/// The whitespace set is HTML's ASCII whitespace (tab, LF, form-feed, CR, space),
/// so `<html\u{c}lang>`/`<!doctype\u{c}html>` are recognized like any other space.
fn is_tag_delim(next: Option<char>) -> bool {
    matches!(
        next,
        None | Some(' ')
            | Some('>')
            | Some('\t')
            | Some('\n')
            | Some('\x0c')
            | Some('\r')
            | Some('/')
    )
}

/// `<!doctype>` / `<!doctype html>` — but not `<!doctypeevil>` (require a delimiter).
fn starts_with_doctype(lower_head: &str) -> bool {
    lower_head
        .strip_prefix("<!doctype")
        .is_some_and(|after| is_tag_delim(after.chars().next()))
}

/// `<html>` or `<html …>` — but not `<htmlx>` (require a tag-name delimiter).
fn starts_with_html_tag(lower_head: &str) -> bool {
    lower_head
        .strip_prefix("<html")
        .is_some_and(|after| is_tag_delim(after.chars().next()))
}

/// Skip a leading BOM, ASCII whitespace, complete leading HTML comments, and a
/// leading XML/processing-instruction prolog (`<?xml …?>`). Stops at the first
/// byte of real content (or an unterminated comment/PI). Skipping the `<?xml?>`
/// prolog keeps a valid XHTML document (`<?xml …?><!doctype html><html>…`)
/// classified as a full document instead of being double-wrapped as a fragment.
fn skip_prelude(html: &str) -> &str {
    let mut s = html.strip_prefix('\u{feff}').unwrap_or(html);
    loop {
        let trimmed = s.trim_start();
        if let Some(after_open) = trimmed.strip_prefix("<!--") {
            if let Some(end) = after_open.find("-->") {
                s = &after_open[end + 3..];
                continue;
            }
            // Unterminated comment: no real markup follows — treat as fragment.
            return "";
        }
        if let Some(after_open) = trimmed.strip_prefix("<?") {
            if let Some(end) = after_open.find("?>") {
                s = &after_open[end + 2..];
                continue;
            }
            // Unterminated processing instruction: no real markup follows.
            return "";
        }
        return trimmed;
    }
}

/// Wrap a fragment into a full, themed document with `base.css` linked and
/// `bridge.js` injected. `theme` is inlined on `<html>` (no FOUC). The fragment
/// body is embedded verbatim (already-untrusted sandboxed content).
///
/// `bridge.js` is injected in `<head>` (with `defer`) **before** the untrusted
/// fragment bytes, so a malformed fragment (an unterminated `<style>`, a
/// `document.write`, a stray close tag) cannot swallow the first-party script the
/// way a trailing `<body>` tag could. It only attaches listeners, so it needs no
/// body. (A *hostile* fragment can still refuse to cooperate — it owns its DOM —
/// but "auto-injected into every fragment" is now a reliable property.)
///
/// `<base target="_top">` makes the document's default hyperlink target the
/// **top-level** browsing context, so a link to *another* hosted page breaks out
/// of the null-origin sandbox instead of navigating in-frame into a shell served
/// `x-frame-options: DENY` + `frame-ancestors 'none'` (which the browser refuses
/// → "refused to connect"). The content iframe already carries the
/// `allow-top-navigation-by-user-activation` sandbox flag, so a user click may
/// perform that top navigation. This does NOT weaken isolation: `<base>` has no
/// `href` (so `base-uri 'none'` has nothing to restrict, and subresource URLs are
/// unaffected — it governs only the default link target), and it does not disturb
/// the bridge's **same-space** relative-link swap: the bridge reads each anchor's
/// own `target` *attribute* (which `<base>` never sets) and `preventDefault`s
/// before any navigation, so those links still swap in place.
pub fn wrap_fragment(fragment: &str, theme: Theme) -> String {
    // The content-version of the (raw, pre-wrap) fragment body — the same value the
    // submit handler recomputes from the stored body — inlined so `bridge.js` can
    // echo it in a submission (cross-round binding). It is a hex digest of the
    // body: header-safe, no escaping needed, and it can carry no markup.
    let version = crate::submissions::content_version(fragment);
    format!(
        r#"<!doctype html>
<html lang="en" data-theme="{theme}">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="color-scheme" content="light dark">
<meta name="gp-content-version" content="{version}">
<base target="_top">
<link rel="stylesheet" href="/_gp/v1/base.css">
<script src="/_gp/v1/bridge.js" defer></script>
</head>
<body>
{fragment}
</body>
</html>
"#,
        theme = theme.as_attr(),
        version = version,
        fragment = fragment
    )
}

/// Serve an artifact: wrap it if it is a fragment, else return it verbatim. This
/// is the single call site the content route uses.
pub fn render_artifact(html: &str, theme: Theme) -> String {
    if is_fragment(html) {
        wrap_fragment(html, theme)
    } else {
        html.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_documents_are_detected() {
        assert!(is_full_document(
            "<!doctype html><html><body>x</body></html>"
        ));
        assert!(is_full_document("<!DOCTYPE HTML>\n<html>…"));
        assert!(is_full_document("<html><head></head></html>"));
        assert!(is_full_document("<html lang=\"en\">…"));
        // Leading BOM + whitespace + a comment must not fool detection.
        assert!(is_full_document("\u{feff}  \n<!doctype html><html>…"));
        assert!(is_full_document(
            "<!-- license header -->\n<!doctype html><html>…"
        ));
        assert!(is_full_document("  <!-- a --> <!-- b --> <html>…"));
        assert!(is_full_document("<!doctype html>")); // bare doctype, no trailing markup
        // A leading XML/XHTML prolog must be skipped, not mistaken for a fragment:
        // `<?xml …?><!doctype html>` is a full document (else it gets double-wrapped).
        assert!(is_full_document(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE html><html>…"
        ));
        assert!(is_full_document(
            "\u{feff}<?xml version=\"1.0\"?><!-- c --><html>…"
        ));
        // Form feed (an HTML ASCII whitespace) is a valid tag-name delimiter.
        assert!(is_full_document("<html\u{c}lang=\"en\">…"));
        assert!(is_full_document("<!doctype\u{c}html>"));
    }

    #[test]
    fn fragments_are_detected() {
        assert!(is_fragment("<h1>Hello</h1><p>world</p>"));
        assert!(is_fragment("<div class=\"card\">…</div>"));
        assert!(is_fragment("plain text with no tags"));
        assert!(is_fragment("<htmlish>not really html</htmlish>")); // not a real <html> tag
        assert!(is_fragment("")); // empty is trivially a fragment
        // A fragment that OPENS with a well-formed leading comment (a license
        // banner, an authoring note) must stay a fragment: skipping the comment
        // reveals a bare `<h1>`/`<div>`, not a doctype/html token. This is the
        // mirror of the full-document `<!-- license --><!doctype html>` case — the
        // comment tolerance must not tip a fragment into "served verbatim".
        assert!(is_fragment("<!-- banner --><h1>Hi</h1>"));
        assert!(is_fragment(
            "\u{feff}  <!-- a --> <!-- b -->\n<div>card</div>"
        ));
        // An unterminated leading comment leaves no real markup → fragment.
        assert!(is_fragment("<!-- never closed <html>"));
        // A PI-like prolog that is NOT followed by a doctype/html token is still a
        // fragment; and an unterminated PI leaves no real markup → fragment.
        assert!(is_fragment("<?xml version=\"1.0\"?><h1>Hi</h1>"));
        assert!(is_fragment("<?xml never closed <html>"));
        // Look-alikes that are NOT a real doctype/html token must not slip through
        // as "full documents" (that would skip wrapping + bridge injection).
        assert!(is_fragment("<!doctypeevil>hi"));
        assert!(is_fragment("<!doctype-html>hi"));
    }

    #[test]
    fn wrap_injects_theme_base_css_and_bridge() {
        let out = wrap_fragment("<h1>hi</h1>", Theme::Dark);
        assert!(out.contains(r#"data-theme="dark""#));
        // The default hyperlink target is the top-level context, so a link to
        // another hosted page breaks out of the sandbox instead of navigating the
        // frame into an `x-frame-options: DENY` shell.
        assert!(out.contains(r#"<base target="_top">"#));
        // `<base>` sits in <head> before the untrusted fragment so it governs the
        // fragment's links, and carries no `href` (nothing for `base-uri 'none'`
        // to reject, subresource URLs untouched).
        let base_pos = out.find(r#"<base target="_top">"#).unwrap();
        assert!(base_pos < out.find("</head>").unwrap());
        assert!(base_pos < out.find("<h1>hi</h1>").unwrap());
        assert!(out.contains(r#"<link rel="stylesheet" href="/_gp/v1/base.css">"#));
        // bridge.js is injected in <head>, BEFORE the untrusted fragment bytes.
        assert!(out.contains(r#"<script src="/_gp/v1/bridge.js" defer></script>"#));
        let head_end = out.find("</head>").unwrap();
        assert!(
            out.find("bridge.js").unwrap() < head_end,
            "bridge.js must be in <head>"
        );
        assert!(
            out.find("<h1>hi</h1>").unwrap() > head_end,
            "fragment body after </head>"
        );
        assert!(out.contains("<h1>hi</h1>"));
        assert!(out.starts_with("<!doctype html>"));
    }

    #[test]
    fn wrap_injects_content_version_meta_matching_the_body() {
        // The wrapped fragment carries the server-computed content-version of the
        // RAW body, so bridge.js can echo it and the submit handler (which recomputes
        // the same value from the stored body) can bind/reject rounds consistently.
        let frag = "<form><input name=q></form>";
        let out = wrap_fragment(frag, Theme::Auto);
        let expected = crate::submissions::content_version(frag);
        assert!(out.contains(&format!(
            r#"<meta name="gp-content-version" content="{expected}">"#
        )));
        // A hex digest — no markup can ride in it.
        assert_eq!(expected.len(), 16);
        assert!(expected.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn theme_from_query_is_allowlisted() {
        assert_eq!(Theme::from_query(Some("dark")), Theme::Dark);
        assert_eq!(Theme::from_query(Some("light")), Theme::Light);
        assert_eq!(Theme::from_query(Some("auto")), Theme::Auto);
        // Anything else — including an injection attempt — collapses to auto.
        assert_eq!(Theme::from_query(Some("dark\"><script>")), Theme::Auto);
        assert_eq!(Theme::from_query(None), Theme::Auto);
        assert_eq!(Theme::from_query(Some("")), Theme::Auto);
    }

    #[test]
    fn render_artifact_wraps_fragment_but_not_full_doc() {
        let full = "<!doctype html><title>t</title><h1>x</h1>";
        assert_eq!(render_artifact(full, Theme::Auto), full); // verbatim
        let frag = "<h1>x</h1>";
        let wrapped = render_artifact(frag, Theme::Auto);
        assert!(wrapped.contains("bridge.js"));
        assert_ne!(wrapped, frag);
    }

    #[test]
    fn interpage_link_carries_top_level_navigation_intent() {
        // Regression (issue hosted-interpage-link-refused): a fragment that links to
        // ANOTHER hosted page must render with `<base target="_top">` so the click
        // navigates the top-level tab, not the sandboxed frame (whose target shell is
        // served `x-frame-options: DENY` → "refused to connect"). The absolute link
        // itself is preserved verbatim; only the default target changes.
        let frag = r#"<a href="https://share.example.com/p/nzmfhhskgustkzlknzktygy6uu/">Lue syväluotaus »</a>"#;
        let out = render_artifact(frag, Theme::Auto);
        assert!(
            out.contains(r#"<base target="_top">"#),
            "inter-page link must carry top-level navigation intent"
        );
        assert!(
            out.contains(r#"href="https://share.example.com/p/nzmfhhskgustkzlknzktygy6uu/""#),
            "the link itself must be preserved"
        );
    }

    #[test]
    fn wrapped_full_doc_never_double_injects_bridge() {
        // A full document is served verbatim, so a fixture that already ships its
        // own scripts never gets a second bridge.js.
        let full = "<!doctype html><html><body><script>1</script></body></html>";
        assert!(!render_artifact(full, Theme::Auto).contains("bridge.js"));
    }
}
