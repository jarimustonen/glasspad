---
created: 2026-04-11
updated: 2026-07-23
type: improvement
reporter: maintainer
assignee: jari
status: obsolete
priority: low
slug: validate-structural-semantic-phases
closed: 2026-07-23
---

# Separate structural and semantic validation phases in validator

_Source: `src/spec/validate.rs`_

## Description

The validator mixes structural checks (e.g. "is encoding an object?") with semantic checks (e.g. "does `interactive_filter.field` exist in encoding?") in the same functions. It relies on ad-hoc early `return` statements to prevent semantic validation from running on structurally invalid data.

This is fragile — removing an early return can cause misleading cascading errors. A formal two-phase approach (structural validation first, semantic validation only on valid structure) would make the validator more maintainable and prevent regression bugs.

### Current behavior

Validation functions check structure and semantics inline, using early returns to short-circuit when structure is invalid:

```rust
fn validate_chart(...) {
    if encoding.is_null() {
        errors.push(...);
        return;  // prevents semantic checks from running on null
    }
    // semantic checks follow...
}
```

### Proposed approach

Split each validation function into two phases:

1. **Structural phase** — validate shapes, types, required fields, non-null/non-empty constraints
2. **Semantic phase** — validate cross-references, field existence, consistency between sections

Run the semantic phase only when the structural phase produces no errors for that section.

## Found during

Review of issue @chart-encoding-allows-null (reject null `chart.encoding`), where the fragility of the current approach became apparent.

## Related

- **@chart-encoding-allows-null** — chart encoding null validation (immediate trigger)
- **@selectable-batch-actions-unvalidated** — split structural vs runtime validation (orthogonal but complementary refactor)
