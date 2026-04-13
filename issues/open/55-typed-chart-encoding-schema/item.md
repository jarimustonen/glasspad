---
created: 2026-04-11
updated: 2026-04-11
type: improvement
reporter: claude
assignee: jari
status: open
priority: normal
---

# 55. Replace raw serde_json::Value with typed chart encoding schema

_Source: spec validation_

## Description

`chart.encoding` is typed as `serde_json::Value`, requiring manual runtime checks (`is_object()`, `is_null()`, `.get("field")`) throughout `validate_chart()`. This is the root cause of a class of validation bugs (#31, #36) — the validator must probe untyped JSON dynamically instead of relying on Rust's type system.

## Proposed approach

Define typed Rust structs for encoding channels so serde handles structural validation at deserialization time:

```rust
#[derive(Deserialize)]
#[serde(untagged)]
pub enum EncodingChannel {
    Detailed {
        field: Option<String>,
        #[serde(flatten)]
        extra: BTreeMap<String, serde_json::Value>,
    },
    Value(serde_json::Value),
}

pub struct ChartConfig {
    pub mark: String,
    pub encoding: Option<BTreeMap<String, EncodingChannel>>,
}
```

This would:
- Make null/missing encoding explicit as `Option`
- Remove manual `is_object()`/`is_null()` checks
- Enable compile-time guarantees on field access
- Reduce validator complexity

## Risks

- Encoding schema may need to remain permissive for Vega-Lite passthrough
- Migration requires updating all encoding access sites
- May need `#[serde(flatten)]` for unknown channel properties
