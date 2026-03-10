//! SuperTrend Indicator
//!
//! SuperTrend is a trend-following indicator that uses ATR to calculate
//! dynamic support and resistance levels. It's useful for identifying
//! trend direction and potential entry/exit points.
//!
//! # Components
//! - SuperTrend Line: Dynamic support/resistance level
//! - Trend Direction: Bullish (below price) or Bearish (above price)
//!
//! # Interpretation
//! - SuperTrend below price (green): Bullish trend
//! - SuperTrend above price (red): Bearish trend
//! - Price crossing SuperTrend signals trend change
//!
//! # Example
//! ```ignore
//! use egui_charts::SuperTrend;
//!
//! let mut st = SuperTrend::new(10, 3.0);
//! st.calculate(&bars);
//! ```

use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// SuperTrend indicator
#[derive(Clone)]
pub struct SuperTrend {
    atr_period: usize,
    multiplier: f64,
    values: Vec<IndicatorValue>,
    bullish_color: Color32,
    bearish_color: Color32,
    visible: bool,
}

impl SuperTrend {
    /// Create a new SuperTrend indicator
    ///
    /// # Arguments
    /// * `atr_period` - ATR period (typically 10)
    /// * `multiplier` - ATR multiplier (typically 3.0)
    pub fn new(atr_period: usize, multiplier: f64) -> Self {
        Self {
            atr_period: atr_period.max(1),
            multiplier,
            values: Vec::new(),
            bullish_color: DESIGN_TOKENS.semantic.extended.success, // Green
            bearish_color: DESIGN_TOKENS.semantic.extended.error,   // Red
            visible: true,
        }
    }

    /// Create with default params (10, 3.0)
    pub fn default_params() -> Self {
        Self::new(10, 3.0)
    }

    /// Set colors for bullish and bearish trends
    pub fn with_colors(mut self, bullish: Color32, bearish: Color32) -> Self {
        self.bullish_color = bullish;
        self.bearish_color = bearish;
        self
    }

    /// Calculate True Range
    #[inline]
    fn true_range(bar: &Bar, prev_close: f64) -> f64 {
        let hl = bar.high - bar.low;
        let hc = (bar.high - prev_close).abs();
        let lc = (bar.low - prev_close).abs();
        hl.max(hc).max(lc)
    }
}

impl Indicator for SuperTrend {
    fn name(&self) -> &str {
        "SuperTrend"
    }

    fn desc(&self) -> &str {
        "SuperTrend - Trend following indicator"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.len() < self.atr_period + 1 {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
        }

        // Calculate ATR
        let mut tr = Vec::with_capacity(data.len());
        tr.push(data[0].high - data[0].low);

        for i in 1..data.len() {
            tr.push(Self::true_range(&data[i], data[i - 1].close));
        }

        // Calculate ATR using Wilder's smoothing
        let mut atr = vec![0.0; data.len()];
        let atr_mult = 1.0 / self.atr_period as f64;

        // Initial ATR is simple avg
        atr[self.atr_period - 1] =
            tr[..self.atr_period].iter().sum::<f64>() / self.atr_period as f64;

        for i in self.atr_period..data.len() {
            atr[i] = atr[i - 1] + atr_mult * (tr[i] - atr[i - 1]);
        }

        // Calculate basic upper and lower bands
        let mut basic_upper = vec![0.0; data.len()];
        let mut basic_lower = vec![0.0; data.len()];

        for i in 0..data.len() {
            let hl2 = (data[i].high + data[i].low) / 2.0;
            basic_upper[i] = hl2 + self.multiplier * atr[i];
            basic_lower[i] = hl2 - self.multiplier * atr[i];
        }

        // Calculate final upper and lower bands
        let mut final_upper = vec![0.0; data.len()];
        let mut final_lower = vec![0.0; data.len()];
        let mut supertrend = vec![0.0; data.len()];
        let mut direction = vec![1i32; data.len()]; // 1 = bullish, -1 = bearish

        // Initialize
        final_upper[self.atr_period - 1] = basic_upper[self.atr_period - 1];
        final_lower[self.atr_period - 1] = basic_lower[self.atr_period - 1];
        supertrend[self.atr_period - 1] = final_lower[self.atr_period - 1];

        for i in self.atr_period..data.len() {
            // Final Upper Band
            if basic_upper[i] < final_upper[i - 1] || data[i - 1].close > final_upper[i - 1] {
                final_upper[i] = basic_upper[i];
            } else {
                final_upper[i] = final_upper[i - 1];
            }

            // Final Lower Band
            if basic_lower[i] > final_lower[i - 1] || data[i - 1].close < final_lower[i - 1] {
                final_lower[i] = basic_lower[i];
            } else {
                final_lower[i] = final_lower[i - 1];
            }

            // Determine SuperTrend and direction
            if supertrend[i - 1] == final_upper[i - 1] {
                // Previous was bearish
                if data[i].close > final_upper[i] {
                    supertrend[i] = final_lower[i];
                    direction[i] = 1; // Bullish
                } else {
                    supertrend[i] = final_upper[i];
                    direction[i] = -1; // Bearish
                }
            } else {
                // Previous was bullish
                if data[i].close < final_lower[i] {
                    supertrend[i] = final_upper[i];
                    direction[i] = -1; // Bearish
                } else {
                    supertrend[i] = final_lower[i];
                    direction[i] = 1; // Bullish
                }
            }
        }

        // Build output values: [supertrend, direction]
        for i in 0..data.len() {
            if i < self.atr_period - 1 {
                self.values.push(IndicatorValue::None);
            } else {
                // direction: 1.0 for bullish, -1.0 for bearish
                self.values.push(IndicatorValue::Multiple(vec![
                    supertrend[i],
                    direction[i] as f64,
                ]));
            }
        }
    }

