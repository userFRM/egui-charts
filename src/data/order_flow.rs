//! Stream inputs for studies that cannot be calculated from OHLC bars.

use super::OrderFilter;
use super::footprint::{Footprint, FootprintBar};
use crate::model::{BarClock, Level1Quote, OrderBook, OrderBookLevel, OrderSide, Tick};
use chrono::{DateTime, TimeDelta, Utc};
use std::collections::{BTreeMap, HashMap, VecDeque};
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
    /// Distinct explicitly identified aggressor orders: the bar total, and the rolling and
    /// instant rates per second over `(now - seconds, now]`. Fills sharing an aggressor order
    /// id count once, however many fills or windows they span; fills without one are neither
    /// merged nor counted.
    AggressorRate {
        /// Rolling window in seconds.
        seconds: usize,
        /// Instant window in seconds.
        instant: usize,
    },
    /// Distinct prices that buy and sell executions printed at, per bar when `seconds == 0`,
    /// otherwise over `(now - seconds, now]`, and their sum. Only executions count, so a
    /// quote that moves because a resting order was cancelled adds nothing, unlike
    /// [`LevelsTradedThrough`](Self::LevelsTradedThrough). Executions of unknown side count
    /// for neither side.
    LevelsExecuted {
        /// Rolling seconds, or zero for per-bar totals.
        seconds: usize,
        /// Restart a side's count when its executions reverse: a buy below the previous buy,
        /// or a sell above the previous sell.
        reversal: bool,
    },
    /// Buy minus sell volume per bar, counting only executions whose size lies in each of three
    /// inclusive `[min, max]` ranges, all from the same trades. A range with `min > max`
    /// matches nothing; executions of unknown side count for no range.
    SizeDelta {
        /// Inclusive size ranges, one per output.
        ranges: [[f64; 2]; 3],
    },
    /// Total volume and buy minus sell volume per bar, which a study sums over `bars` bars.
    FlowSum {
        /// Bars in the rolling sum.
        bars: usize,
    },
    /// Buy minus sell volume per bar of executions whose size lies in the inclusive range,
    /// which a study accumulates into a cumulative volume delta. The sum starts at the oldest
    /// bar with a retained value, so it starts again from a later bar after the input drops its
    /// oldest bars (see [`OrderFlowInput`]).
    FilteredCvd {
        /// Smallest size counted.
        min: f64,
        /// Largest size counted.
        max: f64,
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
            Self::AggressorRate { seconds, instant } => Self::AggressorRate {
                seconds: seconds.clamp(1, 86400),
                instant: instant.clamp(1, 86400),
            },
            Self::LevelsExecuted { seconds, reversal } => Self::LevelsExecuted {
                seconds: seconds.min(86400),
                reversal,
            },
            Self::SizeDelta { ranges } => Self::SizeDelta {
                ranges: ranges.map(|[min, max]| [size_bound(min), size_bound(max)]),
            },
            Self::FlowSum { bars } => Self::FlowSum {
                bars: bars.clamp(1, KEPT_BARS),
            },
            Self::FilteredCvd { min, max } => Self::FilteredCvd {
                min: size_bound(min),
                max: size_bound(max),
            },
            Self::LevelsTradedThrough { seconds } => Self::LevelsTradedThrough {
                seconds: seconds.min(86400),
            },
            Self::Spread => Self::Spread,
        }
    }
}

/// A size bound: NaN counts as zero, and the bound is kept in 0..=`f64::MAX`.
fn size_bound(size: f64) -> f64 {
    if size.is_nan() {
        0.0
    } else {
        size.clamp(0.0, f64::MAX)
    }
}

/// Bars an input keeps when it drops its oldest: the longest span of a flow sum.
const KEPT_BARS: usize = 10_000;
/// Bars an input holds before it drops its oldest down to `KEPT_BARS`. A drop recalculates the
/// studies on the input in full, so it comes once per 1,001 new bars rather than on every bar.
const MAX_BARS: usize = 11_000;
/// Price rows a footprint or book source keeps when it drops its oldest bars.
const KEPT_ROWS: usize = 100_000;
/// Price rows a footprint or book source holds before it drops its oldest bars, down to
/// `KEPT_BARS` bars and `KEPT_ROWS` rows.
const MAX_ROWS: usize = 110_000;

/// Bars copied from an [`OrderFlowInput`] at one moment, with what they cover.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct OrderFlowBars<T> {
    /// Bars by opening time, from the opening time asked for. A bar before
    /// [`complete_from`](Self::complete_from) is left out. During a chart replay only bars before
    /// the [`cutoff`](Self::cutoff) are present, and the bar containing it holds the data before
    /// the cutoff when the source knows it.
    pub bars: BTreeMap<DateTime<Utc>, T>,
    /// Opening time of the oldest bar the source holds in full, or `None` while it holds none.
    /// Older bars were dropped for retention or lost data. A bar from here on that is absent had
    /// none, except the bar containing the cutoff, which is also absent when its data before the
    /// cutoff is not known.
    pub complete_from: Option<DateTime<Utc>>,
    /// The exclusive replay cutoff applied to these bars, or `None` outside replay.
    pub cutoff: Option<DateTime<Utc>>,
    /// The tick size the source was given: footprint level `n` lies at the price
    /// `n * tick_size`.
    pub tick_size: Option<f64>,
    /// The source's revision when copied. It changes whenever the bars may have changed.
    pub revision: u64,
}

/// The last synchronized order book of one bar, as an [`OrderFlowInput::book`] source holds it.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct BarBook {
    /// Time of the update that left the book in this state.
    pub time: DateTime<Utc>,
    /// The nearest bid levels, best (highest price) first.
    pub bids: Vec<OrderBookLevel>,
    /// The nearest ask levels, best (lowest price) first.
    pub asks: Vec<OrderBookLevel>,
}

/// What a source aggregates.
#[derive(Clone, Copy)]
enum Source {
    /// Three outputs per bar.
    Fixed(OrderFlowKind),
    /// Each bar's executions by price.
    Footprint,
    /// Each bar's last synchronized book, this many levels per side.
    Book(usize),
}

/// A chart replay cutoff and the bar containing it, with what is known of that bar before it.
/// Options rather than an enum: only the chart builds one, and without `widget` an enum's
/// variants would never be constructed (`dead_code`).
struct Replay {
    cutoff: DateTime<Utc>,
    open: DateTime<Utc>,
    row: Option<[f64; 3]>,
    footprint: Option<FootprintBar>,
    book: Option<BarBook>,
}

