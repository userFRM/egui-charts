//! Median
//!
//! A short rolling median of a price source with an EMA of it and average-true-range bands,
//! for reading direction (the median against its EMA) and volatility (the band width) together.
//!
//! # Formula
//! - median = the median of the source, the bar midpoint (high + low) / 2 by default, over the
//!   last `n` bars (3 by default), taken by nearest rank: the `ceil(n / 2)`-th smallest value,
//!   so the lower middle one for an even `n`
//! - Median EMA = EMA(median, n), starting with the simple mean of the first `n` medians
//! - upper and lower bands = median +/- k * ATR(m), Wilder's average true range with `m` = 14
//!   and `k` = 2 by default, always measured on the bars' own high, low and close
//!
//! A bar with a non-finite high, low or close, or without a source value, has no values and
//! is left out of the window, so the median spans the last `n` bars that have one.
//!
//! The band between the median and its EMA is filled green while the median is at or above
//! its EMA and violet below.
//!
//! # Example
//! ```
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{Indicator, Median};
//!
//! use egui_charts::model::PriceSource;
//!
//! let mut median = Median::new(3, 14, 2.0).with_source(PriceSource::Close);
//! median.calculate(&bars);
//! ```

use super::clamp_param;
use super::smoothing::Smoothing;
use crate::model::{Bar, PriceSource};
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{ATR, Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Median: rolling median of a price source, its EMA and ATR bands around it.
#[derive(Clone)]
pub struct Median {
    length: usize,
    atr_length: usize,
    multiplier: f64,
    source: PriceSource,
    values: IndicatorValues,
    atr: ATR,
    stepper: Stepper<(usize, std::collections::VecDeque<f64>, Smoothing)>,
    /// Median, upper band, lower band, Median EMA, rising fill, falling fill.
    colors: [Color32; 6],
    visible: bool,
}

impl Median {
    /// Create the study from the median length (typically 3), the ATR length (typically 14)
    /// and the ATR multiplier (typically 2, clamped into 0 to `f64::MAX`, NaN to 0). The median
    /// reads the bar midpoint until [`with_source`](Self::with_source) selects another price.
    pub fn new(length: usize, atr_length: usize, multiplier: f64) -> Self {
        Self {
            length: length.max(1),
            atr_length: atr_length.max(1),
            multiplier: clamp_param(multiplier, 0.0, f64::MAX),
            source: PriceSource::HL2,
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

    /// Take the median of `source` instead of the bar midpoint. The bands keep using the bars.
    pub fn with_source(mut self, source: PriceSource) -> Self {
        self.source = source;
        self
    }

    fn update_prices(&mut self, data: &[Bar], from: usize, source: Option<&[f64]>) {
        if from == 0 {
            self.atr = ATR::new(self.atr_length);
        }
        self.atr.update(data, from);
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                let index = state.0;
                state.0 += 1;
                let price = source.map_or_else(
                    || self.source.compute(bar.open, bar.high, bar.low, bar.close),
                    |source| source.get(index).copied().unwrap_or(f64::NAN),
                );
                if ![price, bar.high, bar.low, bar.close]
                    .iter()
                    .all(|x| x.is_finite())
                {
                    return IndicatorValue::None;
                }
                state.1.push_back(price);
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
}

/// Construct with the conventional default parameters.
impl Default for Median {
    fn default() -> Self {
        Self::new(3, 14, 2.0)
    }
}

impl Indicator for Median {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(self.source))
    }

    /// Only the median uses the aligned source; the bands retain the bars' own true range.
    fn update_source(&mut self, data: &[Bar], from: usize, source: &[f64]) {
        self.update_prices(data, from, Some(source));
    }

    fn line_style(&self, line: usize) -> crate::studies::IndicatorLineStyle {
        crate::studies::IndicatorLineStyle {
            width: if line == 0 { 3.0 } else { 1.0 },
            ..Default::default()
        }
    }

    crate::studies::inputs::study_inputs!(self;
        length: length("Median length", 1, usize::MAX),
        atr_length: length("ATR length", 1, usize::MAX),
        multiplier: number("ATR multiplier", 0.0, f64::MAX),
        source: choice("Median source", PriceSource::all())
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
        "Median - Rolling median of a price source with its EMA and ATR bands"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        self.update_prices(data, from, None);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::studies::{ConfiguredStudy, InputValue, StudySource, StudyTemplate};
    use chrono::{TimeZone, Utc};

    /// Median, upper band, lower band and Median EMA of each bar.
    fn rows(study: &dyn Indicator) -> Vec<[Option<f64>; 4]> {
        (0..study.values().len())
            .map(|bar| std::array::from_fn(|plot| study.values().value(plot, bar)))
            .collect()
    }

    /// The rows of `medians` and `emas` with the bands of the test bars around each median.
    fn want(medians: [Option<f64>; 6], emas: [Option<f64>; 6]) -> Vec<[Option<f64>; 4]> {
        // Twice Wilder's ATR(2) of the true ranges 4, 4, 6, 4, 4, 6: 4, 5, 4.5, 4.25, 5.125.
        let bands = [f64::NAN, 8.0, 10.0, 9.0, 8.5, 10.25];
        (0..6)
            .map(|bar| match medians[bar] {
                Some(median) => [
                    Some(median),
                    Some(median + bands[bar]),
                    Some(median - bands[bar]),
                    emas[bar],
                ],
                None => [None; 4],
            })
            .collect()
    }

    fn assert_rows(got: &[[Option<f64>; 4]], want: &[[Option<f64>; 4]], case: &str) {
        assert_eq!(got.len(), want.len(), "{case}");
        for (bar, (got, want)) in got.iter().zip(want).enumerate() {
            for (got, want) in got.iter().zip(want) {
                match (got, want) {
                    (Some(got), Some(want)) => {
                        assert!(
                            (got - want).abs() < 1e-12,
                            "{case}, bar {bar}: {got} {want}"
                        )
                    }
                    _ => assert_eq!(got, want, "{case}, bar {bar}"),
                }
            }
        }
    }

    /// Length 3, ATR 2 and multiplier 2 over six bars whose open, midpoint and close differ,
    /// worked by hand. The midpoints 10, 13, 14, 12, 15, 13 have the medians 13, 13, 14, 13 from
    /// bar 2 and the closes 11, 14, 12, 13, 14, 12 have 12, 13, 13, 13; each EMA starts at the
    /// mean of the first three medians and then moves half way to the next. Bar 3 has no open,
    /// so the opens 10, 11, 14, 13, 14 skip it: 11, then 13, then 14 for 14, 13, 14, where the
    /// repeated value is the middle one, and the EMA starts on bar 5. The bands keep the true
    /// range of the bars under every source. Correcting the open of bar 1 from 11 to 15 gives
    /// the opens the medians 14, 14, 14 and leaves the other sources as they were. A template
    /// of the edited study restores the same source.
    #[test]
    fn the_source_sets_the_median_and_the_bands_keep_the_bars() {
        let mut bars: Vec<Bar> = [
            (10.0, 12.0, 8.0, 11.0),
            (11.0, 15.0, 11.0, 14.0),
            (14.0, 17.0, 11.0, 12.0),
            (f64::NAN, 14.0, 10.0, 13.0),
            (13.0, 17.0, 13.0, 14.0),
            (14.0, 16.0, 10.0, 12.0),
        ]
        .into_iter()
        .enumerate()
        .map(|(i, (open, high, low, close))| {
            let time = Utc.timestamp_opt(i as i64 * 60, 0).unwrap();
            Bar::new(time, open, high, low, close, 1.0)
        })
        .collect();
        type Row = (PriceSource, [Option<f64>; 6], [Option<f64>; 6]);
        let midpoint: Row = (
            PriceSource::HL2,
            [None, None, Some(13.0), Some(13.0), Some(14.0), Some(13.0)],
            [None, None, None, None, Some(40.0 / 3.0), Some(79.0 / 6.0)],
        );
        let close: Row = (
            PriceSource::Close,
            [None, None, Some(12.0), Some(13.0), Some(13.0), Some(13.0)],
            [None, None, None, None, Some(38.0 / 3.0), Some(77.0 / 6.0)],
        );
        let open: Row = (
            PriceSource::Open,
            [None, None, Some(11.0), None, Some(13.0), Some(14.0)],
            [None, None, None, None, None, Some(38.0 / 3.0)],
        );
        let corrected: Row = (
            PriceSource::Open,
            [None, None, Some(14.0), None, Some(14.0), Some(14.0)],
            [None, None, None, None, None, Some(14.0)],
        );
        for (first, second) in [(midpoint, midpoint), (close, close), (open, corrected)] {
            let source = first.0;
            let mut built = Median::new(3, 2, 2.0).with_source(source);
            let mut edited = ConfiguredStudy::new(Box::new(Median::new(3, 2, 2.0)));
            edited
                .set_input("source", InputValue::Source(StudySource::Price(source)))
                .unwrap();
            bars[1].open = 11.0;
            built.calculate(&bars);
            edited.calculate(&bars);
            let expected = want(first.1, first.2);
            assert_rows(&rows(&built), &expected, &format!("{source:?} built"));
            assert_rows(&rows(&edited), &expected, &format!("{source:?} edited"));
            bars[1].open = 15.0;
            built.update(&bars, 1);
            edited.update(&bars, 1);
            let expected = want(second.1, second.2);
            assert_rows(
                &rows(&built),
                &expected,
                &format!("{source:?} built, corrected"),
            );
            assert_rows(
                &rows(&edited),
                &expected,
                &format!("{source:?} edited, corrected"),
            );
            let mut restored = StudyTemplate::from_study(&edited).create().unwrap();
            restored.calculate(&bars);
            assert_rows(
                &rows(restored.as_ref()),
                &expected,
                &format!("{source:?} restored"),
            );
        }
        let editor = ConfiguredStudy::new(Box::new(Median::default())).inputs();
        let input = editor.iter().find(|input| input.id == "source").unwrap();
        assert_eq!(input.label, "Median source");
        assert_eq!(
            input.default,
            InputValue::Source(StudySource::Price(PriceSource::HL2))
        );
    }
}
