# Review: Cross-Filtering & UI Improvements (209d1057..HEAD)

**Reviewed:** 25+ commits adding interactive filtering, temporal axis fixes, hour range slider, TOC sidebar, table sorting, collapse/expand
**Reviewers:** Gemini, Codex (GPT-5.4)
**Rounds:** 2

---

## Critical Issues (Consensus)

Both reviewers agree these must be fixed:

### 1. Table data truncation on sort
- **What:** Data is sliced to MAX_ROWS (1000) *before* sorting, so sort only applies to an arbitrary subset
- **Where:** `dashboard.js` — `rebuildTbody`, `sourceData = allData.slice(0, totalRows)` before sort
- **Why it matters:** Users see wrong top/bottom values. Silent data corruption.
- **Fix:** Sort full array first, then slice for display

### 2. Discrete filter broken for non-string values
- **What:** `distinctKey()` uses `typeof + ':' + value`, but `extractFieldFromLabel()` returns strings from aria-labels. Keys never match for numbers/booleans.
- **Where:** `dashboard.js` — `distinctKey`, `extractFieldFromLabel`, `renderChartWithSelection`
- **Why it matters:** Click-to-filter silently fails for numeric/boolean fields. Selection UI lies.
- **Fix:** Coerce extracted values using field type metadata, or stop scraping aria-labels

### 3. Edit mode discards all active cross-filters
- **What:** Both discrete and temporal edit modes replace chart data with `rawData`, visually losing all other active filters
- **Where:** `dashboard.js` — `enterEditMode()`, `enterTemporalEdit()`
- **Why it matters:** Dashboard shows misleading data during editing. Other filters appear to vanish.
- **Fix:** Use `getFilteredDataExcluding(source, fieldBeingEdited)` — show data filtered by everything except the field being edited

### 4. Brush visual not cleared on "Reset all"
- **What:** Clearing filters resets JS state but leaves Vega's brush rectangle visible on the chart
- **Where:** `dashboard.js` — `clearFilters()`, brush signal handling
- **Why it matters:** UI lies — chart shows active selection when no filter exists
- **Fix:** Push `view.signal('brush', {}).run()` when clearing range filters

### 5. Chart updates not blocked during temporal edit mode
- **What:** Only `filterMode === 'edit'` is checked, not `temporalFilterMode`
- **Where:** `dashboard.js` — `updateChart()` return function
- **Why it matters:** Background filter changes overwrite chart data mid-slider-drag
- **Fix:** Guard both: `if (filterMode === 'edit' || temporalFilterMode === 'edit') return`

### 6. `getFilteredDataExcludingRange` is dead code
- **What:** Function defined but never called. Brush charts self-filter their own selected bars.
- **Where:** `dashboard.js` — function definition vs `updateChart()` usage
- **Why it matters:** Temporal brush context collapses to selected range only
- **Fix:** Use it in `updateChart()` for brush-owning charts

### 7. `renderChartWithSelection` infinite retry loop
- **What:** Polls for SVG element with no retry limit. Loops forever if chart fails.
- **Where:** `dashboard.js` — `renderChartWithSelection`
- **Fix:** Add max retry counter (e.g., 20 attempts)

### 8. `extractFieldFromLabel` substring collision
- **What:** `indexOf(field + ': ')` matches substrings of longer field names (e.g., "id" matches "provider_id")
- **Where:** `dashboard.js` — `extractFieldFromLabel`
- **Fix:** Use boundary-aware regex or structural match

---

## Disputed Issues

### 9. Timezone handling (local vs UTC)
- **Gemini:** Exaggerated — if dashboard is for local use, `getHours()` is correct
- **Codex:** Mixing local and UTC across different functions guarantees bugs across timezones
- **Moderator:** Codex is right that inconsistency exists (hour filter uses local, temporalExtent uses UTC ISO). For single-timezone use it works, but the inconsistency should be documented or unified.

### 10. Three parallel filter stores
- **Gemini:** Separating discrete/range/hour stores is a practical ES5 choice
- **Codex:** Should be unified into one normalized structure for maintainability
- **Moderator:** Both have points. Current approach works but creates duplication in filter bar, counting, and clearFilters. A unified model would reduce code but add type-checking. Not urgent but worth tracking.

### 11. Ghost layer performance
- **Gemini:** Doubles SVG DOM and rendering overhead for large datasets
- **Codex:** Valid concern but acceptable for small aggregates
- **Moderator:** For timeUnit:hours charts (24 bins max), ghost layer is fine. For raw temporal charts with large datasets, it would be expensive. Current code only uses it for timeUnit charts, so acceptable for now.

### 12. Sorting transitivity violation
- **Gemini:** Invalid dates falling to string comparison breaks sort guarantees
- **Codex:** Not provably non-transitive, just semantically wrong
- **Moderator:** Codex is right — it's sloppy but not mathematically broken. Invalid dates should be pushed to end consistently.

---

## Minor Findings

- **TOC layout hacks body padding** — should use content wrapper instead of `body.has-toc { padding-left }`
- **TOC scroll-spy index mismatch** — if sections fail to render, link indices diverge from sectionEls indices
- **`datasetHasField` checks only first 10 rows** — false rejection for sparse datasets
- **Inline datasets mutate global `datasets`** — minor collision risk with `_inline_*` names
- **Hour slider lacks keyboard accessibility** — no tabindex, ARIA roles, or keyboard handlers
- **Filter bar overflow** — many selected values render as one giant comma-separated button
- **`temporalEnc` dead variable** — assigned but never read
- **No `touchcancel` handler** in drag — can leak document listeners
- **CSS gradient overlay** on barely-collapsed tables (11 rows)
- **Synchronous `offsetTop` in scroll handler** — should use IntersectionObserver or cached offsets
- **No CSS focus-visible** on most new buttons
- **Missing schema tests** for `toc`, `SortType`, and new schema fields

---

## What's Solid

- **Section-level error isolation** — one broken section doesn't take down the dashboard
- **`deny_unknown_fields` on Rust schema** — prevents silent spec drift
- **Table sorting interaction model** — three-state toggle with aria-sort is well-designed
- **Ghost layer pattern** (for timeUnit charts) — elegant solution to the axis-locking problem
- **Filter edit mode UX** — All/None/Cancel/Apply with visual feedback is clean

---

## Moderator's Assessment

**Strongest reviewer:** Codex provided better architectural analysis (especially the DOM scraping critique), while Gemini found more specific bugs (table pre-slice, substring collision). Both contributed unique critical findings.

**Neither reviewer caught:**
- The `collapseCtrl.setExpanded()` not calling `onToggle` was a real bug fixed during the session but the pattern of "render state change without side-effect callback" could recur elsewhere.

**Single most important thing to address:**
The **edit mode context loss** (issue #3) is the highest-impact fix because it affects every filter interaction. When editing any filter, all other cross-filters vanish visually, which destroys user trust in the filtering system. A `getFilteredDataExcluding(source, field)` function would fix both discrete and temporal edit modes.

**Second priority** is the table sort truncation (issue #1) — it silently returns wrong data, which is worse than a visual glitch.
