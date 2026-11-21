//! Stream inputs for studies that cannot be calculated from OHLC bars.

use super::FootprintBarView;
use super::OrderFilter;
use super::footprint::{Footprint, FootprintBar, FootprintState};
use super::trades::{OrderSizes, SizeBasis};
use crate::model::{BarClock, Level1Quote, OrderBook, OrderBookLevel, OrderSide, Placement, Tick};
use chrono::{DateTime, TimeDelta, Utc};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::ops::RangeBounds;
use std::sync::{Arc, Mutex, MutexGuard};

#[cfg(all(feature = "widget", feature = "studies"))]
mod checkpoint;
#[cfg(all(feature = "widget", feature = "studies"))]
pub(crate) use checkpoint::{PackedTradeRate, RateGuard};
#[cfg(feature = "widget")]
mod footprint_checkpoint;
#[cfg(feature = "widget")]
pub(crate) use footprint_checkpoint::{FootprintGuard, PackedFootprintInput};

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
    /// matches nothing; executions of unknown side count for no range. With
    /// [`OrderFlowInput::with_size_basis`] a range can test the combined size of each
    /// execution's aggressive order instead.
    SizeDelta {
        /// Inclusive size ranges, one per output.
        ranges: [[f64; 2]; 3],
    },
    /// Total volume and buy minus sell volume per bar, which a study sums over `bars` bars.
    FlowSum {
        /// Bars in the rolling sum.
        bars: usize,
    },
    /// Buy minus sell volume, its highest and lowest running value including zero, per bar,
    /// counting executions whose size lies in the inclusive range, or whose aggressive order's
    /// combined size does (see [`OrderFlowInput::with_size_basis`]). A study accumulates the
    /// delta into a cumulative volume delta. The sum starts at the oldest
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
    /// Last synchronized resting balance, `(bid - ask) / (bid + ask) * 100`, of two
    /// non-overlapping bands, from -100 (asks only) to 100 (bids only).
    /// With zero `near_distance`, the near band takes `near_levels` levels per side and
    /// the far band the next `far_levels`. Otherwise, the near band reaches `near_distance`
    /// price units from each side's inside market, inclusive; the far band reaches
    /// `far_distance` further, inclusive. Level counts are unused in distance mode.
    /// An empty or lost band has no value (NaN), as does a band whose total size overflows.
    /// The third output is unused (NaN). An [`OrderFilter`] applies as for
    /// [`BookSize`](Self::BookSize).
    BookBalance {
        /// Near levels per side when `near_distance` is zero.
        near_levels: usize,
        /// Further levels per side, after the near band, in level mode.
        far_levels: usize,
        /// Near price distance, or zero to select both bands by level count.
        near_distance: f64,
        /// Further price distance beyond the near edge, in distance mode.
        far_distance: f64,
    },
    /// Highest minus lowest execution price over `(now - seconds, now]`, followed by
    /// the high and low prices, at each bar's latest execution or heartbeat.
    /// An empty window gives zero range and NaN prices. Late executions count at their
    /// own times. Each extreme retains at most 100,000 candidates; exceeding that bound
    /// leaves all outputs missing until the dropped executions expire. The Volatility
    /// Regime study ranks these ranges against earlier bars.
    PriceRange {
        /// Rolling window in seconds, clamped to 1..=86400.
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
            Self::BookBalance {
                near_levels,
                far_levels,
                near_distance,
                far_distance,
            } => Self::BookBalance {
                near_levels: near_levels.clamp(1, 10000),
                far_levels: far_levels.clamp(1, 10000),
                near_distance: size_bound(near_distance),
                far_distance: size_bound(far_distance),
            },
            Self::PriceRange { seconds } => Self::PriceRange {
                seconds: seconds.clamp(1, 86400),
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

/// An immutable read of a footprint input. Completed rows retain their owners; only mutable
/// bars are copied. Profile calculation and iteration hold no source lock.
#[derive(Clone, Debug)]
pub struct OrderFlowFootprints {
    flow: Option<FootprintState>,
    covered_from: Option<DateTime<Utc>>,
    replay_open: Option<DateTime<Utc>>,
    /// Opening time of the complete suffix, as in [`OrderFlowBars::complete_from`].
    pub complete_from: Option<DateTime<Utc>>,
    /// Exclusive replay cutoff, or `None` outside replay.
    pub cutoff: Option<DateTime<Utc>>,
    /// Instrument tick size used by these rows.
    pub tick_size: Option<f64>,
    /// Source revision at capture.
    pub revision: u64,
}

impl OrderFlowFootprints {
    /// The clock used to group these executions, or `None` before a footprint is initialized.
    pub fn clock(&self) -> Option<&BarClock> {
        self.flow.as_ref().map(|flow| flow.view().clock())
    }

    /// Complete held bars in `range`, including prepared bars before a missing join.
    pub fn bars(
        &self,
        range: impl RangeBounds<DateTime<Utc>>,
    ) -> impl DoubleEndedIterator<Item = FootprintBarView<'_>> {
        let bounds = (range.start_bound().cloned(), range.end_bound().cloned());
        self.flow
            .iter()
            .flat_map(move |flow| flow.view().complete_bars(bounds))
    }

    /// Whether every execution from `first` through `last` is known, including quiet slots.
    pub fn span_is_complete(&self, first: DateTime<Utc>, last: DateTime<Utc>) -> bool {
        self.flow.as_ref().is_some_and(|flow| {
            flow.view()
                .span_is_complete_from(self.covered_from, first, last)
                && self.replay_open.is_none_or(|open| {
                    let last = flow.view().clock().bar_open(last);
                    last < open || (last == open && flow.view().bar_at(open).is_some())
                })
        })
    }
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
#[derive(Clone)]
struct Replay {
    cutoff: DateTime<Utc>,
    open: DateTime<Utc>,
    row: Option<[f64; 3]>,
    footprint: Option<FootprintBar>,
    book: Option<BarBook>,
}

/// Parts of chart bars (execution, bar open and allocated size) and the grouping after them.
#[cfg(feature = "widget")]
type ChartRun<'a> = (Vec<(&'a Tick, DateTime<Utc>, f64)>, super::inbox::TradeBars);

/// Whether `clock`'s bars span time, as book sources require.
fn time_based(clock: &BarClock) -> bool {
    clock
        .timeframe()
        .is_some_and(|timeframe| !timeframe.is_trade_based())
}

#[derive(Clone, Default)]
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
#[derive(Clone, Default)]
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

/// Time-ordered candidates: decreasing high prices and increasing low prices.
#[derive(Clone, Default)]
struct Extremes {
    high: VecDeque<(DateTime<Utc>, f64)>,
    low: VecDeque<(DateTime<Utc>, f64)>,
    lost_until: Option<DateTime<Utc>>,
    now: Option<DateTime<Utc>>,
}

impl Extremes {
    /// Add a price, or just expire, relative to the newest observed time.
    fn push(&mut self, time: DateTime<Utc>, price: Option<f64>, seconds: usize) -> [f64; 3] {
        let now = self.now.map_or(time, |last| last.max(time));
        self.now = Some(now);
        let cutoff = now
            .checked_sub_signed(TimeDelta::seconds(seconds.clamp(1, 86400) as i64))
            .unwrap_or(DateTime::<Utc>::MIN_UTC);
        for side in [&mut self.high, &mut self.low] {
            while side.front().is_some_and(|(time, _)| *time <= cutoff) {
                side.pop_front();
            }
        }
        if let Some(price) = price.filter(|_| time > cutoff) {
            Self::insert(&mut self.high, time, price, |a, b| a >= b);
            Self::insert(&mut self.low, time, price, |a, b| a <= b);
        }
        for side in [&mut self.high, &mut self.low] {
            if side.len() > 100_000 {
                let (time, _) = side.pop_front().unwrap();
                self.lost_until = self.lost_until.max(Some(time));
            }
        }
        if self.lost_until.is_some_and(|time| time > cutoff) {
            return [f64::NAN; 3];
        }
        match (self.high.front(), self.low.front()) {
            (Some(&(_, high)), Some(&(_, low))) => [high - low, high, low],
            _ => [0.0, f64::NAN, f64::NAN],
        }
    }

    /// A later, at least as extreme price outlives this one; otherwise remove the
    /// earlier candidates it dominates, including any weaker price at the same time.
    fn insert(
        side: &mut VecDeque<(DateTime<Utc>, f64)>,
        time: DateTime<Utc>,
        price: f64,
        dominates: fn(f64, f64) -> bool,
    ) {
        let later = side.partition_point(|(held, _)| *held < time);
        if side
            .get(later)
            .is_some_and(|&(_, held)| dominates(held, price))
        {
            return;
        }
        let end = side.partition_point(|(held, _)| *held <= time);
        let mut start = later;
        while start > 0 && dominates(price, side[start - 1].1) {
            start -= 1;
        }
        side.drain(start..end);
        side.insert(start, (time, price));
    }
}

#[derive(Clone, Default)]
struct PendingParts {
    sizes: OrderSizes,
    parts: Vec<(Tick, DateTime<Utc>, f64, bool)>,
    rows: Vec<Option<([f64; 3], u64)>>,
}

#[derive(Clone)]
struct State {
    source: Source,
    trade_bars: super::inbox::TradeBars,
    /// These rows have no live allocation anchor. The first sink allocation replaces them.
    paged_rate: bool,
    /// Open of the newest bar a rebuild left out because the store had lost executions of it:
    /// later executions of that bar or an older one add to no bar.
    cut: Option<DateTime<Utc>>,
    bars: BTreeMap<DateTime<Utc>, ([f64; 3], u64)>,
    last: Option<DateTime<Utc>>,
    inside: Option<(f64, f64)>,
    window: Window,
    instant: Window,
    /// Candidate highs and lows of the rolling execution range.
    extremes: Extremes,
    /// Distinct keys of the bar, rolling and instant windows, or of the buy and sell prices.
    distinct: [Distinct; 3],
    /// Price of the previous buy and sell execution.
    last_price: [Option<f64>; 2],
    revision: u64,
    reset_revision: u64,
    /// The revision of the latest bar that dropped the oldest bars, which restarts the readers
    /// like `reset_revision` while the values of the bars kept still hold.
    dropped_revision: u64,
    changes: BTreeMap<DateTime<Utc>, u64>,
    filter: Option<OrderFilter>,
    /// What the size ranges of SizeDelta and FilteredCvd measure.
    basis: SizeBasis,
    /// Allocated parts and their previous rows for orders of the newest timestamp.
    pending: PendingParts,
    /// A footprint source's executions, from its first execution with a tick size.
    footprint: Option<FootprintState>,
    /// A book source's book at the end of each bar.
    books: BTreeMap<DateTime<Utc>, BarBook>,
    /// Levels held in `books`.
    book_rows: usize,
    tick_size: Option<f64>,
    replay: Option<Replay>,
    /// The bar of the last update placed by time, kept while later ones share its date.
    placement: Placement,
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
/// drawing their earlier values until then, and otherwise updates them from the first changed
/// bar. At most 100,000 events per rolling window are retained; an overflowing window gives no
/// value until the missing events have expired, rather than understating the rate.
///
/// Besides the fixed aggregations of [`OrderFlowKind`], a source can hold each bar's executions
/// by price ([`footprint`](Self::footprint)) or its last synchronized book
/// ([`book`](Self::book)) for a host's study. These keep at most 11,000 bars and 110,000 price
/// rows, footprint levels or book levels; past either, the oldest bars go at once, down to 10,000
/// bars and 100,000 rows, and the chart recalculates their studies in full.
/// Immutable prepared footprint pages are shared with the chart and retained independently
/// of these raw-source limits.
///
/// During a chart replay every reader stops at the cutoff (see [`OrderFlowBars`]).
#[derive(Clone)]
pub struct OrderFlowInput(Arc<Mutex<State>>);

impl std::fmt::Debug for OrderFlowInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OrderFlowInput").finish_non_exhaustive()
    }
}

impl OrderFlowInput {
    /// An empty source for this fixed aggregation.
    pub fn new(kind: OrderFlowKind) -> Self {
        Self::of(Source::Fixed(kind.normalized()))
    }

    /// An empty source of each bar's executions by price, one level per instrument tick, as
    /// [`Footprint`] aggregates them, for a study to read with
    /// [`footprint_snapshot`](Self::footprint_snapshot), or copy with
    /// [`footprints_from`](Self::footprints_from). It keeps nothing until it has a tick size
    /// ([`set_tick_size`](Self::set_tick_size); the chart gives it its instrument's). Tick,
    /// volume, range and delta intervals share the chart's execution allocations. An execution
    /// older than the one before it is left out, as from the chart's footprint.
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

