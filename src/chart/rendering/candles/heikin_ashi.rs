//! Heikin-Ashi transform with full-series caching.
//!
//! Heikin-Ashi is a cumulative recurrence:
//!
//! ```text
//! ha_close[n] = (open[n] + high[n] + low[n] + close[n]) / 4
//! ha_open[n]  = (ha_open[n-1] + ha_close[n-1]) / 2
//! ```
//!
//! Because `ha_open[n]` depends on the *previous* Heikin-Ashi bar, the series
//! must be seeded from the first bar of the **full** dataset. Seeding from the
//! first bar of the visible window makes the same underlying bar render
//! differently as the user pans or zooms, since the seed (and thus the decaying
//! recurrence tail) changes with the window. Computing the transform once over
//! the complete dataset and slicing the visible window out of it keeps every
//! bar's Heikin-Ashi value invariant under panning and zooming.

use std::cell::{Ref, RefCell};

use crate::model::Bar;

/// A whitespace bar stays whitespace, and the bar after it continues from the last bar with a
/// value.
fn extend(ha_bars: &mut Vec<Bar>, bars: &[Bar]) {
    for bar in bars {
        if bar.is_whitespace() {
            ha_bars.push(Bar::whitespace(bar.time));
            continue;
        }
        let ha_close = (bar.open + bar.high + bar.low + bar.close) / 4.0;
        let (ha_open, ha_high, ha_low) = match ha_bars.iter().rev().find(|ha| !ha.is_whitespace()) {
            Some(prev) => {
                let open = (prev.open + prev.close) / 2.0;
                (
                    open,
                    bar.high.max(open).max(ha_close),
                    bar.low.min(open).min(ha_close),
                )
            }
            None => ((bar.open + bar.close) / 2.0, bar.high, bar.low),
        };
        ha_bars.push(Bar {
            time: bar.time,
            open: ha_open,
            high: ha_high,
            low: ha_low,
            close: ha_close,
            volume: bar.volume,
            colors: None,
            payload: None,
        });
    }
}

#[derive(Default)]
struct CachedSeries {
    revision: Option<u64>,
    last: Option<Bar>,
    bars: Vec<Bar>,
}

fn same_bar(a: &Bar, b: &Bar) -> bool {
    let bits = |bar: &Bar| [bar.open, bar.high, bar.low, bar.close, bar.volume].map(f64::to_bits);
    a.time == b.time && bits(a) == bits(b)
}

/// The Heikin-Ashi history of one series, retained until its owner is dropped.
///
/// Keep one cache per series and pass it to [`super::CandleDataContext`]. Unchanged frames
/// reuse the history; appends and forming-bar updates recompute only the tail.
#[derive(Default)]
pub struct HeikinAshiCache {
    entry: RefCell<CachedSeries>,
}

impl HeikinAshiCache {
    /// Returns the full transformed history at `revision`, following
    /// [`ChartState::revision`](crate::studies::ChartState::revision)'s contract.
    pub(crate) fn bars(&self, full_data: &[Bar], revision: u64) -> Ref<'_, [Bar]> {
        {
            let mut entry = self.entry.borrow_mut();
            if entry.revision != Some(revision) || entry.bars.len() > full_data.len() {
                entry.bars.clear();
            }
            let old_len = entry.bars.len();
            let from = old_len.checked_sub(1).map_or(0, |last| {
                if entry
                    .last
                    .as_ref()
                    .is_some_and(|old| same_bar(old, &full_data[last]))
                {
                    old_len
                } else {
                    last
                }
            });
            entry.bars.truncate(from);
            extend(&mut entry.bars, &full_data[from..]);
            entry.revision = Some(revision);
            entry.last = full_data.last().cloned();
        }
        Ref::map(self.entry.borrow(), |entry| entry.bars.as_slice())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    fn bar(i: i64, open: f64, high: f64, low: f64, close: f64) -> Bar {
        Bar {
            time: Utc.timestamp_opt(1_700_000_000 + i * 60, 0).unwrap(),
            open,
            high,
            low,
            close,
            volume: 100.0,
            colors: None,
            payload: None,
        }
    }

    fn sample_series() -> Vec<Bar> {
        vec![
            bar(0, 10.0, 12.0, 9.0, 11.0),
            bar(1, 11.0, 13.0, 10.5, 12.5),
            bar(2, 12.5, 14.0, 12.0, 13.0),
            bar(3, 13.0, 13.5, 11.0, 11.5),
            bar(4, 11.5, 12.0, 10.0, 10.5),
            bar(5, 10.5, 11.0, 9.5, 10.0),
        ]
    }

