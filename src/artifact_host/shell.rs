//! The trusted parent shell (`/{space}/{slug}` and `/{space}/`).
//!
//! First-party chrome that frames the artifact in a null-origin sandbox and runs
//! the **parent side of the postMessage bridge**. Per design.md §6 the parent:
//!
//! * validates `event.source === iframe.contentWindow` — **not** `event.origin`,
//!   which is the useless string `"null"` for every sandboxed frame;
//! * accepts only a fixed low-authority schema — **exactly** `{type, slug}`, no
//!   extra keys — `navigate` to a *known* slug resolved against the server-
//!   provided artifact table;
//! * bounds the slug size and the message *rate*, and invalidates state on iframe
//!   reload. (A hostile frame's structured-clone/queue cost cannot be bounded
//!   inside the receiving handler — the clone happens before the listener runs —
//!   so that DoS residual is accepted, exactly as it was pre-bridge: the artifact
//!   could always `postMessage` the parent. Rate + exact-schema minimize per-
//!   message work; they do not claim to bound clone cost.)
//! * inserts artifact-derived text as **text, never innerHTML** (Trusted Types on).
//!
//! **Wave 4 — nav chrome.** The trusted parent now renders a **navigation list**
//! of the space's artifacts (`nav`, an ordered `(slug, title)` table the server
//! resolves and hands down). Every list item is built **client-side with
//! `createElement` + `textContent`** — the artifact-derived title never touches an
//! HTML sink, so it can never break out of text context (Trusted Types would throw
//! on any accidental `innerHTML` too — `require-trusted-types-for 'script'` with
//! no default policy makes every string→sink assignment throw). Clicking a list
//! entry swaps the framed artifact **in place via the same validated navigate
//! path** (no full reload); the shell never leaves the trusted parent, so its
//! `frame-src 'self'` keeps containing whatever is framed. (URL-sync / deep-linking
//! for in-place swaps is deferred — see the wave's terminal report.) A
//! full-document artifact gets **no** injected `bridge.js` — its author writes
//! native same-space links with `target="_top"`, the D1-sanctioned top-nav path;
//! the parent chrome itself never needs it because the parent is not sandboxed.
//!
//! The inline script is authorized by a per-response nonce, not `'unsafe-inline'`.

use serde_json::json;

