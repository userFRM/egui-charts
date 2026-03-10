//! Data validation and mismatch detection for bar sequences.
//!
//! [`DataValidator`] checks incoming bar data for common integrity issues:
//!
//! - **Timestamp ordering**: bars must be in the expected chronological order.
//! - **Duplicate timestamps**: consecutive bars must not share the same time.
//! - **OHLC invariants**: `high >= max(open, close)`, `low <= min(open, close)`,
//!   and volume is non-negative.
//!
//! Use [`validate_sequence`](DataValidator::validate_sequence) to check a batch
//! of bars, or [`validate_new_bar`](DataValidator::validate_new_bar) to
//! incrementally validate as bars arrive from a live feed.

use crate::model::Bar;
use chrono::{DateTime, Utc};

/// Expected chronological direction of a bar sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MismatchDirection {
    /// Timestamps must be strictly increasing (oldest first).
    ExpectedIncrement,
    /// Timestamps must be strictly decreasing (newest first).
    ExpectedDecrement,
}

/// Outcome of validating one or more bars.
#[derive(Debug, Clone, PartialEq)]
pub enum ValidationResult {
    /// All checks passed.
    Valid,
    /// A timestamp was out of the expected chronological order.
    TsMismatch {
        /// The direction that was expected.
        expected: MismatchDirection,
        /// Timestamp of the preceding bar.
        prev_time: DateTime<Utc>,
        /// Timestamp of the offending bar.
        curr_time: DateTime<Utc>,
        /// Index of the offending bar in the input slice.
        index: usize,
    },
    /// Two consecutive bars share the same timestamp.
    DuplicateTs {
        /// The duplicated timestamp.
        ts: DateTime<Utc>,
        /// Index of the second occurrence.
        index: usize,
    },
    /// An OHLC relationship invariant was violated (e.g., `high < low`).
    InvalidOHLC {
        /// Index of the offending bar.
        index: usize,
        /// Human-readable explanation of the violation.
        reason: String,
    },
}

/// Validates bar sequences for timestamp ordering and OHLC integrity.
///
/// # Example
///
/// ```
/// use egui_charts::validation::DataValidator;
///
/// let validator = DataValidator::new();
/// // validate_sequence(&bars) returns Vec<ValidationResult>
/// ```
pub struct DataValidator {
    /// Expected direction of ts
    expected_direction: MismatchDirection,
    /// Whether to log warnings
    log_warnings: bool,
    /// Whether to validate OHLC relationships
    validate_ohlc: bool,
}

impl Default for DataValidator {
    fn default() -> Self {
        Self::new()
    }
}

impl DataValidator {
    /// Create a new data validator with default settings
    pub fn new() -> Self {
        Self {
            expected_direction: MismatchDirection::ExpectedIncrement,
            log_warnings: true,
            validate_ohlc: true,
        }
    }

    /// Set the expected direction of ts
    pub fn with_direction(mut self, direction: MismatchDirection) -> Self {
        self.expected_direction = direction;
        self
    }

    /// Enable or disable warning logging
    pub fn with_logging(mut self, log_warnings: bool) -> Self {
        self.log_warnings = log_warnings;
        self
    }

    /// Enable or disable OHLC validation
    pub fn with_ohlc_validation(mut self, validate: bool) -> Self {
        self.validate_ohlc = validate;
        self
    }

    /// Validate a sequence of bars
    pub fn validate_sequence(&self, bars: &[Bar]) -> Vec<ValidationResult> {
        let mut results = Vec::new();

        for (i, bar) in bars.iter().enumerate() {
            // Validate OHLC relationships
            if self.validate_ohlc
                && let Some(error) = self.validate_ohlc_bar(bar, i)
            {
                if self.log_warnings {
                    eprintln!("OHLC validation error at index {}: {}", i, error.reason());
                }
                results.push(error);
            }

            // Validate ts sequence
            if i > 0 {
                let prev_bar = &bars[i - 1];
                if let Some(error) = self.validate_ts_sequence(prev_bar, bar, i) {
                    if self.log_warnings {
                        eprintln!("Ts mismatch at index {i}: {error:?}");
                    }
                    results.push(error);
                }
            }
        }

        if results.is_empty() {
            results.push(ValidationResult::Valid);
        }

        results
    }

