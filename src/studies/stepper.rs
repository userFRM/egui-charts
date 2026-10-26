//! Shared driver for studies computed one bar at a time.
//! Also the rolling highest and lowest values that window studies share.

#[cfg(feature = "studies")]
pub(crate) use super::helpers::{
    highest as rolling_max, lowest as rolling_min, sma as rolling_mean,
};
use crate::model::Bar;
use crate::studies::IndicatorValue;
use crate::studies::IndicatorValues;

/// Runs a causal study one `step` per bar and keeps the state before the last bar, so a
/// last-bar update or an append resumes instead of recomputing the whole series.
/// `calculate` is `run(data, 0, ..)`, so batch and streaming share one code path.
///
/// A custom study with a three-bar mean of squared closes, available without
/// the `studies` feature:
///
/// ```
/// use egui_charts::model::Bar;
/// use egui_charts::studies::{helpers, Indicator, IndicatorValue, IndicatorValues, Stepper};
/// use std::collections::VecDeque;
///
/// #[derive(Clone, Default)]
/// struct MeanSquare {
///     driver: Stepper<VecDeque<f64>>,
///     values: IndicatorValues,
/// }
/// impl Indicator for MeanSquare {
///     fn name(&self) -> &str { "Mean square" }
///     fn calculate(&mut self, bars: &[Bar]) { self.update(bars, 0); }
///     fn update(&mut self, bars: &[Bar], from: usize) {
///         self.driver.run(bars, from, &mut self.values, |window, bar| {
///             window.push_back(bar.close * bar.close);
///             if window.len() > 3 { window.pop_front(); }
///             let mean = helpers::sma(window.make_contiguous(), 3);
///             IndicatorValue::Single(*mean.last().unwrap())
///         });
///     }
///     fn is_incremental(&self) -> bool { true }
///     fn values(&self) -> &IndicatorValues { &self.values }
///     fn colors(&self) -> Vec<ecolor::Color32> { vec![ecolor::Color32::YELLOW] }
///     fn set_colors(&mut self, _: Vec<ecolor::Color32>) {}
///     fn set_visible(&mut self, _: bool) {}
///     fn clone_box(&self) -> Box<dyn Indicator> { Box::new(self.clone()) }
/// }
/// let mut bars: Vec<_> = [1.0, 2.0, 4.0].into_iter().enumerate().map(|(i, p)| {
///     Bar::new(chrono::DateTime::from_timestamp(i as i64, 0).unwrap(), p, p, p, p, 1.0)
/// }).collect();
/// let mut study = MeanSquare::default();
/// study.calculate(&bars);
/// assert_eq!(study.values().value(0, 2), Some(7.0)); // (1 + 4 + 16) / 3
/// bars[2].close = 5.0;
/// study.update(&bars, 2);
/// let mut fresh = MeanSquare::default();
/// fresh.calculate(&bars);
/// assert_eq!(format!("{:?}", study.values()), format!("{:?}", fresh.values()));
/// bars.push(Bar::new(chrono::DateTime::from_timestamp(3, 0).unwrap(), 6.0, 6.0, 6.0, 6.0, 1.0));
/// study.update(&bars, 3);
/// fresh.calculate(&bars);
/// assert_eq!(format!("{:?}", study.values()), format!("{:?}", fresh.values()));
/// ```
#[derive(Clone, Default)]
pub struct Stepper<S> {
    state: S,
    before_last: S,
    len: usize,
}

impl<S: Clone + Default> Stepper<S> {
    /// The state after the previous run's last bar.
    pub fn state(&self) -> &S {
        &self.state
    }

    /// Recomputes `out[from..]`, given that `data[..from]` is unchanged since the previous run.
    /// Resumes when `from >= len - 1` (last bar changed or bars appended), restarts from bar 0
    /// otherwise (correction deep in history, prepended page, removed bars).
    pub fn run(
        &mut self,
        data: &[Bar],
        from: usize,
        out: &mut IndicatorValues,
        mut step: impl FnMut(&mut S, &Bar) -> IndicatorValue,
    ) {
        let start = if self.len > 0 && from >= self.len - 1 && data.len() >= self.len {
            if from == self.len - 1 {
                self.state.clone_from(&self.before_last);
            }
            from.min(self.len)
        } else {
            self.state = S::default();
            0
        };
        out.truncate(start);
        for (i, bar) in data.iter().enumerate().skip(start) {
            if i + 1 == data.len() {
                self.before_last.clone_from(&self.state);
            }
            out.push(step(&mut self.state, bar));
        }
        self.len = data.len();
    }
}

/// Recomputes `values[from..]` of a window study, given that `data[..from]` is unchanged, from
/// `compute(&data[read..])`, where `read = read_from(data, from)` is the first bar the values
/// from `from` on depend on. Equals a full `compute` bit for bit when `compute` evaluates every
/// window afresh and no value from `from` on depends on a bar before `read`.
#[cfg(feature = "studies")]
pub(crate) fn update_window(
    values: &mut IndicatorValues,
    data: &[Bar],
    from: usize,
    read_from: impl FnOnce(&[Bar], usize) -> usize,
    compute: impl FnOnce(&[Bar]) -> IndicatorValues,
) {
    let from = from.min(values.len()).min(data.len());
    values.truncate(from);
    if from < data.len() {
        let read = read_from(data, from).min(from);
        values.append_from(&compute(&data[read..]), from - read);
    }
}

