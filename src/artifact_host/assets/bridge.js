/* Glasspad artifact bridge — /_gp/v1/bridge.js
 *
 * The CHILD side of the parent<->iframe security channel (design.md §6). It is
 * auto-injected into **fragment-wrapped** artifacts only (`wrap.rs`); a
 * full-document artifact opts in itself and instead falls back to
 * `target="_top"` for cross-navigation. Two jobs, both low-authority:
 *
 *   1. Same-space RELATIVE link clicks -> `postMessage` the parent to swap the
 *      iframe. It never navigates the frame itself and never sends a URL — only a
 *      candidate slug the parent re-validates against its own artifact table,
 *      grammar, size, and rate caps (all frozen in Wave 1's shell). External /
 *      absolute links are deliberately NOT intercepted (default browser
 *      behavior, contained by the parent's `frame-src 'self'`).
 *
 *   2. Apply the theme the parent chrome hands down on a later toggle
 *      (`{type:"theme", theme}`). The *correct* theme is inlined at wrap time to
 *      avoid FOUC — the bridge only handles subsequent toggles. Setting
 *      `data-theme` on <html> re-themes base.css and re-renders tracked charts
 *      (charts.js observes the attribute).
 *
 *   3. The RETURN CHANNEL: `gp.submit(data)` sends user input BACK to the agent
 *      that authored the artifact. It only `postMessage`s the parent a
 *      `{type:"submit", data, contentVersion}` message — the trusted shell
 *      validates it, binds it to this shell's own space/slug, and POSTs it (the
 *      artifact itself stays under `connect-src 'none'`, no egress). A native
 *      `<form>` submit (blocked by the sandbox anyway — no `allow-forms`) is
 *      intercepted and routed through the same one audited helper.
 *
 * This script grants the artifact NO new reach: it can already run inline JS and
 * `postMessage` the parent under Wave 1's CSP. The bridge only standardizes the
 * one message shape the parent accepts and the one the parent may send back.
 */
