# Pivot Table Competitor Analysis

Competitor analysis of pivot table features across major products.
Conducted 2026-04-10 for issue **#17 Pivot table view**.

---

## 1. Microsoft Excel PivotTables

The gold standard. Introduced in 1994, defines the vocabulary everyone else copies.

### Core concepts

- **Rows** — fields placed on the vertical axis; each unique value becomes a row header
- **Columns** — fields on the horizontal axis; each unique value becomes a column header
- **Values** — the numeric (or count) data summarized in the cells at row/column intersections
- **Filters** — global filters applied to the entire pivot (also called "Report Filter" or "Slicer")
- A fifth zone, **Rows above Columns** nesting, allows multi-level hierarchies on both axes

### Cell function (aggregation)

- Called **"Summarize Values By"**
- Options: SUM, COUNT, AVERAGE, MAX, MIN, PRODUCT, COUNT NUMBERS, STDDEV, STDDEVP, VAR, VARP
- Secondary option: **"Show Values As"** — % of Grand Total, % of Column Total, % of Row Total, Running Total, Rank, Difference From, % Difference From, Index
- Default: SUM for numeric fields, COUNT for text/date fields

### Interactive features

- **Drag-and-drop field list** — four drop zones (Filters, Columns, Rows, Values) in a panel; drag fields between zones to restructure
- **Expand/collapse** — click +/- on grouped row/column headers to show/hide detail
- **Drill-down** — double-click a cell to create a new sheet with the underlying records
- **Grouping** — right-click to group dates (by month/quarter/year), numbers (by range), or custom sets
- **Sorting** — sort rows/columns by label or by value (e.g., sort products by total revenue descending)
- **Slicers** — floating visual filter buttons (introduced Excel 2010); multi-select, connected to one or more pivots
- **Timelines** — date-range slider for date fields
- **Calculated fields** — user-defined formulas that create new virtual fields in the value area (e.g., `Profit = Revenue - Cost`)
- **Calculated items** — custom formulas within a field (e.g., "Q1 = Jan + Feb + Mar")
- **Conditional formatting** — color scales, data bars, icon sets applied to pivot cells
- **Refresh** — manual or auto-refresh when data source changes
- **Pivot Charts** — chart directly linked to a PivotTable; changes to one update the other

### Row/column selection UI

- Field List panel on the right side
- Checkboxes to add a field; it auto-places (text to Rows, numbers to Values)
- Drag fields between the four zones to rearrange
- Drag within a zone to reorder nesting hierarchy

### Standout features

- **Show Values As** — the secondary calculation layer (running totals, % of parent, rank) is extremely powerful and often overlooked
- **GetPivotData** — spreadsheet function to extract specific pivot cell values into other cells
- **OLAP connectivity** — can connect to Analysis Services cubes with server-side aggregation
- **Power Pivot / Data Model** — in-memory columnar engine enabling millions of rows, DAX measures, and relationships between tables

---

## 2. Microsoft Power BI (Matrix Visual)

Power BI's pivot equivalent is the **Matrix visual**. Different philosophy: analysis model is defined in the data layer (DAX), not in the visual.

### Core concepts

- **Rows** — dimension fields on the vertical axis
- **Columns** — dimension fields on the horizontal axis
- **Values** — measures or aggregated fields in the cells
- No explicit "Filters" zone on the visual itself; filtering is done via **Slicers** (separate visuals), **Visual-level filters**, **Page-level filters**, and **Report-level filters** in the Filters pane

### Cell function (aggregation)

- Called **"Measures"** (DAX expressions) or implicit aggregation
- Implicit: SUM, AVERAGE, MIN, MAX, COUNT, DISTINCTCOUNT, MEDIAN, VARIANCE, STDEV
- Explicit: any DAX formula — `CALCULATE`, `SUMX`, `AVERAGEX`, time intelligence functions, etc.
- DAX measures are far more powerful than Excel calculated fields; they support context transition, filter propagation, and complex business logic

### Interactive features

