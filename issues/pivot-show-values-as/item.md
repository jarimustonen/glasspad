---
created: 2026-04-11
updated: 2026-04-11
type: feature
reporter: jari
assignee: jari
status: open
priority: normal
slug: fiercely-small-spiders
---

# Pivot "Show Values As" secondary calculations

_Source: pad content types_
_Epic: **@pivot-table-view** Pivot tables_

## Description

Add secondary calculation layer to pivot table cells. After the primary aggregation (SUM, COUNT, etc.), a secondary calculation transforms the result relative to totals or other cells. This is the feature Excel calls "Show Values As" — it covers the next ~8% of use cases beyond basic aggregation.

## Supported Calculations

Priority order based on competitor analysis (@pivot-competitor-analysis):

1. **% of grand total** — cell value as percentage of the overall total
2. **% of column total** — cell value as percentage of its column's total
3. **% of row total** — cell value as percentage of its row's total
4. **Running total** — cumulative sum across rows
5. **Difference from** — difference from a specific row/column value
6. **% difference from** — percentage change from a specific row/column value
7. **Rank** — rank within column (largest/smallest)

## Configuration

```yaml
values:
  - field: revenue
    aggregate: sum
    show_as: pct_of_column    # secondary calculation
    label: "Revenue %"
```

## Acceptance Criteria

- [ ] `show_as` option in pivot value config
- [ ] % of grand/column/row total renders correctly
- [ ] Running total across rows works
- [ ] Number formatting reflects the calculation type (% symbol, etc.)
