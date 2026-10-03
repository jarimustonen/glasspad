---
created: 2026-10-03
updated: 2026-10-03
type: feature
reporter: jari
status: done
priority: normal
closed: 2026-10-03
closed_by: jari
---

# Render Mermaid fences in Markdown artifacts

## Description

Render fenced mermaid diagrams in Markdown across serve, publish, render, and build without relaxing the artifact sandbox or requiring a CDN. Preserve readable source when scripts fail; respond to theme changes.

## Acceptance Criteria

- [x] Fenced Mermaid blocks render as diagrams from locally bundled assets in sandboxed Markdown pages.
- [x] Light/dark theme changes rerender diagrams; invalid diagrams and disabled JavaScript leave readable source.
- [x] Static builds bundle the scripts and dependency notices; shared-libs builds resolve the host scripts.
- [x] Browser security suite and Rust tests cover the paths.

## Resolution

### 2026-10-03T19:13:17Z · @jari

Shipped fenced Mermaid rendering in 0.19.0, with browser and build coverage.
