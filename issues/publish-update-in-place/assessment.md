# Findings assessment — publish-update-in-place

Panel: gemini-3.1-pro, gpt-5.6-sol, claude-opus-4-7, deepseek-v4-pro (1 round).
Full reviews: `history/review-publish-update-in-place.md`.

| # | Finding | Raised by | Verdict | Action |
|---|---------|-----------|---------|--------|
| 1 | Page/space slug **collision** → `update_space`/`space_tenant` can replace a served page (type confusion), breaking the fail-closed collision policy | all 4 (P0) | **CONFIRMED** | FIX — fail closed in `update_space` when `pages/<slug>` exists (already added pre-review; strengthened to dir-existence). Early `space_tenant` hole closed by removing the early check (see #6). |
| 2 | Consolidate ownership + `created_at` into **one validated meta read**; `space_owned_by` skips schema/grammar checks; `read_space_created_at().unwrap_or(now)` silently resets the clock for an existing space | gemini, openai, deepseek | **CONFIRMED** | FIX — `update_space` now does a single `read_space_meta` validated on schema/slug/tenant/grammar; unreadable/invalid → `NoSuchSpace` (no `unwrap_or(now)`). |
| 3 | CLI `--update` not validated against slug grammar, and validated **after** server/key resolution (wrong error precedence) | all 4 | **CONFIRMED** | FIX — `valid_space` check, moved before server/key resolution; `invalid_update_slug`. +CLI test. |
| 4 | Early `space_tenant` check = blocking I/O on the async worker + TOCTOU **false-404** (races `materialize_space`'s rename-aside window) + validation drift, and duplicates the authoritative locked check | openai, deepseek | **CONFIRMED** | FIX — remove the early check + unused `space_tenant`; rely solely on the locked authoritative `update_space`. Fixes blocking-I/O, false-404, and drift at once. |
| 5 | `materialize_space(replace)` can strand the live tree in `.old` (or diverge disk/memory) on an **ordinary** (non-crash) I/O error; recovery only runs at startup/GC | openai, deepseek (opus traced → self-heals) | **PARTIAL** | FIX the lost-live-tree case (synchronous rollback `.old→final` on failed swap). DEFER the fsync-divergence + generation-pointer redesign to a follow-up issue (pre-existing; shared with `publish_space`). |
| 6 | Retention comment contradicts GC: preserving `created_at` does NOT preserve the lease when GC keys on `updated_at` (activity lease — update extends it) | openai | **CONFIRMED (doc only)** | FIX — corrected the comment to describe the activity lease accurately. Behaviour matches existing keyed re-publish; intended. |
| 7 | PUT is replace-the-whole-representation: omitting `title`/`favicon`/`nav` clears them | opus | **CONFIRMED** | DOCUMENT — note in `--update` help / skill.md. Consistent with the existing `--space-key` re-publish (no NEW surprise). |
| 8 | `space_key` in a PUT body → generic `deny_unknown_fields` 400, not a targeted code | opus | **DECLINE** | `deny_unknown_fields` already names the offending field (actionable); accepting-then-rejecting would weaken the one-addressing-mode-per-request-type structural guarantee. |
| 9 | Optimistic concurrency (ETag/If-Match); last-writer-wins | openai, opus | **DEFER** | Out of scope; `--space-key` re-publish has identical last-writer-wins. Noted as future work. |
| 10 | Missing tests: corrupt-collision, concurrent PUT, PUT racing GC, oversize 413, CLI wire | all | **PARTIAL FIX** | Added collision test (pre-review) + concurrent-update store test. Page-only-slug + fail-shapes already covered. Oversize/GC-race left as lower-value (bounded by body limit / locked recheck). |
| 11 | `ValidatedSpace` newtype; O(n) snapshot clone under one global lock; `write_mapping` always-fsync base_dir | openai, opus | **DECLINE/DEFER** | Pre-existing architecture, applies equally to `publish_space`, explicitly documented trade-off (plan §6). `write_mapping` is not on the update path. Out of scope. |
| 12 | `updated_at` clock-monotonicity (backward clock jump) | opus (M5) | **DECLINE** | Non-blocking edge; pre-existing across all space writes. |

Deferred issues filed: `materialize-space-replace-durability` (#5 remainder), and PUT optimistic-concurrency noted therein (#9).
