//! Volume Weighted MACD (VW-MACD) indicator.
//!
//! A variation of MACD that uses volume-weighted prices instead of raw close
//! prices, giving more weight to price movements backed by higher volume.
//! This can reduce false signals during low-volume price moves.
//!
//! # Components
//!
//! Same as regular MACD (MACD line, signal line, histogram), but the input
//! prices are cumulative volume-weighted average prices (typical price
//! weighted by volume).
//!
//! # Default parameters
//!
//! `VolumeWeightedMACD::new(12, 26, 9)` -- same periods as classic MACD.
//!
//! # Example
//!
//! ```rust,ignore
//! use egui_charts::studies::{VolumeWeightedMACD, Indicator};
//!
//! let mut vw = VolumeWeightedMACD::new(12, 26, 9);
//! vw.calculate(&bars);
//! ```

use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Volume Weighted MACD indicator
///
/// Similar to regular MACD but applies volume weighting to price data
/// before calculating the moving averages.
#[derive(Clone)]
pub struct VolumeWeightedMACD {
    fast_period: usize,
    slow_period: usize,
    signal_period: usize,
    values: Vec<IndicatorValue>,
    colors: Vec<Color32>,
    visible: bool,
}

impl Default for VolumeWeightedMACD {
    fn default() -> Self {
        Self::new(12, 26, 9)
    }
}

impl VolumeWeightedMACD {
    /// Create a new VW-MACD with specified periods
    pub fn new(fast_period: usize, slow_period: usize, signal_period: usize) -> Self {
        Self {
            fast_period,
            slow_period,
            signal_period,
            values: Vec::new(),
            colors: vec![
                DESIGN_TOKENS.semantic.indicators.macd_line, // MACD line
                DESIGN_TOKENS.semantic.indicators.macd_signal, // Signal line
                DESIGN_TOKENS.semantic.extended.success,     // Histogram positive
            ],
            visible: true,
        }
    }

    /// Calculate volume-weighted price for a bar
    fn volume_weighted_price(bar: &Bar) -> f64 {
        // Typical price weighted by volume
        // Using (H+L+C)/3 as typical price
        let typical_price = (bar.high + bar.low + bar.close) / 3.0;
        typical_price * bar.volume
    }

    /// Calculate cumulative volume-weighted average price
    fn calculate_vwap_series(data: &[Bar]) -> Vec<f64> {
        let mut cum_volume = 0.0;
        let mut cum_vw_price = 0.0;
        let mut vwap = Vec::with_capacity(data.len());

        for bar in data {
            cum_vw_price += Self::volume_weighted_price(bar);
            cum_volume += bar.volume;

            if cum_volume > 0.0 {
                vwap.push(cum_vw_price / cum_volume);
            } else {
                vwap.push(bar.close);
            }
        }

        vwap
    }
}

impl Indicator for VolumeWeightedMACD {
    fn name(&self) -> &str {
        "VW-MACD"
    }

    fn desc(&self) -> &str {
        "Volume Weighted MACD - MACD using volume-weighted prices"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.len() < self.slow_period {
            return;
        }

        // Use volume-weighted prices instead of raw close
        let vw_prices = Self::calculate_vwap_series(data);

        // Calculate fast EMA on volume-weighted prices
        let fast_mult = 2.0 / (self.fast_period as f64 + 1.0);
        let mut fast_ema = vw_prices[0];
        let mut fast_emas = vec![fast_ema];

        for &price in vw_prices.iter().skip(1) {
            fast_ema = (price - fast_ema) * fast_mult + fast_ema;
            fast_emas.push(fast_ema);
        }

        // Calculate slow EMA on volume-weighted prices
        let slow_mult = 2.0 / (self.slow_period as f64 + 1.0);
        let mut slow_ema = vw_prices[0];
        let mut slow_emas = vec![slow_ema];

        for &price in vw_prices.iter().skip(1) {
            slow_ema = (price - slow_ema) * slow_mult + slow_ema;
            slow_emas.push(slow_ema);
        }

        // Calculate MACD line
        let macd_line: Vec<f64> = fast_emas
            .iter()
            .zip(slow_emas.iter())
            .map(|(fast, slow)| fast - slow)
            .collect();

        // Calculate signal line (EMA of MACD)
        if macd_line.len() < self.signal_period {
            return;
        }

        let signal_mult = 2.0 / (self.signal_period as f64 + 1.0);
        let mut signal_ema = macd_line[0];
        let mut signal_line = vec![signal_ema];

        for &macd_val in macd_line.iter().skip(1) {
            signal_ema = (macd_val - signal_ema) * signal_mult + signal_ema;
            signal_line.push(signal_ema);
        }

        // Calculate histogram and store values
        for i in 0..macd_line.len() {
            let histogram = macd_line[i] - signal_line[i];
            self.values.push(IndicatorValue::Multiple(vec![
                macd_line[i],
                signal_line[i],
                histogram,
            ]));
        }
    }

    fn values(&self) -> &[IndicatorValue] {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        self.colors.clone()
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if colors.len() >= 3 {
            self.colors = colors;
        } else if !colors.is_empty() {
            self.colors = vec![colors[0], colors[0], colors[0]];
        }
    }

