//! Aroon Indicator
//!
//! Aroon identifies trend changes and trend strength by measuring the time
//! since the highest high and lowest low within a lookback period.
//!
//! # Components
//! - Aroon Up: ((period - days since highest high) / period) * 100
//! - Aroon Down: ((period - days since lowest low) / period) * 100
//! - Aroon Oscillator: Aroon Up - Aroon Down (optional)
//!
//! # Interpretation
//! - Aroon Up > 70 and Aroon Down < 30: Strong uptrend
//! - Aroon Down > 70 and Aroon Up < 30: Strong downtrend
//! - Crossovers indicate potential trend changes
//!
//! # Example
//! ```ignore
//! use egui_charts::Aroon;
//!
//! let mut aroon = Aroon::new(25);
//! aroon.calculate(&bars);
//! ```

use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Aroon indicator
#[derive(Clone)]
pub struct Aroon {
    period: usize,
    values: Vec<IndicatorValue>,
    up_color: Color32,
    down_color: Color32,
    visible: bool,
}

impl Aroon {
    /// Create a new Aroon indicator
    ///
    /// # Arguments
    /// * `period` - Lookback period (typically 25)
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            values: Vec::new(),
            up_color: DESIGN_TOKENS.semantic.extended.success,
            down_color: DESIGN_TOKENS.semantic.extended.error,
            visible: true,
        }
    }

    /// Create with default param (25)
    pub fn default_params() -> Self {
        Self::new(25)
    }

    /// Set colors for up and down lines
    pub fn with_colors(mut self, up: Color32, down: Color32) -> Self {
        self.up_color = up;
        self.down_color = down;
        self
    }
}

impl Indicator for Aroon {
    fn name(&self) -> &str {
        "Aroon"
    }

    fn desc(&self) -> &str {
        "Aroon - Trend identification indicator"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.is_empty() {
            return;
        }

        for i in 0..data.len() {
            if i < self.period {
                self.values.push(IndicatorValue::None);
                continue;
            }

            // Find highest high and lowest low positions in lookback
            let start = i - self.period;
            let mut highest_idx = start;
            let mut lowest_idx = start;
            let mut highest = data[start].high;
            let mut lowest = data[start].low;

            for j in (start + 1)..=i {
                if data[j].high >= highest {
                    highest = data[j].high;
                    highest_idx = j;
                }
                if data[j].low <= lowest {
                    lowest = data[j].low;
                    lowest_idx = j;
                }
            }

            // Days since highest/lowest
            let days_since_high = i - highest_idx;
            let days_since_low = i - lowest_idx;

            // Calculate Aroon Up and Down (0-100 scale)
            let aroon_up = ((self.period - days_since_high) as f64 / self.period as f64) * 100.0;
            let aroon_down = ((self.period - days_since_low) as f64 / self.period as f64) * 100.0;

            self.values
                .push(IndicatorValue::Multiple(vec![aroon_up, aroon_down]));
        }
    }

    fn values(&self) -> &[IndicatorValue] {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        vec![self.up_color, self.down_color]
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if !colors.is_empty() {
            self.up_color = colors[0];
        }
        if colors.len() > 1 {
            self.down_color = colors[1];
        }
    }

    fn is_overlay(&self) -> bool {
        false // Aroon is a separate oscillator pane
    }

    fn line_cnt(&self) -> usize {
        2 // Aroon Up and Aroon Down
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
        vec![
            format!("Aroon Up({})", self.period),
            format!("Aroon Down({})", self.period),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};

    fn create_uptrending_bars() -> Vec<Bar> {
        let start = Utc::now();
        (0..50)
            .map(|i| {
                let price = 100.0 + i as f64 * 0.5;
                Bar {
                    time: start + Duration::minutes(i * 5),
                    open: price - 0.2,
                    high: price + 1.0,
                    low: price - 1.0,
                    close: price,
                    volume: 1000.0,
                }
            })
            .collect()
    }

    fn create_downtrending_bars() -> Vec<Bar> {
        let start = Utc::now();
        (0..50)
            .map(|i| {
                let price = 150.0 - i as f64 * 0.5;
                Bar {
                    time: start + Duration::minutes(i * 5),
                    open: price + 0.2,
                    high: price + 1.0,
                    low: price - 1.0,
                    close: price,
                    volume: 1000.0,
                }
            })
            .collect()
    }

    #[test]
    fn test_aroon_range() {
        let bars = create_uptrending_bars();
        let mut aroon = Aroon::new(25);
        aroon.calculate(&bars);

        for value in aroon.values() {
            if let IndicatorValue::Multiple(vals) = value {
                assert!(
                    vals[0] >= 0.0 && vals[0] <= 100.0,
                    "Aroon Up should be 0-100"
                );
                assert!(
                    vals[1] >= 0.0 && vals[1] <= 100.0,
                    "Aroon Down should be 0-100"
                );
            }
        }
    }

    #[test]
    fn test_aroon_uptrend() {
        let bars = create_uptrending_bars();
        let mut aroon = Aroon::new(25);
        aroon.calculate(&bars);

        // In uptrend, Aroon Up should be higher than Aroon Down
        if let Some(IndicatorValue::Multiple(vals)) = aroon.values().last() {
            assert!(
                vals[0] > vals[1],
                "In uptrend, Aroon Up ({}) should be > Aroon Down ({})",
                vals[0],
                vals[1]
            );
        }
    }

    #[test]
    fn test_aroon_downtrend() {
        let bars = create_downtrending_bars();
        let mut aroon = Aroon::new(25);
        aroon.calculate(&bars);

        // In downtrend, Aroon Down should be higher than Aroon Up
        if let Some(IndicatorValue::Multiple(vals)) = aroon.values().last() {
            assert!(
                vals[1] > vals[0],
                "In downtrend, Aroon Down ({}) should be > Aroon Up ({})",
                vals[1],
                vals[0]
            );
        }
    }

    #[test]
    fn test_aroon_is_not_overlay() {
        let aroon = Aroon::new(25);
        assert!(!aroon.is_overlay());
    }

    #[test]
    fn test_aroon_line_cnt() {
        let aroon = Aroon::new(25);
        assert_eq!(aroon.line_cnt(), 2);
    }

    #[test]
    fn test_aroon_empty_data() {
        let mut aroon = Aroon::new(25);
        aroon.calculate(&[]);
        assert!(aroon.values().is_empty());
    }
}
