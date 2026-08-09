# Review findings assessment — idempotency_key

Multi-model review (`/llm-review`: gemini-3.1-pro, gpt-5.6-sol, opus-4-7, deepseek-v4-pro).
Full raw report in this repo's `history/` is not retained; decisions below.

## FIX (applied)

1. **Tenant-dir creation not made durable** (gpt #4, opus #2, deepseek #2) — `write_idem`
   fsync'd only `tenant_dir`, not `idem_dir`, so the *first* key for a tenant could lose
   the whole tenant directory on a crash, breaking the durability claim. → fsync `idem_dir`
   when the tenant dir is newly created; also fsync `root` after creating pages/idem dirs in
   `open`.
2. **Per-tenant isolation only path-scoped, not verified** (gpt #2, opus #7 — and the explicit
   quality gate) → `IdemRecord` now records `tenant`; lookup rejects a mapping whose recorded
   tenant ≠ requester, AND re-reads the mapped page's `meta.json` to confirm the page itself is
   owned by the requester. A mismatch falls through to a fresh create (self-healing).
3. **Unbounded idem-mapping growth; GC never reclaims** (all four) → `gc()` now sweeps the idem
   tree, deleting any mapping whose slug is no longer served (dangling), plus leftover `.tmp`
   staging entries in both `pages/` and `idem/` (safe: GC holds the mutation lock, so no write
   is in flight). Bounds growth to live pages + one GC interval.
4. **Poisoned mutex bricks the endpoint** (opus #3, gpt #19) → recover the guard with
   `unwrap_or_else(|e| e.into_inner())` (state under the lock is disk + a fresh snapshot read).
5. **No `deny_unknown_fields`** (opus #5) → a misspelled `idempotency_key` silently minted a
   new page every time. Added `#[serde(deny_unknown_fields)]` to `PublishRequest` + a test.
6. New tests: concurrent same-key → one page; cross-tenant crafted mapping not honored; GC
   reclaims dangling mappings; unknown field rejected.

## NOT DOING (with rationale)

- **Reservation/PENDING-COMMITTED transaction, request-fingerprint, 409 on body mismatch**
  (gpt #1/#16, gemini #4). The issue explicitly specifies mirroring publish-html's proven
  design ("return the SAME page (200)") and *accepts* the narrow page-durable-but-mapping-not
  crash window as the documented residual "one orphaned duplicate page (GC'd)". A retry there
  mints a fresh page — strictly better than today (no key ⇒ always duplicate). Redesigning to a
  state machine is out of scope and contradicts the specified semantics.
- **O(n) snapshot clone / global-mutex throughput** (gemini #1, gpt #14/#15, opus #9). Pre-existing
  architecture, explicitly acknowledged in existing code comments ("O(n) rebuild is acceptable at
  this iteration's scale … see plan §6"). Not introduced by this change.
- **Cross-process / multiple Store instances** (gpt #9) — host runs a single Store in one process.
- **openat/O_NOFOLLOW/O_EXCL + random tmp names on write paths** (gpt #11) — store root is
  operator-controlled; in-process single-writer via the lock; matches the pre-existing write
  pattern. Large change, out of scope.
- **Windows atomic-replace / non-unix durability** (gpt #10) — deploy target is unix; `fsync_dir`
  is already a documented cfg(unix) no-op elsewhere.
- **Key bound in chars not bytes / body limit** (gpt #18, opus #26) — the ingest router already
  sets `DefaultBodyLimit` (MAX_FILE_BYTES + 128 KiB), so the JSON body is capped before serde.
- **Trim-check-but-hash-raw whitespace** (opus #11) — contract is "exact byte sequence"; a
  deterministic caller sends identical bytes on retry. Defensible; documented.
