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
    /// [`ChartState::revision`](crate::model::ChartState::revision)'s contract.
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

    /// Transform a full OHLC series into its Heikin-Ashi representation.
    ///
    /// The input must be the complete dataset (not a visible sub-window) so that
    /// the recurrence is seeded deterministically from the first bar.
    fn transform_to_heikin_ashi(bars: &[Bar]) -> Vec<Bar> {
        let mut ha_bars = Vec::with_capacity(bars.len());
        extend(&mut ha_bars, bars);
        ha_bars
    }

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

    fn assert_bars_eq(a: &Bar, b: &Bar, ctx: &str) {
        assert_eq!(a.open.to_bits(), b.open.to_bits(), "{ctx}: open");
        assert_eq!(a.high.to_bits(), b.high.to_bits(), "{ctx}: high");
        assert_eq!(a.low.to_bits(), b.low.to_bits(), "{ctx}: low");
        assert_eq!(a.close.to_bits(), b.close.to_bits(), "{ctx}: close");
    }

    /// Seeding from a later bar (the old visible-window behavior) produces a
    /// different early value, confirming the defect the cache fixes.
    #[test]
    fn windowed_seed_diverges_from_full_seed() {
        let full = sample_series();
        let ha_full = transform_to_heikin_ashi(&full);
        let ha_windowed = transform_to_heikin_ashi(&full[2..]);

        assert_ne!(
            ha_windowed[0].open.to_bits(),
            ha_full[2].open.to_bits(),
            "windowed seed should differ from full-series value"
        );
    }

    #[test]
    fn streaming_updates_retain_the_prefix_and_match_a_fresh_transform() {
        use crate::model::{BarData, ChartState};
        let mut state = ChartState::new(BarData::new());
        let cache = HeikinAshiCache::default();
        let check = |state: &ChartState| {
            let full = &state.data().bars;
            let fresh = transform_to_heikin_ashi(full);
            let cached = cache.bars(full, state.revision());
            assert_eq!(cached.len(), fresh.len());
            for (a, b) in cached.iter().zip(&fresh) {
                assert_bars_eq(a, b, "stream");
                assert_eq!(a.time, b.time);
                assert_eq!(a.volume.to_bits(), b.volume.to_bits());
            }
            cached.as_ptr()
        };
        for next in sample_series() {
            state.edit_bars(|bars| {
                let from = bars.len();
                bars.push(next);
                Some(from)
            });
            let prefix = check(&state);
            for tick in [0.25, -0.5, 0.0] {
                state.edit_bars(|bars| {
                    let from = bars.len() - 1;
                    bars[from].close += tick;
                    bars[from].volume += 1.0;
                    Some(from)
                });
                let updated = check(&state);
                if state.data().len() > 1 {
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
        check(&state);
        state.edit_bars(|bars| {
            bars[2].close += 0.5;
            Some(2)
        });
        check(&state);
        state.edit_bars(|bars| {
            bars.insert(0, bar(-1, 9.0, 11.0, 8.0, 10.0));
            Some(0)
        });
        check(&state);
        state.edit_bars(|bars| {
            bars.truncate(3);
            Some(3)
        });
        check(&state);
        state.set_data(BarData::default());
        check(&state);
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
