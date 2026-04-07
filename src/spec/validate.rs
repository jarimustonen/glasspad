use std::collections::HashSet;

use super::schema::{DashboardSpec, SectionType};

#[derive(Debug, PartialEq)]
pub struct SpecError {
    pub section: Option<String>,
    pub message: String,
}

impl std::fmt::Display for SpecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.section {
            Some(s) => write!(f, "section \"{}\": {}", s, self.message),
            None => write!(f, "{}", self.message),
        }
    }
}

fn err(section: Option<&str>, msg: impl Into<String>) -> SpecError {
    SpecError {
        section: section.map(|s| s.to_string()),
        message: msg.into(),
    }
}

const SUPPORTED_SPEC_VERSIONS: &[u32] = &[1];
const SUPPORTED_MARKS: &[&str] = &["bar", "line", "arc"];
const SUPPORTED_AGGREGATES: &[&str] = &["count", "distinct", "sum", "avg", "min", "max"];

/// Validate a parsed DashboardSpec.
///
/// `provided_datasets` is the set of dataset names supplied via --data flags or API.
pub fn validate(
    spec: &DashboardSpec,
    provided_datasets: &HashSet<String>,
) -> Vec<SpecError> {
    let mut errors = Vec::new();

    // spec_version
    if !SUPPORTED_SPEC_VERSIONS.contains(&spec.spec_version) {
        errors.push(err(None, format!(
            "unsupported spec_version {}, supported: {:?}",
            spec.spec_version, SUPPORTED_SPEC_VERSIONS
        )));
    }

    // sections non-empty
    if spec.sections.is_empty() {
        errors.push(err(None, "sections list is empty"));
    }

    // Collect declared dataset names
    let declared: HashSet<String> = spec.datasets.keys().cloned().collect();

    for (i, section) in spec.sections.iter().enumerate() {
        let default_label = format!("[{}] {}", i, section.title);
        let label = section
            .id
            .as_deref()
            .unwrap_or(&default_label);

        // source + inline_data together is valid: source is the shared identity for
        // cross-filtering, inline_data is the content (injected by CLI --data)

        // source references existing dataset
        if let Some(ref source) = section.source {
            if !declared.contains(source) {
                errors.push(err(
                    Some(label),
                    format!("source \"{}\" not declared in datasets", source),
                ));
            }
        }

        // Type-specific validations
        match section.section_type {
            SectionType::Chart => validate_chart(section, label, &mut errors),
            SectionType::Table => validate_table(section, label, &mut errors),
            SectionType::Stats => validate_stats(section, label, &mut errors),
            SectionType::List => validate_list(section, label, &mut errors),
        }

        // interactive_filter requires id
        if section.interactive_filter.is_some() && section.id.is_none() {
            errors.push(err(
                Some(label),
                "interactive_filter requires section id",
            ));
        }

        // interactive_filter only supported on charts
        if section.interactive_filter.is_some() && section.section_type != SectionType::Chart {
            errors.push(err(
                Some(label),
                "interactive_filter is only supported on chart sections",
            ));
        }
    }

    errors
}

fn validate_chart(
    section: &super::schema::Section,
    label: &str,
    errors: &mut Vec<SpecError>,
) {
    let chart = match &section.chart {
        Some(c) => c,
        None => {
            errors.push(err(Some(label), "chart section requires chart config"));
            return;
        }
    };

    if !SUPPORTED_MARKS.contains(&chart.mark.as_str()) {
        errors.push(err(
            Some(label),
            format!(
                "unknown chart mark \"{}\", supported: {:?}",
                chart.mark, SUPPORTED_MARKS
            ),
        ));
    }

    // chart.encoding must be an object
    if !chart.encoding.is_object() && !chart.encoding.is_null() {
        errors.push(err(Some(label), "chart.encoding must be a JSON object"));
        return;
    }

    // interactive_filter.field should appear in encoding
    if let Some(ref filter) = section.interactive_filter {
        if let serde_json::Value::Object(ref enc) = chart.encoding {
            let field_found = enc.values().any(|channel| {
                channel
                    .get("field")
                    .and_then(|f| f.as_str())
                    .is_some_and(|f| f == filter.field)
            });
            if !field_found {
                errors.push(err(
                    Some(label),
                    format!(
                        "interactive_filter.field \"{}\" not found in chart encoding",
                        filter.field
                    ),
                ));
            }
        }
    }
}

