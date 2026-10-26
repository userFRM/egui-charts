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

use super::over_finite_bars;
use crate::model::Bar;
use crate::studies::helpers;
use crate::studies::palette;
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
            colors: [palette::ACCENT, palette::PINK, palette::GREEN],
            visible: true,
        }
    }

    /// Set the forward displacement of the jaw, teeth and lips in bars.
    pub fn with_offsets(mut self, jaw: usize, teeth: usize, lips: usize) -> Self {
        (self.jaw_offset, self.teeth_offset, self.lips_offset) = (jaw, teeth, lips);
        self
    }

    /// The values over `data`, every bar of which has a finite high and low.
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let median: Vec<f64> = data.iter().map(|b| (b.high + b.low) / 2.0).collect();
        let lines = [self.jaw_length, self.teeth_length, self.lips_length]
            .map(|n| helpers::rma(&median, n));
        (0..data.len())
            .map(|i| IndicatorValue::Multiple(lines.iter().map(|line| line[i]).collect()))
            .collect()
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
        self.values = over_finite_bars(
            data,
            |b| b.high.is_finite() && b.low.is_finite(),
            |data| self.compute(data),
        );
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
