---
created: 2026-04-11
updated: 2026-04-11
type: enhancement
reporter: jari
assignee: jari
status: open
priority: low
---

# 26. Bound memory usage for pivot distinct aggregation

_Source: pivot table aggregation_
_Epic: **#17** Pivot table view_

## Description

The `distinct` aggregate stores a full set of unique values per cell, then copies/merges these sets for subtotals and grand totals. For high-cardinality fields (e.g. 50k unique user IDs) across many pivot cells, memory usage can grow rapidly.

## Scope

- Document that `distinct` on large/high-cardinality datasets is expensive
- Consider hard caps on distinct set size with warnings
- Consider approximate distinct (HyperLogLog) for scale
- Consider lazy total computation to avoid redundant set merging
- Add configurable cardinality limit with error/warning when exceeded

## References

- Review feedback from LLM review rounds 2 and 3
