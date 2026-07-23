---
created: 2026-04-09
updated: 2026-04-09
type: feature
reporter: jari
assignee: jari
status: open
priority: normal
slug: marginally-frequent-airport
---

# Technical chart types

_Source: pad content types_
_Epic: **@diagrams-and-charts** Diagrams and charts_

## Description

Expand Glasspad's charting beyond Vega-Lite data visualizations to cover the full range of technical diagram and chart types that developers and AI agents commonly produce.

## Chart Categories

### Data charts (existing — Vega-Lite)
- Bar, line, area, scatter, pie, heatmap
- Already supported via Vega-Lite section type

### Flow & process diagrams
- **Flowchart** — decision trees, process flows, if/else logic
- **Activity diagram** — workflow steps with branching and merging
- **BPMN** — business process modeling

### Architecture & system diagrams
- **Component diagram** — services, modules, dependencies
- **Deployment diagram** — infrastructure, containers, nodes
- **Network topology** — servers, load balancers, databases
- **C4 model** — context, container, component, code levels

### Sequence & interaction diagrams
- **Sequence diagram** — actor-to-actor message flows, API call traces
- **Communication diagram** — object interactions with numbered messages

### Structure diagrams
- **Class diagram** — OOP structures, relationships
- **ER diagram** — database schema, entity relationships
- **Package diagram** — module/namespace organization

### State & behavior
- **State diagram** — state machines, transitions, guards
- **Timing diagram** — state changes over time

### Planning & project
- **Gantt chart** — project timeline, dependencies
- **Timeline** — chronological events
- **Kanban board** — task status columns

### Hierarchy & relationships
- **Tree / org chart** — hierarchies, org structures
- **Mind map** — brainstorming, concept mapping
- **Sankey diagram** — flow quantities between nodes

## Overlap with @uml-diagrams UML Diagrams

Issue @uml-diagrams covers UML specifically. This issue is broader — it includes UML but also non-UML technical diagrams (flowcharts, C4, ER, Gantt, mind maps, network topology). The two issues share rendering technology (Mermaid covers most of both) but differ in scope.

## Implementation

Mermaid.js covers the majority of these out of the box:
- Flowchart, sequence, class, state, ER, Gantt, pie, mindmap, timeline, C4, sankey, kanban

For types Mermaid doesn't cover well:
- **D2** — better aesthetics for architecture diagrams
- **Graphviz/DOT** — classic graph rendering, good for complex dependency graphs
- **Excalidraw JSON** — hand-drawn style diagrams

## Scope

- [ ] Inventory which chart types Mermaid covers vs. needs other renderers
- [ ] Section type that accepts diagram source text + type hint
- [ ] Auto-detection of diagram type from content where possible
- [ ] Pan/zoom for large diagrams
- [ ] Light and dark theme support for all diagram types
- [ ] Export to SVG/PNG
