---
created: 2026-07-22
updated: 2026-07-24
type: feature
reporter: jari
assignee: jari
status: done
priority: high
slug: html-artifact-host-rewrite
commits:
- hash: 01c1498
  summary: Wave 1 security gate — sandbox host + CSP contract + adversarial suite
- hash: 4969c2d
  summary: Wave 1 review fixes — bridge/host-guard hardening, both-origin CSP, guard tests
- hash: dfeb9a0
  summary: Wave 2b — real base.css/charts.js/manifest.json + vendored Vega under /_gp/v1/; fragment chart renders end-to-end in both themes
- hash: f4b523a
  summary: Wave 2a — space model + live directory serving (atomic snapshot, slug/reserved/collision + symlink/traversal rejection, MIME + size limits, SSE reload)
- hash: 5fc6154
  summary: Wave 2a /llm-review FIX findings (CORS asset leak, connect-src back to 'none', capped reads, watcher retry, snapshot-per-request, entry/manifest caps, title parser)
- hash: d1de5d360ec4dd754a49da83c7f4d980ac1a9aed
  summary: Wave 3a deferred /llm-review — CLI-contract + fragment-detector hardening
- hash: '7017010'
  summary: Wave 4 nav chrome + same-space cross-nav (trusted parent), injection probe
- hash: cc3ff85
  summary: Wave 4 /llm-review fixes — injection-probe broadening, shell script-src nonce-only, same-slug no-op, bidi strip, a11y
- hash: 1b21d34a26e844ab3d4384315a49c4f6dd926680
  summary: Wave 5 — remove section-DSL path, add glasspad data helper
- hash: 1992b88de665bf0fa9749eba6c5d226af05c2000
  summary: Wave 5 /llm-review fixes — harden glasspad data contract + tests
closed: 2026-07-24
---

# Rewrite Glasspad as an HTML-artifact host

_Source: whole project — replaces the section-DSL architecture_

## Description

Rewrite Glasspad so the **calling agent authors HTML directly** and Glasspad
just hosts and serves it. Today the agent must encode content (charts, tables,
stats, pivots) into a rigid section-DSL expressed in YAML, validated by
~2000 lines of Rust and rendered by a ~3000-line client renderer. The goal is
to make this a **lightweight way for an agent to show HTML content to a user**,
with no content-DSL at all.

Requirements from the product owner:

- Agent defines content as **HTML**, not a structured YAML content schema.
- Support **multiple artifacts** per unit, with **links between them**.
- Provide **navigation chrome** and some sensible **base structures / base
  libraries** (design tokens, chart helper, link/theme bridge) — opt-in.
- Make it **as easy as possible for the calling agent** (convention over
  configuration; a directory of `.html` files is a valid space).

**v0.2 is localhost-only.** Team/shared-server and glasspad.ai cloud tiers are
explicitly dropped from this scope (see "Out of scope"). That removes accounts,
persistence backends, separate content domains, and per-space subdomains from
the picture and lets v0.2 stay small.

## Key decisions (settled with PO, post-review)

Locked after the multi-model review (`analysis.md`); "go with recommendations,
localhost only":

- **Security model** (D1): arbitrary agent HTML+JS is allowed, rendered in a
  null-origin `<iframe sandbox="allow-scripts">`, **plus** a `Content-Security-
  Policy: sandbox allow-scripts` **response header** on artifacts (so a directly-
  opened artifact URL is still sandboxed), an egress-restricting CSP that names
  the **explicit content host** (not `'self'`, which is meaningless under a null
  origin), loopback-only binding, and a control/API that rejects `Origin: null`
  and requires its own capability token — the sandbox is **not** an API auth
  boundary. Path to `allow-scripts allow-same-origin` kept open for when an
  artifact needs storage/workers/same-origin data-fetch. See `design.md`.
- **Model**: a **Space** = a set of artifacts sharing a URL namespace + nav; an
  **Artifact** = one HTML view addressed by a slug. Cross-links use **ordinary
  relative links** (`./sales`), intercepted by a parent-frame bridge — the
  `glasspad:` custom scheme is dropped (it broke static-file portability).
- **Assets/data** (D2): a space is a static-site tree — HTML **plus** first-class
  assets and data files (`.json`/`.css`/`.js`/images/fonts). This **reverses**
  the earlier "cut data ingestion" call: inlining large data blows up the agent's
  context, so `./data.json` alongside the HTML is first-class.
- **Persistence / update model** (D3): localhost serves the directory **live**
  from disk (`glasspad serve ./dir`, re-read per request via an atomic snapshot).
  The directory literally IS the source of truth — no server-side store, no
  `push`, no incremental mutation API. The agent edits files; the browser
  refreshes.
- Container is named **space** (was "pad").

## Scope

See `plan.md` for the phased implementation plan and `design.md` for the
localhost security model.

## Out of scope (dropped for v0.2)

- **Team shared server and glasspad.ai cloud tiers** — and everything they imply:
  accounts/real auth, DB/persistent storage, separate content origins, per-space
  subdomains, wildcard-DNS + TLS. Revisit as a later epic if sharing is wanted.

## Decisions

### 2026-07-23T06:20:51Z · @jari

Locked v0.2 as localhost-only (dropped team/cloud tiers, accounts, separate content origin, per-space subdomains). Going with review recommendations: D1 null-origin sandbox + CSP:sandbox response header + egress CSP naming the host (allow-same-origin flip kept for later); D2 first-class assets/data (reverses the data-parser cut); D3 serve directory live, no push/no store; relative links replace the glasspad: scheme. plan.md + design.md rewritten accordingly.
