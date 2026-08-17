---
created: 2026-04-11
updated: 2026-07-24
type: decision
reporter: maintainer
assignee: jari
status: wontfix
priority: normal
slug: auth-status-codes
closed: 2026-07-24
---

# Decide on auth error status codes (401 vs 403)

_Source: src/routes/api.rs_

## Decision Needed

`update_pad()` and `delete_pad()` return `403 Forbidden` for invalid/missing tokens. Per HTTP semantics:

- **401 Unauthorized**: credentials missing or invalid (authentication failure)
- **403 Forbidden**: credentials valid but insufficient permissions (authorization failure)

The current per-pad token is an authentication credential, not a permission check, so `401` may be more correct.

## Options

### A: Use 401 for invalid/missing token

Semantically correct per RFC 7235. Clients can distinguish "bad credentials" from "insufficient permissions" if roles are added later.

### B: Keep 403

Simpler. Many APIs use 403 for both cases. Avoids the `WWW-Authenticate` header requirement that 401 technically implies.

### C: Return 404 for both missing pad and invalid token

Masks pad existence entirely. Ties into @pad-visibility-model (visibility model decision).

## Context

This overlaps with @pad-visibility-model. The status code choice depends on the visibility model:
- If pads are public-read (option A in @pad-visibility-model), 401/403 distinction matters
- If pads are fully private (option B in @pad-visibility-model), returning 404 for everything is simplest
