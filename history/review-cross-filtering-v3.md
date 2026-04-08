# Review: All Changes Since 209d1057 (v3)

**Reviewed:** 39 commits, ~6000 lines across dashboard.js, dashboard.css, schema.rs, cli.rs, mbox.rs
**Reviewers:** Gemini, Codex (GPT-5.4)
**Rounds:** 2

---

## Critical Issues (Consensus)

### 1. Mbox import via read_to_string will fail on valid email files
- **What:** `cli.rs` reads mbox/eml files with `fs::read_to_string()`, which fails on non-UTF-8 content and loads entire file into memory
- **Where:** `src/cli.rs:128`
- **Why it matters:** Valid mbox files often contain non-UTF-8 byte sequences (attachments, legacy encodings). Large mbox files will OOM.
- **Fix:** Use `fs::read()` → `&[u8]`, add `parse_mbox_bytes()` entry point, or stream via `MessageIterator::new(File)`

### 2. "None" button clears filter instead of showing zero results
- **What:** `setFilter(source, field, [])` deletes the filter field entirely, reverting to unfiltered data
- **Where:** `src/client/dashboard.js` — `setFilter()` line 32
- **Why it matters:** User selects "None" expecting empty results, gets all results instead
- **Fix:** Distinguish `null` (clear filter) from `[]` (empty allowed set). Only delete on `null`.

### 3. SVG DOM manipulation wiped by Vega re-renders
- **What:** `renderChartWithSelection()` and `dimBarsOutsideRange()` set `.style.opacity` directly on SVG paths. Any Vega re-render (tooltip, resize, data change) wipes these.
- **Where:** `src/client/dashboard.js` — multiple functions
- **Why it matters:** Selection/dimming visuals are unreliable and can silently disappear
- **Fix:** Use Vega params/signals with conditional opacity encoding instead of post-render DOM mutation

### 4. Brush chart update suppression always fails
- **What:** `getFilteredDataExcluding()` returns a new array every call. `filtered !== lastFilteredData` is always true for brush charts, causing unnecessary Vega data replacement on every filter change.
- **Where:** `src/client/dashboard.js` — `updateChart()` + `getFilteredDataExcluding()`
- **Why it matters:** Performance waste and may destabilize brush selection state
- **Fix:** Memoize by filter state version, or compare content hash instead of reference identity

### 5. Hour slider activates for all timeUnit values, not just hours
- **What:** `hasTimeUnit` is a boolean that's true for any temporal timeUnit (month, day, year...), but the slider UI assumes hours (0-23)
- **Where:** `src/client/dashboard.js` — temporal filter detection
- **Why it matters:** Charts with `timeUnit: month` get a meaningless 0-23 hour slider
- **Fix:** Check `enc.timeUnit === 'hours' || enc.timeUnit === 'utchours'` specifically

### 6. formatTemporalRange ignores spec.timezone
- **What:** Range filter tags always display in browser local time, even when `spec.timezone: utc`
- **Where:** `src/client/dashboard.js` — `formatTemporalRange()`
- **Why it matters:** Filter display contradicts chart axis timezone, confusing users
- **Fix:** Pass `timeZone: 'UTC'` to date formatters when `useUtc` is true

---

## Disputed Issues

### 7. CLI dataset injection model (source + inline_data)
- **Codex:** Fatal — client prioritizes `source` over `inline_data`, so injected data is ignored
- **Gemini:** Agrees this is broken and fatal
- **Moderator:** Both agree. However, this may already work if the server's `collect_datasets()` extracts inline_data before rendering. Needs verification against actual server code path. If broken, fix by either stripping `source` when injecting or populating top-level datasets.

### 8. Ghost layer embedding rawData inline
- **Gemini:** Expensive for large datasets, should inject post-embed
- **Codex:** Directionally right but post-embed injection needs careful implementation
- **Moderator:** For timeUnit charts (max 24 bins of aggregated data), the ghost layer processes rawData through Vega's aggregation. The real question is whether rawData duplication in the spec JSON is expensive. For typical dashboard datasets (<10K rows), acceptable. For larger datasets, consider deferred injection.

### 9. TOC CSS direct-child selectors
- **Gemini:** Nitpick, works fine for controlled architecture
- **Codex:** Fragile, should use dedicated container
- **Moderator:** Gemini is right that it works now, but Codex is right that it's brittle. Low priority.

### 10. Mark schema String vs object
- **Both agree:** Client supports object marks, Rust schema rejects them
- **Moderator:** The Rust schema uses `serde_json::Value` for encoding but `String` for mark. Should be `serde_json::Value` for mark too. Real but low severity since current specs only use string marks.

---

## Minor Findings

- **Mbox missing fields → empty string instead of null** — should use CellValue::Null
- **Mbox extract_all_addresses drops name-only addresses** — lossy but acceptable for email use case
- **Touch listeners may need {passive: false}** — browser-dependent, low severity
- **Table header rebuilt on every sort click** — minor DOM churn, not a real perf issue
- **Filter count semantics inconsistent** — discrete counted per value, range per field
- **Synthetic inline dataset naming unstable** — order-dependent counter
- **getDataResult mutates global datasets** — impure accessor
- **No cleanup/teardown for chart views and observers** — acceptable for single-page-load model
- **Attachment size formatting truncates KB** — cosmetic
- **`mountChart()` function is enormous** — needs structural decomposition

---

## What's Solid

- **Ghost layer pattern** for temporal axis locking — elegant and correct for timeUnit charts
- **Type-aware filter key coercion** — fixes the non-string filter matching from v2
- **Edit mode context preservation** (getFilteredDataExcluding) — correct approach
- **Mbox parser using mail-parser's MessageIterator** — robust, well-tested library
- **Schema tests** for toc, timezone, SortType — good regression protection
- **Timezone-aware hour extraction** via `getHourOfDate()` — clean abstraction

---

## Moderator's Assessment

**Strongest reviewer:** Codex provided better structural analysis and caught the hour slider scope bug. Gemini found the "None" button semantics bug and the brush array identity issue — both excellent catches.

**Issues NEITHER caught:**
- `getFilteredDataExcluding` is not cached and will be slow on large datasets when multiple filters are active and charts update frequently. The `filteredCache` only caches `getFilteredData`, not the excluding variant.

**Top 3 priorities:**
1. **Mbox read_to_string → read bytes** — blocks email import for real data
2. **"None" button semantics** — user-visible correctness bug
3. **Hour slider timeUnit scope** — easy fix, prevents nonsensical UI on non-hour charts

The DOM/aria-label scraping issue (#3) is architecturally important but a larger refactor. It should be planned as a separate effort rather than blocking current work.
