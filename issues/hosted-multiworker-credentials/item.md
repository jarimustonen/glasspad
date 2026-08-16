---
created: 2026-08-12
updated: 2026-08-16
type: feature
status: wontfix
priority: low
labels: [deferred]
closed: 2026-08-16
---

# Secure credential model for many workers publishing to a hosted glasspad

## Description

FUTURE / deferred concern surfaced during publish-first-surface design (see its design.md § Multi-worker credential security).

Per-key config merge means the API key typically lives in the home config and is inherited by every repo — fine for one operator, but does NOT scale securely to many workers/employees publishing to the same hosted server: one shared, broad, hard-to-rotate, hard-to-attribute key.

Design a secure credential model for many publishers: per-worker scoped tokens (attributable, independently revocable), short-lived/rotatable credentials, key sourced from a secret manager / env rather than a plaintext home file, per-tenant scoping so a worker's key can only touch its own spaces.

NOT part of the publish-first first cut (which keeps the single-key model). Constraint on that work: the .glasspad.yaml `api_key` key should accept an INDIRECTION (env / key-file / secret-manager ref), not only an inline secret, so this can layer on later without a schema break. Revisit before rolling hosted publish out to a team.