    #[test]
    fn streaming_updates_retain_the_prefix_and_match_hand_computed_bars() {
        use crate::model::BarData;
        use crate::studies::ChartState;
        let mut state = ChartState::new(BarData::new());
        let cache = HeikinAshiCache::default();
        let check = |state: &ChartState, expected: &[(f64, f64, f64, f64)]| {
            let full = &state.data().bars;
            let cached = cache.bars(full, state.revision());
            let actual: Vec<_> = cached
                .iter()
                .map(|bar| (bar.open, bar.high, bar.low, bar.close))
                .collect();
            assert_eq!(actual, expected);
            for (transformed, source) in cached.iter().zip(full) {
                assert_eq!(transformed.time, source.time);
                assert_eq!(transformed.volume, source.volume);
            }
            cached.as_ptr()
        };
        // Each close is OHLC / 4; each later open averages the previous transformed open/close.
        let expected = [
            (10.5, 12.0, 9.0, 10.5),
            (10.5, 13.0, 10.5, 11.75),
            (11.125, 14.0, 11.125, 12.875),
            (12.0, 13.5, 11.0, 12.25),
            (12.125, 12.125, 10.0, 11.0),
            (11.5625, 11.5625, 9.5, 10.25),
        ];
        for (i, next) in sample_series().into_iter().enumerate() {
            let close = next.close;
            state.edit_bars(|bars| {
                let from = bars.len();
                bars.push(next);
                Some(from)
            });
            let prefix = check(&state, &expected[..=i]);
            // Changing only the source close by 1/4 or -1/2 changes HA close by 1/16 or -1/8.
            for (tick, ha_tick, seed_tick) in [
                (0.25, 0.0625, 0.125),
                (-0.5, -0.125, -0.25),
                (0.0, 0.0, 0.0),
            ] {
                state.edit_bars(|bars| {
                    bars[i].close = close + tick;
                    bars[i].volume += 1.0;
                    Some(i)
                });
                let mut forming = expected[..=i].to_vec();
                forming[i].3 += ha_tick;
                if i == 0 {
                    forming[0].0 += seed_tick;
                }
                let updated = check(&state, &forming);
                if i > 0 {
                    assert_eq!(
                        prefix, updated,
                        "a forming-bar tick must retain the cached prefix"
                    );
                }
            }
        }
        // Several bars can arrive between frames, including a final update of the old tail.
        state.edit_bars(|bars| {
            let from = bars.len() - 1;
            bars[from].close += 0.125;
            bars.extend([
                bar(6, 10.0, 12.0, 9.0, 11.0),
                bar(7, 11.0, 13.0, 10.0, 12.0),
            ]);
            Some(from)
        });
        let mut extended = expected.to_vec();
        extended[5].3 = 10.28125;
        extended.extend([(10.921875, 12.0, 9.0, 10.5), (10.7109375, 13.0, 10.0, 11.5)]);
        check(&state, &extended);
        state.edit_bars(|bars| {
            bars[2].close += 0.5;
            Some(2)
        });
        let corrected = [
            expected[0],
            expected[1],
            (11.125, 14.0, 11.125, 13.0),
            (12.0625, 13.5, 11.0, 12.25),
            (12.15625, 12.15625, 10.0, 11.0),
            (11.578125, 11.578125, 9.5, 10.28125),
            (10.9296875, 12.0, 9.0, 10.5),
            (10.71484375, 13.0, 10.0, 11.5),
        ];
        check(&state, &corrected);
        state.edit_bars(|bars| {
            bars.insert(0, bar(-1, 9.0, 11.0, 8.0, 10.0));
            Some(0)
        });
        let prepended = [
            (9.5, 11.0, 8.0, 9.5),
            (9.5, 12.0, 9.0, 10.5),
            (10.0, 13.0, 10.0, 11.75),
            (10.875, 14.0, 10.875, 13.0),
            (11.9375, 13.5, 11.0, 12.25),
            (12.09375, 12.09375, 10.0, 11.0),
            (11.546875, 11.546875, 9.5, 10.28125),
            (10.9140625, 12.0, 9.0, 10.5),
            (10.70703125, 13.0, 10.0, 11.5),
        ];
        check(&state, &prepended);
        state.edit_bars(|bars| {
            bars.truncate(3);
            Some(3)
        });
        check(&state, &prepended[..3]);
        state.set_data(BarData::default());
        check(&state, &[]);
    }

    #[test]
    fn active_series_do_not_evict_each_other() {
        let owners: Vec<_> = (0..9).map(|_| HeikinAshiCache::default()).collect();
        let histories: Vec<_> = (0..9)
            .map(|i| {
                let mut bars = sample_series();
                bars[0].open += i as f64;
                bars[0].high += i as f64;
                bars
            })
            .collect();
        let pointers: Vec<_> = owners
            .iter()
            .zip(&histories)
            .enumerate()
            .map(|(i, (cache, bars))| cache.bars(bars, i as u64).as_ptr())
            .collect();
        for _ in 0..3 {
            for (i, (cache, bars)) in owners.iter().zip(&histories).enumerate() {
                let transformed = cache.bars(bars, i as u64);
                assert_eq!(transformed.as_ptr(), pointers[i]);
                assert_eq!(transformed[0].open, 10.5 + i as f64 / 2.0);
                assert_eq!(transformed[1].open, 10.5 + i as f64 / 2.0);
            }
        }
    }
}
