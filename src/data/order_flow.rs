//! Stream inputs for studies that cannot be calculated from OHLC bars.

use super::OrderFilter;
use crate::model::{BarClock, Level1Quote, OrderBook, OrderBookLevel, Tick};
use chrono::{DateTime, TimeDelta, Utc};
use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard};

/// Fixed stream aggregation performed before a feed coalesces its updates for painting.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum OrderFlowKind {
    /// Last, arithmetic mean and maximum quoted spread, in instrument ticks, per bar.
    Spread,
    /// Last resting bid size, ask size and their difference. `distance == 0` selects the
    /// nearest `levels` on each side; a positive distance selects prices within that many
    /// price units of each side's inside market, inclusive. With an [`OrderFilter`] (see
    /// [`OrderFlowInput::with_order_filter`]) the sizes are those of the individual orders it
    /// accepts, and a bar whose feed sends aggregate depth only has no value.
    BookSize {
        /// Maximum levels when distance is zero.
        levels: usize,
        /// Price distance, zero for level count.
        distance: f64,
    },
    /// Bar total, rolling rate per second and instant rate per second. Windows are
    /// `(now - seconds, now]`; `volume == false` counts executions, including zero-size ones.
    TradeRate {
        /// Sum size rather than execution count.
        volume: bool,
        /// Rolling window in seconds.
        seconds: usize,
        /// Instant window in seconds.
        instant: usize,
    },
    /// Absolute bid and ask movement in ticks and their sum, per bar when `seconds == 0`,
    /// otherwise over `(now - seconds, now]`. The first quote after book loss seeds anew.
    LevelsTradedThrough {
        /// Rolling seconds, or zero for per-bar totals.
        seconds: usize,
    },
}

impl OrderFlowKind {
    pub(crate) fn normalized(self) -> Self {
        match self {
            Self::BookSize { levels, distance } => Self::BookSize {
                levels: levels.clamp(1, 10000),
                distance: if distance.is_nan() {
                    0.0
                } else {
                    distance.clamp(0.0, f64::MAX)
                },
            },
            Self::TradeRate {
                volume,
                seconds,
                instant,
            } => Self::TradeRate {
                volume,
                seconds: seconds.clamp(1, 86400),
                instant: instant.clamp(1, 86400),
            },
            Self::LevelsTradedThrough { seconds } => Self::LevelsTradedThrough {
                seconds: seconds.min(86400),
            },
            Self::Spread => Self::Spread,
        }
    }
}

#[derive(Default)]
struct Window {
    events: VecDeque<(DateTime<Utc>, [f64; 2])>,
    sum: [f64; 2],
    lost_until: Option<DateTime<Utc>>,
    now: Option<DateTime<Utc>>,
}

impl Window {
    fn push(&mut self, time: DateTime<Utc>, value: [f64; 2], seconds: usize) -> [f64; 2] {
        let now = self.now.map_or(time, |last| time.max(last));
        self.now = Some(now);
        let cutoff = now
            .checked_sub_signed(TimeDelta::seconds(seconds.clamp(1, 86400) as i64))
            .unwrap_or(DateTime::<Utc>::MIN_UTC);
        while self.events.front().is_some_and(|(t, _)| *t <= cutoff) {
            let (_, old) = self.events.pop_front().unwrap();
            for (line, old) in old.into_iter().enumerate() {
                self.sum[line] =
                    if !self.sum[line].is_finite() || (old > 0.0 && old >= self.sum[line] * 0.5) {
                        self.events.iter().map(|(_, value)| value[line]).sum()
                    } else {
                        self.sum[line] - old
                    };
            }
        }
        if self.events.is_empty() {
            self.sum = [0.0; 2];
        }
        if time > cutoff && value != [0.0; 2] {
            let at = self.events.partition_point(|(t, _)| *t <= time);
            self.events.insert(at, (time, value));
            for (sum, x) in self.sum.iter_mut().zip(value) {
                *sum += x;
            }
        }
        if self.events.len() > 100_000 {
            let (time, old) = self.events.pop_front().unwrap();
            self.lost_until = self.lost_until.max(Some(time));
            for (sum, old) in self.sum.iter_mut().zip(old) {
                *sum -= old;
            }
        }
        if self.lost_until.is_some_and(|t| t > cutoff) {
            [f64::NAN; 2]
        } else {
            self.sum
        }
    }
}

