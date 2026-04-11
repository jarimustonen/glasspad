use std::collections::BTreeSet;

use glasspad::spec::schema::DashboardSpec;
use glasspad::spec::validate;

fn load_fixture(name: &str) -> String {
    std::fs::read_to_string(format!("tests/fixtures/{}", name)).unwrap()
}

#[test]
fn parse_valid_dashboard() {
    let yaml = load_fixture("valid_dashboard.yaml");
    let spec: DashboardSpec = serde_yaml::from_str(&yaml).unwrap();

    assert_eq!(spec.spec_version, 1);
    assert_eq!(spec.title, "Test Analytics");
    assert_eq!(spec.sections.len(), 4);
    assert!(spec.datasets.contains_key("events"));
}

#[test]
fn validate_valid_dashboard() {
    let yaml = load_fixture("valid_dashboard.yaml");
    let spec: DashboardSpec = serde_yaml::from_str(&yaml).unwrap();
    let provided = BTreeSet::from(["events".to_string()]);
    let errors = validate::validate(&spec, &provided);
    assert!(errors.is_empty(), "errors: {:?}", errors);
}

#[test]
fn validate_declared_dataset_without_provided_data() {
    let yaml = load_fixture("valid_dashboard.yaml");
    let spec: DashboardSpec = serde_yaml::from_str(&yaml).unwrap();
    let errors = validate::validate(&spec, &BTreeSet::new());
    assert_eq!(errors.len(), 1, "expected exactly one error, got: {:?}", errors);
    assert!(errors[0].message.contains("dataset \"events\" is declared but no data was provided"));
}

#[test]
fn parse_invalid_no_version_fails() {
    let yaml = load_fixture("invalid_no_version.yaml");
    let result = serde_yaml::from_str::<DashboardSpec>(&yaml);
    assert!(result.is_err());
}

#[test]
fn validate_bad_aggregate() {
    let yaml = load_fixture("invalid_bad_aggregate.yaml");
    let spec: DashboardSpec = serde_yaml::from_str(&yaml).unwrap();
    let errors = validate::validate(&spec, &BTreeSet::new());
    assert_eq!(errors.len(), 2, "expected 2 errors, got: {:?}", errors);
    assert!(errors.iter().any(|e| e.message.contains("unknown aggregate")));
    assert!(errors.iter().any(|e| e.message.contains("requires field")));
}

#[test]
fn sections_have_correct_types() {
    let yaml = load_fixture("valid_dashboard.yaml");
    let spec: DashboardSpec = serde_yaml::from_str(&yaml).unwrap();

    assert_eq!(spec.sections[0].section_type, glasspad::spec::schema::SectionType::Chart);
    assert_eq!(spec.sections[1].section_type, glasspad::spec::schema::SectionType::Chart);
    assert_eq!(spec.sections[2].section_type, glasspad::spec::schema::SectionType::Stats);
    assert_eq!(spec.sections[3].section_type, glasspad::spec::schema::SectionType::Table);
}

#[test]
fn interactive_filter_parsed_correctly() {
    let yaml = load_fixture("valid_dashboard.yaml");
    let spec: DashboardSpec = serde_yaml::from_str(&yaml).unwrap();

    assert_eq!(spec.sections[0].interactive_filter.as_ref().unwrap().field, "country");
    assert_eq!(spec.sections[1].interactive_filter.as_ref().unwrap().field, "device");
    assert!(spec.sections[2].interactive_filter.is_none());
    assert!(spec.sections[3].interactive_filter.is_none());
}

// --- markdown section tests ---

#[test]
fn parse_valid_markdown() {
    let yaml = load_fixture("valid_markdown.yaml");
    let spec: DashboardSpec = serde_yaml::from_str(&yaml).unwrap();

    assert_eq!(spec.sections.len(), 2);
    assert_eq!(spec.sections[0].section_type, glasspad::spec::schema::SectionType::Markdown);
    assert_eq!(spec.sections[1].section_type, glasspad::spec::schema::SectionType::Markdown);

    let md0 = spec.sections[0].markdown.as_ref().unwrap();
    assert!(md0.content.is_some());
    assert!(md0.content.as_ref().unwrap().contains("# Project Overview"));
    assert!(md0.content_field.is_none());

    let md1 = spec.sections[1].markdown.as_ref().unwrap();
    assert!(md1.content.is_none());
    assert_eq!(md1.content_field.as_deref(), Some("body"));
}

