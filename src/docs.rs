pub fn print_index() {
    print!(
        r#"glasspad — AI scratchpad for rich data views

A localhost server that renders YAML dashboard specs as visual HTML pages.
AI agents describe what to show, glasspad handles how it looks.

Flow: agent writes YAML → glasspad create → user opens URL in browser
      agent gets JSON (id, url, token) on stdout for subsequent operations

USAGE
  glasspad serve                        Start the server (default port 3000)
  glasspad create [--file F] [--data N=P]  Create a pad from file or stdin
  glasspad list                         List all pads
  glasspad open <id>                    Open a pad in the browser
  glasspad skill --install-claude       Install Claude Code skill

DOCS
  glasspad docs               This overview
  glasspad docs spec          YAML spec reference (structure, fields, types)
  glasspad docs sections      Section types: chart, table, stats, list, pivot
  glasspad docs charts        Chart marks: bar, arc, line + encoding
  glasspad docs examples      Complete example specs
  glasspad docs api           REST API endpoints

QUICK START
  1. glasspad create --file dashboard.yaml --data events=data.csv
  2. Open the URL from stdout JSON
"#
    );
}

pub fn print_spec() {
    print!(
        r#"YAML SPEC REFERENCE

A glasspad spec describes a dashboard. All specs require spec_version: 1.

TOP-LEVEL FIELDS
  spec_version: 1       (required) Schema version
  title:        string  (required) Dashboard title
  description:  string  (optional) Subtitle
  layout:       string  (optional) "grid-2col" (default), "grid-3col", "stack"
  datasets:     map     (optional) Named dataset declarations
  sections:     list    (required) Dashboard sections

DATASETS
  Declare datasets that will be provided via --data:
    datasets:
      events: {{}}

  Then reference in sections with source:
    source: events

DATA BINDING (two ways, mutually exclusive per section)
  source: events              Reference a named dataset
  inline_data:                Embed data directly in the spec
    - {{ x: 1, y: 2 }}

SECTION COMMON FIELDS
  id:      string    (required for interactive sections)
  title:   string    (required) Section heading
  type:    string    (required) "chart", "table", "stats", "list", "pivot"
  source:  string    (optional) Reference to datasets entry
  interactive_filter:          (optional, chart only)
    field: country             Field to filter on when chart is clicked

CHART CONFIG
  chart:
    mark:     string    (required) "bar", "line", or "arc"
    encoding: object    (required) Vega-Lite encoding

TABLE CONFIG
  table:
    columns:
      - {{ field: name, title: "Name", width: 100 }}
    row_id_field: id          (required if row_actions)
    row_actions:
      - {{ id: approve, label: "OK", style: success }}

STATS CONFIG (aggregation from dataset)
  stats:
    items:
      - {{ label: "Total", aggregate: count }}
      - {{ label: "Visits", aggregate: count, where: {{ event_type: visit }} }}
      - {{ label: "Countries", aggregate: distinct, field: country }}
      - {{ label: "Revenue", aggregate: sum, field: amount }}

  Supported aggregates: count, distinct, sum, avg, min, max

LIST CONFIG
  list:
    id_field: id              (required if actions or selectable)
    layout: cards             cards, rows, or compact
    title_field: subject
    subtitle_field: from
    meta_field: date
    preview_field: body_preview
    detail:
      body_field: body
      body_format: text       text (default) or sanitized_html
      actions:
        - {{ id: archive, label: "Archive" }}

PIVOT CONFIG
  pivot:
    rows:
      - region
      - product
    columns:
      - quarter
    values:
      - {{ field: revenue, aggregate: sum, label: "Revenue", format: currency, currency: USD }}
      - {{ field: orders, aggregate: count, label: "Orders" }}
    show_totals: true
    show_subtotals: true
    sort:
      by: value            "label" (default) or "value"
      direction: desc      "asc" (default) or "desc"
      value_index: 0       Which value to sort by (default: 0)

  Supported aggregates: sum, count, avg, min, max, distinct
  Value formats: currency (requires currency code), number, percent

ENCODING FIELDS (Vega-Lite)
  field:      string    Data field name
  type:       string    "quantitative", "nominal", "ordinal", "temporal"
  aggregate:  string    "count", "sum", etc. (optional)
  title:      string    Axis label (optional)
  sort:       string    e.g. "-x" for descending (optional)
  scale:      object    e.g. {{ domain: [0, 100] }} (optional)

COLUMN FIELDS
  field:  string    Data field name
  title:  string    Header text (optional, defaults to field)
  width:  number    Column width in px (optional)

UNKNOWN FIELDS ARE REJECTED — typos in field names produce parse errors.
"#
    );
}

