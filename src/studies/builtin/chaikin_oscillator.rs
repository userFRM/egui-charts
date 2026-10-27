//! Chaikin Oscillator
//!
//! Marc Chaikin's oscillator is the momentum of the Accumulation/Distribution line: the
//! difference between a fast and a slow exponential average of it.
//!
//! # Formula
//! ```text
//! money flow multiplier = ((close - low) - (high - close)) / (high - low), 0 when high == low
//! ADL = previous ADL + money flow multiplier * volume
//! Chaikin Oscillator = EMA(ADL, fast) - EMA(ADL, slow)
//! ```
//! with conventional lengths 3 and 10. Each EMA has alpha 2 / (length + 1) and is seeded with
//! the first ADL value, as [`EMA`](super::EMA) and the Chaikin Volatility average are, so the
//! oscillator starts at 0 on the first bar.
//!
//! # Example
//! ```
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{ChaikinOscillator, Indicator};
//!
//! let mut oscillator = ChaikinOscillator::new(3, 10);
//! oscillator.calculate(&bars);
//! assert_eq!(oscillator.values().value(0, 0), Some(0.0));
//! ```

use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Fast minus slow EMA of the Accumulation/Distribution line.
#[derive(Clone)]
pub struct ChaikinOscillator {
    fast_length: usize,
    slow_length: usize,
    values: IndicatorValues,
    stepper: Stepper<(f64, Option<[f64; 2]>)>,
    color: Color32,
    visible: bool,
}

impl ChaikinOscillator {
    /// Create a Chaikin Oscillator from its fast and slow EMA lengths (conventionally 3 and 10).
    pub fn new(fast_length: usize, slow_length: usize) -> Self {
        Self {
            fast_length: fast_length.max(1),
            slow_length: slow_length.max(1),
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            color: palette::RED,
            visible: true,
        }
    }
}

impl Default for ChaikinOscillator {
    fn default() -> Self {
        Self::new(3, 10)
    }
}

impl Indicator for ChaikinOscillator {
    crate::studies::inputs::study_inputs!(self;
        fast_length: length("Fast Length", 1, usize::MAX),
        slow_length: length("Slow Length", 1, usize::MAX)
    ; {});

    fn value_format(&self) -> crate::scales::StudyFormat {
        crate::scales::StudyFormat::Volume
    }

    fn name(&self) -> &str {
        "Chaikin Oscillator"
    }

    fn desc(&self) -> &str {
        "Chaikin Oscillator - Momentum of the Accumulation/Distribution line"
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
                    state.0 += multiplier * bar.volume;
                }
                let adl = if state.0.is_finite() {
                    state.0
                } else {
                    f64::NAN
                };
                let ema = state.1.get_or_insert([adl; 2]);
                ema[0] += (adl - ema[0]) * (2.0 / (self.fast_length as f64 + 1.0));
                ema[1] += (adl - ema[1]) * (2.0 / (self.slow_length as f64 + 1.0));
                IndicatorValue::Single(ema[0] - ema[1])
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
        if let Some(&color) = colors.first() {
            self.color = color;
        }
    }

    fn pane(&self) -> crate::studies::StudyPane {
        crate::studies::StudyPane {
            levels: vec![(0.0, Color32::from_gray(128), "Zero")],
            ..Default::default()
        }
    }

    fn is_overlay(&self) -> bool {
        false
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
            "Chaikin Osc({}, {})",
            self.fast_length, self.slow_length
        )]
    }
}
