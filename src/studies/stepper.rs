//! Shared driver for studies computed one bar at a time.

use crate::model::Bar;
use crate::studies::IndicatorValue;

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

#[cfg(test)]
mod tests {
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
}
