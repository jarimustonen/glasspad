---
created: 2026-04-11
updated: 2026-04-11
type: decision
reporter: jari
assignee: jari
status: open
priority: normal
---

# 39. Decide on structured API error responses

_Source: src/routes/api.rs_

## Decision Needed

API error responses are currently plain text strings (`(StatusCode, String)`), while success responses are JSON. This inconsistency makes client implementation harder — clients must handle both `application/json` and `text/plain` responses.

## Options

### A: Structured JSON errors

```json
{"error": "Expected Content-Type: application/x-yaml"}
```

or with detail:

```json
{"error": "Spec validation failed", "details": ["missing source for section 0"]}
```

Implement via an `ApiError` enum with `IntoResponse`.

### B: Keep plain text

Simpler server-side. Clients already handle it. Error messages are human-readable.

### C: RFC 7807 Problem Details

```json
{"type": "about:blank", "status": 415, "title": "Unsupported Media Type", "detail": "Expected Content-Type: application/x-yaml"}
```

Standard format but heavier.

## Current Inconsistencies

| Endpoint | Success | Error |
|----------|---------|-------|
| `POST /api/pads` | `201 + JSON` | `4xx + plain text` |
| `PUT /api/pads/{id}` | `200` (no body) | `4xx + plain text` |
| `DELETE /api/pads/{id}` | `204` (no body) | `4xx + plain text` |
| `GET /api/pads/{id}` | `200 + JSON` | `404` (no body) |
