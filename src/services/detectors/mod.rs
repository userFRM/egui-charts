//! A signal service that finds order-flow events in the executions and depth the host streams.

mod absorption;
mod alerts;
mod big_trades;
mod depth;
mod footprint;
mod icebergs;
mod liquidity;
mod stops;

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Mutex;

use chrono::{DateTime, Utc};

use super::{Events, SignalEvent, Signals};
use crate::data::depth::Book;
use crate::data::{FeedError, MarketEvent, OrderQueue};
use crate::model::{
    Alert, AlertCondition, AlertStatus, BarClock, FootprintAlertRule, Instrument, Signal, Tick,
    Timeframe, TimezoneId,
};
use absorption::Absorption;
pub use absorption::AbsorptionRule;
pub use alerts::{CountAlert, CountWindow, DetectionAlert, DetectionCounts};
use alerts::{Counting, Delivery};
pub use big_trades::BigTradeRule;
use big_trades::BigTrades;
use depth::Depth;
pub use footprint::FootprintRule;
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
/// let mut settings = detectors.settings();
/// settings.big_trades.enabled = true;
/// settings.big_trades.rule.passive_orders = true;
/// detectors.set_settings(settings);
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

impl std::fmt::Debug for OrderFlowDetectors {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OrderFlowDetectors").finish_non_exhaustive()
    }
}

/// The detections the service makes, each drawn in its own signal category.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DetectionKind {
    /// A large trade, as a marker: see [`BigTradeRule`].
    BigTrade,
    /// The prices a large trade traded at, as a zone: see [`BigTradeRule`].
    BigTradeZone,
    /// A rapid sequence of same-side aggressive fills.
    StopRun,
    /// A closed bar meeting the stop sweep rule.
    StopSweep,
    /// A bar meeting a footprint condition: see [`FootprintRule`].
    FootprintCondition,
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
    /// Every detection category, in panel order.
    pub const ALL: [Self; 13] = [
        Self::BigTrade,
        Self::BigTradeZone,
        Self::StopRun,
        Self::StopSweep,
        Self::FootprintCondition,
        Self::Iceberg,
        Self::Absorption,
        Self::LargeLiquidity,
        Self::Wall,
        Self::LiquiditySweep,
        Self::PulledLiquidity,
        Self::Spoofing,
        Self::FreshLiquidity,
    ];

    /// The signal category of the kind's detections.
    pub fn category(self) -> &'static str {
        match self {
            Self::BigTrade => "Big trades",
            Self::BigTradeZone => "Big trade zones",
            Self::StopRun => "Stop runs",
            Self::StopSweep => "Stop sweeps",
            Self::FootprintCondition => "Footprint conditions",
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

/// The settings of every detector, disabled by default, applied by
/// [`OrderFlowDetectors::set_settings`].
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct DetectorSettings {
    /// Large trades and their zones.
    pub big_trades: Detector<BigTradeRule>,
    /// Rapid same-side execution bursts.
    pub stop_runs: Detector<StopRunRule>,
    /// Directional footprint bars with exceptional volume.
    pub stop_sweeps: Detector<StopSweepRule>,
    /// Footprint imbalances, stacked imbalances, and bar volume and delta levels. Settings
    /// saved without it restore it disabled.
    #[serde(default)]
    pub footprint: Detector<FootprintRule>,
    /// Exact order refills and estimates from aggregate depth.
    pub icebergs: Detector<IcebergRule>,
    /// Aggressive volume absorbed at a resting inside price.
    pub absorption: Detector<AbsorptionRule>,
    /// Large resting levels, walls, sweeps, pulls and fresh size.
    pub liquidity: Detector<LiquidityRule>,
    /// Optional rising-edge alert on the number of icebergs.
    pub iceberg_count_alert: Option<CountAlert>,
    /// Optional rising-edge alert on the number of stop runs.
    pub stop_run_count_alert: Option<CountAlert>,
}

impl DetectorSettings {
    /// Checks thresholds and minima before applying settings from storage or a user interface.
    pub fn validate(&self) -> Result<(), FeedError> {
        for threshold in [
            self.big_trades.rule.threshold,
            self.absorption.rule.threshold,
            self.liquidity.rule.threshold,
            self.liquidity.rule.fresh_threshold,
        ] {
            if let Threshold::Fixed(size) = threshold
                && (!size.is_finite() || size <= 0.0)
            {
                return Err(FeedError::InvalidData(
                    "fixed threshold must be positive and finite".into(),
                ));
            }
        }
        if self.icebergs.rule.refills == 0 {
            return Err(FeedError::InvalidData(
                "iceberg refills must be positive".into(),
            ));
        }
        let min_size = self.stop_runs.rule.min_size;
        if !min_size.is_finite() || min_size < 0.0 {
            return Err(FeedError::InvalidData(
                "stop run minimum size must be finite and not negative".into(),
            ));
        }
        for alert in [self.iceberg_count_alert, self.stop_run_count_alert]
            .into_iter()
            .flatten()
        {
            if alert.minimum == 0 {
                return Err(FeedError::InvalidData(
                    "count alert minimum must be positive".into(),
                ));
            }
        }
        let rule = &self.footprint.rule;
        for level in [rule.buy_volume_above, rule.sell_volume_above]
            .into_iter()
            .flatten()
        {
            if !level.is_finite() || level < 0.0 {
                return Err(FeedError::InvalidData(
                    "footprint volume level must be finite and not negative".into(),
                ));
            }
        }
        for level in [rule.delta_above, rule.delta_below].into_iter().flatten() {
            if !level.is_finite() {
                return Err(FeedError::InvalidData(
                    "footprint delta level must be finite".into(),
                ));
            }
        }
        Ok(())
    }
}

/// The settings of one detector, disabled by default with alerts and sounds off.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct Detector<R> {
    /// Whether the detector detects. A disabled detector still calibrates its automatic
    /// threshold.
    pub enabled: bool,
    /// Whether live detections should raise an alert. Detections from loaded history never do.
    pub alert: bool,
    /// Whether that alert should play a sound.
    pub sound: bool,
    /// What the detector detects. A rule saved as `null`, as rules without settings were,
    /// restores the default rule.
    #[serde(
        deserialize_with = "rule_or_default",
        bound(deserialize = "R: serde::Deserialize<'de> + Default")
    )]
    pub rule: R,
}

/// Reads a rule object without an option wrapper, or the unit form of an older settings file.
fn rule_or_default<'de, D, R>(deserializer: D) -> Result<R, D::Error>
where
    D: serde::Deserializer<'de>,
    R: serde::Deserialize<'de> + Default,
{
    struct Rule<R>(std::marker::PhantomData<R>);

    impl<'de, R: serde::Deserialize<'de> + Default> serde::de::Visitor<'de> for Rule<R> {
        type Value = R;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a rule object or unit")
        }

        fn visit_unit<E: serde::de::Error>(self) -> Result<R, E> {
            Ok(R::default())
        }

        fn visit_map<A: serde::de::MapAccess<'de>>(self, map: A) -> Result<R, A::Error> {
            R::deserialize(serde::de::value::MapAccessDeserializer::new(map))
        }
    }

    deserializer.deserialize_any(Rule(std::marker::PhantomData))
}

impl<R: Default> Default for Detector<R> {
    fn default() -> Self {
        Self {
            enabled: false,
            alert: false,
            sound: false,
            rule: R::default(),
        }
    }
}

/// The size or count from which a detector detects.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
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
    delivery: Delivery,
    footprint_events: VecDeque<crate::model::FootprintAlertEvent>,
    watched: Vec<Watched>,
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
    watches: Vec<Watch>,
    counting: Counting,
}

/// An interval and rule read for one symbol, or every tracked symbol for a watchlist alert.
#[derive(Clone, PartialEq)]
struct Watched {
    symbol: Option<String>,
    timeframe: Timeframe,
    rule: Option<FootprintAlertRule>,
}

/// The footprint bars of one watched interval and rule on a tracked symbol.
// shortcut: each watched interval and rule keeps its own copy of the up to 100,000 retained
// trades; share one buffer per symbol when hosts watch many distinct rules.
struct Watch {
    timeframe: Timeframe,
    rule: Option<FootprintAlertRule>,
    /// Every detector disabled, with the watched rule, or the service's for `rule: None`.
    settings: DetectorSettings,
    sweeps: StopSweeps,
}

impl Default for OrderFlowDetectors {
    fn default() -> Self {
        Self::new()
    }
}

impl OrderFlowDetectors {
    /// A service tracking no symbol, with detectors, alerts and sounds disabled.
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
    /// threshold is not positive and finite, a refill or count-alert minimum is zero, or a
    /// footprint level is not finite or a volume level is negative.
    pub fn set_settings(&self, settings: DetectorSettings) {
        settings.validate().expect("invalid detector settings");
        let mut state = self.lock();
        for tracked in state.symbols.values_mut() {
            for watch in &mut tracked.watches {
                if watch.rule.is_none() {
                    watch.settings.footprint.rule = settings.footprint.rule.clone();
                }
            }
        }
        state.settings = settings;
    }

