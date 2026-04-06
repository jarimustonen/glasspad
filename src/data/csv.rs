use std::io::Read;

use super::infer::infer_cell_value;
use super::limits::{self, LimitError};
use super::types::{Dataset, Row};

#[derive(Debug)]
pub enum CsvError {
    Csv(csv::Error),
    Limit(LimitError),
}

impl std::fmt::Display for CsvError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CsvError::Csv(e) => write!(f, "CSV parse error: {}", e),
            CsvError::Limit(e) => write!(f, "{}", e),
        }
    }
}

impl From<csv::Error> for CsvError {
    fn from(e: csv::Error) -> Self {
        CsvError::Csv(e)
    }
}

/// Parse CSV data from a reader into a Dataset with type inference.
pub fn parse_csv<R: Read>(reader: R) -> Result<Dataset, CsvError> {
    let mut csv_reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .from_reader(reader);

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

    let mut rows = Dataset::new();

    for result in csv_reader.records() {
        let record = result?;

        if rows.len() >= limits::MAX_ROWS_PER_DATASET {
            return Err(CsvError::Limit(LimitError::TooManyRows {
                max: limits::MAX_ROWS_PER_DATASET,
            }));
        }

        let mut row = Row::new();
        for (i, field) in record.iter().enumerate() {
            if let Some(header) = headers.get(i) {
                row.insert(header.clone(), infer_cell_value(field));
            }
        }
        rows.push(row);
    }

    Ok(rows)
}

/// Parse CSV from a string.
pub fn parse_csv_str(s: &str) -> Result<Dataset, CsvError> {
    parse_csv(s.as_bytes())
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
        assert_eq!(rows[1]["name"], CellValue::String("Bob".to_string()));
    }

    #[test]
    fn parse_csv_with_types() {
        let csv = "datetime,path,count\n2026-04-04T18:00:00Z,/en/,5\n2026-04-04T19:00:00Z,/blog/,3\n";
        let rows = parse_csv_str(csv).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(
            rows[0]["datetime"],
            CellValue::String("2026-04-04T18:00:00Z".to_string())
        );
        assert_eq!(
            rows[0]["path"],
            CellValue::String("/en/".to_string())
        );
        assert_eq!(rows[0]["count"], CellValue::Number(5.0));
    }

    #[test]
    fn parse_csv_empty_fields() {
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
        let csv = "a,b,c\n";
        let rows = parse_csv_str(csv).unwrap();
        assert_eq!(rows.len(), 0);
    }

    #[test]
    fn parse_empty_csv() {
        let result = parse_csv_str("");
        // empty input has no headers — csv crate returns empty
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 0);
    }

    #[test]
    fn parse_csv_negative_numbers() {
        let csv = "val\n-42\n-3.14\n";
        let rows = parse_csv_str(csv).unwrap();
        assert_eq!(rows[0]["val"], CellValue::Number(-42.0));
        assert_eq!(rows[1]["val"], CellValue::Number(-3.14));
    }

    #[test]
    fn parse_csv_boolean_case_insensitive() {
        let csv = "flag\nTRUE\nFalse\n";
        let rows = parse_csv_str(csv).unwrap();
        assert_eq!(rows[0]["flag"], CellValue::Bool(true));
        assert_eq!(rows[1]["flag"], CellValue::Bool(false));
    }

    #[test]
    fn too_many_columns_rejected() {
        let headers: Vec<String> = (0..=limits::MAX_COLUMNS).map(|i| format!("c{}", i)).collect();
        let csv = format!("{}\n", headers.join(","));
        let result = parse_csv_str(&csv);
        assert!(matches!(result, Err(CsvError::Limit(LimitError::TooManyColumns { .. }))));
    }
}