/// Whether `clock`'s bars span time: footprint and book sources keep no others.
fn time_based(clock: &BarClock) -> bool {
    clock
        .timeframe()
        .is_some_and(|timeframe| !timeframe.is_trade_based())
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

/// Distinct keys with an event inside a window, for counts that must not repeat a key.
#[derive(Default)]
struct Distinct {
    events: VecDeque<(DateTime<Utc>, u64)>,
    counts: HashMap<u64, usize>,
    lost_until: Option<DateTime<Utc>>,
    now: Option<DateTime<Utc>>,
}

impl Distinct {
    /// Adds `key` at `time` (none to only expire), then expires events not after the cutoff:
    /// `seconds` before the newest time seen, or `bar` when given. Returns the distinct keys
    /// held, NaN while expired events were dropped to stay within bounds.
    fn push(
        &mut self,
        time: DateTime<Utc>,
        key: Option<u64>,
        seconds: usize,
        bar: Option<DateTime<Utc>>,
    ) -> f64 {
        let now = self.now.map_or(time, |last| time.max(last));
        self.now = Some(now);
        let cutoff = bar
            .map(|open| open - TimeDelta::nanoseconds(1))
            .or_else(|| now.checked_sub_signed(TimeDelta::seconds(seconds.clamp(1, 86400) as i64)))
            .unwrap_or(DateTime::<Utc>::MIN_UTC);
        while self.events.front().is_some_and(|(t, _)| *t <= cutoff) {
            let (_, old) = self.events.pop_front().unwrap();
            self.forget(old);
        }
        if let Some(key) = key.filter(|_| time > cutoff) {
            self.events.push_back((time, key));
            *self.counts.entry(key).or_default() += 1;
        }
        if self.events.len() > 100_000 {
            let (time, old) = self.events.pop_front().unwrap();
            self.lost_until = self.lost_until.max(Some(time));
            self.forget(old);
        }
        if self.lost_until.is_some_and(|t| t > cutoff) {
            f64::NAN
        } else {
            self.counts.len() as f64
        }
    }

    fn forget(&mut self, key: u64) {
        if let Some(count) = self.counts.get_mut(&key) {
            *count -= 1;
            if *count == 0 {
                self.counts.remove(&key);
            }
        }
    }

    /// Drops every key, keeping the time seen.
    fn restart(&mut self) {
        self.events.clear();
        self.counts.clear();
    }
}

struct State {
    source: Source,
    trade_bars: super::inbox::TradeBars,
    bars: BTreeMap<DateTime<Utc>, ([f64; 3], u64)>,
    last: Option<DateTime<Utc>>,
    inside: Option<(f64, f64)>,
    window: Window,
    instant: Window,
    /// Distinct keys of the bar, rolling and instant windows, or of the buy and sell prices.
    distinct: [Distinct; 3],
    /// Price of the previous buy and sell execution.
    last_price: [Option<f64>; 2],
    revision: u64,
    reset_revision: u64,
    changes: BTreeMap<DateTime<Utc>, u64>,
    filter: Option<OrderFilter>,
    /// A footprint source's executions, from its first execution with a tick size.
    footprint: Option<Footprint>,
    /// A book source's book at the end of each bar.
    books: BTreeMap<DateTime<Utc>, BarBook>,
    /// Levels held in `books`.
    book_rows: usize,
    tick_size: Option<f64>,
    replay: Option<Replay>,
}

/// A bounded, thread-safe source shared by a stream study and its feed subscription.
///
/// The chart connects these automatically. Hosts without a feed can push validated historical
/// and live events through the same handle, in chronological order, then recalculate the study.
/// Clones share the source, including when a study is cloned for presentation or undo.
///
/// At most 11,000 bars are retained: the bar after that drops the oldest at once, down to the
/// newest 10,000, which still cover the longest flow sum. A study recalculated after a drop has
/// no values for the dropped bars, and a cumulative one, such as
/// [`FilteredCvd`](OrderFlowKind::FilteredCvd), starts again from the oldest retained bar. The
/// chart recalculates the studies on a source in full after each drop, once per 1,001 new bars,
/// and otherwise updates them from the first changed bar. At most 100,000 events per rolling
/// window are retained; an overflowing window gives no value until the missing events have
/// expired, rather than understating the rate.
///
/// Besides the fixed aggregations of [`OrderFlowKind`], a source can hold each bar's executions
/// by price ([`footprint`](Self::footprint)) or its last synchronized book
/// ([`book`](Self::book)) for a host's study. These keep at most 11,000 bars and 110,000 price
/// rows, footprint levels or book levels; past either, the oldest bars go at once, down to 10,000
/// bars and 100,000 rows, and the chart recalculates their studies in full.
///
/// During a chart replay every reader stops at the cutoff (see [`OrderFlowBars`]).
#[derive(Clone)]
pub struct OrderFlowInput(Arc<Mutex<State>>);

impl OrderFlowInput {
    /// An empty source for this fixed aggregation.
    pub fn new(kind: OrderFlowKind) -> Self {
        Self::of(Source::Fixed(kind.normalized()))
    }

    /// An empty source of each bar's executions by price, one level per instrument tick, as
    /// [`Footprint`] aggregates them, for a study to read with
    /// [`footprints_from`](Self::footprints_from). It keeps nothing until it has a tick size
    /// ([`set_tick_size`](Self::set_tick_size); the chart gives it its instrument's), and only
    /// bars of an interval measured in time: under a clock without a timeframe, or with a tick,
    /// volume, range or delta interval, it ignores executions. An execution older than the one
    /// before it is left out, as from the chart's footprint.
    pub fn footprint() -> Self {
        Self::of(Source::Footprint)
    }

    /// An empty source of the last synchronized book of each bar, its nearest `levels` on each
    /// side (clamped to 1..=100), for a study to read with [`books_from`](Self::books_from). A
    /// bar without a synchronized update keeps no book, and neither does one whose last update
    /// was a loss. An update with an invalid level, or older than the source's last, is ignored.
    /// Only bars of an interval measured in time are kept, and an order filter does not apply.
    pub fn book(levels: usize) -> Self {
        Self::of(Source::Book(levels.clamp(1, 100)))
    }

    fn of(source: Source) -> Self {
        Self(Arc::new(Mutex::new(State::new(source))))
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
            .filter(|_| matches!(state.source, Source::Fixed(OrderFlowKind::BookSize { .. })))
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

    /// Clears values on a new symbol, interval or subscription. The aggregation, order filter,
    /// tick size and any replay cutoff stay.
    pub fn clear(&self) {
        let mut state = self.state();
        let revision = state.revision.wrapping_add(1);
        // The cutoff stays, so nothing after it shows; what was known before it is gone.
        let replay = state.replay.take().map(|replay| Replay {
            row: None,
            footprint: None,
            book: None,
            ..replay
        });
        let filter = state.filter.take();
        *state = State {
            filter,
            tick_size: state.tick_size,
            replay,
            revision,
            reset_revision: revision,
            ..State::new(state.source)
        };
    }

    /// Sets the instrument tick size that footprint levels and range intervals count in. A size
    /// that is not finite and positive counts as none; a footprint source keeps nothing without
    /// one. Another size clears the footprint, whose levels no longer apply. Fixed sources use it
    /// for range bars. The chart sets its instrument's when connecting or subscribing the source.
    pub fn set_tick_size(&self, tick_size: Option<f64>) {
        let tick_size = tick_size.filter(|size| size.is_finite() && *size > 0.0);
        let mut state = self.state();
        if state.tick_size == tick_size {
            return;
        }
        state.tick_size = tick_size;
        if state.footprint.take().is_some() {
            state.changes.clear();
            state.revision = state.revision.wrapping_add(1);
            state.reset_revision = state.revision;
        }
    }

    #[cfg(feature = "studies")]
    pub(crate) fn reset_revision(&self) -> u64 {
        self.state().reset_revision
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

    pub(crate) fn values_from(&self, time: DateTime<Utc>) -> BTreeMap<DateTime<Utc>, [f64; 3]> {
        let state = self.state();
        let first = state.bars.keys().next().copied();
        // The count beside each row only serves the spread; readers drop it.
        (state.visible(&state.bars, first, time, |replay| {
            replay.row.map(|row| (row, 0))
        }))
        .into_iter()
        .map(|(time, (row, _))| (time, row))
        .collect()
    }

    /// Copies the retained bar outputs in time order. Missing outputs are NaN. During a chart
    /// replay, as [`OrderFlowBars::bars`]: nothing from the cutoff's bar on, except that bar's
    /// output of the executions before the cutoff, when known.
    pub fn values(&self) -> BTreeMap<DateTime<Utc>, [f64; 3]> {
        self.values_from(DateTime::<Utc>::MIN_UTC)
    }

    /// Copies the bars of a footprint source that open at or after `open`, with what they
    /// cover. Empty for other sources.
    pub fn footprints_from(&self, open: DateTime<Utc>) -> OrderFlowBars<FootprintBar> {
        let state = self.state();
        let complete_from = state.footprint.as_ref().and_then(Footprint::complete_from);
        let bars = match &state.footprint {
            Some(flow) => state.visible(flow.bars(), complete_from, open, |replay| {
                replay.footprint.clone()
            }),
            None => BTreeMap::new(),
        };
        state.snapshot(bars, complete_from)
    }

    /// Copies the books of a book source for the bars that open at or after `open`, with what
    /// they cover. Empty for other sources.
    pub fn books_from(&self, open: DateTime<Utc>) -> OrderFlowBars<BarBook> {
        let state = self.state();
        let complete_from = state.books.keys().next().copied();
        let bars = state.visible(&state.books, complete_from, open, |replay| {
            replay.book.clone()
        });
        state.snapshot(bars, complete_from)
    }

    /// Adds a quote. Invalid quotes, tick sizes and timestamps older than this stream are
    /// ignored. Crossed quotes are retained, so their spread can be negative. Footprint and book
    /// sources ignore it.
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
        let Source::Fixed(kind) = state.source else {
            return;
        };
        let Some(open) = state.open(clock, quote.ts) else {
            return;
        };
        match kind {
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

    /// Adds an execution already deduplicated by the host or chart's feed. A footprint source
    /// adds it at its price level; a book source ignores it. Fixed sources on tick, volume,
    /// range and delta intervals use the same allocations as [`super::aggregate_trades`]. Push
    /// every execution from the same grouping anchor as the chart, including into quote and
    /// book size sources, so later updates belong to the current bar. Range intervals need
    /// [`set_tick_size`](Self::set_tick_size) first. A split fill contributes only its allocated
    /// size to each bar, with size filters tested against the original execution.
    pub fn push_trade(&self, clock: &BarClock, trade: &Tick) {
        if !trade.price.is_finite() || !trade.size.is_finite() || trade.size < 0.0 {
            return;
        }
        let mut state = self.state();
        if matches!(state.source, Source::Footprint) {
            return state.push_footprint(clock, trade);
        }
        if let Some(timeframe) = clock.timeframe().filter(|tf| tf.is_trade_based()) {
            let tick_size = state.tick_size;
            let Ok(parts) = state.trade_bars.push(timeframe, trade, tick_size) else {
                return;
            };
            for (i, (bar, size)) in parts.into_iter().enumerate() {
                state.add_trade(trade, bar.time, size, i == 0);
            }
        } else {
            state.add_trade(trade, clock.bar_open(trade.ts), trade.size, true);
        }
    }

    /// The sink has already validated and allocated this execution to its chart bars.
    pub(crate) fn push_trade_part(
        &self,
        trade: &Tick,
        open: DateTime<Utc>,
        size: f64,
        first: bool,
    ) {
        let mut state = self.state();
        state.trade_bars.forming = Some(open);
        state.add_trade(trade, open, size, first);
    }

    pub(crate) fn set_trade_bar(&self, open: DateTime<Utc>) {
        self.state().trade_bars.forming = Some(open);
    }

    /// Expires rolling rates and movement during an idle part of a bar with stream data.
    /// A bar without that data remains missing. Hosts without a feed call this on a heartbeat.
    pub fn advance_time(&self, clock: &BarClock, now: DateTime<Utc>) {
        let mut state = self.state();
        if state.last.is_some_and(|t| now < t) {
            return;
        }
        let Some(open) = state.open(clock, now) else {
            return;
        };
        let Some((row, _)) = state.bars.get(&open) else {
            return;
        };
        let total = row[0];
        let Source::Fixed(kind) = state.source else {
            return;
        };
        let row = match kind {
            OrderFlowKind::TradeRate {
                seconds, instant, ..
            } => {
                let rolling = state.window.push(now, [0.0; 2], seconds)[0] / seconds as f64;
                let fast = state.instant.push(now, [0.0; 2], instant)[0] / instant as f64;
                [total, rolling, fast]
            }
            OrderFlowKind::AggressorRate { seconds, instant } => {
                let [_, rolling, fast] = &mut state.distinct;
                [
                    total,
                    rolling.push(now, None, seconds, None) / seconds as f64,
                    fast.push(now, None, instant, None) / instant as f64,
                ]
            }
            OrderFlowKind::LevelsExecuted { seconds, .. } if seconds > 0 => {
                let [buys, sells, _] = &mut state.distinct;
                let levels = [
                    buys.push(now, None, seconds, None),
                    sells.push(now, None, seconds, None),
                ];
                [levels[0], levels[1], levels[0] + levels[1]]
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

    /// Rebuilds the source from the executions the chart holds, in place of what it holds. A
    /// bar that lost an execution to the store's limit is left out, and a rolling window gives no
    /// value while it reaches back to the loss, as when the window overflows.
    #[cfg(feature = "widget")]
    pub(crate) fn replace_trades(&self, clock: &BarClock, trades: &super::trades::Trades) {
        let footprint = match self.state().source {
            Source::Fixed(
                OrderFlowKind::TradeRate { .. }
                | OrderFlowKind::AggressorRate { .. }
                | OrderFlowKind::LevelsExecuted { .. }
                | OrderFlowKind::SizeDelta { .. }
                | OrderFlowKind::FlowSum { .. }
                | OrderFlowKind::FilteredCvd { .. },
            ) => false,
            Source::Footprint => true,
            _ => return,
        };
        let as_of = self.state().last;
        self.clear();
        let lost = trades.lost_until();
        if footprint {
            // In arrival order, as the chart's footprint is built.
            for trade in trades.iter() {
                self.push_trade(clock, trade);
            }
            let mut state = self.state();
            if let (Some(lost), Some(flow)) = (lost, &mut state.footprint) {
                flow.drop_through(lost, trades.iter());
            }
        } else {
            {
                let mut state = self.state();
                state.window.lost_until = lost;
                state.instant.lost_until = lost;
                for distinct in &mut state.distinct {
                    distinct.lost_until = lost;
                }
            }
            let mut sorted: Vec<_> = trades.iter().collect();
            sorted.sort_by_key(|trade| trade.ts);
            for trade in sorted {
                self.push_trade(clock, trade);
            }
            if let Some(lost) = lost {
                let cut = clock.bar_open(lost);
                let mut state = self.state();
                state.bars.retain(|open, _| *open > cut);
                state.changes.retain(|open, _| *open > cut);
            }
        }
        if let Some(now) = as_of {
            self.advance_time(clock, now);
        }
    }

    /// Applies a chart replay cutoff, exclusive, or removes it with `None`. Readers then stop at
    /// the cutoff's bar. That bar holds the held executions before the cutoff for a source that
    /// sums them, unless the store dropped one of that bar's, and a book source shows the bar's
    /// book when the bar's last update came before the cutoff.
    #[cfg(feature = "widget")]
    pub(crate) fn set_replay(
        &self,
        clock: &BarClock,
        cutoff: Option<DateTime<Utc>>,
        trades: Option<&super::trades::Trades>,
    ) {
        let mut state = self.state();
        let replay = cutoff.map(|cutoff| {
            let trade_timeframe = clock.timeframe().filter(|tf| tf.is_trade_based());
            // shortcut: scan the held executions per input per seek; index them by time if
            // replay playback profiles show it.
            let partial = if let Some(timeframe) = trade_timeframe
                && let Source::Fixed(kind) = state.source
                && let Some(trades) = trades.filter(|trades| trades.lost_until().is_none())
            {
                let mut part = State::new(Source::Fixed(kind));
                let mut open = None;
                let mut sorted: Vec<_> = trades.iter().collect();
                sorted.sort_by_key(|trade| trade.ts);
                for trade in sorted {
                    let Ok(parts) = part.trade_bars.push(timeframe, trade, state.tick_size) else {
                        continue;
                    };
                    for (i, (bar, size)) in parts.into_iter().enumerate() {
                        if bar.time <= cutoff {
                            open = Some(bar.time);
                        }
                        if trade.ts < cutoff {
                            part.add_trade(trade, bar.time, size, i == 0);
                        }
                    }
                    // The first excluded execution can open a bar exactly at the cutoff.
                    if trade.ts >= cutoff {
                        break;
                    }
                }
                Some((open.unwrap_or(cutoff), part))
            } else {
                None
            };
            let trade_based = trade_timeframe.is_some();
            let open = if let Some((open, _)) = &partial {
                *open
            } else if trade_based {
                state
                    .bars
                    .range(..=cutoff)
                    .next_back()
                    .map_or(cutoff, |(open, _)| *open)
            } else {
                clock.bar_open(cutoff)
            };
            let held = trades
                .filter(|trades| {
                    (trades.lost_until())
                        .is_none_or(|lost| !trade_based && clock.bar_open(lost) < open)
                })
                .into_iter()
                .flat_map(|trades| trades.iter())
                .filter(|trade| (trade_based || open <= trade.ts) && trade.ts < cutoff);
            let mut replay = Replay {
                cutoff,
                open,
                row: None,
                footprint: None,
                book: None,
            };
            match state.source {
                Source::Fixed(kind) => {
                    if let Some((_, part)) = &partial {
                        replay.row = part.bars.get(&open).map(|(row, _)| *row);
                    } else if !trade_based
                        && matches!(
                            kind,
                            OrderFlowKind::SizeDelta { .. }
                                | OrderFlowKind::FlowSum { .. }
                                | OrderFlowKind::FilteredCvd { .. }
                        )
                    {
                        let part = OrderFlowInput::new(kind);
                        held.for_each(|trade| part.push_trade(clock, trade));
                        replay.row = part.state().bars.get(&open).map(|(row, _)| *row);
                    }
                }
                Source::Footprint => {
                    if let Some(tick_size) = state.tick_size.filter(|_| time_based(clock))
                        && let Ok(mut part) = Footprint::new(clock.clone(), tick_size, 1)
                    {
                        // In arrival order, so an execution older than the one before is left out.
                        for trade in held {
                            let _ = part.push(trade);
                        }
                        replay.footprint = part.bars().get(&open).cloned();
                    }
                }
                Source::Book(_) => {
                    replay.book = (state.books.get(&open))
                        .filter(|book| book.time < cutoff)
                        .cloned();
                }
            }
            replay
        });
        state.replay = replay;
        state.revision = state.revision.wrapping_add(1);
        state.reset_revision = state.revision;
    }

    /// Adds a synchronized book, or marks its loss with `None`. A loss clears the current
    /// bar's book values and breaks the inside-market movement path until it is seeded again.
    /// A book source keeps the nearest levels as its bar's book.
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
        let (count, distance) = match state.source {
            Source::Fixed(OrderFlowKind::BookSize {
                levels: count,
                distance,
            }) => (count, distance),
            Source::Book(count) => return state.push_bar_book(clock, time, count, levels),
            _ => return,
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
        let Some(open) = state.open(clock, time) else {
            return;
        };
        state.bars.insert(open, (row, 0));
        state.finish(time, open);
    }
}

/// The execution's size, negative for a sell, when it is classified and its size lies in
/// `min..=max`; zero otherwise.
fn signed_size(trade: &Tick, min: f64, max: f64, size: f64) -> f64 {
    if !(min..=max).contains(&trade.size) {
        return 0.0;
    }
    match trade.side {
        Some(OrderSide::Buy) => size,
        Some(OrderSide::Sell) => -size,
        None => 0.0,
    }
}

impl State {
    fn open(&self, clock: &BarClock, time: DateTime<Utc>) -> Option<DateTime<Utc>> {
        if clock.timeframe().is_some_and(|tf| tf.is_trade_based()) {
            self.trade_bars.forming
        } else {
            Some(clock.bar_open(time))
        }
    }

    fn add_trade(&mut self, trade: &Tick, open: DateTime<Utc>, size: f64, first: bool) {
        let Source::Fixed(kind) = self.source else {
            return;
        };
        let row = match kind {
            OrderFlowKind::TradeRate {
                volume,
                seconds,
                instant,
            } => {
                let x = if volume { size } else { 1.0 };
                let event = if first {
                    if volume { trade.size } else { 1.0 }
                } else {
                    0.0
                };
                let rolling = self.window.push(trade.ts, [event, 0.0], seconds)[0]
                    / seconds.clamp(1, 86400) as f64;
                let fast = self.instant.push(trade.ts, [event, 0.0], instant)[0]
                    / instant.clamp(1, 86400) as f64;
                let total = self.bars.get(&open).map_or(0.0, |(v, _)| v[0]) + x;
                [total, rolling, fast]
            }
            OrderFlowKind::AggressorRate { seconds, instant } => {
                let key = trade.aggressor_order_id;
                let [bar, rolling, fast] = &mut self.distinct;
                [
                    bar.push(trade.ts.max(open), key, 1, Some(open)),
                    rolling.push(trade.ts, key.filter(|_| first), seconds, None)
                        / seconds.clamp(1, 86400) as f64,
                    fast.push(trade.ts, key.filter(|_| first), instant, None)
                        / instant.clamp(1, 86400) as f64,
                ]
            }
            OrderFlowKind::LevelsExecuted { seconds, reversal } => {
                let Some(side) = trade.side else { return };
                let side = usize::from(side == OrderSide::Sell);
                // Buyers lift offers upward and sellers hit bids downward.
                let reversed = self.last_price[side].is_some_and(|last| {
                    if side == 0 {
                        trade.price < last
                    } else {
                        trade.price > last
                    }
                });
                self.last_price[side] = Some(trade.price);
                if reversal && reversed {
                    self.distinct[side].restart();
                }
                let bar = (seconds == 0).then_some(open);
                let mut levels = [f64::NAN; 2];
                for (other, level) in levels.iter_mut().enumerate() {
                    let key = (other == side).then(|| trade.price.to_bits());
                    *level = self.distinct[other].push(
                        if seconds == 0 {
                            trade.ts.max(open)
                        } else {
                            trade.ts
                        },
                        key,
                        seconds,
                        bar,
                    );
                }
                [levels[0], levels[1], levels[0] + levels[1]]
            }
            OrderFlowKind::SizeDelta { ranges } => {
                let mut row = self.bars.get(&open).map_or([0.0; 3], |(v, _)| *v);
                for (sum, [min, max]) in row.iter_mut().zip(ranges) {
                    *sum += signed_size(trade, min, max, size);
                }
                row
            }
            OrderFlowKind::FlowSum { .. } => {
                let old = self.bars.get(&open).map_or([0.0; 3], |(v, _)| *v);
                [
                    old[0] + size,
                    old[1] + signed_size(trade, 0.0, f64::MAX, size),
                    0.0,
                ]
            }
            OrderFlowKind::FilteredCvd { min, max } => {
                let old = self.bars.get(&open).map_or(0.0, |(v, _)| v[0]);
                [old + signed_size(trade, min, max, size), 0.0, 0.0]
            }
            _ => return,
        };
        self.bars.insert(open, (row, 0));
        let latest = self.last.map_or(trade.ts, |t| t.max(trade.ts));
        self.finish(latest, open);
    }

    fn new(source: Source) -> Self {
        Self {
            source,
            trade_bars: super::inbox::TradeBars::default(),
            bars: BTreeMap::new(),
            last: None,
            inside: None,
            window: Window::default(),
            instant: Window::default(),
            distinct: Default::default(),
            last_price: [None; 2],
            revision: 0,
            reset_revision: 0,
            changes: BTreeMap::new(),
            filter: None,
            footprint: None,
            books: BTreeMap::new(),
            book_rows: 0,
            tick_size: None,
            replay: None,
        }
    }

    /// The bars of `stored` that readers see from `open` on: none before `complete_from`, none
    /// at or after the replay cutoff's bar, and that bar as `partial` gives it.
    fn visible<T: Clone>(
        &self,
        stored: &BTreeMap<DateTime<Utc>, T>,
        complete_from: Option<DateTime<Utc>>,
        open: DateTime<Utc>,
        partial: impl FnOnce(&Replay) -> Option<T>,
    ) -> BTreeMap<DateTime<Utc>, T> {
        let Some(from) = complete_from.map(|first| first.max(open)) else {
            return BTreeMap::new();
        };
        let end = self.replay.as_ref().map(|replay| replay.open);
        let mut bars: BTreeMap<_, _> = stored
            .range(from..)
            .take_while(|(time, _)| end.is_none_or(|end| **time < end))
            .map(|(time, bar)| (*time, bar.clone()))
            .collect();
        if let Some(replay) = &self.replay
            && replay.open >= from
            && let Some(bar) = partial(replay)
        {
            bars.insert(replay.open, bar);
        }
        bars
    }

    fn snapshot<T>(
        &self,
        bars: BTreeMap<DateTime<Utc>, T>,
        complete_from: Option<DateTime<Utc>>,
    ) -> OrderFlowBars<T> {
        OrderFlowBars {
            bars,
            complete_from,
            cutoff: self.replay.as_ref().map(|replay| replay.cutoff),
            tick_size: self.tick_size,
            revision: self.revision,
        }
    }

    /// Adds an execution to a footprint source. Past `MAX_BARS` bars or `MAX_ROWS` rows the
    /// footprint's budget takes the oldest bar; the rest of the chunk then goes at once, down to
    /// `KEPT_BARS` and `KEPT_ROWS`, so the studies start over once per chunk.
    fn push_footprint(&mut self, clock: &BarClock, trade: &Tick) {
        let Some(tick_size) = self.tick_size.filter(|_| time_based(clock)) else {
            return;
        };
        if self.footprint.is_none() {
            let Ok(mut flow) = Footprint::new(clock.clone(), tick_size, 1) else {
                return;
            };
            flow.set_budget(MAX_BARS, MAX_ROWS);
            self.footprint = Some(flow);
        }
        let Some(flow) = &mut self.footprint else {
            return;
        };
        let first = flow.bars().keys().next().copied();
        if !matches!(flow.push(trade), Ok(true)) {
            return;
        }
        let dropped = first.is_some() && flow.bars().keys().next().copied() != first;
        if dropped {
            flow.set_budget(KEPT_BARS, KEPT_ROWS);
            flow.set_budget(MAX_BARS, MAX_ROWS);
        }
        let first = flow.bars().keys().next().copied();
        let latest = self.last.map_or(trade.ts, |last| last.max(trade.ts));
        self.finish(latest, clock.bar_open(trade.ts));
        if dropped {
            // `changes` names only held bars, so it shares their bound; readers start over.
            if let Some(first) = first {
                self.changes.retain(|bar, _| *bar >= first);
            }
            self.reset_revision = self.revision;
        }
    }

    /// Keeps a book source's book at `time` as its bar's, or forgets the bar's book on a loss
    /// (`None`). An update older than the source's last, one with an invalid level, kept or not,
    /// and bars that do not span time are ignored. Past `MAX_BARS` bars with an update or
    /// `MAX_ROWS` levels, the oldest bars go at once, down to `KEPT_BARS` and `KEPT_ROWS`.
    fn push_bar_book<'a>(
        &mut self,
        clock: &BarClock,
        time: DateTime<Utc>,
        count: usize,
        levels: Option<impl Iterator<Item = (bool, &'a OrderBookLevel)>>,
    ) {
        if !time_based(clock) || self.last.is_some_and(|last| time < last) {
            return;
        }
        let open = clock.bar_open(time);
        let old = match levels {
            None => self.books.remove(&open),
            Some(levels) => {
                let mut book = BarBook {
                    time,
                    bids: Vec::new(),
                    asks: Vec::new(),
                };
                for (bid, level) in levels {
                    if !level.price.is_finite() || !level.size.is_finite() || level.size < 0.0 {
                        return;
                    }
                    let side = if bid { &mut book.bids } else { &mut book.asks };
                    if side.len() < count {
                        side.push(level.clone());
                    }
                }
                self.book_rows += book.bids.len() + book.asks.len();
                self.books.insert(open, book)
            }
        };
        if let Some(old) = old {
            self.book_rows -= old.bids.len() + old.asks.len();
        }
        self.finish(time, open);
        // Every bar with a book or a loss has a change record, so the records bound the bars;
        // dropping a record drops its bar's book.
        if self.changes.len() > MAX_BARS || self.book_rows > MAX_ROWS {
            while self.changes.len() > 1
                && (self.changes.len() > KEPT_BARS || self.book_rows > KEPT_ROWS)
                && let Some((open, _)) = self.changes.pop_first()
            {
                if let Some(book) = self.books.remove(&open) {
                    self.book_rows -= book.bids.len() + book.asks.len();
                }
            }
            self.reset_revision = self.revision;
        }
    }

    fn finish(&mut self, time: DateTime<Utc>, open: DateTime<Utc>) {
        self.last = Some(time);
        self.revision = self.revision.wrapping_add(1);
        self.changes.insert(open, self.revision);
        if self.bars.len() > MAX_BARS {
            while self.bars.len() > KEPT_BARS {
                self.bars.pop_first();
            }
            // `changes` names only retained bars, so it shares their bound.
            if let Some((&first, _)) = self.bars.first_key_value() {
                self.changes.retain(|bar, _| *bar >= first);
            }
            self.reset_revision = self.revision;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{OrderBook, OrderBookLevel, Timeframe};

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

    /// Quotes and books between fills belong to the current three-tick bar, even when
    /// the feed sends the entire burst before the chart drains it.
    #[cfg(feature = "widget")]
    #[test]
    fn trade_bar_streams_share_the_sinks_allocation_before_drain() {
        let clock = BarClock::new("3t".parse().unwrap());
        let start: DateTime<Utc> = "2026-01-05T10:00:00Z".parse().unwrap();
        let inbox = Arc::new(super::super::inbox::Inbox::default());
        let sink = inbox.subscribe("A".into(), clock.clone(), 100, Some(1.0));
        let spread = OrderFlowInput::new(OrderFlowKind::Spread);
        let book = OrderFlowInput::new(OrderFlowKind::BookSize {
            levels: 1,
            distance: 0.0,
        });
        let flow = OrderFlowInput::new(OrderFlowKind::FlowSum { bars: 1 });
        inbox.study_inputs(sink.id, vec![spread.clone(), book.clone()]);
        for second in 0..6 {
            if second == 2 {
                inbox.study_inputs(sink.id, vec![spread.clone(), book.clone(), flow.clone()]);
            }
            let at = start + TimeDelta::seconds(second);
            sink.push_tick(Tick::new(at, 100.0, 1.0, Some(OrderSide::Buy)));
            sink.push_quote(Level1Quote {
                ts: at + TimeDelta::milliseconds(100),
                symbol: "A".into(),
                bid: 99.0,
                ask: 101.0 + second as f64,
                bid_size: 1.0,
                ask_size: 1.0,
            });
            sink.push_depth_snapshot(
                second as u64,
                OrderBook {
                    ts: at + TimeDelta::milliseconds(200),
                    symbol: "A".into(),
                    bids: vec![OrderBookLevel {
                        price: 99.0,
                        size: second as f64 + 1.0,
                        order_cnt: None,
                    }],
                    asks: vec![],
                },
            );
        }
        let opens = [start, start + TimeDelta::seconds(3)];
        let got: Vec<_> = [&spread, &book, &flow]
            .iter()
            .map(|input| input.values().into_iter().collect::<Vec<_>>())
            .collect();
        assert_eq!(
            got,
            vec![
                vec![(opens[0], [4.0, 3.0, 4.0]), (opens[1], [7.0, 6.0, 7.0])],
                vec![(opens[0], [3.0, 0.0, 3.0]), (opens[1], [6.0, 0.0, 6.0])],
                vec![(opens[0], [3.0, 3.0, 0.0]), (opens[1], [3.0, 3.0, 0.0])],
            ]
        );
        let (_, updates) = inbox.drain().updates.pop().unwrap();
        let cutoff = start + TimeDelta::seconds(2);
        for input in [&spread, &book, &flow] {
            input.set_replay(&clock, Some(cutoff), Some(&updates.trades));
        }
        assert_eq!(flow.values(), BTreeMap::from([(start, [2.0, 2.0, 0.0])]));
        assert!(
            spread.values().is_empty(),
            "past quotes cannot be reconstructed"
        );
        assert!(
            book.values().is_empty(),
            "past depth cannot be reconstructed"
        );
        for input in [&spread, &book, &flow] {
            input.set_replay(&clock, Some(opens[1]), Some(&updates.trades));
        }
        assert_eq!(spread.values().keys().copied().collect::<Vec<_>>(), [start]);
        assert_eq!(book.values().keys().copied().collect::<Vec<_>>(), [start]);
        assert_eq!(flow.values(), BTreeMap::from([(start, [3.0, 3.0, 0.0])]));
        let mut trades = updates.trades;
        for second in [6, 7] {
            let trade = Tick::new(
                start + TimeDelta::seconds(second),
                100.0,
                1.0,
                Some(OrderSide::Buy),
            );
            sink.push_tick(trade.clone());
            trades.push(trade);
        }
        for input in [&spread, &book, &flow] {
            input.set_replay(&clock, Some(start + TimeDelta::seconds(7)), Some(&trades));
        }
        assert_eq!(spread.values().keys().copied().collect::<Vec<_>>(), opens);
        assert_eq!(book.values().keys().copied().collect::<Vec<_>>(), opens);
        assert_eq!(
            flow.values()[&(start + TimeDelta::seconds(6))],
            [1.0, 1.0, 0.0]
        );
    }

    /// Hand counts: ids 7 and 8 fill in the first minute (7 twice, an unidentified fill
    /// besides), and 7 again in the next, when the first minute has left the 60 and 3 second
    /// windows' opening and their earlier fills have expired.
    #[test]
    fn aggressor_rate_counts_each_identified_order_once() {
        let clock = BarClock::new(Timeframe::Min1);
        let start: DateTime<Utc> = "2026-01-05T10:00:00Z".parse().unwrap();
        let input = OrderFlowInput::new(OrderFlowKind::AggressorRate {
            seconds: 60,
            instant: 3,
        });
        for (second, id) in [
            (0, Some(7)),
            (1, Some(7)),
            (2, Some(8)),
            (3, None),
            (70, Some(7)),
        ] {
            let mut fill = Tick::new(start + TimeDelta::seconds(second), 100.0, 1.0, None);
            fill.aggressor_order_id = id;
            input.push_trade(&clock, &fill);
            if second == 3 {
                let row = input.values()[&start];
                assert_eq!(row, [2.0, 2.0 / 60.0, 2.0 / 3.0]);
            }
        }
        assert_eq!(
            input.values()[&(start + TimeDelta::minutes(1))],
            [1.0, 1.0 / 60.0, 1.0 / 3.0]
        );
    }

    /// Hand counts for one bar: buys at 100, 101, 102 and 101 again, a sell at 99.5, and a
    /// trade of unknown side. Without restarts the buys cover 3 prices; a restart on the
    /// reversal at 102 then 101 leaves the one. Quotes, even a cancellation that moves the
    /// bid three ticks, add nothing.
    #[test]
    fn executed_levels_count_distinct_prices_per_side_and_restart_on_reversal() {
        let clock = BarClock::new(Timeframe::Min1);
        let start: DateTime<Utc> = "2026-01-05T10:00:00Z".parse().unwrap();
        for (reversal, want) in [(false, [3.0, 1.0, 4.0]), (true, [1.0, 1.0, 2.0])] {
            let input = OrderFlowInput::new(OrderFlowKind::LevelsExecuted {
                seconds: 0,
                reversal,
            });
            let quote = |second, bid| Level1Quote {
                ts: start + TimeDelta::seconds(second),
                symbol: String::new(),
                bid,
                bid_size: 1.0,
                ask: 100.0,
                ask_size: 1.0,
            };
            input.push_quote(&clock, &quote(0, 99.0), 1.0);
            for (second, price, side) in [
                (1, 100.0, Some(OrderSide::Buy)),
                (2, 101.0, Some(OrderSide::Buy)),
                (3, 102.0, Some(OrderSide::Buy)),
                (4, 101.0, Some(OrderSide::Buy)),
                (5, 99.5, Some(OrderSide::Sell)),
                (6, 110.0, None),
            ] {
                let at = start + TimeDelta::seconds(second);
                input.push_trade(&clock, &Tick::new(at, price, 1.0, side));
            }
            input.push_quote(&clock, &quote(7, 96.0), 1.0);
            assert_eq!(input.values()[&start], want, "reversal {reversal}");
        }
    }

    /// Hand values: the book of 00:20 replaces that of 00:05 in bar 10:00, and the older one of
    /// 00:10 is ignored; in bar 10:01 a loss removes the first book, the next keeps its two best
    /// bids; bar 10:02 ends on a loss, and an update with an invalid third bid changes nothing.
    #[test]
    fn book_sources_keep_each_bars_last_synchronized_book() {
        let clock = BarClock::new(Timeframe::Min1);
        let start: DateTime<Utc> = "2026-01-05T10:00:00Z".parse().unwrap();
        let at = |second| start + TimeDelta::seconds(second);
        let level = |price, size| OrderBookLevel {
            price,
            size,
            order_cnt: None,
        };
        let levels = |sides: &[(f64, f64)]| sides.iter().map(|&(p, s)| level(p, s)).collect();
        let input = OrderFlowInput::book(2);
        for (second, sides) in [
            (
                5,
                Some((
                    vec![(99.0, 1.0), (98.0, 2.0), (97.0, 3.0)],
                    vec![(101.0, 4.0), (102.0, 5.0)],
                )),
            ),
            (20, Some((vec![(99.0, 7.0)], vec![(101.0, 8.0)]))),
            (10, Some((vec![(90.0, 1.0)], vec![(110.0, 1.0)]))),
            (65, Some((vec![(100.0, 1.0)], vec![(102.0, 1.0)]))),
            (70, None),
            (
                75,
                Some((
                    vec![(100.0, 5.0), (99.0, 6.0), (98.0, 7.0)],
                    vec![(102.0, 5.0)],
                )),
            ),
            (125, None),
            (
                130,
                Some((
                    vec![(100.0, 1.0), (99.0, 1.0), (98.0, f64::NAN)],
                    vec![(101.0, 1.0)],
                )),
            ),
        ] {
            let book = sides.map(|(bids, asks): (Vec<_>, Vec<_>)| OrderBook {
                ts: at(second),
                symbol: "A".into(),
                bids: levels(&bids),
                asks: levels(&asks),
            });
            input.push_book(&clock, at(second), book.as_ref());
        }
        let books = input.books_from(DateTime::<Utc>::MIN_UTC);
        let bar = |time, bids: &[(f64, f64)], asks: &[(f64, f64)]| BarBook {
            time,
            bids: levels(bids),
            asks: levels(asks),
        };
        assert_eq!(
            books.bars,
            BTreeMap::from([
                (at(0), bar(at(20), &[(99.0, 7.0)], &[(101.0, 8.0)])),
                (
                    at(60),
                    bar(at(75), &[(100.0, 5.0), (99.0, 6.0)], &[(102.0, 5.0)])
                ),
            ])
        );
        assert_eq!(
            (books.complete_from, books.cutoff, books.tick_size),
            (Some(at(0)), None, None)
        );
    }

    /// Hand counts, one bar a minute: a footprint of one row per bar drops at bar 11,000, the
    /// 11,001st, down to bars 1,001 to 11,000; a book of 200 levels per bar reaches 110,000 rows
    /// at bar 549 and drops at bar 550 down to 500 bars, 51 to 550; losses alone keep no book but
    /// a change record per bar, bounded the same way. Each drop recalculates in full once, and
    /// the next bar updates from itself.
    #[test]
    fn footprint_and_book_sources_drop_their_oldest_bars_in_chunks() {
        let clock = BarClock::new(Timeframe::Min1);
        let start: DateTime<Utc> = "2026-01-05T10:00:00Z".parse().unwrap();
        let minute = |bar: usize| start + TimeDelta::minutes(bar as i64);
        let footprint = OrderFlowInput::footprint();
        footprint.set_tick_size(Some(1.0));
        let bids: Vec<_> = (1..=100)
            .rev()
            .map(|price| OrderBookLevel {
                price: f64::from(price),
                size: 1.0,
                order_cnt: None,
            })
            .collect();
        let asks = (101..=200)
            .map(|price| OrderBookLevel {
                price: f64::from(price),
                size: 1.0,
                order_cnt: None,
            })
            .collect();
        let deep = OrderBook {
            ts: start,
            symbol: "A".into(),
            bids,
            asks,
        };
        let cases = [
            (footprint, 11_000, 1_001),
            (OrderFlowInput::book(100), 550, 51),
            (OrderFlowInput::book(1), 11_000, 1_001),
        ];
        for (case, (input, full, kept_from)) in cases.into_iter().enumerate() {
            let push = |bar| match case {
                0 => input.push_trade(
                    &clock,
                    &Tick::new(minute(bar), 100.0, 1.0, Some(OrderSide::Buy)),
                ),
                1 => input.push_book(&clock, minute(bar), Some(&deep)),
                _ => input.push_book(&clock, minute(bar), None),
            };
            let held = || {
                if case == 0 {
                    let copy = input.footprints_from(DateTime::<Utc>::MIN_UTC);
                    (copy.bars.len(), copy.complete_from)
                } else {
                    let copy = input.books_from(DateTime::<Utc>::MIN_UTC);
                    (copy.bars.len(), copy.complete_from)
                }
            };
            // Losses keep no book.
            let bars = |from: usize, to: usize| {
                if case == 2 {
                    (0, None)
                } else {
                    (to - from, Some(minute(from)))
                }
            };
            for bar in 0..full {
                push(bar);
            }
            assert_eq!(held(), bars(0, full), "case {case}, {full} bars");
            #[cfg(feature = "widget")]
            let before = input.revision();
            push(full);
            assert_eq!(
                held(),
                bars(kept_from, full + 1),
                "case {case}, drop at bar {full}"
            );
            #[cfg(feature = "widget")]
            assert_eq!(
                input.changed_since(before),
                None,
                "case {case}: full recalculation"
            );
            #[cfg(feature = "widget")]
            let before = input.revision();
            push(full + 1);
            assert_eq!(held(), bars(kept_from, full + 2), "case {case}, next bar");
            #[cfg(feature = "widget")]
            assert_eq!(input.changed_since(before), Some(minute(full + 1)));
        }
    }

    /// Hand levels: 100.2 is 200.4 ticks of 0.5, level 200, and 400.8 ticks of 0.25, level 401.
    /// A footprint keeps nothing without a tick size or on bars that do not span time.
    #[test]
    fn footprint_sources_follow_their_tick_size_and_time_bars() {
        let clock = BarClock::new(Timeframe::Min1);
        let start: DateTime<Utc> = "2026-01-05T10:00:00Z".parse().unwrap();
        let input = OrderFlowInput::footprint();
        let push = |clock: &BarClock, second, price| {
            let at = start + TimeDelta::seconds(second);
            input.push_trade(clock, &Tick::new(at, price, 1.0, Some(OrderSide::Buy)));
        };
        let held = || {
            let copy = input.footprints_from(DateTime::<Utc>::MIN_UTC);
            let levels: Vec<_> = (copy.bars.values())
                .flat_map(|bar| bar.levels.keys().copied())
                .collect();
            (copy.tick_size, levels)
        };
        push(&clock, 0, 100.2);
        assert_eq!(held(), (None, vec![]), "no tick size");
        input.set_tick_size(Some(0.5));
        push(&clock, 1, 100.2);
        assert_eq!(held(), (Some(0.5), vec![200]));
        input.set_tick_size(Some(0.25));
        assert_eq!(
            held(),
            (Some(0.25), vec![]),
            "another size clears the levels"
        );
        push(&clock, 2, 100.2);
        assert_eq!(held(), (Some(0.25), vec![401]));
        input.set_tick_size(Some(f64::NAN));
        assert_eq!(held(), (None, vec![]), "NaN is no tick size");
        input.set_tick_size(Some(1.0));
        push(&BarClock::new("1t".parse().unwrap()), 3, 100.0);
        assert_eq!(held(), (Some(1.0), vec![]), "tick bars do not span time");
    }

    /// Hand values: a store of two drops the buy of 00:10 when the sell of 01:05 arrives, so bar
    /// 10:00 is cut and bar 10:01 whole. The 60 second window at 01:05 reaches back to the loss;
    /// the 3 second window holds only the sell of 4.
    #[cfg(feature = "widget")]
    #[test]
    fn rebuilt_sources_leave_out_what_the_store_dropped() {
        let clock = BarClock::new(Timeframe::Min1);
        let start: DateTime<Utc> = "2026-01-05T10:00:00Z".parse().unwrap();
        let at = |second| start + TimeDelta::seconds(second);
        let mut store = super::super::trades::Trades::new(2);
        for (second, size, side) in [
            (10, 1.0, OrderSide::Buy),
            (50, 2.0, OrderSide::Buy),
            (65, 4.0, OrderSide::Sell),
        ] {
            store.push(Tick::new(at(second), 100.0, size, Some(side)));
        }
        let rate = OrderFlowInput::new(OrderFlowKind::TradeRate {
            volume: true,
            seconds: 60,
            instant: 3,
        });
        rate.replace_trades(&clock, &store);
        let values = rate.values();
        assert_eq!(values.keys().copied().collect::<Vec<_>>(), [at(60)]);
        let [total, rolling, instant] = values[&at(60)];
        assert_eq!((total, instant), (4.0, 4.0 / 3.0));
        assert!(
            rolling.is_nan(),
            "the rolling window reaches back to the loss"
        );
        let footprint = OrderFlowInput::footprint();
        footprint.set_tick_size(Some(1.0));
        footprint.replace_trades(&clock, &store);
        let flow = footprint.footprints_from(DateTime::<Utc>::MIN_UTC);
        assert_eq!(flow.complete_from, Some(at(60)));
        assert_eq!(flow.bars.keys().copied().collect::<Vec<_>>(), [at(60)]);
        assert_eq!(flow.bars[&at(60)].bid, 4.0);
    }
}