    /// Detects on `event` of `symbol`, observed at feed time `time`. Events of untracked symbols
    /// are ignored.
    pub fn push(&self, symbol: &str, time: DateTime<Utc>, event: &MarketEvent) {
        let mut state = self.lock();
        let State {
            settings,
            symbols,
            subscriptions,
            delivery,
            footprint_events,
            ..
        } = &mut *state;
        let Some(tracked) = symbols.get_mut(symbol) else {
            return;
        };
        tracked.stop_sweeps.go_live(time);
        for watch in &mut tracked.watches {
            watch.sweeps.go_live(time);
        }
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
                    tracked.update_depth(
                        Depth::from_book(&tracked.book, tracked.tick),
                        time,
                        true,
                        None,
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
                let replayed = tracked
                    .book
                    .sequence()
                    .is_some_and(|last| *sequence <= last);
                if tracked.book.delta(*sequence, book).is_err() {
                    tracked.lose_depth(&mut out);
                } else if !replayed {
                    tracked.update_depth(
                        Depth::from_book(&tracked.book, tracked.tick),
                        time,
                        false,
                        Some(book),
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
                    tracked.update_depth(depth, time, true, None, settings, &mut out);
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
                    tracked.update_depth(depth, time, false, None, settings, &mut out);
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
        tracked.stop_sweeps.advance(time, settings, &mut out);
        tracked.stop_sweeps.met.clear();
        for watch in &mut tracked.watches {
            watch.sweeps.advance(time, &watch.settings, &mut Vec::new());
            for met in watch.sweeps.met.drain(..) {
                footprint_events.push_back(crate::model::FootprintAlertEvent {
                    symbol: symbol.to_owned(),
                    exchange: tracked.instrument.exchange.clone(),
                    timeframe: watch.timeframe,
                    rule: watch.rule,
                    condition: met.condition,
                    bar: met.bar,
                    closed: met.closed,
                    time: met.time,
                    price: met.price,
                    message: met.message,
                });
                if footprint_events.len() > RETAINED {
                    footprint_events.pop_front();
                }
            }
        }
        tracked
            .counting
            .advance(time, settings, true, symbol, delivery);
        tracked.record(&out, time, true, settings, delivery, &mut HashSet::new());
        deliver(Some(tracked), subscriptions.get_mut(symbol), out);
    }

    /// Drains footprint conditions met by live bars of the intervals and rules that
    /// [`watch_footprint_alerts`](Self::watch_footprint_alerts) reads, oldest first, for
    /// `AlertEngine::footprint_events`. A forming bar reports each condition once, at the
    /// execution that first meets it; a closed bar reports each condition it meets when the
    /// event that closes it arrives. The footprint rule's ratio, comparison, minimum volume,
    /// stack and rows apply even with the detector or its switches disabled: those choose
    /// chart markers and panel alerts only. Loaded history, bars opened before the first live
    /// event's bar and the first retained bar report nothing. Trade-based intervals are not
    /// read, and an interval and rule newly read report nothing for the trades before it.
    /// At most the newest 5,000 events are kept; drain them every frame.
    pub fn take_footprint_events(&self) -> Vec<crate::model::FootprintAlertEvent> {
        self.lock().footprint_events.drain(..).collect()
    }

    /// Reads the footprint conditions of `alerts` for
    /// [`take_footprint_events`](Self::take_footprint_events): each active
    /// `AlertCondition::Footprint` alert with a time-based interval, on the bars of that interval
    /// and with the rule of its `AlertInputs::footprint`, or this service's footprint rule when
    /// its inputs hold none, independently of chart subscriptions. A watchlist alert is read
    /// on every tracked symbol. Call whenever alerts change, for example every frame with
    /// `AlertEngine::alerts`: an interval and rule already read keep their bars, and a new one
    /// starts from retained trades without reporting them, as an interval change does.
    pub fn watch_footprint_alerts(&self, alerts: &[Alert]) {
        let mut watched = Vec::new();
        for alert in alerts {
            if !matches!(alert.condition, AlertCondition::Footprint(_))
                || alert.status != AlertStatus::Active
            {
                continue;
            }
            let Some(timeframe) = alert.timeframe.filter(|tf| !tf.is_trade_based()) else {
                continue;
            };
            let key = Watched {
                symbol: alert.watchlist.is_none().then(|| alert.symbol.clone()),
                timeframe,
                rule: alert.inputs.as_deref().and_then(|inputs| inputs.footprint),
            };
            if !watched.contains(&key) {
                watched.push(key);
            }
        }
        let mut state = self.lock();
        if state.watched == watched {
            return;
        }
        state.watched = watched;
        let State {
            watched,
            symbols,
            settings,
            ..
        } = &mut *state;
        for tracked in symbols.values_mut() {
            tracked.watch(watched, settings);
        }
    }

    /// Detects on the loaded trade history of `symbol`, in time order, before the live trades
    /// that follow it. Its detections are made and pushed like live ones and warm the automatic
    /// thresholds and counts without raising alerts; trades with an id already seen are dropped.
    pub fn push_history(&self, symbol: &str, trades: &[Tick]) {
        let mut state = self.lock();
        let State {
            settings,
            symbols,
            subscriptions,
            delivery,
            ..
        } = &mut *state;
        let Some(tracked) = symbols.get_mut(symbol) else {
            return;
        };
        let mut out = Vec::new();
        let mut added = HashSet::new();
        for trade in trades {
            let from = out.len();
            tracked.trade(trade, settings, false, &mut out);
            tracked
                .counting
                .advance(trade.ts, settings, false, symbol, delivery);
            tracked.record(
                &out[from..],
                trade.ts,
                false,
                settings,
                delivery,
                &mut added,
            );
        }
        tracked.stop_sweeps.met.clear();
        for watch in &mut tracked.watches {
            watch.sweeps.met.clear();
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
            if new || subscription.timeframe != timeframe {
                let mut changes: Vec<_> = tracked
                    .detections
                    .iter()
                    .filter(|signal| {
                        signal.category == DetectionKind::StopSweep.category()
                            || signal.category == DetectionKind::FootprintCondition.category()
                    })
                    .map(|signal| SignalEvent::Remove(signal.id.clone()))
                    .collect();
                tracked.stop_sweeps.set_clock(
                    clock(&tracked.instrument, timeframe),
                    settings,
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
            icebergs: Icebergs::new(instrument.volume_decimals),
            absorption: Absorption::new(instrument.volume_decimals),
            liquidity: Liquidity::new(instrument.volume_decimals),
            trade_ids: VecDeque::new(),
            seen_ids: HashSet::new(),
            detections: VecDeque::new(),
            big_trades: BigTrades::new(tick, instrument.volume_decimals),
            stop_runs: StopRuns::new(instrument.volume_decimals),
            stop_sweeps: StopSweeps::new(tick, instrument.volume_decimals),
            watches: Vec::new(),
            counting: Counting::default(),
        };
        let timeframe = subscription.as_ref().map(|s| s.timeframe);
        (tracked.big_trades).set_clock(timeframe.and_then(|tf| clock(instrument, tf)));
        tracked.stop_sweeps.set_clock(
            timeframe.and_then(|tf| clock(instrument, tf)),
            &self.settings,
            &mut Vec::new(),
        );
        tracked.watch(&self.watched, &self.settings);
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
    fn watch(&mut self, watched: &[Watched], settings: &DetectorSettings) {
        let mut wanted = Vec::new();
        for key in watched {
            if key
                .symbol
                .as_ref()
                .is_none_or(|symbol| *symbol == self.instrument.symbol)
                && !wanted.contains(&(key.timeframe, key.rule))
            {
                wanted.push((key.timeframe, key.rule));
            }
        }
        self.watches
            .retain(|watch| wanted.contains(&(watch.timeframe, watch.rule)));
        for (timeframe, rule) in wanted {
            if self
                .watches
                .iter()
                .any(|watch| watch.timeframe == timeframe && watch.rule == rule)
            {
                continue;
            }
            let mut watch_settings = DetectorSettings::default();
            watch_settings.footprint.rule =
                rule.map_or_else(|| settings.footprint.rule.clone(), FootprintRule::reading);
            let sweeps = self
                .stop_sweeps
                .watching(clock(&self.instrument, timeframe), &watch_settings);
            self.watches.push(Watch {
                timeframe,
                rule,
                settings: watch_settings,
                sweeps,
            });
        }
    }

    fn lose_depth(&mut self, out: &mut Vec<SignalEvent>) {
        self.depth = Depth::default();
        self.icebergs.clear();
        self.absorption.clear();
        self.liquidity.clear(out);
    }

    fn update_depth(
        &mut self,
        depth: Depth,
        time: DateTime<Utc>,
        snapshot: bool,
        changes: Option<&crate::model::OrderBook>,
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
            changes,
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
        self.stop_runs.trade(
            trade,
            self.tick,
            &settings.stop_runs.rule,
            settings.stop_runs.enabled,
            out,
        );
        self.stop_sweeps.trade(trade, settings, out);
        for watch in &mut self.watches {
            watch.sweeps.trade(trade, &watch.settings, &mut Vec::new());
        }
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

    #[test]
    fn detectors_start_disabled_and_can_be_enabled_without_reconnecting() {
        let host = Host::new(Some(10.0));
        let settings = DetectorSettings::default();
        assert!(!settings.big_trades.enabled);
        assert!(!settings.stop_runs.enabled);
        assert!(!settings.stop_sweeps.enabled);
        assert!(!settings.icebergs.enabled);
        assert!(!settings.absorption.enabled);
        assert!(!settings.liquidity.enabled);
        host.detectors.set_settings(settings);
        let trade =
            |second, size| MarketEvent::Trade(fill(second, 100.0, size, OrderSide::Buy, None));
        assert!(host.push(trade(0, 100.0)).is_empty());
        assert!(host.detectors.take_alerts().is_empty());
        let mut settings = host.detectors.settings();
        settings.big_trades.enabled = true;
        settings.big_trades.alert = true;
        settings.big_trades.rule.threshold = Threshold::Fixed(10.0);
        host.detectors.set_settings(settings);
        assert!(!host.push(trade(1, 10.0)).is_empty());
        let alerts = host.detectors.take_alerts();
        assert_eq!(alerts.len(), 1);
        assert!(!alerts[0].sound);
    }

    #[test]
    fn detection_counts_follow_feed_windows_without_alerting_loaded_history() {
        use crate::services::{Notification, Notifications};

        let host = Host::new(Some(10.0));
        let mut settings = host.detectors.settings();
        settings.big_trades.alert = true;
        settings.big_trades.sound = true;
        host.detectors.set_settings(settings);
        let inbox = Arc::new(Inbox::default());
        let (_listener, events) = Listener::new::<Notification>(&inbox, 1);
        host.detectors.watch(events);
        host.detectors.push_history(
            "ES",
            &[
                fill(0, 100.0, 10.0, OrderSide::Buy, None),
                fill(270, 100.0, 10.0, OrderSide::Buy, None),
            ],
        );
        assert!(host.detectors.take_alerts().is_empty());
        assert!(inbox.drain().answers.is_empty());
        // At 299 both are within five minutes; at 300 the first and the 30-second
        // boundary expire. These feed times are unrelated to the test's wall clock.
        for (now, minutes, seconds) in [(299, 2, 1), (300, 1, 0), (570, 0, 0)] {
            assert_eq!(
                host.detectors
                    .counts("ES", DetectionKind::BigTrade, time(now)),
                DetectionCounts {
                    total: 2,
                    last_five_minutes: minutes,
                    last_thirty_seconds: seconds
                }
            );
        }
        host.detectors.push(
            "ES",
            time(571),
            &MarketEvent::Trade(fill(571, 100.0, 10.0, OrderSide::Buy, None).with_id(7)),
        );
        host.detectors.push(
            "ES",
            time(572),
            &MarketEvent::Trade(fill(571, 100.0, 10.0, OrderSide::Buy, None).with_id(7)),
        );
        let alerts = host.detectors.take_alerts();
        assert_eq!(
            alerts.iter().map(|alert| alert.kind).collect::<Vec<_>>(),
            [DetectionKind::BigTrade],
            "a big trade alerts once, not again for its zone"
        );
        assert!(alerts.iter().all(|alert| alert.sound
            && alert.notification.symbol.as_deref() == Some("ES")
            && !alert.notification.title.is_empty()
            && !alert.notification.body.is_empty()));
        let notifications: Vec<_> = inbox
            .drain()
            .answers
            .into_iter()
            .map(|(_, event)| *event.downcast::<Notification>().unwrap())
            .collect();
        assert_eq!(
            notifications,
            alerts
                .iter()
                .map(|alert| alert.notification.clone())
                .collect::<Vec<_>>()
        );
        assert!(host.detectors.take_alerts().is_empty());
        let (_replay_listener, replay) = Listener::new::<Notification>(&inbox, 2);
        host.detectors.watch(replay);
        assert_eq!(
            inbox.drain().answers.len(),
            1,
            "late subscribers receive the live log only"
        );
        assert_eq!(
            host.detectors
                .counts("ES", DetectionKind::BigTrade, time(572))
                .total,
            3
        );
        host.detectors.reset("ES");
        assert_eq!(
            host.detectors
                .counts("ES", DetectionKind::BigTrade, time(572)),
            DetectionCounts::default()
        );
    }

    #[test]
    fn stop_count_alerts_rearm_only_below_the_minimum_and_ignore_signal_updates() {
        for window in [
            CountWindow::ThirtySeconds,
            CountWindow::FiveMinutes,
            CountWindow::Total,
        ] {
            for history_bursts in [1, 2] {
                let host = Host::new(Some(1_000.0));
                let mut settings = host.detectors.settings();
                settings.stop_runs.enabled = true;
                settings.stop_run_count_alert = Some(CountAlert {
                    window,
                    minimum: 2,
                    sound: true,
                });
                host.detectors.set_settings(settings);
                let burst = |second| {
                    (0..6)
                        .map(|i| {
                            Tick::new(
                                time(second) + chrono::TimeDelta::milliseconds(i * 10),
                                100.0 + i as f64 * 0.25,
                                1.0,
                                Some(OrderSide::Buy),
                            )
                        })
                        .collect::<Vec<_>>()
                };
                for second in 0..history_bursts {
                    host.detectors.push_history("ES", &burst(second));
                }
                assert!(host.detectors.take_alerts().is_empty());
                for second in [2, 3] {
                    for trade in burst(second) {
                        host.detectors
                            .push("ES", trade.ts, &MarketEvent::Trade(trade));
                    }
                }
                let alerts = host.detectors.take_alerts();
                assert_eq!(alerts.len(), usize::from(history_bursts == 1), "{window:?}");
                if let Some(alert) = alerts.first() {
                    assert_eq!(alert.count, Some((window, 2)));
                    assert!(alert.sound);
                }
                assert_eq!(
                    host.detectors
                        .counts("ES", DetectionKind::StopRun, time(4))
                        .total,
                    history_bursts as u64 + 2
                );
                // No events between the bursts: expiration must rearm before the next detection.
                for second in [303, 304] {
                    for trade in burst(second) {
                        host.detectors
                            .push("ES", trade.ts, &MarketEvent::Trade(trade));
                    }
                }
                let alerts = host.detectors.take_alerts();
                assert_eq!(
                    alerts.len(),
                    usize::from(window != CountWindow::Total),
                    "{window:?}"
                );
                assert_eq!(
                    host.detectors
                        .counts("ES", DetectionKind::StopRun, time(305))
                        .total,
                    history_bursts as u64 + 4
                );
            }
        }
    }

    pub(super) fn time(seconds: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(seconds, 0).unwrap()
    }

    pub(super) fn fill(
        seconds: i64,
        price: f64,
        size: f64,
        side: OrderSide,
        aggressor: Option<u64>,
    ) -> Tick {
        Tick {
            aggressor_order_id: aggressor,
            ..Tick::new(time(seconds), price, size, Some(side))
        }
    }

    pub(super) fn marker(
        number: u64,
        at: i64,
        price: f64,
        direction: Direction,
        label: &str,
    ) -> SignalEvent {
        SignalEvent::Upsert(Signal {
            id: SignalId(format!("big-trade-{number}")),
            shape: SignalShape::Marker {
                time: time(at),
                price: Some(price),
                direction,
            },
            category: "Big trades".into(),
            label: Some(label.into()),
            tooltip: None,
        })
    }

    pub(super) fn zone(
        number: u64,
        from: i64,
        to: Option<i64>,
        low: f64,
        high: f64,
    ) -> SignalEvent {
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

    /// A service tracking "ES", with a fixed big trade threshold of `threshold` unless `None`.
    pub(super) struct Host {
        pub(super) detectors: OrderFlowDetectors,
        inbox: Arc<Inbox>,
        listeners: Vec<Listener>,
        timeframe: Timeframe,
    }

    impl Host {
        /// Tick 0.25, subscribed at one minute.
        fn new(threshold: Option<f64>) -> Self {
            let mut instrument = Instrument::new("ES").with_volume_decimals(0);
            instrument.tick_size = Some(0.25);
            Self::tracking(&instrument, Timeframe::Min1, threshold)
        }

        /// `instrument`, as "ES", subscribed at `timeframe`.
        pub(super) fn tracking(
            instrument: &Instrument,
            timeframe: Timeframe,
            threshold: Option<f64>,
        ) -> Self {
            let detectors = OrderFlowDetectors::new();
            detectors.track(instrument).unwrap();
            let mut settings = DetectorSettings::default();
            settings.big_trades.enabled = true;
            if let Some(size) = threshold {
                settings.big_trades.rule.threshold = Threshold::Fixed(size);
            }
            detectors.set_settings(settings);
            let mut host = Self {
                detectors,
                inbox: Arc::new(Inbox::default()),
                listeners: Vec::new(),
                timeframe,
            };
            host.subscribe();
            host
        }

        fn subscribe(&mut self) {
            let (listener, events) = Listener::new(&self.inbox, 1);
            self.listeners.push(listener);
            self.detectors.subscribe("ES", self.timeframe, events);
        }

        /// Pushes `event` and returns the signal events pushed since the last call.
        pub(super) fn push(&self, event: MarketEvent) -> Vec<SignalEvent> {
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
                marker(1, 1, 100.875, Direction::Up, "16 aggressive"),
                zone(1, 1, None, 100.0, 101.0)
            ]
        );
        assert_eq!(
            trade(self::fill(3, 101.0, 16.0, Buy, Some(7))),
            [
                marker(1, 1, 100.9375, Direction::Up, "32 aggressive"),
                zone(1, 1, None, 100.0, 101.0)
            ]
        );
        // A sell below the buy zone ends it; its own zone is one tick around its single price.
        assert_eq!(
            trade(self::fill(4, 99.0, 12.0, Sell, None)),
            [
                zone(1, 1, Some(4), 100.0, 101.0),
                marker(2, 4, 99.0, Direction::Down, "12 aggressive"),
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
        for kind in [
            DetectionKind::BigTrade,
            DetectionKind::StopRun,
            DetectionKind::Iceberg,
        ] {
            let host = Host::new(Some(10.0));
            let mut settings = host.detectors.settings();
            settings.stop_runs.enabled = kind == DetectionKind::StopRun;
            settings.stop_runs.alert = true;
            settings.icebergs.enabled = kind == DetectionKind::Iceberg;
            settings.icebergs.alert = true;
            settings.big_trades.alert = kind == DetectionKind::BigTrade;
            settings.liquidity.enabled = kind == DetectionKind::BigTrade;
            settings.liquidity.rule.threshold = Threshold::Fixed(f64::MAX);
            settings.liquidity.rule.fresh_threshold = Threshold::Fixed(1.0);
            host.detectors.set_settings(settings);
            let push = |ms, event| {
                host.detectors
                    .push("ES", time(0) + chrono::TimeDelta::milliseconds(ms), &event)
            };
            if kind == DetectionKind::BigTrade {
                push(
                    0,
                    MarketEvent::Trade(fill(0, 100.0, 10.0, OrderSide::Buy, Some(7))),
                );
            } else if kind == DetectionKind::StopRun {
                for i in 0..5 {
                    let ts = time(0) + chrono::TimeDelta::milliseconds(i * 10);
                    push(
                        i * 10,
                        MarketEvent::Trade(Tick::new(
                            ts,
                            100.0 + i as f64 * 0.25,
                            1.0,
                            Some(OrderSide::Buy),
                        )),
                    );
                }
            } else {
                push(
                    0,
                    MarketEvent::OrderSnapshot {
                        sequence: 1,
                        orders: vec![RestingOrder::new(1, OrderSide::Sell, 100.0, 10.0)],
                    },
                );
                for i in 0..2 {
                    push(
                        i * 20,
                        MarketEvent::Order {
                            sequence: 2 + i as u64 * 2,
                            event: OrderEvent::Execute {
                                id: 1 + i as u64,
                                size: 10.0,
                                trade: None,
                            },
                        },
                    );
                    push(
                        i * 20 + 10,
                        MarketEvent::Order {
                            sequence: 3 + i as u64 * 2,
                            event: OrderEvent::Add(RestingOrder::new(
                                2 + i as u64,
                                OrderSide::Sell,
                                100.0,
                                10.0,
                            )),
                        },
                    );
                }
            }
            assert_eq!(host.detectors.take_alerts().len(), 1, "{kind:?}");
            let ts = time(0) + chrono::TimeDelta::milliseconds(45);
            if kind == DetectionKind::BigTrade {
                // Depth detections evict the marker without ending its aggressor's fills.
                for sequence in 1..=5001 {
                    let book = OrderBook {
                        ts,
                        symbol: "ES".into(),
                        bids: vec![OrderBookLevel {
                            price: 99.0,
                            size: sequence as f64,
                            order_cnt: None,
                        }],
                        asks: Vec::new(),
                    };
                    push(
                        45,
                        if sequence == 1 {
                            MarketEvent::DepthSnapshot { sequence, book }
                        } else {
                            MarketEvent::DepthDelta { sequence, book }
                        },
                    );
                }
                push(
                    50,
                    MarketEvent::Trade(fill(0, 100.0, 5.0, OrderSide::Buy, Some(7))),
                );
            } else {
                host.detectors
                    .push_history("ES", &vec![Tick::new(ts, 100.0, 10.0, None); 2501]);
            }
            if kind == DetectionKind::StopRun {
                push(
                    50,
                    MarketEvent::Trade(Tick::new(
                        time(0) + chrono::TimeDelta::milliseconds(50),
                        101.25,
                        1.0,
                        Some(OrderSide::Buy),
                    )),
                );
            } else if kind == DetectionKind::Iceberg {
                push(
                    50,
                    MarketEvent::Order {
                        sequence: 6,
                        event: OrderEvent::Execute {
                            id: 3,
                            size: 10.0,
                            trade: None,
                        },
                    },
                );
                push(
                    60,
                    MarketEvent::Order {
                        sequence: 7,
                        event: OrderEvent::Add(RestingOrder::new(4, OrderSide::Sell, 100.0, 10.0)),
                    },
                );
            }
            assert_eq!(
                host.detectors.counts("ES", kind, time(1)).total,
                1,
                "an evicted {kind:?} keeps its detection identity"
            );
            assert!(
                host.detectors.take_alerts().is_empty(),
                "an update of an evicted {kind:?} stays silent"
            );
        }
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
        assert!(!replay.contains(&marker(1, 1, 50.0, Direction::Up, "10 aggressive")));
        assert!(!replay.contains(&marker(2, 1, 100.0, Direction::Up, "10 aggressive")));
        assert_eq!(live.len(), 5000, "live detections after one history page");
        let replay: HashMap<_, _> = replay
            .into_iter()
            .map(|event| match event {
                SignalEvent::Upsert(signal) => (signal.id.clone(), signal),
                _ => panic!("replay must contain only detections"),
            })
            .collect();
        assert_eq!(live, replay);
        for kind in [DetectionKind::BigTrade, DetectionKind::BigTradeZone] {
            assert_eq!(
                host.detectors.counts("ES", kind, time(2)).total,
                2501,
                "eviction and zone closure do not change running counts"
            );
        }

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
                marker(2501, 1, 100.0, Direction::Up, "10 aggressive"),
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
    fn invalid_settings_are_refused_and_saved_settings_restore() {
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
        for size in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0] {
            let mut settings = detectors.settings();
            settings.stop_runs.rule.min_size = size;
            assert!(
                std::panic::catch_unwind(|| detectors.set_settings(settings)).is_err(),
                "accepted stop run minimum {size}"
            );
            assert_eq!(detectors.settings(), DetectorSettings::default());
        }
        for size in [f64::MIN_POSITIVE, 10.0, f64::MAX] {
            let mut settings = detectors.settings();
            settings.big_trades.rule.threshold = Threshold::Fixed(size);
            detectors.set_settings(settings.clone());
            assert_eq!(detectors.settings(), settings);
        }
        for (volume, delta) in [
            (f64::NAN, 0.0),
            (f64::INFINITY, 0.0),
            (-1.0, 0.0),
            (0.0, f64::NAN),
            (0.0, f64::NEG_INFINITY),
        ] {
            let mut settings = detectors.settings();
            settings.footprint.rule.sell_volume_above = Some(volume);
            settings.footprint.rule.delta_below = Some(delta);
            assert!(
                std::panic::catch_unwind(|| detectors.set_settings(settings)).is_err(),
                "accepted {volume}, {delta}"
            );
        }
        let mut settings = detectors.settings();
        settings.footprint.rule.buy_volume_above = Some(0.0);
        settings.footprint.rule.delta_above = Some(-5.0);
        detectors.set_settings(settings.clone());
        assert_eq!(detectors.settings(), settings);
        // Literal settings in the format saved before footprint conditions existed.
        let saved = r#"{
            "big_trades":{"enabled":false,"alert":false,"sound":false,"rule":{"threshold":"Auto","body_zones":true,"wick_zones":true}},
            "stop_runs":{"enabled":false,"alert":false,"sound":false,"rule":null},
            "stop_sweeps":{"enabled":false,"alert":false,"sound":false,"rule":null},
            "icebergs":{"enabled":false,"alert":false,"sound":false,"rule":{"refills":2}},
            "absorption":{"enabled":false,"alert":false,"sound":false,"rule":{"threshold":"Auto"}},
            "liquidity":{"enabled":false,"alert":false,"sound":false,"rule":{"threshold":"Auto","fresh_threshold":"Auto"}},
            "iceberg_count_alert":null,"stop_run_count_alert":null
        }"#;
        assert_eq!(
            serde_json::from_str::<DetectorSettings>(saved).unwrap(),
            DetectorSettings::default()
        );
        // Footprint settings saved before rows could group restore with rows of one tick.
        let saved = saved.replacen(
            "\"iceberg_count_alert\"",
            r#""footprint":{"enabled":true,"alert":false,"sound":false,"rule":{
                "buy_imbalance":true,"sell_imbalance":false,
                "stacked_buy_imbalance":false,"stacked_sell_imbalance":false,
                "buy_zone_rejection":false,"sell_zone_rejection":false,
                "buy_volume_above":null,"sell_volume_above":null,
                "delta_above":null,"delta_below":null,
                "imbalance_ratio":3.0,"diagonal":true,"imbalance_min_volume":0.0,"stack":3
            }},
            "iceberg_count_alert""#,
            1,
        );
        let mut expected = DetectorSettings::default();
        expected.footprint.enabled = true;
        expected.footprint.rule.buy_imbalance = true;
        assert_eq!(
            serde_json::from_str::<DetectorSettings>(&saved).unwrap(),
            expected
        );
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
                marker(1, 1, 100.0, Direction::Neutral, "10 aggressive"),
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
            marker(1, 2000, 100.0, Direction::Up, "990 aggressive")
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
        assert_eq!(
            detections[0],
            marker(1, 1, 100.0, Direction::Up, "10 aggressive")
        );

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
                marker(1, 2, 100.0, Direction::Up, "12 aggressive"),
                zone(1, 2, None, 99.875, 100.125)
            ],
            "the validated fill precedes the invalid cancel in the snapshot buffer"
        );
        assert!(
            host.push(MarketEvent::Trade(trade)).is_empty(),
            "the tape copy counts once"
        );

        let host = Host::new(Some(1_000.0));
        let mut settings = host.detectors.settings();
        settings.icebergs.enabled = true;
        host.detectors.set_settings(settings);
        host.push(MarketEvent::DepthDelta {
            sequence: 2,
            book: depth_book(&[], &[(100.0, 10.0)]),
        });
        host.push(MarketEvent::DepthSnapshot {
            sequence: 1,
            book: depth_book(&[], &[(100.0, 10.0)]),
        });
        host.push(MarketEvent::Trade(fill(
            0,
            100.0,
            30.0,
            OrderSide::Buy,
            None,
        )));
        for sequence in [2, 3] {
            let signals = of_kind(
                host.push(MarketEvent::DepthDelta {
                    sequence,
                    book: depth_book(&[], &[(100.0, 10.0)]),
                }),
                "Icebergs",
            );
            assert_eq!(
                signals.len(),
                usize::from(sequence == 3),
                "only a new book update confirms the retained level"
            );
        }
    }

    #[test]
    fn stop_sweeps_use_closed_footprints_and_rebuild_on_interval_changes() {
        for (name, buy_size, close, extra_low, extra_sell, expected) in [
            ("qualifies", 10.0, 100.75, 0.0, 0.0, true),
            ("loaded history", 10.0, 100.75, 0.0, 0.0, true),
            ("live finish", 10.0, 100.75, 0.0, 0.0, true),
            ("ratio below three", 2.0, 100.75, 30.0, 0.0, false),
            ("small body", 10.0, 100.25, 0.0, 0.0, false),
            ("low volume", 3.0, 100.75, 0.0, 0.0, false),
            ("opposite delta", 10.0, 100.75, 0.0, 40.0, false),
            ("truncated executions", 10.0, 100.75, 0.0, 0.0, false),
        ] {
            let mut host = Host::new(Some(1_000.0));
            let mut settings = host.detectors.settings();
            settings.stop_sweeps.enabled = true;
            settings.stop_sweeps.alert = true;
            host.detectors.set_settings(settings);
            let push = |trade: Tick| {
                if matches!(name, "loaded history" | "live finish") {
                    host.detectors.push_history("ES", &[trade]);
                    host.events();
                } else {
                    host.push(MarketEvent::Trade(trade));
                }
            };
            // Eleven preceding bars: the first may be partial, the next ten each have volume 20.
            for minute in 0..11 {
                push(fill(minute * 60, 100.0, 20.0, OrderSide::Buy, None));
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
                push(fill(second, price, size, side, None));
            }
            if name == "truncated executions" {
                let trades: Vec<_> =
                    std::iter::repeat_n(fill(665, 100.25, 0.001, OrderSide::Buy, None), 100_001)
                        .collect();
                host.detectors.push_history("ES", &trades);
                host.events();
            }
            let last = fill(666, close, 0.1, OrderSide::Buy, None);
            if name == "loaded history" {
                host.detectors.push_history("ES", &[last]);
                host.events();
            } else {
                host.push(MarketEvent::Trade(last));
            }
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
            let alerts = host.detectors.take_alerts();
            assert_eq!(
                alerts.len(),
                usize::from(expected && name != "loaded history"),
                "{name}: only bars with live executions alert"
            );
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
            assert!(
                host.detectors.take_alerts().is_empty(),
                "{name}: rebuilt bars"
            );
        }
    }

    /// The third adjacent diagonal buy row completes the stack at second 65, before close.
    /// A sell imbalance disappears before close; repeated deliveries and recrosses stay silent.
    #[cfg(feature = "alert-engine")]
    #[test]
    fn footprint_alerts_fire_by_frequency_as_bars_form_and_close() {
        use crate::model::{
            Alert, AlertCondition, AlertFrequency as F, AlertStatus, FootprintCondition as C,
        };
        use crate::services::AlertEngine;
        use OrderSide::{Buy, Sell};
        let host = Host::new(None);
        host.detectors.set_settings(DetectorSettings::default());
        let engine = AlertEngine::new().with_alerts(
            [
                ("once", C::StackedBuyImbalance, F::Once),
                ("bar", C::StackedBuyImbalance, F::OncePerBar),
                ("close", C::StackedBuyImbalance, F::OncePerBarClose),
                ("sell bar", C::SellImbalance, F::OncePerBar),
                ("sell close", C::SellImbalance, F::OncePerBarClose),
            ]
            .map(|(name, condition, frequency)| {
                let mut alert = Alert::new("ES", AlertCondition::Footprint(condition));
                alert.name = name.into();
                alert.frequency = frequency;
                alert.timeframe = Some(Timeframe::Min1);
                alert
            })
            .to_vec(),
        );
        host.detectors.watch_footprint_alerts(&engine.alerts());
        let mut seen = HashSet::new();
        let mut fired = 0;
        for (second, price, size, side, names) in [
            (0, 100.0, 1.0, Buy, &[][..]),
            (60, 100.0, 1.0, Sell, &[][..]),
            (61, 100.25, 1.0, Sell, &["sell bar"][..]),
            (62, 100.5, 1.0, Sell, &[][..]),
            (63, 100.25, 12.0, Buy, &[][..]),
            (64, 100.5, 9.0, Buy, &[][..]),
            (65, 100.75, 15.0, Buy, &["once", "bar"][..]),
            (66, 100.5, 20.0, Sell, &[][..]),
            (67, 100.75, 60.0, Buy, &[][..]),
            (120, 100.0, 1.0, Sell, &["close"][..]),
            (121, 100.25, 1.0, Sell, &["sell bar"][..]),
            (122, 100.5, 1.0, Sell, &[][..]),
            (123, 100.25, 12.0, Buy, &[][..]),
            (124, 100.5, 9.0, Buy, &[][..]),
            (125, 100.75, 15.0, Buy, &["bar"][..]),
            (180, 100.5, 1.0, Sell, &["close"][..]),
        ] {
            host.detectors.push(
                "ES",
                time(second),
                &MarketEvent::Trade(fill(second, price, size, side, None)),
            );
            let events = host.detectors.take_footprint_events();
            for event in &events {
                assert!(
                    seen.insert((event.condition, event.bar, event.closed)),
                    "duplicate event at {second}: {event:?}"
                );
            }
            engine.footprint_events(&events, time(second));
            engine.footprint_events(&events, time(second));
            let triggers = engine.triggers();
            let actual: Vec<_> = triggers[fired..]
                .iter()
                .map(|trigger| {
                    (
                        trigger.name.as_str(),
                        trigger.time,
                        trigger.price,
                        trigger.message.as_str(),
                    )
                })
                .collect();
            let expected: Vec<_> = names
                .iter()
                .map(|&name| {
                    let (price, message) = if name == "sell bar" {
                        (100.25, "Sell imbalance on 1 row")
                    } else {
                        (100.75, "Stacked buy imbalance of 3 rows")
                    };
                    (name, time(second), Some(price), message)
                })
                .collect();
            assert_eq!(actual, expected, "second {second}");
            fired = triggers.len();
        }
        for alert in engine.alerts() {
            assert_eq!(
                alert.status,
                if alert.name == "once" {
                    AlertStatus::Triggered
                } else {
                    AlertStatus::Active
                }
            );
        }
        assert!(host.detectors.take_alerts().is_empty());
    }

    /// Simultaneous saved intervals, ratios and rows give the same notifications as isolated
    /// runs while chart subscriptions and the service rule change. Re-registering keeps bars.
    #[cfg(feature = "alert-engine")]
    #[test]
    fn footprint_alerts_keep_their_interval_and_rule_across_chart_subscriptions() {
        use crate::model::{AlertFrequency as F, AlertInputs, FootprintCondition as C};
        use crate::services::AlertEngine;
        use OrderSide::{Buy, Sell};
        let rule = FootprintAlertRule::default();
        let alerts = [
            (
                "stack",
                C::StackedBuyImbalance,
                F::OncePerBar,
                Timeframe::Min1,
                rule,
            ),
            (
                "stack close 5m",
                C::StackedBuyImbalance,
                F::OncePerBarClose,
                Timeframe::Min5,
                rule,
            ),
            ("buy", C::BuyImbalance, F::OncePerBar, Timeframe::Min1, rule),
            (
                "buy ratio 13",
                C::BuyImbalance,
                F::OncePerBar,
                Timeframe::Min1,
                FootprintAlertRule {
                    imbalance_ratio: 13.0,
                    ..rule
                },
            ),
            (
                "buy rows 2",
                C::BuyImbalance,
                F::OncePerBar,
                Timeframe::Min1,
                FootprintAlertRule {
                    row_ticks: 2,
                    ..rule
                },
            ),
        ]
        .map(|(name, condition, frequency, timeframe, rule)| {
            let mut alert = Alert::new("ES", AlertCondition::Footprint(condition));
            alert.name = name.into();
            alert.frequency = frequency;
            alert.timeframe = Some(timeframe);
            alert.inputs = Some(Box::new(AlertInputs {
                footprint: Some(rule),
                ..AlertInputs::default()
            }));
            serde_json::from_str(&serde_json::to_string(&alert).unwrap()).unwrap()
        });
        let executions = [
            (0, 100.0, 1.0, Buy),
            (360, 100.0, 1.0, Sell),
            (361, 100.25, 1.0, Sell),
            (362, 100.5, 1.0, Sell),
            (363, 100.25, 12.0, Buy),
            (364, 100.5, 9.0, Buy),
            (365, 100.75, 15.0, Buy),
            (420, 100.0, 1.0, Sell),
            (600, 100.0, 1.0, Buy),
        ];
        let run = |alerts: Vec<Alert>, churn: bool| {
            let mut host = Host::new(None);
            host.detectors.set_settings(DetectorSettings::default());
            let engine = AlertEngine::new().with_alerts(alerts);
            host.detectors.watch_footprint_alerts(&engine.alerts());
            let mut seen = HashSet::new();
            for (second, price, size, side) in executions {
                host.detectors.push(
                    "ES",
                    time(second),
                    &MarketEvent::Trade(fill(second, price, size, side, None)),
                );
                host.detectors.watch_footprint_alerts(&engine.alerts());
                let events = host.detectors.take_footprint_events();
                for event in &events {
                    let rule = event.rule.map(|rule| {
                        (
                            rule.imbalance_ratio.to_bits(),
                            rule.diagonal,
                            rule.imbalance_min_volume.to_bits(),
                            rule.stack,
                            rule.row_ticks,
                        )
                    });
                    assert!(
                        seen.insert((
                            event.timeframe,
                            rule,
                            event.condition,
                            event.bar,
                            event.closed
                        )),
                        "duplicate event at {second}: {event:?}"
                    );
                }
                engine.footprint_events(&events, time(second));
                engine.footprint_events(&events, time(second));
                if churn {
                    match second {
                        361 => {
                            host.timeframe = Timeframe::Min5;
                            host.subscribe();
                        }
                        363 => {
                            let mut settings = DetectorSettings::default();
                            settings.footprint.rule.imbalance_ratio = 100.0;
                            settings.footprint.rule.stack = 5;
                            settings.footprint.rule.row_ticks = 4;
                            host.detectors.set_settings(settings);
                        }
                        364 => {
                            host.timeframe = Timeframe::Min1;
                            host.subscribe();
                        }
                        _ => {}
                    }
                }
            }
            engine
                .triggers()
                .into_iter()
                .map(|trigger| (trigger.name, trigger.time, trigger.price, trigger.message))
                .collect::<Vec<_>>()
        };
        let actual = run(alerts.to_vec(), true);
        // Diagonal buys are 12/1, 9/1 and 15/1. Grouping two ticks makes the lower row's
        // sells 2, so the upper row first qualifies at 364 (9 >= 3 * 2). The 5m close
        // includes the sale at 420: its three buy rows still meet 12/2, 9/1 and 15/1.
        let expected = [
            ("buy", 363, 100.25, "Buy imbalance on 1 row"),
            ("buy rows 2", 364, 100.5, "Buy imbalance on 1 row"),
            ("stack", 365, 100.75, "Stacked buy imbalance of 3 rows"),
            ("buy ratio 13", 365, 100.75, "Buy imbalance on 1 row"),
            (
                "stack close 5m",
                600,
                100.0,
                "Stacked buy imbalance of 3 rows",
            ),
        ]
        .map(|(name, second, price, message)| {
            (
                name.to_owned(),
                time(second),
                Some(price),
                message.to_owned(),
            )
        });
        assert_eq!(actual, expected);
        for alert in alerts {
            let expected: Vec<_> = actual
                .iter()
                .filter(|(name, ..)| *name == alert.name)
                .cloned()
                .collect();
            assert_eq!(run(vec![alert], false), expected);
        }
    }

    #[test]
    fn footprint_conditions_alert_once_per_live_bar() {
        use OrderSide::{Buy, Sell};
        /// Feed second, price, size and side of an execution, and the alerts it raises as
        /// "title: body".
        type Step = (i64, f64, f64, OrderSide, &'static [&'static str]);
        // Bids of 1 at 100, 100.25 and 100.5 under asks of 12, 9 and 15 one row higher.
        let stacked = |closing: &'static [&'static str]| -> [Step; 8] {
            [
                (0, 100.0, 1.0, Buy, &[]),
                (60, 100.0, 1.0, Sell, &[]),
                (61, 100.25, 1.0, Sell, &[]),
                (62, 100.5, 1.0, Sell, &[]),
                (63, 100.25, 12.0, Buy, &[]),
                (64, 100.5, 9.0, Buy, &[]),
                (65, 100.75, 15.0, Buy, &[]),
                (120, 100.0, 1.0, Buy, closing),
            ]
        };
        let crossed: &'static [&'static str] = &[
            "Sell volume above: Sell volume 101 above 100",
            "Delta below: Delta -101 below -100",
        ];
        let selling: [Step; 6] = [
            (0, 100.0, 1.0, Sell, &[]),
            (60, 100.0, 60.0, Sell, &[]),
            (61, 100.0, 41.0, Sell, crossed),
            (62, 100.0, 5.0, Sell, &[]),
            (120, 100.0, 100.0, Sell, &[]),
            (121, 100.0, 1.0, Sell, crossed),
        ];
        let gapped: [Step; 5] = [
            (0, 100.0, 1.0, Buy, &[]),
            (60, 100.0, 12.0, Buy, &[]),
            (61, 100.25, 9.0, Buy, &[]),
            (62, 100.75, 15.0, Buy, &[]),
            (
                120,
                100.0,
                1.0,
                Buy,
                &["Buy imbalance: Buy imbalance on 3 rows"],
            ),
        ];
        // Bar 60's buy zone at 100.25 to 100.75: bar 120 rejects it, bar 180 closes below
        // it, ending it silently, and bar 240 trades into the ended zone without a signal.
        let rejection: [Step; 7] = [
            (
                120,
                100.5,
                1.0,
                Sell,
                &[
                    "Buy imbalance: Buy imbalance on 3 rows",
                    "Stacked buy imbalance: Stacked buy imbalance of 3 rows",
                ],
            ),
            (121, 100.75, 1.0, Buy, &[]),
            (
                180,
                100.25,
                1.0,
                Buy,
                &["Buy zone rejection: Buy zone rejection at 100.25 to 100.75"],
            ),
            (181, 100.0, 1.0, Sell, &[]),
            (240, 100.5, 1.0, Sell, &[]),
            (241, 100.75, 1.0, Buy, &[]),
            (300, 100.0, 1.0, Buy, &[]),
        ];
        // Bar 120 repeats bar 60's stack. Both later rejections name the shared bounds once.
        let repeated: [Step; 9] = [
            (
                120,
                100.0,
                1.0,
                Sell,
                &[
                    "Buy imbalance: Buy imbalance on 3 rows",
                    "Stacked buy imbalance: Stacked buy imbalance of 3 rows",
                ],
            ),
            (121, 100.25, 1.0, Sell, &[]),
            (122, 100.5, 1.0, Sell, &[]),
            (123, 100.25, 12.0, Buy, &[]),
            (124, 100.5, 9.0, Buy, &[]),
            (125, 100.75, 15.0, Buy, &[]),
            (
                180,
                100.5,
                1.0,
                Sell,
                &[
                    "Buy imbalance: Buy imbalance on 3 rows",
                    "Stacked buy imbalance: Stacked buy imbalance of 3 rows",
                    "Buy zone rejection: Buy zone rejection at 100.25 to 100.75",
                ],
            ),
            (181, 100.75, 1.0, Buy, &[]),
            (
                240,
                101.0,
                1.0,
                Buy,
                &["Buy zone rejection: Buy zone rejection at 100.25 to 100.75"],
            ),
        ];
        // At 100: 10 bought and 1 sold; at 100.25: 20 sold. Rows of 2 ticks hold 10 against 21.
        let example = |closing: &'static [&'static str]| -> [Step; 5] {
            [
                (0, 100.0, 1.0, Buy, &[]),
                (60, 100.0, 10.0, Buy, &[]),
                (61, 100.0, 1.0, Sell, &[]),
                (62, 100.25, 20.0, Sell, &[]),
                (120, 100.0, 1.0, Buy, closing),
            ]
        };
        // On rows of 2 ticks, bids of 1 at 100.5 and 101 under asks of 12, 9 and 15 one row
        // higher, making the buy zone 100.50 to 101.75. Bar 120 rejects it and bar 180 closes
        // below it, ending it silently. On one-tick rows the asks are 2 imbalances, not a stack.
        let grouped: [Step; 10] = [
            (0, 100.0, 1.0, Buy, &[]),
            (60, 100.0, 1.0, Sell, &[]),
            (61, 100.5, 1.0, Sell, &[]),
            (62, 101.0, 1.0, Sell, &[]),
            (63, 100.75, 12.0, Buy, &[]),
            (64, 101.25, 9.0, Buy, &[]),
            (65, 101.75, 15.0, Buy, &[]),
            (
                120,
                100.5,
                1.0,
                Sell,
                &[
                    "Buy imbalance: Buy imbalance on 3 rows",
                    "Stacked buy imbalance: Stacked buy imbalance of 3 rows",
                ],
            ),
            (
                180,
                100.25,
                1.0,
                Buy,
                &["Buy zone rejection: Buy zone rejection at 100.50 to 101.75"],
            ),
            (240, 100.5, 1.0, Buy, &[]),
        ];
        let settings = |minimum: f64| {
            let mut settings = DetectorSettings::default();
            settings.footprint.enabled = true;
            settings.footprint.alert = true;
            let rule = &mut settings.footprint.rule;
            rule.buy_imbalance = true;
            rule.sell_imbalance = true;
            rule.stacked_buy_imbalance = true;
            rule.stacked_sell_imbalance = true;
            rule.buy_zone_rejection = true;
            rule.sell_zone_rejection = true;
            rule.imbalance_min_volume = minimum;
            rule.buy_volume_above = Some(100.0);
            rule.sell_volume_above = Some(100.0);
            rule.delta_above = Some(100.0);
            rule.delta_below = Some(-100.0);
            settings
        };
        let alerts = |host: &Host| -> Vec<String> {
            (host.detectors.take_alerts().into_iter())
                .map(|alert| {
                    assert_eq!(alert.kind, DetectionKind::FootprintCondition);
                    format!("{}: {}", alert.notification.title, alert.notification.body)
                })
                .collect()
        };
        for (name, minimum, row_ticks, steps) in [
            (
                "stack",
                0.0,
                1,
                stacked(&[
                    "Buy imbalance: Buy imbalance on 3 rows",
                    "Stacked buy imbalance: Stacked buy imbalance of 3 rows",
                ])
                .to_vec(),
            ),
            (
                "minimum breaks the stack",
                10.0,
                1,
                stacked(&["Buy imbalance: Buy imbalance on 2 rows"]).to_vec(),
            ),
            ("levels", 0.0, 1, selling.to_vec()),
            ("horizontal gap breaks the stack", 0.0, 1, gapped.to_vec()),
            (
                "horizontal example, rows of 1 tick",
                0.0,
                1,
                example(&[
                    "Buy imbalance: Buy imbalance on 1 row",
                    "Sell imbalance: Sell imbalance on 1 row",
                ])
                .to_vec(),
            ),
            (
                "horizontal example, rows of 2 ticks",
                0.0,
                2,
                example(&[]).to_vec(),
            ),
            ("grouped stack and zone", 0.0, 2, grouped.to_vec()),
            (
                "zone rejection",
                0.0,
                1,
                [&stacked(&[])[..7], &rejection[..]].concat(),
            ),
            (
                "repeated stack",
                0.0,
                1,
                [&stacked(&[])[..7], &repeated[..]].concat(),
            ),
        ] {
            let mut host = Host::new(Some(1_000.0));
            let mut configured = settings(minimum);
            configured.footprint.rule.row_ticks = row_ticks;
            if name.starts_with("horizontal") {
                configured.footprint.rule.diagonal = false;
            }
            host.detectors.set_settings(configured);
            for (second, price, size, side, expected) in steps {
                let trade = MarketEvent::Trade(fill(second, price, size, side, None));
                host.detectors.push("ES", time(second), &trade);
                assert_eq!(alerts(&host), expected, "{name}, second {second}");
            }
            if name == "stack" {
                host.events();
                let (listener, events) = Listener::new(&host.inbox, 1);
                host.listeners.push(listener);
                host.detectors.subscribe("ES", Timeframe::Min5, events);
                let id = SignalId("footprint-stacked-buy-60000".into());
                assert!(host.events().contains(&SignalEvent::Remove(id)), "{name}");
                assert!(alerts(&host).is_empty(), "{name}: rebuilt five-minute bars");
                let (listener, events) = Listener::new(&host.inbox, 1);
                host.listeners.push(listener);
                host.detectors.subscribe("ES", Timeframe::Min1, events);
                host.detectors.push(
                    "ES",
                    time(121),
                    &MarketEvent::Trade(fill(121, 100.5, 1.0, Buy, None)),
                );
            }
        }
        // A closed bar skipped while disabled or after retention ends every active zone.
        // Disabling and enabling inside a bar still reads its close and keeps unbroken zones.
        for name in [
            "disabled across close",
            "first retained bar",
            "disabled inside bar",
        ] {
            let host = Host::new(None);
            let mut configured = DetectorSettings::default();
            configured.footprint.enabled = true;
            configured.footprint.alert = true;
            configured.footprint.rule.buy_zone_rejection = true;
            configured.footprint.rule.sell_zone_rejection = true;
            host.detectors.set_settings(configured.clone());
            let push = |second, price, size, side, expected: &[&str]| {
                let trade = MarketEvent::Trade(fill(second, price, size, side, None));
                host.detectors.push("ES", time(second), &trade);
                assert_eq!(alerts(&host), expected, "{name}, second {second}");
            };
            for &(second, price, size, side, expected) in &stacked(&[])[..7] {
                push(second, price, size, side, expected);
            }
            push(120, 100.5, 1.0, Sell, &[]);
            if name != "first retained bar" {
                configured.footprint.enabled = false;
                host.detectors.set_settings(configured.clone());
            }
            if name == "disabled inside bar" {
                push(121, 100.75, 1.0, Buy, &[]);
                configured.footprint.enabled = true;
                host.detectors.set_settings(configured);
                let rejected = &["Buy zone rejection: Buy zone rejection at 100.25 to 100.75"];
                push(180, 100.5, 1.0, Sell, rejected);
                push(181, 100.75, 1.0, Buy, &[]);
                push(240, 101.0, 1.0, Buy, rejected);
            } else {
                let fills = if name == "first retained bar" {
                    100_010
                } else {
                    1
                };
                for _ in 0..fills {
                    push(121, 99.0, 1.0, Sell, &[]);
                }
                push(180, 99.0, 1.0, Sell, &[]);
                configured.footprint.enabled = true;
                host.detectors.set_settings(configured);
                push(181, 100.25, 1.0, Buy, &[]);
                push(240, 100.5, 1.0, Buy, &[]);
            }
        }
        // A zone keeps its prices when the rows change: on one-tick rows, bar 120 trades into
        // the zone that rows of 2 ticks made, at 100.50 to 101.75, and closes inside it.
        let host = Host::new(None);
        let mut configured = DetectorSettings::default();
        configured.footprint.enabled = true;
        configured.footprint.alert = true;
        configured.footprint.rule.buy_zone_rejection = true;
        configured.footprint.rule.row_ticks = 2;
        host.detectors.set_settings(configured.clone());
        let push = |second, price, size, side, expected: &[&str]| {
            let trade = MarketEvent::Trade(fill(second, price, size, side, None));
            host.detectors.push("ES", time(second), &trade);
            assert_eq!(alerts(&host), expected, "rows change, second {second}");
        };
        for &(second, price, size, side, _) in &grouped[..8] {
            push(second, price, size, side, &[]);
        }
        configured.footprint.rule.row_ticks = 1;
        host.detectors.set_settings(configured);
        push(121, 100.75, 1.0, Buy, &[]);
        push(
            180,
            100.0,
            1.0,
            Buy,
            &["Buy zone rejection: Buy zone rejection at 100.50 to 101.75"],
        );
        // Loaded history, with 101 sold on its own row: marked, never alerted, also when the first
        // live event closes the last history bar.
        let host = Host::new(Some(1_000.0));
        let mut minute_alert = Alert::new(
            "ES",
            AlertCondition::Footprint(crate::model::FootprintCondition::StackedBuyImbalance),
        );
        minute_alert.timeframe = Some(Timeframe::Min1);
        host.detectors.watch_footprint_alerts(&[minute_alert]);
        host.detectors.set_settings(settings(0.0));
        let mut history: Vec<_> = stacked(&[])[..7]
            .iter()
            .map(|&(second, price, size, side, _)| fill(second, price, size, side, None))
            .collect();
        history.push(fill(66, 99.0, 101.0, Sell, None));
        host.detectors.push_history("ES", &history);
        assert!(alerts(&host).is_empty());
        let trade = MarketEvent::Trade(fill(120, 100.0, 1.0, Buy, None));
        host.detectors.push("ES", time(120), &trade);
        assert!(alerts(&host).is_empty());
        assert!(host.detectors.take_footprint_events().is_empty());
        // A history page overlapping a live bar must not leave queued live events.
        host.detectors.push_history(
            "ES",
            &[
                fill(121, 100.25, 1.0, Sell, None),
                fill(122, 100.5, 1.0, Sell, None),
            ],
        );
        host.detectors.push(
            "ES",
            time(123),
            &MarketEvent::Trade(fill(123, 100.5, 1.0, Sell, None)),
        );
        assert!(
            host.detectors.take_footprint_events().is_empty(),
            "overlapping history reports nothing"
        );
        let marked: Vec<_> = of_kind(host.events(), "Footprint conditions")
            .into_iter()
            .map(|signal| signal.id.0)
            .collect();
        assert_eq!(
            marked,
            [
                "footprint-sell-volume-60000",
                "footprint-buy-imbalance-60000",
                "footprint-stacked-buy-60000"
            ]
        );
        // An active bar can outlive chart marker retention. Recrossing still alerts only once.
        let host = Host::new(Some(1.0));
        let mut active = settings(0.0);
        active.big_trades.enabled = true;
        active.big_trades.rule.threshold = Threshold::Fixed(1.0);
        host.detectors.set_settings(active);
        for (second, size, side) in [(0, 1.0, Sell), (60, 101.0, Sell)] {
            host.push(MarketEvent::Trade(fill(second, 100.0, size, side, None)));
        }
        assert_eq!(alerts(&host).len(), 2);
        for _ in 0..=RETAINED {
            host.push(MarketEvent::Trade(fill(61, 100.0, 1.0, Buy, None)));
        }
        assert_eq!(alerts(&host).len(), 2);
        host.push(MarketEvent::Trade(fill(62, 100.0, 6_000.0, Sell, None)));
        assert!(alerts(&host).is_empty(), "a recross after marker eviction");
        assert_eq!(
            host.detectors
                .counts("ES", DetectionKind::FootprintCondition, time(62))
                .total,
            4
        );
        // A longer interval's bar is a new bar, even when it opens before the last one alerted.
        let mut host = Host::new(Some(1_000.0));
        host.detectors.set_settings(settings(0.0));
        for (index, (second, price, size, side, _)) in stacked(&[]).into_iter().enumerate() {
            let second = if index == 0 { second } else { second + 300 };
            let trade = MarketEvent::Trade(fill(second, price, size, side, None));
            host.detectors.push("ES", time(second), &trade);
        }
        assert_eq!(alerts(&host).len(), 2);
        host.timeframe = Timeframe::Min5;
        host.subscribe();
        let trade = MarketEvent::Trade(fill(600, 100.0, 1.0, Buy, None));
        host.detectors.push("ES", time(600), &trade);
        assert_eq!(
            alerts(&host),
            [
                "Buy imbalance: Buy imbalance on 3 rows",
                "Stacked buy imbalance: Stacked buy imbalance of 3 rows"
            ],
            "the five-minute bar opened at 300"
        );
    }

