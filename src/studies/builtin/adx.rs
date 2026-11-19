//! Average Directional Index (ADX) Indicator
//!
//! ADX measures trend strength regardless of direction. It's derived from the
//! Directional Movement Index (DMI) system which includes +DI and -DI.
//!
//! # Components
//! - ADX: Trend strength (0-100)
//! - +DI: Positive directional indicator
//! - -DI: Negative directional indicator
//!
//! # Interpretation
//! - ADX > 25: Strong trend
//! - ADX < 20: Weak or no trend
//! - +DI > -DI: Bullish trend
//! - -DI > +DI: Bearish trend
//!
//! # Example
//! ```
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{ADX, Indicator};
//!
//! let mut adx = ADX::new(14);
//! adx.calculate(&bars);
//! ```

use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{ATR, Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Average Directional Index indicator. Draws its ADX line; +DI and -DI are computed and can be shown
/// from its style settings.
///
/// The `period` and `adx_smoothing` inputs are independent; setting either leaves the other.
/// A template saved before the smoothing had an input of its own restores it equal to `period`.
#[derive(Clone, Debug)]
pub struct ADX {
    period: usize,
    /// Length of Wilder's average of DX that forms the ADX line.
    adx_smoothing: usize,
    values: IndicatorValues,
    stepper: Stepper<State>,
    adx_color: Color32,
    plus_di_color: Color32,
    minus_di_color: Color32,
    visible: bool,
}

#[derive(Clone, Debug, Default)]
struct State {
    previous: Option<[f64; 3]>,
    count: usize,
    plus: f64,
    minus: f64,
    tr: f64,
    dx_count: usize,
    adx: f64,
}

impl ADX {
    /// Create a new ADX indicator
    ///
    /// # Arguments
    /// * `period` - Lookback period (typically 14)
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            adx_smoothing: period.max(1),
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            adx_color: palette::RED,
            plus_di_color: palette::BLUE,
            minus_di_color: palette::ORANGE,
            visible: true,
        }
    }

    /// Set the length of Wilder's average of DX that forms the ADX line, clamped to at least one.
    /// `new` sets it to the DI period.
    pub fn with_smoothing(mut self, smoothing: usize) -> Self {
        self.adx_smoothing = smoothing.max(1);
        self
    }

    /// Set colors for all three lines
    pub fn with_colors(mut self, adx: Color32, plus_di: Color32, minus_di: Color32) -> Self {
        self.adx_color = adx;
        self.plus_di_color = plus_di;
        self.minus_di_color = minus_di;
        self
    }

    /// Wilder's smoothing
    #[inline]
    fn wilders_smooth(prev: f64, current: f64, period: usize) -> f64 {
        prev - (prev / period as f64) + current
    }
}

/// Construct with the conventional default parameters.
impl Default for ADX {
    fn default() -> Self {
        Self::new(14)
    }
}

impl Indicator for ADX {
    crate::studies::inputs::study_inputs!(self;
        period: length("DI period", 1, usize::MAX),
        adx_smoothing: length("ADX smoothing period", 1, usize::MAX)
    ; {});

    fn plot_style(&self, line: usize) -> crate::studies::PlotStyle {
        crate::studies::PlotStyle {
            color: self.colors().get(line).copied().unwrap_or(Color32::WHITE),
            width: self.line_style(line).width,
            pattern: crate::model::LineStyle::Solid,
            plot_type: crate::studies::PlotType::Line,
            // +DI and -DI are available in the style settings.
            visible: line == 0,
        }
    }

    fn name(&self) -> &str {
        "ADX"
    }

