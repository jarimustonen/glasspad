---
created: 2026-08-15
updated: 2026-08-17
type: improvement
status: in-progress
priority: normal
related: ['@hosted-store-generation-pointer']
lane: hosted-hardening
lane_seq: 10
collision: [src/hosted/store.rs]
---

# Hosted idempotency sweep: don't delete a mapping on a transient read error

## Description

`sweep_mappings` in `src/hosted/store.rs` deletes an idempotency mapping whenever
`read_capped` returns **any** error — including transient ones (`EMFILE`, `EACCES`, `EIO`).
That discards duplicate-publish protection precisely when it matters: under load, or during
the retry that the idempotency key exists to make safe.

A single client is enough to hit this. Idempotency keys exist for one publisher's retries
(network blip → retry with the same key); if the sweep dropped the mapping in between, the
retry mints a second page instead of returning the first.

**Fix:** delete only on `NotFound` or an explicit parse/validation failure. Any other error
means "could not determine" — leave the mapping in place and let the next sweep decide.

## Scope

This issue is **only** the transient-error delete. Three items from the original review-panel
report were deliberately dropped on 2026-08-17 and are **not** in scope:

- *Symlinked tenant dirs* — requires an attacker who already has write access to the server's
  own storage directory, i.e. post-compromise.
- *Empty tenant-directory reap* — a slow inode leak nobody has observed.
- *Retaining invalid-schema/slug/tenant records* — dead weight, cosmetic.

They were rejected under the standing no-speculative-hardening rule (see `TODO.md` →
Standing lessons). Do not reintroduce them; a review finding that resurfaces them should be
rejected on the same grounds.

## Provenance

From the `hosted-store-generation-pointer` review panel (pre-existing, not introduced by that
work; raised by gemini + deepseek). Narrowed from four bundled items to one on 2026-08-17.
