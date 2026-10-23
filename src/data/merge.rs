//! The merge rules: how history pages, live bars and ticks enter a series.

use crate::model::{Bar, BarClock};
use chrono::{DateTime, Utc};
use std::collections::BTreeSet;

/// Where an incoming bar comes from, which decides what happens to a stored bar of the same slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Origin {
    /// A history page: the stored bar wins unless it was built from ticks.
    History,
    /// A live bar: replaces the stored bar.
    Live,
    /// This many trades folded into a bar: extends the last bar or appends a newer one; older
    /// ones are late.
    Ticks(u64),
}

/// The bars of one series: valid, in time order, at most one per slot of the series' clock.
#[derive(Default)]
pub(crate) struct SeriesBars {
    bars: Vec<Bar>,
    /// Times of bars built from ticks, which a history bar of the same slot replaces.
    from_ticks: BTreeSet<DateTime<Utc>>,
}

/// What one merge changed.
pub(crate) struct Merged {
    /// Index of the first bar inserted or changed. Bars after it may have moved.
    pub(crate) first_changed: Option<usize>,
    /// Invalid bars dropped, and the trades of a bar built from ticks that would overflow its stored
    /// bar.
    pub(crate) rejected: u64,
    /// Trades of bars older than the last bar, dropped.
    pub(crate) late_ticks: u64,
}

impl SeriesBars {
    pub(crate) fn bars(&self) -> &[Bar] {
        &self.bars
    }

    /// Merges `incoming`, in any order, by the rules of each bar's origin. Bars are identified by
    /// their slot in `clock`, which must be the same for every merge into this series.
    pub(crate) fn merge(&mut self, clock: &BarClock, mut incoming: Vec<(Bar, Origin)>) -> Merged {
        let received = incoming.len();
        incoming.retain(|(bar, _)| valid(bar));
        let mut merged = Merged {
            first_changed: None,
            rejected: (received - incoming.len()) as u64,
            late_ticks: 0,
        };
        if merged.rejected > 0 {
            log::warn!("dropped {} invalid bars", merged.rejected);
        }
        // A stable sort, so the last of several bars in one slot wins.
        incoming.sort_by_key(|(bar, _)| bar.time);
        incoming.dedup_by(|later, earlier| {
            let same = clock.same_bar(later.0.time, earlier.0.time);
            if same {
                if later.0 != earlier.0 {
                    log::debug!(
                        "dropped the bar at {} for a later bar of its slot",
                        earlier.0.time
                    );
                }
                std::mem::swap(later, earlier);
            }
            same
        });
        let (Some((first, _)), Some((newest, _))) = (incoming.first(), incoming.last()) else {
            return merged;
        };
        let slot = |bar: &Bar| clock.bar_open(bar.time);
        let (first, newest) = (slot(first), slot(newest));
        let last = self.bars.last().map(slot);
        let lo = self.bars.partition_point(|bar| slot(bar) < first);
        let hi = self.bars.partition_point(|bar| slot(bar) <= newest);
        let mut stored = self.bars[lo..hi].iter().peekable();
        let mut window = Vec::with_capacity(hi - lo + incoming.len());
        for (bar, origin) in incoming {
            let at = slot(&bar);
            if let Origin::Ticks(trades) = origin
                && last.is_some_and(|last| at < last)
            {
                merged.late_ticks += trades;
                continue;
            }
            while let Some(old) = stored.next_if(|old| slot(old) < at) {
                window.push(old.clone());
            }
            let old = stored.next_if(|old| slot(old) == at);
            let new = match (old, origin) {
                (Some(old), Origin::Ticks(trades)) => fold(old, &bar).unwrap_or_else(|| {
                    merged.rejected += trades;
                    old.clone()
                }),
                (Some(old), Origin::History) if !self.from_ticks.contains(&old.time) => old.clone(),
                (Some(old), _) => {
                    self.from_ticks.remove(&old.time);
                    bar
                }
                (None, Origin::Ticks(_)) => {
                    self.from_ticks.insert(bar.time);
                    bar
                }
                (None, _) => bar,
            };
            if old != Some(&new) {
                merged.first_changed.get_or_insert(lo + window.len());
            }
            window.push(new);
        }
        window.extend(stored.cloned());
        self.bars.splice(lo..hi, window);
        merged
    }
}

/// Finite values, a high at or above open and close, a low at or below them, no negative volume.
pub(super) fn valid(bar: &Bar) -> bool {
    [bar.open, bar.high, bar.low, bar.close, bar.volume]
        .into_iter()
        .all(f64::is_finite)
        && bar.high >= bar.open.max(bar.close)
        && bar.low <= bar.open.min(bar.close)
        && bar.volume >= 0.0
}

/// `bar` extended with `ticks`, a bar of later trades in the same slot, or `None` if the result is
/// not valid: two valid volumes can add up to infinity.
pub(super) fn fold(bar: &Bar, ticks: &Bar) -> Option<Bar> {
    let folded = Bar {
        high: bar.high.max(ticks.high),
        low: bar.low.min(ticks.low),
        close: ticks.close,
        volume: bar.volume + ticks.volume,
        ..*bar
    };
    valid(&folded).then_some(folded)
}

#[cfg(test)]
mod tests {
    use super::Origin::{History, Live, Ticks};
    use super::*;
    use crate::model::Timeframe;
    use chrono::TimeDelta;

    fn at(minute: i64, second: i64) -> DateTime<Utc> {
        "2024-03-15T10:00:00Z".parse::<DateTime<Utc>>().unwrap()
            + TimeDelta::minutes(minute)
            + TimeDelta::seconds(second)
    }

