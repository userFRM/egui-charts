//! Core OHLCV Bar Structure
//!
//! The fundamental data unit for financial charting.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Represents a single OHLCV (Open, High, Low, Close, Volume) bar
///
/// Each bar represents a discrete time period of trading activity.
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bar {
    /// Timestamp of the bar
    pub time: DateTime<Utc>,
    /// Opening price
    pub open: f64,
    /// Highest price during the period
    pub high: f64,
    /// Lowest price during the period
    pub low: f64,
    /// Closing price
    pub close: f64,
    /// Trading volume
    pub volume: f64,
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
        }
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

    /// Returns the percentage change ((close - open) / open * 100)
    pub fn change_percent(&self) -> f64 {
        if self.open == 0.0 {
            return 0.0;
        }
        (self.close - self.open) / self.open * 100.0
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
    fn test_bullish_bearish() {
        let bullish = sample_bullish_bar();
        let bearish = sample_bearish_bar();

        assert!(bullish.is_bullish());
        assert!(!bullish.is_bearish());
        assert!(bearish.is_bearish());
        assert!(!bearish.is_bullish());
    }

    #[test]
    fn test_measurements() {
        let bar = sample_bullish_bar();

        assert_eq!(bar.body_height(), 5.0); // |105 - 100|
        assert_eq!(bar.range(), 15.0); // 110 - 95
        assert_eq!(bar.upper_wick(), 5.0); // 110 - 105
        assert_eq!(bar.lower_wick(), 5.0); // 100 - 95
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

    #[test]
    fn test_change_percent() {
        let bar = Bar::new(Utc::now(), 100.0, 110.0, 95.0, 105.0, 1000.0);
        assert_eq!(bar.change_percent(), 5.0); // (105-100)/100 * 100
    }

    #[test]
    fn test_body_positions() {
        let bullish = sample_bullish_bar();
        assert_eq!(bullish.body_top(), 105.0);
        assert_eq!(bullish.body_bottom(), 100.0);

        let bearish = sample_bearish_bar();
        assert_eq!(bearish.body_top(), 105.0);
        assert_eq!(bearish.body_bottom(), 100.0);
    }
}
