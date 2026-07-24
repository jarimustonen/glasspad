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
  artifacts (exfil / escape / eval / postMessage-abuse probes). Wave 2a replaces
  this static registry with a live directory scanner; the probes stay as the
  regression suite. Also serves the `/_gp/v1/*` base libraries (Wave 2b): the
  real `base.css` (the `--gp-*` design system), `charts.js` (`gp.chart()` over
  Vega-Lite), `manifest.json`, and the vendored Vega stack (`vega`/`vega-lite`/
  `vega-embed`, pinned to the SRI-matched builds under `assets/`). The Vega
  bundles are vendored, not CDN-loaded, because the artifact `script-src` names
  only the loopback host — `charts.js` lazily loads them from `/_gp/v1/*`.
- `mod.rs` — routes (`/{space}/`, `/{space}/{slug}`, `/{space}/_c/{slug}`,
  `/_gp/v1/*`), slug/space grammar + reserved-name rejection, and header wiring.

## Frozen decisions (do not silently relax)

- **`script-src` includes `'unsafe-eval'`** — Vega-Lite needs it; verified
  empirically (design.md §4). Acceptable only *because* egress + null-origin
  isolation are untouched. Do not add `allow-same-origin` to the artifact iframe.
- **`connect-src 'none'`** is the exfil boundary. Wave 2a may widen it to the
  **named host only** (for SSE) — never to a foreign host.
- The artifact iframe is `sandbox="allow-scripts allow-top-navigation-by-user-activation"`.
  No `allow-same-origin`.

## Testing

- `cargo test artifact_host` — header-contract + grammar + guard unit/HTTP tests.
- `./test-security.sh` (repo root) — the **adversarial browser suite** (headless
  Chromium via Playwright, `tests/security/run.mjs`): proves the browser
  *enforces* the contract — per-channel exfil blocked at a network canary,
  sandbox escape fails, direct-open is sandboxed by the response header,
  postMessage abuse rejected, and the Vega `'unsafe-eval'` dependency. This is
  the gate; keep it green and **extend it** when later waves add attack surface
  (traversal/symlink probes in 2a, injection probes in Wave 4).
