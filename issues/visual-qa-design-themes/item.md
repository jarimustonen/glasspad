---
created: 2026-04-11
updated: 2026-04-11
type: task
reporter: jari
assignee: jari
status: open
priority: normal
slug: highly-breezy-bedroom
---

# Visual QA for design themes

_Source: follow-up from **@design-themes** design themes_

## Description

Visually inspect all test datasets in both light and dark themes to verify rendering quality. This is the QA pass after the theme system was implemented in @design-themes.

## Scope

- [ ] Open each test dataset in the browser with light theme
- [ ] Open each test dataset in the browser with dark theme
- [ ] Verify charts (bar, line, area, point, arc) render with correct colors
- [ ] Verify tables (headers, rows, hover, sort indicators) look correct
- [ ] Verify stats cards are readable in both themes
- [ ] Verify list sections (cards, rows, compact, detail view) render well
- [ ] Verify markdown sections (headings, code blocks, blockquotes, tables, TOC) look correct
- [ ] Verify filter bar and filter tags are visible and functional
- [ ] Verify TOC sidebar styling in both themes
- [ ] Check highlight.js syntax highlighting in dark mode

## Test Datasets

- `global-co2-emissions.yaml`
- `project-management.yaml`
- `email-inbox.yaml`
- `iris-flowers.yaml`
- `titanic-survival.yaml`
- `restaurant-orders.yaml`
- `iot-sensor-readings.yaml`
- `bug-tracker.yaml`
- `gapminder-life-expectancy.yaml`
- `world-population.yaml`
- `software-architecture.yaml`
- `theme-showcase.yaml`
- `fixtures/markdown_showcase.yaml`
