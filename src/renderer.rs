use std::collections::{BTreeMap, HashSet};

use crate::data::types::{CellValue, Dataset};
use crate::models::Pad;
use crate::security::json_embed::safe_json_script_tag;
use crate::security::sanitize::sanitize_html;
use crate::spec::schema::{BodyFormat, Layout, SectionType, Theme};

const CSS: &str = include_str!("client/dashboard.css");
const CLIENT_JS: &str = include_str!("client/dashboard.js");
const LOGO_SVG: &str = include_str!("client/logo.svg");
const THEME_TOGGLE_JS: &str = include_str!("client/theme-toggle.js");

/// Render a complete HTML page for a pad.
/// Server generates only the shell — all section rendering happens client-side.
pub fn render_dashboard(pad: &Pad) -> String {
    let spec = &pad.spec;

    let theme_attr = match &spec.theme {
        Some(Theme::Light) => "light",
        Some(Theme::Dark) => "dark",
        Some(Theme::Auto) | None => "auto",
    };

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

    // Sanitize HTML body fields for list sections with body_format: sanitized_html
    let sanitized_datasets = sanitize_body_fields(spec, &pad.datasets);
    let datasets_json = datasets_to_json(&sanitized_datasets);
    let data_tag = safe_json_script_tag("glasspad-data", &datasets_json);

    format!(
        r#"<!DOCTYPE html>
<html lang="en" data-theme="{theme}">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <link rel="icon" type="image/svg+xml" href="{favicon}">
  <title>{title}</title>
  <script src="https://cdn.jsdelivr.net/npm/vega@5.30.0/build/vega.min.js" integrity="sha384-em7CHpJd+SsMugVFf6TY7AKQcLWMcbPhD84hmNK8o6WFDkK+2uHSUQRVQV1/w827" crossorigin="anonymous"></script>
  <script src="https://cdn.jsdelivr.net/npm/vega-lite@5.21.0/build/vega-lite.min.js" integrity="sha384-GhkD6ks9/zgY1m5EFOUZWz/vMVMUFF/92DL61RZc+B42J8osL+jNufKv68bNHHZ2" crossorigin="anonymous"></script>
  <script src="https://cdn.jsdelivr.net/npm/vega-embed@6.26.0/build/vega-embed.min.js" integrity="sha384-TqXb8su49m5OnEpKGO8m+VrgHesrUxyP22HgpXi4hnh1Hm43dXroiSYemNf5D8lv" crossorigin="anonymous"></script>
  <script src="https://cdn.jsdelivr.net/npm/marked@15.0.7/marked.min.js" integrity="sha384-H+hy9ULve6xfxRkWIh/YOtvDdpXgV2fmAGQkIDTxIgZwNoaoBal14Di2YTMR6MzR" crossorigin="anonymous"></script>
  <script src="https://cdn.jsdelivr.net/gh/highlightjs/cdn-release@11.11.1/build/highlight.min.js" integrity="sha384-RH2xi4eIQ/gjtbs9fUXM68sLSi99C7ZWBRX1vDrVv6GQXRibxXLbwO2NGZB74MbU" crossorigin="anonymous"></script>
  <script src="https://cdn.jsdelivr.net/npm/dompurify@3.2.4/dist/purify.min.js" integrity="sha384-eEu5CTj3qGvu9PdJuS+YlkNi7d2XxQROAFYOr59zgObtlcux1ae1Il3u7jvdCSWu" crossorigin="anonymous"></script>
  <link rel="stylesheet" href="https://cdn.jsdelivr.net/gh/highlightjs/cdn-release@11.11.1/build/styles/github.min.css" integrity="sha384-eFTL69TLRZTkNfYZOLM+G04821K1qZao/4QLJbet1pP4tcF+fdXq/9CdqAbWRl/L" crossorigin="anonymous">
  <style>{css}</style>
</head>
<body>
  <div class="page-header">
    <h1>{title}</h1>
    <button id="theme-toggle" class="theme-toggle" aria-label="Toggle theme"></button>
  </div>
  {description}
  <div id="dashboard" class="{layout}"></div>
  {spec_tag}
  {data_tag}
  <script>{js}</script>
  <script>{theme_toggle_js}</script>
</body>
</html>"#,
        title = html_escape(&spec.title),
        description = description_html,
        layout = layout_class,
        theme = theme_attr,
        favicon = svg_favicon_data_uri(LOGO_SVG),
        spec_tag = spec_tag,
        data_tag = data_tag,
        css = CSS,
        js = CLIENT_JS,
        theme_toggle_js = THEME_TOGGLE_JS,
    )
}

/// Collect (source, body_field) pairs that need HTML sanitization from list sections,
/// then return datasets with those fields sanitized.
/// Per-row body_format field ("html"/"text") is respected: only "html" rows are sanitized.
fn sanitize_body_fields(
    spec: &crate::spec::schema::DashboardSpec,
    datasets: &BTreeMap<String, Dataset>,
) -> BTreeMap<String, Dataset> {
    // Collect which (source, body_field) pairs need sanitization
    let mut to_sanitize: HashSet<(String, String)> = HashSet::new();
    for section in &spec.sections {
        if section.section_type != SectionType::List {
            continue;
        }
        let source = match &section.source {
            Some(s) => s.clone(),
            None => continue,
        };
        let list = match &section.list {
            Some(l) => l,
            None => continue,
        };
        let detail = match &list.detail {
            Some(d) => d,
            None => continue,
        };
        let body_field = match &detail.body_field {
            Some(f) => f.clone(),
            None => continue,
        };
        let format = detail.body_format.as_ref().cloned().unwrap_or_default();
        if format == BodyFormat::SanitizedHtml {
            to_sanitize.insert((source, body_field));
        }
    }

    if to_sanitize.is_empty() {
        return datasets.clone();
    }

    datasets
        .iter()
        .map(|(name, data)| {
            let fields_for_source: Vec<&str> = to_sanitize
                .iter()
                .filter(|(src, _)| src == name)
                .map(|(_, field)| field.as_str())
                .collect();

            if fields_for_source.is_empty() {
                return (name.clone(), data.clone());
            }

            let sanitized: Dataset = data
                .iter()
                .map(|row| {
                    // Check per-row body_format: only sanitize if "html" (or absent = use section default)
                    let row_format = row
                        .get("body_format")
                        .and_then(|v| if let CellValue::String(s) = v { Some(s.as_str()) } else { None });
                    let is_html = row_format != Some("text");

                    row.iter()
                        .map(|(k, v)| {
                            if is_html && fields_for_source.contains(&k.as_str()) {
                                if let CellValue::String(html) = v {
                                    (k.clone(), CellValue::String(sanitize_html(html)))
                                } else {
                                    (k.clone(), v.clone())
                                }
                            } else {
                                (k.clone(), v.clone())
                            }
                        })
                        .collect()
                })
                .collect();
            (name.clone(), sanitized)
        })
        .collect()
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

/// Encode an SVG string as a data: URI suitable for a favicon link href.
fn svg_favicon_data_uri(svg: &str) -> String {
    let encoded: String = svg
        .chars()
        .map(|c| match c {
            ' ' => "%20".to_string(),
            '#' => "%23".to_string(),
            '"' => "%22".to_string(),
            '<' => "%3C".to_string(),
            '>' => "%3E".to_string(),
            _ => c.to_string(),
        })
        .collect();
    format!("data:image/svg+xml,{encoded}")
}