/// Render the shell document for `space`/`slug`. `nav` is the ordered artifact
/// table `(slug, title)` the chrome lists and the bridge resolves navigation
/// against — its slugs are the low-authority allowlist, its titles are inserted
/// as **text** (client `textContent`, server-side escaped). `title` is the current
/// artifact's resolved display title (empty → fall back to `space / slug`).
/// `nonce` matches the CSP.
pub fn render(
    mount: &str,
    space: &str,
    slug: &str,
    title: &str,
    nav: &[(&str, &str)],
    nonce: &str,
) -> String {
    // All dynamic values are serialized as JSON, so they land in the script as
    // data literals — never as HTML that could break out of context. `json` also
    // neutralizes `</script>` / U+2028 / U+2029 so a hostile value cannot close
    // the script element.
    let display_title = if title.is_empty() {
        format!("{space} / {slug}")
    } else {
        title.to_string()
    };
    // Nav as an array of {slug, title} objects, in server-resolved order. Titles
    // are artifact-derived; the JSON-for-script encoding below neutralizes any
    // markup, and the client inserts each one via textContent (never innerHTML).
    let nav_json_value = json!(
        nav.iter()
            .map(|(s, t)| json!({ "slug": s, "title": t }))
            .collect::<Vec<_>>()
    );
    let known: Vec<&str> = nav.iter().map(|(s, _)| *s).collect();

    let space_json = json_for_script(&json!(space));
    let slug_json = json_for_script(&json!(slug));
    let slugs_json = json_for_script(&json!(known));
    let title_json = json_for_script(&json!(display_title));
    let nav_json = json_for_script(&nav_json_value);
    // The URL mount prefix (`""` loopback, `/p` hosted) prepended to every
    // `/{space}/…` content + nav link — NOT to `/_gp/*` (base libs + reload stay
    // at root in both run modes). It is a fixed server constant, never client
    // input; emitted as a JSON string literal so it lands as data in the script.
    let mount_json = json_for_script(&json!(mount));

    // Content path for the iframe src. space/slug are path-validated upstream;
    // `mount` is a trusted server constant.
    let content_src = format!("{mount}/{space}/_c/{slug}");
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
  body {{ display:flex; flex-direction:column; }}
  header.gp-chrome {{ font:14px system-ui,sans-serif; flex:0 0 auto;
                      border-bottom:1px solid #ccc; }}
  header.gp-chrome .gp-bar {{ display:flex; align-items:center; gap:12px; padding:6px 12px; }}
  header.gp-chrome #gp-title {{ flex:1 1 auto; overflow:hidden; text-overflow:ellipsis;
                      white-space:nowrap; font-weight:600; }}
  header.gp-chrome #gp-theme-toggle {{ flex:0 0 auto; font:inherit; padding:2px 10px; cursor:pointer;
                      border:1px solid #ccc; border-radius:6px; background:transparent; color:inherit; }}
  nav.gp-nav {{ display:flex; gap:4px; padding:0 8px 6px; overflow-x:auto; white-space:nowrap; }}
  nav.gp-nav a {{ font:inherit; color:inherit; text-decoration:none; padding:3px 10px;
                  border:1px solid transparent; border-radius:6px; cursor:pointer;
                  max-width:22ch; overflow:hidden; text-overflow:ellipsis; }}
  nav.gp-nav a:hover {{ background:rgba(127,127,127,0.14); }}
  nav.gp-nav a[aria-current="page"] {{ background:rgba(127,127,127,0.22); font-weight:600; }}
  nav.gp-nav:empty {{ display:none; }}
  iframe#gp-artifact {{ display:block; border:0; flex:1 1 auto; width:100%; min-height:0; }}
</style>
</head><body>
<header class="gp-chrome">
  <div class="gp-bar"><span id="gp-title"></span><button id="gp-theme-toggle" type="button" aria-label="Toggle theme"></button></div>
  <nav class="gp-nav" id="gp-nav" aria-label="Artifacts in this space"></nav>
</header>
<iframe id="gp-artifact" title="{esc_title}"
        sandbox="allow-scripts allow-top-navigation-by-user-activation"
        data-src="{esc_src}"></iframe>
<script nonce="{nonce}">
(function () {{
  "use strict";
  var MOUNT = {mount_json};
  var SPACE = {space_json};
  var SLUG = {slug_json};
  var KNOWN = {slugs_json};
  var TITLE = {title_json};
  var NAV = {nav_json};   // [{{slug, title}}] — artifact-derived text, inserted via textContent only
  var MAX_SLUG = 64;       // matches the server-side slug grammar
  var RATE_MAX = 20;       // messages...
  var RATE_WINDOW = 1000;  // ...per this many ms

  var frame = document.getElementById("gp-artifact");
  var KNOWN_SET = new Set(KNOWN);
  var TITLE_BY_SLUG = Object.create(null);
  for (var n = 0; n < NAV.length; n++) {{
    if (NAV[n] && typeof NAV[n].slug === "string") TITLE_BY_SLUG[NAV[n].slug] = NAV[n].title;
  }}
  var current = SLUG;

  // Nav chrome title inserted as TEXT (never innerHTML) — Trusted Types on.
  var titleEl = document.getElementById("gp-title");
  titleEl.textContent = TITLE;

  // --- Nav list: built entirely with createElement + textContent, so an
  // artifact-derived title can NEVER become live markup (no innerHTML sink; the
  // shell CSP's Trusted Types would throw on one anyway). Each entry is an <a>
  // whose href is the real shell URL — a meaningful open-in-new-tab / modified-
  // click target (the shell needs JS to run, so this is NOT a no-JS fallback); a
  // primary click is intercepted and swaps the iframe in place. Built into a
  // DocumentFragment and attached once (a single layout pass, not one per entry).
  var navEl = document.getElementById("gp-nav");
  var linkBySlug = Object.create(null);
  var navFrag = document.createDocumentFragment();
  for (var k = 0; k < NAV.length; k++) {{
    var item = NAV[k];
    if (!item || typeof item.slug !== "string") continue;
    var a = document.createElement("a");
    a.setAttribute("href", MOUNT + "/" + SPACE + "/" + item.slug);
    a.setAttribute("data-slug", item.slug);
    a.setAttribute("rel", "noopener");
    a.textContent = (typeof item.title === "string" && item.title !== "") ? item.title : item.slug;
    navFrag.appendChild(a);
    linkBySlug[item.slug] = a;   // last wins if the server ever sent a dup slug
  }}
  navEl.appendChild(navFrag);
  function paintActive() {{
    for (var s in linkBySlug) {{
      if (s === current) linkBySlug[s].setAttribute("aria-current", "page");
      else linkBySlug[s].removeAttribute("aria-current");
    }}
  }}
  paintActive();

  // --- Theme: the trusted shell owns the toggle; a fragment artifact applies it
  // via bridge.js. The correct theme is ALSO inlined at wrap time (the `?gp_theme=`
  // on the swapped iframe src below), so an iframe SWAP is FOUC-free; this path
  // handles live toggles and re-applying the persisted choice after each load.
  var THEMES = ["auto", "light", "dark"];
  var THEME_LABEL = {{ auto: "Theme: Auto", light: "Theme: Light", dark: "Theme: Dark" }};
  var theme = "auto";
  try {{
    var saved = window.localStorage.getItem("gp-theme");
    if (saved === "light" || saved === "dark" || saved === "auto") theme = saved;
  }} catch (e) {{ /* storage blocked — default to auto */ }}

  var toggle = document.getElementById("gp-theme-toggle");
  function paintToggle() {{ if (toggle) toggle.textContent = THEME_LABEL[theme]; }}
  function sendTheme() {{
    // Post to the framed artifact; bridge.js validates source + schema on receipt.
    try {{ frame.contentWindow.postMessage({{ type: "theme", theme: theme }}, "*"); }} catch (e) {{}}
  }}
  // Query string that inlines the theme at wrap time (auto is the default → omit).
  function themeQuery() {{ return theme === "auto" ? "" : ("?gp_theme=" + theme); }}
  paintToggle();

  // --- The single validated navigation path. Used by BOTH the nav-chrome clicks
  // and the postMessage bridge, so every swap goes through the same allowlist +
  // grammar check and updates the chrome consistently. It only ever sets the
  // iframe src to a same-space content URL for a KNOWN slug — no full reload, and
  // the shell never leaves the trusted parent (its `frame-src 'self'` keeps
  // containing whatever is framed).
  //
  // The shell's own top-level URL is intentionally NOT updated on an in-place swap
  // (no `history.pushState`): syncing it means also owning Back/Forward, and an
  // iframe navigation adds its own session-history entry that entangles with a
  // pushState in browser-dependent ways. Deep-linking / URL-sync is deferred (see
  // the terminal report) rather than shipped fragile in trusted parent code.
  var validSlug = /^[a-z0-9][a-z0-9-]{{0,63}}$/;
  function navigateTo(slug) {{
    if (typeof slug !== "string" || slug.length > MAX_SLUG) return false;
    if (!validSlug.test(slug) || !KNOWN_SET.has(slug)) return false;
    // Same-slug is a validated no-op: it must NOT re-assign frame.src (that would
    // reload the artifact) — a hostile child posting navigate-to-self on load would
    // otherwise loop up to the rate cap. Return true so a nav click still
    // preventDefault()s and a bridge message still counts as accepted.
    if (slug === current) return true;
    current = slug;
    var t = TITLE_BY_SLUG[slug];
    var shown = (typeof t === "string" && t !== "") ? t : (SPACE + " / " + slug);
    // Inline the current theme into the swapped artifact so the new fragment wraps
    // with the right `data-theme` (no FOUC). The `load` handler re-sends it too.
    frame.src = MOUNT + "/" + SPACE + "/_c/" + slug + themeQuery();
    titleEl.textContent = shown;   // textContent — never innerHTML
    document.title = shown;
    frame.setAttribute("title", shown);
    paintActive();
    return true;
  }}

  // Nav-chrome clicks: a primary, unmodified click on a known entry swaps the
  // iframe in place. Modified / non-primary clicks keep native behavior (the href
  // opens the shell for that artifact in a new tab), and any unknown slug is left
  // to the browser too.
  navEl.addEventListener("click", function (event) {{
    if (event.defaultPrevented) return;
    if (event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
    var a = event.target && event.target.closest ? event.target.closest("a[data-slug]") : null;
    if (!a) return;
    var slug = a.getAttribute("data-slug");
    if (navigateTo(slug)) event.preventDefault();
  }}, false);

  // Re-apply the theme once the (possibly just-swapped) artifact has loaded — a
  // belt-and-braces follow-up to the inlined `?gp_theme` below.
  frame.addEventListener("load", sendTheme);
  // FIRST load: the iframe is rendered with `data-src` (no `src`), so it has not
  // started fetching yet. Set the themed src HERE — before the first request — so
  // the wrapped fragment inlines the persisted theme with NO FOUC (not even the
  // brief auto→persisted flash a post-load message would cause).
  if (!frame.getAttribute("src")) {{
    frame.src = frame.getAttribute("data-src") + themeQuery();
  }}
  if (toggle) {{
    toggle.addEventListener("click", function () {{
      theme = THEMES[(THEMES.indexOf(theme) + 1) % THEMES.length];
      try {{ window.localStorage.setItem("gp-theme", theme); }} catch (e) {{}}
      paintToggle();
      sendTheme();
    }});
  }}

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
    // EXACT schema — exactly the two own keys {{type, slug}}. Reject any extra
    // property so the accepted message is precisely the documented low-authority
    // shape (a hostile frame cannot smuggle a large/extra field past the schema).
    var keys = Object.keys(data);
    if (keys.length !== 2
        || !Object.prototype.hasOwnProperty.call(data, "type")
        || !Object.prototype.hasOwnProperty.call(data, "slug")) {{
      stats.rejectedSchema++;
      return;
    }}
    if (data.slug.length > MAX_SLUG) {{ stats.rejectedSize++; return; }}
    // Route through the same validated navigation path as the nav chrome. It
    // re-checks grammar + the KNOWN_SET allowlist, so a slug not in the artifact
    // table is rejected here too.
    if (!navigateTo(data.slug)) {{ stats.rejectedSchema++; return; }}
    stats.accepted++;
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

    /// Build a nav table from bare slugs (title == slug) for the tests that don't
    /// care about titles.
    fn nav_of<'a>(slugs: &'a [&'a str]) -> Vec<(&'a str, &'a str)> {
        slugs.iter().map(|s| (*s, *s)).collect()
    }

    #[test]
    fn shell_has_null_origin_sandbox() {
        let html = render("", "demo", "index", "", &nav_of(&["index"]), "n0nce");
        assert!(
            html.contains(r#"sandbox="allow-scripts allow-top-navigation-by-user-activation""#)
        );
        assert!(!html.contains("allow-same-origin"));
    }

    #[test]
    fn shell_frames_content_route() {
        let html = render("", "demo", "eval", "", &nav_of(&["eval"]), "n0nce");
        assert!(html.contains(r#"data-src="/demo/_c/eval""#));
    }

    #[test]
    fn shell_validates_event_source() {
        let html = render("", "demo", "index", "", &nav_of(&["index"]), "n0nce");
        assert!(html.contains("event.source !== frame.contentWindow"));
    }

    #[test]
    fn shell_script_is_nonce_gated() {
        let html = render("", "demo", "index", "", &nav_of(&["index"]), "abc123");
        assert!(html.contains(r#"<script nonce="abc123">"#));
    }

    #[test]
    fn shell_embeds_slugs_as_json_data() {
        let html = render("", "demo", "index", "", &nav_of(&["index", "eval"]), "n");
        assert!(html.contains(r#"["index","eval"]"#));
    }

    #[test]
    fn shell_opens_reload_event_source() {
        let html = render("", "demo", "index", "", &nav_of(&["index"]), "n");
        assert!(html.contains(r#"new EventSource("/_gp/reload")"#));
    }

    #[test]
    fn shell_uses_resolved_title_as_text() {
        // A provided title lands in the chrome as a JSON string literal + escaped
        // <title>, never as live markup.
        let html = render("", "demo", "index", "Sales & Q3", &nav_of(&["index"]), "n");
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
        let fallback = render("", "demo", "index", "", &nav_of(&["index"]), "n");
        assert!(fallback.contains("demo / index"));
    }

    #[test]
    fn shell_has_theme_toggle_and_messaging() {
        let html = render("", "demo", "index", "", &nav_of(&["index"]), "n");
        // A toggle control exists in the trusted chrome…
        assert!(html.contains(r#"id="gp-theme-toggle""#));
        // …and the shell sends a low-authority theme message to the framed artifact.
        assert!(html.contains(r#"type: "theme""#));
        // Persisted choice is read from the shell's own storage (default auto).
        assert!(html.contains(r#"getItem("gp-theme")"#));
    }

    #[test]
    fn shell_navigation_carries_theme_query() {
        // A swapped iframe src inlines the current theme so the new fragment wraps
        // FOUC-free; `auto` omits the query.
        let html = render("", "demo", "index", "", &nav_of(&["index"]), "n");
        assert!(html.contains(r#"MOUNT + "/" + SPACE + "/_c/" + slug + themeQuery()"#));
        assert!(html.contains("gp_theme="));
    }

    #[test]
    fn shell_escapes_injection_in_title_context() {
        // A hostile slug must not break out of the HTML title/attr context.
        let html = render("", "demo", "a\"><script>x", "", &nav_of(&["a"]), "n");
        assert!(!html.contains("<script>x"));
        // A hostile *title* likewise cannot break out (escaped + JSON-encoded).
        let html2 = render(
            "",
            "demo",
            "index",
            "</title><script>evil()</script>",
            &nav_of(&["index"]),
            "n",
        );
        assert!(!html2.contains("<script>evil()"));
    }

    // --- Wave 4: nav chrome -------------------------------------------------

    #[test]
    fn shell_renders_nav_container_and_navigate_path() {
        let html = render(
            "",
            "demo",
            "index",
            "Home",
            &nav_of(&["index", "sales"]),
            "n",
        );
        // The nav container exists and is built client-side (no server-rendered
        // artifact-title markup — the list is populated via createElement/textContent).
        assert!(html.contains(r#"<nav class="gp-nav" id="gp-nav""#));
        assert!(html.contains("createElement(\"a\")"));
        assert!(html.contains(".textContent ="));
        // The shared validated navigate path exists and both the nav click handler
        // and the bridge use it.
        assert!(html.contains("function navigateTo(slug)"));
        assert!(html.contains("navEl.addEventListener(\"click\""));
        assert!(html.contains("if (!navigateTo(data.slug))"));
    }

    #[test]
    fn shell_nav_carries_titles_as_json_data_not_markup() {
        // A hostile artifact TITLE in the nav table must be JSON-for-script encoded
        // (so it can't close the <script>) and must NOT appear as raw markup
        // anywhere in the document — the client inserts it via textContent.
        let hostile = r#"<img src=x onerror=alert(1)>"#;
        let html = render(
            "",
            "demo",
            "index",
            "Home",
            &[("index", "Home"), ("evil", hostile)],
            "n",
        );
        // Raw, executable markup for the title is absent everywhere in the document.
        assert!(!html.contains("<img src=x onerror"));
        // The encoded form is present in the NAV data literal.
        let nav_line = html.lines().find(|l| l.contains("var NAV =")).unwrap();
        assert!(nav_line.contains("\\u003cimg"));
        assert!(!nav_line.contains('<'));
        assert!(!nav_line.contains('>'));
    }

    #[test]
    fn shell_nav_navigate_is_allowlist_bounded() {
        // navigateTo only ever swaps to a KNOWN slug via the content route; the
        // grammar + KNOWN_SET checks are present.
        let html = render("", "demo", "index", "", &nav_of(&["index", "sales"]), "n");
        assert!(html.contains("!KNOWN_SET.has(slug)"));
        assert!(html.contains(r#"MOUNT + "/" + SPACE + "/_c/" + slug"#));
    }

    #[test]
    fn shell_nav_title_encodes_script_terminator_and_line_separators() {
        // A title that tries to close the <script> element or inject a line
        // separator (both legal in HTML, illegal in a JS string literal) must be
        // fully neutralized in the NAV data literal — mixed-case `</ScRiPt>`, the
        // `<`/`>`/`&` bytes, and U+2028/U+2029.
        let hostile = "</ScRiPt><b>&\u{2028}\u{2029}";
        let html = render(
            "",
            "demo",
            "index",
            "Home",
            &[("index", "Home"), ("evil", hostile)],
            "n",
        );
        // No raw `</script>` (any case) survives to close the shell's own script.
        assert!(!html.to_ascii_lowercase().contains("</script><b>"));
        let nav_line = html.lines().find(|l| l.contains("var NAV =")).unwrap();
        // Encoded, not raw: no bare `<`/`>`/U+2028/U+2029 in the data literal.
        assert!(!nav_line.contains('<'));
        assert!(!nav_line.contains('>'));
        assert!(!nav_line.contains('\u{2028}'));
        assert!(!nav_line.contains('\u{2029}'));
        assert!(nav_line.contains("\\u003c/ScRiPt")); // the terminator is <-escaped
        assert!(nav_line.contains("\\u2028") && nav_line.contains("\\u2029"));
    }

    #[test]
    fn shell_renders_with_empty_nav_without_panicking() {
        // A space with no artifacts (empty nav table) must render a valid shell —
        // the nav loop is a no-op and paintActive iterates nothing.
        let html = render("", "demo", "index", "", &[], "n");
        assert!(html.contains(r#"<nav class="gp-nav" id="gp-nav""#));
        assert!(
            html.contains(r#"var NAV = [];"#)
                || html.contains("var NAV = [ ]")
                || html.contains("var NAV = []")
        );
    }

    #[test]
    fn shell_same_slug_navigate_is_a_no_op() {
        // A same-slug navigate must be a validated no-op (return true, no frame
        // reassignment) so a hostile child can't loop the iframe by re-posting the
        // current slug on every load.
        let html = render("", "demo", "index", "", &nav_of(&["index", "sales"]), "n");
        assert!(html.contains("if (slug === current) return true;"));
    }
}
