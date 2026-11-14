use super::{ATR, smoothing::Smoothing};
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;
use std::collections::VecDeque;

/// Pretty Good Oscillator (PGO)
/// Measures (close - SMA of closes) / EMA of true range over the same period.
/// The EMA starts at the mean of the first `period` true ranges.
/// Values > 3 or < -3 indicate potential reversal
#[derive(Clone)]
pub struct PrettyGoodOscillator {
    /// Period for calculations (typically 14)
    period: usize,
    values: IndicatorValues,
    stepper: Stepper<State>,
    color: Color32,
    visible: bool,
}

#[derive(Clone, Default)]
struct State {
    previous: Option<f64>,
    closes: VecDeque<f64>,
    range: Smoothing,
}

impl PrettyGoodOscillator {
    /// Create with a lookback of `period` bars, clamped to at least 1.
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            color: palette::PURPLE,
            visible: true,
        }
    }

    /// Set the plot color.
    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }
}

impl Default for PrettyGoodOscillator {
    fn default() -> Self {
        Self::new(14)
    }
}

impl Indicator for PrettyGoodOscillator {
    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "PGO"
    }

    fn desc(&self) -> &str {
        "Pretty Good Oscillator - Price distance in ATR units"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                if ![bar.high, bar.low, bar.close].iter().all(|p| p.is_finite()) {
                    return IndicatorValue::None;
                }
                let tr = state
                    .previous
                    .map_or(bar.high - bar.low, |close| ATR::true_range(bar, close));
                state.previous = Some(bar.close);
                let range = state
                    .range
                    .next(tr, self.period, 2.0 / (self.period as f64 + 1.0));
                state.closes.push_back(bar.close);
                if state.closes.len() > self.period {
                    state.closes.pop_front();
                }
                if state.closes.len() < self.period || range == 0.0 {
                    return IndicatorValue::None;
                }
                let mean = state.closes.iter().sum::<f64>() / self.period as f64;
                IndicatorValue::Single((bar.close - mean) / range)
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
        if !colors.is_empty() {
            self.color = colors[0];
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
        vec![format!("PGO({})", self.period)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::studies::test_support::make_range_bar as make_bar;

    #[test]
    fn test_pgo_calculation() {
        let mut pgo = PrettyGoodOscillator::new(5);

        let data = vec![
            make_bar(102.0, 98.0, 100.0),
            make_bar(103.0, 99.0, 101.0),
            make_bar(104.0, 100.0, 102.0),
            make_bar(105.0, 101.0, 103.0),
            make_bar(106.0, 102.0, 104.0),
            make_bar(110.0, 105.0, 109.0),
            make_bar(110.0, 106.0, 108.0),
        ];

        pgo.calculate(&data);

        assert_eq!(pgo.values().len(), data.len());
        // Mark Johnson's PGO by hand: (close-SMA(5))/EMA(5) of true range.
        // Ranges 4,4,4,4,4,6,4 seed at 4, then alpha=1/3 gives 14/3,40/9.
        // Close minus mean is 2,26/5,14/5, giving 1/2,39/35,63/100.
        for bar in 0..4 {
            assert_eq!(pgo.values().value(0, bar), None);
        }
        for (bar, want) in [(4, 0.5), (5, 39.0 / 35.0), (6, 0.63)] {
            let got = pgo.values().value(0, bar).unwrap();
            assert!((got - want).abs() < 1e-12, "bar {bar}: {got}");
        }
    }
}
