//! Core OHLCV Bar Structure
//!
//! The fundamental data unit for financial charting.

use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use egui::Color32;
use serde::{Deserialize, Deserializer, Serialize};
use std::fmt;
use std::sync::Arc;

/// Percent change from `reference` to `value`, divided by the magnitude of the
/// reference so it has the sign of the change for a negative reference too (spreads,
/// some futures). A zero or non-finite reference or value gives 0.
pub(crate) fn percent_change(reference: f64, value: f64) -> f64 {
    let pct = (value - reference) / reference.abs() * 100.0;
    if pct.is_finite() { pct } else { 0.0 }
}

/// The last bar of `bars` with a value, skipping whitespace bars.
pub(crate) fn last_with_value(bars: &[Bar]) -> Option<&Bar> {
    bars.iter().rev().find(|bar| !bar.is_whitespace())
}

/// Represents a single OHLCV (Open, High, Low, Close, Volume) bar
///
/// Each bar represents a discrete time period of trading activity. A bar can also carry
/// [`colors`](Self::colors) of its own and a host [`payload`](Self::payload), stand for a single
/// value ([`from_value`](Self::from_value)), or hold a slot of the time axis without a value
/// ([`whitespace`](Self::whitespace)).
///
/// # Example
///
/// ```
/// use chrono::Utc;
/// use egui_charts::model::Bar;
///
/// let bar = Bar::new(
///     Utc::now(),
///     100.0, // open
///     105.0, // high
///     98.0,  // low
///     103.0, // close
///     1000.0 // volume
/// );
///
/// assert!(bar.is_bullish()); // close > open
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bar {
    /// Timestamp of the bar. Read from JSON as an RFC 3339 time, or as a date alone
    /// (`"2024-03-15"`), which is midnight UTC of that day, the open of its day bar.
    #[serde(deserialize_with = "time_or_date")]
    pub time: DateTime<Utc>,
    /// Opening price
    #[serde(default = "no_value", deserialize_with = "value_or_null")]
    pub open: f64,
    /// Highest price during the period
    #[serde(default = "no_value", deserialize_with = "value_or_null")]
    pub high: f64,
    /// Lowest price during the period
    #[serde(default = "no_value", deserialize_with = "value_or_null")]
    pub low: f64,
    /// Closing price
    #[serde(default = "no_value", deserialize_with = "value_or_null")]
    pub close: f64,
    /// Trading volume
    #[serde(default = "no_value", deserialize_with = "value_or_null")]
    pub volume: f64,
    /// Colours of this bar that replace the series' colours, `None` for the series' colours.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub colors: Option<Box<BarColors>>,
    /// Data of the host's own attached to this bar. The chart does not read it; it comes back
    /// with the bar in the chart's pointer and data events and in its bar lookups.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<Arc<serde_json::Value>>,
}

/// Colours of one bar that replace the colours of its series for that bar, in
/// [`Bar::colors`]. A colour left `None` keeps the series' colour, and each chart type reads the
/// colours that apply to it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BarColors {
    /// The body of a candle; an OHLC bar, a column or a line's marker; or the line from this bar
    /// to the next on line, step line, area and HLC area charts.
    pub color: Option<Color32>,
    /// The border of a candle's body.
    pub border: Option<Color32>,
    /// The wick of a candle.
    pub wick: Option<Color32>,
    /// A baseline's line above its base value, from this bar to the next.
    pub top_line: Option<Color32>,
    /// A baseline's fill above its base value, from this bar to the next.
    pub top_fill: Option<Color32>,
    /// A baseline's line below its base value, from this bar to the next.
    pub bottom_line: Option<Color32>,
    /// A baseline's fill below its base value, from this bar to the next.
    pub bottom_fill: Option<Color32>,
}

/// A value missing from a JSON bar, which leaves the bar without it.
fn no_value() -> f64 {
    f64::NAN
}

/// A number, or `null`, which JSON writes for a value that is not a number, such as the values
/// of a whitespace bar.
fn value_or_null<'de, D: Deserializer<'de>>(deserializer: D) -> Result<f64, D::Error> {
    Ok(Option::<f64>::deserialize(deserializer)?.unwrap_or(f64::NAN))
}