    /// A flat bar of the one-minute slot `minute` minutes after 10:00, stamped `second` into it.
    fn bar_at(minute: i64, second: i64, close: f64) -> Bar {
        Bar::new(at(minute, second), close, close, close, close, 1.0)
    }

    fn bar(minute: i64, close: f64) -> Bar {
        bar_at(minute, 0, close)
    }

    fn ohlcv(minute: i64, [open, high, low, close, volume]: [f64; 5]) -> Bar {
        Bar::new(at(minute, 0), open, high, low, close, volume)
    }

    /// Guards the merge rules and `first_changed`: out-of-order bars must be kept rather than
    /// dropped, a page must not overwrite newer live bars, and an under-reported `first_changed`
    /// leaves studies stale.
    #[test]
    fn merge_rules() {
        let mut invalid = vec![bar(1, 5.0), bar(2, f64::NAN), bar(3, 5.0)];
        invalid[0].high = 4.0;
        invalid[2].volume = -1.0;
        #[rustfmt::skip]
        let cases = [
            ("a newer live bar is appended",
                vec![(History, vec![bar(0, 1.0), bar(1, 2.0)]), (Live, vec![bar(2, 3.0)])],
                vec![bar(0, 1.0), bar(1, 2.0), bar(2, 3.0)], (Some(2), 0, 0)),
            ("a live bar stamped inside the last slot replaces the last bar",
                vec![(History, vec![bar(0, 1.0), bar(1, 2.0)]), (Live, vec![bar_at(1, 30, 5.0)])],
                vec![bar(0, 1.0), bar_at(1, 30, 5.0)], (Some(1), 0, 0)),
            ("an identical live bar changes nothing",
                vec![(History, vec![bar(0, 1.0), bar(1, 2.0)]), (Live, vec![bar(1, 2.0)])],
                vec![bar(0, 1.0), bar(1, 2.0)], (None, 0, 0)),
            ("an older live bar corrects its slot",
                vec![(History, vec![bar(0, 1.0), bar(1, 2.0), bar(2, 3.0)]),
                    (Live, vec![bar(1, 9.0)])],
                vec![bar(0, 1.0), bar(1, 9.0), bar(2, 3.0)], (Some(1), 0, 0)),
            ("an out-of-order live bar is inserted",
                vec![(History, vec![bar(0, 1.0), bar(2, 3.0)]), (Live, vec![bar(1, 2.0)])],
                vec![bar(0, 1.0), bar(1, 2.0), bar(2, 3.0)], (Some(1), 0, 0)),
            ("a page is sorted, keeps the last bar of a slot and loses to stored bars",
                vec![(Live, vec![bar(2, 7.0), bar(3, 8.0)]),
                    (History, vec![bar(2, 99.0), bar(1, 2.0), bar(0, 1.0), bar(1, 5.0)])],
                vec![bar(0, 1.0), bar(1, 5.0), bar(2, 7.0), bar(3, 8.0)], (Some(0), 0, 0)),
            ("a page bar replaces a bar built from ticks in its slot",
                vec![(History, vec![bar(0, 1.0)]), (Ticks(1), vec![bar(1, 4.0)]),
                    (History, vec![bar_at(1, 20, 6.0)])],
                vec![bar(0, 1.0), bar_at(1, 20, 6.0)], (Some(1), 0, 0)),
            ("a live bar that replaced a bar built from ticks wins over a page",
                vec![(Ticks(1), vec![bar(1, 4.0)]), (Live, vec![bar(1, 5.0)]),
                    (History, vec![bar(1, 6.0)])],
                vec![bar(1, 5.0)], (None, 0, 0)),
            ("ticks extend the last bar",
                vec![(History, vec![bar(0, 1.0), ohlcv(1, [5.0, 7.0, 4.0, 6.0, 10.0])]),
                    (Ticks(3), vec![ohlcv(1, [8.0, 8.0, 3.0, 3.0, 3.0])])],
                vec![bar(0, 1.0), ohlcv(1, [5.0, 8.0, 3.0, 3.0, 13.0])], (Some(1), 0, 0)),
            ("every trade of a bar that would overflow the volume is rejected",
                vec![(History, vec![ohlcv(1, [5.0, 5.0, 5.0, 5.0, 1.5e308])]),
                    (Ticks(3), vec![ohlcv(1, [5.0, 5.0, 5.0, 5.0, 1e308])])],
                vec![ohlcv(1, [5.0, 5.0, 5.0, 5.0, 1.5e308])], (None, 3, 0)),
            ("every trade of a bar older than the last bar is late",
                vec![(History, vec![bar(0, 1.0), bar(1, 2.0)]), (Ticks(3), vec![bar(0, 9.0)])],
                vec![bar(0, 1.0), bar(1, 2.0)], (None, 0, 3)),
            ("invalid bars are dropped",
                vec![(History, [vec![bar(0, 1.0)], invalid].concat())],
                vec![bar(0, 1.0)], (Some(0), 3, 0)),
        ];
        let clock = BarClock::new(Timeframe::Min1);
        for (name, steps, bars, expected) in cases {
            let mut series = SeriesBars::default();
            let mut merged = None;
            for (origin, incoming) in steps {
                merged = Some(series.merge(
                    &clock,
                    incoming.into_iter().map(|bar| (bar, origin)).collect(),
                ));
            }
            let merged = merged.unwrap();
            assert_eq!(series.bars(), bars, "{name}");
            assert_eq!(
                (merged.first_changed, merged.rejected, merged.late_ticks),
                expected,
                "{name}"
            );
        }
    }
}
