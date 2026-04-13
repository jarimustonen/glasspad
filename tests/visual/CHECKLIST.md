# Visual Test Checklist

Manual QA guide for Glasspad visual test specs. Post each YAML to the API and verify rendering in a browser.

## chart-variants.yaml
- [ ] Bar chart renders with grouped bars by region, correct x/y axes
- [ ] Line chart shows temporal progression with multiple city series
- [ ] Arc chart displays pie/donut with category slices and legend
- [ ] Interactive filter on bar-revenue works (click region to filter)
- [ ] Interactive filter on bar-filtered-sales works (click product to filter)
- [ ] Color encoding distinguishes series correctly in all charts

## table-variants.yaml
- [ ] All columns render with correct titles and widths
- [ ] String sort works on Name and Department columns
- [ ] Number sort works on ID and Salary columns
- [ ] Temporal sort works on Hired column
- [ ] Boolean sort works on Active column
- [ ] Row action buttons appear (Promote/Transfer/Terminate)
- [ ] Row action styles render correctly (success=green, default, danger=red)
- [ ] Selectable checkboxes appear on each row
- [ ] Batch action bar appears when rows are selected
- [ ] 15 rows provide enough data to test scrolling

## stats-variants.yaml
- [ ] Count aggregate displays total row count
- [ ] Distinct aggregate shows unique customer count
- [ ] Sum aggregate shows correct revenue total
- [ ] Avg aggregate shows mean order value
- [ ] Min aggregate shows smallest order
- [ ] Max aggregate shows largest order
- [ ] Where-filtered counts match expected values (completed/pending/refunded)
- [ ] Where-filtered sum shows correct filtered revenue
- [ ] Website traffic section renders with different data shape

## list-variants.yaml
- [ ] Cards layout shows title/subtitle/meta/preview fields
- [ ] Clicking a card opens detail view with fields table
- [ ] Detail body_field renders as plain text
- [ ] Detail actions appear (Approve/Pause/Cancel) with correct styles
- [ ] Rows layout renders compact list items
- [ ] Compact layout renders minimal items
- [ ] HTML detail view renders sanitized HTML content
- [ ] Selectable checkboxes appear on knowledge base cards
- [ ] Batch actions (Archive/Publish) appear when cards are selected
- [ ] Detail actions in HTML section work (Edit/Delete)

## markdown-variants.yaml
- [ ] Inline content renders headings, paragraphs, bold, italic
- [ ] Code block renders with syntax highlighting (bash)
- [ ] Blockquote renders with styling
- [ ] Markdown table renders (API parameters, rate limits)
- [ ] TOC sidebar appears on the left side
- [ ] TOC includes h1, h2, h3 headings
- [ ] Links open in new tab (link_target: _blank)
- [ ] Horizontal rule renders
- [ ] Content from dataset field renders release notes
- [ ] Multiple data rows concatenate into continuous markdown

## pivot-variants.yaml
- [ ] Single row dimension with column dimension renders grid correctly
- [ ] Currency format shows USD values with proper formatting
- [ ] Count aggregate (no field) works in pivot values
- [ ] Sort by value (desc) orders rows by revenue
- [ ] Show totals displays row/column totals
- [ ] Multiple row dimensions create hierarchical row headers
- [ ] Show subtotals displays subtotal rows for each outer group
- [ ] EUR currency format renders correctly
- [ ] Percent format displays margin values as percentages
- [ ] Sort by label (asc) alphabetizes rows
- [ ] Min/max aggregates work in pivot values
- [ ] Multiple column dimension values render correctly (headcount table)
- [ ] Average aggregate in pivot values works

## layout-themes.yaml (grid-2col, light theme)
- [ ] Grid-2col layout shows 2 columns of sections
- [ ] Light theme applies light backgrounds and dark text
- [ ] Chart, stats, table, and markdown coexist visually
- [ ] Sections are evenly distributed in 2-column grid

## layout-dark.yaml (grid-3col, dark theme)
- [ ] Grid-3col layout shows 3 columns of sections
- [ ] Dark theme applies dark backgrounds and light text
- [ ] Chart colors are readable on dark background
- [ ] Table rows have sufficient contrast
- [ ] All 6 section types render in dark mode (chart, stats, table, list, markdown, arc chart)

## layout-stack.yaml (stack layout)
- [ ] Stack layout shows sections full-width, stacked vertically
- [ ] Chart spans full width
- [ ] Table spans full width
- [ ] Stats section spans full width
