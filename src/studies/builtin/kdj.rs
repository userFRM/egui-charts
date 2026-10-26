use super::over_finite_bars;
use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// KDJ, the stochastic oscillator as used in Chinese securities analysis.
///
/// The raw stochastic value of each bar is `RSV = (close - lowest low) / (highest high - lowest low) * 100`
/// over the last `period` bars, or 50 when that window has no range. K and D are smoothed recursively,
/// both starting from 50: `K = ((k_smooth - 1) * K[-1] + RSV) / k_smooth` and
/// `D = ((d_smooth - 1) * D[-1] + K) / d_smooth`, so the default lengths of 3 weight the new value by a
/// third. `J = 3K - 2D`. The first value is on bar `period - 1`; earlier bars are `None`.
/// A bar with a non-finite high, low or close has no value and is left out: the windows span usable
/// bars only, so every other bar is that of the series without it.
#[derive(Clone)]
pub struct KDJ {
    /// Period for %K calculation (typically 9)
    period: usize,
    /// Smoothing period for %K (typically 3)
    k_smooth: usize,
    /// Smoothing period for %D (typically 3)
    d_smooth: usize,
    values: IndicatorValues,
    /// Colors: [K, D, J]
    colors: Vec<Color32>,
    visible: bool,
}

impl KDJ {
    /// Create with `period`, `k_smooth`, `d_smooth` in bars, each clamped to at least one.
    pub fn new(period: usize, k_smooth: usize, d_smooth: usize) -> Self {
        Self {
            period: period.max(1),
            k_smooth: k_smooth.max(1),
            d_smooth: d_smooth.max(1),
            values: IndicatorValues::new(),
            colors: vec![
                DESIGN_TOKENS.semantic.extended.info,    // Blue - K
                DESIGN_TOKENS.semantic.extended.warning, // Orange - D
                DESIGN_TOKENS.semantic.extended.purple,  // Purple - J
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

impl Default for KDJ {
    fn default() -> Self {
        Self::new(9, 3, 3)
    }
}

impl KDJ {
    /// The values over `data`, every bar of which has finite inputs.
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let mut values = IndicatorValues::new();
        let k_len = self.k_smooth as f64;
        let d_len = self.d_smooth as f64;
        let (mut k, mut d) = (50.0, 50.0);

        for i in 0..data.len() {
            if i + 1 < self.period {
                values.push(IndicatorValue::None);
                continue;
            }
            let window = &data[i + 1 - self.period..=i];
            let highest = window
                .iter()
                .map(|b| b.high)
                .fold(f64::NEG_INFINITY, f64::max);
            let lowest = window.iter().map(|b| b.low).fold(f64::INFINITY, f64::min);
            let range = highest - lowest;
            if !(range.is_finite() && data[i].close.is_finite()) {
                // The range of finite prices overflowed; K and D carry over the bar unchanged.
                values.push(IndicatorValue::None);
                continue;
            }
            let rsv = if range == 0.0 {
                50.0
            } else {
                (data[i].close - lowest) / range * 100.0
            };

            k = ((k_len - 1.0) * k + rsv) / k_len;
            d = ((d_len - 1.0) * d + k) / d_len;
            values.push_lines(&[k, d, 3.0 * k - 2.0 * d]);
        }

        values
    }
}

impl Indicator for KDJ {
    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX),
        k_smooth: length("K smooth", 1, usize::MAX),
        d_smooth: length("D smooth", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "KDJ"
    }

    fn desc(&self) -> &str {
        "KDJ - Stochastic-derived indicator with K, D, J lines"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values = over_finite_bars(
            data,
            |b| b.high.is_finite() && b.low.is_finite() && b.close.is_finite(),
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
        if colors.len() >= 3 {
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

    fn line_count(&self) -> usize {
        3
    }

    fn line_names(&self) -> Vec<String> {
        vec![
            format!("K({})", self.period),
            format!("D({})", self.d_smooth),
            "J".to_string(),
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

    /// Reference values from the published KDJ formula, computed in exact rational arithmetic:
    /// `RSV = (C - LLV(L, N)) / (HHV(H, N) - LLV(L, N)) * 100`, `K = ((M1 - 1) * K[-1] + RSV) / M1`,
    /// `D = ((M2 - 1) * D[-1] + K) / M2`, both 50 before the first value, and `J = 3K - 2D`.
    #[test]
    fn kdj_matches_published_recursion() {
        let data: Vec<Bar> = [
            (102.0, 98.0, 100.0),
            (104.0, 99.0, 103.0),
            (106.0, 101.0, 104.0),
            (105.0, 100.0, 101.0),
            (103.0, 97.0, 98.0),
            (104.0, 99.0, 103.0),
            (101.0, 101.0, 101.0),
            (101.0, 101.0, 101.0),
            (101.0, 101.0, 101.0),
            (108.0, 100.0, 107.0),
            (110.0, 104.0, 105.0),
            (109.0, 103.0, 104.0),
            (107.0, 102.0, 106.0),
            (112.0, 105.0, 111.0),
            (111.0, 106.0, 107.0),
        ]
        .iter()
        .map(|&(h, l, c)| make_bar(h, l, c))
        .collect();
        // Bar 2: RSV = (104 - 98) / (106 - 98) * 100 = 75, K = 50 * 2/3 + 75 / 3, D = 50 / 2 + K / 2.
        // Bar 8 closes a window with no range, where RSV is 50.
        let expected: [Option<[f64; 3]>; 15] = [
            None,
            None,
            Some([58.333333333333336, 54.166666666666664, 66.66666666666667]),
            Some([48.41269841269841, 51.28968253968254, 42.65873015873016]),
            Some([35.97883597883598, 43.63425925925926, 20.667989417989418]),
            Some([48.985890652557316, 46.31007495590829, 54.33752204585538]),
            Some([51.70487948265726, 49.00747721928278, 57.09968400940623]),
            Some([47.803252988438174, 48.405365103860476, 46.59902875759357]),
            Some([48.53550199229212, 48.470433548076294, 48.665638880723755]),
            Some([61.52366799486141, 54.99705077146885, 74.57690244164652]),
            Some([57.68244532990761, 56.33974805068823, 60.36783988834636]),
            Some([51.788296886605075, 54.06402246864665, 47.23684572252191]),
            Some([51.19219792440338, 52.628110196525014, 48.32037338016011]),
            Some([64.12813194960225, 58.37812107306363, 75.6281537026795]),
            Some([59.41875463306817, 58.8984378530659, 60.459388193072705]),
        ];

        // The same bars with a NaN close on bar 9 and an infinite high on bar 13: those two bars have no
        // value and the others are those of the series without them.
        let mut hostile = data.clone();
        hostile[9].close = f64::NAN;
        hostile[13].high = f64::INFINITY;
        let mut hostile_expected = expected;
        hostile_expected[9] = None;
        hostile_expected[10] = Some([47.17181614300956, 47.821124845542926, 45.87319873794282]);
        hostile_expected[11] = Some([42.55898853978415, 45.19005669266354, 37.29685223402537]);
        hostile_expected[12] = Some([45.03932569318943, 45.114691192926486, 44.888594693715326]);
        hostile_expected[13] = None;
        hostile_expected[14] = Some([48.54473564731148, 46.82971342011898, 51.97478010169646]);

        // Distinct K and D lengths so that swapping their weights fails.
        let mut kdj = KDJ::new(3, 3, 2);
        for (case, data, expected) in [
            ("finite", &data, expected),
            ("non-finite", &hostile, hostile_expected),
        ] {
            kdj.calculate(data);
            assert_eq!(kdj.values().len(), expected.len(), "{case}");
            for (i, (value, want)) in kdj.values().to_rows().iter().zip(expected).enumerate() {
                match (value, want) {
                    (IndicatorValue::None, None) => {}
                    (IndicatorValue::Multiple(got), Some(want)) => {
                        assert_eq!(got.len(), kdj.line_count(), "{case} bar {i}");
                        for (g, w) in got.iter().zip(want) {
                            assert!(
                                (g - w).abs() < 1e-12,
                                "{case} bar {i}: got {got:?}, want {want:?}"
                            );
                        }
                    }
                    _ => panic!("{case} bar {i}: got {value:?}, want {want:?}"),
                }
            }
        }
    }
}