#[test]
fn validate_valid_markdown() {
    let yaml = load_fixture("valid_markdown.yaml");
    let spec: DashboardSpec = serde_yaml::from_str(&yaml).unwrap();
    let provided = BTreeSet::from(["docs".to_string()]);
    let errors = validate::validate(&spec, &provided);
    assert!(errors.is_empty(), "errors: {:?}", errors);
}

#[test]
fn reject_typo_in_markdown_config() {
    let yaml = r#"
spec_version: 1
title: "Test"
sections:
  - title: "Doc"
    type: markdown
    markdown:
      contnet: "typo field"
"#;
    let result = serde_yaml::from_str::<DashboardSpec>(yaml);
    assert!(result.is_err(), "Should reject unknown field 'contnet'");
}

// --- deny_unknown_fields tests ---

#[test]
fn reject_typo_in_top_level_field() {
    let yaml = load_fixture("invalid_typo_field.yaml");
    let result = serde_yaml::from_str::<DashboardSpec>(&yaml);
    assert!(result.is_err(), "Should reject unknown field 'sectons'");
    let err = result.unwrap_err().to_string();
    assert!(err.contains("unknown field"), "Error: {}", err);
}

#[test]
fn reject_typo_in_section() {
    let yaml = r#"
spec_version: 1
title: "Test"
sections:
  - title: "Chart"
    type: chart
    chrat:
      mark: bar
      encoding: {}
"#;
    let result = serde_yaml::from_str::<DashboardSpec>(yaml);
    assert!(result.is_err(), "Should reject unknown field 'chrat'");
}

#[test]
fn reject_typo_in_interactive_filter() {
    let yaml = r#"
spec_version: 1
title: "Test"
sections:
  - id: c1
    title: "Chart"
    type: chart
    interactive_filter:
      feild: country
    chart:
      mark: bar
      encoding: {}
"#;
    let result = serde_yaml::from_str::<DashboardSpec>(yaml);
    assert!(result.is_err(), "Should reject unknown field 'feild'");
}

// --- validation rule tests ---

#[test]
fn validate_encoding_must_be_object() {
    let yaml = r#"
spec_version: 1
title: "Test"
sections:
  - title: "Chart"
    type: chart
    inline_data:
      - { x: 1 }
    chart:
      mark: bar
      encoding: "not an object"
"#;
    let spec: DashboardSpec = serde_yaml::from_str(yaml).unwrap();
    let errors = validate::validate(&spec, &BTreeSet::new());
    assert_eq!(errors.len(), 1, "expected 1 error, got: {:?}", errors);
    assert!(errors[0].message.contains("chart.encoding must be a JSON object"));
}

#[test]
fn validate_null_encoding_rejected_without_filter() {
    let yaml = r#"
spec_version: 1
title: "Test"
sections:
  - id: c1
    title: "Chart"
    type: chart
    inline_data:
      - { x: 1 }
    chart:
      mark: bar
      encoding: null
"#;
    let spec: DashboardSpec = serde_yaml::from_str(yaml).unwrap();
    let errors = validate::validate(&spec, &BTreeSet::new());
    assert_eq!(errors.len(), 1, "expected 1 error, got: {:?}", errors);
    assert!(errors[0].message.contains("chart.encoding must be a JSON object"),
        "null encoding should be rejected even without interactive_filter, got: {:?}", errors);
}

