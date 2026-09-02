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

/// One member of the grouped-nav sidebar: an artifact slug + its display title +
/// up to one level of nested companion children. The `slug` is validated against
/// `nav`'s allowlist upstream; the `title` is artifact/producer-derived and is
/// inserted client-side via `textContent` (never an HTML sink), exactly like the
/// flat nav.
pub struct NavItemView<'a> {
    pub slug: &'a str,
    pub title: &'a str,
    pub children: Vec<NavItemView<'a>>,
}

/// A named, ordered group of the grouped-nav sidebar (e.g. "ADR:t"). `label` is
/// producer text, inserted as `textContent`.
pub struct NavGroupView<'a> {
    pub label: &'a str,
    pub members: Vec<NavItemView<'a>>,
}

/// Render the shell document for `space`/`slug` with the **flat** nav bar only
/// (today's byte-compatible chrome). Thin shim over [`render_with_groups`] with no
/// groups — the grouped sidebar is inactive and the horizontal `nav` bar renders
/// exactly as before. Test-only: the production CLI adapter's `render_shell`
/// calls [`render_with_groups`] (passing the space's reconciled groups, which
/// may be empty), so this shim exists purely to keep the many flat-nav unit tests
/// concise and to document/exercise the byte-compatible fallback in the CLI adapter.
#[cfg(test)]
pub fn render(
    mount: &str,
    space: &str,
    slug: &str,
    title: &str,
    nav: &[(&str, &str)],
    nonce: &str,
    favicon: Option<&str>,
) -> String {
    render_with_groups(mount, space, slug, title, nav, &[], nonce, favicon)
}

