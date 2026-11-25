//! Booker Reversal.
//!
//! Marks a bar where the MACD line crosses zero while the Stochastic %K stands at the opposite
//! extreme, a momentum turn against a stretched range.
//!
//! # Rule
//!
//! The MACD line is the exponential average of the close over the fast period less the one over
//! the slow period, as [`MACD`] computes it. %K is the [`Stochastic`] of `k_period` bars
//! smoothed by a simple average over `slowing` bars; a slowing of 0 or 1 leaves it unsmoothed.
//!
//! ```text
//! Buy:   MACD crosses above 0  and  %K < lower
//! Sell:  MACD crosses below 0  and  %K > upper
//! ```
//!
//! A cross compares the MACD with the sign of its latest nonzero value: a value of exactly zero
//! keeps that sign, and a bar without a MACD value forgets it, as
//! [`helpers::crossover`](crate::studies::helpers::crossover) does. The buy output holds the
//! bar's low and the sell output its high, so the triangles sit beneath and above the bar. A mark
//! is known when its bar closes and never moves. The defaults are a fast period of 12, a slow
//! period of 26, a %K of 70 bars slowed over 10, and levels of 70 and 30, which take decimals.
//! The buy triangles are teal and the sell triangles red, both at half opacity.

use super::{MACD, Stochastic, clamp_param};
use crate::model::Bar;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;
use std::cmp::Ordering;

/// Booker Reversal: MACD zero crosses confirmed by a Stochastic extreme.
#[derive(Clone, Debug)]
pub struct BookerReversal {
    fast_period: usize,
    slow_period: usize,
    k_period: usize,
    slowing: usize,
    stoch_upper: f64,
    stoch_lower: f64,
    macd: MACD,
    stochastic: Stochastic,
    values: IndicatorValues,
    stepper: Stepper<State>,
    colors: [Color32; 2],
    visible: bool,
}

#[derive(Clone, Debug, Default)]
struct State {
    /// The index of the next bar.
    bar: usize,
    /// Bars with a finite close; the MACD has no value before `slow_period` of them.
    closes: usize,
    /// The sign of the latest nonzero MACD value since the last bar without one.
    side: Option<Ordering>,
}

impl BookerReversal {
    /// MACD averages of `fast_period` and `slow_period` bars (1 to 2000) and a %K of `k_period`
    /// bars (1 to 100) slowed over `slowing` bars (at most 100), with levels of 70 and 30.
    pub fn new(fast_period: usize, slow_period: usize, k_period: usize, slowing: usize) -> Self {
        let (fast_period, slow_period) = (fast_period.clamp(1, 2000), slow_period.clamp(1, 2000));
        let (k_period, slowing) = (k_period.clamp(1, 100), slowing.min(100));
        Self {
            fast_period,
            slow_period,
            k_period,
            slowing,
            stoch_upper: 70.0,
            stoch_lower: 30.0,
            macd: MACD::new(fast_period, slow_period, 1),
            stochastic: Stochastic::new(k_period, slowing, 1),
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            // Teal and red at half opacity.
            colors: [
                Color32::from_rgba_unmultiplied(0x00, 0x96, 0x88, 128),
                Color32::from_rgba_unmultiplied(0xF4, 0x43, 0x36, 128),
            ],
            visible: true,
        }
    }

    /// The %K levels a sell must exceed and a buy must stay under, each clamped to 0 to 100;
    /// NaN counts as 0.
    pub fn with_levels(mut self, upper: f64, lower: f64) -> Self {
        self.stoch_upper = clamp_param(upper, 0.0, 100.0);
        self.stoch_lower = clamp_param(lower, 0.0, 100.0);
        self
    }
}

/// Construct with the conventional default parameters.
impl Default for BookerReversal {
    fn default() -> Self {
        Self::new(12, 26, 70, 10)
    }
}

