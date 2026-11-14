use crate::model::Bar;
use std::rc::Rc;

/// Identity of a span of bars of a chart's series, used to decide when a
/// result cached from it (such as a Heikin-Ashi transform or a drawing's
/// volume profile) must be recomputed.
///
/// It captures the index of the span's first bar, its length, the timestamps
/// of its first and last bars, the first close, every value of the last bar,
/// and the series' [`ChartState::revision`](crate::studies::ChartState::revision),
/// which changes when any bar before the series' last one changes. So it
/// changes when bars are appended or removed, the endpoints move, the forming
/// last bar updates, or a bar inside the span is corrected.
#[cfg(feature = "widget")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SpanFingerprint {
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
    pub(crate) fn of(first: usize, bars: &[Bar], revision: u64) -> Self {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A cached result follows the bars it was computed from: more volume on
    /// the forming bar at an unchanged price, another anchor, or a corrected
    /// bar strictly inside the span recomputes it. Updating the last bar or
    /// appending one keeps the revision, so a streaming span can be extended.
    #[cfg(feature = "widget")]
    #[test]
    fn span_cache_recomputes_when_bars_change() {
        use crate::model::BarData;
        use crate::studies::ChartState;
        let mut cache = SpanCache::new(4);
        let total_volume = |bars: &[Bar]| bars.iter().map(|bar| bar.volume).sum::<f64>();
        let mut get = |state: &ChartState, first: usize| {
            let bars = &state.data().bars[first..];
            *cache.get((), first, bars, state.revision(), total_volume)
        };
        let mut bars: Vec<_> = [100., 200., 150., 300., 250., 400., 50.]
            .into_iter()
            .enumerate()
            .map(|(i, volume)| {
                Bar::new(
                    chrono::DateTime::from_timestamp(i as i64 * 60, 0).unwrap(),
                    10.,
                    12.,
                    9.,
                    11.,
                    volume,
                )
            })
            .collect();
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
}
