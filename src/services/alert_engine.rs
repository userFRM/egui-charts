//! An alert service that evaluates alerts on the bars the host feeds it.

use std::collections::HashMap;
use std::sync::Mutex;

use chrono::{DateTime, Utc};

use super::{AlertEvent, Alerts, Events};
use crate::data::{FeedError, MaybeSend, Reply};
use crate::model::{
    Alert, AlertChannelComparison, AlertComparison, AlertCondition, AlertFrequency, AlertId,
    AlertSource, AlertStatus, Bar, BarClock, Timeframe,
};

/// An [`Alerts`] service that keeps alerts in memory and evaluates them on the bars the host
/// feeds it with [`update`](Self::update), as [`PaperAccount`](super::PaperAccount) fills orders
/// on the prices it is fed. It needs no egui, so a server can run the same code. A host that
/// evaluates alerts on its own server implements [`Alerts`] itself instead.
/// With `studies`, `AlertEvaluator` reads the saved source definitions from a datafeed and
/// supplies these evaluations without constructing a chart.
///
/// The host keeps the alerts: [`alerts`](Self::alerts) returns them with their status for
/// storage, and [`with_alerts`](Self::with_alerts) restores them. Evaluation state lives in
/// memory only, so after a restart an alert that fires on every occurrence may fire again while
/// its condition still holds; an alert with frequency [`Once`](AlertFrequency::Once) that fired
/// stays [`Triggered`](AlertStatus::Triggered).
///
/// # Rules
///
/// *Subject.* Each symbol or watchlist member retains independent state. An unset first interval
/// takes the first interval not reserved by an additional override (or one minute), saved in an
/// `Upsert` event. The datafeed evaluator selects one minute. Additional rules
/// follow it unless `additional_timeframes` supplies an override. Unused intervals are ignored.
///
/// *Synchronization.* Each interval evaluates its own newest forming bar, including its own
/// lookbacks and preceding source values. For `OncePerBarClose`, the shortest interval uses its
/// newest completed bar; other intervals keep their live observations. Each completed occurrence
/// is evaluated once after compatible observations arrive. The shortest interval supplies the
/// occurrence bar. Other observations must cover its open, wherever inside its period the feed
/// stamped their bar. A longer bar closes together with the last shorter bar of its period, so
/// for `OncePerBarClose` each other interval also keeps the bar before its newest, which that
/// shorter bar joins when the next longer bar has arrived, a session has ended or trading has
/// paused. Observations from later windows cannot satisfy an earlier event, nor can expired
/// observations satisfy a newer one. Each direct update evaluates the latest overlapping
/// observations; `AlertEvaluator` gathers a complete poll before evaluating. Missing or non-finite
/// sources make their conditions false. Price and volume come from each interval's own bars.
/// Messages are filled when the alert fires, with its firing time and the first condition's
/// interval, bar and source values in the joined observations. Hosts feeding the engine
/// directly use UTC windows; the evaluator retains feed calendars.
///
/// *Change.* An update whose evaluated bar, its close and volume and the value of every source
/// equal those of the previous update does nothing, unless a per-minute alert enters a new
/// minute or its schedule opens. An update
/// whose evaluated bar is older than the newest one already evaluated for its interval is ignored:
/// reloading history, rewinding a replay or switching to a coarser timeframe never fires an
/// alert again.
///
/// *Crossings.* On every interval the previous value is the value at the alert's previous
/// evaluation, within a bar and across bars, and on the first evaluation the value one bar earlier.
/// A crossing on a longer interval therefore fires once whatever the shorter interval does, and
/// one between two closes of the shorter interval is caught at the later close. Crossing up needs
/// a previous value strictly below the level and a current value at or above it, crossing down the
/// reverse; a value that touches the level and stays there fires once. Moving by and volume
/// above average read the bars before the evaluated one, as [`AlertCondition`] documents.
/// Moving channels resolve the preceding bar again when a new bar arrives, so edits and
/// skipped updates use that bar's current geometry. Within a bar they use the previous evaluation.
///
/// *Frequency.* [`Once`](AlertFrequency::Once) fires on the first evaluation that holds and the
/// alert becomes [`Triggered`](AlertStatus::Triggered); a watchlist alert fires once per member
/// and stays active. [`EveryTime`](AlertFrequency::EveryTime) fires on every evaluation that
/// holds, [`OncePerBar`](AlertFrequency::OncePerBar) at most once per bar open time, and
/// [`OncePerBarClose`](AlertFrequency::OncePerBarClose) evaluates each completed bar once.
/// [`OncePerMinute`](AlertFrequency::OncePerMinute) fires at most once per minute of the update
/// time `now`; moving the clock backwards does not repeat a minute.
///
/// *Schedule.* An alert's `schedule` restricts notifications. Outside it the engine keeps
/// observing values for crossings and records each trigger as a silent
/// [`AlertEvent::SilentTriggered`] entry without calling the delivery callback or consuming an
/// occurrence, so a once alert stays armed. It records at most one entry per occurrence of its
/// frequency, a once alert one per bar. `starts` holds back evaluation altogether until that time.
///
/// *Expiry.* Every `update` and [`tick`](Self::tick) first marks active alerts whose expiry has
/// passed as [`Expired`](AlertStatus::Expired), so an alert never fires at or after its expiry
/// and expires without data. Disabled, triggered and expired alerts are not evaluated, and
/// saving or removing an alert discards its evaluation state, so a resumed alert starts afresh.
///
/// ```
/// use chrono::{DateTime, Utc};
/// use egui_charts::model::{Alert, AlertCondition, Bar, Timeframe};
/// use egui_charts::services::AlertEngine;
///
/// let mut alert = Alert::new("AAPL", AlertCondition::CrossingUp(100.0));
/// alert.timeframe = Some(Timeframe::Min1);
/// let engine = AlertEngine::new().with_alerts(vec![alert]);
/// let time = |seconds| DateTime::<Utc>::from_timestamp(seconds, 0).unwrap();
/// let bar = |seconds, close| Bar::new(time(seconds), close, close, close, close, 1.0);
/// // The close rises through 100 on the second bar; `update` also takes the chart's source values.
/// engine.update("AAPL", None, Timeframe::Min1, &[bar(0, 99.0), bar(60, 101.0)], time(61), |_, _| None);
/// ```
pub struct AlertEngine {
    state: Mutex<State>,
    deliver: Option<Box<dyn Deliver>>,
}

/// The host's callback for triggers.
trait Deliver: MaybeSend {
    fn deliver(&self, alert: &Alert, event: &AlertEvent);
}

impl<F: Fn(&Alert, &AlertEvent) + MaybeSend> Deliver for F {
    fn deliver(&self, alert: &Alert, event: &AlertEvent) {
        self(alert, event);
    }
}

#[derive(Default)]
struct State {
    alerts: Vec<Alert>,
    events: Option<Events<AlertEvent>>,
    next_id: u64,
    watchlists: HashMap<String, Vec<String>>,
    runs: HashMap<(AlertId, String), Run>,
}

/// What the engine remembers of one alert on one symbol.
#[derive(Default)]
struct Run {
    intervals: HashMap<Timeframe, Observation>,
    /// Inputs of the most recent synchronized evaluation.
    joined: Vec<Seen>,
    /// The occurrence the alert last fired on: the open time of a bar, or the minute for a
    /// per-minute alert.
    fired: Option<DateTime<Utc>>,
    /// The occurrence of the last silent entry, so a lasting condition outside the schedule
    /// does not log every update.
    silent: Option<DateTime<Utc>>,
    /// A watchlist alert with frequency once has fired on this symbol.
    done: bool,
    #[cfg(feature = "strategies")]
    last_fill: u64,
}

/// The latest observation for conditions sharing one calculation interval.
struct Observation {
    available: bool,
    seen: Seen,
    /// Whether the rules that read this bar alone hold. Those that compare two evaluations are
    /// tested when the alert evaluates, see [`reads_previous`].
    holds: bool,
    /// The values of `seen.rules` one bar earlier, read when this bar first arrived.
    before: Vec<[Option<f64>; 3]>,
    clock: BarClock,
    event: DateTime<Utc>,
    bar: Bar,
    /// What the message of the first condition's interval reads on this bar. An update of another
    /// interval, which may fire the alert, cannot read it.
    values: Vec<(AlertSource, Option<f64>)>,
    /// The bar before this one as it ended, or as last observed when the update held only this
    /// one, for a shorter bar that closes with it. Only a longer interval of an alert evaluated at
    /// bar close keeps it.
    earlier: Option<Box<Observation>>,
}

/// The inputs of one evaluation: the bar, its close and volume, and for each rule the value of
/// its source and of the other output it compares with.
#[derive(Clone, PartialEq)]
struct Seen {
    time: DateTime<Utc>,
    scheduled: bool,
    minute: Option<DateTime<Utc>>,
    /// The bits of the close and volume, so that a value that is not a number equals itself.
    bar: (u64, u64),
    rules: Vec<[Option<f64>; 3]>,
}

impl Default for AlertEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl AlertEngine {
    /// An engine without alerts. Alerts saved without an id get "1", "2" and so on, skipping the
    /// ids in use.
    pub fn new() -> Self {
        Self {
            state: Mutex::new(State::default()),
            deliver: None,
        }
    }

    /// Calls `deliver` with the alert and its [`AlertEvent::TriggeredForSymbol`] on every trigger,
    /// after the event is pushed, so the host can send email, push notifications or web requests.
    /// The call comes from the thread that fed the engine and must return at once; it may use
    /// the engine.
    pub fn on_trigger(
        mut self,
        deliver: impl Fn(&Alert, &AlertEvent) + MaybeSend + 'static,
    ) -> Self {
        self.deliver = Some(Box::new(deliver));
        self
    }

