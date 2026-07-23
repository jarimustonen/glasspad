---
created: 2026-04-11
updated: 2026-07-23
type: bug
reporter: claude
assignee: jari
status: obsolete
priority: normal
slug: validate-empty-filter-field
closed: 2026-07-23
---

# No validation for empty interactive_filter.field

_Source: spec validation_

## Description

`validate_chart()` checks that `interactive_filter.field` exists in the chart encoding, but does not validate the field value itself. An empty or whitespace-only field name passes the structural check and produces a misleading "not found in chart encoding" error instead of a clear "field must not be empty" error.

## Reproduction

```yaml
sections:
  - id: c1
    title: "Chart"
    type: chart
    interactive_filter:
      field: "  "
    chart:
      mark: bar
      encoding:
        x: { field: "year" }
```

Produces: `interactive_filter.field "  " not found in chart encoding`
Expected: `interactive_filter.field must not be empty`

## Fix

Add an early check in the interactive_filter validation block:

```rust
if filter.field.trim().is_empty() {
    errors.push(err(Some(label), "interactive_filter.field must not be empty"));
}
```
