use super::over_finite_bars;
use crate::model::Bar;
/// Fisher Transform
/// Converts prices into a Gaussian normal distribution
/// Creates sharper turning points for identifying reversals
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

#[derive(Clone)]
pub struct FisherTransform {
    /// Lookback period
    period: usize,
    values: Vec<IndicatorValue>,
    /// Colors: [Fisher, Signal/Trigger]
    colors: Vec<Color32>,
    visible: bool,
}

impl FisherTransform {
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            values: Vec::new(),
            colors: vec![
                DESIGN_TOKENS.semantic.extended.info,    // Blue - Fisher
                DESIGN_TOKENS.semantic.extended.warning, // Orange - Signal
            ],
            visible: true,
        }
    }

    pub fn with_colors(mut self, colors: Vec<Color32>) -> Self {
        self.colors = colors;
        self
    }

    /// The values over `data`, every bar of which has finite inputs.
    fn compute(&self, data: &[Bar]) -> Vec<IndicatorValue> {
        let mut values = Vec::with_capacity(data.len());

        if data.len() < self.period {
            for _ in 0..data.len() {
                values.push(IndicatorValue::None);
            }
            return values;
        }

        // Calculate HL2 (typical price simplified)
        let hl2: Vec<f64> = data.iter().map(|b| (b.high + b.low) / 2.0).collect();

        let mut fisher_values = Vec::with_capacity(data.len());
        let mut prev_val = 0.0;
        let mut prev_fisher = 0.0;

        for i in 0..data.len() {
            if i < self.period - 1 {
                fisher_values.push((f64::NAN, f64::NAN));
                continue;
            }

            // Find highest and lowest in period
            let window = &hl2[i + 1 - self.period..=i];
            let highest = window.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b));
            let lowest = window.iter().fold(f64::INFINITY, |a, &b| a.min(b));

            // Normalize to range [-1, 1]
            let range = highest - lowest;
            let normalized = if range > 0.0 {
                2.0 * ((hl2[i] - lowest) / range - 0.5)
            } else {
                0.0
            };

            // Smooth the normalized value, then keep it inside the open interval
            // the logarithm needs. Ehlers' published code limits values beyond
            // 0.99 to 0.999, and the limited value feeds the next bar.
            let mut value = 0.33 * normalized + 0.67 * prev_val;
            if value > 0.99 {
                value = 0.999;
            } else if value < -0.99 {
                value = -0.999;
            }

            // Apply Fisher Transform
            let fisher = 0.5 * ((1.0 + value) / (1.0 - value)).ln() + 0.5 * prev_fisher;
            let signal = prev_fisher;

            prev_val = value;
            prev_fisher = fisher;

            fisher_values.push((fisher, signal));
        }

        // Store results
        for (fisher, signal) in fisher_values {
            if fisher.is_nan() {
                values.push(IndicatorValue::None);
            } else if signal.is_nan() || signal == 0.0 {
                values.push(IndicatorValue::Single(fisher));
            } else {
                values.push(IndicatorValue::Multiple(vec![fisher, signal]));
            }
        }
        values
    }
}

/// Construct with the conventional default parameters.
impl Default for FisherTransform {
    fn default() -> Self {
        Self::new(9)
    }
}

impl Indicator for FisherTransform {
    fn name(&self) -> &str {
        "Fisher Transform"
    }

    fn desc(&self) -> &str {
        "Fisher Transform - Identifies price reversals with Gaussian distribution"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values = over_finite_bars(
            data,
            |b| b.high.is_finite() && b.low.is_finite(),
            |data| self.compute(data),
        );
    }

    fn values(&self) -> &[IndicatorValue] {
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
        }
    }

    /// (high, low, expected (fisher, trigger)) for length 5, from Ehlers'
    /// published construction computed independently. The series rises long
    /// enough for the smoothed value to reach the clamp, stays flat until the
    /// window has no range, then falls.
    #[allow(clippy::type_complexity)]
    const REFERENCE: &[(f64, f64, Option<(f64, f64)>)] = &[
        (101.0, 99.0, None),
        (102.0, 100.0, None),
        (103.0, 101.0, None),
        (104.0, 102.0, None),
        (105.0, 103.0, Some((0.342828254415394, 0.0))),
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
        for (i, (value, &(_, _, expected))) in ft.values.iter().zip(REFERENCE).enumerate() {
            // A zero trigger is reported as a single line.
            let actual = match value {
                IndicatorValue::None => None,
                IndicatorValue::Single(f) => Some((*f, 0.0)),
                IndicatorValue::Multiple(v) => Some((v[0], v[1])),
            };
            match (actual, expected) {
                (None, None) => {}
                (Some((f, t)), Some((ef, et))) => assert!(
                    (f - ef).abs() < 1e-9 && (t - et).abs() < 1e-9,
                    "bar {i}: got ({f}, {t}), expected ({ef}, {et})"
                ),
                _ => panic!("bar {i}: got {actual:?}, expected {expected:?}"),
            }
        }
    }
}
