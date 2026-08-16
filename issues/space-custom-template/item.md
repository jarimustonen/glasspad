---
created: 2026-08-14
updated: 2026-08-16
type: feature
reporter: jari
status: done
priority: normal
lane: space-polish
lane_seq: 10
commits:
- hash: 44299819d3a39fc043b0b586428d23261d60759c
  summary: 'feat: support custom templates in spaces'
- hash: b9be6f9b354b62a283e872152d29cf075a6eb791
  summary: 'fix: preserve branded template layout and limits'
closed: 2026-08-16
---

# Custom/branded template for a whole space (not just built-in prose/dashboard)

_Source: aggountant project-view visuals_

## Description

**Motivation:** aggountant's Project View (multi-page space published with `glasspad publish docs/site`) looks less polished than its previous bespoke docsite — the built-in prose theme's sidebar/TOC/prose styling is generic compared to the old editorial theme. A space is **locked to built-in template names**: `resolve_space_template` accepts only `prose`/`dashboard` (space.rs), and `--template <path>` custom templates apply only to single-file markdown publishes, not a directory space. So producers cannot brand/restyle a whole space from their repo. **Ask:** let a space declare a custom template (a repo file with a `{{content}}` slot, uploaded/inlined on publish) via `glasspad.yaml` `template:` or `.glasspad.yaml`, applied to every markdown page — so a project can carry its own visual identity while keeping the trusted shell/nav. If shell chrome (grouped sidebar, TOC rail) also needs theming hooks, note that here. Tracked in aggountant `project-view`.