- **Drag-and-drop** — field well with Rows, Columns, Values buckets in the Visualizations pane
- **Expand/collapse** — row and column hierarchies can be expanded one level at a time or all at once via +/- buttons
- **Drill-down / Drill-up** — toolbar buttons to navigate hierarchy levels; right-click to drill into a specific member
- **Stepped layout vs. tabular** — rows can show hierarchy indented (stepped) or in separate columns (tabular)
- **Conditional formatting** — background color, font color, data bars, icons, web URLs; can be driven by DAX rules
- **Subtotals and grand totals** — configurable per row/column level; can use different measures for totals
- **Cross-filtering** — clicking a cell in the matrix filters other visuals on the page
- **Tooltips** — hover to see additional measures or even an entire report page as a tooltip
- **Export** — export underlying data or summary to CSV/Excel

### Row/column selection UI

- Fields pane on the right lists all tables/columns
- Drag fields into Rows, Columns, or Values wells
- Hierarchy fields can be dragged as a group
- No checkbox auto-placement like Excel

### Standout features

- **DAX measures** — the aggregation logic lives in the data model, not the visual; one measure works across all visuals
- **Row-level security (RLS)** — the same matrix can show different data to different users
- **Composite models** — matrix can pull from multiple data sources (DirectQuery + Import) simultaneously
- **Smart narratives** — AI-generated text summaries of what the matrix shows
- **Key difference from Excel**: the visual is a "renderer" — all intelligence is in the DAX model

---

## 3. Google Sheets Pivot Tables

Simplified, web-native pivot tables. Intentionally less powerful than Excel but more approachable.

### Core concepts

- **Rows** — fields on the vertical axis
- **Columns** — fields on the horizontal axis
- **Values** — aggregated numeric data in cells
- **Filters** — field-level filters that restrict which records are included
- Same four-zone model as Excel

### Cell function (aggregation)

- Called **"Summarize by"**
- Options: SUM, COUNTA, COUNT, COUNTUNIQUE, AVERAGE, MAX, MIN, MEDIAN, PRODUCT, STDEV, STDEVP, VAR, VARP, Custom formula
- Secondary: **"Show as"** — Default, % of Row, % of Column, % of Grand Total
- Fewer "Show as" options than Excel (no running total, no rank, no difference from)

### Interactive features

- **Pivot table editor panel** — side panel with Add buttons for Rows, Columns, Values, Filters
- **Drag to reorder** — drag fields within a zone to change nesting order
- **Sorting** — sort rows by label (ascending/descending) or by a value column
- **Grouping** — right-click to group dates or numbers into ranges
- **Calculated fields** — basic support via custom formulas in the Values section
- **Filter conditions** — per-field filtering (equals, contains, greater than, etc.) or manual item selection
- **Auto-refresh** — pivot updates automatically when source data changes
- **Suggested pivots** — Google suggests pivot configurations when you first create one (AI-assisted)

### Row/column selection UI

- Side panel with four sections: Rows, Columns, Values, Filters
- Click "Add" in any section and pick a field from a dropdown
- Drag field pills to reorder within a section
- No drag between sections — must remove and re-add to move a field from Rows to Columns

### Standout features

- **Suggested pivot tables** — when creating a new pivot, Google Sheets analyzes data and suggests useful configurations
- **GETPIVOTDATA function** — same as Excel, reference specific pivot cells from other cells
- **Collaboration** — real-time multi-user editing of the same pivot table
- **Connected Sheets** — can create pivot tables over BigQuery datasets (millions of rows) directly in Sheets
- **Simplicity** — lower learning curve; fewer options means less confusion

---

## 4. Tableau (Cross-Tab / Pivot)

Tableau calls it a **text table** or **cross-tab**. Tableau's core paradigm is visual analytics; the pivot/crosstab is one of many visualization types.

### Core concepts

- **Rows shelf** — fields that define the vertical structure
- **Columns shelf** — fields that define the horizontal structure
- **Marks card** — controls what appears in each cell (text, color, size, shape, detail, tooltip)
- **Filters shelf** — fields used to filter the view
- **Pages shelf** — creates an animation/pagination dimension
- The Rows/Columns shelves accept both dimensions (categorical) and measures (numeric)

### Cell function (aggregation)

