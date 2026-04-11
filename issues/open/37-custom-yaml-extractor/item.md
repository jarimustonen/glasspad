---
created: 2026-04-11
updated: 2026-04-11
type: improvement
reporter: jari
assignee: jari
status: open
priority: normal
---

# 37. Replace manual header/body handling with custom Axum Yaml extractor

_Source: src/routes/api.rs_

## Description

`create_pad()` and `update_pad()` manually extract `HeaderMap` + `Bytes`, validate Content-Type, and deserialize YAML inside the handler. Axum's idiomatic approach is a custom `FromRequest` extractor.

A `Yaml<T>` extractor (analogous to Axum's built-in `Json<T>`) would:
- Enforce Content-Type in one place
- Deserialize directly from bytes (zero-copy via `serde_yaml::from_slice`)
- Produce consistent 415/400 error responses
- Simplify handler signatures

## Scope

This builds on #36 (shared parsing) and could supersede it — if the extractor handles both Content-Type and deserialization, the shared helper becomes the extractor itself.

## Considerations

- Handlers still need access to raw `HeaderMap` for auth token — extractor only replaces the body parsing part
- Error responses should match the structured format from #39 if that lands first
