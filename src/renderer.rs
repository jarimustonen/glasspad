use crate::models::{DashboardSpec, Section};

pub fn render_dashboard(spec: &DashboardSpec) -> String {
    let layout_class = match spec.layout.as_deref() {
        Some("grid-3col") => "dashboard-grid grid-3",
        Some("stack") => "dashboard-stack",
        _ => "dashboard-grid grid-2",
    };

    let mut chart_scripts = Vec::new();
    let mut sections_html = String::new();

    for (i, section) in spec.sections.iter().enumerate() {
        let section_html = render_section(section, i, &mut chart_scripts);
        sections_html.push_str(&section_html);
    }

    let description_html = spec
        .description
        .as_ref()
        .map(|d| format!("<p class=\"description\">{}</p>", html_escape(d)))
        .unwrap_or_default();

    let scripts = chart_scripts.join("\n");

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>{title}</title>
  <script src="https://cdn.jsdelivr.net/npm/vega@5"></script>
  <script src="https://cdn.jsdelivr.net/npm/vega-lite@5"></script>
  <script src="https://cdn.jsdelivr.net/npm/vega-embed@6"></script>
  <style>
    * {{ box-sizing: border-box; margin: 0; padding: 0; }}
    body {{
      font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', system-ui, sans-serif;
      max-width: 1400px;
      margin: 0 auto;
      padding: 2rem;
      background: #f8f9fa;
      color: #1a1a2e;
    }}
    h1 {{
      font-size: 1.75rem;
      font-weight: 700;
      margin-bottom: 0.25rem;
    }}
    .description {{
      color: #6b7280;
      margin-bottom: 1.5rem;
    }}
    .dashboard-grid {{
      display: grid;
      gap: 1.5rem;
      margin-top: 1.5rem;
    }}
    .grid-2 {{ grid-template-columns: repeat(2, 1fr); }}
    .grid-3 {{ grid-template-columns: repeat(3, 1fr); }}
    .dashboard-stack {{
      display: flex;
      flex-direction: column;
      gap: 1.5rem;
      margin-top: 1.5rem;
    }}
    .section-card {{
      background: #fff;
      border: 1px solid #e5e7eb;
      border-radius: 12px;
      padding: 1.5rem;
      box-shadow: 0 1px 3px rgba(0,0,0,0.04);
    }}
    .section-card h3 {{
      font-size: 0.95rem;
      font-weight: 600;
      color: #374151;
      margin-bottom: 1rem;
      text-transform: uppercase;
      letter-spacing: 0.03em;
    }}
    .chart-container {{
      width: 100%;
    }}
    .chart-container .vega-embed {{
      width: 100%;
    }}

    /* Tables */
    table {{
      width: 100%;
      border-collapse: collapse;
      font-size: 0.9rem;
    }}
    thead th {{
      text-align: left;
      padding: 0.6rem 0.75rem;
      border-bottom: 2px solid #e5e7eb;
      font-weight: 600;
      color: #6b7280;
      font-size: 0.8rem;
      text-transform: uppercase;
      letter-spacing: 0.04em;
    }}
    tbody td {{
      padding: 0.55rem 0.75rem;
      border-bottom: 1px solid #f3f4f6;
    }}
    tbody tr:hover {{
      background: #f9fafb;
    }}

    /* Stats */
    .stats-grid {{
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(140px, 1fr));
      gap: 1rem;
    }}
    .stat-card {{
      text-align: center;
      padding: 1rem;
      background: #f9fafb;
      border-radius: 8px;
    }}
    .stat-value {{
      font-size: 1.75rem;
      font-weight: 700;
      color: #1a1a2e;
      line-height: 1.2;
    }}
    .stat-label {{
      font-size: 0.8rem;
      color: #6b7280;
      margin-top: 0.25rem;
    }}

    @media (max-width: 768px) {{
      .grid-2, .grid-3 {{ grid-template-columns: 1fr; }}
      body {{ padding: 1rem; }}
    }}
  </style>
</head>
<body>
  <h1>{title}</h1>
  {description}
  <div class="{layout}">
    {sections}
  </div>
  <script>
    {scripts}
  </script>
</body>
</html>"#,
        title = html_escape(&spec.title),
        description = description_html,
        layout = layout_class,
        sections = sections_html,
        scripts = scripts,
    )
}

