use std::collections::HashSet;

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
    let provided = HashSet::from(["events".to_string()]);
    let errors = validate::validate(&spec, &provided);
    assert!(errors.is_empty(), "errors: {:?}", errors);
}

#[test]
fn validate_valid_dashboard_without_provided_data() {
    let yaml = load_fixture("valid_dashboard.yaml");
    let spec: DashboardSpec = serde_yaml::from_str(&yaml).unwrap();
    let errors = validate::validate(&spec, &HashSet::new());
    assert!(errors.is_empty(), "errors: {:?}", errors);
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
    let errors = validate::validate(&spec, &HashSet::new());
    assert!(errors.iter().any(|e| e.message.contains("unknown aggregate")));
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
    chart:
      mark: bar
      encoding: "not an object"
"#;
    let spec: DashboardSpec = serde_yaml::from_str(yaml).unwrap();
    let errors = validate::validate(&spec, &HashSet::new());
    assert!(errors.iter().any(|e| e.message.contains("chart.encoding must be a JSON object")));
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
    interactive_filter:
      field: country
    stats:
      items:
        - { label: "Total", aggregate: count }
"#;
    let spec: DashboardSpec = serde_yaml::from_str(yaml).unwrap();
    let errors = validate::validate(&spec, &HashSet::new());
    assert!(errors.iter().any(|e| e.message.contains("only supported on chart")));
}