    /// Starts with `alerts` as [`alerts`](Self::alerts) returned them, giving those without an
    /// id one after the others.
    pub fn with_alerts(self, alerts: Vec<Alert>) -> Self {
        {
            let mut state = self.lock();
            // Ids are given once every kept id is known, so that none is given twice.
            let (kept, new): (Vec<_>, Vec<_>) = alerts.into_iter().partition(|a| a.id.is_some());
            for alert in kept.into_iter().chain(new) {
                state.insert(alert);
            }
        }
        self
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// The alerts with their current status, for the host to keep.
    pub fn alerts(&self) -> Vec<Alert> {
        self.lock().alerts.clone()
    }

    /// Active symbols still armed for `id`, including watchlist members. A once-per-member
    /// alert omits symbols that already fired, so a host can release their subscriptions.
    pub fn subjects(&self, id: &AlertId) -> Vec<String> {
        let state = self.lock();
        let Some(alert) = state.alerts.iter().find(|alert| {
            alert.id.as_ref() == Some(id)
                && alert.status == AlertStatus::Active
                && alert.is_valid()
                && !matches!(alert.condition, AlertCondition::StrategyFills(_))
        }) else {
            return Vec::new();
        };
        let symbols = match &alert.watchlist {
            Some(list) => state.watchlists.get(list).cloned().unwrap_or_default(),
            None => vec![alert.symbol.clone()],
        };
        symbols
            .into_iter()
            .filter(|symbol| {
                !state
                    .runs
                    .get(&(id.clone(), symbol.clone()))
                    .is_some_and(|run| run.done)
            })
            .collect()
    }

    /// Sets the members of the watchlist `id`, as the host's watchlist holds them. A member that
    /// leaves loses its evaluation state, so it starts afresh if it returns.
    pub fn set_watchlist(&self, id: &str, symbols: Vec<String>) {
        let mut state = self.lock();
        let State { alerts, runs, .. } = &mut *state;
        runs.retain(|(alert, symbol), _| {
            symbols.contains(symbol)
                || !alerts
                    .iter()
                    .any(|a| a.id.as_ref() == Some(alert) && a.watchlist.as_deref() == Some(id))
        });
        state.watchlists.insert(id.to_owned(), symbols);
    }

    /// Updates active conditions of `symbol` on `timeframe` against the newest of `bars`, oldest
    /// first, at the time `now`. Feed every interval in [`Alert::timeframes`]. `value` resolves a
    /// study or drawing source at a bar index of `bars`, for instance with `Chart::alert_value`;
    /// price and volume come from `bars`. `exchange` is the exchange of `symbol` for the
    /// `{{exchange}}` placeholder, as [`Alert::expand_message`] takes it. Call it each frame with
    /// the bars of every live chart, but not with a chart in replay.
    pub fn update(
        &self,
        symbol: &str,
        exchange: Option<&str>,
        timeframe: Timeframe,
        bars: &[Bar],
        now: DateTime<Utc>,
        value: impl Fn(&AlertSource, usize) -> Option<f64>,
    ) {
        self.update_matching(
            None,
            symbol,
            exchange,
            &[(&BarClock::new(timeframe), bars)],
            now,
            |_, source, at| value(source, at),
        );
    }

    /// Updates one interval of an alert with its retained source definitions. Other alerts may
    /// refer to different study instances with the same chart-local identifiers. `exchange` is
    /// the exchange of `symbol`, as [`AlertEngine::update`] takes it.
    pub fn update_alert(
        &self,
        id: &AlertId,
        symbol: &str,
        exchange: Option<&str>,
        timeframe: Timeframe,
        bars: &[Bar],
        now: DateTime<Utc>,
        value: impl Fn(&AlertSource, usize) -> Option<f64>,
    ) {
        self.update_matching(
            Some(id),
            symbol,
            exchange,
            &[(&BarClock::new(timeframe), bars)],
            now,
            |_, source, at| value(source, at),
        );
    }

    /// Updates the active alerts of `symbol`, or only `id`, with one set of interval inputs.
    pub(super) fn update_matching(
        &self,
        id: Option<&AlertId>,
        symbol: &str,
        exchange: Option<&str>,
        inputs: &[(&BarClock, &[Bar])],
        now: DateTime<Utc>,
        value: impl Fn(Timeframe, &AlertSource, usize) -> Option<f64>,
    ) {
        let mut fired = Vec::new();
        let mut state = self.lock();
        state.expire(now);
        let State {
            alerts,
            events,
            watchlists,
            runs,
            ..
        } = &mut *state;
        for alert in alerts.iter_mut().filter(|alert| {
            alert.status == AlertStatus::Active
                && alert.is_valid()
                && alert.starts.is_none_or(|starts| starts <= now)
                && !matches!(alert.condition, AlertCondition::StrategyFills(_))
                && id.is_none_or(|id| alert.id.as_ref() == Some(id))
        }) {
            let Some(id) = alert.id.clone() else { continue };
            let subject = match &alert.watchlist {
                Some(list) => watchlists
                    .get(list)
                    .is_some_and(|members| members.iter().any(|member| member == symbol)),
                None => alert.symbol == symbol,
            };
            if !subject {
                continue;
            }
            if alert.timeframe.is_none()
                && let Some(timeframe) = inputs
                    .iter()
                    .filter_map(|(clock, _)| clock.timeframe())
                    .find(|timeframe| {
                        *timeframe == Timeframe::default()
                            || !alert.additional_timeframes.contains(&Some(*timeframe))
                    })
            {
                alert.timeframe = Some(timeframe);
                if let Some(events) = events {
                    events.push(AlertEvent::Upsert(Box::new(alert.clone())));
                }
            }
            let intervals = alert.timeframes();
            let mut matched = false;
            let mut ready = true;
            let run = runs.entry((id.clone(), symbol.to_owned())).or_default();
            for (clock, bars) in inputs {
                let Some(timeframe) = clock.timeframe().filter(|tf| intervals.contains(tf)) else {
                    continue;
                };
                matched = true;
                if !observe(
                    alert,
                    run,
                    (symbol, exchange, clock),
                    bars,
                    now,
                    &|source, at| value(timeframe, source, at),
                ) {
                    ready = false;
                }
            }
            if !matched || !ready {
                continue;
            }
            let Some((bar, message, silent)) = evaluate(alert, run, symbol, exchange, now) else {
                continue;
            };
            let price = bar.close.is_finite().then_some(bar.close);
            if silent {
                if let Some(events) = events {
                    events.push(AlertEvent::SilentTriggered {
                        id,
                        symbol: symbol.to_owned(),
                        time: now,
                        price,
                        message,
                    });
                }
                continue;
            }
            alert.last_triggered_at = Some(alert.last_triggered_at.map_or(now, |old| old.max(now)));
            if alert.frequency == AlertFrequency::Once {
                run.done = true;
                if alert.watchlist.is_none() {
                    alert.status = AlertStatus::Triggered;
                }
            }
            run.fired = Some(occurrence(alert.frequency, &bar, now));
            if let Some(events) = events
                && alert.status != AlertStatus::Active
            {
                events.push(AlertEvent::Upsert(Box::new(alert.clone())));
            }
            let event = AlertEvent::TriggeredForSymbol {
                id,
                symbol: symbol.to_owned(),
                time: now,
                price,
                message,
            };
            if let Some(events) = events {
                events.push(event.clone());
            }
            fired.push((alert.clone(), event));
        }
        runs.retain(|(id, _), _| {
            alerts
                .iter()
                .any(|a| a.id.as_ref() == Some(id) && a.status == AlertStatus::Active)
        });
        drop(state);
        if let Some(deliver) = &self.deliver {
            for (alert, event) in &fired {
                deliver.deliver(alert, event);
            }
        }
    }

    /// Delivers each new fill of `run` to matching strategy-fill alerts. Use a unique run id
    /// for each `Backtest` instance, retained for its lifetime, and forward its update batches.
    /// Repeated batches are ignored by sequence, including after edits or rearming an alert
    /// on the same run and when multiple fills share a timestamp.
    /// `now` gates expiry and notification schedules; event times are the source fill times.
    /// Empty messages use the execution side, quantity, price and resulting signed position.
    /// Custom messages expand the usual placeholders using a flat bar at the fill price, with
    /// its source time and zero volume, and `exchange` as [`Alert::expand_message`] takes it. A
    /// fill carries no study values, so `{{plot_ID_N}}` reads `NaN`. Strategy-fill alerts need no
    /// price-reader subscription.
    /// Delivery callbacks run after unlocking, as they do for price alerts.
    #[cfg(feature = "strategies")]
    pub fn strategy_fills(
        &self,
        run: &str,
        symbol: &str,
        exchange: Option<&str>,
        timeframe: Timeframe,
        fills: &[crate::strategies::StrategyFill],
        now: DateTime<Utc>,
    ) {
        let mut fired = Vec::new();
        let mut state = self.lock();
        state.expire(now);
        let State {
            alerts,
            runs,
            events,
            watchlists,
            ..
        } = &mut *state;
        for alert in alerts.iter_mut().filter(|alert| {
            alert.status == AlertStatus::Active
                && alert.is_valid()
                && alert.starts.is_none_or(|starts| starts <= now)
                && matches!(&alert.condition, AlertCondition::StrategyFills(id) if id == run)
                && alert.timeframe.is_none_or(|own| own == timeframe)
                && match &alert.watchlist {
                    Some(list) => watchlists
                        .get(list)
                        .is_some_and(|members| members.iter().any(|member| member == symbol)),
                    None => alert.symbol == symbol,
                }
        }) {
            let Some(id) = alert.id.clone() else { continue };
            if alert.timeframe.is_none() {
                alert.timeframe = Some(timeframe);
                if let Some(events) = events {
                    events.push(AlertEvent::Upsert(Box::new(alert.clone())));
                }
            }
            let seen = runs.entry((id.clone(), symbol.to_owned())).or_default();
            for fill in fills {
                if fill.sequence <= seen.last_fill
                    || !fill.price.is_finite()
                    || !fill.quantity.is_finite()
                    || fill.quantity <= 0.0
                    || !fill.position.is_finite()
                {
                    continue;
                }
                seen.last_fill = fill.sequence;
                if seen.done {
                    continue;
                }
                let side = match fill.side {
                    crate::model::OrderSide::Buy => "Buy",
                    crate::model::OrderSide::Sell => "Sell",
                };
                let mut price = fill.price.to_string();
                match price.split_once('.') {
                    None => price.push_str(".00"),
                    Some((_, fraction)) if fraction.len() == 1 => price.push('0'),
                    _ => {}
                }
                let message = if alert.message.is_empty() {
                    format!(
                        "{side} {} at {price}, position {}",
                        fill.quantity, fill.position
                    )
                } else {
                    let bar = Bar::new(
                        fill.time, fill.price, fill.price, fill.price, fill.price, 0.0,
                    );
                    alert.expand_message(symbol, exchange, timeframe, &bar, fill.time, |source| {
                        (*source == AlertSource::Price).then_some(fill.price)
                    })
                };
                if alert
                    .schedule
                    .as_ref()
                    .is_some_and(|schedule| !schedule.allows(now))
                {
                    if let Some(events) = events {
                        events.push(AlertEvent::SilentTriggered {
                            id: id.clone(),
                            symbol: symbol.to_owned(),
                            time: fill.time,
                            price: Some(fill.price),
                            message,
                        });
                    }
                    continue;
                }
                alert.last_triggered_at = Some(
                    alert
                        .last_triggered_at
                        .map_or(fill.time, |old| old.max(fill.time)),
                );
                if alert.frequency == AlertFrequency::Once {
                    seen.done = true;
                    if alert.watchlist.is_none() {
                        alert.status = AlertStatus::Triggered;
                    }
                    if let Some(events) = events {
                        events.push(AlertEvent::Upsert(Box::new(alert.clone())));
                    }
                }
                let event = AlertEvent::TriggeredForSymbol {
                    id: id.clone(),
                    symbol: symbol.to_owned(),
                    time: fill.time,
                    price: Some(fill.price),
                    message,
                };
                if let Some(events) = events {
                    events.push(event.clone());
                }
                fired.push((alert.clone(), event));
            }
        }
        drop(state);
        if let Some(deliver) = &self.deliver {
            for (alert, event) in &fired {
                deliver.deliver(alert, event);
            }
        }
    }

    /// Marks the alerts whose expiry has passed as expired, without waiting for data. Call it
    /// at least once a second.
    pub fn tick(&self, now: DateTime<Utc>) {
        self.lock().expire(now);
    }
}

impl State {
    /// Adds `alert` or replaces the one with its id, giving it an id if it has none.
    fn insert(&mut self, mut alert: Alert) -> Alert {
        if alert.id.is_none() {
            alert.created_at = Some(Utc::now());
            alert.last_triggered_at = None;
            loop {
                self.next_id += 1;
                let id = AlertId(self.next_id.to_string());
                if !self.alerts.iter().any(|a| a.id.as_ref() == Some(&id)) {
                    alert.id = Some(id);
                    break;
                }
            }
        }
        let existing = self.alerts.iter().find(|old| old.id == alert.id);
        if let Some(old) = existing {
            alert.created_at = old.created_at;
            alert.last_triggered_at = old.last_triggered_at;
        }
        let same_monitoring = existing.is_some_and(|old| {
            old.same_monitoring(&alert)
                && (old.extend_line != alert.extend_line
                    || old.blink_on_trigger != alert.blink_on_trigger
                    || old.delivery != alert.delivery)
        });
        let same_strategy = matches!(alert.condition, AlertCondition::StrategyFills(_))
            && self
                .alerts
                .iter()
                .any(|old| old.id == alert.id && old.condition == alert.condition);
        self.runs.retain(|(id, _), run| {
            if Some(id) != alert.id.as_ref() || same_monitoring {
                return true;
            }
            if same_strategy {
                // Editing or rearming a run must not replay its executions.
                run.done = false;
            }
            same_strategy
        });
        match self.alerts.iter_mut().find(|a| a.id == alert.id) {
            Some(kept) => *kept = alert.clone(),
            None => self.alerts.push(alert.clone()),
        }
        alert
    }

