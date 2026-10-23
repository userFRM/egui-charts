use super::over_finite_bars;
use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Rank Correlation Index (Spearman) - rank correlation of prices vs time index
/// over a rolling window, scaled to [-100, 100].
#[derive(Clone)]
pub struct RankCorrelationIndex {
    period: usize,
    values: IndicatorValues,
    colors: Vec<Color32>,
    visible: bool,
}

impl RankCorrelationIndex {
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(2),
            values: IndicatorValues::new(),
            colors: vec![DESIGN_TOKENS.semantic.extended.indigo],
            visible: true,
        }
    }

    /// Fill average ranks for ties, reusing the sorting and output buffers.
    fn rank(data: &[Bar], indexed: &mut Vec<(usize, f64)>, ranks: &mut [f64]) {
        indexed.clear();
        indexed.extend(data.iter().enumerate().map(|(i, b)| (i, b.close)));
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

impl RankCorrelationIndex {
    /// The values over `data`, every bar of which has finite inputs.
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let mut values = IndicatorValues::new();
        if data.is_empty() {
            return values;
        }

        let mut indexed = Vec::with_capacity(self.period.min(data.len()));
        let mut ranks = vec![0.0; self.period.min(data.len())];
        for i in 0..data.len() {
            if i + 1 < self.period {
                values.push(IndicatorValue::None);
                continue;
            }

            let start = i + 1 - self.period;
            Self::rank(&data[start..=i], &mut indexed, &mut ranks);
            let n = self.period as f64;
            let mean = (n + 1.0) / 2.0;
            let covariance: f64 = ranks
                .iter()
                .enumerate()
                .map(|(i, rank)| (i as f64 + 1.0 - mean) * (rank - mean))
                .sum();
            let variance: f64 = ranks.iter().map(|rank| (rank - mean).powi(2)).sum();
            if variance == 0.0 {
                values.push(IndicatorValue::None);
                continue;
            }
            // Spearman is Pearson correlation of the ranks, including ties.
            let spearman = covariance / (variance * n * (n * n - 1.0) / 12.0).sqrt();

            // Scale to [-100, 100]
            values.push(IndicatorValue::Single(
                (spearman * 100.0).clamp(-100.0, 100.0),
            ));
        }

        values
    }
}

impl Indicator for RankCorrelationIndex {
    fn name(&self) -> &str {
        "RankCorr"
    }

    fn desc(&self) -> &str {
        "Rank Correlation Index (Spearman) - Price vs time rank correlation [-100, 100]"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values = over_finite_bars(data, |b| b.close.is_finite(), |data| self.compute(data));
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        self.colors.clone()
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if !colors.is_empty() {
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
        vec![format!("RankCorr({})", self.period)]
    }
}