/// An RFC 3339 time, or a date alone for midnight UTC of that day.
fn time_or_date<'de, D: Deserializer<'de>>(deserializer: D) -> Result<DateTime<Utc>, D::Error> {
    let text = String::deserialize(deserializer)?;
    text.parse::<DateTime<Utc>>()
        .or_else(|error| {
            NaiveDate::parse_from_str(&text, "%Y-%m-%d")
                .map(|date| date.and_time(NaiveTime::MIN).and_utc())
                .map_err(|_| error)
        })
        .map_err(serde::de::Error::custom)
}

impl Bar {
    /// Creates a new bar
    pub fn new(
        time: DateTime<Utc>,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
    ) -> Self {
        Self {
            time,
            open,
            high,
            low,
            close,
            volume,
            colors: None,
            payload: None,
        }
    }

    /// A bar of a single value, such as a point of a line, area, baseline or column series: its
    /// open, high, low and close are `value`, its volume zero.
    pub fn from_value(time: DateTime<Utc>, value: f64) -> Self {
        Self::new(time, value, value, value, value, 0.0)
    }

    /// A bar without a value: it holds its slot on the time axis, so a gap such as a halted
    /// session stays visible and a time of another series has a slot, but nothing is drawn there
    /// and studies take it as a bar without a value. Its open, high, low, close and volume are NaN.
    pub fn whitespace(time: DateTime<Utc>) -> Self {
        Self::new(time, f64::NAN, f64::NAN, f64::NAN, f64::NAN, f64::NAN)
    }

    /// Whether this is a [`whitespace`](Self::whitespace) bar: open, high, low, close and volume
    /// are all NaN.
    pub fn is_whitespace(&self) -> bool {
        [self.open, self.high, self.low, self.close, self.volume]
            .into_iter()
            .all(f64::is_nan)
    }

    // ============= Direction Methods =============

    /// Returns true if this is a bullish bar (close > open)
    #[inline]
    pub fn is_bullish(&self) -> bool {
        self.close > self.open
    }

    /// Returns true if this is a bearish bar (close < open)
    #[inline]
    pub fn is_bearish(&self) -> bool {
        self.close < self.open
    }

    /// Returns true if this is a doji (open ≈ close)
    ///
    /// # Arguments
    /// * `threshold` - Ratio of body to range below which it's considered a doji
    pub fn is_doji(&self, threshold: f64) -> bool {
        let range = self.range();
        if range == 0.0 {
            return true;
        }
        (self.close - self.open).abs() / range < threshold
    }

    // ============= Price Measurements =============

    /// Returns the body height (absolute difference between open and close)
    #[inline]
    pub fn body_height(&self) -> f64 {
        (self.close - self.open).abs()
    }

    /// Returns the total range (high - low)
    #[inline]
    pub fn range(&self) -> f64 {
        self.high - self.low
    }

    /// Returns the upper wick height
    #[inline]
    pub fn upper_wick(&self) -> f64 {
        self.high - self.open.max(self.close)
    }

    /// Returns the lower wick height
    #[inline]
    pub fn lower_wick(&self) -> f64 {
        self.open.min(self.close) - self.low
    }

    // ============= Derived Prices =============

    /// Returns the typical price ((high + low + close) / 3)
    ///
    /// Commonly used in technical analysis (e.g., VWAP, CCI)
    #[inline]
    pub fn typical_price(&self) -> f64 {
        (self.high + self.low + self.close) / 3.0
    }

    /// Returns the weighted close ((high + low + close + close) / 4)
    #[inline]
    pub fn weighted_close(&self) -> f64 {
        (self.high + self.low + self.close * 2.0) / 4.0
    }

    /// Returns the midpoint price ((high + low) / 2)
    #[inline]
    pub fn midpoint(&self) -> f64 {
        (self.high + self.low) / 2.0
    }

    /// Returns the average price (OHLC / 4)
    #[inline]
    pub fn avg_price(&self) -> f64 {
        (self.open + self.high + self.low + self.close) / 4.0
    }

