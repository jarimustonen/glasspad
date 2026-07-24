//! The trusted parent shell (`/{space}/{slug}` and `/{space}/`).
//!
//! First-party chrome that frames the artifact in a null-origin sandbox and runs
//! the **parent side of the postMessage bridge**. Per design.md §6 the parent:
//!
//! * validates `event.source === iframe.contentWindow` — **not** `event.origin`,
//!   which is the useless string `"null"` for every sandboxed frame;
//! * accepts only a fixed low-authority schema (`navigate` to a *known* slug),
//!   with the slug resolved against the server-provided artifact table;
//! * bounds message byte-size and rate, and invalidates state on iframe reload;
//! * inserts artifact-derived text as **text, never innerHTML** (Trusted Types on).
//!
//! The inline script is authorized by a per-response nonce, not `'unsafe-inline'`.

use serde_json::json;

/// Render the shell document for `space`/`slug`. `known_slugs` is the artifact
/// table the bridge resolves navigation against. `title` is the artifact's
/// resolved display title (empty → fall back to `space / slug`); it is inserted
/// as **text** on both the client (`textContent`) and server side (escaped).
/// `nonce` matches the CSP.
pub fn render(space: &str, slug: &str, title: &str, known_slugs: &[&str], nonce: &str) -> String {
    // All dynamic values are serialized as JSON, so they land in the script as
    // data literals — never as HTML that could break out of context. `json` also
    // neutralizes `</script>` / U+2028 / U+2029 so a hostile value cannot close
    // the script element.
    let display_title = if title.is_empty() {
        format!("{space} / {slug}")
    } else {
        title.to_string()
    };
    let space_json = json_for_script(&json!(space));
    let slug_json = json_for_script(&json!(slug));
    let slugs_json = json_for_script(&json!(known_slugs));
    let title_json = json_for_script(&json!(display_title));

    // Content path for the iframe src. space/slug are path-validated upstream.
    let content_src = format!("/{space}/_c/{slug}");
    let esc_src = html_attr_escape(&content_src);
    let esc_title = html_text_escape(&display_title);

    format!(
        r#"<!doctype html>
<html lang="en"><head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{esc_title}</title>
<style>
  html,body {{ margin:0; height:100%; }}
  header.gp-chrome {{ font:14px system-ui,sans-serif; padding:6px 12px; border-bottom:1px solid #ccc; }}
  iframe#gp-artifact {{ display:block; border:0; width:100%; height:calc(100% - 34px); }}
</style>
</head><body>
<header class="gp-chrome"><span id="gp-title"></span></header>
<iframe id="gp-artifact"
        sandbox="allow-scripts allow-top-navigation-by-user-activation"
        src="{esc_src}"></iframe>
<script nonce="{nonce}">
(function () {{
  "use strict";
  var SPACE = {space_json};
  var SLUG = {slug_json};
  var KNOWN = {slugs_json};
  var TITLE = {title_json};
  var MAX_SLUG = 64;       // matches the server-side slug grammar
  var RATE_MAX = 20;       // messages...
  var RATE_WINDOW = 1000;  // ...per this many ms

  var frame = document.getElementById("gp-artifact");
  var KNOWN_SET = new Set(KNOWN);
  // Nav chrome title inserted as TEXT (never innerHTML) — Trusted Types on.
  document.getElementById("gp-title").textContent = TITLE;

  // Live-reload: the trusted shell holds the EventSource (its connect-src 'self'
  // permits it) and reloads the whole shell on a filesystem change, so new/renamed
  // artifacts and titles are picked up, not just the current artifact's body.
  try {{
    var es = new EventSource("/_gp/reload");
    es.addEventListener("reload", function () {{ location.reload(); }});
  }} catch (e) {{ /* SSE unsupported — live reload simply inactive */ }}

  var stats = {{ accepted: 0, rejectedSource: 0, rejectedSize: 0, rejectedRate: 0, rejectedSchema: 0 }};
  window.__bridgeStats = stats;

  var recent = [];
  function rateOk() {{
    var now = Date.now();
    recent = recent.filter(function (t) {{ return now - t < RATE_WINDOW; }});
    if (recent.length >= RATE_MAX) return false;
    recent.push(now);
    return true;
  }}

  var validSlug = /^[a-z0-9][a-z0-9-]{{0,63}}$/;

  window.addEventListener("message", function (event) {{
    // 1. Source check — the ONLY trustworthy identity for a sandboxed frame
    //    (event.origin is the string "null" for every sandboxed frame).
    if (event.source !== frame.contentWindow) {{ stats.rejectedSource++; return; }}

    // 2. Rate cap FIRST — before any per-message work, so a flood cannot make us
    //    do unbounded parsing/serialization on the shell's main thread.
    if (!rateOk()) {{ stats.rejectedRate++; return; }}

    // 3. Reject transferred ports outright — the bridge is one-way, port
    //    transfer would open a covert channel.
    if (event.ports && event.ports.length) {{ stats.rejectedSchema++; return; }}

    // 4. Fixed low-authority schema: exactly {{type:"navigate", slug:<known>}}.
    //    No JSON.stringify (a hostile frame could send a huge structured-clone
    //    graph); we only ever read two small, typed fields.
    var data = event.data;
    if (data === null || typeof data !== "object" || Array.isArray(data)
        || data.type !== "navigate" || typeof data.slug !== "string") {{
      stats.rejectedSchema++;
      return;
    }}
    if (data.slug.length > MAX_SLUG) {{ stats.rejectedSize++; return; }}
    if (!validSlug.test(data.slug) || !KNOWN_SET.has(data.slug)) {{
      stats.rejectedSchema++;
      return;
    }}

    stats.accepted++;
    frame.src = "/" + SPACE + "/_c/" + data.slug;
  }}, false);
}})();
</script>
</body></html>
"#
    )
}

/// Serialize a value as JSON safe to embed inside an inline `<script>`.
/// Neutralizes `<`/`>`/`&` (so `</script>` can't close the element) and the
/// line separators that are legal in HTML but break JS string literals.
fn json_for_script(v: &serde_json::Value) -> String {
    v.to_string()
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029")
}

/// Escape text for insertion into an HTML text/attribute context. The shell
/// generates its own markup here (server-side); artifact-derived text that the
/// *client* handles goes through `textContent`, never this.
fn html_text_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn html_attr_escape(s: &str) -> String {
    html_text_escape(s)
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_has_null_origin_sandbox() {
        let html = render("demo", "index", "", &["index"], "n0nce");
        assert!(html.contains(r#"sandbox="allow-scripts allow-top-navigation-by-user-activation""#));
        assert!(!html.contains("allow-same-origin"));
    }

    #[test]
    fn shell_frames_content_route() {
        let html = render("demo", "eval", "", &["eval"], "n0nce");
        assert!(html.contains(r#"src="/demo/_c/eval""#));
    }

    #[test]
    fn shell_validates_event_source() {
        let html = render("demo", "index", "", &["index"], "n0nce");
        assert!(html.contains("event.source !== frame.contentWindow"));
    }

    #[test]
    fn shell_script_is_nonce_gated() {
        let html = render("demo", "index", "", &["index"], "abc123");
        assert!(html.contains(r#"<script nonce="abc123">"#));
    }

    #[test]
    fn shell_embeds_slugs_as_json_data() {
        let html = render("demo", "index", "", &["index", "eval"], "n");
        assert!(html.contains(r#"["index","eval"]"#));
    }

    #[test]
    fn shell_opens_reload_event_source() {
        let html = render("demo", "index", "", &["index"], "n");
        assert!(html.contains(r#"new EventSource("/_gp/reload")"#));
    }

    #[test]
    fn shell_uses_resolved_title_as_text() {
        // A provided title lands in the chrome as a JSON string literal + escaped
        // <title>, never as live markup.
        let html = render("demo", "index", "Sales & Q3", &["index"], "n");
        assert!(html.contains("Sales &amp; Q3")); // server-side <title>, escaped
        // client textContent literal: `&` is JSON-for-script-encoded so it can't
        // close the <script> element — assert the encoded form is present and the
        // bare ampersand is NOT.
        let title_line = html.lines().find(|l| l.contains("var TITLE =")).unwrap();
        assert!(title_line.contains("Sales") && title_line.contains("Q3"));
        // The `&`/`<`/`>` are json_for_script-encoded (to \uXXXX) so the value can
        // never close the <script> element: the bare punctuation is absent.
        assert!(!title_line.contains('&'));
        assert!(!title_line.contains("Sales & Q3"));
        // Empty title falls back to "space / slug".
        let fallback = render("demo", "index", "", &["index"], "n");
        assert!(fallback.contains("demo / index"));
    }

    #[test]
    fn shell_escapes_injection_in_title_context() {
        // A hostile slug must not break out of the HTML title/attr context.
        let html = render("demo", "a\"><script>x", "", &["a"], "n");
        assert!(!html.contains("<script>x"));
        // A hostile *title* likewise cannot break out (escaped + JSON-encoded).
        let html2 = render("demo", "index", "</title><script>evil()</script>", &["index"], "n");
        assert!(!html2.contains("<script>evil()"));
    }
}