    fn values(&self) -> &[IndicatorValue] {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        vec![self.bullish_color, self.bearish_color]
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if !colors.is_empty() {
            self.bullish_color = colors[0];
        }
        if colors.len() > 1 {
            self.bearish_color = colors[1];
        }
    }

    fn is_overlay(&self) -> bool {
        true // SuperTrend is drawn on main chart
    }

    fn line_cnt(&self) -> usize {
        1 // SuperTrend line (color changes with direction)
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

    fn line_names(&self) -> Vec<String> {
        vec![format!(
            "SuperTrend({}, {})",
            self.atr_period, self.multiplier
        )]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};

    fn create_trending_bars(count: usize, uptrend: bool) -> Vec<Bar> {
        let start = Utc::now();
        let mut price = 100.0;
        let direction = if uptrend { 1.0 } else { -1.0 };

        (0..count)
            .map(|i| {
                price += direction * 0.5;
                Bar {
                    time: start + Duration::minutes(i as i64 * 5),
                    open: price - direction * 0.2,
                    high: price + 1.0,
                    low: price - 1.0,
                    close: price,
                    volume: 1000.0,
                }
            })
            .collect()
    }

    #[test]
    fn test_supertrend_uptrend() {
        let bars = create_trending_bars(50, true);
        let mut st = SuperTrend::new(10, 3.0);
        st.calculate(&bars);

        // In uptrend, SuperTrend should mostly be below price (bullish)
        let mut bullish_cnt = 0;
        let mut total = 0;

        for (i, value) in st.values().iter().enumerate() {
            if let IndicatorValue::Multiple(vals) = value {
                total += 1;
                if vals[1] > 0.0 {
                    // Bullish direction
                    bullish_cnt += 1;
                }
                // SuperTrend should be below close price when bullish
                if vals[1] > 0.0 {
                    assert!(
                        vals[0] <= bars[i].close,
                        "In bullish trend, SuperTrend should be <= close"
                    );
                }
            }
        }

        assert!(
            bullish_cnt as f64 / total as f64 > 0.5,
            "Should be mostly bullish in uptrend"
        );
    }

    #[test]
    fn test_supertrend_is_overlay() {
        let st = SuperTrend::new(10, 3.0);
        assert!(st.is_overlay());
    }

    #[test]
    fn test_supertrend_empty_data() {
        let mut st = SuperTrend::new(10, 3.0);
        st.calculate(&[]);
        assert!(st.values().is_empty());
    }
}
