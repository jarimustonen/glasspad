---
created: 2026-04-11
updated: 2026-04-11
type: bug
reporter: ai-review
assignee: jari
status: open
priority: normal
slug: unusually-guarded-jump
---

# validate_chart() allows encoding: null despite comment saying "must be an object"

_Source: `src/spec/validate.rs` — `validate_chart()` function_

## Description

The validation code for chart encoding has a comment stating "chart.encoding must be an object" but the condition explicitly allows `null`:

```rust
if !chart.encoding.is_object() && !chart.encoding.is_null() {
    errors.push(err(Some(label), "chart.encoding must be a JSON object"));
}
```

Either `null` is intentionally valid (in which case the comment and error message are wrong) or it should be rejected (in which case the condition is wrong). Downstream interactive filter checks assume an object, so allowing `null` is likely unintended.

## Found by

LLM code review (Codex) during @provided-datasets-unused-2 fix review.

## Fix

Decide the intended behavior and align comment, condition, and error message.
