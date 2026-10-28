//! Price formatting for Y-axis labels.
//!
//! The [`PriceFormatter`] trait lets users control how numeric price values are
//! rendered as text on the price axis.  The library ships with several built-in
//! formatters ([`DefaultPriceFormatter`], [`CurrencyFormatter`],
//! [`VolumeFormatter`], [`PercentageFormatter`], [`ScientificFormatter`]) and
//! supports closures via [`CustomPriceFormatter`].
//!
//! Built-in formatters cap precision at 324 decimal places, enough to display the
//! smallest positive `f64`. This also applies when public fields are set directly.
//! Use [`PriceFormatterBuilder`] for a fluent construction API.

use crate::model::{Instrument, percent_change};
use std::sync::Arc;

// Enough decimal places to keep even the smallest positive f64 from rounding to zero.
const MAX_PRECISION: usize = 324;

// Inspect the rounded digits so rounding ties follow Rust's formatter.
pub(super) fn format_decimal(value: f64, precision: usize) -> String {
    let precision = precision.min(MAX_PRECISION);
    let mut formatted = format!("{value:.precision$}");
    if formatted
        .strip_prefix('-')
        .is_some_and(|digits| digits.bytes().all(|b| b == b'0' || b == b'.'))
    {
        formatted.remove(0);
    }
    formatted
}

/// Instrument decimals, falling back to the decimal places needed to write its minimum tick.
/// Invalid tick sizes are ignored; explicit decimals take precedence.
pub fn instrument_price_precision(instrument: &Instrument) -> Option<u8> {
    instrument.price_decimals.or_else(|| {
        let tick = instrument
            .tick_size
            .filter(|tick| tick.is_finite() && *tick > 0.0)?;
        let text = tick.to_string();
        Some(text.split_once('.').map_or(0, |(_, decimals)| {
            decimals.len().min(usize::from(u8::MAX)) as u8
        }))
    })
}

/// Trait for custom price formatting on the Y axis.
///
/// Implement this trait to control how numeric prices are displayed as labels.
/// All formatters must be `Send + Sync` so they can be shared across the
/// render pipeline.
pub trait PriceFormatter: Send + Sync {
    /// Format a price value into a display string
    ///
    /// # Arguments
    /// * `price` - The price value to format
    ///
    /// # Returns
    /// Formatted string to display on the price axis
    fn format(&self, price: f64) -> String;

    /// Format a price axis tick. Defaults to [`format`](Self::format); override it
    /// to write ticks differently from crosshair and price tag labels.
    fn format_tick(&self, price: f64) -> String {
        self.format(price)
    }

    /// Clone this formatter into a Box
    fn clone_box(&self) -> Box<dyn PriceFormatter>;
}

/// Default price formatter with configurable precision
#[derive(Debug, Clone)]
pub struct DefaultPriceFormatter {
    /// Number of decimal places, capped at 324 when formatting.
    pub precision: usize,

    /// Min number of decimal places (0 = auto)
    pub min_precision: usize,
}

impl Default for DefaultPriceFormatter {
    fn default() -> Self {
        Self {
            precision: 2,
            min_precision: 0,
        }
    }
}

impl DefaultPriceFormatter {
    /// Create formatter with fixed precision
    pub fn with_precision(precision: usize) -> Self {
        Self {
            precision,
            min_precision: precision,
        }
    }

    /// Create formatter with auto precision (shows significant digits)
    pub fn auto() -> Self {
        Self {
            precision: 8,
            min_precision: 0,
        }
    }
}

impl PriceFormatter for DefaultPriceFormatter {
    fn format(&self, price: f64) -> String {
        if self.min_precision > 0 {
            // Fixed precision
            format_decimal(price, self.precision)
        } else {
            // Auto precision - remove trailing zeros
            let formatted = format_decimal(price, self.precision);
            if self.precision == 0 {
                return formatted;
            }
            formatted
                .trim_end_matches('0')
                .trim_end_matches('.')
                .to_string()
        }
    }

    fn clone_box(&self) -> Box<dyn PriceFormatter> {
        Box::new(self.clone())
    }
}

/// Percentage price formatter
#[derive(Debug, Clone)]
pub struct PercentageFormatter {
    /// Number of decimal places for percentage, capped at 324 when formatting.
    pub precision: usize,

    /// Base value for percentage calculation (100.0 = 100%)
    pub base_val: f64,
}

impl Default for PercentageFormatter {
    fn default() -> Self {
        Self {
            precision: 2,
            base_val: 100.0,
        }
    }
}

