//! A signal service that finds order-flow events in the executions and depth the host streams.

mod big_trades;

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Mutex;

use chrono::{DateTime, Utc};

use super::{Events, SignalEvent, Signals};
use crate::data::depth::Book;
use crate::data::{FeedError, MarketEvent, OrderQueue};
use crate::model::{BarClock, Instrument, Signal, Tick, Timeframe, TimezoneId};
pub use big_trades::BigTradeRule;
use big_trades::BigTrades;

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
/// Trades that are not finite numbers or have a negative size are ignored. [`push_history`](Self::push_history) runs loaded trades
/// through the same detection.
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
}

impl DetectionKind {
    /// The signal category of the kind's detections.
    pub fn category(self) -> &'static str {
        match self {
            Self::BigTrade => "Big trades",
            Self::BigTradeZone => "Big trade zones",
        }
    }
}

/// The settings of every detector, applied by [`OrderFlowDetectors::set_settings`].
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct DetectorSettings {
    /// Large aggressive executions and their zones.
    pub big_trades: Detector<BigTradeRule>,
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
    /// Ids of the latest trades, oldest first, and the same as a set.
    trade_ids: VecDeque<u64>,
    seen_ids: HashSet<u64>,
    detections: VecDeque<Signal>,
    big_trades: BigTrades,
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

    /// Replaces the settings of every detector from the next event on. Detections already made
    /// stay. Panics if a fixed threshold is not positive and finite.
    pub fn set_settings(&self, settings: DetectorSettings) {
        if let Threshold::Fixed(size) = settings.big_trades.rule.threshold {
            assert!(
                size.is_finite() && size > 0.0,
                "fixed threshold must be positive and finite"
            );
        }
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
            MarketEvent::Trade(trade) => tracked.trade(trade, settings, &mut out),
            MarketEvent::Quote(_) => {}
            MarketEvent::DepthSnapshot { sequence, book } => {
                tracked.orders = None;
                let _ = tracked.book.snapshot(*sequence, book.clone());
            }
            MarketEvent::DepthDelta { sequence, book } => {
                tracked.orders = None;
                let _ = tracked.book.delta(*sequence, book);
            }
            MarketEvent::OrderSnapshot { sequence, orders } => {
                let tick = Some(tracked.tick);
                let queue = tracked.orders.get_or_insert_with(|| OrderQueue::new(tick));
                let _ = queue.set_snapshot(*sequence, time, orders.clone());
                for trade in queue.take_executions() {
                    tracked.trade(&trade, settings, &mut out);
                }
            }
            MarketEvent::Order { sequence, event } => {
                let tick = Some(tracked.tick);
                let queue = tracked.orders.get_or_insert_with(|| OrderQueue::new(tick));
                let _ = queue.event(*sequence, time, event.clone());
                for trade in queue.take_executions() {
                    tracked.trade(&trade, settings, &mut out);
                }
            }
        }
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
            tracked.trade(trade, settings, &mut out);
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
            ..
        } = &mut *state;
        let new = !subscriptions.contains_key(symbol);
        let subscription = subscriptions
            .entry(symbol.to_owned())
            .or_insert_with(|| Subscription {
                timeframe,
                streams: Vec::new(),
            });
        if let Some(tracked) = symbols.get_mut(symbol) {
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
            trade_ids: VecDeque::new(),
            seen_ids: HashSet::new(),
            detections: VecDeque::new(),
            big_trades: BigTrades::new(tick, instrument.volume_decimals),
        };
        let timeframe = subscription.as_ref().map(|s| s.timeframe);
        (tracked.big_trades).set_clock(timeframe.and_then(|tf| clock(instrument, tf)));
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
    fn trade(&mut self, trade: &Tick, settings: &DetectorSettings, out: &mut Vec<SignalEvent>) {
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
        let big = &settings.big_trades;
        (self.big_trades).trade(trade, &big.rule, big.enabled, out);
    }
}

/// Keeps `events` in the detections of `tracked`, if any, and pushes them to `subscription`.
fn deliver(
    tracked: Option<&mut Tracked>,
    subscription: Option<&mut Subscription>,
    mut events: Vec<SignalEvent>,
) {
    let mut removed = Vec::new();
    if let Some(tracked) = tracked {
        for event in &events {
            let detections = &mut tracked.detections;
            match event {
                SignalEvent::Upsert(signal) => {
                    match detections.iter().rposition(|kept| kept.id == signal.id) {
                        Some(at) => detections[at] = signal.clone(),
                        None => {
                            if detections.len() == RETAINED
                                && let Some(old) = detections.pop_front()
                            {
                                tracked.big_trades.forget(&old.id);
                                removed.push(SignalEvent::Remove(old.id));
                            }
                            detections.push_back(signal.clone());
                        }
                    }
                }
                SignalEvent::Remove(id) => detections.retain(|kept| kept.id != *id),
                SignalEvent::Clear => detections.clear(),
            }
        }
    }
    events.extend(removed);
    if let Some(subscription) = subscription {
        subscription.streams.retain(Events::is_active);
        for event in events {
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
    fn trades_that_become_large_after_their_first_bar_keep_their_zones() {
        let host = Host::new(Some(10.0));
        let mut settings = host.detectors.settings();
        settings.big_trades.rule.body_zones = false;
        settings.big_trades.rule.wick_zones = false;
        host.detectors.set_settings(settings);
        let trade = |second, price, size, id| {
            host.push(MarketEvent::Trade(fill(
                second,
                price,
                size,
                OrderSide::Buy,
                id,
            )))
        };
        assert!(trade(-1, 100.0, 1.0, None).is_empty());
        assert!(trade(0, 100.0, 4.0, Some(7)).is_empty());
        // The first bar has closed when the last six lots make ten, at VWAP 1006 / 10.
        assert_eq!(
            trade(60, 101.0, 6.0, Some(7)),
            [
                marker(1, 0, 100.6, Direction::Up, "10"),
                zone(1, 0, None, 100.0, 101.0)
            ]
        );
        assert!(trade(120, 100.0, 1.0, None).is_empty());
    }

    #[test]
    fn detections_past_the_retained_5000_leave_the_live_chart_too() {
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
                zone(2501, 1, None, 99.875, 100.125),
                SignalEvent::Remove(SignalId("big-trade-1".into())),
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
    fn empty_invalid_and_repeated_trades_leave_the_aggregate_alone() {
        let host = Host::new(Some(10.0));
        host.detectors.push_history("ES", &[]);
        assert!(host.events().is_empty());
        let trade = |price, size| fill(1, price, size, OrderSide::Buy, Some(7)).with_id(1);
        for (price, size) in [(f64::NAN, 6.0), (100.0, f64::NAN), (100.0, -1.0)] {
            assert!(host.push(MarketEvent::Trade(trade(price, size))).is_empty());
        }
        assert!(host.push(MarketEvent::Trade(trade(100.0, 6.0))).is_empty());
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
    }
}