fn render_section(section: &Section, index: usize, chart_scripts: &mut Vec<String>) -> String {
    let inner = match section.section_type.as_str() {
        "chart" => render_chart_section(section, index, chart_scripts),
        "table" => render_table_section(section),
        "stats" => render_stats_section(section),
        _ => format!("<p>Unknown section type: {}</p>", section.section_type),
    };

    format!(
        "<div class=\"section-card\">\n  <h3>{}</h3>\n  {}\n</div>\n",
        html_escape(&section.title),
        inner
    )
}

fn render_chart_section(
    section: &Section,
    index: usize,
    chart_scripts: &mut Vec<String>,
) -> String {
    let chart = match &section.chart {
        Some(c) => c,
        None => return "<p>No chart spec provided</p>".to_string(),
    };

    let div_id = format!("vis-{}", index);

    // Build Vega-Lite spec
    let mark = if chart.mark == "arc" {
        serde_json::json!({"type": "arc", "tooltip": true})
    } else {
        serde_json::json!(chart.mark)
    };

    let encoding = chart.encoding.clone();

    let mut vl_spec = serde_json::json!({
        "$schema": "https://vega.github.io/schema/vega-lite/v5.json",
        "width": "container",
        "height": 300,
        "mark": mark,
        "data": { "values": chart.data },
        "encoding": encoding,
    });

    // For arc/pie charts, remove width/height (they auto-size)
    if chart.mark == "arc" {
        if let serde_json::Value::Object(ref mut obj) = vl_spec {
            obj.remove("width");
            obj.insert("height".to_string(), serde_json::json!(300));
            obj.insert(
                "view".to_string(),
                serde_json::json!({"stroke": null}),
            );
        }
    }

    let spec_json = serde_json::to_string(&vl_spec).unwrap_or_default();

    chart_scripts.push(format!(
        "vegaEmbed('#{div_id}', {spec}, {{actions: false, renderer: 'svg'}}).catch(console.error);",
        div_id = div_id,
        spec = spec_json,
    ));

    format!("<div id=\"{}\" class=\"chart-container\"></div>", div_id)
}

fn render_table_section(section: &Section) -> String {
    let columns = match &section.columns {
        Some(c) => c,
        None => return "<p>No columns defined</p>".to_string(),
    };
    let data = match &section.data {
        Some(d) => d,
        None => return "<p>No data</p>".to_string(),
    };

    let mut html = String::from("<table>\n<thead><tr>\n");

    for col in columns {
        let title = col.title.as_deref().unwrap_or(&col.field);
        let style = col
            .width
            .map(|w| format!(" style=\"width: {}px\"", w))
            .unwrap_or_default();
        html.push_str(&format!("  <th{}>{}</th>\n", style, html_escape(title)));
    }

    html.push_str("</tr></thead>\n<tbody>\n");

    for row in data {
        html.push_str("<tr>\n");
        for col in columns {
            let val = row
                .get(&col.field)
                .map(|v| match v {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                })
                .unwrap_or_default();
            html.push_str(&format!("  <td>{}</td>\n", html_escape(&val)));
        }
        html.push_str("</tr>\n");
    }

    html.push_str("</tbody>\n</table>");
    html
}

fn render_stats_section(section: &Section) -> String {
    let data = match &section.data {
        Some(d) => d,
        None => return "<p>No data</p>".to_string(),
    };

    let mut html = String::from("<div class=\"stats-grid\">\n");

    for item in data {
        let label = item
            .get("label")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let value = item
            .get("value")
            .map(|v| match v {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Number(n) => {
                    // Format numbers with thousand separators
                    if let Some(i) = n.as_i64() {
                        format_number(i)
                    } else {
                        n.to_string()
                    }
                }
                other => other.to_string(),
            })
            .unwrap_or_default();

        html.push_str(&format!(
            "  <div class=\"stat-card\">\n    <div class=\"stat-value\">{}</div>\n    <div class=\"stat-label\">{}</div>\n  </div>\n",
            html_escape(&value),
            html_escape(label),
        ));
    }

    html.push_str("</div>");
    html
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn format_number(n: i64) -> String {
    let s = n.abs().to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    if n < 0 {
        result.push('-');
    }
    result.chars().rev().collect()
}
