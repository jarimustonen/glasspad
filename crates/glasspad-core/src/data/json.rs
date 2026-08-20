use super::limits::{self, LimitError};
use super::types::{CellValue, Dataset, Row};

#[derive(Debug)]
pub enum JsonDataError {
    Json(serde_json::Error),
    NotAnArray,
    RowNotObject { index: usize },
    NestedValue { index: usize, field: String },
    InvalidNumber { index: usize, field: String },
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
                write!(
                    f,
                    "Row {} field '{}': nested objects/arrays not supported",
                    index, field
                )
            }
            JsonDataError::InvalidNumber { index, field } => {
                write!(
                    f,
                    "Row {} field '{}': number cannot be represented as f64",
                    index, field
                )
            }
            JsonDataError::Limit(e) => write!(f, "{}", e),
        }
    }
}

impl std::error::Error for JsonDataError {}

/// Convert a serde_json::Value (scalar) to CellValue.
/// JSON types are preserved — strings are NOT re-inferred.
fn json_value_to_cell(
    value: &serde_json::Value,
    index: usize,
    field: &str,
) -> Result<CellValue, JsonDataError> {
    match value {
        serde_json::Value::Null => Ok(CellValue::Null),
        serde_json::Value::Bool(b) => Ok(CellValue::Bool(*b)),
        serde_json::Value::Number(n) => n
            .as_f64()
            .filter(|v| v.is_finite())
            .map(CellValue::Number)
            .ok_or(JsonDataError::InvalidNumber {
                index,
                field: field.to_string(),
            }),
        serde_json::Value::String(s) => Ok(CellValue::String(s.clone())),
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
            Err(JsonDataError::NestedValue {
                index,
                field: field.to_string(),
            })
        }
    }
}

/// Parse a JSON array of objects into a Dataset.
pub fn parse_json_str(s: &str) -> Result<Dataset, JsonDataError> {
    let value: serde_json::Value = serde_json::from_str(s).map_err(JsonDataError::Json)?;

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
            let cell = json_value_to_cell(val, i, key)?;
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
    fn json_strings_preserved_not_inferred() {
        // "42" stays String, "true" stays String — JSON types are explicit
        let json = r#"[{"num": "42", "flag": "true", "zip": "00123"}]"#;
        let rows = parse_json_str(json).unwrap();
        assert_eq!(rows[0]["num"], CellValue::String("42".to_string()));
        assert_eq!(rows[0]["flag"], CellValue::String("true".to_string()));
        assert_eq!(rows[0]["zip"], CellValue::String("00123".to_string()));
    }

    #[test]
    fn json_whitespace_strings_preserved() {
        let json = r#"[{"val": " "}]"#;
        let rows = parse_json_str(json).unwrap();
        assert_eq!(rows[0]["val"], CellValue::String(" ".to_string()));
    }

    #[test]
    fn json_temporal_strings_stay_strings() {
        let json = r#"[{"dt": "2026-04-06T12:00:00Z"}]"#;
        let rows = parse_json_str(json).unwrap();
        assert_eq!(
            rows[0]["dt"],
            CellValue::String("2026-04-06T12:00:00Z".to_string())
        );
    }

    #[test]
    fn reject_non_array() {
        assert!(matches!(
            parse_json_str(r#"{"not": "array"}"#),
            Err(JsonDataError::NotAnArray)
        ));
    }

    #[test]
    fn reject_non_object_row() {
        assert!(matches!(
            parse_json_str(r#"[1, 2, 3]"#),
            Err(JsonDataError::RowNotObject { index: 0 })
        ));
    }

    #[test]
    fn reject_nested_objects() {
        assert!(matches!(
            parse_json_str(r#"[{"a": {"nested": true}}]"#),
            Err(JsonDataError::NestedValue { index: 0, .. })
        ));
    }

    #[test]
    fn reject_nested_arrays() {
        assert!(matches!(
            parse_json_str(r#"[{"a": [1, 2]}]"#),
            Err(JsonDataError::NestedValue { index: 0, .. })
        ));
    }

    #[test]
    fn parse_empty_array() {
        assert_eq!(parse_json_str(r#"[]"#).unwrap().len(), 0);
    }

    #[test]
    fn reject_invalid_number() {
        // serde_json can parse 1e9999 but it won't fit in f64 as finite
        let json = r#"[{"val": 1e9999}]"#;
        let result = parse_json_str(json);
        // serde_json may reject this at parse level or produce infinity
        assert!(
            result.is_err() || {
                let _rows = result.unwrap();
                // If it parsed, the value should have been rejected as non-finite
                false
            }
        );
    }
}