    fn push(&self, event: AlertEvent) {
        if let Some(events) = &self.events {
            events.push(event);
        }
    }

    fn expire(&mut self, now: DateTime<Utc>) {
        let mut expired = Vec::new();
        for alert in self.alerts.iter_mut().filter(|alert| {
            alert.status == AlertStatus::Active && alert.expires.is_some_and(|at| at <= now)
        }) {
            alert.status = AlertStatus::Expired;
            expired.push(alert.clone());
        }
        for alert in expired {
            if !matches!(alert.condition, AlertCondition::StrategyFills(_)) {
                self.runs.retain(|(id, _), _| Some(id) != alert.id.as_ref());
            }
            self.push(AlertEvent::Upsert(Box::new(alert)));
        }
    }
}

impl Alerts for AlertEngine {
    fn engine(&self) -> Option<&AlertEngine> {
        Some(self)
    }

    fn watch(&self, events: Events<AlertEvent>) {
        let mut state = self.lock();
        for alert in &state.alerts {
            events.push(AlertEvent::Upsert(Box::new(alert.clone())));
        }
        state.events = Some(events);
    }

    fn save(&self, alert: Alert, reply: Reply<()>) {
        if !alert.is_valid() {
            reply.send(Err(FeedError::InvalidData(
                "Alert contains invalid conditions".into(),
            )));
            return;
        }
        let mut state = self.lock();
        let alert = state.insert(alert);
        state.push(AlertEvent::Upsert(Box::new(alert)));
        reply.send(Ok(()));
    }

