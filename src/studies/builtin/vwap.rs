//! Volume Weighted Avg Price (VWAP) Indicator
//!
//! VWAP is the avg price weighted by volume, commonly used as a trading benchmark.
//! It's often reset at the start of each trading session.
//!
//! # Interpretation
//! - Price above VWAP: Bullish bias
//! - Price below VWAP: Bearish bias
//! - VWAP can act as support/resistance
//! - Institutional traders often use VWAP as execution benchmark
//!
//! # Calculation
//! VWAP = Cumulative(Typical Price * Volume) / Cumulative(Volume)
//! Typical Price = (High + Low + Close) / 3
//!
//! # Example
//! ```ignore
//! use egui_charts::VolumeWeightedAvgPrice;
//!
//! let mut vwap = VolumeWeightedAvgPrice::new();
//! vwap.calculate(&bars);
//! ```

use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Volume Weighted Avg Price indicator
#[derive(Clone)]
pub struct VolumeWeightedAvgPrice {
    values: Vec<IndicatorValue>,
    color: Color32,
    visible: bool,
    /// Whether to reset VWAP at session boundaries (requires proper session data)
    reset_on_session: bool,
}

impl VolumeWeightedAvgPrice {
    /// Create a new VWAP indicator
    pub fn new() -> Self {
        Self {
            values: Vec::new(),
            color: DESIGN_TOKENS.semantic.extended.favorite_gold, // Amber
            visible: true,
            reset_on_session: false,
        }
    }

    /// Enable session-based VWAP reset
    pub fn with_session_reset(mut self) -> Self {
        self.reset_on_session = true;
        self
    }

    /// Set the indicator color
    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }

    /// Calculate typical price
    #[inline]
    fn typical_price(bar: &Bar) -> f64 {
        (bar.high + bar.low + bar.close) / 3.0
    }

    /// Check if this is a new session (simplified: checks for day change)
    fn is_new_session(current: &Bar, previous: &Bar) -> bool {
        current.time.date_naive() != previous.time.date_naive()
    }
}

impl Default for VolumeWeightedAvgPrice {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for VolumeWeightedAvgPrice {
    fn name(&self) -> &str {
        "VWAP"
    }

    fn desc(&self) -> &str {
        "Volume Weighted Avg Price - Trading benchmark indicator"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.is_empty() {
            return;
        }

        let mut cumulative_tp_vol = 0.0;
        let mut cumulative_vol = 0.0;

        for i in 0..data.len() {
            // Check for session reset
            if self.reset_on_session && i > 0 && Self::is_new_session(&data[i], &data[i - 1]) {
                cumulative_tp_vol = 0.0;
                cumulative_vol = 0.0;
            }

            let tp = Self::typical_price(&data[i]);
            cumulative_tp_vol += tp * data[i].volume;
            cumulative_vol += data[i].volume;

            if cumulative_vol.abs() < 1e-10 {
                self.values.push(IndicatorValue::Single(tp));
            } else {
                let vwap = cumulative_tp_vol / cumulative_vol;
                self.values.push(IndicatorValue::Single(vwap));
            }
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
        true // VWAP is drawn on the main chart
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
        vec!["VWAP".to_string()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};

    #[test]
    fn test_vwap_calculation() {
        let start = Utc::now();
        let bars = vec![
            Bar {
                time: start,
                open: 100.0,
                high: 102.0,
                low: 98.0,
                close: 101.0,
                volume: 1000.0,
            },
            Bar {
                time: start + Duration::minutes(5),
                open: 101.0,
                high: 104.0,
                low: 100.0,
                close: 103.0,
                volume: 2000.0,
            },
        ];

        let mut vwap = VolumeWeightedAvgPrice::new();
        vwap.calculate(&bars);

        assert_eq!(vwap.values().len(), 2);

        // First VWAP = typical price of first bar = (102+98+101)/3 = 100.33
        if let IndicatorValue::Single(v) = vwap.values()[0] {
            assert!((v - 100.333).abs() < 0.01);
        }

        // Second VWAP = cumulative calculation
        // TP1 = 100.33, TP2 = (104+100+103)/3 = 102.33
        // VWAP = (100.33*1000 + 102.33*2000) / 3000 = 101.66
        if let IndicatorValue::Single(v) = vwap.values()[1] {
            assert!((v - 101.666).abs() < 0.01);
        }
    }

    #[test]
    fn test_vwap_is_overlay() {
        let vwap = VolumeWeightedAvgPrice::new();
        assert!(vwap.is_overlay(), "VWAP should be an overlay indicator");
    }

    #[test]
    fn test_vwap_empty_data() {
        let mut vwap = VolumeWeightedAvgPrice::new();
        vwap.calculate(&[]);
        assert!(vwap.values().is_empty());
    }
}