(function () {
  "use strict";

  var loc = window.location;
  // A servable slug/space matches the server-side grammar exactly.
  var NAME = /^[a-z0-9][a-z0-9-]{0,63}$/;

  // Only meaningful when actually FRAMED by the shell. A direct-opened content
  // route has `window.parent === window`, so intercepting a click would
  // `preventDefault` it and post a navigate to ourselves (dead link). Leave those
  // to the browser entirely.
  var FRAMED = window !== window.parent;

  // Our own space, parsed from the content path `/{space}/_c/{slug}`. Absent (the
  // artifact was not served on the content route) -> link interception is inert.
  var pathMatch = loc.pathname.match(/^\/([a-z0-9][a-z0-9-]{0,63})\/_c\/([a-z0-9][a-z0-9-]{0,63})$/);
  var SPACE = pathMatch ? pathMatch[1] : null;

  // The real serving origin. The artifact document itself has an opaque (null)
  // origin, so `loc.origin` is the string "null" and useless for comparison —
  // parse the concrete origin out of the (still-real) href instead.
  var ORIGIN = (function () {
    try {
      return new URL(loc.href).origin;
    } catch (e) {
      return null;
    }
  })();

  // --- 1. same-space relative-link interception --------------------------------
  document.addEventListener(
    "click",
    function (event) {
      if (!FRAMED || !SPACE || !ORIGIN) return;
      // Respect a handler that already claimed the event, and let the browser
      // own modified / non-primary clicks (new tab, download, context menu).
      if (event.defaultPrevented) return;
      if (event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;

      var a = event.target && event.target.closest ? event.target.closest("a[href]") : null;
      if (!a) return;

      // Opt-out targets keep their native behavior (`_blank`, `_top`, a named
      // frame). Only a same-frame navigation is a candidate for swapping.
      var target = a.getAttribute("target");
      if (target && target !== "_self") return;
      // A download link keeps its native behavior — never a navigation.
      if (a.hasAttribute && a.hasAttribute("download")) return;

      // Classify the RAW href first: only a **path-relative** link is a candidate.
      // Absolute-path (`/x`), scheme-relative (`//host`), and explicit-scheme
      // (`http:`, `mailto:`, `javascript:`, …) links are external/absolute and are
      // deliberately NOT intercepted — they keep native behavior (contained by the
      // parent's `frame-src 'self'`). This matches the documented contract.
      var raw = a.getAttribute("href");
      if (!raw) return;
      var t = raw.replace(/^\s+/, "");
      if (t === "" || t.charAt(0) === "#" || t.charAt(0) === "?") return; // same-page
      if (t.charAt(0) === "/") return; // absolute-path or scheme-relative (`//`)
      if (/^[a-zA-Z][a-zA-Z0-9+.-]*:/.test(t)) return; // explicit scheme

      // Defense in depth: the resolved anchor must still be same-origin (an
      // SVGAElement has no `.origin` → left to the browser) and a same-space
      // content URL `/{SPACE}/_c/{something}`.
      if (a.origin !== ORIGIN) return;
      var m = a.pathname.match(/^\/([a-z0-9][a-z0-9-]{0,63})\/_c\/([^\/?#]+)$/);
      if (!m || m[1] !== SPACE) return;

      // Tolerate an explicit `.html`/`.htm` (agents link files); the slug is the
      // stem. Anything the server grammar would reject is left to the browser.
      var slug = m[2].replace(/\.html?$/i, "");
      if (!NAME.test(slug)) return;

      event.preventDefault();
      try {
        parent.postMessage({ type: "navigate", slug: slug }, "*");
      } catch (e) {
        /* parent gone / postMessage unavailable — fall through to no-op */
      }
    },
    false
  );

  // --- 2. theme applied on a later parent toggle -------------------------------
  var THEMES = { light: true, dark: true, auto: true };
  window.addEventListener(
    "message",
    function (event) {
      // The only trustworthy sender for a framed artifact is our own parent.
      if (event.source !== window.parent) return;
      var d = event.data;
      if (!d || typeof d !== "object" || d.type !== "theme") return;
      if (typeof d.theme !== "string" || THEMES[d.theme] !== true) return;
      document.documentElement.setAttribute("data-theme", d.theme);
    },
    false
  );

  // --- 3. return channel: gp.submit + native <form> interception ---------------
  // The content-version this artifact was wrapped as (inlined by wrap.rs). Echoed
  // to the shell so the server can reject a submission for a stale round; absent
  // (a full-document artifact never gets this meta) means "no echo" and the server
  // stamps its own authoritative version.
  var CONTENT_VERSION = (function () {
    var m = document.querySelector('meta[name="gp-content-version"]');
    return m ? m.getAttribute("content") : null;
  })();

  window.gp = window.gp || {};
  // Send `data` (any JSON-serializable value) back to the authoring agent. Returns
  // false so it is convenient as an inline handler (`onclick="return gp.submit(…)"`).
  // It only messages the parent; the parent is the airlock that actually POSTs.
  window.gp.submit = function (data) {
    if (!FRAMED) return false; // direct-open: no parent shell to receive it
    try {
      var msg = { type: "submit", data: data };
      if (CONTENT_VERSION) msg.contentVersion = CONTENT_VERSION;
      parent.postMessage(msg, "*");
    } catch (e) {
      /* parent gone / postMessage unavailable — no-op */
    }
    return false;
  };

  // A native form submit is blocked by the sandbox (no `allow-forms`), but the
  // `submit` event still fires — intercept it and route the fields through the one
  // audited helper, so an author can write an ordinary <form> and it "just works".
  document.addEventListener(
    "submit",
    function (event) {
      if (!FRAMED) return;
      var form = event.target;
      if (!form || form.tagName !== "FORM") return;
      event.preventDefault();
      var data = {};
      try {
        new FormData(form).forEach(function (value, name) {
          // Only string fields (skip File inputs — the null-origin sandbox has no
          // real file access, and a File would not serialize). Repeated names
          // collapse into an array so multi-selects/checkbox groups round-trip.
          if (typeof value !== "string") return;
          if (Object.prototype.hasOwnProperty.call(data, name)) {
            if (!Array.isArray(data[name])) data[name] = [data[name]];
            data[name].push(value);
          } else {
            data[name] = value;
          }
        });
      } catch (e) {
        /* FormData unavailable — submit an empty object rather than throwing */
      }
      window.gp.submit(data);
    },
    false
  );
})();
