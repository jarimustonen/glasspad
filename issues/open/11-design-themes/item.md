---
created: 2026-04-09
updated: 2026-04-09
type: feature
reporter: jari
assignee: jari
status: open
priority: normal
---

# 11. Design themes and visual identity

_Source: UI/UX_

## Description

Design one or more cohesive visual themes for Glasspad. Rather than inventing from scratch, leverage the DESIGN.md format from the [awesome-design-md](https://github.com/VoltAgent/awesome-design-md) collection — curated design system documents that AI agents can consume directly.

## Background: DESIGN.md Format

The awesome-design-md repo contains 58 ready-to-use `DESIGN.md` files extracted from real products. Each follows a standard 9-section structure:

1. Visual Theme & Atmosphere
2. Color Palette & Roles
3. Typography Rules
4. Component Stylings (buttons, cards, inputs, nav + states)
5. Layout Principles (spacing scale, grid, whitespace)
6. Depth & Elevation (shadows, surface hierarchy)
7. Do's and Don'ts
8. Responsive Behavior (breakpoints, touch targets)
9. Agent Prompt Guide (quick color ref, AI-ready prompts)

Each also ships `preview.html` / `preview-dark.html` for visual catalog.

## Candidate Inspirations

Relevant design systems for a developer/AI-facing tool like Glasspad:

| Design System | Why it fits |
|---------------|-------------|
| **Linear** | Ultra-minimal, precise, fast feel — suits a dev tool |
| **Vercel** | Black/white precision, Geist font, clean data display |
| **Supabase** | Dark emerald, code-first aesthetic |
| **Resend** | Minimal dark theme, monospace accents |
| **Notion** | Warm minimalism, soft surfaces, content-first |
| **Raycast** | macOS-native feel, command palette aesthetic |
| **PostHog** | Data-heavy dashboard design, good chart integration |

## Deliverables

- [ ] Evaluate 3-4 candidate design systems from awesome-design-md
- [ ] Create a `DESIGN.md` for Glasspad (either adapted from one reference or synthesized)
- [ ] Implement as CSS theme(s) — at minimum one dark and one light variant
- [ ] Component catalog page showing all UI elements in the theme
- [ ] Ensure themes work well with all content types (charts, tables, lists, markdown, chat)

## Approach

1. Pull relevant `DESIGN.md` files from the repo
2. Evaluate each against Glasspad's content types (data dashboards, email, markdown, chat, UML)
3. Synthesize a Glasspad-specific `DESIGN.md` — possibly blending Linear's precision with PostHog's data density
4. Implement as CSS custom properties for easy theme switching
