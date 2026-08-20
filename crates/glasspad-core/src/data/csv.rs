use std::collections::HashSet;

use super::infer::infer_cell_value;
use super::limits::{self, LimitError};
use super::types::{CellValue, Dataset, Row};

#[derive(Debug)]
pub enum CsvError {
    Csv(csv::Error),
    Limit(LimitError),
    DuplicateHeader(String),
    EmptyHeader {
        position: usize,
    },
    CellTooLarge {
        row: usize,
        field: String,
        size: usize,
        max: usize,
    },
}

impl std::fmt::Display for CsvError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CsvError::Csv(e) => write!(f, "CSV parse error: {}", e),
            CsvError::Limit(e) => write!(f, "{}", e),
            CsvError::DuplicateHeader(h) => write!(f, "Duplicate CSV header: \"{}\"", h),
            CsvError::EmptyHeader { position } => {
                write!(f, "Empty CSV header at position {}", position)
            }
            CsvError::CellTooLarge {
                row,
                field,
                size,
                max,
            } => {
                write!(
                    f,
                    "Row {} field \"{}\": cell is {} bytes, max is {}",
                    row, field, size, max
                )
            }
        }
    }
}

impl std::error::Error for CsvError {}

impl From<csv::Error> for CsvError {
    fn from(e: csv::Error) -> Self {
        CsvError::Csv(e)
    }
}

/// Maximum size of a single cell value in bytes.
pub const DEFAULT_MAX_CELL_BYTES: usize = 1024 * 1024; // 1 MB

/// Parse CSV data already loaded by the caller into a Dataset with type inference.
/// Enforces the byte-size limit before parsing.
pub fn parse_csv_bytes(data: &[u8], max_bytes: usize) -> Result<Dataset, CsvError> {
    if data.len() > max_bytes {
        return Err(CsvError::Limit(LimitError::PayloadTooLarge {
            size: data.len(),
            max: max_bytes,
        }));
    }

    parse_csv_bytes_with_cell_limit(data, DEFAULT_MAX_CELL_BYTES)
}

/// Parse CSV from bytes with configurable max cell size.
fn parse_csv_bytes_with_cell_limit(
    data: &[u8],
    max_cell_bytes: usize,
) -> Result<Dataset, CsvError> {
    let mut csv_reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(false) // reject rows with wrong column count
        .from_reader(data);

    let headers: Vec<String> = csv_reader
        .headers()?
        .iter()
        .map(|h| h.to_string())
        .collect();

    if headers.len() > limits::MAX_COLUMNS {
        return Err(CsvError::Limit(LimitError::TooManyColumns {
            count: headers.len(),
            max: limits::MAX_COLUMNS,
        }));
    }

    // Validate headers: no duplicates, no empty names
    let mut seen = HashSet::new();
    for (pos, h) in headers.iter().enumerate() {
        if h.is_empty() {
            return Err(CsvError::EmptyHeader { position: pos });
        }
        if !seen.insert(h.as_str()) {
            return Err(CsvError::DuplicateHeader(h.clone()));
        }
    }

    let mut rows = Dataset::new();

    for result in csv_reader.records() {
        let record = result?;

        if rows.len() >= limits::MAX_ROWS_PER_DATASET {
            return Err(CsvError::Limit(LimitError::TooManyRows {
                max: limits::MAX_ROWS_PER_DATASET,
            }));
        }

        let mut row = Row::new();
        // Fill all headers — missing trailing fields become Null
        for (i, header) in headers.iter().enumerate() {
            let value = match record.get(i) {
                Some(field) => {
                    if field.len() > max_cell_bytes {
                        return Err(CsvError::CellTooLarge {
                            row: rows.len(),
                            field: header.clone(),
                            size: field.len(),
                            max: max_cell_bytes,
                        });
                    }
                    infer_cell_value(field)
                }
                None => CellValue::Null,
            };
            row.insert(header.clone(), value);
        }
        rows.push(row);
    }

    Ok(rows)
}