pub fn print_sections() {
    print!(
        r#"SECTION TYPES

chart
  Renders a Vega-Lite chart. Supports bar, line, and arc (pie) marks.
  Data comes from source (external dataset) or inline_data.

  - id: by-country
    title: "Revenue per month"
    type: chart
    source: sales
    interactive_filter:
      field: country
    chart:
      mark: bar
      encoding:
        x: {{ field: month, type: nominal }}
        y: {{ field: revenue, type: quantitative }}

  With inline data:
  - title: "Quick chart"
    type: chart
    inline_data:
      - {{ x: "A", y: 10 }}
      - {{ x: "B", y: 20 }}
    chart:
      mark: bar
      encoding:
        x: {{ field: x, type: nominal }}
        y: {{ field: y, type: quantitative }}

table
  Renders an HTML table. Columns define headers, data comes from source or inline.

  - title: "Recent events"
    type: table
    source: events
    table:
      columns:
        - {{ field: time, title: "When", width: 100 }}
        - {{ field: event, title: "Event" }}

stats
  Renders KPI cards. Two modes:

  Aggregation mode (from dataset):
  - title: "Summary"
    type: stats
    source: events
    stats:
      items:
        - {{ label: "Total", aggregate: count }}
        - {{ label: "Countries", aggregate: distinct, field: country }}

  Inline mode (label/value pairs):
  - title: "Status"
    type: stats
    inline_data:
      - {{ label: "Build", value: "passing" }}
      - {{ label: "Tests", value: 142 }}

list
  Renders a scrollable list with optional detail view.

  - title: "Inbox"
    type: list
    source: emails
    list:
      id_field: id
      layout: cards
      title_field: subject
      subtitle_field: from
      detail:
        body_field: body
        body_format: text
        actions:
          - {{ id: archive, label: "Archive" }}

pivot
  Renders a 2D aggregation matrix. Rows and columns define grouping dimensions,
  values define what to aggregate. Supports multi-level row hierarchies with
  subtotals, grand totals, sorting, and currency/percent formatting.

  - title: "Revenue by Region and Quarter"
    type: pivot
    source: sales
    pivot:
      rows:
        - region
        - product
      columns:
        - quarter
      values:
        - {{ field: revenue, aggregate: sum, label: "Revenue", format: currency, currency: USD }}
        - {{ field: orders, aggregate: count, label: "Orders" }}
      show_totals: true
      show_subtotals: true
      sort:
        by: value
        direction: desc
        value_index: 0

  Simple pivot (no column dimension):
  - title: "Totals by Category"
    type: pivot
    source: data
    pivot:
      rows:
        - category
      values:
        - {{ field: amount, aggregate: sum }}
        - {{ field: amount, aggregate: avg }}

  Aggregates: sum, count, avg, min, max, distinct
  Value formats: currency (+ currency: USD/EUR/...), number, percent
"#
    );
}

pub fn print_charts() {
    print!(
        r#"CHART MARKS

bar
  Vertical bar chart. Use encoding x (nominal/ordinal/temporal) and y (quantitative).
  For horizontal bars, swap x and y in encoding.

  chart:
    mark: bar
    encoding:
      x: {{ field: category, type: nominal }}
      y: {{ field: value, type: quantitative }}

  Horizontal variant:
    encoding:
      x: {{ field: value, type: quantitative }}
      y: {{ field: category, type: nominal, sort: "-x" }}

line
  Line chart for trends. Use temporal or ordinal x-axis.

  chart:
    mark: line
    encoding:
      x: {{ field: date, type: temporal }}
      y: {{ field: temp, type: quantitative, title: "°C" }}

arc
  Pie/donut chart. Use theta (size) and color (category).

  chart:
    mark: arc
    encoding:
      theta: {{ field: pct, type: quantitative }}
      color: {{ field: lang, type: nominal }}
"#
    );
}