/// Render the shell document for `space`/`slug`. `nav` is the ordered artifact
/// table `(slug, title)` — the complete low-authority allowlist the bridge resolves
/// navigation against (its titles are inserted as **text**). `groups`, when
/// non-empty, drives a **grouped vertical sidebar** (named groups with ordered
/// members and one level of nested companions) instead of the flat horizontal bar;
/// every group member's slug is still in `nav` (the allowlist), and its label/title
/// are inserted client-side via `textContent` only. Empty `groups` → the flat bar
/// (byte-compatible fallback). `title` is the current artifact's resolved display
/// title (empty → `space / slug`); `nonce` matches the CSP.
#[allow(clippy::too_many_arguments)]
pub fn render_with_groups(
    mount: &str,
    space: &str,
    slug: &str,
    title: &str,
    nav: &[(&str, &str)],
    groups: &[NavGroupView<'_>],
    nonce: &str,
    favicon: Option<&str>,
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

    // Grouped nav as nested {label, members:[{slug, title, children:[…]}]} objects.
    // Labels + titles are producer/artifact-derived; the json_for_script encoding
    // below neutralizes any markup (so a hostile label can't close the <script>),
    // and the client inserts each one via textContent (never innerHTML). Empty →
    // the flat nav bar renders instead (byte-compatible fallback).
    fn item_json(item: &NavItemView<'_>) -> serde_json::Value {
        json!({
            "slug": item.slug,
            "title": item.title,
            "children": item.children.iter().map(item_json).collect::<Vec<_>>(),
        })
    }
    let groups_json_value = json!(
        groups
            .iter()
            .map(|g| json!({
                "label": g.label,
                "members": g.members.iter().map(item_json).collect::<Vec<_>>(),
            }))
            .collect::<Vec<_>>()
    );

    let space_json = json_for_script(&json!(space));
    let slug_json = json_for_script(&json!(slug));
    let slugs_json = json_for_script(&json!(known));
    let nav_json = json_for_script(&nav_json_value);
    let groups_json = json_for_script(&groups_json_value);
    // The URL mount prefix (`""` loopback, `/p` hosted) prepended to every
    // `/{space}/…` content + nav link — NOT to `/_gp/*` (base libs + reload stay
    // at root in both run modes). It is a fixed server constant, never client
    // input; emitted as a JSON string literal so it lands as data in the script.
    let mount_json = json_for_script(&json!(mount));

    // The return-channel submit endpoint the shell POSTs a submission to. It differs
    // structurally by run mode, discriminated by the (server-constant) mount: the
    // loopback path is space-scoped under `_gp`; the hosted path is the root
    // `/api/v1/pages/<slug>/submit` (the shell's `connect-src 'self'` permits the
    // same-origin POST in both). `space` is path-validated upstream.
    let submit_path = if mount.is_empty() {
        format!("/{space}/_gp/submit")
    } else {
        format!("/api/v1/pages/{space}/submit")
    };
    let submit_json = json_for_script(&json!(submit_path));
    // Hosted pages can pull the state of the exact submission they just made. The
    // loopback shell deliberately has no public status route (its local agent read
    // path remains unchanged), so `null` keeps it to the truthful stored message.
    let status_path = if mount.is_empty() {
        serde_json::Value::Null
    } else {
        json!(format!("/api/v1/pages/{space}/submission-status"))
    };
    let status_json = json_for_script(&status_path);

    // Content path for the iframe src. space/slug are path-validated upstream;
    // `mount` is a trusted server constant.
    let content_src = format!("{mount}/{space}/_c/{slug}");
    let esc_src = html_attr_escape(&content_src);
    // Two escapings of the same title: text context for `<title>…</title>`, and the
    // stricter **attribute** context for `title="…"` on the iframe. A resolved
    // artifact title keeps straight quotes (`resolve_title` strips only `<`/`>` and
    // spoof chars), so a text-only escape in the double-quoted iframe `title`
    // attribute would let a hostile title break out and inject a *duplicate*
    // `sandbox="…allow-same-origin"` attribute (HTML keeps the FIRST duplicate) —
    // a null-origin sandbox escape in the trusted shell. `html_attr_escape` also
    // encodes `"`/`'`, closing that hole.
    let esc_title_text = html_text_escape(&display_title);
    let esc_title_attr = html_attr_escape(&display_title);

    // Emoji SVG favicon for THIS outer document only (never the sandboxed artifact).
    // `link_tag` base64-encodes the whole SVG, so the emitted attribute carries only
    // `[A-Za-z0-9+/=]` — the emoji cannot break out of the tag even if it reached here
    // unvalidated (it is validated at every ingress; this is defense-in-depth).
    let favicon_link = crate::favicon::link_tag(favicon);

    format!(
        r#"<!doctype html>
<html lang="en"><head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
{favicon_link}
<title>{esc_title_text}</title>
<style>
  :root, [data-theme="light"] {{
    color-scheme:light;
    --gp-shell-bg:#fff; --gp-shell-text:#1a1b25; --gp-shell-border:#e5e7eb;
  }}
  [data-theme="dark"] {{
    color-scheme:dark;
    --gp-shell-bg:#1a1b26; --gp-shell-text:#e5e7eb; --gp-shell-border:rgba(255,255,255,0.12);
  }}
  @media (prefers-color-scheme:dark) {{
    [data-theme="auto"] {{
      color-scheme:dark;
      --gp-shell-bg:#1a1b26; --gp-shell-text:#e5e7eb; --gp-shell-border:rgba(255,255,255,0.12);
    }}
  }}
  html,body {{ margin:0; height:100%; }}
  body {{ display:flex; flex-direction:column; background:var(--gp-shell-bg); color:var(--gp-shell-text); }}
  header.gp-chrome {{ font:14px system-ui,sans-serif; flex:0 0 auto;
                      background:var(--gp-shell-bg); color:var(--gp-shell-text);
                      border-bottom:1px solid var(--gp-shell-border); }}
  header.gp-chrome .gp-bar {{ display:flex; justify-content:flex-end; align-items:center; gap:12px; padding:6px 12px; }}
  header.gp-chrome #gp-delivery {{ flex:0 1 auto; max-width:42ch; font-size:12px;
                      color:inherit; opacity:0.78; text-align:right; white-space:nowrap;
                      overflow:hidden; text-overflow:ellipsis; }}
  header.gp-chrome #gp-delivery[data-state="long-wait"],
  header.gp-chrome #gp-delivery[data-state="collected"] {{ opacity:1; font-weight:600; }}
  header.gp-chrome #gp-theme-toggle {{ flex:0 0 auto; font:inherit; padding:2px 10px; cursor:pointer;
                      border:1px solid var(--gp-shell-border); border-radius:6px; background:transparent; color:inherit; }}
  nav.gp-nav {{ display:flex; gap:4px; padding:0 8px 6px; overflow-x:auto; white-space:nowrap; }}
  nav.gp-nav a {{ font:inherit; color:inherit; text-decoration:none; padding:3px 10px;
                  border:1px solid transparent; border-radius:6px; cursor:pointer;
                  max-width:22ch; overflow:hidden; text-overflow:ellipsis; }}
  nav.gp-nav a:hover {{ background:rgba(127,127,127,0.14); }}
  nav.gp-nav a[aria-current="page"] {{ background:rgba(127,127,127,0.22); font-weight:600; }}
  nav.gp-nav:empty {{ display:none; }}
  /* Body area below the header: a row so the grouped sidebar can sit beside the
     iframe. With no sidebar (flat nav) the empty aside is hidden and the iframe
     fills the row exactly as before. */
  .gp-body {{ flex:1 1 auto; display:flex; min-height:0; }}
  aside.gp-sidebar {{ flex:0 0 auto; width:240px; max-width:42vw; overflow-y:auto;
                      font:14px system-ui,sans-serif; background:var(--gp-shell-bg);
                      color:var(--gp-shell-text); border-right:1px solid var(--gp-shell-border);
                      padding:10px 6px; box-sizing:border-box; }}
  aside.gp-sidebar:empty {{ display:none; }}
  aside.gp-sidebar .sb-group {{ margin-bottom:16px; }}
  aside.gp-sidebar .sb-group-h {{ font-weight:600; opacity:0.72; padding:2px 8px;
                      text-transform:uppercase; letter-spacing:0.03em; font-size:11px; }}
  aside.gp-sidebar ul {{ list-style:none; margin:2px 0; padding:0; }}
  aside.gp-sidebar li {{ margin:0; }}
  aside.gp-sidebar a {{ display:block; font:inherit; color:inherit; text-decoration:none;
                      padding:3px 8px; border-radius:6px; cursor:pointer;
                      overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }}
  aside.gp-sidebar a:hover {{ background:rgba(127,127,127,0.14); }}
  aside.gp-sidebar a[aria-current="page"] {{ background:rgba(127,127,127,0.22); font-weight:600; }}
  aside.gp-sidebar ul ul {{ margin-left:12px; border-left:1px solid rgba(127,127,127,0.25); }}
  iframe#gp-artifact {{ display:block; border:0; flex:1 1 auto; width:100%; min-height:0; }}
</style>
</head><body>
<header class="gp-chrome">
  <div class="gp-bar"><span id="gp-delivery" role="status" aria-live="polite" hidden></span><button id="gp-theme-toggle" type="button" aria-label="Toggle theme"></button></div>
  <nav class="gp-nav" id="gp-nav" aria-label="Artifacts in this space"></nav>
</header>
<div class="gp-body">
<aside class="gp-sidebar" id="gp-sidebar" aria-label="Documents in this space"></aside>
<iframe id="gp-artifact" title="{esc_title_attr}"
        sandbox="allow-scripts allow-top-navigation-by-user-activation"
        data-src="{esc_src}"></iframe>
