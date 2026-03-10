//! Donchian Channels Indicator
//!
//! Donchian Channels consist of three lines based on moving highest high
//! and lowest low calculations. They're used to identify breakouts and trends.
//!
//! # Components
//! - Upper Band: Highest high over N periods
//! - Lower Band: Lowest low over N periods
//! - Middle Band: Avg of upper and lower
//!
//! # Interpretation
//! - Price breaking above upper band: Bullish breakout
//! - Price breaking below lower band: Bearish breakout
//! - Channel width indicates volatility
//!
//! # Example
//! ```ignore
//! use egui_charts::DonchianChannels;
//!
//! let mut donchian = DonchianChannels::new(20);
//! donchian.calculate(&bars);
//! ```

use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Donchian Channels indicator
#[derive(Clone)]
pub struct DonchianChannels {
    period: usize,
    values: Vec<IndicatorValue>,
    upper_color: Color32,
    middle_color: Color32,
    lower_color: Color32,
    visible: bool,
}

impl DonchianChannels {
    /// Create a new Donchian Channels indicator
    ///
    /// # Arguments
    /// * `period` - Lookback period (typically 20)
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            values: Vec::new(),
            upper_color: DESIGN_TOKENS.semantic.extended.info, // Blue
            middle_color: DESIGN_TOKENS.semantic.extended.purple, // Purple
            lower_color: DESIGN_TOKENS.semantic.extended.info, // Blue
            visible: true,
        }
    }

    /// Set colors for upper, middle, and lower bands
    pub fn with_colors(mut self, upper: Color32, middle: Color32, lower: Color32) -> Self {
        self.upper_color = upper;
        self.middle_color = middle;
        self.lower_color = lower;
        self
    }

    /// Calculate highest high over period
    fn highest_high(data: &[Bar], end: usize, period: usize) -> f64 {
        let start = end.saturating_sub(period - 1);
        data[start..=end]
            .iter()
            .map(|b| b.high)
            .fold(f64::NEG_INFINITY, f64::max)
    }

    /// Calculate lowest low over period
    fn lowest_low(data: &[Bar], end: usize, period: usize) -> f64 {
        let start = end.saturating_sub(period - 1);
        data[start..=end]
            .iter()
            .map(|b| b.low)
            .fold(f64::INFINITY, f64::min)
    }
}

impl Indicator for DonchianChannels {
    fn name(&self) -> &str {
        "Donchian"
    }

    fn desc(&self) -> &str {
        "Donchian Channels - Breakout indicator"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.len() < self.period {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
        }

        for i in 0..data.len() {
            if i < self.period - 1 {
                self.values.push(IndicatorValue::None);
            } else {
                let upper = Self::highest_high(data, i, self.period);
                let lower = Self::lowest_low(data, i, self.period);
                let middle = (upper + lower) / 2.0;

                self.values
                    .push(IndicatorValue::Multiple(vec![middle, upper, lower]));
            }
        }
    }

    fn values(&self) -> &[IndicatorValue] {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        vec![self.middle_color, self.upper_color, self.lower_color]
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if !colors.is_empty() {
            self.middle_color = colors[0];
        }
        if colors.len() > 1 {
            self.upper_color = colors[1];
        }
        if colors.len() > 2 {
            self.lower_color = colors[2];
        }
    }

    fn is_overlay(&self) -> bool {
        true // Donchian is drawn on main chart
    }

    fn line_cnt(&self) -> usize {
        3 // Middle, Upper, Lower
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
            format!("Middle({})", self.period),
            "Upper".to_string(),
            "Lower".to_string(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};

    fn create_test_bars() -> Vec<Bar> {
        let start = Utc::now();
        (0..30)
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
    fn test_donchian_ordering() {
        let bars = create_test_bars();
        let mut donchian = DonchianChannels::new(20);
        donchian.calculate(&bars);

        for value in donchian.values() {
            if let IndicatorValue::Multiple(vals) = value {
                let middle = vals[0];
                let upper = vals[1];
                let lower = vals[2];

                assert!(upper >= middle, "Upper should be >= middle");
                assert!(middle >= lower, "Middle should be >= lower");
            }
        }
    }

    #[test]
    fn test_donchian_is_overlay() {
        let donchian = DonchianChannels::new(20);
        assert!(donchian.is_overlay());
    }

    #[test]
    fn test_donchian_line_cnt() {
        let donchian = DonchianChannels::new(20);
        assert_eq!(donchian.line_cnt(), 3);
    }

    #[test]
    fn test_donchian_empty_data() {
        let mut donchian = DonchianChannels::new(20);
        donchian.calculate(&[]);
        assert!(donchian.values().is_empty());
    }
}
