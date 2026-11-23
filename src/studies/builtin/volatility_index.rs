//! Volatility Index
//!
//! Welles Wilder's volatility system (New Concepts in Technical Trading Systems, 1978): a
//! stop-and-reverse level a multiple of the average true range away from the significant close,
//! the most favourable close of the current position.
//!
//! # Formula
//! With the true range `TR` (high minus low on the first bar), `ATR` its Wilder average (alpha
//! 1/n) or exponential average (alpha 2/(n+1)), both starting at the mean of the first `n` true
//! ranges, and `ARC = k * ATR` (n = 10 and k = 3 by default):
//! - the significant close `SIC` is the highest close since the position turned long, or the
//!   lowest since it turned short
//! - a long position has `SAR = SIC - ARC`, a short one `SAR = SIC + ARC`
//! - a close below the previous bar's SAR while long, or above it while short, reverses the
//!   position and restarts `SIC` at that close; a close at the SAR keeps the position
//!
//! The first value is on the `n`-th usable bar, long when its close is at or above the first
//! usable close and short otherwise. A bar without a finite high, low and close has no value and
//! leaves the system unchanged.
//!
//! # Example
//! ```
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{Indicator, VolatilityIndex, VolatilityIndexSmoothing};
//!
//! let mut index = VolatilityIndex::new(10, 3.0).with_smoothing(VolatilityIndexSmoothing::Exponential);
//! index.calculate(&bars);
//! ```

use crate::model::Bar;
use crate::studies::stepper::Stepper;
use crate::studies::{ATR, Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// The average of the true range a [`VolatilityIndex`] multiplies. The `Debug` names are the
/// choices its method input lists.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum VolatilityIndexSmoothing {
    /// Alpha 2/(period+1), starting at the mean of the first `period` ranges.
    Exponential,
    /// Wilder's average, alpha 1/period, starting at the mean of the first `period` ranges.
    #[default]
    Wilder,
}

impl VolatilityIndexSmoothing {
    /// The label study inputs show for this choice.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Exponential => "Exponential",
            Self::Wilder => "Wilder Smoothing",
        }
    }
}

impl std::fmt::Debug for VolatilityIndexSmoothing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// Volatility Index: Wilder's stop and reverse around the significant close.
#[derive(Clone, Debug)]
pub struct VolatilityIndex {
    period: usize,
    multiplier: f64,
    smoothing: VolatilityIndexSmoothing,
    values: IndicatorValues,
    stepper: Stepper<State>,
    color: Color32,
    visible: bool,
}

#[derive(Clone, Debug, Default)]
struct State {
    /// The close of the last usable bar.
    previous: Option<f64>,
    first: f64,
    usable: usize,
    atr: f64,
    long: bool,
    sic: f64,
    sar: f64,
}

impl VolatilityIndex {
    /// Create with the ATR period (typically 10, clamped into 2 to 10000) and the ATR
    /// multiplier (typically 3, clamped into 0 to 10000, NaN to 0), over Wilder's average.
    pub fn new(period: usize, multiplier: f64) -> Self {
        Self {
            period: period.clamp(2, 10_000),
            multiplier: if multiplier.is_nan() {
                0.0
            } else {
                multiplier.clamp(0.0, 10_000.0)
            },
            smoothing: VolatilityIndexSmoothing::Wilder,
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            color: crate::studies::palette::HISTOGRAM_RED,
            visible: true,
        }
    }

    /// Choose the average of the true range; Wilder's by default.
    pub fn with_smoothing(mut self, smoothing: VolatilityIndexSmoothing) -> Self {
        self.smoothing = smoothing;
        self
    }
}

/// Construct with the conventional default parameters.
impl Default for VolatilityIndex {
    fn default() -> Self {
        Self::new(10, 3.0)
    }
}

impl Indicator for VolatilityIndex {
    crate::studies::inputs::study_inputs!(self;
        period: length("Periods", 2, 10_000),
        multiplier: number("ATR multiplier", 0.0, 10_000.0),
        smoothing: choice("Method", [VolatilityIndexSmoothing::Exponential, VolatilityIndexSmoothing::Wilder])
    ; {});

    fn name(&self) -> &str {
        "Volatility Index"
    }

