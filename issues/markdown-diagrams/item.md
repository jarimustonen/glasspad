---
created: 2026-08-14
updated: 2026-08-14
type: feature
reporter: maintainer
status: done
priority: normal
commits:
- hash: d68d7a0
  summary: themed inline-SVG diagram pattern (base.css tokens/classes + render regression tests + docs + example)
- hash: 580b788
  summary: review fixes — accurate security prose, real HTTP CSP integration test, CSS overflow/label/marker fixes, example HTML-block blank-line bug fixed
closed: 2026-08-14
---

# First-class diagram support in markdown spaces (flow/stack/status-DAG, or documented inline-SVG asset pattern)

_Source: producer-example docs/ port_

## Description

**Motivation:** producer-example's docsite uses a bespoke diagrams.py to render data-driven SVG diagrams inside its markdown pages — process flow diagrams, layered stack diagrams, and a **colour-coded status implementation-DAG** (done/next/blocked/future). glasspad 0.8.0 ships Vega-Lite (gp.chart) for data charts but has no path for these authored structural diagrams. **Ask (either):** (a) a documented, supported pattern for embedding inline SVG / an SVG asset from assets/ into a rendered markdown page (confirm CSP/sandbox lets a themed inline SVG display), so producers keep owning the diagram rendering; and/or (b) native mermaid (or similar) fenced-block rendering in the prose template. The colour-coded status DAG (the project's live 'where are we' view) is the priority case. Tracked in producer-example `project-view`.
