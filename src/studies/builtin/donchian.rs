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
//! ```
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{DonchianChannels, Indicator};
//!
//! let mut donchian = DonchianChannels::new(20);
//! donchian.calculate(&bars);
//! ```

use super::over_finite_bars;
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::{rolling_max, rolling_min};
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Donchian Channels indicator
#[derive(Clone)]
pub struct DonchianChannels {
    period: usize,
    values: IndicatorValues,
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
            values: IndicatorValues::new(),
            upper_color: palette::BLUE,    // Blue
            middle_color: palette::PURPLE, // Purple
            lower_color: palette::BLUE,    // Blue
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
}

/// Construct with the conventional default parameters.
impl Default for DonchianChannels {
    fn default() -> Self {
        Self::new(20)
    }
}

impl DonchianChannels {
    /// The values over `data`, every bar of which has finite inputs.
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let mut values = IndicatorValues::new();
        if data.len() < self.period {
            for _ in 0..data.len() {
                values.push(IndicatorValue::None);
            }
            return values;
        }

        let highest = rolling_max(data.iter().map(|b| b.high), self.period);
        let lowest = rolling_min(data.iter().map(|b| b.low), self.period);
        for i in 0..data.len() {
            if i < self.period - 1 {
                values.push(IndicatorValue::None);
            } else {
                let (upper, lower) = (highest[i], lowest[i]);
                let middle = (upper + lower) / 2.0;

                values.push_lines(&[middle, upper, lower]);
            }
        }

        values
    }
}

impl Indicator for DonchianChannels {
    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "Donchian"
    }

    fn desc(&self) -> &str {
        "Donchian Channels - Breakout indicator"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values = over_finite_bars(
            data,
            |b| b.high.is_finite() && b.low.is_finite(),
            |data| self.compute(data),
        );
    }

    fn values(&self) -> &IndicatorValues {
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
