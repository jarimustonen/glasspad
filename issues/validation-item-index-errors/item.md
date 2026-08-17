---
created: 2026-04-11
updated: 2026-07-23
type: improvement
reporter: maintainer
assignee: jari
status: obsolete
priority: normal
slug: validation-item-index-errors
closed: 2026-07-23
---

# Add item index to per-item validation error messages

_Source: spec validation layer_

## Description

Validation errors for collection items (stats items, table columns, pivot values) report generic messages like `"aggregate X requires field"` without identifying which item failed. For example, `stats.items[2].field` would be much more useful than a generic error when multiple items exist.

No section-type validator currently includes indices in error messages — this would be a cross-cutting improvement to the validation layer.

## Files

- `src/spec/validate.rs` — all `validate_*` functions that iterate over collections.
