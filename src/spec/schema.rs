use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::data::types::Row;

/// Top-level dashboard spec (canonical form).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DashboardSpec {
    pub spec_version: u32,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub layout: Layout,
    #[serde(default)]
    pub toc: bool,
    #[serde(default)]
    pub timezone: Option<Timezone>,
    #[serde(default)]
    pub datasets: BTreeMap<String, DatasetDecl>,
    pub sections: Vec<Section>,
}

/// Timezone for temporal operations (hour-of-day extraction, display).
/// If omitted, defaults to browser local time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Timezone {
    Utc,
    Local,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub enum Layout {
    #[default]
    #[serde(rename = "grid-2col")]
    Grid2col,
    #[serde(rename = "grid-3col")]
    Grid3col,
    #[serde(rename = "stack")]
    Stack,
}

/// Dataset declaration in the spec.
/// Empty object = "this dataset will be provided externally via --data".
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DatasetDecl {}

/// A section in the dashboard.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Section {
    #[serde(default)]
    pub id: Option<String>,
    pub title: String,
    #[serde(rename = "type")]
    pub section_type: SectionType,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub inline_data: Option<Vec<Row>>,
    #[serde(default)]
    pub chart: Option<ChartConfig>,
    #[serde(default)]
    pub table: Option<TableConfig>,
    #[serde(default)]
    pub stats: Option<StatsConfig>,
    #[serde(default)]
    pub list: Option<ListConfig>,
    #[serde(default)]
    pub markdown: Option<MarkdownConfig>,
    #[serde(default)]
    pub interactive_filter: Option<InteractiveFilter>,
    #[serde(default)]
    pub selectable: Option<bool>,
    #[serde(default)]
    pub batch_actions: Option<Vec<ActionDef>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SectionType {
    Chart,
    Table,
    Stats,
    List,
    Markdown,
}

/// Chart configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChartConfig {
    pub mark: String,
    #[serde(default)]
    pub encoding: serde_json::Value,
}

/// Table configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TableConfig {
    pub columns: Vec<ColumnDef>,
    #[serde(default)]
    pub row_id_field: Option<String>,
    #[serde(default)]
    pub row_actions: Option<Vec<ActionDef>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColumnDef {
    pub field: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub sort: Option<SortType>,
}

/// Stats configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatsConfig {
    pub items: Vec<StatsItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatsItem {
    pub label: String,
    pub aggregate: String,
    #[serde(default)]
    pub field: Option<String>,
    #[serde(rename = "where", default)]
    pub where_clause: Option<BTreeMap<String, serde_json::Value>>,
}

/// List configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListConfig {
    pub id_field: Option<String>,
    #[serde(default)]
    pub layout: Option<ListLayout>,
    #[serde(default)]
    pub title_field: Option<String>,
    #[serde(default)]
    pub subtitle_field: Option<String>,
    #[serde(default)]
    pub meta_field: Option<String>,
    #[serde(default)]
    pub preview_field: Option<String>,
    #[serde(default)]
    pub item_click: Option<String>,
    #[serde(default)]
    pub detail: Option<DetailConfig>,
    #[serde(default)]
    pub on_action: Option<String>,
}

/// Which side to place a TOC sidebar.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TocSide {
    #[default]
    Left,
    Right,
}

/// Link target for markdown-rendered anchor tags.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LinkTarget {
    #[serde(rename = "_blank")]
    Blank,
    #[serde(rename = "_self")]
    Self_,
}

