use super::{clamp_param, over_finite_bars};
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::{ATR, Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Chandelier Exit
/// Trailing stop-loss indicator based on Wilder's ATR
/// Developed by Chuck LeBeau
#[derive(Clone)]
pub struct ChandelierExit {
    /// ATR period (typically 22)
    period: usize,
    /// ATR multiplier (typically 3.0)
    multiplier: f64,
    values: IndicatorValues,
    /// Colors: [Long exit, Short exit]
    colors: Vec<Color32>,
    visible: bool,
}

impl ChandelierExit {
    /// `multiplier` is clamped into 0 to `f64::MAX`, NaN to 0.
    pub fn new(period: usize, multiplier: f64) -> Self {
        Self {
            period: period.max(1),
            multiplier: clamp_param(multiplier, 0.0, f64::MAX),
            values: IndicatorValues::new(),
            colors: vec![
                palette::GREEN, // Green - Long exit
                palette::RED,   // Red - Short exit
            ],
            visible: true,
        }
    }

    /// Set plot colors in the order returned by [`Indicator::line_names`].
    pub fn with_colors(mut self, colors: Vec<Color32>) -> Self {
        self.colors = colors;
        self
    }

    /// The values over `data`, every bar of which has finite inputs.
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let mut values = IndicatorValues::new();

        let mut atr = ATR::new(self.period);
        atr.calculate(data);

        for i in 0..data.len() {
            match atr.values().value(0, i) {
                Some(atr) => {
                    // Find highest high and lowest low in period
                    let window = &data[i + 1 - self.period..=i];
                    let highest = window
                        .iter()
                        .map(|b| b.high)
                        .fold(f64::NEG_INFINITY, f64::max);
                    let lowest = window.iter().map(|b| b.low).fold(f64::INFINITY, f64::min);

                    // Long exit = Highest High - ATR * multiplier
                    let long_exit = highest - atr * self.multiplier;
                    // Short exit = Lowest Low + ATR * multiplier
                    let short_exit = lowest + atr * self.multiplier;

                    values.push_lines(&[long_exit, short_exit]);
                }
                _ => values.push(IndicatorValue::None),
            }
        }
        values
    }
}

impl Default for ChandelierExit {
    fn default() -> Self {
        Self::new(22, 3.0)
    }
}

impl Indicator for ChandelierExit {
    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX),
        multiplier: number("Multiplier", 0.0, f64::MAX)
    ; {});

    fn name(&self) -> &str {
        "Chandelier"
    }

    fn desc(&self) -> &str {
        "Chandelier Exit - ATR-based trailing stop"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values = over_finite_bars(
            data,
            |b| [b.high, b.low, b.close].iter().all(|p| p.is_finite()),
            |data| self.compute(data),
        );
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
        true // Overlay on price chart
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
        2
    }

    fn line_names(&self) -> Vec<String> {
        vec![
            format!("Long Exit({},{})", self.period, self.multiplier),
            format!("Short Exit({},{})", self.period, self.multiplier),
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

    /// Reference values from LeBeau's definition over Wilder's ATR, computed
    /// independently: highest high or lowest low of the last `period` bars,
    /// minus or plus `multiplier` times ATR(`period`). The gap on bar 4 makes
    /// Wilder's smoothing differ from a simple average of true range.
    #[test]
    fn matches_reference_values() {
        let trend = [
            (102.0, 98.0, 100.0),
            (104.0, 99.0, 103.0),
            (106.0, 101.0, 105.0),
            (108.0, 103.0, 107.0),
            (118.0, 112.0, 116.0),
            (117.0, 113.0, 114.0),
            (115.0, 109.0, 110.0),
            (112.0, 108.0, 111.0),
        ];
        let trend_expected = [
            (96.66666666666667, 107.33333333333333),
            (98.44444444444444, 108.55555555555556),
            (104.29629629629629, 114.70370370370371),
            (106.19753086419753, 114.80246913580247),
            (106.13168724279835, 120.86831275720165),
            (106.42112482853224, 118.57887517146776),
        ];
        let flat = [(101.0, 99.0, 100.0); 5];
        let flat_expected = [(97.0, 103.0); 3];
        let cases = [
            ("trend with gap", &trend[..], &trend_expected[..]),
            ("flat", &flat[..], &flat_expected[..]),
        ];

        let mut failures = Vec::new();
        if ChandelierExit::new(3, 2.0).line_count() != 2 {
            failures.push("short exit line is not drawn".to_string());
        }
        for (label, bars, expected) in cases {
            let data: Vec<Bar> = bars.iter().map(|&(h, l, c)| make_bar(h, l, c)).collect();
            let mut ce = ChandelierExit::new(3, 2.0);
            ce.calculate(&data);

            let got = ce.values().to_rows();
            let warm_up = got
                .iter()
                .take(2)
                .all(|v| matches!(v, IndicatorValue::None));
            if got.len() != data.len() || !warm_up {
                failures.push(format!("{label}: warm-up or length wrong: {got:?}"));
                continue;
            }
            for (i, &(long, short)) in expected.iter().enumerate() {
                let ok = matches!(&got[i + 2], IndicatorValue::Multiple(v)
                    if (v[0] - long).abs() < 1e-9 && (v[1] - short).abs() < 1e-9);
                if !ok {
                    failures.push(format!(
                        "{label}: bar {} is {:?}, expected [{long}, {short}]",
                        i + 2,
                        got[i + 2]
                    ));
                }
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }
}
