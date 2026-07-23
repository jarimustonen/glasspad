# Review synthesis — plan.md + design.md

> **RESOLVED (2026-07-23).** Decisions locked with the PO: **v0.2 is
> localhost-only** (D4 and the whole team/cloud story dropped), and we go with
> the recommendations — D1 = null-origin sandbox + `CSP: sandbox` response
> header + egress CSP naming the host (single local origin; `allow-same-origin`
> flip kept for later); D2 = first-class assets/data; D3 = serve the directory
> live, no push / no server store; `glasspad:` scheme replaced by relative links.
> The updated `plan.md` and `design.md` reflect this; the "Decisions the PO needs
> to make" section below is kept for the record.

Multi-model review (`/llm-review`, 2 rounds): Gemini 3.1 Pro, GPT-5.6-sol,
Claude Opus 4.7, DeepSeek v4 Pro. The direction (delete the section-DSL, host
agent HTML) was endorsed by all four. The security model needs the most work.
Full transcript: `history/review-html-artifact-host-rewrite.md`.

## Critical issues (consensus — all/most reviewers)

1. **Sandbox does NOT block egress.** A null-origin `sandbox="allow-scripts"`
   iframe can still *send* requests (fetch/img/sendBeacon/WebSocket); it only
   can't *read* cross-origin responses. `design.md` §2's claim "cannot make
   requests to the API" is **wrong**. Exfiltration is blocked only by CSP. The
   API must have its own auth + CSRF and reject `Origin: null` — the sandbox is
   never an authorization boundary. → rewrite `design.md` §2.

