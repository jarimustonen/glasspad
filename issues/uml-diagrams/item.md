---
created: 2026-04-09
updated: 2026-07-23
type: feature
reporter: maintainer
assignee: jari
status: obsolete
priority: normal
slug: uml-diagrams
closed: 2026-07-23
---

# UML diagram support

_Source: pad content types_
_Epic: **@diagrams-and-charts** Diagrams and charts_

## Description

Add support for rendering common UML diagram types in Glasspad. AI agents frequently generate diagrams as part of their analysis — Glasspad should render these natively.

## Diagram Types

Priority UML diagrams to support:

1. **Sequence diagrams** — most common in API/service documentation
2. **Class diagrams** — code structure visualization
3. **State diagrams** — workflow and state machine visualization
4. **Activity diagrams** — flowcharts and process flows
5. **Component diagrams** — system architecture
6. **Use case diagrams** — requirements visualization

## Implementation Options

- **Mermaid.js** — widely used, supports all major UML types, markdown-friendly syntax
- **PlantUML** — comprehensive UML support, requires server-side rendering or WASM
- **D2** — modern diagramming language, good aesthetics

Mermaid is the natural first choice given its browser-native rendering and widespread adoption (GitHub, GitLab, Notion all support it).

## Scope

- Section type or embed for diagram content
- Accept diagram source in text format (Mermaid, PlantUML, or similar)
- Client-side rendering
- Pan/zoom for large diagrams
- Export to SVG/PNG
