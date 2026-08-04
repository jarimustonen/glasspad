use std::collections::BTreeMap;

use super::types::{CellValue, DatasetMeta, FieldKind, Row};

/// Infer a CellValue from a raw CSV string field.
///
/// Does NOT trim whitespace. If the field has leading/trailing whitespace,
/// it is treated as a string. Callers must explicitly trim if desired.
pub fn infer_cell_value(s: &str) -> CellValue {
    if s.is_empty() {
        return CellValue::Null;
    }

    // Bool (exact match, case-insensitive)
    match s.to_ascii_lowercase().as_str() {
        "true" => return CellValue::Bool(true),
        "false" => return CellValue::Bool(false),
        _ => {}
    }

    // Number (integer or decimal, including negative)
    if let Ok(n) = s.parse::<f64>()
        && n.is_finite()
    {
        return CellValue::Number(n);
    }

    // Temporal: strict ISO-8601 patterns
    if is_temporal(s) {
        return CellValue::String(s.to_string());
    }

    CellValue::String(s.to_string())
}

/// Strict ISO-8601 date/datetime detection.
/// Accepts: YYYY-MM-DD (with valid month 01-12, day 01-31)
/// Accepts: YYYY-MM-DDTHH:MM:SS (with optional timezone and fractional seconds)
fn is_temporal(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.len() < 10 {
        return false;
    }

    // Year: 4 digits
    if !bytes[0..4].iter().all(|b| b.is_ascii_digit()) {
        return false;
    }
    if bytes[4] != b'-' {
        return false;
    }

    // Month: 01-12
    let month = matches!(
        (bytes[5], bytes[6]),
        (b'0', b'1'..=b'9') | (b'1', b'0'..=b'2')
    );
    if !month {
        return false;
    }

    if bytes[7] != b'-' {
        return false;
    }

    // Day: 01-31
    let day = matches!(
        (bytes[8], bytes[9]),
        (b'0', b'1'..=b'9') | (b'1' | b'2', b'0'..=b'9') | (b'3', b'0'..=b'1')
    );
    if !day {
        return false;
    }

    // Date only
    if bytes.len() == 10 {
        return true;
    }

    // Must have T or space separator for time part
    if bytes[10] != b'T' && bytes[10] != b' ' {
        return false;
    }

    // Remaining chars must be valid ISO-8601 time characters
    if bytes.len() < 19 {
        return false; // need at least HH:MM:SS
    }
    for &b in &bytes[11..] {
        if !matches!(b, b'0'..=b'9' | b':' | b'.' | b'+' | b'-' | b'Z') {
            return false;
        }
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
            CellValue::Null => {}
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

    if has_string {
        return FieldKind::String;
    }
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
        FieldKind::String
    }
}

