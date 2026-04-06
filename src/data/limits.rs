pub const MAX_ROWS_PER_DATASET: usize = 50_000;
pub const MAX_DATASETS_PER_PAD: usize = 10;
pub const MAX_PAYLOAD_BYTES: usize = 20 * 1024 * 1024; // 20 MB
pub const MAX_CSV_BYTES: usize = 50 * 1024 * 1024; // 50 MB
pub const MAX_COLUMNS: usize = 100;

#[derive(Debug)]
pub enum LimitError {
    TooManyRows { max: usize },
    TooManyColumns { count: usize, max: usize },
    TooManyDatasets { count: usize, max: usize },
    PayloadTooLarge { size: usize, max: usize },
}

impl std::fmt::Display for LimitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LimitError::TooManyRows { max } => {
                write!(f, "Dataset exceeds {} row limit", max)
            }
            LimitError::TooManyColumns { count, max } => {
                write!(f, "Dataset has {} columns, max is {}", count, max)
            }
            LimitError::TooManyDatasets { count, max } => {
                write!(f, "Pad has {} datasets, max is {}", count, max)
            }
            LimitError::PayloadTooLarge { size, max } => {
                write!(
                    f,
                    "Payload is {} bytes, max is {} bytes",
                    size, max
                )
            }
        }
    }
}

/// Check that the number of datasets is within limits.
pub fn check_dataset_count(count: usize) -> Result<(), LimitError> {
    if count > MAX_DATASETS_PER_PAD {
        Err(LimitError::TooManyDatasets {
            count,
            max: MAX_DATASETS_PER_PAD,
        })
    } else {
        Ok(())
    }
}

/// Check payload size.
pub fn check_payload_size(size: usize) -> Result<(), LimitError> {
    if size > MAX_PAYLOAD_BYTES {
        Err(LimitError::PayloadTooLarge {
            size,
            max: MAX_PAYLOAD_BYTES,
        })
    } else {
        Ok(())
    }
}

/// Check CSV file size.
pub fn check_csv_size(size: usize) -> Result<(), LimitError> {
    if size > MAX_CSV_BYTES {
        Err(LimitError::PayloadTooLarge {
            size,
            max: MAX_CSV_BYTES,
        })
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dataset_count_within_limit() {
        assert!(check_dataset_count(5).is_ok());
        assert!(check_dataset_count(10).is_ok());
    }

    #[test]
    fn dataset_count_over_limit() {
        assert!(check_dataset_count(11).is_err());
    }

    #[test]
    fn payload_within_limit() {
        assert!(check_payload_size(1024).is_ok());
    }

    #[test]
    fn payload_over_limit() {
        assert!(check_payload_size(MAX_PAYLOAD_BYTES + 1).is_err());
    }

    #[test]
    fn csv_within_limit() {
        assert!(check_csv_size(1024).is_ok());
    }

    #[test]
    fn csv_over_limit() {
        assert!(check_csv_size(MAX_CSV_BYTES + 1).is_err());
    }
}
