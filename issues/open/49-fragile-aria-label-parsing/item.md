---
created: 2026-04-11
updated: 2026-04-11
type: improvement
reporter: ai-review
assignee: jari
status: open
priority: normal
---

# 49. Replace aria-label parsing with stable data access

_Source: `src/client/dashboard.js`_

## Description

The dashboard extracts chart data values by parsing Vega-Lite's SVG `aria-label` attributes using regex (`extractFieldFromLabel()`). This couples application logic to an accessibility presentation layer that can change between Vega-Lite versions without notice.

### Affected functions

- `extractFieldFromLabel()` — regex-based label parser
- `binIndexFromBar()` — recovers bin index from bar label text
- `dimBarsOutsideRange()` — dims bars by scraping labels and comparing values
- `renderChartWithSelection()` — restyles marks after entering edit mode

### Why it's fragile

- Vega-Lite's aria-label format is not a stable API
- Localization, formatting changes, or mark structure changes break parsing
- Layered marks or custom formatting can produce ambiguous labels
- Using accessibility text as application state is an anti-pattern

## Scope

Replace DOM scraping with data-driven approaches:

- Use Vega event APIs (`view.addEventListener`) and `item.datum` for interaction capture
- Drive visual state (opacity, selection highlighting) through Vega-Lite encodings or signal-based conditions rather than post-render DOM mutation
- Use stable data identifiers from the data model instead of parsed label strings

## Found by

Gemini and Codex (consensus) during #27 code review.
