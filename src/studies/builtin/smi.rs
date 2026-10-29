use super::smoothing::Smoothing;
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Stochastic Momentum Index (SMI)
/// Enhanced stochastic that measures close relative to HL range midpoint
/// More responsive than standard stochastic
#[derive(Clone)]
pub struct StochasticMomentumIndex {
    /// Lookback period (typically 10)
    period: usize,
    /// First EMA smoothing (typically 3)
    smooth1: usize,
    /// Second EMA smoothing (typically 3)
    smooth2: usize,
    /// Signal line period (typically 10)
    signal_period: usize,
    values: IndicatorValues,
    stepper: Stepper<(std::collections::VecDeque<[f64; 2]>, [Smoothing; 5])>,
    /// Colors: [SMI, Signal]
    colors: Vec<Color32>,
    visible: bool,
}

impl StochasticMomentumIndex {
    /// Create with `period`, `smooth1`, `smooth2`, `signal_period` in bars, each clamped to at
    /// least one.
    pub fn new(period: usize, smooth1: usize, smooth2: usize, signal_period: usize) -> Self {
        Self {
            period: period.max(1),
            smooth1: smooth1.max(1),
            smooth2: smooth2.max(1),
            signal_period: signal_period.max(1),
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            colors: vec![
                palette::BLUE,        // SMI
                palette::WARM_ORANGE, // Signal
            ],
            visible: true,
        }
    }

    /// Set plot colors in the order returned by [`Indicator::line_names`].
    pub fn with_colors(mut self, colors: Vec<Color32>) -> Self {
        self.colors = colors;
        self
    }
}

impl Default for StochasticMomentumIndex {
    fn default() -> Self {
        Self::new(10, 3, 3, 10)
    }
}

impl Indicator for StochasticMomentumIndex {
    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX),
        smooth1: length("Smooth1", 1, usize::MAX),
        smooth2: length("Smooth2", 1, usize::MAX),
        signal_period: length("Signal period", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "SMI"
    }

    fn desc(&self) -> &str {
        "Stochastic Momentum Index - Enhanced stochastic"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                if ![bar.high, bar.low, bar.close].iter().all(|x| x.is_finite()) {
                    return IndicatorValue::None;
                }
                state.0.push_back([bar.high, bar.low]);
                if state.0.len() > self.period {
                    state.0.pop_front();
                }
                if state.0.len() < self.period {
                    return IndicatorValue::None;
                }
                let high = state
                    .0
                    .iter()
                    .map(|v| v[0])
                    .fold(f64::NEG_INFINITY, f64::max);
                let low = state.0.iter().map(|v| v[1]).fold(f64::INFINITY, f64::min);
                let distance = bar.close - (high + low) / 2.0;
                let range = high - low;
                let a1 = 2.0 / (self.smooth1 as f64 + 1.0);
                let a2 = 2.0 / (self.smooth2 as f64 + 1.0);
                let d1 = state.1[0].next(distance, 1, a1);
                let d2 = state.1[1].next(d1, 1, a2);
                let r1 = state.1[2].next(range, 1, a1);
                let r2 = state.1[3].next(r1, 1, a2);
                let smi = if d2.is_nan() || r2.is_nan() || r2 == 0.0 {
                    f64::NAN
                } else {
                    100.0 * d2 / (r2 / 2.0)
                };
                let signal = state.1[4].next(smi, 1, 2.0 / (self.signal_period as f64 + 1.0));
                if smi.is_nan() {
                    IndicatorValue::None
                } else {
                    IndicatorValue::Multiple(vec![smi, signal])
                }
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
        if colors.len() >= 2 {
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

    fn line_names(&self) -> Vec<String> {
        vec![
            format!("SMI({},{},{})", self.period, self.smooth1, self.smooth2),
            format!("Signal({})", self.signal_period),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_bar(high: f64, low: f64, close: f64) -> Bar {
        Bar {
            time: Utc::now(),
            open: (high + low) / 2.0,
            high,
            low,
            close,
            volume: 1000.0,
            colors: None,
            payload: None,
        }
    }

    #[test]
    fn test_smi_calculation() {
        let mut smi = StochasticMomentumIndex::new(5, 2, 2, 3);

        let data: Vec<Bar> = (0..20)
            .map(|i| make_bar(102.0 + i as f64, 98.0 + i as f64, 101.0 + i as f64))
            .collect();

        smi.calculate(&data);

        assert_eq!(smi.values.len(), 20);

        // Should have valid values
        let valid_cnt = smi
            .values
            .to_rows()
            .iter()
            .filter(|v| !matches!(v, IndicatorValue::None))
            .count();

        assert!(valid_cnt > 0);
    }
}
