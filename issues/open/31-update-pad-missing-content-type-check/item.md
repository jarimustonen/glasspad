---
created: 2026-04-11
updated: 2026-04-11
type: bug
reporter: ai-review
status: open
priority: normal
---

# 31. update_pad() missing Content-Type validation

_Source: `src/routes/api.rs` — `update_pad()` function_

## Description

`create_pad()` checks the `Content-Type` header and rejects non-YAML content types with `415 Unsupported Media Type`. `update_pad()` does not perform the same check, creating inconsistent API behavior.

## Found by

LLM code review (Codex) during #28 fix review.

## Fix

Factor the content-type check into a shared helper and call it from both `create_pad()` and `update_pad()`.
