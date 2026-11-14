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
    /// Signal line period (typically 3)
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
        Self::new(10, 3, 3, 3)
    }
}

impl Indicator for StochasticMomentumIndex {
    fn inputs(&self) -> Vec<crate::studies::StudyInput> {
        use crate::studies::{InputKind, InputValue, StudyInput};
        let length = |id, label, value, default| {
            StudyInput::new(
                id,
                label,
                InputValue::Length(value),
                InputKind::Length(1, usize::MAX),
            )
            .with_default(InputValue::Length(default))
        };
        let mut inputs = vec![length("period", "Range length", self.period, 10)];
        if self.smooth1 == self.smooth2 {
            inputs.push(length("smoothing", "Smoothing length", self.smooth1, 3));
        } else {
            // Retain independently chosen lengths from constructors and older saved studies.
            inputs.push(length("smooth1", "First smoothing length", self.smooth1, 3));
            inputs.push(length(
                "smooth2",
                "Second smoothing length",
                self.smooth2,
                3,
            ));
        }
        inputs.push(length(
            "signal_period",
            "Signal length",
            self.signal_period,
            3,
        ));
        inputs
    }

    fn set_input(
        &mut self,
        id: &str,
        value: crate::studies::InputValue,
    ) -> Result<(), crate::studies::StudyError> {
        use crate::studies::{InputValue, StudyError};
        if !matches!(
            id,
            "period" | "smoothing" | "smooth1" | "smooth2" | "signal_period"
        ) {
            return Err(StudyError::UnknownInput(id.into()));
        }
        let InputValue::Length(value) = value else {
            return Err(StudyError::invalid_input(id));
        };
        if value == 0 {
            return Err(StudyError::invalid_input(id));
        }
        match id {
            "period" => self.period = value,
            "smoothing" => {
                self.smooth1 = value;
                self.smooth2 = value;
            }
            "smooth1" => self.smooth1 = value,
            "smooth2" => self.smooth2 = value,
            "signal_period" => self.signal_period = value,
            _ => unreachable!(),
        }
        self.calculate(&[]);
        Ok(())
    }

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

    fn pane(&self) -> crate::studies::StudyPane {
        crate::studies::StudyPane {
            height: None,
            range: None,
            levels: vec![
                (40.0, palette::MUTED, "Upper guide"),
                (-40.0, palette::MUTED, "Lower guide"),
                (0.0, palette::MUTED.gamma_multiply_u8(128), "Middle guide"),
            ],
            fill_between_levels: true,
        }
    }

    fn level_fill_indices(&self) -> Option<[usize; 2]> {
        Some([0, 1])
    }

    fn level_fill_color(&self) -> Color32 {
        palette::BLUE.gamma_multiply_u8(26)
    }

    fn gradients(&self) -> Vec<crate::studies::StudyGradient> {
        [
            (
                "Upper shading",
                [100.0, 40.0],
                [palette::GREEN.gamma_multiply_u8(128), Color32::TRANSPARENT],
            ),
            (
                "Lower shading",
                [-40.0, -100.0],
                [Color32::TRANSPARENT, palette::RED.gamma_multiply_u8(128)],
            ),
        ]
        .into_iter()
        .map(|(name, range, colors)| crate::studies::StudyGradient {
            name,
            plot: 0,
            baseline: 0.0,
            range,
            colors,
        })
        .collect()
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
    use crate::studies::test_support::make_range_bar as make_bar;

    #[test]
    fn test_smi_calculation() {
        // Blau's SMI by hand: first distance=105-(106+98)/2=3, range=8,
        // so SMI=75. Next distance=104-(107+99)/2=1; double EMA(2), alpha 2/3,
        // gives 5/3 then 19/9, so SMI=475/9. EMA(3) signal is (75+475/9)/2=575/9.
        let mut smi = StochasticMomentumIndex::new(5, 2, 2, 3);
        let mut data: Vec<Bar> = (0..6)
            .map(|i| make_bar(102.0 + i as f64, 98.0 + i as f64, 101.0 + i as f64))
            .collect();
        data[5].close = 104.0;
        smi.calculate(&data);
        assert_eq!(smi.values().len(), 6);
        for bar in 0..4 {
            assert_eq!(smi.values().value(0, bar), None);
        }
        for (bar, expected) in [(4, [75.0, 75.0]), (5, [475.0 / 9.0, 575.0 / 9.0])] {
            for (line, want) in expected.into_iter().enumerate() {
                let got = smi.values().value(line, bar).unwrap();
                assert!((got - want).abs() < 1e-12, "line {line}, bar {bar}: {got}");
            }
        }
        // The shared length sets both EMA stages. A correction gives d2=23/9,
        // then appending gives d2=61/27; the range stays 8 throughout.
        smi.set_input("smoothing", crate::studies::InputValue::Length(2))
            .unwrap();
        smi.calculate(&data);
        data[5].close = 105.0;
        smi.update(&data, 5);
        for (line, expected) in [575.0 / 9.0, 625.0 / 9.0].into_iter().enumerate() {
            assert!((smi.values().value(line, 5).unwrap() - expected).abs() < 1e-12);
        }
        data.push(make_bar(108.0, 104.0, 106.0));
        smi.update(&data, 6);
        for (line, expected) in [1525.0 / 27.0, 1700.0 / 27.0].into_iter().enumerate() {
            assert!((smi.values().value(line, 6).unwrap() - expected).abs() < 1e-12);
        }
        // Older saved lengths keep their exact values, even when unequal.
        let study = crate::studies::ConfiguredStudy::new(Box::new(smi.clone()));
        let mut saved = crate::studies::StudyTemplate::from_study(&study);
        saved.inputs.retain(|(id, _)| id != "smoothing");
        saved.inputs.extend([
            ("smooth1".into(), crate::studies::InputValue::Length(2)),
            ("smooth2".into(), crate::studies::InputValue::Length(4)),
        ]);
        let mut older = serde_json::to_value(saved).unwrap();
        older["settings"]
            .as_object_mut()
            .unwrap()
            .remove("gradients");
        let restored = serde_json::from_value::<crate::studies::StudyTemplate>(older)
            .unwrap()
            .create()
            .unwrap();
        assert_eq!(
            serde_json::to_value(restored.settings()).unwrap()["gradients"]
                .as_array()
                .map(Vec::len),
            Some(2)
        );
        for (id, value) in [("smooth1", 2), ("smooth2", 4)] {
            assert_eq!(
                restored.inputs().iter().find(|i| i.id == id).unwrap().value,
                crate::studies::InputValue::Length(value)
            );
        }
        assert_eq!(
            StochasticMomentumIndex::default()
                .inputs()
                .iter()
                .find(|input| input.id == "signal_period")
                .unwrap()
                .value,
            crate::studies::InputValue::Length(3)
        );
        // A zero high-low range has no defined ratio.
        smi.calculate(&vec![make_bar(100.0, 100.0, 100.0); 6]);
        assert_eq!(smi.values().len(), 6);
        assert!(
            smi.values()
                .to_rows()
                .iter()
                .all(|v| matches!(v, IndicatorValue::None))
        );
    }
}
