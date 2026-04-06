use super::infer::infer_cell_value;
use super::limits::{self, LimitError};
use super::types::{CellValue, Dataset, Row};

#[derive(Debug)]
pub enum JsonDataError {
    Json(serde_json::Error),
    NotAnArray,
    RowNotObject { index: usize },
    NestedValue { index: usize, field: String },
    Limit(LimitError),
}

impl std::fmt::Display for JsonDataError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JsonDataError::Json(e) => write!(f, "JSON parse error: {}", e),
            JsonDataError::NotAnArray => write!(f, "Expected JSON array of objects"),
            JsonDataError::RowNotObject { index } => {
                write!(f, "Row {} is not a JSON object", index)
            }
            JsonDataError::NestedValue { index, field } => {
                write!(f, "Row {} field '{}': nested objects/arrays not supported", index, field)
            }
            JsonDataError::Limit(e) => write!(f, "{}", e),
        }
    }
}

/// Convert a serde_json::Value (scalar) to CellValue.
fn json_value_to_cell(value: &serde_json::Value) -> Result<CellValue, ()> {
    match value {
        serde_json::Value::Null => Ok(CellValue::Null),
        serde_json::Value::Bool(b) => Ok(CellValue::Bool(*b)),
        serde_json::Value::Number(n) => {
            Ok(CellValue::Number(n.as_f64().unwrap_or(0.0)))
        }
        serde_json::Value::String(s) => {
            // Apply type inference to string values (temporal detection)
            Ok(infer_cell_value(s))
        }
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => Err(()),
    }
}

/// Parse a JSON array of objects into a Dataset.
pub fn parse_json_str(s: &str) -> Result<Dataset, JsonDataError> {
    let value: serde_json::Value =
        serde_json::from_str(s).map_err(JsonDataError::Json)?;

    parse_json_value(&value)
}

/// Parse a serde_json::Value (expected array of objects) into a Dataset.
pub fn parse_json_value(value: &serde_json::Value) -> Result<Dataset, JsonDataError> {
    let arr = value.as_array().ok_or(JsonDataError::NotAnArray)?;

    if arr.len() > limits::MAX_ROWS_PER_DATASET {
        return Err(JsonDataError::Limit(LimitError::TooManyRows {
            max: limits::MAX_ROWS_PER_DATASET,
        }));
    }

    let mut rows = Dataset::with_capacity(arr.len());

    for (i, item) in arr.iter().enumerate() {
        let obj = item
            .as_object()
            .ok_or(JsonDataError::RowNotObject { index: i })?;

        if obj.len() > limits::MAX_COLUMNS {
            return Err(JsonDataError::Limit(LimitError::TooManyColumns {
                count: obj.len(),
                max: limits::MAX_COLUMNS,
            }));
        }

        let mut row = Row::new();
        for (key, val) in obj {
            let cell = json_value_to_cell(val).map_err(|_| JsonDataError::NestedValue {
                index: i,
                field: key.clone(),
            })?;
            row.insert(key.clone(), cell);
        }
        rows.push(row);
    }

    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_json_array() {
        let json = r#"[{"name": "Alice", "age": 30}, {"name": "Bob", "age": 25}]"#;
        let rows = parse_json_str(json).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["name"], CellValue::String("Alice".to_string()));
        assert_eq!(rows[0]["age"], CellValue::Number(30.0));
    }

    #[test]
    fn parse_json_with_nulls() {
        let json = r#"[{"a": 1, "b": null}]"#;
        let rows = parse_json_str(json).unwrap();
        assert_eq!(rows[0]["a"], CellValue::Number(1.0));
        assert_eq!(rows[0]["b"], CellValue::Null);
    }

    #[test]
    fn parse_json_with_booleans() {
        let json = r#"[{"flag": true}, {"flag": false}]"#;
        let rows = parse_json_str(json).unwrap();
        assert_eq!(rows[0]["flag"], CellValue::Bool(true));
        assert_eq!(rows[1]["flag"], CellValue::Bool(false));
    }

    #[test]
    fn parse_json_temporal_strings() {
        let json = r#"[{"dt": "2026-04-06T12:00:00Z"}]"#;
        let rows = parse_json_str(json).unwrap();
        // Temporal strings stay as strings (infer_cell_value detects them)
        assert_eq!(
            rows[0]["dt"],
            CellValue::String("2026-04-06T12:00:00Z".to_string())
        );
    }

    #[test]
    fn reject_non_array() {
        let json = r#"{"not": "array"}"#;
        assert!(matches!(parse_json_str(json), Err(JsonDataError::NotAnArray)));
    }

    #[test]
    fn reject_non_object_row() {
        let json = r#"[1, 2, 3]"#;
        assert!(matches!(
            parse_json_str(json),
            Err(JsonDataError::RowNotObject { index: 0 })
        ));
    }

    #[test]
    fn reject_nested_objects() {
        let json = r#"[{"a": {"nested": true}}]"#;
        assert!(matches!(
            parse_json_str(json),
            Err(JsonDataError::NestedValue { index: 0, .. })
        ));
    }

    #[test]
    fn reject_nested_arrays() {
        let json = r#"[{"a": [1, 2]}]"#;
        assert!(matches!(
            parse_json_str(json),
            Err(JsonDataError::NestedValue { index: 0, .. })
        ));
    }

    #[test]
    fn parse_empty_array() {
        let json = r#"[]"#;
        let rows = parse_json_str(json).unwrap();
        assert_eq!(rows.len(), 0);
    }
}