</div>
<script nonce="{nonce}">
(function () {{
  "use strict";
  var MOUNT = {mount_json};
  var SPACE = {space_json};
  var SLUG = {slug_json};
  var KNOWN = {slugs_json};
  var NAV = {nav_json};   // [{{slug, title}}] — artifact-derived text, inserted via textContent only
  var GROUPS = {groups_json};   // [{{label, members:[{{slug, title, children}}]}}] — grouped sidebar; empty → flat nav bar
  var SUBMIT_PATH = {submit_json};   // return-channel POST target (same-origin)
  var STATUS_PATH = {status_json};   // hosted exact-submission status base; null on loopback
  var MAX_SLUG = 64;       // matches the server-side slug grammar
  var MAX_SUBMIT_BYTES = 80 * 1024;  // reject an oversize submission before POSTing
  var RATE_MAX = 20;       // messages...
  var RATE_WINDOW = 1000;  // ...per this many ms
  var STATUS_FIRST_POLL_MS = 800;
  var STATUS_FAST_POLL_MS = 2000;
  var STATUS_SLOW_POLL_MS = 10000;
  var STATUS_RETRY_MS = 15000;
  var STATUS_LONG_WAIT_MS = 60000;
  var STATUS_MONITOR_MS = 10 * 60 * 1000;

  var frame = document.getElementById("gp-artifact");
  var KNOWN_SET = new Set(KNOWN);
  var TITLE_BY_SLUG = Object.create(null);
  for (var n = 0; n < NAV.length; n++) {{
    if (NAV[n] && typeof NAV[n].slug === "string") TITLE_BY_SLUG[NAV[n].slug] = NAV[n].title;
  }}
  var current = SLUG;

  // The current title remains in the document title and iframe's accessible title.
  // It is deliberately not repeated as visible header text: prose artifacts already
  // render their own H1, while the navigation provides page context.

  // --- Nav list: built entirely with createElement + textContent, so an
  // artifact-derived title can NEVER become live markup (no innerHTML sink; the
  // shell CSP's Trusted Types would throw on one anyway). Each entry is an <a>
  // whose href is the real shell URL — a meaningful open-in-new-tab / modified-
  // click target (the shell needs JS to run, so this is NOT a no-JS fallback); a
  // primary click is intercepted and swaps the iframe in place. Built into a
  // DocumentFragment and attached once (a single layout pass, not one per entry).
  var navEl = document.getElementById("gp-nav");
  var sidebarEl = document.getElementById("gp-sidebar");
  var linkBySlug = Object.create(null);

  // One anchor for a slug, built with createElement + textContent so an artifact-
  // derived title can NEVER become live markup (no innerHTML sink; the shell CSP's
  // Trusted Types would throw on one anyway). The href is the real shell URL — a
  // meaningful open-in-new-tab / modified-click target; a primary click is
  // intercepted (below) and swaps the iframe in place. Registered in `linkBySlug`
  // so paintActive() can mark the active entry across both nav surfaces.
  function makeNavLink(slug, title) {{
    var a = document.createElement("a");
    a.setAttribute("href", MOUNT + "/" + SPACE + "/" + slug);
    a.setAttribute("data-slug", slug);
    a.setAttribute("rel", "noopener");
    a.textContent = (typeof title === "string" && title !== "") ? title : slug;
    linkBySlug[slug] = a;   // last wins if the server ever sent a dup slug
    return a;
  }}

  // --- Grouped sidebar takes precedence when the space declares groups; otherwise
  // the flat horizontal nav bar renders (byte-compatible fallback). Exactly one of
  // the two surfaces is populated; the other stays :empty and is hidden by CSS.
  if (GROUPS.length) {{
    document.body.classList.add("gp-grouped");
    var sbFrag = document.createDocumentFragment();
    for (var gi = 0; gi < GROUPS.length; gi++) {{
      var group = GROUPS[gi];
      if (!group || !group.members || !group.members.length) continue;
      var section = document.createElement("div");
      section.className = "sb-group";
      if (typeof group.label === "string" && group.label !== "") {{
        var h = document.createElement("div");
        h.className = "sb-group-h";
        h.textContent = group.label;   // textContent — never innerHTML
        section.appendChild(h);
      }}
      var ul = document.createElement("ul");
      for (var mi = 0; mi < group.members.length; mi++) {{
        var m = group.members[mi];
        if (!m || typeof m.slug !== "string") continue;
        var li = document.createElement("li");
        li.appendChild(makeNavLink(m.slug, m.title));
        if (m.children && m.children.length) {{
          var subUl = document.createElement("ul");
          for (var ci = 0; ci < m.children.length; ci++) {{
            var c = m.children[ci];
            if (!c || typeof c.slug !== "string") continue;
            var subLi = document.createElement("li");
            subLi.appendChild(makeNavLink(c.slug, c.title));
            subUl.appendChild(subLi);
          }}
          if (subUl.childNodes.length) li.appendChild(subUl);
        }}
        ul.appendChild(li);
      }}
      section.appendChild(ul);
      sbFrag.appendChild(section);
    }}
    sidebarEl.appendChild(sbFrag);
  }} else {{
    var navFrag = document.createDocumentFragment();
    for (var k = 0; k < NAV.length; k++) {{
      var item = NAV[k];
      if (!item || typeof item.slug !== "string") continue;
      navFrag.appendChild(makeNavLink(item.slug, item.title));
    }}
    navEl.appendChild(navFrag);
  }}
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
  function paintTheme() {{
    document.documentElement.setAttribute("data-theme", theme);
    if (toggle) toggle.textContent = THEME_LABEL[theme];
  }}
  function sendTheme() {{
    // Post to the framed artifact; bridge.js validates source + schema on receipt.
    try {{ frame.contentWindow.postMessage({{ type: "theme", theme: theme }}, "*"); }} catch (e) {{}}
  }}
  // Query string that inlines the theme at wrap time (auto is the default → omit).
  function themeQuery() {{ return theme === "auto" ? "" : ("?gp_theme=" + theme); }}
  paintTheme();

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
    document.title = shown;
    frame.setAttribute("title", shown);
    paintActive();
    return true;
  }}

  // Nav-chrome clicks: a primary, unmodified click on a known entry swaps the
  // iframe in place. Modified / non-primary clicks keep native behavior (the href
  // opens the shell for that artifact in a new tab), and any unknown slug is left
  // to the browser too. Attached to BOTH the flat nav bar and the grouped sidebar,
  // so every swap goes through the same validated navigateTo path.
  function onNavClick(event) {{
    if (event.defaultPrevented) return;
    if (event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
    var a = event.target && event.target.closest ? event.target.closest("a[data-slug]") : null;
    if (!a) return;
    var slug = a.getAttribute("data-slug");
    if (navigateTo(slug)) event.preventDefault();
  }}
  navEl.addEventListener("click", onNavClick, false);
  sidebarEl.addEventListener("click", onNavClick, false);

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
      paintTheme();
      sendTheme();
    }});
  }}

  // Re-fetch the CURRENT slug's content in place (theme inlined so the swap is
  // FOUC-free), appending a cache-buster (the content route ignores the extra query
  // param) so an otherwise-identical URL reassignment still forces a fresh fetch of
  // the new round body. Used by the B2 round swap and the stale-submit recovery.
  function swapCurrentArtifact(bust) {{
    var q = themeQuery();
    frame.src = MOUNT + "/" + SPACE + "/_c/" + current + (q ? q + "&" : "?") + bust;
  }}

  // Live-reload: the trusted shell holds the EventSource (its connect-src 'self'
  // permits it) and reloads the whole shell on a filesystem change, so new/renamed
  // artifacts and titles are picked up, not just the current artifact's body. Round
  // events are SERVER-SIDE scoped to this space (`?space=`), so this connection only
  // ever receives round events for its own page — never another page's slug.
  try {{
    var es = new EventSource("/_gp/reload?space=" + encodeURIComponent(SPACE));
    es.addEventListener("reload", function () {{ location.reload(); }});
    // B2 multi-round: the SAME EventSource also carries a keyed `round` event when
    // the agent re-renders this page's artifact. Rather than a full reload, swap the
    // framed artifact IN PLACE — a fresh content-route fetch under the identical
    // frozen CSP (the server re-applies it), keeping the shell, this EventSource,
    // the theme, and the nav alive: a conversational UI in one live page.
    es.addEventListener("round", function (event) {{
      var d;
      try {{ d = JSON.parse(event.data); }} catch (e) {{ return; }}
      // Per-page isolation (defense-in-depth on top of the server-side `?space=`
      // scope): only react to a round for OUR OWN space. The event carries no URL —
      // we only ever re-fetch our own current content route, so a hostile/misdirected
      // payload can at most reload our own artifact, never redirect us elsewhere.
      if (!d || typeof d !== "object" || d.space !== SPACE) return;
      // Cache-buster: prefer the content-version (the immutable body identity), fall
      // back to the round number, then wall-clock — each validated so a hostile SSE
      // payload can only ever land as a bounded query value the content route ignores.
      var bust;
      if (typeof d.contentVersion === "string" && /^[0-9a-f]{{1,64}}$/.test(d.contentVersion)) {{
        bust = "gp_cv=" + d.contentVersion;
      }} else if (typeof d.round === "number" && d.round >= 0 && d.round < 1e15) {{
        bust = "gp_r=" + String(d.round);
      }} else {{
        bust = "gp_r=" + Date.now();
      }}
      swapCurrentArtifact(bust);
    }});
  }} catch (e) {{ /* SSE unsupported — live reload simply inactive */ }}

  var stats = {{ accepted: 0, rejectedSource: 0, rejectedSize: 0, rejectedRate: 0, rejectedSchema: 0, submitAccepted: 0, submitFailed: 0 }};
  window.__bridgeStats = stats;
  var deliveryEl = document.getElementById("gp-delivery");
  var latestSubmitAttempt = 0;

  function showDelivery(state, text) {{
    if (!deliveryEl) return;
    if (!deliveryEl.hidden && deliveryEl.getAttribute("data-state") === state
        && deliveryEl.textContent === text) return;
    deliveryEl.hidden = false;
    deliveryEl.setAttribute("data-state", state);
    deliveryEl.textContent = text;
  }}

  // Pull only the opaque state of the exact `(id, token)` returned by this submit.
  // A 404 is deliberately opaque at the API boundary; for the shell that received
  // this capability it truthfully means only that status is no longer available.
  // Transient failures retain the known fact that the POST was stored, back off,
  // and eventually stop checking rather than polling a long-lived tab forever.
  function watchDelivery(statusId, statusToken, attempt) {{
    if (!STATUS_PATH || typeof statusId !== "string" || !/^[1-9][0-9]*$/.test(statusId)
        || typeof statusToken !== "string" || !/^[0-9a-f]{{32}}$/.test(statusToken)) return;
    var started = Date.now();
    function schedule(delay) {{
      if (attempt === latestSubmitAttempt) window.setTimeout(poll, delay);
    }}
    function poll() {{
      if (attempt !== latestSubmitAttempt) return;
      var elapsed = Date.now() - started;
      if (elapsed >= STATUS_MONITOR_MS) {{
        showDelivery("long-wait", "Response is stored. Automatic status checks have stopped.");
        return;
      }}
      fetch(STATUS_PATH + "/" + statusId + "/" + encodeURIComponent(statusToken), {{
        method: "GET", credentials: "omit", cache: "no-store"
      }}).then(function (r) {{
        if (!r) return {{ retry: true }};
        if (r.status === 404) return {{ unavailable: true }};
        if (!r.ok) return {{ retry: true }};
        return r.json().catch(function () {{ return {{ retry: true }}; }});
      }}).then(function (body) {{
        if (attempt !== latestSubmitAttempt) return;
        if (body && body.unavailable) {{
          showDelivery("unavailable", "Response status is no longer available.");
          return;
        }}
        if (body && body.retry) {{
          showDelivery("status-unavailable", "Response stored. Status check temporarily unavailable.");
          schedule(STATUS_RETRY_MS);
          return;
        }}
        if (body && body.state === "collected") {{
          showDelivery("collected", "Response collected by the listening agent.");
          return;
        }}
        if (!body || body.state !== "waiting") {{
          showDelivery("status-unavailable", "Response stored. Status check temporarily unavailable.");
          schedule(STATUS_RETRY_MS);
          return;
        }}
        elapsed = Date.now() - started;
        if (elapsed >= STATUS_LONG_WAIT_MS) {{
          showDelivery("long-wait", "Response is stored, but has not been collected yet.");
        }} else {{
          showDelivery("waiting", "Response stored. Waiting for the listening agent to collect it…");
        }}
        schedule(elapsed < STATUS_LONG_WAIT_MS ? STATUS_FAST_POLL_MS : STATUS_SLOW_POLL_MS);
      }}).catch(function () {{
        if (attempt === latestSubmitAttempt) {{
          showDelivery("status-unavailable", "Response stored. Status check temporarily unavailable.");
          schedule(STATUS_RETRY_MS);
        }}
      }});
    }}
    window.setTimeout(poll, STATUS_FIRST_POLL_MS);
  }}

  var recent = [];
  function rateOk() {{
    var now = Date.now();
    recent = recent.filter(function (t) {{ return now - t < RATE_WINDOW; }});
    if (recent.length >= RATE_MAX) return false;
    recent.push(now);
    return true;
  }}

  // A validated NAVIGATE: exactly {{type, slug}}, slug in the artifact table.
  function handleNavigate(data) {{
    var keys = Object.keys(data);
    if (keys.length !== 2
        || !Object.prototype.hasOwnProperty.call(data, "type")
        || !Object.prototype.hasOwnProperty.call(data, "slug")
        || typeof data.slug !== "string") {{
      stats.rejectedSchema++;
      return;
    }}
    if (data.slug.length > MAX_SLUG) {{ stats.rejectedSize++; return; }}
    if (!navigateTo(data.slug)) {{ stats.rejectedSchema++; return; }}
    stats.accepted++;
  }}

  // A SUBMIT: {{type, data, contentVersion?}}. The `data` is the untrusted user
  // payload — the shell NEVER `eval`s or `innerHTML`s it, only forwards it as an
  // opaque JSON body to the same-origin submit endpoint. The submission is bound
  // server-side to THIS shell's own SPACE (the URL it POSTs to) and its own
  // `current` slug — an artifact-supplied space/slug in the payload is ignored, so
  // a hostile frame cannot direct a submission at another space/page.
  function handleSubmit(data) {{
    var keys = Object.keys(data);
    for (var i = 0; i < keys.length; i++) {{
      var k = keys[i];
      if (k !== "type" && k !== "data" && k !== "contentVersion") {{ stats.rejectedSchema++; return; }}
    }}
    if (!Object.prototype.hasOwnProperty.call(data, "data")) {{ stats.rejectedSchema++; return; }}
    // Serialize the trusted-context envelope: the payload plus THIS shell's own
    // current slug + the artifact's version echo (a string only). Size-cap before
    // any network call so a hostile huge payload is dropped here, not sent.
    var body;
    try {{
      var envelope = {{ data: data.data, slug: current }};
      if (typeof data.contentVersion === "string") envelope.content_version = data.contentVersion;
      body = JSON.stringify(envelope);
    }} catch (e) {{ stats.rejectedSchema++; return; }}
    if (typeof body !== "string" || body.length > MAX_SUBMIT_BYTES) {{ stats.rejectedSize++; return; }}
    // NB: `accepted` counts NAVIGATE messages only; a submit is tracked by
    // submitAccepted/submitFailed below, so an observer of `accepted` is unaffected.
    var attempt = ++latestSubmitAttempt;
    showDelivery("sending", "Sending response…");
    try {{
      fetch(SUBMIT_PATH, {{
        method: "POST",
        headers: {{ "content-type": "application/json" }},
        body: body,
        credentials: "omit",
        cache: "no-store"
      }}).then(function (r) {{
        if (r && r.ok) {{
          stats.submitAccepted++;
          if (attempt === latestSubmitAttempt) {{
            showDelivery(
              STATUS_PATH ? "waiting" : "stored",
              STATUS_PATH ? "Response stored. Waiting for the listening agent to collect it…" : "Response stored."
            );
            r.json().then(function (result) {{
              if (attempt === latestSubmitAttempt && result) {{
                watchDelivery(result.status_id, result.status_token, attempt);
              }}
            }}).catch(function () {{
              if (attempt === latestSubmitAttempt && STATUS_PATH) {{
                showDelivery("status-unavailable", "Response stored. Automatic status checking unavailable.");
              }}
            }});
          }}
          return;
        }}
        stats.submitFailed++;
        if (attempt === latestSubmitAttempt) {{
          showDelivery("failed", "Response could not be stored. Please try again.");
        }}
        // 409 = the agent advanced the round after this view rendered (a missed
        // round swap): the submission answered a now-stale round and was rejected.
        // Re-fetch the CURRENT round in place so the user sees the latest and can
        // re-answer — a single re-fetch (the swap posts nothing, so no retry loop).
        if (r && r.status === 409) swapCurrentArtifact("gp_r=" + Date.now());
      }}).catch(function () {{
        stats.submitFailed++;
        if (attempt === latestSubmitAttempt) {{
          showDelivery("failed", "Response could not be stored. Please try again.");
        }}
      }});
    }} catch (e) {{
      stats.submitFailed++;
      if (attempt === latestSubmitAttempt) {{
        showDelivery("failed", "Response could not be stored. Please try again.");
      }}
    }}
  }}

  window.addEventListener("message", function (event) {{
    // 1. Source check — the ONLY trustworthy identity for a sandboxed frame
    //    (event.origin is the string "null" for every sandboxed frame).
    if (event.source !== frame.contentWindow) {{ stats.rejectedSource++; return; }}

    // 2. Rate cap FIRST — before any per-message work, so a flood cannot make us
    //    do unbounded parsing/serialization/POSTs on the shell's main thread.
    if (!rateOk()) {{ stats.rejectedRate++; return; }}

    // 3. Reject transferred ports outright — the bridge is one-way, port
    //    transfer would open a covert channel.
    if (event.ports && event.ports.length) {{ stats.rejectedSchema++; return; }}

    // 4. Fixed low-authority schema, dispatched by `type`. We only ever read small
    //    typed fields (no JSON.stringify of the whole clone graph until a submit's
    //    bounded envelope is built).
    var data = event.data;
    if (data === null || typeof data !== "object" || Array.isArray(data)
        || typeof data.type !== "string") {{
      stats.rejectedSchema++;
      return;
    }}
    if (data.type === "navigate") {{ handleNavigate(data); return; }}
    if (data.type === "submit") {{ handleSubmit(data); return; }}
    stats.rejectedSchema++;
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
        let html = render("", "demo", "index", "", &nav_of(&["index"]), "n0nce", None);
        assert!(
            html.contains(r#"sandbox="allow-scripts allow-top-navigation-by-user-activation""#)
        );
        assert!(!html.contains("allow-same-origin"));
    }

    #[test]
    fn shell_frames_content_route() {
        let html = render("", "demo", "eval", "", &nav_of(&["eval"]), "n0nce", None);
        assert!(html.contains(r#"data-src="/demo/_c/eval""#));
    }

    #[test]
    fn shell_emits_emoji_favicon_link_on_outer_document() {
        // A configured emoji lands as a `<link rel="icon">` (base64 SVG data URI) in
        // the OUTER shell <head> — before </head>, and before the framed iframe.
        let html = render(
            "",
            "demo",
            "index",
            "",
            &nav_of(&["index"]),
            "n",
            Some("🚀"),
        );
        let link_pos = html
            .find(r#"<link rel="icon" type="image/svg+xml" href="data:image/svg+xml;base64,"#)
            .expect("favicon link present on the outer document");
        assert!(
            link_pos < html.find("</head>").unwrap(),
            "favicon is in <head>"
        );
        assert!(
            link_pos < html.find("<iframe").unwrap(),
            "favicon is on the outer doc, above the sandboxed iframe"
        );
        // The configured emoji round-trips through the base64 SVG.
        let b64 = html[link_pos..]
            .split("base64,")
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap();
        use base64::Engine as _;
        let svg = String::from_utf8(
            base64::engine::general_purpose::STANDARD
                .decode(b64)
                .unwrap(),
        )
        .unwrap();
        assert!(svg.contains('🚀'));
        // With no configured emoji the default is still emitted (every page gets one).
        let html_default = render("", "demo", "index", "", &nav_of(&["index"]), "n", None);
        assert!(html_default.contains(r#"<link rel="icon" type="image/svg+xml""#));
    }

    #[test]
    fn shell_favicon_does_not_touch_the_sandbox_tokens() {
        // Adding a favicon must not perturb the frozen sandbox contract: the iframe
        // sandbox tokens are byte-for-byte unchanged and no allow-same-origin appears,
        // whether or not a favicon is configured.
        let with = render(
            "",
            "demo",
            "index",
            "",
            &nav_of(&["index"]),
            "n",
            Some("🦀"),
        );
        let without = render("", "demo", "index", "", &nav_of(&["index"]), "n", None);
        for html in [&with, &without] {
            assert!(
                html.contains(r#"sandbox="allow-scripts allow-top-navigation-by-user-activation""#)
            );
            assert!(!html.contains("allow-same-origin"));
        }
        // The only difference between the two renders is the favicon's base64 payload —
        // the framed content route (the sandbox src) is identical.
        assert!(with.contains(r#"data-src="/demo/_c/index""#));
        assert!(without.contains(r#"data-src="/demo/_c/index""#));
    }

    #[test]
    fn shell_validates_event_source() {
        let html = render("", "demo", "index", "", &nav_of(&["index"]), "n0nce", None);
        assert!(html.contains("event.source !== frame.contentWindow"));
    }

    #[test]
    fn shell_script_is_nonce_gated() {
        let html = render("", "demo", "index", "", &nav_of(&["index"]), "abc123", None);
        assert!(html.contains(r#"<script nonce="abc123">"#));
    }

    #[test]
    fn shell_embeds_slugs_as_json_data() {
        let html = render(
            "",
            "demo",
            "index",
            "",
            &nav_of(&["index", "eval"]),
            "n",
            None,
        );
        assert!(html.contains(r#"["index","eval"]"#));
    }

    #[test]
    fn shell_opens_reload_event_source() {
        let html = render("", "demo", "index", "", &nav_of(&["index"]), "n", None);
        // The reload stream is opened space-scoped (`?space=`) so B2 round events are
        // delivered server-side only to this page's own shell (no cross-page slug leak).
        assert!(
            html.contains(r#"new EventSource("/_gp/reload?space=" + encodeURIComponent(SPACE))"#)
        );
    }

    #[test]
    fn shell_uses_resolved_title_for_document_and_accessible_frame() {
        // A provided title remains available to the browser tab and as the iframe's
        // accessible name, escaped for each HTML context, without visible duplication.
        let html = render(
            "",
            "demo",
            "index",
            "Sales & Q3",
            &nav_of(&["index"]),
            "n",
            None,
        );
        assert!(html.contains("<title>Sales &amp; Q3</title>"));
        assert!(html.contains(r#"title="Sales &amp; Q3""#));
        // Empty title falls back to "space / slug".
        let fallback = render("", "demo", "index", "", &nav_of(&["index"]), "n", None);
        assert!(fallback.contains("demo / index"));
    }

    #[test]
    fn shell_has_theme_toggle_and_messages_and_themes_its_own_chrome() {
        let html = render("", "demo", "index", "", &nav_of(&["index"]), "n", None);
        // A toggle control exists in the trusted chrome…
        assert!(html.contains(r#"id="gp-theme-toggle""#));
        // …and the shell sends a low-authority theme message to the framed artifact.
        assert!(html.contains(r#"type: "theme""#));
        // Persisted choice is read from the shell's own storage (default auto).
        assert!(html.contains(r#"getItem("gp-theme")"#));
        // The same choice is applied to the trusted shell itself, whose header uses
        // explicit light/dark variables rather than fixed browser-default colours.
        assert!(html.contains(r#"document.documentElement.setAttribute("data-theme", theme)"#));
        assert!(html.contains(r#"[data-theme="dark"]"#));
        assert!(html.contains("background:var(--gp-shell-bg)"));
    }

    #[test]
    fn shell_does_not_repeat_the_artifact_title_in_visible_header_chrome() {
        let html = render(
            "",
            "demo",
            "index",
            "Article title",
            &nav_of(&["index"]),
            "n",
            None,
        );
        // Title semantics remain for the browser tab and the iframe's accessible name…
        assert!(html.contains("<title>Article title</title>"));
        assert!(html.contains(r#"title="Article title""#));
        // …but the shell no longer paints a second visible copy above an artifact's H1.
        assert!(!html.contains(r#"id="gp-title""#));
        assert!(!html.contains("titleEl.textContent"));
    }

    #[test]
    fn shell_navigation_carries_theme_query() {
        // A swapped iframe src inlines the current theme so the new fragment wraps
        // FOUC-free; `auto` omits the query.
        let html = render("", "demo", "index", "", &nav_of(&["index"]), "n", None);
        assert!(html.contains(r#"MOUNT + "/" + SPACE + "/_c/" + slug + themeQuery()"#));
        assert!(html.contains("gp_theme="));
    }

    #[test]
    fn shell_escapes_injection_in_title_context() {
        // A hostile slug must not break out of the HTML title/attr context.
        let html = render("", "demo", "a\"><script>x", "", &nav_of(&["a"]), "n", None);
        assert!(!html.contains("<script>x"));
        // A hostile *title* likewise cannot break out (escaped + JSON-encoded).
        let html2 = render(
            "",
            "demo",
            "index",
            "</title><script>evil()</script>",
            &nav_of(&["index"]),
            "n",
            None,
        );
        assert!(!html2.contains("<script>evil()"));
    }

    #[test]
    fn shell_title_attribute_cannot_inject_a_sandbox_attribute() {
        // Regression: a resolved title keeps straight quotes, so the iframe `title="…"`
        // attribute must be ATTRIBUTE-escaped — a text-only escape would let a hostile
        // title inject a duplicate `sandbox="…allow-same-origin"` (HTML keeps the first
        // duplicate) and escape the null-origin sandbox in the trusted shell.
        let hostile = r#"x" sandbox="allow-scripts allow-same-origin"#;
        let html = render("", "demo", "index", hostile, &nav_of(&["index"]), "n", None);
        // The quote is encoded, so the title never closes the attribute — no real
        // second `sandbox="` attribute is injected (the breakout form is absent).
        assert!(!html.contains(r#"title="x" sandbox="#));
        // The hostile text survives on the iframe only as INERT escaped text.
        assert!(html.contains(r#"title="x&quot; sandbox=&quot;allow-scripts allow-same-origin""#));
        // No live `sandbox="…allow-same-origin"` attribute is injected anywhere, and
        // the real frozen sandbox token set is intact.
        assert!(!html.contains(r#"sandbox="allow-scripts allow-same-origin""#));
        assert!(
            html.contains(r#"sandbox="allow-scripts allow-top-navigation-by-user-activation""#)
        );
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
            None,
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
            None,
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
        let html = render(
            "",
            "demo",
            "index",
            "",
            &nav_of(&["index", "sales"]),
            "n",
            None,
        );
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
            None,
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
        let html = render("", "demo", "index", "", &[], "n", None);
        assert!(html.contains(r#"<nav class="gp-nav" id="gp-nav""#));
        assert!(
            html.contains(r#"var NAV = [];"#)
                || html.contains("var NAV = [ ]")
                || html.contains("var NAV = []")
        );
    }

    // --- return channel: submit branch --------------------------------------

    #[test]
    fn shell_loopback_submit_endpoint_and_handler() {
        // Loopback (empty mount): the submit target is the space-scoped `_gp` path,
        // the handler dispatches the `submit` type, and it POSTs to that path.
        let html = render("", "demo", "index", "", &nav_of(&["index"]), "n", None);
        assert!(html.contains(r#"var SUBMIT_PATH = "/demo/_gp/submit""#));
        assert!(html.contains(r#"if (data.type === "submit")"#));
        assert!(html.contains("function handleSubmit(data)"));
        assert!(html.contains("fetch(SUBMIT_PATH"));
        assert!(html.contains("var STATUS_PATH = null"));
        assert!(html.contains(r#": "Response stored."#));
        // The submission is bound to THIS shell's own current slug (anti-spoof) —
        // an artifact-supplied slug in the payload is never used for addressing.
        assert!(html.contains("slug: current"));
    }

    #[test]
    fn shell_hosted_submit_endpoint_is_api_route() {
        // Hosted (mount `/p`): the submit target is the root `/api/v1/pages/<slug>`
        // route (the page's space name IS its capability slug), not a `_gp` path.
        let html = render("/p", "abcslug", "index", "", &nav_of(&["index"]), "n", None);
        assert!(html.contains(r#"var SUBMIT_PATH = "/api/v1/pages/abcslug/submit""#));
        assert!(html.contains(r#"var STATUS_PATH = "/api/v1/pages/abcslug/submission-status""#));
        assert!(html.contains("watchDelivery(result.status_id, result.status_token, attempt)"));
        assert!(html.contains("Response stored. Waiting for the listening agent"));
        assert!(html.contains("Response is stored, but has not been collected yet."));
        assert!(html.contains("Response collected by the listening agent."));
        assert!(!html.contains("/_gp/submit"));
    }

    #[test]
    fn shell_submit_handler_bounds_size_and_rejects_extra_keys() {
        // The submit envelope is size-capped before any POST, and only the
        // {type,data,contentVersion} keys are accepted (extra keys rejected).
        let html = render("", "demo", "index", "", &nav_of(&["index"]), "n", None);
        assert!(html.contains("MAX_SUBMIT_BYTES"));
        assert!(html.contains(r#"k !== "type" && k !== "data" && k !== "contentVersion""#));
    }

    #[test]
    fn shell_round_event_swaps_current_artifact_in_place_scoped_to_space() {
        // B2: the reload EventSource also handles a keyed `round` event, swapping the
        // framed artifact in place (a content-route re-fetch) rather than a full
        // reload — and only for the shell's OWN space (per-page isolation). The event
        // carries no URL, so the swap targets our own `current` slug's content route.
        let html = render("", "demo", "index", "", &nav_of(&["index"]), "n", None);
        assert!(html.contains(r#"es.addEventListener("round""#));
        // Round events are SERVER-SIDE scoped to this space (`?space=`) — the shell
        // never receives another page's round event (with its slug) to leak.
        assert!(
            html.contains(r#"new EventSource("/_gp/reload?space=" + encodeURIComponent(SPACE))"#)
        );
        // Per-page isolation (client-side defense-in-depth): react only to our SPACE.
        assert!(html.contains("d.space !== SPACE"));
        // The swap re-fetches OUR current slug's content route (no URL from the event).
        assert!(html.contains(r#"MOUNT + "/" + SPACE + "/_c/" + current"#));
        // The cache-buster prefers the validated content-version, else the round.
        assert!(html.contains("gp_cv="));
        assert!(html.contains("gp_r="));
        // The full-reload path is unchanged (dev file-watch still location.reload()s).
        assert!(
            html.contains(r#"es.addEventListener("reload", function () { location.reload(); })"#)
        );
        // A stale-submit 409 re-fetches the current round (conversational recovery).
        assert!(html.contains("if (r && r.status === 409) swapCurrentArtifact"));
    }

    // --- grouped nav sidebar (space-docsite-nav) ----------------------------

    fn item<'a>(slug: &'a str, title: &'a str, children: Vec<NavItemView<'a>>) -> NavItemView<'a> {
        NavItemView {
            slug,
            title,
            children,
        }
    }

    #[test]
    fn shell_renders_grouped_sidebar_with_nested_children() {
        let groups = vec![
            NavGroupView {
                label: "Perusarkkitehtuuri",
                members: vec![item("intent", "Intent", vec![])],
            },
            NavGroupView {
                label: "Suunnitteludokumentit",
                members: vec![item(
                    "backtest",
                    "Backtest",
                    vec![
                        item("backtest-arkkitehdille", "Arkkitehdille", vec![]),
                        item("backtest-kirjanpitajalle", "Kirjanpitäjälle", vec![]),
                    ],
                )],
            },
        ];
        let nav = nav_of(&[
            "index",
            "intent",
            "backtest",
            "backtest-arkkitehdille",
            "backtest-kirjanpitajalle",
        ]);
        let html = render_with_groups("", "docs", "index", "Home", &nav, &groups, "n", None);
        // The grouped sidebar container exists and is built client-side (no server-
        // rendered artifact/label markup — createElement/textContent only).
        assert!(html.contains(r#"<aside class="gp-sidebar" id="gp-sidebar""#));
        assert!(html.contains("document.body.classList.add(\"gp-grouped\")"));
        // GROUPS is emitted as a JSON data literal carrying labels + nested children.
        let groups_line = html.lines().find(|l| l.contains("var GROUPS =")).unwrap();
        assert!(groups_line.contains("Perusarkkitehtuuri"));
        assert!(groups_line.contains("backtest-arkkitehdille"));
        assert!(groups_line.contains("\"children\""));
        // The shared validated navigate path is reused for BOTH surfaces.
        assert!(html.contains("sidebarEl.addEventListener(\"click\", onNavClick"));
        assert!(html.contains("navEl.addEventListener(\"click\", onNavClick"));
        // The full slug allowlist (NAV) still gates navigation.
        assert!(html.contains("!KNOWN_SET.has(slug)"));
    }

    #[test]
    fn shell_grouped_label_and_title_encoded_not_markup() {
        // A hostile group label / member title must be JSON-for-script encoded (so it
        // can't close the <script>) and never appear as raw markup — the client
        // inserts each via textContent.
        let hostile = r#"</script><img src=x onerror=alert(1)>"#;
        let groups = vec![NavGroupView {
            label: hostile,
            members: vec![item("evil", hostile, vec![])],
        }];
        let nav = nav_of(&["index", "evil"]);
        let html = render_with_groups("", "docs", "index", "Home", &nav, &groups, "n", None);
        assert!(!html.contains("<img src=x onerror"));
        assert!(!html.to_ascii_lowercase().contains("</script><img"));
        let groups_line = html.lines().find(|l| l.contains("var GROUPS =")).unwrap();
        assert!(!groups_line.contains('<'));
        assert!(!groups_line.contains('>'));
        assert!(groups_line.contains("\\u003c/script"));
    }

    #[test]
    fn shell_flat_fallback_when_no_groups_is_byte_identical() {
        // Empty groups → the thin `render` shim and `render_with_groups(&[])` produce
        // the identical document, and the flat nav bar (not the sidebar) is populated.
        let nav = nav_of(&["index", "sales"]);
        let shim = render("", "demo", "index", "Home", &nav, "n", None);
        let full = render_with_groups("", "demo", "index", "Home", &nav, &[], "n", None);
        assert_eq!(shim, full);
        // GROUPS is empty, so the client takes the flat-nav branch (the
        // classList.add("gp-grouped") call is present in the script but never runs).
        assert!(
            shim.contains("var GROUPS = [];")
                || shim.contains("var GROUPS = []")
                || shim.contains("var GROUPS = [ ]")
        );
        // The flat nav bar is populated (createElement path present) and the sidebar
        // aside stays empty (hidden by `aside.gp-sidebar:empty`).
        assert!(shim.contains(r#"<nav class="gp-nav" id="gp-nav""#));
        assert!(shim.contains(r#"<aside class="gp-sidebar" id="gp-sidebar""#));
    }

    #[test]
    fn shell_same_slug_navigate_is_a_no_op() {
        // A same-slug navigate must be a validated no-op (return true, no frame
        // reassignment) so a hostile child can't loop the iframe by re-posting the
        // current slug on every load.
        let html = render(
            "",
            "demo",
            "index",
            "",
            &nav_of(&["index", "sales"]),
            "n",
            None,
        );
        assert!(html.contains("if (slug === current) return true;"));
    }
}