pub fn print_examples() {
    print!(
        r#"EXAMPLES

Minimal stats (inline data):

  spec_version: 1
  title: "Build Status"
  sections:
    - title: "Status"
      type: stats
      inline_data:
        - {{ label: "Result", value: "PASS" }}
        - {{ label: "Duration", value: "4m 32s" }}
        - {{ label: "Tests", value: 142 }}

---

Two charts with inline data:

  spec_version: 1
  title: "Sales Q1"
  sections:
    - title: "Monthly revenue"
      type: chart
      inline_data:
        - {{ month: "Jan", revenue: 42000 }}
        - {{ month: "Feb", revenue: 51000 }}
      chart:
        mark: bar
        encoding:
          x: {{ field: month, type: nominal }}
          y: {{ field: revenue, type: quantitative }}
    - title: "By region"
      type: chart
      inline_data:
        - {{ region: "EU", sales: 55 }}
        - {{ region: "US", sales: 30 }}
      chart:
        mark: arc
        encoding:
          theta: {{ field: sales, type: quantitative }}
          color: {{ field: region, type: nominal }}

---

Dashboard with external CSV data:

  spec_version: 1
  title: "Site Analytics"
  datasets:
    events: {{}}
  sections:
    - id: by-country
      title: "By country"
      type: chart
      source: events
      interactive_filter:
        field: country
      chart:
        mark: bar
        encoding:
          x: {{ field: country, type: nominal }}
          y: {{ aggregate: count, type: quantitative }}
    - id: summary
      title: "Summary"
      type: stats
      source: events
      stats:
        items:
          - {{ label: "Total events", aggregate: count }}
          - {{ label: "Countries", aggregate: distinct, field: country }}
    - id: all-events
      title: "All events"
      type: table
      source: events
      table:
        columns:
          - {{ field: datetime, title: "Time" }}
          - {{ field: path, title: "Page" }}
          - {{ field: country, title: "Country" }}

  CLI: glasspad create --file analytics.yaml --data events=events.csv

---

Pivot table with external data:

  spec_version: 1
  title: "Sales Analysis"
  datasets:
    sales: {{}}
  sections:
    - title: "Revenue by Region"
      type: pivot
      source: sales
      pivot:
        rows:
          - region
          - product_category
        columns:
          - segment
        values:
          - {{ field: total_amount, aggregate: sum, label: "Revenue", format: currency, currency: USD }}
          - {{ field: quantity, aggregate: sum, label: "Units" }}
        show_totals: true
        show_subtotals: true

  CLI: glasspad create --file sales.yaml --data sales=transactions.json

---

Create output is JSON on stdout:
  {{"id":"abc...","url":"http://localhost:3000/abc...","token":"def...","title":"..."}}
"#
    );
}

pub fn print_api() {
    print!(
        r#"REST API

All endpoints are on http://localhost:3000 (default).

POST /api/pads
  Create a pad. Send YAML spec as body.
  Content-Type: application/x-yaml
  Returns: {{ "id", "url", "title", "token", "created_at" }}

GET /api/pads
  List all pads.
  Returns: [{{ "id", "title", "type", "url", "created_at" }}]

GET /api/pads/:id
  Get pad metadata.
  Returns: {{ "id", "title", "type", "url", "created_at" }}

PUT /api/pads/:id
  Update pad content. Send YAML spec as body.
  Content-Type: application/x-yaml
  Header: X-Glasspad-Token: <token from create>
  Returns: 200 OK

DELETE /api/pads/:id
  Delete a pad.
  Header: X-Glasspad-Token: <token from create>
  Returns: 204 No Content

GET /:id
  Render the pad as HTML in a browser.
  Returns: text/html (with CSP headers)

AUTHENTICATION
  Create returns a token. Include it in X-Glasspad-Token header for
  PUT and DELETE operations. Tokens are 32 hex characters.
"#
    );
}