struct State {
    kind: OrderFlowKind,
    bars: BTreeMap<DateTime<Utc>, ([f64; 3], u64)>,
    last: Option<DateTime<Utc>>,
    inside: Option<(f64, f64)>,
    window: Window,
    instant: Window,
    revision: u64,
    #[cfg(feature = "widget")]
    reset_revision: u64,
    changes: BTreeMap<DateTime<Utc>, u64>,
    filter: Option<OrderFilter>,
}

/// A bounded, thread-safe source shared by a stream study and its feed subscription.
///
/// The chart connects these automatically. Hosts without a feed can push validated historical
/// and live events through the same handle, in chronological order, then recalculate the study.
/// Clones share the source, including when a study is cloned for presentation or undo. At most
/// 10,000 bars and 100,000 events per rolling window are retained; an overflowing window gives
/// no value until the missing events have expired, rather than understating the rate.
#[derive(Clone)]
pub struct OrderFlowInput(Arc<Mutex<State>>);

impl OrderFlowInput {
    /// An empty source for this fixed aggregation.
    pub fn new(kind: OrderFlowKind) -> Self {
        Self(Arc::new(Mutex::new(State {
            kind: kind.normalized(),
            bars: BTreeMap::new(),
            last: None,
            inside: None,
            window: Window::default(),
            instant: Window::default(),
            revision: 0,
            #[cfg(feature = "widget")]
            reset_revision: 0,
            changes: BTreeMap::new(),
            filter: None,
        })))
    }

    /// The same source, with book size summed over the individual orders `filter` accepts. The
    /// feed must send order events; with aggregate depth only, or when the filter needs a
    /// participant or placement time that no held order has, a book size bar has no value (NaN),
    /// instead of an unfiltered one. Other aggregations ignore it. A filter that accepts every
    /// order is the same as none.
    pub fn with_order_filter(self, filter: OrderFilter) -> Self {
        self.state().filter = (filter != OrderFilter::default()).then_some(filter);
        self
    }

