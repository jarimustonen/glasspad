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
//! **Nav chrome.** The trusted parent renders a top bar whose page switcher opens
//! a **navigation list** of the space's artifacts (`nav`, an ordered `(slug, title)` table the server
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

/// One member of the grouped page menu: an artifact slug + its display title +
/// up to one level of nested companion children. The `slug` is validated against
/// `nav`'s allowlist upstream; the `title` is artifact/producer-derived and is
/// inserted client-side via `textContent` (never an HTML sink), exactly like the
/// flat nav.
pub struct NavItemView<'a> {
    pub slug: &'a str,
    pub title: &'a str,
    pub children: Vec<NavItemView<'a>>,
}

/// A named, ordered group of the grouped page menu (e.g. "ADR:t"). `label` is
/// producer text, inserted as `textContent`.
pub struct NavGroupView<'a> {
    pub label: &'a str,
    pub members: Vec<NavItemView<'a>>,
}

/// Render the shell document for `space`/`slug` with a **flat** page menu and no
/// space title. Thin shim over [`render_with_groups`] with no groups. Test-only: the production CLI adapter's `render_shell`
/// calls [`render_with_groups`] (passing the space's reconciled groups, which
/// may be empty), so this shim exists purely to keep the many flat-nav unit tests
/// concise and to document/exercise the flat fallback in the CLI adapter.
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
    render_with_groups(mount, space, slug, title, nav, &[], "", nonce, favicon)
}