    fn desc(&self) -> &str {
        "Volatility Index - Wilder's ATR stop and reverse around the significant close"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        let (period, multiplier) = (self.period, self.multiplier);
        let alpha = match self.smoothing {
            VolatilityIndexSmoothing::Exponential => 2.0 / (period as f64 + 1.0),
            VolatilityIndexSmoothing::Wilder => 1.0 / period as f64,
        };
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                if ![bar.high, bar.low, bar.close].iter().all(|p| p.is_finite()) {
                    return IndicatorValue::None;
                }
                let close = bar.close;
                let tr = match state.previous {
                    Some(previous) => ATR::true_range(bar, previous),
                    None => {
                        state.first = close;
                        bar.high - bar.low
                    }
                };
                state.previous = Some(close);
                state.usable += 1;
                if state.usable <= period {
                    state.atr += tr;
                    if state.usable < period {
                        return IndicatorValue::None;
                    }
                    state.atr /= period as f64;
                    state.long = close >= state.first;
                    state.sic = close;
                } else {
                    state.atr += alpha * (tr - state.atr);
                    if (state.long && close < state.sar) || (!state.long && close > state.sar) {
                        state.long = !state.long;
                        state.sic = close;
                    } else if state.long {
                        state.sic = state.sic.max(close);
                    } else {
                        state.sic = state.sic.min(close);
                    }
                }
                let arc = multiplier * state.atr;
                state.sar = if state.long {
                    state.sic - arc
                } else {
                    state.sic + arc
                };
                IndicatorValue::Single(state.sar)
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
        vec!["Volatility Index".into()]
    }

    fn draws_only_plots(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use VolatilityIndexSmoothing::{Exponential, Wilder};

    /// Asymmetric bars with an unusable bar, a close exactly at the previous stop that keeps the
    /// position, reversals both ways, then the second bar's close corrected so the system opens
    /// short. Expected values are Wilder's volatility system evaluated independently in exact
    /// rational arithmetic, `None` before the period-th usable bar and on the unusable one.
    #[test]
    fn follows_wilders_volatility_system_across_gaps_ties_and_corrections() {
        let bars = |second_close: f64| -> Vec<Bar> {
            [
                (10.0, 12.0, 9.0, 11.0),
                (11.0, 13.0, 10.0, second_close),
                (12.0, 14.0, 11.0, f64::NAN),
                (12.0, 14.0, 11.0, 13.0),
                (13.0, 13.5, 9.5, 10.0),
                (10.0, 10.5, 8.0, 8.5),
                (8.5, 9.0, 7.0, 7.5),
                (8.0, 11.0, 8.0, 10.5),
            ]
            .map(|(o, h, l, c)| Bar::new(chrono::DateTime::UNIX_EPOCH, o, h, l, c, 1.0))
            .to_vec()
        };
        type Row = (
            usize,
            f64,
            VolatilityIndexSmoothing,
            [Option<f64>; 8],
            [Option<f64>; 8],
        );
        let n = None;
        #[rustfmt::skip]
        let rows: [Row; 3] = [
            (2, 1.0, Wilder,
                [n, Some(9.0), n, Some(10.0), Some(9.5), Some(11.5), Some(10.0), Some(7.5)],
                [n, Some(13.5), n, Some(13.75), Some(13.625), Some(11.5625), Some(10.03125), Some(7.484375)]),
            (3, 2.0, Exponential,
                [n, n, n, Some(7.0), Some(6.0), Some(7.0), Some(8.0), Some(7.0)],
                [n, n, n, Some(20.0 / 3.0), Some(35.0 / 6.0), Some(83.0 / 12.0), Some(191.0 / 24.0), Some(335.0 / 48.0)]),
            (3, 2.0, Wilder,
                [n, n, n, Some(7.0), Some(19.0 / 3.0), Some(62.0 / 9.0), Some(205.0 / 27.0), Some(572.0 / 81.0)],
                [n, n, n, Some(20.0 / 3.0), Some(55.0 / 9.0), Some(182.0 / 27.0), Some(607.0 / 81.0), Some(1700.0 / 243.0)]),
        ];
        for (period, multiplier, smoothing, as_saved, corrected) in rows {
            let mut study = VolatilityIndex::new(period, multiplier).with_smoothing(smoothing);
            let label = format!("{period}, {multiplier}, {smoothing:?}");
            study.calculate(&bars(12.0));
            for (expected, data) in [(as_saved, None), (corrected, Some(bars(10.5)))] {
                if let Some(data) = data {
                    study.update(&data, 1);
                }
                for (bar, want) in expected.into_iter().enumerate() {
                    let got = study.values().value(0, bar);
                    assert!(
                        got.zip(want)
                            .map_or(got == want, |(g, w)| (g - w).abs() < 1e-12),
                        "{label} bar {bar}: {got:?} != {want:?}"
                    );
                }
            }
        }
    }
}
