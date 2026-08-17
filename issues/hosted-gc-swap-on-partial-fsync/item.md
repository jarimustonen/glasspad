---
created: 2026-08-15
updated: 2026-08-17
type: improvement
status: wontfix
priority: normal
related: ['@hosted-store-generation-pointer']
lane: hosted-hardening
lane_seq: 30
collision: [src/hosted/store.rs]
closed: 2026-08-17
---

# Hosted GC: swap the served snapshot before surfacing a post-removal fsync error

## Description

From the hosted-store-generation-pointer review panel (pre-existing). `Store::gc` in src/hosted/store.rs removes expired page/space dirs, then `fsync_dir(pages)?; fsync_dir(spaces)?; scan_disk(); swap()`. If either fsync (or the intermediate `read_dir?`) fails, gc returns Err BEFORE swapping — the in-memory snapshot keeps serving already-deleted pages until restart (a divergence a restart would not reproduce). Fix: follow the commit-then-surface pattern used elsewhere — rebuild+swap the snapshot from current disk state, THEN surface any fsync error. Raised by openai + deepseek.
