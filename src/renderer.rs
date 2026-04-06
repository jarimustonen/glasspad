use std::collections::BTreeMap;

use crate::data::types::{CellValue, Dataset};
use crate::models::Pad;
use crate::security::json_embed::safe_json_script_tag;
use crate::spec::schema::Layout;

/// Render a complete HTML page for a pad.
/// The server generates only the shell — all section rendering happens client-side in JS.
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

    // Serialize spec to JSON for client-side rendering
    let spec_json = serde_json::to_value(spec).unwrap_or(serde_json::Value::Null);
    let spec_tag = safe_json_script_tag("glasspad-spec", &spec_json);

    // Serialize datasets to JSON
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

const CSS: &str = r#"
    * { box-sizing: border-box; margin: 0; padding: 0; }
    body {
      font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', system-ui, sans-serif;
      max-width: 1400px; margin: 0 auto; padding: 2rem;
      background: #f8f9fa; color: #1a1a2e;
    }
    h1 { font-size: 1.75rem; font-weight: 700; margin-bottom: 0.25rem; }
    .description { color: #6b7280; margin-bottom: 1.5rem; }
    .dashboard-grid { display: grid; gap: 1.5rem; margin-top: 1.5rem; }
    .grid-2 { grid-template-columns: repeat(2, 1fr); }
    .grid-3 { grid-template-columns: repeat(3, 1fr); }
    .dashboard-stack { display: flex; flex-direction: column; gap: 1.5rem; margin-top: 1.5rem; }
    .section-card {
      background: #fff; border: 1px solid #e5e7eb; border-radius: 12px;
      padding: 1.5rem; box-shadow: 0 1px 3px rgba(0,0,0,0.04);
    }
    .section-card h3 {
      font-size: 0.95rem; font-weight: 600; color: #374151;
      margin-bottom: 1rem; text-transform: uppercase; letter-spacing: 0.03em;
    }
    .chart-container { width: 100%; }
    .chart-container .vega-embed { width: 100%; }
    table { width: 100%; border-collapse: collapse; font-size: 0.9rem; }
    thead th {
      text-align: left; padding: 0.6rem 0.75rem; border-bottom: 2px solid #e5e7eb;
      font-weight: 600; color: #6b7280; font-size: 0.8rem;
      text-transform: uppercase; letter-spacing: 0.04em;
    }
    tbody td { padding: 0.55rem 0.75rem; border-bottom: 1px solid #f3f4f6; }
    tbody tr:hover { background: #f9fafb; }
    .stats-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(140px, 1fr)); gap: 1rem; }
    .stat-card { text-align: center; padding: 1rem; background: #f9fafb; border-radius: 8px; }
    .stat-value { font-size: 1.75rem; font-weight: 700; color: #1a1a2e; line-height: 1.2; }
    .stat-label { font-size: 0.8rem; color: #6b7280; margin-top: 0.25rem; }
    .section-error { color: #dc2626; font-style: italic; }
    @media (max-width: 768px) {
      .grid-2, .grid-3 { grid-template-columns: 1fr; }
      body { padding: 1rem; }
    }
"#;

const CLIENT_JS: &str = r#"
(function() {
  'use strict';

  const spec = JSON.parse(document.getElementById('glasspad-spec').textContent);
  const datasets = JSON.parse(document.getElementById('glasspad-data').textContent);
  const container = document.getElementById('dashboard');

  // Resolve data for a section
  function getData(section) {
    if (section.source && datasets[section.source]) {
      return datasets[section.source];
    }
    if (section.inline_data) {
      return section.inline_data;
    }
    return null;
  }

  // Format a cell value for display
  function formatCell(v) {
    if (v === null || v === undefined) return '';
    if (typeof v === 'boolean') return String(v);
    if (typeof v === 'number') {
      if (Number.isInteger(v)) return v.toLocaleString('en-US');
      return v.toLocaleString('en-US', { maximumFractionDigits: 1 });
    }
    return String(v);
  }

  // Format a large number with separators
  function formatNumber(n) {
    return Math.round(n).toLocaleString('en-US');
  }

  // Escape HTML
  function esc(s) {
    const d = document.createElement('div');
    d.textContent = s;
    return d.innerHTML;
  }

  // Render a single section
  function renderSection(section, index) {
    const card = document.createElement('div');
    card.className = 'section-card';
    card.id = section.id || ('section-' + index);

    const h3 = document.createElement('h3');
    h3.textContent = section.title;
    card.appendChild(h3);

    const data = getData(section);

    try {
      switch (section.type) {
        case 'chart':
          renderChart(card, section, data, index);
          break;
        case 'table':
          renderTable(card, section, data);
          break;
        case 'stats':
          renderStats(card, section, data);
          break;
        case 'list':
          card.innerHTML += '<p class="section-error">List rendering coming soon</p>';
          break;
        default:
          card.innerHTML += '<p class="section-error">Unknown section type: ' + esc(section.type) + '</p>';
      }
    } catch (e) {
      card.innerHTML += '<p class="section-error">Render error: ' + esc(e.message) + '</p>';
      console.error('Section "' + section.title + '" render error:', e);
    }

    return card;
  }

  // --- Chart ---
  function renderChart(card, section, data, index) {
    const cfg = section.chart;
    if (!cfg) { card.innerHTML += '<p class="section-error">No chart config</p>'; return; }

    const divId = 'vis-' + index;
    const div = document.createElement('div');
    div.id = divId;
    div.className = 'chart-container';
    card.appendChild(div);

    const mark = cfg.mark === 'arc' ? { type: 'arc', tooltip: true } : cfg.mark;
    const vlSpec = {
      '$schema': 'https://vega.github.io/schema/vega-lite/v5.json',
      width: 'container',
      height: 300,
      mark: mark,
      data: { values: data || [] },
      encoding: cfg.encoding || {}
    };

    if (cfg.mark === 'arc') {
      delete vlSpec.width;
      vlSpec.height = 300;
      vlSpec.view = { stroke: null };
    }

    vegaEmbed('#' + divId, vlSpec, { actions: false, renderer: 'svg' }).catch(function(err) {
      console.error('Chart "' + section.title + '":', err);
      div.innerHTML = '<p class="section-error">Chart error: ' + esc(err.message) + '</p>';
    });
  }

  // --- Table ---
  function renderTable(card, section, data) {
    const cfg = section.table;
    if (!cfg || !cfg.columns) { card.innerHTML += '<p class="section-error">No table config</p>'; return; }
    if (!data || data.length === 0) { card.innerHTML += '<p class="section-error">No data</p>'; return; }

    let html = '<table><thead><tr>';
    for (const col of cfg.columns) {
      const title = col.title || col.field;
      const style = col.width ? ' style="width:' + col.width + 'px"' : '';
      html += '<th' + style + '>' + esc(title) + '</th>';
    }
    html += '</tr></thead><tbody>';

    for (const row of data) {
      html += '<tr>';
      for (const col of cfg.columns) {
        const val = row[col.field];
        html += '<td>' + esc(formatCell(val)) + '</td>';
      }
      html += '</tr>';
    }
    html += '</tbody></table>';

    const wrapper = document.createElement('div');
    wrapper.innerHTML = html;
    card.appendChild(wrapper);
  }

  // --- Stats ---
  function renderStats(card, section, data) {
    // Aggregation mode
    if (section.stats && section.stats.items) {
      renderAggregateStats(card, section.stats.items, data || []);
      return;
    }
    // Inline label/value mode
    if (section.inline_data) {
      renderInlineStats(card, section.inline_data);
      return;
    }
    card.innerHTML += '<p class="section-error">No stats config</p>';
  }

  function renderInlineStats(card, rows) {
    const grid = document.createElement('div');
    grid.className = 'stats-grid';
    for (const row of rows) {
      grid.innerHTML += '<div class="stat-card"><div class="stat-value">' +
        esc(formatCell(row.value)) + '</div><div class="stat-label">' +
        esc(formatCell(row.label)) + '</div></div>';
    }
    card.appendChild(grid);
  }

  function renderAggregateStats(card, items, data) {
    const grid = document.createElement('div');
    grid.className = 'stats-grid';

    for (const item of items) {
      const val = computeAggregate(item, data);
      grid.innerHTML += '<div class="stat-card"><div class="stat-value">' +
        esc(val) + '</div><div class="stat-label">' +
        esc(item.label) + '</div></div>';
    }
    card.appendChild(grid);
  }

  function computeAggregate(item, data) {
    // Apply where filter
    let filtered = data;
    if (item.where) {
      filtered = data.filter(function(row) {
        return Object.keys(item.where).every(function(k) {
          return row[k] === item.where[k];
        });
      });
    }

    const agg = item.aggregate;
    const field = item.field;

    if (agg === 'count') {
      return formatNumber(filtered.length);
    }
    if (!field) return '\u26a0 missing field';

    if (agg === 'distinct') {
      const seen = new Set();
      for (const row of filtered) {
        const v = row[field];
        if (v !== null && v !== undefined) seen.add(String(v));
      }
      return formatNumber(seen.size);
    }

    const nums = filtered.map(function(r) { return r[field]; })
                         .filter(function(v) { return typeof v === 'number' && isFinite(v); });

    if (nums.length === 0) return '\u2014';

    if (agg === 'sum') return formatNumber(nums.reduce(function(a, b) { return a + b; }, 0));
    if (agg === 'avg') {
      const avg = nums.reduce(function(a, b) { return a + b; }, 0) / nums.length;
      return avg % 1 === 0 ? formatNumber(avg) : avg.toLocaleString('en-US', { maximumFractionDigits: 1 });
    }
    if (agg === 'min') return formatCell(Math.min.apply(null, nums));
    if (agg === 'max') return formatCell(Math.max.apply(null, nums));

    return '\u26a0 unknown: ' + agg;
  }

  // --- Render all sections ---
  spec.sections.forEach(function(section, i) {
    container.appendChild(renderSection(section, i));
  });

})();
"#;
