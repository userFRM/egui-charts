//! # Cumulative Volume Delta (CVD) Indicator
//!
//! CVD tracks the running sum of buying volume minus selling volume.
//! Useful for identifying divergences between price and order flow.

use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue};
use egui::Color32;

/// Cumulative Volume Delta (CVD)
///
/// Shows the running total of (buy volume - sell volume).
/// Volume is estimated using bar internals when tick data unavailable.
#[derive(Clone)]
pub struct CVD {
    /// Values calculated from bars
    values: Vec<IndicatorValue>,
    /// Line color
    color: Color32,
    /// Visibility flag
    visible: bool,
    /// CVD calculation mode
    mode: CVDMode,
    /// Show histogram alongside line
    show_histogram: bool,
    /// Reset at session start
    reset_at_session: bool,
}

/// CVD calculation mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CVDMode {
    /// Estimate based on bar close position within range
    #[default]
    ClosePosition,
    /// Estimate based on body ratio
    BodyRatio,
    /// Use tick volume split (requires footprint data)
    TickSplit,
    /// Bullish candles = buy, bearish = sell
    Simple,
}

impl CVD {
    /// Create a new CVD indicator
    pub fn new() -> Self {
        Self {
            values: Vec::new(),
            color: Color32::from_rgb(76, 175, 80), // Green for volume/delta
            visible: true,
            mode: CVDMode::default(),
            show_histogram: false,
            reset_at_session: false,
        }
    }

    /// Set the CVD calculation mode
    pub fn with_mode(mut self, mode: CVDMode) -> Self {
        self.mode = mode;
        self
    }

    /// Show histogram
    pub fn with_histogram(mut self, show: bool) -> Self {
        self.show_histogram = show;
        self
    }

    /// Set color
    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }

    /// Estimate delta volume for a bar
    fn estimate_delta(&self, bar: &Bar) -> f64 {
        let range = bar.high - bar.low;
        if range <= 0.0 {
            // Doji - no delta
            return 0.0;
        }

        match self.mode {
            CVDMode::ClosePosition => {
                // Delta based on close position within bar range
                // Close at high = 100% buy, at low = 100% sell
                let close_pos = (bar.close - bar.low) / range;
                let buy_pct = close_pos;
                let sell_pct = 1.0 - close_pos;
                bar.volume * (buy_pct - sell_pct)
            }
            CVDMode::BodyRatio => {
                // Delta based on body size and direction
                let body = (bar.close - bar.open).abs();
                let body_ratio = body / range;
                let direction = if bar.close >= bar.open { 1.0 } else { -1.0 };
                bar.volume * body_ratio * direction
            }
            CVDMode::TickSplit => {
                // Would need footprint data - fallback to close position
                let close_pos = (bar.close - bar.low) / range;
                bar.volume * (2.0 * close_pos - 1.0)
            }
            CVDMode::Simple => {
                // Bullish = buy, bearish = sell
                if bar.close >= bar.open {
                    bar.volume
                } else {
                    -bar.volume
                }
            }
        }
    }
}

impl Default for CVD {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for CVD {
    fn name(&self) -> &str {
        "CVD"
    }

    fn desc(&self) -> &str {
        "Cumulative Volume Delta - Running sum of buy/sell volume difference"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.is_empty() {
            return;
        }

        let mut cumulative_delta = 0.0;
        let mut last_session_date = data[0].time.date_naive();

        for bar in data {
            // Check for session reset
            if self.reset_at_session {
                let current_date = bar.time.date_naive();
                if current_date != last_session_date {
                    cumulative_delta = 0.0;
                    last_session_date = current_date;
                }
            }

            let delta = self.estimate_delta(bar);
            cumulative_delta += delta;

            self.values.push(IndicatorValue::Single(cumulative_delta));
        }
    }

    fn values(&self) -> &[IndicatorValue] {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        vec![self.color]
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if !colors.is_empty() {
            self.color = colors[0];
        }
    }

    fn is_overlay(&self) -> bool {
        false // CVD is drawn in separate pane
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
        vec!["CVD".to_string()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn create_test_bars() -> Vec<Bar> {
        let ts = Utc::now();
        vec![
            Bar {
                time: ts,
                open: 100.0,
                high: 102.0,
                low: 99.0,
                close: 101.5, // Bullish, close near high
                volume: 1000.0,
            },
            Bar {
                time: ts,
                open: 101.5,
                high: 103.0,
                low: 100.0,
                close: 100.5, // Bearish, close near low
                volume: 1000.0,
            },
            Bar {
                time: ts,
                open: 100.5,
                high: 102.0,
                low: 99.0,
                close: 101.8, // Bullish
                volume: 1500.0,
            },
        ]
    }

    #[test]
    fn test_cvd_calculation() {
        let mut cvd = CVD::new();
        let bars = create_test_bars();

        cvd.calculate(&bars);

        assert_eq!(cvd.values.len(), 3);

        // First bar is bullish (close near high) - positive delta
        if let IndicatorValue::Single(val) = cvd.values[0] {
            assert!(val > 0.0, "First bar should have positive delta");
        }

        // After second bar (bearish), delta should decrease
        if let (IndicatorValue::Single(v1), IndicatorValue::Single(v2)) =
            (&cvd.values[0], &cvd.values[1])
        {
            assert!(v2 < v1, "Second bar should decrease cumulative delta");
        }
    }

    #[test]
    fn test_cvd_simple_mode() {
        let mut cvd = CVD::new().with_mode(CVDMode::Simple);
        let bars = create_test_bars();

        cvd.calculate(&bars);

        // Simple mode: first bar bullish = +1000, second bearish = -1000
        if let IndicatorValue::Single(val) = cvd.values[1] {
            // First is +1000 (bullish), second is -1000 (bearish) = 0
            assert!(
                val.abs() < 1.0,
                "After one bullish and one bearish, delta should be near 0"
            );
        }
    }
}