    /// The filter of book size, if it has one.
    pub(crate) fn order_filter(&self) -> Option<OrderFilter> {
        let state = self.state();
        state
            .filter
            .clone()
            .filter(|_| matches!(state.kind, OrderFlowKind::BookSize { .. }))
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    #[cfg(feature = "widget")]
    pub(crate) fn same_source(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }

    /// Clears values on a new symbol, interval or subscription.
    pub fn clear(&self) {
        let mut state = self.state();
        let kind = state.kind;
        let filter = state.filter.take();
        let revision = state.revision.wrapping_add(1);
        *state = State {
            kind,
            filter,
            bars: BTreeMap::new(),
            last: None,
            inside: None,
            window: Window::default(),
            instant: Window::default(),
            revision,
            #[cfg(feature = "widget")]
            reset_revision: revision,
            changes: BTreeMap::new(),
        };
    }

    #[cfg(feature = "widget")]
    pub(crate) fn revision(&self) -> u64 {
        self.state().revision
    }

    #[cfg(feature = "widget")]
    pub(crate) fn changed_since(&self, revision: u64) -> Option<DateTime<Utc>> {
        let state = self.state();
        if state.reset_revision > revision {
            return None;
        }
        state
            .changes
            .iter()
            .find(|(_, changed)| **changed > revision)
            .map(|(time, _)| *time)
    }

    #[cfg(feature = "studies")]
    pub(crate) fn values_from(&self, time: DateTime<Utc>) -> BTreeMap<DateTime<Utc>, [f64; 3]> {
        self.state()
            .bars
            .range(time..)
            .map(|(t, (row, _))| (*t, *row))
            .collect()
    }

    /// Copies the retained bar outputs in time order. Missing outputs are NaN.
    pub fn values(&self) -> BTreeMap<DateTime<Utc>, [f64; 3]> {
        self.state()
            .bars
            .iter()
            .map(|(t, (values, _))| (*t, *values))
            .collect()
    }

    /// Adds a quote. Invalid quotes, tick sizes and timestamps older than this stream are
    /// ignored. Crossed quotes are retained, so their spread can be negative.
    pub fn push_quote(&self, clock: &BarClock, quote: &Level1Quote, tick_size: f64) {
        if !tick_size.is_finite()
            || tick_size <= 0.0
            || !quote.bid.is_finite()
            || !quote.ask.is_finite()
        {
            return;
        }
        let mut state = self.state();
        if state.last.is_some_and(|t| quote.ts < t) {
            return;
        }
        let open = clock.bar_open(quote.ts);
        match state.kind {
            OrderFlowKind::Spread => {
                let spread = (quote.ask - quote.bid) / tick_size;
                let (row, count) = state.bars.entry(open).or_insert(([spread; 3], 0));
                *count += 1;
                row[0] = spread;
                row[1] += (spread - row[1]) / *count as f64;
                row[2] = row[2].max(spread);
            }
            OrderFlowKind::LevelsTradedThrough { seconds } => {
                let movement = state.inside.map_or([0.0; 2], |(bid, ask)| {
                    [
                        ((quote.bid - bid) / tick_size).round().abs(),
                        ((quote.ask - ask) / tick_size).round().abs(),
                    ]
                });
                state.inside = Some((quote.bid, quote.ask));
                let values = if seconds == 0 {
                    let old = state.bars.get(&open).map_or([0.0; 3], |(v, _)| *v);
                    [old[0] + movement[0], old[1] + movement[1]]
                } else {
                    state.window.push(quote.ts, movement, seconds)
                };
                state
                    .bars
                    .insert(open, ([values[0], values[1], values[0] + values[1]], 0));
            }
            _ => return,
        }
        state.finish(quote.ts, open);
    }

    /// Adds an execution already deduplicated by the host or chart's feed.
    pub fn push_trade(&self, clock: &BarClock, trade: &Tick) {
        if !trade.price.is_finite() || !trade.size.is_finite() || trade.size < 0.0 {
            return;
        }
        let mut state = self.state();
        let OrderFlowKind::TradeRate {
            volume,
            seconds,
            instant,
        } = state.kind
        else {
            return;
        };
        let x = if volume { trade.size } else { 1.0 };
        let rolling =
            state.window.push(trade.ts, [x, 0.0], seconds)[0] / seconds.clamp(1, 86400) as f64;
        let fast =
            state.instant.push(trade.ts, [x, 0.0], instant)[0] / instant.clamp(1, 86400) as f64;
        let open = clock.bar_open(trade.ts);
        let total = state.bars.get(&open).map_or(0.0, |(v, _)| v[0]) + x;
        state.bars.insert(open, ([total, rolling, fast], 0));
        let latest = state.last.map_or(trade.ts, |t| t.max(trade.ts));
        state.finish(latest, open);
    }

    /// Expires rolling rates and movement during an idle part of a bar with stream data.
    /// A bar without that data remains missing. Hosts without a feed call this on a heartbeat.
    pub fn advance_time(&self, clock: &BarClock, now: DateTime<Utc>) {
        let mut state = self.state();
        if state.last.is_some_and(|t| now < t) {
            return;
        }
        let open = clock.bar_open(now);
        let Some((row, _)) = state.bars.get(&open) else {
            return;
        };
        let total = row[0];
        let row = match state.kind {
            OrderFlowKind::TradeRate {
                seconds, instant, ..
            } => {
                let rolling = state.window.push(now, [0.0; 2], seconds)[0] / seconds as f64;
                let fast = state.instant.push(now, [0.0; 2], instant)[0] / instant as f64;
                [total, rolling, fast]
            }
            OrderFlowKind::LevelsTradedThrough { seconds } if seconds > 0 => {
                let values = state.window.push(now, [0.0; 2], seconds);
                [values[0], values[1], values[0] + values[1]]
            }
            _ => return,
        };
        state.bars.insert(open, (row, 0));
        state.finish(now, open);
    }

    #[cfg(feature = "widget")]
    pub(crate) fn replace_trades<'a>(
        &self,
        clock: &BarClock,
        trades: impl Iterator<Item = &'a Tick>,
    ) {
        if !matches!(self.state().kind, OrderFlowKind::TradeRate { .. }) {
            return;
        }
        let as_of = self.state().last;
        self.clear();
        let mut sorted: Vec<_> = trades.collect();
        sorted.sort_by_key(|trade| trade.ts);
        for trade in sorted {
            self.push_trade(clock, trade);
        }
        if let Some(now) = as_of {
            self.advance_time(clock, now);
        }
    }

    /// Adds a synchronized book, or marks its loss with `None`. A loss clears the current
    /// bar's book values and breaks the inside-market movement path until it is seeded again.
    pub fn push_book(&self, clock: &BarClock, time: DateTime<Utc>, book: Option<&OrderBook>) {
        self.book_levels(
            clock,
            time,
            book.map(|b| {
                b.bids
                    .iter()
                    .map(|l| (true, l))
                    .chain(b.asks.iter().map(|l| (false, l)))
            }),
        );
    }

