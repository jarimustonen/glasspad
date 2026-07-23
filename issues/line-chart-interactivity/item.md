---
created: 2026-04-11
updated: 2026-04-11
type: improvement
reporter: jari
assignee: jari
status: open
priority: normal
slug: mildly-bizarre-cracker
---

# Line chart interactivity — hover tooltips and zoom

_Source: chart rendering (Vega-Lite client)_

## Description

Line charts need best-in-class interactive features for reading data. Currently the charts render but lack the polish expected from a modern data tool.

## Requirements

### Hover / Mouse-over

- Tooltip showing exact values on hover (field labels + formatted numbers)
- Nearest-point highlighting — snap to closest data point on the line
- Crosshair or vertical rule at hover position for reading across multiple lines
- Highlight the active line, dim others (focus + context)

### Zoom / Magnification

- The zoom/magnifying glass interaction should do something useful:
  - Scroll-to-zoom on the chart area (x-axis, y-axis, or both)
  - Brush selection to zoom into a region
  - Double-click or button to reset zoom
- Panning when zoomed in

## Scope

- Implement via Vega-Lite interaction parameters (selection, tooltips, signals)
- Should work with all line chart datasets (single and multi-series)
- No backend changes needed — purely client-side

## Acceptance Criteria

- [ ] Hovering over a line chart shows a tooltip with exact values
- [ ] Nearest-point snapping highlights the closest data point
- [ ] Crosshair or vertical rule visible on hover
- [ ] Multi-line charts: hovered line highlighted, others dimmed
- [ ] Scroll or brush zoom works on line charts
- [ ] Zoom can be reset (double-click or button)
- [ ] Works with existing test datasets (CO₂ emissions, Gapminder, IoT sensors)
