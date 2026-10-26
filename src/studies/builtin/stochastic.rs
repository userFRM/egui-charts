//! Stochastic Oscillator Indicator
//!
//! The Stochastic Oscillator is a momentum indicator comparing the closing price
//! to the range of prices over a specified period. It generates overbought/oversold
//! signals when %K crosses above 80 or below 20.
//!
//! # Components
//! - %K (Fast): Main oscillator line
//! - %D (Slow): Signal line (SMA of %K)
//!
//! # Calculation
//! %K = 100 * (Close - Lowest Low) / (Highest High - Lowest Low)
//! %D = SMA(%K, d_period)
//!
//! # Example
//! ```
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{Stochastic, Indicator};
//!
//! let mut stoch = Stochastic::new(14, 3, 3);
//! stoch.calculate(&bars);
//! ```

use super::over_finite_bars;
use crate::model::Bar;
use crate::studies::helpers;
use crate::studies::stepper::{rolling_max, rolling_min, update_window, usable_behind};
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Stochastic Oscillator indicator
#[derive(Clone)]
pub struct Stochastic {
    k_period: usize,
    k_smooth: usize,
    d_period: usize,
    values: IndicatorValues,
    k_color: Color32,
    d_color: Color32,
    visible: bool,
}

impl Stochastic {
    /// Create a new Stochastic Oscillator
    ///
    /// # Arguments
    /// * `k_period` - Lookback period for %K (typically 14)
    /// * `k_smooth` - Smoothing period for %K (typically 3)
    /// * `d_period` - Period for %D signal line (typically 3)
    pub fn new(k_period: usize, k_smooth: usize, d_period: usize) -> Self {
        Self {
            k_period: k_period.max(1),
            k_smooth: k_smooth.max(1),
            d_period: d_period.max(1),
            values: IndicatorValues::new(),
            k_color: DESIGN_TOKENS.semantic.extended.info, // Blue
            d_color: DESIGN_TOKENS.semantic.extended.warning, // Orange
            visible: true,
        }
    }

    /// Create with default params (14, 3, 3)
    pub fn default_params() -> Self {
        Self::new(14, 3, 3)
    }

    /// Set colors for %K and %D lines
    pub fn with_colors(mut self, k_color: Color32, d_color: Color32) -> Self {
        self.k_color = k_color;
        self.d_color = d_color;
        self
    }
}

/// Construct with the conventional default parameters.
impl Default for Stochastic {
    fn default() -> Self {
        Self::new(14, 3, 3)
    }
}

impl Stochastic {
    /// The values over `data`, every bar of which has finite inputs.
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let mut values = IndicatorValues::new();
        let min_period = self.k_period.saturating_add(self.k_smooth) - 1;
        if data.len() < min_period {
            for _ in 0..data.len() {
                values.push(IndicatorValue::None);
            }
            return values;
        }

        // Calculate raw %K (Fast Stochastic)
        let highest = rolling_max(data.iter().map(|b| b.high), self.k_period);
        let lowest = rolling_min(data.iter().map(|b| b.low), self.k_period);
        let mut raw_k = Vec::with_capacity(data.len());
        for i in 0..data.len() {
            if i < self.k_period - 1 {
                raw_k.push(f64::NAN);
            } else {
                let (highest, lowest) = (highest[i], lowest[i]);
                let range = highest - lowest;

                if range.abs() < 1e-10 {
                    raw_k.push(50.0); // Middle value when no range
                } else {
                    let k = 100.0 * (data[i].close - lowest) / range;
                    raw_k.push(k.clamp(0.0, 100.0));
                }
            }
        }

        let smooth_k = helpers::sma(&raw_k, self.k_smooth);
        let d_values = helpers::sma(&smooth_k, self.d_period);

        // Build output values
        for i in 0..data.len() {
            if smooth_k[i].is_nan() || d_values[i].is_nan() {
                values.push(IndicatorValue::None);
            } else {
                values.push_lines(&[smooth_k[i], d_values[i]]);
            }
        }

        values
    }
}

impl Indicator for Stochastic {
    crate::studies::inputs::study_inputs!(self;
        k_period: length("K period", 1, usize::MAX),
        k_smooth: length("K smooth", 1, usize::MAX),
        d_period: length("D period", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "Stochastic"
    }

    fn precision(&self) -> Option<usize> {
        Some(2)
    }

    fn desc(&self) -> &str {
        "Stochastic Oscillator - Momentum oscillator (0-100)"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        let mut values = std::mem::take(&mut self.values);
        let behind = (self.k_period - 1)
            .saturating_add(self.k_smooth - 1)
            .saturating_add(self.d_period - 1);
        let usable = |b: &Bar| b.high.is_finite() && b.low.is_finite() && b.close.is_finite();
        update_window(
            &mut values,
            data,
            from,
            usable_behind(behind, usable),
            |data| over_finite_bars(data, usable, |data| self.compute(data)),
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
        vec![self.k_color, self.d_color]
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if !colors.is_empty() {
            self.k_color = colors[0];
        }
        if colors.len() > 1 {
            self.d_color = colors[1];
        }
    }

    fn pane(&self) -> crate::studies::StudyPane {
        crate::studies::StudyPane {
            height: None,
            range: Some((0.0, 100.0)),
            levels: vec![
                (
                    80.0,
                    crate::tokens::DESIGN_TOKENS.semantic.extended.bearish,
                    "Overbought",
                ),
                (
                    20.0,
                    crate::tokens::DESIGN_TOKENS.semantic.extended.bullish,
                    "Oversold",
                ),
            ],
            fill_between_levels: false,
        }
    }

    fn is_overlay(&self) -> bool {
        false // Stochastic is drawn in separate pane
    }

    fn line_cnt(&self) -> usize {
        2 // %K and %D
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
            format!("%K({},{},{})", self.k_period, self.k_smooth, self.d_period),
            "%D".to_string(),
        ]
    }
}
