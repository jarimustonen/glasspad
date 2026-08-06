---
created: 2026-08-06
updated: 2026-08-06
type: task
status: done
priority: normal
related: ['@hosted-share-server']
commits:
- hash: 41fa81c
  summary: add mode-routing guidance to src/skill.md (serve/render/publish/preview/build)
closed: 2026-08-06
---

# Skill: routing guidance (serve vs publish vs preview)

## Description

Expand the glasspad skill (`src/skill.md`, ships via `glasspad skill install`)
with routing guidance: when to use loopback `serve` (local interactive) vs
`glasspad publish` (hosted/shareable, md+template) vs an external seat preview.

Depends: markdown-template-render, hosted-share-server. Part of the agent→browser-HTML consolidation. Full design + rationale: homebase `issues/glasspad-html-consolidation/design.md` (Option D). These features make glasspad the single canonical agent→HTML surface.
Ref: src/skill.md.
