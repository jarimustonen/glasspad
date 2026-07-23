---
created: 2026-04-11
updated: 2026-04-11
type: bug
reporter: llm-review
assignee: jari
status: done
priority: normal
commits:
- hash: e836e51
  summary: 'fix: reject null/missing chart.encoding instead of silently skipping'
- hash: 95a246e
  summary: 'fix: add tests proving null/missing encoding rejection is intentional'
slug: seriously-truculent-ghost
closed: 2026-04-11
---

# 31. `interactive_filter` validation silently passes when `encoding` is null

_Source: `src/spec/validate.rs`_

## Description

In `validate_chart()`, the `interactive_filter.field` check only runs when `chart.encoding` is a JSON object:

```rust
if let Some(ref filter) = section.interactive_filter {
    if let serde_json::Value::Object(ref enc) = chart.encoding {
        // field check runs here
    }
    // no else branch - silently passes when encoding is null
}
```

When `encoding` is null (which validation currently allows), the filter field check is silently skipped. An `interactive_filter` cannot function without an encoding, so this should be an error.

## Fix

Add an else branch that rejects `interactive_filter` when encoding is not an object:

```rust
if let Some(ref filter) = section.interactive_filter {
    match &chart.encoding {
        serde_json::Value::Object(enc) => { /* existing field check */ }
        _ => {
            errors.push(err(
                Some(label),
                "interactive_filter requires chart.encoding to be an object",
            ));
        }
    }
}
```