    fn remove(&self, id: AlertId, reply: Reply<()>) {
        let mut state = self.lock();
        state.runs.retain(|(kept, _), _| *kept != id);
        state.alerts.retain(|a| a.id.as_ref() != Some(&id));
        state.push(AlertEvent::Remove(id));
        reply.send(Ok(()));
    }
}

/// Observe one interval before testing the complete set of conditions.
fn observe(
    alert: &Alert,
    run: &mut Run,
    (symbol, exchange, clock): (&str, Option<&str>, &BarClock),
    bars: &[Bar],
    now: DateTime<Utc>,
    value: &impl Fn(&AlertSource, usize) -> Option<f64>,
) -> bool {
    let Some(timeframe) = clock.timeframe() else {
        return false;
    };
    let intervals = alert.timeframes();
    let closed = alert.frequency == AlertFrequency::OncePerBarClose
        && timeframe.as_str() == intervals[0].as_str();
    let Some(index) = bars.len().checked_sub(if closed { 2 } else { 1 }) else {
        if let Some(observation) = run.intervals.get_mut(&timeframe) {
            observation.available = false;
        }
        return false;
    };
    let bar = &bars[index];
    let previous = run.intervals.get(&timeframe);
    if previous.is_some_and(|previous| bar.time < previous.seen.time) {
        return false;
    }
    if closed && previous.is_some_and(|previous| bar.time == previous.seen.time) {
        if let Some(previous) = run.intervals.get_mut(&timeframe) {
            previous.available = true;
        }
        return true;
    }
    let source = |source: &AlertSource, at: usize| -> Option<f64> {
        let found = match source {
            AlertSource::Price => bars.get(at)?.close,
            AlertSource::Volume => bars.get(at)?.volume,
            _ => value(source, at)?,
        };
        found.is_finite().then_some(found)
    };
    let rules: Vec<_> = alert
        .rules()
        .filter(|(_, _, interval)| *interval == timeframe)
        .map(|(source, condition, _)| (source, condition))
        .collect();
    let read = |at: Option<usize>| -> Vec<_> {
        let source = |src| at.and_then(|at| source(src, at));
        rules
            .iter()
            .map(|(src, condition)| {
                let [a, b] = condition.partners().map(|other| other.and_then(source));
                [source(src), a, b]
            })
            .collect()
    };
    let seen_at = |at: usize| Seen {
        time: bars[at].time,
        scheduled: alert
            .schedule
            .as_ref()
            .is_none_or(|schedule| schedule.allows(now)),
        minute: (alert.frequency == AlertFrequency::OncePerMinute)
            .then(|| occurrence(alert.frequency, &bars[at], now)),
        bar: (bars[at].close.to_bits(), bars[at].volume.to_bits()),
        rules: read(Some(at)),
    };
    let seen = seen_at(index);
    if previous.is_some_and(|previous| previous.seen == seen) {
        if let Some(previous) = run.intervals.get_mut(&timeframe) {
            previous.available = true;
        }
        return true;
    }
    let observed = |at: usize, seen: Seen, before, event, earlier| {
        let holds = rules
            .iter()
            .zip(&seen.rules)
            .all(|((src, condition), &[current, a, b])| {
                reads_previous(condition)
                    || current.is_some_and(|current| {
                        condition_holds(
                            condition,
                            current,
                            [None; 3],
                            [a, b],
                            &bars[..=at],
                            |back| at.checked_sub(back as usize).and_then(|at| source(src, at)),
                        )
                    })
            });
        // Only the message knows the sources it names, so this expansion collects them.
        let values = std::cell::RefCell::new(Vec::new());
        if holds && timeframe == alert.timeframe.unwrap_or_default() {
            alert.expand_message(symbol, exchange, timeframe, &bars[at], now, |src| {
                let found = source(src, at);
                values.borrow_mut().push((src.clone(), found));
                found
            });
        }
        Observation {
            available: true,
            seen,
            holds,
            before,
            clock: clock.clone(),
            event,
            bar: bars[at].clone(),
            values: values.into_inner(),
            earlier,
        }
    };
    let event = |at: usize| clock.bar_open(bars[at].time);
    let newest = match run.intervals.remove(&timeframe) {
        Some(old) if old.seen.time == bar.time => {
            observed(index, seen, old.before, old.event, old.earlier)
        }
        old => {
            let earlier = match index.checked_sub(1) {
                _ if alert.frequency != AlertFrequency::OncePerBarClose || closed => None,
                Some(at) => {
                    let before = read(at.checked_sub(1));
                    Some(Box::new(observed(at, seen_at(at), before, event(at), None)))
                }
                // The update holds only the newest bar, so keep the one before as last observed.
                None => old.map(|mut old| {
                    old.earlier = None;
                    Box::new(old)
                }),
            };
            observed(
                index,
                seen,
                read(index.checked_sub(1)),
                event(index),
                earlier,
            )
        }
    };
    run.intervals.insert(timeframe, newest);
    true
}

/// Test one coherent set of interval observations and consume its occurrence. Returns the bar
/// and message to fire, and whether the trigger falls outside the schedule and is only logged.
fn evaluate(
    alert: &Alert,
    run: &mut Run,
    symbol: &str,
    exchange: Option<&str>,
    now: DateTime<Utc>,
) -> Option<(Bar, String, bool)> {
    let intervals = alert.timeframes();
    let anchor = run.intervals.get(&intervals[0])?;
    if alert.frequency == AlertFrequency::OncePerBarClose
        && run
            .joined
            .first()
            .is_some_and(|seen| seen.time == anchor.seen.time)
    {
        return None;
    }
    let joined: Option<Vec<_>> = intervals
        .iter()
        .map(|interval| {
            let newest = run.intervals.get(interval)?;
            std::iter::once(newest)
                .chain(newest.earlier.as_deref())
                .find(|observation| {
                    newest.available
                        && observation.event <= anchor.event
                        && (intervals.len() == 1
                            || observation.clock.same_bar(observation.event, anchor.event))
                })
        })
        .collect();
    let joined = joined?;
    if joined
        .iter()
        .map(|observation| &observation.seen)
        .eq(&run.joined)
    {
        return None;
    }
    let holds =
        joined
            .iter()
            .zip(&intervals)
            .enumerate()
            .all(|(position, (observation, interval))| {
                observation.holds
                    && previous_rules_hold(alert, *interval, observation, run.joined.get(position))
            });
    run.joined = joined
        .iter()
        .map(|observation| observation.seen.clone())
        .collect();
    let bar = &anchor.bar;
    let fires = holds
        && match alert.frequency {
            AlertFrequency::Once => !run.done,
            AlertFrequency::EveryTime => true,
            AlertFrequency::OncePerMinute => run
                .fired
                .is_none_or(|last| occurrence(alert.frequency, bar, now) > last),
            AlertFrequency::OncePerBar | AlertFrequency::OncePerBarClose => {
                run.fired != Some(occurrence(alert.frequency, bar, now))
            }
        };
    if !fires {
        return None;
    }
    let silent = alert
        .schedule
        .as_ref()
        .is_some_and(|schedule| !schedule.allows(now));
    if silent {
        let at = occurrence(alert.frequency, bar, now);
        if alert.frequency != AlertFrequency::EveryTime && run.silent == Some(at) {
            return None;
        }
        run.silent = Some(at);
    }
    let first = alert.timeframe.unwrap_or_default();
    let (primary, _) = joined
        .iter()
        .zip(&intervals)
        .find(|(_, interval)| **interval == first)?;
    let message = alert.expand_message(symbol, exchange, first, &primary.bar, now, |source| {
        primary
            .values
            .iter()
            .find(|(kept, _)| kept == source)
            .and_then(|(_, value)| *value)
    });
    Some((bar.clone(), message, silent))
}

/// Whether the rules of `interval` that compare two evaluations hold between `last`, the inputs of
/// the alert's previous evaluation, and `observation`. Without `last`, and for a moving channel
/// on a new bar, the earlier side is the bar before the observed one.
fn previous_rules_hold(
    alert: &Alert,
    interval: Timeframe,
    observation: &Observation,
    last: Option<&Seen>,
) -> bool {
    alert
        .rules()
        .filter(|(_, _, own)| *own == interval)
        .enumerate()
        .filter(|(_, (_, condition, _))| reads_previous(condition))
        .all(|(n, (_, condition, _))| {
            let [current, a, b] = observation.seen.rules[n];
            let previous = match last {
                Some(last)
                    if last.time == observation.seen.time
                        || !matches!(condition, AlertCondition::Channel { .. }) =>
                {
                    last.rules[n]
                }
                _ => observation.before[n],
            };
            current.is_some_and(|current| {
                condition_holds(condition, current, previous, [a, b], &[], |_| None)
            })
        })
}

/// Whether `condition` compares a value with its previous one, and reads no bars.
fn reads_previous(condition: &AlertCondition) -> bool {
    match condition {
        AlertCondition::Crossing(_)
        | AlertCondition::CrossingUp(_)
        | AlertCondition::CrossingDown(_)
        | AlertCondition::EnteringChannel { .. }
        | AlertCondition::ExitingChannel { .. } => true,
        AlertCondition::Compare { comparison, .. } => matches!(
            comparison,
            AlertComparison::Crossing | AlertComparison::CrossingUp | AlertComparison::CrossingDown
        ),
        AlertCondition::Channel { comparison, .. } => matches!(
            comparison,
            AlertChannelComparison::Entering | AlertChannelComparison::Exiting
        ),
        _ => false,
    }
}

/// What a frequency fires at most once per: the bar, or for a per-minute alert the minute of `now`.
fn occurrence(frequency: AlertFrequency, bar: &Bar, now: DateTime<Utc>) -> DateTime<Utc> {
    match frequency {
        AlertFrequency::OncePerMinute => {
            DateTime::from_timestamp(now.timestamp().div_euclid(60) * 60, 0).unwrap_or(now)
        }
        _ => bar.time,
    }
}

/// Whether `condition` holds for `value`, given the previous value of the source, the current and
/// previous value of the other output for a comparison, the bars up to the evaluated one and the
/// value of the source `back` bars before it.
fn condition_holds(
    condition: &AlertCondition,
    value: f64,
    previous: [Option<f64>; 3],
    other: [Option<f64>; 2],
    bars: &[Bar],
    lookback: impl Fn(u32) -> Option<f64>,
) -> bool {
    let [previous, previous_other, previous_second] = previous;
    match condition {
        AlertCondition::StrategyFills(_) => false,
        AlertCondition::Crossing(level) => compare(
            AlertComparison::Crossing,
            value,
            previous,
            *level,
            Some(*level),
        ),
        AlertCondition::CrossingUp(level) => compare(
            AlertComparison::CrossingUp,
            value,
            previous,
            *level,
            Some(*level),
        ),
        AlertCondition::CrossingDown(level) => compare(
            AlertComparison::CrossingDown,
            value,
            previous,
            *level,
            Some(*level),
        ),
        AlertCondition::Above(level) => value > *level,
        AlertCondition::Below(level) => value < *level,
        AlertCondition::EnteringChannel { lower, upper } => previous
            .is_some_and(|p| (p < *lower || p > *upper) && value >= *lower && value <= *upper),
        AlertCondition::ExitingChannel { lower, upper } => previous
            .is_some_and(|p| p >= *lower && p <= *upper && (value < *lower || value > *upper)),
        AlertCondition::InsideChannel { lower, upper } => value >= *lower && value <= *upper,
        AlertCondition::OutsideChannel { lower, upper } => value < *lower || value > *upper,
        AlertCondition::MovingBy {
            value: change,
            bars: back,
        } => lookback(*back).is_some_and(|p| reaches(value - p, *change)),
        AlertCondition::MovingByPercent {
            percent,
            bars: back,
        } => lookback(*back)
            .is_some_and(|p| p != 0.0 && reaches(100.0 * (value - p) / p.abs(), *percent)),
        AlertCondition::Compare { comparison, .. } => other[0]
            .is_some_and(|level| compare(*comparison, value, previous, level, previous_other)),
        AlertCondition::Channel { comparison, .. } => {
            let inside = |value: f64, a: f64, b: f64| {
                (a != b).then_some(value >= a.min(b) && value <= a.max(b))
            };
            let Some(current) = other[0]
                .zip(other[1])
                .and_then(|(a, b)| inside(value, a, b))
            else {
                return false;
            };
            let previous = previous
                .zip(previous_other)
                .zip(previous_second)
                .and_then(|((value, a), b)| inside(value, a, b));
            match comparison {
                AlertChannelComparison::Entering => previous == Some(false) && current,
                AlertChannelComparison::Exiting => previous == Some(true) && !current,
                AlertChannelComparison::Inside => current,
                AlertChannelComparison::Outside => !current,
            }
        }
        AlertCondition::VolumeAboveAverage {
            multiplier,
            bars: count,
        } => {
            let n = *count as usize;
            if n == 0 || bars.len() <= n {
                return false;
            }
            let mean = bars[bars.len() - n - 1..bars.len() - 1]
                .iter()
                .map(|b| b.volume)
                .sum::<f64>()
                / n as f64;
            mean.is_finite() && bars[bars.len() - 1].volume > multiplier * mean
        }
    }
}

fn compare(
    comparison: AlertComparison,
    value: f64,
    previous: Option<f64>,
    level: f64,
    previous_level: Option<f64>,
) -> bool {
    let sides = previous.zip(previous_level);
    let up = sides.is_some_and(|(p, l)| p < l && value >= level);
    let down = sides.is_some_and(|(p, l)| p > l && value <= level);
    match comparison {
        AlertComparison::Crossing => up || down,
        AlertComparison::CrossingUp => up,
        AlertComparison::CrossingDown => down,
        AlertComparison::Above => value > level,
        AlertComparison::Below => value < level,
    }
}

/// Whether a signed `change` reaches `target`: at least for a positive one, at most for a negative.
fn reaches(change: f64, target: f64) -> bool {
    if target > 0.0 {
        change >= target
    } else {
        change <= target
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::inbox::Inbox;
    use crate::services::events::Listener;
    use std::sync::Arc;

    fn time(seconds: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(seconds, 0).unwrap()
    }

    /// A one-minute bar opening at minute `minute` that opens, highs, lows and closes at `close`.
    fn bar(minute: i64, close: f64, volume: f64) -> Bar {
        Bar::new(time(minute * 60), close, close, close, close, volume)
    }

    fn alert(condition: AlertCondition, frequency: AlertFrequency) -> Alert {
        let mut alert = Alert::new("SYM", condition);
        alert.frequency = frequency;
        alert.timeframe = Some(Timeframe::Min1);
        alert
    }

    /// An engine with `alerts` and the events it pushes since the last call of `events`.
    struct Host {
        engine: AlertEngine,
        inbox: Arc<Inbox>,
        _listener: Listener,
        delivered: Arc<std::sync::Mutex<Vec<(Alert, AlertEvent)>>>,
    }

    impl Host {
        fn new(alerts: Vec<Alert>) -> Self {
            let inbox = Arc::new(Inbox::default());
            let (_listener, events) = Listener::new(&inbox, 1);
            let delivered = Arc::new(std::sync::Mutex::new(Vec::new()));
            let sink = Arc::clone(&delivered);
            let engine = AlertEngine::new()
                .with_alerts(alerts)
                .on_trigger(move |alert, event| {
                    sink.lock().unwrap().push((alert.clone(), event.clone()));
                });
            engine.watch(events);
            Self {
                engine,
                inbox,
                _listener,
                delivered,
            }
        }

        fn events(&self) -> Vec<AlertEvent> {
            self.inbox
                .drain()
                .answers
                .into_iter()
                .filter(|(key, _)| *key == 1)
                .map(|(_, event)| *event.downcast::<AlertEvent>().unwrap())
                .collect()
        }

        /// The prices of the triggers pushed since the last call.
        fn fired(&self) -> Vec<f64> {
            self.events()
                .into_iter()
                .filter_map(|event| match event {
                    AlertEvent::TriggeredForSymbol { price, .. } => price,
                    _ => None,
                })
                .collect()
        }

        fn save(&self, alert: Alert) -> Result<(), FeedError> {
            self.engine.save(alert, self.inbox.reply(0));
            let answers = self.inbox.drain().answers;
            let reply = answers.into_iter().find(|(key, _)| *key == 0).unwrap().1;
            *reply.downcast::<Result<(), FeedError>>().unwrap()
        }
    }

    /// Feeds `bars` with the study value `study` of each bar, as one update.
    fn feed(host: &Host, bars: &[Bar], study: &[f64], now: i64) {
        host.engine
            .update("SYM", None, Timeframe::Min1, bars, time(now), |_, at| {
                study.get(at).copied()
            });
    }

    /// A close, a volume and a study value; the history and the ticks of one row, and the ticks
    /// on which the alert fires.
    type Tick = (f64, f64, f64);
    type Row = (AlertCondition, Vec<Tick>, Vec<Tick>, Vec<usize>);

    #[test]
    fn crossings_follow_the_previous_value_tick_to_tick() {
        let study = AlertSource::Study {
            id: "s".into(),
            output: 0,
        };
        // History and ticks are (close, volume, study value). Each tick is one update of the
        // forming bar after the history; the rows list the ticks on which the alert fires.
        let n = f64::NAN;
        let rows: Vec<Row> = vec![
            // Touching the level and staying fires once; the repeated tick changes nothing.
            (
                AlertCondition::CrossingUp(100.0),
                vec![(99.0, 10.0, n)],
                [99.5, 100.0, 100.0, 101.0, 99.0, 100.5]
                    .map(|c| (c, 10.0, n))
                    .to_vec(),
                vec![1, 5],
            ),
            (
                AlertCondition::CrossingDown(100.0),
                vec![(101.0, 10.0, n)],
                [100.0, 100.0, 99.0, 101.0, 100.0]
                    .map(|c| (c, 10.0, n))
                    .to_vec(),
                vec![0, 4],
            ),
            (
                AlertCondition::Crossing(100.0),
                vec![(99.0, 10.0, n)],
                [101.0, 99.0, 99.5, 100.0].map(|c| (c, 10.0, n)).to_vec(),
                vec![0, 1, 3],
            ),
            // A missing value fails its rule and leaves no previous value: the next tick cannot
            // cross, while a level needing no previous value holds at once.
            (
                AlertCondition::CrossingUp(100.0),
                vec![(99.0, 10.0, n)],
                [n, 101.0, 99.0, 101.0].map(|c| (c, 10.0, n)).to_vec(),
                vec![3],
            ),
            (
                AlertCondition::Above(100.0),
                vec![(99.0, 10.0, n)],
                [n, 101.0, 100.0].map(|c| (c, 10.0, n)).to_vec(),
                vec![1],
            ),
            // A repeated tick changes nothing, even when the volume is not a number.
            (
                AlertCondition::Above(100.0),
                vec![(99.0, n, n)],
                [101.0, 101.0].map(|c| (c, n, n)).to_vec(),
                vec![0],
            ),
            // The other output moves too: its previous value is the one of the previous tick.
            (
                AlertCondition::Compare {
                    other: study.clone(),
                    comparison: AlertComparison::CrossingUp,
                },
                vec![(99.0, 10.0, 100.0)],
                vec![
                    (101.0, 10.0, 100.0),
                    (101.0, 10.0, 102.0),
                    (103.0, 10.0, 102.0),
                ],
                vec![0, 2],
            ),
            (
                AlertCondition::EnteringChannel {
                    lower: 100.0,
                    upper: 110.0,
                },
                vec![(95.0, 10.0, n)],
                [100.0, 105.0, 111.0, 110.0].map(|c| (c, 10.0, n)).to_vec(),
                vec![0, 3],
            ),
            (
                AlertCondition::ExitingChannel {
                    lower: 100.0,
                    upper: 110.0,
                },
                vec![(105.0, 10.0, n)],
                [110.0, 111.0, 105.0, 99.0].map(|c| (c, 10.0, n)).to_vec(),
                vec![1, 3],
            ),
            (
                AlertCondition::InsideChannel {
                    lower: 100.0,
                    upper: 110.0,
                },
                vec![],
                [100.0, 105.0, 110.0, 111.0, 99.0]
                    .map(|c| (c, 10.0, n))
                    .to_vec(),
                vec![0, 1, 2],
            ),
            (
                AlertCondition::OutsideChannel {
                    lower: 100.0,
                    upper: 110.0,
                },
                vec![],
                [99.0, 100.0, 105.0, 110.0, 111.0]
                    .map(|c| (c, 10.0, n))
                    .to_vec(),
                vec![0, 4],
            ),
            // Moving by reads the value `bars` bars before the evaluated bar, not the previous tick.
            (
                AlertCondition::MovingBy {
                    value: 10.0,
                    bars: 2,
                },
                vec![(100.0, 10.0, n), (100.0, 10.0, n), (100.0, 10.0, n)],
                [105.0, 110.0, 111.0].map(|c| (c, 10.0, n)).to_vec(),
                vec![1, 2],
            ),
            (
                AlertCondition::MovingBy {
                    value: -10.0,
                    bars: 1,
                },
                vec![(100.0, 10.0, n)],
                [95.0, 90.0].map(|c| (c, 10.0, n)).to_vec(),
                vec![1],
            ),
            (
                AlertCondition::MovingByPercent {
                    percent: 10.0,
                    bars: 1,
                },
                vec![(100.0, 10.0, n)],
                [109.0, 110.0].map(|c| (c, 10.0, n)).to_vec(),
                vec![1],
            ),
            (
                AlertCondition::MovingByPercent {
                    percent: -10.0,
                    bars: 1,
                },
                vec![(-100.0, 10.0, n)],
                [-109.0, -110.0].map(|c| (c, 10.0, n)).to_vec(),
                vec![1],
            ),
            (
                AlertCondition::MovingByPercent {
                    percent: 10.0,
                    bars: 1,
                },
                vec![(0.0, 10.0, n)],
                [1.0, 5.0].map(|c| (c, 10.0, n)).to_vec(),
                vec![],
            ),
            // The mean of the three bars before the forming one is 20, so it takes more than 40.
            (
                AlertCondition::VolumeAboveAverage {
                    multiplier: 2.0,
                    bars: 3,
                },
                vec![(100.0, 10.0, n), (100.0, 20.0, n), (100.0, 30.0, n)],
                [50.0, 40.0, 41.0].map(|v| (100.0, v, n)).to_vec(),
                vec![0, 2],
            ),
        ];
        for (row, (condition, history, ticks, expected)) in rows.into_iter().enumerate() {
            let host = Host::new(vec![alert(condition, AlertFrequency::EveryTime)]);
            host.events();
            let mut fired = Vec::new();
            for (tick, current) in ticks.iter().enumerate() {
                let bars: Vec<Bar> = history
                    .iter()
                    .chain([current])
                    .enumerate()
                    .map(|(minute, (close, volume, _))| bar(minute as i64, *close, *volume))
                    .collect();
                let values: Vec<f64> = history.iter().chain([current]).map(|t| t.2).collect();
                feed(&host, &bars, &values, 1_000 + tick as i64);
                if !host.fired().is_empty() {
                    fired.push(tick);
                }
            }
            assert_eq!(fired, expected, "row {row}");
        }
    }

    #[test]
    fn independent_intervals_join_only_overlapping_observations() {
        for frequency in [
            AlertFrequency::EveryTime,
            AlertFrequency::OncePerBar,
            AlertFrequency::OncePerBarClose,
        ] {
            for reverse in [false, true] {
                let mut watching = alert(AlertCondition::Above(100.0), frequency);
                watching.timeframe = Some(Timeframe::Min5);
                let mut stored = serde_json::to_value(&watching).unwrap();
                stored["additional_timeframes"] = serde_json::json!(["Min1"]);
                stored["additional_conditions"] = serde_json::json!([{
                    "source": "Volume", "condition": {"Above": 5.0}
                }]);
                let host = Host::new(vec![serde_json::from_value(stored).unwrap()]);
                host.events();
                let closed = frequency == AlertFrequency::OncePerBarClose;
                let send = |tf, minute, close, volume| {
                    let mut bars = vec![bar(minute, close, volume)];
                    if closed && tf == Timeframe::Min1 {
                        bars.push(bar(minute + 1, 1.0, 1.0));
                    }
                    host.engine
                        .update("SYM", None, tf, &bars, time(1000), |_, _| None);
                };
                let slow = || send(Timeframe::Min5, 0, 101.0, 1.0);
                let fast = || send(Timeframe::Min1, 0, 99.0, 10.0);
                if reverse {
                    fast();
                    slow();
                } else {
                    slow();
                    fast();
                }
                assert_eq!(host.fired().len(), 1, "first join: {frequency}, {reverse}");
                slow();
                fast();
                assert!(host.fired().is_empty(), "repeated observations");
                send(Timeframe::Min1, 1, 99.0, 10.0);
                assert_eq!(
                    host.fired().len(),
                    1,
                    "frequency follows the smaller interval"
                );
                if closed {
                    send(Timeframe::Min1, 4, 99.0, 10.0);
                    assert_eq!(host.fired().len(), 1, "closes with the slow bar");
                }
                send(Timeframe::Min1, 5, 99.0, 10.0);
                assert!(host.fired().is_empty(), "expired slow observation");
                send(Timeframe::Min5, 5, 101.0, 1.0);
                assert_eq!(host.fired().len(), 1, "late matching interval");
                send(Timeframe::Min5, 10, 101.0, 1.0);
                assert!(
                    host.fired().is_empty(),
                    "future interval cannot use an older fast observation"
                );
                send(Timeframe::Min1, if closed { 9 } else { 10 }, 99.0, 10.0);
                assert_eq!(host.fired().len(), 1);
                send(Timeframe::Min1, 5, 99.0, 10.0);
                assert!(host.fired().is_empty(), "rewind");
            }
        }
        // A failed close is consumed: a later change of the larger interval waits for the next close.
        let stored = serde_json::json!({
            "symbol":"SYM", "timeframe":"Min5", "condition":{"Above":100.0},
            "frequency":"OncePerBarClose", "additional_timeframes":["Min1"],
            "additional_conditions":[{"source":"Volume","condition":{"Above":5.0}}]
        });
        let host = Host::new(vec![serde_json::from_value(stored.clone()).unwrap()]);
        host.events();
        let slow = |close| {
            host.engine.update(
                "SYM",
                None,
                Timeframe::Min5,
                &[bar(0, close, 1.0)],
                time(1000),
                |_, _| None,
            )
        };
        let fast = |minute| {
            host.engine.update(
                "SYM",
                None,
                Timeframe::Min1,
                &[bar(minute, 99.0, 10.0), bar(minute + 1, 99.0, 0.0)],
                time(1000),
                |_, _| None,
            )
        };
        slow(99.0);
        fast(0);
        assert!(host.fired().is_empty());
        slow(101.0);
        assert!(
            host.fired().is_empty(),
            "do not reconsider an already evaluated close"
        );
        fast(1);
        assert_eq!(host.fired().len(), 1);

        // A missing input blocks a cached close, preserving its comparison or crossing on recovery.
        for condition in [
            serde_json::json!({"Above":5.0}),
            serde_json::json!({"CrossingUp":5.0}),
        ] {
            let mut stored = stored.clone();
            stored["additional_conditions"][0]["condition"] = condition;
            let host = Host::new(vec![serde_json::from_value(stored).unwrap()]);
            host.events();
            let fast = [bar(-1, 99.0, 0.0), bar(0, 99.0, 10.0), bar(1, 99.0, 0.0)];
            host.engine
                .update("SYM", None, Timeframe::Min1, &fast, time(1000), |_, _| None);
            host.engine
                .update("SYM", None, Timeframe::Min1, &[], time(1000), |_, _| None);
            host.engine.update(
                "SYM",
                None,
                Timeframe::Min5,
                &[bar(0, 101.0, 1.0)],
                time(1000),
                |_, _| None,
            );
            assert!(
                host.fired().is_empty(),
                "missing interval blocks cached observations"
            );
            host.engine
                .update("SYM", None, Timeframe::Min1, &fast, time(1000), |_, _| None);
            assert_eq!(host.fired().len(), 1, "restored close is still eligible");
        }

        // Equivalent interval spellings close together, rather than mixing a closed and a forming bar.
        let stored = serde_json::json!({
            "symbol":"SYM", "timeframe":"Min1", "condition":{"Above":100.0},
            "frequency":"OncePerBarClose", "additional_timeframes":[{"Custom":60}],
            "additional_conditions":[{"source":"Volume","condition":{"Above":5.0}}]
        });
        let host = Host::new(vec![serde_json::from_value(stored).unwrap()]);
        host.events();
        host.engine.update(
            "SYM",
            None,
            Timeframe::Min1,
            &[bar(0, 101.0, 1.0), bar(1, 99.0, 0.0)],
            time(1000),
            |_, _| None,
        );
        host.engine.update(
            "SYM",
            None,
            "60s".parse().unwrap(),
            &[bar(0, 99.0, 10.0), bar(1, 99.0, 0.0)],
            time(1000),
            |_, _| None,
        );
        assert_eq!(host.fired().len(), 1);
    }

    #[test]
    fn frequencies_count_ticks_bars_and_closed_bars() {
        // Four frames of one rising feed: the first bar forming at 101 and then 102, then a new
        // bar forming at 103 and then 104. Each trigger reports the close of the evaluated bar.
        let frames = [
            vec![bar(0, 99.0, 1.0), bar(1, 101.0, 1.0)],
            vec![bar(0, 99.0, 1.0), bar(1, 102.0, 1.0)],
            vec![bar(0, 99.0, 1.0), bar(1, 102.0, 1.0), bar(2, 103.0, 1.0)],
            vec![bar(0, 99.0, 1.0), bar(1, 102.0, 1.0), bar(2, 104.0, 1.0)],
        ];
        for (frequency, expected, status) in [
            (AlertFrequency::Once, vec![101.0], AlertStatus::Triggered),
            (
                AlertFrequency::EveryTime,
                vec![101.0, 102.0, 103.0, 104.0],
                AlertStatus::Active,
            ),
            (
                AlertFrequency::OncePerBar,
                vec![101.0, 103.0],
                AlertStatus::Active,
            ),
            // The completed bars are the first, which is below the level, and then the second.
            (
                AlertFrequency::OncePerBarClose,
                vec![102.0],
                AlertStatus::Active,
            ),
        ] {
            let host = Host::new(vec![alert(AlertCondition::Above(100.0), frequency)]);
            host.events();
            let mut fired = Vec::new();
            for (n, frame) in frames.iter().enumerate() {
                feed(&host, frame, &[], 1_000 + n as i64);
                fired.extend(host.fired());
            }
            assert_eq!(fired, expected, "{frequency}");
            assert_eq!(host.engine.alerts()[0].status, status, "{frequency}");
        }
    }

    #[test]
    fn schedules_gate_channel_states_without_consuming_occurrences() {
        use crate::model::{AlertRule, NotificationSchedule, NotificationWindow, TimezoneId};
        use chrono::{NaiveTime, Weekday};

        for condition in [
            AlertCondition::InsideChannel {
                lower: 100.0,
                upper: 110.0,
            },
            AlertCondition::OutsideChannel {
                lower: 90.0,
                upper: 99.0,
            },
        ] {
            for frequency in [
                AlertFrequency::Once,
                AlertFrequency::EveryTime,
                AlertFrequency::OncePerBar,
                AlertFrequency::OncePerBarClose,
                AlertFrequency::OncePerMinute,
            ] {
                let mut watching = alert(condition.clone(), frequency);
                watching.additional_conditions.push(AlertRule {
                    source: AlertSource::Volume,
                    condition: AlertCondition::Above(5.0),
                });
                watching.schedule = Some(Box::new(NotificationSchedule {
                    timezone: TimezoneId::utc(),
                    days: vec![Weekday::Thu],
                    windows: vec![NotificationWindow {
                        start: NaiveTime::from_hms_opt(0, 1, 0).unwrap(),
                        end: NaiveTime::from_hms_opt(0, 2, 0).unwrap(),
                    }],
                }));
                let host = Host::new(vec![watching]);
                host.events();
                let bars = [bar(-2, 100.0, 10.0), bar(-1, 100.0, 10.0)];
                feed(&host, &bars, &[], 59);
                // Outside the window the trigger is only logged, once, and nothing is delivered.
                let events = host.events();
                assert!(
                    matches!(events.as_slice(), [AlertEvent::SilentTriggered { price: Some(p), .. }] if *p == 100.0),
                    "{condition:?}, {frequency}: {events:?}"
                );
                feed(&host, &bars, &[], 59);
                assert!(host.events().is_empty());
                assert_eq!(host.engine.alerts()[0].status, AlertStatus::Active);
                assert!(host.delivered.lock().unwrap().is_empty());

                // At the opening, the same forming bar may fire. A completed bar needs a new close.
                let next = if frequency == AlertFrequency::OncePerBarClose {
                    vec![bars[0].clone(), bars[1].clone(), bar(0, 100.0, 10.0)]
                } else {
                    bars.to_vec()
                };
                feed(&host, &next, &[], 60);
                assert_eq!(host.fired(), [100.0], "{condition:?}, {frequency}");
                feed(&host, &next, &[], 61);
                assert!(host.fired().is_empty());
                assert_eq!(host.delivered.lock().unwrap().len(), 1);

                let closed = [bar(0, 101.0, 10.0), bar(1, 102.0, 10.0)];
                feed(&host, &closed, &[], 120);
                assert!(host.fired().is_empty(), "the end of a window is excluded");
            }
        }
    }

    #[test]
    fn minute_frequency_uses_clock_minutes_per_watchlist_member() {
        let mut watching = alert(
            AlertCondition::InsideChannel {
                lower: 100.0,
                upper: 110.0,
            },
            AlertFrequency::OncePerMinute,
        );
        watching.watchlist = Some("list".into());
        let host = Host::new(vec![watching]);
        host.engine
            .set_watchlist("list", vec!["SYM".into(), "OTHER".into()]);
        host.events();
        // UTC seconds -61, -60 and 0 belong to minutes 23:58, 23:59 and 00:00.
        // The chart bar never changes; neither boundaries nor occurrence counts use its time.
        for (now, count) in [
            (-61, 2),
            (-60, 2),
            (-1, 0),
            (0, 2),
            (59, 0),
            (60, 2),
            // A corrected clock must not fire a minute a second time.
            (0, 0),
            (60, 0),
            (120, 2),
        ] {
            for symbol in ["SYM", "OTHER"] {
                host.engine.update(
                    symbol,
                    None,
                    Timeframe::Min1,
                    &[bar(-5, 100.0, 1.0)],
                    time(now),
                    |_, _| None,
                );
            }
            assert_eq!(host.fired().len(), count, "second {now}");
        }
    }

    #[test]
    fn a_trigger_follows_the_change_of_status_and_reloads_never_repeat_it() {
        let mut watching = alert(AlertCondition::Above(100.0), AlertFrequency::EveryTime);
        watching.timeframe = None;
        watching.message = "{{symbol}} closed at {{close}} on {{trigger_time}}".into();
        let host = Host::new(vec![watching]);
        host.events();
        let bars = [bar(0, 99.0, 1.0), bar(1, 101.0, 1.0), bar(2, 103.0, 1.0)];
        feed(&host, &bars, &[], 1_000);
        let events = host.events();
        // The first update fixes the timeframe, then the trigger comes.
        assert!(
            matches!(&events[0], AlertEvent::Upsert(a) if a.timeframe == Some(Timeframe::Min1))
        );
        assert!(
            matches!(&events[1], AlertEvent::TriggeredForSymbol { price: Some(p), .. } if *p == 103.0)
        );
        assert_eq!(events.len(), 2);
        let saved = serde_json::to_value(host.engine.alerts()[0].clone()).unwrap();
        assert!(saved["created_at"].is_string());
        assert_eq!(
            saved["last_triggered_at"],
            serde_json::to_value(time(1_000)).unwrap()
        );
        // The message is filled in the event, and the host callback gets the same event.
        assert!(
            matches!(&events[1], AlertEvent::TriggeredForSymbol { message, .. } if *message == format!("SYM closed at 103 on {}", time(1_000).to_rfc3339()))
        );
        let delivered = host.delivered.lock().unwrap().clone();
        assert_eq!(delivered.len(), 1);
        assert_eq!(delivered[0].1, events[1]);
        feed(&host, &bars, &[], 1_001);
        for (line, blink, push) in [
            (true, false, false),
            (false, true, false),
            (false, true, true),
        ] {
            let mut edited = saved.clone();
            edited["extend_line"] = line.into();
            edited["blink_on_trigger"] = blink.into();
            edited["delivery"]["push"] = push.into();
            let alert: Alert = serde_json::from_value(edited).unwrap();
            assert_eq!(host.save(alert), Ok(()));
            feed(&host, &bars, &[], 1_001);
            assert!(
                host.fired().is_empty(),
                "display and delivery changes must preserve occurrences"
            );
            let kept = serde_json::to_value(host.engine.alerts()[0].clone()).unwrap();
            assert_eq!(kept["created_at"], saved["created_at"]);
            assert_eq!(kept["last_triggered_at"], saved["last_triggered_at"]);
        }
        // History that grew at the front, a rewound replay and another timeframe: nothing fires.
        let mut reloaded = vec![bar(-1, 98.0, 1.0)];
        reloaded.extend(bars.clone());
        feed(&host, &reloaded, &[], 1_002);
        feed(&host, &bars[..2], &[], 1_003);
        host.engine
            .update("SYM", None, Timeframe::Min5, &bars, time(1_004), |_, _| {
                None
            });
        assert!(host.events().is_empty());
    }

    /// An alert stored before names and messages had limits, here with a 4001-character
    /// message, loads with `with_alerts` and keeps working.
    #[test]
    fn an_alert_stored_with_a_long_message_keeps_firing_and_can_be_paused() {
        let message = "é".repeat(Alert::MAX_MESSAGE_CHARS + 1);
        let mut stored = alert(AlertCondition::Above(100.0), AlertFrequency::EveryTime);
        stored.message.clone_from(&message);
        let host = Host::new(vec![stored]);
        host.events();
        let id = host.engine.alerts()[0].id.clone().unwrap();
        let subjects = host.engine.subjects(&id);
        feed(&host, &[bar(0, 99.0, 1.0), bar(1, 101.0, 1.0)], &[], 61);
        let fired: Vec<_> = host
            .events()
            .into_iter()
            .filter_map(|event| match event {
                AlertEvent::TriggeredForSymbol { price, message, .. } => Some((price, message)),
                _ => None,
            })
            .collect();
        let mut paused = host.engine.alerts()[0].clone();
        paused.status = AlertStatus::Disabled;
        let pause = host.save(paused);
        assert_eq!(
            (subjects, fired, pause),
            (vec!["SYM".to_owned()], vec![(Some(101.0), message)], Ok(()))
        );
    }

    #[cfg(all(feature = "ui", not(target_arch = "wasm32")))]
    #[test]
    fn dropping_a_feed_under_the_context_lock_does_not_wake_cancelled_replies() {
        use crate::services::AlertFeed;
        #[derive(Default)]
        struct Feed(Mutex<Option<Reply<crate::data::HistoryPage>>>);
        impl crate::data::Datafeed for Feed {
            fn history(
                &self,
                _: crate::data::HistoryRequest,
                reply: Reply<crate::data::HistoryPage>,
            ) {
                *self.0.lock().unwrap() = Some(reply);
            }
            fn unsubscribe(&self, _: crate::data::SubscriptionId) {
                drop(self.0.lock().unwrap().take());
            }
        }
        let (done, finished) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            let ctx = egui::Context::default();
            let host = Arc::new(Feed::default());
            let mut feed = AlertFeed::new(
                host.clone(),
                AlertFeed::history_request("SYM", Timeframe::Min1),
                None,
            );
            assert!(feed.poll(&ctx, time(1000)).is_empty());
            assert!(host.0.lock().unwrap().is_some());
            ctx.input(|_| drop(feed));
            done.send(()).unwrap();
        });
        finished
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("dropping cancelled replies must not wait for the context lock");
        worker.join().unwrap();
    }

