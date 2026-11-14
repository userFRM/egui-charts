//! SMI Ergodic Indicator and SMI Ergodic Oscillator, William Blau.
//!
//! The Stochastic Momentum Index of Blau's ergodic system is his True Strength Index as a ratio,
//! with an exponential signal line. The oscillator is their difference, drawn as a histogram.
//!
//! # Formula
//!
//! ```text
//! m          = close - previous close
//! SMI        = EMA(EMA(m, long), short) / EMA(EMA(|m|, long), short)
//! Signal     = EMA(SMI, signal)
//! Oscillator = SMI - Signal
//! ```
//!
//! The SMI runs from -1 to 1. Every EMA starts at its first input, as in
//! [`TrueStrengthIndex`](super::TrueStrengthIndex), which computes the same lines scaled by 100: the
//! changes start on bar 1 and values start on bar `long + short`. Defaults: long 20, short 5,
//! signal 5.
//!
//! # Example
//! ```
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{Indicator, SMIErgodicIndicator, SMIErgodicOscillator};
//!
//! let mut lines = SMIErgodicIndicator::default();
//! lines.calculate(&bars);
//! let mut histogram = SMIErgodicOscillator::default();
//! histogram.calculate(&bars);
//! let (smi, signal) = (lines.values().value(0, 40).unwrap(), lines.values().value(1, 40).unwrap());
//! assert!((histogram.values().value(0, 40).unwrap() - (smi - signal)).abs() < 1e-12);
//! ```

use super::TrueStrengthIndex;
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// SMI and signal lines of the SMI Ergodic Indicator.
pub type SMIErgodicIndicator = SMIErgodic<false>;

/// Histogram of the SMI minus its signal, the SMI Ergodic Oscillator.
pub type SMIErgodicOscillator = SMIErgodic<true>;

/// Blau's SMI Ergodic study: the SMI and signal lines, or with `OSCILLATOR` their difference.
#[derive(Clone)]
pub struct SMIErgodic<const OSCILLATOR: bool> {
    long_length: usize,
    short_length: usize,
    signal_length: usize,
    values: IndicatorValues,
    tsi: TrueStrengthIndex,
    /// Colors: [SMI, Signal], or [Oscillator]
    colors: Vec<Color32>,
    visible: bool,
}

impl<const OSCILLATOR: bool> SMIErgodic<OSCILLATOR> {
    /// Create the study with the long and short smoothing and signal lengths (20, 5 and 5 by convention).
    pub fn new(long_length: usize, short_length: usize, signal_length: usize) -> Self {
        Self {
            long_length: long_length.max(1),
            short_length: short_length.max(1),
            signal_length: signal_length.max(1),
            values: IndicatorValues::new(),
            tsi: TrueStrengthIndex::new(long_length, short_length, signal_length),
            colors: if OSCILLATOR {
                vec![Color32::from_rgb(255, 82, 82)]
            } else {
                vec![palette::BLUE, palette::ORANGE]
            },
            visible: true,
        }
    }
}

impl<const OSCILLATOR: bool> Default for SMIErgodic<OSCILLATOR> {
    fn default() -> Self {
        Self::new(20, 5, 5)
    }
}

impl<const OSCILLATOR: bool> Indicator for SMIErgodic<OSCILLATOR> {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::Close,
        ))
    }

    crate::studies::inputs::study_inputs!(self;
        long_length: length("Long length", 1, usize::MAX),
        short_length: length("Short length", 1, usize::MAX),
        signal_length: length("Signal line length", 1, usize::MAX)
    ; {});

    fn plot_style(&self, line: usize) -> crate::studies::PlotStyle {
        crate::studies::PlotStyle {
            color: self.colors[line.min(self.colors.len() - 1)],
            width: 1.0,
            pattern: crate::model::LineStyle::Solid,
            plot_type: if OSCILLATOR {
                crate::studies::PlotType::Histogram
            } else {
                crate::studies::PlotType::Line
            },
            visible: true,
        }
    }

    fn value_format(&self) -> crate::scales::StudyFormat {
        crate::scales::StudyFormat::Fixed(4)
    }

    fn name(&self) -> &str {
        if OSCILLATOR {
            "SMI Ergodic Oscillator"
        } else {
            "SMI Ergodic Indicator"
        }
    }

    fn desc(&self) -> &str {
        if OSCILLATOR {
            "SMI Ergodic Oscillator - SMI minus its signal line"
        } else {
            "SMI Ergodic Indicator - True strength ratio with a signal line"
        }
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        if from == 0 {
            self.tsi =
                TrueStrengthIndex::new(self.long_length, self.short_length, self.signal_length);
        }
        self.tsi.update(data, from);
        let start = from.min(self.values.len()).min(data.len());
        self.values.truncate(start);
        for bar in start..data.len() {
            let (Some(smi), Some(signal)) = (
                self.tsi.values().value(0, bar),
                self.tsi.values().value(1, bar),
            ) else {
                self.values.push(IndicatorValue::None);
                continue;
            };
            let (smi, signal) = (smi / 100.0, signal / 100.0);
            if OSCILLATOR {
                self.values.push(IndicatorValue::Single(smi - signal));
            } else {
                self.values.push_lines(&[smi, signal]);
            }
        }
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        self.colors.clone()
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if colors.len() >= self.line_count() {
            self.colors = colors;
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

    fn line_count(&self) -> usize {
        if OSCILLATOR { 1 } else { 2 }
    }

    fn line_names(&self) -> Vec<String> {
        let smi = format!("SMI({},{})", self.long_length, self.short_length);
        if OSCILLATOR {
            vec![format!("{smi} - Signal({})", self.signal_length)]
        } else {
            vec![smi, format!("Signal({})", self.signal_length)]
        }
    }
}
