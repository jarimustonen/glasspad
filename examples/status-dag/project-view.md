# Project view — implementation DAG

The live "where are we" view: a colour-coded status DAG. Each node is coloured by
status from the `--gp-*` palette, so it reads correctly in both Glass Light and
Glass Dark. **glasspad renders nothing bespoke here** — the producing agent owns
diagram generation and embeds the result as inline `<svg>`; the classes below
(`gp-node`, `gp-edge`, `gp-status-*`) are the only thing glasspad supplies.

Note: the whole `<figure>` below is one block of raw HTML with **no blank lines
inside it** — a blank line ends the HTML block and CommonMark would parse the rest
as an indented code block. Keep authored SVG contiguous.

<figure class="gp-diagram">
<svg viewBox="0 0 640 220" role="img" aria-label="Implementation DAG. Done: Design, Schema. Next: Render, Docs. Blocked: Deploy, Migrate. Future: v2.">
<defs>
<marker id="dag-arrow" markerWidth="8" markerHeight="8" refX="7" refY="4" orient="auto"><path class="gp-edge-arrow" d="M0 0 L8 4 L0 8 z"/></marker>
</defs>
<path class="gp-edge" d="M150 60 H210" marker-end="url(#dag-arrow)"/>
<path class="gp-edge" d="M150 160 H210" marker-end="url(#dag-arrow)"/>
<path class="gp-edge" d="M330 60 H390" marker-end="url(#dag-arrow)"/>
<path class="gp-edge" d="M330 160 H390" marker-end="url(#dag-arrow)"/>
<path class="gp-edge" d="M510 60 H540 V110 H570" marker-end="url(#dag-arrow)"/>
<path class="gp-edge" d="M510 160 H540 V110 H570" marker-end="url(#dag-arrow)"/>
<g class="gp-status-done"><rect class="gp-node" x="30" y="40" width="120" height="40" rx="8"/><text class="gp-node-label" x="90" y="65" text-anchor="middle">Design — done</text></g>
<g class="gp-status-done"><rect class="gp-node" x="30" y="140" width="120" height="40" rx="8"/><text class="gp-node-label" x="90" y="165" text-anchor="middle">Schema — done</text></g>
<g class="gp-status-next"><rect class="gp-node" x="210" y="40" width="120" height="40" rx="8"/><text class="gp-node-label" x="270" y="65" text-anchor="middle">Render — next</text></g>
<g class="gp-status-next"><rect class="gp-node" x="210" y="140" width="120" height="40" rx="8"/><text class="gp-node-label" x="270" y="165" text-anchor="middle">Docs — next</text></g>
<g class="gp-status-blocked"><rect class="gp-node" x="390" y="40" width="120" height="40" rx="8"/><text class="gp-node-label" x="450" y="65" text-anchor="middle">Deploy — blocked</text></g>
<g class="gp-status-blocked"><rect class="gp-node" x="390" y="140" width="120" height="40" rx="8"/><text class="gp-node-label" x="450" y="165" text-anchor="middle">Migrate — blocked</text></g>
<g class="gp-status-future"><rect class="gp-node" x="570" y="90" width="60" height="40" rx="8"/><text class="gp-node-label" x="600" y="115" text-anchor="middle">v2</text></g>
</svg>
<ul class="gp-legend"><li class="gp-chip gp-status-done">Done</li><li class="gp-chip gp-status-next">Next</li><li class="gp-chip gp-status-blocked">Blocked</li><li class="gp-chip gp-status-future">Future</li></ul>
<figcaption>Implementation status — updated by the producing agent</figcaption>
</figure>

Each node names its own status in text (`Deploy — blocked`), so the DAG does not
rely on colour alone, and the `<svg aria-label>` restates the whole status list for
assistive tech.

## How this works

The diagram is plain inline SVG in the markdown body. glasspad does not sanitize
it — it is untrusted artifact content like any other markup, and it is safe only
because it renders inside glasspad's **null-origin sandbox** with `connect-src 'none'`
and no `allow-same-origin`. The status colours come from `base.css`. See the full
pattern in `src/artifact_host/AGENTS-DIAGRAMS.md`.

## Why not a diagram DSL

The producing agent already renders data-driven SVG (producer-example's `diagrams.py`),
so glasspad stays out of the rendering business and only owns the theme tokens.
That keeps the security boundary untouched: no new library, no new runtime/eval
surface, no CSP change.