    fn is_overlay(&self) -> bool {
        false // VW-MACD is drawn in separate pane
    }

    fn line_cnt(&self) -> usize {
        3
    }

    fn line_names(&self) -> Vec<String> {
        vec![
            format!(
                "VW-MACD({},{},{})",
                self.fast_period, self.slow_period, self.signal_period
            ),
            "Signal".to_string(),
            "Histogram".to_string(),
        ]
    }

    fn is_visible(&self) -> bool {
        self.visible
    }

    fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    fn clone_box(&self) -> Box<dyn Indicator> {
        Box::new(self.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    fn make_test_bars() -> Vec<Bar> {
        vec![
            Bar {
                time: Utc.with_ymd_and_hms(2024, 1, 1, 9, 30, 0).unwrap(),
                open: 100.0,
                high: 105.0,
                low: 98.0,
                close: 102.0,
                volume: 1000.0,
            },
            Bar {
                time: Utc.with_ymd_and_hms(2024, 1, 1, 9, 31, 0).unwrap(),
                open: 102.0,
                high: 108.0,
                low: 101.0,
                close: 107.0,
                volume: 1500.0,
            },
            Bar {
                time: Utc.with_ymd_and_hms(2024, 1, 1, 9, 32, 0).unwrap(),
                open: 107.0,
                high: 110.0,
                low: 105.0,
                close: 108.0,
                volume: 1200.0,
            },
            Bar {
                time: Utc.with_ymd_and_hms(2024, 1, 1, 9, 33, 0).unwrap(),
                open: 108.0,
                high: 112.0,
                low: 106.0,
                close: 110.0,
                volume: 2000.0,
            },
            Bar {
                time: Utc.with_ymd_and_hms(2024, 1, 1, 9, 34, 0).unwrap(),
                open: 110.0,
                high: 115.0,
                low: 109.0,
                close: 114.0,
                volume: 1800.0,
            },
            Bar {
                time: Utc.with_ymd_and_hms(2024, 1, 1, 9, 35, 0).unwrap(),
                open: 114.0,
                high: 116.0,
                low: 111.0,
                close: 112.0,
                volume: 1400.0,
            },
            Bar {
                time: Utc.with_ymd_and_hms(2024, 1, 1, 9, 36, 0).unwrap(),
                open: 112.0,
                high: 118.0,
                low: 110.0,
                close: 117.0,
                volume: 2500.0,
            },
            Bar {
                time: Utc.with_ymd_and_hms(2024, 1, 1, 9, 37, 0).unwrap(),
                open: 117.0,
                high: 120.0,
                low: 115.0,
                close: 119.0,
                volume: 1600.0,
            },
            Bar {
                time: Utc.with_ymd_and_hms(2024, 1, 1, 9, 38, 0).unwrap(),
                open: 119.0,
                high: 122.0,
                low: 118.0,
                close: 121.0,
                volume: 1300.0,
            },
            Bar {
                time: Utc.with_ymd_and_hms(2024, 1, 1, 9, 39, 0).unwrap(),
                open: 121.0,
                high: 125.0,
                low: 120.0,
                close: 124.0,
                volume: 1700.0,
            },
        ]
    }

    #[test]
    fn test_vw_macd_creation() {
        let vw_macd = VolumeWeightedMACD::new(3, 5, 3);
        assert_eq!(vw_macd.name(), "VW-MACD");
        assert_eq!(vw_macd.line_cnt(), 3);
        assert!(!vw_macd.is_overlay());
    }

    #[test]
    fn test_vw_macd_calculate() {
        let mut vw_macd = VolumeWeightedMACD::new(3, 5, 3);
        let bars = make_test_bars();

        vw_macd.calculate(&bars);

        // Should have values after calculation
        assert!(!vw_macd.values().is_empty());
    }

    #[test]
    fn test_vw_macd_insufficient_data() {
        let mut vw_macd = VolumeWeightedMACD::new(12, 26, 9);
        let bars = make_test_bars(); // Only 10 bars, need 26 for slow period

        vw_macd.calculate(&bars);

        // Should have no values with insufficient data
        assert!(vw_macd.values().is_empty());
    }

    #[test]
    fn test_vw_macd_line_names() {
        let vw_macd = VolumeWeightedMACD::new(12, 26, 9);
        let names = vw_macd.line_names();

        assert_eq!(names.len(), 3);
        assert!(names[0].contains("VW-MACD"));
        assert_eq!(names[1], "Signal");
        assert_eq!(names[2], "Histogram");
    }

    #[test]
    fn test_vwap_series_calculation() {
        let bars = make_test_bars();
        let vwap = VolumeWeightedMACD::calculate_vwap_series(&bars);

        assert_eq!(vwap.len(), bars.len());
        // First value should be typical price (since only one bar)
        let first_typical = (bars[0].high + bars[0].low + bars[0].close) / 3.0;
        assert!((vwap[0] - first_typical).abs() < 0.001);
    }

    #[test]
    fn test_visibility() {
        let mut vw_macd = VolumeWeightedMACD::default();
        assert!(vw_macd.is_visible());

        vw_macd.set_visible(false);
        assert!(!vw_macd.is_visible());
    }
}
