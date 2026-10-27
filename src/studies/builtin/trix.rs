use super::smoothing::Smoothing;
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// TRIX - Triple Exponential Avg
/// A momentum oscillator that shows the rate of change of a triple-smoothed EMA
/// Filters out insignificant price movements
#[derive(Clone)]
pub struct TRIX {
    period: usize,
    signal_period: usize,
    values: IndicatorValues,
    stepper: Stepper<State>,
    /// Colors: [TRIX line, Signal line]
    colors: Vec<Color32>,
    visible: bool,
}

#[derive(Clone, Default)]
struct State {
    ema: [Smoothing; 3],
    signal: Smoothing,
    previous: Option<f64>,
    count: usize,
    bars: usize,
    pending: Vec<(usize, [f64; 2])>,
}

impl TRIX {
    /// Create with a lookback of `period` bars, clamped to at least 1.
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            signal_period: 9,
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            colors: vec![
                palette::BLUE,   // Blue - TRIX
                palette::ORANGE, // Orange - Signal
            ],
            visible: true,
        }
    }

    /// Set the signal EMA period in bars, clamped to at least one.
    pub fn with_signal(mut self, signal_period: usize) -> Self {
        self.signal_period = signal_period.max(1);
        self
    }

    /// Set plot colors in the order returned by [`Indicator::line_names`].
    pub fn with_colors(mut self, colors: Vec<Color32>) -> Self {
        self.colors = colors;
        self
    }
}

/// Construct with the conventional default parameters.
impl Default for TRIX {
    fn default() -> Self {
        Self::new(15)
    }
}

impl Indicator for TRIX {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::Close,
        ))
    }

    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX),
        signal_period: length("Signal period", 1, usize::MAX)
    ; {});

    fn value_format(&self) -> crate::scales::StudyFormat {
        crate::scales::StudyFormat::Percent
    }

    fn name(&self) -> &str {
        "TRIX"
    }

    fn desc(&self) -> &str {
        "TRIX - Rate of change of triple-smoothed EMA"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        let required = self.period.saturating_mul(3).saturating_add(1);
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                let index = state.bars;
                state.bars += 1;
                if !bar.close.is_finite() {
                    return IndicatorValue::None;
                }
                state.count += 1;
                let alpha = 2.0 / (self.period as f64 + 1.0);
                let ema1 = state.ema[0].next(bar.close, self.period, alpha);
                let ema2 = state.ema[1].next(ema1, self.period, alpha);
                let ema3 = state.ema[2].next(ema2, self.period, alpha);
                let previous = state.previous.replace(ema3).unwrap_or(f64::NAN);
                let trix = if previous == 0.0 {
                    f64::NAN
                } else {
                    (ema3 - previous) / previous * 100.0
                };
                let signal = state.signal.next(
                    trix,
                    self.signal_period,
                    2.0 / (self.signal_period as f64 + 1.0),
                );
                if state.count < required {
                    if trix.is_finite() {
                        state.pending.push((index, [trix, signal]));
                    }
                    IndicatorValue::None
                } else {
                    IndicatorValue::Multiple(vec![trix, signal])
                }
            });
        // Crossing the batch warm-up threshold reveals the preceding TRIX values too.
        for &(index, row) in &self.stepper.state().pending {
            self.values.set(
                index,
                if self.stepper.state().count < required {
                    IndicatorValue::None
                } else {
                    IndicatorValue::Multiple(row.to_vec())
                },
            );
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
        if colors.len() >= 2 {
            self.colors = colors;
        }
    }

    fn is_overlay(&self) -> bool {
        false // Separate pane
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
            format!("TRIX({})", self.period),
            format!("Signal({})", self.signal_period),
        ]
    }
}
