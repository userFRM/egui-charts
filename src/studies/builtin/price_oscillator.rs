use super::smoothing::Smoothing;
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;
use std::collections::VecDeque;

/// Average formula of the oscillator's two averages or of its signal line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OscillatorAverage {
    /// Alpha 2/(length+1), seeded with the mean of the first `length` values.
    Exponential,
    /// Arithmetic mean of the last `length` values.
    Simple,
}

/// Resumable [`OscillatorAverage`] over the finite values it is fed.
#[derive(Clone, Default)]
pub(super) struct Mean {
    exponential: Smoothing,
    window: VecDeque<f64>,
}

impl Mean {
    fn next(&mut self, kind: OscillatorAverage, input: f64, length: usize) -> f64 {
        match kind {
            OscillatorAverage::Exponential => {
                self.exponential
                    .next(input, length, 2.0 / (length as f64 + 1.0))
            }
            OscillatorAverage::Simple => {
                if !input.is_finite() {
                    return f64::NAN;
                }
                self.window.push_back(input);
                if self.window.len() > length {
                    self.window.pop_front();
                }
                if self.window.len() < length {
                    return f64::NAN;
                }
                self.window.iter().sum::<f64>() / length as f64
            }
        }
    }
}

/// Percentage Price Oscillator (PPO): MACD taken as a percentage of the long average.
///
/// - PPO = 100 * (MA(close, short) - MA(close, long)) / MA(close, long).
/// - Signal = MA(PPO, signal length), computed only over the bars where the PPO is defined.
///   An exponential average is seeded with the mean of its first `signal` values, like the
///   study's other averages, which are seeded with the mean of their first values; a simple
///   one is the mean of the last `signal` values.
/// - Histogram = PPO - signal.
///
/// MA is exponential by default; the oscillator's averages and the signal line each choose an
/// [`OscillatorAverage`] independently. With lengths 12, 26 and 9 a bar has no value before
/// bar 25, the PPO alone on bars 25 to 32, and from bar 33, the mean of the PPO on bars 25 to
/// 33 being the first signal, the PPO, the signal and the histogram, under either formula. A
/// bar with a non-finite close has no value and is skipped by the averages.
#[derive(Clone)]
pub struct PriceOscillator {
    /// Short EMA period (typically 12)
    short_period: usize,
    /// Long EMA period (typically 26)
    long_period: usize,
    /// Signal line EMA period (typically 9)
    signal_period: usize,
    /// Formula of the short and long averages
    oscillator_average: OscillatorAverage,
    /// Formula of the signal line
    signal_average: OscillatorAverage,
    values: IndicatorValues,
    stepper: Stepper<[Mean; 3]>,
    /// Colors: [PPO line, Signal line, Histogram]
    colors: Vec<Color32>,
    visible: bool,
}

impl PriceOscillator {
    /// Create with `short_period`, `long_period`, `signal_period` in bars, each clamped to at
    /// least one.
    pub fn new(short_period: usize, long_period: usize, signal_period: usize) -> Self {
        Self {
            short_period: short_period.max(1),
            long_period: long_period.max(1),
            signal_period: signal_period.max(1),
            oscillator_average: OscillatorAverage::Exponential,
            signal_average: OscillatorAverage::Exponential,
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            colors: vec![
                palette::BLUE,   // PPO
                palette::ORANGE, // Signal
                palette::HISTOGRAM_TEAL,
                palette::LIGHT_TEAL,
                palette::HISTOGRAM_RED,
                palette::LIGHT_RED, // Histogram
            ],
            visible: true,
        }
    }

    /// Choose the formula of the short and long averages and of the signal line.
    pub fn with_averages(
        mut self,
        oscillator: OscillatorAverage,
        signal: OscillatorAverage,
    ) -> Self {
        self.oscillator_average = oscillator;
        self.signal_average = signal;
        self
    }

    /// Set plot colors in the order returned by [`Indicator::line_names`].
    pub fn with_colors(mut self, colors: Vec<Color32>) -> Self {
        self.set_colors(colors);
        self
    }
}

/// Resume the percentage difference, its signal over defined values, and histogram.
pub(super) fn percentage_oscillator(
    state: &mut [Mean; 3],
    value: f64,
    lengths: [usize; 3],
    averages: [OscillatorAverage; 2],
) -> IndicatorValue {
    let [short, long, signal] = lengths;
    let short_value = state[0].next(averages[0], value, short);
    let long_value = state[1].next(averages[0], value, long);
    let line = if long_value == 0.0 {
        f64::NAN
    } else {
        (short_value - long_value) / long_value * 100.0
    };
    let signal = state[2].next(averages[1], line, signal);
    if line.is_finite() {
        IndicatorValue::Multiple(vec![line, signal, line - signal])
    } else {
        IndicatorValue::None
    }
}

