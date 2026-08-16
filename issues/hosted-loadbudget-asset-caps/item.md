---
created: 2026-08-15
updated: 2026-08-16
type: improvement
status: wontfix
priority: low
related: ['@hosted-store-generation-pointer']
closed: 2026-08-16
---

# Hosted space loader: per-file asset cap + budget-charge directory fan-out

## Description

From the hosted-store-generation-pointer review panel (pre-existing loader). In src/hosted/store.rs `read_space_assets` uses `read_capped(path, MAX_FILE_BYTES)` but does NOT reject a returned MAX_FILE_BYTES+1 (unlike `read_space_pages` via read_capped_utf8) — over-cap assets are re-rejected downstream by build_space_bundle, so this is defense-in-depth. Also, directory traversal in read_space_assets does not charge the LoadBudget per directory, so a tampered store with a huge wide/deep empty-dir tree can burn CPU on startup/GC without tripping the entry/byte budget. Fix: reject over-cap assets before push; charge budget.reserve_entry() for each directory before recursing. Raised by gemini, anthropic, deepseek.
