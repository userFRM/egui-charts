use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Connors RSI (CRSI)
///
/// Composite momentum indicator combining:
/// 1. Standard RSI
/// 2. Up/Down Streak Length RSI
/// 3. Percent Rank of Rate of Change
///
/// Developed by Larry Connors
#[derive(Clone)]
pub struct ConnorsRSI {
    /// RSI period (typically 3)
    rsi_period: usize,
    /// Streak RSI period (typically 2)
    streak_period: usize,
    /// Percent Rank lookback (typically 100)
    rank_period: usize,
    values: Vec<IndicatorValue>,
    color: Color32,
    visible: bool,
}

impl ConnorsRSI {
    pub fn new() -> Self {
        Self {
            rsi_period: 3,
            streak_period: 2,
            rank_period: 100,
            values: Vec::new(),
            color: DESIGN_TOKENS.semantic.extended.pink, // Pink
            visible: true,
        }
    }

    pub fn with_periods(mut self, rsi: usize, streak: usize, rank: usize) -> Self {
        self.rsi_period = rsi;
        self.streak_period = streak;
        self.rank_period = rank;
        self
    }

    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }

    /// Calculate standard RSI
    fn calculate_rsi(data: &[f64], period: usize) -> Vec<f64> {
        if data.len() < period + 1 {
            return vec![f64::NAN; data.len()];
        }

        let mut result = vec![f64::NAN; data.len()];
        let mut gains = Vec::with_capacity(data.len());
        let mut losses = Vec::with_capacity(data.len());

        // Calculate changes
        gains.push(0.0);
        losses.push(0.0);

        for i in 1..data.len() {
            let change = data[i] - data[i - 1];
            gains.push(if change > 0.0 { change } else { 0.0 });
            losses.push(if change < 0.0 { -change } else { 0.0 });
        }

        // Calculate first avgs
        let mut avg_gain: f64 = gains[1..=period].iter().sum::<f64>() / period as f64;
        let mut avg_loss: f64 = losses[1..=period].iter().sum::<f64>() / period as f64;

        for i in period..data.len() {
            if i == period {
                // First RSI value
            } else {
                avg_gain = (avg_gain * (period - 1) as f64 + gains[i]) / period as f64;
                avg_loss = (avg_loss * (period - 1) as f64 + losses[i]) / period as f64;
            }

            let rs = if avg_loss > 0.0 {
                avg_gain / avg_loss
            } else {
                100.0
            };
            result[i] = 100.0 - (100.0 / (1.0 + rs));
        }

        result
    }

    /// Calculate up/down streak
    fn calculate_streak(data: &[Bar]) -> Vec<f64> {
        let mut streak = vec![0.0; data.len()];

        for i in 1..data.len() {
            let prev_streak = streak[i - 1];

            if data[i].close > data[i - 1].close {
                // Up day
                streak[i] = if prev_streak > 0.0 {
                    prev_streak + 1.0
                } else {
                    1.0
                };
            } else if data[i].close < data[i - 1].close {
                // Down day
                streak[i] = if prev_streak < 0.0 {
                    prev_streak - 1.0
                } else {
                    -1.0
                };
            } else {
                // Unchanged
                streak[i] = 0.0;
            }
        }

        streak
    }

    /// Calculate percent rank
    fn calculate_percent_rank(data: &[f64], period: usize) -> Vec<f64> {
        let mut result = vec![f64::NAN; data.len()];

        for i in period..data.len() {
            let current = data[i];
            if current.is_nan() {
                continue;
            }

            let window = &data[i - period..i];
            let cnt_below = window
                .iter()
                .filter(|&&v| !v.is_nan() && v < current)
                .count();

            result[i] = (cnt_below as f64 / period as f64) * 100.0;
        }

        result
    }

    /// Calculate ROC
    fn calculate_roc(data: &[Bar]) -> Vec<f64> {
        let mut roc = vec![f64::NAN; data.len()];

        for i in 1..data.len() {
            if data[i - 1].close != 0.0 {
                roc[i] = (data[i].close - data[i - 1].close) / data[i - 1].close * 100.0;
            }
        }

        roc
    }
}

impl Default for ConnorsRSI {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for ConnorsRSI {
    fn name(&self) -> &str {
        "Connors RSI"
    }

    fn desc(&self) -> &str {
        "Connors RSI - Composite momentum indicator"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        let required = self
            .rsi_period
            .max(self.streak_period)
            .max(self.rank_period)
            + 1;
        if data.len() < required {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
        }

        // 1. Calculate standard RSI of close prices
        let closes: Vec<f64> = data.iter().map(|b| b.close).collect();
        let rsi = Self::calculate_rsi(&closes, self.rsi_period);

        // 2. Calculate streak and RSI of streak
        let streak = Self::calculate_streak(data);
        let streak_rsi = Self::calculate_rsi(&streak, self.streak_period);

        // 3. Calculate percent rank of ROC
        let roc = Self::calculate_roc(data);
        let percent_rank = Self::calculate_percent_rank(&roc, self.rank_period);

        // Combine: CRSI = (RSI + StreakRSI + PercentRank) / 3
        for i in 0..data.len() {
            if rsi[i].is_nan() || streak_rsi[i].is_nan() || percent_rank[i].is_nan() {
                self.values.push(IndicatorValue::None);
            } else {
                let crsi = (rsi[i] + streak_rsi[i] + percent_rank[i]) / 3.0;
                self.values.push(IndicatorValue::Single(crsi));
            }
        }
    }

    fn values(&self) -> &[IndicatorValue] {
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
        vec![format!(
            "CRSI({},{},{})",
            self.rsi_period, self.streak_period, self.rank_period
        )]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_bar(close: f64) -> Bar {
        Bar {
            time: Utc::now(),
            open: close,
            high: close + 1.0,
            low: close - 1.0,
            close,
            volume: 1000.0,
        }
    }

    #[test]
    fn test_streak_calculation() {
        let data = vec![
            make_bar(100.0),
            make_bar(101.0), // +1
            make_bar(102.0), // +2
            make_bar(101.0), // -1
            make_bar(100.0), // -2
            make_bar(100.0), // 0
        ];

        let streak = ConnorsRSI::calculate_streak(&data);

        assert_eq!(streak[0], 0.0);
        assert_eq!(streak[1], 1.0);
        assert_eq!(streak[2], 2.0);
        assert_eq!(streak[3], -1.0);
        assert_eq!(streak[4], -2.0);
        assert_eq!(streak[5], 0.0);
    }

    #[test]
    fn test_connors_rsi_calculation() {
        let mut crsi = ConnorsRSI::new().with_periods(3, 2, 20);

        // Create enough data
        let data: Vec<Bar> = (0..50)
            .map(|i| make_bar(100.0 + (i as f64 * 0.5).sin() * 5.0))
            .collect();

        crsi.calculate(&data);

        assert_eq!(crsi.values.len(), 50);

        // Should have valid values after warm-up period
        let valid_cnt = crsi
            .values
            .iter()
            .filter(|v| matches!(v, IndicatorValue::Single(_)))
            .count();

        assert!(valid_cnt > 0);
    }

    #[test]
    fn test_connors_rsi_range() {
        let mut crsi = ConnorsRSI::new().with_periods(3, 2, 20);

        let data: Vec<Bar> = (0..100).map(|i| make_bar(100.0 + i as f64)).collect();

        crsi.calculate(&data);

        // CRSI should be bounded between 0 and 100
        for value in &crsi.values {
            if let IndicatorValue::Single(v) = value {
                assert!(*v >= 0.0 && *v <= 100.0, "CRSI {} out of range", v);
            }
        }
    }
}
