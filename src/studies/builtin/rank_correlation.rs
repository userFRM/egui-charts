use super::smoothing::{SMOOTHING_COLORS, Smoothed, SmoothingType};
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;
use std::collections::VecDeque;

/// Rank Correlation Index (Spearman) - rank correlation of prices vs time index
/// over a rolling window, scaled to [-100, 100].
///
/// The pane draws dashed levels at 80, 0 and -80 and fills the band between the outer two. The
/// smoothing inputs add an average of the index, with bands for the band type, as they do for
/// RSI. [`Default`] is a lookback of 10 with a 14-bar SMA of the index; [`new`](Self::new) stays
/// without the average, so its values remain single numbers.
#[derive(Clone)]
pub struct RankCorrelationIndex {
    period: usize,
    smoothing: SmoothingType,
    smoothing_length: usize,
    smoothing_deviation: f64,
    values: IndicatorValues,
    stepper: Stepper<Rolling>,
    colors: Vec<Color32>,
    visible: bool,
}

/// The closes of the last `period` bars with a finite close, and the average of the index.
#[derive(Clone, Default)]
struct Rolling {
    window: VecDeque<f64>,
    smoothed: Smoothed,
}

impl RankCorrelationIndex {
    /// Create with a lookback of `period` bars, clamped to at least 2.
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(2),
            smoothing: SmoothingType::None,
            smoothing_length: 14,
            smoothing_deviation: 2.0,
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            colors: [palette::BLUE]
                .into_iter()
                .chain(SMOOTHING_COLORS)
                .collect(),
            visible: true,
        }
    }

    /// Fill average ranks for ties, reusing the sorting and output buffers.
    fn rank(closes: &VecDeque<f64>, indexed: &mut Vec<(usize, f64)>, ranks: &mut [f64]) {
        indexed.clear();
        indexed.extend(closes.iter().copied().enumerate());
        indexed.sort_unstable_by(|a, b| a.1.total_cmp(&b.1));
        let mut i = 0;
        while i < indexed.len() {
            let mut end = i + 1;
            while end < indexed.len() && indexed[end].1 == indexed[i].1 {
                end += 1;
            }
            let rank = (i + end + 1) as f64 / 2.0;
            for &(at, _) in &indexed[i..end] {
                ranks[at] = rank;
            }
            i = end;
        }
    }
}

impl Default for RankCorrelationIndex {
    fn default() -> Self {
        Self {
            smoothing: SmoothingType::Sma,
            ..Self::new(10)
        }
    }
}

