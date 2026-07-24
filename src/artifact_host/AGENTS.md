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
  contains a framed artifact's own navigations (see design.md §4).
- `guards.rs` — control-plane guards (design.md §5): `host_guard` (DNS-rebinding
  defense, all routes, **fail-closed** on missing/foreign/malformed Host; only
  `127.0.0.1`/`localhost` + our port) + `control_origin_guard` (reject
  `Origin: null`/foreign on the `/api` control surface). Exercised end-to-end by
  the `server::tests` integration tests against `build_app()`.
- `fixtures.rs` — Wave-1 built-in `demo` space of **deliberately hostile**
  artifacts (exfil / escape / eval / postMessage-abuse probes). These are the
  security regression suite and stay forever. `mod.rs` resolves a request against
  the **live snapshot first** (Wave 2a) and falls back to these fixtures only for
  spaces the snapshot doesn't contain. Also serves the `/_gp/v1/*` base libraries
  (Wave 2b): the real `base.css` (the `--gp-*` design system), `charts.js`
  (`gp.chart()` over Vega-Lite), `manifest.json`, and the vendored Vega stack
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
  `sandbox` directive is ignored for subresource loads, so JS/CSS/img/fonts still
  load into an artifact) + `Access-Control-Allow-Origin: *` for module/font reads.
- The directory watch is a **dependency-free 500 ms poll** (`server.rs`
  `spawn_watcher`/`fingerprint`) that rescans + atomically swaps + fires the SSE
  reload on change; a rescan that fails (e.g. a fresh collision) keeps the
  last-good snapshot serving.

## Frozen decisions (do not silently relax)

- **`script-src` includes `'unsafe-eval'`** — Vega-Lite needs it; verified
  empirically (design.md §4). Acceptable only *because* egress + null-origin
  isolation are untouched. Do not add `allow-same-origin` to the artifact iframe.
- **`connect-src` is the exfil boundary.** Wave 1 set it to `'none'`; Wave 2a
  widened it to name **exactly the loopback SSE-reload path** on both origins
  (`http://127.0.0.1:PORT/_gp/reload http://localhost:PORT/_gp/reload`) — a CSP
  path-source, so `/api/*`, any other path, foreign hosts, and canaries all still
  violate. Do **not** relax it to a bare origin (that re-opens `/api/*`) or a
  foreign host. The security suite proves a self-host `/api` fetch still blocks.
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
  rejected, the Vega `'unsafe-eval'` dependency (**21 checks — keep green**). Phase
  2 (Wave 2a): live-directory **server-side** probes — path traversal (browsers
  can't help here) and symlink escape are HTTP/exit-code checks against a real
  served space, plus hostile-SVG-asset sandboxing and the SSE-scoped `connect-src`.
  Keep it green and **extend it** when later waves add attack surface (injection
  probes in Wave 4).
