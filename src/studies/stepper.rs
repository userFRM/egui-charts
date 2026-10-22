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
    use crate::model::Bar;
    use crate::studies::{EMA, Indicator, IndicatorValue, SMA};
    use chrono::{Duration, TimeZone, Utc};

    /// xorshift64*, so the event sequence is the same on every platform and run.
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 >> 12;
            self.0 ^= self.0 << 25;
            self.0 ^= self.0 >> 27;
            self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
        }

        /// Uniform in [0, 1).
        fn unit(&mut self) -> f64 {
            (self.next() >> 11) as f64 / (1u64 << 53) as f64
        }
    }

    fn bar(rng: &mut Rng, i: usize, prev_close: f64) -> Bar {
        let close = prev_close + rng.unit() - 0.5;
        Bar {
            time: Utc.timestamp_opt(1_700_000_000, 0).unwrap() + Duration::minutes(i as i64),
            open: prev_close,
            high: prev_close.max(close) + rng.unit() * 0.2,
            low: prev_close.min(close) - rng.unit() * 0.2,
            close,
            volume: 100.0 + rng.unit() * 900.0,
        }
    }

    fn same(a: &IndicatorValue, b: &IndicatorValue) -> bool {
        let close = |x: f64, y: f64| (x.is_nan() && y.is_nan()) || (x - y).abs() <= 1e-9;
        match (a, b) {
            (IndicatorValue::Single(x), IndicatorValue::Single(y)) => close(*x, *y),
            (IndicatorValue::Multiple(x), IndicatorValue::Multiple(y)) => {
                x.len() == y.len() && x.iter().zip(y).all(|(x, y)| close(*x, *y))
            }
            (IndicatorValue::None, IndicatorValue::None) => true,
            _ => false,
        }
    }

    /// Streaming equals batch: after every last-bar update, append, correction or removal,
    /// `update(data, from)` leaves the same values as a fresh `calculate(data)`.
    /// Each incremental study adds a row here.
    #[test]
    fn update_matches_fresh_calculate() {
        let studies: &[fn() -> Box<dyn Indicator>] =
            &[|| Box::new(SMA::new(14)), || Box::new(EMA::new(10))];
        for make in studies {
            let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
            let mut data = Vec::new();
            let mut close = 100.0;
            for i in 0..300 {
                let b = bar(&mut rng, i, close);
                close = b.close;
                data.push(b);
            }
            let mut study = make();
            let name = study.line_names().remove(0);
            assert!(study.is_incremental(), "{name} must be incremental");
            study.calculate(&data);

            for event in 0..200 {
                let roll = rng.unit();
                let last = data.len() - 1;
                let from = if roll < 0.70 {
                    data[last] = bar(&mut rng, last, data[last].open);
                    last
                } else if roll < 0.93 {
                    data.push(bar(&mut rng, last + 1, data[last].close));
                    last + 1
                } else if roll < 0.98 {
                    let i = last - 1 - (rng.next() % 5) as usize;
                    data[i] = bar(&mut rng, i, data[i].open);
                    i
                } else {
                    data.pop();
                    last
                };
                study.update(&data, from);

                let mut fresh = make();
                fresh.calculate(&data);
                assert_eq!(
                    study.values().len(),
                    fresh.values().len(),
                    "{name} event {event}"
                );
                for (i, (a, b)) in study.values().iter().zip(fresh.values()).enumerate() {
                    assert!(same(a, b), "{name} event {event} bar {i}: {a:?} != {b:?}");
                }
            }
        }
    }

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