/// Construct with the conventional default parameters.
impl Default for PriceOscillator {
    fn default() -> Self {
        Self::new(12, 26, 9)
    }
}

impl Indicator for PriceOscillator {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::Close,
        ))
    }

    crate::studies::inputs::study_inputs!(self;
        short_period: length("Short period", 1, usize::MAX),
        long_period: length("Long period", 1, usize::MAX),
        signal_period: length("Signal period", 1, usize::MAX),
        oscillator_average: choice("Oscillator MA", [OscillatorAverage::Exponential, OscillatorAverage::Simple]),
        signal_average: choice("Signal MA", [OscillatorAverage::Exponential, OscillatorAverage::Simple])
    ; {});

    fn plot_color(&self, line: usize, bar: usize) -> Option<Color32> {
        if line != 2 {
            return None;
        }
        super::macd::histogram_color(&self.values, bar, &self.colors[2..])
    }

    fn plot_style(&self, line: usize) -> crate::studies::PlotStyle {
        crate::studies::PlotStyle {
            color: self.colors[line],
            width: 1.0,
            pattern: crate::model::LineStyle::Solid,
            plot_type: if line == 2 {
                crate::studies::PlotType::Columns
            } else {
                crate::studies::PlotType::Line
            },
            visible: true,
        }
    }

    fn value_format(&self) -> crate::scales::StudyFormat {
        crate::scales::StudyFormat::Percent
    }

    fn name(&self) -> &str {
        "PPO"
    }

    fn desc(&self) -> &str {
        "Percentage Price Oscillator - Percentage difference between EMAs"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                if !bar.close.is_finite() {
                    return IndicatorValue::None;
                }
                percentage_oscillator(
                    state,
                    bar.close,
                    [self.short_period, self.long_period, self.signal_period],
                    [self.oscillator_average, self.signal_average],
                )
            });
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
        if colors.len() >= 3 {
            self.colors = colors;
            if self.colors.len() < 6 {
                self.colors.truncate(3);
            }
            self.colors.resize(6, self.colors[2]);
        } else {
            for (stored, color) in self.colors.iter_mut().zip(colors) {
                *stored = color;
            }
        }
    }

    fn set_color(&mut self, line: usize, color: Color32) {
        match line {
            0 | 1 => self.colors[line] = color,
            2 => self.colors[2..].fill(color),
            _ => {}
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
        3
    }

    fn line_names(&self) -> Vec<String> {
        vec![
            format!("PPO({},{})", self.short_period, self.long_period),
            format!("Signal({})", self.signal_period),
            "Histogram".to_string(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::studies::test_support::make_bar;

    /// Hand values for lengths 2, 3 and 2 on closes 10, 12, 11, 13, 14, for each pairing of the
    /// oscillator and signal formulas. The averages of 2 are 11, 11.5, 12, 13.5 (simple) and
    /// 11, 11, 37/3, 121/9 (exponential, alpha 2/3), those of 3 are 11, 12, 38/3 (simple) and
    /// 11, 12, 13 (exponential, alpha 1/2). The signal's exponential seed is the mean of its
    /// first two values, which is also its first simple value.
    #[test]
    fn each_average_pairing_matches_hand_values() {
        use OscillatorAverage::{Exponential as E, Simple as S};
        let pct = |short: f64, long: f64| 100.0 * (short - long) / long;
        let exp_line = [
            pct(11.0, 11.0),
            pct(37.0 / 3.0, 12.0),
            pct(121.0 / 9.0, 13.0),
        ];
        let sma_line = [pct(11.5, 11.0), pct(12.0, 12.0), pct(13.5, 38.0 / 3.0)];
        let data: Vec<Bar> = [10.0, 12.0, 11.0, 13.0, 14.0]
            .into_iter()
            .map(make_bar)
            .collect();
        for (osc, signal, line) in [
            (E, E, exp_line),
            (E, S, exp_line),
            (S, E, sma_line),
            (S, S, sma_line),
        ] {
            let seed = (line[0] + line[1]) / 2.0;
            let last = match signal {
                E => seed + (line[2] - seed) * 2.0 / 3.0,
                S => (line[1] + line[2]) / 2.0,
            };
            let mut ppo = PriceOscillator::new(2, 3, 2).with_averages(osc, signal);
            ppo.calculate(&data);
            for (bar, ppo_line, signal) in [(3, line[1], seed), (4, line[2], last)] {
                let got: Vec<f64> = (0..3)
                    .map(|l| ppo.values().value(l, bar).unwrap())
                    .collect();
                for (got, want) in got.iter().zip([ppo_line, signal, ppo_line - signal]) {
                    assert!((got - want).abs() < 1e-9, "{osc:?}/{signal:?} bar {bar}");
                }
            }
        }
    }
}
