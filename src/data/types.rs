use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

/// A single cell value in a dataset row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CellValue {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
}

impl CellValue {
    pub fn is_null(&self) -> bool {
        matches!(self, CellValue::Null)
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            CellValue::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            CellValue::Number(n) => Some(*n),
            _ => None,
        }
    }
}

impl fmt::Display for CellValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CellValue::Null => write!(f, ""),
            CellValue::Bool(b) => write!(f, "{}", b),
            CellValue::Number(n) => {
                if n.fract() == 0.0 && *n >= i64::MIN as f64 && *n <= i64::MAX as f64 {
                    write!(f, "{:.0}", n)
                } else {
                    write!(f, "{}", n)
                }
            }
            CellValue::String(s) => write!(f, "{}", s),
        }
    }
}

/// A single row in a dataset.
pub type Row = BTreeMap<String, CellValue>;

/// A dataset is a collection of rows.
pub type Dataset = Vec<Row>;

/// Inferred type of a field across a dataset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FieldKind {
    String,
    Number,
    Bool,
    Temporal,
}

/// Metadata about a dataset's fields.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DatasetMeta {
    pub fields: BTreeMap<String, FieldKind>,
    pub row_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_value_null() {
        let v = CellValue::Null;
        assert!(v.is_null());
        assert_eq!(v.to_string(), "");
    }

    #[test]
    fn cell_value_bool() {
        let v = CellValue::Bool(true);
        assert!(!v.is_null());
        assert_eq!(v.to_string(), "true");
    }

    #[test]
    fn cell_value_number_integer() {
        let v = CellValue::Number(42.0);
        assert_eq!(v.as_f64(), Some(42.0));
        assert_eq!(v.to_string(), "42");
    }

    #[test]
    fn cell_value_number_decimal() {
        let v = CellValue::Number(3.14);
        assert_eq!(v.to_string(), "3.14");
    }

    #[test]
    fn cell_value_string() {
        let v = CellValue::String("hello".to_string());
        assert_eq!(v.as_str(), Some("hello"));
        assert_eq!(v.to_string(), "hello");
    }

    #[test]
    fn json_roundtrip_null() {
        let v = CellValue::Null;
        let json = serde_json::to_string(&v).unwrap();
        assert_eq!(json, "null");
        let back: CellValue = serde_json::from_str(&json).unwrap();
        assert_eq!(back, CellValue::Null);
    }

    #[test]
    fn json_roundtrip_number() {
        let v = CellValue::Number(42.0);
        let json = serde_json::to_string(&v).unwrap();
        assert_eq!(json, "42.0");
        let back: CellValue = serde_json::from_str(&json).unwrap();
        assert_eq!(back, CellValue::Number(42.0));
    }

    #[test]
    fn json_roundtrip_bool() {
        let v = CellValue::Bool(false);
        let json = serde_json::to_string(&v).unwrap();
        assert_eq!(json, "false");
        let back: CellValue = serde_json::from_str(&json).unwrap();
        assert_eq!(back, CellValue::Bool(false));
    }

    #[test]
    fn json_roundtrip_string() {
        let v = CellValue::String("hello".to_string());
        let json = serde_json::to_string(&v).unwrap();
        assert_eq!(json, "\"hello\"");
        let back: CellValue = serde_json::from_str(&json).unwrap();
        assert_eq!(back, CellValue::String("hello".to_string()));
    }

    #[test]
    fn row_construction() {
        let mut row = Row::new();
        row.insert("name".to_string(), CellValue::String("Alice".to_string()));
        row.insert("age".to_string(), CellValue::Number(30.0));
        row.insert("active".to_string(), CellValue::Bool(true));
        assert_eq!(row.len(), 3);
        assert_eq!(row["name"].as_str(), Some("Alice"));
    }

    #[test]
    fn dataset_meta_construction() {
        let mut fields = BTreeMap::new();
        fields.insert("date".to_string(), FieldKind::Temporal);
        fields.insert("count".to_string(), FieldKind::Number);
        fields.insert("name".to_string(), FieldKind::String);
        let meta = DatasetMeta {
            fields,
            row_count: 100,
        };
        assert_eq!(meta.fields["date"], FieldKind::Temporal);
        assert_eq!(meta.row_count, 100);
    }
}
