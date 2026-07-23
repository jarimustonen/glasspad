# Plan — Glasspad v2: localhost HTML-artifact host

> Scope locked to **localhost-only** after the multi-model review. Team/cloud
> tiers are dropped (see `item.md`). Decisions D1–D3 are settled; `design.md`
> holds the security model.

## 1. Motivation

The weight in v1 is not YAML-as-a-format — it is that **content is encoded in a
rigid section-DSL**:

- `src/spec/schema.rs` (810) + `src/spec/validate.rs` (2043) — the section
  grammar and validator.
- `src/client/dashboard.js` (3062) — the client renderer.

~6000 lines whose job is "describe a dashboard structurally instead of in HTML".
The rewrite deletes this and lets the agent write HTML.

**Reused as-is (pending a coupling audit — see Phase 6):** axum server, token
scheme, `ensure_server` auto-spawn, `glasspad open`, skill install, the design
system (`DESIGN.md` + `--gp-*` tokens), the Vega-Lite choice, CSP infra.

## 2. Concept

Glasspad becomes a **local host for agent-authored HTML artifacts**, rendered
safely inside a sandboxed iframe, grouped into **spaces** with navigation and
cross-links. The only structured config left is a tiny, optional manifest — all
*content* is HTML. There is no server-side store: `glasspad serve ./dir` renders
a directory live from disk.

## 3. Model

- **Space** — a directory of artifacts sharing a URL namespace and a nav.
- **Artifact** — one HTML view within a space, addressed by a **slug** the agent
  assigns (so it can link at authoring time).

URL structure (single local origin, e.g. `http://127.0.0.1:PORT`):

```
/{space}/                    → space entry (home artifact + nav chrome)
/{space}/{artifact-slug}     → the trusted shell hosting that artifact
/{space}/_c/{artifact-slug}  → the raw artifact document (iframe src; carries the sandbox CSP)
/_gp/v1/*                    → pinned base libraries
```

## 4. Authoring: content is HTML

**Fragment level (default).** The agent writes body content; Glasspad wraps it
in a skeleton (`<!doctype>`, reset, design tokens, correct theme inlined at wrap
time, the bridge, opt-in base libs):

```html
<h1>Sales Q3</h1>
<div id="chart"></div>
<script>gp.chart('#chart', { /* vega-lite spec */ })</script>
```

**Full-document level.** If the payload starts with `<!doctype`/`<html>` (after
skipping BOM, whitespace, and comments — not a naive prefix check), it is served
verbatim, and it opts into nav only by including `/_gp/v1/bridge.js` itself.

## 5. A space is a static-site tree (D2)

A space holds HTML **plus first-class assets and data** — not `.html` only:

```
myspace/
  glasspad.yaml        # OPTIONAL: title, theme, explicit nav order/grouping
  index.html           # home
  sales.html
  detail.html
  assets/
    sales.js
    data.json
    logo.svg
```

Rules (all enforced on load):

- **Slug = filename stem, literally** — no numeric-prefix magic. Ordering comes
  from `glasspad.yaml` (`nav: [home, sales, detail]`) or lexicographic fallback.
- Slugs validated against a canonical grammar; **collisions and reserved names
  (`_gp`, `_c`, `assets`, `api`) are hard errors**, never silently resolved.
- Assets served by path with MIME detection + `X-Content-Type-Options: nosniff`;
  symlink / path-traversal rejected; per-file and per-space size limits.
- Home = `index.html` > `home.html` > first in nav order.
- Title = manifest > `<title>` > first `<h1>`, parsed (not regexed), decoded,
  length-bounded, and inserted into the trusted chrome **as text**.

## 6. CLI contract (localhost)

```bash
glasspad serve ./myspace     # render the directory LIVE from disk (primary loop)
glasspad create ./report.html # one-artifact space from a single file
glasspad open <space>        # open in browser
```

No `push`, no `artifact add/update/rm`, no `--token` juggling — the directory on
disk is the single source of truth (D3). The agent edits files; a filesystem
watcher + SSE reload (narrow `connect-src`) refreshes the browser. `serve` builds
an immutable in-memory snapshot per rescan and swaps it atomically, so a
half-written file is never served.

`glasspad.yaml` is the only YAML left, and it is *structure* (title / theme / nav
order / grouping), never *content* — usually absent.

## 7. Base libraries (`/_gp/v1/*`, pinned)

Served locally so the egress CSP can name the host and stay closed. All opt-in
except the bridge:

- **`base.css`** — the existing design system (`--gp-*` tokens, typography,
  light/dark). Auto-included by the fragment wrapper. Preserves `DESIGN.md`.
- **`charts.js`** — a thin `gp.chart(el, spec)` over Vega-Lite. (Vega may need
  `'unsafe-eval'` in the artifact CSP — verified in Phase 1, acceptable inside
  the sandbox.)
- **`bridge.js`** — the only auto-injected script: intercepts same-space relative
  links → `postMessage` the parent to swap the iframe, and applies the theme.
- **`manifest.json`** — so the agent can discover `gp.chart()`'s signature.

Versioned under `/_gp/v1/` from day one so a committed space renders the same
after a Glasspad upgrade.

## 8. Navigation and cross-links

- Nav chrome renders in the **trusted parent frame** from the space's artifacts
  (+ optional `glasspad.yaml`).
- Cross-links are **ordinary relative links** (`<a href="./detail">`). The bridge
  intercepts clicks on same-space links and asks the parent to swap the iframe;
  the raw href still resolves under `file://` and copy-link, preserving
  portability. No custom `glasspad:` scheme.
- Full-document artifacts that skip the bridge fall back to `target="_top"` +
  `/{space}/{slug}`, which requires `allow-top-navigation-by-user-activation` on
  the iframe (user-gesture only).

## 9. Security

Single local origin; isolation via null-origin sandbox + a `CSP: sandbox`
response header + egress CSP + a control/API that never trusts the sandbox. Full
model, threat analysis, and the exact headers: **`design.md`**.

## 10. Phased implementation (reordered per review)

1. **Security contract + iframe shell.** URL topology; artifact-response headers
   (`CSP: sandbox allow-scripts` + egress CSP naming the host + `nosniff` +
   `Permissions-Policy` deny-list); null-origin iframe via `/{space}/_c/{slug}`;
   validated `postMessage` bridge (`event.source` check). Ship with an
   **adversarial browser test** (exfil attempts per channel, sandbox-escape,
   direct-open, postMessage abuse). Verify what Vega needs.
2. **Space model + directory serving.** Snapshot scanner, slug grammar +
   collision/reserved rejection, asset routing + MIME + limits, live rescan +
   SSE reload.
3. **CLI.** `serve` / `create` / `open`; fragment vs full-document detection.
4. **Base libraries.** `base.css`, `charts.js`, `bridge.js`, `manifest.json`,
   pinned under `/_gp/v1/`.
5. **Nav + relative-link cross-navigation** via the bridge.
6. **Removals + migration.** Coupling audit of the "reused" pieces first; then
   drop `spec/`, `dashboard.js`; move data parsers to an optional `glasspad data`
   helper; keep `sanitize.rs` only if an optional static-safe mode wants it.
   Delete the old path only after the new one passes the Phase 1 test suite.

## 11. What gets deleted

- `src/spec/schema.rs` + `src/spec/validate.rs` — the section DSL and validator.
- `src/client/dashboard.js` — the section renderer.
- `src/security/sanitize.rs` as the primary mechanism (sandbox replaces it).
- `src/data/*` from core (moved to an optional `glasspad data` helper).

Net: ~6000 lines of the most complex code replaced by a small host + sandbox +
thin pinned helpers.