#[test]
fn validate_missing_encoding_rejected_without_filter() {
    let yaml = r#"
spec_version: 1
title: "Test"
sections:
  - id: c1
    title: "Chart"
    type: chart
    inline_data:
      - { x: 1 }
    chart:
      mark: bar
"#;
    let spec: DashboardSpec = serde_yaml::from_str(yaml).unwrap();
    let errors = validate::validate(&spec, &BTreeSet::new());
    assert_eq!(errors.len(), 1, "expected 1 error, got: {:?}", errors);
    assert!(errors[0].message.contains("chart.encoding must be a JSON object"),
        "missing encoding should be rejected even without interactive_filter, got: {:?}", errors);
}

#[test]
fn validate_interactive_filter_on_non_chart_rejected() {
    let yaml = r#"
spec_version: 1
title: "Test"
sections:
  - id: s1
    title: "Stats"
    type: stats
    inline_data:
      - { x: 1 }
    interactive_filter:
      field: country
    stats:
      items:
        - { label: "Total", aggregate: count }
"#;
    let spec: DashboardSpec = serde_yaml::from_str(yaml).unwrap();
    let errors = validate::validate(&spec, &BTreeSet::new());
    assert_eq!(errors.len(), 1, "expected 1 error, got: {:?}", errors);
    assert!(errors[0].message.contains("only supported on chart"));
}

// --- data source validation tests ---

#[test]
fn validate_chart_requires_data_source() {
    let yaml = r#"
spec_version: 1
title: "Test"
sections:
  - title: "Chart"
    type: chart
    chart:
      mark: bar
      encoding: {}
"#;
    let spec: DashboardSpec = serde_yaml::from_str(yaml).unwrap();
    let errors = validate::validate(&spec, &BTreeSet::new());
    assert_eq!(errors.len(), 1, "expected 1 error, got: {:?}", errors);
    assert!(errors[0].message.contains("chart section requires source or inline_data"));
}

#[test]
fn validate_table_requires_data_source() {
    let yaml = r#"
spec_version: 1
title: "Test"
sections:
  - title: "Table"
    type: table
    table:
      columns:
        - { field: name }
"#;
    let spec: DashboardSpec = serde_yaml::from_str(yaml).unwrap();
    let errors = validate::validate(&spec, &BTreeSet::new());
    assert_eq!(errors.len(), 1, "expected 1 error, got: {:?}", errors);
    assert!(errors[0].message.contains("table section requires source or inline_data"));
}

#[test]
fn validate_stats_requires_data_source() {
    let yaml = r#"
spec_version: 1
title: "Test"
sections:
  - title: "Stats"
    type: stats
    stats:
      items:
        - { label: "Total", aggregate: count }
"#;
    let spec: DashboardSpec = serde_yaml::from_str(yaml).unwrap();
    let errors = validate::validate(&spec, &BTreeSet::new());
    assert_eq!(errors.len(), 1, "expected 1 error, got: {:?}", errors);
    assert!(errors[0].message.contains("stats section requires source or inline_data"));
}

// --- section id validation tests ---

#[test]
fn validate_section_id_empty_rejected() {
    let yaml = r#"
spec_version: 1
title: "Test"
sections:
  - id: ""
    title: "Chart"
    type: chart
    inline_data:
      - { x: 1 }
    chart:
      mark: bar
      encoding: {}
"#;
    let spec: DashboardSpec = serde_yaml::from_str(yaml).unwrap();
    let errors = validate::validate(&spec, &BTreeSet::new());
    assert_eq!(errors.len(), 1, "expected 1 error, got: {:?}", errors);
    assert!(errors[0].message.contains("must not be empty or whitespace"));
}

#[test]
fn validate_section_id_whitespace_rejected() {
    let yaml = r#"
spec_version: 1
title: "Test"
sections:
  - id: "   "
    title: "Chart"
    type: chart
    inline_data:
      - { x: 1 }
    chart:
      mark: bar
      encoding: {}
"#;
    let spec: DashboardSpec = serde_yaml::from_str(yaml).unwrap();
    let errors = validate::validate(&spec, &BTreeSet::new());
    assert_eq!(errors.len(), 1, "expected 1 error, got: {:?}", errors);
    assert!(errors[0].message.contains("must not be empty or whitespace"));
}