    /// Validate a single new bar against the last bar in a sequence
    pub fn validate_new_bar(&self, last_bar: Option<&Bar>, new_bar: &Bar) -> ValidationResult {
        // Validate OHLC first
        if self.validate_ohlc
            && let Some(error) = self.validate_ohlc_bar(new_bar, 0)
        {
            if self.log_warnings {
                eprintln!("OHLC validation error: {}", error.reason());
            }
            return error;
        }

        // Validate ts if we have a previous bar
        if let Some(prev) = last_bar
            && let Some(error) = self.validate_ts_sequence(prev, new_bar, 0)
        {
            if self.log_warnings {
                eprintln!("Ts mismatch: {error:?}");
            }
            return error;
        }

        ValidationResult::Valid
    }

    /// Validate OHLC relationships for a single bar
    fn validate_ohlc_bar(&self, bar: &Bar, index: usize) -> Option<ValidationResult> {
        // High must be >= Open, Close, Low
        if bar.high < bar.open || bar.high < bar.close || bar.high < bar.low {
            return Some(ValidationResult::InvalidOHLC {
                index,
                reason: format!(
                    "High ({}) is less than Open ({}), Close ({}), or Low ({})",
                    bar.high, bar.open, bar.close, bar.low
                ),
            });
        }

        // Low must be <= Open, Close, High
        if bar.low > bar.open || bar.low > bar.close || bar.low > bar.high {
            return Some(ValidationResult::InvalidOHLC {
                index,
                reason: format!(
                    "Low ({}) is greater than Open ({}), Close ({}), or High ({})",
                    bar.low, bar.open, bar.close, bar.high
                ),
            });
        }

        // Volume should be non-negative
        if bar.volume < 0.0 {
            return Some(ValidationResult::InvalidOHLC {
                index,
                reason: format!("Volume ({}) is negative", bar.volume),
            });
        }

        None
    }

    /// Validate ts sequence between two bars
    fn validate_ts_sequence(
        &self,
        prev: &Bar,
        current: &Bar,
        index: usize,
    ) -> Option<ValidationResult> {
        // Check for duplicate ts
        if prev.time == current.time {
            return Some(ValidationResult::DuplicateTs {
                ts: current.time,
                index,
            });
        }

        // Check sequence direction
        match self.expected_direction {
            MismatchDirection::ExpectedIncrement => {
                if current.time < prev.time {
                    return Some(ValidationResult::TsMismatch {
                        expected: MismatchDirection::ExpectedIncrement,
                        prev_time: prev.time,
                        curr_time: current.time,
                        index,
                    });
                }
            }
            MismatchDirection::ExpectedDecrement => {
                if current.time > prev.time {
                    return Some(ValidationResult::TsMismatch {
                        expected: MismatchDirection::ExpectedDecrement,
                        prev_time: prev.time,
                        curr_time: current.time,
                        index,
                    });
                }
            }
        }

        None
    }
}

impl ValidationResult {
    /// Returns true if the validation passed
    pub fn is_valid(&self) -> bool {
        matches!(self, ValidationResult::Valid)
    }

    /// Returns true if there's an error
    pub fn is_error(&self) -> bool {
        !self.is_valid()
    }

