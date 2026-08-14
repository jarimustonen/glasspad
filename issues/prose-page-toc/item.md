---
created: 2026-08-14
updated: 2026-08-14
type: feature
reporter: jari
status: open
priority: normal
---

# Per-page TOC rail (on-this-page H2/H3 navigation) for prose spaces

_Source: aggountant docs/ port_

## Description

**Motivation:** aggountant's design docsite (port off build_docs.py → glasspad space, tracked in aggountant `project-view` epic) renders a right-hand per-page table of contents (H2/H3 of the current doc) alongside the left grouped nav. glasspad 0.8.0 has the grouped left sidebar (space-docsite-nav, done) but **no per-page TOC** — verified: no toc/on-this-page in src/artifact_host/shell.rs or base.css. Long spec pages (candidate.md ~80KB, decisions.md) are hard to navigate without it. **Ask:** prose template (and/or the shell) extracts the rendered page's H2/H3 and shows an 'on this page' rail (collapsible, hidden below a width breakpoint, like the grouped sidebar stacks). This is the last structural feature keeping build_docs.py alive for aggountant.
