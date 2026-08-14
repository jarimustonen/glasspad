---
created: 2026-08-14
updated: 2026-08-14
type: feature
reporter: jari
status: open
priority: normal
---

# Custom/branded template for a whole space (not just built-in prose/dashboard)

_Source: aggountant project-view visuals_

## Description

**Motivation:** aggountant's Project View (multi-page space published with `glasspad publish docs/site`) looks less polished than its previous bespoke docsite — the built-in prose theme's sidebar/TOC/prose styling is generic compared to the old editorial theme. A space is **locked to built-in template names**: `resolve_space_template` accepts only `prose`/`dashboard` (space.rs), and `--template <path>` custom templates apply only to single-file markdown publishes, not a directory space. So producers cannot brand/restyle a whole space from their repo. **Ask:** let a space declare a custom template (a repo file with a `{{content}}` slot, uploaded/inlined on publish) via `glasspad.yaml` `template:` or `.glasspad.yaml`, applied to every markdown page — so a project can carry its own visual identity while keeping the trusted shell/nav. If shell chrome (grouped sidebar, TOC rail) also needs theming hooks, note that here. Tracked in aggountant `project-view`.
