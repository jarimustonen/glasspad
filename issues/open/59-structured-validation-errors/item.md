---
created: 2026-04-11
updated: 2026-04-11
type: improvement
reporter: jari
assignee: jari
status: open
priority: normal
---

# 59. Replace string-based validation errors with structured error types

_Source: src/spec/validate.rs_

## Description

The validation layer uses free-form strings (`SpecError { message: String, section: Option<String> }`) for all errors. Tests match on substrings which is fragile — message wording changes break tests. A structured error enum (e.g., `SpecErrorKind::EmptyStatsItems`) would allow tests to match on kind rather than text, enable programmatic error classification by consumers, and support localization. This is architectural debt that grows with every new validation check.

## Files

- `src/spec/validate.rs` — `SpecError` struct and all `err()` calls

## Approach

1. Define a `SpecErrorKind` enum with a variant for each validation failure.
2. Add a `kind: SpecErrorKind` field to `SpecError`, keeping `message` for human-readable text.
3. Migrate `err()` call sites to set the appropriate kind.
4. Update tests to assert on `kind` instead of substring matching on `message`.
