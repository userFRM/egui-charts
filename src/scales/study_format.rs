//! Study value units and scale placement.

use super::price_formatter::format_decimal;
use crate::scales::VolumeFormatter;

/// Units used in study legends, axes, tooltips and data windows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub enum StudyFormat {
    /// Price, following instrument precision unless explicitly overridden.
    Price,
    /// Signed volume with K, M and B suffixes. Without a precision, values below a thousand have
    /// no decimals and K, M and B values have two, also those that reach a thousand only after
    /// rounding: 999.4 reads 999 and 999.6 reads 1.00K.
    Volume,
    /// Percentage units: a value of 5 represents 5%, not 500%.
    Percent,
    /// A fixed number of decimal places, capped at twelve.
    Fixed(usize),
}

impl StudyFormat {
    /// Format a finite value with an optional precision override; non-finite values use a dash.
    pub fn format(self, value: f64, precision: Option<usize>) -> String {
        if !value.is_finite() {
            return "-".to_owned();
        }
        let decimals = precision
            .unwrap_or(match self {
                Self::Fixed(decimals) => decimals,
                Self::Volume => 0,
                _ => 2,
            })
            .min(if self == Self::Price { 18 } else { 12 });
        match self {
            // Without a precision, K, M and B values keep two decimals, also after rounding up.
            Self::Volume => VolumeFormatter {
                precision: decimals,
            }
            .format_with_min_unit_decimals(value, if precision.is_none() { 2 } else { 0 }),
            Self::Percent => format!("{}%", format_decimal(value, decimals)),
            Self::Price | Self::Fixed(_) => format_decimal(value, decimals),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::StudyFormat::*;

    #[test]
    fn formats_each_unit() {
        for (format, value, precision, expected) in [
            (Volume, 1_500_000_000.0, None, "1.50B"),
            (Volume, 2_500_000.0, None, "2.50M"),
            (Volume, 3_500.0, None, "3.50K"),
            (Volume, 500.0, None, "500"),
            (Volume, -12_345_678.0, None, "-12.35M"),
            // Values below a thousand have no decimals; those that round up to a K or M value
            // take its two.
            (Volume, 999.4, None, "999"),
            (Volume, 999.6, None, "1.00K"),
            (Volume, 1_000.4, None, "1.00K"),
            (Volume, 999_999.6, None, "1.00M"),
            (Volume, 0.0, Some(2), "0.00"),
            (Volume, 1_234.0, Some(3), "1.234K"),
            (Price, 1.23456, Some(5), "1.23456"),
            (Price, -37.625, Some(3), "-37.625"),
            (Percent, 5.125, None, "5.12%"),
            (Percent, -2.5, Some(1), "-2.5%"),
            (Fixed(4), 0.0004, None, "0.0004"),
            (Fixed(4), 1.25, Some(1), "1.2"),
            (Fixed(usize::MAX), 1.0, None, "1.000000000000"),
            (Volume, f64::NAN, None, "-"),
            (Percent, f64::INFINITY, None, "-"),
        ] {
            assert_eq!(format.format(value, precision), expected, "{format:?}");
        }
    }
}
