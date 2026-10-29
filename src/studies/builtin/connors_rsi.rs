use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
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
    values: IndicatorValues,
    stepper: Stepper<State>,
    color: Color32,
    visible: bool,
}

#[derive(Clone, Default)]
struct State {
    previous: Option<f64>,
    streak: f64,
    price_rsi: (usize, f64, f64),
    streak_rsi: (usize, f64, f64),
    roc: std::collections::VecDeque<f64>,
}

impl ConnorsRSI {
    /// Create with price RSI 3, streak RSI 2 and percent-rank period 100.
    pub fn new() -> Self {
        Self {
            rsi_period: 3,
            streak_period: 2,
            rank_period: 100,
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            color: palette::PINK, // Pink
            visible: true,
        }
    }

    /// Set price RSI, streak RSI and percent-rank periods in bars, each at least one.
    pub fn with_periods(mut self, rsi: usize, streak: usize, rank: usize) -> Self {
        self.rsi_period = rsi.max(1);
        self.streak_period = streak.max(1);
        self.rank_period = rank.max(1);
        self
    }

    /// Set the plot color.
    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }

    /// Resume the mean of gains and losses, then Wilder smoothing.
    fn next_rsi(state: &mut (usize, f64, f64), change: f64, period: usize) -> f64 {
        let gain = if change > 0.0 { change } else { 0.0 };
        let loss = if change < 0.0 { -change } else { 0.0 };
        if state.0 < period {
            state.0 += 1;
            state.1 += gain;
            state.2 += loss;
            if state.0 < period {
                return f64::NAN;
            }
            state.1 /= period as f64;
            state.2 /= period as f64;
        } else {
            state.1 = (state.1 * (period - 1) as f64 + gain) / period as f64;
            state.2 = (state.2 * (period - 1) as f64 + loss) / period as f64;
        }
        super::rsi::from_averages(state.1, state.2)
    }
}

impl Default for ConnorsRSI {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for ConnorsRSI {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::Close,
        ))
    }

    crate::studies::inputs::study_inputs!(self;
        rsi_period: length("RSI period", 1, usize::MAX),
        streak_period: length("Streak period", 1, usize::MAX),
        rank_period: length("Rank period", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "Connors RSI"
    }

    fn desc(&self) -> &str {
        "Connors RSI - Composite momentum indicator"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                if !bar.close.is_finite() {
                    return IndicatorValue::None;
                }
                let previous = state.previous.replace(bar.close);
                let (mut price_rsi, mut streak_rsi, mut roc) = (f64::NAN, f64::NAN, f64::NAN);
                if let Some(previous) = previous {
                    price_rsi =
                        Self::next_rsi(&mut state.price_rsi, bar.close - previous, self.rsi_period);
                    let streak = if bar.close > previous {
                        if state.streak > 0.0 {
                            state.streak + 1.0
                        } else {
                            1.0
                        }
                    } else if bar.close < previous {
                        if state.streak < 0.0 {
                            state.streak - 1.0
                        } else {
                            -1.0
                        }
                    } else {
                        0.0
                    };
                    streak_rsi = Self::next_rsi(
                        &mut state.streak_rsi,
                        streak - state.streak,
                        self.streak_period,
                    );
                    state.streak = streak;
                    if previous != 0.0 {
                        roc = (bar.close - previous) / previous * 100.0;
                    }
                }
                let rank = if state.roc.len() == self.rank_period && !roc.is_nan() {
                    state
                        .roc
                        .iter()
                        .filter(|&&v| !v.is_nan() && v < roc)
                        .count() as f64
                        / self.rank_period as f64
                        * 100.0
                } else {
                    f64::NAN
                };
                state.roc.push_back(roc);
                if state.roc.len() > self.rank_period {
                    state.roc.pop_front();
                }
                if price_rsi.is_nan() || streak_rsi.is_nan() || rank.is_nan() {
                    IndicatorValue::None
                } else {
                    IndicatorValue::Single((price_rsi + streak_rsi + rank) / 3.0)
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
            colors: None,
            payload: None,
        }
    }

    #[test]
    fn test_streak_calculation() {
        let data = [
            make_bar(100.0),
            make_bar(101.0), // +1
            make_bar(102.0), // +2
            make_bar(101.0), // -1
            make_bar(100.0), // -2
            make_bar(100.0), // 0
        ];

        let mut study = ConnorsRSI::new();
        for (i, expected) in [0.0, 1.0, 2.0, -1.0, -2.0, 0.0].into_iter().enumerate() {
            study.update(&data[..=i], i);
            assert_eq!(study.stepper.state().streak, expected);
        }
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
            .to_rows()
            .iter()
            .filter(|v| matches!(v, IndicatorValue::Single(_)))
            .count();

        assert!(valid_cnt > 0);
    }

    /// A flat series has no gains or losses: both RSI components are the neutral 50 and no
    /// return ranks above an equal one, so the composite is (50 + 50 + 0) / 3.
    #[test]
    fn flat_series_uses_neutral_rsi() {
        let mut crsi = ConnorsRSI::new().with_periods(3, 2, 5);
        let data: Vec<Bar> = (0..20).map(|_| make_bar(100.0)).collect();
        crsi.calculate(&data);
        match crsi.values.to_rows()[19] {
            IndicatorValue::Single(v) => assert!((v - 100.0 / 3.0).abs() < 1e-9),
            _ => panic!("expected a value after warmup"),
        }
    }

    #[test]
    fn test_connors_rsi_range() {
        let mut crsi = ConnorsRSI::new().with_periods(3, 2, 20);

        let data: Vec<Bar> = (0..100).map(|i| make_bar(100.0 + i as f64)).collect();

        crsi.calculate(&data);

        // CRSI should be bounded between 0 and 100
        let mut checked = 0;
        for value in &crsi.values.to_rows() {
            if let IndicatorValue::Single(v) = value {
                assert!(*v >= 0.0 && *v <= 100.0, "CRSI {v} out of range");
                checked += 1;
            }
        }
        assert!(checked > 0, "no value to check");
    }
}
