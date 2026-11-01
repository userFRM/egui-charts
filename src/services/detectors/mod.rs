//! A signal service that finds order-flow events in the executions and depth the host streams.

mod absorption;
mod big_trades;
mod depth;
mod icebergs;
mod liquidity;
mod stops;

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Mutex;

use chrono::{DateTime, Utc};

use super::{Events, SignalEvent, Signals};
use crate::data::depth::Book;
use crate::data::{FeedError, MarketEvent, OrderQueue};
use crate::model::{BarClock, Instrument, Signal, Tick, Timeframe, TimezoneId};
use absorption::Absorption;
pub use absorption::AbsorptionRule;
pub use big_trades::BigTradeRule;
use big_trades::BigTrades;
use depth::Depth;
pub use icebergs::IcebergRule;
use icebergs::Icebergs;
use liquidity::Liquidity;
pub use liquidity::LiquidityRule;
pub use stops::{StopRunRule, StopSweepRule};
use stops::{StopRuns, StopSweeps};

/// Detections of a symbol kept for the next subscription, most recent last.
const RETAINED: usize = 5_000;
/// Trade ids of a symbol remembered to drop repeated trades.
const TRADE_IDS: usize = 100_000;

/// A [`Signals`] service that finds order-flow events in the executions and depth a host already
/// streams, as [`PaperAccount`](super::PaperAccount) is a ready-made
/// [`Broker`](super::Broker). It needs no egui, so a server can run the same code.
///
/// The host [`track`](Self::track)s each symbol and feeds the service the events it pushes into
/// the chart's [`Sink`](crate::data::Sink), as [`MarketEvent`] values tagged with the symbol and
/// their feed time, so live feeds, recordings and replay share one input. Every window runs on
/// that feed time, never the wall clock, so replayed data gives the signals live data gave.
///
/// *Trades.* A trade with an id already seen is dropped, so the trade an order execution carries
/// counts once even when the feed also sends it as a trade. Fills group by the aggressor id on
/// their first copy; later copies cannot identify it retroactively. A feed that prints the tape
/// first must carry that id on the print or push the order execution before its tape copy.
/// Trades that are not finite numbers or have a negative size are ignored.
/// [`push_history`](Self::push_history) runs loaded trades through the same detection.
/// Trade timestamps must be valid feed times, including in loaded
/// history: the service trusts them without a tolerance against observation time. A far-future
/// timestamp can end zones at that time and advance the forming bar past later prints, preventing
/// body and wick classification until [`reset`](Self::reset). Validate times before pushing them.
///
/// *Book.* Depth snapshots and deltas, and order snapshots and events, keep the symbol's book
/// with the sequencing rules of the `Sink`: a gap or an invalid event clears the book without a
/// signal, and the next snapshot restores it.
///
/// *Signals.* Each [`DetectionKind`] has its own signal category, which the user can hide.
/// [`subscribe`](Signals::subscribe) pushes the symbol's most recent 5,000 detections, then each
/// new or changed one. Evicted detections are removed from existing subscriptions too.
///
/// ```
/// use std::sync::Arc;
///
/// use egui_charts::data::{ReplayEvent, Sink};
/// use egui_charts::model::Instrument;
/// use egui_charts::services::OrderFlowDetectors;
///
/// let detectors = Arc::new(OrderFlowDetectors::new());
/// let mut instrument = Instrument::new("ES");
/// instrument.tick_size = Some(0.25);
/// detectors.track(&instrument).unwrap();
/// // `Chart::set_signal_source(Some(detectors.clone()))` draws the detections.
///
/// /// Pushes one event of the feed into the chart and into the detectors.
/// fn push(sink: &Sink, detectors: &OrderFlowDetectors, event: &ReplayEvent) {
///     event.push_to(sink);
///     detectors.push(&event.symbol, event.time, &event.event);
/// }
/// ```
pub struct OrderFlowDetectors {
    state: Mutex<State>,
}

/// The detections the service makes, each drawn in its own signal category.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DetectionKind {
    /// A large aggressive execution, as a marker: see [`BigTradeRule`].
    BigTrade,
    /// The prices a large aggressive execution traded at, as a zone: see [`BigTradeRule`].
    BigTradeZone,
    /// A rapid sequence of same-side aggressive fills.
    StopRun,
    /// A closed bar meeting the stop sweep rule.
    StopSweep,
    /// Repeated visible refills at one price, exact or estimated.
    Iceberg,
    /// Aggressive volume absorbed at an inside price.
    Absorption,
    /// A large resting level followed from its arrival.
    LargeLiquidity,
    /// A resting level rejecting aggression as the inside moves away.
    Wall,
    /// A large level traded away before price passes it.
    LiquiditySweep,
    /// A large level withdrawn from the book.
    PulledLiquidity,
    /// A large individual order quickly withdrawn with little execution.
    Spoofing,
    /// Size added at one nearby price in a single update.
    FreshLiquidity,
}

impl DetectionKind {
    /// The signal category of the kind's detections.
    pub fn category(self) -> &'static str {
        match self {
            Self::BigTrade => "Big trades",
            Self::BigTradeZone => "Big trade zones",
            Self::StopRun => "Stop runs",
            Self::StopSweep => "Stop sweeps",
            Self::Iceberg => "Icebergs",
            Self::Absorption => "Absorption",
            Self::LargeLiquidity => "Large liquidity",
            Self::Wall => "Walls",
            Self::LiquiditySweep => "Liquidity sweeps",
            Self::PulledLiquidity => "Pulled liquidity",
            Self::Spoofing => "Spoofing",
            Self::FreshLiquidity => "Fresh liquidity",
        }
    }
}

/// The settings of every detector, applied by [`OrderFlowDetectors::set_settings`].
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct DetectorSettings {
    /// Large aggressive executions and their zones.
    pub big_trades: Detector<BigTradeRule>,
    /// Rapid same-side execution bursts.
    pub stop_runs: Detector<StopRunRule>,
    /// Directional footprint bars with exceptional volume.
    pub stop_sweeps: Detector<StopSweepRule>,
    /// Exact order refills and estimates from aggregate depth.
    pub icebergs: Detector<IcebergRule>,
    /// Aggressive volume absorbed at a resting inside price.
    pub absorption: Detector<AbsorptionRule>,
    /// Large resting levels, walls, sweeps, pulls and fresh size.
    pub liquidity: Detector<LiquidityRule>,
}

/// The settings of one detector.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Detector<R> {
    /// Whether the detector detects. A disabled detector still calibrates its automatic
    /// threshold.
    pub enabled: bool,
    /// Whether live detections should raise an alert. Detections from loaded history never do.
    pub alert: bool,
    /// Whether that alert should play a sound.
    pub sound: bool,
    /// What the detector detects.
    pub rule: R,
}

impl<R: Default> Default for Detector<R> {
    fn default() -> Self {
        Self {
            enabled: true,
            alert: false,
            sound: false,
            rule: R::default(),
        }
    }
}

/// The size or count from which a detector detects.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum Threshold {
    /// This positive and finite value, validated by [`OrderFlowDetectors::set_settings`].
    Fixed(f64),
    /// Calibrated on the symbol's own recent flow, as the detector's rule documents.
    Auto,
}

#[derive(Default)]
struct State {
    settings: DetectorSettings,
    symbols: HashMap<String, Tracked>,
    /// Subscriptions by symbol, tracked or not.
    subscriptions: HashMap<String, Subscription>,
}

struct Subscription {
    /// The interval of the latest subscription.
    timeframe: Timeframe,
    streams: Vec<Events<SignalEvent>>,
}

/// The state of one tracked symbol.
struct Tracked {
    instrument: Instrument,
    tick: f64,
    book: Book,
    orders: Option<OrderQueue>,
    depth: Depth,
    depth_sequence: Option<u64>,
    icebergs: Icebergs,
    absorption: Absorption,
    liquidity: Liquidity,
    /// Ids of the latest trades, oldest first, and the same as a set.
    trade_ids: VecDeque<u64>,
    seen_ids: HashSet<u64>,
    detections: VecDeque<Signal>,
    big_trades: BigTrades,
    stop_runs: StopRuns,
    stop_sweeps: StopSweeps,
}

impl Default for OrderFlowDetectors {
    fn default() -> Self {
        Self::new()
    }
}

