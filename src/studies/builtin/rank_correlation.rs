use super::smoothing::{SMOOTHING_COLORS, Smoothed, SmoothingType};
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;
use std::collections::VecDeque;

/// Rank Correlation Index (Spearman) - rank correlation of prices vs time index
/// over a rolling window, scaled to [-100, 100].
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
        Self::new(14)
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
