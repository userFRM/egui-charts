//! The merge rules: how history pages, live bars and ticks enter a series.

use crate::model::{Bar, BarClock};
use chrono::{DateTime, Utc};
use std::collections::BTreeMap;

/// Where an incoming bar comes from, which decides what happens to a stored bar of the same slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Origin {
    /// A history page: extends its newest bar with stored ticks, replaces completed tick bars.
    History,
    /// A live bar: replaces the stored bar.
    Live,
    /// This many trades folded into a bar: extends the last bar or appends a newer one; older
    /// ones are late. Zero trades denotes an empty heartbeat candle.
    Ticks(u64),
    /// Trades whose first execution replaces a heartbeat, or extends a page already in its slot.
    TicksAfterHeartbeat(u64),
}

/// What one merge changed.
#[derive(Default)]
pub(crate) struct Merged {
    /// Index of the first bar inserted or changed. Bars after it may have moved.
    pub(crate) first_changed: Option<usize>,
    /// Invalid bars dropped, the trades of a bar built from ticks that would overflow its stored
    /// bar, and history bars that would overflow with the trades stored in their slot.
    pub(crate) rejected: u64,
    /// Trades of bars older than the last bar, dropped.
    pub(crate) late_ticks: u64,
}

/// Merges `incoming`, in any order, into `bars` by the rules of each bar's origin. `bars` are the
/// bars of one series: valid, in time order, at most one per slot of `clock`, which must be the
/// same for every merge into the series. `from_ticks` holds the origins of the series' bars built
/// from ticks, distinguishing empty heartbeats from trades that extend the newest history bar
/// of the same slot once; keep it with the bars.
pub(crate) fn merge(
    bars: &mut Vec<Bar>,
    from_ticks: &mut BTreeMap<DateTime<Utc>, Origin>,
    clock: &BarClock,
    mut incoming: Vec<(Bar, Origin)>,
) -> Merged {
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
    let page_end = incoming
        .iter()
        .rev()
        .find(|(_, origin)| *origin == Origin::History)
        .map(|(bar, _)| slot(bar));
    let (first, newest) = (slot(first), slot(newest));
    let last = bars.last().map(slot);
    let lo = bars.partition_point(|bar| slot(bar) < first);
    let hi = bars.partition_point(|bar| slot(bar) <= newest);
    let mut stored = bars[lo..hi].iter().peekable();
    let mut window = Vec::with_capacity(hi - lo + incoming.len());
    for (bar, origin) in incoming {
        let at = slot(&bar);
        if let Origin::Ticks(trades) | Origin::TicksAfterHeartbeat(trades) = origin
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
            (Some(old), Origin::Ticks(0)) if !old.is_whitespace() => old.clone(),
            (Some(old), Origin::TicksAfterHeartbeat(_)) if from_ticks.contains_key(&old.time) => {
                from_ticks.remove(&old.time);
                from_ticks.insert(bar.time, origin);
                bar
            }
            (Some(old), Origin::Ticks(trades) | Origin::TicksAfterHeartbeat(trades)) => {
                // Trades filling a whitespace bar extend its eventual history bar.
                if old.is_whitespace() {
                    from_ticks.insert(old.time, origin);
                }
                fold(old, &bar).unwrap_or_else(|| {
                    merged.rejected += trades;
                    old.clone()
                })
            }
            (Some(old), Origin::History) if !from_ticks.contains_key(&old.time) => old.clone(),
            (Some(old), Origin::History)
                if Some(at) != page_end || from_ticks.get(&old.time) == Some(&Origin::Ticks(0)) =>
            {
                from_ticks.remove(&old.time);
                bar
            }
            (Some(old), Origin::History) => match fold(&bar, old) {
                Some(folded) => {
                    from_ticks.remove(&old.time);
                    folded
                }
                None => {
                    merged.rejected += 1;
                    old.clone()
                }
            },
            (Some(old), Origin::Live) => {
                from_ticks.remove(&old.time);
                bar
            }
            (None, Origin::Ticks(_) | Origin::TicksAfterHeartbeat(_)) => {
                from_ticks.insert(bar.time, origin);
                bar
            }
            (None, _) => bar,
        };
        if old.is_none_or(|old| !same(old, &new)) {
            merged.first_changed.get_or_insert(lo + window.len());
        }
        window.push(new);
    }
    window.extend(stored.cloned());
    bars.splice(lo..hi, window);
    merged
}

