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

use std::cell::RefCell;

use crate::model::Bar;

/// Transform a full OHLC series into its Heikin-Ashi representation.
///
/// The input must be the complete dataset (not a visible sub-window) so that
/// the recurrence is seeded deterministically from the first bar.
pub fn transform_to_heikin_ashi(bars: &[Bar]) -> Vec<Bar> {
    let mut ha_bars = Vec::with_capacity(bars.len());
    extend(&mut ha_bars, bars);
    ha_bars
}

fn extend(ha_bars: &mut Vec<Bar>, bars: &[Bar]) {
    for bar in bars {
        let ha_close = (bar.open + bar.high + bar.low + bar.close) / 4.0;
        let (ha_open, ha_high, ha_low) = match ha_bars.last() {
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
        });
    }
}

struct CachedSeries {
    revision: u64,
    first: Bar,
    last: Bar,
    bars: Vec<Bar>,
}

fn same_bar(a: &Bar, b: &Bar) -> bool {
    let bits = |bar: &Bar| [bar.open, bar.high, bar.low, bar.close, bar.volume].map(f64::to_bits);
    a.time == b.time && bits(a) == bits(b)
}

thread_local! {
    static HEIKIN_ASHI_CACHE: RefCell<Vec<CachedSeries>> = const { RefCell::new(Vec::new()) };
}

/// Calls `use_window` with the Heikin-Ashi values of the visible window
/// [start_idx, start_idx + len) of full_data, a series at revision; indices outside the dataset are clamped.
pub(super) fn with_window<R>(
    full_data: &[Bar],
    revision: u64,
    start_idx: usize,
    len: usize,
    use_window: impl FnOnce(&[Bar]) -> R,
) -> R {
    let (Some(first), Some(last)) = (full_data.first(), full_data.last()) else {
        return use_window(&[]);
    };
    HEIKIN_ASHI_CACHE.with_borrow_mut(|cache| {
        let pos = cache.iter().position(|entry| {
            entry.revision == revision
                && same_bar(&entry.first, first)
                && entry.bars.len() <= full_data.len()
        });
        let entry = match pos {
            Some(pos) => {
                // A live tail update retains the preceding bars.
                let mut entry = cache.remove(pos);
                let old_len = entry.bars.len();
                let from = if same_bar(&entry.last, &full_data[old_len - 1]) {
                    old_len
                } else {
                    old_len - 1
                };
                entry.bars.truncate(from);
                extend(&mut entry.bars, &full_data[from..]);
                entry.last = last.clone();
                entry
            }
            None => CachedSeries {
                revision,
                first: first.clone(),
                last: last.clone(),
                bars: transform_to_heikin_ashi(full_data),
            },
        };
        let start = start_idx.min(entry.bars.len());
        let end = start.saturating_add(len).min(entry.bars.len());
        let result = use_window(&entry.bars[start..end]);
        // Eight recently drawn datasets cover a multi-chart layout without unbounded history.
        if cache.len() >= 8 {
            cache.remove(0);
        }
        cache.push(entry);
        result
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    fn window(full: &[Bar], revision: u64, start: usize, len: usize) -> Vec<Bar> {
        with_window(full, revision, start, len, <[Bar]>::to_vec)
    }

    fn bar(i: i64, open: f64, high: f64, low: f64, close: f64) -> Bar {
        Bar {
            time: Utc.timestamp_opt(1_700_000_000 + i * 60, 0).unwrap(),
            open,
            high,
            low,
            close,
            volume: 100.0,
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

    /// A bar's Heikin-Ashi value must not depend on where the visible window
    /// begins: the windowed result must equal the tail of the full transform.
    #[test]
    fn ha_window_is_independent_of_start() {
        let full = sample_series();
        let ha_full = transform_to_heikin_ashi(&full);

        for start in 0..full.len() {
            let len = full.len() - start;
            let win = window(&full, 0, start, len);
            assert_eq!(win.len(), len, "start={start}");
            for (i, w) in win.iter().enumerate() {
                assert_bars_eq(w, &ha_full[start + i], &format!("start={start}, i={i}"));
            }
        }
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
        let check = |state: &ChartState| {
            let full = &state.data().bars;
            let fresh = transform_to_heikin_ashi(full);
            with_window(full, state.revision(), 0, full.len(), |cached| {
                assert_eq!(cached.len(), fresh.len());
                for (a, b) in cached.iter().zip(&fresh) {
                    assert_bars_eq(a, b, "stream");
                    assert_eq!(a.time, b.time);
                    assert_eq!(a.volume.to_bits(), b.volume.to_bits());
                }
                cached.as_ptr()
            })
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

    /// Out-of-range indices clamp to the available data instead of panicking.
    #[test]
    fn window_clamps_out_of_range() {
        let full = sample_series();
        assert!(window(&full, 0, 100, 10).is_empty());
        let tail = window(&full, 0, full.len() - 1, 50);
        assert_eq!(tail.len(), 1);
    }

    /// An empty dataset yields an empty window without panicking.
    #[test]
    fn empty_series_is_empty() {
        assert!(window(&[], 0, 0, 10).is_empty());
    }
}
