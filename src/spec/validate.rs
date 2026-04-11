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

    // Check that declared datasets are actually provided (BTreeMap iteration = stable order)
    for name in spec.datasets.keys() {
        if !provided_datasets.contains(name) {
            errors.push(err(
                None,
                format!("dataset \"{}\" is declared but no data was provided", name),
            ));
        }
    }

    // Duplicate section.id check
    let mut seen_ids: HashSet<String> = HashSet::new();
    for section in &spec.sections {
        if let Some(ref id) = section.id {
            if !seen_ids.insert(id.clone()) {
                errors.push(err(
                    Some(id),
                    format!("duplicate section id \"{}\"", id),
                ));
            }
        }
    }

    // Track markdown TOC sections for cross-section validation
    let mut md_toc_count = 0u32;

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
            if !spec.datasets.contains_key(source) {
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
            SectionType::Markdown => validate_markdown(section, label, &mut errors),
        }

        // Count markdown sections with active TOC
        if section.section_type == SectionType::Markdown {
            if let Some(ref md) = section.markdown {
                if md.toc_levels.as_ref().is_some_and(|v| !v.is_empty()) {
                    md_toc_count += 1;
                }
            }
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

    // Only one markdown section may define toc_levels (renderer creates one global sidebar)
    if md_toc_count > 1 {
        errors.push(err(
            None,
            "only one markdown section may define toc_levels (single global TOC sidebar)",
        ));
    }

    // Dashboard TOC and markdown TOC cannot coexist (overlapping fixed sidebars)
    if spec.toc && md_toc_count > 0 {
        errors.push(err(
            None,
            "spec.toc and markdown toc_levels cannot both be enabled (overlapping sidebars)",
        ));
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

    // id_field is required for all list sections (detail view navigation)
    if list.id_field.is_none() {
        errors.push(err(
            Some(label),
            "list section requires list.id_field",
        ));
    }
}

fn validate_markdown(
    section: &super::schema::Section,
    label: &str,
    errors: &mut Vec<SpecError>,
) {
    let md = match &section.markdown {
        Some(m) => m,
        None => {
            errors.push(err(Some(label), "markdown section requires markdown config"));
            return;
        }
    };

    // content and content_field are mutually exclusive
    if md.content.is_some() && md.content_field.is_some() {
        errors.push(err(
            Some(label),
            "markdown config cannot specify both content and content_field",
        ));
    }
    if md.content.is_none() && md.content_field.is_none() {
        errors.push(err(
            Some(label),
            "markdown config requires either content or content_field",
        ));
    }

    // content_field requires a data source
    if md.content_field.is_some() && section.source.is_none() && section.inline_data.is_none() {
        errors.push(err(
            Some(label),
            "markdown content_field requires source or inline_data",
        ));
    }

    // toc_levels values must be valid heading levels (1..=6)
    if let Some(ref levels) = md.toc_levels {
        for level in levels {
            if !(1..=6).contains(level) {
                errors.push(err(
                    Some(label),
                    format!("markdown.toc_levels must contain values 1-6, got {}", level),
                ));
            }
        }
        // toc_levels requires section.id for stable heading anchors
        if !levels.is_empty() && section.id.is_none() {
            errors.push(err(
                Some(label),
                "markdown toc_levels requires section id for stable heading anchors",
            ));
        }
    }

    // max_rows must be > 0 if provided
    if let Some(max_rows) = md.max_rows {
        if max_rows == 0 {
            errors.push(err(
                Some(label),
                "markdown max_rows must be greater than 0",
            ));
        }
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
            toc: false,
            timezone: None,
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
                markdown: None,
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
        let provided = HashSet::from(["events".to_string()]);
        let errors = validate(&spec, &provided);
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
    fn list_without_id_field() {
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
            detail: None,
            on_action: None,
        });
        let errors = validate(&spec, &HashSet::new());
        assert!(errors.iter().any(|e| e.message.contains("requires list.id_field")));
    }

    #[test]
    fn markdown_missing_config() {
        let mut spec = minimal_spec();
        spec.sections[0].section_type = SectionType::Markdown;
        spec.sections[0].stats = None;
        let errors = validate(&spec, &HashSet::new());
        assert!(errors.iter().any(|e| e.message.contains("markdown section requires markdown config")));
    }

    #[test]
    fn markdown_missing_content_and_content_field() {
        let mut spec = minimal_spec();
        spec.sections[0].section_type = SectionType::Markdown;
        spec.sections[0].stats = None;
        spec.sections[0].markdown = Some(MarkdownConfig {
            content: None,
            content_field: None,
            toc_levels: None,
            toc_side: None,
            link_target: None,
            max_rows: None,
        });
        let errors = validate(&spec, &HashSet::new());
        assert!(errors.iter().any(|e| e.message.contains("requires either content or content_field")));
    }

    #[test]
    fn markdown_with_content_valid() {
        let mut spec = minimal_spec();
        spec.sections[0].section_type = SectionType::Markdown;
        spec.sections[0].stats = None;
        spec.sections[0].markdown = Some(MarkdownConfig {
            content: Some("# Hello".to_string()),
            content_field: None,
            toc_levels: None,
            toc_side: None,
            link_target: None,
            max_rows: None,
        });
        let errors = validate(&spec, &HashSet::new());
        assert!(errors.is_empty(), "errors: {:?}", errors);
    }

    #[test]
    fn markdown_with_content_field_valid() {
        let mut spec = minimal_spec();
        spec.sections[0].section_type = SectionType::Markdown;
        spec.sections[0].stats = None;
        spec.datasets.insert("notes".to_string(), DatasetDecl {});
        spec.sections[0].source = Some("notes".to_string());
        spec.sections[0].markdown = Some(MarkdownConfig {
            content: None,
            content_field: Some("body".to_string()),
            toc_levels: None,
            toc_side: None,
            link_target: None,
            max_rows: None,
        });
        let provided = HashSet::from(["notes".to_string()]);
        let errors = validate(&spec, &provided);
        assert!(errors.is_empty(), "errors: {:?}", errors);
    }

    #[test]
    fn list_with_id_field_valid() {
        let mut spec = minimal_spec();
        spec.sections[0].section_type = SectionType::List;
        spec.sections[0].stats = None;
        spec.sections[0].list = Some(ListConfig {
            id_field: Some("id".to_string()),
            layout: None,
            title_field: Some("subject".to_string()),
            subtitle_field: None,
            meta_field: None,
            preview_field: None,
            item_click: None,
            detail: None,
            on_action: None,
        });
        let errors = validate(&spec, &HashSet::new());
        assert!(errors.is_empty(), "errors: {:?}", errors);
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
                markdown: None,
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
                markdown: None,
                interactive_filter: None,
                selectable: None,
                batch_actions: None,
            },
        ];
        let provided = HashSet::from(["events".to_string()]);
        let errors = validate(&spec, &provided);
        assert!(errors.is_empty(), "errors: {:?}", errors);
    }

    // --- markdown validation: new rules ---

    #[test]
    fn markdown_reject_both_content_and_content_field() {
        let mut spec = minimal_spec();
        spec.sections[0].section_type = SectionType::Markdown;
        spec.sections[0].stats = None;
        spec.sections[0].markdown = Some(MarkdownConfig {
            content: Some("# Hello".to_string()),
            content_field: Some("body".to_string()),
            toc_levels: None,
            toc_side: None,
            link_target: None,
            max_rows: None,
        });
        let errors = validate(&spec, &HashSet::new());
        assert!(errors.iter().any(|e| e.message.contains("cannot specify both content and content_field")));
    }

    #[test]
    fn markdown_content_field_requires_source() {
        let mut spec = minimal_spec();
        spec.sections[0].section_type = SectionType::Markdown;
        spec.sections[0].stats = None;
        spec.sections[0].markdown = Some(MarkdownConfig {
            content: None,
            content_field: Some("body".to_string()),
            toc_levels: None,
            toc_side: None,
            link_target: None,
            max_rows: None,
        });
        // No source, no inline_data
        let errors = validate(&spec, &HashSet::new());
        assert!(errors.iter().any(|e| e.message.contains("content_field requires source or inline_data")));
    }

    #[test]
    fn markdown_toc_levels_reject_invalid() {
        let mut spec = minimal_spec();
        spec.sections[0].id = Some("md1".to_string());
        spec.sections[0].section_type = SectionType::Markdown;
        spec.sections[0].stats = None;
        spec.sections[0].markdown = Some(MarkdownConfig {
            content: Some("# Hello".to_string()),
            content_field: None,
            toc_levels: Some(vec![1, 2, 7]),
            toc_side: None,
            link_target: None,
            max_rows: None,
        });
        let errors = validate(&spec, &HashSet::new());
        assert!(errors.iter().any(|e| e.message.contains("toc_levels must contain values 1-6, got 7")));
    }

    #[test]
    fn markdown_toc_levels_reject_zero() {
        let mut spec = minimal_spec();
        spec.sections[0].id = Some("md1".to_string());
        spec.sections[0].section_type = SectionType::Markdown;
        spec.sections[0].stats = None;
        spec.sections[0].markdown = Some(MarkdownConfig {
            content: Some("# Hello".to_string()),
            content_field: None,
            toc_levels: Some(vec![0]),
            toc_side: None,
            link_target: None,
            max_rows: None,
        });
        let errors = validate(&spec, &HashSet::new());
        assert!(errors.iter().any(|e| e.message.contains("toc_levels must contain values 1-6, got 0")));
    }

    #[test]
    fn markdown_toc_levels_empty_is_valid() {
        let mut spec = minimal_spec();
        spec.sections[0].section_type = SectionType::Markdown;
        spec.sections[0].stats = None;
        spec.sections[0].markdown = Some(MarkdownConfig {
            content: Some("# Hello".to_string()),
            content_field: None,
            toc_levels: Some(vec![]),
            toc_side: None,
            link_target: None,
            max_rows: None,
        });
        let errors = validate(&spec, &HashSet::new());
        assert!(errors.is_empty(), "errors: {:?}", errors);
    }

    #[test]
    fn markdown_toc_levels_requires_section_id() {
        let mut spec = minimal_spec();
        spec.sections[0].section_type = SectionType::Markdown;
        spec.sections[0].stats = None;
        spec.sections[0].id = None; // no id
        spec.sections[0].markdown = Some(MarkdownConfig {
            content: Some("# Hello".to_string()),
            content_field: None,
            toc_levels: Some(vec![1, 2]),
            toc_side: None,
            link_target: None,
            max_rows: None,
        });
        let errors = validate(&spec, &HashSet::new());
        assert!(errors.iter().any(|e| e.message.contains("toc_levels requires section id")));
    }

    #[test]
    fn markdown_max_rows_reject_zero() {
        let mut spec = minimal_spec();
        spec.sections[0].section_type = SectionType::Markdown;
        spec.sections[0].stats = None;
        spec.sections[0].markdown = Some(MarkdownConfig {
            content: Some("# Hello".to_string()),
            content_field: None,
            toc_levels: None,
            toc_side: None,
            link_target: None,
            max_rows: Some(0),
        });
        let errors = validate(&spec, &HashSet::new());
        assert!(errors.iter().any(|e| e.message.contains("max_rows must be greater than 0")));
    }

    #[test]
    fn markdown_toc_with_spec_toc_rejected() {
        let mut spec = minimal_spec();
        spec.toc = true;
        spec.sections[0].id = Some("md1".to_string());
        spec.sections[0].section_type = SectionType::Markdown;
        spec.sections[0].stats = None;
        spec.sections[0].markdown = Some(MarkdownConfig {
            content: Some("# Hello".to_string()),
            content_field: None,
            toc_levels: Some(vec![1, 2]),
            toc_side: None,
            link_target: None,
            max_rows: None,
        });
        let errors = validate(&spec, &HashSet::new());
        assert!(errors.iter().any(|e| e.message.contains("spec.toc and markdown toc_levels cannot both be enabled")));
    }

    #[test]
    fn markdown_multiple_toc_sections_rejected() {
        let mut spec = minimal_spec();
        spec.sections = vec![
            Section {
                id: Some("md1".to_string()),
                title: "Doc1".to_string(),
                section_type: SectionType::Markdown,
                source: None,
                inline_data: None,
                chart: None,
                table: None,
                stats: None,
                list: None,
                markdown: Some(MarkdownConfig {
                    content: Some("# A".to_string()),
                    content_field: None,
                    toc_levels: Some(vec![1, 2]),
                    toc_side: None,
                    link_target: None,
                    max_rows: None,
                }),
                interactive_filter: None,
                selectable: None,
                batch_actions: None,
            },
            Section {
                id: Some("md2".to_string()),
                title: "Doc2".to_string(),
                section_type: SectionType::Markdown,
                source: None,
                inline_data: None,
                chart: None,
                table: None,
                stats: None,
                list: None,
                markdown: Some(MarkdownConfig {
                    content: Some("# B".to_string()),
                    content_field: None,
                    toc_levels: Some(vec![1]),
                    toc_side: None,
                    link_target: None,
                    max_rows: None,
                }),
                interactive_filter: None,
                selectable: None,
                batch_actions: None,
            },
        ];
        let errors = validate(&spec, &HashSet::new());
        assert!(errors.iter().any(|e| e.message.contains("only one markdown section may define toc_levels")));
    }

    #[test]
    fn declared_dataset_not_provided() {
        let mut spec = minimal_spec();
        spec.datasets.insert("events".to_string(), DatasetDecl {});
        let errors = validate(&spec, &HashSet::new());
        assert_eq!(errors.len(), 1, "expected exactly one error, got: {:?}", errors);
        assert!(errors[0].message.contains("dataset \"events\" is declared but no data was provided"));
        assert!(errors[0].section.is_none());
    }

    #[test]
    fn declared_dataset_provided_is_valid() {
        let mut spec = minimal_spec();
        spec.datasets.insert("events".to_string(), DatasetDecl {});
        spec.sections[0].source = Some("events".to_string());
        let provided = HashSet::from(["events".to_string()]);
        let errors = validate(&spec, &provided);
        assert!(errors.is_empty(), "errors: {:?}", errors);
    }

    #[test]
    fn duplicate_section_id_rejected() {
        let mut spec = minimal_spec();
        spec.sections = vec![
            Section {
                id: Some("dup".to_string()),
                title: "A".to_string(),
                section_type: SectionType::Markdown,
                source: None,
                inline_data: None,
                chart: None,
                table: None,
                stats: None,
                list: None,
                markdown: Some(MarkdownConfig {
                    content: Some("# A".to_string()),
                    content_field: None,
                    toc_levels: None,
                    toc_side: None,
                    link_target: None,
                    max_rows: None,
                }),
                interactive_filter: None,
                selectable: None,
                batch_actions: None,
            },
            Section {
                id: Some("dup".to_string()),
                title: "B".to_string(),
                section_type: SectionType::Markdown,
                source: None,
                inline_data: None,
                chart: None,
                table: None,
                stats: None,
                list: None,
                markdown: Some(MarkdownConfig {
                    content: Some("# B".to_string()),
                    content_field: None,
                    toc_levels: None,
                    toc_side: None,
                    link_target: None,
                    max_rows: None,
                }),
                interactive_filter: None,
                selectable: None,
                batch_actions: None,
            },
        ];
        let errors = validate(&spec, &HashSet::new());
        assert!(errors.iter().any(|e| e.message.contains("duplicate section id")));
    }
}
