//! Accumulation/Distribution Line (A/D Line) Indicator
//!
//! The A/D Line measures the cumulative flow of money into and out of a security
//! based on the close's position within the bar's high-low range.
//!
//! # Formula
//! Money Flow Multiplier = ((Close - Low) - (High - Close)) / (High - Low)
//! Money Flow Volume = Money Flow Multiplier * Volume
//! A/D Line = Previous A/D + Money Flow Volume
//!
//! # Interpretation
//! - Rising A/D Line: Accumulation (buying pressure)
//! - Falling A/D Line: Distribution (selling pressure)
//! - Divergence with price: Potential reversal signal
//!
//! # Example
//! ```
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{AccumulationDistribution, Indicator};
//!
//! let mut ad = AccumulationDistribution::new();
//! ad.calculate(&bars);
//! ```

use crate::model::Bar;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Accumulation/Distribution Line indicator
#[derive(Clone)]
pub struct AccumulationDistribution {
    values: IndicatorValues,
    stepper: Stepper<f64>,
    color: Color32,
    visible: bool,
}

impl AccumulationDistribution {
    /// Create a new A/D Line indicator
    pub fn new() -> Self {
        Self {
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            color: Color32::from_rgb(153, 153, 21), // Blue
            visible: true,
        }
    }

    /// Set the line color
    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }
}

impl Default for AccumulationDistribution {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for AccumulationDistribution {
    fn value_format(&self) -> crate::scales::StudyFormat {
        crate::scales::StudyFormat::Volume
    }

    fn name(&self) -> &str {
        "A/D"
    }

    fn desc(&self) -> &str {
        "Accumulation/Distribution Line - Measures buying/selling pressure"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                if ![bar.high, bar.low, bar.close, bar.volume]
                    .iter()
                    .all(|x| x.is_finite())
                {
                    return IndicatorValue::None;
                }
                let range = bar.high - bar.low;
                if range > 0.0 {
                    let multiplier = (2.0 * bar.close - bar.high - bar.low) / range;
                    *state += multiplier * bar.volume;
                }
                IndicatorValue::Single(*state)
            });
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
        false // A/D is a separate pane
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
        vec!["A/D Line".to_string()]
    }
}
