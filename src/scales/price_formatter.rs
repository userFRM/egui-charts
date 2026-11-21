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

use crate::model::{Instrument, SeriesDisplayFormat, SeriesFormat, percent_change};
use std::sync::Arc;

// Enough decimal places to keep even the smallest positive f64 from rounding to zero.
const MAX_PRECISION: usize = 324;

// Inspect the rounded digits so rounding ties follow Rust's formatter.
pub(crate) fn format_decimal(value: f64, precision: usize) -> String {
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

/// Rounds once to the fraction's step, with halfway values away from zero. Zero has no sign;
/// non-finite values and values whose step count overflows retain Rust's decimal spelling.
fn format_fraction(value: f64, fraction: crate::model::PriceFraction) -> String {
    use crate::model::PriceFraction as F;
    let per_unit = f64::from(fraction.steps_per_unit());
    // Scaling by a power of two is exact until overflow.
    let steps = (value.abs() * per_unit).round();
    if !steps.is_finite() {
        return format_decimal(value, 0);
    }
    let whole = (steps / per_unit).floor();
    let count = (steps - whole * per_unit) as u32;
    let sign = if value < 0.0 && steps > 0.0 { "-" } else { "" };
    let part = match fraction {
        F::Halves | F::Quarters | F::Eighths => count.to_string(),
        F::Sixteenths | F::ThirtySeconds | F::SixtyFourths => format!("{count:02}"),
        F::HalfThirtySeconds | F::QuarterThirtySeconds | F::EighthThirtySeconds => {
            // Steps per thirty-second, written as the first decimal of the part: 2/8 is 2.
            let parts = u32::from(fraction.steps_per_unit()) / 32;
            format!("{:02}{}", count / parts, count % parts * 10 / parts)
        }
        F::OneTwentyEighths | F::TwoFiftySixths => format!("{count:03}"),
    };
    format!("{sign}{whole:.0}'{part}")
}

/// `value` as the whole multiple of `step` that `round` picks from `value / step`, written at the
/// step's decimals.
#[cfg(feature = "widget")]
pub(crate) fn to_step(value: f64, step: f64, round: impl Fn(f64) -> f64) -> f64 {
    let scale = 10f64.powi(decimals(step) as i32);
    (round(value / step) * step * scale).round() / scale
}

/// The price alert and order rows act on: the nearest positive finite tick, else the menu's
/// decimals, with halfway values away from zero. Non-finite prices and decimal counts above
/// the instrument precision cap stay unchanged.
#[cfg(feature = "ui")]
pub(crate) fn snap_price(price: f64, tick: Option<f64>, decimals: usize) -> f64 {
    if !price.is_finite() {
        return price;
    }
    match tick.filter(|tick| tick.is_finite() && *tick > 0.0) {
        Some(tick) => to_step(price, tick, f64::round),
        None if decimals <= MAX_PRICE_DECIMALS => {
            to_step(price, 10f64.powi(-(decimals as i32)), f64::round)
        }
        None => price,
    }
}

/// Decimals needed to write `step`, such as 2 for 0.25.
pub(crate) fn decimals(step: f64) -> usize {
    let rounded = format!("{:.14e}", step.abs())
        .parse::<f64>()
        .unwrap_or(step.abs());
    let text = format!("{rounded}");
    let Some((_, fractional)) = text.split_once('.') else {
        return 0;
    };
    fractional.len()
}

/// Writes `separator` between the groups of three digits of the integer part of `text`, its
/// first run of ASCII digits: `-42541.77` becomes `-42,541.77` with `','`. Text whose integer
/// part has three digits or fewer, text without digits (`NaN`, `inf`) and a `None` separator
/// come back unchanged, without allocating.
pub(crate) fn group_thousands(text: String, separator: Option<char>) -> String {
    let Some(separator) = separator else {
        return text;
    };
    let Some(start) = text.find(|c: char| c.is_ascii_digit()) else {
        return text;
    };
    let digits = text[start..]
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(text.len() - start);
    if digits <= 3 {
        return text;
    }
    let mut grouped = String::with_capacity(text.len() + (digits - 1) / 3 * separator.len_utf8());
    grouped.push_str(&text[..start]);
    for (index, digit) in text[start..start + digits].chars().enumerate() {
        if index > 0 && (digits - index) % 3 == 0 {
            grouped.push(separator);
        }
        grouped.push(digit);
    }
    grouped.push_str(&text[start + digits..]);
    grouped
}

/// The most decimals an instrument's precision gives price labels. An `f64` holds 15 to 17
/// significant digits, so 12 decimals still write prices below 1000 exactly, and a label stays
/// short enough to leave the plot its room.
const MAX_PRICE_DECIMALS: usize = 12;

/// Instrument decimals, falling back to the decimal places needed to write its minimum tick in
/// its shortest form, so a tick computed in floating point, such as `0.1 * 3.0`, has one.
/// Invalid tick sizes are ignored; explicit decimals take precedence. Both are capped at
/// 12 decimals.
pub fn instrument_price_precision(instrument: &Instrument) -> Option<u8> {
    let count = instrument.price_decimals.map(usize::from).or_else(|| {
        let tick = instrument
            .tick_size
            .filter(|tick| tick.is_finite() && *tick > 0.0)?;
        Some(match decimals(tick) {
            // A tick widened from single precision keeps its noise past the cap; its single
            // precision form is the one the host wrote.
            wide if wide > MAX_PRICE_DECIMALS && f64::from(tick as f32) == tick => {
                decimals((tick as f32).to_string().parse().unwrap_or(tick))
            }
            exact => exact,
        })
    })?;
    Some(count.min(MAX_PRICE_DECIMALS) as u8)
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

    /// Format an absolute price change. The default preserves the formatter's text;
    /// chart-owned display formats add the built-in sign for non-negative changes.
    fn format_change(&self, change: f64) -> String {
        self.format(change)
    }

    /// Format a price axis tick. Defaults to [`format`](Self::format); override it
    /// to write ticks differently from crosshair and price tag labels.
    fn format_tick(&self, price: f64) -> String {
        self.format(price)
    }

    /// Format the complete tick list for one axis, in ascending value order.
    /// Values are raw prices in normal and logarithmic modes, percentages or index
    /// values otherwise. Return one label per value, in the same order, to choose
    /// units or precision from the whole list. The default calls
    /// [`format_tick`](Self::format_tick) for each value, and a different label count
    /// falls back to that per-value method for the whole list.
    ///
    /// Layout and painting can each request labels; ticks covered by tags or clipped
    /// at the pane boundary are still included, as are the edge values when edge ticks
    /// are enabled. Layout and painting use the same list. Crosshair and price tags use
    /// [`format`](Self::format) instead.
    fn format_ticks(&self, prices: &[f64]) -> Vec<String> {
        prices
            .iter()
            .map(|&price| self.format_tick(price))
            .collect()
    }

    /// Clone this formatter into a Box
    fn clone_box(&self) -> Box<dyn PriceFormatter>;
}

impl PriceFormatter for SeriesDisplayFormat {
    fn format(&self, value: f64) -> String {
        if let SeriesFormat::Fraction(fraction) = self.kind {
            return format_fraction(value, fraction);
        }
        let step = self.min_move;
        let rounded = (value / step).round() * step;
        let value = if step.is_finite() && step > 0.0 && rounded.is_finite() {
            rounded
        } else {
            value
        };
        let precision = usize::from(self.precision);
        match self.kind {
            // Fractions returned above.
            SeriesFormat::Price | SeriesFormat::Fraction(_) => {
                DefaultPriceFormatter::with_precision(precision).format(value)
            }
            SeriesFormat::Volume => VolumeFormatter { precision }.format(value),
            SeriesFormat::Percentage => format!("{}%", format_decimal(value, precision)),
        }
    }

    fn clone_box(&self) -> Box<dyn PriceFormatter> {
        Box::new(*self)
    }
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
#[non_exhaustive]
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
        // The decimal point is replaced first, so a '.' thousands separator is never confused with it.
        let val_str = group_thousands(
            format_decimal(price, self.precision).replace('.', &self.decimal_sep.to_string()),
            self.thousands_sep.filter(|sep| *sep != self.decimal_sep),
        );

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

impl VolumeFormatter {
    /// The volume as [`format`](PriceFormatter::format) writes it, with K, M and B values at no
    /// fewer than `unit_decimals` decimals. Values below a thousand keep the precision.
    pub(crate) fn format_with_min_unit_decimals(&self, value: f64, unit_decimals: usize) -> String {
        let decimals = |unit: f64| {
            if unit == 1.0 {
                self.precision
            } else {
                self.precision.max(unit_decimals)
            }
        };
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
        let mut formatted = format_decimal(value / unit, decimals(unit));
        if unit < 1_000_000_000.0
            && formatted.trim_start_matches('-').split('.').next() == Some("1000")
        {
            unit *= 1_000.0;
            suffix = match suffix {
                "" => "K",
                "K" => "M",
                _ => "B",
            };
            formatted = format_decimal(value / unit, decimals(unit));
        }
        formatted.push_str(suffix);
        formatted
    }
}

impl PriceFormatter for VolumeFormatter {
    fn format(&self, value: f64) -> String {
        self.format_with_min_unit_decimals(value, 0)
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

impl std::fmt::Debug for CustomPriceFormatter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CustomPriceFormatter")
            .finish_non_exhaustive()
    }
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
#[derive(Debug)]
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
    fn decimals_ignore_binary_noise_and_keep_subnormal_precision() {
        assert_eq!(decimals(0.025 * 3.0), 3);
        assert_eq!(decimals(5e-324), 324);
        assert_eq!(decimals(f64::NAN), 0);
        assert_eq!(decimals(f64::INFINITY), 0);
        // Instrument ticks count the same digits, capped where an f64 stops writing prices.
        let tick =
            |tick: f64| instrument_price_precision(&Instrument::new("T").with_tick_size(tick));
        for (size, expected) in [
            (0.1 * 3.0, 1),
            (0.07 * 3.0, 2),
            (f64::from(0.01f32), 2),
            (0.25, 2),
            (1.0 / 32.0, 5),
            (5.0, 0),
            (1e-7, 7),
            (1e-15, 12),
        ] {
            assert_eq!(tick(size), Some(expected), "tick {size}");
        }
        let explicit = Instrument::new("T").with_price_decimals(255);
        assert_eq!(instrument_price_precision(&explicit), Some(12));
    }

    /// Fractional counts are padded by denominator, half-32nds use 0/5, and rounding happens
    /// once to the fraction, with ties away from zero, carry and no minus on rounded zero.
    #[test]
    fn fractions_write_whole_part_and_padded_count() {
        use crate::model::PriceFraction::*;
        // Counts by hand: .4375 * 16 = 7; .515625 * 64 = 33; .5078125 * 128 = 65 (16 1/4
        // thirty-seconds); .5234375 * 128 = 67 (16 3/4); .51171875 * 256 = 131 (16 3/8);
        // .52734375 * 256 = 135 (16 7/8).
        // 99.984375 * 32 = 3199.5 carries to 3200; 100.0154 * 32 = 3200.4928 stays 3200.
        for (fraction, value, expected) in [
            (Halves, 101.5, "101'1"),
            (Quarters, 101.75, "101'3"),
            (Eighths, 101.625, "101'5"),
            (Sixteenths, 101.4375, "101'07"),
            (ThirtySeconds, 112.5, "112'16"),
            (ThirtySeconds, 0.03125, "0'01"),
            (ThirtySeconds, 100.0154, "100'00"),
            (ThirtySeconds, 99.984375, "100'00"),
            (ThirtySeconds, -99.984375, "-100'00"),
            (ThirtySeconds, -0.001, "0'00"),
            (HalfThirtySeconds, 112.515625, "112'165"),
            (HalfThirtySeconds, 112.5, "112'160"),
            (QuarterThirtySeconds, 112.5078125, "112'162"),
            (QuarterThirtySeconds, 112.5234375, "112'167"),
            (EighthThirtySeconds, 112.51171875, "112'163"),
            (EighthThirtySeconds, 112.52734375, "112'168"),
            (SixtyFourths, 112.515625, "112'33"),
            (OneTwentyEighths, 112.5078125, "112'065"),
            (TwoFiftySixths, 112.51171875, "112'131"),
            (ThirtySeconds, f64::NAN, "NaN"),
            (ThirtySeconds, f64::INFINITY, "inf"),
            (ThirtySeconds, f64::NEG_INFINITY, "-inf"),
        ] {
            let format = SeriesDisplayFormat {
                kind: SeriesFormat::Fraction(fraction),
                ..Default::default()
            };
            assert_eq!(format.format(value), expected, "{fraction:?}: {value}");
        }
    }

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
        for (value, expected) in [(0.0, "0"), (10.0, "10"), (100.0, "100"), (-100.0, "-100")] {
            assert_eq!(
                DefaultPriceFormatter::with_precision(0).format(value),
                expected
            );
        }
    }

    #[test]
    fn test_percentage_formatter() {
        assert_eq!(PercentageFormatter::default().format(105.0), "+5.00%");
        // A spread keeps the sign of the change; a zero base has no defined percentage.
        for (base_val, value, expected) in [
            (100.0, 95.0, "-5.00%"),
            (100.0, 99.999, "+0.00%"),
            (100.0, 99.994, "-0.01%"),
            (-10.0, -5.0, "+50.00%"),
            (-10.0, -15.0, "-50.00%"),
            (0.0, 123.0, "+0.00%"),
        ] {
            let formatter = PercentageFormatter {
                precision: 2,
                base_val,
            };
            assert_eq!(
                formatter.format(value),
                expected,
                "base {base_val}, value {value}"
            );
        }
    }

    #[test]
    fn test_currency_formatter() {
        let usd = CurrencyFormatter::usd();
        assert_eq!(usd.format(1234.56), "$1,234.56");

        for (formatter, value, expected) in [
            (CurrencyFormatter::usd(), 1234567.891, "$1,234,567.89"),
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
        // Precision, the fewest decimals of K, M and B values, value and text.
        for (precision, units, value, expected) in [
            (2, 0, 999.996, "1.00K"),
            (2, 0, 999_999.0, "1.00M"),
            (2, 0, 999_996_000.0, "1.00B"),
            (2, 0, 999.994, "999.99"),
            (2, 0, 999_994.0, "999.99K"),
            (2, 0, 999_994_000.0, "999.99M"),
            (0, 0, 999.6, "1K"),
            (0, 0, 999_600.0, "1M"),
            (3, 0, 999_999.4, "999.999K"),
            (3, 0, 999_999.6, "1.000M"),
            (2, 0, 999_999_999_999.0, "1000.00B"),
            (0, 2, 999.4, "999"),
            (0, 2, 999.6, "1.00K"),
            (0, 2, 1_234.5, "1.23K"),
            (0, 2, 999_996.0, "1.00M"),
            (3, 2, 1_234.5, "1.234K"),
        ] {
            let formatter = VolumeFormatter { precision };
            let text = |value| formatter.format_with_min_unit_decimals(value, units);
            assert_eq!(text(value), expected);
            assert_eq!(text(-value), format!("-{expected}"));
            if units == 0 {
                assert_eq!(formatter.format(value), expected);
            }
        }
    }

    #[test]
    fn test_scientific_formatter() {
        let formatter = ScientificFormatter::default();
        let result = formatter.format(0.000123);
        assert_eq!(result, "1.230e-4");
        assert_eq!(formatter.format(-0.0), "0.000e0");
        assert_eq!(formatter.format(-0.001), "-1.000e-3");
    }

    #[test]
    fn test_formatter_builder() {
        let formatter = PriceFormatterBuilder::new().with_precision(4).build();
        assert_eq!(formatter.format(123.456789), "123.4568");

        let currency_formatter = PriceFormatterBuilder::new()
            .with_currency("$")
            .with_precision(2)
            .build();
        assert_eq!(currency_formatter.format(100.0), "$100.00");
    }
}