fn validate_table(
    section: &super::schema::Section,
    label: &str,
    errors: &mut Vec<SpecError>,
) {
    let table = match &section.table {
        Some(t) => t,
        None => {
            errors.push(err(Some(label), "table section requires table config"));
            return;
        }
    };

    if table.columns.is_empty() {
        errors.push(err(Some(label), "table columns list is empty"));
    }

    // row_actions requires row_id_field
    if table.row_actions.is_some() && table.row_id_field.is_none() {
        errors.push(err(
            Some(label),
            "row_actions requires table.row_id_field",
        ));
    }
}

fn validate_stats(
    section: &super::schema::Section,
    label: &str,
    errors: &mut Vec<SpecError>,
) {
    let stats = match &section.stats {
        Some(s) => s,
        None => {
            errors.push(err(Some(label), "stats section requires stats config"));
            return;
        }
    };

    for item in &stats.items {
        if !SUPPORTED_AGGREGATES.contains(&item.aggregate.as_str()) {
            errors.push(err(
                Some(label),
                format!(
                    "unknown aggregate \"{}\", supported: {:?}",
                    item.aggregate, SUPPORTED_AGGREGATES
                ),
            ));
        }

        // distinct, sum, avg, min, max require field
        if item.aggregate != "count" && item.field.is_none() {
            errors.push(err(
                Some(label),
                format!(
                    "aggregate \"{}\" requires field",
                    item.aggregate
                ),
            ));
        }
    }
}

