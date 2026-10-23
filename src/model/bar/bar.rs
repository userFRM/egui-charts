//! Core OHLCV Bar Structure
//!
//! The fundamental data unit for financial charting.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
#[cfg(feature = "widget")]
use std::rc::Rc;

/// Percent change from `reference` to `value`, divided by the magnitude of the
/// reference so it has the sign of the change for a negative reference too (spreads,
/// some futures). A zero or non-finite reference or value gives 0.
pub(crate) fn percent_change(reference: f64, value: f64) -> f64 {
    let pct = (value - reference) / reference.abs() * 100.0;
    if pct.is_finite() { pct } else { 0.0 }
}

/// Represents a single OHLCV (Open, High, Low, Close, Volume) bar
///
/// Each bar represents a discrete time period of trading activity.
///
/// # Example
///
/// ```
/// use chrono::Utc;
/// use egui_charts::model::Bar;
///
/// let bar = Bar::new(
///     Utc::now(),
///     100.0, // open
///     105.0, // high
///     98.0,  // low
///     103.0, // close
///     1000.0 // volume
/// );
///
/// assert!(bar.is_bullish()); // close > open
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bar {
    /// Timestamp of the bar
    pub time: DateTime<Utc>,
    /// Opening price
    pub open: f64,
    /// Highest price during the period
    pub high: f64,
    /// Lowest price during the period
    pub low: f64,
    /// Closing price
    pub close: f64,
    /// Trading volume
    pub volume: f64,
}

impl Bar {
    /// Creates a new bar
    pub fn new(
        time: DateTime<Utc>,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
    ) -> Self {
        Self {
            time,
            open,
            high,
            low,
            close,
            volume,
        }
    }

    // ============= Direction Methods =============

    /// Returns true if this is a bullish bar (close > open)
    #[inline]
    pub fn is_bullish(&self) -> bool {
        self.close > self.open
    }

    /// Returns true if this is a bearish bar (close < open)
    #[inline]
    pub fn is_bearish(&self) -> bool {
        self.close < self.open
    }

    /// Returns true if this is a doji (open ≈ close)
    ///
    /// # Arguments
    /// * `threshold` - Ratio of body to range below which it's considered a doji
    pub fn is_doji(&self, threshold: f64) -> bool {
        let range = self.range();
        if range == 0.0 {
            return true;
        }
        (self.close - self.open).abs() / range < threshold
    }

    // ============= Price Measurements =============

    /// Returns the body height (absolute difference between open and close)
    #[inline]
    pub fn body_height(&self) -> f64 {
        (self.close - self.open).abs()
    }

    /// Returns the total range (high - low)
    #[inline]
    pub fn range(&self) -> f64 {
        self.high - self.low
    }

    /// Returns the upper wick height
    #[inline]
    pub fn upper_wick(&self) -> f64 {
        self.high - self.open.max(self.close)
    }

    /// Returns the lower wick height
    #[inline]
    pub fn lower_wick(&self) -> f64 {
        self.open.min(self.close) - self.low
    }

    // ============= Derived Prices =============

    /// Returns the typical price ((high + low + close) / 3)
    ///
    /// Commonly used in technical analysis (e.g., VWAP, CCI)
    #[inline]
    pub fn typical_price(&self) -> f64 {
        (self.high + self.low + self.close) / 3.0
    }

    /// Returns the weighted close ((high + low + close + close) / 4)
    #[inline]
    pub fn weighted_close(&self) -> f64 {
        (self.high + self.low + self.close * 2.0) / 4.0
    }

    /// Returns the midpoint price ((high + low) / 2)
    #[inline]
    pub fn midpoint(&self) -> f64 {
        (self.high + self.low) / 2.0
    }

    /// Returns the average price (OHLC / 4)
    #[inline]
    pub fn avg_price(&self) -> f64 {
        (self.open + self.high + self.low + self.close) / 4.0
    }

    // ============= Ratios and Percentages =============