impl Indicator for BookerReversal {
    crate::studies::inputs::study_inputs!(self;
        fast_period: length("Fast MA Period", 1, 2000),
        slow_period: length("Slow MA Period", 1, 2000),
        k_period: length("K Period", 1, 100),
        slowing: length("Slowing", 0, 100),
        stoch_upper: number("Stochastic Upper", 0.0, 100.0),
        stoch_lower: number("Stochastic Lower", 0.0, 100.0)
    ; {
        self.macd = MACD::new(self.fast_period, self.slow_period, 1);
        self.stochastic = Stochastic::new(self.k_period, self.slowing, 1);
    });

    fn name(&self) -> &str {
        "Booker Reversal"
    }

    fn desc(&self) -> &str {
        "Booker Reversal - MACD zero crosses confirmed by a Stochastic extreme"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        // The MACD reveals all its values once it has `slow_period` closes, as `MACD` does.
        let from = if self.stepper.state().closes < self.slow_period {
            0
        } else {
            from
        };
        self.macd.update(data, from);
        self.stochastic.update(data, from);
        let (macd, stochastic) = (self.macd.values(), self.stochastic.values());
        let (upper, lower) = (self.stoch_upper, self.stoch_lower);
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                let at = state.bar;
                state.bar += 1;
                if bar.close.is_finite() {
                    state.closes += 1;
                }
                let Some(line) = macd.value(0, at) else {
                    state.side = None;
                    return IndicatorValue::None;
                };
                let side = line.partial_cmp(&0.0).filter(|side| side.is_ne());
                let mut row = [f64::NAN; 2];
                if let (Some(side), Some(previous), Some(k)) =
                    (side, state.side, stochastic.value(0, at))
                {
                    if side == Ordering::Greater && previous == Ordering::Less && k < lower {
                        row[0] = bar.low;
                    }
                    if side == Ordering::Less && previous == Ordering::Greater && k > upper {
                        row[1] = bar.high;
                    }
                }
                state.side = side.or(state.side);
                IndicatorValue::Multiple(row.to_vec())
            });
        // Removing the bar that completed the slow average hides every MACD value again.
        if self.stepper.state().closes < self.slow_period {
            self.values.fill(IndicatorValue::None);
        }
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn plot_style(&self, line: usize) -> crate::studies::PlotStyle {
        crate::studies::PlotStyle {
            color: self.colors.get(line).copied().unwrap_or(Color32::WHITE),
            width: 1.0,
            pattern: crate::model::LineStyle::Solid,
            plot_type: if line == 0 {
                crate::studies::PlotType::TriangleUp
            } else {
                crate::studies::PlotType::TriangleDown
            },
            visible: true,
        }
    }

    fn plot_shape_side(&self, line: usize) -> crate::studies::ShapeSide {
        // The buy output holds the bar's low, the sell one its high.
        if line == 0 {
            crate::studies::ShapeSide::Below
        } else {
            crate::studies::ShapeSide::Above
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
        vec!["Buy".to_string(), "Sell".to_string()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// MACD of 1 and 2 bars and an unslowed 3-bar %K, levels 70 and 30. With fast 1 the MACD line
    /// is close - EMA2, and EMA2[t] = (2 close[t] + EMA2[t - 1]) / 3 from EMA2[0] = close[0], so
    /// MACD[t] = (close[t] - EMA2[t - 1]) / 3. Closes 10, 13, 11 give MACD 0, 1, -1/3: a cross
    /// below zero on bar 2. Closes 10, 7, 9 give 0, -1, 1/3: a cross above. %K on bar 2 is
    /// 100 (close - lowest low) / (highest high - lowest low) over bars 0 to 2, and only the first
    /// bar's high or low moves it: (11 - 3) / (13 - 3) = 80 and (11 - 8) / (13 - 8) = 60 on the
    /// way down, (9 - 7) / (17 - 7) = 20 and (9 - 7) / (12 - 7) = 40 on the way up.
    ///
    /// The levels take decimals. With an upper level of 72.5 and the first bar's low at 3, a
    /// last bar of high 12 and low 10 leaves the range at 3 to 13: a close of 10.26 gives a MACD
    /// of (10.26 - 12) / 3 and a %K of 100 (10.26 - 3) / 10 = 72.6, a sell, and a close of 10.24
    /// a %K of 72.4, no sell. A level cut to 72 would mark both and one rounded to 73 neither.
    #[test]
    fn marks_need_a_macd_zero_cross_at_a_stochastic_extreme() {
        // (label, (high, low, close) bars, MACD, %K on bar 2, upper and lower level, marks)
        type Row = (
            &'static str,
            [(f64, f64, f64); 3],
            [f64; 3],
            f64,
            [f64; 2],
            [Option<f64>; 2],
        );
        let down = [0.0, 1.0, -1.0 / 3.0];
        let up = [0.0, -1.0, 1.0 / 3.0];
        let rows: [Row; 6] = [
            (
                "sell",
                [(10.0, 3.0, 10.0), (13.0, 12.0, 13.0), (12.0, 10.5, 11.0)],
                down,
                80.0,
                [70.0, 30.0],
                [None, Some(12.0)],
            ),
            (
                "sell filtered",
                [(10.0, 8.0, 10.0), (13.0, 12.0, 13.0), (12.0, 10.5, 11.0)],
                down,
                60.0,
                [70.0, 30.0],
                [None, None],
            ),
            (
                "buy",
                [(17.0, 9.0, 10.0), (8.0, 7.0, 7.0), (9.5, 8.5, 9.0)],
                up,
                20.0,
                [70.0, 30.0],
                [Some(8.5), None],
            ),
            (
                "buy filtered",
                [(12.0, 9.0, 10.0), (8.0, 7.0, 7.0), (9.5, 8.5, 9.0)],
                up,
                40.0,
                [70.0, 30.0],
                [None, None],
            ),
            (
                "sell over a decimal level",
                [(10.0, 3.0, 10.0), (13.0, 12.0, 13.0), (12.0, 10.0, 10.26)],
                [0.0, 1.0, (10.26 - 12.0) / 3.0],
                72.6,
                [72.5, 30.0],
                [None, Some(12.0)],
            ),
            (
                "sell under a decimal level",
                [(10.0, 3.0, 10.0), (13.0, 12.0, 13.0), (12.0, 10.0, 10.24)],
                [0.0, 1.0, (10.24 - 12.0) / 3.0],
                72.4,
                [72.5, 30.0],
                [None, None],
            ),
        ];
        for (label, hlc, macd, k, levels, marks) in rows {
            let bars = super::super::hlc_bars(&hlc);
            let mut line = MACD::new(1, 2, 1);
            line.calculate(&bars);
            let mut stochastic = Stochastic::new(3, 0, 1);
            stochastic.calculate(&bars);
            for (at, want) in macd.into_iter().enumerate() {
                let got = line.values().value(0, at).unwrap();
                assert!((got - want).abs() < 1e-12, "{label}: MACD {at} {got}");
            }
            let got = stochastic.values().value(0, 2).unwrap();
            assert!((got - k).abs() < 1e-12, "{label}: %K {got}");

            let mut study = BookerReversal::new(1, 2, 3, 0);
            for (id, level) in [("stoch_upper", levels[0]), ("stoch_lower", levels[1])] {
                study
                    .set_input(id, crate::studies::InputValue::Number(level))
                    .unwrap();
            }
            super::super::assert_replay_matches_batch(&study, &bars);
            study.calculate(&bars);
            for (plot, want) in marks.into_iter().enumerate() {
                let got: Vec<_> = (0..3).map(|at| study.values().value(plot, at)).collect();
                assert_eq!(got, [None, None, want], "{label}: plot {plot}");
            }
        }
    }
}