fn validate_list(
    section: &super::schema::Section,
    label: &str,
    errors: &mut Vec<SpecError>,
) {
    let list = match &section.list {
        Some(l) => l,
        None => {
            errors.push(err(Some(label), "list section requires list config"));
            return;
        }
    };

    // Actions require id_field
    let has_actions = list
        .detail
        .as_ref()
        .and_then(|d| d.actions.as_ref())
        .is_some_and(|a| !a.is_empty());

    let is_selectable = section.selectable.unwrap_or(false);

    if (has_actions || is_selectable) && list.id_field.is_none() {
        errors.push(err(
            Some(label),
            "list with actions or selectable requires list.id_field",
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::schema::*;
    use std::collections::BTreeMap;

    fn minimal_spec() -> DashboardSpec {
        DashboardSpec {
            spec_version: 1,
            title: "Test".to_string(),
            description: None,
            layout: Layout::default(),
            datasets: BTreeMap::new(),
            sections: vec![Section {
                id: None,
                title: "S1".to_string(),
                section_type: SectionType::Stats,
                source: None,
                inline_data: None,
                chart: None,
                table: None,
                stats: Some(StatsConfig {
                    items: vec![StatsItem {
                        label: "Total".to_string(),
                        aggregate: "count".to_string(),
                        field: None,
                        where_clause: None,
                    }],
                }),
                list: None,
                interactive_filter: None,
                selectable: None,
                batch_actions: None,
            }],
        }
    }

    #[test]
    fn valid_minimal_spec() {
        let spec = minimal_spec();
        let errors = validate(&spec, &HashSet::new());
        assert!(errors.is_empty(), "errors: {:?}", errors);
    }

    #[test]
    fn unsupported_spec_version() {
        let mut spec = minimal_spec();
        spec.spec_version = 99;
        let errors = validate(&spec, &HashSet::new());
        assert!(errors.iter().any(|e| e.message.contains("unsupported spec_version")));
    }

    #[test]
    fn empty_sections() {
        let mut spec = minimal_spec();
        spec.sections.clear();
        let errors = validate(&spec, &HashSet::new());
        assert!(errors.iter().any(|e| e.message.contains("sections list is empty")));
    }

    #[test]
    fn source_references_undeclared_dataset() {
        let mut spec = minimal_spec();
        spec.sections[0].source = Some("nonexistent".to_string());
        let errors = validate(&spec, &HashSet::new());
        assert!(errors.iter().any(|e| e.message.contains("not declared in datasets")));
    }

    #[test]
    fn source_and_inline_data_together_is_valid() {
        // source provides shared identity for filtering, inline_data provides content
        let mut spec = minimal_spec();
        spec.datasets.insert("events".to_string(), DatasetDecl {});
        spec.sections[0].source = Some("events".to_string());
        spec.sections[0].inline_data = Some(vec![]);
        let errors = validate(&spec, &HashSet::new());
        assert!(errors.is_empty(), "errors: {:?}", errors);
    }

    #[test]
    fn chart_unknown_mark() {
        let mut spec = minimal_spec();
        spec.sections[0].section_type = SectionType::Chart;
        spec.sections[0].stats = None;
        spec.sections[0].chart = Some(ChartConfig {
            mark: "scatter".to_string(),
            encoding: serde_json::json!({}),
        });
        let errors = validate(&spec, &HashSet::new());
        assert!(errors.iter().any(|e| e.message.contains("unknown chart mark")));
    }

    #[test]
    fn chart_missing_config() {
        let mut spec = minimal_spec();
        spec.sections[0].section_type = SectionType::Chart;
        spec.sections[0].stats = None;
        spec.sections[0].chart = None;
        let errors = validate(&spec, &HashSet::new());
        assert!(errors.iter().any(|e| e.message.contains("chart section requires chart config")));
    }

    #[test]
    fn interactive_filter_field_not_in_encoding() {
        let mut spec = minimal_spec();
        spec.sections[0].id = Some("c1".to_string());
        spec.sections[0].section_type = SectionType::Chart;
        spec.sections[0].stats = None;
        spec.sections[0].chart = Some(ChartConfig {
            mark: "bar".to_string(),
            encoding: serde_json::json!({
                "x": { "field": "date", "type": "temporal" },
                "y": { "aggregate": "count", "type": "quantitative" }
            }),
        });
        spec.sections[0].interactive_filter = Some(InteractiveFilter {
            field: "country".to_string(),
        });
        let errors = validate(&spec, &HashSet::new());
        assert!(errors.iter().any(|e| e.message.contains("not found in chart encoding")));
    }

    #[test]
    fn interactive_filter_field_found_in_encoding() {
        let mut spec = minimal_spec();
        spec.sections[0].id = Some("c1".to_string());
        spec.sections[0].section_type = SectionType::Chart;
        spec.sections[0].stats = None;
        spec.sections[0].chart = Some(ChartConfig {
            mark: "bar".to_string(),
            encoding: serde_json::json!({
                "x": { "field": "country", "type": "nominal" },
                "y": { "aggregate": "count", "type": "quantitative" }
            }),
        });
        spec.sections[0].interactive_filter = Some(InteractiveFilter {
            field: "country".to_string(),
        });
        let errors = validate(&spec, &HashSet::new());
        assert!(errors.is_empty(), "errors: {:?}", errors);
    }

    #[test]
    fn interactive_filter_requires_id() {
        let mut spec = minimal_spec();
        spec.sections[0].section_type = SectionType::Chart;
        spec.sections[0].stats = None;
        spec.sections[0].chart = Some(ChartConfig {
            mark: "bar".to_string(),
            encoding: serde_json::json!({
                "x": { "field": "country", "type": "nominal" }
            }),
        });
        spec.sections[0].interactive_filter = Some(InteractiveFilter {
            field: "country".to_string(),
        });
        // id is None
        let errors = validate(&spec, &HashSet::new());
        assert!(errors.iter().any(|e| e.message.contains("requires section id")));
    }

    #[test]
    fn table_missing_config() {
        let mut spec = minimal_spec();
        spec.sections[0].section_type = SectionType::Table;
        spec.sections[0].stats = None;
        let errors = validate(&spec, &HashSet::new());
        assert!(errors.iter().any(|e| e.message.contains("table section requires table config")));
    }

    #[test]
    fn table_row_actions_without_id_field() {
        let mut spec = minimal_spec();
        spec.sections[0].section_type = SectionType::Table;
        spec.sections[0].stats = None;
        spec.sections[0].table = Some(TableConfig {
            columns: vec![ColumnDef {
                field: "name".to_string(),
                title: None,
                width: None,
                sort: None,
            }],
            row_id_field: None,
            row_actions: Some(vec![ActionDef {
                id: "approve".to_string(),
                label: "OK".to_string(),
                style: None,
            }]),
        });
        let errors = validate(&spec, &HashSet::new());
        assert!(errors.iter().any(|e| e.message.contains("row_actions requires table.row_id_field")));
    }

    #[test]
    fn stats_unknown_aggregate() {
        let mut spec = minimal_spec();
        spec.sections[0].stats.as_mut().unwrap().items[0].aggregate = "median".to_string();
        let errors = validate(&spec, &HashSet::new());
        assert!(errors.iter().any(|e| e.message.contains("unknown aggregate \"median\"")));
    }

    #[test]
    fn stats_distinct_requires_field() {
        let mut spec = minimal_spec();
        spec.sections[0].stats.as_mut().unwrap().items[0].aggregate = "distinct".to_string();
        spec.sections[0].stats.as_mut().unwrap().items[0].field = None;
        let errors = validate(&spec, &HashSet::new());
        assert!(errors.iter().any(|e| e.message.contains("requires field")));
    }

    #[test]
    fn list_actions_without_id_field() {
        let mut spec = minimal_spec();
        spec.sections[0].section_type = SectionType::List;
        spec.sections[0].stats = None;
        spec.sections[0].list = Some(ListConfig {
            id_field: None,
            layout: None,
            title_field: Some("subject".to_string()),
            subtitle_field: None,
            meta_field: None,
            preview_field: None,
            item_click: None,
            detail: Some(DetailConfig {
                fields: None,
                body_field: None,
                body_format: None,
                actions: Some(vec![ActionDef {
                    id: "delete".to_string(),
                    label: "Delete".to_string(),
                    style: None,
                }]),
            }),
            on_action: None,
        });
        let errors = validate(&spec, &HashSet::new());
        assert!(errors.iter().any(|e| e.message.contains("requires list.id_field")));
    }

    #[test]
    fn list_selectable_without_id_field() {
        let mut spec = minimal_spec();
        spec.sections[0].section_type = SectionType::List;
        spec.sections[0].stats = None;
        spec.sections[0].selectable = Some(true);
        spec.sections[0].list = Some(ListConfig {
            id_field: None,
            layout: None,
            title_field: None,
            subtitle_field: None,
            meta_field: None,
            preview_field: None,
            item_click: None,
            detail: None,
            on_action: None,
        });
        let errors = validate(&spec, &HashSet::new());
        assert!(errors.iter().any(|e| e.message.contains("requires list.id_field")));
    }

    #[test]
    fn valid_complex_spec() {
        let mut spec = minimal_spec();
        spec.datasets.insert("events".to_string(), DatasetDecl {});
        spec.sections = vec![
            Section {
                id: Some("chart1".to_string()),
                title: "By country".to_string(),
                section_type: SectionType::Chart,
                source: Some("events".to_string()),
                inline_data: None,
                chart: Some(ChartConfig {
                    mark: "bar".to_string(),
                    encoding: serde_json::json!({
                        "x": { "field": "country", "type": "nominal" },
                        "y": { "aggregate": "count", "type": "quantitative" }
                    }),
                }),
                table: None,
                stats: None,
                list: None,
                interactive_filter: Some(InteractiveFilter {
                    field: "country".to_string(),
                }),
                selectable: None,
                batch_actions: None,
            },
            Section {
                id: Some("stats1".to_string()),
                title: "Summary".to_string(),
                section_type: SectionType::Stats,
                source: Some("events".to_string()),
                inline_data: None,
                chart: None,
                table: None,
                stats: Some(StatsConfig {
                    items: vec![
                        StatsItem {
                            label: "Total".to_string(),
                            aggregate: "count".to_string(),
                            field: None,
                            where_clause: None,
                        },
                        StatsItem {
                            label: "Countries".to_string(),
                            aggregate: "distinct".to_string(),
                            field: Some("country".to_string()),
                            where_clause: None,
                        },
                    ],
                }),
                list: None,
                interactive_filter: None,
                selectable: None,
                batch_actions: None,
            },
        ];
        let provided = HashSet::from(["events".to_string()]);
        let errors = validate(&spec, &provided);
        assert!(errors.is_empty(), "errors: {:?}", errors);
    }
}