/// `read_from` for a study whose value at a bar depends on that bar and the `count` usable bars
/// before it.
#[cfg(feature = "studies")]
pub(crate) fn usable_behind(
    count: usize,
    usable: impl Fn(&Bar) -> bool,
) -> impl FnOnce(&[Bar], usize) -> usize {
    move |data, from| {
        let (mut read, mut left) = (from, count);
        while left > 0 && read > 0 {
            read -= 1;
            left -= usize::from(usable(&data[read]));
        }
        read
    }
}

#[cfg(test)]
mod tests {
    use crate::studies::helpers;
    use crate::studies::helpers::{highest as rolling_max, lowest as rolling_min};

    /// The rolling extremes equal a scan of every window, folded with `f64::max` and `f64::min`
    /// as the window studies did, including when an extreme leaves the window.
    #[test]
    fn rolling_extremes_match_a_window_scan() {
        let (nan, inf) = (f64::NAN, f64::INFINITY);
        let cases: &[(&[f64], usize)] = &[
            (&[], 3),
            (&[5.0, 1.0, 2.0, 3.0, 0.0, 4.0], 2),
            (&[1.0, 5.0, 2.0, 5.0, 3.0, 1.0, 1.0, 0.0], 3),
            (&[2.0, 7.0, 1.0, 8.0], 1),
            (&[3.0, 1.0, 2.0], 10),
            (&[4.0, nan, 1.0, nan, nan, nan, 2.0], 3),
            (&[nan, nan, nan], 2),
            (&[1.0, inf, 2.0, -inf, 3.0, 0.5, 0.25], 2),
        ];
        for &(values, period) in cases {
            let scan = |fold: fn(f64, f64) -> f64, init: f64| -> Vec<f64> {
                (0..values.len())
                    .map(|i| {
                        values[(i + 1).saturating_sub(period)..=i]
                            .iter()
                            .copied()
                            .fold(init, fold)
                    })
                    .collect()
            };
            let case = format!("{values:?} period {period}");
            assert_eq!(
                rolling_max(values.iter().copied(), period),
                scan(f64::max, -inf),
                "max {case}"
            );
            assert_eq!(
                rolling_min(values.iter().copied(), period),
                scan(f64::min, inf),
                "min {case}"
            );
        }
        // Every complete-window helper is compared with its definition, not another helper.
        for &(values, period) in cases {
            let actual = [
                helpers::sma(values, period),
                helpers::sum(values, period),
                helpers::wma(values, period),
                helpers::stdev(values, period),
            ];
            for i in 0..values.len() {
                let window = &values[(i + 1).saturating_sub(period)..=i];
                let expected = if window.len() == period && window.iter().all(|v| v.is_finite()) {
                    let sum = window.iter().sum::<f64>();
                    let mean = sum / period as f64;
                    [
                        mean,
                        sum,
                        window
                            .iter()
                            .enumerate()
                            .map(|(j, v)| (j + 1) as f64 * v)
                            .sum::<f64>()
                            / (period as f64 * (period as f64 + 1.0) / 2.0),
                        (window.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / period as f64)
                            .sqrt(),
                    ]
                } else {
                    [nan; 4]
                };
                for (got, want) in actual.iter().map(|line| line[i]).zip(expected) {
                    assert!(
                        got == want
                            || (got.is_nan() && want.is_nan())
                            || (got - want).abs() < 1e-10,
                        "{values:?}, period {period}, bar {i}: {got} != {want}"
                    );
                }
            }
            for (line, alpha, seed_len) in [
                (helpers::ema(values, period), 2.0 / (period as f64 + 1.0), 1),
                (
                    helpers::ema_sma(values, period),
                    2.0 / (period as f64 + 1.0),
                    period,
                ),
                (helpers::rma(values, period), 1.0 / period as f64, period),
            ] {
                for (i, &got) in line.iter().enumerate() {
                    let inputs: Vec<_> = values[..=i]
                        .iter()
                        .copied()
                        .filter(|v| v.is_finite())
                        .collect();
                    // Expand the recurrence as a geometrically weighted sum from the seed.
                    let want = if !values[i].is_finite() || inputs.len() < seed_len {
                        nan
                    } else {
                        let seed = inputs[..seed_len].iter().sum::<f64>() / seed_len as f64;
                        let tail = inputs.len() - seed_len;
                        seed * (1.0 - alpha).powf(tail as f64)
                            + inputs[seed_len..]
                                .iter()
                                .enumerate()
                                .map(|(j, v)| alpha * v * (1.0 - alpha).powf((tail - 1 - j) as f64))
                                .sum::<f64>()
                    };
                    assert!(
                        got == want
                            || (got.is_nan() && want.is_nan())
                            || (got - want).abs() < 1e-10,
                        "average {values:?}, period {period}, bar {i}: {got} != {want}"
                    );
                }
            }
            for (i, got) in helpers::change(values, period).into_iter().enumerate() {
                let want = i.checked_sub(period).map_or(nan, |j| values[i] - values[j]);
                assert!(got == want || (got.is_nan() && !want.is_finite()));
            }
        }
        // A touch, a return to the same side and a gap produce no crossing.
        let a = [-1.0, 0.0, -1.0, 1.0, 0.0, 1.0, -1.0, nan, 1.0];
        let b = [0.0; 9];
        assert_eq!(
            helpers::crossover(&a, &b),
            [false, false, false, true, false, false, false, false, false]
        );
        assert_eq!(
            helpers::crossunder(&a, &b),
            [false, false, false, false, false, false, true, false, false]
        );
        assert_eq!(helpers::sma(&[2.0, 4.0], 0), [2.0, 4.0]);
    }
}
