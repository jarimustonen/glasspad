# Project view — implementation DAG

The live "where are we" view: a colour-coded status DAG. Each node is coloured by
status from the `--gp-*` palette, so it reads correctly in both Glass Light and
Glass Dark. **glasspad renders nothing bespoke here** — the producing agent owns
diagram generation and embeds the result as inline `<svg>`; the classes below
(`gp-node`, `gp-edge`, `gp-status-*`) are the only thing glasspad supplies.

<figure class="gp-diagram">
  <svg viewBox="0 0 640 220" role="img" aria-label="Implementation DAG: Design and Schema are done, Render is in progress, Docs is next, Deploy is blocked, Telemetry is future work">
    <defs>
      <marker id="arrow" markerWidth="8" markerHeight="8" refX="7" refY="4" orient="auto">
        <path class="gp-edge-arrow" d="M0 0 L8 4 L0 8 z"/>
      </marker>
    </defs>

    <!-- edges -->
    <path class="gp-edge" d="M150 60 H210" marker-end="url(#arrow)"/>
    <path class="gp-edge" d="M150 160 H210" marker-end="url(#arrow)"/>
    <path class="gp-edge" d="M330 60 H390" marker-end="url(#arrow)"/>
    <path class="gp-edge" d="M330 160 H390" marker-end="url(#arrow)"/>
    <path class="gp-edge" d="M510 60 H540 V110 H570" marker-end="url(#arrow)"/>
    <path class="gp-edge" d="M510 160 H540 V110 H570" marker-end="url(#arrow)"/>

    <!-- nodes: coloured by status -->
    <g class="gp-node gp-status-done">
      <rect x="30" y="40" width="120" height="40" rx="8"/>
      <text class="gp-node-label" x="90" y="65" text-anchor="middle">Design</text>
    </g>
    <g class="gp-node gp-status-done">
      <rect x="30" y="140" width="120" height="40" rx="8"/>
      <text class="gp-node-label" x="90" y="165" text-anchor="middle">Schema</text>
    </g>
    <g class="gp-node gp-status-next">
      <rect x="210" y="40" width="120" height="40" rx="8"/>
      <text class="gp-node-label" x="270" y="65" text-anchor="middle">Render</text>
    </g>
    <g class="gp-node gp-status-next">
      <rect x="210" y="140" width="120" height="40" rx="8"/>
      <text class="gp-node-label" x="270" y="165" text-anchor="middle">Docs</text>
    </g>
    <g class="gp-node gp-status-blocked">
      <rect x="390" y="40" width="120" height="40" rx="8"/>
      <text class="gp-node-label" x="450" y="65" text-anchor="middle">Deploy</text>
    </g>
    <g class="gp-node gp-status-blocked">
      <rect x="390" y="140" width="120" height="40" rx="8"/>
      <text class="gp-node-label" x="450" y="165" text-anchor="middle">Migrate</text>
    </g>
    <g class="gp-node gp-status-future">
      <rect x="570" y="90" width="60" height="40" rx="8"/>
      <text class="gp-node-label" x="600" y="115" text-anchor="middle">v2</text>
    </g>
  </svg>
  <ul class="gp-legend">
    <li class="gp-chip gp-status-done">Done</li>
    <li class="gp-chip gp-status-next">Next</li>
    <li class="gp-chip gp-status-blocked">Blocked</li>
    <li class="gp-chip gp-status-future">Future</li>
  </ul>
  <figcaption>Implementation status — updated by the producing agent</figcaption>
</figure>

## How this works

The diagram is plain inline SVG in the markdown body. Because the artifact is
served inside glasspad's null-origin sandbox, the SVG carries **no script** and
reaches **no network** — it is pure themed markup. See the diagram pattern in
`src/artifact_host/AGENTS-DIAGRAMS.md`.

## Why not a diagram DSL

The producing agent already renders data-driven SVG (aggountant's `diagrams.py`),
so glasspad stays out of the rendering business and only owns the theme tokens.
That keeps the security boundary untouched: no new library, no `eval`, no CSP change.
