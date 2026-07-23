---
created: 2026-04-11
updated: 2026-07-23
type: feature
reporter: jari
assignee: jari
status: obsolete
priority: normal
slug: pivot-expand-collapse
closed: 2026-07-23
---

# Pivot expand/collapse row hierarchies

_Source: pad content types_
_Epic: **@pivot-table-view** Pivot tables_

## Description

Add interactive expand/collapse for multi-level row hierarchies in pivot tables. When rows have multiple fields (e.g. region → product), each parent level can be collapsed to show only the subtotal, or expanded to show child rows.

This is standard in desktop tools (Excel, Power BI, Tableau) but often missing in web-based pivot tables — an opportunity to differentiate.

## Behavior

- **Collapsed state**: shows parent row with subtotal values only
- **Expanded state**: shows parent row + all child rows + subtotal
- **Toggle**: click +/− icon on the row header
- **Expand/collapse all**: button to toggle all levels at once
- **Default state**: configurable (expanded or collapsed)

## Configuration

```yaml
pivot:
  rows:
    - region
    - product
  row_hierarchy:
    default: expanded    # or "collapsed"
```

## Acceptance Criteria

- [ ] +/− icons on hierarchical row headers
- [ ] Clicking toggles between collapsed (subtotal only) and expanded (children visible)
- [ ] Expand/collapse all button works
- [ ] Configurable default state
- [ ] Smooth CSS transition for expand/collapse