#[test]
fn validate_section_id_invalid_chars_rejected() {
    let yaml = r#"
spec_version: 1
title: "Test"
sections:
  - id: "my section!"
    title: "Chart"
    type: chart
    inline_data:
      - { x: 1 }
    chart:
      mark: bar
      encoding: {}
"#;
    let spec: DashboardSpec = serde_yaml::from_str(yaml).unwrap();
    let errors = validate::validate(&spec, &BTreeSet::new());
    assert_eq!(errors.len(), 1, "expected 1 error, got: {:?}", errors);
    assert!(errors[0].message.contains("contains invalid characters"));
}

#[test]
fn validate_section_id_duplicate_detected() {
    let yaml = r#"
spec_version: 1
title: "Test"
sections:
  - id: "foo"
    title: "First"
    type: chart
    inline_data:
      - { x: 1 }
    chart:
      mark: bar
      encoding: {}
  - id: "foo"
    title: "Second"
    type: chart
    inline_data:
      - { x: 1 }
    chart:
      mark: bar
      encoding: {}
"#;
    let spec: DashboardSpec = serde_yaml::from_str(yaml).unwrap();
    let errors = validate::validate(&spec, &BTreeSet::new());
    assert_eq!(errors.len(), 1, "expected 1 error, got: {:?}", errors);
    assert!(errors[0].message.contains("duplicate section id"));
}

#[test]
fn validate_interactive_filter_with_missing_encoding_rejected() {
    let yaml = r#"
spec_version: 1
title: "Test"
sections:
  - id: c1
    title: "Chart"
    type: chart
    inline_data:
      - { x: 1 }
    interactive_filter:
      field: country
    chart:
      mark: bar
"#;
    let spec: DashboardSpec = serde_yaml::from_str(yaml).unwrap();
    let errors = validate::validate(&spec, &BTreeSet::new());
    assert_eq!(errors.len(), 1, "expected 1 error, got: {:?}", errors);
    assert!(errors[0].message.contains("chart.encoding must be a JSON object"),
        "expected encoding error, got: {:?}", errors);
}

#[test]
fn validate_interactive_filter_with_explicit_null_encoding_rejected() {
    let yaml = r#"
spec_version: 1
title: "Test"
sections:
  - id: c1
    title: "Chart"
    type: chart
    inline_data:
      - { x: 1 }
    interactive_filter:
      field: country
    chart:
      mark: bar
      encoding: null
"#;
    let spec: DashboardSpec = serde_yaml::from_str(yaml).unwrap();
    let errors = validate::validate(&spec, &BTreeSet::new());
    assert_eq!(errors.len(), 1, "expected 1 error, got: {:?}", errors);
    assert!(errors[0].message.contains("chart.encoding must be a JSON object"),
        "expected encoding error, got: {:?}", errors);
}

#[test]
fn validate_interactive_filter_with_non_object_encoding_rejected() {
    let yaml = r#"
spec_version: 1
title: "Test"
sections:
  - id: c1
    title: "Chart"
    type: chart
    inline_data:
      - { x: 1 }
    interactive_filter:
      field: country
    chart:
      mark: bar
      encoding: "not an object"
"#;
    let spec: DashboardSpec = serde_yaml::from_str(yaml).unwrap();
    let errors = validate::validate(&spec, &BTreeSet::new());
    assert_eq!(errors.len(), 1, "expected 1 error, got: {:?}", errors);
    assert!(errors[0].message.contains("chart.encoding must be a JSON object"),
        "expected encoding error, got: {:?}", errors);
}

// --- stats validation tests ---

#[test]
fn validate_stats_whitespace_field_rejected() {
    let yaml = r#"
spec_version: 1
title: "Test"
sections:
  - title: "Stats"
    type: stats
    inline_data:
      - { x: 1 }
    stats:
      items:
        - { label: "Total", aggregate: sum, field: "   " }
"#;
    let spec: DashboardSpec = serde_yaml::from_str(yaml).unwrap();
    let errors = validate::validate(&spec, &BTreeSet::new());
    assert_eq!(errors.len(), 1, "expected 1 error, got: {:?}", errors);
    assert!(errors[0].message.contains("field must not be empty"));
}

