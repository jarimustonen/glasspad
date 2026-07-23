---
created: 2026-04-10
updated: 2026-04-11
type: epic
owner: jari
status: open
priority: normal
slug: wholly-shiny-cart
---

# Pivot tables

## Goal

Full pivot table / cross-tab support in Glasspad. Data displayed as a two-dimensional aggregation matrix where rows, columns, cell function (aggregation), and filters are configured declaratively via the API.

## Core Concepts

The universal pivot table model (Excel, Power BI, Google Sheets, Tableau):

1. **Rows** — fields on the vertical axis; each unique value becomes a row header
2. **Columns** — fields on the horizontal axis; each unique value becomes a column header
3. **Values** — the data aggregated in cells at row/column intersections
4. **Cell function** (aggregation) — how values are computed: SUM, COUNT, AVG, MIN, MAX, COUNT_DISTINCT
5. **Filters** — restrict which records are included before aggregation

## Issues

- **@pivot-competitor-analysis** Pivot table competitor analysis (done)
- **@pivot-basic-2d** Basic 2D pivot table (open)
- **@pivot-show-values-as** Pivot "Show Values As" secondary calculations (open)
- **@pivot-expand-collapse** Pivot expand/collapse row hierarchies (open)
- **@pivot-conditional-formatting** Pivot conditional formatting (open)

## Phases

### Phase 1: Research & core
- [x] Competitor analysis (@pivot-competitor-analysis)
- [ ] Basic 2D pivot with aggregation (@pivot-basic-2d)

### Phase 2: Enhanced display
- [ ] "Show Values As" secondary calculations (@pivot-show-values-as)
- [ ] Conditional formatting (@pivot-conditional-formatting)
- [ ] Number formatting (currency, percentage, decimals)

### Phase 3: Interactivity
- [ ] Expand/collapse for hierarchical row groupings (@pivot-expand-collapse)
- [ ] Click cell → cross-filter other sections
- [ ] Click cell → drill-down to underlying records

## Comments

See @pivot-competitor-analysis analysis.md for detailed competitor feature comparison.