impl PriceFormatter for PercentageFormatter {
    fn format(&self, price: f64) -> String {
        // A zero base has no defined percentage: percent_change gives a neutral 0%
        // rather than "inf%"/"NaN%" on the axis.
        let percentage = percent_change(self.base_val, price);
        let formatted = format_decimal(percentage, self.precision);
        let sign = if formatted.starts_with('-') { "" } else { "+" };
        format!("{sign}{formatted}%")
    }

    fn clone_box(&self) -> Box<dyn PriceFormatter> {
        Box::new(self.clone())
    }
}

/// Currency formatter with symbol.
///
/// If the thousands and decimal separators match, grouping is omitted so an amount
/// never uses the same character for both separators, including after direct field edits.
#[derive(Debug, Clone)]
pub struct CurrencyFormatter {
    /// Currency symbol (e.g., "$", "€", "£", "¥")
    pub symbol: String,

    /// Number of decimal places, capped at 324 when formatting.
    pub precision: usize,

    /// Whether to place symbol before or after price
    pub symbol_before: bool,

    /// Whether to add space between symbol and value
    pub add_space: bool,

    /// Thousands separator (e.g., "," for US, "." for EU)
    pub thousands_sep: Option<char>,

    /// Decimal separator (e.g., '.' for US, ',' for EU).
    pub decimal_sep: char,
}

impl CurrencyFormatter {
    /// Create USD formatter
    pub fn usd() -> Self {
        Self {
            symbol: "$".to_string(),
            precision: 2,
            symbol_before: true,
            add_space: false,
            thousands_sep: Some(','),
            decimal_sep: '.',
        }
    }

    /// Create EUR formatter
    pub fn eur() -> Self {
        Self {
            symbol: "€".to_string(),
            precision: 2,
            symbol_before: false,
            add_space: true,
            thousands_sep: Some('.'),
            decimal_sep: ',',
        }
    }

    /// Create BTC formatter
    pub fn btc() -> Self {
        Self {
            symbol: "₿".to_string(),
            precision: 8,
            symbol_before: false,
            add_space: true,
            thousands_sep: Some(','),
            decimal_sep: '.',
        }
    }

    /// Create custom currency formatter
    pub fn new(symbol: impl Into<String>, precision: usize) -> Self {
        Self {
            symbol: symbol.into(),
            precision,
            symbol_before: true,
            add_space: false,
            thousands_sep: Some(','),
            decimal_sep: '.',
        }
    }
}

impl PriceFormatter for CurrencyFormatter {
    fn format(&self, price: f64) -> String {
        let val_str = if let Some(sep) = self.thousands_sep.filter(|sep| *sep != self.decimal_sep) {
            self.format_with_separator(price, sep)
        } else {
            format_decimal(price, self.precision).replace('.', &self.decimal_sep.to_string())
        };

        let space = if self.add_space { " " } else { "" };

        if self.symbol_before {
            // A negative amount reads "-$1,234.50": the sign moves in front of
            // the symbol instead of between it and the digits.
            match val_str.strip_prefix('-') {
                Some(digits) => format!("-{}{}{}", self.symbol, space, digits),
                None => format!("{}{}{}", self.symbol, space, val_str),
            }
        } else {
            format!("{}{}{}", val_str, space, self.symbol)
        }
    }

    fn clone_box(&self) -> Box<dyn PriceFormatter> {
        Box::new(self.clone())
    }
}

impl CurrencyFormatter {
    fn format_with_separator(&self, price: f64, separator: char) -> String {
        let formatted = format_decimal(price, self.precision);
        let mut parts = formatted.splitn(2, '.');

        let integer = parts.next().unwrap_or("");
        let decimal = parts.next().unwrap_or("");

        // Group only the digits of the magnitude; the sign is never counted.
        let (sign, magnitude) = match integer.strip_prefix('-') {
            Some(digits) => ("-", digits),
            None => ("", integer),
        };

        let integer_with_sep: String = magnitude
            .chars()
            .rev()
            .enumerate()
            .fold(String::new(), |mut acc, (i, c)| {
                if i > 0 && i % 3 == 0 {
                    acc.push(separator);
                }
                acc.push(c);
                acc
            })
            .chars()
            .rev()
            .collect();

        let body = if decimal.is_empty() {
            integer_with_sep
        } else {
            format!("{integer_with_sep}{}{decimal}", self.decimal_sep)
        };
        format!("{sign}{body}")
    }
}

