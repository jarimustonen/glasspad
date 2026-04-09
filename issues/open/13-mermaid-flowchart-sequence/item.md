---
created: 2026-04-09
updated: 2026-04-09
type: feature
reporter: jari
assignee: jari
status: open
priority: normal
---

# 13. Mermaid flowchart and sequence diagram

_Source: pad content types_
_Epic: **#12** Diagrams and charts_

## Description

Add a new `diagram` section type to Glasspad that renders Mermaid diagrams client-side. Start with the two most commonly used types: flowcharts and sequence diagrams.

## Scope

### Flowchart support
- All directions: TB (top-bottom), LR (left-right), TD (top-down), RL
- Subgraphs for grouping components
- Node shapes (rectangles, rounded, diamonds, etc.)
- Link styles (arrows, dotted, thick)
- HTML labels in nodes (bold, line breaks)

### Sequence diagram support
- Participants and actors
- Synchronous and asynchronous messages (`->>` and `-->>`)
- Notes (left, right, over)
- Activation/deactivation bars
- Loops, alt, opt blocks

### Rendering
- Mermaid.js loaded via CDN (same pattern as Vega-Lite for charts)
- Diagram source text passed via section config
- Theme-aware rendering (light/dark)

## Based on real usage

From OpenClaw docs analysis:
- Flowchart TB: system topology diagrams
- Flowchart LR: component architecture
- Flowchart TD: troubleshooting decision trees
- Flowchart + subgraph: SSH tunnel architecture with grouped components
- Sequence diagram: WebSocket protocol message flows

## Acceptance Criteria

- [ ] New `diagram` section type in backend (schema + validation)
- [ ] `mountDiagram()` in dashboard.js loading Mermaid.js via CDN
- [ ] Flowchart renders correctly in all directions (TB, LR, TD)
- [ ] Subgraphs render correctly
- [ ] Sequence diagram renders with participants, messages, and notes
- [ ] Clean styling that fits Glasspad's look