#[test]
fn validate_stats_count_with_whitespace_field_rejected() {
    let yaml = r#"
spec_version: 1
title: "Test"
sections:
  - title: "Stats"
    type: stats
    inline_data:
      - { x: 1 }
    stats:
      items:
        - { label: "Count", aggregate: count, field: "  " }
"#;
    let spec: DashboardSpec = serde_yaml::from_str(yaml).unwrap();
    let errors = validate::validate(&spec, &BTreeSet::new());
    assert_eq!(errors.len(), 1, "expected 1 error, got: {:?}", errors);
    assert!(errors[0].message.contains("field must not be empty"));
}

// --- markdown schema tests ---

#[test]
fn markdown_toc_side_enum_left() {
    let yaml = r##"
spec_version: 1
title: "Test"
sections:
  - id: md1
    title: "Doc"
    type: markdown
    markdown:
      content: "# Hello"
      toc_levels: [1, 2]
      toc_side: left
"##;
    let spec: DashboardSpec = serde_yaml::from_str(yaml).unwrap();
    let md = spec.sections[0].markdown.as_ref().unwrap();
    assert_eq!(md.toc_side, Some(glasspad::spec::schema::TocSide::Left));
}

#[test]
fn markdown_toc_side_enum_right() {
    let yaml = r##"
spec_version: 1
title: "Test"
sections:
  - id: md1
    title: "Doc"
    type: markdown
    markdown:
      content: "# Hello"
      toc_levels: [1, 2]
      toc_side: right
"##;
    let spec: DashboardSpec = serde_yaml::from_str(yaml).unwrap();
    let md = spec.sections[0].markdown.as_ref().unwrap();
    assert_eq!(md.toc_side, Some(glasspad::spec::schema::TocSide::Right));
}

#[test]
fn reject_invalid_toc_side() {
    let yaml = r##"
spec_version: 1
title: "Test"
sections:
  - title: "Doc"
    type: markdown
    markdown:
      content: "# Hello"
      toc_side: center
"##;
    let result = serde_yaml::from_str::<DashboardSpec>(yaml);
    assert!(result.is_err(), "Should reject invalid toc_side 'center'");
}

#[test]
fn markdown_link_target_blank() {
    let yaml = r##"
spec_version: 1
title: "Test"
sections:
  - title: "Doc"
    type: markdown
    markdown:
      content: "# Hello"
      link_target: _blank
"##;
    let spec: DashboardSpec = serde_yaml::from_str(yaml).unwrap();
    let md = spec.sections[0].markdown.as_ref().unwrap();
    assert_eq!(md.link_target, Some(glasspad::spec::schema::LinkTarget::Blank));
}

#[test]
fn markdown_link_target_self() {
    let yaml = r##"
spec_version: 1
title: "Test"
sections:
  - title: "Doc"
    type: markdown
    markdown:
      content: "# Hello"
      link_target: _self
"##;
    let spec: DashboardSpec = serde_yaml::from_str(yaml).unwrap();
    let md = spec.sections[0].markdown.as_ref().unwrap();
    assert_eq!(md.link_target, Some(glasspad::spec::schema::LinkTarget::Self_));
}

#[test]
fn markdown_max_rows_deserialize() {
    let yaml = r##"
spec_version: 1
title: "Test"
sections:
  - title: "Doc"
    type: markdown
    source: docs
    markdown:
      content_field: body
      max_rows: 50
datasets:
  docs: {}
"##;
    let spec: DashboardSpec = serde_yaml::from_str(yaml).unwrap();
    let md = spec.sections[0].markdown.as_ref().unwrap();
    assert_eq!(md.max_rows, Some(50));
}

// --- pivot section tests ---

