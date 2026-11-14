//! Williams %R Indicator
//!
//! Williams %R is a momentum oscillator that measures overbought/oversold levels.
//! It's the inverse of the Fast Stochastic Oscillator, ranging from -100 to 0.
//!
//! # Interpretation
//! - Readings from -20 to 0: Overbought
//! - Readings from -100 to -80: Oversold
//!
//! The pane draws both levels, dashed and muted, with the band between them filled in the study's
//! colour at a tenth of full opacity.
//!
//! # Calculation
//! %R = (Highest High - Source) / (Highest High - Lowest Low) * -100
//!
//! The source is the close by default. Another source replaces the close alone, so the highest
//! high and lowest low stay those of the bars, and a source outside their range gives a value
//! outside -100 to 0.
//!
//! # Example
//! ```
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{WilliamsR, Indicator};
//!
//! let mut williams = WilliamsR::new(14);
//! williams.calculate(&bars);
//! ```

use super::smoothing::{SMOOTHING_COLORS, Smoothed, SmoothingType};
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;
use std::collections::VecDeque;

/// Williams %R indicator
#[derive(Clone)]
pub struct WilliamsR {
    period: usize,
    smoothing: SmoothingType,
    smoothing_length: usize,
    smoothing_deviation: f64,
    values: IndicatorValues,
    stepper: Stepper<Rolling>,
    color: Color32,
    smoothing_colors: [Color32; 3],
    visible: bool,
}

/// The highs and lows of the last `period` bars with a finite high and low, and the average of %R.
#[derive(Clone, Default)]
struct Rolling {
    window: VecDeque<(f64, f64)>,
    smoothed: Smoothed,
}

impl WilliamsR {
    /// Create a new Williams %R indicator
    ///
    /// # Arguments
    /// * `period` - Lookback period (typically 14)
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            smoothing: SmoothingType::None,
            smoothing_length: 14,
            smoothing_deviation: 2.0,
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            color: palette::PURPLE,
            smoothing_colors: SMOOTHING_COLORS,
            visible: true,
        }
    }

    /// Set the indicator color
    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }
}

/// Construct with the conventional default parameters.
impl Default for WilliamsR {
    fn default() -> Self {
        Self::new(14)
    }
}

