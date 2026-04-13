---
created: 2026-04-11
updated: 2026-04-11
type: enhancement
reporter: jari
assignee: jari
status: open
priority: low
---

# 47. Precompute pivot row/column/grand totals in buildPivotData

_Source: pivot table rendering_
_Epic: **#17** Pivot table view_

## Description

Grand total corner cell and row totals are recomputed by scanning all cells during rendering (O(R*C*V) repeated). Should precompute `rowTotals`, `colTotals`, and `grandTotals` during the single `buildPivotData` pass and pass them to `renderPivotTable`.

Currently:
- Each row total calls `getRowTotal()` which loops all columns
- Each column grand total loops all rows
- Grand total corner loops all rows x all columns
- Subtotals accumulate in a separate pass

## Scope

- Precompute `rowTotals[rk][vi]` during data scan
- Precompute `colTotals[ck][vi]` during data scan
- Precompute `grandTotals[vi]` during data scan
- Pass precomputed totals to `renderPivotTable` and remove inline recomputation loops

## References

- Review feedback from LLM review rounds 2 and 3
