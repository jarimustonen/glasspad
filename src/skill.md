---
name: glasspad
description: Show visual dashboards, charts, and tables to the user. Use when asked to visualize, plot, chart, dashboard, or "show me" data.
---

# Glasspad — Visual Output

Render data as visual dashboards the user can view in their browser.
Write a YAML spec describing sections (charts, tables, stats) and pipe it to `glasspad create`.
The server starts automatically.

Run `glasspad docs` for full reference. Run `glasspad docs examples` for ready-to-use examples.

## Section types

- **chart** — bar, line, arc (pie). Uses Vega-Lite encoding: field + type (quantitative/nominal/ordinal/temporal).
- **table** — columns with field/title + data rows.
- **stats** — label/value KPI cards.

## Layouts

grid-2col (default), grid-3col, stack.
