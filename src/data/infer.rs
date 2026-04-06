use std::collections::BTreeMap;

use super::types::{CellValue, DatasetMeta, FieldKind, Row};

/// Infer a CellValue from a raw string (e.g., from CSV).
pub fn infer_cell_value(s: &str) -> CellValue {
    let trimmed = s.trim();

    if trimmed.is_empty() {
        return CellValue::Null;
    }

    // Bool
    match trimmed.to_ascii_lowercase().as_str() {
        "true" => return CellValue::Bool(true),
        "false" => return CellValue::Bool(false),
        _ => {}
    }

    // Number (integer or decimal, including negative)
    if let Ok(n) = trimmed.parse::<f64>() {
        if n.is_finite() {
            return CellValue::Number(n);
        }
    }

    // Temporal: ISO-8601 patterns (YYYY-MM-DD or YYYY-MM-DDTHH:MM:SS...)
    if is_temporal(trimmed) {
        return CellValue::String(trimmed.to_string());
    }

    CellValue::String(trimmed.to_string())
}

fn is_temporal(s: &str) -> bool {
    // Must start with 4 digits (year)
    let bytes = s.as_bytes();
    if bytes.len() < 10 {
        return false;
    }
    if !bytes[0..4].iter().all(|b| b.is_ascii_digit()) {
        return false;
    }
    if bytes[4] != b'-' {
        return false;
    }
    if !bytes[5..7].iter().all(|b| b.is_ascii_digit()) {
        return false;
    }
    if bytes[7] != b'-' {
        return false;
    }
    if !bytes[8..10].iter().all(|b| b.is_ascii_digit()) {
        return false;
    }
    // Could have time part (T or space)
    if bytes.len() > 10 && bytes[10] != b'T' && bytes[10] != b' ' {
        return false;
    }
    true
}

/// Determine the FieldKind for a column based on sampled CellValues.
fn infer_field_kind(values: &[&CellValue]) -> FieldKind {
    let mut has_number = false;
    let mut has_bool = false;
    let mut has_temporal = false;
    let mut has_string = false;

    for v in values {
        match v {
            CellValue::Null => {} // nulls don't affect type
            CellValue::Number(_) => has_number = true,
            CellValue::Bool(_) => has_bool = true,
            CellValue::String(s) => {
                if is_temporal(s) {
                    has_temporal = true;
                } else {
                    has_string = true;
                }
            }
        }
    }

    // If any plain string exists, the whole column is String
    if has_string {
        return FieldKind::String;
    }
    // Mixed types → String
    let type_count = has_number as u8 + has_bool as u8 + has_temporal as u8;
    if type_count > 1 {
        return FieldKind::String;
    }

    if has_temporal {
        FieldKind::Temporal
    } else if has_number {
        FieldKind::Number
    } else if has_bool {
        FieldKind::Bool
    } else {
        FieldKind::String // all nulls
    }
}

