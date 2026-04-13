---
created: 2026-04-11
updated: 2026-04-11
type: improvement
reporter: jari
assignee: jari
status: open
priority: medium
---

# 56. Replace untyped serde_json::Value for chart.encoding with typed structs

_Source: `src/spec/schema.rs`, `src/spec/validate.rs`_

## Description

`chart.encoding` is currently `serde_json::Value`, which forces runtime type checks (`is_object()`, `is_null()`) in `validate_chart()`. This caused a bug where `encoding: null` was silently accepted (issue #32), because the validator had to manually guard against every invalid shape.

Replacing the untyped value with typed structs (e.g. `HashMap<String, ChannelDef>` or an explicit `ChartEncoding` struct with known channels) would reject invalid shapes at deserialization time, eliminating this entire class of bugs. Serde would enforce that encoding is an object with the expected structure before validation even runs.

### Affected files

- `src/spec/schema.rs` — field type change from `serde_json::Value` to typed struct
- `src/spec/validate.rs` — remove runtime shape checks that become unnecessary
- Client-side rendering — may need updates if encoding serialization changes

### Benefits

- Invalid encoding shapes (null, arrays, strings) rejected at parse time
- No more manual `is_object()` / `is_null()` guards in validation
- Better editor support and documentation through explicit field types
- Compile-time guarantees on encoding structure

## Found during

Review of issue #32 (reject null `chart.encoding`), where the fragility of runtime type checks on untyped JSON became apparent.

## Related

- **#32** — encoding null bug (immediate trigger for this issue)
- **#36** — structural/semantic validation phases (complementary refactor)
