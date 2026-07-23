---
created: 2026-04-11
updated: 2026-07-23
type: improvement
reporter: llm-review
assignee: jari
status: obsolete
priority: normal
slug: chart-encoding-allows-null
closed: 2026-07-23
---

# 32. `chart.encoding` allows null despite being practically required

_Source: `src/spec/validate.rs`_

## Description

The chart encoding validation explicitly allows null:

```rust
if !chart.encoding.is_object() && !chart.encoding.is_null() {
    errors.push(err(Some(label), "chart.encoding must be a JSON object"));
    return;
}
```

Combined with `#[serde(default)]` on the `encoding` field (which defaults to null when omitted), this means a chart can pass validation with no encoding at all. The comment says "must be a JSON object" but the code contradicts this.

This creates a compound bug with @chartconfig-deny-unknown-fields: if `ChartConfig` lacks `deny_unknown_fields`, a misspelled `encoding` key silently defaults to null and passes validation.

## Fix

Require encoding to be an object:

```rust
if !chart.encoding.is_object() {
    errors.push(err(Some(label), "chart.encoding must be a JSON object"));
    return;
}
```

If there is a legitimate use case for omitting encoding, document it with a test.