/// Volume formatter (K, M, B suffixes)
#[derive(Debug, Clone)]
pub struct VolumeFormatter {
    /// Number of decimal places, capped at 324 when formatting.
    pub precision: usize,
}

impl Default for VolumeFormatter {
    fn default() -> Self {
        Self { precision: 2 }
    }
}

impl PriceFormatter for VolumeFormatter {
    fn format(&self, value: f64) -> String {
        let abs_val = value.abs();
        let (mut unit, mut suffix) = if abs_val >= 1_000_000_000.0 {
            (1_000_000_000.0, "B")
        } else if abs_val >= 1_000_000.0 {
            (1_000_000.0, "M")
        } else if abs_val >= 1_000.0 {
            (1_000.0, "K")
        } else {
            (1.0, "")
        };
        let mut formatted = format_decimal(value / unit, self.precision);
        if unit < 1_000_000_000.0
            && formatted.trim_start_matches('-').split('.').next() == Some("1000")
        {
            unit *= 1_000.0;
            suffix = match suffix {
                "" => "K",
                "K" => "M",
                _ => "B",
            };
            formatted = format_decimal(value / unit, self.precision);
        }
        format!("{formatted}{suffix}")
    }

    fn clone_box(&self) -> Box<dyn PriceFormatter> {
        Box::new(self.clone())
    }
}

/// Scientific notation formatter
#[derive(Debug, Clone)]
pub struct ScientificFormatter {
    /// Number of digits after the decimal point, capped at 324 when formatting.
    pub precision: usize,
}

impl Default for ScientificFormatter {
    fn default() -> Self {
        Self { precision: 3 }
    }
}

impl PriceFormatter for ScientificFormatter {
    fn format(&self, price: f64) -> String {
        let price = if price == 0.0 { 0.0 } else { price };
        format!(
            "{:.prec$e}",
            price,
            prec = self.precision.min(MAX_PRECISION)
        )
    }

    fn clone_box(&self) -> Box<dyn PriceFormatter> {
        Box::new(self.clone())
    }
}

/// Custom price formatter using a closure
pub struct CustomPriceFormatter {
    formatter: Arc<dyn Fn(f64) -> String + Send + Sync>,
}

impl CustomPriceFormatter {
    /// Create a new custom formatter from a closure
    pub fn new<F>(formatter: F) -> Self
    where
        F: Fn(f64) -> String + Send + Sync + 'static,
    {
        Self {
            formatter: Arc::new(formatter),
        }
    }
}

impl PriceFormatter for CustomPriceFormatter {
    fn format(&self, price: f64) -> String {
        (self.formatter)(price)
    }

    fn clone_box(&self) -> Box<dyn PriceFormatter> {
        Box::new(Self {
            formatter: Arc::clone(&self.formatter),
        })
    }
}

/// Builder for creating price formatters
pub struct PriceFormatterBuilder {
    precision: usize,
    currency: Option<String>,
    percentage: bool,
    volume: bool,
}

impl PriceFormatterBuilder {
    /// Create a new formatter builder
    pub fn new() -> Self {
        Self {
            precision: 2,
            currency: None,
            percentage: false,
            volume: false,
        }
    }

    /// Set precision
    pub fn with_precision(mut self, precision: usize) -> Self {
        self.precision = precision;
        self
    }

    /// Set currency symbol
    pub fn with_currency(mut self, symbol: impl Into<String>) -> Self {
        self.currency = Some(symbol.into());
        self
    }

    /// Enable percentage formatting
    pub fn as_percentage(mut self) -> Self {
        self.percentage = true;
        self
    }

    /// Enable volume formatting
    pub fn as_volume(mut self) -> Self {
        self.volume = true;
        self
    }

    /// Build the formatter
    pub fn build(self) -> Box<dyn PriceFormatter> {
        if self.percentage {
            Box::new(PercentageFormatter {
                precision: self.precision,
                base_val: 100.0,
            })
        } else if self.volume {
            Box::new(VolumeFormatter {
                precision: self.precision,
            })
        } else if let Some(symbol) = self.currency {
            Box::new(CurrencyFormatter::new(symbol, self.precision))
        } else {
            Box::new(DefaultPriceFormatter::with_precision(self.precision))
        }
    }
}

