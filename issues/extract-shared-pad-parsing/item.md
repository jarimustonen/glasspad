---
created: 2026-04-11
updated: 2026-07-23
type: improvement
reporter: jari
assignee: jari
status: obsolete
priority: normal
slug: extract-shared-pad-parsing
closed: 2026-07-23
---

# Extract shared parse/validate pipeline from create_pad and update_pad

_Source: src/routes/api.rs_

## Description

`create_pad()` and `update_pad()` duplicate ~25 lines of identical logic:
- YAML deserialization
- Dataset collection via `collect_datasets()`
- Spec validation
- Dataset metadata inference

This duplication is what caused issue @interactive-filter-null-encoding (Content-Type check present in create but missing in update). The next validation change will likely drift again.

## Proposed Change

Extract a shared helper:

```rust
struct ParsedPadInput {
    spec: DashboardSpec,
    datasets: BTreeMap<String, Dataset>,
    dataset_meta: BTreeMap<String, DatasetMeta>,
}

fn parse_and_validate_pad(body: &[u8]) -> Result<ParsedPadInput, (StatusCode, String)> { ... }
```

Both handlers call this after their endpoint-specific checks (auth, Content-Type).
