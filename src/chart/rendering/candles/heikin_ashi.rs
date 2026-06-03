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
    let mut ha_bars: Vec<Bar> = Vec::with_capacity(bars.len());

    let Some(first) = bars.first() else {
        return ha_bars;
    };

    let mut prev_ha_open = (first.open + first.close) / 2.0;
    let mut prev_ha_close = (first.open + first.high + first.low + first.close) / 4.0;

    ha_bars.push(Bar {
        time: first.time,
        open: prev_ha_open,
        high: first.high,
        low: first.low,
        close: prev_ha_close,
        volume: first.volume,
    });

    for bar in bars.iter().skip(1) {
        let ha_close = (bar.open + bar.high + bar.low + bar.close) / 4.0;
        let ha_open = (prev_ha_open + prev_ha_close) / 2.0;
        let ha_high = bar.high.max(ha_open).max(ha_close);
        let ha_low = bar.low.min(ha_open).min(ha_close);

        ha_bars.push(Bar {
            time: bar.time,
            open: ha_open,
            high: ha_high,
            low: ha_low,
            close: ha_close,
            volume: bar.volume,
        });

        prev_ha_open = ha_open;
        prev_ha_close = ha_close;
    }

    ha_bars
}

/// Identity fingerprint of a source dataset, used to decide when a cached
/// Heikin-Ashi series must be recomputed.
///
/// The full recurrence is `O(n)`; recomputing it every frame would be wasteful.
/// The fingerprint captures the length plus the timestamps and closing prices of
/// the first and last bars, which changes whenever the dataset is replaced,
/// appended to, or its endpoints are mutated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SeriesFingerprint {
    len: usize,
    first_time: i64,
    last_time: i64,
    first_close: u64,
    last_close: u64,
}

impl SeriesFingerprint {
    fn of(bars: &[Bar]) -> Self {
        match (bars.first(), bars.last()) {
            (Some(first), Some(last)) => Self {
                len: bars.len(),
                first_time: first.time.timestamp_millis(),
                last_time: last.time.timestamp_millis(),
                first_close: first.close.to_bits(),
                last_close: last.close.to_bits(),
            },
            // An empty dataset has a well-defined empty fingerprint.
            _ => Self {
                len: 0,
                first_time: 0,
                last_time: 0,
                first_close: 0,
                last_close: 0,
            },
        }
    }
}

/// Bounded set of recently computed Heikin-Ashi series.
///
/// Keyed by [`SeriesFingerprint`] so that several charts sharing the render
/// thread each keep their own cached series instead of thrashing a single slot.
struct HeikinAshiCache {
    entries: Vec<(SeriesFingerprint, Vec<Bar>)>,
}

/// Maximum number of distinct datasets cached per render thread. Charts are
/// rendered one at a time in immediate mode, so a small bound covers realistic
/// multi-chart layouts while keeping memory bounded.
const MAX_CACHED_SERIES: usize = 8;

impl HeikinAshiCache {
    const fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Return the cached Heikin-Ashi series for `full_data`, computing and
    /// storing it only if this dataset has not been seen (or has changed).
    fn series(&mut self, full_data: &[Bar]) -> Vec<Bar> {
        let fingerprint = SeriesFingerprint::of(full_data);

        if let Some(pos) = self.entries.iter().position(|(fp, _)| *fp == fingerprint) {
            // Move the hit to the back so the eviction policy is least-recently-used.
            let entry = self.entries.remove(pos);
            let series = entry.1.clone();
            self.entries.push(entry);
            return series;
        }

        let series = transform_to_heikin_ashi(full_data);

        if self.entries.len() >= MAX_CACHED_SERIES {
            self.entries.remove(0);
        }
        self.entries.push((fingerprint, series.clone()));
        series
    }
}

thread_local! {
    static HEIKIN_ASHI_CACHE: RefCell<HeikinAshiCache> = const { RefCell::new(HeikinAshiCache::new()) };
}

/// Heikin-Ashi values for the visible window `[start_idx, start_idx + len)`.
///
/// The full-series transform is computed once per dataset and cached on the
/// render thread; this returns the requested sub-window sliced out of it, so the
/// values are invariant under panning and zooming. Indices outside the dataset
/// are clamped, never panicking.
pub fn window(full_data: &[Bar], start_idx: usize, len: usize) -> Vec<Bar> {
    HEIKIN_ASHI_CACHE.with(|cache| {
        let series = cache.borrow_mut().series(full_data);
        let start = start_idx.min(series.len());
        let end = start.saturating_add(len).min(series.len());
        series[start..end].to_vec()
    })
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
            let win = window(&full, start, len);
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

    /// Re-requesting the same dataset reuses the cached series and yields stable
    /// values across calls.
    #[test]
    fn cache_returns_stable_values() {
        let full = sample_series();
        let first = window(&full, 1, 3);
        let second = window(&full, 1, 3);
        assert_eq!(first.len(), second.len());
        for (a, b) in first.iter().zip(second.iter()) {
            assert_bars_eq(a, b, "stable");
        }
    }

    /// Appending to the dataset invalidates the cached series for the new shape.
    #[test]
    fn cache_invalidates_on_data_change() {
        let series_a = sample_series();
        let win_a = window(&series_a, 0, series_a.len());
        assert_eq!(win_a.len(), series_a.len());

        let mut series_b = sample_series();
        series_b.push(bar(6, 10.0, 10.5, 9.0, 9.5));
        let win_b = window(&series_b, 0, series_b.len());
        assert_eq!(win_b.len(), series_b.len());
        assert_ne!(win_a.len(), win_b.len());
    }

    /// Out-of-range indices clamp to the available data instead of panicking.
    #[test]
    fn window_clamps_out_of_range() {
        let full = sample_series();
        assert!(window(&full, 100, 10).is_empty());
        let tail = window(&full, full.len() - 1, 50);
        assert_eq!(tail.len(), 1);
    }

    /// An empty dataset yields an empty window without panicking.
    #[test]
    fn empty_series_is_empty() {
        assert!(window(&[], 0, 10).is_empty());
    }
}