    /// An independent input holding this moment's data, configuration and replay cutoff.
    /// Completed footprint owners are shared; mutable state is copied under the source lock.
    /// Unlike [`Clone`], updates or clears through either input cannot change the other.
    ///
    /// This creates no feed subscription. For a frozen export, keep the returned input and
    /// its clones disconnected from live updates.
    pub fn frozen(&self) -> Self {
        Self(Arc::new(Mutex::new(self.state().clone())))
    }

    /// The same source, with book size and book balance summed over the individual orders `filter` accepts. The
    /// feed must send order events; with aggregate depth only, or when the filter needs a
    /// participant or placement time that no held order has, a book size or book balance bar has no value (NaN),
    /// instead of an unfiltered one. Other aggregations ignore it. A filter that accepts every
    /// order is the same as none.
    pub fn with_order_filter(self, filter: OrderFilter) -> Self {
        {
            let mut state = self.state();
            let filter = (filter != OrderFilter::default()).then_some(filter);
            if state.filter != filter {
                state.filter = filter;
                state.checkpoint_state_changed();
            }
        }
        self
    }

    /// The same source, with the size ranges of [`SizeDelta`](OrderFlowKind::SizeDelta) and
    /// [`FilteredCvd`](OrderFlowKind::FilteredCvd) measuring each execution's own size or the
    /// combined size of its aggressive order, as [`SizeBasis`] describes. Other aggregations ignore
    /// it. It applies to the executions pushed after it. Under
    /// [`SizeBasis::AggressiveOrder`] the values of the bars of the newest timestamp can change
    /// while fills of its orders arrive. Changing the basis finalizes those pending decisions.
    pub fn with_size_basis(self, basis: SizeBasis) -> Self {
        {
            let mut state = self.state();
            if state.basis != basis {
                state.pending = PendingParts::default();
                state.basis = basis;
                state.checkpoint_state_changed();
            }
        }
        self
    }

