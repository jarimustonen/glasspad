---
created: 2026-04-11
updated: 2026-04-11
type: bug
reporter: llm-review
assignee: jari
status: closed
priority: normal
commits:
  - hash: 7e00cd7
    summary: "fix: reject empty stats.items list in validation"
---

# 34. `stats.items` can be empty

_Source: `src/spec/validate.rs`_

## Description

`validate_stats()` checks individual items for valid aggregates but does not reject an empty items list. This is inconsistent with `validate_table()` which rejects empty `columns`.

A stats section with zero items is semantically useless and likely an error.

## Fix

Add empty check in `validate_stats()`:

```rust
if stats.items.is_empty() {
    errors.push(err(Some(label), "stats items list is empty"));
}
```