    /// Returns the body as a percentage of the total range (0.0 to 1.0)
    pub fn body_percentage(&self) -> f64 {
        let range = self.range();
        if range == 0.0 {
            return 0.0;
        }
        self.body_height() / range
    }

    /// Returns the ratio of upper wick to lower wick
    ///
    /// Returns infinity if lower wick is zero.
    pub fn wick_ratio(&self) -> f64 {
        let lower = self.lower_wick();
        if lower == 0.0 {
            return f64::INFINITY;
        }
        self.upper_wick() / lower
    }

    /// Returns the price change (close - open)
    #[inline]
    pub fn change(&self) -> f64 {
        self.close - self.open
    }

    /// Returns the percentage change ((close - open) / |open| * 100), divided by the
    /// magnitude of the open so it has the sign of the change for a negative open too
    pub fn change_percent(&self) -> f64 {
        percent_change(self.open, self.close)
    }

    // ============= Body Position Helpers =============

    /// Returns the top of the body (max of open and close)
    #[inline]
    pub fn body_top(&self) -> f64 {
        self.open.max(self.close)
    }

    /// Returns the bottom of the body (min of open and close)
    #[inline]
    pub fn body_bottom(&self) -> f64 {
        self.open.min(self.close)
    }
}

/// Identity of a span of bars of a chart's series, used to decide when a
/// result cached from it (such as a Heikin-Ashi transform or a drawing's
/// volume profile) must be recomputed.
///
/// It captures the index of the span's first bar, its length, the timestamps
/// of its first and last bars, the first close, every value of the last bar,
/// and the series' [`ChartState::revision`](crate::model::ChartState::revision),
/// which changes when any bar before the series' last one changes. So it
/// changes when bars are appended or removed, the endpoints move, the forming
/// last bar updates, or a bar inside the span is corrected.
#[cfg(feature = "widget")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SpanFingerprint {
    first: usize,
    len: usize,
    first_time: i64,
    last_time: i64,
    first_close: u64,
    last_bar: [u64; 5],
    revision: u64,
}

#[cfg(feature = "widget")]
impl SpanFingerprint {
    fn of(first: usize, bars: &[Bar], revision: u64) -> Self {
        let (first_bar, last_bar) = match (bars.first(), bars.last()) {
            (Some(first), Some(last)) => (
                (first.time.timestamp_millis(), first.close.to_bits()),
                (
                    last.time.timestamp_millis(),
                    [last.open, last.high, last.low, last.close, last.volume].map(f64::to_bits),
                ),
            ),
            // An empty span has a well-defined empty fingerprint.
            _ => ((0, 0), (0, [0; 5])),
        };
        Self {
            first,
            len: bars.len(),
            first_time: first_bar.0,
            last_time: last_bar.0,
            first_close: first_bar.1,
            last_bar: last_bar.1,
            revision,
        }
    }
}

/// Results computed from spans of bars, kept per render thread so that a
/// steady frame does no work proportional to a span. An entry is keyed by a
/// parameter of the computation and the span's [`SpanFingerprint`]; the least
/// recently used entry is evicted first.
#[cfg(feature = "widget")]
pub(crate) struct SpanCache<T, P = ()> {
    entries: Vec<((P, SpanFingerprint), Rc<T>)>,
    capacity: usize,
}

#[cfg(feature = "widget")]
impl<T, P: Copy + PartialEq> SpanCache<T, P> {
    /// A cache that keeps up to `capacity` results.
    pub(crate) const fn new(capacity: usize) -> Self {
        Self {
            entries: Vec::new(),
            capacity,
        }
    }

    fn find(&mut self, key: (P, SpanFingerprint)) -> Option<Rc<T>> {
        let pos = self.entries.iter().position(|(cached, _)| *cached == key)?;
        let entry = self.entries.remove(pos);
        let value = Rc::clone(&entry.1);
        self.entries.push(entry);
        Some(value)
    }

    fn insert(&mut self, key: (P, SpanFingerprint), value: T) -> Rc<T> {
        if self.entries.len() >= self.capacity {
            self.entries.remove(0);
        }
        let value = Rc::new(value);
        self.entries.push((key, Rc::clone(&value)));
        value
    }

