---
created: 2026-08-09
updated: 2026-08-09
type: feature
status: in-progress
priority: normal
commits:
- hash: 58cf078
  summary: 'feat(hosted): optional idempotency_key for POST /api/v1/pages'
---

# hosted ingest idempotency key for POST /api/v1/pages

## Description


## Context

Discovered during homebase `digest-cron-to-glasspad` — migrating the openclaw
digest article publisher from publish-html to `glasspad publish`
(`glasspad.maalla.dev`). publish-html's `POST /pages` accepts an optional
`idempotency_key`: a repeated create with the same key returns the first page
(HTTP 200) instead of a new one, durably recording `key → slug` (fsync) only
after the page files are on disk. This gives the deterministic digest wrapper an
**exactly-once** guarantee even if its own receipt is lost in the crash window
between the page write and the receipt fsync.

glasspad's hosted ingest (`POST /api/v1/pages`, `src/hosted/ingest.rs`) has **no
such key** — every publish mints a fresh slug. The digest caller's receipt still
covers ordinary retries (it fsyncs the returned URL before notify/mark), so the
only residual is a narrow crash window that mints **one orphaned duplicate page**
(unreachable slug, GC'd at 90-day retention — harmless but untidy).

## Proposal

Add an optional `idempotency_key` field to the `PublishRequest` (bounded length).
When present, record `key → {slug}` durably (fsync + atomic rename) **after** the
page is stored, and on a repeat return the same page (200) instead of a new one
(201) — a dangling key (page GC'd/deleted) falls through to a fresh create. This
mirrors publish-html's proven design (`software/publish-html` in homebase; see
`infra/openclaw/AGENTS-DIGEST.md` → "The publish crash window"). It restores
full lost-receipt exactly-once for API-key publishers without changing the
default (no key → fresh every time, byte-for-byte today's behaviour).

Non-blocking for the digest migration (which shipped with the receipt-only
guarantee); this closes the residual orphan-page window.
