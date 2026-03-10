use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Rank Correlation Index (Spearman) - rank correlation of prices vs time index
/// over a rolling window, scaled to [-100, 100].
#[derive(Clone)]
pub struct RankCorrelationIndex {
    period: usize,
    values: Vec<IndicatorValue>,
    colors: Vec<Color32>,
    visible: bool,
}

impl RankCorrelationIndex {
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(2),
            values: Vec::new(),
            colors: vec![DESIGN_TOKENS.semantic.extended.indigo],
            visible: true,
        }
    }

    /// Compute ranks for a slice of f64 values (average rank for ties).
    fn rank(values: &[f64]) -> Vec<f64> {
        let n = values.len();
        let mut indexed: Vec<(usize, f64)> = values.iter().copied().enumerate().collect();
        indexed.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        let mut ranks = vec![0.0; n];
        let mut i = 0;
        while i < n {
            let mut j = i;
            while j < n - 1 && (indexed[j + 1].1 - indexed[j].1).abs() < 1e-15 {
                j += 1;
            }
            let avg_rank = (i + j) as f64 / 2.0 + 1.0;
            for k in i..=j {
                ranks[indexed[k].0] = avg_rank;
            }
            i = j + 1;
        }
        ranks
    }
}

impl Default for RankCorrelationIndex {
    fn default() -> Self {
        Self::new(14)
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
        self.values.clear();

        if data.is_empty() {
            return;
        }

        for i in 0..data.len() {
            if i + 1 < self.period {
                self.values.push(IndicatorValue::None);
                continue;
            }

            let start = i + 1 - self.period;
            let prices: Vec<f64> = data[start..=i].iter().map(|b| b.close).collect();
            let n = self.period as f64;

            let price_ranks = Self::rank(&prices);
            // Time ranks are simply 1, 2, ..., n
            let time_ranks: Vec<f64> = (1..=self.period).map(|t| t as f64).collect();

            // Spearman = 1 - 6 * sum(d^2) / (n * (n^2 - 1))
            let sum_d2: f64 = price_ranks
                .iter()
                .zip(time_ranks.iter())
                .map(|(pr, tr)| (pr - tr).powi(2))
                .sum();

            let denom = n * (n * n - 1.0);
            let spearman = if denom.abs() < 1e-15 {
                0.0
            } else {
                1.0 - 6.0 * sum_d2 / denom
            };

            // Scale to [-100, 100]
            self.values.push(IndicatorValue::Single(
                (spearman * 100.0).clamp(-100.0, 100.0),
            ));
        }
    }

    fn values(&self) -> &[IndicatorValue] {
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
