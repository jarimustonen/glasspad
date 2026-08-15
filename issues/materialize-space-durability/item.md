---
created: 2026-08-15
updated: 2026-08-15
type: task
status: open
priority: normal
---

# materialize_space(replace): synchronous rollback on fsync-divergence + generation-pointer redesign

## Description


## Context

Raised by the `publish-update-in-place` review panel (gpt-5.6-sol, deepseek-v4-pro;
opus traced it as self-healing). Pre-existing in `src/hosted/store.rs::materialize_space`,
shared by `publish_space` (keyed re-publish) and now the new `PUT /api/v1/spaces/{slug}`
update path.

Two residual non-crash I/O-error windows (the "lost live tree" case is **already fixed**
in publish-update-in-place — a synchronous `.old → final` restore on a failed swap):

1. **fsync-after-swap divergence.** If `fsync_dir(spaces_dir)` fails *after* the new tree
   is renamed into `final_dir`, `materialize_space` returns `Err` → the handler returns
   500 and the served snapshot is NOT swapped, yet the new tree is already on disk. Disk
   and memory diverge until a restart (which then serves the new tree — the content the
   failed request intended). Narrow (fsync failure is rare) and self-heals toward the
   intended content, but the caller's `Err = "unchanged"` assumption is not strictly true.

2. **Richer outcome / generation-pointer redesign.** The robust fix (per gpt-5.6-sol) is
   immutable generation directories + an atomically-swapped "current generation" pointer,
   or a richer `materialize_space` return so a caller can distinguish "final rename
   happened, must swap snapshot" from "nothing changed". Removes the missing-final window
   entirely and simplifies rollback/recovery.

## Also noted (same review): PUT optimistic concurrency

`PUT /api/v1/spaces/{slug}` and `--space-key` re-publish are both last-writer-wins with no
stale-write protection. Optional future enhancement: return a content-version/ETag on
publish/update and honor `If-Match` on PUT (`409`/`412` on stale). Not required by the
publish-update-in-place scope; filed here so the durability/concurrency work travels together.