    #[test]
    fn expired_paused_and_watchlist_alerts() {
        let bars = [bar(0, 99.0, 1.0), bar(1, 101.0, 1.0)];
        // Expiry does not wait for data, and an expired alert never fires.
        let mut expiring = alert(AlertCondition::Above(100.0), AlertFrequency::EveryTime);
        expiring.expires = Some(time(2_000));
        let host = Host::new(vec![expiring]);
        host.events();
        host.engine.tick(time(2_000));
        assert!(
            matches!(&host.events()[..], [AlertEvent::Upsert(a)] if a.status == AlertStatus::Expired)
        );
        feed(&host, &bars, &[], 2_001);
        assert!(host.events().is_empty());

        // An alert with a start time ignores data until then.
        let mut timed = alert(AlertCondition::Above(100.0), AlertFrequency::EveryTime);
        timed.starts = Some(time(1_000));
        let host = Host::new(vec![timed]);
        host.events();
        feed(&host, &bars, &[], 999);
        assert!(host.events().is_empty());
        feed(&host, &bars, &[], 1_000);
        assert_eq!(host.fired(), [101.0]);

        // A disabled alert is ignored; saving it active again starts with a first observation.
        let mut paused = alert(AlertCondition::CrossingUp(100.0), AlertFrequency::Once);
        paused.status = AlertStatus::Disabled;
        let host = Host::new(vec![paused.clone()]);
        host.events();
        feed(&host, &bars, &[], 1_000);
        assert!(host.events().is_empty());
        paused.id = host.engine.alerts()[0].id.clone();
        paused.status = AlertStatus::Active;
        host.save(paused).unwrap();
        host.events();
        feed(&host, &bars, &[], 1_001);
        // Once reports the new status before the trigger.
        let events = host.events();
        assert!(matches!(&events[0], AlertEvent::Upsert(a) if a.status == AlertStatus::Triggered));
        assert!(
            matches!(&events[1], AlertEvent::TriggeredForSymbol { symbol, time: t, price: Some(p), .. }
            if symbol == "SYM" && *t == time(1_001) && *p == 101.0)
        );

        // A watchlist alert fires once for each member and stays active.
        let mut list = alert(AlertCondition::Above(100.0), AlertFrequency::Once);
        list.symbol = String::new();
        list.watchlist = Some("tech".into());
        let host = Host::new(vec![list]);
        host.engine
            .set_watchlist("tech", vec!["SYM".into(), "OTHER".into()]);
        host.events();
        for symbol in ["SYM", "SYM", "OTHER", "NOT_A_MEMBER"] {
            host.engine
                .update(symbol, None, Timeframe::Min1, &bars, time(1_000), |_, _| {
                    None
                });
        }
        let symbols: Vec<_> = host
            .events()
            .into_iter()
            .filter_map(|event| match event {
                AlertEvent::TriggeredForSymbol { symbol, .. } => Some(symbol),
                _ => None,
            })
            .collect();
        assert_eq!(symbols, ["SYM", "OTHER"]);
        assert_eq!(host.engine.alerts()[0].status, AlertStatus::Active);

        // Invalid alerts are refused.
        assert!(matches!(
            host.save(Alert::new("SYM", AlertCondition::Above(f64::NAN))),
            Err(FeedError::InvalidData(_))
        ));

        // A restored alert without an id never takes the id of a later one.
        let mut kept = Alert::new("B", AlertCondition::Above(1.0));
        kept.id = Some(AlertId("1".into()));
        let restored =
            AlertEngine::new().with_alerts(vec![Alert::new("A", AlertCondition::Above(1.0)), kept]);
        assert_eq!(restored.alerts().len(), 2);
    }

