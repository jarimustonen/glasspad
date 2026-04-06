use glasspad::data::csv::parse_csv_str;
use glasspad::data::infer::infer_dataset_meta;
use glasspad::data::json::parse_json_str;
use glasspad::data::types::{CellValue, FieldKind};

fn load_fixture(name: &str) -> String {
    std::fs::read_to_string(format!("tests/fixtures/{}", name)).unwrap()
}

// --- CSV integration tests ---

#[test]
fn load_events_csv() {
    let csv = load_fixture("events.csv");
    let rows = parse_csv_str(&csv).unwrap();
    assert_eq!(rows.len(), 8);
}

#[test]
fn events_csv_field_types() {
    let csv = load_fixture("events.csv");
    let rows = parse_csv_str(&csv).unwrap();

    // datetime should be temporal string
    assert!(matches!(&rows[0]["datetime"], CellValue::String(s) if s.starts_with("2026")));

    // path should be string
    assert!(matches!(&rows[0]["path"], CellValue::String(_)));

    // country should be string
    assert!(matches!(&rows[0]["country"], CellValue::String(_)));
}

#[test]
fn events_csv_metadata() {
    let csv = load_fixture("events.csv");
    let rows = parse_csv_str(&csv).unwrap();
    let meta = infer_dataset_meta(&rows);

    assert_eq!(meta.row_count, 8);
    assert_eq!(meta.fields["datetime"], FieldKind::Temporal);
    assert_eq!(meta.fields["path"], FieldKind::String);
    assert_eq!(meta.fields["country"], FieldKind::String);
    assert_eq!(meta.fields["device"], FieldKind::String);
    assert_eq!(meta.fields["event_type"], FieldKind::String);
}

// --- JSON integration tests ---

#[test]
fn parse_json_events() {
    let json = r#"[
        {"datetime": "2026-04-04T18:00:00Z", "path": "/en/", "country": "OM", "count": 5},
        {"datetime": "2026-04-04T19:00:00Z", "path": "/blog/", "country": "IN", "count": 3}
    ]"#;
    let rows = parse_json_str(json).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["count"], CellValue::Number(5.0));
    assert_eq!(rows[1]["country"], CellValue::String("IN".to_string()));
}

#[test]
fn json_metadata_inference() {
    let json = r#"[
        {"date": "2026-04-01", "value": 42, "active": true, "name": "Alice"},
        {"date": "2026-04-02", "value": 17, "active": false, "name": "Bob"}
    ]"#;
    let rows = parse_json_str(json).unwrap();
    let meta = infer_dataset_meta(&rows);

    assert_eq!(meta.fields["date"], FieldKind::Temporal);
    assert_eq!(meta.fields["value"], FieldKind::Number);
    assert_eq!(meta.fields["active"], FieldKind::Bool);
    assert_eq!(meta.fields["name"], FieldKind::String);
}

// --- Edge cases ---

#[test]
fn csv_with_all_empty_values() {
    let csv = "a,b\n,\n,\n";
    let rows = parse_csv_str(csv).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["a"], CellValue::Null);
    assert_eq!(rows[0]["b"], CellValue::Null);
}

#[test]
fn csv_mixed_types_in_column() {
    let csv = "val\n42\nhello\n";
    let rows = parse_csv_str(csv).unwrap();
    // First row is number, second is string
    assert_eq!(rows[0]["val"], CellValue::Number(42.0));
    assert_eq!(rows[1]["val"], CellValue::String("hello".to_string()));

    // Metadata should show String (mixed column)
    let meta = infer_dataset_meta(&rows);
    assert_eq!(meta.fields["val"], FieldKind::String);
}

#[test]
fn json_with_null_values() {
    let json = r#"[{"a": 1, "b": null}, {"a": null, "b": "hello"}]"#;
    let rows = parse_json_str(json).unwrap();
    assert_eq!(rows[0]["b"], CellValue::Null);
    assert_eq!(rows[1]["a"], CellValue::Null);
}
