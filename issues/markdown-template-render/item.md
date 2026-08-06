---
created: 2026-08-06
updated: 2026-08-06
type: feature
status: done
priority: high
related: ['@prose-theme']
blocked_by: ['@prose-theme']
commits:
- hash: f197497
  summary: commit implementation plan; in-progress
- hash: cc49294
  summary: markdown + reusable-template render path (glasspad render)
- hash: '94564e4'
  summary: apply /llm-review findings (F1-F6)
closed: 2026-08-06
---

# Markdown + reusable-template render path

## Description

The headline feature: render **markdown into a referenced, reusable template**
and host the result.

**Decided model: server-side render; payload = `markdown + template reference`.**
The template is either a named built-in theme (`prose`/`dashboard`) or a
repo-local template file whose content the client ships (`--template ./x.html`).
Client-side render (ship final HTML) is rejected — it duplicates the renderer,
weakens single-source-of-truth, and gains no security (the server re-wraps the
body in the sandbox iframe regardless).

The template governs the **artifact body** (plugs into the existing `wrap.rs`
fragment-wrap seam), never the trusted parent shell — glasspad keeps sole control
of CSP / Trusted Types / nav / sandbox, so a custom template never widens the
security boundary.

Depends: prose-theme (default template). Part of the agent→browser-HTML consolidation. Full design + rationale: homebase `issues/glasspad-html-consolidation/design.md` (Option D). These features make glasspad the single canonical agent→HTML surface.
Ref: src/artifact_host/wrap.rs, shell.rs.