    #[test]
    fn moving_channels_resolve_both_boundaries_and_forget_missing_values() {
        // Price stays at 105. Bounds rise 10/bar: [90,100], [100,110], [110,120].
        let rising = |source: &AlertSource, at: usize| {
            Some(
                90.0 + at as f64 * 10.0
                    + if *source == AlertSource::Drawing("d#1".into()) {
                        10.0
                    } else {
                        0.0
                    },
            )
        };
        for (comparison, expected) in [
            ("Entering", vec![1]),
            ("Exiting", vec![2]),
            ("Inside", vec![1]),
            ("Outside", vec![0, 2]),
        ] {
            let condition: AlertCondition = serde_json::from_value(serde_json::json!({
                "Channel": {"boundaries": [{"Drawing":"d"}, {"Drawing":"d#1"}], "comparison": comparison}
            })).unwrap();
            let watching = alert(condition, AlertFrequency::EveryTime);
            assert_eq!(watching.sources().count(), 3);
            let restored: Alert =
                serde_json::from_str(&serde_json::to_string(&watching).unwrap()).unwrap();
            let host = Host::new(vec![restored]);
            host.events();
            let bars: Vec<_> = (0..3).map(|i| bar(i, 105.0, 1.0)).collect();
            let mut fired = Vec::new();
            for i in 0..3 {
                host.engine.update(
                    "SYM",
                    None,
                    Timeframe::Min1,
                    &bars[..=i],
                    time(1000 + i as i64),
                    rising,
                );
                if !host.fired().is_empty() {
                    fired.push(i);
                }
            }
            assert_eq!(fired, expected, "{comparison}");
            // A same-bar edit moves both bounds, then one disappears. Recovery cannot invent a transition.
            for (bounds, fires) in [
                (
                    [Some(100.0), Some(110.0)],
                    comparison == "Entering" || comparison == "Inside",
                ),
                ([Some(100.0), None], false),
                ([Some(200.0), Some(210.0)], comparison == "Outside"),
                ([None, None], false),
                ([Some(f64::NAN), Some(210.0)], false),
                ([Some(105.0), Some(105.0)], false),
                ([Some(110.0), Some(100.0)], comparison == "Inside"),
            ] {
                host.engine.update(
                    "SYM",
                    None,
                    Timeframe::Min1,
                    &bars,
                    time(1010),
                    |source, _| bounds[usize::from(*source == AlertSource::Drawing("d#1".into()))],
                );
                assert_eq!(!host.fired().is_empty(), fires, "{comparison} {bounds:?}");
            }
            // A later bar uses the edited drawing at the preceding bar, even after skipped updates.
            for (last, current) in [(0, 2), (1, 2)] {
                let host = Host::new(vec![watching.clone()]);
                host.events();
                host.engine.update(
                    "SYM",
                    None,
                    Timeframe::Min1,
                    &bars[..=last],
                    time(1020),
                    |source, _| {
                        Some(if *source == AlertSource::Drawing("d#1".into()) {
                            210.0
                        } else {
                            200.0
                        })
                    },
                );
                host.fired();
                host.engine.update(
                    "SYM",
                    None,
                    Timeframe::Min1,
                    &bars[..=current],
                    time(1030),
                    rising,
                );
                assert_eq!(
                    !host.fired().is_empty(),
                    comparison == "Exiting" || comparison == "Outside",
                    "{comparison}: changed geometry after bar {last}",
                );
            }
            // First evaluation on bar 1 must use bar 0's bounds, not bar 1's bounds twice.
            let host = Host::new(host.engine.alerts());
            host.events();
            host.engine
                .update("SYM", None, Timeframe::Min1, &bars[..2], time(1020), rising);
            assert_eq!(
                !host.fired().is_empty(),
                comparison == "Entering" || comparison == "Inside"
            );
        }
    }

