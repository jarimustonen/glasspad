---
created: 2026-04-09
updated: 2026-04-09
type: epic
owner: jari
status: open
priority: normal
---

# E12. Diagrams and charts

## Goal

Comprehensive support for rendering technical diagrams and charts in Glasspad. Covers flowcharts, sequence diagrams, UML, architecture diagrams, and other visual formats that AI agents and developers commonly produce.

## Issues

- **#09** Technical chart types (open) — inventory and scope of all chart types
- **#10** UML diagram support (open) — UML-specific diagrams
- **#13** Mermaid flowchart and sequence diagram (open) — first implementation

## Phases

### Phase 1: Core Mermaid support
- [ ] Flowchart rendering (TB, LR, TD + subgraphs) (#13)
- [ ] Sequence diagram rendering (#13)
- [ ] Client-side Mermaid.js via CDN

### Phase 2: Extended Mermaid types
- [ ] Class diagrams
- [ ] State diagrams
- [ ] ER diagrams
- [ ] Gantt charts

### Phase 3: Advanced diagrams
- [ ] Mind maps, timeline, kanban (Mermaid)
- [ ] Alternative renderers (D2, Graphviz) if needed
- [ ] Pan/zoom for large diagrams
- [ ] SVG/PNG export

## Notes

Research from OpenClaw docs shows flowcharts (multiple directions + subgraphs) and sequence diagrams are the most commonly used types in real documentation. Starting there gives immediate practical value.