    fn desc(&self) -> &str {
        "Average Directional Index - Trend strength indicator"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                if !bar.high.is_finite() || !bar.low.is_finite() || !bar.close.is_finite() {
                    return IndicatorValue::None;
                }
                let Some(previous) = state.previous.replace([bar.high, bar.low, bar.close]) else {
                    return IndicatorValue::None;
                };
                state.count += 1;
                let up = bar.high - previous[0];
                let down = previous[1] - bar.low;
                let plus = if up > down && up > 0.0 { up } else { 0.0 };
                let minus = if down > up && down > 0.0 { down } else { 0.0 };
                let tr = ATR::true_range(bar, previous[2]);
                if state.count <= self.period {
                    state.plus += plus;
                    state.minus += minus;
                    state.tr += tr;
                    if state.count < self.period {
                        return IndicatorValue::None;
                    }
                } else {
                    state.plus = Self::wilders_smooth(state.plus, plus, self.period);
                    state.minus = Self::wilders_smooth(state.minus, minus, self.period);
                    state.tr = Self::wilders_smooth(state.tr, tr, self.period);
                }
                let (plus, minus) = if state.tr != 0.0 {
                    (
                        100.0 * state.plus / state.tr,
                        100.0 * state.minus / state.tr,
                    )
                } else {
                    (0.0, 0.0)
                };
                let sum = plus + minus;
                let dx = if sum.abs() > 1e-10 {
                    100.0 * (plus - minus).abs() / sum
                } else {
                    0.0
                };
                if state.dx_count < self.adx_smoothing {
                    state.dx_count += 1;
                    state.adx += dx;
                    if state.dx_count < self.adx_smoothing {
                        return IndicatorValue::Multiple(vec![f64::NAN, plus, minus]);
                    }
                    state.adx /= self.adx_smoothing as f64;
                } else {
                    state.adx = (state.adx * (self.adx_smoothing - 1) as f64 + dx)
                        / self.adx_smoothing as f64;
                }
                IndicatorValue::Multiple(vec![state.adx, plus, minus])
            });
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        vec![self.adx_color, self.plus_di_color, self.minus_di_color]
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if !colors.is_empty() {
            self.adx_color = colors[0];
        }
        if colors.len() > 1 {
            self.plus_di_color = colors[1];
        }
        if colors.len() > 2 {
            self.minus_di_color = colors[2];
        }
    }

    fn is_overlay(&self) -> bool {
        false // ADX is drawn in separate pane
    }

    fn line_count(&self) -> usize {
        3 // ADX, +DI, -DI
    }

    fn draws_only_plots(&self) -> bool {
        true
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
            if self.adx_smoothing == self.period {
                format!("ADX({})", self.period)
            } else {
                format!("ADX({},{})", self.period, self.adx_smoothing)
            },
            "+DI".to_string(),
            "-DI".to_string(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use crate::studies::{ConfiguredStudy, Indicator, IndicatorFactory};

    #[test]
    fn a_new_adx_draws_only_its_adx_line() {
        for (name, visible) in [("ADX", [true, false, false]), ("DMI", [true; 3])] {
            let study = ConfiguredStudy::new(IndicatorFactory::new().create(name).unwrap());
            assert_eq!(
                visible,
                [0, 1, 2].map(|line| study.plot_style(line).visible)
            );
        }
    }

    /// The DI period and the ADX smoothing are separate inputs: set in either order, or the
    /// period alone after `with_smoothing`, both keep their values and the smoothing drives the
    /// ADX. Bars rise by one to bar 19 and fall by one after, each with a true range of 1.5, so
    /// with a DI period of 10 DX is 100 on bars 10 to 19 and 100 |2 (0.9^m) - 1| on the m-th
    /// falling bar; the 20-bar ADX starts on bar 29 at (1000 + 314.43359218) / 20.
    #[test]
    fn the_di_period_leaves_the_adx_smoothing() {
        use crate::model::Bar;
        use crate::studies::{ADX, DirectionalMovement, InputValue};
        use chrono::{TimeZone, Utc};
        let bars: Vec<Bar> = (0..31i64)
            .map(|i| {
                let low = (if i <= 19 { i } else { 38 - i }) as f64;
                let time = Utc.timestamp_opt(i * 60, 0).unwrap();
                Bar::new(time, low + 0.5, low + 1.0, low, low + 0.5, 1.0)
            })
            .collect();
        type Build = fn() -> Box<dyn Indicator>;
        type Applied = &'static [(&'static str, usize)];
        let adx: Build = || Box::new(ADX::default());
        let adx_20: Build = || Box::new(ADX::new(14).with_smoothing(20));
        let dmi: Build = || Box::new(DirectionalMovement::default());
        let dmi_20: Build = || Box::new(DirectionalMovement::new(14).with_smoothing(20));
        let smoothing_first: Applied = &[("adx_smoothing", 20), ("period", 10)];
        let period_first: Applied = &[("period", 10), ("adx_smoothing", 20)];
        let period: Applied = &[("period", 10)];
        // (study, inputs set in order, plot of the ADX line)
        for (build, applied, line) in [
            (adx, smoothing_first, 0),
            (adx, period_first, 0),
            (adx_20, period, 0),
            (dmi, smoothing_first, 2),
            (dmi, period_first, 2),
            (dmi_20, period, 2),
        ] {
            let mut study = build();
            for &(id, value) in applied {
                study.set_input(id, InputValue::Length(value)).unwrap();
            }
            let name = format!("{} {applied:?}", study.name());
            let inputs: Vec<_> = study
                .inputs()
                .into_iter()
                .map(|input| (input.id, input.value))
                .collect();
            assert_eq!(
                inputs,
                [
                    ("period".to_owned(), InputValue::Length(10)),
                    ("adx_smoothing".to_owned(), InputValue::Length(20)),
                ],
                "{name}"
            );
            study.calculate(&bars);
            assert_eq!(study.values().value(line, 28), None, "{name}");
            for (bar, expected) in [(29, 65.721679609), (30, 64.29748966765)] {
                let value = study.values().value(line, bar).unwrap();
                assert!((value - expected).abs() < 1e-9, "{name} bar {bar}: {value}");
            }
        }
    }
}
