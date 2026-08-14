# Diagrams in markdown/prose spaces (markdown-diagrams)

**The supported pattern is inline SVG, themed from the `--gp-*` design system.**
glasspad does not ship a diagram DSL or a server-side diagram renderer — the
*producing agent* owns diagram generation (e.g. a data-driven SVG generator like
aggountant's `diagrams.py`) and embeds the result as inline `<svg>` in a markdown
body or an HTML artifact. glasspad's only contribution is a small set of
theme-aware CSS classes so an authored diagram reads correctly in both Glass Light
and Glass Dark.

## Security — the sandbox is the boundary, not the format

An authored SVG is **untrusted artifact content, exactly like raw HTML**. glasspad
does **not** sanitize it. Inline SVG is *not* "just markup": it can carry
`<script>`, event handlers (`onload=`), `<foreignObject>` (arbitrary HTML),
`<a href>`/`<a href="javascript:…">`, and URL-bearing elements (`<image href>`,
`<use href>`, `<style>` with `@import`/`url(...)`). The `.md`/template render path
passes all of that through verbatim (`render::render_markdown`), and the artifact
`script-src` includes `'unsafe-inline' 'unsafe-eval'`, so any script inside an
authored SVG **will execute**.

That is safe **because of the existing artifact boundary, not because of the SVG
format** (`headers::artifact_csp_from_origins`):

- `sandbox allow-scripts allow-top-navigation-by-user-activation` — **no
  `allow-same-origin`**, so the document is null-origin: it cannot read the parent
  shell, app storage, or same-origin responses. Script executes but confined.
- `connect-src 'none'` — the automated-exfil boundary: no `fetch`/XHR/WebSocket/
  `sendBeacon`/`EventSource`, including to self.
- `img-src <loopback-host> data:`, `default-src 'none'` — external resource loads
  are blocked; only the named loopback host + `data:` images resolve.

What is **not** fully closed (unchanged by this feature — true for all artifact
HTML): a script may still issue requests to the *named loopback host* (a same-host
side channel), and `allow-top-navigation-by-user-activation` permits a
**user-gesture-gated** top-level navigation (`top.location = …`) that can carry
data in the URL. So "no *automated, external* egress", not literally "no network".
This feature adds **no new authority** — inline SVG receives exactly what hostile
artifact HTML already had. **Do not** treat "it only contains SVG" as a reason to
relax the sandbox anywhere.

Producers that want a genuinely static diagram should emit a **script-free,
self-contained SVG** (shapes/paths/text + the classes below, no `<script>`, no
external `href`).

## The classes (`base.css`)

| Class | On | Effect |
|-------|----|--------|
| `.gp-diagram` | `<figure>` wrapper | centers; SVG scales to fit the column by default (never a body scroll). Set `--gp-diagram-min-width` on the `<svg>` to hold a minimum width and scroll inside the figure instead. |
| `.gp-status-done` / `.gp-status-next` / `.gp-status-blocked` / `.gp-status-future` | node/edge/chip (shape, `<g>`, or `<li>`) | sets `--gp-status` (solid stroke), `--gp-status-bg` (soft fill), `--gp-status-fg` (label/chip text) from the palette |
| `.gp-node` | the SVG **shape** (`<rect>`/`<circle>`/`<path>`) | soft status fill + solid status stroke (neutral chrome when no status class). Put it on the shape, **not** the wrapping `<g>`, so the paint does not inherit onto the label. |
| `.gp-node-label` | SVG `<text>` | reading foreground colour + sans font; `stroke: none` keeps glyphs un-outlined |
| `.gp-edge` | SVG `<path>`/`<line>` | solid status stroke, never filled |
| `.gp-edge-arrow` | the arrowhead `<path>` inside a `<marker>` | solid status fill — see the marker caveat below |
| `.gp-legend` / `.gp-chip` | `<ul>`/`<li>` | an HTML colour key; each chip shows its status swatch + `--gp-status-fg` text |

Node labels use the neutral reading foreground (`--gp-text`), not `--gp-status-fg`,
so they stay legible on the soft fill; `--gp-status-fg` is the per-status text
colour used by the legend chips. The status palette tokens (`--gp-status-done`,
`--gp-status-done-soft`, `--gp-status-done-text`, and the `next`/`blocked`/`future`
triples) are defined in all three theme blocks in `base.css`.

## Canonical shape — a colour-coded status DAG

```html
<figure class="gp-diagram">
  <svg viewBox="0 0 300 80" role="img"
       aria-label="Status DAG. Ship: done. Docs: next.">
    <path class="gp-edge" d="M70 40 H130"/>
    <g class="gp-status-done">
      <rect class="gp-node" x="10" y="20" width="60" height="40" rx="6"/>
      <text class="gp-node-label" x="40" y="44" text-anchor="middle">Ship</text>
    </g>
    <g class="gp-status-next">
      <rect class="gp-node" x="130" y="20" width="60" height="40" rx="6"/>
      <text class="gp-node-label" x="160" y="44" text-anchor="middle">Docs</text>
    </g>
  </svg>
  <figcaption>Implementation DAG</figcaption>
</figure>
```

Drop that straight into a `.md` page (or an HTML artifact). A runnable example is
`examples/status-dag/` — `glasspad loopback serve ./examples/status-dag`
(or `glasspad build ./examples/status-dag <out>` for static output).

## Gotchas for data-driven producers

- **No blank lines inside the embedded HTML.** In a `.md` body the diagram is a
  CommonMark *HTML block*, which ends at the **first blank line** — content after
  that blank line is parsed as markdown (and indented SVG lines become an escaped
  `<pre><code>` block, breaking the diagram). Emit the whole `<figure>…</figure>`
  contiguously, with no blank line between elements. (Full HTML artifacts and the
  built-in templates are not affected — only inline HTML *inside markdown*.)
- **Coloured arrowheads need a per-status marker.** A `<marker>` in `<defs>` does
  **not** inherit `--gp-status` from the edge that references it via
  `marker-end="url(#…)"` — the marker content is cloned from the `<marker>`
  ancestor, not the referencing `<path>`. An arrowhead with `.gp-edge-arrow` alone
  renders in the neutral fallback. For a status-coloured arrowhead, emit one marker
  per status and put the status class on it: `<marker id="arrow-done" class="gp-status-done">`.
- **Unique SVG ids per diagram.** Inline SVG ids share the *document's* id
  namespace. A page with multiple diagrams must use diagram-unique id prefixes
  (`id="proj-arrow"`, not `id="arrow"`) or `url(#…)` references resolve to the
  wrong element.

## Accessibility & responsiveness

- Give the `<svg>` `role="img"` + a descriptive `aria-label` that names each node
  **and its status** (the visual encoding is not readable otherwise), and keep it in
  sync with the rendered nodes.
- **Do not rely on colour alone.** done/next/blocked/future are distinguished only
  by hue, so put the status in text too — either in each node's label
  (`Deploy — blocked`) or an adjacent status list — not only in a colour legend.
- Use a `viewBox` (not a fixed `width`/`height`) so `.gp-diagram svg` can scale to
  the reading column.

## Security note (implementation)

This feature is **CSS + docs only**. It adds no route, no header, no script, and
does not change `render.rs`'s output for any existing input — a diagram renders
through the pre-existing raw-HTML passthrough. The artifact CSP/sandbox is
unchanged. Regression coverage: `render.rs`'s
`inline_svg_status_dag_passes_through_prose_render` (the SVG, incl. a `<script>`,
survives the render path un-sanitized) and `mod.rs`'s
`diagram_artifact_serves_under_the_frozen_csp` (the served diagram artifact carries
the identical frozen CSP header as any other artifact — null-origin, no
`allow-same-origin`, `connect-src 'none'`).
