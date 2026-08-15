---
created: 2026-08-15
updated: 2026-08-15
type: improvement
status: open
priority: normal
related: ['@hosted-store-generation-pointer']
---

# Harden hosted idempotency-mapping sweep (transient-error deletes, invalid-mapping retention, symlink safety, empty-tenant reap)

## Description

From the hosted-store-generation-pointer review panel (pre-existing, not introduced by that work). `sweep_mappings` in src/hosted/store.rs deletes an idempotency mapping when `read_capped` returns ANY error (incl. transient EMFILE/EACCES/EIO), which weakens exactly-once semantics precisely under load. It also (a) retains mappings whose schema/slug/tenant are invalid as long as the target slug is served (dead weight), (b) never reaps empty tenant directories (slow inode leak), and (c) follows symlinked tenant dirs when reading/deleting (a tampered store could read/delete outside the store). Fix: only delete on NotFound or an explicit parse/validation failure; reap wrong-schema/slug/tenant records; remove empty tenant dirs; reject symlinked tenant dirs. Raised by gemini + deepseek.
