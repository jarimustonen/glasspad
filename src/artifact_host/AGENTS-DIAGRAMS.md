# Diagrams in markdown/prose spaces (markdown-diagrams)

**The supported pattern is inline SVG, themed from the `--gp-*` design system.**
glasspad does not ship a diagram DSL or a server-side diagram renderer — the
*producing agent* owns diagram generation (e.g. a data-driven SVG generator like
aggountant's `diagrams.py`) and embeds the result as inline `<svg>` in a markdown
body or an HTML artifact. glasspad's only contribution is a small set of
theme-aware CSS classes so an authored diagram reads correctly in both Glass Light
and Glass Dark.

## Why this is the pattern (not mermaid)

The `.md`/template render path passes **raw HTML/SVG through verbatim** (see
`render::render_markdown`), and the artifact is served inside the **null-origin
sandbox** with the frozen artifact CSP (`headers::artifact_csp_from_origins`):
`sandbox allow-scripts` (no `allow-same-origin`), `connect-src 'none'`, `img-src`
naming only the loopback host + `data:`. An inline SVG is pure markup — no script,
no network — so it displays under that CSP with **no header change at all**. A
bundled diagram library (mermaid, etc.) would add a large runtime, a new
`eval`/parse surface, and nothing the producer needs (it already renders its own
SVG). So the boundary stays exactly as it was: **null-origin, egress closed.**

## The classes (`base.css`)

| Class | On | Effect |
|-------|----|--------|
| `.gp-diagram` | `<figure>` wrapper | centers, wide diagrams scroll inside the figure (never a body scroll); `<svg>` child capped to `max-width:100%` |
| `.gp-status-done` / `.gp-status-next` / `.gp-status-blocked` / `.gp-status-future` | any node/edge/chip | sets `--gp-status` (solid), `--gp-status-bg` (soft fill), `--gp-status-fg` (label) from the palette |
| `.gp-node` | SVG `<rect>`/`<g>` | soft status fill + solid status stroke (neutral chrome when no status class) |
| `.gp-node-label` | SVG `<text>` | reading foreground colour + sans font, legible on the soft fill in either theme |
| `.gp-edge` | SVG `<path>`/`<line>` | solid status stroke, never filled |
| `.gp-edge-arrow` | SVG arrowhead `<path>` | solid status fill (for `<marker>` arrowheads) |
| `.gp-legend` / `.gp-chip` | `<ul>`/`<li>` | an HTML colour key; each chip shows its status swatch |

The status palette tokens (`--gp-status-done`, `--gp-status-done-soft`,
`--gp-status-done-text`, and the `next`/`blocked`/`future` triples) are defined in
all three theme blocks in `base.css`, so they follow the active theme.

## Canonical shape — a colour-coded status DAG

```html
<figure class="gp-diagram">
  <svg viewBox="0 0 300 80" role="img" aria-label="Status DAG: Ship done, Docs next">
    <path class="gp-edge" d="M70 40 H130"/>
    <g class="gp-node gp-status-done">
      <rect x="10" y="20" width="60" height="40" rx="6"/>
      <text class="gp-node-label" x="40" y="44" text-anchor="middle">Ship</text>
    </g>
    <g class="gp-node gp-status-next">
      <rect x="130" y="20" width="60" height="40" rx="6"/>
      <text class="gp-node-label" x="160" y="44" text-anchor="middle">Docs</text>
    </g>
  </svg>
  <figcaption>Implementation DAG</figcaption>
</figure>
```

Drop that straight into a `.md` page (or an HTML artifact). A runnable example is
`examples/status-dag/` — `glasspad serve ./examples/status-dag`.

## Accessibility & responsiveness

- Give the `<svg>` `role="img"` + a descriptive `aria-label` (the visual encoding
  is not readable otherwise).
- Do **not** rely on colour alone — pair each status with a text label (node text
  and/or the `.gp-legend`), since done/next/blocked/future are only distinguished
  by hue.
- Use a `viewBox` (not fixed `width`/`height`) so `.gp-diagram svg` can scale to
  the reading column; wider diagrams scroll horizontally inside the figure.

## Security note

This feature is **CSS + docs only**. It adds no route, no header, no script, and
does not change `render.rs`'s output for any existing input — a diagram renders
through the pre-existing raw-HTML passthrough. The artifact CSP/sandbox is
unchanged (null-origin, `connect-src 'none'`); the `render.rs` regression tests
`inline_svg_status_dag_passes_through_prose_render` and
`diagram_render_does_not_touch_the_artifact_csp_boundary` pin both halves.