impl Indicator for RankCorrelationIndex {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::Close,
        ))
    }

    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 2, usize::MAX),
        smoothing: choice("Smoothing", SmoothingType::ALL),
        smoothing_length: length("Smoothing length", 1, usize::MAX)
            if smoothing in SmoothingType::WITH_LENGTH,
        smoothing_deviation: number("Band deviation", 0.0, f64::MAX)
            if smoothing in SmoothingType::WITH_BANDS
    ; {});

    fn name(&self) -> &str {
        "RankCorr"
    }

    fn desc(&self) -> &str {
        "Rank Correlation Index (Spearman) - Price vs time rank correlation [-100, 100]"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        let (period, kind, length, deviation) = (
            self.period,
            self.smoothing,
            self.smoothing_length,
            self.smoothing_deviation,
        );
        let (mut indexed, mut ranks) = (Vec::new(), Vec::new());
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                if !bar.close.is_finite() {
                    return IndicatorValue::None;
                }
                state.window.push_back(bar.close);
                if state.window.len() > period {
                    state.window.pop_front();
                }
                if state.window.len() < period {
                    return IndicatorValue::None;
                }
                ranks.resize(period, 0.0);
                Self::rank(&state.window, &mut indexed, &mut ranks);
                let n = period as f64;
                let mean = (n + 1.0) / 2.0;
                let covariance: f64 = ranks
                    .iter()
                    .enumerate()
                    .map(|(i, rank)| (i as f64 + 1.0 - mean) * (rank - mean))
                    .sum();
                let variance: f64 = ranks.iter().map(|rank| (rank - mean).powi(2)).sum();
                if variance == 0.0 {
                    return IndicatorValue::None;
                }
                // Spearman is Pearson correlation of the ranks, including ties.
                let spearman = covariance / (variance * n * (n * n - 1.0) / 12.0).sqrt();
                // Scale to [-100, 100]
                let index = (spearman * 100.0).clamp(-100.0, 100.0);
                state
                    .smoothed
                    .next(kind, length, deviation, index, bar.volume)
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
        for (slot, color) in self.colors.iter_mut().zip(colors) {
            *slot = color;
        }
    }

    fn fills(&self) -> Vec<crate::studies::StudyFill> {
        self.smoothing.fills()
    }

    fn level_fill_color(&self) -> Color32 {
        self.colors[0].gamma_multiply_u8(26)
    }

    fn pane(&self) -> crate::studies::StudyPane {
        crate::studies::StudyPane {
            height: None,
            // Fitted to the values: the bands of the average can pass either end of the index.
            range: None,
            levels: vec![
                (80.0, palette::MUTED, "Upper Band"),
                (0.0, palette::MUTED, "Middle Band"),
                (-80.0, palette::MUTED, "Lower Band"),
            ],
            fill_between_levels: true,
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
        let mut names = vec![format!("RankCorr({})", self.period)];
        names.extend(self.smoothing.line_names("RankCorr MA"));
        names
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::studies::InputValue;

    /// Bars whose open, high and low lie away from the close the index reads.
    fn bars(closes: &[f64]) -> Vec<Bar> {
        closes
            .iter()
            .enumerate()
            .map(|(i, &close)| {
                let time = chrono::DateTime::from_timestamp(i as i64 * 60, 0).unwrap();
                Bar::new(time, close - 0.5, close + 2.0, close - 1.0, close, 1.0)
            })
            .collect()
    }

    /// The index over three closes, then its two-value average with bands 1.5 deviations away, by
    /// hand. Tied closes share their average rank, so 1, 2, 2 ranks 1, 2.5, 2.5 against the times
    /// 1, 2, 3: the covariance is 1.5 and the variances are 1.5 and 2, giving 1.5 / sqrt(3), or
    /// `r` = 50 * sqrt(3) once scaled. The bar without a close has no value and leaves both
    /// windows as they were, so the next windows are 2, 2, 1 (`-r`), 2, 1, 3 (50) and 1, 3, 5
    /// (100). The average of two values is their midpoint and their population deviation half
    /// their distance. Correcting the close of bar 4 from 1 to 4 makes the windows 2, 2, 4 (`r`),
    /// 2, 4, 3 (50) and 4, 3, 5 (50), and the average and bands follow from that bar on.
    #[test]
    fn the_average_and_bands_follow_the_index_through_ties_a_gap_and_a_correction() {
        let r = 50.0 * 3.0f64.sqrt();
        let band = |a: f64, b: f64| {
            let (average, reach) = ((a + b) / 2.0, 1.5 * (a - b).abs() / 2.0);
            [
                Some(b),
                Some(average),
                Some(average + reach),
                Some(average - reach),
            ]
        };
        let mut study = RankCorrelationIndex::new(3);
        for (id, value) in [
            ("smoothing", InputValue::Choice(2)),
            ("smoothing_length", InputValue::Length(2)),
            ("smoothing_deviation", InputValue::Number(1.5)),
        ] {
            study.set_input(id, value).unwrap();
        }
        let mut closes = [1.0, 2.0, 2.0, f64::NAN, 1.0, 3.0, 5.0];
        study.calculate(&bars(&closes));
        let saved = [
            (1, [None; 4]),
            (2, [Some(r), None, None, None]),
            (3, [None; 4]),
            (4, band(r, -r)),
            (5, band(-r, 50.0)),
            (6, band(50.0, 100.0)),
        ];
        closes[4] = 4.0;
        let mut corrected = study.clone();
        corrected.update(&bars(&closes), 4);
        let rows = [(4, band(r, r)), (5, band(r, 50.0)), (6, band(50.0, 50.0))];
        for (study, rows) in [
            (&study, &saved[..]),
            (&corrected, &saved[..3]),
            (&corrected, &rows[..]),
        ] {
            for &(bar, want) in rows {
                for (line, want) in want.into_iter().enumerate() {
                    let got = study.values().value(line, bar);
                    let equal = match (got, want) {
                        (Some(got), Some(want)) => (got - want).abs() < 1e-9,
                        (got, want) => got == want,
                    };
                    assert!(equal, "bar {bar}, line {line}: {got:?}, not {want:?}");
                }
            }
        }
    }
}
