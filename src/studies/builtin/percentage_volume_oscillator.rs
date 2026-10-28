use super::price_oscillator::percentage_oscillator;
use super::smoothing::Smoothing;
use crate::model::Bar;
use crate::studies::IndicatorValues;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue};
use egui::Color32;

/// Percentage Volume Oscillator (PVO): the [`PriceOscillator`](super::PriceOscillator)
/// computed from volume instead of close.
///
/// - PVO = 100 * (EMA(volume, short) - EMA(volume, long)) / EMA(volume, long).
/// - Signal = EMA(PVO, signal length), computed only over the bars where the PVO is defined
///   and seeded with the mean of its first `signal` values.
/// - Histogram = PVO - signal.
///
/// With lengths 12, 26 and 9 a bar has no value before bar 25, the PVO alone on bars 25 to 32,
/// and all three lines from bar 33. A bar with a non-finite volume has no value and is skipped
/// by the averages.
#[derive(Clone)]
pub struct PercentageVolumeOscillator {
    short_period: usize,
    long_period: usize,
    signal_period: usize,
    values: IndicatorValues,
    stepper: Stepper<[Smoothing; 3]>,
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
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            colors: vec![
                palette::BLUE,   // Blue - PVO
                palette::ORANGE, // Orange - Signal
                palette::GREEN,  // Green - Histogram
            ],
            visible: true,
        }
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
        signal_period: length("Signal period", 1, usize::MAX)
    ; {});

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
                    self.short_period,
                    self.long_period,
                    self.signal_period,
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
        // Lines without a colour in `colors` keep theirs.
        for (line, color) in self.colors.iter_mut().zip(colors) {
            *line = color;
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