/// Markdown configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MarkdownConfig {
    /// Inline markdown content string (mutually exclusive with content_field).
    #[serde(default)]
    pub content: Option<String>,
    /// Field name to pull markdown content from dataset rows (mutually exclusive with content).
    #[serde(default)]
    pub content_field: Option<String>,
    /// Heading levels to show in the table of contents (e.g. [1, 2, 3]).
    /// Omit or set to empty array to disable the TOC.
    #[serde(default)]
    pub toc_levels: Option<Vec<u8>>,
    /// Which side to place the TOC sidebar (default: left).
    #[serde(default)]
    pub toc_side: Option<TocSide>,
    /// Link target for rendered links (default: browser default / _self).
    #[serde(default)]
    pub link_target: Option<LinkTarget>,
    /// Maximum number of dataset rows to concatenate (default: 100).
    #[serde(default)]
    pub max_rows: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ListLayout {
    Cards,
    Rows,
    Compact,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DetailConfig {
    #[serde(default)]
    pub fields: Option<Vec<ColumnDef>>,
    #[serde(default)]
    pub body_field: Option<String>,
    #[serde(default)]
    pub body_format: Option<BodyFormat>,
    #[serde(default)]
    pub actions: Option<Vec<ActionDef>>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BodyFormat {
    #[default]
    Text,
    #[serde(rename = "sanitized_html")]
    SanitizedHtml,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionDef {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub style: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SortType {
    Number,
    String,
    Temporal,
    Boolean,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InteractiveFilter {
    pub field: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_spec_deserialize() {
        let yaml = r#"
spec_version: 1
title: "Test"
sections:
  - title: "Numbers"
    type: stats
    inline_data:
      - { label: "Count", value: 42 }
    stats:
      items:
        - { label: "Total", aggregate: count }
"#;
        let spec: DashboardSpec = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(spec.spec_version, 1);
        assert_eq!(spec.title, "Test");
        assert_eq!(spec.sections.len(), 1);
        assert_eq!(spec.sections[0].section_type, SectionType::Stats);
    }

    #[test]
    fn chart_section_deserialize() {
        let yaml = r#"
spec_version: 1
title: "Charts"
datasets:
  events: {}
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
        x: { field: country, type: nominal }
        y: { aggregate: count, type: quantitative }
"#;
        let spec: DashboardSpec = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(spec.datasets.len(), 1);
        assert!(spec.datasets.contains_key("events"));
        let section = &spec.sections[0];
        assert_eq!(section.id.as_deref(), Some("by-country"));
        assert_eq!(section.source.as_deref(), Some("events"));
        assert_eq!(
            section.interactive_filter.as_ref().unwrap().field,
            "country"
        );
        assert_eq!(section.chart.as_ref().unwrap().mark, "bar");
    }

    #[test]
    fn layout_defaults_to_grid_2col() {
        let yaml = r#"
spec_version: 1
title: "Test"
sections: []
"#;
        let spec: DashboardSpec = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(spec.layout, Layout::Grid2col);
    }

    #[test]
    fn layout_stack() {
        let yaml = r#"
spec_version: 1
title: "Test"
layout: stack
sections: []
"#;
        let spec: DashboardSpec = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(spec.layout, Layout::Stack);
    }

    #[test]
    fn table_with_row_actions() {
        let yaml = r#"
spec_version: 1
title: "Review"
sections:
  - title: "Items"
    type: table
    source: reviews
    table:
      columns:
        - { field: name, title: "Name" }
      row_id_field: id
      row_actions:
        - { id: approve, label: "OK", style: success }
"#;
        let spec: DashboardSpec = serde_yaml::from_str(yaml).unwrap();
        let table = spec.sections[0].table.as_ref().unwrap();
        assert_eq!(table.row_id_field.as_deref(), Some("id"));
        assert_eq!(table.row_actions.as_ref().unwrap().len(), 1);
    }

    #[test]
    fn list_with_detail_and_actions() {
        let yaml = r#"
spec_version: 1
title: "Inbox"
sections:
  - title: "Messages"
    type: list
    source: emails
    list:
      id_field: id
      layout: cards
      title_field: subject
      detail:
        body_field: body
        body_format: text
        actions:
          - { id: archive, label: "Archive" }
          - { id: delete, label: "Delete", style: danger }
"#;
        let spec: DashboardSpec = serde_yaml::from_str(yaml).unwrap();
        let list = spec.sections[0].list.as_ref().unwrap();
        assert_eq!(list.id_field.as_deref(), Some("id"));
        let detail = list.detail.as_ref().unwrap();
        assert_eq!(detail.body_format.as_ref().unwrap(), &BodyFormat::Text);
        assert_eq!(detail.actions.as_ref().unwrap().len(), 2);
    }

    #[test]
    fn stats_with_aggregates() {
        let yaml = r#"
spec_version: 1
title: "Summary"
sections:
  - title: "KPIs"
    type: stats
    source: events
    stats:
      items:
        - { label: "Total", aggregate: count }
        - { label: "Visits", aggregate: count, where: { event_type: visit } }
        - { label: "Countries", aggregate: distinct, field: country }
"#;
        let spec: DashboardSpec = serde_yaml::from_str(yaml).unwrap();
        let stats = spec.sections[0].stats.as_ref().unwrap();
        assert_eq!(stats.items.len(), 3);
        assert_eq!(stats.items[0].aggregate, "count");
        assert!(stats.items[1].where_clause.is_some());
        assert_eq!(stats.items[2].field.as_deref(), Some("country"));
    }

    #[test]
    fn toc_default_false() {
        let yaml = r#"
spec_version: 1
title: "Test"
sections:
  - title: "A"
    type: table
    inline_data: [{ x: 1 }]
"#;
        let spec: DashboardSpec = serde_yaml::from_str(yaml).unwrap();
        assert!(!spec.toc);
    }

    #[test]
    fn toc_explicit_true() {
        let yaml = r#"
spec_version: 1
title: "Test"
toc: true
sections:
  - title: "A"
    type: table
    inline_data: [{ x: 1 }]
"#;
        let spec: DashboardSpec = serde_yaml::from_str(yaml).unwrap();
        assert!(spec.toc);
    }

    #[test]
    fn timezone_utc() {
        let yaml = r#"
spec_version: 1
title: "Test"
timezone: utc
sections:
  - title: "A"
    type: table
    inline_data: [{ x: 1 }]
"#;
        let spec: DashboardSpec = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(spec.timezone, Some(Timezone::Utc));
    }

    #[test]
    fn timezone_local() {
        let yaml = r#"
spec_version: 1
title: "Test"
timezone: local
sections:
  - title: "A"
    type: table
    inline_data: [{ x: 1 }]
"#;
        let spec: DashboardSpec = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(spec.timezone, Some(Timezone::Local));
    }

    #[test]
    fn timezone_default_none() {
        let yaml = r#"
spec_version: 1
title: "Test"
sections:
  - title: "A"
    type: table
    inline_data: [{ x: 1 }]
"#;
        let spec: DashboardSpec = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(spec.timezone, None);
    }

    #[test]
    fn markdown_inline_content_deserialize() {
        let yaml = r##"
spec_version: 1
title: "Docs"
sections:
  - title: "Readme"
    type: markdown
    markdown:
      content: "# Hello World"
"##;
        let spec: DashboardSpec = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(spec.sections[0].section_type, SectionType::Markdown);
        let md = spec.sections[0].markdown.as_ref().unwrap();
        assert_eq!(md.content.as_deref(), Some("# Hello World"));
        assert!(md.content_field.is_none());
    }

    #[test]
    fn markdown_content_field_deserialize() {
        let yaml = r#"
spec_version: 1
title: "Docs"
datasets:
  notes: {}
sections:
  - title: "Notes"
    type: markdown
    source: notes
    markdown:
      content_field: body
"#;
        let spec: DashboardSpec = serde_yaml::from_str(yaml).unwrap();
        let md = spec.sections[0].markdown.as_ref().unwrap();
        assert!(md.content.is_none());
        assert_eq!(md.content_field.as_deref(), Some("body"));
    }

    #[test]
    fn table_column_sort_type() {
        let yaml = r#"
spec_version: 1
title: "Test"
sections:
  - title: "Data"
    type: table
    inline_data: [{ ts: "2026-01-01", n: 1 }]
    table:
      columns:
        - { field: ts, title: "Time", sort: temporal }
        - { field: n, title: "Num", sort: number }
"#;
        let spec: DashboardSpec = serde_yaml::from_str(yaml).unwrap();
        let cols = &spec.sections[0].table.as_ref().unwrap().columns;
        assert_eq!(cols[0].sort, Some(SortType::Temporal));
        assert_eq!(cols[1].sort, Some(SortType::Number));
    }
}
