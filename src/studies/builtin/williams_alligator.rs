//! Williams Alligator
//!
//! Bill Williams' Alligator (Trading Chaos, 1995) is three smoothed moving averages of the
//! median price, each displaced into the future, that intertwine while the market ranges and
//! fan out in order while it trends.
//!
//! # Formula
//! - median = (high + low) / 2
//! - Jaw (blue) = SMMA(median, 13), drawn 8 bars ahead
//! - Teeth (red) = SMMA(median, 8), drawn 5 bars ahead
//! - Lips (green) = SMMA(median, 5), drawn 3 bars ahead
//!
//! SMMA(x, n) starts with the simple mean of its first `n` values, then
//! SMMA = (SMMA[1] * (n - 1) + x) / n. Values stay indexed by their source bar; the
//! displacement is [`plot_offset`](Indicator::plot_offset), so the last values are drawn in
//! the future whitespace.
//!
//! # Example
//! ```
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{Indicator, WilliamsAlligator};
//!
//! let mut alligator = WilliamsAlligator::new(13, 8, 5);
//! alligator.calculate(&bars);
//! assert_eq!(alligator.plot_offset(0), 8);
//! ```

use super::smoothing::Smoothing;
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Williams Alligator: jaw, teeth and lips lines, displaced forward.
#[derive(Clone)]
pub struct WilliamsAlligator {
    jaw_length: usize,
    teeth_length: usize,
    lips_length: usize,
    jaw_offset: usize,
    teeth_offset: usize,
    lips_offset: usize,
    values: IndicatorValues,
    stepper: Stepper<[Smoothing; 3]>,
    colors: [Color32; 3],
    visible: bool,
}

impl WilliamsAlligator {
    /// Create an Alligator with the jaw, teeth and lips smoothing lengths (typically 13, 8, 5)
    /// and the conventional displacements 8, 5 and 3.
    pub fn new(jaw_length: usize, teeth_length: usize, lips_length: usize) -> Self {
        Self {
            jaw_length: jaw_length.max(1),
            teeth_length: teeth_length.max(1),
            lips_length: lips_length.max(1),
            jaw_offset: 8,
            teeth_offset: 5,
            lips_offset: 3,
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            colors: [
                palette::ACCENT,
                palette::PINK,
                Color32::from_rgb(102, 187, 106),
            ],
            visible: true,
        }
    }

    /// Set the forward displacement of the jaw, teeth and lips in bars.
    pub fn with_offsets(mut self, jaw: usize, teeth: usize, lips: usize) -> Self {
        (self.jaw_offset, self.teeth_offset, self.lips_offset) = (jaw, teeth, lips);
        self
    }
}

/// Construct with the conventional default parameters.
impl Default for WilliamsAlligator {
    fn default() -> Self {
        Self::new(13, 8, 5)
    }
}

impl Indicator for WilliamsAlligator {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::HL2,
        ))
    }

    crate::studies::inputs::study_inputs!(self;
        jaw_length: length("Jaw length", 1, usize::MAX),
        teeth_length: length("Teeth length", 1, usize::MAX),
        lips_length: length("Lips length", 1, usize::MAX),
        jaw_offset: length("Jaw offset", 0, isize::MAX as usize),
        teeth_offset: length("Teeth offset", 0, isize::MAX as usize),
        lips_offset: length("Lips offset", 0, isize::MAX as usize)
    ; {});

    fn name(&self) -> &str {
        "Williams Alligator"
    }

    fn desc(&self) -> &str {
        "Williams Alligator - Displaced smoothed averages of the median price"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                if !bar.high.is_finite() || !bar.low.is_finite() {
                    return IndicatorValue::None;
                }
                let median = (bar.high + bar.low) / 2.0;
                let values = state
                    .iter_mut()
                    .zip([self.jaw_length, self.teeth_length, self.lips_length])
                    .map(|(average, n)| average.next(median, n, 1.0 / n as f64))
                    .collect();
                IndicatorValue::Multiple(values)
            });
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn plot_offset(&self, line: usize) -> isize {
        let offset = [self.jaw_offset, self.teeth_offset, self.lips_offset]
            .get(line)
            .copied()
            .unwrap_or(0);
        offset.min(isize::MAX as usize) as isize
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        self.colors.to_vec()
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        for (old, new) in self.colors.iter_mut().zip(colors) {
            *old = new;
        }
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
        vec!["Jaw".into(), "Teeth".into(), "Lips".into()]
    }
}