/// Whether `a` and `b` are the same bar, with values compared bitwise so that a whitespace bar is
/// the same as itself.
fn same(a: &Bar, b: &Bar) -> bool {
    let bits = |bar: &Bar| [bar.open, bar.high, bar.low, bar.close, bar.volume].map(f64::to_bits);
    a.time == b.time && bits(a) == bits(b) && a.colors == b.colors && a.payload == b.payload
}

/// Finite values, a high at or above open and close, a low at or below them, no negative volume;
/// or a whitespace bar.
pub(crate) fn valid(bar: &Bar) -> bool {
    [bar.open, bar.high, bar.low, bar.close, bar.volume]
        .into_iter()
        .all(f64::is_finite)
        && bar.high >= bar.open.max(bar.close)
        && bar.low <= bar.open.min(bar.close)
        && bar.volume >= 0.0
        || bar.is_whitespace()
}

/// `bar` extended with `ticks`, a bar of later trades in the same slot, or `None` if the result is
/// not valid: two valid volumes can add up to infinity. Whitespace `ticks` leave `bar` as it is,
/// and the values of `ticks` fill a whitespace `bar`.
pub(super) fn fold(bar: &Bar, ticks: &Bar) -> Option<Bar> {
    if ticks.is_whitespace() {
        return Some(bar.clone());
    }
    if bar.is_whitespace() {
        return Some(Bar {
            open: ticks.open,
            high: ticks.high,
            low: ticks.low,
            close: ticks.close,
            volume: ticks.volume,
            ..bar.clone()
        });
    }
    let folded = Bar {
        high: bar.high.max(ticks.high),
        low: bar.low.min(ticks.low),
        close: ticks.close,
        volume: bar.volume + ticks.volume,
        ..bar.clone()
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
        let zero_size_trades: Vec<_> = {
            let inbox = std::sync::Arc::new(crate::data::inbox::Inbox::default());
            let sink = inbox.subscribe("ES".into(), Timeframe::Min1.into(), 100, None);
            [vec![(0, 10.0)], vec![(10, 12.0), (20, 11.0)]]
                .into_iter()
                .map(|trades| {
                    for (second, price) in trades {
                        sink.push_tick(crate::model::Tick::new(at(0, second), price, 0.0, None));
                    }
                    let (_, updates) = inbox.drain().updates.pop().unwrap();
                    let (bar, origin) = updates.bars.into_values().next().unwrap();
                    (origin, vec![bar])
                })
                .collect()
        };
        let heartbeat = {
            let inbox = std::sync::Arc::new(crate::data::inbox::Inbox::default());
            let sink = inbox.subscribe("ES".into(), Timeframe::Min1.into(), 100, None);
            sink.push_tick(crate::model::Tick::new(at(0, 0), 10.0, 1.0, None));
            inbox.drain();
            sink.advance_time(at(1, 0));
            let (_, updates) = inbox.drain().updates.pop().unwrap();
            let (bar, origin) = updates.bars.into_values().next().unwrap();
            (origin, vec![bar])
        };
        let heartbeat_trade = {
            let inbox = std::sync::Arc::new(crate::data::inbox::Inbox::default());
            let sink = inbox.subscribe("ES".into(), Timeframe::Min1.into(), 100, None);
            sink.push_tick(crate::model::Tick::new(at(0, 0), 10.0, 1.0, None));
            inbox.drain();
            sink.advance_time(at(1, 0));
            sink.push_tick(crate::model::Tick::new(at(1, 10), 14.0, 2.0, None));
            let (_, updates) = inbox.drain().updates.pop().unwrap();
            let (bar, origin) = updates.bars.into_values().next().unwrap();
            (origin, vec![bar])
        };
        #[rustfmt::skip]
        let cases = [
            ("a heartbeat leaves the page bar in its slot unchanged",
                vec![(History, vec![ohlcv(1, [12.0, 13.0, 11.5, 12.5, 6.0])]), heartbeat],
                vec![ohlcv(1, [12.0, 13.0, 11.5, 12.5, 6.0])], (None, 0, 0)),
            ("a page bar folds in zero-size trades already in its slot",
                vec![zero_size_trades[0].clone(),
                    (History, vec![ohlcv(0, [12.0, 13.0, 11.5, 12.5, 6.0])])],
                vec![ohlcv(0, [12.0, 13.0, 10.0, 10.0, 6.0])], (Some(0), 0, 0)),
            ("zero-size trades keep the forming candle across drains",
                zero_size_trades,
                vec![ohlcv(0, [10.0, 12.0, 10.0, 11.0, 0.0])], (Some(0), 0, 0)),
            ("a trade after a pending heartbeat extends the page in its slot",
                vec![(History, vec![ohlcv(1, [12.0, 13.0, 11.5, 12.5, 6.0])]), heartbeat_trade],
                vec![ohlcv(1, [12.0, 14.0, 11.5, 14.0, 8.0])], (Some(0), 0, 0)),
            ("a page replaces a zero-volume heartbeat candle",
                vec![(Ticks(0), vec![ohlcv(1, [10.0, 10.0, 10.0, 10.0, 0.0])]),
                    (History, vec![ohlcv(1, [12.0, 13.0, 11.5, 12.5, 6.0])])],
                vec![ohlcv(1, [12.0, 13.0, 11.5, 12.5, 6.0])], (Some(0), 0, 0)),
            ("a retried page replaces completed streamed slots",
                vec![(Ticks(1), vec![bar_at(1, 30, 10.0)]),
                    (Ticks(1), vec![bar_at(2, 10, 11.0)]),
                    (History, vec![ohlcv(1, [9.0, 10.0, 9.0, 10.0, 4.0]), bar(2, 11.0), bar(3, 12.0)])],
                vec![ohlcv(1, [9.0, 10.0, 9.0, 10.0, 4.0]), bar(2, 11.0), bar(3, 12.0)], (Some(0), 0, 0)),
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
            ("a page bar folds in the trades already in its slot",
                vec![(Ticks(1), vec![ohlcv(1, [205.0, 205.0, 205.0, 205.0, 2.0])]),
                    (Ticks(1), vec![ohlcv(1, [210.0, 210.0, 210.0, 210.0, 2.0])]),
                    (Ticks(1), vec![ohlcv(1, [207.0, 207.0, 207.0, 207.0, 2.0])]),
                    (History, vec![ohlcv(1, [200.0, 201.0, 199.0, 200.0, 50.0])])],
                vec![ohlcv(1, [200.0, 210.0, 199.0, 207.0, 56.0])], (Some(0), 0, 0)),
            ("a repeated page does not fold the same trades twice",
                vec![(Ticks(2), vec![ohlcv(1, [200.0, 200.0, 90.0, 90.0, 4.0])]),
                    (History, vec![bar(1, 105.0)]), (History, vec![bar(1, 105.0)])],
                vec![ohlcv(1, [105.0, 200.0, 90.0, 90.0, 5.0])], (None, 0, 0)),
            ("a page that would overflow is rejected and leaves trades intact",
                vec![(Ticks(3), vec![ohlcv(1, [5.0, 5.0, 5.0, 5.0, 1e308])]),
                    (History, vec![ohlcv(1, [5.0, 5.0, 5.0, 5.0, 1.5e308])])],
                vec![ohlcv(1, [5.0, 5.0, 5.0, 5.0, 1e308])], (None, 1, 0)),
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
            ("a whitespace bar holds its slot",
                vec![(History, vec![bar(0, 1.0), Bar::whitespace(at(1, 0)), bar(2, 3.0)])],
                vec![bar(0, 1.0), Bar::whitespace(at(1, 0)), bar(2, 3.0)], (Some(0), 0, 0)),
            ("an identical whitespace bar changes nothing",
                vec![(History, vec![Bar::whitespace(at(1, 0))]),
                    (Live, vec![Bar::whitespace(at(1, 0))])],
                vec![Bar::whitespace(at(1, 0))], (None, 0, 0)),
            ("trades fill a whitespace bar and then extend its page bar",
                vec![(History, vec![bar(0, 1.0), Bar::whitespace(at(1, 0))]),
                    (Ticks(1), vec![bar_at(1, 10, 4.0)]), (History, vec![bar_at(1, 20, 6.0)])],
                vec![bar(0, 1.0), Bar::new(at(1, 20), 6.0, 6.0, 4.0, 4.0, 2.0)], (Some(1), 0, 0)),
        ];
        let clock = BarClock::new(Timeframe::Min1);
        for (name, steps, bars, expected) in cases {
            let (mut series, mut from_ticks) = (Vec::new(), BTreeMap::new());
            let mut merged = None;
            for (origin, incoming) in steps {
                merged = Some(merge(
                    &mut series,
                    &mut from_ticks,
                    &clock,
                    incoming.into_iter().map(|bar| (bar, origin)).collect(),
                ));
            }
            let merged = merged.unwrap();
            // Debug output, in which a whitespace bar's NaN equals itself.
            assert_eq!(format!("{series:?}"), format!("{bars:?}"), "{name}");
            assert_eq!(
                (merged.first_changed, merged.rejected, merged.late_ticks),
                expected,
                "{name}"
            );
        }
    }
}