    /// Feed services read by this source. Filtered book size and book balance need individual
    /// orders; executions need trade-side classification only for side-dependent outputs.
    pub fn services(&self) -> &'static [crate::model::FeedService] {
        use crate::model::FeedService::*;
        let state = self.state();
        match state.source {
            Source::Fixed(OrderFlowKind::Spread | OrderFlowKind::LevelsTradedThrough { .. }) => {
                &[BestBidAsk]
            }
            Source::Fixed(OrderFlowKind::BookSize { .. } | OrderFlowKind::BookBalance { .. })
                if state.filter.is_some() =>
            {
                &[OrderDepth]
            }
            Source::Fixed(OrderFlowKind::BookSize { .. } | OrderFlowKind::BookBalance { .. })
            | Source::Book(_) => &[Depth],
            Source::Footprint
            | Source::Fixed(
                OrderFlowKind::TradeRate { .. }
                | OrderFlowKind::AggressorRate { .. }
                | OrderFlowKind::PriceRange { .. },
            ) => &[LiveTrades],
            Source::Fixed(
                OrderFlowKind::LevelsExecuted { .. }
                | OrderFlowKind::SizeDelta { .. }
                | OrderFlowKind::FlowSum { .. }
                | OrderFlowKind::FilteredCvd { .. },
            ) => &[LiveTrades, TradeSides],
        }
    }

    /// The filter of book size or book balance, if it has one.
    pub(crate) fn order_filter(&self) -> Option<OrderFilter> {
        let state = self.state();
        state.filter.clone().filter(|_| {
            matches!(
                state.source,
                Source::Fixed(OrderFlowKind::BookSize { .. } | OrderFlowKind::BookBalance { .. })
            )
        })
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    #[cfg(any(feature = "widget", feature = "alert-engine"))]
    pub(crate) fn same_source(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }

    #[cfg(feature = "widget")]
    pub(crate) fn is_footprint(&self) -> bool {
        matches!(self.state().source, Source::Footprint)
    }

    /// Clears values on a new symbol, interval or subscription. The aggregation, order filter,
    /// size basis, tick size and any replay cutoff stay.
    pub fn clear(&self) {
        self.state().clear();
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
        state.checkpoint_state_changed();
        if state.footprint.take().is_some() {
            state.changes.clear();
            state.revision = state.revision.wrapping_add(1);
            state.reset_revision = state.revision;
        }
    }

    #[cfg(feature = "studies")]
    pub(crate) fn reset_revision(&self) -> u64 {
        self.state().restart_revision()
    }

    #[cfg(feature = "widget")]
    pub(crate) fn revision(&self) -> u64 {
        self.state().revision
    }

    #[cfg(feature = "widget")]
    pub(crate) fn changed_since(&self, revision: u64) -> Option<DateTime<Utc>> {
        let state = self.state();
        if state.restart_revision() > revision {
            return None;
        }
        state
            .changes
            .iter()
            .find(|(_, changed)| **changed > revision)
            .map(|(time, _)| *time)
    }

    /// Whether the source only dropped its oldest bars since `revision`: the values of the bars
    /// it kept still hold until the studies on it are recalculated.
    #[cfg(feature = "widget")]
    pub(crate) fn only_dropped_since(&self, revision: u64) -> bool {
        let state = self.state();
        state.dropped_revision > revision && state.reset_revision <= revision
    }

    /// Updates the chart's execution cells from the sink's exact trade-bar allocations.
    #[cfg(feature = "widget")]
    pub(crate) fn copy_footprint(&self, target: &mut Footprint, since: u64) -> u64 {
        let state = self.state();
        if state.revision != since
            && let Some(flow) = &state.footprint
        {
            if since > state.revision || state.restart_revision() > since {
                target.copy_trade_bars(flow.view(), None);
            } else if let Some((&open, _)) = state.changes.iter().find(|(_, rev)| **rev > since) {
                target.copy_trade_bars(flow.view(), Some(open));
            }
        }
        state.revision
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
        let complete_from = state
            .footprint
            .as_ref()
            .and_then(|flow| flow.view().complete_from());
        let mut bars = BTreeMap::new();
        if let (Some(flow), Some(first)) = (&state.footprint, complete_from) {
            let from = open.max(first);
            bars.extend(
                flow.view()
                    .bars(from..)
                    .take_while(|bar| {
                        state
                            .replay
                            .as_ref()
                            .is_none_or(|replay| bar.time < replay.open)
                    })
                    .map(|bar| (bar.time, FootprintBar::copy_from(bar))),
            );
            if let Some(replay) = &state.replay
                && replay.open >= from
                && let Some(bar) = &replay.footprint
            {
                bars.insert(replay.open, bar.clone());
            }
        }
        state.snapshot(bars, complete_from)
    }

    /// Captures immutable footprint history from `open` on. Completed history shares its
    /// owners; only currently mutable rows are copied under the lock. Reads afterwards never
    /// lock the input. Unlike [`footprints_from`](Self::footprints_from), this also exposes
    /// complete prepared bars before a missing join; use [`OrderFlowFootprints::span_is_complete`]
    /// before treating a profile spanning them as complete.
    ///
    /// ```
    /// use egui_charts::{data::OrderFlowInput, model::{BarClock, Tick, Timeframe}};
    /// let input = OrderFlowInput::footprint();
    /// input.set_tick_size(Some(0.25));
    /// let clock = BarClock::new(Timeframe::Min1);
    /// let time = "2026-01-05T10:00:00Z".parse()?;
    /// input.push_trade(&clock, &Tick::new(time, 100.0, 2.0, None));
    /// let saved = input.footprint_snapshot(time);
    /// input.push_trade(&clock, &Tick::new(time, 100.0, 3.0, None));
    /// assert_eq!(saved.bars(..).next().unwrap().volume(), 2.0);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn footprint_snapshot(&self, open: DateTime<Utc>) -> OrderFlowFootprints {
        self.state().footprint_snapshot(open)
    }

    /// Reject another calendar before capturing rows, under the same lock as the capture.
    #[cfg(feature = "studies")]
    pub(crate) fn footprint_snapshot_for_clock(
        &self,
        open: DateTime<Utc>,
        clock: &BarClock,
    ) -> Option<OrderFlowFootprints> {
        let state = self.state();
        (state.footprint.as_ref()?.view().clock() == clock).then(|| state.footprint_snapshot(open))
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
            return state.push_footprint(clock, trade, None);
        }
        if let Some(timeframe) = clock.timeframe().filter(|tf| tf.is_trade_based()) {
            state.discard_paged_rate();
            let tick_size = state.tick_size;
            let Ok(parts) = state.trade_bars.push(timeframe, trade, tick_size) else {
                return;
            };
            for (i, (bar, size)) in parts.into_iter().enumerate() {
                state.add_trade(trade, bar.time, size, i == 0);
            }
        } else {
            let open = state.placement.open(clock, trade.ts);
            state.add_trade(trade, open, trade.size, true);
        }
    }

    /// The sink has already validated and allocated this execution to its chart bars.
    pub(crate) fn push_trade_part(
        &self,
        clock: &BarClock,
        trade: &Tick,
        open: DateTime<Utc>,
        size: f64,
        first: bool,
    ) {
        let mut state = self.state();
        state.discard_paged_rate();
        if matches!(state.source, Source::Footprint) {
            state.push_footprint(clock, trade, Some((open, size)));
            if state.trade_bars.forming.replace(open) != Some(open) {
                state.checkpoint_state_changed();
            }
        } else {
            state.add_trade(trade, open, size, first);
            state.trade_bars.forming = Some(open);
        }
    }

    pub(crate) fn set_trade_bar(&self, open: DateTime<Utc>) {
        let mut state = self.state();
        state.discard_paged_rate();
        if state.trade_bars.forming.replace(open) != Some(open) {
            state.checkpoint_state_changed();
        }
    }

    /// Leaves out queued executions that cannot be allocated again, as after a store loss.
    /// Summing sources omit bars through `open` and rolling values while their windows reach
    /// `until`, the newest execution lost or left out. Footprints mark those bars incomplete.
    #[cfg(any(feature = "widget", feature = "alert-engine"))]
    pub(crate) fn leave_out_queued(&self, open: DateTime<Utc>, until: DateTime<Utc>) {
        let mut state = self.state();
        state.discard_paged_rate();
        match state.source {
            Source::Footprint => {
                if let Some(flow) = &mut state.footprint {
                    flow.live.received_from(open);
                }
            }
            Source::Fixed(
                OrderFlowKind::TradeRate { .. }
                | OrderFlowKind::AggressorRate { .. }
                | OrderFlowKind::LevelsExecuted { .. }
                | OrderFlowKind::SizeDelta { .. }
                | OrderFlowKind::FlowSum { .. }
                | OrderFlowKind::FilteredCvd { .. }
                | OrderFlowKind::PriceRange { .. },
            ) => {
                state.bars.retain(|bar, _| *bar > open);
                state.pending = PendingParts::default();
                state.changes.retain(|bar, _| *bar > open);
                state.cut = state.cut.max(Some(open));
                let until = Some(until);
                state.window.lost_until = state.window.lost_until.max(until);
                state.instant.lost_until = state.instant.lost_until.max(until);
                state.extremes.lost_until = state.extremes.lost_until.max(until);
                for distinct in &mut state.distinct {
                    distinct.lost_until = distinct.lost_until.max(until);
                }
            }
            _ => return,
        }
        // Bars a reader already took may be gone: it starts over.
        state.revision = state.revision.wrapping_add(1);
        state.reset_revision = state.revision;
    }

    /// Forgets the forming bar, so that [`replace_trades`](Self::replace_trades) rebuilds a source
    /// the sink allocated to before. Sources it does not rebuild keep their values.
    #[cfg(feature = "widget")]
    pub(crate) fn forget_trade_bar(&self) {
        let mut state = self.state();
        if state.trade_bars.forming.take().is_some() {
            state.checkpoint_state_changed();
        }
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
            OrderFlowKind::PriceRange { seconds } => state.extremes.push(now, None, seconds),
            _ => return,
        };
        state.bars.insert(open, (row, 0));
        state.finish(now, open);
    }

    /// Require the complete tape to reproduce each candle with the sink's allocator. A short
    /// final bar is valid. The historical grouping is not a live allocation anchor.
    #[cfg(feature = "widget")]
    pub(crate) fn rebuild_paged_trade_rate(
        &self,
        timeframe: crate::model::Timeframe,
        tick_size: Option<f64>,
        trades: &super::trades::Trades,
        history: &[crate::model::Bar],
    ) {
        let mut state = self.state();
        if !matches!(state.source, Source::Fixed(OrderFlowKind::TradeRate { .. }))
            || state.trade_bars.forming.is_some()
        {
            return;
        }
        state.discard_paged_rate();
        if history.is_empty() {
            return;
        }
        let mut grouped = super::inbox::TradeBars::default();
        let mut tape = trades.iter().peekable();
        let mut candles = history.iter();
        let matched = 'tape: {
            while let Some(trade) = tape.next() {
                let Ok(parts) = grouped.push(timeframe, trade, tick_size) else {
                    break 'tape false;
                };
                let n = parts.len();
                let check_last = grouped.closed || tape.peek().is_none();
                for (i, (bar, size)) in parts.into_iter().enumerate() {
                    // Every non-final part closes a candle. The last may still be forming.
                    if (i + 1 < n || check_last)
                        && !candles
                            .next()
                            .is_some_and(|expected| super::merge::same_values(&bar, expected))
                    {
                        break 'tape false;
                    }
                    state.add_trade(trade, bar.time, size, i == 0);
                }
            }
            candles.next().is_none()
        };
        if matched {
            state.paged_rate = true;
        } else {
            // No reader can see a partially matched run: the lock covers the entire rebuild.
            state.clear();
        }
    }

    /// Rebuilds the source from the executions the chart holds, in place of what it holds. A
    /// bar that lost an execution to the store's limit is left out, with the executions of it
    /// that arrive later, and a rolling window gives no value while it reaches back to the loss,
    /// or to a held execution the rebuild leaves out, as when the window overflows. On a
    /// trade-based interval it rebuilds only a source the sink has not allocated an execution to
    /// yet, from the executions that arrived live, grouped along `history`, the chart's bars, from
    /// the oldest bar whose executions are all held, and when they rebuild no bar after a loss,
    /// the newest bar is left out as well. A page's copy of a live execution the store dropped as
    /// a repeat counts in its place, except at the first instant where it dropped one, where the
    /// dropped ones count themselves; neither counts once the store has lost an execution at or
    /// after the open of the chart bar that holds the oldest live execution. After a loss, a
    /// footprint source whose held executions do not rebuild the chart's bars holds none of them
    /// and takes the sink's allocations, of the executions still queued too. Without a loss, a
    /// source whose executions do not match the chart's bars groups them from the oldest live one.
    #[cfg(feature = "widget")]
    pub(crate) fn replace_trades(
        &self,
        clock: &BarClock,
        trades: &super::trades::Trades,
        history: &[crate::model::Bar],
        prepared: Option<&FootprintState>,
    ) {
        let footprint = match self.state().source {
            Source::Fixed(
                OrderFlowKind::TradeRate { .. }
                | OrderFlowKind::AggressorRate { .. }
                | OrderFlowKind::LevelsExecuted { .. }
                | OrderFlowKind::SizeDelta { .. }
                | OrderFlowKind::FlowSum { .. }
                | OrderFlowKind::FilteredCvd { .. }
                | OrderFlowKind::PriceRange { .. },
            ) => false,
            Source::Footprint => true,
            _ => return,
        };
        let trade_based = clock.timeframe().is_some_and(|tf| tf.is_trade_based());
        if footprint
            && !trade_based
            && let Some(mut flow) = prepared.and_then(FootprintState::prepared_snapshot)
        {
            flow.live.rebuild_prepared(trades);
            flow.seal();
            if flow.over_budget(MAX_BARS, MAX_ROWS) {
                flow.retain(KEPT_BARS, KEPT_ROWS);
            }
            let mut state = self.state();
            let last = state.last;
            state.clear();
            state.last = last;
            state.footprint = Some(flow);
            return;
        }
        // Only the sink knows how the chart split its executions into trade-based bars.
        if trade_based && self.state().trade_bars.forming.is_some() {
            return;
        }
        let as_of = self.state().last;
        self.clear();
        let lost = trades.lost_until();
        let tick_size = self.state().tick_size;
        let chart_run = || Self::chart_run(clock, tick_size, trades, history, None, None);
        if footprint {
            if let Some((parts, bars)) = chart_run() {
                let mut state = self.state();
                for (trade, open, size) in parts {
                    state.push_footprint(clock, trade, Some((open, size)));
                }
                // The executions still queued in the sink continue the chart's grouping.
                if let Some(flow) = &mut state.footprint {
                    flow.live.trade_bars = bars.clone();
                }
                state.trade_bars = bars;
                state.checkpoint_state_changed();
            } else if trade_based && lost.is_some() {
                // After a loss the oldest live execution held need not open a chart bar: without
                // the original live anchor, take the sink's allocations, of queued executions too.
                return;
            } else {
                // In arrival order, as the chart's footprint is built.
                for trade in trades.iter().skip(if trade_based {
                    trades.iter().count() - trades.live().count()
                } else {
                    0
                }) {
                    self.push_trade(clock, trade);
                }
            }
            let mut state = self.state();
            if let (Some(lost), Some(flow)) = (lost, &mut state.footprint) {
                flow.drop_through(lost, trades.iter());
            }
        } else {
            let run = chart_run();
            // After a loss on a trade-based interval the run counts only the live executions after
            // it, from the oldest chart bar whose executions are all held, and none without a run:
            // a rolling window gives no value while it reaches a held execution the run leaves
            // out, a page's included, as while it reaches the loss.
            let left_out = lost.filter(|_| trade_based).and_then(|_| {
                let counted: std::collections::HashSet<*const Tick> =
                    (run.iter().flat_map(|(parts, _)| parts))
                        .map(|(trade, ..)| std::ptr::from_ref(*trade))
                        .collect();
                (trades.iter())
                    .filter(|trade| !counted.contains(&std::ptr::from_ref(*trade)))
                    .map(|trade| trade.ts)
                    .max()
            });
            {
                let mut state = self.state();
                let lost = lost.max(left_out);
                state.checkpoint_state_changed();
                state.window.lost_until = lost;
                state.instant.lost_until = lost;
                state.extremes.lost_until = lost;
                for distinct in &mut state.distinct {
                    distinct.lost_until = lost;
                }
            }
            let rebuilt = run.is_some();
            if let Some((parts, bars)) = run {
                let mut state = self.state();
                let mut previous: Option<&Tick> = None;
                for (trade, open, size) in parts {
                    let first = previous.is_none_or(|previous| !std::ptr::eq(previous, trade));
                    state.add_trade(trade, open, size, first);
                    previous = Some(trade);
                }
                if previous.is_none() {
                    // The grouping changes even when the rebuild had no rows to `finish`.
                    state.checkpoint_state_changed();
                }
                state.trade_bars = bars;
            } else if !trade_based || lost.is_none() {
                // Paged executions belong to history bars, which the chart did not fold from them.
                let mut sorted: Vec<_> = if trade_based {
                    trades.live().collect()
                } else {
                    trades.iter().collect()
                };
                sorted.sort_by_key(|trade| trade.ts);
                for trade in sorted {
                    self.push_trade(clock, trade);
                }
            }
            if let Some(lost) = lost {
                let mut cut = clock.instant_open(lost);
                // Without a run the held executions of the chart's newest bar, which the sink adds
                // to next, never reached the source either.
                if trade_based
                    && !rebuilt
                    && let Some(bar) = history.last()
                {
                    cut = cut.max(bar.time);
                }
                let mut state = self.state();
                state.bars.retain(|open, _| *open > cut);
                state.pending = PendingParts::default();
                state.changes.retain(|open, _| *open > cut);
                state.cut = Some(cut);
                state.checkpoint_state_changed();
            }
        }
        if let Some(now) = as_of {
            self.advance_time(clock, now);
        }
    }

    #[cfg(feature = "widget")]
    pub(crate) fn refresh_footprint_coverage(
        &self,
        trades: &super::trades::Trades,
        lost: Option<DateTime<Utc>>,
        retained: Option<DateTime<Utc>>,
    ) {
        let mut state = self.state();
        let last = state.last;
        if let Some(flow) = &mut state.footprint {
            let from = lost.and_then(|lost| {
                flow.live
                    .prepared_before()
                    .map(|before| last.unwrap_or(before).min(lost))
            });
            if let Some(changed) = flow.live.refresh_prepared_coverage(trades, from, retained) {
                let open = flow.view().clock().instant_open(changed);
                state.revision = state.revision.wrapping_add(1);
                let revision = state.revision;
                state.changes.insert(open, revision);
            }
        }
    }

    /// Records that a footprint source lacks no execution in `from..end`, as a trade page proved,
    /// and has the studies on it update from the bar holding `from`.
    #[cfg(feature = "widget")]
    pub(crate) fn cover_footprint(&self, from: DateTime<Utc>, end: DateTime<Utc>) {
        let mut state = self.state();
        if let Some(flow) = &mut state.footprint {
            let revision = flow.live.revision();
            flow.live.cover_missing(from, end);
            if flow.live.revision() != revision {
                let open = flow.view().clock().instant_open(from);
                state.revision = state.revision.wrapping_add(1);
                let revision = state.revision;
                state.changes.insert(open, revision);
            }
        }
    }

    /// Publish only the admitted prepared span; received live rows outlive the raw tape.
    #[cfg(feature = "widget")]
    pub(crate) fn apply_prepared_span(
        &self,
        clock: &BarClock,
        trades: &super::trades::Trades,
        history: &[crate::model::Bar],
        prepared: &FootprintState,
        span: std::ops::Range<DateTime<Utc>>,
        available: bool,
    ) -> Result<(), super::FeedError> {
        let mut state = self.state();
        if !matches!(state.source, Source::Footprint) {
            return Ok(());
        }
        if state
            .footprint
            .as_ref()
            .and_then(|flow| flow.live.prepared_before())
            .is_none()
        {
            drop(state);
            self.replace_trades(clock, trades, history, Some(prepared));
            return Ok(());
        }
        let flow = state.footprint.as_mut().unwrap();
        if available {
            flow.insert_prepared(
                prepared.snapshot(span.start, Some(span.end)),
                span.start,
                span.end,
            )?;
        }
        flow.record_prepared_span(span.start, span.end, available);
        state.revision = state.revision.wrapping_add(1);
        let revision = state.revision;
        state.changes.insert(clock.bar_open(span.start), revision);
        Ok(())
    }

    /// Groups the executions the sink allocated along `history`, the chart's bars, on a
    /// trade-based interval: the live ones, after a loss only those after it. A page's copy
    /// stands in for a live execution the store dropped as a repeat, so paged ones from the
    /// open of the chart bar holding the oldest live one, held or kept apart as below, join
    /// them, or from `anchor`, the open of an older bar known to hold the sink's allocations,
    /// unless the store lost an execution at or after that open: it lost none after
    /// `lost_until`, so the bars from a later open hold all of theirs. A bar the sink opened a
    /// nanosecond after the one before starts with an execution no later than that one's open,
    /// so a run of such bars counts from the open of its first. Older paged ones belong to
    /// history bars, which the chart did not fold from them. At the first instant, as the page
    /// stamps it, where the store left a live execution to a page, the page may hold executions the
    /// stream never sent, so the live ones the store kept apart count in place of the page's
    /// executions of that instant. They are live executions too:
    /// when the chart holds a bar at or before the oldest of them, the run starts at that bar,
    /// unless the executions from there fail to rebuild the chart's bars or, from a bar that
    /// opens at the oldest of them, leave part of their volume to an older bar. A paged execution
    /// the sink never allocated, such as one the stream sent late, does either, and the run then
    /// starts as it would without them. `before` is the open of a bar the chart dropped when the
    /// sink opened the chart's oldest bar a nanosecond after it: the run then opens the oldest bar
    /// after it, and the copies count from `anchor` though no chart bar is at or before it. Returns
    /// the parts and the grouping after them, or `None` unless they rebuild every bar through the
    /// newest.
    #[cfg(feature = "widget")]
    fn chart_run<'a>(
        clock: &BarClock,
        tick_size: Option<f64>,
        trades: &'a super::trades::Trades,
        history: &[crate::model::Bar],
        anchor: Option<DateTime<Utc>>,
        before: Option<DateTime<Utc>>,
    ) -> Option<ChartRun<'a>> {
        let timeframe = clock.timeframe().filter(|tf| tf.is_trade_based())?;
        let lost = trades.lost_until();
        let paged = trades.iter().count() - trades.live().count();
        let (unheld_at, unheld) = trades.unheld_live().unzip();
        let streamed = (trades.live().map(|trade| trade.ts)).chain(anchor).min();
        // The live executions the store kept apart are the stream's too, so the oldest of them
        // opens a bar the sink allocated, unless the chart starts after it.
        let kept_from = (unheld.and_then(|kept| kept.first()))
            .map(|trade| trade.ts)
            .filter(|at| history.first().is_some_and(|bar| bar.time <= *at))
            .filter(|at| streamed.is_none_or(|oldest| *at < oldest));
        let run = |oldest: Option<DateTime<Utc>>| {
            let from = oldest.and_then(|oldest| {
                let bars = &history[..history.partition_point(|bar| bar.time <= oldest)];
                let chained = (bars.windows(2).rev())
                    .take_while(|pair| pair[1].time - pair[0].time == TimeDelta::nanoseconds(1))
                    .count();
                (bars[..bars.len() - chained].last().map(|bar| bar.time))
                    .or(anchor.filter(|_| before.is_some()))
                    .filter(|from| lost.is_none_or(|lost| lost < *from))
            });
            let live = trades
                .live()
                .filter(|trade| lost.is_none_or(|lost| trade.ts > lost));
            let mut held: Vec<_> = (trades.iter().take(paged))
                .filter(|trade| Some(trade.ts) != unheld_at)
                .chain(unheld.into_iter().flatten())
                .filter(|trade| from.is_some_and(|from| trade.ts >= from))
                .chain(live)
                .collect();
            held.sort_by_key(|trade| trade.ts);
            let grouped = super::footprint::live_run(timeframe, tick_size, &held, history, before)?;
            // Whether the run holds all of the held volume, none of it left to older bars.
            let parted: f64 = grouped.0.iter().map(|part| part.2).sum();
            let total: f64 = held.iter().map(|trade| trade.size).sum();
            Some((grouped, from, total - parted <= 1e-9 * total))
        };
        // From a bar that opens at the oldest kept execution, every held one is an execution the
        // sink allocated there or later, so none of them belongs to an older bar.
        (kept_from.and_then(|kept| run(Some(kept))))
            .filter(|(_, from, whole)| *whole || *from != kept_from)
            .or_else(|| run(streamed))
            .map(|(grouped, ..)| grouped)
    }

    /// Applies a chart replay cutoff, exclusive, or removes it with `None`. Readers then stop at
    /// the cutoff's bar. That bar holds the held executions before the cutoff for a source that
    /// sums them, unless the store dropped one of that bar's, and a book source shows the bar's
    /// book when the bar's last update came before the cutoff. Every other source leaves that bar
    /// empty: quotes and books are not held, and a rolling window could reach executions before
    /// the oldest held one. On a trade-based interval the cutoff's bar is the one of `history`,
    /// all the chart's bars and not only those before the cutoff, that holds it, unless the
    /// source grouped its own bar holding the cutoff apart from them, and a page's copy counts for
    /// a live execution the store dropped as a repeat. Once the store has lost an execution to its
    /// limit, that bar is filled there only when the executions held after the loss group into
    /// `history` from that bar or an older one.
    #[cfg(feature = "widget")]
    pub(crate) fn set_replay(
        &self,
        clock: &BarClock,
        cutoff: Option<DateTime<Utc>>,
        trades: Option<&super::trades::Trades>,
        history: &[crate::model::Bar],
    ) {
        let mut state = self.state();
        let replay = cutoff.map(|cutoff| {
            let trade_timeframe = clock.timeframe().filter(|tf| tf.is_trade_based());
            // Besides a book source's own book, only a source that sums each bar's executions
            // fills the cutoff's bar: quotes and books are not held, and a rolling window can
            // reach executions before the held ones or that the store lost. Every kind is named,
            // so that a new one has to be placed here.
            let sums = match state.source {
                Source::Footprint
                | Source::Fixed(
                    OrderFlowKind::SizeDelta { .. }
                    | OrderFlowKind::FlowSum { .. }
                    | OrderFlowKind::FilteredCvd { .. },
                ) => true,
                Source::Fixed(
                    OrderFlowKind::Spread
                    | OrderFlowKind::BookSize { .. }
                    | OrderFlowKind::BookBalance { .. }
                    | OrderFlowKind::PriceRange { .. }
                    | OrderFlowKind::TradeRate { .. }
                    | OrderFlowKind::AggressorRate { .. }
                    | OrderFlowKind::LevelsExecuted { .. }
                    | OrderFlowKind::LevelsTradedThrough { .. },
                )
                | Source::Book(_) => false,
            };
            let lost = trades.is_some_and(|trades| trades.lost_until().is_some());
            // shortcut: scan the held executions per input per seek; index them by time if
            // replay playback profiles show it.
            // Without a loss every source takes the grouping, which places the cutoff's bar on the
            // chart's bars also for a source that leaves that bar empty.
            let partial = if let Some(timeframe) = trade_timeframe
                && matches!(state.source, Source::Fixed(_) | Source::Footprint)
                && (sums || !lost)
                && let Some(trades) = trades
            {
                let tick_size = state.tick_size;
                let mut part = State::new(state.source);
                part.tick_size = tick_size;
                part.basis = state.basis;
                // A row a nanosecond before the chart's oldest bar is a bar the chart dropped and
                // the sink opened the oldest one right after it, so a fill at one instant may run
                // from that bar into the oldest one.
                let before = (history.first())
                    .and_then(|first| first.time.checked_sub_signed(TimeDelta::nanoseconds(1)))
                    .filter(|before| {
                        state.bars.contains_key(before)
                            || (state.footprint.as_ref())
                                .is_some_and(|flow| flow.view().bar_at(*before).is_some())
                    });
                // The sink allocated the oldest bar this source holds and every bar after it, so
                // a page's copies count from there, or from the chart's oldest bar once the chart
                // no longer holds that one and does not continue such a fill, or from the oldest
                // live execution's bar when the page lacks one of them.
                let anchor = (state.footprint.as_ref())
                    .and_then(|flow| flow.view().first_time())
                    .or_else(|| state.bars.keys().next().copied())
                    .zip(history.first())
                    .map(|(anchor, first)| {
                        if before.is_some() {
                            anchor
                        } else {
                            first.time.max(anchor)
                        }
                    });
                let run = Self::chart_run(clock, tick_size, trades, history, anchor, before)
                    .or_else(|| Self::chart_run(clock, tick_size, trades, history, None, None));
                let parts = run.map_or_else(
                    || {
                        // After a loss the oldest held execution need not open a chart bar.
                        if lost {
                            return Vec::new();
                        }
                        // Executions that do not match the chart's bars are grouped from the
                        // oldest live one: history pages did not form the chart's live trade bars.
                        let mut bars = super::inbox::TradeBars::default();
                        let mut sorted: Vec<_> = trades.live().collect();
                        sorted.sort_by_key(|trade| trade.ts);
                        let mut parts = Vec::new();
                        for trade in sorted {
                            let Ok(allocated) = bars.push(timeframe, trade, tick_size) else {
                                continue;
                            };
                            for (bar, size) in allocated {
                                parts.push((trade, bar.time, size));
                            }
                            if trade.ts >= cutoff {
                                break;
                            }
                        }
                        parts
                    },
                    |(parts, _)| parts,
                );
                let mut open = None;
                let mut previous: Option<&Tick> = None;
                for (trade, bar, size) in parts {
                    if bar <= cutoff {
                        open = Some(bar);
                    }
                    // The first excluded execution can open a bar exactly at the cutoff.
                    if trade.ts >= cutoff {
                        break;
                    }
                    if matches!(part.source, Source::Footprint) {
                        part.push_footprint(clock, trade, Some((bar, size)));
                    } else {
                        let first = previous.is_none_or(|previous| !std::ptr::eq(previous, trade));
                        part.add_trade(trade, bar, size, first);
                    }
                    previous = Some(trade);
                }
                // After a loss the run can start after the cutoff's bar, which then has no value.
                match open {
                    Some(open) => Some((open, part)),
                    None => (!lost).then_some((cutoff, part)),
                }
            } else {
                None
            };
            let trade_based = trade_timeframe.is_some();
            let open = if let Some((open, _)) = &partial {
                *open
            } else if trade_based {
                // The chart's bar holding the cutoff: the source's own newest bar at or before it
                // is older when the source has no value in that bar, as without a quote or a
                // classified execution. A bar the source grouped apart from the chart's bars
                // can hold executions after the cutoff, so it stays the cutoff's bar.
                let own = state
                    .footprint
                    .as_ref()
                    .and_then(|flow| flow.view().bars(..=cutoff).next_back().map(|bar| bar.time))
                    .or_else(|| {
                        state
                            .bars
                            .range(..=cutoff)
                            .next_back()
                            .map(|(open, _)| *open)
                    });
                let chart = (history[..history.partition_point(|bar| bar.time <= cutoff)].last())
                    .map(|bar| bar.time);
                (own.filter(|own| history.binary_search_by_key(own, |bar| bar.time).is_err()))
                    .or(chart)
                    .unwrap_or(cutoff)
            } else {
                clock.instant_open(cutoff)
            };
            let prepared = state
                .footprint
                .as_ref()
                .and_then(|flow| flow.live.prepared_before());
            // A bar of a trading date can start before its own time, and the saved boundary
            // already holds executions before the prepared cutoff.
            let start =
                (clock.instant_start(cutoff)).max(prepared.unwrap_or(DateTime::<Utc>::MIN_UTC));
            let held = trades.filter(|trades| {
                prepared.is_none_or(|before| cutoff >= before)
                    && (trades.lost_until()).is_none_or(|lost| !trade_based && lost < start)
            });
            let known = held.is_some();
            let held = held
                .into_iter()
                .flat_map(|trades| trades.iter())
                .filter(|trade| (trade_based || start <= trade.ts) && trade.ts < cutoff);
            let mut replay = Replay {
                cutoff,
                open,
                row: None,
                footprint: None,
                book: None,
            };
            match state.source {
                Source::Fixed(kind) => {
                    if let Some((_, part)) = &partial
                        && sums
                    {
                        replay.row = part.bars.get(&open).map(|(row, _)| *row);
                    } else if !trade_based && sums {
                        let part = OrderFlowInput::new(kind).with_size_basis(state.basis);
                        // The store's executions are valid, and those held are the cutoff bar's.
                        held.for_each(|trade| {
                            part.push_trade_part(clock, trade, open, trade.size, true)
                        });
                        replay.row = part.state().bars.get(&open).map(|(row, _)| *row);
                    }
                }
                Source::Footprint => {
                    if let Some((_, part)) = &partial {
                        replay.footprint = part
                            .footprint
                            .as_ref()
                            .and_then(|flow| flow.view().bar_at(open))
                            .map(FootprintBar::copy_from);
                    } else if known
                        && let Some(tick_size) = state.tick_size.filter(|_| time_based(clock))
                    {
                        let seed = state
                            .footprint
                            .as_ref()
                            .and_then(|flow| flow.live.prepared_boundary())
                            .filter(|bar| bar.time == open);
                        let Ok(mut part) = Footprint::from_bars(
                            clock.clone(),
                            tick_size,
                            1,
                            seed.into_iter().cloned(),
                        ) else {
                            return replay;
                        };
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
        let kind = match state.source {
            Source::Fixed(
                kind @ (OrderFlowKind::BookSize { .. } | OrderFlowKind::BookBalance { .. }),
            ) => kind,
            Source::Book(count) => return state.push_bar_book(clock, time, count, levels),
            _ => return,
        };
        if state.last.is_some_and(|t| time < t) {
            return;
        }
        let mut row = [f64::NAN; 3];
        if let Some(levels) = levels {
            let mut sizes = [[0.0; 2]; 2];
            let mut seen = [0; 2];
            let mut inside = [None; 2];
            for (bid, level) in levels {
                if !level.price.is_finite() || !level.size.is_finite() || level.size < 0.0 {
                    return;
                }
                let side = usize::from(!bid);
                let first = *inside[side].get_or_insert(level.price);
                let tolerance = 4.0 * f64::EPSILON * level.price.abs().max(first.abs());
                let within = |count: usize, distance: f64| {
                    if distance > 0.0 {
                        (level.price - first).abs() <= distance + tolerance
                    } else {
                        seen[side] < count
                    }
                };
                let band = match kind {
                    OrderFlowKind::BookSize { levels, distance } => {
                        within(levels, distance).then_some(0)
                    }
                    OrderFlowKind::BookBalance {
                        near_levels,
                        far_levels,
                        near_distance,
                        far_distance,
                    } => {
                        let far = if near_distance > 0.0 {
                            near_distance + far_distance
                        } else {
                            0.0
                        };
                        if within(near_levels, near_distance) {
                            Some(0)
                        } else if within(near_levels + far_levels, far) {
                            Some(1)
                        } else {
                            None
                        }
                    }
                    _ => None,
                };
                if let Some(band) = band {
                    sizes[band][side] += level.size;
                }
                seen[side] += 1;
            }
            row = match kind {
                OrderFlowKind::BookBalance { .. } => {
                    [balance(sizes[0]), balance(sizes[1]), f64::NAN]
                }
                _ => [sizes[0][0], sizes[0][1], sizes[0][0] - sizes[0][1]],
            };
        }
        let Some(open) = state.open(clock, time) else {
            return;
        };
        state.bars.insert(open, (row, 0));
        state.finish(time, open);
    }
}

/// `size` signed by the execution's side when `measured`, its own or its order's size,
/// lies in `min..=max`; zero for an unknown side or a size outside the range.
fn signed_size(trade: &Tick, measured: f64, min: f64, max: f64, size: f64) -> f64 {
    if !(min..=max).contains(&measured) {
        return 0.0;
    }
    match trade.side {
        Some(OrderSide::Buy) => size,
        Some(OrderSide::Sell) => -size,
        None => 0.0,
    }
}

/// Percentage resting balance; no value without a finite positive total.
fn balance([bid, ask]: [f64; 2]) -> f64 {
    let total = bid + ask;
    if total > 0.0 && total.is_finite() {
        (bid - ask) / total * 100.0
    } else {
        f64::NAN
    }
}

impl State {
    /// A replay plan observes the source settings and allocation, even when no row changes.
    fn checkpoint_state_changed(&mut self) {
        #[cfg(feature = "widget")]
        if matches!(self.source, Source::Footprint)
            || cfg!(feature = "studies")
                && matches!(self.source, Source::Fixed(OrderFlowKind::TradeRate { .. }))
        {
            self.revision = self.revision.wrapping_add(1);
        }
    }

    /// Neither supported checkpoint owns a book, a time placement or pending order fills.
    /// A rate rebuild may mark all the unused auxiliary windows with the same loss.
    #[cfg(feature = "widget")]
    fn plain_checkpoint_state(&self) -> bool {
        self.trade_bars.is_plain()
            && self.placement.is_empty()
            && self.filter.is_none()
            && self.books.is_empty()
            && self.book_rows == 0
            && self.pending.sizes.checkpoint_empty()
            && self.pending.parts.is_empty()
            && self.pending.rows.is_empty()
            && self.inside.is_none()
            && self.last_price == [None; 2]
            && self.extremes.high.is_empty()
            && self.extremes.low.is_empty()
            && self.extremes.now.is_none()
            && self.distinct.iter().all(|d| {
                d.events.is_empty()
                    && d.counts.is_empty()
                    && d.now.is_none()
                    && d.lost_until == self.extremes.lost_until
            })
    }

    /// The revision from which readers start over: the latest reset or drop.
    #[cfg(any(feature = "studies", feature = "widget"))]
    fn restart_revision(&self) -> u64 {
        self.reset_revision.max(self.dropped_revision)
    }

    fn open(&mut self, clock: &BarClock, time: DateTime<Utc>) -> Option<DateTime<Utc>> {
        if clock.timeframe().is_some_and(|tf| tf.is_trade_based()) {
            self.trade_bars.forming
        } else {
            Some(self.placement.open(clock, time))
        }
    }

    fn add_trade(&mut self, trade: &Tick, open: DateTime<Utc>, size: f64, first: bool) {
        let Source::Fixed(kind) = self.source else {
            return;
        };
        // Which size ranges count a size: three for SizeDelta, the first for FilteredCvd.
        let counted = |size: f64| match kind {
            OrderFlowKind::SizeDelta { ranges } => {
                ranges.map(|[min, max]| (min..=max).contains(&size))
            }
            OrderFlowKind::FilteredCvd { min, max } => [(min..=max).contains(&size), false, false],
            _ => [false; 3],
        };
        if self.basis != SizeBasis::AggressiveOrder
            || !matches!(
                kind,
                OrderFlowKind::SizeDelta { .. } | OrderFlowKind::FilteredCvd { .. }
            )
        {
            return self.add_measured(trade, open, size, first, trade.size);
        }
        if !self.pending.sizes.holds(trade) {
            self.pending = PendingParts::default();
        }
        // Each execution adds to its order once, with its first part.
        let (before, after) = if first {
            self.pending.sizes.add(trade)
        } else {
            let size = self.pending.sizes.of(trade);
            (size, size)
        };
        let from = (trade.aggressor_order_id)
            .filter(|_| counted(before) != counted(after))
            .and_then(|id| {
                (self.pending.parts.iter())
                    .position(|(fill, ..)| fill.aggressor_order_id == Some(id))
            });
        let start = match from {
            Some(k) => {
                // Restore the rows from before the order's first part, newest first.
                for i in (k..self.pending.parts.len()).rev() {
                    let open = self.pending.parts[i].1;
                    match self.pending.rows[i] {
                        Some(row) => self.bars.insert(open, row),
                        None => self.bars.remove(&open),
                    };
                }
                self.pending.rows.truncate(k);
                k
            }
            None => self.pending.parts.len(),
        };
        self.pending.parts.push((trade.clone(), open, size, first));
        for i in start..self.pending.parts.len() {
            let (fill, open, size, first) = self.pending.parts[i].clone();
            self.pending.rows.push(self.bars.get(&open).copied());
            let measured = self.pending.sizes.of(&fill);
            self.add_measured(&fill, open, size, first, measured);
        }
    }

    fn add_measured(
        &mut self,
        trade: &Tick,
        open: DateTime<Utc>,
        size: f64,
        first: bool,
        measured: f64,
    ) {
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
                    *sum += signed_size(trade, measured, min, max, size);
                }
                row
            }
            OrderFlowKind::FlowSum { .. } => {
                let old = self.bars.get(&open).map_or([0.0; 3], |(v, _)| *v);
                [
                    old[0] + size,
                    old[1] + signed_size(trade, trade.size, 0.0, f64::MAX, size),
                    0.0,
                ]
            }
            OrderFlowKind::FilteredCvd { min, max } => {
                let old = self.bars.get(&open).map_or([0.0; 3], |(v, _)| *v);
                let delta = old[0] + signed_size(trade, measured, min, max, size);
                [delta, old[1].max(delta), old[2].min(delta)]
            }
            OrderFlowKind::PriceRange { seconds } => {
                self.extremes
                    .push(trade.ts, first.then_some(trade.price), seconds)
            }
            _ => return,
        };
        let latest = self.last.map_or(trade.ts, |t| t.max(trade.ts));
        if self.cut.is_some_and(|cut| open <= cut) {
            self.last = Some(latest);
            self.checkpoint_state_changed();
            return;
        }
        self.bars.insert(open, (row, 0));
        self.finish(latest, open);
    }

    fn new(source: Source) -> Self {
        Self {
            source,
            trade_bars: super::inbox::TradeBars::default(),
            paged_rate: false,
            cut: None,
            bars: BTreeMap::new(),
            last: None,
            inside: None,
            window: Window::default(),
            instant: Window::default(),
            extremes: Extremes::default(),
            distinct: Default::default(),
            last_price: [None; 2],
            revision: 0,
            reset_revision: 0,
            dropped_revision: 0,
            changes: BTreeMap::new(),
            filter: None,
            basis: SizeBasis::Fill,
            pending: PendingParts::default(),
            footprint: None,
            books: BTreeMap::new(),
            book_rows: 0,
            tick_size: None,
            replay: None,
            placement: Placement::default(),
        }
    }

    fn discard_paged_rate(&mut self) {
        if self.paged_rate {
            self.clear();
        }
    }

    fn clear(&mut self) {
        let revision = self.revision.wrapping_add(1);
        // The cutoff stays, so nothing after it shows; what was known before it is gone.
        let replay = self.replay.take().map(|replay| Replay {
            row: None,
            footprint: None,
            book: None,
            ..replay
        });
        *self = Self {
            filter: self.filter.take(),
            basis: self.basis,
            tick_size: self.tick_size,
            replay,
            revision,
            reset_revision: revision,
            ..Self::new(self.source)
        };
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

    fn footprint_snapshot(&self, open: DateTime<Utc>) -> OrderFlowFootprints {
        let complete_from = self
            .footprint
            .as_ref()
            .and_then(|flow| flow.view().complete_from());
        let mut flow = self
            .footprint
            .as_ref()
            .map(|flow| flow.snapshot(open, self.replay.as_ref().map(|replay| replay.open)));
        if let (Some(flow), Some(replay)) = (&mut flow, &self.replay)
            && replay.open >= open
            && let Some(bar) = &replay.footprint
        {
            flow.insert_partial(bar.clone());
        }
        OrderFlowFootprints {
            covered_from: self
                .footprint
                .as_ref()
                .and_then(|flow| flow.view().covered_from())
                .map(|first| first.max(open)),
            replay_open: self.replay.as_ref().map(|replay| replay.open),
            flow,
            complete_from,
            cutoff: self.replay.as_ref().map(|replay| replay.cutoff),
            tick_size: self.tick_size,
            revision: self.revision,
        }
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

    /// Adds an execution; past the source budget, drop oldest bars down to `KEPT_BARS` and
    /// `KEPT_ROWS` so studies start over once per chunk.
    fn push_footprint(
        &mut self,
        clock: &BarClock,
        trade: &Tick,
        part: Option<(DateTime<Utc>, f64)>,
    ) {
        let Some(tick_size) = self.tick_size else {
            return;
        };
        let new_source = self.footprint.is_none();
        if new_source {
            let Ok(mut flow) = Footprint::new(clock.clone(), tick_size, 1) else {
                return;
            };
            if part.is_some()
                && let Some(open) = self.trade_bars.forming
            {
                flow.received_from(open);
            }
            self.footprint = Some(FootprintState::new(flow));
        }
        let Some(flow) = &mut self.footprint else {
            return;
        };
        #[cfg(feature = "widget")]
        if flow
            .live
            .prepared_before()
            .is_some_and(|before| trade.ts < before)
        {
            return;
        }
        let changed = match part {
            Some((open, _)) => Some(open),
            None if clock.timeframe().is_some_and(|tf| tf.is_trade_based()) => {
                flow.live.bars().last_key_value().map(|(open, _)| *open)
            }
            None => Some(self.placement.open(clock, trade.ts)),
        };
        let result = match part {
            Some((open, size)) => flow.live.push_part(trade, open, size),
            None => flow.live.push(trade),
        };
        if !matches!(result, Ok(true)) {
            // Even without a counted row, creating the source or accepting a zero-sized
            // unallocated fill can change its private grouping for the next execution.
            if new_source || (result.is_ok() && part.is_none()) {
                self.checkpoint_state_changed();
            }
            return;
        }
        flow.seal();
        let dropped = flow.over_budget(MAX_BARS, MAX_ROWS) && flow.retain(KEPT_BARS, KEPT_ROWS);
        let first = flow.first_source_time();
        let latest = self.last.map_or(trade.ts, |last| last.max(trade.ts));
        let open = flow
            .live
            .bars()
            .last_key_value()
            .map_or(trade.ts, |(open, _)| *open);
        if clock.timeframe().is_some_and(|tf| tf.is_trade_based()) {
            self.trade_bars.forming = Some(open);
        }
        self.finish(latest, changed.or(first).unwrap_or(open));
        if dropped {
            // `changes` names only held bars, so it shares their bound; readers start over.
            if let Some(first) = first {
                self.changes.retain(|bar, _| *bar >= first);
            }
            self.dropped_revision = self.revision;
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
        let open = self.placement.open(clock, time);
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
            self.dropped_revision = self.revision;
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
            self.dropped_revision = self.revision;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{OrderBook, OrderBookLevel, Timeframe};

    #[test]
    fn frozen_input_shares_completed_rows_and_detaches_mutable_state() {
        let start: DateTime<Utc> = "2026-01-05T10:00:00Z".parse().unwrap();
        let clock = BarClock::new(Timeframe::Min1);
        let input = OrderFlowInput::footprint();
        input.set_tick_size(Some(0.25));
        for minute in 0..2 {
            input.push_trade(
                &clock,
                &Tick::new(start + TimeDelta::minutes(minute), 100.0, 2.0, None),
            );
        }
        let shared = input.clone();
        let frozen = input.frozen();
        assert!(Arc::ptr_eq(&input.0, &shared.0));
        assert!(!Arc::ptr_eq(&input.0, &frozen.0));
        let original = input.footprint_snapshot(start);
        let saved = frozen.footprint_snapshot(start);
        let row = |read: &OrderFlowFootprints| {
            read.bars(..)
                .next()
                .unwrap()
                .levels
                .values()
                .next()
                .unwrap() as *const _
        };
        assert_eq!(row(&original), row(&saved));
        let volume = |input: &OrderFlowInput| {
            input
                .footprint_snapshot(start)
                .bars(..)
                .map(|bar| bar.volume())
                .sum::<f64>()
        };
        let time = start + TimeDelta::minutes(1);
        input.push_trade(&clock, &Tick::new(time, 100.0, 3.0, None));
        assert_eq!(volume(&shared), 7.0);
        assert_eq!(volume(&frozen), 4.0);
        frozen.push_trade(&clock, &Tick::new(time, 100.0, 5.0, None));
        assert_eq!(volume(&frozen), 9.0);
        assert_eq!(volume(&input), 7.0);
        input.clear();
        assert_eq!(volume(&frozen), 9.0);
    }

    #[test]
    fn snapshot_coverage_keeps_quiet_slots_without_copying_earlier_bars() {
        let start: DateTime<Utc> = "2026-01-05T10:00:00Z".parse().unwrap();
        let at = |minute| start + TimeDelta::minutes(minute);
        let clock = BarClock::new(Timeframe::Min1);
        let input = OrderFlowInput::footprint();
        input.set_tick_size(Some(1.0));
        for minute in [0, 10] {
            input.push_trade(&clock, &Tick::new(at(minute), 100.0, 1.0, None));
        }
        let read = input.footprint_snapshot(at(5));
        assert_eq!(
            read.bars(..).map(|bar| bar.time).collect::<Vec<_>>(),
            [at(10)]
        );
        assert!(read.span_is_complete(at(5), at(10)));
        assert!(
            read.span_is_complete(at(5), at(9)),
            "known empty slots stay known"
        );
        assert!(
            !read.span_is_complete(at(0), at(10)),
            "capture omitted requested bars before 5"
        );
    }

    #[test]
    #[cfg(feature = "widget")]
    fn footprint_snapshot_replay_uses_the_allocated_partial_bar() {
        let start: DateTime<Utc> = "2026-01-05T10:00:00Z".parse().unwrap();
        let at = |second| start + TimeDelta::seconds(second);
        let clock = BarClock::new("2t".parse().unwrap());
        let input = OrderFlowInput::footprint();
        input.set_tick_size(Some(1.0));
        for second in 0..4 {
            input.push_trade(&clock, &Tick::new(at(second), 100.0, 1.0, None));
        }
        input.set_replay(&clock, Some(at(3)), None, &[]);
        let read = input.footprint_snapshot(start);
        assert_eq!(
            read.bars(..).map(|bar| bar.time).collect::<Vec<_>>(),
            [at(0)]
        );
        assert!(
            !read.span_is_complete(at(0), at(2)),
            "the allocated bar at 2 is unknown before cut 3"
        );

        let input = OrderFlowInput::footprint();
        let clock = BarClock::new(Timeframe::Min1);
        input.set_tick_size(Some(1.0));
        let mut trades = super::super::trades::Trades::new(10);
        for second in [0, 20, 60] {
            let trade = Tick::new(at(second), 100.0, 1.0, None);
            input.push_trade(&clock, &trade);
            trades.push(trade);
        }
        input.set_replay(&clock, Some(at(30)), Some(&trades), &[]);
        let read = input.footprint_snapshot(start);
        assert!(
            read.span_is_complete(at(0), at(10)),
            "times inside a known partial slot align to its open"
        );
        assert_eq!(
            read.bars(..).map(|bar| bar.volume()).collect::<Vec<_>>(),
            [2.0]
        );
    }

    #[test]
    fn footprint_snapshots_share_closed_rows_and_freeze_mutable_rows() {
        let start: DateTime<Utc> = "2026-01-05T10:00:00Z".parse().unwrap();
        let at = |bar| start + TimeDelta::minutes(bar);
        let clock = BarClock::new(Timeframe::Min1);
        let input = OrderFlowInput::footprint();
        input.set_tick_size(Some(0.25));
        for bar in 0..2 {
            input.push_trade(&clock, &Tick::new(at(bar), 100.0, 2.0, None));
        }
        let first = input.footprint_snapshot(start);
        let second = input.footprint_snapshot(start);
        let row = |snapshot: &OrderFlowFootprints| {
            snapshot
                .bars(..)
                .next()
                .unwrap()
                .levels
                .values()
                .next()
                .unwrap() as *const _
        };
        assert_eq!(row(&first), row(&second), "finished rows share ownership");
        input.push_trade(&clock, &Tick::new(at(1), 100.0, 3.0, None));
        input.push_trade(&clock, &Tick::new(at(2), 100.0, 7.0, None));
        let latest = input.footprint_snapshot(start);
        assert_eq!(
            row(&first),
            row(&latest),
            "a held read does not copy history on write"
        );
        assert_eq!(
            first.bars(..).map(|bar| bar.volume()).collect::<Vec<_>>(),
            [2.0, 2.0]
        );
        assert_eq!(
            latest.bars(..).map(|bar| bar.volume()).collect::<Vec<_>>(),
            [2.0, 5.0, 7.0]
        );
        assert_eq!(
            input
                .footprint_snapshot(at(1))
                .bars(..)
                .map(|bar| bar.time)
                .collect::<Vec<_>>(),
            [at(1), at(2)]
        );
        assert_eq!(
            latest
                .bars(..)
                .map(|bar| (bar.time, FootprintBar::copy_from(bar)))
                .collect::<BTreeMap<_, _>>(),
            input.footprints_from(start).bars
        );
        input.clear();
        assert_eq!(first.bars(..).map(|bar| bar.volume()).sum::<f64>(), 4.0);
        assert!(input.footprint_snapshot(start).bars(..).next().is_none());
    }

    #[test]
    fn footprint_snapshot_retention_counts_shared_rows() {
        let start: DateTime<Utc> = "2026-01-05T10:00:00Z".parse().unwrap();
        let clock = BarClock::new(Timeframe::Min1);
        let input = OrderFlowInput::footprint();
        input.set_tick_size(Some(1.0));
        for bar in 0..5 {
            for price in 100..103 {
                input.push_trade(
                    &clock,
                    &Tick::new(start + TimeDelta::minutes(bar), f64::from(price), 1.0, None),
                );
            }
        }
        let saved = input.footprint_snapshot(start);
        input.state().footprint.as_mut().unwrap().retain(3, 7);
        let kept = input.footprint_snapshot(start);
        assert_eq!(kept.bars(..).count(), 2);
        assert_eq!(kept.complete_from, Some(start + TimeDelta::minutes(3)));
        assert_eq!(kept.bars(..).map(|bar| bar.levels.len()).sum::<usize>(), 6);
        assert_eq!(
            saved.bars(..).count(),
            5,
            "retention does not mutate an older snapshot"
        );
    }

    #[cfg(feature = "widget")]
    #[test]
    fn prepared_history_keeps_raw_change_records_bounded() {
        let start: DateTime<Utc> = "2026-01-05T10:00:00Z".parse().unwrap();
        let at = |minute: usize| start + TimeDelta::minutes(minute as i64);
        let clock = BarClock::new(Timeframe::Min1);
        let mut history = Footprint::new(clock.clone(), 1.0, 1).unwrap();
        history.push(&Tick::new(start, 100.0, 7.0, None)).unwrap();
        let mut history = FootprintState::new(history);
        let tape = crate::data::trades::Trades::new(100_000);
        history.prepare_history(at(1), &tape, true);
        history.share_prepared();
        let input = OrderFlowInput::footprint();
        input.set_tick_size(Some(1.0));
        input.replace_trades(&clock, &tape, &[], Some(&history));
        let saved = input.footprint_snapshot(start);
        let row = |snapshot: &OrderFlowFootprints| {
            snapshot
                .bars(..)
                .next()
                .unwrap()
                .levels
                .values()
                .next()
                .unwrap() as *const _
        };
        let mut next = 1;
        for end in [MAX_BARS + 1, MAX_BARS + 1 + (MAX_BARS - KEPT_BARS + 1)] {
            for minute in next..=end {
                input.push_trade(&clock, &Tick::new(at(minute), 100.0, 1.0, None));
            }
            let revision = {
                let state = input.state();
                assert_eq!(
                    state
                        .footprint
                        .as_ref()
                        .unwrap()
                        .view()
                        .bars(at(1)..)
                        .count(),
                    KEPT_BARS
                );
                assert_eq!(
                    state.changes.len(),
                    KEPT_BARS,
                    "prepared owners must not retain change records of evicted raw bars"
                );
                assert_eq!(
                    state.changes.first_key_value().unwrap().0,
                    &at(end - KEPT_BARS + 1)
                );
                state.revision
            };
            assert_eq!(
                input.changed_since(revision - 1),
                None,
                "eviction requires recalculation"
            );
            let current = input.footprint_snapshot(start);
            assert_eq!(
                row(&saved),
                row(&current),
                "prepared rows stay shared across eviction"
            );
            assert_eq!(current.bars(..).next().unwrap().volume(), 7.0);
            input.push_trade(&clock, &Tick::new(at(end), 100.0, 1.0, None));
            assert_eq!(input.changed_since(revision), Some(at(end)));
            next = end + 1;
        }
    }

    #[test]
    fn services_name_the_data_each_source_reads() {
        use crate::model::FeedService::*;
        use OrderFlowKind::*;
        let book = BookSize {
            levels: 5,
            distance: 0.0,
        };
        let cases: [(_, &[crate::model::FeedService]); 15] = [
            (OrderFlowInput::new(Spread), &[BestBidAsk]),
            (
                OrderFlowInput::new(PriceRange { seconds: 60 }),
                &[LiveTrades],
            ),
            (
                OrderFlowInput::new(LevelsTradedThrough { seconds: 0 }),
                &[BestBidAsk],
            ),
            (OrderFlowInput::new(book), &[Depth]),
            (
                OrderFlowInput::new(BookBalance {
                    near_levels: 5,
                    far_levels: 10,
                    near_distance: 0.0,
                    far_distance: 0.0,
                })
                .with_order_filter(OrderFilter {
                    min_size: 10.0,
                    ..Default::default()
                }),
                &[OrderDepth],
            ),
            (
                OrderFlowInput::new(book).with_order_filter(OrderFilter {
                    min_size: 10.0,
                    ..Default::default()
                }),
                &[OrderDepth],
            ),
            (
                OrderFlowInput::new(book).with_order_filter(OrderFilter::default()),
                &[Depth],
            ),
            (
                OrderFlowInput::new(TradeRate {
                    volume: false,
                    seconds: 60,
                    instant: 3,
                }),
                &[LiveTrades],
            ),
            (
                OrderFlowInput::new(AggressorRate {
                    seconds: 60,
                    instant: 3,
                }),
                &[LiveTrades],
            ),
            (
                OrderFlowInput::new(LevelsExecuted {
                    seconds: 0,
                    reversal: false,
                }),
                &[LiveTrades, TradeSides],
            ),
            (
                OrderFlowInput::new(SizeDelta {
                    ranges: [[0.0, 10.0]; 3],
                }),
                &[LiveTrades, TradeSides],
            ),
            (
                OrderFlowInput::new(FlowSum { bars: 10 }),
                &[LiveTrades, TradeSides],
            ),
            (
                OrderFlowInput::new(FilteredCvd {
                    min: 0.0,
                    max: 10.0,
                }),
                &[LiveTrades, TradeSides],
            ),
            (OrderFlowInput::footprint(), &[LiveTrades]),
            (
                OrderFlowInput::book(5).with_order_filter(OrderFilter {
                    min_size: 10.0,
                    ..Default::default()
                }),
                &[Depth],
            ),
        ];
        for (input, expected) in cases {
            assert_eq!(input.services(), expected);
        }
    }

    /// The late 99 remains the minimum until the exclusive left edge reaches its time.
    /// Candidate overflow must leave a gap instead of quietly understating the range.
    #[test]
    fn price_range_keeps_late_executions_and_drops_none_silently() {
        let clock = BarClock::new(Timeframe::Min1);
        let start: DateTime<Utc> = "2026-01-05T10:00:00Z".parse().unwrap();
        let input = OrderFlowInput::new(OrderFlowKind::PriceRange { seconds: 10 });
        for (second, price, expected) in [
            (1, 100.0, [0.0, 100.0, 100.0]),
            (3, 102.0, [2.0, 102.0, 100.0]),
            (2, 99.0, [3.0, 102.0, 99.0]),
            (5, 101.0, [3.0, 102.0, 99.0]),
        ] {
            input.push_trade(
                &clock,
                &Tick::new(start + TimeDelta::seconds(second), price, 1.0, None),
            );
            assert_eq!(input.values()[&start], expected, "second {second}");
        }
        input.advance_time(&clock, start + TimeDelta::seconds(12));
        assert_eq!(input.values()[&start], [1.0, 102.0, 101.0]);
        input.advance_time(&clock, start + TimeDelta::seconds(16));
        let empty = input.values()[&start];
        assert_eq!(empty[0], 0.0);
        assert!(empty[1..].iter().all(|v| v.is_nan()));

        let input = OrderFlowInput::new(OrderFlowKind::PriceRange { seconds: 10 });
        for i in 0..=100_000 {
            input.push_trade(
                &clock,
                &Tick::new(
                    start + TimeDelta::nanoseconds(i),
                    100.0 + i as f64,
                    1.0,
                    None,
                ),
            );
        }
        assert!(input.values()[&start].iter().all(|v| v.is_nan()));
        input.advance_time(&clock, start + TimeDelta::seconds(10));
        assert_eq!(input.values()[&start], [99_999.0, 100_100.0, 101.0]);
    }

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

    /// A repeat queued only to complete a drained execution's order ids is no new execution:
    /// a source connected while it waits counts the buy once, as one connected from the start
    /// does, and the drain still gives the held execution its aggressor order id.
    #[cfg(feature = "widget")]
    #[test]
    fn a_source_connected_before_the_drain_skips_a_queued_repeat() {
        let clock = BarClock::new("2t".parse().unwrap());
        let start: DateTime<Utc> = "2026-01-05T10:00:00Z".parse().unwrap();
        let inbox = Arc::new(super::super::inbox::Inbox::default());
        let sink = inbox.subscribe("A".into(), clock.clone(), 100, Some(1.0));
        let first = OrderFlowInput::new(OrderFlowKind::FlowSum { bars: 1 });
        inbox.study_inputs(sink.id, vec![first.clone()]);
        let buy = Tick::new(start, 100.0, 1.0, Some(OrderSide::Buy)).with_id(7);
        sink.push_tick(buy.clone());
        let (_, updates) = inbox.drain().updates.pop().unwrap();
        let bars: Vec<_> = updates.bars.values().map(|(bar, _)| bar.clone()).collect();
        let mut held = super::super::trades::Trades::new(100);
        held.append(updates.trades, &mut |_, _| ());
        sink.push_tick(buy.with_aggressor_order_id(9));
        // The chart adds a study in the frame before its next drain.
        let added = OrderFlowInput::new(OrderFlowKind::FlowSum { bars: 1 });
        added.forget_trade_bar();
        added.replace_trades(&clock, &held, &bars, None);
        inbox.study_inputs(sink.id, vec![first.clone(), added.clone()]);
        let once = BTreeMap::from([(start, [1.0, 1.0, 0.0])]);
        assert_eq!([first.values(), added.values()], [once.clone(), once]);
        let (_, updates) = inbox.drain().updates.pop().unwrap();
        held.append(updates.trades, &mut |_, _| ());
        let held: Vec<_> = held.iter().map(|t| (t.id, t.aggressor_order_id)).collect();
        assert_eq!(held, [(Some(7), Some(9))]);
    }

    /// Quotes and books between fills belong to the current three-tick bar, even when
    /// the feed sends the entire burst before the chart drains it. Paging older executions
    /// must keep that allocation during replay too.
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
        let footprint = OrderFlowInput::footprint();
        footprint.set_tick_size(Some(1.0));
        inbox.study_inputs(
            sink.id,
            vec![spread.clone(), book.clone(), footprint.clone()],
        );
        for second in 0..6 {
            if second == 2 {
                inbox.study_inputs(
                    sink.id,
                    vec![
                        spread.clone(),
                        book.clone(),
                        flow.clone(),
                        footprint.clone(),
                    ],
                );
            }
            let at = start + TimeDelta::seconds(second);
            sink.push_tick(Tick::new(at, 100.0, 1.0, Some(OrderSide::Buy)).with_id(second as u64));
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
        // An older page belongs to history bars: it neither shifts the live bars of a connected
        // source nor those a new source rebuilds from the held executions.
        let mut held = super::super::trades::Trades::new(100);
        updates
            .trades
            .iter()
            .for_each(|trade| held.push(trade.clone()));
        let old = Tick::new(
            start - TimeDelta::seconds(1),
            100.0,
            1.0,
            Some(OrderSide::Buy),
        );
        let boundary = Tick::new(start, 100.0, 1.0, Some(OrderSide::Buy)).with_id(99);
        held.prepend(vec![old, boundary], &mut |_, _| ());
        let rebuilt = OrderFlowInput::new(OrderFlowKind::FlowSum { bars: 1 });
        for input in [&flow, &rebuilt, &footprint] {
            input.replace_trades(&clock, &held, &[], None);
        }
        let live: BTreeMap<_, _> = got[2].iter().copied().collect();
        assert_eq!([flow.values(), rebuilt.values()], [live.clone(), live]);
        let cutoff = start + TimeDelta::seconds(2);
        for input in [&spread, &book, &flow, &footprint] {
            input.set_replay(&clock, Some(cutoff), Some(&held), &[]);
        }
        assert_eq!(flow.values(), BTreeMap::from([(start, [2.0, 2.0, 0.0])]));
        assert_eq!(footprint.footprints_from(start).bars[&start].volume(), 2.0);
        assert!(
            spread.values().is_empty(),
            "past quotes cannot be reconstructed"
        );
        assert!(
            book.values().is_empty(),
            "past depth cannot be reconstructed"
        );
        for input in [&spread, &book, &flow, &footprint] {
            input.set_replay(&clock, Some(opens[1]), Some(&held), &[]);
        }
        assert_eq!(spread.values().keys().copied().collect::<Vec<_>>(), [start]);
        assert_eq!(book.values().keys().copied().collect::<Vec<_>>(), [start]);
        assert_eq!(flow.values(), BTreeMap::from([(start, [3.0, 3.0, 0.0])]));
        assert_eq!(
            footprint
                .footprints_from(start)
                .bars
                .values()
                .map(|bar| (bar.time, bar.volume()))
                .collect::<Vec<_>>(),
            [(start, 3.0)]
        );
        let mut trades = held;
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
        for input in [&spread, &book, &flow, &footprint] {
            input.set_replay(
                &clock,
                Some(start + TimeDelta::seconds(7)),
                Some(&trades),
                &[],
            );
        }
        assert_eq!(spread.values().keys().copied().collect::<Vec<_>>(), opens);
        assert_eq!(book.values().keys().copied().collect::<Vec<_>>(), opens);
        assert_eq!(
            flow.values()[&(start + TimeDelta::seconds(6))],
            [1.0, 1.0, 0.0]
        );
        let open = start + TimeDelta::seconds(6);
        assert_eq!(footprint.footprints_from(open).bars[&open].volume(), 1.0);
        let mut limited = super::super::trades::Trades::new(2);
        for trade in trades.iter() {
            limited.push(trade.clone());
        }
        footprint.set_replay(
            &clock,
            Some(start + TimeDelta::seconds(7)),
            Some(&limited),
            &[],
        );
        assert!(
            footprint.footprints_from(open).bars.is_empty(),
            "a partial bar without its raw anchor must not expose the next fill"
        );
        // After the loss the cutoff's bar is still the chart's bar holding the cutoff, the 6 s
        // one, where neither quote source has a value, so both keep the bars before it.
        let history = [(0, 3.0), (3, 3.0), (6, 2.0)].map(|(second, volume)| {
            let at = start + TimeDelta::seconds(second);
            crate::model::Bar::new(at, 100.0, 100.0, 100.0, 100.0, volume)
        });
        for input in [&spread, &book] {
            let cutoff = start + TimeDelta::seconds(7);
            input.set_replay(&clock, Some(cutoff), Some(&limited), &history);
            assert_eq!(input.values().keys().copied().collect::<Vec<_>>(), opens);
        }
        // Grouped from the buy of 2 s, apart from the chart's bars, a source's bars open at 2 s
        // and 5 s. Its bar of 5 s also holds the buy of 7 s, after a cutoff of 6.5 s, so it is
        // the cutoff's bar, though the chart's bar holding the cutoff opens at 6 s.
        let apart = OrderFlowInput::new(OrderFlowKind::FlowSum { bars: 1 });
        for second in 2..8 {
            let at = start + TimeDelta::seconds(second);
            apart.push_trade(&clock, &Tick::new(at, 100.0, 1.0, Some(OrderSide::Buy)));
        }
        let cutoff = start + TimeDelta::milliseconds(6500);
        apart.set_replay(&clock, Some(cutoff), None, &history);
        let first = start + TimeDelta::seconds(2);
        assert_eq!(apart.values(), BTreeMap::from([(first, [3.0, 3.0, 0.0])]));
        // The first excluded sell would finish the old volume bar and open another. Only
        // the buy before it belongs in replay, not the old bar's completed volume of five.
        let clock = BarClock::new("5v".parse().unwrap());
        let inbox = Arc::new(super::super::inbox::Inbox::default());
        let sink = inbox.subscribe("A".into(), clock.clone(), 100, Some(1.0));
        let footprint = OrderFlowInput::footprint();
        footprint.set_tick_size(Some(1.0));
        let flow = OrderFlowInput::new(OrderFlowKind::FlowSum { bars: 1 });
        inbox.study_inputs(sink.id, vec![footprint.clone(), flow.clone()]);
        let cutoff = start + TimeDelta::seconds(1);
        let mut held = super::super::trades::Trades::new(100);
        for trade in [
            Tick::new(start, 100.0, 2.0, Some(OrderSide::Buy)),
            Tick::new(cutoff, 101.0, 6.0, Some(OrderSide::Sell)),
        ] {
            sink.push_tick(trade.clone());
            held.push(trade);
        }
        for input in [&footprint, &flow] {
            input.set_replay(&clock, Some(cutoff), Some(&held), &[]);
        }
        assert_eq!(
            footprint
                .footprints_from(start)
                .bars
                .values()
                .map(|bar| (bar.time, bar.volume(), bar.delta()))
                .collect::<Vec<_>>(),
            [(start, 2.0, 2.0)]
        );
        assert_eq!(flow.values(), BTreeMap::from([(start, [2.0, 2.0, 0.0])]));
    }

    /// Hand counts on 3t at price 100, every execution of 1 with an id. A trade page answered in
    /// the drain that streams them holds copies it stamps apart from the stream: the sell of
    /// 10:00:00, stamped 400 microseconds later, after a buy at 09:59:59 that the page lacks; or
    /// a buy at 10:00:00 and a sell 400 microseconds later, both stamped 10:00:00. With buys at
    /// 10:00:01 and 10:00:02, the chart's bars hold the first three executions, of volume 3 and
    /// delta 1, and the last, of 1 and 1. A source rebuilt from the held executions counts each
    /// copied execution once and shows those bars, as the source connected from the start does.
    #[cfg(feature = "widget")]
    #[test]
    fn a_page_copy_stamped_apart_from_its_live_execution_counts_once() {
        use OrderSide::{Buy, Sell};
        let clock = BarClock::new("3t".parse().unwrap());
        let start: DateTime<Utc> = "2026-01-05T10:00:00Z".parse().unwrap();
        let fill = |(micros, side, id)| {
            let at = start + TimeDelta::microseconds(micros);
            Tick::new(at, 100.0, 1.0, Some(side)).with_id(id)
        };
        let second = 1_000_000;
        let rows = [
            (
                [(-second, Buy, 1), (0, Sell, 2)],
                vec![(400, Sell, 2)],
                -second,
            ),
            (
                [(0, Buy, 1), (400, Sell, 2)],
                vec![(0, Buy, 1), (0, Sell, 2)],
                0,
            ),
        ];
        for (first, page, open) in rows {
            let inbox = Arc::new(super::super::inbox::Inbox::default());
            let sink = inbox.subscribe("A".into(), clock.clone(), 100, Some(1.0));
            let stayed = OrderFlowInput::new(OrderFlowKind::FlowSum { bars: 1 });
            inbox.study_inputs(sink.id, vec![stayed.clone()]);
            let buys = [(second, Buy, 3), (2 * second, Buy, 4)];
            for trade in first.into_iter().chain(buys) {
                sink.push_tick(fill(trade));
            }
            let (_, updates) = inbox.drain().updates.pop().unwrap();
            let bars: Vec<_> = updates.bars.values().map(|(bar, _)| bar.clone()).collect();
            // The chart applies the page before the executions it drains with it.
            let mut held = super::super::trades::Trades::new(100);
            held.prepend(page.into_iter().map(fill).collect(), &mut |_, _| ());
            held.append(updates.trades, &mut |_, _| ());
            let added = OrderFlowInput::new(OrderFlowKind::FlowSum { bars: 1 });
            added.replace_trades(&clock, &held, &bars, None);
            let expected = BTreeMap::from([
                (start + TimeDelta::microseconds(open), [3.0, 1.0, 0.0]),
                (start + TimeDelta::seconds(2), [1.0, 1.0, 0.0]),
            ]);
            let values = [stayed.values(), added.values()];
            assert_eq!(values, [expected.clone(), expected], "first bar at {open}");
        }
    }

    /// Hand counts: three-tick bars of buys of one lot, one a second from 09:59:57, so the
    /// chart's bars open at 09:59:57, 10:00:00, :03, :06 and :09. The chart drained the buys to
    /// 10:00:01 one at a time into a store of 100, 2 or 1: it kept them all, lost the 09:59:57
    /// bar, or lost the 10:00:00 buy too, which leaves no bar to rebuild along. The buys of :02
    /// to :06 wait in a queue of 100, of 3, which drops those of :02 and :03, or of zero, which
    /// drops them all; in the last row the host pushes a bar at :02 after the :02 buy, which
    /// restarts the stream's grouping without making a queued buy late. A volume and delta
    /// source and a footprint source connected then, and given the buys of :07 to :09, hold
    /// three buys in each chart bar from the oldest whose executions they all hold, and one at
    /// :09; after a lost or regrouped queue only the :09 bar.
    #[cfg(feature = "widget")]
    #[test]
    fn sources_connected_while_executions_wait_hold_the_charts_bars() {
        // Store and queue capacity, whether the host pushes a bar, and the bars held whole.
        type Row = (usize, usize, bool, &'static [i64]);
        let clock = BarClock::new("3t".parse().unwrap());
        let start: DateTime<Utc> = "2026-01-05T10:00:00Z".parse().unwrap();
        let at = |second: i64| start + TimeDelta::seconds(second);
        let buy = |second: i64| Tick::new(at(second), 100.0, 1.0, Some(OrderSide::Buy));
        let rows: [Row; 6] = [
            (100, 100, false, &[-3, 0, 3, 6]),
            (2, 100, false, &[0, 3, 6]),
            (1, 100, false, &[3, 6]),
            (100, 3, false, &[]),
            (1, 0, false, &[]),
            (1, 100, true, &[]),
        ];
        for (store, queue, pushed, whole) in rows {
            let inbox = Arc::new(super::super::inbox::Inbox::default());
            let sink = inbox.subscribe("A".into(), clock.clone(), queue, Some(1.0));
            let mut held = super::super::trades::Trades::new(store);
            let mut bars = BTreeMap::new();
            for second in -3..=1 {
                sink.push_tick(buy(second));
                let (_, drained) = inbox.drain().updates.pop().unwrap();
                held.append(drained.trades, &mut |_, _| ());
                bars.extend(drained.bars.into_iter().map(|(open, (bar, _))| (open, bar)));
            }
            let bars: Vec<_> = bars.into_values().collect();
            let flow = OrderFlowInput::new(OrderFlowKind::FlowSum { bars: 1 });
            let footprint = OrderFlowInput::footprint();
            footprint.set_tick_size(Some(1.0));
            // As the chart connects new sources: rebuilt from its store along its bars, then
            // given the queue.
            for input in [&flow, &footprint] {
                input.replace_trades(&clock, &held, &bars, None);
            }
            for second in 2..=6 {
                if pushed && second == 3 {
                    let bar = crate::model::Bar::new(at(2), 100.0, 100.0, 100.0, 100.0, 3.0);
                    sink.push_bar(bar);
                }
                sink.push_tick(buy(second));
            }
            inbox.study_inputs(sink.id, vec![flow.clone(), footprint.clone()]);
            for second in 7..=9 {
                sink.push_tick(buy(second));
            }
            let got: Vec<_> = (footprint.footprints_from(DateTime::<Utc>::MIN_UTC).bars)
                .values()
                .map(|bar| (bar.time, bar.volume()))
                .collect();
            let want_flow = (whole.iter().map(|s| (at(*s), [3.0, 3.0, 0.0])))
                .chain([(at(9), [1.0, 1.0, 0.0])])
                .collect::<BTreeMap<_, _>>();
            let want = (whole.iter().map(|s| (at(*s), 3.0)))
                .chain([(at(9), 1.0)])
                .collect::<Vec<_>>();
            assert_eq!(
                (flow.values(), got),
                (want_flow, want),
                "store {store}, queue {queue}, pushed bar {pushed}"
            );
        }
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
    /// A footprint keeps nothing without a tick size, including on trade-based bars.
    #[test]
    fn footprint_sources_follow_their_tick_size_and_bar_clock() {
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
        assert_eq!(
            held(),
            (Some(1.0), vec![100]),
            "tick bars use the execution grid"
        );
    }

    /// Hand values for Trade Rate summing volume over 60 and 3 seconds, rebuilt from a store of
    /// two. On 1min the sell of 01:05 drops the buy of 00:10, so bar 10:00 is cut and bar 10:01
    /// whole: the 60 second window at 01:05 reaches back to the loss, and the 3 second one holds
    /// only the sell of 4. On 2t the sell of 01:10 drops the buy of 00:00, whose bar the buy of 2
    /// at 01:05 completes, so the rebuild starts at the sell: its 60 second window reaches the buy
    /// of 2, which the rebuild leaves out, and gives no value either. On 3t the streamed buys of 2,
    /// 1 and 1 from 01:05 reach a store of four only as a page's copies, the buy of 01:05 kept
    /// apart, and the sell drops the page's buy of 00:00, older than them: the rebuild counts the
    /// kept buy and the copies of 01:06 and 01:07 in bar 10:01:05 and leaves out the page's copy of
    /// 01:05, which the 60 second window of the sell reaches. Later executions of a bar the rebuild
    /// left out add nothing to it: on 1min the sell of 01:30 drops the buy of 01:10, so the buy of
    /// 01:40 adds no bar 10:01, and the sell of 02:05 opens bar 10:02 alone. On 4t the sell of
    /// 01:10 drops the buy of 00:00 from the chart's forming bar, so nothing rebuilds, the sink's
    /// buy of 01:15 to that bar adds nothing, and its sell of 01:20 opens the next bar alone. The
    /// footprint of 1min keeps bar 10:01 only.
    #[cfg(feature = "widget")]
    #[test]
    fn rebuilt_sources_leave_out_what_the_store_dropped() {
        use OrderSide::{Buy, Sell};
        let start: DateTime<Utc> = "2026-01-05T10:00:00Z".parse().unwrap();
        let at = |second| start + TimeDelta::seconds(second);
        let held = |tape: [(i64, f64, OrderSide); 3]| {
            let mut store = super::super::trades::Trades::new(2);
            for (second, size, side) in tape {
                store.push(Tick::new(at(second), 100.0, size, Some(side)));
            }
            store
        };
        // The 2t chart's bars: the buys of 1 and 2, then the sell.
        let bars = [(0, 3.0), (70, 4.0)].map(|(second, volume)| {
            crate::model::Bar::new(at(second), 100.0, 100.0, 100.0, 100.0, volume)
        });
        // The 3t chart's bars: the streamed buys, then the sell.
        let streamed = [(65, 4.0), (70, 4.0)].map(|(second, volume)| {
            crate::model::Bar::new(at(second), 100.0, 100.0, 100.0, 100.0, volume)
        });
        let tick = |(second, size, side)| Tick::new(at(second), 100.0, size, Some(side));
        let buys = [(65, 2.0, Buy), (66, 1.0, Buy), (67, 1.0, Buy)];
        let mut copied = super::super::trades::Trades::new(4);
        let page = [(0, 1.0, Buy)].into_iter().chain(buys).map(tick).collect();
        copied.prepend(page, &mut |_, _| ());
        let mut drained = super::super::trades::Trades::new(4);
        buys.into_iter()
            .chain([(70, 4.0, Sell)])
            .for_each(|trade| drained.push(tick(trade)));
        copied.append(drained, &mut |_, _| ());
        // The 4t chart's forming bar: the buys of 1 and 2, then the sell.
        let forming = [(0, 7.0)].map(|(second, volume)| {
            crate::model::Bar::new(at(second), 100.0, 100.0, 100.0, 100.0, volume)
        });
        // The interval, the store, the chart's bars, the bars expected, and executions after the
        // rebuild: pushed by time, or as the sink's part of a chart bar.
        let rows = [
            (
                "1min",
                held([(10, 1.0, Buy), (50, 2.0, Buy), (65, 4.0, Sell)]),
                &[][..],
                &[60][..],
                &[][..],
            ),
            (
                "2t",
                held([(0, 1.0, Buy), (65, 2.0, Buy), (70, 4.0, Sell)]),
                &bars[..],
                &[70],
                &[],
            ),
            ("3t", copied, &streamed[..], &[65, 70], &[]),
            (
                "1min",
                held([(70, 1.0, Buy), (80, 2.0, Buy), (90, 4.0, Sell)]),
                &[],
                &[120],
                &[(100, 1.0, Buy, None), (125, 4.0, Sell, None)],
            ),
            (
                "4t",
                held([(0, 1.0, Buy), (65, 2.0, Buy), (70, 4.0, Sell)]),
                &forming[..],
                &[80],
                &[(75, 1.0, Buy, Some(0)), (80, 4.0, Sell, Some(80))],
            ),
        ];
        for (interval, store, history, opens, later) in &rows {
            let clock = BarClock::new(interval.parse().unwrap());
            let rate = OrderFlowInput::new(OrderFlowKind::TradeRate {
                volume: true,
                seconds: 60,
                instant: 3,
            });
            rate.replace_trades(&clock, store, history, None);
            let range = OrderFlowInput::new(OrderFlowKind::PriceRange { seconds: 60 });
            range.replace_trades(&clock, store, history, None);
            for &(second, size, side, bar) in *later {
                let trade = Tick::new(at(second), 100.0, size, Some(side));
                for input in [&rate, &range] {
                    match bar {
                        Some(bar) => input.push_trade_part(&clock, &trade, at(bar), size, true),
                        None => input.push_trade(&clock, &trade),
                    }
                }
            }
            assert!(
                range.values()[&at(opens[opens.len() - 1])]
                    .iter()
                    .all(|v| v.is_nan()),
                "{interval}: the price range reaches lost executions"
            );
            let values = rate.values();
            assert_eq!(
                values.keys().copied().collect::<Vec<_>>(),
                opens.iter().map(|second| at(*second)).collect::<Vec<_>>(),
                "{interval}"
            );
            let [total, rolling, instant] = values[&at(opens[opens.len() - 1])];
            assert_eq!((total, instant), (4.0, 4.0 / 3.0), "{interval}");
            assert!(
                rolling.is_nan(),
                "{interval}: the rolling window reaches the loss or what the rebuild left out"
            );
        }
        let (_, store, ..) = &rows[0];
        let clock = BarClock::new(Timeframe::Min1);
        let footprint = OrderFlowInput::footprint();
        footprint.set_tick_size(Some(1.0));
        footprint.replace_trades(&clock, store, &[], None);
        let flow = footprint.footprints_from(DateTime::<Utc>::MIN_UTC);
        assert_eq!(flow.complete_from, Some(at(60)));
        assert_eq!(flow.bars.keys().copied().collect::<Vec<_>>(), [at(60)]);
        assert_eq!(flow.bars[&at(60)].bid, 4.0);
    }

    /// On a daily clock with a session a footprint source cuts a replay at the trading date of
    /// the cutoff, and a live execution names the bar of its date as the one it changed.
    #[cfg(all(feature = "widget", feature = "timezones"))]
    #[test]
    fn footprint_sources_replay_and_recalculate_by_trading_date() {
        use crate::model::{Session, SessionWindow, TimezoneId};
        use chrono::{NaiveTime, Weekday::*};
        let t = |time: &str| time.parse::<DateTime<Utc>>().unwrap();
        let hm = |hour, minute| NaiveTime::from_hms_opt(hour, minute, 0).unwrap();
        let weekdays = [Mon, Tue, Wed, Thu, Fri];
        let chicago = TimezoneId::named("America/Chicago").unwrap();
        // A date trades from 17:00 the evening before to 16:00 in Chicago, 23:00 to 22:00 UTC.
        let futures = BarClock::new(Timeframe::Day1)
            .with_timezone(chicago.clone())
            .with_session(
                chicago,
                Session::new([SessionWindow::new(weekdays, hm(17, 0), hm(16, 0))]),
                false,
            );
        let input = OrderFlowInput::footprint();
        input.set_tick_size(Some(1.0));
        let mut held = super::super::trades::Trades::new(10);
        // Monday 15:00, Monday 18:30 on Tuesday's date and Tuesday 09:00 in Chicago.
        for (time, size) in [
            ("2024-01-08T21:00:00Z", 1.0),
            ("2024-01-09T00:30:00Z", 2.0),
            ("2024-01-09T15:00:00Z", 4.0),
        ] {
            let trade = Tick::new(t(time), 100.0, size, Some(OrderSide::Buy));
            input.push_trade(&futures, &trade);
            held.push(trade);
        }
        input.set_replay(&futures, Some(t("2024-01-09T14:00:00Z")), Some(&held), &[]);
        let replayed = input.footprints_from(DateTime::<Utc>::MIN_UTC).bars;
        assert_eq!(
            (replayed.iter())
                .map(|(open, bar)| (*open, bar.volume()))
                .collect::<Vec<_>>(),
            [
                (t("2024-01-08T06:00:00Z"), 1.0),
                (t("2024-01-09T06:00:00Z"), 2.0)
            ]
        );
        // On UTC days the post-market hour after midnight trades the date before.
        let equities = BarClock::new(Timeframe::Day1).with_session(
            TimezoneId::named("America/New_York").unwrap(),
            Session::new([SessionWindow::new(weekdays, hm(9, 30), hm(16, 0))])
                .with_extended_windows([
                    SessionWindow::new(weekdays, hm(4, 0), hm(9, 30)),
                    SessionWindow::new(weekdays, hm(16, 0), hm(20, 0)),
                ]),
            true,
        );
        let input = OrderFlowInput::footprint();
        input.set_tick_size(Some(1.0));
        let revision = input.revision();
        let late = Tick::new(t("2024-01-10T00:30:00Z"), 100.0, 1.0, Some(OrderSide::Buy));
        input.push_trade(&equities, &late);
        assert_eq!(
            input.changed_since(revision),
            Some(t("2024-01-09T00:00:00Z"))
        );
    }
}