/// Infer field kinds from a dataset, sampling up to the first 100 rows.
/// Missing fields in sparse rows are treated as Null.
pub fn infer_dataset_meta(rows: &[Row]) -> DatasetMeta {
    let sample_size = rows.len().min(100);
    let sample = &rows[..sample_size];

    // Collect all field names across all sampled rows
    let mut field_names: BTreeMap<String, ()> = BTreeMap::new();
    for row in sample {
        for key in row.keys() {
            field_names.insert(key.clone(), ());
        }
    }

    let null_sentinel = CellValue::Null;
    let fields = field_names
        .keys()
        .map(|key| {
            let values: Vec<&CellValue> = sample
                .iter()
                .map(|row| row.get(key).unwrap_or(&null_sentinel))
                .collect();
            (key.clone(), infer_field_kind(&values))
        })
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
    }

    #[test]
    fn whitespace_is_string_not_null() {
        // Whitespace is NOT trimmed — it becomes a string
        assert_eq!(infer_cell_value("  "), CellValue::String("  ".to_string()));
    }

    #[test]
    fn whitespace_around_number_is_string() {
        // " 42 " is a string because we don't trim
        assert_eq!(
            infer_cell_value(" 42 "),
            CellValue::String(" 42 ".to_string())
        );
    }

    #[test]
    fn whitespace_around_bool_is_string() {
        assert_eq!(
            infer_cell_value(" true "),
            CellValue::String(" true ".to_string())
        );
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
    #[allow(clippy::approx_constant)] // 3.14 is a decimal-parse sample, not π
    fn infer_decimals() {
        assert_eq!(infer_cell_value("3.14"), CellValue::Number(3.14));
        assert_eq!(infer_cell_value("-0.5"), CellValue::Number(-0.5));
    }

    #[test]
    fn infer_scientific_notation() {
        assert_eq!(infer_cell_value("1e10"), CellValue::Number(1e10));
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
    fn infer_iso_datetime_with_offset() {
        assert_eq!(
            infer_cell_value("2026-04-06T12:30:00+03:00"),
            CellValue::String("2026-04-06T12:30:00+03:00".to_string())
        );
    }

    #[test]
    fn reject_invalid_month() {
        assert_eq!(
            infer_cell_value("2026-13-01"),
            CellValue::String("2026-13-01".to_string())
        );
        // Month 13 → not temporal, but it's still a string
        // Check that is_temporal rejects it
        assert!(!is_temporal("2026-13-01"));
    }

    #[test]
    fn reject_invalid_day() {
        assert!(!is_temporal("2026-04-32"));
        assert!(!is_temporal("2026-04-00"));
    }

    #[test]
    fn reject_garbage_after_date() {
        assert!(!is_temporal("2026-04-06Tgarbage"));
        assert!(!is_temporal("2026-04-06 bad"));
    }

    #[test]
    fn reject_too_short_time() {
        assert!(!is_temporal("2026-04-06T12")); // too short for time
    }

    #[test]
    fn accept_fractional_seconds() {
        assert!(is_temporal("2026-04-06T12:30:00.123Z"));
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
    fn field_kind_all_numbers() {
        let vals = [CellValue::Number(1.0), CellValue::Number(2.0)];
        let refs: Vec<&CellValue> = vals.iter().collect();
        assert_eq!(infer_field_kind(&refs), FieldKind::Number);
    }

    #[test]
    fn field_kind_numbers_with_nulls() {
        let vals = [
            CellValue::Number(1.0),
            CellValue::Null,
            CellValue::Number(3.0),
        ];
        let refs: Vec<&CellValue> = vals.iter().collect();
        assert_eq!(infer_field_kind(&refs), FieldKind::Number);
    }

    #[test]
    fn field_kind_temporal() {
        let vals = [
            CellValue::String("2026-04-01".to_string()),
            CellValue::String("2026-04-02".to_string()),
        ];
        let refs: Vec<&CellValue> = vals.iter().collect();
        assert_eq!(infer_field_kind(&refs), FieldKind::Temporal);
    }

    #[test]
    fn field_kind_mixed_becomes_string() {
        let vals = [CellValue::Number(1.0), CellValue::Bool(true)];
        let refs: Vec<&CellValue> = vals.iter().collect();
        assert_eq!(infer_field_kind(&refs), FieldKind::String);
    }

    #[test]
    fn field_kind_all_nulls() {
        let vals = [CellValue::Null, CellValue::Null];
        let refs: Vec<&CellValue> = vals.iter().collect();
        assert_eq!(infer_field_kind(&refs), FieldKind::String);
    }

    #[test]
    fn dataset_meta_with_sparse_rows() {
        // Row 0 has "a", row 1 doesn't — should still infer "a" considering the null
        let rows = vec![
            BTreeMap::from([
                ("a".to_string(), CellValue::Number(1.0)),
                ("b".to_string(), CellValue::Number(2.0)),
            ]),
            BTreeMap::from([("b".to_string(), CellValue::Number(3.0))]),
        ];
        let meta = infer_dataset_meta(&rows);
        assert_eq!(meta.fields["a"], FieldKind::Number); // Number + Null = Number
        assert_eq!(meta.fields["b"], FieldKind::Number);
    }

    #[test]
    fn dataset_meta_empty() {
        let meta = infer_dataset_meta(&[]);
        assert_eq!(meta.row_count, 0);
        assert!(meta.fields.is_empty());
    }
}
