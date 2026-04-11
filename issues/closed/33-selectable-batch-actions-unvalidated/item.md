---
created: 2026-04-11
updated: 2026-04-11
type: bug
reporter: llm-review
assignee: jari
status: closed
priority: normal
---

# 33. `selectable` and `batch_actions` fields are not validated

_Source: `src/spec/validate.rs`_

## Description

`Section` has `selectable: Option<bool>` and `batch_actions: Option<Vec<ActionDef>>` fields, but the validator never checks them. Missing validations:

1. `batch_actions` without `selectable: true` is likely invalid (actions need selected rows)
2. Empty `batch_actions` list should be rejected
3. Duplicate action IDs within `batch_actions` are not caught
4. These fields may only make sense on certain section types (table, list)

## Fix

Add validation rules:

```rust
if section.batch_actions.as_ref().is_some_and(|a| !a.is_empty())
    && section.selectable != Some(true)
{
    errors.push(err(Some(label), "batch_actions requires selectable: true"));
}
```

Also validate action ID uniqueness and non-empty action lists.
