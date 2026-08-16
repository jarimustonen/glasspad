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
  contains a framed artifact's own navigations (see design.md §4). It also emits
  the **emoji SVG favicon** on the OUTER shell `<head>` only (a base64 `data:` SVG
  `<link rel="icon">` built by the top-level `crate::favicon` module; per-space emoji
  wins over the loopback host default, else a built-in default) — the sandboxed
  artifact / content route is untouched. It also owns the **theme toggle** (Wave 3b): on toggle it `postMessage`s the framed artifact
  `{type:"theme", theme}` and, on the next iframe swap, inlines the theme via
  `?gp_theme=` so the wrap is FOUC-free. **Wave 4 — nav chrome:** renders a
  `<nav>` listing the space's artifacts from the server-resolved `(slug, title)`
  table, built **entirely client-side with `createElement` + `textContent`** (the
  artifact-derived title never touches an HTML sink; `require-trusted-types-for
  'script'` with no default policy makes any accidental `innerHTML` throw). A
  single validated `navigateTo(slug)` path — grammar + `KNOWN_SET` allowlist, same
  one the postMessage bridge uses — swaps the framed artifact in place (no full
  reload, same-slug is a no-op so a hostile child can't loop it) and updates the
  active entry + document/iframe title. (URL-sync / deep-linking for in-place swaps
  is deferred — see the wave's terminal report.) A full-document artifact gets
  **no** injected `bridge.js`; its author writes native same-space links with
  `target="_top"` (the D1 top-nav path). The parent chrome never needs it because
  the parent is not sandboxed. Shell `script-src` names only the nonce (not
  `'self'`) — the shell loads no same-origin script file, so this shrinks the
  injection blast radius.
- `wrap.rs` (Wave 3b) — **fragment detection + the bridge/theme injection point**.
  `is_fragment` is BOM/whitespace/comment-tolerant (a full document opens with
  `<!doctype>`/`<html …>`; anything else is a fragment). A fragment is wrapped
  into a full document with `data-theme` inlined (no FOUC), `base.css` linked, and
  `bridge.js` injected — **only here**, so a full document never gets a bridge
  silently. Full documents are served verbatim. Wrapping runs under the same
  frozen artifact CSP (`headers::artifact_csp`) and widens nothing; it is NOT
  sanitization (the sandbox/CSP is the boundary, design.md §7).
- `render.rs` (0.3.0) — the **markdown + reusable-template renderer** (the
  `glasspad render` path). Renders a markdown body to HTML (CommonMark + GFM via
  `pulldown-cmark`) and splices it into a template's single `{{content}}` slot,
  producing an artifact **body** string that flows through the ordinary serve
  path (`one_artifact_snapshot` → `artifact_content` → `wrap::render_artifact`).
  The template is **client-shipped and untrusted** but governs **only the body**:
  the CSP / sandbox / Trusted-Types / hardening headers are set server-side on the
  `_c` response regardless of body bytes (a `<meta http-equiv=CSP>` can only
  *tighten* — fails closed), and the trusted shell is a different route built from
  the resolved title via `textContent`, so a template can neither widen the
  boundary nor inject the shell. Built-in templates (`prose` =
  `<article class="gp-prose">…</article>` [default], `dashboard` = `.gp-card`) are
  **fragments**, so they inherit `base.css` (incl. the hardened `.gp-prose`
  reading theme) + `bridge.js` for free. `wrap.rs`/`shell.rs` are unchanged.
  **Per-page TOC rail (prose-page-toc):** the built-in `prose` path stamps a
  **server-generated** anchor `id` on every heading (slugify heading text +
  deterministic collision disambiguation — never an attacker-controlled raw id) and,
  when the page has ≥2 H2/H3 headings, emits an "on this page" `<nav class="gp-toc">`
  as a **sibling** of `.gp-prose` inside a `.gp-doc` grid (a native `<details>` —
  collapsible, no JS; CSS hides it below a width breakpoint). This is **approach (a)**:
  the rail lives inside the artifact's OWN fragment, so `#anchor` links resolve natively
  inside the null-origin sandbox — **no shell involvement, no postMessage surface, CSP
  unchanged**. Heading text is untrusted and reaches the rail only server-side
  HTML-escaped. Fewer than 2 H2/H3 (or a non-prose / full-document artifact) degrades to
  the plain prose fragment (no empty rail). `dashboard`/custom templates are unchanged.
  **Diagrams (markdown-diagrams):** authored **inline SVG** is the supported diagram
  path — the producer owns SVG generation and embeds it; the `.md`/template renderer
  passes raw HTML/SVG through verbatim (no strip, no rewrite, **no sanitization**), so a
  diagram displays inside the null-origin sandbox with **no CSP change**. An authored SVG
  is untrusted content like any markup (it *may* carry `<script>`/`<foreignObject>`/URL
  refs — SVG is a scripting host); it is safe because of the **existing** boundary
  (null-origin, no `allow-same-origin`, `connect-src 'none'`), NOT because SVG is inert.
  This feature grants no new authority. `base.css` supplies the theming only — a `--gp-*`
  status palette (done/next/blocked/future) + `.gp-diagram`/`.gp-node`/`.gp-edge`/
  `.gp-status-*`/`.gp-legend` classes — so a colour-coded status DAG reads in both themes.
  Full pattern + security/accessibility notes: [`AGENTS-DIAGRAMS.md`](AGENTS-DIAGRAMS.md);
  runnable example: `examples/status-dag/`.
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
  loopback host — `charts.js` lazily loads them from `/_gp/v1/*`. `BASE_LIB_NAMES`
  enumerates the served set (minus the `probe.js` test stub) that a self-contained
  `glasspad build` (`src/build.rs`) bundles under `_gp/v1/`, resolved through
  `gp_asset` so the list never drifts from what the server serves.
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
  **Markdown-native spaces (Gap 2):** a top-level `.md`/`.markdown` file is a page
  too — `scan_dir` buffers it, then renders it through a built-in fragment template
  (`render::render_to_body`; `prose` default or `dashboard`, selected per-space via
  `glasspad.yaml`'s `template:` key) into an artifact **body** (slug = stem), which
  then flows through the identical serve path as an `.html` artifact — so the
  security boundary is unchanged (the template governs only the body; the CSP /
  sandbox / Trusted-Types headers are set server-side on the `_c` response, and
  `wrap` injects `base.css` + `bridge.js`). `.md` and `.html` pages coexist; a
  same-stem `.md`+`.html` (or `.md`+`.markdown`) pair is a `DuplicateSlug` hard
  error. The rendered body is re-capped at `MAX_FILE_BYTES` (markup can amplify);
  an unknown `template:` name is a hard error. A path-like `template:` value (for
  example `templates/brand.html`) loads one regular UTF-8 **fragment** file relative
  to the space root, rejects symlinks/traversal/full documents, and applies its
  exactly-one `{{content}}` slot to every Markdown page during scanning; rendered
  bodies are then uploaded, so hosted serving never reads the local template path.
  Custom pages retain server-generated heading anchors and the TOC rail; the trusted
  shell still owns grouped navigation and landing pages. All of `serve`/`build`/
  `publish-space` inherit this for free (they consume the produced `Space`).
  **Grouped nav + generated landing (space-docsite-nav):** the manifest gains an
  optional `groups:` key (named groups → ordered `members`; a member is a bare slug
  or a map with `title`/`desc`/one level of companion `children`). `finalize`
  reconciles it against the artifact set into `Space.nav_groups` (drops dangling
  slugs, dedups, discards grandchildren, drops empty groups, sanitizes labels/titles
  like a resolved title). The flat `Space.nav` stays the **complete allowlist** — no
  groups → empty `nav_groups` → today's flat nav is byte-compatible. **Companion
  nesting is a manifest-level mapping** — glasspad never parses dotted
  `x.arkkitehdille.md` stems (out of scope; the producer ships slug-safe pages +
  declares `children:`). When a space has no `index`/`home` page AND (declares groups
  OR has ≥2 pages), `finalize` **generates an `index` landing artifact** (a `gp-prose`
  table of contents, grouped or flat, with per-doc descriptions from the manifest or
  the doc's first paragraph) instead of the old redirect stub; it is a normal artifact
  so serve/build/hosted inherit it and it is idempotent. The shell renders the grouped
  vertical sidebar via `render_with_groups` (client `createElement`+`textContent`,
  reusing the one validated `navigateTo` allowlist — the security model is unchanged);
  wire/store carry `nav_groups` (`#[serde(default)]`, re-reconciled on the untrusted
  ingest boundary).
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
- **Return channel = the shell is the airlock; the artifact stays frozen.** An
  interactive artifact sends user input back via `gp.submit(data)` (bridge.js) →
  `postMessage({type:"submit",…})` → the trusted shell (`connect-src 'self'`) POSTs
  it to `/{space}/_gp/submit` (loopback) / `/api/v1/pages/{slug}/submit` (hosted).
  The **artifact keeps `connect-src 'none'` and no `allow-forms`** — do **not** add
  either. The server binds the submission's slug/space + owning tenant +
  content-version from the *trusted request context* (URL path + stored page
  meta/body), never the payload; the submit endpoint is `Origin`-allowlisted (CSRF),
  size + rate capped; hosted reads are API-key + per-tenant scoped. See
  `issues/artifact-return-channel/`. The store + long-poll + SSE stream are in
  `src/submissions.rs`; handlers in `hosted/submit.rs` (hosted) and `server.rs`
  (loopback). The agent consumes submissions three ways over the **same** persisted-
  cursor store (`since=<id>`, no re-deliver/skip): plain poll (A1 `…/submissions`),
  long-poll (A3 `…/submissions/wait`, the default `await-submission`), and an **SSE
  stream** (A2 `…/submissions/stream`, `await-submission --stream`) that pushes each
  submission as a `submission` event. The stream reuses `wait`'s keyed broadcast but a
  **separate** held-connection budget (`MAX_STREAM_WAITERS` + a per-key cap, so
  indefinitely-held streams never starve the long-poll) and is agent-facing only
  (API-key / loopback); the **artifact** never reaches it (`connect-src 'none'` unchanged
  — the stream path is not named in the artifact CSP).
- **Multi-round (B2) reuses the reload SSE carrier — no new push channel.** After a
  submission the agent re-renders the *same live page* and the connected shell swaps
  the framed artifact **in place**. The shell's one `EventSource("/_gp/reload")` now
  multiplexes two signals ([`ArtifactHost::ReloadEvent`]): a full-shell `reload` (the
  loopback dev file-watch, unchanged) and a keyed `round` event (space + new
  content-version + monotonic round id) that swaps the current artifact in place — a
  fresh content-route fetch under the **identical frozen CSP** (each round stays
  null-origin, `connect-src 'none'`, no `allow-forms`; pushing a round widens
  nothing). The event carries **no URL**, and round delivery is **scoped server-side**
  by `?space=<slug>` on the reload stream — a connection receives a `round` event only
  for the exact page slug it named, so the global broadcast can't fan one tenant's
  capability slug out to another's shell (client-side `space === SPACE` is
  defense-in-depth on top). Loopback
  multi-round = rewrite the served file (the watcher fires the full reload). Hosted =
  `POST /api/v1/pages/{slug}/rounds` (`hosted::rounds`, API-key + owner-scoped): the
  re-render is a durable **live overlay** stored as an immutable generation under
  `pages/<slug>/live/generations/<id>/` with an atomically-swapped `current` pointer
  (so a crash during round N+1 keeps round N — see `hosted::store`'s generation-pointer
  model; a pre-generation `live.html`/`live.json` overlay is still read on upgrade) over
  the immutable baseline `artifact.html`, the served snapshot body is swapped, and
  `notify_round` pushes the SSE swap. Cross-round binding is the existing content-version check — a
  submission answering a stale round is rejected `409`. Client surface: `glasspad
  push-round`.

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
  nodes anywhere in the chrome, no layout break), and full-document `target="_top"`
  cross-nav (**41 checks — keep green**).
  Phase
  2 (Wave 2a): live-directory **server-side** probes — path traversal (browsers
  can't help here) and symlink escape are HTTP/exit-code checks against a real
  served space, plus hostile-SVG-asset sandboxing, the SSE-scoped `connect-src`,
  and (Wave 4) a **server-side nav-injection check** (a hostile artifact title is
  emitted only `<`-encoded in the nav data literal, never as raw markup) +
  the shell Trusted-Types header. Plus (loopback-lan-serve) **LAN-serve probes**:
  with `loopback serve --bind <LAN-IP>`, the opted-in host + loopback are served but
  a foreign `Host` to the LAN socket is STILL `421`-refused (DNS-rebinding held), the
  sandbox/CSP/airlock are unchanged (the LAN origin is only *added* to the host set),
  and a wildcard `--bind 0.0.0.0` is refused; the reachable-socket probe self-SKIPs
  on a LAN-less host so the suite stays hermetic.
  Keep it green and **extend it** when later waves add attack surface (injection
  probes in Wave 4).
- **Green means the WHOLE suite ran, not just Phase 1.** `./test-security.sh` is
  `set -euo pipefail`, so the FIRST failing probe aborts the run mid-suite — you can
  see Phase 1's `✅ ALL PASSED (48 checks)` and still have Wave 2a (the Gap/space
  probes) never execute. Confirm the run reaches its final **`✅ Wave 2a space-model
  probes PASSED`** line AND exits 0; the 48-check count alone is not "green." (0.10.0:
  a stale Gap-2 `grep -q '<h1>Home</h1>'` broke on the new prose heading `id`s and
  aborted Wave 2a — a worker's "48 green" claim missed it. Re-verify worker green
  claims against the full suite.) Also kill stray `target/debug/glasspad`
  serve/host-serve processes between runs — a leftover port bind causes a spurious
  early death (a false red).
