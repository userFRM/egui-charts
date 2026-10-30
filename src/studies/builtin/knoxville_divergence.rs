//! Knoxville Divergence.
//!
//! Marks a divergence between the swings of the price and of its momentum, when the RSI confirms
//! an extreme: a lower low in price with a higher low in momentum below an oversold RSI is
//! bullish, a higher high with a lower high in momentum above an overbought RSI is bearish.
//!
//! # Rule
//!
//! Momentum is the close minus the close `momentum_period` bars before, and the RSI is Wilder's
//! ([`RSI`](super::RSI)) of the close. Price pivots are found with `left` and `right` bars on each
//! side as in [`PivotPointsHighLow`](super::PivotPointsHighLow), so a pivot is known, and confirmed,
//! `right` bars after its bar. Each pivot low `p` is compared with the previous pivot low `q` when
//! `p - q <= lookback` bars:
//!
//! ```text
//! Bullish: low[p]  < low[q]   and momentum[p] > momentum[q]  and RSI[p] <= oversold
//! Bearish: high[p] > high[q]  and momentum[p] < momentum[q]  and RSI[p] >= overbought  (pivot highs)
//! ```
//!
//! A signal is marked on its confirmation bar, `right` bars after the pivot `p`, at the momentum
//! of `p`, and never moves. A pivot or momentum without a value, in the warm-up, takes no part.
//! Bars with a non-finite high, low or close are skipped. The defaults are a momentum of 20 bars, an
//! RSI of 14, 5 bars on each side of a pivot, a lookback of 50 bars, and an RSI of 30 and 70.

use super::rsi::State as RsiState;
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;
use std::collections::VecDeque;

/// Knoxville Divergence: price and momentum swings that disagree at an RSI extreme.
#[derive(Clone)]
pub struct KnoxvilleDivergence {
    momentum_period: usize,
    rsi_period: usize,
    left: usize,
    right: usize,
    lookback: usize,
    oversold: usize,
    overbought: usize,
    values: IndicatorValues,
    stepper: Stepper<State>,
    colors: [Color32; 3],
    visible: bool,
}

#[derive(Clone, Copy)]
struct Pivot {
    position: usize,
    high: f64,
    low: f64,
    momentum: f64,
    rsi: f64,
}

#[derive(Clone, Default)]
struct State {
    position: usize,
    rsi: RsiState,
    closes: VecDeque<f64>,
    window: VecDeque<Pivot>,
    /// The previous pivot low and pivot high.
    previous: [Option<Pivot>; 2],
}

impl KnoxvilleDivergence {
    /// Momentum and RSI of the given periods and pivots with `left` and `right` bars on each side,
    /// each raised to at least 1, divergences between pivots at most `lookback` bars apart, and
    /// RSI limits of 30 and 70, set through the `oversold` and `overbought` inputs.
    pub fn new(
        momentum_period: usize,
        rsi_period: usize,
        left: usize,
        right: usize,
        lookback: usize,
    ) -> Self {
        Self {
            momentum_period: momentum_period.max(1),
            rsi_period: rsi_period.max(1),
            left: left.max(1),
            right: right.max(1),
            lookback: lookback.max(1),
            oversold: 30,
            overbought: 70,
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            colors: [palette::ACCENT, palette::GREEN, palette::RED],
            visible: true,
        }
    }
}

/// Construct with the conventional default parameters.
impl Default for KnoxvilleDivergence {
    fn default() -> Self {
        Self::new(20, 14, 5, 5, 50)
    }
}

impl Indicator for KnoxvilleDivergence {
    crate::studies::inputs::study_inputs!(self;
        momentum_period: length("Momentum period", 1, usize::MAX),
        rsi_period: length("RSI period", 1, usize::MAX),
        left: length("Pivot bars left", 1, usize::MAX),
        right: length("Pivot bars right", 1, usize::MAX),
        lookback: length("Bars between pivots", 1, usize::MAX),
        oversold: length("RSI oversold", 0, 100),
        overbought: length("RSI overbought", 0, 100)
    ; {});

    fn name(&self) -> &str {
        "Knoxville Divergence"
    }

