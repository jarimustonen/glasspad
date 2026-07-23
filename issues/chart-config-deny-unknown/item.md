---
created: 2026-04-09
type: improvement
reporter: ai-review
status: done
closed: 2026-04-11
priority: low
slug: strikingly-spectacular-clam
---

# ChartConfig lacks deny_unknown_fields

_Source: `src/spec/schema.rs` — `ChartConfig` struct_

## Description

`ChartConfig` does not have `#[serde(deny_unknown_fields)]`, unlike most other config structs in the schema. This means typos like `makr: bar` instead of `mark: bar` are silently ignored if `mark` is also present.

## Found by

Codex (gpt-5.4) during plan review, round 2.
