//! Streaming helpers: one value per bar, equal bit for bit to the function of the same name in
//! `egui_charts::studies::helpers` over the whole series up to that bar, with `ema` matching
//! `helpers::ema_sma`.

use std::cmp::Ordering;
use std::collections::VecDeque;

/// A helper a script calls, with the state it keeps between bars.
#[derive(Clone, Debug)]
pub(crate) enum Kernel {
    /// A complete-window calculation over the last `period` inputs, missing until the window
    /// is full and while it holds a missing input.
    Window {
        kind: WindowKind,
        period: usize,
        window: VecDeque<f64>,
        /// The missing inputs in `window`, so a missing input costs no scan of it.
        missing: usize,
    },
    /// A recursive average seeded by the mean of the first `seed` finite inputs.
    Average {
        seed: usize,
        alpha: f64,
        count: usize,
        average: f64,
    },
    /// The highest or lowest input of the last `period`, ignoring missing ones.
    Rolling {
        highest: bool,
        period: usize,
        window: VecDeque<(usize, f64)>,
        index: usize,
    },
    /// The difference from the input `period` bars ago.
    Change {
        period: usize,
        window: VecDeque<f64>,
    },
    /// Whether the first input crosses the second in `direction`.
    Cross {
        direction: Ordering,
        previous: Option<Ordering>,
    },
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum WindowKind {
    Sma,
    Sum,
    Wma,
    Stdev,
}

impl Kernel {
    /// The kernel of helper `name` with length `length`, which is at least 0; lengths of 0
    /// count as 1 except for `change`, as in `helpers`. Its window grows with the bars it is
    /// fed, so a length beyond the bars costs only the bars.
    pub fn new(name: &str, length: usize) -> Self {
        let period = length.max(1);
        let window = |kind| Self::Window {
            kind,
            period,
            window: VecDeque::new(),
            missing: 0,
        };
        let average = |seed, alpha| Self::Average {
            seed,
            alpha,
            count: 0,
            average: 0.0,
        };
        let rolling = |highest| Self::Rolling {
            highest,
            period,
            window: VecDeque::new(),
            index: 0,
        };
        match name {
            "sma" => window(WindowKind::Sma),
            "sum" => window(WindowKind::Sum),
            "wma" => window(WindowKind::Wma),
            "stdev" => window(WindowKind::Stdev),
            "ema" => average(period, 2.0 / (period as f64 + 1.0)),
            "rma" => average(period, 1.0 / period as f64),
            "highest" => rolling(true),
            "lowest" => rolling(false),
            "change" => Self::Change {
                period: length,
                window: VecDeque::new(),
            },
            "crossover" => Self::Cross {
                direction: Ordering::Greater,
                previous: None,
            },
            "crossunder" => Self::Cross {
                direction: Ordering::Less,
                previous: None,
            },
            _ => unreachable!("no helper {name}"),
        }
    }

    /// The bytes the kernel holds beyond its own size once fed `bars` bars: its window keeps
    /// at most one value a bar.
    pub fn bytes(&self, bars: usize) -> usize {
        match *self {
            Self::Window { period, .. } => period.min(bars) * 8,
            Self::Change { period, .. } => period.saturating_add(1).min(bars) * 8,
            Self::Rolling { period, .. } => period.min(bars) * 16,
            Self::Average { .. } | Self::Cross { .. } => 0,
        }
    }

    /// Feed the next bar's input, `b` being the second input of the crossings, and return the
    /// output on that bar; crossings return 1 or 0.
    pub fn step(&mut self, x: f64, b: f64) -> f64 {
        match self {
            Self::Window {
                kind,
                period,
                window,
                missing,
            } => {
                if window.len() == *period && window.pop_front().is_some_and(|v| !v.is_finite()) {
                    *missing -= 1;
                }
                window.push_back(x);
                if !x.is_finite() {
                    *missing += 1;
                }
                if window.len() < *period || *missing > 0 {
                    return f64::NAN;
                }
                let len = window.len() as f64;
                match kind {
                    WindowKind::Sma => window.iter().sum::<f64>() / len,
                    WindowKind::Sum => window.iter().sum(),
                    WindowKind::Wma => {
                        let denominator = *period as f64 * (*period as f64 + 1.0) / 2.0;
                        window
                            .iter()
                            .rev()
                            .enumerate()
                            .map(|(j, v)| v * (*period - j) as f64)
                            .sum::<f64>()
                            / denominator
                    }
                    WindowKind::Stdev => {
                        let mean = window.iter().sum::<f64>() / len;
                        (window.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / len).sqrt()
                    }
                }
            }
            Self::Average {
                seed,
                alpha,
                count,
                average,
            } => {
                if !x.is_finite() {
                    return f64::NAN;
                }
                if *count < *seed {
                    *count += 1;
                    *average += x;
                    if *count < *seed {
                        return f64::NAN;
                    }
                    *average /= *seed as f64;
                } else {
                    *average += (x - *average) * *alpha;
                }
                *average
            }
            Self::Rolling {
                highest,
                period,
                window,
                index,
            } => {
                let i = *index;
                *index += 1;
                if window.front().is_some_and(|&(j, _)| i - j >= *period) {
                    window.pop_front();
                }
                if !x.is_nan() {
                    let beaten = |kept: f64| if *highest { kept <= x } else { kept >= x };
                    while window.back().is_some_and(|&(_, kept)| beaten(kept)) {
                        window.pop_back();
                    }
                    window.push_back((i, x));
                }
                let empty = if *highest {
                    f64::NEG_INFINITY
                } else {
                    f64::INFINITY
                };
                window.front().map_or(empty, |&(_, best)| best)
            }
            Self::Change { period, window } => {
                // Keeps the last `period + 1` inputs; `period + 1` itself overflows for the
                // longest lengths on 32-bit targets.
                if window.len() > *period {
                    window.pop_front();
                }
                window.push_back(x);
                match window.front() {
                    Some(&old) if window.len() > *period && x.is_finite() && old.is_finite() => {
                        x - old
                    }
                    _ => f64::NAN,
                }
            }
            Self::Cross {
                direction,
                previous,
            } => {
                if !x.is_finite() || !b.is_finite() {
                    *previous = None;
                    return 0.0;
                }
                let order = x.partial_cmp(&b).unwrap();
                if order == Ordering::Equal {
                    return 0.0;
                }
                let crosses = order == *direction && previous.is_some_and(|p| p != order);
                *previous = Some(order);
                f64::from(u8::from(crosses))
            }
        }
    }
}
