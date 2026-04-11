---
created: 2026-04-11
updated: 2026-04-11
type: improvement
reporter: ai-review
status: open
priority: normal
---

# 33. Split validate() into structural and runtime validation

_Source: `src/spec/validate.rs` — `validate()` function_

## Description

`validate()` currently mixes two concerns:

1. **Structural spec validation** — is the YAML schema valid, are section configs internally consistent?
2. **Runtime dataset validation** — are declared datasets actually provided with data?

This coupling makes the function context-dependent. Callers that want schema-only validation (e.g. linting, IDE tooling) must fabricate a `provided_datasets` set. The API paths in `api.rs` hardcode `BTreeMap::new()` for external datasets, meaning declared external datasets always fail validation via the body-only API.

## Proposal

Split into:
- `validate_spec(spec) -> Vec<SpecError>` — pure structural/schema checks
- `validate_dataset_bindings(spec, provided_datasets) -> Vec<SpecError>` — runtime data completeness

Callers in `api.rs` would call both; schema-only callers would call just the first.

## Found by

LLM code review (Gemini + Codex consensus) during #28 fix review.

## Related

- #28 — added the `provided_datasets` check that surfaced this concern