    /// The result for `bars`, the span that starts at bar `first` of a
    /// series at `revision`, computed with `compute` unless it is cached.
    pub(crate) fn get(
        &mut self,
        param: P,
        first: usize,
        bars: &[Bar],
        revision: u64,
        compute: impl FnOnce(&[Bar]) -> T,
    ) -> Rc<T> {
        let key = (param, SpanFingerprint::of(first, bars, revision));
        match self.find(key) {
            Some(value) => value,
            None => self.insert(key, compute(bars)),
        }
    }

    /// Like [`get`](Self::get), for a span that ends on the series' last bar,
    /// which may be forming. The result for the bars before it is kept too,
    /// and `extend` adds one bar to such a result, so a tick on the last bar,
    /// or a new bar, costs one bar instead of the span while `extend` can add
    /// it; `build` computes a span from scratch.
    pub(crate) fn get_extended(
        &mut self,
        param: P,
        first: usize,
        bars: &[Bar],
        revision: u64,
        build: impl Fn(&[Bar]) -> T,
        extend: impl Fn(&T, &Bar) -> Option<T>,
    ) -> Rc<T> {
        let Some((last, opening)) = bars.split_last() else {
            return self.get(param, first, bars, revision, build);
        };
        let key = |bars: &[Bar]| (param, SpanFingerprint::of(first, bars, revision));
        // Looked up on every frame so that it stays cached while the span is drawn.
        let base = self.find(key(opening));
        if let Some(value) = self.find(key(bars)) {
            return value;
        }
        let base = match base {
            Some(base) => base,
            None => {
                // After a new bar, the result before the bar that just closed.
                let value = opening
                    .split_last()
                    .and_then(|(closed, older)| extend(&*self.find(key(older))?, closed))
                    .unwrap_or_else(|| build(opening));
                self.insert(key(opening), value)
            }
        };
        let value = extend(&base, last).unwrap_or_else(|| build(bars));
        self.insert(key(bars), value)
    }
}

