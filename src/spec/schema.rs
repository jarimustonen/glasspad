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
    pub datasets: BTreeMap<String, DatasetDecl>,
    pub sections: Vec<Section>,
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
}

/// Chart configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
}
