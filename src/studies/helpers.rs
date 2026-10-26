//! Calculations on scalar series for built-in and host-defined studies.
//!
//! Outputs retain the input length. Missing results are NaN; lengths of zero are
//! treated as one, except [`change`], where zero means no change. Complete-window
//! calculations require finite inputs throughout the window. Recursive averages
//! skip non-finite inputs, emitting NaN there and retaining their previous state.

use std::collections::VecDeque;

/// Arithmetic mean of each complete trailing window.
pub fn sma(values: &[f64], period: usize) -> Vec<f64> {
    windows(values, period.max(1), |window| {
        window.iter().sum::<f64>() / window.len() as f64
    })
}

/// Sum of each complete trailing window, recovering after missing inputs leave it.
pub fn sum(values: &[f64], period: usize) -> Vec<f64> {
    let period = period.max(1);
    let (mut sum, mut count) = (0.0, 0usize);
    values
        .iter()
        .enumerate()
        .map(|(i, &v)| {
            if v.is_finite() {
                sum += v;
                count += 1;
            }
            if i >= period && values[i - period].is_finite() {
                sum -= values[i - period];
                count -= 1;
            }
            if !sum.is_finite() {
                sum = values[(i + 1).saturating_sub(period)..=i]
                    .iter()
                    .filter(|v| v.is_finite())
                    .sum();
            }
            if count == period { sum } else { f64::NAN }
        })
        .collect()
}

/// Exponential average with alpha `2 / (period + 1)`, seeded by the first finite value.
pub fn ema(values: &[f64], period: usize) -> Vec<f64> {
    average(values, 1, 2.0 / (period.max(1) as f64 + 1.0))
}

/// Exponential average with alpha `2 / (period + 1)`, seeded by the mean of the
/// first `period` finite values. Useful for nested averages with a full warm-up.
pub fn ema_sma(values: &[f64], period: usize) -> Vec<f64> {
    average(values, period.max(1), 2.0 / (period.max(1) as f64 + 1.0))
}

/// Wilder's average with alpha `1 / period`, seeded by the mean of the first
/// `period` finite values.
pub fn rma(values: &[f64], period: usize) -> Vec<f64> {
    average(values, period.max(1), 1.0 / period.max(1) as f64)
}

fn average(values: &[f64], seed: usize, alpha: f64) -> Vec<f64> {
    let (mut count, mut average) = (0, 0.0);
    values
        .iter()
        .map(|&x| {
            if !x.is_finite() {
                return f64::NAN;
            }
            if count < seed {
                count += 1;
                average += x;
                if count < seed {
                    return f64::NAN;
                }
                average /= seed as f64;
            } else {
                average += (x - average) * alpha;
            }
            average
        })
        .collect()
}

/// Linear weighted mean of each complete window, weights `1..=period`, newest heaviest.
pub fn wma(values: &[f64], period: usize) -> Vec<f64> {
    let period = period.max(1);
    let denominator = period as f64 * (period as f64 + 1.0) / 2.0;
    windows(values, period, |window| {
        window
            .iter()
            .rev()
            .enumerate()
            .map(|(j, v)| v * (period - j) as f64)
            .sum::<f64>()
            / denominator
    })
}

/// Population standard deviation of each complete window, with divisor `period`.
pub fn stdev(values: &[f64], period: usize) -> Vec<f64> {
    windows(values, period.max(1), |window| {
        let mean = window.iter().sum::<f64>() / window.len() as f64;
        (window.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / window.len() as f64).sqrt()
    })
}

fn windows(values: &[f64], period: usize, compute: impl Fn(&[f64]) -> f64) -> Vec<f64> {
    (0..values.len())
        .map(|i| {
            if i + 1 < period {
                return f64::NAN;
            }
            let window = &values[i + 1 - period..=i];
            if window.iter().all(|v| v.is_finite()) {
                compute(window)
            } else {
                f64::NAN
            }
        })
        .collect()
}

/// Highest value in the trailing window, including partial warm-up windows.
/// NaN never wins; an all-NaN window yields negative infinity, as `f64::max` does.
pub fn highest(values: impl IntoIterator<Item = f64>, period: usize) -> Vec<f64> {
    rolling(values, period.max(1), f64::NEG_INFINITY, |kept, new| {
        kept <= new
    })
}

/// Lowest value in the trailing window, including partial warm-up windows.
/// NaN never wins; an all-NaN window yields positive infinity, as `f64::min` does.
pub fn lowest(values: impl IntoIterator<Item = f64>, period: usize) -> Vec<f64> {
    rolling(values, period.max(1), f64::INFINITY, |kept, new| {
        kept >= new
    })
}

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

/// Difference from `period` bars ago. Unavailable or non-finite endpoints yield NaN.
pub fn change(values: &[f64], period: usize) -> Vec<f64> {
    values
        .iter()
        .enumerate()
        .map(|(i, &value)| {
            i.checked_sub(period)
                .filter(|&j| value.is_finite() && values[j].is_finite())
                .map_or(f64::NAN, |j| value - values[j])
        })
        .collect()
}

/// True where `a` moves from below `b` to above it. Equal values retain the last
/// unequal comparison, so a touch and return does not cross. Missing values break
/// comparisons. Output has `a.len()` entries; missing `b` entries are gaps.
pub fn crossover(a: &[f64], b: &[f64]) -> Vec<bool> {
    crossings(a, b, std::cmp::Ordering::Greater)
}

/// True where `a` moves from above `b` to below it; otherwise as [`crossover`].
pub fn crossunder(a: &[f64], b: &[f64]) -> Vec<bool> {
    crossings(a, b, std::cmp::Ordering::Less)
}

fn crossings(a: &[f64], b: &[f64], direction: std::cmp::Ordering) -> Vec<bool> {
    let mut previous = None;
    a.iter()
        .enumerate()
        .map(|(i, &a)| {
            let Some(&b) = b.get(i).filter(|b| a.is_finite() && b.is_finite()) else {
                previous = None;
                return false;
            };
            let order = a.partial_cmp(&b).unwrap();
            if order == std::cmp::Ordering::Equal {
                return false;
            }
            let crosses = order == direction && previous.is_some_and(|p| p != order);
            previous = Some(order);
            crosses
        })
        .collect()
}
