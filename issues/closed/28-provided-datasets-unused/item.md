---
created: 2026-04-09
updated: 2026-04-11
type: improvement
reporter: ai-review
status: closed
priority: low
---

# 28. provided_datasets parameter unused in validate()

_Source: `src/spec/validate.rs` — `validate()` function_

## Description

The `validate()` function accepts `provided_datasets: &HashSet<String>` but never uses it. Validation only checks that `section.source` references a dataset declared in `spec.datasets`, not whether declared datasets are actually provided at runtime.

This means a spec can validate successfully but fail at runtime because a declared dataset was never supplied.

## Found by

Both Gemini and Codex during markdown section code review.

## Options

1. Use `provided_datasets` to validate that declared datasets are actually available
2. Remove the parameter if runtime validation happens elsewhere
