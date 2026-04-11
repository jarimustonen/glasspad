---
created: 2026-04-11
updated: 2026-04-11
type: bug
reporter: llm-review
assignee: jari
status: closed
priority: high
commits:
  - hash: 8106ed9
    summary: "fix: validate provided_datasets properly with clean separation of concerns"
---

# 29. `provided_datasets` parameter in `validate()` is completely unused

_Source: `src/spec/validate.rs`_

## Description

The `validate()` function accepts `provided_datasets: &HashSet<String>` and documents it as "the set of dataset names supplied via --data flags or API", but the parameter is never used in the function body.

Validation only checks that section `source` references exist in `spec.datasets`. It never verifies that declared datasets were actually provided externally, or that provided datasets are declared.

This is a contract violation: the function signature and docs promise dataset-aware validation that doesn't exist.

## Reproduction

```rust
// This passes validation even though "events" is never provided:
spec.datasets.insert("events".to_string(), DatasetDecl {});
spec.sections[0].source = Some("events".to_string());
let errors = validate(&spec, &HashSet::new()); // empty provided set
assert!(errors.is_empty()); // passes! should it?
```

## Fix

Either:
1. Validate that declared datasets referenced by sections are present in `provided_datasets` (unless section has `inline_data`)
2. Validate that provided datasets are declared in `spec.datasets`
3. Or remove the parameter if this is intentionally spec-only validation