    // ============= Ratios and Percentages =============

    /// Returns the body as a percentage of the total range (0.0 to 1.0)
    pub fn body_percentage(&self) -> f64 {
        let range = self.range();
        if range == 0.0 {
            return 0.0;
        }
        self.body_height() / range
    }

    /// Returns the ratio of upper wick to lower wick
    ///
    /// Returns infinity if lower wick is zero.
    pub fn wick_ratio(&self) -> f64 {
        let lower = self.lower_wick();
        if lower == 0.0 {
            return f64::INFINITY;
        }
        self.upper_wick() / lower
    }

    /// Returns the price change (close - open)
    #[inline]
    pub fn change(&self) -> f64 {
        self.close - self.open
    }

    /// Returns the percentage change ((close - open) / |open| * 100), divided by the
    /// magnitude of the open so it has the sign of the change for a negative open too
    pub fn change_percent(&self) -> f64 {
        percent_change(self.open, self.close)
    }

    // ============= Body Position Helpers =============

    /// Returns the top of the body (max of open and close)
    #[inline]
    pub fn body_top(&self) -> f64 {
        self.open.max(self.close)
    }

    /// Returns the bottom of the body (min of open and close)
    #[inline]
    pub fn body_bottom(&self) -> f64 {
        self.open.min(self.close)
    }
}

impl fmt::Display for Bar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Bar[{}, O:{:.2}, H:{:.2}, L:{:.2}, C:{:.2}, V:{:.2}]",
            self.time.format("%Y-%m-%d %H:%M:%S"),
            self.open,
            self.high,
            self.low,
            self.close,
            self.volume
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_bullish_bar() -> Bar {
        Bar::new(Utc::now(), 100.0, 110.0, 95.0, 105.0, 1000.0)
    }

    fn sample_bearish_bar() -> Bar {
        Bar::new(Utc::now(), 105.0, 110.0, 95.0, 100.0, 1000.0)
    }

    #[test]
    fn bar_shape() {
        for (bar, bullish, bearish) in [
            (sample_bullish_bar(), true, false),
            (sample_bearish_bar(), false, true),
        ] {
            assert_eq!(bar.is_bullish(), bullish);
            assert_eq!(bar.is_bearish(), bearish);
            assert_eq!(bar.body_height(), 5.0);
            assert_eq!(bar.range(), 15.0);
            assert_eq!(bar.upper_wick(), 5.0);
            assert_eq!(bar.lower_wick(), 5.0);
            assert_eq!(bar.body_top(), 105.0);
            assert_eq!(bar.body_bottom(), 100.0);
        }
    }

    #[test]
    fn test_derived_prices() {
        let bar = Bar::new(Utc::now(), 100.0, 120.0, 80.0, 110.0, 1000.0);

        // typical = (120 + 80 + 110) / 3 = 103.33...
        assert!((bar.typical_price() - 103.333).abs() < 0.01);

        // midpoint = (120 + 80) / 2 = 100
        assert_eq!(bar.midpoint(), 100.0);

        // avg = (100 + 120 + 80 + 110) / 4 = 102.5
        assert_eq!(bar.avg_price(), 102.5);
    }

    /// The percentage has the sign of the price change for any reference sign;
    /// a zero or non-finite reference or value gives 0 instead of inf or NaN.
    #[test]
    fn percent_change_follows_the_sign_of_the_change() {
        let rows = [
            // (reference, value, expected)
            (100.0, 105.0, 5.0),
            (10.0, 5.0, -50.0),
            (-10.0, -5.0, 50.0),
            (-5.0, -10.0, -100.0),
            (0.0, 5.0, 0.0),
            (f64::NAN, 5.0, 0.0),
            (f64::INFINITY, 5.0, 0.0),
            (5.0, f64::NAN, 0.0),
        ];
        for (reference, value, expected) in rows {
            assert_eq!(
                percent_change(reference, value),
                expected,
                "{reference} -> {value}"
            );
        }
        let bar = Bar::new(Utc::now(), -10.0, -4.0, -11.0, -5.0, 1000.0);
        assert_eq!(bar.change_percent(), 50.0);
    }
}