- Called **"Aggregation"** or **"Measure"**
- Options: SUM, AVG, MEDIAN, COUNT, COUNTD (distinct), MIN, MAX, PERCENTILE, STDEV, STDEVP, VAR, VARP, ATTR (returns value if unique, * if not)
- **Table calculations** — secondary calculations applied after initial aggregation: running total, percent of total, moving average, rank, percentile, difference, percent difference, YTD total, compound growth rate, etc.
- **LOD expressions** — FIXED, INCLUDE, EXCLUDE expressions that control the level of detail for aggregation (e.g., `{FIXED [Customer] : SUM([Sales])}`)

### Interactive features

- **Drag-and-drop** — drag dimensions/measures from the data pane to Rows, Columns, Marks, or Filters shelves
- **Drill-down** — click +/- on hierarchy fields in row/column headers to expand/collapse levels
- **Show Me panel** — recommends chart types based on selected fields; cross-tab is one option
- **Sort** — click header to sort; sort by field, manual, or nested sort
- **Totals** — grand totals and subtotals via Analysis menu; can be placed at top or bottom
- **Reference lines / bands** — add statistical reference lines to crosstab
- **Highlight actions** — clicking a cell can highlight related data across multiple sheets in a dashboard
- **Sets** — create named subsets of dimension members for reuse across views
- **Parameters** — user-input values that can drive calculations, filters, or reference lines
- **Dashboard actions** — clicking a crosstab cell can filter, highlight, or navigate to other sheets/URLs

### Row/column selection UI

- Data pane on the left lists dimensions (blue) and measures (green)
- Drag fields to Rows or Columns shelf at the top of the canvas
- Order on the shelf determines nesting (left = outermost)
- Right-click a field pill to change aggregation, convert to discrete/continuous, or add table calculation
- **Swap button** — one-click swap Rows and Columns

### Standout features

- **LOD expressions** — level-of-detail calculations are unique to Tableau; they decouple aggregation granularity from the visual layout
- **Table calculations** — post-aggregation calculations are very flexible (addressing/partitioning)
- **Dual-axis / combined marks** — can overlay multiple mark types in a single crosstab
- **Visual cohesion** — a crosstab and a chart share the same underlying view; toggle between them instantly
- **Performance** — Hyper engine and live connections handle very large datasets efficiently

---

## 5. Looker / Metabase

Modern BI tools with different pivot philosophies. Looker is model-driven; Metabase is self-serve.

### Looker

#### Core concepts

- **Dimensions** — categorical fields (go on rows or columns, called "pivot" when on columns)
- **Measures** — pre-defined aggregations in LookML (the modeling layer)
- **Pivoting** — selecting a dimension and clicking "Pivot" moves it to the column axis
- **Filters** — filter bar at the top of the explore view
- No four-zone panel; pivoting is a property toggled on a dimension

#### Cell function (aggregation)

- Called **"Measures"** — defined in LookML, not by the end user
- Types: sum, count, count_distinct, average, min, max, median, percentile, list, sum_distinct, average_distinct, and custom SQL
- **Table calculations** — user-defined expressions in the results table (Looker expression syntax or custom SQL); applied after query results return
- **Derived tables** — SQL-defined tables in LookML for complex aggregations

#### Interactive features

- **Explore interface** — select dimensions and measures from a field picker; toggle "Pivot" on a dimension
- **Column sort** — click column headers to sort
- **Row totals / column totals** — toggle on/off; supports subtotals
- **Table calculations** — add calculated columns to results using offset functions, running totals, % of total, etc.
- **Drill-down** — click a cell to see underlying records or linked explores
- **Conditional formatting** — color rules on cells
- **Merge results** — combine queries from different explores into a single table
- **Download** — CSV, Excel, PDF, PNG, JSON

#### Row/column selection UI

- Field picker on the left organized by LookML view (table)
- Click a dimension/measure to add it as a column
- Click the "Pivot" button on a dimension to move it to column headers
- Drag columns to reorder in the results table
- Only one (or sometimes two) dimensions can be pivoted at a time

#### Standout features

- **LookML-defined metrics** — aggregations are governed; end users cannot define arbitrary SUMs, preventing metric inconsistency
- **Persistent derived tables (PDTs)** — materialized aggregation tables for performance
- **Git-versioned model** — LookML is version-controlled; changes go through code review
- **Row limit awareness** — Looker warns when results are truncated; pivots respect row limits