impl OrderFlowDetectors {
    /// A service tracking no symbol, with the default settings.
    pub fn new() -> Self {
        Self {
            state: Mutex::new(State::default()),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Starts detecting on `instrument`, which needs a positive and finite tick size. Tracking a
    /// tracked symbol starts it afresh.
    pub fn track(&self, instrument: &Instrument) -> Result<(), FeedError> {
        let Some(tick) = instrument
            .tick_size
            .filter(|tick| tick.is_finite() && *tick > 0.0)
        else {
            return Err(FeedError::InvalidData(format!(
                "{} has no positive finite tick size",
                instrument.symbol
            )));
        };
        self.lock().track(instrument, tick);
        Ok(())
    }

    /// Stops detecting on `symbol` and drops its state and detections.
    pub fn untrack(&self, symbol: &str) {
        let mut state = self.lock();
        if state.symbols.remove(symbol).is_some() {
            deliver(
                None,
                state.subscriptions.get_mut(symbol),
                vec![SignalEvent::Clear],
            );
        }
    }

    /// Drops the state and detections of `symbol` and starts it afresh, as after a session
    /// change or a replay seek.
    pub fn reset(&self, symbol: &str) {
        let mut state = self.lock();
        if let Some(tracked) = state.symbols.get(symbol) {
            let (instrument, tick) = (tracked.instrument.clone(), tracked.tick);
            state.track(&instrument, tick);
        }
    }

    /// The settings of every detector.
    pub fn settings(&self) -> DetectorSettings {
        self.lock().settings.clone()
    }

    /// Replaces the settings of every detector from the next event on. Completed detections
    /// stay; a disabled liquidity tracker drops its unfinished zones. Panics if a fixed
    /// threshold is not positive and finite, or the refill count is zero.
    pub fn set_settings(&self, settings: DetectorSettings) {
        for threshold in [
            settings.big_trades.rule.threshold,
            settings.absorption.rule.threshold,
            settings.liquidity.rule.threshold,
            settings.liquidity.rule.fresh_threshold,
        ] {
            if let Threshold::Fixed(size) = threshold {
                assert!(
                    size.is_finite() && size > 0.0,
                    "fixed threshold must be positive and finite"
                );
            }
        }
        assert!(
            settings.icebergs.rule.refills > 0,
            "iceberg refills must be positive"
        );
        self.lock().settings = settings;
    }

    /// Detects on `event` of `symbol`, observed at feed time `time`. Events of untracked symbols
    /// are ignored.
    pub fn push(&self, symbol: &str, time: DateTime<Utc>, event: &MarketEvent) {
        let mut state = self.lock();
        let State {
            settings,
            symbols,
            subscriptions,
        } = &mut *state;
        let Some(tracked) = symbols.get_mut(symbol) else {
            return;
        };
        let mut out = Vec::new();
        // A loss clears the book, which waits for the next snapshot, and is not a detection.
        match event {
            MarketEvent::Trade(trade) => tracked.trade(trade, settings, true, &mut out),
            MarketEvent::Quote(_) => {}
            MarketEvent::DepthSnapshot { sequence, book } => {
                if tracked.orders.take().is_some() {
                    tracked.book = Book::new(Some(tracked.tick));
                    tracked.lose_depth(&mut out);
                }
                if tracked.book.snapshot(*sequence, book.clone()).is_ok() {
                    tracked.depth_sequence = Some(*sequence);
                    tracked.update_depth(
                        Depth::from_book(&tracked.book, tracked.tick),
                        time,
                        true,
                        settings,
                        &mut out,
                    );
                } else {
                    tracked.lose_depth(&mut out);
                }
            }
            MarketEvent::DepthDelta { sequence, book } => {
                if tracked.orders.take().is_some() {
                    tracked.book = Book::new(Some(tracked.tick));
                    tracked.lose_depth(&mut out);
                }
                let replayed = tracked.depth_sequence.is_some_and(|last| *sequence <= last);
                if tracked.book.delta(*sequence, book).is_err() {
                    tracked.lose_depth(&mut out);
                } else if !replayed {
                    tracked.depth_sequence = Some(*sequence);
                    tracked.update_depth(
                        Depth::from_book(&tracked.book, tracked.tick),
                        time,
                        false,
                        settings,
                        &mut out,
                    );
                }
            }
            MarketEvent::OrderSnapshot { sequence, orders } => {
                tracked.lose_depth(&mut out);
                let queue = tracked
                    .orders
                    .get_or_insert_with(|| OrderQueue::new(Some(tracked.tick)));
                let valid = queue.set_snapshot(*sequence, time, orders.clone()).is_ok();
                let trades = queue.take_executions();
                let depth = valid.then(|| Depth::from_orders(queue, tracked.tick));
                for trade in trades {
                    tracked.trade(&trade, settings, false, &mut out);
                }
                if let Some(depth) = depth {
                    tracked.update_depth(depth, time, true, settings, &mut out);
                }
            }
            MarketEvent::Order { sequence, event } => {
                let queue = tracked
                    .orders
                    .get_or_insert_with(|| OrderQueue::new(Some(tracked.tick)));
                let replayed = queue.sequence().is_some_and(|last| *sequence <= last);
                let id = match event {
                    crate::data::OrderEvent::Add(order) => order.id,
                    crate::data::OrderEvent::Modify { id, .. }
                    | crate::data::OrderEvent::Cancel { id }
                    | crate::data::OrderEvent::Execute { id, .. } => *id,
                };
                let before = queue.orders().find(|order| order.id == id).cloned();
                let valid = queue.event(*sequence, time, event.clone()).is_ok();
                let trades = queue.take_executions();
                if !valid {
                    tracked.lose_depth(&mut out);
                } else if !replayed {
                    let depth = Depth::from_orders(queue, tracked.tick);
                    for trade in trades {
                        tracked.trade(&trade, settings, true, &mut out);
                    }
                    tracked.icebergs.order(
                        event,
                        before.as_ref(),
                        time,
                        tracked.tick,
                        &settings.icebergs,
                        &mut out,
                    );
                    tracked.liquidity.order(
                        event,
                        before.as_ref(),
                        &depth,
                        time,
                        tracked.tick,
                        &settings.liquidity,
                        &mut out,
                    );
                    tracked.update_depth(depth, time, false, settings, &mut out);
                }
            }
        }
        tracked.absorption.advance(
            time,
            tracked.tick,
            &tracked.depth,
            &settings.absorption,
            &mut out,
        );
        tracked
            .stop_sweeps
            .advance(time, settings.stop_sweeps.enabled, &mut out);
        deliver(Some(tracked), subscriptions.get_mut(symbol), out);
    }

    /// Detects on the loaded trade history of `symbol`, in time order, before the live trades
    /// that follow it. Its detections are made and pushed like live ones and warm the automatic
    /// thresholds; trades with an id already seen are dropped.
    pub fn push_history(&self, symbol: &str, trades: &[Tick]) {
        let mut state = self.lock();
        let State {
            settings,
            symbols,
            subscriptions,
        } = &mut *state;
        let Some(tracked) = symbols.get_mut(symbol) else {
            return;
        };
        let mut out = Vec::new();
        for trade in trades {
            tracked.trade(trade, settings, false, &mut out);
        }
        deliver(Some(tracked), subscriptions.get_mut(symbol), out);
    }
}

impl Signals for OrderFlowDetectors {
    fn subscribe(&self, symbol: &str, timeframe: Timeframe, events: Events<SignalEvent>) {
        let mut state = self.lock();
        let State {
            symbols,
            subscriptions,
            settings,
        } = &mut *state;
        let new = !subscriptions.contains_key(symbol);
        let subscription = subscriptions
            .entry(symbol.to_owned())
            .or_insert_with(|| Subscription {
                timeframe,
                streams: Vec::new(),
            });
        if let Some(tracked) = symbols.get_mut(symbol) {
            if new || subscription.timeframe != timeframe {
                let mut changes: Vec<_> = tracked
                    .detections
                    .iter()
                    .filter(|signal| signal.category == DetectionKind::StopSweep.category())
                    .map(|signal| SignalEvent::Remove(signal.id.clone()))
                    .collect();
                tracked.stop_sweeps.set_clock(
                    clock(&tracked.instrument, timeframe),
                    &settings.stop_sweeps,
                    &mut changes,
                );
                deliver(Some(tracked), Some(subscription), changes);
            }
            for signal in &tracked.detections {
                events.push(SignalEvent::Upsert(signal.clone()));
            }
            if new || subscription.timeframe != timeframe {
                let clock = clock(&tracked.instrument, timeframe);
                tracked.big_trades.set_clock(clock);
            }
        }
        subscription.timeframe = timeframe;
        subscription.streams.retain(Events::is_active);
        subscription.streams.push(events);
    }
}

impl State {
    /// Starts tracking `instrument` of tick size `tick` afresh.
    fn track(&mut self, instrument: &Instrument, tick: f64) {
        let subscription = self.subscriptions.get_mut(&instrument.symbol);
        let mut tracked = Tracked {
            instrument: instrument.clone(),
            tick,
            book: Book::new(Some(tick)),
            orders: None,
            depth: Depth::default(),
            depth_sequence: None,
            icebergs: Icebergs::default(),
            absorption: Absorption::default(),
            liquidity: Liquidity::default(),
            trade_ids: VecDeque::new(),
            seen_ids: HashSet::new(),
            detections: VecDeque::new(),
            big_trades: BigTrades::new(tick, instrument.volume_decimals),
            stop_runs: StopRuns::default(),
            stop_sweeps: StopSweeps::new(tick),
        };
        let timeframe = subscription.as_ref().map(|s| s.timeframe);
        (tracked.big_trades).set_clock(timeframe.and_then(|tf| clock(instrument, tf)));
        tracked.stop_sweeps.set_clock(
            timeframe.and_then(|tf| clock(instrument, tf)),
            &self.settings.stop_sweeps,
            &mut Vec::new(),
        );
        if self
            .symbols
            .insert(instrument.symbol.clone(), tracked)
            .is_some()
        {
            deliver(None, subscription, vec![SignalEvent::Clear]);
        }
    }
}

impl Tracked {
    fn lose_depth(&mut self, out: &mut Vec<SignalEvent>) {
        self.depth = Depth::default();
        self.depth_sequence = None;
        self.icebergs.clear();
        self.absorption.clear();
        self.liquidity.clear(out);
    }

    fn update_depth(
        &mut self,
        depth: Depth,
        time: DateTime<Utc>,
        snapshot: bool,
        settings: &DetectorSettings,
        out: &mut Vec<SignalEvent>,
    ) {
        self.absorption.advance(
            time.checked_sub_signed(chrono::TimeDelta::nanoseconds(1))
                .unwrap_or(time),
            self.tick,
            &self.depth,
            &settings.absorption,
            out,
        );
        self.absorption.update(&self.depth, &depth);
        self.liquidity.update(
            &self.depth,
            &depth,
            time,
            self.tick,
            snapshot,
            &settings.liquidity,
            out,
        );
        self.depth = depth;
        self.icebergs
            .update(&self.depth, time, self.tick, &settings.icebergs, out);
    }

    fn trade(
        &mut self,
        trade: &Tick,
        settings: &DetectorSettings,
        live: bool,
        out: &mut Vec<SignalEvent>,
    ) {
        if !trade.price.is_finite() || !trade.size.is_finite() || trade.size < 0.0 {
            return;
        }
        if let Some(id) = trade.id {
            if !self.seen_ids.insert(id) {
                return;
            }
            self.trade_ids.push_back(id);
            if self.trade_ids.len() > TRADE_IDS
                && let Some(old) = self.trade_ids.pop_front()
            {
                self.seen_ids.remove(&old);
            }
        }
        if live {
            self.icebergs.trade(trade, self.tick, &self.depth);
            self.absorption
                .trade(trade, self.tick, &self.depth, &settings.absorption, out);
            self.liquidity.trade(
                trade,
                self.tick,
                settings.liquidity.enabled,
                &self.depth,
                out,
            );
        }
        self.stop_runs
            .trade(trade, self.tick, settings.stop_runs.enabled, out);
        self.stop_sweeps.trade(trade, &settings.stop_sweeps, out);
        let big = &settings.big_trades;
        (self.big_trades).trade(trade, &big.rule, big.enabled, out);
    }
}

/// Keeps `events` in the detections of `tracked`, if any, and pushes them to `subscription`.
fn deliver(
    tracked: Option<&mut Tracked>,
    subscription: Option<&mut Subscription>,
    events: Vec<SignalEvent>,
) {
    let mut delivered = Vec::new();
    if let Some(tracked) = tracked {
        for event in events {
            delivered.push(event.clone());
            let detections = &mut tracked.detections;
            match event {
                SignalEvent::Upsert(signal) => {
                    match detections.iter().rposition(|kept| kept.id == signal.id) {
                        Some(at) => detections[at] = signal,
                        None => {
                            if detections.len() == RETAINED
                                && let Some(old) = detections.pop_front()
                            {
                                tracked.big_trades.forget(&old.id);
                                tracked.liquidity.forget(&old.id);
                                delivered.push(SignalEvent::Remove(old.id));
                            }
                            detections.push_back(signal);
                        }
                    }
                }
                SignalEvent::Remove(id) => detections.retain(|kept| kept.id != id),
                SignalEvent::Clear => detections.clear(),
            }
        }
    } else {
        delivered = events;
    }
    if let Some(subscription) = subscription {
        subscription.streams.retain(Events::is_active);
        for event in delivered {
            for stream in &subscription.streams {
                stream.push(event.clone());
            }
        }
    }
}

/// The service bars of `timeframe` for `instrument`, or `None` for a trade-based interval.
fn clock(instrument: &Instrument, timeframe: Timeframe) -> Option<BarClock> {
    if timeframe.is_trade_based() {
        return None;
    }
    let zone = (instrument.time_zone.clone()).unwrap_or_else(TimezoneId::utc);
    let mut clock = BarClock::new(timeframe);
    if timeframe.duration_ms() < Timeframe::Day1.duration_ms() {
        clock = clock.with_timezone(zone.clone());
    }
    if let Some(session) = &instrument.session {
        clock = clock.with_session(zone, session.clone(), false);
    }
    Some(clock)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::OrderEvent;
    use crate::data::RestingOrder;
    use crate::data::inbox::Inbox;
    use crate::model::{Direction, OrderBook, OrderBookLevel, OrderSide, SignalId, SignalShape};
    use crate::services::events::Listener;
    use std::sync::Arc;

    fn time(seconds: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(seconds, 0).unwrap()
    }

    fn fill(seconds: i64, price: f64, size: f64, side: OrderSide, aggressor: Option<u64>) -> Tick {
        Tick {
            aggressor_order_id: aggressor,
            ..Tick::new(time(seconds), price, size, Some(side))
        }
    }

    fn marker(number: u64, at: i64, price: f64, direction: Direction, size: &str) -> SignalEvent {
        SignalEvent::Upsert(Signal {
            id: SignalId(format!("big-trade-{number}")),
            shape: SignalShape::Marker {
                time: time(at),
                price: Some(price),
                direction,
            },
            category: "Big trades".into(),
            label: Some(size.into()),
            tooltip: None,
        })
    }

    fn zone(number: u64, from: i64, to: Option<i64>, low: f64, high: f64) -> SignalEvent {
        SignalEvent::Upsert(Signal {
            id: SignalId(format!("big-trade-zone-{number}")),
            shape: SignalShape::Zone {
                from: time(from),
                to: to.map(time),
                low,
                high,
            },
            category: "Big trade zones".into(),
            label: None,
            tooltip: None,
        })
    }

    /// A service tracking "ES", tick 0.25, subscribed at one minute, with a fixed big trade
    /// threshold of `threshold` unless `None`.
    struct Host {
        detectors: OrderFlowDetectors,
        inbox: Arc<Inbox>,
        listeners: Vec<Listener>,
    }

    impl Host {
        fn new(threshold: Option<f64>) -> Self {
            let detectors = OrderFlowDetectors::new();
            let mut instrument = Instrument::new("ES").with_volume_decimals(0);
            instrument.tick_size = Some(0.25);
            detectors.track(&instrument).unwrap();
            let mut settings = DetectorSettings::default();
            settings.icebergs.enabled = false;
            settings.absorption.enabled = false;
            settings.liquidity.enabled = false;
            settings.stop_runs.enabled = false;
            settings.stop_sweeps.enabled = false;
            if let Some(size) = threshold {
                settings.big_trades.rule.threshold = Threshold::Fixed(size);
            }
            detectors.set_settings(settings);
            let mut host = Self {
                detectors,
                inbox: Arc::new(Inbox::default()),
                listeners: Vec::new(),
            };
            host.subscribe();
            host
        }

        fn subscribe(&mut self) {
            let (listener, events) = Listener::new(&self.inbox, 1);
            self.listeners.push(listener);
            self.detectors.subscribe("ES", Timeframe::Min1, events);
        }

        /// Pushes `event` and returns the signal events pushed since the last call.
        fn push(&self, event: MarketEvent) -> Vec<SignalEvent> {
            self.detectors.push("ES", time(0), &event);
            self.events()
        }

        fn events(&self) -> Vec<SignalEvent> {
            (self.inbox.drain().answers.into_iter())
                .map(|(_, event)| *event.downcast::<SignalEvent>().unwrap())
                .collect()
        }
    }

    #[test]
    fn big_trades_are_marked_and_project_zones_until_traded_through() {
        use OrderSide::{Buy, Sell};
        let host = Host::new(Some(10.0));
        let trade = |fill| host.push(MarketEvent::Trade(fill));
        assert!(trade(self::fill(1, 100.0, 2.0, Buy, Some(7))).is_empty());
        assert!(trade(self::fill(1, 101.0, 6.0, Buy, Some(7))).is_empty());
        // 16 reaches 10: the marker sits at the first fill and the volume-weighted price.
        assert_eq!(
            trade(self::fill(2, 101.0, 8.0, Buy, Some(7))),
            [
                marker(1, 1, 100.875, Direction::Up, "16"),
                zone(1, 1, None, 100.0, 101.0)
            ]
        );
        assert_eq!(
            trade(self::fill(3, 101.0, 16.0, Buy, Some(7))),
            [
                marker(1, 1, 100.9375, Direction::Up, "32"),
                zone(1, 1, None, 100.0, 101.0)
            ]
        );
        // A sell below the buy zone ends it; its own zone is one tick around its single price.
        assert_eq!(
            trade(self::fill(4, 99.0, 12.0, Sell, None)),
            [
                zone(1, 1, Some(4), 100.0, 101.0),
                marker(2, 4, 99.0, Direction::Down, "12"),
                zone(2, 4, None, 98.875, 99.125)
            ]
        );
        // A late trade from before the sell zone began does not end it.
        assert!(trade(self::fill(3, 99.5, 1.0, Buy, None)).is_empty());
        assert_eq!(
            trade(self::fill(5, 99.25, 1.0, Buy, None)),
            [zone(2, 4, Some(5), 98.875, 99.125)]
        );
    }

    #[test]
    fn zones_are_kept_by_where_they_traded_in_their_bar() {
        use OrderSide::{Buy, Sell};
        let mut host = Host::new(Some(10.0));
        let mut settings = host.detectors.settings();
        settings.big_trades.rule.wick_zones = false;
        host.detectors.set_settings(settings);
        let mut removed = Vec::new();
        let mut trade = |host: &Host, fill| {
            let events = host.push(MarketEvent::Trade(fill));
            removed.extend(
                events
                    .into_iter()
                    .filter(|event| matches!(event, SignalEvent::Remove(_))),
            );
        };
        // The first bar was not seen from its open, so its big buy in the upper wick stays.
        trade(&host, fill(-30, 100.0, 1.0, Buy, None));
        trade(&host, fill(-20, 103.0, 10.0, Buy, None));
        trade(&host, fill(-10, 101.0, 1.0, Buy, None));
        // The next bar opens at 100 and closes at 101: a big sell at 100.5 in the body and a big
        // buy at 103 in the upper wick.
        trade(&host, fill(0, 100.0, 1.0, Buy, None));
        trade(&host, fill(10, 100.5, 10.0, Sell, None));
        trade(&host, fill(20, 103.0, 10.0, Buy, None));
        // A new subscription at the same interval, once the old stream stopped, keeps the bar.
        host.listeners.clear();
        trade(&host, fill(25, 101.0, 1.0, Buy, None));
        host.subscribe();
        trade(&host, fill(30, 101.0, 1.0, Buy, None));
        trade(&host, fill(60, 101.0, 1.0, Buy, None));
        let mut settings = host.detectors.settings();
        settings.big_trades.rule.threshold = Threshold::Fixed(6.0);
        host.detectors.set_settings(settings);
        // The next bar closes on a six-lot at 99.99: (99.99 * (1 + 2 + 3)) / 6 = 99.99.
        trade(&host, fill(120, 99.98, 1.0, Buy, None));
        for (second, size) in [(130, 1.0), (131, 2.0), (132, 3.0)] {
            trade(&host, fill(second, 99.99, size, Buy, Some(5)));
        }
        trade(&host, fill(180, 99.99, 1.0, Buy, None));
        assert_eq!(
            removed,
            [SignalEvent::Remove(SignalId("big-trade-zone-3".into()))]
        );
    }

    #[test]
    fn detections_past_the_retained_5000_leave_the_live_chart_too() {
        let mut host = Host::new(Some(10.0));
        let buy = |price| fill(1, price, 10.0, OrderSide::Buy, None);
        // One page evicts the first marker and zone, then ends all 2,501 zones.
        let history: Vec<Tick> = std::iter::once(buy(50.0))
            .chain(std::iter::repeat_n(buy(100.0), 2500))
            .chain(std::iter::once(fill(2, 49.0, 1.0, OrderSide::Sell, None)))
            .collect();
        host.detectors.push_history("ES", &history);
        let mut live = HashMap::new();
        for event in host.events() {
            match event {
                SignalEvent::Upsert(signal) => {
                    live.insert(signal.id.clone(), signal);
                }
                SignalEvent::Remove(id) => {
                    live.remove(&id);
                }
                SignalEvent::Clear => live.clear(),
            }
        }
        host.listeners.clear();
        host.subscribe();
        let replay = host.events();
        // 5,002 detections lose marker 1 and zone 1; restoring ended zone 1 loses marker 2.
        assert_eq!(replay.len(), 5000);
        assert!(replay.contains(&zone(1, 1, Some(2), 49.875, 50.125)));
        assert!(!replay.contains(&marker(1, 1, 50.0, Direction::Up, "10")));
        assert!(!replay.contains(&marker(2, 1, 100.0, Direction::Up, "10")));
        assert_eq!(live.len(), 5000, "live detections after one history page");
        let replay: HashMap<_, _> = replay
            .into_iter()
            .map(|event| match event {
                SignalEvent::Upsert(signal) => (signal.id.clone(), signal),
                _ => panic!("replay must contain only detections"),
            })
            .collect();
        assert_eq!(live, replay);

        let mut host = Host::new(Some(10.0));
        let buy = |price| fill(1, price, 10.0, OrderSide::Buy, None);
        // A big buy at 50 and 2,499 at 100: a marker and a zone each, 5,000 detections.
        let history: Vec<Tick> = std::iter::once(buy(50.0))
            .chain(std::iter::repeat_n(buy(100.0), 2499))
            .collect();
        host.detectors.push_history("ES", &history);
        host.events();
        // The next one evicts the first marker and its zone, which is still open.
        assert_eq!(
            host.push(MarketEvent::Trade(buy(100.0))),
            [
                marker(2501, 1, 100.0, Direction::Up, "10"),
                SignalEvent::Remove(SignalId("big-trade-1".into())),
                zone(2501, 1, None, 99.875, 100.125),
                SignalEvent::Remove(SignalId("big-trade-zone-1".into()))
            ]
        );
        host.listeners.clear();
        host.subscribe();
        let replay = host.events();
        assert_eq!(replay.len(), 5000);
        assert!(!replay.contains(&zone(1, 1, None, 49.875, 50.125)));
        assert_eq!(
            host.push(MarketEvent::Trade(fill(
                2,
                49.0,
                1.0,
                OrderSide::Sell,
                None
            )))
            .len(),
            2500
        );
    }

    #[test]
    fn fixed_thresholds_must_be_positive_and_finite() {
        let detectors = OrderFlowDetectors::new();
        for size in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 0.0, -1.0] {
            let mut settings = detectors.settings();
            settings.big_trades.rule.threshold = Threshold::Fixed(size);
            assert!(
                std::panic::catch_unwind(|| detectors.set_settings(settings)).is_err(),
                "accepted {size}"
            );
            assert_eq!(detectors.settings(), DetectorSettings::default());
        }
        for size in [f64::MIN_POSITIVE, 10.0, f64::MAX] {
            let mut settings = detectors.settings();
            settings.big_trades.rule.threshold = Threshold::Fixed(size);
            detectors.set_settings(settings.clone());
            assert_eq!(detectors.settings(), settings);
        }
    }

    #[test]
    fn empty_and_invalid_trades_leave_the_aggregate_alone() {
        let host = Host::new(Some(10.0));
        host.detectors.push_history("ES", &[]);
        assert!(host.events().is_empty());
        let trade = |price, size| fill(1, price, size, OrderSide::Buy, Some(7)).with_id(1);
        for (price, size) in [(f64::NAN, 6.0), (100.0, f64::NAN), (100.0, -1.0)] {
            assert!(host.push(MarketEvent::Trade(trade(price, size))).is_empty());
        }
        assert!(host.push(MarketEvent::Trade(trade(100.0, 6.0))).is_empty());
        assert!(
            host.push(MarketEvent::Trade(fill(
                1,
                101.0,
                0.0,
                OrderSide::Buy,
                Some(7)
            )))
            .is_empty()
        );
        // A distinct fill with an earlier time still extends the first copy's aggressor.
        assert_eq!(
            host.push(MarketEvent::Trade(fill(
                0,
                100.0,
                4.0,
                OrderSide::Sell,
                Some(7)
            ))),
            [
                marker(1, 1, 100.0, Direction::Neutral, "10"),
                zone(1, 1, None, 100.0, 101.0)
            ]
        );
    }

    #[test]
    fn the_automatic_threshold_needs_1000_trades_then_takes_their_99th_percentile() {
        let host = Host::new(None);
        let history: Vec<Tick> = (1..=1000)
            .map(|size| fill(size, 100.0, size as f64, OrderSide::Buy, None))
            .collect();
        host.detectors.push_history("ES", &history);
        assert!(host.events().is_empty());
        // The trade of 989 closes the 1,000th, setting the threshold to 990.
        let trade = |size| {
            host.push(MarketEvent::Trade(fill(
                2000,
                100.0,
                size,
                OrderSide::Buy,
                None,
            )))
        };
        assert!(trade(989.0).is_empty());
        assert_eq!(
            trade(990.0)[0],
            marker(1, 2000, 100.0, Direction::Up, "990")
        );
    }

    #[test]
    fn a_depth_gap_clears_the_book_and_a_new_subscription_gets_the_detections() {
        let mut host = Host::new(Some(10.0));
        let execution = Tick {
            id: Some(9),
            ..fill(1, 100.0, 6.0, OrderSide::Buy, Some(5))
        };
        let resting = RestingOrder::new(1, OrderSide::Sell, 100.0, 20.0);
        host.push(MarketEvent::OrderSnapshot {
            sequence: 1,
            orders: vec![resting],
        });
        let event = OrderEvent::Execute {
            id: 1,
            size: 6.0,
            trade: Some(execution.clone()),
        };
        assert!(
            host.push(MarketEvent::Order { sequence: 2, event })
                .is_empty()
        );
        // The same trade from the trade feed is not counted again, or 12 would be big.
        assert!(host.push(MarketEvent::Trade(execution)).is_empty());
        let rest = Tick {
            id: Some(10),
            ..fill(1, 100.0, 4.0, OrderSide::Buy, Some(5))
        };
        let detections = host.push(MarketEvent::Trade(rest));
        assert_eq!(detections[0], marker(1, 1, 100.0, Direction::Up, "10"));

        let book = |price| OrderBook {
            ts: time(2),
            symbol: "ES".into(),
            bids: vec![OrderBookLevel {
                price,
                size: 5.0,
                order_cnt: None,
            }],
            asks: Vec::new(),
        };
        let held = |host: &Host| host.detectors.lock().symbols["ES"].book.levels().is_some();
        let snapshot = |sequence| MarketEvent::DepthSnapshot {
            sequence,
            book: book(99.0),
        };
        assert!(host.push(snapshot(1)).is_empty() && held(&host));
        let delta = MarketEvent::DepthDelta {
            sequence: 3,
            book: book(99.25),
        };
        assert!(host.push(delta).is_empty() && !held(&host));
        assert!(host.push(snapshot(4)).is_empty() && held(&host));

        host.subscribe();
        assert_eq!(host.events(), detections);

        let host = Host::new(Some(10.0));
        let trade = fill(2, 100.0, 12.0, OrderSide::Buy, None).with_id(7);
        assert!(
            host.push(MarketEvent::Order {
                sequence: 2,
                event: OrderEvent::Execute {
                    id: 1,
                    size: 12.0,
                    trade: Some(trade.clone())
                }
            })
            .is_empty()
        );
        assert!(
            host.push(MarketEvent::Order {
                sequence: 3,
                event: OrderEvent::Cancel { id: 99 }
            })
            .is_empty()
        );
        let events = host.push(MarketEvent::OrderSnapshot {
            sequence: 1,
            orders: vec![RestingOrder::new(1, OrderSide::Sell, 100.0, 12.0)],
        });
        assert_eq!(
            events,
            [
                marker(1, 2, 100.0, Direction::Up, "12"),
                zone(1, 2, None, 99.875, 100.125)
            ],
            "the validated fill precedes the invalid cancel in the snapshot buffer"
        );
        assert!(
            host.push(MarketEvent::Trade(trade)).is_empty(),
            "the tape copy counts once"
        );
    }
    #[test]
    fn stop_runs_require_a_fast_monotone_burst_and_distinct_known_aggressors() {
        for (name, ids, gap, prices, known, expected) in [
            (
                "three aggressors",
                vec![1, 1, 2, 2, 3],
                20,
                vec![400, 401, 401, 402, 403],
                true,
                true,
            ),
            (
                "no ids",
                vec![],
                10,
                vec![400, 401, 401, 402, 403],
                true,
                true,
            ),
            (
                "one aggressor",
                vec![1; 5],
                10,
                vec![400, 401, 401, 402, 403],
                true,
                false,
            ),
            (
                "gap",
                vec![],
                21,
                vec![400, 401, 401, 402, 403],
                true,
                false,
            ),
            (
                "reversal",
                vec![],
                10,
                vec![400, 401, 400, 402, 403],
                true,
                false,
            ),
            (
                "unknown side",
                vec![],
                10,
                vec![400, 401, 401, 402, 403],
                false,
                false,
            ),
        ] {
            for side in [OrderSide::Buy, OrderSide::Sell] {
                let host = Host::new(Some(1_000.0));
                let mut settings = host.detectors.settings();
                settings.stop_runs.enabled = true;
                host.detectors.set_settings(settings);
                let mut events = Vec::new();
                for (i, price) in prices.iter().enumerate() {
                    let price = if side == OrderSide::Buy {
                        *price
                    } else {
                        800 - price
                    };
                    let mut fill = Tick::new(
                        time(0) + chrono::TimeDelta::milliseconds(i as i64 * gap),
                        price as f64 * 0.25,
                        2.0,
                        known.then_some(side),
                    );
                    fill.aggressor_order_id = ids.get(i).copied();
                    events.extend(host.push(MarketEvent::Trade(fill)));
                }
                let signals: Vec<_> = events
                    .into_iter()
                    .filter_map(|event| match event {
                        SignalEvent::Upsert(signal) if signal.category == "Stop runs" => {
                            Some(signal)
                        }
                        _ => None,
                    })
                    .collect();
                assert_eq!(signals.len(), usize::from(expected), "{name} {side:?}");
                if let Some(signal) = signals.first() {
                    assert_eq!(
                        signal.tooltip.as_deref(),
                        Some(format!("5 fills, 3 ticks, size 10, {} ms", gap * 4).as_str())
                    );
                    assert_eq!(
                        signal.shape,
                        SignalShape::Marker {
                            time: time(0) + chrono::TimeDelta::milliseconds(gap * 4),
                            price: Some(if side == OrderSide::Buy {
                                100.75
                            } else {
                                99.25
                            }),
                            direction: stops::direction(side),
                        }
                    );
                }
            }
        }
    }

    #[test]
    fn stop_sweeps_use_closed_footprints_and_rebuild_on_interval_changes() {
        for (name, buy_size, close, extra_low, extra_sell, expected) in [
            ("qualifies", 10.0, 100.75, 0.0, 0.0, true),
            ("ratio below three", 2.0, 100.75, 30.0, 0.0, false),
            ("small body", 10.0, 100.25, 0.0, 0.0, false),
            ("low volume", 3.0, 100.75, 0.0, 0.0, false),
            ("opposite delta", 10.0, 100.75, 0.0, 40.0, false),
            ("truncated executions", 10.0, 100.75, 0.0, 0.0, false),
        ] {
            let mut host = Host::new(Some(1_000.0));
            let mut settings = host.detectors.settings();
            settings.stop_sweeps.enabled = true;
            host.detectors.set_settings(settings);
            // Eleven preceding bars: the first may be partial, the next ten each have volume 20.
            for minute in 0..11 {
                host.push(MarketEvent::Trade(fill(
                    minute * 60,
                    100.0,
                    20.0,
                    OrderSide::Buy,
                    None,
                )));
            }
            // Bid at 100, 100.25, 100.5 is 1 each. Ask at the next three ticks is 10 each:
            // diagonal ratios 10:1, volume 33, delta 27 (81.8%), full-range body, baseline 20.
            for (second, price, size, side) in [
                (660, 100.0, 1.0, OrderSide::Sell),
                (661, 100.25, 1.0, OrderSide::Sell),
                (662, 100.5, 1.0, OrderSide::Sell),
                (663, 100.25, buy_size, OrderSide::Buy),
                (664, 100.5, buy_size, OrderSide::Buy),
                (665, 100.75, buy_size, OrderSide::Buy),
                (665, 100.0, extra_low, OrderSide::Buy),
                (665, 100.75, extra_sell, OrderSide::Sell),
            ] {
                host.push(MarketEvent::Trade(fill(second, price, size, side, None)));
            }
            if name == "truncated executions" {
                let trades: Vec<_> =
                    std::iter::repeat_n(fill(665, 100.25, 0.001, OrderSide::Buy, None), 100_001)
                        .collect();
                host.detectors.push_history("ES", &trades);
                host.events();
            }
            host.push(MarketEvent::Trade(fill(
                666,
                close,
                0.1,
                OrderSide::Buy,
                None,
            )));
            // An unclassified print advances feed time without entering the footprint history.
            host.detectors.push(
                "ES",
                time(720),
                &MarketEvent::Trade(Tick::new(time(720), 100.75, 1.0, None)),
            );
            let events = host.events();
            let sweeps: Vec<_> = events
                .into_iter()
                .filter(|event| {
                    matches!(event,
                SignalEvent::Upsert(signal) if signal.category == "Stop sweeps")
                })
                .collect();
            assert_eq!(sweeps.len(), usize::from(expected), "{name}");
            let (listener, events) = Listener::new(&host.inbox, 1);
            host.listeners.push(listener);
            host.detectors.subscribe("ES", Timeframe::Min5, events);
            let events = host.events();
            assert!(
                !events.iter().any(|event| matches!(event,
                SignalEvent::Upsert(signal) if signal.category == "Stop sweeps")),
                "{name}: rebuilt five-minute bars"
            );
            if expected {
                assert!(events.iter().any(|event| matches!(event,
                    SignalEvent::Remove(id) if id.0 == "stop-sweep-660000")));
            }
            host.listeners.clear();
            host.subscribe();
            assert_eq!(
                of_kind(host.events(), "Stop sweeps").len(),
                usize::from(expected),
                "{name}: rebuilt back at one minute after the last sided execution"
            );
        }
    }
    fn depth_book(bids: &[(f64, f64)], asks: &[(f64, f64)]) -> OrderBook {
        let levels = |rows: &[(f64, f64)]| {
            rows.iter()
                .map(|&(price, size)| OrderBookLevel {
                    price,
                    size,
                    order_cnt: None,
                })
                .collect()
        };
        OrderBook {
            ts: time(0),
            symbol: "ES".into(),
            bids: levels(bids),
            asks: levels(asks),
        }
    }

    fn of_kind(events: Vec<SignalEvent>, category: &str) -> Vec<Signal> {
        events
            .into_iter()
            .filter_map(|event| match event {
                SignalEvent::Upsert(signal) if signal.category == category => Some(signal),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn icebergs_count_exact_refills_and_confirm_aggregate_estimates_against_the_book() {
        for exact in [false, true] {
            for consumed in [false, true] {
                let host = Host::new(Some(1_000.0));
                let mut settings = host.detectors.settings();
                settings.icebergs.enabled = true;
                host.detectors.set_settings(settings);
                let push = |ms, event| {
                    host.detectors.push(
                        "ES",
                        time(0) + chrono::TimeDelta::milliseconds(ms),
                        &event,
                    );
                    of_kind(host.events(), "Icebergs")
                };
                let initial = if exact {
                    MarketEvent::OrderSnapshot {
                        sequence: 1,
                        orders: vec![RestingOrder::new(1, OrderSide::Sell, 100.0, 10.0)],
                    }
                } else {
                    MarketEvent::DepthSnapshot {
                        sequence: 1,
                        book: depth_book(&[], &[(100.0, 10.0)]),
                    }
                };
                push(0, initial);
                let mut found = Vec::new();
                for index in 0..3 {
                    let mut trade = fill(0, 100.0, 10.0, OrderSide::Buy, None);
                    trade.ts += chrono::TimeDelta::milliseconds(index * 20 + 10);
                    trade.id = Some(index as u64 + 1);
                    if exact {
                        found.extend(push(
                            index * 20 + 10,
                            MarketEvent::Order {
                                sequence: index as u64 * 2 + 2,
                                event: OrderEvent::Execute {
                                    id: index as u64 + 1,
                                    size: 10.0,
                                    trade: Some(trade.clone()),
                                },
                            },
                        ));
                        // The tape repeats the carried execution; it must never count twice.
                        found.extend(push(index * 20 + 10, MarketEvent::Trade(trade)));
                        if !consumed || index < 1 {
                            found.extend(push(
                                index * 20 + 20,
                                MarketEvent::Order {
                                    sequence: index as u64 * 2 + 3,
                                    event: OrderEvent::Add(RestingOrder::new(
                                        index as u64 + 2,
                                        OrderSide::Sell,
                                        100.0,
                                        10.0,
                                    )),
                                },
                            ));
                        } else {
                            break;
                        }
                    } else {
                        assert!(
                            push(index * 20 + 10, MarketEvent::Trade(trade)).is_empty(),
                            "wait for depth confirmation"
                        );
                        found.extend(push(
                            index * 20 + 20,
                            MarketEvent::DepthDelta {
                                sequence: index as u64 + 2,
                                book: depth_book(
                                    &[],
                                    &[(100.0, if consumed && index == 2 { 0.0 } else { 10.0 })],
                                ),
                            },
                        ));
                    }
                }
                if consumed {
                    assert!(found.is_empty(), "consumed exact={exact}");
                } else {
                    assert_eq!(found.len(), if exact { 2 } else { 1 });
                    assert!(found.iter().all(|signal| signal.id == found[0].id));
                    assert_eq!(
                        found.last().unwrap().tooltip.as_deref(),
                        Some(if exact {
                            "3 refills, executed 30, shown 10; order-level refills"
                        } else {
                            "2 refills, executed 30, shown 10; estimate from aggregated depth"
                        })
                    );
                }
            }
        }

        // 0.3 / 0.1 rounds just below three in binary floating point.
        let host = Host::new(Some(1_000.0));
        let mut settings = host.detectors.settings();
        settings.icebergs.enabled = true;
        host.detectors.set_settings(settings);
        host.push(MarketEvent::DepthSnapshot {
            sequence: 1,
            book: depth_book(&[], &[(100.0, 0.1)]),
        });
        host.push(MarketEvent::Trade(fill(
            0,
            100.0,
            0.3,
            OrderSide::Buy,
            None,
        )));
        let signals = of_kind(
            host.push(MarketEvent::DepthDelta {
                sequence: 2,
                book: depth_book(&[], &[(100.0, 0.1)]),
            }),
            "Icebergs",
        );
        assert_eq!(signals.len(), 1);
        assert!(
            signals[0]
                .tooltip
                .as_deref()
                .unwrap()
                .starts_with("2 refills")
        );

        let host = Host::new(Some(1_000.0));
        let mut settings = host.detectors.settings();
        settings.icebergs.enabled = true;
        host.detectors.set_settings(settings);
        host.push(MarketEvent::OrderSnapshot {
            sequence: 1,
            orders: vec![
                RestingOrder::new(1, OrderSide::Sell, 100.0, 10.0),
                RestingOrder::new(2, OrderSide::Sell, 100.0, 10.0),
                RestingOrder::new(3, OrderSide::Sell, 100.0, 1.0),
            ],
        });
        for (sequence, event) in [
            (
                2,
                OrderEvent::Execute {
                    id: 1,
                    size: 10.0,
                    trade: None,
                },
            ),
            (
                3,
                OrderEvent::Execute {
                    id: 2,
                    size: 10.0,
                    trade: None,
                },
            ),
            (4, OrderEvent::Cancel { id: 3 }),
            (
                5,
                OrderEvent::Add(RestingOrder::new(4, OrderSide::Sell, 100.0, 7.0)),
            ),
            (
                6,
                OrderEvent::Add(RestingOrder::new(5, OrderSide::Sell, 100.0, 10.0)),
            ),
            (
                7,
                OrderEvent::Add(RestingOrder::new(6, OrderSide::Sell, 100.0, 10.0)),
            ),
        ] {
            host.detectors
                .push("ES", time(0), &MarketEvent::Order { sequence, event });
        }
        let signals = of_kind(host.events(), "Icebergs");
        assert_eq!(
            signals.len(),
            1,
            "both depletions survive unrelated orders at the same price"
        );
        assert_eq!(
            signals[0].tooltip.as_deref(),
            Some("2 refills, executed 20, shown 10; order-level refills")
        );

        let host = Host::new(Some(1_000.0));
        let mut settings = host.detectors.settings();
        settings.icebergs.enabled = true;
        host.detectors.set_settings(settings);
        host.push(MarketEvent::OrderSnapshot {
            sequence: 1,
            orders: vec![RestingOrder::new(1, OrderSide::Sell, 100.0, 0.3)],
        });
        for i in 0..2 {
            for (offset, size) in [(0, 0.1), (1, 0.2)] {
                host.detectors.push(
                    "ES",
                    time(0),
                    &MarketEvent::Order {
                        sequence: 2 + i * 3 + offset,
                        event: OrderEvent::Execute {
                            id: 1 + i,
                            size,
                            trade: None,
                        },
                    },
                );
            }
            host.detectors.push(
                "ES",
                time(0),
                &MarketEvent::Order {
                    sequence: 4 + i * 3,
                    event: OrderEvent::Add(RestingOrder::new(2 + i, OrderSide::Sell, 100.0, 0.3)),
                },
            );
        }
        let signals = of_kind(host.events(), "Icebergs");
        assert_eq!(
            signals.len(),
            1,
            "decimal fills replenish twice without a false depth loss"
        );
        assert!(
            signals[0]
                .tooltip
                .as_deref()
                .unwrap()
                .starts_with("2 refills")
        );
    }

    #[test]
    fn iceberg_refills_reject_late_replacements_priority_keeps_and_depth_gaps() {
        for (name, gap, delay, keep, expected) in [
            ("priority lost", false, 10, false, 1),
            ("changed displayed size", false, 10, false, 1),
            ("smaller refill clip", false, 10, false, 1),
            ("priority retained", false, 10, true, 0),
            ("gap", true, 10, false, 0),
            ("replacement too late", false, 101, false, 0),
            ("partial refill too late", false, 101, false, 0),
        ] {
            let host = Host::new(Some(1_000.0));
            let mut settings = host.detectors.settings();
            settings.icebergs.enabled = true;
            host.detectors.set_settings(settings);
            host.detectors.push(
                "ES",
                time(0),
                &MarketEvent::OrderSnapshot {
                    sequence: 1,
                    orders: vec![RestingOrder::new(1, OrderSide::Buy, 100.0, 10.0)],
                },
            );
            let mut found = Vec::new();
            for i in 0..2 {
                let seq = 2 + i * 2;
                let at = time(0) + chrono::TimeDelta::milliseconds(i as i64 * 200);
                host.detectors.push(
                    "ES",
                    at,
                    &MarketEvent::Order {
                        sequence: seq + u64::from(gap),
                        event: OrderEvent::Execute {
                            id: 1,
                            size: if name == "replacement too late" {
                                10.0
                            } else {
                                5.0
                            },
                            trade: None,
                        },
                    },
                );
                let event = if name == "replacement too late" {
                    OrderEvent::Add(RestingOrder::new(1, OrderSide::Buy, 100.0, 10.0))
                } else {
                    OrderEvent::Modify {
                        id: 1,
                        price: None,
                        size: Some(if name == "changed displayed size" {
                            12.0 + i as f64
                        } else if name == "smaller refill clip" {
                            8.0 + i as f64
                        } else {
                            10.0
                        }),
                        keep_priority: keep,
                    }
                };
                host.detectors.push(
                    "ES",
                    at + chrono::TimeDelta::milliseconds(delay),
                    &MarketEvent::Order {
                        sequence: seq + 1,
                        event,
                    },
                );
                found.extend(of_kind(host.events(), "Icebergs"));
            }
            assert_eq!(found.len(), expected, "{name}");
        }
    }
    #[test]
    fn absorption_requires_a_held_inside_level_and_discards_broken_or_lost_episodes() {
        for side in [OrderSide::Buy, OrderSide::Sell] {
            for (name, remaining, broken, gap, expected) in [
                ("held", 5.0, false, false, true),
                ("decimal lots", 0.15, false, false, true),
                ("less than half remains", 4.0, false, false, false),
                ("traded through", 10.0, true, false, false),
                ("idle close before later break", 10.0, true, false, true),
                ("idle close before withdrawal", 0.0, false, false, true),
                ("gap", 10.0, false, true, false),
            ] {
                let host = Host::new(Some(1_000.0));
                let mut settings = host.detectors.settings();
                settings.absorption.enabled = true;
                settings.absorption.rule.threshold =
                    Threshold::Fixed(if name == "decimal lots" { 0.3 } else { 10.0 });
                host.detectors.set_settings(settings);
                let book = |size| {
                    if side == OrderSide::Buy {
                        depth_book(&[(100.0, size)], &[])
                    } else {
                        depth_book(&[], &[(100.0, size)])
                    }
                };
                host.push(MarketEvent::DepthSnapshot {
                    sequence: 1,
                    book: book(if name == "decimal lots" {
                        0.1 + 0.2
                    } else {
                        10.0
                    }),
                });
                let aggressor = if side == OrderSide::Buy {
                    OrderSide::Sell
                } else {
                    OrderSide::Buy
                };
                for (ms, size) in [
                    (0, if name == "decimal lots" { 0.15 } else { 4.0 }),
                    (200, if name == "decimal lots" { 0.15 } else { 6.0 }),
                ] {
                    let trade = Tick::new(
                        time(0) + chrono::TimeDelta::milliseconds(ms),
                        100.0,
                        size,
                        Some(aggressor),
                    );
                    host.detectors
                        .push("ES", trade.ts, &MarketEvent::Trade(trade));
                }
                if broken {
                    let price = if side == OrderSide::Buy {
                        99.75
                    } else {
                        100.25
                    };
                    let at = time(0)
                        + chrono::TimeDelta::milliseconds(
                            if name == "idle close before later break" {
                                1_500
                            } else {
                                500
                            },
                        );
                    host.detectors.push(
                        "ES",
                        at,
                        &MarketEvent::Trade(Tick::new(at, price, 1.0, None)),
                    );
                }
                host.detectors.push(
                    "ES",
                    if name.starts_with("idle close") {
                        time(2)
                    } else {
                        time(1) + chrono::TimeDelta::milliseconds(200)
                    },
                    &MarketEvent::DepthDelta {
                        sequence: if gap { 3 } else { 2 },
                        book: book(remaining),
                    },
                );
                if gap {
                    host.detectors.push(
                        "ES",
                        time(1) + chrono::TimeDelta::milliseconds(200),
                        &MarketEvent::DepthSnapshot {
                            sequence: 3,
                            book: book(10.0),
                        },
                    );
                }
                let signals = of_kind(host.events(), "Absorption");
                assert_eq!(signals.len(), usize::from(expected), "{name} {side:?}");
                if expected {
                    assert_eq!(
                        signals[0].shape,
                        SignalShape::PriceZone {
                            from: time(0),
                            to: Some(time(0) + chrono::TimeDelta::milliseconds(200)),
                            price: 100.0,
                            direction: stops::direction(side)
                        }
                    );
                }
            }
        }
    }

    #[test]
    fn absorption_auto_threshold_uses_closed_episode_nearest_rank_and_order_adds_can_hold_a_level()
    {
        let host = Host::new(Some(1_000.0));
        let mut settings = host.detectors.settings();
        settings.absorption.enabled = true;
        host.detectors.set_settings(settings);
        host.push(MarketEvent::DepthSnapshot {
            sequence: 1,
            book: depth_book(&[], &[(100.0, 1.0)]),
        });
        for i in 0..100 {
            host.detectors.push(
                "ES",
                time(i * 2),
                &MarketEvent::Trade(fill(i * 2, 100.0, (i + 1) as f64, OrderSide::Buy, None)),
            );
        }
        host.detectors.push(
            "ES",
            time(199),
            &MarketEvent::DepthDelta {
                sequence: 2,
                book: depth_book(&[], &[]),
            },
        );
        assert!(
            of_kind(host.events(), "Absorption").is_empty(),
            "100-episode warm-up"
        );
        // Sorted 1..100 has nearest-rank 99th percentile 99. The next 98 fails, then 99 passes.
        for (at, size, expected) in [(200, 98.0, 0), (202, 99.0, 1)] {
            host.detectors.push(
                "ES",
                time(at),
                &MarketEvent::Trade(fill(at, 100.0, size, OrderSide::Buy, None)),
            );
            host.detectors.push(
                "ES",
                time(at + 1),
                &MarketEvent::DepthDelta {
                    sequence: (at / 2 - 97) as u64,
                    book: depth_book(&[], &[]),
                },
            );
            assert_eq!(of_kind(host.events(), "Absorption").len(), expected);
        }
        host.detectors.reset("ES");
        let mut settings = host.detectors.settings();
        settings.absorption.rule.threshold = Threshold::Fixed(10.0);
        host.detectors.set_settings(settings);
        host.detectors.push(
            "ES",
            time(300),
            &MarketEvent::OrderSnapshot {
                sequence: 1,
                orders: vec![RestingOrder::new(1, OrderSide::Sell, 100.0, 10.0)],
            },
        );
        host.detectors.push(
            "ES",
            time(300),
            &MarketEvent::Trade(fill(300, 100.0, 10.0, OrderSide::Buy, None)),
        );
        host.detectors.push(
            "ES",
            time(300),
            &MarketEvent::Order {
                sequence: 2,
                event: OrderEvent::Add(RestingOrder::new(2, OrderSide::Sell, 100.0, 1.0)),
            },
        );
        host.detectors.push(
            "ES",
            time(300),
            &MarketEvent::Order {
                sequence: 3,
                event: OrderEvent::Modify {
                    id: 1,
                    price: None,
                    size: Some(2.0),
                    keep_priority: true,
                },
            },
        );
        host.detectors.push(
            "ES",
            time(301),
            &MarketEvent::Order {
                sequence: 4,
                event: OrderEvent::Modify {
                    id: 1,
                    price: None,
                    size: Some(2.0),
                    keep_priority: true,
                },
            },
        );
        assert_eq!(
            of_kind(host.events(), "Absorption").len(),
            1,
            "added size holds despite less than half remaining"
        );
    }
    #[test]
    fn liquidity_zones_end_at_the_pull_or_sweep_and_walls_need_execution_and_rejection() {
        for bid in [false, true] {
            for outcome in ["pull", "sweep", "sweep before depth", "wall"] {
                let mut host = Host::new(Some(1_000.0));
                let mut settings = host.detectors.settings();
                settings.liquidity.enabled = true;
                settings.liquidity.rule.threshold = Threshold::Fixed(50.0);
                settings.liquidity.rule.fresh_threshold = Threshold::Fixed(50.0);
                host.detectors.set_settings(settings);
                let book = |rows: &[(f64, f64)]| {
                    if bid {
                        depth_book(rows, &[])
                    } else {
                        depth_book(&[], rows)
                    }
                };
                let initial = host.push(MarketEvent::DepthSnapshot {
                    sequence: 1,
                    book: book(&[(100.0, 100.0)]),
                });
                let zones = of_kind(initial, "Large liquidity");
                assert_eq!(zones.len(), 1);
                let side = if bid { OrderSide::Sell } else { OrderSide::Buy };
                let traded = match outcome {
                    "sweep" | "sweep before depth" => 90.0,
                    "wall" => 10.0,
                    _ => 0.0,
                };
                if traded > 0.0 {
                    host.detectors.push(
                        "ES",
                        time(1),
                        &MarketEvent::Trade(fill(1, 100.0, traded, side, None)),
                    );
                }
                let away = if bid { 100.75 } else { 99.25 };
                if outcome == "sweep before depth" {
                    let beyond = if bid { 99.75 } else { 100.25 };
                    host.detectors.push(
                        "ES",
                        time(1),
                        &MarketEvent::Trade(fill(1, beyond, 1.0, side, None)),
                    );
                }
                let rows = if outcome == "wall" {
                    vec![(100.0, 90.0), (away, 1.0)]
                } else {
                    vec![(100.0, 10.0)]
                };
                host.detectors.push(
                    "ES",
                    time(2),
                    &MarketEvent::DepthDelta {
                        sequence: 2,
                        book: book(&rows),
                    },
                );
                if outcome == "sweep" {
                    let beyond = if bid { 99.75 } else { 100.25 };
                    host.detectors.push(
                        "ES",
                        time(3),
                        &MarketEvent::Trade(fill(3, beyond, 1.0, side, None)),
                    );
                }
                let events = host.events();
                let category = match outcome {
                    "pull" => "Pulled liquidity",
                    "sweep" | "sweep before depth" => "Liquidity sweeps",
                    _ => "Walls",
                };
                assert_eq!(
                    of_kind(events.clone(), category).len(),
                    1,
                    "{outcome} bid={bid}"
                );
                let ended = of_kind(events, "Large liquidity");
                if outcome == "wall" {
                    assert!(ended.is_empty());
                } else {
                    assert_eq!(ended.len(), 1);
                    assert_eq!(ended[0].id, zones[0].id);
                    assert_eq!(
                        ended[0].shape,
                        SignalShape::PriceZone {
                            from: time(0),
                            to: Some(time(if outcome == "sweep" { 3 } else { 2 })),
                            price: 100.0,
                            direction: if bid { Direction::Up } else { Direction::Down }
                        }
                    );
                }
                // Resubscribing retains the final zone and marker, including its end.
                host.listeners.clear();
                host.subscribe();
                assert_eq!(of_kind(host.events(), category).len(), 1);
            }
        }

        let host = Host::new(Some(1_000.0));
        let mut settings = host.detectors.settings();
        settings.liquidity.enabled = true;
        settings.liquidity.rule.threshold = Threshold::Fixed(50.0);
        host.detectors.set_settings(settings);
        host.push(MarketEvent::OrderSnapshot {
            sequence: 1,
            orders: vec![RestingOrder::new(1, OrderSide::Buy, 100.0, 100.0)],
        });
        host.push(MarketEvent::Order {
            sequence: 2,
            event: OrderEvent::Execute {
                id: 1,
                size: 10.0,
                trade: Some(Tick::new(time(0), 100.0, 10.0, None)),
            },
        });
        let walls = of_kind(
            host.push(MarketEvent::Order {
                sequence: 3,
                event: OrderEvent::Add(RestingOrder::new(2, OrderSide::Buy, 100.75, 1.0)),
            }),
            "Walls",
        );
        assert_eq!(
            walls.len(),
            1,
            "order-level execution needs no tape aggressor inference"
        );
        assert!(walls[0].tooltip.as_deref().unwrap().contains("executed 10"));
    }

    #[test]
    fn fresh_liquidity_uses_the_nearby_median_and_spoofing_requires_an_individual_order() {
        for (name, size, milliseconds, execute, expected) in [
            ("qualifies", 100.0, 2_000, 9.0, true),
            ("too old", 100.0, 2_001, 9.0, false),
            ("ten percent executed", 100.0, 1_000, 10.0, false),
            ("too small", 3.0, 1_000, 0.0, false),
        ] {
            let host = Host::new(Some(1_000.0));
            let mut settings = host.detectors.settings();
            settings.liquidity.enabled = true;
            host.detectors.set_settings(settings);
            host.detectors.push(
                "ES",
                time(0),
                &MarketEvent::OrderSnapshot {
                    sequence: 1,
                    orders: vec![
                        RestingOrder::new(1, OrderSide::Buy, 99.0, 1.0),
                        RestingOrder::new(2, OrderSide::Sell, 101.0, 1.0),
                    ],
                },
            );
            host.detectors.push(
                "ES",
                time(0),
                &MarketEvent::Order {
                    sequence: 2,
                    event: OrderEvent::Add(RestingOrder::new(3, OrderSide::Buy, 99.25, size)),
                },
            );
            let fresh = of_kind(host.events(), "Fresh liquidity");
            // Median [1, 1, size] is 1, threshold 4. The aggregate size cannot identify a spoof.
            assert_eq!(fresh.len(), usize::from(size >= 4.0), "{name}");
            if let Some(signal) = fresh.first() {
                assert!(
                    signal
                        .tooltip
                        .as_deref()
                        .unwrap()
                        .contains("largest resting order 3")
                );
            }
            let mut seq = 3;
            if execute > 0.0 {
                host.detectors.push(
                    "ES",
                    time(0),
                    &MarketEvent::Order {
                        sequence: seq,
                        event: OrderEvent::Execute {
                            id: 3,
                            size: execute,
                            trade: None,
                        },
                    },
                );
                seq += 1;
            }
            host.detectors.push(
                "ES",
                time(0) + chrono::TimeDelta::milliseconds(milliseconds),
                &MarketEvent::Order {
                    sequence: seq,
                    event: OrderEvent::Cancel { id: 3 },
                },
            );
            let events = host.events();
            assert_eq!(
                of_kind(events.clone(), "Spoofing").len(),
                usize::from(expected),
                "{name}"
            );
            if expected {
                let pulls = of_kind(events, "Pulled liquidity");
                assert_eq!(pulls.len(), 1);
                assert_eq!(
                    pulls[0].label, None,
                    "the coincident spoof supplies the label"
                );
            }
        }
        let host = Host::new(Some(1_000.0));
        let mut settings = host.detectors.settings();
        settings.liquidity.enabled = true;
        host.detectors.set_settings(settings);
        host.push(MarketEvent::DepthSnapshot {
            sequence: 1,
            book: depth_book(&[(99.0, 1.0)], &[(101.0, 1.0)]),
        });
        let added = host.push(MarketEvent::DepthDelta {
            sequence: 2,
            book: depth_book(&[(99.25, 100.0)], &[]),
        });
        assert_eq!(of_kind(added, "Fresh liquidity").len(), 1);
        let removed = host.push(MarketEvent::DepthDelta {
            sequence: 3,
            book: depth_book(&[(99.25, 0.0)], &[]),
        });
        assert!(of_kind(removed.clone(), "Spoofing").is_empty());
        assert_eq!(of_kind(removed, "Pulled liquidity").len(), 1);
    }
}