    pub(crate) fn book_levels<'a>(
        &self,
        clock: &BarClock,
        time: DateTime<Utc>,
        levels: Option<impl Iterator<Item = (bool, &'a OrderBookLevel)>>,
    ) {
        let mut state = self.state();
        if levels.is_none() {
            state.inside = None;
        }
        let OrderFlowKind::BookSize {
            levels: count,
            distance,
        } = state.kind
        else {
            return;
        };
        if state.last.is_some_and(|t| time < t) {
            return;
        }
        let mut row = [f64::NAN; 3];
        if let Some(levels) = levels {
            let mut sizes = [0.0; 2];
            let mut seen = [0; 2];
            let mut inside = [None; 2];
            for (bid, level) in levels {
                if !level.price.is_finite() || !level.size.is_finite() || level.size < 0.0 {
                    return;
                }
                let side = usize::from(!bid);
                let first = *inside[side].get_or_insert(level.price);
                let tolerance = 4.0 * f64::EPSILON * level.price.abs().max(first.abs());
                if (distance > 0.0 && (level.price - first).abs() <= distance + tolerance)
                    || (distance == 0.0 && seen[side] < count.max(1))
                {
                    sizes[side] += level.size;
                }
                seen[side] += 1;
            }
            row = [sizes[0], sizes[1], sizes[0] - sizes[1]];
        }
        state.bars.insert(clock.bar_open(time), (row, 0));
        state.finish(time, clock.bar_open(time));
    }
}

impl State {
    fn finish(&mut self, time: DateTime<Utc>, open: DateTime<Utc>) {
        self.last = Some(time);
        self.revision = self.revision.wrapping_add(1);
        self.changes.insert(open, self.revision);
        if self.changes.len() > 10_001 {
            self.changes.pop_first();
        }
        if self.bars.len() > 10_000
            && let Some((time, _)) = self.bars.pop_first()
        {
            self.changes.insert(time, self.revision);
            #[cfg(feature = "widget")]
            {
                self.reset_revision = self.revision;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Timeframe;

    #[test]
    fn late_trades_do_not_clear_an_overflow_gap_early() {
        let clock = BarClock::new(Timeframe::Min1);
        let start: DateTime<Utc> = "2026-01-05T10:00:00Z".parse().unwrap();
        let input = OrderFlowInput::new(OrderFlowKind::TradeRate {
            volume: false,
            seconds: 3,
            instant: 3,
        });
        for _ in 0..100_001 {
            input.push_trade(
                &clock,
                &Tick::new(start + TimeDelta::seconds(1), 100.0, 1.0, None),
            );
        }
        input.push_trade(&clock, &Tick::new(start, 100.0, 1.0, None));
        input.advance_time(&clock, start + TimeDelta::milliseconds(3500));
        assert!(
            input.values()[&start][1..].iter().all(|x| x.is_nan()),
            "the lost trade at second 1 is still in both windows"
        );
        input.advance_time(&clock, start + TimeDelta::seconds(4));
        assert_eq!(&input.values()[&start][1..], &[0.0, 0.0]);
    }

    #[test]
    fn rolling_windows_expire_without_counting_heartbeats_as_events() {
        let clock = BarClock::new(Timeframe::Min1);
        let start: DateTime<Utc> = "2026-01-05T10:00:00Z".parse().unwrap();
        for kind in [
            OrderFlowKind::TradeRate {
                volume: false,
                seconds: 60,
                instant: 3,
            },
            OrderFlowKind::TradeRate {
                volume: true,
                seconds: 60,
                instant: 3,
            },
            OrderFlowKind::LevelsTradedThrough { seconds: 3 },
        ] {
            let input = OrderFlowInput::new(kind);
            for second in [0, 1] {
                input.push_trade(
                    &clock,
                    &Tick::new(start + TimeDelta::seconds(second), 100.0, 2.0, None),
                );
                input.push_quote(
                    &clock,
                    &Level1Quote {
                        ts: start + TimeDelta::seconds(second),
                        symbol: "A".into(),
                        bid: 99.0 + second as f64,
                        ask: 101.0 + second as f64,
                        bid_size: 1.0,
                        ask_size: 1.0,
                    },
                    1.0,
                );
            }
            for _ in 0..100_001 {
                input.advance_time(&clock, start + TimeDelta::seconds(2));
            }
            assert!(
                input.values()[&start].iter().all(|x| x.is_finite()),
                "{kind:?}: heartbeats consume no event capacity"
            );
            input.advance_time(&clock, start + TimeDelta::seconds(4));
            let row = input.values()[&start];
            assert_eq!(row[2], 0.0, "{kind:?}: events at the cutoff expire");
            if matches!(kind, OrderFlowKind::LevelsTradedThrough { .. }) {
                assert_eq!(row, [0.0; 3]);
            }
        }
    }
}
