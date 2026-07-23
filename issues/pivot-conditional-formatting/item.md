---
created: 2026-04-11
updated: 2026-04-11
type: feature
reporter: jari
assignee: jari
status: open
priority: normal
slug: moderately-six-snow
---

# Pivot conditional formatting

_Source: pad content types_
_Epic: **@pivot-table-view** Pivot tables_

## Description

Add conditional formatting to pivot table cells to improve readability. Color scales, thresholds, and data bars make patterns in aggregated data immediately visible. Identified as a key differentiator in the competitor analysis (@pivot-competitor-analysis).

## Formatting Types

1. **Color scale** — gradient from low to high values (e.g. red → yellow → green)
2. **Threshold rules** — specific colors for value ranges (e.g. red if < 0, green if > target)
3. **Data bars** — horizontal bar within the cell proportional to value
4. **Icon sets** — arrows, traffic lights, stars based on value

## Configuration

```yaml
values:
  - field: revenue
    aggregate: sum
    formatting:
      type: color_scale
      min_color: "#ff4444"
      max_color: "#44bb44"
  - field: margin
    aggregate: avg
    formatting:
      type: threshold
      rules:
        - condition: "< 0.1"
          color: "#ff4444"
        - condition: ">= 0.1"
          color: "#44bb44"
```

## Acceptance Criteria

- [ ] Color scale renders gradient across cell values
- [ ] Threshold rules apply correct colors
- [ ] Data bars render proportionally
- [ ] Formatting config in pivot YAML spec
- [ ] Works with both raw values and "Show Values As" calculations (@pivot-show-values-as)
