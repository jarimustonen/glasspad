---
created: 2026-04-11
updated: 2026-04-11
type: bug
reporter: claude
assignee: jari
status: open
priority: normal
slug: altogether-screeching-help
---

# Shallow encoding field lookup falsely rejects nested fields

_Source: spec validation_

## Description

`validate_chart()` searches for `interactive_filter.field` in chart encoding using a shallow one-level lookup: `enc.values().any(|channel| channel.get("field")...)`. This only finds fields at the top level of each encoding channel.

Vega-Lite encodings frequently use nested structures where `field` appears deeper:
- **Array channels**: `tooltip: [{ field: "country" }, { field: "sales" }]`
- **Condition blocks**: `color: { condition: { field: "country", ... }, value: "grey" }`

These valid specs are falsely rejected with "interactive_filter.field not found in chart encoding".

## Reproduction

```yaml
sections:
  - id: c1
    title: "Chart"
    type: chart
    interactive_filter:
      field: country
    chart:
      mark: bar
      encoding:
        x: { field: "year" }
        color:
          condition:
            selection: brush
            field: country
            type: nominal
          value: grey
```

Validation incorrectly reports `country` not found.

## Fix

Replace the shallow lookup with a recursive search through the encoding JSON tree:

```rust
fn encoding_contains_field(value: &serde_json::Value, target: &str) -> bool {
    match value {
        serde_json::Value::Object(map) => {
            if map.get("field").and_then(|v| v.as_str()) == Some(target) {
                return true;
            }
            map.values().any(|v| encoding_contains_field(v, target))
        }
        serde_json::Value::Array(items) => {
            items.iter().any(|v| encoding_contains_field(v, target))
        }
        _ => false,
    }
}
```
