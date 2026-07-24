# `artifact_host` — v0.2 HTML-artifact host (security core)

The null-origin sandboxed iframe host and its security contract. This is the
**Wave 1 security gate** the whole `html-artifact-host-rewrite` branches from;
read `issues/html-artifact-host-rewrite/{design,plan,wave-plan}.md` first.
It runs **alongside** the v0.1 pad server — no old code is removed until Wave 5.

## Files

- `headers.rs` — the two header sets. `artifact_csp()` = sandbox + egress CSP
  that **names both explicit loopback origins** (`'self'` is meaningless under a
  null origin; the shell is reachable at `127.0.0.1` *and* `localhost`);
  `shell_csp()` = trusted-chrome CSP (nonce'd script + Trusted Types).
  `hardening_headers()` = `nosniff` / `no-referrer` / `Permissions-Policy` deny.
  Content + shell responses also carry `Cache-Control: no-store`.
- `shell.rs` — the trusted parent document. Runs the **parent side of the
  postMessage bridge**, in this order: `event.source === iframe.contentWindow`
  (NOT `event.origin`, which is `"null"` for every sandboxed frame) → rate cap →
  reject transferred `ports` → exact `{type:"navigate", slug:<known>}` schema on
  small typed fields (no `JSON.stringify` of the payload). Inserts artifact text
  as `textContent` (never `innerHTML`). The shell's `frame-src 'self'` also
  contains a framed artifact's own navigations (see design.md §4). It also owns
  the **theme toggle** (Wave 3b): on toggle it `postMessage`s the framed artifact
  `{type:"theme", theme}` and, on the next iframe swap, inlines the theme via
  `?gp_theme=` so the wrap is FOUC-free. **Wave 4 — nav chrome:** renders a
  `<nav>` listing the space's artifacts from the server-resolved `(slug, title)`
  table, built **entirely client-side with `createElement` + `textContent`** (the
  artifact-derived title never touches an HTML sink; Trusted Types would throw on
  one). A single validated `navigateTo(slug)` path — grammar + `KNOWN_SET`
  allowlist, same one the postMessage bridge uses — swaps the framed artifact in
  place (no full reload) and updates the active entry + document title. A
  full-document artifact's own links still fall back to `target="_top"` (the D1
  top-nav path, author-controlled via `bridge.js`); the parent chrome never needs
  it because the parent is not sandboxed.
- `wrap.rs` (Wave 3b) — **fragment detection + the bridge/theme injection point**.
  `is_fragment` is BOM/whitespace/comment-tolerant (a full document opens with
  `<!doctype>`/`<html …>`; anything else is a fragment). A fragment is wrapped
  into a full document with `data-theme` inlined (no FOUC), `base.css` linked, and
  `bridge.js` injected — **only here**, so a full document never gets a bridge
  silently. Full documents are served verbatim. Wrapping runs under the same
  frozen artifact CSP (`headers::artifact_csp`) and widens nothing; it is NOT
  sanitization (the sandbox/CSP is the boundary, design.md §7).
- `guards.rs` — control-plane guards (design.md §5): `host_guard` (DNS-rebinding
  defense, all routes, **fail-closed** on missing/foreign/malformed Host; only
  `127.0.0.1`/`localhost` + our port) + `control_origin_guard` (reject
  `Origin: null`/foreign on the `/api` control surface). Exercised end-to-end by
  the `server::tests` integration tests against `build_app()`.
- `fixtures.rs` — Wave-1 built-in `demo` space of **deliberately hostile**
  artifacts (exfil / escape / eval / postMessage-abuse probes). These are the
  security regression suite and stay forever. `mod.rs` resolves a request against
  the **live snapshot first** (Wave 2a) and falls back to these fixtures only for
  spaces the snapshot doesn't contain. The `demo` space also carries two benign
  **fragment** artifacts (`nav-a`/`nav-b`) that link to each other — the Wave 3b
  bridge nav demonstration the adversarial suite drives end-to-end — plus (Wave 4)
  `nav-full` (a **full document** whose same-space link uses `target="_top"`, the
  D1 top-nav path) and `inject` (an artifact whose `<title>` decodes to raw hostile
  markup — the trusted-parent nav-injection probe target). Also serves
  the `/_gp/v1/*` base libraries (Wave 2b/3b): the real `base.css` (the `--gp-*`
  design system), `charts.js` (`gp.chart()` over Vega-Lite), `bridge.js` (the
  fragment-only parent↔iframe channel), `manifest.json`, and the vendored Vega stack
  (`vega`/`vega-lite`/`vega-embed`, SRI-pinned under `assets/`). The Vega bundles
  are vendored, not CDN-loaded, because the artifact `script-src` names only the
  loopback host — `charts.js` lazily loads them from `/_gp/v1/*`.
