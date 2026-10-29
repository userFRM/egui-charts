//! Median
//!
//! A short rolling median of the bar midpoint with an EMA of it and average-true-range bands,
//! for reading direction (the median against its EMA) and volatility (the band width) together.
//!
//! # Formula
//! - median = the median of (high + low) / 2 over the last `n` bars (3 by default), taken by
//!   nearest rank: the `ceil(n / 2)`-th smallest value, so the lower middle one for an even `n`
//! - Median EMA = EMA(median, n), starting with the simple mean of the first `n` medians
//! - upper and lower bands = median +/- k * ATR(m), Wilder's average true range with `m` = 14
//!   and `k` = 2 by default
//!
//! The band between the median and its EMA is filled green while the median is at or above
//! its EMA and violet below.
//!
//! # Example
//! ```
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{Indicator, Median};
//!
//! let mut median = Median::new(3, 14, 2.0);
//! median.calculate(&bars);
//! ```

use super::clamp_param;
use super::smoothing::Smoothing;
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{ATR, Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Median: rolling median of hl2, its EMA and ATR bands around it.
#[derive(Clone)]
pub struct Median {
    length: usize,
    atr_length: usize,
    multiplier: f64,
    values: IndicatorValues,
    atr: ATR,
    stepper: Stepper<(usize, std::collections::VecDeque<f64>, Smoothing)>,
    /// Median, upper band, lower band, Median EMA, rising fill, falling fill.
    colors: [Color32; 6],
    visible: bool,
}

impl Median {
    /// Create the study from the median length (typically 3), the ATR length (typically 14)
    /// and the ATR multiplier (typically 2, clamped into 0 to `f64::MAX`, NaN to 0).
    pub fn new(length: usize, atr_length: usize, multiplier: f64) -> Self {
        Self {
            length: length.max(1),
            atr_length: atr_length.max(1),
            multiplier: clamp_param(multiplier, 0.0, f64::MAX),
            values: IndicatorValues::new(),
            atr: ATR::new(atr_length),
            stepper: Stepper::default(),
            colors: [
                palette::RED,
                Color32::from_rgb(0, 230, 118),
                Color32::from_rgb(224, 64, 251),
                palette::BLUE,
                Color32::from_rgb(0, 230, 118),
                Color32::from_rgb(224, 64, 251),
            ],
            visible: true,
        }
    }
}

/// Construct with the conventional default parameters.
impl Default for Median {
    fn default() -> Self {
        Self::new(3, 14, 2.0)
    }
}

impl Indicator for Median {
    fn line_style(&self, line: usize) -> crate::studies::IndicatorLineStyle {
        crate::studies::IndicatorLineStyle {
            width: if line == 0 { 3.0 } else { 1.0 },
            ..Default::default()
        }
    }

    crate::studies::inputs::study_inputs!(self;
        length: length("Median length", 1, usize::MAX),
        atr_length: length("ATR length", 1, usize::MAX),
        multiplier: number("ATR multiplier", 0.0, f64::MAX)
    ; {});

    fn fills(&self) -> Vec<crate::studies::StudyFill> {
        let alpha = |c: Color32| Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), 230);
        vec![crate::studies::StudyFill {
            first: crate::studies::FillBoundary::Plot(0),
            second: crate::studies::FillBoundary::Plot(3),
            color: alpha(self.colors[4]),
            inverted_color: Some(alpha(self.colors[5])),
        }]
    }

    fn name(&self) -> &str {
        "Median"
    }

    fn desc(&self) -> &str {
        "Median - Rolling median of the bar midpoint with its EMA and ATR bands"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        if from == 0 {
            self.atr = ATR::new(self.atr_length);
        }
        self.atr.update(data, from);
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                let index = state.0;
                state.0 += 1;
                if ![bar.high, bar.low, bar.close].iter().all(|x| x.is_finite()) {
                    return IndicatorValue::None;
                }
                state.1.push_back((bar.high + bar.low) / 2.0);
                if state.1.len() > self.length {
                    state.1.pop_front();
                }
                let median = if state.1.len() < self.length {
                    f64::NAN
                } else {
                    let mut window: Vec<_> = state.1.iter().copied().collect();
                    *window
                        .select_nth_unstable_by(self.length.div_ceil(2) - 1, f64::total_cmp)
                        .1
                };
                let ema = state
                    .2
                    .next(median, self.length, 2.0 / (self.length as f64 + 1.0));
                let band = self
                    .atr
                    .values()
                    .value(0, index)
                    .map_or(f64::NAN, |atr| self.multiplier * atr);
                IndicatorValue::Multiple(vec![median, median + band, median - band, ema])
            });
    }

    fn is_incremental(&self) -> bool {
        true
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
        vec![
            "Median".into(),
            "Upper Band".into(),
            "Lower Band".into(),
            "Median EMA".into(),
        ]
    }
}
