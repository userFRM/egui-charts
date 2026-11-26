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
/// 3. Percent rank of the one-bar return: the share of the previous rank-period bars whose
///    return is at or below the latest one. The first bar, and a bar after a zero close, have
///    no return and count as not at or below.
///
/// The pane starts with an upper guide at 70, a lower guide at 30 and a light blue fill between
/// them. Applied study settings edit each guide and the fill independently.
///
/// Developed by Larry Connors
#[derive(Clone, Debug)]
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

#[derive(Clone, Debug, Default)]
struct State {
    previous: Option<f64>,
    streak: f64,
    price_rsi: (usize, f64, f64),
    streak_rsi: (usize, f64, f64),
    /// The one-bar returns of the last rank-period bars, NaN for a bar without one.
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
            color: palette::BLUE,
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
                let (mut price_rsi, mut streak_rsi, mut roc) = (f64::NAN, f64::NAN, None);
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
                        roc = Some((bar.close - previous) / previous * 100.0);
                    }
                } else {
                    // The first bar is neither equal to nor above a missing close.
                    state.streak = -1.0;
                }
                // The share of the previous `rank_period` bars whose return is at or below this
                // one. A bar without a return holds NaN, which is never at or below, and has no
                // rank itself.
                let roc = roc.unwrap_or(f64::NAN);
                let mut rank = f64::NAN;
                if state.roc.len() == self.rank_period {
                    if !roc.is_nan() {
                        rank = state.roc.iter().filter(|&&v| v <= roc).count() as f64
                            / self.rank_period as f64
                            * 100.0;
                    }
                    state.roc.pop_front();
                }
                state.roc.push_back(roc);
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

    fn level_fill_color(&self) -> Color32 {
        palette::LIGHT_BLUE.gamma_multiply_u8(26)
    }

    fn pane(&self) -> crate::studies::StudyPane {
        crate::studies::StudyPane {
            height: None,
            range: Some((0.0, 100.0)),
            levels: vec![
                (70.0, palette::MUTED, "Upper guide"),
                (30.0, palette::MUTED, "Lower guide"),
            ],
            fill_between_levels: true,
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

    /// Asserts each bar's value, `None` where the study has none, to 1e-9.
    fn assert_values(study: &mut ConnorsRSI, closes: &[f64], expected: &[Option<f64>]) {
        let data: Vec<Bar> = closes.iter().copied().map(make_bar).collect();
        study.calculate(&data);
        assert_eq!(study.values().len(), expected.len());
        for (bar, &want) in expected.iter().enumerate() {
            let got = study.values().value(0, bar);
            let same = match (got, want) {
                (Some(got), Some(want)) => (got - want).abs() < 1e-9,
                (got, want) => got.is_none() && want.is_none(),
            };
            assert!(same, "bar {bar}: {got:?} != {want:?}");
        }
    }

    /// The first bar is neither equal to nor above a missing close, so the streaks run -1, 1, 2,
    /// -1, -2, 0, 0. Wilder RSI(2) of the closes reads 100, 50, 25, 25, 25 and of the streaks 100,
    /// 100/3, 300/13, 1900/29, 1900/29 from bar 2. From bar 2 the rank counts the two previous
    /// bars, the first without a return, so bar 2 ranks 100/101 percent above neither: 0. The
    /// returns 1, 100/101, -100/102, -100/101, 0 and 0 percent rank 0, 0, 100 and, the last tied
    /// with the zero before it, 100 percent. RSI(3) of the closes first reads 200/3 at bar 3 and
    /// 400/9 at bar 5, where the rank over five bars is 40.
    #[test]
    fn test_streak_calculation() {
        let closes = [100.0, 101.0, 102.0, 101.0, 100.0, 100.0, 100.0];
        let rows = [
            (
                (2, 2, 2),
                [
                    None,
                    None,
                    Some(200.0 / 3.0),
                    Some(250.0 / 9.0),
                    Some(625.0 / 39.0),
                    Some(5525.0 / 87.0),
                    Some(5525.0 / 87.0),
                ],
            ),
            (
                (3, 2, 2),
                [
                    None,
                    None,
                    None,
                    Some(100.0 / 3.0),
                    Some(7900.0 / 351.0),
                    Some(54800.0 / 783.0),
                    Some(54800.0 / 783.0),
                ],
            ),
            (
                (3, 2, 5),
                [
                    None,
                    None,
                    None,
                    None,
                    None,
                    Some(39140.0 / 783.0),
                    Some(44360.0 / 783.0),
                ],
            ),
        ];
        for ((rsi, streak, rank), expected) in rows {
            let mut study = ConnorsRSI::new().with_periods(rsi, streak, rank);
            assert_values(&mut study, &closes, &expected);
        }
    }

    /// Doubling closes only gain, so both RSI parts read 100 once warm, and every return is 100
    /// percent. Bar 100 ranks over bars 0 to 99: bar 0 has no return, the other 99 tie, so the
    /// rank is 99 and the value (100 + 100 + 99) / 3. Bar 101 ranks over 100 tied returns.
    #[test]
    fn first_value_at_rank_period() {
        let closes: Vec<f64> = (0..102).map(|i| 2f64.powi(i)).collect();
        let mut expected = vec![None; 100];
        expected.extend([Some(299.0 / 3.0), Some(100.0)]);
        assert_values(&mut ConnorsRSI::new(), &closes, &expected);
    }

    /// The bar after the zero close has no return and takes a rank slot that is never at or
    /// below. Bar 3's return of 100 percent ranks over -100 and that slot: 50, and
    /// (100 + 100 + 50) / 3. Bar 4's 50 percent ranks over that slot and 100: 0.
    #[test]
    fn bar_without_return_fills_a_rank_slot() {
        let expected = [None, None, None, Some(250.0 / 3.0), Some(200.0 / 3.0)];
        let closes = [1.0, 0.0, 1.0, 2.0, 3.0];
        assert_values(
            &mut ConnorsRSI::new().with_periods(1, 1, 2),
            &closes,
            &expected,
        );
    }

    /// A flat series has no losses, so both RSI components are 100, and each return ties with the
    /// previous ones and ranks at 100: (100 + 100 + 100) / 3.
    #[test]
    fn flat_series_reads_100() {
        let mut crsi = ConnorsRSI::new().with_periods(3, 2, 5);
        let data: Vec<Bar> = (0..20).map(|_| make_bar(100.0)).collect();
        crsi.calculate(&data);
        match crsi.values.to_rows()[19] {
            IndicatorValue::Single(v) => assert!((v - 100.0).abs() < 1e-9),
            _ => panic!("expected a value after warmup"),
        }
    }
}
