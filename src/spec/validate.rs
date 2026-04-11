use std::collections::{BTreeSet, HashSet};

use super::schema::{ActionDef, DashboardSpec, SectionType};

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
/// `external_datasets` contains only dataset names provided externally (e.g. via
/// `--data` flags or API uploads). Inline datasets derived from `section.inline_data`
/// are computed internally from the spec — callers should NOT include them here.
///
/// Uses `BTreeSet` for deterministic error ordering.
pub fn validate(
    spec: &DashboardSpec,
    external_datasets: &BTreeSet<String>,
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

    // Compute inline dataset names from the spec (single source of truth:
    // Section::inline_dataset_name). These supplement external datasets.
    let inline_names: BTreeSet<String> = spec
        .sections
        .iter()
        .enumerate()
        .filter_map(|(idx, s)| s.inline_dataset_name(idx))
        .collect();

    // Check that declared datasets are actually provided (BTreeMap iteration = stable order).
    // A dataset is available if it's either externally provided or supplied via inline_data.
    for name in spec.datasets.keys() {
        if !external_datasets.contains(name) && !inline_names.contains(name) {
            errors.push(err(
                None,
                format!("dataset \"{}\" is declared but no data was provided", name),
            ));
        }
    }

    // Check that externally provided datasets are declared in spec (catches typos /
    // stale --data flags). Only checks external datasets — inline names are derived
    // from the spec and don't need top-level declarations. BTreeSet iteration = stable order.
    for name in external_datasets {
        if !spec.datasets.contains_key(name) {
            errors.push(err(
                None,
                format!(
                    "provided dataset \"{}\" is not declared in spec datasets",
                    name
                ),
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
            SectionType::Pivot => validate_pivot(section, label, &mut errors),
        }

        // Reject config blocks that don't match the section type
        let extra: &[(&str, bool)] = &[
            ("chart", section.chart.is_some() && section.section_type != SectionType::Chart),
            ("table", section.table.is_some() && section.section_type != SectionType::Table),
            ("stats", section.stats.is_some() && section.section_type != SectionType::Stats),
            ("list", section.list.is_some() && section.section_type != SectionType::List),
            ("markdown", section.markdown.is_some() && section.section_type != SectionType::Markdown),
            ("pivot", section.pivot.is_some() && section.section_type != SectionType::Pivot),
        ];
        for (block, present) in extra {
            if *present {
                errors.push(err(
                    Some(label),
                    format!("\"{}\" config block not allowed on {} section", block, section.section_type.as_spec_str()),
                ));
            }
        }

        // Count markdown sections with active TOC
        if section.section_type == SectionType::Markdown {
            if let Some(ref md) = section.markdown {
                if md.toc_levels.as_ref().is_some_and(|v| !v.is_empty()) {
                    md_toc_count += 1;
                }
            }
        }

        // selectable / batch_actions: only supported on table and list sections
        if section.selectable.is_some() || section.batch_actions.is_some() {
            if !matches!(section.section_type, SectionType::Table | SectionType::List) {
                errors.push(err(
                    Some(label),
                    format!(
                        "selectable and batch_actions are not supported on {} sections",
                        section.section_type.as_spec_str()
                    ),
                ));
            }
        }

        // batch_actions validation
        if let Some(ref actions) = section.batch_actions {
            validate_actions(actions, "batch_actions", label, &mut errors);
            if section.selectable != Some(true) {
                errors.push(err(
                    Some(label),
                    "batch_actions requires selectable: true",
                ));
            }
            // batch_actions requires a stable row identity
            match section.section_type {
                SectionType::Table => {
                    let has_row_id = section.table.as_ref()
                        .and_then(|t| t.row_id_field.as_deref())
                        .is_some_and(|s| !s.trim().is_empty());
                    if !has_row_id {
                        errors.push(err(
                            Some(label),
                            "batch_actions on table requires table.row_id_field",
                        ));
                    }
                }
                SectionType::List => {
                    let has_id_field = section.list.as_ref()
                        .and_then(|l| l.id_field.as_deref())
                        .is_some_and(|s| !s.trim().is_empty());
                    if !has_id_field {
                        errors.push(err(
                            Some(label),
                            "batch_actions on list requires list.id_field",
                        ));
                    }
                }
                _ => {} // already caught by section-type check above
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

fn validate_actions(
    actions: &[ActionDef],
    context: &str,
    label: &str,
    errors: &mut Vec<SpecError>,
) {
    if actions.is_empty() {
        errors.push(err(Some(label), format!("{} list must not be empty", context)));
    }
    let mut seen_ids: HashSet<String> = HashSet::new();
    for action in actions {
        let id = action.id.trim();
        if id.is_empty() {
            errors.push(err(
                Some(label),
                format!("{} contains action with empty id", context),
            ));
        } else if !seen_ids.insert(id.to_string()) {
            errors.push(err(
                Some(label),
                format!("duplicate {} id \"{}\"", context, id),
            ));
        }
        if action.label.trim().is_empty() {
            errors.push(err(
                Some(label),
                format!("{} action \"{}\" has empty label", context, id),
            ));
        }
    }
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

    // chart.encoding must be an object (null/missing is not allowed)
    let enc = match &chart.encoding {
        serde_json::Value::Object(enc) => enc,
        _ => {
            errors.push(err(Some(label), "chart.encoding must be a JSON object"));
            return;
        }
    };

    // interactive_filter.field should appear in encoding
    if let Some(ref filter) = section.interactive_filter {
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

    // row_actions validation
    if let Some(ref actions) = table.row_actions {
        validate_actions(actions, "row_actions", label, errors);
        if table.row_id_field.as_deref().is_none_or(|s| s.trim().is_empty()) {
            errors.push(err(
                Some(label),
                "row_actions requires table.row_id_field",
            ));
        }
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

    if stats.items.is_empty() {
        errors.push(err(Some(label), "stats.items must not be empty"));
    }

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

        // field validation: required for non-count, must not be empty/whitespace if provided
        match item.field.as_deref().map(str::trim) {
            Some("") => {
                errors.push(err(
                    Some(label),
                    format!("aggregate \"{}\" field must not be empty", item.aggregate),
                ));
            }
            None if item.aggregate != "count" => {
                errors.push(err(
                    Some(label),
                    format!("aggregate \"{}\" requires field", item.aggregate),
                ));
            }
            _ => {}
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
    if list.id_field.as_deref().is_none_or(|s| s.trim().is_empty()) {
        errors.push(err(
            Some(label),
            "list section requires list.id_field",
        ));
    }

    // detail.actions validation
    if let Some(ref detail) = list.detail {
        if let Some(ref actions) = detail.actions {
            validate_actions(actions, "detail.actions", label, errors);
        }
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

fn validate_pivot(
    section: &super::schema::Section,
    label: &str,
    errors: &mut Vec<SpecError>,
) {
    let pivot = match &section.pivot {
        Some(p) => p,
        None => {
            errors.push(err(Some(label), "pivot section requires pivot config"));
            return;
        }
    };

    // rows must be non-empty
    if pivot.rows.is_empty() {
        errors.push(err(Some(label), "pivot.rows must not be empty"));
    }

    // values must be non-empty
    if pivot.values.is_empty() {
        errors.push(err(Some(label), "pivot.values must not be empty"));
    }

    let supported_formats = ["currency", "percent"];
    for value in &pivot.values {
        if !SUPPORTED_AGGREGATES.contains(&value.aggregate.as_str()) {
            errors.push(err(
                Some(label),
                format!(
                    "unknown pivot aggregate \"{}\", supported: {:?}",
                    value.aggregate, SUPPORTED_AGGREGATES
                ),
            ));
        }

        if let Some(ref fmt) = value.format {
            if !supported_formats.contains(&fmt.as_str()) {
                errors.push(err(
                    Some(label),
                    format!(
                        "unknown pivot value format \"{}\", supported: {:?}",
                        fmt, supported_formats
                    ),
                ));
            }
            if fmt == "currency" {
                match value.currency.as_deref().map(str::trim) {
                    Some(code) if code.len() == 3 && code.chars().all(|c| c.is_ascii_alphabetic()) => {}
                    _ => {
                        errors.push(err(
                            Some(label),
                            "pivot value format \"currency\" requires a valid 3-letter currency code (e.g. \"USD\", \"EUR\")",
                        ));
                    }
                }
            }
        }

        if value.currency.is_some() && value.format.as_deref() != Some("currency") {
            errors.push(err(
                Some(label),
                "pivot value currency is only valid with format: currency",
            ));
        }
    }

    // Empty field names
    for field in &pivot.rows {
        if field.trim().is_empty() {
            errors.push(err(Some(label), "pivot.rows contains empty field name"));
        }
    }
    for field in &pivot.columns {
        if field.trim().is_empty() {
            errors.push(err(Some(label), "pivot.columns contains empty field name"));
        }
    }
    for value in &pivot.values {
        match value.field.as_deref() {
            None if value.aggregate != "count" => {
                errors.push(err(
                    Some(label),
                    format!("pivot aggregate \"{}\" requires field", value.aggregate),
                ));
            }
            Some(f) if f.trim().is_empty() => {
                errors.push(err(Some(label), "pivot.values contains empty field name"));
            }
            _ => {}
        }
    }

    // Duplicate fields in rows
    let mut seen_row_fields: HashSet<&str> = HashSet::new();
    for field in &pivot.rows {
        if !seen_row_fields.insert(field.as_str()) {
            errors.push(err(
                Some(label),
                format!("duplicate field \"{}\" in pivot.rows", field),
            ));
        }
    }

    // Duplicate fields in columns
    let mut seen_col_fields: HashSet<&str> = HashSet::new();
    for field in &pivot.columns {
        if !seen_col_fields.insert(field.as_str()) {
            errors.push(err(
                Some(label),
                format!("duplicate field \"{}\" in pivot.columns", field),
            ));
        }
    }

    // Overlap between rows and columns
    for field in &pivot.columns {
        if seen_row_fields.contains(field.as_str()) {
            errors.push(err(
                Some(label),
                format!("field \"{}\" appears in both pivot.rows and pivot.columns", field),
            ));
        }
    }

    // show_subtotals requires at least 2 row fields
    if pivot.show_subtotals && pivot.rows.len() < 2 {
        errors.push(err(
            Some(label),
            "pivot.show_subtotals requires at least 2 row fields",
        ));
    }

    // sort.value_index must be within bounds
    if let Some(ref sort) = pivot.sort {
        if let Some(value_index) = sort.value_index {
            if value_index >= pivot.values.len() {
                errors.push(err(
                    Some(label),
                    format!(
                        "pivot.sort.value_index {} out of range for {} values",
                        value_index, pivot.values.len()
                    ),
                ));
            }
        }
    }

    // pivot requires data source
    if section.source.is_none() && section.inline_data.is_none() {
        errors.push(err(
            Some(label),
            "pivot section requires source or inline_data",
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
            toc: false,
            timezone: None,
            theme: None,
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
                pivot: None,
                interactive_filter: None,
                selectable: None,
                batch_actions: None,
            }],
        }
    }

    #[test]
    fn valid_minimal_spec() {
        let spec = minimal_spec();
        let errors = validate(&spec, &BTreeSet::new());
        assert!(errors.is_empty(), "errors: {:?}", errors);
    }

    #[test]
    fn unsupported_spec_version() {
        let mut spec = minimal_spec();
        spec.spec_version = 99;
        let errors = validate(&spec, &BTreeSet::new());
        assert!(errors.iter().any(|e| e.message.contains("unsupported spec_version")));
    }

    #[test]
    fn empty_sections() {
        let mut spec = minimal_spec();
        spec.sections.clear();
        let errors = validate(&spec, &BTreeSet::new());
        assert!(errors.iter().any(|e| e.message.contains("sections list is empty")));
    }

    #[test]
    fn source_references_undeclared_dataset() {
        let mut spec = minimal_spec();
        spec.sections[0].source = Some("nonexistent".to_string());
        let errors = validate(&spec, &BTreeSet::new());
        assert!(errors.iter().any(|e| e.message.contains("not declared in datasets")));
    }

    #[test]
    fn source_and_inline_data_together_is_valid() {
        // source provides shared identity for filtering, inline_data provides content
        let mut spec = minimal_spec();
        spec.datasets.insert("events".to_string(), DatasetDecl {});
        spec.sections[0].source = Some("events".to_string());
        spec.sections[0].inline_data = Some(vec![]);
        // No external datasets needed — inline_data satisfies the "events" declaration
        let errors = validate(&spec, &BTreeSet::new());
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
        let errors = validate(&spec, &BTreeSet::new());
        assert!(errors.iter().any(|e| e.message.contains("unknown chart mark")));
    }

    #[test]
    fn chart_missing_config() {
        let mut spec = minimal_spec();
        spec.sections[0].section_type = SectionType::Chart;
        spec.sections[0].stats = None;
        spec.sections[0].chart = None;
        let errors = validate(&spec, &BTreeSet::new());
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
        let errors = validate(&spec, &BTreeSet::new());
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
        let errors = validate(&spec, &BTreeSet::new());
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
        let errors = validate(&spec, &BTreeSet::new());
        assert!(errors.iter().any(|e| e.message.contains("requires section id")));
    }

    #[test]
    fn table_missing_config() {
        let mut spec = minimal_spec();
        spec.sections[0].section_type = SectionType::Table;
        spec.sections[0].stats = None;
        let errors = validate(&spec, &BTreeSet::new());
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
        let errors = validate(&spec, &BTreeSet::new());
        assert!(errors.iter().any(|e| e.message.contains("row_actions requires table.row_id_field")));
    }

    #[test]
    fn stats_empty_items() {
        let mut spec = minimal_spec();
        spec.sections[0].stats.as_mut().unwrap().items.clear();
        let errors = validate(&spec, &BTreeSet::new());
        assert_eq!(errors.len(), 1, "expected exactly 1 error, got: {:?}", errors);
        assert_eq!(errors[0].message, "stats.items must not be empty");
    }

    #[test]
    fn stats_unknown_aggregate() {
        let mut spec = minimal_spec();
        spec.sections[0].stats.as_mut().unwrap().items[0].aggregate = "median".to_string();
        let errors = validate(&spec, &BTreeSet::new());
        assert!(errors.iter().any(|e| e.message.contains("unknown aggregate \"median\"")));
    }

    #[test]
    fn stats_distinct_requires_field() {
        let mut spec = minimal_spec();
        spec.sections[0].stats.as_mut().unwrap().items[0].aggregate = "distinct".to_string();
        spec.sections[0].stats.as_mut().unwrap().items[0].field = None;
        let errors = validate(&spec, &BTreeSet::new());
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
        let errors = validate(&spec, &BTreeSet::new());
        assert!(errors.iter().any(|e| e.message.contains("requires list.id_field")));
    }

    #[test]
    fn markdown_missing_config() {
        let mut spec = minimal_spec();
        spec.sections[0].section_type = SectionType::Markdown;
        spec.sections[0].stats = None;
        let errors = validate(&spec, &BTreeSet::new());
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
        let errors = validate(&spec, &BTreeSet::new());
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
        let errors = validate(&spec, &BTreeSet::new());
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
        let provided = BTreeSet::from(["notes".to_string()]);
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
        let errors = validate(&spec, &BTreeSet::new());
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
                pivot: None,
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
                pivot: None,
                interactive_filter: None,
                selectable: None,
                batch_actions: None,
            },
        ];
        let provided = BTreeSet::from(["events".to_string()]);
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
        let errors = validate(&spec, &BTreeSet::new());
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
        let errors = validate(&spec, &BTreeSet::new());
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
        let errors = validate(&spec, &BTreeSet::new());
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
        let errors = validate(&spec, &BTreeSet::new());
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
        let errors = validate(&spec, &BTreeSet::new());
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
        let errors = validate(&spec, &BTreeSet::new());
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
        let errors = validate(&spec, &BTreeSet::new());
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
        let errors = validate(&spec, &BTreeSet::new());
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
                pivot: None,
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
                pivot: None,
                interactive_filter: None,
                selectable: None,
                batch_actions: None,
            },
        ];
        let errors = validate(&spec, &BTreeSet::new());
        assert!(errors.iter().any(|e| e.message.contains("only one markdown section may define toc_levels")));
    }

    #[test]
    fn declared_dataset_not_provided() {
        let mut spec = minimal_spec();
        spec.datasets.insert("events".to_string(), DatasetDecl {});
        let errors = validate(&spec, &BTreeSet::new());
        assert_eq!(errors.len(), 1, "expected exactly one error, got: {:?}", errors);
        assert!(errors[0].message.contains("dataset \"events\" is declared but no data was provided"));
        assert!(errors[0].section.is_none());
    }

    #[test]
    fn declared_dataset_provided_externally() {
        let mut spec = minimal_spec();
        spec.datasets.insert("events".to_string(), DatasetDecl {});
        spec.sections[0].source = Some("events".to_string());
        let external = BTreeSet::from(["events".to_string()]);
        let errors = validate(&spec, &external);
        assert!(errors.is_empty(), "errors: {:?}", errors);
    }

    #[test]
    fn declared_dataset_provided_via_inline_data() {
        // A declared dataset satisfied by inline_data (no external data needed)
        let mut spec = minimal_spec();
        spec.datasets.insert("events".to_string(), DatasetDecl {});
        spec.sections[0].source = Some("events".to_string());
        spec.sections[0].inline_data = Some(vec![]);
        let errors = validate(&spec, &BTreeSet::new());
        assert!(errors.is_empty(), "errors: {:?}", errors);
    }

    #[test]
    fn external_dataset_not_declared() {
        let spec = minimal_spec();
        let external = BTreeSet::from(["unknown".to_string()]);
        let errors = validate(&spec, &external);
        assert_eq!(errors.len(), 1, "expected exactly one error, got: {:?}", errors);
        assert!(errors[0].message.contains("provided dataset \"unknown\" is not declared"));
    }

    #[test]
    fn multiple_external_datasets_not_declared_deterministic_order() {
        let spec = minimal_spec();
        let external = BTreeSet::from(["zebra".to_string(), "alpha".to_string()]);
        let errors = validate(&spec, &external);
        let undeclared: Vec<&str> = errors
            .iter()
            .filter(|e| e.message.contains("not declared"))
            .map(|e| e.message.as_str())
            .collect();
        assert_eq!(undeclared.len(), 2);
        // BTreeSet guarantees alphabetical order
        assert!(undeclared[0].contains("alpha"));
        assert!(undeclared[1].contains("zebra"));
    }

    #[test]
    fn inline_dataset_named_by_source_not_flagged() {
        // inline_data section with source → dataset name comes from source
        let mut spec = minimal_spec();
        spec.datasets.insert("events".to_string(), DatasetDecl {});
        spec.sections[0].source = Some("events".to_string());
        spec.sections[0].inline_data = Some(vec![]);
        // No external datasets — inline_data satisfies the declaration
        let errors = validate(&spec, &BTreeSet::new());
        assert!(errors.is_empty(), "errors: {:?}", errors);
    }

    #[test]
    fn inline_dataset_named_by_id_not_flagged() {
        // inline_data section with id but no source → dataset name comes from id
        let mut spec = minimal_spec();
        spec.sections[0].id = Some("my_data".to_string());
        spec.sections[0].inline_data = Some(vec![]);
        // Declared dataset matches the id
        spec.datasets.insert("my_data".to_string(), DatasetDecl {});
        let errors = validate(&spec, &BTreeSet::new());
        assert!(errors.is_empty(), "errors: {:?}", errors);
    }

    #[test]
    fn inline_dataset_synthetic_name_no_false_positive() {
        // inline_data section with no source and no id → _inline_0
        // This synthetic name is NOT in spec.datasets, which is fine
        let mut spec = minimal_spec();
        spec.sections[0].inline_data = Some(vec![]);
        let errors = validate(&spec, &BTreeSet::new());
        assert!(
            !errors.iter().any(|e| e.message.contains("not declared")),
            "synthetic inline name should not trigger undeclared error: {:?}",
            errors,
        );
    }

    #[test]
    fn multiple_inline_sections_all_exempt() {
        let mut spec = minimal_spec();
        spec.datasets.insert("ds1".to_string(), DatasetDecl {});
        spec.sections[0].source = Some("ds1".to_string());
        spec.sections[0].inline_data = Some(vec![]);
        // Add a second inline section with id
        spec.sections.push(Section {
            id: Some("ds2".to_string()),
            title: "S2".to_string(),
            section_type: SectionType::Stats,
            source: None,
            inline_data: Some(vec![]),
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
            pivot: None,
            interactive_filter: None,
            selectable: None,
            batch_actions: None,
        });
        let errors = validate(&spec, &BTreeSet::new());
        assert!(errors.is_empty(), "errors: {:?}", errors);
    }

    #[test]
    fn extra_config_block_rejected() {
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
            row_actions: None,
        });
        // Add a chart block that doesn't match the table type
        spec.sections[0].chart = Some(ChartConfig {
            mark: "bar".to_string(),
            encoding: serde_json::json!({}),
        });
        let errors = validate(&spec, &BTreeSet::new());
        assert!(errors.iter().any(|e| e.message.contains("\"chart\" config block not allowed on table section")));
    }

    #[test]
    fn multiple_extra_config_blocks_all_reported() {
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
            row_actions: None,
        });
        spec.sections[0].chart = Some(ChartConfig {
            mark: "bar".to_string(),
            encoding: serde_json::json!({}),
        });
        spec.sections[0].markdown = Some(MarkdownConfig {
            content: Some("x".to_string()),
            content_field: None,
            toc_levels: None,
            toc_side: None,
            link_target: None,
            max_rows: None,
        });
        let errors = validate(&spec, &BTreeSet::new());
        assert!(errors.iter().any(|e| e.message.contains("\"chart\" config block not allowed on table section")));
        assert!(errors.iter().any(|e| e.message.contains("\"markdown\" config block not allowed on table section")));
    }

    #[test]
    fn extra_config_block_on_chart_section() {
        let mut spec = minimal_spec();
        spec.sections[0].section_type = SectionType::Chart;
        spec.sections[0].stats = None;
        spec.sections[0].chart = Some(ChartConfig {
            mark: "bar".to_string(),
            encoding: serde_json::json!({}),
        });
        spec.sections[0].list = Some(ListConfig {
            id_field: Some("id".to_string()),
            layout: None,
            title_field: None,
            subtitle_field: None,
            meta_field: None,
            preview_field: None,
            item_click: None,
            detail: None,
            on_action: None,
        });
        let errors = validate(&spec, &BTreeSet::new());
        assert!(errors.iter().any(|e| e.message.contains("\"list\" config block not allowed on chart section")));
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
                pivot: None,
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
                pivot: None,
                interactive_filter: None,
                selectable: None,
                batch_actions: None,
            },
        ];
        let errors = validate(&spec, &BTreeSet::new());
        assert!(errors.iter().any(|e| e.message.contains("duplicate section id")));
    }

    // --- pivot validation ---

    #[test]
    fn pivot_valid() {
        let mut spec = minimal_spec();
        let provided = BTreeSet::from(["sales".to_string()]);
        spec.datasets.insert("sales".to_string(), DatasetDecl {});
        spec.sections[0].section_type = SectionType::Pivot;
        spec.sections[0].stats = None;
        spec.sections[0].source = Some("sales".to_string());
        spec.sections[0].pivot = Some(PivotConfig {
            rows: vec!["region".to_string()],
            columns: vec!["quarter".to_string()],
            values: vec![PivotValue {
                field: Some("revenue".to_string()),
                aggregate: "sum".to_string(),
                label: Some("Revenue".to_string()),
                format: None,
                currency: None,
            }],
            show_totals: true,
            show_subtotals: false,
            sort: None,
        });
        let errors = validate(&spec, &provided);
        assert!(errors.is_empty(), "errors: {:?}", errors);
    }

    #[test]
    fn pivot_missing_config() {
        let mut spec = minimal_spec();
        spec.sections[0].section_type = SectionType::Pivot;
        spec.sections[0].stats = None;
        spec.sections[0].pivot = None;
        spec.sections[0].inline_data = Some(vec![]);
        let errors = validate(&spec, &BTreeSet::new());
        assert!(errors.iter().any(|e| e.message.contains("pivot section requires pivot config")));
    }

    #[test]
    fn pivot_empty_rows() {
        let mut spec = minimal_spec();
        spec.sections[0].section_type = SectionType::Pivot;
        spec.sections[0].stats = None;
        spec.sections[0].inline_data = Some(vec![]);
        spec.sections[0].pivot = Some(PivotConfig {
            rows: vec![],
            columns: vec![],
            values: vec![PivotValue {
                field: Some("x".to_string()),
                aggregate: "sum".to_string(),
                label: None,
                format: None,
                currency: None,
            }],
            show_totals: false,
            show_subtotals: false,
            sort: None,
        });
        let errors = validate(&spec, &BTreeSet::new());
        assert!(errors.iter().any(|e| e.message.contains("pivot.rows must not be empty")));
    }

    #[test]
    fn pivot_empty_values() {
        let mut spec = minimal_spec();
        spec.sections[0].section_type = SectionType::Pivot;
        spec.sections[0].stats = None;
        spec.sections[0].inline_data = Some(vec![]);
        spec.sections[0].pivot = Some(PivotConfig {
            rows: vec!["a".to_string()],
            columns: vec![],
            values: vec![],
            show_totals: false,
            show_subtotals: false,
            sort: None,
        });
        let errors = validate(&spec, &BTreeSet::new());
        assert!(errors.iter().any(|e| e.message.contains("pivot.values must not be empty")));
    }

    #[test]
    fn pivot_unknown_aggregate() {
        let mut spec = minimal_spec();
        spec.sections[0].section_type = SectionType::Pivot;
        spec.sections[0].stats = None;
        spec.sections[0].inline_data = Some(vec![]);
        spec.sections[0].pivot = Some(PivotConfig {
            rows: vec!["a".to_string()],
            columns: vec![],
            values: vec![PivotValue {
                field: Some("x".to_string()),
                aggregate: "median".to_string(),
                label: None,
                format: None,
                currency: None,
            }],
            show_totals: false,
            show_subtotals: false,
            sort: None,
        });
        let errors = validate(&spec, &BTreeSet::new());
        assert!(errors.iter().any(|e| e.message.contains("unknown pivot aggregate \"median\"")));
    }

    #[test]
    fn pivot_requires_data_source() {
        let mut spec = minimal_spec();
        spec.sections[0].section_type = SectionType::Pivot;
        spec.sections[0].stats = None;
        spec.sections[0].source = None;
        spec.sections[0].inline_data = None;
        spec.sections[0].pivot = Some(PivotConfig {
            rows: vec!["a".to_string()],
            columns: vec![],
            values: vec![PivotValue {
                field: Some("x".to_string()),
                aggregate: "sum".to_string(),
                label: None,
                format: None,
                currency: None,
            }],
            show_totals: false,
            show_subtotals: false,
            sort: None,
        });
        let errors = validate(&spec, &BTreeSet::new());
        assert!(errors.iter().any(|e| e.message.contains("pivot section requires source or inline_data")));
    }

    // -- Helper: creates a table section suitable for batch_actions tests --
    fn table_section_with_batch_actions(
        selectable: Option<bool>,
        batch_actions: Option<Vec<ActionDef>>,
        row_id_field: Option<String>,
    ) -> Section {
        Section {
            id: None,
            title: "Items".to_string(),
            section_type: SectionType::Table,
            source: None,
            inline_data: None,
            chart: None,
            table: Some(TableConfig {
                columns: vec![ColumnDef {
                    field: "name".to_string(),
                    title: None,
                    width: None,
                    sort: None,
                }],
                row_id_field,
                row_actions: None,
            }),
            stats: None,
            list: None,
            markdown: None,
            pivot: None,
            interactive_filter: None,
            selectable,
            batch_actions,
        }
    }

    fn sample_actions() -> Vec<ActionDef> {
        vec![
            ActionDef { id: "approve".to_string(), label: "Approve".to_string(), style: None },
            ActionDef { id: "reject".to_string(), label: "Reject".to_string(), style: None },
        ]
    }

    // -- batch_actions: section type constraints --

    #[test]
    fn batch_actions_rejected_on_stats() {
        let mut spec = minimal_spec();
        spec.sections[0].selectable = Some(true);
        spec.sections[0].batch_actions = Some(sample_actions());
        let errors = validate(&spec, &BTreeSet::new());
        assert!(errors.iter().any(|e| e.message.contains("not supported on stats sections")));
    }

    #[test]
    fn selectable_rejected_on_markdown() {
        let mut spec = minimal_spec();
        spec.sections[0].section_type = SectionType::Markdown;
        spec.sections[0].stats = None;
        spec.sections[0].markdown = Some(MarkdownConfig {
            content: Some("hello".to_string()),
            content_field: None,
            toc_levels: None,
            toc_side: None,
            link_target: None,
            max_rows: None,
        });
        spec.sections[0].selectable = Some(true);
        let errors = validate(&spec, &BTreeSet::new());
        assert!(errors.iter().any(|e| e.message.contains("not supported on markdown sections")));
    }

    // -- batch_actions: requires selectable --

    #[test]
    fn batch_actions_requires_selectable() {
        let mut spec = minimal_spec();
        spec.sections[0] = table_section_with_batch_actions(
            None, // selectable not set
            Some(sample_actions()),
            Some("id".to_string()),
        );
        let errors = validate(&spec, &BTreeSet::new());
        assert!(errors.iter().any(|e| e.message.contains("batch_actions requires selectable: true")));
    }

    #[test]
    fn batch_actions_with_selectable_false() {
        let mut spec = minimal_spec();
        spec.sections[0] = table_section_with_batch_actions(
            Some(false),
            Some(sample_actions()),
            Some("id".to_string()),
        );
        let errors = validate(&spec, &BTreeSet::new());
        assert!(errors.iter().any(|e| e.message.contains("batch_actions requires selectable: true")));
    }

    // -- batch_actions: requires row identity --

    #[test]
    fn batch_actions_on_table_requires_row_id_field() {
        let mut spec = minimal_spec();
        spec.sections[0] = table_section_with_batch_actions(
            Some(true),
            Some(sample_actions()),
            None, // no row_id_field
        );
        let errors = validate(&spec, &BTreeSet::new());
        assert!(errors.iter().any(|e| e.message.contains("batch_actions on table requires table.row_id_field")));
    }

    // -- batch_actions: empty list --

    #[test]
    fn batch_actions_empty_rejected() {
        let mut spec = minimal_spec();
        spec.sections[0] = table_section_with_batch_actions(
            Some(true),
            Some(vec![]),
            Some("id".to_string()),
        );
        let errors = validate(&spec, &BTreeSet::new());
        assert!(errors.iter().any(|e| e.message.contains("batch_actions list must not be empty")));
    }

    // -- batch_actions: duplicate IDs --

    #[test]
    fn batch_actions_duplicate_ids() {
        let mut spec = minimal_spec();
        spec.sections[0] = table_section_with_batch_actions(
            Some(true),
            Some(vec![
                ActionDef { id: "approve".to_string(), label: "Approve".to_string(), style: None },
                ActionDef { id: "approve".to_string(), label: "Approve Again".to_string(), style: None },
            ]),
            Some("id".to_string()),
        );
        let errors = validate(&spec, &BTreeSet::new());
        assert!(errors.iter().any(|e| e.message.contains("duplicate batch_actions id \"approve\"")));
    }

    // -- batch_actions: empty/blank action IDs --

    #[test]
    fn batch_actions_empty_action_id_rejected() {
        let mut spec = minimal_spec();
        spec.sections[0] = table_section_with_batch_actions(
            Some(true),
            Some(vec![
                ActionDef { id: "".to_string(), label: "No ID".to_string(), style: None },
            ]),
            Some("id".to_string()),
        );
        let errors = validate(&spec, &BTreeSet::new());
        assert!(errors.iter().any(|e| e.message.contains("batch_actions contains action with empty id")));
    }

    #[test]
    fn batch_actions_blank_action_label_rejected() {
        let mut spec = minimal_spec();
        spec.sections[0] = table_section_with_batch_actions(
            Some(true),
            Some(vec![
                ActionDef { id: "approve".to_string(), label: "   ".to_string(), style: None },
            ]),
            Some("id".to_string()),
        );
        let errors = validate(&spec, &BTreeSet::new());
        assert!(errors.iter().any(|e| e.message.contains("has empty label")));
    }

    // -- batch_actions: valid --

    #[test]
    fn batch_actions_valid_on_table() {
        let mut spec = minimal_spec();
        spec.sections[0] = table_section_with_batch_actions(
            Some(true),
            Some(sample_actions()),
            Some("id".to_string()),
        );
        let errors = validate(&spec, &BTreeSet::new());
        assert!(errors.is_empty(), "expected no errors, got: {:?}", errors);
    }

    // -- row_actions: validation via shared helper --

    #[test]
    fn row_actions_empty_rejected() {
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
            row_id_field: Some("id".to_string()),
            row_actions: Some(vec![]),
        });
        let errors = validate(&spec, &BTreeSet::new());
        assert!(errors.iter().any(|e| e.message.contains("row_actions list must not be empty")));
    }

    #[test]
    fn row_actions_duplicate_ids_rejected() {
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
            row_id_field: Some("id".to_string()),
            row_actions: Some(vec![
                ActionDef { id: "edit".to_string(), label: "Edit".to_string(), style: None },
                ActionDef { id: "edit".to_string(), label: "Edit 2".to_string(), style: None },
            ]),
        });
        let errors = validate(&spec, &BTreeSet::new());
        assert!(errors.iter().any(|e| e.message.contains("duplicate row_actions id \"edit\"")));
    }

    // -- selectable: false on unsupported section --

    #[test]
    fn selectable_false_rejected_on_stats() {
        let mut spec = minimal_spec();
        spec.sections[0].selectable = Some(false);
        let errors = validate(&spec, &BTreeSet::new());
        assert!(errors.iter().any(|e| e.message.contains("not supported on stats sections")));
    }

    // -- batch_actions: list section --

    #[test]
    fn batch_actions_valid_on_list() {
        let mut spec = minimal_spec();
        spec.sections[0].section_type = SectionType::List;
        spec.sections[0].stats = None;
        spec.sections[0].selectable = Some(true);
        spec.sections[0].list = Some(ListConfig {
            id_field: Some("id".to_string()),
            layout: None,
            title_field: None,
            subtitle_field: None,
            meta_field: None,
            preview_field: None,
            item_click: None,
            detail: None,
            on_action: None,
        });
        spec.sections[0].batch_actions = Some(sample_actions());
        let errors = validate(&spec, &BTreeSet::new());
        assert!(errors.is_empty(), "expected no errors, got: {:?}", errors);
    }

    #[test]
    fn batch_actions_on_list_requires_id_field() {
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
        spec.sections[0].batch_actions = Some(sample_actions());
        let errors = validate(&spec, &BTreeSet::new());
        assert!(errors.iter().any(|e| e.message.contains("batch_actions on list requires list.id_field")));
    }

    // -- blank identity fields --

    #[test]
    fn blank_row_id_field_rejected_for_batch_actions() {
        let mut spec = minimal_spec();
        spec.sections[0] = table_section_with_batch_actions(
            Some(true),
            Some(sample_actions()),
            Some("   ".to_string()),
        );
        let errors = validate(&spec, &BTreeSet::new());
        assert!(errors.iter().any(|e| e.message.contains("batch_actions on table requires table.row_id_field")));
    }

    #[test]
    fn blank_row_id_field_rejected_for_row_actions() {
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
            row_id_field: Some("".to_string()),
            row_actions: Some(sample_actions()),
        });
        let errors = validate(&spec, &BTreeSet::new());
        assert!(errors.iter().any(|e| e.message.contains("row_actions requires table.row_id_field")));
    }

    // -- trimmed duplicate detection --

    #[test]
    fn batch_actions_whitespace_duplicate_ids_caught() {
        let mut spec = minimal_spec();
        spec.sections[0] = table_section_with_batch_actions(
            Some(true),
            Some(vec![
                ActionDef { id: "approve".to_string(), label: "Approve".to_string(), style: None },
                ActionDef { id: " approve ".to_string(), label: "Approve 2".to_string(), style: None },
            ]),
            Some("id".to_string()),
        );
        let errors = validate(&spec, &BTreeSet::new());
        assert!(errors.iter().any(|e| e.message.contains("duplicate batch_actions id \"approve\"")));
    }

    // -- interactive_filter: null encoding --

    #[test]
    fn interactive_filter_null_encoding_rejected() {
        let mut spec = minimal_spec();
        spec.sections[0].id = Some("c1".to_string());
        spec.sections[0].section_type = SectionType::Chart;
        spec.sections[0].stats = None;
        spec.sections[0].chart = Some(ChartConfig {
            mark: "bar".to_string(),
            encoding: serde_json::Value::Null,
        });
        spec.sections[0].interactive_filter = Some(InteractiveFilter {
            field: "country".to_string(),
        });
        let errors = validate(&spec, &BTreeSet::new());
        assert!(errors.iter().any(|e| e.message.contains("chart.encoding must be a JSON object")));
    }

    // -- detail.actions validation --

    #[test]
    fn detail_actions_duplicate_ids_rejected() {
        let mut spec = minimal_spec();
        spec.sections[0].section_type = SectionType::List;
        spec.sections[0].stats = None;
        spec.sections[0].list = Some(ListConfig {
            id_field: Some("id".to_string()),
            layout: None,
            title_field: None,
            subtitle_field: None,
            meta_field: None,
            preview_field: None,
            item_click: None,
            detail: Some(DetailConfig {
                fields: None,
                body_field: None,
                body_format: None,
                actions: Some(vec![
                    ActionDef { id: "edit".to_string(), label: "Edit".to_string(), style: None },
                    ActionDef { id: "edit".to_string(), label: "Edit 2".to_string(), style: None },
                ]),
            }),
            on_action: None,
        });
        let errors = validate(&spec, &BTreeSet::new());
        assert!(errors.iter().any(|e| e.message.contains("duplicate detail.actions id \"edit\"")));
    }

}
