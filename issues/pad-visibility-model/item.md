---
created: 2026-04-11
updated: 2026-07-23
type: decision
reporter: maintainer
assignee: jari
status: obsolete
priority: normal
slug: pad-visibility-model
closed: 2026-07-23
---

# Decide pad visibility model: public-read vs fully private

_Source: src/routes/api.rs_

## Decision Needed

The API has inconsistent visibility semantics:

- `GET /api/pads/{id}` returns metadata (title, URL, timestamp) **without any token**
- `PUT /api/pads/{id}` and `DELETE /api/pads/{id}` require a token
- Invalid token on update/delete returns `403`, which leaks pad existence (vs `404`)

## Options

### A: Public-read, token-protected-write (capability URL model)

The UUID v4 pad ID acts as the read capability. Anyone with the URL can view. Only the token holder can modify/delete. This is the Pastebin/unlisted-Google-Doc model.

- `GET` stays public
- `403` on invalid token is fine (existence is already public via GET)
- Document this as the intended model

### B: Fully private pads

All endpoints require token. Invalid/missing token returns `404` to mask existence.

- `GET` requires token
- `PUT`/`DELETE` return `404` for both missing pad and invalid token
- Breaking change for existing clients

### C: Current behavior, documented

Keep as-is but document explicitly that GET is public and mutations are token-gated.

## Context

UUIDs are v4 (128-bit random), so brute-force enumeration is impractical. The main risk is URL leakage via logs, referrer headers, or shared links.
