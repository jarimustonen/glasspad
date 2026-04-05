pub fn print_index() {
    print!(
        r#"glasspad — AI scratchpad for rich data views

A localhost server that renders YAML dashboard specs as visual HTML pages.
AI agents describe what to show, glasspad handles how it looks.

Flow: agent writes YAML → pipes to glasspad → user opens URL in browser

USAGE
  glasspad serve              Start the server (default port 3000)
  glasspad create [--file F]  Create a pad from file or stdin
  glasspad list               List all pads
  glasspad open <id>          Open a pad in the browser

DOCS
  glasspad docs               This overview
  glasspad docs spec          YAML spec reference (structure, fields, types)
  glasspad docs sections      Section types: chart, table, stats
  glasspad docs charts        Chart marks: bar, arc, line + encoding
  glasspad docs examples      Complete example specs
  glasspad docs api           REST API endpoints

QUICK START
  1. Start the server:     glasspad serve
  2. Create a pad:         cat spec.yaml | glasspad create
  3. Open in browser:      glasspad open <id>
"#
    );
}

pub fn print_spec() {
    print!(
        r#"YAML SPEC REFERENCE

A glasspad YAML spec describes a dashboard with sections.

TOP-LEVEL FIELDS
  title:        string    (required) Dashboard title
  description:  string    (optional) Subtitle / description
  layout:       string    (optional) "grid-2col" (default), "grid-3col", "stack"
  sections:     list      (required) List of sections to render

SECTION FIELDS
  title:    string    (required) Section heading
  type:     string    (required) "chart", "table", or "stats"
  chart:    object    (required if type=chart) Chart specification
  columns:  list      (required if type=table) Column definitions
  data:     list      (required if type=table or stats) Data rows

CHART FIELDS
  mark:       string    (required) "bar", "line", or "arc"
  data:       list      (required) Array of data objects
  encoding:   object    (required) Vega-Lite encoding (x, y, theta, color, etc.)

ENCODING FIELDS (Vega-Lite)
  field:  string    Data field name
  type:   string    "quantitative", "nominal", "ordinal", "temporal"
  title:  string    Axis label (optional)
  sort:   string    Sort order, e.g. "-x" for descending (optional)
  scale:  object    Scale config, e.g. {{ domain: [0, 100] }} (optional)

COLUMN FIELDS (tables)
  field:  string    Data field name
  title:  string    Column header (optional, defaults to field name)
  width:  number    Column width in px (optional)

STATS DATA
  label:  string    Metric name
  value:  any       Metric value (number or string)
"#
    );
}

pub fn print_sections() {
    print!(
        r#"SECTION TYPES

chart
  Renders a Vega-Lite chart. Supports bar, line, and arc (pie) marks.
  The encoding maps data fields to visual channels (x, y, color, theta).

  - title: "Revenue per month"
    type: chart
    chart:
      mark: bar
      data:
        - {{ month: "Jan", revenue: 1200 }}
        - {{ month: "Feb", revenue: 1800 }}
      encoding:
        x: {{ field: month, type: nominal }}
        y: {{ field: revenue, type: quantitative }}

table
  Renders an HTML table with headers from columns and rows from data.

  - title: "Recent events"
    type: table
    columns:
      - {{ field: time, title: "When", width: 100 }}
      - {{ field: event, title: "Event" }}
    data:
      - {{ time: "2m ago", event: "Deploy completed" }}
      - {{ time: "1h ago", event: "Tests passed" }}

stats
  Renders KPI cards with large values and labels.

  - title: "Overview"
    type: stats
    data:
      - {{ label: "Users", value: 1234 }}
      - {{ label: "Uptime", value: "99.9%" }}
      - {{ label: "Errors", value: 3 }}
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
    data:
      - {{ category: "A", value: 10 }}
      - {{ category: "B", value: 25 }}
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
    data:
      - {{ date: "2026-04-01", temp: 12 }}
      - {{ date: "2026-04-02", temp: 15 }}
    encoding:
      x: {{ field: date, type: temporal }}
      y: {{ field: temp, type: quantitative, title: "°C" }}

arc
  Pie/donut chart. Use theta (size) and color (category).

  chart:
    mark: arc
    data:
      - {{ lang: "Rust", pct: 60 }}
      - {{ lang: "Python", pct: 30 }}
      - {{ lang: "Other", pct: 10 }}
    encoding:
      theta: {{ field: pct, type: quantitative }}
      color: {{ field: lang, type: nominal }}
"#
    );
}

pub fn print_examples() {
    print!(
        r#"EXAMPLES

Minimal dashboard (one stat card):

  title: "Build Status"
  sections:
    - title: "Status"
      type: stats
      data:
        - {{ label: "Result", value: "PASS" }}
        - {{ label: "Duration", value: "4m 32s" }}
        - {{ label: "Tests", value: 142 }}

---

Two charts side by side (grid-2col is default):

  title: "Sales Q1"
  sections:
    - title: "Monthly revenue"
      type: chart
      chart:
        mark: bar
        data:
          - {{ month: "Jan", revenue: 42000 }}
          - {{ month: "Feb", revenue: 51000 }}
          - {{ month: "Mar", revenue: 47000 }}
        encoding:
          x: {{ field: month, type: nominal }}
          y: {{ field: revenue, type: quantitative }}
    - title: "By region"
      type: chart
      chart:
        mark: arc
        data:
          - {{ region: "EU", sales: 55 }}
          - {{ region: "US", sales: 30 }}
          - {{ region: "APAC", sales: 15 }}
        encoding:
          theta: {{ field: sales, type: quantitative }}
          color: {{ field: region, type: nominal }}

---

Full dashboard (charts + table + stats):

  title: "Project Health"
  layout: grid-2col
  sections:
    - title: "Commits this week"
      type: chart
      chart:
        mark: bar
        data:
          - {{ day: "Mon", commits: 5 }}
          - {{ day: "Tue", commits: 12 }}
          - {{ day: "Wed", commits: 8 }}
        encoding:
          x: {{ field: day, type: ordinal }}
          y: {{ field: commits, type: quantitative }}
    - title: "Key metrics"
      type: stats
      data:
        - {{ label: "Open PRs", value: 7 }}
        - {{ label: "Coverage", value: "74%" }}
        - {{ label: "Build", value: "passing" }}
    - title: "Recent commits"
      type: table
      columns:
        - {{ field: hash, title: "Hash", width: 80 }}
        - {{ field: msg, title: "Message" }}
        - {{ field: author, title: "Author" }}
      data:
        - {{ hash: "a1b2c3d", msg: "feat: add dashboard", author: "Jari" }}
        - {{ hash: "d4e5f6g", msg: "fix: chart rendering", author: "Mika" }}

---

Pipe any of the above to create a pad:

  cat <<'YAML' | glasspad create
  title: "Build Status"
  sections:
    - title: "Status"
      type: stats
      data:
        - {{ label: "Result", value: "PASS" }}
  YAML
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
  Returns: {{ "id": "...", "url": "...", "title": "...", "created_at": "..." }}

GET /api/pads
  List all pads.
  Returns: [{{ "id", "title", "type", "url", "created_at" }}]

GET /api/pads/:id
  Get pad metadata.
  Returns: {{ "id", "title", "type", "url", "created_at" }}

PUT /api/pads/:id
  Update pad content. Send YAML spec as body.
  Content-Type: application/x-yaml
  Returns: 200 OK

DELETE /api/pads/:id
  Delete a pad.
  Returns: 204 No Content

GET /:id
  Render the pad as HTML in a browser.
  Returns: text/html
"#
    );
}
