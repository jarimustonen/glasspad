---
created: 2026-04-11
updated: 2026-04-11
type: improvement
reporter: jari
assignee: 
status: open
priority: normal
---

# 51. Refactor selectable from Option<bool> to bool

## Description

The `selectable` field on `Section` in `src/spec/schema.rs` uses `Option<bool>`, giving three states: `None`, `Some(false)`, and `Some(true)`. However, `None` and `Some(false)` are semantically identical — both mean "not selectable".

This should be simplified to `bool` with `#[serde(default)]`, which:
- Removes tri-state ambiguity
- Simplifies validation (`section.selectable` instead of `section.selectable == Some(true)`)
- Aligns with how other boolean fields like `toc` and `show_totals` are handled

Identified during LLM review of batch_actions validation (#33).

## Scope

- Change `pub selectable: Option<bool>` to `pub selectable: bool` in `Section` struct
- Update all validation checks from `section.selectable == Some(true)` to `section.selectable`
- Update all test code that sets `selectable: None` to remove the field or use `false`
- Verify serde round-trip: omitted field defaults to `false`, explicit `true`/`false` both work