impl Default for PriceFormatterBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Decimals for a price by its magnitude: 2 from 100 up, 4 from 1 up, 6 below 1.
#[cfg(feature = "widget")]
pub(crate) fn price_decimals(price: f64) -> usize {
    if price.abs() >= 100.0 {
        2
    } else if price.abs() >= 1.0 {
        4
    } else {
        6
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_formatter() {
        let formatter = DefaultPriceFormatter::default();
        assert_eq!(formatter.format(123.456), "123.46");
        assert_eq!(formatter.format(123.0), "123");
        for value in [-0.0, -0.001] {
            assert_eq!(formatter.format(value), "0");
            assert_eq!(
                DefaultPriceFormatter::with_precision(2).format(value),
                "0.00"
            );
            for unit in [
                super::super::StudyFormat::Price,
                super::super::StudyFormat::Fixed(2),
            ] {
                assert_eq!(unit.format(value, None), "0.00");
            }
            assert_eq!(
                super::super::StudyFormat::Percent.format(value, None),
                "0.00%"
            );
        }
        assert_eq!(formatter.format(-0.006), "-0.01");
    }

    #[test]
    fn extreme_precision_is_bounded() {
        let decimals = "0".repeat(324);
        for precision in [324, 325, usize::MAX] {
            assert_eq!(
                DefaultPriceFormatter::with_precision(precision).format(100.0),
                format!("100.{decimals}")
            );
            assert_eq!(
                DefaultPriceFormatter {
                    precision,
                    min_precision: 0,
                }
                .format(100.0),
                "100"
            );
            assert_eq!(
                CurrencyFormatter::new("$", precision).format(100.0),
                format!("$100.{decimals}")
            );
            assert_eq!(
                PercentageFormatter {
                    precision,
                    base_val: 100.0
                }
                .format(200.0),
                format!("+100.{decimals}%")
            );
            for (value, suffix) in [
                (100.0, ""),
                (100_000.0, "K"),
                (100_000_000.0, "M"),
                (100_000_000_000.0, "B"),
            ] {
                assert_eq!(
                    VolumeFormatter { precision }.format(value),
                    format!("100.{decimals}{suffix}")
                );
            }
            assert_eq!(
                ScientificFormatter { precision }.format(100.0),
                format!("1.{decimals}e2")
            );
        }
    }

    #[test]
    fn test_fixed_precision() {
        let formatter = DefaultPriceFormatter::with_precision(4);
        assert_eq!(formatter.format(123.456), "123.4560");
        for value in [0.0, 10.0, 100.0, -100.0] {
            assert_eq!(
                DefaultPriceFormatter::with_precision(0).format(value),
                format!("{value:.0}")
            );
        }
    }

    #[test]
    fn test_percentage_formatter() {
        let formatter = PercentageFormatter::default();
        assert_eq!(formatter.format(105.0), "+5.00%");
        assert_eq!(formatter.format(95.0), "-5.00%");
        assert_eq!(formatter.format(99.999), "+0.00%");
        assert_eq!(formatter.format(99.994), "-0.01%");

        // A negative base (a spread) reads with the sign of the change.
        let spread = PercentageFormatter {
            precision: 2,
            base_val: -10.0,
        };
        assert_eq!(spread.format(-5.0), "+50.00%");
        assert_eq!(spread.format(-15.0), "-50.00%");
    }

    #[test]
    fn test_percentage_formatter_zero_base_is_neutral() {
        // A zero base has no defined percentage; the formatter must emit a
        // neutral 0% rather than "inf%"/"NaN%".
        let formatter = PercentageFormatter {
            precision: 2,
            base_val: 0.0,
        };
        let out = formatter.format(123.0);
        assert_eq!(out, "+0.00%");
        assert!(!out.contains("inf") && !out.contains("NaN"));
    }

    #[test]
    fn test_currency_formatter() {
        let usd = CurrencyFormatter::usd();
        assert_eq!(usd.format(1234.56), "$1,234.56");

        for (formatter, value, expected) in [
            (CurrencyFormatter::eur(), 1234.56, "1.234,56 €"),
            (CurrencyFormatter::eur(), -1234.5, "-1.234,50 €"),
            (CurrencyFormatter::eur(), 0.5, "0,50 €"),
            (CurrencyFormatter::eur(), 123.45, "123,45 €"),
            (CurrencyFormatter::btc(), 0.12345678, "0.12345678 ₿"),
            (
                CurrencyFormatter {
                    thousands_sep: None,
                    ..CurrencyFormatter::eur()
                },
                1234.56,
                "1234,56 €",
            ),
            (
                CurrencyFormatter {
                    decimal_sep: '.',
                    ..CurrencyFormatter::eur()
                },
                1234.56,
                "1234.56 €",
            ),
            (
                CurrencyFormatter {
                    decimal_sep: ',',
                    ..CurrencyFormatter::usd()
                },
                1234.56,
                "$1234,56",
            ),
            (
                CurrencyFormatter {
                    precision: 0,
                    ..CurrencyFormatter::eur()
                },
                1234.56,
                "1.235 €",
            ),
        ] {
            assert_eq!(formatter.format(value), expected);
        }
    }

    #[test]
    fn test_currency_negative_amounts_sign_before_symbol_and_never_grouped() {
        // Expected strings written by hand: the sign sits before the symbol when
        // the symbol comes first, and no separator ever lands next to the sign.
        let cases = [
            (CurrencyFormatter::usd(), -123.45, "-$123.45"),
            (CurrencyFormatter::usd(), -123456.0, "-$123,456.00"),
            (CurrencyFormatter::new("CHF", 1), -1234.5, "-CHF1,234.5"),
            (CurrencyFormatter::usd(), -0.5, "-$0.50"),
            (CurrencyFormatter::usd(), 0.0, "$0.00"),
            (CurrencyFormatter::usd(), -0.0, "$0.00"),
            (CurrencyFormatter::usd(), -0.001, "$0.00"),
            (CurrencyFormatter::usd(), -0.006, "-$0.01"),
            (CurrencyFormatter::new("JPY", 0), -0.1, "JPY0"),
            (CurrencyFormatter::new("JPY", 0), -0.6, "-JPY1"),
            (CurrencyFormatter::btc(), -0.000000001, "0.00000000 ₿"),
            (CurrencyFormatter::eur(), -0.001, "0,00 €"),
            (
                CurrencyFormatter {
                    thousands_sep: None,
                    ..CurrencyFormatter::usd()
                },
                -0.001,
                "$0.00",
            ),
            (CurrencyFormatter::usd(), 123.45, "$123.45"),
            // Symbol after the value keeps the sign on the number itself.
            (CurrencyFormatter::btc(), -1234.5, "-1,234.50000000 ₿"),
        ];
        for (formatter, value, expected) in cases {
            assert_eq!(formatter.format(value), expected);
        }
    }

    #[test]
    fn test_volume_formatter() {
        let formatter = VolumeFormatter::default();
        assert_eq!(formatter.format(1_500_000_000.0), "1.50B");
        assert_eq!(formatter.format(2_500_000.0), "2.50M");
        assert_eq!(formatter.format(3_500.0), "3.50K");
        assert_eq!(formatter.format(500.0), "500.00");
        for value in [-0.0, -0.001] {
            assert_eq!(formatter.format(value), "0.00");
        }
        assert_eq!(formatter.format(-0.006), "-0.01");
        assert_eq!(formatter.format(-3_500.0), "-3.50K");
        for (precision, value, expected) in [
            (2, 999.996, "1.00K"),
            (2, 999_999.0, "1.00M"),
            (2, 999_996_000.0, "1.00B"),
            (2, 999.994, "999.99"),
            (2, 999_994.0, "999.99K"),
            (2, 999_994_000.0, "999.99M"),
            (0, 999.6, "1K"),
            (0, 999_600.0, "1M"),
            (3, 999_999.4, "999.999K"),
            (3, 999_999.6, "1.000M"),
            (2, 999_999_999_999.0, "1000.00B"),
        ] {
            let formatter = VolumeFormatter { precision };
            assert_eq!(formatter.format(value), expected);
            assert_eq!(formatter.format(-value), format!("-{expected}"));
        }
    }

    #[test]
    fn test_scientific_formatter() {
        let formatter = ScientificFormatter::default();
        let result = formatter.format(0.000123);
        assert!(result.contains("e"));
        assert_eq!(formatter.format(-0.0), "0.000e0");
        assert_eq!(formatter.format(-0.001), "-1.000e-3");
    }

    #[test]
    fn test_custom_formatter() {
        let formatter = CustomPriceFormatter::new(|price| format!("Price: {price:.2}"));
        assert_eq!(formatter.format(123.456), "Price: 123.46");
        assert_eq!(formatter.clone_box().format(123.456), "Price: 123.46");
    }

    #[test]
    fn test_formatter_builder() {
        let formatter = PriceFormatterBuilder::new().with_precision(4).build();
        assert_eq!(formatter.format(123.456789), "123.4568");

        let currency_formatter = PriceFormatterBuilder::new()
            .with_currency("$")
            .with_precision(2)
            .build();
        assert!(currency_formatter.format(100.0).contains("$"));
    }
}