    /// Get a human-readable reason for the validation failure
    pub fn reason(&self) -> String {
        match self {
            ValidationResult::Valid => "Valid".to_string(),
            ValidationResult::TsMismatch {
                expected,
                prev_time,
                curr_time,
                index,
            } => {
                let direction = match expected {
                    MismatchDirection::ExpectedIncrement => "increasing",
                    MismatchDirection::ExpectedDecrement => "decreasing",
                };
                format!(
                    "Ts mismatch at index {index}: expected {direction} ts, but {prev_time} came before {curr_time}"
                )
            }
            ValidationResult::DuplicateTs { ts, index } => {
                format!("Duplicate ts {ts} at index {index}")
            }
            ValidationResult::InvalidOHLC { index, reason } => {
                format!("Invalid OHLC at index {index}: {reason}")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn create_bar(ts: DateTime<Utc>, open: f64, high: f64, low: f64, close: f64) -> Bar {
        Bar {
            time: ts,
            open,
            high,
            low,
            close,
            volume: 1000.0,
        }
    }

    #[test]
    fn test_valid_sequence() {
        let validator = DataValidator::new();
        let bars = vec![
            create_bar(
                Utc.with_ymd_and_hms(2024, 1, 1, 10, 0, 0).unwrap(),
                100.0,
                105.0,
                95.0,
                102.0,
            ),
            create_bar(
                Utc.with_ymd_and_hms(2024, 1, 1, 11, 0, 0).unwrap(),
                102.0,
                108.0,
                100.0,
                105.0,
            ),
            create_bar(
                Utc.with_ymd_and_hms(2024, 1, 1, 12, 0, 0).unwrap(),
                105.0,
                110.0,
                103.0,
                107.0,
            ),
        ];

        let results = validator.validate_sequence(&bars);
        assert_eq!(results.len(), 1);
        assert!(results[0].is_valid());
    }

    #[test]
    fn test_ts_mismatch() {
        let validator = DataValidator::new().with_logging(false);
        let bars = vec![
            create_bar(
                Utc.with_ymd_and_hms(2024, 1, 1, 10, 0, 0).unwrap(),
                100.0,
                105.0,
                95.0,
                102.0,
            ),
            create_bar(
                Utc.with_ymd_and_hms(2024, 1, 1, 9, 0, 0).unwrap(),
                102.0,
                108.0,
                100.0,
                105.0,
            ), // Out of order!
        ];

        let results = validator.validate_sequence(&bars);
        assert!(
            results
                .iter()
                .any(|r| matches!(r, ValidationResult::TsMismatch { .. }))
        );
    }

    #[test]
    fn test_duplicate_ts() {
        let validator = DataValidator::new().with_logging(false);
        let bars = vec![
            create_bar(
                Utc.with_ymd_and_hms(2024, 1, 1, 10, 0, 0).unwrap(),
                100.0,
                105.0,
                95.0,
                102.0,
            ),
            create_bar(
                Utc.with_ymd_and_hms(2024, 1, 1, 10, 0, 0).unwrap(),
                102.0,
                108.0,
                100.0,
                105.0,
            ), // Duplicate!
        ];

        let results = validator.validate_sequence(&bars);
        assert!(
            results
                .iter()
                .any(|r| matches!(r, ValidationResult::DuplicateTs { .. }))
        );
    }

    #[test]
    fn test_invalid_ohlc_high_too_low() {
        let validator = DataValidator::new().with_logging(false);
        let bar = create_bar(
            Utc.with_ymd_and_hms(2024, 1, 1, 10, 0, 0).unwrap(),
            100.0,
            95.0, // High less than open!
            90.0,
            98.0,
        );

        let result = validator.validate_new_bar(None, &bar);
        assert!(matches!(result, ValidationResult::InvalidOHLC { .. }));
    }

    #[test]
    fn test_invalid_ohlc_low_too_high() {
        let validator = DataValidator::new().with_logging(false);
        let bar = create_bar(
            Utc.with_ymd_and_hms(2024, 1, 1, 10, 0, 0).unwrap(),
            100.0,
            110.0,
            105.0, // Low greater than open!
            102.0,
        );

        let result = validator.validate_new_bar(None, &bar);
        assert!(matches!(result, ValidationResult::InvalidOHLC { .. }));
    }

    #[test]
    fn test_descending_sequence() {
        let validator = DataValidator::new()
            .with_direction(MismatchDirection::ExpectedDecrement)
            .with_logging(false);

        let bars = vec![
            create_bar(
                Utc.with_ymd_and_hms(2024, 1, 1, 12, 0, 0).unwrap(),
                100.0,
                105.0,
                95.0,
                102.0,
            ),
            create_bar(
                Utc.with_ymd_and_hms(2024, 1, 1, 11, 0, 0).unwrap(),
                102.0,
                108.0,
                100.0,
                105.0,
            ),
            create_bar(
                Utc.with_ymd_and_hms(2024, 1, 1, 10, 0, 0).unwrap(),
                105.0,
                110.0,
                103.0,
                107.0,
            ),
        ];

        let results = validator.validate_sequence(&bars);
        assert_eq!(results.len(), 1);
        assert!(results[0].is_valid());
    }

    #[test]
    fn test_validation_result_reason() {
        let result = ValidationResult::TsMismatch {
            expected: MismatchDirection::ExpectedIncrement,
            prev_time: Utc.with_ymd_and_hms(2024, 1, 1, 10, 0, 0).unwrap(),
            curr_time: Utc.with_ymd_and_hms(2024, 1, 1, 9, 0, 0).unwrap(),
            index: 1,
        };

        let reason = result.reason();
        assert!(reason.contains("mismatch"));
        assert!(reason.contains("increasing"));
    }
}