impl fmt::Display for Bar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Bar[{}, O:{:.2}, H:{:.2}, L:{:.2}, C:{:.2}, V:{:.2}]",
            self.time.format("%Y-%m-%d %H:%M:%S"),
            self.open,
            self.high,
            self.low,
            self.close,
            self.volume
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_bullish_bar() -> Bar {
        Bar::new(Utc::now(), 100.0, 110.0, 95.0, 105.0, 1000.0)
    }

    fn sample_bearish_bar() -> Bar {
        Bar::new(Utc::now(), 105.0, 110.0, 95.0, 100.0, 1000.0)
    }

    #[test]
    fn test_bullish_bearish() {
        let bullish = sample_bullish_bar();
        let bearish = sample_bearish_bar();

        assert!(bullish.is_bullish());
        assert!(!bullish.is_bearish());
        assert!(bearish.is_bearish());
        assert!(!bearish.is_bullish());
    }

    #[test]
    fn test_measurements() {
        let bar = sample_bullish_bar();

        assert_eq!(bar.body_height(), 5.0); // |105 - 100|
        assert_eq!(bar.range(), 15.0); // 110 - 95
        assert_eq!(bar.upper_wick(), 5.0); // 110 - 105
        assert_eq!(bar.lower_wick(), 5.0); // 100 - 95
    }

    #[test]
    fn test_derived_prices() {
        let bar = Bar::new(Utc::now(), 100.0, 120.0, 80.0, 110.0, 1000.0);

        // typical = (120 + 80 + 110) / 3 = 103.33...
        assert!((bar.typical_price() - 103.333).abs() < 0.01);

        // midpoint = (120 + 80) / 2 = 100
        assert_eq!(bar.midpoint(), 100.0);

        // avg = (100 + 120 + 80 + 110) / 4 = 102.5
        assert_eq!(bar.avg_price(), 102.5);
    }

    /// The percentage has the sign of the price change for any reference sign;
    /// a zero or non-finite reference or value gives 0 instead of inf or NaN.
    #[test]
    fn percent_change_follows_the_sign_of_the_change() {
        let rows = [
            // (reference, value, expected)
            (100.0, 105.0, 5.0),
            (10.0, 5.0, -50.0),
            (-10.0, -5.0, 50.0),
            (-5.0, -10.0, -100.0),
            (0.0, 5.0, 0.0),
            (f64::NAN, 5.0, 0.0),
            (f64::INFINITY, 5.0, 0.0),
            (5.0, f64::NAN, 0.0),
        ];
        for (reference, value, expected) in rows {
            assert_eq!(
                percent_change(reference, value),
                expected,
                "{reference} -> {value}"
            );
        }
        let bar = Bar::new(Utc::now(), -10.0, -4.0, -11.0, -5.0, 1000.0);
        assert_eq!(bar.change_percent(), 50.0);
    }

    /// A cached result follows the bars it was computed from: more volume on
    /// the forming bar at an unchanged price, another anchor, or a corrected
    /// bar strictly inside the span recomputes it. Updating the last bar or
    /// appending one keeps the revision, so a streaming span can be extended.
    #[cfg(feature = "widget")]
    #[test]
    fn span_cache_recomputes_when_bars_change() {
        use crate::model::{BarData, ChartState};
        let mut cache = SpanCache::new(4);
        let total_volume = |bars: &[Bar]| bars.iter().map(|bar| bar.volume).sum::<f64>();
        let mut get = |state: &ChartState, first: usize| {
            let bars = &state.data().bars[first..];
            *cache.get((), first, bars, state.revision(), total_volume)
        };
        let mut bars = crate::drawings::rendering::sample_bars();
        let mut state = ChartState::new(BarData::from_bars(bars.clone()));
        let streaming = state.revision();
        assert_eq!(get(&state, 1), 1350.0);
        bars[6].volume += 10.0;
        state.set_data(BarData::from_bars(bars.clone()));
        assert_eq!(get(&state, 1), 1360.0);
        assert_eq!(get(&state, 2), 1160.0);
        bars.push(bars[6].clone());
        bars[7].time += chrono::Duration::minutes(1);
        state.set_data(BarData::from_bars(bars.clone()));
        assert_eq!(state.revision(), streaming);
        assert_eq!(get(&state, 2), 1220.0);
        // A corrected bar strictly inside the span.
        bars[3].volume += 100.0;
        state.set_data(BarData::from_bars(bars.clone()));
        assert_eq!(get(&state, 2), 1320.0);
        // A copy streams on its own: the bar both were forming closes differently.
        let mut copy = state.clone();
        bars.push(bars[7].clone());
        bars[8].time += chrono::Duration::minutes(1);
        state.set_data(BarData::from_bars(bars.clone()));
        let mut copied = bars.clone();
        copied[7].volume += 1000.0;
        copy.set_data(BarData::from_bars(copied));
        assert_eq!(get(&state, 2), 1380.0);
        assert_eq!(get(&copy, 2), 2380.0);
        // A NaN in a kept bar is unchanged data, not a correction.
        bars[0].volume = f64::NAN;
        state.set_data(BarData::from_bars(bars.clone()));
        let streaming = state.revision();
        bars[8].close += 1.0;
        state.set_data(BarData::from_bars(bars.clone()));
        assert_eq!(state.revision(), streaming);
        // Removed bars.
        state.set_data(BarData::from_bars(bars[..3].to_vec()));
        assert_ne!(state.revision(), streaming);
    }

    #[test]
    fn test_body_positions() {
        let bullish = sample_bullish_bar();
        assert_eq!(bullish.body_top(), 105.0);
        assert_eq!(bullish.body_bottom(), 100.0);

        let bearish = sample_bearish_bar();
        assert_eq!(bearish.body_top(), 105.0);
        assert_eq!(bearish.body_bottom(), 100.0);
    }
}