    #[test]
    fn longer_interval_crossing_fires_once_across_shorter_heartbeat_bars() {
        // The five-minute close crosses 100 once (99, then 101) and stays there. One-minute bars 3
        // and 4 are empty heartbeat candles, so only the one-minute input changes in those polls.
        let host = Host::new(vec![
            serde_json::from_value(serde_json::json!({
                "symbol":"SYM", "timeframe":"Min5", "condition":{"CrossingUp":100.0},
                "frequency":"OncePerBar", "additional_timeframes":["Min1"],
                "additional_conditions":[{"source":"Price","condition":{"Above":50.0}}]
            }))
            .unwrap(),
        ]);
        host.events();
        let (five, one) = (
            BarClock::new(Timeframe::Min5),
            BarClock::new(Timeframe::Min1),
        );
        let poll = |slow: &[Bar], fast: &[Bar], now| {
            host.engine.update_matching(
                None,
                "SYM",
                None,
                &[(&five, slow), (&one, fast)],
                time(now),
                |_, _, _| None,
            );
            host.fired()
        };
        let mut fired = poll(
            &[bar(-5, 99.0, 10.0), bar(0, 99.0, 10.0)],
            &[bar(1, 99.0, 5.0), bar(2, 99.0, 5.0)],
            150,
        );
        let slow = [bar(-5, 99.0, 10.0), bar(0, 101.0, 11.0)];
        let mut fast = vec![bar(1, 99.0, 5.0), bar(2, 101.0, 6.0)];
        fired.extend(poll(&slow, &fast, 160));
        for minute in [3, 4] {
            fast.push(bar(minute, 101.0, 0.0));
            fired.extend(poll(&slow, &fast, minute * 60));
        }
        assert_eq!(fired, [101.0], "one crossing fires once");
    }

