//! Ultimate Oscillator (UO) Indicator
//!
//! Developed by Larry Williams, the Ultimate Oscillator combines short, medium,
//! and long-term price action into a single oscillator, reducing false signals.
//!
//! # Formula
//! BP (Buying Pressure) = Close - Min(Low, Previous Close)
//! TR (True Range) = Max(High, Previous Close) - Min(Low, Previous Close)
//! Avg = (4 * Avg7) + (2 * Avg14) + Avg28
//! UO = 100 * Avg / 7
//!
//! # Interpretation
//! - Above 70: Overbought
//! - Below 30: Oversold
//! - Bullish divergence: Price makes lower low, UO makes higher low
//!
//! # Example
//! ```ignore
//! use egui_charts::UltimateOscillator;
//!
//! let mut uo = UltimateOscillator::new(7, 14, 28);
//! uo.calculate(&bars);
//! ```

use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Ultimate Oscillator indicator
#[derive(Clone)]
pub struct UltimateOscillator {
    period1: usize, // Short (typically 7)
    period2: usize, // Medium (typically 14)
    period3: usize, // Long (typically 28)
    values: Vec<IndicatorValue>,
    color: Color32,
    visible: bool,
}

impl UltimateOscillator {
    /// Create a new Ultimate Oscillator
    ///
    /// # Arguments
    /// * `period1` - Short period (typically 7)
    /// * `period2` - Medium period (typically 14)
    /// * `period3` - Long period (typically 28)
    pub fn new(period1: usize, period2: usize, period3: usize) -> Self {
        Self {
            period1: period1.max(1),
            period2: period2.max(1),
            period3: period3.max(1),
            values: Vec::new(),
            color: DESIGN_TOKENS.semantic.extended.purple,
            visible: true,
        }
    }

    /// Create with default params (7, 14, 28)
    pub fn default_params() -> Self {
        Self::new(7, 14, 28)
    }

    /// Set the line color
    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }
}

impl Indicator for UltimateOscillator {
    fn name(&self) -> &str {
        "UO"
    }

    fn desc(&self) -> &str {
        "Ultimate Oscillator - Multi-timeframe momentum indicator"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.is_empty() {
            return;
        }

        let max_period = self.period1.max(self.period2).max(self.period3);
        if data.len() < max_period + 1 {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
        }

        // Calculate BP (Buying Pressure) and TR (True Range) for each bar
        let mut bp = vec![0.0; data.len()];
        let mut tr = vec![0.0; data.len()];

        for i in 1..data.len() {
            let prev_close = data[i - 1].close;
            let low_min = data[i].low.min(prev_close);
            let high_max = data[i].high.max(prev_close);

            bp[i] = data[i].close - low_min;
            tr[i] = high_max - low_min;
        }

        // Calculate UO for each bar
        for i in 0..data.len() {
            if i < max_period {
                self.values.push(IndicatorValue::None);
                continue;
            }

            // Calculate sums for each period
            let bp1: f64 = bp[(i + 1 - self.period1)..=i].iter().sum();
            let tr1: f64 = tr[(i + 1 - self.period1)..=i].iter().sum();

            let bp2: f64 = bp[(i + 1 - self.period2)..=i].iter().sum();
            let tr2: f64 = tr[(i + 1 - self.period2)..=i].iter().sum();

            let bp3: f64 = bp[(i + 1 - self.period3)..=i].iter().sum();
            let tr3: f64 = tr[(i + 1 - self.period3)..=i].iter().sum();

            // Avoid division by zero
            let avg1 = if tr1.abs() > 1e-10 { bp1 / tr1 } else { 0.5 };
            let avg2 = if tr2.abs() > 1e-10 { bp2 / tr2 } else { 0.5 };
            let avg3 = if tr3.abs() > 1e-10 { bp3 / tr3 } else { 0.5 };

            // UO = 100 * ((4 * Avg1) + (2 * Avg2) + Avg3) / 7
            let uo = 100.0 * ((4.0 * avg1) + (2.0 * avg2) + avg3) / 7.0;
            self.values.push(IndicatorValue::Single(uo));
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
        false // UO is a separate oscillator pane
    }

    fn line_cnt(&self) -> usize {
        1
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
            "UO({}, {}, {})",
            self.period1, self.period2, self.period3
        )]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};

    fn create_test_bars() -> Vec<Bar> {
        let start = Utc::now();
        (0..50)
            .map(|i| {
                let price = 100.0 + (i as f64 * 0.2).sin() * 10.0;
                Bar {
                    time: start + Duration::minutes(i * 5),
                    open: price,
                    high: price + 2.0,
                    low: price - 2.0,
                    close: price + 0.5,
                    volume: 1000.0,
                }
            })
            .collect()
    }

    #[test]
    fn test_uo_calculation() {
        let bars = create_test_bars();
        let mut uo = UltimateOscillator::new(7, 14, 28);
        uo.calculate(&bars);

        assert_eq!(uo.values().len(), bars.len());

        // First max_period values should be None
        for i in 0..28 {
            assert!(matches!(uo.values()[i], IndicatorValue::None));
        }
    }

    #[test]
    fn test_uo_range() {
        let bars = create_test_bars();
        let mut uo = UltimateOscillator::new(7, 14, 28);
        uo.calculate(&bars);

        for value in uo.values() {
            if let IndicatorValue::Single(val) = value {
                assert!(
                    *val >= 0.0 && *val <= 100.0,
                    "UO should be 0-100, got {}",
                    val
                );
            }
        }
    }

    #[test]
    fn test_uo_is_not_overlay() {
        let uo = UltimateOscillator::new(7, 14, 28);
        assert!(!uo.is_overlay());
    }

    #[test]
    fn test_uo_empty_data() {
        let mut uo = UltimateOscillator::new(7, 14, 28);
        uo.calculate(&[]);
        assert!(uo.values().is_empty());
    }
}
