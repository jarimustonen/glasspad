# Plan — Glasspad v2: HTML-artifact host

## 1. Motivation

The core problem is not YAML-as-a-format — it is that **content is encoded in a
rigid section-DSL**. Concretely, the weight lives in:

- `src/spec/schema.rs` (810 lines) + `src/spec/validate.rs` (2043 lines) — the
  section grammar (chart/table/stats/list/markdown/pivot, Vega-Lite encodings,
  dataset references) and its validator.
- `src/client/dashboard.js` (3062 lines) — the client renderer that turns that
  spec into DOM.

That is ~6000 lines whose job is "describe a dashboard structurally instead of
in HTML". The rewrite deletes this and lets the agent write HTML.

**Reused as-is**: axum server, in-memory/store abstraction, token-based update
auth, `ensure_server` auto-spawn, `glasspad open`, skill install, the design
system (`DESIGN.md` + `--gp-*` tokens), the Vega-Lite choice, CSP infra.

## 2. Concept

Glasspad becomes a **host for agent-authored HTML artifacts**, rendered safely
inside a sandboxed iframe, grouped into **spaces** with navigation and
cross-links. The only structured config left is a tiny, optional manifest —
all *content* is HTML.

## 3. Model

- **Space** — a set of artifacts sharing a URL namespace and a nav. (Renames
  the current "pad".)
- **Artifact** — one HTML view within a space, addressed by a **slug**
  (e.g. `home`, `sales`, `detail`). The agent assigns slugs so it can link to
  them at authoring time.

URL structure:

```
/{space}/                    → space entry (home artifact + nav chrome)
/{space}/{artifact-slug}     → a specific artifact
```

## 4. Authoring: content is HTML, config is minimal

Two authoring levels, smooth ramp:

**Fragment level (default, easy path).** The agent writes only body content:

```html
<h1>Sales Q3</h1>
<p>Revenue up 12%.</p>
<div id="chart"></div>
<script>gp.chart('#chart', { /* vega-lite spec */ })</script>
```

Glasspad wraps this in a skeleton: `<!doctype>`, CSS reset, design tokens,
theme toggle, and opt-in base libraries come for free.

**Full-document level (full control).** If the payload starts with `<!doctype`
or `<html>`, Glasspad serves it verbatim. The agent owns everything.

Detection is a trivial prefix check.

## 5. Directory = space (persistence + portability in one)

A space is a directory of `.html` files plus an optional `glasspad.yaml`. The
**on-disk format is the wire format** — no separate serialization.

```
myspace/
  glasspad.yaml        # OPTIONAL: title, theme, nav order/grouping
  index.html           # home
  02-sales.html
  03-detail.html
```

Conventions cover the common case (manifest is usually unnecessary):

- nav order = filename sort (numeric prefixes like `02-` are stripped from slug)
- artifact title = `<title>` or first `<h1>`
- home = `index.html` / `home.html` / first artifact
- slug = filename without extension and numeric prefix

`glasspad.yaml` only overrides these (grouping, icons, nesting, explicit order).

## 6. CLI contract

Convention over configuration, three-step ramp:

**Trivial (one artifact):**
```bash
glasspad create ./report.html      # slug = filename, no manifest
```

**Common (directory of HTML, default path):**
```bash
glasspad serve ./myspace           # localhost: serve directory LIVE (re-read per request)
glasspad push  ./myspace           # team/cloud: upload snapshot → stable URL + token
```

**Incremental (long-lived spaces, editing):**
```bash
glasspad artifact add    {space} detail ./detail.html --token …
glasspad artifact update {space} sales  ./sales.html  --token …
glasspad artifact rm     {space} detail                --token …
```

`glasspad.yaml` is the only YAML left, and it is *structure* (title/theme/nav
order), never *content*. Usually ~5 lines or absent.

## 7. Base libraries ("sensible base structures")

Served locally under `/_gp/*` so the iframe CSP can allow `self` for these but
block arbitrary egress → interactive AND safe. All opt-in except the bridge:

- **`/_gp/base.css`** — the existing design system (`--gp-*` tokens, typography,
  light/dark, theme toggle). Auto-included by the fragment wrapper. Preserves
  all of `DESIGN.md`.
- **`/_gp/charts.js`** — a thin `gp.chart(el, spec)` helper over Vega-Lite. Same
  easy charting as today, but embedded in the agent's own HTML.
- **`/_gp/bridge.js`** — a tiny script (the only auto-injected one) that resolves
  `glasspad:<slug>` links and syncs the theme into the iframe.

## 8. Navigation and cross-links

- Nav chrome is rendered in the **trusted parent frame** from the space's
  artifact list (+ optional `glasspad.yaml` overrides).
- Cross-links inside an artifact use `<a href="glasspad:detail">`. Because the
  artifact runs in a sandboxed iframe (null origin), `bridge.js` intercepts the
  click and asks the parent to swap the iframe / navigate.
- Full-document artifacts (no injected bridge) use `target="_top"` +
  `/{space}/{slug}`.

## 9. Deployment modes

| Mode | Origin isolation | Persistence | Auth |
|---|---|---|---|
| localhost | sandbox iframe | none (serve from directory) | none |
| team server | separate content origin / subdomain | disk | token → later accounts |
| glasspad.ai | per-space subdomain | database | accounts |

See `design.md` for the security rationale (why sandbox + separate origin).

## 10. Phased implementation

1. **Iframe host + content origin** — render arbitrary HTML into a sandboxed
   iframe with an egress-restricting CSP; an origin-isolation abstraction that
   degrades to sandbox-only on localhost.
2. **Space/Artifact model + directory format** — `serve ./dir` live, slug
   addressing, conventions (nav order, home, titles).
3. **CLI contract** — `create` / `serve` / `push` / `artifact add|update|rm`;
   ramp: one file → directory → incremental.
4. **Base libraries** — `/_gp/base.css`, `/_gp/charts.js`, `/_gp/bridge.js`
   (fragment wrapper + `glasspad:` links + theme sync).
5. **Nav + cross-links** — parent-frame chrome, bridge script.
6. **Removals + migration** — drop `spec/`, the section `dashboard.js`; move the
   data parsers to a `glasspad data` helper; update skill.md and docs.

Accounts/auth for team & cloud is a separate follow-up; the token model
generalizes into it.

## 11. What gets deleted

- `src/spec/schema.rs` + `src/spec/validate.rs` — the section DSL and validator.
- `src/client/dashboard.js` — the section renderer.
- `src/security/sanitize.rs` as the *primary* mechanism (iframe sandbox replaces
  it; sanitization may remain an optional mode).
- `src/data/*` from core (moved to an optional `glasspad data` helper).

Net: ~6000 lines of the most complex code replaced by a small host + iframe
sandbox + thin optional helpers — a genuinely lighter system.