/// Infer field kinds from a dataset, sampling up to the first 100 rows.
pub fn infer_dataset_meta(rows: &[Row]) -> DatasetMeta {
    let sample_size = rows.len().min(100);
    let sample = &rows[..sample_size];

    // Collect all field names
    let mut all_fields: BTreeMap<String, Vec<&CellValue>> = BTreeMap::new();
    for row in sample {
        for (key, value) in row {
            all_fields.entry(key.clone()).or_default().push(value);
        }
    }

    let fields = all_fields
        .into_iter()
        .map(|(key, values)| (key, infer_field_kind(&values)))
        .collect();

    DatasetMeta {
        fields,
        row_count: rows.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infer_empty_string_is_null() {
        assert_eq!(infer_cell_value(""), CellValue::Null);
        assert_eq!(infer_cell_value("  "), CellValue::Null);
    }

    #[test]
    fn infer_booleans() {
        assert_eq!(infer_cell_value("true"), CellValue::Bool(true));
        assert_eq!(infer_cell_value("false"), CellValue::Bool(false));
        assert_eq!(infer_cell_value("TRUE"), CellValue::Bool(true));
        assert_eq!(infer_cell_value("False"), CellValue::Bool(false));
    }

    #[test]
    fn infer_integers() {
        assert_eq!(infer_cell_value("42"), CellValue::Number(42.0));
        assert_eq!(infer_cell_value("0"), CellValue::Number(0.0));
        assert_eq!(infer_cell_value("-7"), CellValue::Number(-7.0));
    }

    #[test]
    fn infer_decimals() {
        assert_eq!(infer_cell_value("3.14"), CellValue::Number(3.14));
        assert_eq!(infer_cell_value("-0.5"), CellValue::Number(-0.5));
    }

    #[test]
    fn infer_scientific_notation() {
        assert_eq!(infer_cell_value("1e10"), CellValue::Number(1e10));
        assert_eq!(infer_cell_value("2.5E-3"), CellValue::Number(2.5e-3));
    }

    #[test]
    fn infer_infinity_nan_as_string() {
        assert_eq!(
            infer_cell_value("inf"),
            CellValue::String("inf".to_string())
        );
        assert_eq!(
            infer_cell_value("NaN"),
            CellValue::String("NaN".to_string())
        );
    }

    #[test]
    fn infer_iso_date() {
        assert_eq!(
            infer_cell_value("2026-04-06"),
            CellValue::String("2026-04-06".to_string())
        );
    }

    #[test]
    fn infer_iso_datetime() {
        assert_eq!(
            infer_cell_value("2026-04-06T12:30:00Z"),
            CellValue::String("2026-04-06T12:30:00Z".to_string())
        );
    }

    #[test]
    fn infer_not_temporal() {
        // Too short
        assert_eq!(
            infer_cell_value("2026-04"),
            CellValue::String("2026-04".to_string())
        );
        // Bad format
        assert_eq!(
            infer_cell_value("20260406"),
            CellValue::Number(20260406.0)
        );
    }

    #[test]
    fn infer_plain_strings() {
        assert_eq!(
            infer_cell_value("hello"),
            CellValue::String("hello".to_string())
        );
        assert_eq!(
            infer_cell_value("/blog/post"),
            CellValue::String("/blog/post".to_string())
        );
    }

    #[test]
    fn infer_trimming() {
        assert_eq!(infer_cell_value("  42  "), CellValue::Number(42.0));
        assert_eq!(infer_cell_value(" true "), CellValue::Bool(true));
    }

    #[test]
    fn field_kind_all_numbers() {
        let vals = vec![CellValue::Number(1.0), CellValue::Number(2.0)];
        let refs: Vec<&CellValue> = vals.iter().collect();
        assert_eq!(infer_field_kind(&refs), FieldKind::Number);
    }

    #[test]
    fn field_kind_numbers_with_nulls() {
        let vals = vec![CellValue::Number(1.0), CellValue::Null, CellValue::Number(3.0)];
        let refs: Vec<&CellValue> = vals.iter().collect();
        assert_eq!(infer_field_kind(&refs), FieldKind::Number);
    }

    #[test]
    fn field_kind_temporal() {
        let vals = vec![
            CellValue::String("2026-04-01".to_string()),
            CellValue::String("2026-04-02".to_string()),
        ];
        let refs: Vec<&CellValue> = vals.iter().collect();
        assert_eq!(infer_field_kind(&refs), FieldKind::Temporal);
    }

    #[test]
    fn field_kind_mixed_becomes_string() {
        let vals = vec![CellValue::Number(1.0), CellValue::Bool(true)];
        let refs: Vec<&CellValue> = vals.iter().collect();
        assert_eq!(infer_field_kind(&refs), FieldKind::String);
    }

    #[test]
    fn field_kind_all_nulls() {
        let vals = vec![CellValue::Null, CellValue::Null];
        let refs: Vec<&CellValue> = vals.iter().collect();
        assert_eq!(infer_field_kind(&refs), FieldKind::String);
    }

    #[test]
    fn dataset_meta_inference() {
        let rows = vec![
            BTreeMap::from([
                ("date".to_string(), CellValue::String("2026-04-01".to_string())),
                ("count".to_string(), CellValue::Number(5.0)),
                ("active".to_string(), CellValue::Bool(true)),
                ("name".to_string(), CellValue::String("Alice".to_string())),
            ]),
            BTreeMap::from([
                ("date".to_string(), CellValue::String("2026-04-02".to_string())),
                ("count".to_string(), CellValue::Number(3.0)),
                ("active".to_string(), CellValue::Bool(false)),
                ("name".to_string(), CellValue::String("Bob".to_string())),
            ]),
        ];
        let meta = infer_dataset_meta(&rows);
        assert_eq!(meta.row_count, 2);
        assert_eq!(meta.fields["date"], FieldKind::Temporal);
        assert_eq!(meta.fields["count"], FieldKind::Number);
        assert_eq!(meta.fields["active"], FieldKind::Bool);
        assert_eq!(meta.fields["name"], FieldKind::String);
    }

    #[test]
    fn dataset_meta_empty() {
        let meta = infer_dataset_meta(&[]);
        assert_eq!(meta.row_count, 0);
        assert!(meta.fields.is_empty());
    }
}