#[test]
fn parse_valid_pivot() {
    let yaml = load_fixture("valid_pivot.yaml");
    let spec: DashboardSpec = serde_yaml::from_str(&yaml).unwrap();
    assert_eq!(spec.sections[0].section_type, glasspad::spec::schema::SectionType::Pivot);
    let pivot = spec.sections[0].pivot.as_ref().unwrap();
    assert_eq!(pivot.rows, vec!["region", "product"]);
    assert_eq!(pivot.columns, vec!["quarter"]);
    assert_eq!(pivot.values.len(), 2);
    assert_eq!(pivot.values[0].field.as_deref(), Some("revenue"));
    assert_eq!(pivot.values[0].aggregate, "sum");
    assert_eq!(pivot.values[0].format.as_deref(), Some("currency"));
    assert_eq!(pivot.values[0].currency.as_deref(), Some("USD"));
    assert!(pivot.values[1].field.is_none()); // count without field
    assert_eq!(pivot.values[1].aggregate, "count");
    assert!(pivot.show_totals);
    assert!(pivot.show_subtotals);
    let sort = pivot.sort.as_ref().unwrap();
    assert_eq!(sort.by, Some(glasspad::spec::schema::PivotSortBy::Value));
    assert_eq!(sort.direction, Some(glasspad::spec::schema::PivotSortDirection::Desc));
    assert_eq!(sort.value_index, Some(0));
}

#[test]
fn validate_valid_pivot() {
    let yaml = load_fixture("valid_pivot.yaml");
    let spec: DashboardSpec = serde_yaml::from_str(&yaml).unwrap();
    let provided = BTreeSet::from(["sales".to_string()]);
    let errors = validate::validate(&spec, &provided);
    assert!(errors.is_empty(), "errors: {:?}", errors);
}

#[test]
fn parse_valid_pivot_minimal() {
    let yaml = load_fixture("valid_pivot_minimal.yaml");
    let spec: DashboardSpec = serde_yaml::from_str(&yaml).unwrap();
    let pivot = spec.sections[0].pivot.as_ref().unwrap();
    assert_eq!(pivot.values.len(), 3);
    assert_eq!(pivot.values[2].aggregate, "count");
    assert!(pivot.values[2].field.is_none());
    assert!(!pivot.show_totals);
    assert!(!pivot.show_subtotals);
}

#[test]
fn validate_valid_pivot_minimal() {
    let yaml = load_fixture("valid_pivot_minimal.yaml");
    let spec: DashboardSpec = serde_yaml::from_str(&yaml).unwrap();
    let errors = validate::validate(&spec, &BTreeSet::new());
    assert!(errors.is_empty(), "errors: {:?}", errors);
}

#[test]
fn validate_pivot_field_overlap_rejected() {
    let yaml = load_fixture("invalid_pivot_overlap.yaml");
    let spec: DashboardSpec = serde_yaml::from_str(&yaml).unwrap();
    let errors = validate::validate(&spec, &BTreeSet::new());
    assert_eq!(errors.len(), 1, "expected 1 error, got: {:?}", errors);
    assert!(errors[0].message.contains("appears in both pivot.rows and pivot.columns"));
}

#[test]
fn validate_pivot_subtotals_single_row_rejected() {
    let yaml = load_fixture("invalid_pivot_subtotals_single_row.yaml");
    let spec: DashboardSpec = serde_yaml::from_str(&yaml).unwrap();
    let errors = validate::validate(&spec, &BTreeSet::new());
    assert_eq!(errors.len(), 1, "expected 1 error, got: {:?}", errors);
    assert!(errors[0].message.contains("show_subtotals requires at least 2 row fields"));
}

#[test]
fn validate_pivot_non_count_requires_field() {
    let yaml = r#"
spec_version: 1
title: "Test"
sections:
  - title: "Bad"
    type: pivot
    inline_data:
      - { a: 1 }
    pivot:
      rows:
        - a
      values:
        - aggregate: sum
"#;
    let spec: DashboardSpec = serde_yaml::from_str(yaml).unwrap();
    let errors = validate::validate(&spec, &BTreeSet::new());
    assert_eq!(errors.len(), 1, "expected 1 error, got: {:?}", errors);
    assert!(errors[0].message.contains("requires field"));
}

