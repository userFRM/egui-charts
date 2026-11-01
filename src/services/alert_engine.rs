//! An alert service that evaluates alerts on the bars the host feeds it.

use std::collections::HashMap;
use std::sync::Mutex;

use chrono::{DateTime, Utc};

use super::{AlertEvent, Alerts, Events};
use crate::data::{FeedError, MaybeSend, Reply};
use crate::model::{
    Alert, AlertComparison, AlertCondition, AlertFrequency, AlertId, AlertSource, AlertStatus, Bar,
    Timeframe,
};

/// An [`Alerts`] service that keeps alerts in memory and evaluates them on the bars the host
/// feeds it with [`update`](Self::update), as [`PaperAccount`](super::PaperAccount) fills orders
/// on the prices it is fed. It needs no egui, so a server can run the same code. A host that
/// evaluates alerts on its own server implements [`Alerts`] itself instead.
///
/// The host keeps the alerts: [`alerts`](Self::alerts) returns them with their status for
/// storage, and [`with_alerts`](Self::with_alerts) restores them. Evaluation state lives in
/// memory only, so after a restart an alert that fires on every occurrence may fire again while
/// its condition still holds; an alert with frequency [`Once`](AlertFrequency::Once) that fired
/// stays [`Triggered`](AlertStatus::Triggered).
///
/// # Rules
///
/// *Subject.* An alert is evaluated on its `symbol`, or on each member of its `watchlist` as
/// [`set_watchlist`](Self::set_watchlist) gave it, with its own state per symbol. An update for
/// another symbol, or for a timeframe other than the alert's, does not touch the alert. An alert
/// without a timeframe takes the timeframe of the first update that reaches it, which the engine
/// saves and pushes as an [`AlertEvent::Upsert`], so it is fixed from then on.
///
/// *Evaluated bar.* The newest bar, still forming, or for
/// [`OncePerBarClose`](AlertFrequency::OncePerBarClose) the newest completed one. Every rule of
/// an alert, the first and the additional conditions, is judged on that bar, and the alert holds
/// when every rule holds. A missing or non-finite value makes its rule false. Price is the close
/// and volume the volume of a bar; the `value` function of `update` resolves the other sources.
///
/// *Change.* An update whose evaluated bar, its close and volume and the value of every source
/// equal those of the previous update does nothing, unless a per-minute alert enters a new
/// minute or its schedule opens. An update
/// whose evaluated bar is older than the newest one already evaluated for the alert is ignored:
/// reloading history, rewinding a replay or switching to a coarser timeframe never fires an
/// alert again.
///
/// *Crossings.* The previous value is the value at the alert's previous evaluation, within a bar
/// and across bars, and on the first evaluation the value one bar earlier. Crossing up needs a
/// previous value strictly below the level and a current value at or above it, crossing down the
/// reverse; a value that touches the level and stays there fires once. Moving by and volume
/// above average read the bars before the evaluated one, as [`AlertCondition`] documents.
///
/// *Frequency.* [`Once`](AlertFrequency::Once) fires on the first evaluation that holds and the
/// alert becomes [`Triggered`](AlertStatus::Triggered); a watchlist alert fires once per member
/// and stays active. [`EveryTime`](AlertFrequency::EveryTime) fires on every evaluation that
/// holds, [`OncePerBar`](AlertFrequency::OncePerBar) at most once per bar open time, and
/// [`OncePerBarClose`](AlertFrequency::OncePerBarClose) evaluates each completed bar once.
/// [`OncePerMinute`](AlertFrequency::OncePerMinute) fires at most once per minute of the update
/// time `now`; moving the clock backwards does not repeat a minute.
///
/// *Schedule.* An alert's `schedule` restricts triggers. Outside it the engine keeps observing
/// values for crossings, but does not fire or consume an occurrence. A once alert stays armed.
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
/// engine.update("AAPL", Timeframe::Min1, &[bar(0, 99.0), bar(60, 101.0)], time(61), |_, _| None);
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
    /// Open time of the newest bar evaluated.
    newest: Option<DateTime<Utc>>,
    /// The previous evaluation, which the next one is compared with.
    seen: Option<Seen>,
    /// The occurrence the alert last fired on: the open time of a bar, or the minute for a
    /// per-minute alert.
    fired: Option<DateTime<Utc>>,
    /// A watchlist alert with frequency once has fired on this symbol.
    done: bool,
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
    rules: Vec<(Option<f64>, Option<f64>)>,
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
            alert.id.as_ref() == Some(id) && alert.status == AlertStatus::Active && alert.is_valid()
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

    /// Evaluates every active alert whose subject is `symbol` on `timeframe` against the newest
    /// of `bars`, oldest first, at the time `now`. `value` resolves a study or drawing source at
    /// a bar index of `bars`, for instance with `Chart::alert_value`; price and volume come from
    /// `bars`. Call it each frame with the bars of every live chart, but not with a chart in
    /// replay.
    pub fn update(
        &self,
        symbol: &str,
        timeframe: Timeframe,
        bars: &[Bar],
        now: DateTime<Utc>,
        value: impl Fn(&AlertSource, usize) -> Option<f64>,
    ) {
        self.update_matching(None, symbol, timeframe, bars, now, value);
    }

    /// Evaluates one alert with its retained source definitions. Other alerts may refer to
    /// different study instances with the same chart-local identifiers.
    pub fn update_alert(
        &self,
        id: &AlertId,
        symbol: &str,
        timeframe: Timeframe,
        bars: &[Bar],
        now: DateTime<Utc>,
        value: impl Fn(&AlertSource, usize) -> Option<f64>,
    ) {
        self.update_matching(Some(id), symbol, timeframe, bars, now, value);
    }

    fn update_matching(
        &self,
        id: Option<&AlertId>,
        symbol: &str,
        timeframe: Timeframe,
        bars: &[Bar],
        now: DateTime<Utc>,
        value: impl Fn(&AlertSource, usize) -> Option<f64>,
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
            match alert.timeframe {
                Some(own) if own != timeframe => continue,
                Some(_) => {}
                None => {
                    alert.timeframe = Some(timeframe);
                    if let Some(events) = events {
                        events.push(AlertEvent::Upsert(alert.clone()));
                    }
                }
            }
            let run = runs.entry((id.clone(), symbol.to_owned())).or_default();
            let Some((bar, message)) = evaluate(alert, run, (symbol, timeframe), bars, now, &value)
            else {
                continue;
            };
            let price = bar.close.is_finite().then_some(bar.close);
            if alert.frequency == AlertFrequency::Once {
                run.done = true;
                if alert.watchlist.is_none() {
                    alert.status = AlertStatus::Triggered;
                }
            }
            run.fired = Some(occurrence(alert.frequency, bar, now));
            if let Some(events) = events
                && alert.status != AlertStatus::Active
            {
                events.push(AlertEvent::Upsert(alert.clone()));
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
            loop {
                self.next_id += 1;
                let id = AlertId(self.next_id.to_string());
                if !self.alerts.iter().any(|a| a.id.as_ref() == Some(&id)) {
                    alert.id = Some(id);
                    break;
                }
            }
        }
        self.runs.retain(|(id, _), _| Some(id) != alert.id.as_ref());
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
            self.runs.retain(|(id, _), _| Some(id) != alert.id.as_ref());
            self.push(AlertEvent::Upsert(alert));
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
            events.push(AlertEvent::Upsert(alert.clone()));
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
        state.push(AlertEvent::Upsert(alert));
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

/// The other output a comparison tests against.
fn partner(condition: &AlertCondition) -> Option<&AlertSource> {
    match condition {
        AlertCondition::Compare { other, .. } => Some(other),
        _ => None,
    }
}

/// Evaluates `alert` on the newest of `bars` of `subject`, updating what `run` remembers. Returns
/// the bar it fired on and its message, if it did.
fn evaluate<'a>(
    alert: &Alert,
    run: &mut Run,
    (symbol, timeframe): (&str, Timeframe),
    bars: &'a [Bar],
    now: DateTime<Utc>,
    value: &impl Fn(&AlertSource, usize) -> Option<f64>,
) -> Option<(&'a Bar, String)> {
    let closed = alert.frequency == AlertFrequency::OncePerBarClose;
    let index = bars.len().checked_sub(if closed { 2 } else { 1 })?;
    let bar = &bars[index];
    if run.newest.is_some_and(|newest| bar.time < newest)
        || (closed && run.newest == Some(bar.time))
    {
        return None;
    }
    let source = |source: &AlertSource, at: usize| -> Option<f64> {
        let found = match source {
            AlertSource::Price => bars.get(at)?.close,
            AlertSource::Volume => bars.get(at)?.volume,
            _ => value(source, at)?,
        };
        found.is_finite().then_some(found)
    };
    let rules: Vec<_> = std::iter::once((&alert.source, &alert.condition))
        .chain(
            alert
                .additional_conditions
                .iter()
                .map(|rule| (&rule.source, &rule.condition)),
        )
        .collect();
    let seen = Seen {
        time: bar.time,
        scheduled: alert
            .schedule
            .as_ref()
            .is_none_or(|schedule| schedule.allows(now)),
        minute: (alert.frequency == AlertFrequency::OncePerMinute)
            .then(|| occurrence(alert.frequency, bar, now)),
        bar: (bar.close.to_bits(), bar.volume.to_bits()),
        rules: rules
            .iter()
            .map(|(src, condition)| {
                (
                    source(src, index),
                    partner(condition).and_then(|other| source(other, index)),
                )
            })
            .collect(),
    };
    if run.seen.as_ref() == Some(&seen) {
        return None;
    }
    let holds = seen.scheduled
        && rules.iter().enumerate().all(|(n, (src, condition))| {
            let (current, other) = seen.rules[n];
            let previous = match &run.seen {
                Some(last) => last.rules[n],
                None => (
                    index.checked_sub(1).and_then(|at| source(src, at)),
                    partner(condition)
                        .zip(index.checked_sub(1))
                        .and_then(|(other, at)| source(other, at)),
                ),
            };
            current.is_some_and(|current| {
                condition_holds(
                    condition,
                    current,
                    previous,
                    other,
                    &bars[..=index],
                    |back| {
                        index
                            .checked_sub(back as usize)
                            .and_then(|at| source(src, at))
                    },
                )
            })
        });
    run.newest = Some(bar.time);
    run.seen = Some(seen);
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
    fires.then(|| {
        let message = alert.expand_message(symbol, timeframe, bar, |src| source(src, index));
        (bar, message)
    })
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
    previous: (Option<f64>, Option<f64>),
    other: Option<f64>,
    bars: &[Bar],
    lookback: impl Fn(u32) -> Option<f64>,
) -> bool {
    let (previous, previous_other) = previous;
    match condition {
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
        AlertCondition::Compare { comparison, .. } => {
            other.is_some_and(|level| compare(*comparison, value, previous, level, previous_other))
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

/// Historical and streaming inputs for an alert without a rendered chart. Uses the same
/// validation, trade aggregation and history/live merge rules as a chart. Dropping it stops
/// its subscription and discards late replies. Available with `ui` and `alert-engine`.
#[cfg(feature = "ui")]
pub struct AlertFeed {
    feed: std::sync::Arc<dyn crate::data::Datafeed>,
    inbox: std::sync::Arc<crate::data::inbox::Inbox>,
    wake: std::sync::Arc<std::sync::atomic::AtomicBool>,
    request: crate::data::HistoryRequest,
    metric: Option<String>,
    instrument: Option<crate::model::Instrument>,
    subscription: Option<crate::data::SubscriptionId>,
    bars: Vec<Bar>,
    origins: std::collections::BTreeMap<DateTime<Utc>, crate::data::merge::Origin>,
    pending: bool,
    ready: bool,
    retry: Option<DateTime<Utc>>,
    stopped: bool,
    drawings: Option<(
        std::sync::Arc<dyn super::Storage>,
        super::document::StoredDocument,
    )>,
    stored: Option<crate::drawings::persistence::DrawingStorage>,
}

#[cfg(feature = "ui")]
impl AlertFeed {
    /// A latest-history request for 1,000 bars. Set its public calendar, session, conversion
    /// and count fields before passing it to [`new`](Self::new) to match the source chart.
    pub fn history_request(
        symbol: impl Into<String>,
        timeframe: Timeframe,
    ) -> crate::data::HistoryRequest {
        crate::data::HistoryRequest {
            symbol: symbol.into(),
            timeframe,
            clock: crate::model::BarClock::new(timeframe),
            before: None,
            since: None,
            count: 1000,
            extended_hours: false,
            conversion: None,
        }
    }

    /// Requests the symbol's metadata, then its history and updates. `metric` selects a study's
    /// metric series; `None` selects price bars. Calls return promptly as `Datafeed` requires.
    pub fn new(
        feed: std::sync::Arc<dyn crate::data::Datafeed>,
        mut request: crate::data::HistoryRequest,
        metric: Option<String>,
    ) -> Self {
        if metric.is_some() {
            request.clock = crate::model::BarClock::exact();
        }
        let inbox = std::sync::Arc::<crate::data::inbox::Inbox>::default();
        if let Some(conversion) = &request.conversion
            && metric.is_none()
        {
            feed.resolve_converted(&request.symbol, conversion, inbox.reply(1));
        } else {
            feed.resolve(&request.symbol, inbox.reply(1));
        }
        Self {
            feed,
            inbox,
            wake: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true)),
            request,
            metric,
            instrument: None,
            subscription: None,
            bars: Vec::new(),
            origins: Default::default(),
            pending: true,
            ready: false,
            retry: None,
            stopped: false,
            drawings: None,
            stored: None,
        }
    }

    /// Follows this symbol's drawings in host storage, without writing to it.
    pub fn set_storage(&mut self, storage: std::sync::Arc<dyn super::Storage>) {
        if self
            .drawings
            .as_ref()
            .is_some_and(|(current, _)| std::sync::Arc::ptr_eq(current, &storage))
        {
            return;
        }
        let mut document = super::document::StoredDocument::open(
            storage.as_ref(),
            super::StoreKind::Drawings,
            &self.request.symbol,
        );
        document.follow_changes(storage.as_ref());
        self.drawings = Some((storage, document));
    }

    /// Bars available after the initial history reply. Updates received before that reply
    /// remain buffered so evaluation never mistakes an incomplete stream for history.
    pub fn bars(&self) -> &[Bar] {
        if self.ready { &self.bars } else { &[] }
    }

    /// Calendar shared by history, streaming aggregation and study calculation.
    pub fn clock(&self) -> &crate::model::BarClock {
        &self.request.clock
    }

    /// Metadata resolved by the host.
    pub fn instrument(&self) -> Option<&crate::model::Instrument> {
        self.instrument.as_ref()
    }

    /// A stored drawing's level on the evaluated bar, using the chart's interpolation and
    /// price-scale geometry. `None` while drawings are loading or for an unavailable level.
    pub fn drawing_value(
        &self,
        source: &AlertSource,
        at: usize,
        scale: crate::scales::PriceScaleMode,
    ) -> Option<f64> {
        let bars = self.bars();
        bars.get(at)?;
        self.stored
            .as_ref()?
            .drawings()
            .iter()
            .enumerate()
            .find_map(|(id, stored)| {
                let drawing = crate::drawings::persistence::stored_to_drawing_with(
                    stored,
                    |point| {
                        point
                            .time()
                            .and_then(|time| {
                                crate::model::index_at_with_timeframe(
                                    bars,
                                    time,
                                    Some(self.request.timeframe),
                                    self.instrument
                                        .as_ref()
                                        .and_then(|instrument| instrument.time_zone.as_ref()),
                                )
                            })
                            .unwrap_or(f64::NAN) as f32
                    },
                    id,
                );
                let level = drawing.alert_level(source)?;
                drawing
                    .alert_levels(at as f64, scale)
                    .into_iter()
                    .find(|(index, ..)| *index == level)
                    .map(|(.., value)| value)
                    .filter(|value| value.is_finite())
            })
    }

    /// Applies replies and coalesced updates and wakes `ctx` when more arrive. Recoverable
    /// failures retry after the host's delay, or five seconds. Errors are returned for display.
    pub fn poll(&mut self, ctx: &egui::Context, now: DateTime<Utc>) -> Vec<FeedError> {
        use crate::data::merge::{Origin, merge};
        use crate::data::{
            HistoryPage, MetricPage, MetricRequest, MetricSubscription, Subscription,
        };
        let repaint = ctx.clone();
        let viewport = ctx.viewport_id();
        let wake = std::sync::Arc::clone(&self.wake);
        self.inbox.set_waker(move || {
            if wake.load(std::sync::atomic::Ordering::Relaxed) {
                repaint.request_repaint_of(viewport);
            }
        });
        let mut errors = Vec::new();
        let drained = self.inbox.drain();
        for (key, answer) in drained.answers {
            self.pending = false;
            let result = if key == 1 {
                let result = *answer
                    .downcast::<Result<crate::model::Instrument, FeedError>>()
                    .expect("alert instrument");
                result.map(|instrument| {
                    if let Some(session) = &instrument.session
                        && self.metric.is_none()
                    {
                        self.request.clock = self.request.clock.clone().with_session(
                            instrument
                                .time_zone
                                .clone()
                                .unwrap_or_else(crate::model::TimezoneId::utc),
                            session.clone(),
                            self.request.extended_hours,
                        );
                    }
                    self.instrument = Some(instrument);
                })
            } else {
                let result = if self.metric.is_some() {
                    (*answer
                        .downcast::<Result<MetricPage, FeedError>>()
                        .expect("alert metric history"))
                    .map(|page| page.points.into_iter().map(Bar::from).collect::<Vec<_>>())
                } else {
                    (*answer
                        .downcast::<Result<HistoryPage, FeedError>>()
                        .expect("alert history"))
                    .map(|page| page.bars)
                };
                result.map(|bars| {
                    let merged = merge(
                        &mut self.bars,
                        &mut self.origins,
                        &self.request.clock,
                        bars.into_iter().map(|bar| (bar, Origin::History)).collect(),
                    );
                    self.ready = true;
                    if merged.rejected > 0 {
                        errors.push(FeedError::InvalidData("Invalid alert history bars".into()));
                    }
                })
            };
            if let Err(error) = result {
                if error.is_recoverable() {
                    let delay = match &error {
                        FeedError::RateLimited {
                            retry_after: Some(delay),
                        } => chrono::TimeDelta::from_std(*delay)
                            .unwrap_or(chrono::TimeDelta::seconds(5)),
                        _ => chrono::TimeDelta::seconds(5),
                    };
                    self.retry = now.checked_add_signed(delay);
                } else {
                    self.stopped = true;
                }
                errors.push(error);
            }
        }
        for (_, updates) in drained.updates {
            if updates.reset {
                self.bars.clear();
                self.origins.clear();
            }
            merge(
                &mut self.bars,
                &mut self.origins,
                &self.request.clock,
                updates.bars.into_values().collect(),
            );
            if let Some(error) = updates.error {
                // A lost stream requires fresh history before evaluation resumes.
                self.ready = false;
                self.retry = Some(now);
                errors.push(error);
            }
        }
        if !self.pending && !self.stopped && !self.ready && self.retry.is_none_or(|at| at <= now) {
            self.retry = None;
            self.pending = true;
            if self.instrument.is_none() {
                if let Some(conversion) = &self.request.conversion
                    && self.metric.is_none()
                {
                    self.feed.resolve_converted(
                        &self.request.symbol,
                        conversion,
                        self.inbox.reply(1),
                    );
                } else {
                    self.feed.resolve(&self.request.symbol, self.inbox.reply(1));
                }
            } else {
                if self.subscription.is_none() {
                    let clock = if self.metric.is_some() {
                        crate::model::BarClock::exact()
                    } else {
                        self.request.clock.clone()
                    };
                    let sink = self.inbox.subscribe(
                        self.request.symbol.clone(),
                        clock,
                        100_000,
                        self.instrument
                            .as_ref()
                            .and_then(|instrument| instrument.tick_size),
                    );
                    let id = sink.id;
                    self.subscription = Some(id);
                    if let Some(metric) = &self.metric {
                        self.feed.subscribe_metric(MetricSubscription {
                            id,
                            symbol: self.request.symbol.clone(),
                            metric: metric.clone(),
                            sink,
                        });
                    } else {
                        self.feed.subscribe(Subscription {
                            id,
                            symbol: self.request.symbol.clone(),
                            timeframe: self.request.timeframe,
                            clock: self.request.clock.clone(),
                            extended_hours: self.request.extended_hours,
                            conversion: self.request.conversion.clone(),
                            sink,
                        });
                    }
                }
                if let Some(metric) = &self.metric {
                    self.feed.metric_history(
                        MetricRequest {
                            symbol: self.request.symbol.clone(),
                            metric: metric.clone(),
                            before: None,
                            count: self.request.count,
                        },
                        self.inbox.reply(2),
                    );
                } else {
                    self.feed.history(self.request.clone(), self.inbox.reply(2));
                }
            }
        }
        if let Some((storage, document)) = &mut self.drawings {
            for received in document.poll(storage.as_ref(), ctx) {
                match received {
                    super::document::Received::Loaded(json) => {
                        let loaded = json
                            .as_deref()
                            .map(crate::drawings::persistence::DrawingStorage::from_json)
                            .transpose();
                        match loaded {
                            Ok(Some((stored, skipped)))
                                if stored.symbol == self.request.symbol && skipped.is_empty() =>
                            {
                                self.stored = Some(stored)
                            }
                            Ok(None) => {
                                self.stored =
                                    Some(crate::drawings::persistence::DrawingStorage::new(
                                        self.request.symbol.clone(),
                                    ))
                            }
                            _ => errors.push(FeedError::InvalidData(
                                "Could not read alert drawings".into(),
                            )),
                        }
                    }
                    super::document::Received::Failed(error) => {
                        errors.push(FeedError::Other(error))
                    }
                    _ => {}
                }
            }
        }
        errors
    }
}

#[cfg(feature = "ui")]
impl Drop for AlertFeed {
    fn drop(&mut self) {
        // Unsubscribe may drop a pending reply while the caller holds the context's lock.
        self.wake.store(false, std::sync::atomic::Ordering::Relaxed);
        if let Some(id) = self.subscription.take() {
            self.inbox.unsubscribe(id);
            self.feed.unsubscribe(id);
        }
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
            .update("SYM", Timeframe::Min1, bars, time(now), |_, at| {
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
                assert!(host.events().is_empty(), "{condition:?}, {frequency}");
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
        watching.message = "{{symbol}} closed at {{close}}".into();
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
        // The message is filled in the event, and the host callback gets the same event.
        assert!(
            matches!(&events[1], AlertEvent::TriggeredForSymbol { message, .. } if message == "SYM closed at 103")
        );
        let delivered = host.delivered.lock().unwrap().clone();
        assert_eq!(delivered.len(), 1);
        assert_eq!(delivered[0].1, events[1]);
        feed(&host, &bars, &[], 1_001);
        // History that grew at the front, a rewound replay and another timeframe: nothing fires.
        let mut reloaded = vec![bar(-1, 98.0, 1.0)];
        reloaded.extend(bars.clone());
        feed(&host, &reloaded, &[], 1_002);
        feed(&host, &bars[..2], &[], 1_003);
        host.engine
            .update("SYM", Timeframe::Min5, &bars, time(1_004), |_, _| None);
        assert!(host.events().is_empty());
    }

    #[cfg(all(feature = "ui", not(target_arch = "wasm32")))]
    #[test]
    fn dropping_a_feed_under_the_context_lock_does_not_wake_cancelled_replies() {
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
                .update(symbol, Timeframe::Min1, &bars, time(1_000), |_, _| None);
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
}