- `space.rs` (Wave 2a) — the **space model + directory scanner** (security-
  sensitive). `scan_dir` reads a directory into an immutable `Space` (artifacts +
  `assets/`), all-or-nothing: slug grammar, **reserved-name / collision** hard
  errors, **symlink rejection** (`lstat` every entry) + canonical-path containment,
  per-file / per-space **size limits**, extension→**MIME** allowlist, and **title
  resolution** (a small tag tokenizer, *not* a regex — entity-decoded, length-
  bounded). `Snapshot` is swapped atomically by `ArtifactHost` so a half-written
  file is never served. `asset_key_for_request` grammar-checks a request sub-path
  into a key that must exact-match the pre-scanned asset map (traversal is
  structurally impossible — you can only fetch a key that already exists).
- `mod.rs` — routes (`/{space}/`, `/{space}/{slug}`, `/{space}/_c/{slug}`,
  `/{space}/assets/{*path}`, `/_gp/reload`, `/_gp/v1/*`), slug/space grammar +
  reserved-name rejection, header wiring, the live-snapshot + fixtures resolution,
  and the `ArtifactHost` state (atomic snapshot swap + SSE reload broadcast).
  Space **asset** responses carry `nosniff` + `Content-Security-Policy: sandbox`
  so a hostile top-level SVG/HTML asset runs script-less in a null origin (the
  `sandbox` directive is ignored for subresource loads, so JS/CSS/img still load
  into an artifact). **No `Access-Control-Allow-Origin`** on user assets — a
  wildcard would let any foreign page `fetch()`-read a space's assets (the request
  carries a legit loopback `Host`). Classic subresources need no CORS.
- The directory watch is a **dependency-free 500 ms poll** (`server.rs`
  `spawn_watcher`/`fingerprint`) that rescans + atomically swaps + fires the SSE
  reload on change; a rescan that fails (e.g. a fresh collision) keeps the
  last-good snapshot serving.

## Frozen decisions (do not silently relax)

- **`script-src` includes `'unsafe-eval'`** — Vega-Lite needs it; verified
  empirically (design.md §4). Acceptable only *because* egress + null-origin
  isolation are untouched. Do not add `allow-same-origin` to the artifact iframe.
- **`connect-src 'none'` is the exfil boundary — keep it closed.** Live reload is
  driven from the **trusted shell** (its `connect-src 'self'` permits the SSE
  `EventSource`), so the *artifact* stays fully closed. Do not widen the artifact
  `connect-src`. If Wave 3b's `bridge.js` needs in-frame reload, widen to the exact
  `/_gp/reload` **path** (a CSP path-source) **plus a query-rejecting guard** —
  never a bare origin (re-opens `/api/*`), never a foreign host.
- The artifact iframe is `sandbox="allow-scripts allow-top-navigation-by-user-activation"`.
  No `allow-same-origin`.

## Testing

- `cargo test artifact_host` — header-contract + grammar + guard unit/HTTP tests,
  plus the Wave 2a scanner tests (`space::fs_tests`: reserved/collision/oversize/
  non-UTF-8/**symlink**/manifest) and the `atomic_swap_never_serves_a_partial_snapshot`
  concurrency test.
- `./test-security.sh` (repo root) — the **adversarial suite**. Phase 1: the
  headless-Chromium browser probes (`tests/security/run.mjs`) prove the browser
  *enforces* the contract — per-channel exfil blocked at a network canary, sandbox
  escape fails, direct-open is sandboxed by the response header, postMessage abuse
  rejected, the Vega `'unsafe-eval'` dependency, the **Wave 3b bridge nav**
  (a same-space relative-link click swaps the iframe via the validated bridge, an
  unknown-slug / extra-property / transferred-port navigates are still rejected,
  external + absolute-path links are not intercepted, and the theme toggle re-themes
  the artifact — a wrong-source theme message is ignored), **plus the Wave 4 nav
  chrome**: the trusted parent lists the space's artifacts and swaps the iframe in
  place on click (no reload, active entry marked), the **nav-injection probe** (a
  hostile artifact title renders as inert `textContent` — no execution, no element
  nodes, no layout break), and full-document `target="_top"` cross-nav
  (**40 checks — keep green**).
  Phase
  2 (Wave 2a): live-directory **server-side** probes — path traversal (browsers
  can't help here) and symlink escape are HTTP/exit-code checks against a real
  served space, plus hostile-SVG-asset sandboxing, the SSE-scoped `connect-src`,
  and (Wave 4) a **server-side nav-injection check** (a hostile artifact title is
  emitted only `<`-encoded in the nav data literal, never as raw markup) +
  the shell Trusted-Types header.
  Keep it green and **extend it** when later waves add attack surface (injection
  probes in Wave 4).
