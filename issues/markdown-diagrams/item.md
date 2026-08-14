---
created: 2026-08-14
updated: 2026-08-14
type: feature
reporter: jari
status: open
priority: normal
---

# First-class diagram support in markdown spaces (flow/stack/status-DAG, or documented inline-SVG asset pattern)

_Source: aggountant docs/ port_

## Description

**Motivation:** aggountant's docsite uses a bespoke diagrams.py to render data-driven SVG diagrams inside its markdown pages — process flow diagrams, layered stack diagrams, and a **colour-coded status implementation-DAG** (done/next/blocked/future). glasspad 0.8.0 ships Vega-Lite (gp.chart) for data charts but has no path for these authored structural diagrams. **Ask (either):** (a) a documented, supported pattern for embedding inline SVG / an SVG asset from assets/ into a rendered markdown page (confirm CSP/sandbox lets a themed inline SVG display), so producers keep owning the diagram rendering; and/or (b) native mermaid (or similar) fenced-block rendering in the prose template. The colour-coded status DAG (the project's live 'where are we' view) is the priority case. Tracked in aggountant `project-view`.