2. **CSP `'self'` is broken under a null origin.** `'self'` resolves to the
   opaque origin and matches nothing, so `/_gp/*` base libraries won't load. CSP
   must enumerate the **explicit content host**, and `/_gp/*` needs
   `Access-Control-Allow-Origin` (for modules/fonts/data fetches — classic
   `<script src>`/`<link>` don't need CORS). → rewrite `design.md` §4 with a
   concrete, tested header.

3. **Artifact *responses* need their own `Content-Security-Policy: sandbox
   allow-scripts` directive.** (Raised by GPT-5.6, missed in the plan.) The
   iframe `sandbox` attribute does nothing if someone opens the artifact URL
   directly (copied link, new tab, crawler) — it then runs unsandboxed on the
   content origin. This is a foundational gap.

4. **postMessage `origin` validation is meaningless** — every sandboxed iframe
   has `origin === "null"`. Validate `event.source === iframe.contentWindow`,
   cap payload size/rate, whitelist message types, and invalidate bridge state
   on iframe navigation. Keep the bridge low-authority (navigate-to-known-slug
   only; a nonce is not an auth boundary since artifact JS can read it).

5. **Full-document `target="_top"` navigation is dead** under `allow-scripts`
   alone — top-nav (and `window.location`) is blocked. The documented full-doc
   cross-link path silently fails. Fix: **always inject the bridge** (drop the
   two-mode split), or add `allow-top-navigation-by-user-activation` (safe:
   requires a user gesture) as the no-bridge fallback.

6. **Kill the `glasspad:<slug>` scheme.** It breaks static-file portability
   (contradicts "directory is the portable source of truth"), copy-link,
   open-in-new-tab, a11y, PDF export. Use ordinary relative links
   (`./sales` / `./sales.html`) intercepted by the bridge.

7. **Slug derivation is unsafe.** Stripping numeric prefixes collides
   (`02-sales.html` + `sales.html` → both `sales`); `_gp` and other internal
   paths aren't reserved; charset/case/Unicode unspecified. Reject ambiguity,
   don't silently pick. Strong recommendation: **drop the numeric-prefix magic**
   — slug = filename stem, ordering comes from `glasspad.yaml` or lexicographic
   fallback.

8. **Assets are unspecified and must be designed before Phase 1.** Real HTML
   needs images/CSS/JS-modules/JSON/fonts/SVG. "Directory of `.html`" doesn't
   define upload, serving, MIME (`nosniff`), URL routing (`/{space}/{slug}` vs
   `/{space}/{file.ext}`), path-traversal/symlink rejection, or size limits.
   This determines routing and CSP, so it's foundational, not follow-up.

9. **Phasing/deletion is unsafe.** Phase 1 renders arbitrary HTML before the
   origin topology, CSP, response headers, snapshots, and adversarial browser
   tests exist; Phase 6 deletes the only safe path before a converter/rollback
   exists. Reorder: security contract + tests first; delete `spec/` /
   `sanitize.rs` / `dashboard.js` only after the new path bakes in a shadow
   deploy.

10. **Missing operational design** (raised repeatedly): push/snapshot atomicity
    + revision/concurrency (ETag/`If-Match`, content-addressing, rollback);
    server- AND browser-tab DoS limits; token details (header not URL, hashed,
    rotatable, read vs write scopes); cloud abuse/phishing/legal (DMCA, CSAM
    reporting) controls; **version-pinned** base libraries (`/_gp/v1/*`);
    live-reload (SSE) semantics + its CSP `connect-src` cost; Vega likely needs
    `unsafe-eval`; theme-sync FOUC race (inline correct theme at wrap time, not
    via bridge); real cross-browser (Chromium/Firefox/WebKit) security tests.

## Disputed — the origin/sandbox model (panel split 2–2)

Is a per-space subdomain meaningful while keeping `sandbox="allow-scripts"`
(no `allow-same-origin`)?

- **Adopt Option A now** (Gemini, Anthropic): drop to
  `sandbox="allow-scripts allow-same-origin"` on a **separate registrable
  content domain** with per-space subdomains. Then the *origin* is the boundary:
  storage/workers work, `'self'`/CORS work, per-space subdomains actually
  isolate spaces, guaranteed process isolation. Anthropic notes Claude Artifacts
  **does** use `allow-same-origin` on a distinct registrable domain — so the
  design's "this is what Claude does" claim is inaccurate for the null-origin
  variant.
- **Keep Option B now** (GPT-5.6, DeepSeek): null-origin sandbox is the stricter
  primary containment; a **single separate registrable content domain** is the
  real deployment/failure-containment boundary; per-space subdomains add little
  *until* you adopt `allow-same-origin`, so defer them. Migrate to A only when a
  concrete need for storage/workers/same-origin data-fetch appears.

**Points of agreement underneath the split:** (a) a separate *registrable*
domain (e.g. `glasspadusercontent.com`), **not** an app subdomain, is essential
either way — an app subdomain is still same-site for cookies. (b) per-space
subdomains only pull weight under Option A. (c) the choice hinges on whether
artifacts need persistent storage / workers / ES modules / same-origin data
fetch.

**Moderator's take:** these compose, but not the way the plan implies. The
opaque sandbox strictly dominates the subdomain boundary *while it's on*, so
Option B is coherent and safer for the current feature set. But the plan's
biggest simplification — "agent inlines its own data" — collides with the
already-flagged asset/data need (see @greatly-jumbled-park and the data-context concern below); the
moment artifacts fetch `./data.json` or use workers, Option B's CORS/null-origin
friction pushes you to A. **Recommendation: build the origin topology for A
(separate registrable domain + per-space subdomain) from day one, but ship with
`allow-scripts`-only (Option B) plus a `CSP: sandbox` response header, and flip
on `allow-same-origin` per-space when storage/data-fetch lands.** That gets B's
safety now and A's headroom without a re-architecture. Either way, correct the
Claude-Artifacts claim in `design.md`.

## Things the reviewers surfaced that the plan omitted

- **Data-context trap** (Gemini): "agent inlines its own data" forces thousands
  of rows into `<script>` tags → context/output-token blowup. This is a real
  argument to keep *data files* (`.json`/`.csv`) as first-class assets in the
  space — which also re-opens whether cutting `glasspad data` from core is right.
- `srcdoc` vs `src` for the iframe (use `src` + a content endpoint → real
  headers, real artifact URLs).
- Trusted Types on the parent chrome; COOP/COEP on the app origin;
  `Permissions-Policy` deny-list on the artifact; CSP `report-to`.
- Per-space hostname **reuse** is dangerous once storage exists (quarantine /
  random immutable space IDs).
- The `/_gp/*` API needs a discoverable manifest/version so agents know
  `gp.chart()`'s signature.

## Solid (keep as-is)

Replacing the rigid content DSL with HTML; trusted nav chrome outside the
artifact frame; local pinned base libraries so `connect-src` can stay closed;
directory-as-authoring-format. Direction is viable — the documents are not yet
implementation-ready.

## Decisions the PO needs to make

1. **Option A vs B** for the sandbox (moderator recommends: A-topology,
   B-runtime, flip later). Blocks `design.md`.
2. **Data/assets in the space**: first-class data files (reversing the
   `glasspad data` cut), or strict inline-only? Blocks the asset model and @greatly-jumbled-park.
3. **Update model**: is `push` (whole-space snapshot) the only mutation, or do
   incremental `artifact update` commands stay? Two mutation paths + "directory
   is source of truth" risks drift between the repo and the server's edited copy.
4. **Per-space subdomains for team tier**: in-scope from the start, or ship team
   as single-content-origin with honestly-documented "no cross-space isolation"?
