//! Awesome Oscillator (AO) Indicator
//!
//! Developed by Bill Williams, the Awesome Oscillator measures market momentum
//! by comparing recent market momentum to a larger time frame.
//!
//! # Formula
//! AO = SMA(median_price, 5) - SMA(median_price, 34)
//! Where median_price = (high + low) / 2
//!
//! # Interpretation
//! - AO > 0: Bullish momentum
//! - AO < 0: Bearish momentum
//! - Zero line crossovers signal momentum shifts
//! - Twin peaks (divergence) signals
//! - Saucer signals (3-bar pattern)
//!
//! # Example
//! ```ignore
//! use egui_charts::AwesomeOscillator;
//!
//! let mut ao = AwesomeOscillator::new(5, 34);
//! ao.calculate(&bars);
//! ```

use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Awesome Oscillator indicator
#[derive(Clone)]
pub struct AwesomeOscillator {
    fast_period: usize,
    slow_period: usize,
    values: Vec<IndicatorValue>,
    positive_color: Color32,
    negative_color: Color32,
    visible: bool,
}

impl AwesomeOscillator {
    /// Create a new Awesome Oscillator
    ///
    /// # Arguments
    /// * `fast_period` - Fast SMA period (typically 5)
    /// * `slow_period` - Slow SMA period (typically 34)
    pub fn new(fast_period: usize, slow_period: usize) -> Self {
        Self {
            fast_period: fast_period.max(1),
            slow_period: slow_period.max(1),
            values: Vec::new(),
            positive_color: DESIGN_TOKENS.semantic.extended.success, // Green
            negative_color: DESIGN_TOKENS.semantic.extended.error,   // Red
            visible: true,
        }
    }

    /// Create with default params (5, 34)
    pub fn default_params() -> Self {
        Self::new(5, 34)
    }

    /// Set colors for positive and negative values
    pub fn with_colors(mut self, positive: Color32, negative: Color32) -> Self {
        self.positive_color = positive;
        self.negative_color = negative;
        self
    }
}

impl Indicator for AwesomeOscillator {
    fn name(&self) -> &str {
        "AO"
    }

    fn desc(&self) -> &str {
        "Awesome Oscillator - Market momentum indicator"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.is_empty() {
            return;
        }

        let min_period = self.slow_period;

        // Calculate median prices (HL/2)
        let median_prices: Vec<f64> = data.iter().map(|b| (b.high + b.low) / 2.0).collect();

        for i in 0..data.len() {
            if i < min_period - 1 {
                self.values.push(IndicatorValue::None);
                continue;
            }

            // Calculate fast SMA of median prices
            let fast_start = i + 1 - self.fast_period;
            let fast_sma: f64 =
                median_prices[fast_start..=i].iter().sum::<f64>() / self.fast_period as f64;

            // Calculate slow SMA of median prices
            let slow_start = i + 1 - self.slow_period;
            let slow_sma: f64 =
                median_prices[slow_start..=i].iter().sum::<f64>() / self.slow_period as f64;

            // AO = Fast SMA - Slow SMA
            let ao = fast_sma - slow_sma;
            self.values.push(IndicatorValue::Single(ao));
        }
    }

    fn values(&self) -> &[IndicatorValue] {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        vec![self.positive_color, self.negative_color]
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if !colors.is_empty() {
            self.positive_color = colors[0];
        }
        if colors.len() > 1 {
            self.negative_color = colors[1];
        }
    }

    fn is_overlay(&self) -> bool {
        false // AO is a separate histogram pane
    }

    fn line_cnt(&self) -> usize {
        1 // Single histogram
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
        vec![format!("AO({}, {})", self.fast_period, self.slow_period)]
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
    fn test_ao_calculation() {
        let bars = create_uptrending_bars();
        let mut ao = AwesomeOscillator::new(5, 34);
        ao.calculate(&bars);

        assert_eq!(ao.values().len(), bars.len());

        // First slow_period - 1 should be None
        for i in 0..33 {
            assert!(matches!(ao.values()[i], IndicatorValue::None));
        }
    }

    #[test]
    fn test_ao_positive_in_uptrend() {
        let bars = create_uptrending_bars();
        let mut ao = AwesomeOscillator::new(5, 34);
        ao.calculate(&bars);

        // In strong uptrend, AO should be positive
        if let Some(IndicatorValue::Single(val)) = ao.values().last() {
            assert!(*val > 0.0, "AO should be positive in uptrend");
        }
    }

    #[test]
    fn test_ao_negative_in_downtrend() {
        let bars = create_downtrending_bars();
        let mut ao = AwesomeOscillator::new(5, 34);
        ao.calculate(&bars);

        // In strong downtrend, AO should be negative
        if let Some(IndicatorValue::Single(val)) = ao.values().last() {
            assert!(*val < 0.0, "AO should be negative in downtrend");
        }
    }

    #[test]
    fn test_ao_is_not_overlay() {
        let ao = AwesomeOscillator::new(5, 34);
        assert!(!ao.is_overlay());
    }

    #[test]
    fn test_ao_empty_data() {
        let mut ao = AwesomeOscillator::new(5, 34);
        ao.calculate(&[]);
        assert!(ao.values().is_empty());
    }
}