    /// On a daily chart with a session the detectors close the bars of trading dates, as the
    /// chart's candles and footprint date them: an evening session opens the next date's bar.
    #[cfg(feature = "timezones")]
    #[test]
    fn daily_detector_bars_are_trading_dates() {
        use crate::model::{Session, SessionWindow};
        use chrono::{NaiveTime, Weekday::*};
        let hour = |hour| NaiveTime::from_hms_opt(hour, 0, 0).unwrap();
        // A date trades from 17:00 the evening before to 16:00 in Chicago, 23:00 to 22:00 UTC.
        let futures = Instrument::new("ES")
            .with_volume_decimals(0)
            .with_tick_size(0.25)
            .with_time_zone(TimezoneId::named("America/Chicago").unwrap())
            .with_session(Session::new([SessionWindow::new(
                [Mon, Tue, Wed, Thu, Fri],
                hour(17),
                hour(16),
            )]));
        let trade = |time: &str, price, size, side| {
            MarketEvent::Trade(Tick::new(time.parse().unwrap(), price, size, side))
        };
        let host = Host::tracking(&futures, Timeframe::Day1, None);
        let mut settings = host.detectors.settings();
        settings.stop_sweeps.enabled = true;
        host.detectors.set_settings(settings);
        let mut events = Vec::new();
        // A buy of 20 on every weekday from 1 to 16 January 2024, the baseline volume.
        for day in [1, 2, 3, 4, 5, 8, 9, 10, 11, 12, 15, 16] {
            let time = format!("2024-01-{day:02}T14:00:00Z");
            events.extend(host.push(trade(&time, 100.0, 20.0, Some(OrderSide::Buy))));
        }
        // The qualifying sweep of the test above, from 17:10 on Tuesday 16 January: Wednesday's
        // date.
        for (minute, price, size, side) in [
            (10, 100.0, 1.0, OrderSide::Sell),
            (11, 100.25, 1.0, OrderSide::Sell),
            (12, 100.5, 1.0, OrderSide::Sell),
            (13, 100.25, 10.0, OrderSide::Buy),
            (14, 100.5, 10.0, OrderSide::Buy),
            (15, 100.75, 10.0, OrderSide::Buy),
            (16, 100.75, 0.1, OrderSide::Buy),
        ] {
            let time = format!("2024-01-16T23:{minute}:00Z");
            events.extend(host.push(trade(&time, price, size, Some(side))));
        }
        // Feed time at 17:05 on Wednesday trades Thursday's date and closes Wednesday's bar.
        let at = "2024-01-17T23:05:00Z";
        host.detectors
            .push("ES", at.parse().unwrap(), &trade(at, 100.75, 1.0, None));
        events.extend(host.events());
        let sweeps: Vec<_> = (of_kind(events, "Stop sweeps").into_iter())
            .map(|signal| signal.id.0)
            .collect();
        assert_eq!(sweeps, ["stop-sweep-1705449600000"]);
        // A big buy at 103 on Monday evening trades Tuesday's date, which opens at 100 and
        // closes at 101: with wick zones off, its zone goes once Wednesday's date opens.
        let host = Host::tracking(&futures, Timeframe::Day1, Some(10.0));
        let mut settings = host.detectors.settings();
        settings.big_trades.rule.wick_zones = false;
        host.detectors.set_settings(settings);
        let mut removed = Vec::new();
        for (time, price, size) in [
            ("2024-01-08T14:00:00Z", 100.0, 1.0),
            ("2024-01-08T23:10:00Z", 100.0, 1.0),
            ("2024-01-08T23:30:00Z", 103.0, 10.0),
            ("2024-01-09T14:00:00Z", 101.0, 1.0),
            ("2024-01-09T23:10:00Z", 101.0, 1.0),
        ] {
            let events = host.push(trade(time, price, size, Some(OrderSide::Buy)));
            removed.extend(
                (events.into_iter()).filter(|event| matches!(event, SignalEvent::Remove(_))),
            );
        }
        assert_eq!(
            removed,
            [SignalEvent::Remove(SignalId("big-trade-zone-1".into()))]
        );
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
                settings.icebergs.alert = true;
                settings.iceberg_count_alert = Some(CountAlert::default());
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
                assert_eq!(
                    host.detectors
                        .counts("ES", DetectionKind::Iceberg, time(1))
                        .total,
                    u64::from(!consumed),
                    "refill updates count one iceberg"
                );
                assert_eq!(
                    host.detectors.take_alerts().len(),
                    if consumed { 0 } else { 2 },
                    "one live alert and one count alert per iceberg"
                );
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

    /// Absorption, iceberg, liquidity and footprint condition texts, and the alerts that carry
    /// them, write sizes as big trade labels do: with the instrument's volume decimals, two
    /// without them, and K, M and B values with at least two.
    #[test]
    fn detector_texts_write_sizes_with_the_volume_decimals() {
        use OrderSide::{Buy, Sell};
        let ms = chrono::TimeDelta::milliseconds;
        // Volume decimals, the size of each fill, and the texts of the absorption, the iceberg
        // estimate, the large liquidity zone on arrival and once pulled, and the sell volume level.
        let rows = [
            (
                None,
                0.1,
                [
                    "Absorbed 0.30, opening shown 0.10",
                    "2 refills, executed 0.30, shown 0.10; estimate from aggregated depth",
                    "Peak 1.60, executed 0.00, withdrawn 0.00",
                    "Peak 1.60, executed 0.30, withdrawn 1.30",
                    "Sell volume 0.30 above 0.25",
                ],
            ),
            (
                Some(0),
                0.1,
                [
                    "Absorbed 0, opening shown 0",
                    "2 refills, executed 0, shown 0; estimate from aggregated depth",
                    "Peak 2, executed 0, withdrawn 0",
                    "Peak 2, executed 0, withdrawn 1",
                    "Sell volume 0 above 0",
                ],
            ),
            (
                Some(2),
                0.1,
                [
                    "Absorbed 0.30, opening shown 0.10",
                    "2 refills, executed 0.30, shown 0.10; estimate from aggregated depth",
                    "Peak 1.60, executed 0.00, withdrawn 0.00",
                    "Peak 1.60, executed 0.30, withdrawn 1.30",
                    "Sell volume 0.30 above 0.25",
                ],
            ),
            (
                Some(0),
                1_234_567.8,
                [
                    "Absorbed 3.70M, opening shown 1.23M",
                    "2 refills, executed 3.70M, shown 1.23M; estimate from aggregated depth",
                    "Peak 19.75M, executed 0, withdrawn 0",
                    "Peak 19.75M, executed 3.70M, withdrawn 16.05M",
                    "Sell volume 3.70M above 3.09M",
                ],
            ),
        ];
        let (mut written, mut expected) = (Vec::new(), Vec::new());
        for (decimals, size, texts) in rows {
            let mut instrument = Instrument::new("ES").with_tick_size(0.25);
            instrument.volume_decimals = decimals;
            let host = |configure: &dyn Fn(&mut DetectorSettings)| {
                let host = Host::tracking(&instrument, Timeframe::Min1, None);
                let mut settings = host.detectors.settings();
                configure(&mut settings);
                host.detectors.set_settings(settings);
                host
            };
            let row = format!("volume decimals {decimals:?}, size {size}");
            // The text of the only `category` detection among `events`.
            let text = |events: &[SignalEvent], category| {
                let signals = of_kind(events.to_vec(), category);
                assert_eq!(signals.len(), 1, "{category}, {row}");
                signals[0].tooltip.clone().unwrap()
            };
            let alerts = |host: &Host| -> Vec<_> {
                (host.detectors.take_alerts().into_iter())
                    .map(|alert| (alert.kind, alert.notification.body))
                    .collect()
            };
            let buys = |host: &Host| {
                for at in [0, 100, 200] {
                    let trade = Tick::new(time(0) + ms(at), 100.0, size, Some(Buy));
                    host.detectors
                        .push("ES", trade.ts, &MarketEvent::Trade(trade));
                }
            };

            // The ask holds its size, so the episode closes a second after the last fill as
            // absorption, and the depth update estimates two refills.
            let held = host(&|settings| {
                settings.absorption.enabled = true;
                settings.absorption.alert = true;
                settings.absorption.rule.threshold = Threshold::Fixed(size);
                settings.icebergs.enabled = true;
                settings.icebergs.alert = true;
            });
            held.detectors.push(
                "ES",
                time(0),
                &MarketEvent::DepthSnapshot {
                    sequence: 1,
                    book: depth_book(&[], &[(100.0, size)]),
                },
            );
            buys(&held);
            held.detectors.push(
                "ES",
                time(1) + ms(200),
                &MarketEvent::DepthDelta {
                    sequence: 2,
                    book: depth_book(&[], &[(100.0, size)]),
                },
            );
            let events = held.events();
            let (absorption, iceberg) = (text(&events, "Absorption"), text(&events, "Icebergs"));
            assert_eq!(
                alerts(&held),
                [
                    (DetectionKind::Iceberg, iceberg.clone()),
                    (DetectionKind::Absorption, absorption.clone())
                ],
                "{row}"
            );

            // A resting level of 16 fill sizes, filled three times and then withdrawn, is pulled.
            let pulled = host(&|settings| {
                settings.liquidity.enabled = true;
                settings.liquidity.alert = true;
                settings.liquidity.rule.threshold = Threshold::Fixed(16.0 * size);
            });
            pulled.detectors.push(
                "ES",
                time(0),
                &MarketEvent::DepthSnapshot {
                    sequence: 1,
                    book: depth_book(&[], &[(100.0, 16.0 * size)]),
                },
            );
            let arrival = text(&pulled.events(), "Large liquidity");
            buys(&pulled);
            pulled.detectors.push(
                "ES",
                time(1),
                &MarketEvent::DepthDelta {
                    sequence: 2,
                    book: depth_book(&[], &[(100.0, 0.0)]),
                },
            );
            let events = pulled.events();
            let liquidity = text(&events, "Large liquidity");
            let marker = format!("Pulled liquidity: {liquidity}");
            assert_eq!(text(&events, "Pulled liquidity"), marker, "{row}");
            assert_eq!(
                alerts(&pulled),
                [
                    (DetectionKind::LargeLiquidity, arrival.clone()),
                    (DetectionKind::PulledLiquidity, marker)
                ],
                "{row}"
            );

            // The first bar is left out; in the next, sells of one and two fill sizes pass 2.5.
            let footprint = host(&|settings| {
                settings.footprint.enabled = true;
                settings.footprint.alert = true;
                settings.footprint.rule.sell_volume_above = Some(2.5 * size);
            });
            for (second, sizes, side) in [(0, 1.0, Buy), (60, 1.0, Sell), (61, 2.0, Sell)] {
                let trade = MarketEvent::Trade(fill(second, 100.0, sizes * size, side, None));
                footprint.detectors.push("ES", time(second), &trade);
            }
            let condition = text(&footprint.events(), "Footprint conditions");
            assert_eq!(
                alerts(&footprint),
                [(DetectionKind::FootprintCondition, condition.clone())],
                "{row}"
            );

            written.push((
                decimals,
                size,
                [absorption, iceberg, arrival, liquidity, condition],
            ));
            expected.push((decimals, size, texts.map(str::to_owned)));
        }
        // Compared once, so that a failure lists every row.
        assert_eq!(written, expected);
    }

    #[test]
    fn liquidity_zones_end_at_the_pull_or_sweep_and_walls_need_execution_and_rejection() {
        for bid in [false, true] {
            for outcome in [
                "pull",
                "sweep",
                "sweep before depth",
                "sweep unrelated update",
                "pull after refill",
                "wall",
                "leave",
                "sweep of the rest",
                "pull of the rest",
            ] {
                let mut host = Host::new(Some(1_000.0));
                let mut settings = host.detectors.settings();
                settings.liquidity.enabled = true;
                settings.liquidity.alert = true;
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
                    "sweep"
                    | "sweep before depth"
                    | "sweep unrelated update"
                    | "pull after refill" => 90.0,
                    "leave" | "sweep of the rest" => 40.0,
                    "pull of the rest" => 15.0,
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
                let interim = matches!(outcome, "sweep unrelated update" | "pull after refill");
                if interim {
                    host.detectors.push(
                        "ES",
                        time(1),
                        &MarketEvent::DepthDelta {
                            sequence: 2,
                            book: if outcome == "pull after refill" {
                                book(&[(100.0, 100.0)])
                            } else {
                                book(&[(if bid { 99.0 } else { 101.0 }, 1.0)])
                            },
                        },
                    );
                }
                let away = if bid { 100.75 } else { 99.25 };
                let beyond = if bid { 99.75 } else { 100.25 };
                if outcome == "sweep before depth" {
                    host.detectors.push(
                        "ES",
                        time(1),
                        &MarketEvent::Trade(fill(1, beyond, 1.0, side, None)),
                    );
                }
                let rows = match outcome {
                    "wall" => vec![(100.0, 90.0), (away, 1.0)],
                    "leave" => vec![(100.0, 0.0)],
                    "pull of the rest" => vec![(100.0, 15.0)],
                    _ => vec![(100.0, 10.0)],
                };
                host.detectors.push(
                    "ES",
                    time(2),
                    &MarketEvent::DepthDelta {
                        sequence: if interim { 3 } else { 2 },
                        book: book(&rows),
                    },
                );
                if outcome == "sweep of the rest" {
                    // The rest executes after the update: 50 executed covers half the drop of 90.
                    host.detectors.push(
                        "ES",
                        time(3),
                        &MarketEvent::Trade(fill(3, 100.0, 10.0, side, None)),
                    );
                }
                if matches!(
                    outcome,
                    "sweep" | "sweep unrelated update" | "sweep of the rest" | "leave"
                ) {
                    host.detectors.push(
                        "ES",
                        time(3),
                        &MarketEvent::Trade(fill(3, beyond, 1.0, side, None)),
                    );
                }
                if outcome == "pull of the rest" {
                    // 70 withdrawn and the rest of 15 make 85 of the peak of 100.
                    host.detectors.push(
                        "ES",
                        time(3),
                        &MarketEvent::DepthDelta {
                            sequence: 3,
                            book: book(&[(100.0, 0.0)]),
                        },
                    );
                }
                if outcome == "leave" {
                    // 40 executed and 60 withdrawn can be neither a sweep nor a pull, so the level
                    // ends and the next large size at its price is a level of its own.
                    for (sequence, second, size) in [(3, 10, 100.0), (4, 20, 0.0)] {
                        host.detectors.push(
                            "ES",
                            time(second),
                            &MarketEvent::DepthDelta {
                                sequence,
                                book: book(&[(100.0, size)]),
                            },
                        );
                    }
                }
                let events = host.events();
                let category = match outcome {
                    "pull" | "pull after refill" | "pull of the rest" | "leave" => {
                        "Pulled liquidity"
                    }
                    "sweep"
                    | "sweep before depth"
                    | "sweep unrelated update"
                    | "sweep of the rest" => "Liquidity sweeps",
                    _ => "Walls",
                };
                let markers = of_kind(events.clone(), category);
                assert_eq!(markers.len(), 1, "{outcome} bid={bid}");
                let zone = |id: &SignalId, from, to: Option<i64>| {
                    let shape = SignalShape::PriceZone {
                        from: time(from),
                        to: to.map(time),
                        price: 100.0,
                        direction: if bid { Direction::Up } else { Direction::Down },
                    };
                    (id.clone(), shape)
                };
                let first = &zones[0].id;
                let second = SignalId("liquidity-3".into());
                let expected = match outcome {
                    "wall" => vec![],
                    "sweep"
                    | "sweep unrelated update"
                    | "sweep of the rest"
                    | "pull of the rest" => {
                        vec![zone(first, 0, Some(3))]
                    }
                    "leave" => vec![
                        zone(first, 0, Some(2)),
                        zone(&second, 10, None),
                        zone(&second, 10, Some(20)),
                    ],
                    _ => vec![zone(first, 0, Some(2))],
                };
                let ended: Vec<_> = of_kind(events, "Large liquidity")
                    .into_iter()
                    .map(|zone| (zone.id, zone.shape))
                    .collect();
                assert_eq!(ended, expected, "{outcome} bid={bid}");
                if outcome == "leave" {
                    assert_eq!(
                        markers[0].tooltip.as_deref(),
                        Some("Pulled liquidity: Peak 100, executed 0, withdrawn 100")
                    );
                    let alerts: Vec<_> = (host.detectors.take_alerts().into_iter())
                        .map(|alert| alert.kind)
                        .collect();
                    assert_eq!(
                        alerts,
                        [
                            DetectionKind::LargeLiquidity,
                            DetectionKind::FreshLiquidity,
                            DetectionKind::LargeLiquidity,
                            DetectionKind::PulledLiquidity
                        ],
                        "bid={bid}"
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
