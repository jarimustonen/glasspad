---
created: 2026-04-11
updated: 2026-07-23
type: improvement
reporter: maintainer
assignee: jari
status: obsolete
priority: normal
slug: collect-datasets-api
closed: 2026-07-23
---

# Clean up collect_datasets API for route-only usage

_Source: src/routes/api.rs_

## Description

`collect_datasets()` takes `external_datasets: &BTreeMap<String, Dataset>` but both API handlers always pass `&BTreeMap::new()`. The external datasets path exists for CLI/multipart usage.

The current API forces route handlers to allocate an empty map for an unused parameter.

## Proposed Change

Either:
- Split into `collect_inline_datasets(spec)` for routes and keep the merged version for CLI
- Make `external_datasets` an `Option` to avoid the empty-map allocation
- Leave as-is if CLI multipart support is coming soon (document the intent)

## Context

This is a minor ergonomic issue. The `external_datasets` block is not dead code — it exists for the CLI path — but the API signature is awkward for route-only callers.
