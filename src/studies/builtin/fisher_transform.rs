use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Fisher Transform
/// Converts prices into a Gaussian normal distribution
/// Creates sharper turning points for identifying reversals
#[derive(Clone)]
pub struct FisherTransform {
    /// Lookback period
    period: usize,
    values: IndicatorValues,
    stepper: Stepper<(std::collections::VecDeque<f64>, f64, f64, bool)>,
    /// Colors: [Fisher, Signal/Trigger]
    colors: Vec<Color32>,
    visible: bool,
}

impl FisherTransform {
    /// Create with a lookback of `period` bars, clamped to at least 1.
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            colors: vec![
                palette::BLUE,   // Blue - Fisher
                palette::ORANGE, // Orange - Signal
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

/// Construct with the conventional default parameters.
impl Default for FisherTransform {
    fn default() -> Self {
        Self::new(9)
    }
}

impl Indicator for FisherTransform {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::HL2,
        ))
    }

    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "Fisher Transform"
    }

    fn desc(&self) -> &str {
        "Fisher Transform - Identifies price reversals with Gaussian distribution"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                if !bar.high.is_finite() || !bar.low.is_finite() {
                    return IndicatorValue::None;
                }
                let price = (bar.high + bar.low) / 2.0;
                state.0.push_back(price);
                if state.0.len() > self.period {
                    state.0.pop_front();
                }
                if state.0.len() < self.period {
                    return IndicatorValue::None;
                }
                let high = state.0.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                let low = state.0.iter().copied().fold(f64::INFINITY, f64::min);
                let range = high - low;
                let normalized = if range > 0.0 {
                    2.0 * ((price - low) / range - 0.5)
                } else {
                    0.0
                };
                let mut value = 0.33 * normalized + 0.67 * state.1;
                if value > 0.99 {
                    value = 0.999;
                } else if value < -0.99 {
                    value = -0.999;
                }
                let signal = if state.3 { state.2 } else { f64::NAN };
                state.2 = 0.5 * ((1.0 + value) / (1.0 - value)).ln() + 0.5 * state.2;
                state.1 = value;
                state.3 = true;
                if state.2.is_nan() {
                    IndicatorValue::None
                } else {
                    IndicatorValue::Multiple(vec![state.2, signal])
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
        vec![format!("Fisher({})", self.period), "Signal".to_string()]
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

    /// (high, low, expected (fisher, trigger)) for length 5, from Ehlers'
    /// published construction computed independently. The series rises long
    /// enough for the smoothed value to reach the clamp, stays flat until the
    /// window has no range, then falls. The first value has no trigger.
    #[allow(clippy::type_complexity)]
    const REFERENCE: &[(f64, f64, Option<(f64, f64)>)] = &[
        (101.0, 99.0, None),
        (102.0, 100.0, None),
        (103.0, 101.0, None),
        (104.0, 102.0, None),
        (105.0, 103.0, Some((0.342828254415394, f64::NAN))),
        (106.0, 104.0, Some((0.791373872129106, 0.342828254415394))),
        (107.0, 105.0, Some((1.26149294935091, 0.791373872129106))),
        (108.0, 106.0, Some((1.7251749834508, 1.26149294935091))),
        (109.0, 107.0, Some((2.17540858067979, 1.7251749834508))),
        (110.0, 108.0, Some((2.61256860142716, 2.17540858067979))),
        (111.0, 109.0, Some((3.03914327264257, 2.61256860142716))),
        (112.0, 110.0, Some((3.45779932767405, 3.03914327264257))),
        (113.0, 111.0, Some((3.87077398411298, 3.45779932767405))),
        (114.0, 112.0, Some((4.27977042664787, 3.87077398411298))),
        (114.0, 112.0, Some((4.6860228060836, 4.27977042664787))),
        (114.0, 112.0, Some((6.143212570292, 4.6860228060836))),
        (114.0, 112.0, Some((6.8718074523962, 6.143212570292))),
        (114.0, 112.0, Some((4.2454320902449, 6.8718074523962))),
        (114.0, 112.0, Some((2.60547582289472, 4.2454320902449))),
        (114.0, 112.0, Some((1.61276554585803, 2.60547582289472))),
        (112.0, 110.0, Some((0.676974906197813, 1.61276554585803))),
        (110.0, 108.0, Some((-0.104626787889589, 0.676974906197813))),
        (108.0, 106.0, Some((-0.759435607263236, -0.104626787889589))),
        (106.0, 104.0, Some((-1.32566396088824, -0.759435607263236))),
        (104.0, 102.0, Some((-1.83329744102957, -1.32566396088824))),
    ];

    #[test]
    fn matches_published_reference() {
        let data: Vec<Bar> = REFERENCE
            .iter()
            .map(|&(h, l, _)| make_bar(h, l, (h + l) / 2.0))
            .collect();
        let mut ft = FisherTransform::new(5);
        ft.calculate(&data);

        assert_eq!(ft.values.len(), REFERENCE.len());
        for (i, (value, &(_, _, expected))) in ft.values.to_rows().iter().zip(REFERENCE).enumerate()
        {
            let actual = match value {
                IndicatorValue::Multiple(v) => Some((v[0], v[1])),
                _ => None,
            };
            let near = |a: f64, b: f64| (a - b).abs() < 1e-9 || a.is_nan() && b.is_nan();
            match (actual, expected) {
                (None, None) => {}
                (Some((f, t)), Some((ef, et))) => assert!(
                    near(f, ef) && near(t, et),
                    "bar {i}: got ({f}, {t}), expected ({ef}, {et})"
                ),
                _ => panic!("bar {i}: got {actual:?}, expected {expected:?}"),
            }
        }
    }
}
