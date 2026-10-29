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
//! ```
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{UltimateOscillator, Indicator};
//!
//! let mut uo = UltimateOscillator::new(7, 14, 28);
//! uo.calculate(&bars);
//! ```

use super::over_finite_bars;
use crate::model::Bar;
use crate::studies::stepper::{update_window, usable_behind};
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Ultimate Oscillator indicator
#[derive(Clone)]
pub struct UltimateOscillator {
    period1: usize, // Short (typically 7)
    period2: usize, // Medium (typically 14)
    period3: usize, // Long (typically 28)
    values: IndicatorValues,
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
            values: IndicatorValues::new(),
            color: crate::studies::palette::BRIGHT_RED,
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

/// Construct with the conventional default parameters.
impl Default for UltimateOscillator {
    fn default() -> Self {
        Self::new(7, 14, 28)
    }
}

impl UltimateOscillator {
    /// The values over `data`, every bar of which has finite inputs.
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let mut values = IndicatorValues::new();
        if data.is_empty() {
            return values;
        }

        let max_period = self.period1.max(self.period2).max(self.period3);
        if data.len() < max_period.saturating_add(1) {
            for _ in 0..data.len() {
                values.push(IndicatorValue::None);
            }
            return values;
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
                values.push(IndicatorValue::None);
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
            values.push(IndicatorValue::Single(uo));
        }

        values
    }
}

impl Indicator for UltimateOscillator {
    crate::studies::inputs::study_inputs!(self;
        period1: length("Period1", 1, usize::MAX),
        period2: length("Period2", 1, usize::MAX),
        period3: length("Period3", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "UO"
    }

    fn desc(&self) -> &str {
        "Ultimate Oscillator - Multi-timeframe momentum indicator"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        let mut values = std::mem::take(&mut self.values);
        update_window(
            &mut values,
            data,
            from,
            usable_behind(self.period1.max(self.period2).max(self.period3), |b| {
                b.high.is_finite() && b.low.is_finite() && b.close.is_finite()
            }),
            |data| {
                over_finite_bars(
                    data,
                    |b| b.high.is_finite() && b.low.is_finite() && b.close.is_finite(),
                    |data| self.compute(data),
                )
            },
        );
        self.values = values;
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn values(&self) -> &IndicatorValues {
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

    fn line_count(&self) -> usize {
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
