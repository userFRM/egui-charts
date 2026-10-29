use super::price_oscillator::{Mean, OscillatorAverage, percentage_oscillator};
use crate::model::Bar;
use crate::studies::IndicatorValues;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue};
use egui::Color32;

/// Percentage Volume Oscillator (PVO): the [`PriceOscillator`](super::PriceOscillator)
/// computed from volume instead of close.
///
/// - PVO = 100 * (MA(volume, short) - MA(volume, long)) / MA(volume, long).
/// - Signal = MA(PVO, signal length), computed only over the bars where the PVO is defined.
///   An exponential average is seeded with the mean of its first `signal` values; a simple one
///   is the mean of the last `signal` values.
/// - Histogram = PVO - signal.
///
/// MA is exponential by default; the oscillator's averages and the signal line each choose an
/// [`OscillatorAverage`] independently. With lengths 12, 26 and 9 a bar has no value before
/// bar 25, the PVO alone on bars 25 to 32, and all three lines from bar 33, under either
/// formula. A bar with a non-finite volume has no value and is skipped by the averages.
#[derive(Clone)]
pub struct PercentageVolumeOscillator {
    short_period: usize,
    long_period: usize,
    signal_period: usize,
    oscillator_average: OscillatorAverage,
    signal_average: OscillatorAverage,
    values: IndicatorValues,
    stepper: Stepper<[Mean; 3]>,
    /// Colors: [PVO line, Signal line, Histogram]
    colors: Vec<Color32>,
    visible: bool,
}

impl PercentageVolumeOscillator {
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
                palette::BLUE,   // PVO
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

/// Construct with the conventional default parameters.
impl Default for PercentageVolumeOscillator {
    fn default() -> Self {
        Self::new(12, 26, 9)
    }
}

impl Indicator for PercentageVolumeOscillator {
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
        "PVO"
    }

    fn desc(&self) -> &str {
        "Percentage Volume Oscillator - Percentage difference between volume EMAs"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                if !bar.volume.is_finite() {
                    return IndicatorValue::None;
                }
                percentage_oscillator(
                    state,
                    bar.volume,
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
            format!("PVO({},{})", self.short_period, self.long_period),
            format!("Signal({})", self.signal_period),
            "Histogram".to_string(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::studies::PriceOscillator;

    /// The PVO of a volume series is the PPO of the same series as closes, for a chosen formula.
    #[test]
    fn chosen_averages_apply_to_volume() {
        let bar = |x: f64| Bar::new(chrono::Utc::now(), x, x, x, x, x);
        let data: Vec<Bar> = [10.0, 12.0, 11.0, 13.0, 14.0]
            .into_iter()
            .map(bar)
            .collect();
        let averages = (OscillatorAverage::Simple, OscillatorAverage::Exponential);
        let mut pvo =
            PercentageVolumeOscillator::new(2, 3, 2).with_averages(averages.0, averages.1);
        let mut ppo = PriceOscillator::new(2, 3, 2).with_averages(averages.0, averages.1);
        pvo.calculate(&data);
        ppo.calculate(&data);
        assert_eq!(format!("{:?}", pvo.values()), format!("{:?}", ppo.values()));
    }
}