/// Render the shell document for `space`/`slug`. `nav` is the ordered artifact
/// table `(slug, title)` — the complete low-authority allowlist the bridge resolves
/// navigation against (its titles are inserted as **text**). The trusted chrome is
/// a top bar with a page switcher, previous / next links and a theme control;
/// a single-page space gets only the theme control, in a corner. `groups`, when
/// non-empty, groups the switcher's page menu (named groups with ordered members
/// and one level of nested companions); every group member's slug is still in
/// `nav` (the allowlist), and its label/title are inserted client-side via
/// `textContent` only. Empty `groups` → a flat menu in `nav` order. `space_title`
/// is the manifest's space title, shown before the page name (empty → the
/// loopback space name, or nothing when hosted). `title` is the current
/// artifact's resolved display title (empty → `space / slug`); `nonce` matches
/// the CSP.
#[allow(clippy::too_many_arguments)]
pub fn render_with_groups(
    mount: &str,
    space: &str,
    slug: &str,
    title: &str,
    nav: &[(&str, &str)],
    groups: &[NavGroupView<'_>],
    space_title: &str,
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
    // the page menu is a flat list in nav order.
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

    // The space name the top bar shows before the page switcher: the manifest's
    // space title when there is one, else the loopback directory name. A hosted
    // space name is a random capability slug, which reads as noise, so without a
    // title the hosted bar shows only the page. Producer text, so it reaches the
    // client as a data literal and is inserted with `textContent`.
    let space_label = if !space_title.is_empty() {
        space_title
    } else if mount.is_empty() {
        space
    } else {
        ""
    };
    let space_label_json = json_for_script(&json!(space_label));
    // A space with one page has nothing to switch between: the bar collapses to a
    // corner theme control. Decided here as well as in the script so the first
    // paint already has the right layout.
    let body_class = if nav.len() > 1 {
        ""
    } else {
        r#" class="gp-single""#
    };

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
<html lang="en" data-theme="auto"><head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
{favicon_link}
<title>{esc_title_text}</title>
<script nonce="{nonce}">
(function () {{
  "use strict";
  var theme = "auto";
  try {{
    var saved = window.localStorage.getItem("gp-theme");
    if (saved === "light" || saved === "dark" || saved === "auto") theme = saved;
  }} catch (e) {{ /* storage blocked — keep auto */ }}
  document.documentElement.setAttribute("data-theme", theme);
}})();
</script>
<style>
  :root, [data-theme="light"] {{
    color-scheme:light;
    --gp-shell-bg:#fafbfc; --gp-shell-surface:#fff; --gp-shell-hover:rgba(15,23,42,.05);
    --gp-shell-text:#1a1b25; --gp-shell-muted:#6b7280; --gp-shell-border:#e5e7eb; --gp-shell-strong:#d1d5db;
    --gp-shell-accent:#2563eb; --gp-shell-accent-soft:#eff6ff; --gp-shell-accent-text:#1e40af; --gp-shell-focus:#3b82f6;
    --gp-shell-shadow:0 12px 32px rgba(15,23,42,.14),0 2px 6px rgba(15,23,42,.06); --gp-shell-scrim:rgba(250,251,252,.6);
  }}
  [data-theme="dark"] {{
    color-scheme:dark;
    --gp-shell-bg:#0f1117; --gp-shell-surface:#1a1b26; --gp-shell-hover:rgba(255,255,255,.06);
    --gp-shell-text:#e5e7eb; --gp-shell-muted:#8b93a1; --gp-shell-border:rgba(255,255,255,.08); --gp-shell-strong:rgba(255,255,255,.14);
    --gp-shell-accent:#6366f1; --gp-shell-accent-soft:rgba(99,102,241,.14); --gp-shell-accent-text:#a5b4fc; --gp-shell-focus:#6366f1;
    --gp-shell-shadow:0 12px 32px rgba(0,0,0,.5),0 2px 6px rgba(0,0,0,.3); --gp-shell-scrim:rgba(15,17,23,.6);
  }}
  @media (prefers-color-scheme:dark) {{
    [data-theme="auto"] {{
      color-scheme:dark;
      --gp-shell-bg:#0f1117; --gp-shell-surface:#1a1b26; --gp-shell-hover:rgba(255,255,255,.06);
      --gp-shell-text:#e5e7eb; --gp-shell-muted:#8b93a1; --gp-shell-border:rgba(255,255,255,.08); --gp-shell-strong:rgba(255,255,255,.14);
      --gp-shell-accent:#6366f1; --gp-shell-accent-soft:rgba(99,102,241,.14); --gp-shell-accent-text:#a5b4fc; --gp-shell-focus:#6366f1;
      --gp-shell-shadow:0 12px 32px rgba(0,0,0,.5),0 2px 6px rgba(0,0,0,.3); --gp-shell-scrim:rgba(15,17,23,.6);
    }}
  }}
  html,body {{ margin:0; height:100%; }}
  body {{ background:var(--gp-shell-bg); color:var(--gp-shell-text);
    font:14px/1.4 -apple-system,BlinkMacSystemFont,"Segoe UI",system-ui,sans-serif; -webkit-font-smoothing:antialiased; }}
  .gp-body {{ height:100dvh; display:flex; flex-direction:column; min-height:0; }}
  .gp-bar {{ flex:0 0 44px; box-sizing:border-box; display:flex; align-items:center; gap:8px; min-width:0;
    padding:0 8px 0 10px; border-bottom:1px solid var(--gp-shell-border); background:var(--gp-shell-bg);
    color:var(--gp-shell-text); position:relative; z-index:3; }}
  .gp-bar button, .gp-menu button {{ font:inherit; color:inherit; background:none; border:0; cursor:pointer; margin:0; }}
  .gp-bar button:focus-visible, .gp-bar a:focus-visible, .gp-menu button:focus-visible, .gp-menu a:focus-visible {{
    outline:2px solid var(--gp-shell-focus); outline-offset:-2px; }}
  .gp-switch {{ display:flex; align-items:center; gap:8px; min-width:0; height:32px; padding:0 8px; border-radius:7px;
    transition:background-color 120ms ease-out; }}
  .gp-switch:hover, .gp-switch[aria-expanded="true"] {{ background:var(--gp-shell-hover); }}
  .gp-space {{ color:var(--gp-shell-muted); white-space:nowrap; overflow:hidden; text-overflow:ellipsis; max-width:22ch; flex:0 1 auto; }}
  .gp-sep {{ color:var(--gp-shell-strong); }}
  .gp-space:empty, .gp-space:empty + .gp-sep {{ display:none; }}
  .gp-page {{ font-weight:600; white-space:nowrap; overflow:hidden; text-overflow:ellipsis; min-width:0; }}
  .gp-pos {{ color:var(--gp-shell-muted); font-variant-numeric:tabular-nums; font-size:12px; white-space:nowrap; }}
  .gp-pos:empty {{ display:none; }}
  .gp-chev {{ flex:0 0 auto; width:10px; height:10px; color:var(--gp-shell-muted); transition:transform 160ms cubic-bezier(.23,1,.32,1); }}
  .gp-switch[aria-expanded="true"] .gp-chev {{ transform:rotate(180deg); }}
  .gp-grow {{ flex:1 1 auto; }}
  .gp-pn {{ display:flex; gap:2px; flex:0 0 auto; }}
  .gp-icon {{ display:grid; place-items:center; width:32px; height:32px; border-radius:7px; color:var(--gp-shell-muted);
    transition:background-color 120ms ease-out,color 120ms ease-out; }}
  .gp-icon[href]:hover {{ background:var(--gp-shell-hover); color:var(--gp-shell-text); }}
  .gp-icon:not([href]) {{ opacity:.35; cursor:default; }}
  .gp-icon svg {{ width:16px; height:16px; }}
  .gp-div {{ flex:0 0 auto; width:1px; height:18px; background:var(--gp-shell-border); margin:0 4px; }}
  .gp-theme {{ display:flex; flex:0 0 auto; gap:2px; padding:2px; border:1px solid var(--gp-shell-border); border-radius:9px; }}
  .gp-theme button {{ display:grid; place-items:center; width:28px; height:26px; border-radius:6px; color:var(--gp-shell-muted); }}
  .gp-theme button:hover {{ color:var(--gp-shell-text); }}
  .gp-theme button[aria-checked="true"] {{ background:var(--gp-shell-surface); color:var(--gp-shell-text); box-shadow:0 0 0 1px var(--gp-shell-strong); }}
  .gp-theme svg {{ width:15px; height:15px; }}
  iframe#gp-artifact {{ display:block; border:0; flex:1 1 auto; width:100%; min-height:0; }}
  .gp-scrim {{ position:fixed; inset:44px 0 0; background:var(--gp-shell-scrim); z-index:2; opacity:0; transition:opacity 150ms ease-out; }}
  .gp-menu {{ position:absolute; top:40px; left:10px; z-index:4; box-sizing:border-box; width:340px; max-width:calc(100vw - 20px);
    max-height:calc(100dvh - 64px); overflow-y:auto; overscroll-behavior:contain; padding:8px;
    background:var(--gp-shell-surface); color:var(--gp-shell-text); border:1px solid var(--gp-shell-border); border-radius:12px;
    box-shadow:var(--gp-shell-shadow); transform-origin:top left; opacity:0; transform:translateY(-4px) scale(.98);
    transition:opacity 150ms ease-out,transform 180ms cubic-bezier(.23,1,.32,1); }}
  .gp-open .gp-menu {{ opacity:1; transform:none; }}
  .gp-open .gp-scrim {{ opacity:1; }}
  .gp-menu[hidden], .gp-scrim[hidden], .gp-menu-space[hidden], .gp-switch[hidden], .gp-pn[hidden] {{ display:none; }}
  .gp-menu-space {{ padding:8px 10px 10px; margin-bottom:4px; border-bottom:1px solid var(--gp-shell-border); overflow-wrap:anywhere; }}
  .gp-menu-space small {{ display:block; font-size:11px; font-weight:600; letter-spacing:.06em; text-transform:uppercase; color:var(--gp-shell-muted); }}
  .gp-menu-space b {{ font-size:15px; }}
  nav.gp-nav {{ counter-reset:gp-page; }}
  .gp-nav .gp-g {{ margin:12px 10px 4px; font-size:11px; font-weight:600; letter-spacing:.06em; text-transform:uppercase;
    color:var(--gp-shell-muted); overflow-wrap:anywhere; }}
  .gp-nav ul {{ list-style:none; margin:0; padding:0; }}
  .gp-nav ul ul {{ margin-left:18px; border-left:1px solid var(--gp-shell-border); }}
  .gp-nav a {{ display:flex; align-items:baseline; gap:10px; padding:8px 10px; border-radius:7px; color:inherit;
    text-decoration:none; overflow-wrap:anywhere; counter-increment:gp-page; }}
  .gp-nav a::before {{ content:counter(gp-page); flex:0 0 auto; width:1.4em; color:var(--gp-shell-muted); font-size:12px;
    font-variant-numeric:tabular-nums; }}
  .gp-nav a:hover {{ background:var(--gp-shell-hover); }}
  .gp-nav a[aria-current="page"] {{ background:var(--gp-shell-accent-soft); color:var(--gp-shell-accent-text); font-weight:600;
    box-shadow:inset 2px 0 0 var(--gp-shell-accent); }}
  .gp-nav a[aria-current="page"]::before {{ color:inherit; }}
  .gp-menu-theme {{ display:none; }}
  #gp-delivery {{ position:fixed; left:50%; bottom:12px; transform:translateX(-50%); z-index:5; box-sizing:border-box;
    max-width:min(560px, calc(100vw - 24px)); padding:8px 14px; font-size:13px; line-height:1.4; overflow-wrap:anywhere;
    background:var(--gp-shell-surface); color:var(--gp-shell-text); border:1px solid var(--gp-shell-border);
    border-radius:10px; box-shadow:var(--gp-shell-shadow); }}
  #gp-delivery[hidden] {{ display:none; }}
  body.gp-single .gp-bar {{ position:fixed; top:8px; right:8px; flex:none; height:auto; padding:0; border:0;
    background:transparent; }}
  body.gp-single .gp-switch, body.gp-single .gp-grow, body.gp-single .gp-pn, body.gp-single .gp-div {{ display:none; }}
  body.gp-single .gp-theme {{ background:var(--gp-shell-surface); box-shadow:var(--gp-shell-shadow); opacity:.72;
    transition:opacity 120ms ease-out; }}
  body.gp-single .gp-theme:hover, body.gp-single .gp-theme:focus-within {{ opacity:1; }}
  @media (prefers-reduced-motion:reduce) {{ .gp-menu, .gp-scrim, .gp-chev {{ transition:none; }} }}
  @media (max-width: 600px) {{
    .gp-bar {{ flex-basis:48px; padding:0 4px 0 6px; gap:4px; }}
    body:not(.gp-single) .gp-space, body:not(.gp-single) .gp-sep,
    body:not(.gp-single) .gp-theme, body:not(.gp-single) .gp-div {{ display:none; }}
    .gp-switch {{ height:40px; flex:1 1 auto; }}
    .gp-icon {{ width:40px; height:40px; }}
    .gp-scrim {{ inset:48px 0 0; }}
    .gp-menu {{ position:fixed; top:48px; left:0; right:0; width:auto; max-width:none;
      max-height:min(75dvh, calc(100dvh - 48px)); border-radius:0 0 14px 14px; border-width:0 0 1px; transform-origin:top center; }}
    .gp-nav a {{ padding:11px 10px; }}
    .gp-menu-theme {{ display:flex; gap:4px; margin:12px 4px 4px; padding-top:12px; border-top:1px solid var(--gp-shell-border); }}
    .gp-menu-theme button {{ flex:1; height:40px; border:1px solid var(--gp-shell-border); border-radius:8px; color:var(--gp-shell-muted); }}
    .gp-menu-theme button[aria-pressed="true"] {{ color:var(--gp-shell-text); border-color:var(--gp-shell-strong);
      background:var(--gp-shell-hover); font-weight:600; }}
  }}
  @media print {{ .gp-bar, .gp-menu, .gp-scrim, #gp-delivery {{ display:none; }} }}
</style>
</head><body{body_class}>
<div class="gp-body">
<header class="gp-bar" id="gp-bar" aria-label="Space navigation">
  <button class="gp-switch" id="gp-switch" type="button" aria-expanded="false" aria-controls="gp-menu" hidden><span class="gp-space" id="gp-space"></span><span class="gp-sep" aria-hidden="true">/</span><span class="gp-page" id="gp-page"></span><span class="gp-pos" id="gp-pos"></span><svg class="gp-chev" viewBox="0 0 10 10" aria-hidden="true"><path d="M1.5 3.5 5 7l3.5-3.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/></svg></button>
  <span class="gp-grow"></span>
  <div class="gp-pn" id="gp-pn" hidden><a class="gp-icon" id="gp-prev" rel="noopener" aria-disabled="true"><svg viewBox="0 0 16 16" aria-hidden="true"><path d="M10 3 5 8l5 5" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/></svg></a><a class="gp-icon" id="gp-next" rel="noopener" aria-disabled="true"><svg viewBox="0 0 16 16" aria-hidden="true"><path d="m6 3 5 5-5 5" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/></svg></a></div>
  <span class="gp-div" aria-hidden="true"></span>
  <div class="gp-theme" id="gp-theme" role="radiogroup" aria-label="Theme"><button type="button" role="radio" data-theme-choice="auto" aria-checked="true" aria-label="System" title="System theme"><svg viewBox="0 0 16 16" aria-hidden="true"><circle cx="8" cy="8" r="5.5" fill="none" stroke="currentColor" stroke-width="1.4"/><path d="M8 2.5a5.5 5.5 0 0 1 0 11z" fill="currentColor"/></svg></button><button type="button" role="radio" data-theme-choice="light" aria-checked="false" aria-label="Light" title="Light theme" tabindex="-1"><svg viewBox="0 0 16 16" aria-hidden="true"><circle cx="8" cy="8" r="3" fill="none" stroke="currentColor" stroke-width="1.4"/><path d="M8 1.5v1.6M8 12.9v1.6M1.5 8h1.6M12.9 8h1.6M3.4 3.4l1.1 1.1M11.5 11.5l1.1 1.1M3.4 12.6l1.1-1.1M11.5 4.5l1.1-1.1" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"/></svg></button><button type="button" role="radio" data-theme-choice="dark" aria-checked="false" aria-label="Dark" title="Dark theme" tabindex="-1"><svg viewBox="0 0 16 16" aria-hidden="true"><path d="M13.2 10.1A5.6 5.6 0 0 1 5.9 2.8a5.6 5.6 0 1 0 7.3 7.3z" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linejoin="round"/></svg></button></div>
</header>
<div class="gp-scrim" id="gp-scrim" hidden></div>
<div class="gp-menu" id="gp-menu" hidden>
  <div class="gp-menu-space" id="gp-menu-space" hidden><small>Space</small><b id="gp-menu-space-name"></b></div>
  <nav class="gp-nav" id="gp-nav" aria-label="Pages in this space"></nav>
  <div class="gp-menu-theme" role="group" aria-label="Theme"><button type="button" data-theme-choice="auto" aria-pressed="true">System</button><button type="button" data-theme-choice="light" aria-pressed="false">Light</button><button type="button" data-theme-choice="dark" aria-pressed="false">Dark</button></div>
</div>
<span id="gp-delivery" role="status" aria-live="polite" hidden></span>
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
  var GROUPS = {groups_json};   // [{{label, members:[{{slug, title, children}}]}}] — grouped page menu; empty → flat menu
  var SUBMIT_PATH = {submit_json};   // return-channel POST target (same-origin)
  var STATUS_PATH = {status_json};   // hosted exact-submission status base; null on loopback
  var MAX_SLUG = 256;       // matches the server-side slug grammar
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
  var SPACE_LABEL = {space_label_json};   // producer text, inserted via textContent only; "" → no space name
  var MULTI = NAV.length > 1;             // one page → corner theme control only (matches the server's body class)

  // The current title remains in the document title and iframe's accessible title.
  // The top bar shows it once more inside the page switcher, where it names the
  // control rather than heading the page; prose artifacts keep their own H1.

  // --- Top bar and page menu: built entirely with createElement + textContent, so
  // an artifact-derived title can NEVER become live markup (no innerHTML sink; the
  // shell CSP's Trusted Types would throw on one anyway). Each menu entry and the
  // previous / next links are <a> elements whose href is the real shell URL — a
  // meaningful open-in-new-tab / modified-click target (the shell needs JS to run,
  // so this is NOT a no-JS fallback); a primary click is intercepted and swaps the
  // iframe in place. The menu is built into a DocumentFragment and attached once.
  var barEl = document.getElementById("gp-bar");
  var switchEl = document.getElementById("gp-switch");
  var pageEl = document.getElementById("gp-page");
  var posEl = document.getElementById("gp-pos");
  var pnEl = document.getElementById("gp-pn");
  var prevEl = document.getElementById("gp-prev");
  var nextEl = document.getElementById("gp-next");
  var menuEl = document.getElementById("gp-menu");
  var scrimEl = document.getElementById("gp-scrim");
  var navEl = document.getElementById("gp-nav");
  var linkBySlug = Object.create(null);
  var order = [];   // menu order of the listed slugs; drives previous / next and "i / n"

  function shellHref(slug) {{ return MOUNT + "/" + SPACE + "/" + slug; }}
  function titleOf(slug) {{
    var t = TITLE_BY_SLUG[slug];
    return (typeof t === "string" && t !== "") ? t : slug;
  }}

  // One menu anchor for a slug. The visible position number is a CSS counter, so
  // the anchor holds nothing but the title text node.
  function makeNavLink(slug, title) {{
    var a = document.createElement("a");
    a.setAttribute("href", shellHref(slug));
    a.setAttribute("data-slug", slug);
    a.setAttribute("rel", "noopener");
    a.textContent = (typeof title === "string" && title !== "") ? title : slug;
    if (!linkBySlug[slug]) order.push(slug);
    linkBySlug[slug] = a;   // last wins if the server ever sent a dup slug
    return a;
  }}

  // --- Grouped menu when the space declares groups; otherwise a flat list in nav
  // order. A single-page space builds neither.
  if (MULTI && GROUPS.length) {{
    document.body.classList.add("gp-grouped");
    var menuFrag = document.createDocumentFragment();
    for (var gi = 0; gi < GROUPS.length; gi++) {{
      var group = GROUPS[gi];
      if (!group || !group.members || !group.members.length) continue;
      var ul = document.createElement("ul");
      if (typeof group.label === "string" && group.label !== "") {{
        var h = document.createElement("div");
        h.className = "gp-g";
        h.id = "gp-g" + gi;
        h.textContent = group.label;   // textContent — never innerHTML
        menuFrag.appendChild(h);
        ul.setAttribute("aria-labelledby", h.id);
      }}
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
      menuFrag.appendChild(ul);
    }}
    navEl.appendChild(menuFrag);
  }} else if (MULTI) {{
    var flatUl = document.createElement("ul");
    for (var k = 0; k < NAV.length; k++) {{
      var item = NAV[k];
      if (!item || typeof item.slug !== "string") continue;
      var flatLi = document.createElement("li");
      flatLi.appendChild(makeNavLink(item.slug, item.title));
      flatUl.appendChild(flatLi);
    }}
    navEl.appendChild(flatUl);
  }}
  if (MULTI) {{
    document.getElementById("gp-space").textContent = SPACE_LABEL;
    if (SPACE_LABEL !== "") {{
      document.getElementById("gp-menu-space-name").textContent = SPACE_LABEL;
      document.getElementById("gp-menu-space").hidden = false;
    }}
    switchEl.hidden = false;
    pnEl.hidden = false;
  }}

  function setStep(a, slug, label) {{
    if (slug) {{
      a.setAttribute("href", shellHref(slug));
      a.setAttribute("data-slug", slug);
      a.removeAttribute("aria-disabled");
      a.setAttribute("aria-label", label + ": " + titleOf(slug));
      a.setAttribute("title", titleOf(slug));
    }} else {{
      a.removeAttribute("href");
      a.removeAttribute("data-slug");
      a.setAttribute("aria-disabled", "true");
      a.setAttribute("aria-label", "No " + label.toLowerCase());
      a.removeAttribute("title");
    }}
  }}

  // Paint everything that depends on the current page: the switcher label, the
  // position, previous / next, and the single aria-current entry.
  function paintActive() {{
    for (var s in linkBySlug) {{
      if (s === current) linkBySlug[s].setAttribute("aria-current", "page");
      else linkBySlug[s].removeAttribute("aria-current");
    }}
    if (!MULTI) return;
    var i = order.indexOf(current);
    var shown = titleOf(current);
    pageEl.textContent = shown;
    posEl.textContent = i >= 0 ? (i + 1) + " / " + order.length : "";
    switchEl.setAttribute("aria-label", (i >= 0 ? "Page " + (i + 1) + " of " + order.length + ": " : "") + shown + ". Show all pages.");
    // A current page outside the listed order (an ungrouped landing) steps into the list.
    setStep(prevEl, i > 0 ? order[i - 1] : null, "Previous page");
    setStep(nextEl, i < 0 ? (order[0] || null) : (i < order.length - 1 ? order[i + 1] : null), "Next page");
  }}
  paintActive();

  // --- Page menu: a disclosure (button + aria-expanded), not an ARIA menu, so the
  // entries stay ordinary links. Arrow keys, Home and End move between them;
  // Escape, the scrim, or focus leaving the menu closes it.
  function isOpen() {{ return switchEl.getAttribute("aria-expanded") === "true"; }}
  function openMenu() {{
    if (!MULTI || isOpen()) return;
    menuEl.hidden = false;
    scrimEl.hidden = false;
    switchEl.setAttribute("aria-expanded", "true");
    var cur = linkBySlug[current] || navEl.querySelector("a[data-slug]");
    if (cur) cur.focus();
    window.requestAnimationFrame(function () {{ document.body.classList.add("gp-open"); }});
  }}
  function closeMenu(refocus) {{
    if (!isOpen()) return;
    switchEl.setAttribute("aria-expanded", "false");
    document.body.classList.remove("gp-open");
    menuEl.hidden = true;
    scrimEl.hidden = true;
    if (refocus) switchEl.focus();
  }}
  switchEl.addEventListener("click", function () {{ if (isOpen()) closeMenu(true); else openMenu(); }});
  switchEl.addEventListener("keydown", function (event) {{
    if (event.key === "ArrowDown" && !isOpen()) {{ event.preventDefault(); openMenu(); }}
  }});
  scrimEl.addEventListener("click", function () {{ closeMenu(false); }});
  menuEl.addEventListener("keydown", function (event) {{
    if (event.key === "Escape") {{ event.preventDefault(); closeMenu(true); return; }}
    var links = Array.prototype.slice.call(navEl.querySelectorAll("a[data-slug]"));
    if (!links.length) return;
    var at = links.indexOf(document.activeElement);
    var to = -1;
    if (event.key === "ArrowDown") to = at < 0 ? 0 : Math.min(links.length - 1, at + 1);
    else if (event.key === "ArrowUp") to = at < 0 ? links.length - 1 : Math.max(0, at - 1);
    else if (event.key === "Home") to = 0;
    else if (event.key === "End") to = links.length - 1;
    if (to >= 0) {{ event.preventDefault(); links[to].focus(); }}
  }});
  document.addEventListener("keydown", function (event) {{
    if (event.key === "Escape" && isOpen()) {{ event.preventDefault(); closeMenu(true); }}
  }});
  // Tabbing (or clicking into the artifact) out of the open menu closes it.
  menuEl.addEventListener("focusout", function (event) {{
    var to = event.relatedTarget;
    if (to && (menuEl.contains(to) || to === switchEl)) return;
    window.setTimeout(function () {{
      var active = document.activeElement;
      if (active && (menuEl.contains(active) || active === switchEl)) return;
      closeMenu(false);
    }}, 0);
  }});

  // --- Theme: the trusted shell owns the control; a fragment artifact applies it
  // via bridge.js. The correct theme is ALSO inlined at wrap time (the `?gp_theme=`
  // on the swapped iframe src below), so an iframe SWAP is FOUC-free; this path
  // handles live changes and re-applying the persisted choice after each load.
  // Desktop shows a three-way radio group in the bar; phones show the same choice
  // as buttons at the bottom of the page menu (CSS picks which one is visible).
  var THEMES = ["auto", "light", "dark"];
  // The nonce-gated head bootstrap already restored and allowlisted this value
  // before paint; validate once more rather than reading storage along two paths.
  var theme = document.documentElement.getAttribute("data-theme");
  if (THEMES.indexOf(theme) === -1) theme = "auto";

  var themeGroup = document.getElementById("gp-theme");
  var themeRadios = Array.prototype.slice.call(themeGroup.querySelectorAll("[data-theme-choice]"));
  var themeButtons = Array.prototype.slice.call(menuEl.querySelectorAll(".gp-menu-theme [data-theme-choice]"));
  function paintTheme() {{
    document.documentElement.setAttribute("data-theme", theme);
    themeRadios.forEach(function (b) {{
      var on = b.getAttribute("data-theme-choice") === theme;
      b.setAttribute("aria-checked", on ? "true" : "false");
      b.tabIndex = on ? 0 : -1;   // roving tabindex: one tab stop for the group
    }});
    themeButtons.forEach(function (b) {{
      b.setAttribute("aria-pressed", b.getAttribute("data-theme-choice") === theme ? "true" : "false");
    }});
  }}
  function sendTheme() {{
    // Post to the framed artifact; bridge.js validates source + schema on receipt.
    try {{ frame.contentWindow.postMessage({{ type: "theme", theme: theme }}, "*"); }} catch (e) {{}}
  }}
  function setTheme(next) {{
    if (THEMES.indexOf(next) === -1 || next === theme) return;
    theme = next;
    try {{ window.localStorage.setItem("gp-theme", theme); }} catch (e) {{}}
    paintTheme();
    sendTheme();
  }}
  // Query string that inlines the theme at wrap time (auto is the default → omit).
  function themeQuery() {{ return theme === "auto" ? "" : ("?gp_theme=" + theme); }}
  paintTheme();
  function onThemeClick(event) {{
    var b = event.target && event.target.closest ? event.target.closest("[data-theme-choice]") : null;
    if (b) setTheme(b.getAttribute("data-theme-choice"));
  }}
  themeGroup.addEventListener("click", onThemeClick);
  menuEl.querySelector(".gp-menu-theme").addEventListener("click", onThemeClick);
  // Radio-group keyboard model: arrows move and select within the group.
  themeGroup.addEventListener("keydown", function (event) {{
    var step = (event.key === "ArrowRight" || event.key === "ArrowDown") ? 1
      : (event.key === "ArrowLeft" || event.key === "ArrowUp") ? THEMES.length - 1 : 0;
    if (!step) return;
    event.preventDefault();
    var nextTheme = THEMES[(THEMES.indexOf(theme) + step) % THEMES.length];
    setTheme(nextTheme);
    var target = themeGroup.querySelector('[data-theme-choice="' + nextTheme + '"]');
    if (target) target.focus();
  }});

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
  var validSlug = /^[a-z0-9][a-z0-9-]{{0,63}}(?:\/[a-z0-9][a-z0-9-]{{0,63}}){{0,3}}$/;
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
  // to the browser too. Attached to BOTH the page menu and the bar's previous /
  // next links, so every swap goes through the same validated navigateTo path.
  function primaryNavAnchor(event) {{
    if (event.defaultPrevented) return null;
    if (event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return null;
    return event.target && event.target.closest ? event.target.closest("a[data-slug]") : null;
  }}
  function onNavClick(event) {{
    var a = primaryNavAnchor(event);
    if (!a) return;
    if (navigateTo(a.getAttribute("data-slug"))) {{
      event.preventDefault();
      closeMenu(true);
    }}
  }}
  function onStepClick(event) {{
    var a = primaryNavAnchor(event);
    if (!a || (a !== prevEl && a !== nextEl)) return;
    if (navigateTo(a.getAttribute("data-slug"))) {{
      event.preventDefault();
      // At either end the clicked link becomes inert; keep focus in the bar (on the
      // switcher when the space lists a single page and both links are inert).
      if (!a.hasAttribute("href")) {{
        var other = a === prevEl ? nextEl : prevEl;
        (other.hasAttribute("href") ? other : switchEl).focus();
      }}
    }}
  }}
  navEl.addEventListener("click", onNavClick, false);
  barEl.addEventListener("click", onStepClick, false);

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
        // Empty title falls back in both semantic contexts.
        let fallback = render("", "demo", "index", "", &nav_of(&["index"]), "n", None);
        assert!(fallback.contains("<title>demo / index</title>"));
        assert!(fallback.contains(r#"title="demo / index""#));
    }

    #[test]
    fn shell_has_three_way_theme_control_and_themes_its_own_chrome() {
        let html = render("", "demo", "index", "", &nav_of(&["index"]), "n", None);
        // A three-way radio group in the bar, plus the phone copy in the page menu.
        assert!(html.contains(r#"id="gp-theme" role="radiogroup" aria-label="Theme""#));
        for choice in ["auto", "light", "dark"] {
            assert!(html.contains(&format!(r#"role="radio" data-theme-choice="{choice}""#)));
            assert!(html.contains(&format!(
                r#"<button type="button" data-theme-choice="{choice}" aria-pressed="#
            )));
        }
        assert!(html.contains(r#"aria-label="System""#));
        // …and the shell sends a low-authority theme message to the framed artifact.
        assert!(html.contains(r#"type: "theme""#));
        // Persisted choice is restored by an allowlisted, nonce-gated head bootstrap
        // before theme CSS/body paint, with auto inlined as the no-script fallback.
        assert!(html.contains(r#"<html lang="en" data-theme="auto">"#));
        assert!(html.contains(r#"getItem("gp-theme")"#));
        assert!(html.contains(r#"setItem("gp-theme", theme)"#));
        let bootstrap = html.find(r#"<script nonce="n">"#).unwrap();
        assert!(bootstrap < html.find("<style>").unwrap());
        assert!(bootstrap < html.find("<body").unwrap());
        // A theme choice outside the allowlist is ignored.
        assert!(html.contains("if (THEMES.indexOf(next) === -1 || next === theme) return;"));
        // The same choice is applied to the trusted shell itself, whose bar uses
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
        // …but the shell no longer paints a second standalone copy above an artifact's H1.
        assert!(!html.contains(r#"id="gp-title""#));
    }

    #[test]
    fn shell_chrome_is_a_top_bar_and_keeps_status() {
        let html = render(
            "",
            "demo",
            "index",
            "Article",
            &nav_of(&["index", "sales"]),
            "n",
            None,
        );
        assert!(
            html.contains(r#"<header class="gp-bar" id="gp-bar" aria-label="Space navigation">"#)
        );
        assert!(!html.contains("gp-sidebar"));
        // The switcher is a disclosure button controlling the page menu.
        assert!(html.contains(
            r#"id="gp-switch" type="button" aria-expanded="false" aria-controls="gp-menu""#
        ));
        assert!(html.contains(r#"<div class="gp-menu" id="gp-menu" hidden>"#));
        assert!(html.contains(r#"id="gp-prev""#) && html.contains(r#"id="gp-next""#));
        // The submission status survives, outside the bar so single pages keep it.
        assert!(html.contains("id=\"gp-delivery\" role=\"status\" aria-live=\"polite\""));
        assert!(html.contains("function showDelivery(state, text)"));
        // Both chrome surfaces go through the shared validated navigate path.
        assert!(html.contains("navEl.addEventListener(\"click\", onNavClick"));
        assert!(html.contains("barEl.addEventListener(\"click\", onStepClick"));
        assert!(html.contains("sandbox=\"allow-scripts allow-top-navigation-by-user-activation\""));
        // Multi-page: the body carries no single-page class.
        assert!(html.contains("</head><body>\n"));
    }

    #[test]
    fn shell_single_page_space_shows_only_a_corner_theme_control() {
        let html = render(
            "",
            "demo",
            "index",
            "Article",
            &nav_of(&["index"]),
            "n",
            None,
        );
        assert!(html.contains(r#"</head><body class="gp-single">"#));
        assert!(html.contains("var MULTI = NAV.length > 1;"));
        // The switcher and previous / next ship hidden and are revealed only for MULTI.
        assert!(html.contains(r#"aria-controls="gp-menu" hidden>"#));
        assert!(html.contains(r#"<div class="gp-pn" id="gp-pn" hidden>"#));
        assert!(html.contains("body.gp-single .gp-bar { position:fixed; top:8px; right:8px;"));
        // An empty nav table is a single-page layout too.
        let empty = render("", "demo", "index", "", &[], "n", None);
        assert!(empty.contains(r#"</head><body class="gp-single">"#));
    }

    #[test]
    fn shell_space_label_prefers_title_then_loopback_name_never_hosted_slug() {
        let nav = nav_of(&["index", "sales"]);
        let titled = render_with_groups(
            "/p",
            "capslug",
            "index",
            "",
            &nav,
            &[],
            "Q3 review",
            "n",
            None,
        );
        assert!(titled.contains(r#"var SPACE_LABEL = "Q3 review";"#));
        let loopback = render_with_groups("", "reports", "index", "", &nav, &[], "", "n", None);
        assert!(loopback.contains(r#"var SPACE_LABEL = "reports";"#));
        let hosted = render_with_groups("/p", "capslug", "index", "", &nav, &[], "", "n", None);
        assert!(hosted.contains(r#"var SPACE_LABEL = "";"#));
        // A hostile space title is encoded data, inserted with textContent.
        let hostile = render_with_groups(
            "",
            "s",
            "index",
            "",
            &nav,
            &[],
            "</script><img src=x onerror=1>",
            "n",
            None,
        );
        let line = hostile
            .lines()
            .find(|l| l.contains("var SPACE_LABEL ="))
            .unwrap();
        assert!(!line.contains('<') && line.contains("\\u003c/script"));
        assert!(
            hostile.contains(r#"document.getElementById("gp-space").textContent = SPACE_LABEL;"#)
        );
    }

    #[test]
    fn shell_page_menu_has_keyboard_and_focus_handling() {
        let html = render(
            "",
            "demo",
            "index",
            "",
            &nav_of(&["index", "sales"]),
            "n",
            None,
        );
        assert!(html.contains(r#"event.key === "Escape""#));
        assert!(html.contains(r#"event.key === "ArrowDown""#));
        assert!(html.contains(r#"event.key === "Home""#));
        assert!(html.contains("closeMenu(true)"));
        assert!(html.contains(r#"menuEl.addEventListener("focusout""#));
        assert!(html.contains("outline:2px solid var(--gp-shell-focus)"));
        // Long titles truncate in the bar and wrap in the menu.
        assert!(html.contains(".gp-page { font-weight:600; white-space:nowrap; overflow:hidden; text-overflow:ellipsis;"));
        assert!(html.contains("text-decoration:none; overflow-wrap:anywhere;"));
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

    // --- grouped page menu (space-docsite-nav) -------------------------------

    fn item<'a>(slug: &'a str, title: &'a str, children: Vec<NavItemView<'a>>) -> NavItemView<'a> {
        NavItemView {
            slug,
            title,
            children,
        }
    }

    #[test]
    fn shell_renders_grouped_menu_with_nested_children() {
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
        let html = render_with_groups("", "docs", "index", "Home", &nav, &groups, "", "n", None);
        // The grouped page menu is built client-side (no server-rendered
        // artifact/label markup — createElement/textContent only).
        assert!(
            html.contains(r#"<nav class="gp-nav" id="gp-nav" aria-label="Pages in this space">"#)
        );
        assert!(html.contains("document.body.classList.add(\"gp-grouped\")"));
        // GROUPS is emitted as a JSON data literal carrying labels + nested children.
        let groups_line = html.lines().find(|l| l.contains("var GROUPS =")).unwrap();
        assert!(groups_line.contains("Perusarkkitehtuuri"));
        assert!(groups_line.contains("backtest-arkkitehdille"));
        assert!(groups_line.contains("\"children\""));
        // The shared validated navigate path is reused for BOTH surfaces.
        assert!(html.contains("barEl.addEventListener(\"click\", onStepClick"));
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
        let html = render_with_groups("", "docs", "index", "Home", &nav, &groups, "", "n", None);
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
        // the identical document, and the flat page menu is populated.
        let nav = nav_of(&["index", "sales"]);
        let shim = render("", "demo", "index", "Home", &nav, "n", None);
        let full = render_with_groups("", "demo", "index", "Home", &nav, &[], "", "n", None);
        assert_eq!(shim, full);
        // GROUPS is empty, so the client takes the flat-nav branch (the
        // classList.add("gp-grouped") call is present in the script but never runs).
        assert!(
            shim.contains("var GROUPS = [];")
                || shim.contains("var GROUPS = []")
                || shim.contains("var GROUPS = [ ]")
        );
        // The flat navigation is populated client-side inside the trusted page menu.
        assert!(shim.contains(r#"<nav class="gp-nav" id="gp-nav""#));
        assert!(shim.contains(r#"<div class="gp-menu" id="gp-menu" hidden>"#));
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
