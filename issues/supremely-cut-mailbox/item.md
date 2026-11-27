---
created: 2026-11-27
updated: 2026-11-27
type: bug
status: fixed
priority: normal
provenance: other
provenance_detail: release verification screenshot
source_ref: taskfleet:01m8ergvvps7d4t0ztjf119wap/observed:single-page-theme-overlap-390
originating_run: 01m8ergvvps7d4t0ztjf119wap
originating_run_kind: spinoff
lane: unlaned
closed: 2026-11-27
commits:
- hash: bf63cdf
  summary: single-page theme control gets its own strip on phones
---

# Single-page theme control overlaps the first heading on phone-width screens

## Description

In 0.21.0 a single-page space shows only the floating System / Light / Dark theme control in the top-right corner. At a 390 px wide viewport (phone width), that control sits over the page's first heading: on a hosted Markdown page with a long H1 the last word of the title is partly covered by the control.

Seen with headless Chrome at 390 × 844 on a hosted single-page Markdown space (`prose`-style document whose H1 is "Visual acceptance package"). At 1280 px there is enough margin and nothing overlaps.

Expected: the corner control does not cover artifact content at narrow widths, for example by reserving top space for it in the single-page layout or shrinking it on small screens.
