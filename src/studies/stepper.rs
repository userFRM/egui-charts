//! Shared driver for studies computed one bar at a time.
//! Also the rolling highest and lowest values that window studies share.

use crate::model::Bar;
use crate::studies::IndicatorValue;
use std::collections::VecDeque;

/// Runs a causal study one `step` per bar and keeps the state before the last bar, so a
/// last-bar update or an append resumes instead of recomputing the whole series.
/// `calculate` is `run(data, 0, ..)`, so batch and streaming share one code path.
#[derive(Clone, Default)]
pub(crate) struct Stepper<S> {
    state: S,
    before_last: S,
    len: usize,
}

impl<S: Clone + Default> Stepper<S> {
    /// Recomputes `out[from..]`, given that `data[..from]` is unchanged since the previous run.
    /// Resumes when `from >= len - 1` (last bar changed or bars appended), restarts from bar 0
    /// otherwise (correction deep in history, prepended page, removed bars).
    pub(crate) fn run(
        &mut self,
        data: &[Bar],
        from: usize,
        out: &mut Vec<IndicatorValue>,
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

/// Highest value of each window `values[i + 1 - period..=i]`, shorter for the first bars, in
/// linear time. Like folding the window with `f64::max` from negative infinity, NaN never wins
/// and a window holding only NaN gives negative infinity. `period` must be at least 1.
pub(crate) fn rolling_max(values: impl IntoIterator<Item = f64>, period: usize) -> Vec<f64> {
    rolling(values, period, f64::NEG_INFINITY, |kept, new| kept <= new)
}

/// Lowest value of each window, as [`rolling_max`] with `f64::min` and positive infinity.
pub(crate) fn rolling_min(values: impl IntoIterator<Item = f64>, period: usize) -> Vec<f64> {
    rolling(values, period, f64::INFINITY, |kept, new| kept >= new)
}

/// Keeps the window's candidates in a deque, best first: a new value removes every candidate
/// it beats, since none of them can be the extreme while it stays in the window.
fn rolling(
    values: impl IntoIterator<Item = f64>,
    period: usize,
    empty: f64,
    beaten: impl Fn(f64, f64) -> bool,
) -> Vec<f64> {
    let mut window = VecDeque::<(usize, f64)>::new();
    values
        .into_iter()
        .enumerate()
        .map(|(i, v)| {
            if window.front().is_some_and(|&(j, _)| i - j >= period) {
                window.pop_front();
            }
            if !v.is_nan() {
                while window.back().is_some_and(|&(_, kept)| beaten(kept, v)) {
                    window.pop_back();
                }
                window.push_back((i, v));
            }
            window.front().map_or(empty, |&(_, best)| best)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{rolling_max, rolling_min};

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
    }
}