/// Parse CSV from a string (convenience for tests). Uses default limits.
pub fn parse_csv_str(s: &str) -> Result<Dataset, CsvError> {
    parse_csv_bytes(s.as_bytes(), limits::MAX_CSV_BYTES)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::types::CellValue;

    #[test]
    fn parse_simple_csv() {
        let csv = "name,age,active\nAlice,30,true\nBob,25,false\n";
        let rows = parse_csv_str(csv).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["name"], CellValue::String("Alice".to_string()));
        assert_eq!(rows[0]["age"], CellValue::Number(30.0));
        assert_eq!(rows[0]["active"], CellValue::Bool(true));
    }

    #[test]
    fn parse_csv_with_types() {
        let csv = "datetime,path,count\n2026-04-04T18:00:00Z,/en/,5\n";
        let rows = parse_csv_str(csv).unwrap();
        assert_eq!(
            rows[0]["datetime"],
            CellValue::String("2026-04-04T18:00:00Z".to_string())
        );
        assert_eq!(rows[0]["count"], CellValue::Number(5.0));
    }

    #[test]
    fn parse_csv_empty_fields_become_null() {
        let csv = "a,b,c\n1,,3\n";
        let rows = parse_csv_str(csv).unwrap();
        assert_eq!(rows[0]["b"], CellValue::Null);
    }

    #[test]
    fn parse_csv_quoted_fields() {
        let csv = "name,desc\n\"Alice\",\"Has a, comma\"\n";
        let rows = parse_csv_str(csv).unwrap();
        assert_eq!(
            rows[0]["desc"],
            CellValue::String("Has a, comma".to_string())
        );
    }

    #[test]
    fn parse_csv_header_only() {
        let rows = parse_csv_str("a,b,c\n").unwrap();
        assert_eq!(rows.len(), 0);
    }

    #[test]
    fn parse_empty_csv() {
        let rows = parse_csv_str("").unwrap();
        assert_eq!(rows.len(), 0);
    }

    #[test]
    #[allow(clippy::approx_constant)] // -3.14 is a decimal-parse sample, not π
    fn parse_csv_negative_numbers() {
        let csv = "val\n-42\n-3.14\n";
        let rows = parse_csv_str(csv).unwrap();
        assert_eq!(rows[0]["val"], CellValue::Number(-42.0));
        assert_eq!(rows[1]["val"], CellValue::Number(-3.14));
    }

    #[test]
    fn duplicate_headers_rejected() {
        let csv = "a,b,a\n1,2,3\n";
        let result = parse_csv_str(csv);
        assert!(matches!(result, Err(CsvError::DuplicateHeader(h)) if h == "a"));
    }

    #[test]
    fn empty_header_rejected() {
        let csv = "a,,c\n1,2,3\n";
        let result = parse_csv_str(csv);
        assert!(matches!(result, Err(CsvError::EmptyHeader { position: 1 })));
    }

    #[test]
    fn too_many_columns_rejected() {
        let headers: Vec<String> = (0..=limits::MAX_COLUMNS)
            .map(|i| format!("c{}", i))
            .collect();
        let csv = format!("{}\n", headers.join(","));
        let result = parse_csv_str(&csv);
        assert!(matches!(
            result,
            Err(CsvError::Limit(LimitError::TooManyColumns { .. }))
        ));
    }

    #[test]
    fn cell_too_large_rejected() {
        let csv = "val\nhello\n";
        // Use a tiny max cell size for testing
        let result = parse_csv_bytes_with_cell_limit(csv.as_bytes(), 3);
        assert!(matches!(result, Err(CsvError::CellTooLarge { .. })));
    }

    #[test]
    fn cell_within_limit_accepted() {
        let csv = "val\nhello\n";
        let result = parse_csv_bytes_with_cell_limit(csv.as_bytes(), 100);
        assert!(result.is_ok());
    }

    #[test]
    fn byte_size_limit_enforced() {
        let csv = "a\n1\n";
        let result = parse_csv_bytes(csv.as_bytes(), 2); // too small
        assert!(matches!(
            result,
            Err(CsvError::Limit(LimitError::PayloadTooLarge { .. }))
        ));
    }

    #[test]
    fn byte_size_limit_passes() {
        let csv = "a\n1\n";
        let result = parse_csv_bytes(csv.as_bytes(), 1000);
        assert!(result.is_ok());
    }
}