#[test]
fn validate_pivot_count_without_field_valid() {
    let yaml = r#"
spec_version: 1
title: "Test"
sections:
  - title: "OK"
    type: pivot
    inline_data:
      - { a: 1 }
    pivot:
      rows:
        - a
      values:
        - aggregate: count
          label: "Rows"
"#;
    let spec: DashboardSpec = serde_yaml::from_str(yaml).unwrap();
    let errors = validate::validate(&spec, &BTreeSet::new());
    assert!(errors.is_empty(), "errors: {:?}", errors);
}

#[test]
fn validate_pivot_whitespace_field_rejected() {
    let yaml = r#"
spec_version: 1
title: "Test"
sections:
  - title: "Bad"
    type: pivot
    inline_data:
      - { a: 1 }
    pivot:
      rows:
        - "   "
      values:
        - field: a
          aggregate: sum
"#;
    let spec: DashboardSpec = serde_yaml::from_str(yaml).unwrap();
    let errors = validate::validate(&spec, &BTreeSet::new());
    assert_eq!(errors.len(), 1, "expected 1 error, got: {:?}", errors);
    assert!(errors[0].message.contains("empty field name"));
}

#[test]
fn validate_pivot_invalid_currency_rejected() {
    let yaml = r#"
spec_version: 1
title: "Test"
sections:
  - title: "Bad"
    type: pivot
    inline_data:
      - { a: 1 }
    pivot:
      rows:
        - a
      values:
        - field: a
          aggregate: sum
          format: currency
          currency: ""
"#;
    let spec: DashboardSpec = serde_yaml::from_str(yaml).unwrap();
    let errors = validate::validate(&spec, &BTreeSet::new());
    assert_eq!(errors.len(), 1, "expected 1 error, got: {:?}", errors);
    assert!(errors[0].message.contains("valid 3-letter currency code"));
}

#[test]
fn validate_pivot_value_index_out_of_range() {
    let yaml = r#"
spec_version: 1
title: "Test"
sections:
  - title: "Bad"
    type: pivot
    inline_data:
      - { a: 1 }
    pivot:
      rows:
        - a
      values:
        - field: a
          aggregate: sum
      sort:
        by: value
        value_index: 5
"#;
    let spec: DashboardSpec = serde_yaml::from_str(yaml).unwrap();
    let errors = validate::validate(&spec, &BTreeSet::new());
    assert_eq!(errors.len(), 1, "expected 1 error, got: {:?}", errors);
    assert!(errors[0].message.contains("out of range"));
}

#[test]
fn validate_pivot_duplicate_rows_rejected() {
    let yaml = r#"
spec_version: 1
title: "Test"
sections:
  - title: "Bad"
    type: pivot
    inline_data:
      - { a: 1 }
    pivot:
      rows:
        - a
        - a
      values:
        - field: a
          aggregate: sum
"#;
    let spec: DashboardSpec = serde_yaml::from_str(yaml).unwrap();
    let errors = validate::validate(&spec, &BTreeSet::new());
    assert_eq!(errors.len(), 1, "expected 1 error, got: {:?}", errors);
    assert!(errors[0].message.contains("duplicate field"));
}

#[test]
fn pivot_sort_json_serialization() {
    // Verify that Rust enums serialize to lowercase strings matching JS expectations
    let yaml = load_fixture("valid_pivot.yaml");
    let spec: DashboardSpec = serde_yaml::from_str(&yaml).unwrap();
    let json = serde_json::to_value(&spec).unwrap();
    let pivot = &json["sections"][0]["pivot"];
    assert_eq!(pivot["sort"]["by"], "value");
    assert_eq!(pivot["sort"]["direction"], "desc");
    assert_eq!(pivot["sort"]["value_index"], 0);
}

#[test]
fn pivot_type_json_serialization() {
    // Verify section type serializes as lowercase "pivot" for JS switch
    let yaml = load_fixture("valid_pivot.yaml");
    let spec: DashboardSpec = serde_yaml::from_str(&yaml).unwrap();
    let json = serde_json::to_value(&spec).unwrap();
    assert_eq!(json["sections"][0]["type"], "pivot");
}
