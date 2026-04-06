use std::collections::BTreeMap;

use crate::data::types::{CellValue, Dataset};
use crate::models::Pad;
use crate::security::json_embed::safe_json_script_tag;
use crate::spec::schema::Layout;

const CSS: &str = include_str!("client/dashboard.css");
const CLIENT_JS: &str = include_str!("client/dashboard.js");

/// Render a complete HTML page for a pad.
/// Server generates only the shell — all section rendering happens client-side.
pub fn render_dashboard(pad: &Pad) -> String {
    let spec = &pad.spec;

    let layout_class = match spec.layout {
        Layout::Grid3col => "dashboard-grid grid-3",
        Layout::Stack => "dashboard-stack",
        Layout::Grid2col => "dashboard-grid grid-2",
    };

    let description_html = spec
        .description
        .as_ref()
        .map(|d| format!("<p class=\"description\">{}</p>", html_escape(d)))
        .unwrap_or_default();

    let spec_json = match serde_json::to_value(spec) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Spec serialization failed: {}", e);
            return format!(
                "<!DOCTYPE html><html><body><p>Spec serialization error: {}</p></body></html>",
                html_escape(&e.to_string())
            );
        }
    };
    let spec_tag = safe_json_script_tag("glasspad-spec", &spec_json);

    let datasets_json = datasets_to_json(&pad.datasets);
    let data_tag = safe_json_script_tag("glasspad-data", &datasets_json);

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
  <style>{css}</style>
</head>
<body>
  <h1>{title}</h1>
  {description}
  <div id="dashboard" class="{layout}"></div>
  {spec_tag}
  {data_tag}
  <script>{js}</script>
</body>
</html>"#,
        title = html_escape(&spec.title),
        description = description_html,
        layout = layout_class,
        spec_tag = spec_tag,
        data_tag = data_tag,
        css = CSS,
        js = CLIENT_JS,
    )
}

fn datasets_to_json(datasets: &BTreeMap<String, Dataset>) -> serde_json::Value {
    let obj: serde_json::Map<String, serde_json::Value> = datasets
        .iter()
        .map(|(name, data)| {
            let rows: Vec<serde_json::Value> = data
                .iter()
                .map(|row| {
                    let obj: serde_json::Map<String, serde_json::Value> = row
                        .iter()
                        .map(|(k, v)| (k.clone(), cell_to_json(v)))
                        .collect();
                    serde_json::Value::Object(obj)
                })
                .collect();
            (name.clone(), serde_json::Value::Array(rows))
        })
        .collect();
    serde_json::Value::Object(obj)
}

fn cell_to_json(v: &CellValue) -> serde_json::Value {
    match v {
        CellValue::Null => serde_json::Value::Null,
        CellValue::Bool(b) => serde_json::Value::Bool(*b),
        CellValue::Number(n) if n.is_finite() => serde_json::json!(n),
        CellValue::Number(_) => serde_json::Value::Null, // NaN/Infinity → null
        CellValue::String(s) => serde_json::Value::String(s.clone()),
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
