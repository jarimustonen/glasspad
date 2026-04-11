---
created: 2026-04-11
updated: 2026-04-11
type: bug
reporter: llm-review
assignee: jari
status: open
priority: normal
---

# 30. `ChartConfig` missing `deny_unknown_fields`

_Source: `src/spec/schema.rs`_

## Description

Every other config struct (`TableConfig`, `StatsConfig`, `ListConfig`, `MarkdownConfig`, `DetailConfig`, etc.) uses `#[serde(deny_unknown_fields)]`, but `ChartConfig` does not.

This means typos inside `chart:` blocks are silently accepted:

```yaml
chart:
  mark: bar
  encodng: { x: { field: country } }
```

Serde will ignore `encodng`, default `encoding` to null, and the chart will render empty with no error.

## Fix

Add `#[serde(deny_unknown_fields)]` to `ChartConfig`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChartConfig {
    pub mark: String,
    #[serde(default)]
    pub encoding: serde_json::Value,
}
```