    fn desc(&self) -> &str {
        "Knoxville Divergence - Price and momentum swings that disagree at an RSI extreme"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        let keep = self.left.saturating_add(self.right).saturating_add(1);
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                if ![bar.high, bar.low, bar.close].iter().all(|v| v.is_finite()) {
                    return IndicatorValue::None;
                }
                let rsi = match state.rsi.next(bar, self.rsi_period) {
                    IndicatorValue::Single(value) if value.is_finite() => value,
                    _ => f64::NAN,
                };
                state.closes.push_back(bar.close);
                if state.closes.len() > self.momentum_period.saturating_add(1) {
                    state.closes.pop_front();
                }
                let momentum = if state.closes.len() > self.momentum_period {
                    bar.close - state.closes[0]
                } else {
                    f64::NAN
                };
                state.window.push_back(Pivot {
                    position: state.position,
                    high: bar.high,
                    low: bar.low,
                    momentum,
                    rsi,
                });
                state.position += 1;
                if state.window.len() > keep {
                    state.window.pop_front();
                }
                let mut row = [momentum, f64::NAN, f64::NAN];
                if let Some(at) = state
                    .window
                    .len()
                    .checked_sub(self.right.saturating_add(1))
                    .filter(|&at| at >= self.left)
                {
                    let p = state.window[at];
                    for high in [false, true] {
                        let price = |pivot: &Pivot| if high { pivot.high } else { pivot.low };
                        let before = state.window.range(at - self.left..at).all(|v| {
                            if high {
                                price(v) <= price(&p)
                            } else {
                                price(v) >= price(&p)
                            }
                        });
                        let after = state.window.range(at + 1..).all(|v| {
                            if high {
                                price(v) < price(&p)
                            } else {
                                price(v) > price(&p)
                            }
                        });
                        if !(before && after) {
                            continue;
                        }
                        let Some(q) = state.previous[usize::from(high)].replace(p) else {
                            continue;
                        };
                        let diverges = if high {
                            p.high > q.high
                                && p.momentum < q.momentum
                                && p.rsi >= self.overbought as f64
                        } else {
                            p.low < q.low
                                && p.momentum > q.momentum
                                && p.rsi <= self.oversold as f64
                        };
                        // A NaN momentum or RSI fails every comparison above.
                        if diverges && p.position - q.position <= self.lookback {
                            row[1 + usize::from(high)] = p.momentum;
                        }
                    }
                }
                IndicatorValue::Multiple(row.to_vec())
            });
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn plot_style(&self, line: usize) -> crate::studies::PlotStyle {
        crate::studies::PlotStyle {
            color: self.colors.get(line).copied().unwrap_or(Color32::WHITE),
            width: 1.0,
            pattern: crate::model::LineStyle::Solid,
            plot_type: match line {
                0 => crate::studies::PlotType::Line,
                1 => crate::studies::PlotType::TriangleUp,
                _ => crate::studies::PlotType::TriangleDown,
            },
            visible: true,
        }
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        self.colors.to_vec()
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        for (old, new) in self.colors.iter_mut().zip(colors) {
            *old = new;
        }
    }

    fn pane(&self) -> crate::studies::StudyPane {
        crate::studies::StudyPane {
            levels: vec![(0.0, palette::GRAY, "Zero")],
            ..Default::default()
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
        ["Momentum", "Bullish Divergence", "Bearish Divergence"]
            .map(String::from)
            .to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::studies::InputValue;

    /// Momentum of 1 bar, an RSI of 2 bars, one bar on each side of a pivot, flat bars. Closes
    /// 10, 8, 9, 7.5, 8.5 have pivot lows 8 at bar 1 and 7.5 at bar 3, a lower low whose momentum
    /// -1.5 beats -2. The RSI at bar 3: changes -2, +1, -1.5 average (Wilder, seeded by the first two)
    /// gain 0.5 then 0.25, loss 1 then 1.25, so RS = 0.2 and RSI = 100 - 100 / 1.2 = 16.67. The
    /// signal is marked on bar 4, where the pivot is confirmed. Negated prices give the bearish
    /// mirror with an RSI of 83.33.
    #[test]
    fn divergences_need_pivots_in_range_and_an_rsi_extreme() {
        let closes = [10.0, 8.0, 9.0, 7.5, 8.5];
        let bullish = super::super::hlc_bars(&closes.map(|c| (c, c, c)));
        let bearish = super::super::hlc_bars(&closes.map(|c| (-c, -c, -c)));
        let none = None;
        let signal = |bars: &[Bar], study: &KnoxvilleDivergence, line| {
            let mut study = study.clone();
            study.calculate(bars);
            (0..bars.len())
                .map(|at| study.values().value(line, at))
                .collect::<Vec<_>>()
        };
        let study = KnoxvilleDivergence::new(1, 2, 1, 1, 10);
        super::super::assert_replay_matches_batch(&study, &bullish);
        super::super::assert_replay_matches_batch(&study, &bearish);
        assert_eq!(
            signal(&bullish, &study, 1),
            [none, none, none, none, Some(-1.5)]
        );
        assert_eq!(signal(&bullish, &study, 2), [none; 5]);
        assert_eq!(
            signal(&bearish, &study, 2),
            [none, none, none, none, Some(1.5)]
        );
        assert_eq!(signal(&bearish, &study, 1), [none; 5]);
        // The pivots are 2 bars apart, the RSI 16.67 is over a limit of 10, and the mirror's 83.33 under 90.
        let limit = |id: &str, value: usize| {
            let mut study = study.clone();
            study.set_input(id, InputValue::Length(value)).unwrap();
            study
        };
        for (study, bars, line) in [
            (KnoxvilleDivergence::new(1, 2, 1, 1, 1), &bullish, 1),
            (limit("oversold", 10), &bullish, 1),
            (limit("overbought", 90), &bearish, 2),
        ] {
            assert_eq!(signal(bars, &study, line), [none; 5]);
        }
    }
}
