---
created: 2026-08-15
updated: 2026-08-15
type: improvement
status: open
priority: normal
related: ['@hosted-store-generation-pointer']
---

# Hosted snapshot: Arc-share Space bodies to make publish/round O(1) (drop O(n) deep clone) + enforce MAX_PAGES on load

## Description

From the hosted-store-generation-pointer review panel (already-documented future work, 'plan §6'). Snapshot stores HashMap<String, Space> with raw String/Vec<u8> bodies; every publish/update/push_round does `current.spaces.clone()` which DEEP-copies every page body on the server (O(n) RAM+CPU) while holding the mutation lock — at MAX_PAGES=100k this is a large per-write cost and an OOM risk under concurrency. Change Snapshot to HashMap<String, Arc<Space>> so the clone copies references. Also: the global mutation lock + O(N) GC filesystem walk cap write throughput (shrink the critical section — stage/fsync outside the lock, take it only for the pointer flip + snapshot swap); and MAX_PAGES is enforced only at write, not on scan/load. Raised by gemini + anthropic.