impl Indicator for WilliamsR {
    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX),
        smoothing: choice("Smoothing", SmoothingType::ALL),
        smoothing_length: length("Smoothing length", 1, usize::MAX)
            if smoothing in SmoothingType::WITH_LENGTH,
        smoothing_deviation: number("Band deviation", 0.0, f64::MAX)
            if smoothing in SmoothingType::WITH_BANDS
    ; {});

    fn value_format(&self) -> crate::scales::StudyFormat {
        crate::scales::StudyFormat::Percent
    }

    fn name(&self) -> &str {
        "Williams %R"
    }

    fn desc(&self) -> &str {
        "Williams %R - Momentum oscillator (-100 to 0)"
    }

    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::Close,
        ))
    }

    /// A source stands in for the close and leaves each bar's high and low.
    fn update_source(&mut self, data: &[Bar], from: usize, source: &[f64]) {
        let bars: Vec<_> = data
            .iter()
            .enumerate()
            .map(|(at, bar)| Bar {
                close: source.get(at).copied().unwrap_or(f64::NAN),
                ..bar.clone()
            })
            .collect();
        self.update(&bars, from);
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        let (period, kind, length, deviation) = (
            self.period,
            self.smoothing,
            self.smoothing_length,
            self.smoothing_deviation,
        );
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                if !(bar.high.is_finite() && bar.low.is_finite()) {
                    return IndicatorValue::None;
                }
                state.window.push_back((bar.high, bar.low));
                if state.window.len() > period {
                    state.window.pop_front();
                }
                // A gap in the source leaves the bars' range running.
                if state.window.len() < period || !bar.close.is_finite() {
                    return IndicatorValue::None;
                }
                let highest = state.window.iter().fold(f64::MIN, |max, w| max.max(w.0));
                let lowest = state.window.iter().fold(f64::MAX, |min, w| min.min(w.1));
                let range = highest - lowest;
                let r = if range == 0.0 {
                    -50.0 // Middle value
                } else {
                    (highest - bar.close) / range * -100.0
                };
                state.smoothed.next(kind, length, deviation, r, bar.volume)
            });
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        let [average, upper, lower] = self.smoothing_colors;
        vec![self.color, average, upper, lower]
    }

    fn fills(&self) -> Vec<crate::studies::StudyFill> {
        self.smoothing.fills()
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if !colors.is_empty() {
            self.color = colors[0];
            for (slot, color) in self.smoothing_colors.iter_mut().zip(&colors[1..]) {
                *slot = *color;
            }
        }
    }

    fn level_fill_color(&self) -> Color32 {
        self.color.gamma_multiply_u8(26)
    }

    fn pane(&self) -> crate::studies::StudyPane {
        crate::studies::StudyPane {
            height: None,
            // A fitted pane's margin around -100 to 0, so a reading at either end stays whole.
            range: Some((-110.0, 10.0)),
            levels: vec![
                (-20.0, palette::MUTED, "Overbought"),
                (-80.0, palette::MUTED, "Oversold"),
            ],
            fill_between_levels: true,
        }
    }

    fn is_overlay(&self) -> bool {
        false // Williams %R is drawn in separate pane
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
        let mut names = vec![format!("%R({})", self.period)];
        names.extend(self.smoothing.line_names("%R MA"));
        names
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{LineStyle, PriceSource};
    use crate::studies::{ConfiguredStudy, InputValue, StudySource};

    /// Highs 10, 12, 11 and lows 8, 9, 7, so the two-bar windows of bars 1 and 2 span 12 to 8 and
    /// 12 to 7. The closes lie away from any source below.
    fn bars() -> Vec<Bar> {
        [(10.0, 8.0, 9.5), (12.0, 9.0, 11.5), (11.0, 7.0, 7.5)]
            .into_iter()
            .enumerate()
            .map(|(i, (high, low, close))| {
                let time = chrono::DateTime::from_timestamp(i as i64 * 60, 0).unwrap();
                Bar::new(time, high, high, low, close, 1.0)
            })
            .collect()
    }

    /// A source stands in for the close alone. By hand, with the median price of 10.5 on bar 1 and
    /// 9 on bar 2: (12 - 10.5) / 4 * -100 = -37.5 and (12 - 9) / 5 * -100 = -60. Another study's
    /// output keeps the bars' range through its warm-up and is not limited to it: 14 on bar 1
    /// gives (12 - 14) / 4 * -100 = 50, and correcting bar 2 to 12 gives 0.
    #[test]
    fn a_source_replaces_the_close_and_keeps_the_range_of_the_bars() {
        let bars = bars();
        let mut study = ConfiguredStudy::new(Box::new(WilliamsR::new(2)));
        study
            .set_input(
                "source",
                InputValue::Source(StudySource::Price(PriceSource::HL2)),
            )
            .unwrap();
        study.calculate(&bars);
        assert_eq!(study.values().value(0, 1), Some(-37.5));
        assert_eq!(study.values().value(0, 2), Some(-60.0));

        let mut study = WilliamsR::new(2);
        study.update_source(&bars, 0, &[f64::NAN, 14.0, 9.0]);
        assert_eq!(study.values().value(0, 0), None);
        assert_eq!(study.values().value(0, 1), Some(50.0));
        assert_eq!(study.values().value(0, 2), Some(-60.0));
        study.update_source(&bars, 2, &[f64::NAN, 14.0, 12.0]);
        assert_eq!(study.values().value(0, 2).map(f64::abs), Some(0.0));
    }

    /// The study opens with dashed one-pixel bands at -20 and -80, filled in its colour at a tenth
    /// of full opacity, and a closing-price source.
    #[test]
    fn opens_with_its_bands_fill_and_source() {
        let study = ConfiguredStudy::new(Box::new(WilliamsR::default()));
        let settings = study.settings();
        assert_eq!(
            study.pane().levels,
            [
                (-20.0, palette::MUTED, "Overbought"),
                (-80.0, palette::MUTED, "Oversold"),
            ]
        );
        assert_eq!(study.pane().range, Some((-110.0, 10.0)));
        for level in &settings.levels {
            assert_eq!(
                (level.pattern, level.width, level.visible),
                (LineStyle::Dashed, 1.0, true)
            );
        }
        assert!(settings.fill_between_levels);
        assert_eq!(
            settings.level_fill_color,
            Color32::from_rgba_unmultiplied(126, 87, 194, 26)
        );
        assert_eq!(study.source(), Some(StudySource::Price(PriceSource::Close)));
    }
}
