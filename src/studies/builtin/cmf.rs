//! Chaikin Money Flow (CMF) Indicator
//!
//! CMF measures the amount of Money Flow Volume over a specific period,
//! providing a bounded oscillator between -1 and +1.
//!
//! # Formula
//! CMF = Sum(Money Flow Volume, period) / Sum(Volume, period)
//! Where Money Flow Volume = Money Flow Multiplier * Volume
//! And Money Flow Multiplier = ((Close - Low) - (High - Close)) / (High - Low)
//!
//! # Interpretation
//! - CMF > 0: Buying pressure (accumulation)
//! - CMF < 0: Selling pressure (distribution)
//! - CMF near +1: Strong accumulation
//! - CMF near -1: Strong distribution
//!
//! # Example
//! ```
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{ChaikinMoneyFlow, Indicator};
//!
//! let mut cmf = ChaikinMoneyFlow::new(20);
//! cmf.calculate(&bars);
//! ```

use super::over_finite_bars;
use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Chaikin Money Flow indicator
#[derive(Clone)]
pub struct ChaikinMoneyFlow {
    period: usize,
    values: IndicatorValues,
    positive_color: Color32,
    negative_color: Color32,
    visible: bool,
}

impl ChaikinMoneyFlow {
    /// Create a new CMF indicator
    ///
    /// # Arguments
    /// * `period` - Lookback period (typically 20 or 21)
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            values: IndicatorValues::new(),
            positive_color: DESIGN_TOKENS.semantic.extended.success,
            negative_color: DESIGN_TOKENS.semantic.extended.error,
            visible: true,
        }
    }

    /// Create with default param (20)
    pub fn default_params() -> Self {
        Self::new(20)
    }

    /// Set colors for positive and negative values
    pub fn with_colors(mut self, positive: Color32, negative: Color32) -> Self {
        self.positive_color = positive;
        self.negative_color = negative;
        self
    }
}

/// Construct with the conventional default parameters.
impl Default for ChaikinMoneyFlow {
    fn default() -> Self {
        Self::new(20)
    }
}

impl ChaikinMoneyFlow {
    /// The values over `data`, every bar of which has finite inputs.
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let mut values = IndicatorValues::new();
        if data.is_empty() {
            return values;
        }

        // Calculate Money Flow Volume for each bar
        let mut mfv: Vec<f64> = Vec::with_capacity(data.len());
        let mut volumes: Vec<f64> = Vec::with_capacity(data.len());

        for bar in data {
            let high_low = bar.high - bar.low;

            if high_low > 0.0 {
                let mfm = (2.0 * bar.close - bar.high - bar.low) / high_low;
                mfv.push(mfm * bar.volume);
            } else {
                mfv.push(0.0);
            }
            volumes.push(bar.volume);
        }

        // Calculate CMF for each period
        for i in 0..data.len() {
            if i < self.period - 1 {
                values.push(IndicatorValue::None);
                continue;
            }

            let start = i + 1 - self.period;
            let sum_mfv: f64 = mfv[start..=i].iter().sum();
            let sum_vol: f64 = volumes[start..=i].iter().sum();

            let cmf = if sum_vol > 0.0 {
                sum_mfv / sum_vol
            } else {
                0.0
            };

            values.push(IndicatorValue::Single(cmf));
        }

        values
    }
}

impl Indicator for ChaikinMoneyFlow {
    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "CMF"
    }

    fn desc(&self) -> &str {
        "Chaikin Money Flow - Measures buying/selling pressure over period"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values = over_finite_bars(
            data,
            |b| {
                b.high.is_finite()
                    && b.low.is_finite()
                    && b.close.is_finite()
                    && b.volume.is_finite()
            },
            |data| self.compute(data),
        );
    }

    fn values(&self) -> &IndicatorValues {
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
        false // CMF is a separate oscillator pane
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
        vec![format!("CMF({})", self.period)]
    }
}
