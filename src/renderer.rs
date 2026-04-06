use std::collections::BTreeMap;

use crate::data::types::{CellValue, Dataset};
use crate::models::Pad;
use crate::security::json_embed::safe_json_script_tag;
use crate::spec::schema::{Layout, Section, SectionType};

pub fn render_dashboard(pad: &Pad) -> String {
    let spec = &pad.spec;

    let layout_class = match spec.layout {
        Layout::Grid3col => "dashboard-grid grid-3",
        Layout::Stack => "dashboard-stack",
        Layout::Grid2col => "dashboard-grid grid-2",
    };

    let mut chart_spec_tags = Vec::new(); // Safe JSON script tags for Vega specs
    let mut chart_init_scripts = Vec::new(); // JS init code referencing those tags
    let mut sections_html = String::new();

    for (i, section) in spec.sections.iter().enumerate() {
        let data = resolve_section_data(section, &pad.datasets);
        let section_html = render_section(
            section,
            i,
            data.as_deref(),
            &mut chart_spec_tags,
            &mut chart_init_scripts,
        );
        sections_html.push_str(&section_html);
    }

    let description_html = spec
        .description
        .as_ref()
        .map(|d| format!("<p class=\"description\">{}</p>", html_escape(d)))
        .unwrap_or_default();

    let spec_tags = chart_spec_tags.join("\n");
    let init_scripts = chart_init_scripts.join("\n");

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
      max-width: 1400px; margin: 0 auto; padding: 2rem;
      background: #f8f9fa; color: #1a1a2e;
    }}
    h1 {{ font-size: 1.75rem; font-weight: 700; margin-bottom: 0.25rem; }}
    .description {{ color: #6b7280; margin-bottom: 1.5rem; }}
    .dashboard-grid {{ display: grid; gap: 1.5rem; margin-top: 1.5rem; }}
    .grid-2 {{ grid-template-columns: repeat(2, 1fr); }}
    .grid-3 {{ grid-template-columns: repeat(3, 1fr); }}
    .dashboard-stack {{ display: flex; flex-direction: column; gap: 1.5rem; margin-top: 1.5rem; }}
    .section-card {{
      background: #fff; border: 1px solid #e5e7eb; border-radius: 12px;
      padding: 1.5rem; box-shadow: 0 1px 3px rgba(0,0,0,0.04);
    }}
    .section-card h3 {{
      font-size: 0.95rem; font-weight: 600; color: #374151;
      margin-bottom: 1rem; text-transform: uppercase; letter-spacing: 0.03em;
    }}
    .chart-container {{ width: 100%; }}
    .chart-container .vega-embed {{ width: 100%; }}
    table {{ width: 100%; border-collapse: collapse; font-size: 0.9rem; }}
    thead th {{
      text-align: left; padding: 0.6rem 0.75rem; border-bottom: 2px solid #e5e7eb;
      font-weight: 600; color: #6b7280; font-size: 0.8rem;
      text-transform: uppercase; letter-spacing: 0.04em;
    }}
    tbody td {{ padding: 0.55rem 0.75rem; border-bottom: 1px solid #f3f4f6; }}
    tbody tr:hover {{ background: #f9fafb; }}
    .stats-grid {{ display: grid; grid-template-columns: repeat(auto-fit, minmax(140px, 1fr)); gap: 1rem; }}
    .stat-card {{ text-align: center; padding: 1rem; background: #f9fafb; border-radius: 8px; }}
    .stat-value {{ font-size: 1.75rem; font-weight: 700; color: #1a1a2e; line-height: 1.2; }}
    .stat-label {{ font-size: 0.8rem; color: #6b7280; margin-top: 0.25rem; }}
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
  {spec_tags}
  <script>
    {init_scripts}
  </script>
</body>
</html>"#,
        title = html_escape(&spec.title),
        description = description_html,
        layout = layout_class,
        sections = sections_html,
        spec_tags = spec_tags,
        init_scripts = init_scripts,
    )
}

fn resolve_section_data<'a>(
    section: &'a Section,
    datasets: &'a BTreeMap<String, Dataset>,
) -> Option<&'a Dataset> {
    if let Some(ref source) = section.source {
        datasets.get(source.as_str())
    } else if let Some(ref data) = section.inline_data {
        Some(data)
    } else {
        None
    }
}

fn render_section(
    section: &Section,
    index: usize,
    data: Option<&Dataset>,
    spec_tags: &mut Vec<String>,
    init_scripts: &mut Vec<String>,
) -> String {
    let inner = match section.section_type {
        SectionType::Chart => render_chart_section(section, index, data, spec_tags, init_scripts),
        SectionType::Table => render_table_section(section, data),
        SectionType::Stats => render_stats_section(section, data),
        SectionType::List => "<p>List rendering not yet implemented</p>".to_string(),
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
    data: Option<&Dataset>,
    spec_tags: &mut Vec<String>,
    init_scripts: &mut Vec<String>,
) -> String {
    let chart = match &section.chart {
        Some(c) => c,
        None => return "<p>No chart spec provided</p>".to_string(),
    };

    let div_id = format!("vis-{}", index);
    let spec_id = format!("vis-spec-{}", index);

    let mark = if chart.mark == "arc" {
        serde_json::json!({"type": "arc", "tooltip": true})
    } else {
        serde_json::json!(chart.mark)
    };

    let data_values: Vec<serde_json::Value> = data
        .map(|d| d.iter().map(row_to_json).collect())
        .unwrap_or_default();

    let mut vl_spec = serde_json::json!({
        "$schema": "https://vega.github.io/schema/vega-lite/v5.json",
        "width": "container",
        "height": 300,
        "mark": mark,
        "data": { "values": data_values },
        "encoding": chart.encoding,
    });

    if chart.mark == "arc" {
        if let serde_json::Value::Object(ref mut obj) = vl_spec {
            obj.remove("width");
            obj.insert("height".to_string(), serde_json::json!(300));
            obj.insert("view".to_string(), serde_json::json!({"stroke": null}));
        }
    }

    // Embed Vega spec as safe non-executable JSON, then parse in JS
    spec_tags.push(safe_json_script_tag(&spec_id, &vl_spec));

    init_scripts.push(format!(
        "vegaEmbed('#{div_id}', JSON.parse(document.getElementById('{spec_id}').textContent), {{actions: false, renderer: 'svg'}}).catch(console.error);",
        div_id = div_id,
        spec_id = spec_id,
    ));

    format!("<div id=\"{}\" class=\"chart-container\"></div>", div_id)
}

fn render_table_section(section: &Section, data: Option<&Dataset>) -> String {
    let table = match &section.table {
        Some(t) => t,
        None => return "<p>No table config</p>".to_string(),
    };
    let data = match data {
        Some(d) => d,
        None => return "<p>No data</p>".to_string(),
    };

    let mut html = String::from("<table>\n<thead><tr>\n");

    for col in &table.columns {
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
        for col in &table.columns {
            let val = row
                .get(&col.field)
                .map(format_cell)
                .unwrap_or_default();
            html.push_str(&format!("  <td>{}</td>\n", html_escape(&val)));
        }
        html.push_str("</tr>\n");
    }

    html.push_str("</tbody>\n</table>");
    html
}

fn render_stats_section(section: &Section, data: Option<&Dataset>) -> String {
    // Aggregation mode (canonical)
    if let Some(ref stats_config) = section.stats {
        let data = match data {
            Some(d) => d,
            None => return "<p>No data for stats aggregation</p>".to_string(),
        };
        return render_aggregate_stats(stats_config, data);
    }

    // Fallback: inline label/value pairs (legacy)
    if let Some(ref inline) = section.inline_data {
        return render_inline_stats(inline);
    }

    "<p>No stats config or inline data</p>".to_string()
}

fn render_aggregate_stats(stats: &crate::spec::schema::StatsConfig, data: &Dataset) -> String {
    let mut html = String::from("<div class=\"stats-grid\">\n");

    for item in &stats.items {
        let value = compute_aggregate(&item.aggregate, item.field.as_deref(), &item.where_clause, data);
        html.push_str(&format!(
            "  <div class=\"stat-card\">\n    <div class=\"stat-value\">{}</div>\n    <div class=\"stat-label\">{}</div>\n  </div>\n",
            html_escape(&value),
            html_escape(&item.label),
        ));
    }

    html.push_str("</div>");
    html
}

fn render_inline_stats(data: &Dataset) -> String {
    let mut html = String::from("<div class=\"stats-grid\">\n");

    for row in data {
        let label = row.get("label").map(format_cell).unwrap_or_default();
        let value = row.get("value").map(format_cell).unwrap_or_default();

        html.push_str(&format!(
            "  <div class=\"stat-card\">\n    <div class=\"stat-value\">{}</div>\n    <div class=\"stat-label\">{}</div>\n  </div>\n",
            html_escape(&value),
            html_escape(&label),
        ));
    }

    html.push_str("</div>");
    html
}

fn compute_aggregate(
    aggregate: &str,
    field: Option<&str>,
    where_clause: &Option<BTreeMap<String, serde_json::Value>>,
    data: &Dataset,
) -> String {
    let filtered: Vec<_> = data
        .iter()
        .filter(|row| {
            if let Some(wc) = where_clause {
                wc.iter().all(|(k, v)| {
                    row.get(k)
                        .map(|cell| cell_matches_json(cell, v))
                        .unwrap_or(false)
                })
            } else {
                true
            }
        })
        .collect();

    match aggregate {
        "count" => format_number(filtered.len() as i64),
        "distinct" => {
            let field = match field {
                Some(f) => f,
                None => return "⚠ missing field".to_string(),
            };
            let mut seen = std::collections::HashSet::new();
            for row in &filtered {
                if let Some(v) = row.get(field) {
                    if !v.is_null() {
                        seen.insert(format_cell(v));
                    }
                }
            }
            format_number(seen.len() as i64)
        }
        "sum" => {
            let field = match field {
                Some(f) => f,
                None => return "⚠ missing field".to_string(),
            };
            let values: Vec<f64> = filtered
                .iter()
                .filter_map(|row| row.get(field).and_then(|v| v.as_f64()))
                .collect();
            if values.is_empty() {
                "—".to_string()
            } else {
                format_decimal(values.iter().sum())
            }
        }
        "avg" => {
            let field = match field {
                Some(f) => f,
                None => return "⚠ missing field".to_string(),
            };
            let values: Vec<f64> = filtered
                .iter()
                .filter_map(|row| row.get(field).and_then(|v| v.as_f64()))
                .collect();
            if values.is_empty() {
                "—".to_string()
            } else {
                format_decimal(values.iter().sum::<f64>() / values.len() as f64)
            }
        }
        "min" | "max" => {
            let field = match field {
                Some(f) => f,
                None => return "⚠ missing field".to_string(),
            };
            let values: Vec<f64> = filtered
                .iter()
                .filter_map(|row| row.get(field).and_then(|v| v.as_f64()))
                .collect();
            if values.is_empty() {
                "—".to_string()
            } else if aggregate == "min" {
                format_decimal(values.iter().cloned().fold(f64::INFINITY, f64::min))
            } else {
                format_decimal(values.iter().cloned().fold(f64::NEG_INFINITY, f64::max))
            }
        }
        other => format!("⚠ unknown aggregate: {}", other),
    }
}

fn cell_matches_json(cell: &CellValue, json_val: &serde_json::Value) -> bool {
    match (cell, json_val) {
        (CellValue::String(s), serde_json::Value::String(j)) => s == j,
        (CellValue::Number(n), serde_json::Value::Number(j)) => {
            j.as_f64().is_some_and(|jn| *n == jn)
        }
        (CellValue::Bool(b), serde_json::Value::Bool(j)) => b == j,
        (CellValue::Null, serde_json::Value::Null) => true,
        _ => false,
    }
}

/// Format a CellValue for display in tables and stats.
/// Consistent formatting: null→empty, bool→true/false, number→formatted, string→as-is.
fn format_cell(v: &CellValue) -> String {
    match v {
        CellValue::Null => String::new(),
        CellValue::Bool(b) => b.to_string(),
        CellValue::Number(n) => format_decimal(*n),
        CellValue::String(s) => s.clone(),
    }
}

fn row_to_json(row: &BTreeMap<String, CellValue>) -> serde_json::Value {
    let obj: serde_json::Map<String, serde_json::Value> = row
        .iter()
        .map(|(k, v)| (k.clone(), cell_to_json(v)))
        .collect();
    serde_json::Value::Object(obj)
}

fn cell_to_json(v: &CellValue) -> serde_json::Value {
    match v {
        CellValue::Null => serde_json::Value::Null,
        CellValue::Bool(b) => serde_json::Value::Bool(*b),
        CellValue::Number(n) => serde_json::json!(n),
        CellValue::String(s) => serde_json::Value::String(s.clone()),
    }
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

fn format_decimal(n: f64) -> String {
    if n.fract() == 0.0 && n >= i64::MIN as f64 && n <= i64::MAX as f64 {
        format_number(n as i64)
    } else {
        format!("{:.1}", n)
    }
}
