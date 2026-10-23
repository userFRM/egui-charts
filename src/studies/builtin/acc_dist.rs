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

use super::over_finite_bars;
use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Accumulation/Distribution Line indicator
#[derive(Clone)]
pub struct AccumulationDistribution {
    values: Vec<IndicatorValue>,
    color: Color32,
    visible: bool,
}

impl AccumulationDistribution {
    /// Create a new A/D Line indicator
    pub fn new() -> Self {
        Self {
            values: Vec::new(),
            color: DESIGN_TOKENS.semantic.extended.info, // Blue
            visible: true,
        }
    }

    /// Set the line color
    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }

    /// The values over `data`, every bar of which has a finite high, low, close and volume.
    fn compute(&self, data: &[Bar]) -> Vec<IndicatorValue> {
        let mut values = Vec::new();

        if data.is_empty() {
            return values;
        }

        let mut ad = 0.0;

        for bar in data {
            let high_low = bar.high - bar.low;

            if high_low > 0.0 {
                // Money Flow Multiplier: ((Close - Low) - (High - Close)) / (High - Low)
                // Simplifies to: (2 * Close - High - Low) / (High - Low)
                let mfm = (2.0 * bar.close - bar.high - bar.low) / high_low;

                // Money Flow Volume
                let mfv = mfm * bar.volume;

                // Cumulative A/D
                ad += mfv;
            }
            // If high == low (doji), no change to A/D

            values.push(IndicatorValue::Single(ad));
        }

        values
    }
}

impl Default for AccumulationDistribution {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for AccumulationDistribution {
    fn name(&self) -> &str {
        "A/D"
    }

    fn desc(&self) -> &str {
        "Accumulation/Distribution Line - Measures buying/selling pressure"
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
        false // A/D is a separate pane
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
        vec!["A/D Line".to_string()]
    }
}