---

### Metabase

#### Core concepts

- **Rows (Group By)** — dimensions that define row groupings
- **Columns (Pivot column)** — a dimension whose values become column headers (called "pivot column")
- **Values** — aggregated measures displayed in cells
- **Filters** — filter widgets at the top of the question
- Simpler model: one dimension can be designated as the pivot column

#### Cell function (aggregation)

- Called **"Summarize"**
- Options: Count, Sum, Average, Min, Max, Distinct values, Cumulative sum, Cumulative count, Standard deviation
- **Custom expressions** — basic formula support for derived columns (e.g., `[Total] / [Count]`)
- **Custom columns** — add calculated columns before aggregation

#### Interactive features

- **Visual query builder** — no-code interface: pick table, add filters, choose summarize (aggregation), pick group-by fields
- **Pivot table toggle** — when you have two group-by dimensions, Metabase can display results as a pivot table
- **Column sort** — click headers to sort
- **Totals** — row and column totals with a toggle
- **Click actions** — click a cell to filter, zoom in (drill), or see underlying records
- **Column formatting** — number format, currency, percentage, conditional coloring
- **Dashboard filters** — filter widgets on dashboards that update multiple cards including pivots
- **SQL mode** — write raw SQL; results can still be displayed as a pivot

#### Row/column selection UI

- "Summarize" sidebar: choose metrics (Count of..., Sum of...) and "Group by" fields
- When two group-by fields exist, the results table has a "Pivot" toggle
- First group-by becomes rows, second becomes columns (or user can swap)
- No drag-and-drop; field selection is via dropdowns and clicks

#### Standout features

- **Zero-config pivot** — Metabase auto-detects when a pivot view makes sense and offers it
- **Embedding** — pivots can be embedded in other apps via iframe with signed tokens
- **Open source** — self-hostable, no license cost for core features
- **Simplicity** — intentionally minimal; targets non-technical users who would never use Excel pivots

---

## Summary Comparison

| Feature | Excel | Power BI | Google Sheets | Tableau | Looker | Metabase |
|---|---|---|---|---|---|---|
| **Aggregation term** | Summarize Values By | Measure (DAX) | Summarize by | Aggregation | Measure (LookML) | Summarize |
| **Drag-and-drop zones** | 4 zones | 3 wells | 4 zones (no drag between) | Shelves | Field picker + pivot toggle | Dropdowns |
| **Calculated fields** | Yes (limited) | DAX (powerful) | Basic | LOD + Table Calcs | LookML + Table Calcs | Basic expressions |
| **Show Values As** | 11 options | Via DAX | 4 options | Table calcs | Table calcs | Cumulative only |
| **Expand/collapse** | Yes | Yes | No | Yes (hierarchies) | No | No |
| **Drill-down** | Double-click | Button + right-click | No | Click +/- | Click cell | Click cell |
| **Slicers / visual filters** | Slicers + Timelines | Slicer visuals | Filter conditions | Parameters + filters | Filter bar | Dashboard filters |
| **Real-time collab** | Limited (365) | Workspace sharing | Yes (native) | Tableau Server | Yes | Yes |
| **Max data scale** | ~1M rows (Power Pivot: 100M+) | 100M+ (DirectQuery: unlimited) | 10M (Connected Sheets: unlimited) | 100M+ | DB-dependent | DB-dependent |
| **Governed metrics** | No | Partial (measures) | No | No | Yes (LookML) | No |

## Key Takeaways for Glasspad

1. **Four-zone model is universal** — Rows, Columns, Values, Filters. This is the expected mental model.
2. **Aggregation is the core** — SUM, COUNT, AVG, MIN, MAX cover 90% of use cases. "Show Values As" (% of total, running total) covers the next 8%.
3. **Expand/collapse on row hierarchies** is expected in desktop tools but often absent in web tools.
4. **Sorting by value** (not just label) is important — "show me top products by revenue."
5. **Subtotals and grand totals** are expected; they should be toggleable.
6. **Conditional formatting** on cells is a differentiator for readability.
7. **For an API-driven tool**: the spec should define rows, columns, values (with aggregation function), and filters declaratively — similar to how Vega-Lite defines encodings.
