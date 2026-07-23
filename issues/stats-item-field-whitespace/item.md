---
created: 2026-04-11
updated: 2026-07-23
type: bug
reporter: jari
assignee: jari
status: obsolete
priority: normal
slug: stats-item-field-whitespace
closed: 2026-07-23
---

# Validate whitespace/empty strings in stats item fields

_Source: `src/spec/validate.rs` — `validate_stats()` around line 278_

## Description

`validate_stats()` checks `item.field.is_none()` for non-count aggregates but accepts `Some("")` or `Some("   ")`. These are semantically invalid — an empty field name will fail downstream.

The pivot validator already trims field names and rejects whitespace. Stats should do the same for `item.field` and potentially `item.label`.

## Reproduction

Submit a stats spec with an empty or whitespace-only field:

```json
{
  "stats": {
    "items": [
      { "aggregate": "sum", "field": "", "label": "Total" },
      { "aggregate": "avg", "field": "   ", "label": "Average" }
    ]
  }
}
```

Both should be rejected but currently pass validation.

## Fix

In `validate_stats()`, treat `Some("")` and `Some("   ")` the same as `None` for field validation. Apply similar trimming to `item.label` for consistency with pivot validation patterns.
