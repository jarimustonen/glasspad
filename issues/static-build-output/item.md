---
created: 2026-08-06
updated: 2026-08-06
type: feature
status: in-progress
priority: low
commits:
- hash: a1548b5
  summary: 'feat(build): glasspad build — static self-contained render of a space'
---

# glasspad build: static self-contained render (optional)

## Description

`glasspad build <space> <out>` — static render of a space to self-contained HTML
(fragment-wrap + full-doc passthrough + base-lib handling; self-contained vs
shared-libs flag), no server/bind. Useful for offline docsite output and external
preview transports. Lower priority than the hosted-share path.

Part of the agent→browser-HTML consolidation. Full design + rationale: homebase `issues/glasspad-html-consolidation/design.md` (Option D). These features make glasspad the single canonical agent→HTML surface.
Ref: src/artifact_host/wrap.rs.
