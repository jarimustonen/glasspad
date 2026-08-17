---
created: 2026-04-13
updated: 2026-07-23
type: task
reporter: maintainer
assignee: jari
status: obsolete
priority: high
slug: visual-test-patterns
closed: 2026-07-23
---

# Visual test patterns for comprehensive feature coverage

## Description

Create a set of curated test pad specs (YAML fixtures) that together cover all visual features of Glasspad. Each spec targets a specific view/section type with its configuration variants. When all specs render correctly in a browser, we have reasonable confidence that every feature works.

The test patterns serve two purposes:
1. **Manual QA** — open each pad in a browser, visually verify it looks correct
2. **Automation foundation** — once manual patterns are validated, automate with Playwright or similar

## Test Views to Cover

### Section types
- **Chart** — bar, line, arc marks; x/y/color/theta encodings; interactive_filter
- **Table** — columns with sort types; row_actions; selectable + batch_actions; row_id_field
- **Stats** — all aggregates (count, distinct, sum, avg, min, max); where filters
- **List** — cards/rows/compact layouts; detail view (fields, body_field, body_format text/html); item actions; selectable + batch_actions
- **Markdown** — inline content vs content_field; TOC sidebar (left/right, levels); link_target; code blocks; tables; blockquotes
- **Pivot** — row/column dimensions; value aggregates; format (currency, percent); show_totals/subtotals; sort (by label/value)

### Cross-cutting features
- **Themes** — light, dark, auto (each test view should look correct in all themes)
- **Layouts** — grid-2col, grid-3col, stack
- **Data sources** — CSV, JSON, inline_data
- **Responsive** — single-column collapse at <768px
- **Filters** — interactive filter widget interaction
- **Temporal data** — date fields, timezone handling

## Acceptance Criteria

- [ ] One or more test pad specs per section type covering its major variants
- [ ] A manifest or checklist documenting what each spec tests
- [ ] All specs render correctly when served locally and viewed in a browser
- [ ] Coverage is broad enough that Playwright automation would be meaningful

## Comments

This is the manual-first phase. Playwright automation will be a follow-up issue once the patterns are validated and stable.