    #[test]
    fn once_per_close_sees_a_longer_interval_crossing_during_the_bar() {
        // At the close of one-minute bar -1 the five-minute close is 99; at the close of bar 0 it
        // is 102, so it crossed 100 between the alert's two evaluations.
        let host = Host::new(vec![
            serde_json::from_value(serde_json::json!({
                "symbol":"SYM", "timeframe":"Min5", "condition":{"CrossingUp":100.0},
                "frequency":"OncePerBarClose", "additional_timeframes":["Min1"],
                "additional_conditions":[{"source":"Price","condition":{"Above":50.0}}]
            }))
            .unwrap(),
        ]);
        host.events();
        let (five, one) = (
            BarClock::new(Timeframe::Min5),
            BarClock::new(Timeframe::Min1),
        );
        let older = (bar(-5, 99.0, 10.0), bar(-1, 99.0, 10.0));
        let mut fired = Vec::new();
        for (now, close, volume, next) in [
            (30, 99.0, 1.0, false),
            (40, 101.0, 2.0, false),
            (50, 102.0, 3.0, false),
            (65, 102.0, 4.0, true),
        ] {
            let slow = [older.0.clone(), bar(0, close, volume)];
            let mut fast = vec![older.1.clone(), bar(0, close, volume.min(3.0))];
            if next {
                fast.push(bar(1, close, 1.0));
            }
            host.engine.update_matching(
                None,
                "SYM",
                None,
                &[(&five, &slow[..]), (&one, &fast[..])],
                time(now),
                |_, _, _| None,
            );
            fired.extend(host.fired());
        }
        assert_eq!(fired, [102.0], "crossed between two one-minute closes");
    }

    #[test]
    fn once_per_close_evaluates_the_last_bar_of_a_session() {
        use crate::model::{Session, SessionWindow, TimezoneId};
        use chrono::{NaiveTime, Weekday::*};
        let session = Session::new([SessionWindow::new(
            [Mon, Tue, Wed, Thu, Fri],
            NaiveTime::from_hms_opt(9, 30, 0).unwrap(),
            NaiveTime::from_hms_opt(16, 0, 0).unwrap(),
        )]);
        let clock = |tf| BarClock::new(tf).with_session(TimezoneId::utc(), session.clone(), false);
        let (one, five) = (clock(Timeframe::Min1), clock(Timeframe::Min5));
        let at = |text: &str| text.parse::<DateTime<Utc>>().unwrap();
        let bar = |text: &str, close: f64| Bar::new(at(text), close, close, close, close, 1.0);
        let host = Host::new(vec![
            serde_json::from_value(serde_json::json!({
                "symbol":"SYM", "timeframe":"Min1", "condition":{"Above":50.0},
                "frequency":"OncePerBarClose", "additional_timeframes":["Min5"],
                "additional_conditions":[{"source":"Price","condition":{"Above":50.0}}]
            }))
            .unwrap(),
        ]);
        host.events();
        // Tuesday's last two one-minute bars, then Wednesday's first two.
        let fast = [
            bar("2024-03-12T15:58:00Z", 100.0),
            bar("2024-03-12T15:59:00Z", 101.0),
            bar("2024-03-13T09:30:00Z", 102.0),
            bar("2024-03-13T09:31:00Z", 103.0),
        ];
        let mut fired = Vec::new();
        for (count, slow) in [
            (2, vec![bar("2024-03-12T15:55:00Z", 101.0)]),
            (
                3,
                vec![
                    bar("2024-03-12T15:55:00Z", 101.0),
                    bar("2024-03-13T09:30:00Z", 102.0),
                ],
            ),
            (
                4,
                vec![
                    bar("2024-03-12T15:55:00Z", 101.0),
                    bar("2024-03-13T09:30:00Z", 103.0),
                ],
            ),
        ] {
            host.engine.update_matching(
                None,
                "SYM",
                None,
                &[(&one, &fast[..count]), (&five, &slow[..])],
                fast[count - 1].time + chrono::TimeDelta::seconds(30),
                |_, _, _| None,
            );
            fired.extend(host.fired());
        }
        assert_eq!(fired, [100.0, 101.0, 102.0], "every one-minute close");
    }

    #[test]
    fn last_close_of_a_longer_bar_reads_its_final_value() {
        // The five-minute close is 99 at the one-minute close of 00:03 and 101 at that of 00:04,
        // when both bars end together, so it crossed 100 between the two evaluations. By the poll
        // that completes 00:04 the next five-minute bar has opened at 99.
        let host = Host::new(vec![
            serde_json::from_value(serde_json::json!({
                "symbol":"SYM", "timeframe":"Min1", "condition":{"Above":50.0},
                "frequency":"OncePerBarClose", "additional_timeframes":["Min5"],
                "additional_conditions":[{"source":"Price","condition":{"CrossingUp":100.0}}]
            }))
            .unwrap(),
        ]);
        host.events();
        let (one, five) = (
            BarClock::new(Timeframe::Min1),
            BarClock::new(Timeframe::Min5),
        );
        let mut fired = Vec::new();
        for (now, fast, slow) in [
            (
                290,
                vec![bar(3, 99.0, 1.0), bar(4, 99.0, 1.0)],
                vec![bar(0, 99.0, 1.0)],
            ),
            (
                301,
                vec![bar(3, 99.0, 1.0), bar(4, 101.0, 2.0), bar(5, 99.0, 1.0)],
                vec![bar(0, 101.0, 2.0), bar(5, 99.0, 1.0)],
            ),
        ] {
            host.engine.update_matching(
                None,
                "SYM",
                None,
                &[(&one, &fast[..]), (&five, &slow[..])],
                time(now),
                |_, _, _| None,
            );
            fired.extend(host.fired());
        }
        assert_eq!(fired, [101.0], "the 00:00 bar's last close, at 00:05");
    }

    #[test]
    fn daily_bar_stamped_inside_its_day_joins_an_earlier_intraday_bar() {
        // Daily bars stamped at noon UTC, inside their calendar day as history pages allow.
        let host = Host::new(vec![
            serde_json::from_value(serde_json::json!({
                "symbol":"SYM", "timeframe":"Min5", "condition":{"Above":50.0},
                "frequency":"OncePerBar", "additional_timeframes":["Day1"],
                "additional_conditions":[{"source":"Price","condition":{"Above":50.0}}]
            }))
            .unwrap(),
        ]);
        host.events();
        let at = |text: &str| text.parse::<DateTime<Utc>>().unwrap();
        let bar = |text: &str, close: f64| Bar::new(at(text), close, close, close, close, 1.0);
        let daily = [
            bar("2024-03-11T12:00:00Z", 90.0),
            bar("2024-03-12T12:00:00Z", 100.0),
        ];
        let five = [
            bar("2024-03-12T09:00:00Z", 99.0),
            bar("2024-03-12T09:05:00Z", 100.0),
        ];
        host.engine.update_matching(
            None,
            "SYM",
            None,
            &[
                (&BarClock::new(Timeframe::Day1), &daily[..]),
                (&BarClock::new(Timeframe::Min5), &five[..]),
            ],
            at("2024-03-12T09:07:00Z"),
            |_, _, _| None,
        );
        assert_eq!(host.fired(), [100.0], "the bar of 2024-03-12 covers 09:05");
    }

    #[test]
    fn trigger_time_of_a_mixed_interval_message_is_the_firing_time() {
        // The one-minute study rises through 100 while the five-minute bar stays as it was. The
        // other placeholders read the first condition's interval and bar, not the firing one.
        let host = Host::new(vec![
            serde_json::from_value(serde_json::json!({
                "symbol":"SYM", "timeframe":"Min5", "condition":{"Above":50.0},
                "message":"{{trigger_time}} {{interval}} {{close}}", "frequency":"OncePerBar",
                "additional_timeframes":["Min1"],
                "additional_conditions":[{
                    "source":{"Study":{"id":"s","output":0}}, "condition":{"Above":100.0}
                }]
            }))
            .unwrap(),
        ]);
        host.events();
        let (five, one) = (
            BarClock::new(Timeframe::Min5),
            BarClock::new(Timeframe::Min1),
        );
        let (slow, fast) = ([bar(0, 99.0, 10.0)], [bar(2, 98.0, 5.0)]);
        for (now, study) in [(150, 99.0), (170, 101.0)] {
            host.engine.update_matching(
                None,
                "SYM",
                None,
                &[(&five, &slow[..]), (&one, &fast[..])],
                time(now),
                |_, _, _| Some(study),
            );
        }
        let fired: Vec<_> = host
            .events()
            .into_iter()
            .filter_map(|event| match event {
                AlertEvent::TriggeredForSymbol { time, message, .. } => Some((time, message)),
                _ => None,
            })
            .collect();
        assert_eq!(
            fired,
            [(time(170), "1970-01-01T00:02:50+00:00 5min 99".to_owned())],
            "the alert fires at 00:02:50"
        );
    }
}
