---
created: 2026-04-11
updated: 2026-04-11
type: improvement
reporter: llm-review
assignee: jari
status: open
priority: normal
slug: profoundly-polite-queen
---

# 35. `chart.mark` and `stats.aggregate` should be enums instead of strings

_Source: `src/spec/schema.rs`, `src/spec/validate.rs`_

## Description

`ChartConfig.mark` and `StatsItem.aggregate` are modeled as `String` and manually validated against `SUPPORTED_MARKS` and `SUPPORTED_AGGREGATES` arrays in `validate.rs`. This duplicates validation that Serde could handle automatically.

Other similar fields like `SectionType`, `Layout`, `Timezone`, `SortType` are already proper enums with `#[serde(rename_all = "lowercase")]`.

## Fix

Convert to enums:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChartMark {
    Bar,
    Line,
    Arc,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Aggregate {
    Count,
    Distinct,
    Sum,
    Avg,
    Min,
    Max,
}
```

Then remove the manual `SUPPORTED_MARKS` / `SUPPORTED_AGGREGATES` checks from `validate.rs`. Serde will reject unknown values at parse time with clear error messages.
