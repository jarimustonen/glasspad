---
created: 2026-04-11
updated: 2026-04-11
type: feature
reporter: jari
assignee: jari
status: open
priority: normal
---

# 19. Basic 2D pivot table

_Source: pad content types_
_Epic: **#17** Pivot tables_

## Description

Implement the core pivot table section type: a two-dimensional aggregation matrix with configurable rows, columns, and cell function (aggregation). Based on the universal four-zone model (Rows, Columns, Values, Filters) identified in the competitor analysis (#18).

Follows the declarative spec approach — configuration in YAML, similar to how Vega-Lite defines encodings.

## Configuration

```yaml
sections:
  - title: "Sales by Region and Quarter"
    type: pivot
    source: sales_data
    pivot:
      rows:
        - region
        - product
      columns:
        - quarter
      values:
        - field: revenue
          aggregate: sum
          label: "Revenue"
        - field: orders
          aggregate: count
          label: "Orders"
      show_totals: true
      show_subtotals: true
```

## Scope

- New `pivot` section type in backend (schema + validation)
- Client-side aggregation: group-by rows × pivot columns → apply cell function
- Render as HTML table with proper row/column headers
- Supported aggregations: SUM, COUNT, AVG, MIN, MAX, COUNT_DISTINCT
- Multiple value fields with different aggregations
- Grand totals (row and column)
- Subtotals for multi-level row hierarchies
- Sorting by label or by aggregated value
- Works with cross-filtering from other sections

## Acceptance Criteria

- [ ] `cargo build` succeeds with new pivot section type
- [ ] `cargo test` passes including new tests
- [ ] Pivot renders correct aggregated values for single row + single column field
- [ ] Multi-level row hierarchy with subtotals works
- [ ] Multiple value fields display side by side
- [ ] Grand totals row and column are correct
- [ ] Sorting by value works

## References

- **#18** analysis.md — competitor feature comparison informing design decisions
