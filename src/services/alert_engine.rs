//! An alert service that evaluates alerts on the bars the host feeds it.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::Mutex;

use chrono::{DateTime, Utc};

use super::{AlertEvent, Alerts, Events};
use crate::data::{FeedError, MaybeSend, Reply};
use crate::model::{
    Alert, AlertChannelComparison, AlertComparison, AlertCondition, AlertFrequency, AlertId,
    AlertPositionProgress, AlertSource, AlertStatus, AlertTrigger, Bar, BarClock,
    FootprintAlertEvent, Timeframe,
};

/// Triggers the engine keeps for [`AlertEngine::triggers`], silent entries included.
const TRIGGER_HISTORY: usize = 5_000;

/// An [`Alerts`] service that keeps alerts in memory and evaluates them on the bars the host
/// feeds it with [`update`](Self::update), as [`PaperAccount`](super::PaperAccount) fills orders
/// on the prices it is fed. It needs no egui, so a server can run the same code. A host that
/// evaluates alerts on its own server implements [`Alerts`] itself instead.
/// With `studies`, `AlertEvaluator` reads the saved source definitions from a datafeed and
/// supplies these evaluations without constructing a chart.
///
/// The host keeps the alerts: [`alerts`](Self::alerts) returns them with their status for
/// storage, and [`with_alerts`](Self::with_alerts) restores them. In the same way
/// [`triggers`](Self::triggers) returns the newest 5,000 triggers, silent ones included, and
/// [`with_triggers`](Self::with_triggers) restores them; the first `watch` pushes them as
/// [`AlertEvent::Logged`], so the alert log survives a restart. Evaluation state lives in
/// memory; a restored alert counts its [`last_triggered_at`](Alert::last_triggered_at), or on a
/// watchlist each member's time in [`member_triggered_at`](Alert::member_triggered_at), as its
/// last firing (see *Restarts*). An alert with frequency [`Once`](AlertFrequency::Once) that
/// fired stays [`Triggered`](AlertStatus::Triggered). A [`Position`](AlertCondition::Position)
/// keeps its progress in its condition, so it resumes waiting for the entry or for an exit,
/// and retains the levels that progress reached, see *Positions*.
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
/// evaluation, within a bar and across bars. The first evaluation on a symbol of an alert saved
/// through [`save`](Alerts::save), new or with a change to what it watches or its status, only
/// records the values, so a crossing, entering or exiting rule needs a transition observed after
/// the save, and a move made before the save never fires it. An alert restored by
/// [`with_alerts`](Self::with_alerts), or saved under an id the engine did not hold, compares its
/// first evaluation with the value one bar earlier, so a crossing made while the host was closed
/// fires, subject to the saved last firing (see *Restarts*).
/// A crossing on a longer interval therefore fires once whatever the shorter interval does, and
/// one between two closes of the shorter interval is caught at the later close. Crossing up needs
/// a previous value strictly below the level and a current value at or above it, crossing down the
/// reverse; a value that touches the level and stays there fires once. Where a rule reads the
/// range traded, a price strictly beyond the level since the previous evaluation with the current
/// price back at or across it also crosses, on its way back; a price outside a channel with the
/// current one back inside enters it, one inside with the current one outside exits it, and
/// prices on both sides of it enter and exit it. A move across and back therefore meets the same
/// transitions however often the host evaluates, and one evaluation still fires an alert once.
/// Moving by and volume above average read the bars before the evaluated one, as
/// [`AlertCondition`] documents.
/// Moving channels resolve the preceding bar again when a new bar arrives, so edits and
/// skipped updates use that bar's current geometry. Within a bar they use the previous evaluation.
///
/// *Frequency.* [`Once`](AlertFrequency::Once) fires on the first evaluation that holds and the
/// alert becomes [`Triggered`](AlertStatus::Triggered); a watchlist alert fires once per member,
/// records it in [`member_triggered_at`](Alert::member_triggered_at), and becomes `Triggered` when
/// every member of its list has fired, also when [`set_watchlist`](Self::set_watchlist) leaves
/// only members that fired. [`EveryTime`](AlertFrequency::EveryTime) fires on every evaluation that
/// holds, [`OncePerBar`](AlertFrequency::OncePerBar) at most once per bar open time, and
/// [`OncePerBarClose`](AlertFrequency::OncePerBarClose) evaluates each completed bar once.
/// [`OncePerMinute`](AlertFrequency::OncePerMinute) fires at most once per minute of the update
/// time `now`; moving the clock backwards does not repeat a minute.
///
/// *Restarts.* The first evaluation on a symbol of an alert restored by
/// [`with_alerts`](Self::with_alerts), or saved under an id the engine did not hold, takes its
/// saved last firing as the last firing there. A per-minute alert does not fire again in that
/// minute. When the evaluated bar opened at or before that time, or for an alert at bar close
/// the bar after it did, a once-per-bar or once-per-bar-close alert does not fire on that bar,
/// and a crossing, entering or exiting rule needs a new transition, since this evaluation only
/// records the values. A save that changes what the alert watches or its status ends this, so
/// it starts afresh; display and delivery edits keep its restart handling.
///
/// *Schedule.* An alert's `schedule` restricts notifications. Outside it the engine keeps
/// observing values for crossings and records each trigger as a silent
/// [`AlertEvent::SilentTriggered`] entry without calling the delivery callback or consuming an
/// occurrence, so a once alert stays armed. It records at most one entry per occurrence of its
/// frequency, a once alert one per bar. `starts` holds back evaluation altogether until that time.
///
/// *Positions.* A [`Position`](AlertCondition::Position) fires on its entry and its first exit,
/// after an `Upsert` carrying its new progress, and becomes `Triggered` after the exit. Outside
/// its schedule the triggers are silent but progress still moves. An empty message reads the
/// progress reached, such as "Position entered". Saving it active after its exit starts it again
/// from the entry. Moving any level after entry or exit also starts it again from the entry
/// with an `Upsert` and no trigger; the new entry needs a value observed after the edit.
///
/// *Expiry.* Every `update` and [`tick`](Self::tick) first marks active alerts whose expiry has
/// passed as [`Expired`](AlertStatus::Expired), so an alert never fires at or after its expiry
/// and expires without data. Disabled, triggered and expired alerts are not evaluated, except
/// that a position that reached its exit keeps reading its levels. Saving changes to what an
/// alert watches or its status, or removing it, discards its evaluation
/// state, so a resumed alert starts afresh. Display and delivery edits keep a watchlist alert's
/// member record.
///
/// ```
/// use std::sync::{Arc, Mutex};
///
/// use chrono::{DateTime, Utc};
/// use egui_charts::model::{Alert, AlertCondition, Bar, Timeframe};
/// use egui_charts::services::{AlertEngine, AlertNotice};
///
/// let mut alert = Alert::new("AAPL", AlertCondition::CrossingUp(100.0));
/// alert.timeframe = Some(Timeframe::Min1);
/// alert.message = r#"{"symbol": "{{symbol}}", "close": {{close}}}"#.into();
/// alert.delivery.email = true;
/// alert.delivery.plain_text = true;
/// alert.delivery.plain_text_address = "pager@example.com".into();
/// alert.delivery.push = true;
/// alert.delivery.webhook = true;
/// alert.delivery.webhook_url = "https://example.com/hook".into();
/// // What the host hands to its mail, push and web clients, which must not block the callback.
/// let sent = Arc::new(Mutex::new(Vec::new()));
/// let outbox = Arc::clone(&sent);
/// let engine = AlertEngine::new()
///     .with_alerts(vec![alert])
///     .on_trigger(move |alert, event| {
///         let Some(notice) = AlertNotice::new(alert, event) else {
///             return;
///         };
///         let delivery = &alert.delivery;
///         let mut outbox = outbox.lock().unwrap();
///         if delivery.email {
///             outbox.push(format!("email: {}", notice.subject));
///         }
///         if delivery.plain_text {
///             // An empty address sends to the account's alternative address.
///             let to = delivery.plain_text_address.trim();
///             outbox.push(format!("plain text to {to}: {}", notice.subject));
///         }
///         if delivery.push {
///             outbox.push(format!("push: {}", notice.push_title));
///         }
///         if delivery.webhook {
///             let url = delivery.webhook_url.trim();
///             outbox.push(format!("POST {url} {}: {}", notice.content_type, notice.body));
///         }
///     });
/// let time = |seconds| DateTime::<Utc>::from_timestamp(seconds, 0).unwrap();
/// let bar = |seconds, close| Bar::new(time(seconds), close, close, close, close, 1.0);
/// // The close rises through 100 on the second bar; `update` also takes the chart's source values.
/// engine.update("AAPL", None, Timeframe::Min1, &[bar(0, 99.0), bar(60, 101.0)], time(61), |_, _| None);
/// assert_eq!(
///     *sent.lock().unwrap(),
///     [
///         "email: AAPL: Crossing up 100",
///         "plain text to pager@example.com: AAPL: Crossing up 100",
///         "push: AAPL: Crossing up 100",
///         r#"POST https://example.com/hook application/json: {"symbol": "AAPL", "close": 101}"#,
///     ]
/// );
/// ```
pub struct AlertEngine {
    state: Mutex<State>,
    deliver: Option<Box<dyn Deliver>>,
}

impl std::fmt::Debug for AlertEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AlertEngine").finish_non_exhaustive()
    }
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
    /// The custom event sources the host connected.
    routes: std::collections::HashSet<String>,
    /// Alerts restored by `with_alerts`, or saved under an id the engine did not hold, whose
    /// monitoring has not changed since: a run's first evaluation compares crossings with the
    /// bar before and counts their saved last firing.
    restored: std::collections::HashSet<AlertId>,
    /// The newest triggers, oldest first.
    triggers: VecDeque<AlertTrigger>,
    /// A stream was watched, so the history was pushed.
    watched: bool,
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
    /// The sequence of the newest strategy fill or custom event received.
    last_sequence: u64,
    /// The sequence of the newest strategy alert received.
    #[cfg(feature = "strategies")]
    last_alert: u64,
    /// The detector signals that fired, with their anchor times.
    signals: Vec<(crate::model::SignalId, DateTime<Utc>)>,
    /// The latest time before which detector signals are history.
    history: Option<DateTime<Utc>>,
}

/// One firing of an alert on host events: the event's time and price, and the message a trigger
/// shows when the alert has none.
struct Occurrence {
    time: DateTime<Utc>,
    price: Option<f64>,
    /// The price the message's `{{value}}` and bar placeholders read where it differs from
    /// `price`, as a fill price without binary rounding digits.
    shown_price: Option<f64>,
    message: String,
    /// Placeholders only this occurrence fills, as a fill's `{{order_id}}`.
    fields: Vec<(&'static str, String)>,
}

/// The latest observation for conditions sharing one calculation interval.
struct Observation {
    available: bool,
    seen: Seen,
    /// Whether the rules that read this bar alone hold. Those that compare two evaluations are
    /// tested when the alert evaluates, see [`reads_previous`].
    holds: bool,
    /// Whether they hold on the close alone, which they read once an evaluation read this
    /// observation, as nothing traded on it since.
    still_holds: bool,
    /// The values of `seen.rules` one bar earlier, read when this bar first arrived.
    before: Vec<[Option<f64>; 4]>,
    clock: BarClock,
    event: DateTime<Utc>,
    bar: Bar,
    /// The time of the bar after this one in the update that observed it, if any: for an alert
    /// at bar close, the bar whose arrival completed this one.
    next: Option<DateTime<Utc>>,
    /// What the message of the first condition's interval reads on this bar. An update of another
    /// interval, which may fire the alert, cannot read it.
    values: Vec<(AlertSource, Option<f64>)>,
    /// The bar before this one as it ended, or as last observed when the update held only this
    /// one, for a shorter bar that closes with it. Only a longer interval of an alert evaluated at
    /// bar close keeps it.
    earlier: Option<Box<Observation>>,
    /// The lowest and highest price known to have traded since the alert's previous evaluation
    /// of this interval, see [`traded_since`], which its price rules read on the first evaluation
    /// that reads this observation. `None` before the alert's first evaluation and for an alert
    /// at bar close, whose price rules read the close.
    traded: Option<[f64; 2]>,
}

/// The inputs of one evaluation: the bar, its close, volume, high and low, and for each rule the
/// value of its source and of the other output it compares with.
#[derive(Clone, PartialEq)]
struct Seen {
    time: DateTime<Utc>,
    scheduled: bool,
    minute: Option<DateTime<Utc>>,
    /// The bits of the close, volume, high and low, so that a value that is not a number equals
    /// itself and a new high or low is a new evaluation.
    bar: [u64; 4],
    rules: Vec<[Option<f64>; 4]>,
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
    /// [`AlertNotice::new`](super::AlertNotice::new) builds what each of them sends. The call
    /// comes from the thread that fed the engine and must return at once; it may use the engine.
    pub fn on_trigger(
        mut self,
        deliver: impl Fn(&Alert, &AlertEvent) + MaybeSend + 'static,
    ) -> Self {
        self.deliver = Some(Box::new(deliver));
        self
    }

    /// Starts with `alerts` as [`alerts`](Self::alerts) returned them, giving those without an
    /// id one after the others. Their first evaluation on each symbol compares crossings with
    /// the value one bar earlier and counts their saved last firing, as the *Crossings* and
    /// *Restarts* rules describe.
    pub fn with_alerts(self, alerts: Vec<Alert>) -> Self {
        {
            let mut state = self.lock();
            // Ids are given once every kept id is known, so that none is given twice.
            let (kept, new): (Vec<_>, Vec<_>) = alerts.into_iter().partition(|a| a.id.is_some());
            for alert in kept.into_iter().chain(new) {
                // Restored alerts compare with the bar before and count their saved last firing.
                if let Some(id) = state.insert(alert).id {
                    state.restored.insert(id);
                }
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

    /// The newest 5,000 triggers, oldest first, silent entries included, for the host to keep
    /// beside [`alerts`](Self::alerts).
    pub fn triggers(&self) -> Vec<AlertTrigger> {
        self.lock().triggers.iter().cloned().collect()
    }

    /// Starts with `triggers` as [`triggers`](Self::triggers) returned them, oldest first,
    /// keeping the newest 5,000. The first [`watch`](Alerts::watch) pushes them as
    /// [`AlertEvent::Logged`], so the alert log survives a restart; later calls do not.
    pub fn with_triggers(self, triggers: Vec<AlertTrigger>) -> Self {
        {
            let mut state = self.lock();
            let skip = triggers.len().saturating_sub(TRIGGER_HISTORY);
            state.triggers = triggers
                .into_iter()
                .skip(skip)
                .map(|mut trigger| {
                    trigger.price = trigger.price.filter(|p| p.is_finite());
                    trigger
                })
                .collect();
        }
        self
    }

    /// Keeps only the triggers of the history for which `keep` returns true, as the alerts
    /// panel does when the user clears its log.
    pub fn retain_triggers(&self, keep: impl FnMut(&AlertTrigger) -> bool) {
        self.lock().triggers.retain(keep);
    }

    /// Active symbols still armed for `id`, including watchlist members, and the symbol of a
    /// position that reached its exit, whose levels the engine still follows. A once-per-member
    /// alert omits symbols that already fired, so a host can release their subscriptions.
    pub fn subjects(&self, id: &AlertId) -> Vec<String> {
        let state = self.lock();
        let Some(alert) = state.alerts.iter().find(|alert| {
            alert.id.as_ref() == Some(id)
                && (alert.status == AlertStatus::Active || ended_position(alert))
                && alert.is_valid()
                && !alert.condition.is_event()
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
                !member_done(alert, symbol)
                    && !state
                        .runs
                        .get(&(id.clone(), symbol.clone()))
                        .is_some_and(|run| run.done)
            })
            .collect()
    }

    /// Sets the members of the watchlist `id`, as the host's watchlist holds them. A member that
    /// leaves loses its evaluation state, so it starts afresh if it returns, except that a member
    /// a once alert fired for stays done.
    pub fn set_watchlist(&self, id: &str, symbols: Vec<String>) {
        let mut state = self.lock();
        let State {
            alerts,
            runs,
            watchlists,
            events,
            ..
        } = &mut *state;
        runs.retain(|(alert, symbol), _| {
            symbols.contains(symbol)
                || !alerts
                    .iter()
                    .any(|a| a.id.as_ref() == Some(alert) && a.watchlist.as_deref() == Some(id))
        });
        watchlists.insert(id.to_owned(), symbols);
        for alert in alerts.iter_mut().filter(|alert| {
            alert.status == AlertStatus::Active
                && alert.frequency == AlertFrequency::Once
                && alert.watchlist.as_deref() == Some(id)
                && finished(alert, watchlists)
        }) {
            alert.status = AlertStatus::Triggered;
            if let Some(events) = events {
                events.push(AlertEvent::Upsert(Box::new(alert.clone())));
            }
        }
    }

    /// Updates active conditions of `symbol` on `timeframe` against the newest of `bars`, oldest
    /// first, at the time `now`. Feed every interval in [`Alert::timeframes`]. `value` resolves a
    /// study or drawing source at a bar index of `bars`, for instance with `Chart::alert_value`;
    /// price and volume come from `bars`, and a [`TPOLevel`](AlertSource::TPOLevel) is the price
    /// it keeps. `exchange` is the exchange of `symbol` for the
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
            restored,
            triggers,
            ..
        } = &mut *state;
        for alert in alerts.iter_mut().filter(|alert| {
            (alert.status == AlertStatus::Active || ended_position(alert))
                && alert.is_valid()
                && alert.starts.is_none_or(|starts| starts <= now)
                && !alert.condition.is_event()
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
            let done = member_done(alert, symbol);
            let run = runs
                .entry((id.clone(), symbol.to_owned()))
                .or_insert_with(|| Run {
                    done,
                    ..Run::default()
                });
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
            if restart_position(alert, run) {
                if let Some(events) = events {
                    events.push(AlertEvent::Upsert(Box::new(alert.clone())));
                }
                continue;
            }
            // An ended position only follows its levels.
            if alert.status != AlertStatus::Active {
                continue;
            }
            let Some((bar, messages, silent)) =
                evaluate(alert, run, symbol, exchange, now, restored.contains(&id))
            else {
                continue;
            };
            let price = bar.close.is_finite().then_some(bar.close);
            let position = matches!(alert.condition, AlertCondition::Position { .. });
            if silent {
                for message in &messages {
                    let event = AlertEvent::SilentTriggered {
                        id: id.clone(),
                        symbol: symbol.to_owned(),
                        time: now,
                        price,
                        message: message.clone(),
                    };
                    remember(triggers, alert, &event);
                    if let Some(events) = events {
                        events.push(event);
                    }
                }
                // A position alert moves on outside its schedule too.
                if !position {
                    continue;
                }
            } else {
                alert.last_triggered_at =
                    Some(alert.last_triggered_at.map_or(now, |old| old.max(now)));
                record_member(alert, symbol, now);
            }
            // An entered position keeps watching for its exit.
            let open = matches!(
                alert.condition,
                AlertCondition::Position {
                    progress: AlertPositionProgress::Entered,
                    ..
                }
            );
            let once = alert.frequency == AlertFrequency::Once;
            if once && !open {
                run.done = true;
                if alert.watchlist.is_none() || finished(alert, watchlists) {
                    alert.status = AlertStatus::Triggered;
                }
            }
            run.fired = Some(occurrence(alert.frequency, &bar, now));
            // A once alert on a list reports each member it fired for.
            if let Some(events) = events
                && (alert.status != AlertStatus::Active
                    || position
                    || (once && alert.watchlist.is_some()))
            {
                events.push(AlertEvent::Upsert(Box::new(alert.clone())));
            }
            if silent {
                continue;
            }
            for message in messages {
                let event = AlertEvent::TriggeredForSymbol {
                    id: id.clone(),
                    symbol: symbol.to_owned(),
                    time: now,
                    price,
                    message,
                };
                remember(triggers, alert, &event);
                if let Some(events) = events {
                    events.push(event.clone());
                }
                fired.push((alert.clone(), event));
            }
        }
        // An alert on host events keeps what it received while paused, so resuming it replays
        // nothing.
        runs.retain(|(id, _), _| {
            alerts.iter().any(|a| {
                a.id.as_ref() == Some(id)
                    && (a.status == AlertStatus::Active || a.condition.is_event())
            })
        });
        drop(state);
        if let Some(deliver) = &self.deliver {
            for (alert, event) in &fired {
                deliver.deliver(alert, event);
            }
        }
    }

    /// Delivers each new fill of `run` to the strategy-fill alerts of that run whose
    /// [`strategy_events`](Alert::strategy_events) take fills. Use a unique run id for each
    /// `Backtest` instance, retained for its lifetime, and forward its update batches.
    /// Repeated batches are ignored by sequence, including after edits or rearming an alert
    /// on the same run and when multiple fills share a timestamp.
    /// `now` gates expiry and notification schedules; event times are the source fill times.
    /// Empty messages use the order's alert message, as written, or without one the execution
    /// side, quantity, price and resulting signed position, written as the list of trades writes
    /// them: the price with at least the run's
    /// [`price_decimals`](crate::strategies::StrategyFill::price_decimals) and more where it has
    /// them, the quantity and position with their own decimals up to eight, all without binary
    /// rounding digits, such as `Buy 1 at 100.12, position 1`.
    /// Custom messages expand the usual placeholders using a flat bar at the fill price at 15
    /// significant digits, with its source time and zero volume, and `exchange` as
    /// [`Alert::expand_message`] takes it; the trigger reports the exact fill price. A fill
    /// carries no study values, so `{{plot_ID_N}}` reads `NaN`. Strategy-fill alerts need no
    /// price-reader subscription. `{{order_id}}`, `{{side}}` (`Buy` or `Sell`), `{{quantity}}`,
    /// `{{position}}`, the signed position after the fill, `{{market_position}}` (`long`, `flat`
    /// or `short` after the fill), `{{market_position_size}}`, its absolute size,
    /// `{{prev_market_position}}` and `{{prev_market_position_size}}`, the same before the fill,
    /// `{{comment}}`, the order's comment or without one its id, and `{{alert_message}}`, the
    /// order's alert message or empty without one, read the fill; other alerts leave them as
    /// written.
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
        use crate::model::{StrategyEvents, StrategyReport};
        let watches = |alert: &Alert| matches!(&alert.condition, AlertCondition::StrategyFills(id) if id == run);
        self.fire_events(
            symbol,
            exchange,
            timeframe,
            now,
            watches,
            |alert, seen, _| {
                fills
                    .iter()
                    .filter_map(|fill| {
                        if fill.sequence <= seen.last_sequence
                            || !fill.price.is_finite()
                            || !fill.quantity.is_finite()
                            || fill.quantity <= 0.0
                            || !fill.position.is_finite()
                        {
                            return None;
                        }
                        seen.last_sequence = fill.sequence;
                        // Advance excluded fills too, so changing the choice cannot replay them.
                        if alert.strategy_events == StrategyEvents::Alerts {
                            return None;
                        }
                        let side = match fill.side {
                            crate::model::OrderSide::Buy => "Buy",
                            crate::model::OrderSide::Sell => "Sell",
                        };
                        let signed = match fill.side {
                            crate::model::OrderSide::Buy => fill.quantity,
                            crate::model::OrderSide::Sell => -fill.quantity,
                        };
                        let market = |position: f64| {
                            let direction = if position > 0.0 {
                                "long"
                            } else if position < 0.0 {
                                "short"
                            } else {
                                "flat"
                            };
                            (
                                direction.to_owned(),
                                StrategyReport::quantity_text(position.abs()),
                            )
                        };
                        let (after, after_size) = market(fill.position);
                        let (before, before_size) = market(fill.position - signed);
                        let quantity = StrategyReport::quantity_text(fill.quantity);
                        let position = StrategyReport::quantity_text(fill.position);
                        let price =
                            StrategyReport::price_text_with(fill.price, fill.price_decimals);
                        Some(Occurrence {
                            time: fill.time,
                            price: Some(fill.price),
                            shown_price: Some(StrategyReport::without_rounding_digits(fill.price)),
                            fields: vec![
                                ("order_id", fill.order_id.clone()),
                                ("side", side.into()),
                                ("quantity", quantity.clone()),
                                ("position", position.clone()),
                                ("market_position", after),
                                ("market_position_size", after_size),
                                ("prev_market_position", before),
                                ("prev_market_position_size", before_size),
                                (
                                    "comment",
                                    fill.comment
                                        .clone()
                                        .filter(|comment| !comment.is_empty())
                                        .unwrap_or_else(|| fill.order_id.clone()),
                                ),
                                (
                                    "alert_message",
                                    fill.alert_message.clone().unwrap_or_default(),
                                ),
                            ],
                            message: fill
                                .alert_message
                                .clone()
                                .filter(|message| !message.is_empty())
                                .unwrap_or_else(|| {
                                    format!("{side} {quantity} at {price}, position {position}")
                                }),
                        })
                    })
                    .collect()
            },
        );
    }

    /// Delivers each new alert of `run`, raised through `StrategyContext::alert` and read from
    /// `Backtest::new_alerts` after each update, to the strategy-fill alerts of that run whose
    /// [`strategy_events`](Alert::strategy_events) take alert calls, as
    /// [`strategy_fills`](Self::strategy_fills) delivers its fills. Alerts numbered at or below
    /// one already received are ignored, counted apart from fills. A trigger reports the alert's
    /// bar time and finite price, and the strategy's message when the alert has none; custom
    /// messages expand as for `strategy_fills`, with `{{alert_message}}` the strategy's message
    /// and the other fill placeholders empty.
    #[cfg(feature = "strategies")]
    pub fn strategy_alerts(
        &self,
        run: &str,
        symbol: &str,
        exchange: Option<&str>,
        timeframe: Timeframe,
        alerts: &[crate::strategies::StrategyAlert],
        now: DateTime<Utc>,
    ) {
        use crate::model::{StrategyEvents, StrategyReport};
        let watches = |alert: &Alert| matches!(&alert.condition, AlertCondition::StrategyFills(id) if id == run);
        self.fire_events(
            symbol,
            exchange,
            timeframe,
            now,
            watches,
            |watched, seen, _| {
                alerts
                    .iter()
                    .filter_map(|alert| {
                        if alert.sequence <= seen.last_alert {
                            return None;
                        }
                        seen.last_alert = alert.sequence;
                        // Advance excluded calls too, so changing the choice cannot replay them.
                        if watched.strategy_events == StrategyEvents::Fills {
                            return None;
                        }
                        let price = Some(alert.price).filter(|price| price.is_finite());
                        Some(Occurrence {
                            time: alert.time,
                            price,
                            shown_price: price.map(StrategyReport::without_rounding_digits),
                            message: alert.message.clone(),
                            fields: vec![
                                ("order_id", String::new()),
                                ("side", String::new()),
                                ("quantity", String::new()),
                                ("position", String::new()),
                                ("market_position", String::new()),
                                ("market_position_size", String::new()),
                                ("prev_market_position", String::new()),
                                ("prev_market_position_size", String::new()),
                                ("comment", String::new()),
                                ("alert_message", alert.message.clone()),
                            ],
                        })
                    })
                    .collect()
            },
        );
    }

    /// Delivers each new formation or target in `signals`, the current signals of `symbol` on
    /// `timeframe` from the host's [`Signals`](super::Signals) service, to the
    /// [`DetectorSignals`](AlertCondition::DetectorSignals) alerts naming its category. A signal
    /// fires once per alert and symbol, at its anchor time and price: a marker's time and price,
    /// a level's start and price, a price zone's start and price, a zone's start without a
    /// price. The signal is known by its id, so two charts of the symbol fire it once when every
    /// subscription gives it the same id, as `OrderFlowDetectors` does. A signal anchored before
    /// `from`, or before the `from` of an earlier call for the same alert and symbol, is history
    /// and never fires, nor does a level without a start. `AlertEvaluator` passes the open of
    /// the bar before the newest of each chart it is lent, so a formation confirmed as its bar
    /// closes still fires, while one reported later counts as history. Empty messages are the
    /// detector's name and the signal's tooltip or label; custom messages expand as for
    /// `strategy_fills`.
    pub fn detector_signals<'a>(
        &self,
        symbol: &str,
        exchange: Option<&str>,
        timeframe: Timeframe,
        signals: impl IntoIterator<Item = &'a crate::model::Signal>,
        from: DateTime<Utc>,
        now: DateTime<Utc>,
    ) {
        use crate::model::SignalShape;
        let signals: Vec<_> = signals.into_iter().collect();
        let watches = |alert: &Alert| {
            matches!(&alert.condition, AlertCondition::DetectorSignals(detector)
                if signals.iter().any(|signal| signal.category == *detector))
        };
        self.fire_events(
            symbol,
            exchange,
            timeframe,
            now,
            watches,
            |alert, seen, _| {
                let AlertCondition::DetectorSignals(detector) = &alert.condition else {
                    return Vec::new();
                };
                // History never moves back, or another chart's older bars would fire again the
                // signals whose ids were dropped.
                let from = seen.history.map_or(from, |history| history.max(from));
                seen.history = Some(from);
                // Signals anchored before `from` can no longer fire, so their ids are not kept.
                seen.signals.retain(|(_, time)| *time >= from);
                signals
                    .iter()
                    .filter(|signal| signal.category == *detector)
                    .filter_map(|signal| {
                        let (time, price) = match signal.shape {
                            SignalShape::Marker { time, price, .. } => (time, price),
                            SignalShape::Level {
                                price,
                                from: Some(start),
                            } => (start, Some(price)),
                            SignalShape::Level { from: None, .. } => return None,
                            SignalShape::PriceZone { from, price, .. } => (from, Some(price)),
                            SignalShape::Zone { from, .. } => (from, None),
                        };
                        if time < from || seen.signals.iter().any(|(id, _)| *id == signal.id) {
                            return None;
                        }
                        seen.signals.push((signal.id.clone(), time));
                        let text = signal.tooltip.as_ref().or(signal.label.as_ref());
                        Some(Occurrence {
                            time,
                            price: price.filter(|price| price.is_finite()),
                            shown_price: None,
                            fields: Vec::new(),
                            message: text.map_or_else(
                                || detector.clone(),
                                |text| format!("{detector}: {text}"),
                            ),
                        })
                    })
                    .collect()
            },
        );
    }

    /// Connects the host's custom event source `source`, whose events the host feeds to
    /// [`custom_events`](Self::custom_events). Until then saving an active
    /// [`CustomEvents`](AlertCondition::CustomEvents) alert on it fails, since no event could
    /// reach it. Alerts restored by [`with_alerts`](Self::with_alerts) wait for their source.
    pub fn connect_custom_events(&self, source: impl Into<String>) {
        self.lock().routes.insert(source.into());
    }

    /// Delivers each new event of the host's custom source `source` on `symbol` and
    /// `timeframe` to the [`CustomEvents`](AlertCondition::CustomEvents) alerts naming it.
    /// Events numbered at or below one an alert already received are ignored, including after
    /// edits or rearming the alert. A trigger reports the event's time and finite price, and the
    /// event's message when the alert has none; custom messages expand as for
    /// `strategy_fills`.
    pub fn custom_events(
        &self,
        source: &str,
        symbol: &str,
        exchange: Option<&str>,
        timeframe: Timeframe,
        events: &[crate::model::CustomAlertEvent],
        now: DateTime<Utc>,
    ) {
        let watches = |alert: &Alert| matches!(&alert.condition, AlertCondition::CustomEvents(name) if name == source);
        self.fire_events(symbol, exchange, timeframe, now, watches, |_, seen, _| {
            events
                .iter()
                .filter_map(|event| {
                    if event.sequence <= seen.last_sequence {
                        return None;
                    }
                    seen.last_sequence = event.sequence;
                    Some(Occurrence {
                        time: event.time,
                        price: event.price.filter(|price| price.is_finite()),
                        shown_price: None,
                        message: event.message.clone(),
                        fields: Vec::new(),
                    })
                })
                .collect()
        });
    }

    /// Delivers the live conditions drained by `OrderFlowDetectors::take_footprint_events` to
    /// footprint alerts on their symbol and interval. `Once` and `OncePerBar` fire at the first
    /// execution that meets the condition; `OncePerBarClose` fires on a closed bar that meets
    /// it. Each alert fires at most once per bar, including duplicate events from other charts.
    /// Triggers carry the event's time and price, and its message when the alert has none;
    /// custom messages expand as for `strategy_fills`.
    pub fn footprint_events(&self, events: &[FootprintAlertEvent], now: DateTime<Utc>) {
        for event in events {
            self.fire_events(
                &event.symbol,
                event.exchange.as_deref(),
                event.timeframe,
                now,
                |alert| {
                    alert.condition == AlertCondition::Footprint(event.condition)
                        && (alert.frequency == AlertFrequency::OncePerBarClose) == event.closed
                },
                |_, seen, last| {
                    // A restored run counts its saved firing only on its first delivery.
                    if seen.fired.is_none()
                        && last.is_some_and(|last| {
                            if event.closed {
                                event.time <= last
                            } else {
                                event.bar <= last
                            }
                        })
                    {
                        seen.fired = Some(event.bar);
                    }
                    if seen.fired.is_some_and(|bar| event.bar <= bar) {
                        return Vec::new();
                    }
                    seen.fired = Some(event.bar);
                    vec![Occurrence {
                        time: event.time,
                        price: Some(event.price).filter(|price| price.is_finite()),
                        shown_price: None,
                        message: event.message.clone(),
                        fields: Vec::new(),
                    }]
                },
            );
        }
    }

    /// Fires the active alerts of `symbol` on `timeframe` whose alert `watches` accepts,
    /// with the occurrences `occurrences` takes from what the alert's run has seen and, for a
    /// restored alert, the symbol's saved last firing.
    fn fire_events(
        &self,
        symbol: &str,
        exchange: Option<&str>,
        timeframe: Timeframe,
        now: DateTime<Utc>,
        watches: impl Fn(&Alert) -> bool,
        mut occurrences: impl FnMut(&Alert, &mut Run, Option<DateTime<Utc>>) -> Vec<Occurrence>,
    ) {
        let mut fired = Vec::new();
        let mut state = self.lock();
        state.expire(now);
        let State {
            alerts,
            runs,
            events,
            watchlists,
            triggers,
            restored,
            ..
        } = &mut *state;
        for alert in alerts.iter_mut().filter(|alert| {
            alert.status == AlertStatus::Active
                && alert.is_valid()
                && alert.starts.is_none_or(|starts| starts <= now)
                && watches(alert)
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
            let done = member_done(alert, symbol);
            let seen = runs
                .entry((id.clone(), symbol.to_owned()))
                .or_insert_with(|| Run {
                    done,
                    ..Run::default()
                });
            let last = match &alert.watchlist {
                _ if !restored.contains(&id) => None,
                Some(_) => alert.member_triggered_at.get(symbol).copied(),
                None => alert.last_triggered_at,
            };
            for occurrence in occurrences(alert, seen, last) {
                if seen.done {
                    continue;
                }
                let Occurrence {
                    time,
                    price,
                    shown_price,
                    message,
                    fields,
                } = occurrence;
                let message = if alert.message.is_empty() {
                    message
                } else {
                    let shown = shown_price.or(price);
                    let flat = shown.unwrap_or(f64::NAN);
                    let bar = Bar::new(time, flat, flat, flat, flat, 0.0);
                    alert.expand_message_with(
                        symbol,
                        exchange,
                        timeframe,
                        &bar,
                        time,
                        |source| shown.filter(|_| *source == AlertSource::Price),
                        &fields,
                    )
                };
                if alert
                    .schedule
                    .as_ref()
                    .is_some_and(|schedule| !schedule.allows(now))
                {
                    let event = AlertEvent::SilentTriggered {
                        id: id.clone(),
                        symbol: symbol.to_owned(),
                        time,
                        price,
                        message,
                    };
                    remember(triggers, alert, &event);
                    if let Some(events) = events {
                        events.push(event);
                    }
                    continue;
                }
                alert.last_triggered_at =
                    Some(alert.last_triggered_at.map_or(time, |old| old.max(time)));
                record_member(alert, symbol, time);
                if alert.frequency == AlertFrequency::Once {
                    seen.done = true;
                    if alert.watchlist.is_none() || finished(alert, watchlists) {
                        alert.status = AlertStatus::Triggered;
                    }
                    if let Some(events) = events {
                        events.push(AlertEvent::Upsert(Box::new(alert.clone())));
                    }
                }
                let event = AlertEvent::TriggeredForSymbol {
                    id: id.clone(),
                    symbol: symbol.to_owned(),
                    time,
                    price,
                    message,
                };
                remember(triggers, alert, &event);
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
        let restoring = alert.id.is_some();
        // A position alert made active again after its exit waits for the entry again.
        if alert.status == AlertStatus::Active
            && let AlertCondition::Position { progress, .. } = &mut alert.condition
            && matches!(
                progress,
                AlertPositionProgress::TargetReached | AlertPositionProgress::StopReached
            )
        {
            *progress = AlertPositionProgress::AwaitingEntry;
        }
        if alert.id.is_none() {
            alert.created_at = Some(Utc::now());
            alert.last_triggered_at = None;
            alert.member_triggered_at.clear();
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
        if let Some(id) = &alert.id {
            if existing.is_some_and(|old| !old.same_monitoring(&alert)) {
                self.restored.remove(id);
            } else if existing.is_none() && restoring {
                self.restored.insert(id.clone());
            }
        }
        if let Some(old) = existing {
            alert.created_at = old.created_at;
            alert.last_triggered_at = old.last_triggered_at;
            // A save that changes what the alert watches or its status starts its members afresh.
            alert.member_triggered_at = if old.same_monitoring(&alert) {
                old.member_triggered_at.clone()
            } else {
                BTreeMap::new()
            };
        }
        let same_monitoring = existing.is_some_and(|old| {
            old.same_monitoring(&alert)
                && (old.extend_line != alert.extend_line
                    || old.blink_on_trigger != alert.blink_on_trigger
                    || old.delivery != alert.delivery)
        });
        let same_events = alert.condition.is_event()
            && self
                .alerts
                .iter()
                .any(|old| old.id == alert.id && old.condition == alert.condition);
        self.runs.retain(|(id, _), run| {
            if Some(id) != alert.id.as_ref() || same_monitoring {
                return true;
            }
            if same_events {
                // Editing or rearming an alert on host events must not replay them.
                run.done = false;
            }
            same_events
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
            if !alert.condition.is_event() {
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
        if !std::mem::replace(&mut state.watched, true) {
            // The alerts and the history share the stream's 10,000 pushes per frame; 1,000 stay
            // free for the frame's other pushes.
            let room = 9_000usize.saturating_sub(state.alerts.len());
            let skip = state.triggers.len().saturating_sub(room);
            for trigger in state.triggers.iter().skip(skip) {
                events.push(AlertEvent::Logged(trigger.clone()));
            }
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
        if let AlertCondition::CustomEvents(source) = &alert.condition
            && alert.status == AlertStatus::Active
            && !state.routes.contains(source)
        {
            reply.send(Err(FeedError::Other(format!(
                "No event route connects the custom event source {source}"
            ))));
            return;
        }
        let alert = state.insert(alert);
        state.push(AlertEvent::Upsert(Box::new(alert)));
        reply.send(Ok(()));
    }

    fn remove(&self, id: AlertId, reply: Reply<()>) {
        let mut state = self.lock();
        state.runs.retain(|(kept, _), _| *kept != id);
        state.restored.remove(&id);
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
            AlertSource::TPOLevel(level) => level.price,
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
                let [a, b, c] = condition.partners().map(|other| other.and_then(source));
                [source(src), a, b, c]
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
        bar: [bars[at].close, bars[at].volume, bars[at].high, bars[at].low].map(f64::to_bits),
        rules: read(Some(at)),
    };
    let seen = seen_at(index);
    if previous.is_some_and(|previous| previous.seen == seen) {
        if let Some(previous) = run.intervals.get_mut(&timeframe) {
            previous.available = true;
        }
        return true;
    }
    let traded = intervals
        .iter()
        .position(|interval| *interval == timeframe)
        .and_then(|position| run.joined.get(position))
        .filter(|_| alert.frequency != AlertFrequency::OncePerBarClose)
        .map(|last| traded_since(bars, index, last));
    let observed = |at: usize, seen: Seen, before, event, earlier| {
        let traded = traded.filter(|_| at == index);
        let held = |traded| {
            rules
                .iter()
                .zip(&seen.rules)
                .all(|((src, condition), &[current, a, b, c])| {
                    reads_previous(condition)
                        || current.is_some_and(|current| {
                            condition_holds(
                                condition,
                                current,
                                range(traded, src, current),
                                [None; 4],
                                [a, b, c],
                                &bars[..=at],
                                |back| at.checked_sub(back as usize).and_then(|at| source(src, at)),
                            )
                        })
                })
        };
        let (holds, still_holds) = (held(traded), held(None));
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
            still_holds,
            before,
            clock: clock.clone(),
            event,
            bar: bars[at].clone(),
            next: bars.get(at + 1).map(|bar| bar.time),
            values: values.into_inner(),
            earlier,
            traded,
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
/// and messages to fire, one per trigger (a position can enter and exit on one evaluation), and
/// whether they fall outside the schedule and are only logged. `restored` tells whether the
/// alert was restored, see [`State::restored`].
fn evaluate(
    alert: &mut Alert,
    run: &mut Run,
    symbol: &str,
    exchange: Option<&str>,
    now: DateTime<Utc>,
    restored: bool,
) -> Option<(Bar, Vec<String>, bool)> {
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
    // A watchlist member reads its own time: another member may have set the alert's.
    let last = match &alert.watchlist {
        _ if !restored => None,
        Some(_) => alert.member_triggered_at.get(symbol).copied(),
        None => alert.last_triggered_at,
    };
    // A restored alert takes its saved last firing on its first evaluation.
    if run.joined.is_empty()
        && let Some(last) = last
    {
        if alert.frequency == AlertFrequency::OncePerMinute {
            run.fired = Some(occurrence(alert.frequency, &anchor.bar, last));
        }
        // The evaluated bar held the firing, or for an alert at bar close, completed before it.
        let covered = match alert.frequency {
            AlertFrequency::OncePerBarClose => anchor.next.is_some_and(|next| next <= last),
            _ => anchor.bar.time <= last,
        };
        if covered {
            // Rules that compare two evaluations need a transition seen after the restart.
            run.joined = joined
                .iter()
                .map(|observation| observation.seen.clone())
                .collect();
            if matches!(
                alert.frequency,
                AlertFrequency::OncePerBar | AlertFrequency::OncePerBarClose
            ) {
                run.fired = Some(occurrence(alert.frequency, &anchor.bar, now));
            }
        }
    }
    // An alert saved in this session compares only values it observed, so its first evaluation
    // records them: a crossing that happened before the save never fires.
    if run.joined.is_empty() && !restored {
        run.joined = joined
            .iter()
            .map(|observation| observation.seen.clone())
            .collect();
    }
    let holds =
        joined
            .iter()
            .zip(&intervals)
            .enumerate()
            .all(|(position, (observation, interval))| {
                let last = run.joined.get(position);
                // An interval the previous evaluation read has traded nothing since.
                let holds = if last == Some(&observation.seen) {
                    observation.still_holds
                } else {
                    observation.holds
                };
                holds && previous_rules_hold(alert, *interval, observation, last)
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
    let position = match &alert.condition {
        AlertCondition::Position { progress, .. } => Some(*progress),
        _ => None,
    };
    let silent = alert
        .schedule
        .as_ref()
        .is_some_and(|schedule| !schedule.allows(now));
    if silent {
        let at = occurrence(alert.frequency, bar, now);
        // A position alert moves on with each trigger, so it never repeats one.
        if position.is_none()
            && alert.frequency != AlertFrequency::EveryTime
            && run.silent == Some(at)
        {
            return None;
        }
        run.silent = Some(at);
    }
    let first = alert.timeframe.unwrap_or_default();
    let (primary, _) = joined
        .iter()
        .zip(&intervals)
        .find(|(_, interval)| **interval == first)?;
    let message = |alert: &Alert| {
        alert.expand_message(symbol, exchange, first, &primary.bar, now, |source| {
            primary
                .values
                .iter()
                .find(|(kept, _)| kept == source)
                .and_then(|(_, value)| *value)
        })
    };
    let Some(progress) = position else {
        return Some((bar.clone(), vec![message(alert)], silent));
    };
    // A position alert has one rule: the value and its entry, target and stop.
    let [value, entry, target, stop] = primary.seen.rules[0];
    let mut stages = Vec::new();
    if progress != AlertPositionProgress::Entered {
        stages.push(AlertPositionProgress::Entered);
    }
    if let (Some(value), Some(entry), Some(target), Some(stop)) = (value, entry, target, stop) {
        stages.extend(position_exit(value, entry, target, stop));
        alert.position_levels = Some([entry, target, stop]);
    }
    let mut messages = Vec::new();
    for stage in stages {
        if let AlertCondition::Position { progress, .. } = &mut alert.condition {
            *progress = stage;
        }
        messages.push(if alert.message.is_empty() {
            stage.to_string()
        } else {
            message(alert)
        });
    }
    Some((bar.clone(), messages, silent))
}

/// Whether the rules of `interval` that compare two evaluations hold between `last`, the inputs of
/// the alert's previous evaluation, and `observation`. Without `last`, and for a moving channel
/// on a new bar, the earlier side is the bar before the observed one. An observation `last` read
/// has traded nothing since, so its rules read the close.
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
        .all(|(n, (source, condition, _))| {
            let [current, a, b, c] = observation.seen.rules[n];
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
                condition_holds(
                    condition,
                    current,
                    range(
                        observation
                            .traded
                            .filter(|_| last != Some(&observation.seen)),
                        source,
                        current,
                    ),
                    previous,
                    [a, b, c],
                    &[],
                    |_| None,
                )
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
        | AlertCondition::ExitingChannel { .. }
        | AlertCondition::Position { .. } => true,
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

/// Whether `alert` is a position that reached its exit, whose levels still need watching.
fn ended_position(alert: &Alert) -> bool {
    alert.status == AlertStatus::Triggered
        && matches!(
            alert.condition,
            AlertCondition::Position {
                progress: AlertPositionProgress::TargetReached | AlertPositionProgress::StopReached,
                ..
            }
        )
}

/// Restarts a position when any level moved after entry or exit. The
/// observation reading the edit becomes its baseline, so the edit cannot fire. Older saved
/// alerts adopt the first resolved levels without changing their progress.
fn restart_position(alert: &mut Alert, run: &mut Run) -> bool {
    let AlertCondition::Position { progress, .. } = &mut alert.condition else {
        return false;
    };
    let Some(newest) = run.intervals.get(&alert.timeframe.unwrap_or_default()) else {
        return false;
    };
    // A position has one rule: the value and its entry, target and stop.
    let Some(&[_, Some(entry), Some(target), Some(stop)]) = newest.seen.rules.first() else {
        return false;
    };
    let levels = [entry, target, stop];
    let moved = match (*progress, alert.position_levels) {
        (AlertPositionProgress::AwaitingEntry, _) => false,
        (_, None) => {
            alert.position_levels = Some(levels);
            false
        }
        (_, Some(reached)) => reached != levels,
    };
    if moved {
        *progress = AlertPositionProgress::AwaitingEntry;
        alert.status = AlertStatus::Active;
        alert.position_levels = None;
        run.joined = vec![newest.seen.clone()];
        run.fired = None;
        run.silent = None;
        run.done = false;
    }
    moved
}

/// Whether `alert`, a once alert on a watchlist, fired for `symbol` before, as its saved member
/// record holds.
fn member_done(alert: &Alert, symbol: &str) -> bool {
    alert.frequency == AlertFrequency::Once
        && alert.watchlist.is_some()
        && alert.member_triggered_at.contains_key(symbol)
}

/// Whether every member of the watchlist of `alert` has fired. A list the host has not set, or an
/// empty one, is never finished.
fn finished(alert: &Alert, watchlists: &HashMap<String, Vec<String>>) -> bool {
    alert
        .watchlist
        .as_ref()
        .and_then(|list| watchlists.get(list))
        .is_some_and(|members| {
            !members.is_empty()
                && members
                    .iter()
                    .all(|member| alert.member_triggered_at.contains_key(member))
        })
}

/// Records `time` as the latest firing of the watchlist member `symbol`.
fn record_member(alert: &mut Alert, symbol: &str, time: DateTime<Utc>) {
    if alert.watchlist.is_some() {
        let at = alert
            .member_triggered_at
            .entry(symbol.to_owned())
            .or_insert(time);
        *at = (*at).max(time);
    }
}

/// Keeps the trigger `event` of `alert` as the newest of `history`, dropping the oldest beyond
/// the bound.
fn remember(history: &mut VecDeque<AlertTrigger>, alert: &Alert, event: &AlertEvent) {
    let (AlertEvent::TriggeredForSymbol {
        id,
        symbol,
        time,
        price,
        message,
    }
    | AlertEvent::SilentTriggered {
        id,
        symbol,
        time,
        price,
        message,
    }) = event
    else {
        return;
    };
    if history.len() == TRIGGER_HISTORY {
        history.pop_front();
    }
    history.push_back(AlertTrigger {
        id: id.clone(),
        name: alert.display_name(),
        symbol: symbol.clone(),
        time: *time,
        price: price.filter(|p| p.is_finite()),
        message: message.clone(),
        silent: matches!(event, AlertEvent::SilentTriggered { .. }),
    });
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

/// The lowest and highest price known to have traded on `bars` up to `at` since `last`, the
/// inputs of the alert's previous evaluation of this interval: the close at `at`, the low and
/// high of each bar that opened after the one `last` read, and those of that bar where they
/// passed what `last` read. A value that is not finite counts for nothing.
fn traded_since(bars: &[Bar], at: usize, last: &Seen) -> [f64; 2] {
    let close = bars[at].close;
    let [_, _, high, low] = last.bar.map(f64::from_bits);
    bars[..=at]
        .iter()
        .rev()
        .take_while(|bar| bar.time >= last.time)
        .fold([close, close], |[lowest, highest], bar| {
            let later = bar.time > last.time;
            [
                if bar.low.is_finite() && (later || bar.low < low) {
                    lowest.min(bar.low)
                } else {
                    lowest
                },
                if bar.high.is_finite() && (later || bar.high > high) {
                    highest.max(bar.high)
                } else {
                    highest
                },
            ]
        })
}

/// The range a rule on `source` reads at `value`: `traded` for the price, which may hold one,
/// and `value` alone otherwise.
fn range(traded: Option<[f64; 2]>, source: &AlertSource, value: f64) -> [f64; 2] {
    traded
        .filter(|_| *source == AlertSource::Price)
        .unwrap_or([value, value])
}

/// Whether `condition` holds for `value`, given the lowest and highest value traded since the
/// previous evaluation, which a level or a drawing is tested against, the previous value of the
/// source, the current and previous value of the other outputs, the bars up to the evaluated one
/// and the value of the source `back` bars before it.
fn condition_holds(
    condition: &AlertCondition,
    value: f64,
    traded: [f64; 2],
    previous: [Option<f64>; 4],
    other: [Option<f64>; 3],
    bars: &[Bar],
    lookback: impl Fn(u32) -> Option<f64>,
) -> bool {
    let [previous, previous_other, previous_second, _] = previous;
    match condition {
        AlertCondition::Holds => value != 0.0,
        AlertCondition::StrategyFills(_)
        | AlertCondition::DetectorSignals(_)
        | AlertCondition::CustomEvents(_)
        | AlertCondition::Footprint(_) => false,
        AlertCondition::Crossing(level) => compare(
            AlertComparison::Crossing,
            traded,
            value,
            previous,
            *level,
            Some(*level),
        ),
        AlertCondition::CrossingUp(level) => compare(
            AlertComparison::CrossingUp,
            traded,
            value,
            previous,
            *level,
            Some(*level),
        ),
        AlertCondition::CrossingDown(level) => compare(
            AlertComparison::CrossingDown,
            traded,
            value,
            previous,
            *level,
            Some(*level),
        ),
        AlertCondition::Above(level) => traded[1] > *level,
        AlertCondition::Below(level) => traded[0] < *level,
        AlertCondition::EnteringChannel { lower, upper }
        | AlertCondition::ExitingChannel { lower, upper }
        | AlertCondition::InsideChannel { lower, upper }
        | AlertCondition::OutsideChannel { lower, upper } => {
            let comparison = match condition {
                AlertCondition::EnteringChannel { .. } => AlertChannelComparison::Entering,
                AlertCondition::ExitingChannel { .. } => AlertChannelComparison::Exiting,
                AlertCondition::InsideChannel { .. } => AlertChannelComparison::Inside,
                _ => AlertChannelComparison::Outside,
            };
            let was_inside = previous.map(|p| p >= *lower && p <= *upper);
            channel(comparison, traded, value, was_inside, [*lower, *upper])
        }
        AlertCondition::MovingBy {
            value: change,
            bars: back,
        } => lookback(*back).is_some_and(|p| reaches(value - p, *change)),
        AlertCondition::MovingByPercent {
            percent,
            bars: back,
        } => lookback(*back)
            .is_some_and(|p| p != 0.0 && reaches(100.0 * (value - p) / p.abs(), *percent)),
        // A drawing's level or a TPO level does not follow the price, so the range traded meets it.
        AlertCondition::Compare {
            other: partner,
            comparison,
        } => {
            let traded = match partner {
                AlertSource::Drawing(_) | AlertSource::TPOLevel(_) => traded,
                _ => [value, value],
            };
            other[0].is_some_and(|level| {
                compare(*comparison, traded, value, previous, level, previous_other)
            })
        }
        AlertCondition::Channel {
            boundaries,
            comparison,
        } => {
            let bounds = |a: f64, b: f64| (a != b).then_some([a.min(b), a.max(b)]);
            let Some(current) = other[0].zip(other[1]).and_then(|(a, b)| bounds(a, b)) else {
                return false;
            };
            let was_inside =
                previous
                    .zip(previous_other)
                    .zip(previous_second)
                    .and_then(|((value, a), b)| {
                        bounds(a, b).map(|[low, high]| value >= low && value <= high)
                    });
            // Drawings do not follow the price, so the range traded meets them.
            let traded = if boundaries
                .iter()
                .all(|b| matches!(b, AlertSource::Drawing(_)))
            {
                traded
            } else {
                [value, value]
            };
            channel(*comparison, traded, value, was_inside, current)
        }
        AlertCondition::Position { progress, .. } => {
            let [Some(entry), Some(target), Some(stop)] = other else {
                return false;
            };
            match progress {
                AlertPositionProgress::Entered => {
                    position_exit(value, entry, target, stop).is_some()
                }
                _ => compare(
                    AlertComparison::Crossing,
                    [value, value],
                    value,
                    previous,
                    entry,
                    previous_other,
                ),
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

/// Whether `comparison` holds for a value that traded between `low` and `high` since the
/// previous one, which `was_inside` tells was inside the inclusive channel from `lower` to
/// `upper`, if known. The range is inside when it reaches the channel and outside when it passes
/// either boundary, so a wick out of the channel and back is both. Entering also holds when the
/// range passed a boundary and `value`, the current value, is back inside, and exiting when the
/// range reached the channel and `value` is outside: the way back of a move the previous
/// evaluation did not see. A range past both boundaries went through the channel, so it enters
/// and exits it whatever the previous value was.
fn channel(
    comparison: AlertChannelComparison,
    [low, high]: [f64; 2],
    value: f64,
    was_inside: Option<bool>,
    [lower, upper]: [f64; 2],
) -> bool {
    let inside = low <= upper && high >= lower;
    let outside = low < lower || high > upper;
    let ends_inside = value >= lower && value <= upper;
    let through = low < lower && high > upper;
    match comparison {
        AlertChannelComparison::Entering => {
            was_inside.is_some_and(|was| (!was && inside) || (outside && ends_inside) || through)
        }
        AlertChannelComparison::Exiting => {
            was_inside.is_some_and(|was| (was && outside) || (inside && !ends_inside) || through)
        }
        AlertChannelComparison::Inside => inside,
        AlertChannelComparison::Outside => outside,
    }
}

/// Whether `comparison` holds for a value that traded between `low` and `high` since the
/// previous one, `previous`, against `level`, which was `previous_level` then. A range strictly
/// past the level with `value`, the current value, back at or across it also crosses, on the way
/// back.
fn compare(
    comparison: AlertComparison,
    [low, high]: [f64; 2],
    value: f64,
    previous: Option<f64>,
    level: f64,
    previous_level: Option<f64>,
) -> bool {
    let sides = previous.zip(previous_level);
    let up =
        sides.is_some_and(|(p, l)| (p < l && high >= level) || (low < level && value >= level));
    let down =
        sides.is_some_and(|(p, l)| (p > l && low <= level) || (high > level && value <= level));
    match comparison {
        AlertComparison::Crossing => up || down,
        AlertComparison::CrossingUp => up,
        AlertComparison::CrossingDown => down,
        AlertComparison::Above => high > level,
        AlertComparison::Below => low < level,
    }
}

/// The exit a position entered at `entry` reaches at `value`, the stop first: long when the
/// target is above the entry, short when it is below, and none while they are equal.
fn position_exit(value: f64, entry: f64, target: f64, stop: f64) -> Option<AlertPositionProgress> {
    let long = match target.partial_cmp(&entry)? {
        std::cmp::Ordering::Greater => true,
        std::cmp::Ordering::Less => false,
        std::cmp::Ordering::Equal => return None,
    };
    let beyond = |level: f64, above: bool| {
        if above {
            value >= level
        } else {
            value <= level
        }
    };
    if beyond(stop, !long) {
        Some(AlertPositionProgress::StopReached)
    } else if beyond(target, long) {
        Some(AlertPositionProgress::TargetReached)
    } else {
        None
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

    /// Trend flips fire on bars 2, 4 and 6, including after storing the alert and restarting.
    #[test]
    #[cfg(all(feature = "drawings", feature = "studies"))]
    fn a_study_condition_alert_fires_on_each_trend_flip_and_after_a_restart() {
        let bars: Vec<_> = [100., 101., 105., 106., 100., 99., 103.]
            .into_iter()
            .enumerate()
            .map(|(i, c)| Bar::new(time(i as i64 * 60), c, c + 1., c - 1., c, 1.))
            .collect();
        let mut chart = crate::Chart::new(crate::model::BarData::from_bars(bars.clone()));
        let id = chart.add_indicator(Box::new(crate::studies::SuperTrend::new(1, 1.0)));
        let trend = chart
            .indicators()
            .find(|(study, _)| *study == id)
            .unwrap()
            .1;
        assert_eq!(trend.values().value(0, 0), None);
        assert_eq!(trend.values().value(1, 0), None);
        for (bar, (line, value)) in [
            (1, 102.),
            (0, 100.),
            (0, 104.),
            (1, 107.),
            (1, 101.),
            (0, 98.),
        ]
        .into_iter()
        .enumerate()
        {
            assert_eq!(trend.values().value(line, bar + 1), Some(value));
            assert_eq!(trend.values().value(1 - line, bar + 1), None);
        }
        let mut alert = alert(AlertCondition::Holds, AlertFrequency::EveryTime);
        alert.source = AlertSource::StudyCondition {
            id: id.0.to_string(),
            condition: 2,
        };
        chart.prepare_alert(&mut alert);
        let update = |host: &Host, n| {
            host.engine.update(
                "SYM",
                None,
                Timeframe::Min1,
                &bars[..=n],
                time(60 * n as i64 + 1),
                |source, at| chart.alert_value(source, at),
            )
        };
        let host = Host::new(vec![alert.clone()]);
        for n in 0..bars.len() {
            update(&host, n);
        }
        assert_eq!(host.fired(), vec![105., 100., 103.]);
        let host = Host::new(vec![alert]);
        for n in 0..=3 {
            update(&host, n);
        }
        assert_eq!(host.fired(), vec![105.]);
        let saved = host.engine.alerts();
        let restored: Vec<Alert> =
            serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap();
        assert_eq!(restored, saved);
        assert!(matches!(
            restored[0].source,
            AlertSource::StudyCondition { condition: 2, .. }
        ));
        assert!(
            restored[0]
                .inputs
                .as_ref()
                .unwrap()
                .studies
                .iter()
                .any(|study| study.id == id.0 && study.name == "SuperTrend")
        );
        let host = Host::new(restored);
        for n in 4..bars.len() {
            update(&host, n);
        }
        assert_eq!(host.fired(), vec![100., 103.]);
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
    fn position_alerts_report_the_entry_then_the_first_exit_and_end() {
        use crate::model::{NotificationSchedule, NotificationWindow, TimezoneId};
        use AlertPositionProgress::{AwaitingEntry, Entered, StopReached, TargetReached};
        use chrono::{NaiveTime, Weekday};
        type PositionRow = (
            [f64; 3],
            f64,
            Vec<f64>,
            Vec<(&'static str, f64)>,
            Vec<AlertPositionProgress>,
        );
        let entered = "Position entered";
        let target = "Position target reached";
        let stop = "Position stop reached";
        let rows: Vec<PositionRow> = vec![
            (
                [100.0, 110.0, 95.0],
                98.0,
                vec![99.0, 100.0, 105.0, 110.0, 111.0],
                vec![(entered, 100.0), (target, 110.0)],
                vec![Entered, TargetReached],
            ),
            (
                [100.0, 110.0, 95.0],
                102.0,
                vec![101.0, 100.0, 97.0, 95.0, 94.0],
                vec![(entered, 100.0), (stop, 95.0)],
                vec![Entered, StopReached],
            ),
            (
                [100.0, 110.0, 95.0],
                98.0,
                vec![95.0, 93.0, 100.0, 96.0, 95.0],
                vec![(entered, 100.0), (stop, 95.0)],
                vec![Entered, StopReached],
            ),
            (
                [100.0, 110.0, 95.0],
                105.0,
                vec![108.0, 111.0, 112.0],
                vec![],
                vec![],
            ),
            (
                [100.0, 90.0, 105.0],
                102.0,
                vec![101.0, 99.5, 95.0, 90.0, 89.0],
                vec![(entered, 99.5), (target, 90.0)],
                vec![Entered, TargetReached],
            ),
            (
                [100.0, 90.0, 105.0],
                98.0,
                vec![99.0, 100.5, 104.0, 105.5],
                vec![(entered, 100.5), (stop, 105.5)],
                vec![Entered, StopReached],
            ),
            ([100.0, 90.0, 105.0], 95.0, vec![92.0, 89.0], vec![], vec![]),
            (
                [100.0, 110.0, 95.0],
                98.0,
                vec![99.0, 112.0, 113.0],
                vec![(entered, 112.0), (target, 112.0)],
                vec![TargetReached],
            ),
            // A gap down enters and exits a short on the same update.
            (
                [100.0, 90.0, 105.0],
                102.0,
                vec![89.0],
                vec![(entered, 89.0), (target, 89.0)],
                vec![TargetReached],
            ),
            // With both exits reached, the protective stop takes precedence.
            (
                [100.0, 110.0, 115.0],
                98.0,
                vec![112.0],
                vec![(entered, 112.0), (stop, 112.0)],
                vec![StopReached],
            ),
            // Coincident entry and target have no direction, so entry stays open.
            (
                [100.0, 100.0, 95.0],
                98.0,
                vec![100.0, 94.0],
                vec![(entered, 100.0)],
                vec![Entered],
            ),
            // Every level must be available, including the stop before entry.
            (
                [100.0, 110.0, f64::NAN],
                98.0,
                vec![100.0, 112.0],
                vec![],
                vec![],
            ),
        ];
        for (index, (levels, history, updates, triggers, progress)) in rows.into_iter().enumerate()
        {
            // Outside the schedule the same triggers are only logged, and progress still moves.
            for silent in [false, true] {
                let condition = AlertCondition::Position {
                    levels: Box::new(["p", "p#1", "p#2"].map(|id| AlertSource::Drawing(id.into()))),
                    progress: AwaitingEntry,
                };
                let mut watching = alert(condition, AlertFrequency::Once);
                if silent {
                    watching.schedule = Some(Box::new(NotificationSchedule {
                        timezone: TimezoneId::utc(),
                        days: vec![Weekday::Thu],
                        windows: vec![NotificationWindow {
                            start: NaiveTime::from_hms_opt(0, 1, 0).unwrap(),
                            end: NaiveTime::from_hms_opt(0, 2, 0).unwrap(),
                        }],
                    }));
                }
                let host = Host::new(vec![watching]);
                host.events();
                let mut bars = vec![bar(0, history, 1.0)];
                for (i, &close) in updates.iter().enumerate() {
                    bars.push(bar(i as i64 + 1, close, 1.0));
                    host.engine.update(
                        "SYM",
                        None,
                        Timeframe::Min1,
                        &bars,
                        time(1_000 + i as i64),
                        |source, _| {
                            let AlertSource::Drawing(id) = source else {
                                return None;
                            };
                            ["p", "p#1", "p#2"]
                                .iter()
                                .position(|name| *name == id)
                                .map(|n| levels[n])
                        },
                    );
                }
                let events = host.events();
                let fired: Vec<_> = events
                    .iter()
                    .filter_map(|event| match event {
                        AlertEvent::TriggeredForSymbol {
                            message,
                            price: Some(price),
                            ..
                        } if !silent => Some((message.as_str(), *price)),
                        AlertEvent::SilentTriggered {
                            message,
                            price: Some(price),
                            ..
                        } if silent => Some((message.as_str(), *price)),
                        _ => None,
                    })
                    .collect();
                let reported: Vec<_> = events
                    .iter()
                    .filter_map(|event| match event {
                        AlertEvent::Upsert(alert) => match alert.condition {
                            AlertCondition::Position { progress, .. } => Some(progress),
                            _ => None,
                        },
                        _ => None,
                    })
                    .collect();
                assert_eq!(fired, triggers, "row {index}, silent {silent}");
                // The history keeps every trigger pushed, silent ones marked.
                let history = host.engine.triggers();
                let kept: Vec<_> = history
                    .iter()
                    .filter_map(|t| Some((t.message.as_str(), t.price?)))
                    .collect();
                assert_eq!(kept, triggers, "row {index}, silent {silent}");
                assert!(
                    history.iter().all(|t| t.silent == silent),
                    "row {index}, silent {silent}"
                );
                assert_eq!(reported, progress, "row {index}, silent {silent}");
                assert_eq!(
                    host.delivered.lock().unwrap().len(),
                    if silent { 0 } else { triggers.len() },
                    "row {index}, silent {silent}"
                );
                let saved = host.engine.alerts().remove(0);
                let last = progress.last().copied().unwrap_or(AwaitingEntry);
                assert!(
                    matches!(
                        saved.condition,
                        AlertCondition::Position { progress, .. } if progress == last
                    ),
                    "row {index}, silent {silent}"
                );
                assert_eq!(
                    saved.status == AlertStatus::Triggered,
                    matches!(last, TargetReached | StopReached),
                    "row {index}, silent {silent}"
                );
            }
        }
    }

    #[test]
    fn a_position_alert_resumes_after_a_restart_and_restarts_after_a_moved_stop() {
        use AlertPositionProgress::{AwaitingEntry, StopReached};
        let condition = AlertCondition::Position {
            levels: Box::new(["p", "p#1", "p#2"].map(|id| AlertSource::Drawing(id.into()))),
            progress: AwaitingEntry,
        };
        let stop = std::cell::Cell::new(95.0);
        let feed_position = |host: &Host, prices: &[f64]| {
            let bars: Vec<_> = std::iter::once(98.0)
                .chain(prices.iter().copied())
                .enumerate()
                .map(|(i, close)| bar(i as i64, close, 1.0))
                .collect();
            host.engine.update(
                "SYM",
                None,
                Timeframe::Min1,
                &bars,
                time(1_000 + prices.len() as i64),
                |source, _| match source {
                    AlertSource::Drawing(id) => match id.as_str() {
                        "p" => Some(100.0),
                        "p#1" => Some(110.0),
                        "p#2" => Some(stop.get()),
                        _ => None,
                    },
                    _ => None,
                },
            );
        };
        let host = Host::new(vec![alert(condition, AlertFrequency::Once)]);
        feed_position(&host, &[99.0]);
        assert!(host.fired().is_empty());
        let saved = serde_json::to_value(&host.engine.alerts()[0]).unwrap();
        assert_eq!(saved["condition"]["Position"]["progress"], "AwaitingEntry");
        let host = Host::new(vec![serde_json::from_value(saved).unwrap()]);
        feed_position(&host, &[99.0]);
        feed_position(&host, &[99.0, 94.0]);
        assert!(host.fired().is_empty());
        feed_position(&host, &[99.0, 94.0, 100.0]);
        assert_eq!(host.fired(), [100.0]);
        let saved = serde_json::to_value(&host.engine.alerts()[0]).unwrap();
        assert_eq!(
            saved["condition"],
            serde_json::json!({"Position": {
                "levels": [{"Drawing": "p"}, {"Drawing": "p#1"}, {"Drawing": "p#2"}],
                "progress": "Entered"
            }})
        );
        let host = Host::new(vec![serde_json::from_value(saved).unwrap()]);
        feed_position(&host, &[104.0]);
        assert!(host.fired().is_empty());
        stop.set(102.0);
        feed_position(&host, &[104.0]);
        assert!(matches!(
            host.engine.alerts()[0].condition,
            AlertCondition::Position {
                progress: AwaitingEntry,
                ..
            }
        ));
        assert!(host.fired().is_empty());
        feed_position(&host, &[104.0, 103.0]);
        assert!(host.fired().is_empty());
        feed_position(&host, &[104.0, 103.0, 102.0]);
        assert!(host.fired().is_empty());
        feed_position(&host, &[104.0, 103.0, 102.0, 100.0]);
        assert_eq!(host.fired(), [100.0, 100.0]);
        let mut again = host.engine.alerts().remove(0);
        assert_eq!(again.status, AlertStatus::Triggered);
        assert!(matches!(
            again.condition,
            AlertCondition::Position {
                progress: StopReached,
                ..
            }
        ));
        again.status = AlertStatus::Active;
        assert_eq!(host.save(again), Ok(()));
        assert!(matches!(
            host.engine.alerts()[0].condition,
            AlertCondition::Position {
                progress: AwaitingEntry,
                ..
            }
        ));
    }

    #[test]
    fn editing_a_reached_position_level_starts_the_alert_again() {
        use AlertPositionProgress::{AwaitingEntry, Entered, StopReached, TargetReached};
        type PositionRow = (
            [f64; 3],
            f64,
            Vec<f64>,
            AlertPositionProgress,
            [f64; 3],
            Vec<f64>,
            Vec<(&'static str, f64)>,
        );
        let entered = "Position entered";
        let target = "Position target reached";
        let mut rows: Vec<PositionRow> = Vec::new();
        for direction in [1.0, -1.0] {
            let price = |offset| 100.0 + direction * offset;
            let before = [price(0.0), price(10.0), price(-5.0)];
            for (reached, close) in [(Entered, 5.0), (TargetReached, 10.0), (StopReached, -5.0)] {
                for level in 0..3 {
                    let mut after = before;
                    after[level] = [price(3.0), price(20.0), price(-4.0)][level];
                    rows.push((
                        before,
                        price(-2.0),
                        vec![price(0.0), price(close)],
                        reached,
                        after,
                        vec![price(close), after[0], after[1]],
                        vec![(entered, after[0]), (target, after[1])],
                    ));
                }
            }
        }
        for (index, (before, history, closes, reached, after, reloaded, expected)) in
            rows.into_iter().enumerate()
        {
            let position = |ids: [&str; 3], progress| {
                alert(
                    AlertCondition::Position {
                        levels: Box::new(ids.map(|id| AlertSource::Drawing(id.into()))),
                        progress,
                    },
                    AlertFrequency::Once,
                )
            };
            let host = Host::new(vec![
                position(["p", "p#1", "p#2"], AwaitingEntry),
                position(["q", "q#1", "q#2"], Entered),
            ]);
            let levels = std::cell::Cell::new(before);
            let now = std::cell::Cell::new(1_000);
            let update = |host: &Host, bars: &[Bar]| {
                host.engine.update(
                    "SYM",
                    None,
                    Timeframe::Min1,
                    bars,
                    time(now.get()),
                    |source, _| {
                        let AlertSource::Drawing(id) = source else {
                            return None;
                        };
                        ["p", "p#1", "p#2"]
                            .iter()
                            .position(|name| *name == id)
                            .map(|n| levels.get()[n])
                            .or_else(|| {
                                ["q", "q#1", "q#2"]
                                    .iter()
                                    .position(|name| *name == id)
                                    .map(|n| [50.0, 1_000.0, 10.0][n])
                            })
                    },
                );
                now.set(now.get() + 1);
            };
            let mut bars = vec![bar(0, history, 1.0)];
            for close in closes {
                bars.push(bar(bars.len() as i64, close, 1.0));
                update(&host, &bars);
            }
            let p = host.engine.alerts().remove(0);
            let id = p.id.clone().unwrap();
            assert!(
                matches!(p.condition, AlertCondition::Position { progress, .. } if progress == reached)
            );
            assert!(host.events().iter().all(|event| !matches!(
                event, AlertEvent::TriggeredForSymbol { id: fired, .. } if *fired != id
            )));
            // A drawing still loading must not discard reached progress or its recorded levels.
            levels.set([after[0], after[1], f64::NAN]);
            update(&host, &bars);
            assert_eq!(host.engine.alerts()[0], p, "row {index}: unavailable stop");
            assert!(host.events().is_empty());
            levels.set(after);
            update(&host, &bars);
            let restarted = host.engine.alerts().remove(0);
            assert!(
                matches!(
                    restarted.condition,
                    AlertCondition::Position {
                        progress: AwaitingEntry,
                        ..
                    }
                ),
                "row {index}: edited levels restart the position"
            );
            assert_eq!(restarted.status, AlertStatus::Active, "row {index}");
            assert_eq!(restarted.last_triggered_at, p.last_triggered_at);
            assert_eq!(host.events(), vec![AlertEvent::Upsert(Box::new(restarted))]);

            let host = Host::new(
                serde_json::from_value(serde_json::to_value(host.engine.alerts()).unwrap())
                    .unwrap(),
            );
            for close in reloaded {
                bars.push(bar(bars.len() as i64, close, 1.0));
                update(&host, &bars);
            }
            let events = host.events();
            let fired: Vec<_> = events
                .iter()
                .filter_map(|event| match event {
                    AlertEvent::TriggeredForSymbol {
                        id: fired,
                        message,
                        price: Some(price),
                        ..
                    } => {
                        assert_eq!(*fired, id, "row {index}: unrelated position stays entered");
                        Some((message.as_str(), *price))
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(fired, expected, "row {index}");
            let saved = host.engine.alerts();
            let ended = if expected.last().unwrap().0 == target {
                TargetReached
            } else {
                StopReached
            };
            assert_eq!(saved[0].status, AlertStatus::Triggered);
            assert!(
                matches!(saved[0].condition, AlertCondition::Position { progress, .. } if progress == ended)
            );
            assert_eq!(saved[1].status, AlertStatus::Active);
            assert!(matches!(
                saved[1].condition,
                AlertCondition::Position {
                    progress: Entered,
                    ..
                }
            ));
            let host =
                Host::new(serde_json::from_value(serde_json::to_value(saved).unwrap()).unwrap());
            host.events();
            update(&host, &bars);
            assert!(
                host.events().is_empty(),
                "row {index}: unchanged restore stays ended"
            );
            let saved = host.engine.alerts().remove(0);
            assert_eq!(saved.status, AlertStatus::Triggered);
            assert!(
                matches!(saved.condition, AlertCondition::Position { progress, .. } if progress == ended)
            );
        }
    }

    /// An hourly alert fired at 10:05 in the 10:00 bar, was saved and is restored at 10:20. Each
    /// step is the first hour of an update, its closes, the update time and the closes that fire.
    #[test]
    fn a_restored_alert_counts_its_last_firing() {
        use AlertCondition::{Above, CrossingUp};
        use AlertFrequency::{EveryTime, OncePerBar, OncePerBarClose, OncePerMinute};
        let restored = |condition, frequency| {
            let mut saved = alert(condition, frequency);
            saved.timeframe = Some(Timeframe::Hour1);
            saved.id = Some(AlertId("1".into()));
            saved.last_triggered_at = Some(time(36_300));
            saved
        };
        let hourly = |first: i64, closes: &[f64]| -> Vec<Bar> {
            (first..)
                .zip(closes)
                .map(|(hour, &close)| Bar::new(time(hour * 3_600), close, close, close, close, 1.0))
                .collect()
        };
        let update = |host: &Host, symbol: &str, (first, closes): (i64, &[f64]), now: i64| {
            host.engine.update(
                symbol,
                None,
                Timeframe::Hour1,
                &hourly(first, closes),
                time(now),
                |_, _| None,
            );
        };
        let rows = [
            // No firing before the 11:00 bar.
            (
                Above(100.0),
                OncePerBar,
                vec![
                    (9, vec![99.0, 101.0], 37_200, vec![]),
                    (9, vec![99.0, 102.0], 38_400, vec![]),
                    (9, vec![99.0, 102.0, 103.0], 39_660, vec![103.0]),
                ],
            ),
            // The 09:00 bar fired when the 10:00 bar arrived; the 10:00 bar fires at its close.
            (
                Above(100.0),
                OncePerBarClose,
                vec![
                    (8, vec![99.0, 101.0, 101.0], 37_200, vec![]),
                    (8, vec![99.0, 101.0, 102.0, 103.0], 39_660, vec![102.0]),
                ],
            ),
            // Not again before 10:06.
            (
                Above(100.0),
                OncePerMinute,
                vec![
                    (9, vec![99.0, 101.0], 36_330, vec![]),
                    (9, vec![99.0, 102.0], 36_360, vec![102.0]),
                ],
            ),
            // The crossing that fired needs a new one.
            (
                CrossingUp(100.0),
                EveryTime,
                vec![
                    (9, vec![99.0, 101.0], 37_200, vec![]),
                    (9, vec![99.0, 99.0], 37_260, vec![]),
                    (9, vec![99.0, 101.0], 37_320, vec![101.0]),
                ],
            ),
            // In a later bar the alert evaluates as a new one.
            (
                Above(100.0),
                OncePerBar,
                vec![(10, vec![101.0, 101.0], 40_800, vec![101.0])],
            ),
            (
                CrossingUp(100.0),
                EveryTime,
                vec![(10, vec![99.0, 101.0], 40_800, vec![101.0])],
            ),
        ];
        for (row, (condition, frequency, steps)) in rows.into_iter().enumerate() {
            let host = Host::new(vec![restored(condition, frequency)]);
            for (step, (first, closes, now, fired)) in steps.into_iter().enumerate() {
                update(&host, "SYM", (first, &closes), now);
                assert_eq!(host.fired(), fired, "row {row}, step {step}");
            }
        }
        // A display-only save before the first evaluation keeps the saved last firing.
        for list in [false, true] {
            let mut saved = restored(Above(100.0), OncePerBar);
            if list {
                saved.symbol = String::new();
                saved.watchlist = Some("tech".into());
                saved.member_triggered_at.insert("SYM".into(), time(36_300));
            }
            let host = Host::new(vec![saved]);
            host.engine.set_watchlist("tech", vec!["SYM".into()]);
            let mut edited = host.engine.alerts().remove(0);
            edited.extend_line = false;
            assert_eq!(host.save(edited), Ok(()));
            update(&host, "SYM", (9, &[99.0, 101.0]), 37_200);
            assert!(
                host.fired().is_empty(),
                "display-only save, watchlist: {list}"
            );
        }
        // Changing what the alert watches starts it afresh.
        let host = Host::new(vec![restored(Above(100.0), OncePerBar)]);
        update(&host, "SYM", (9, &[99.0, 101.0]), 37_200);
        assert!(host.fired().is_empty());
        let mut edited = host.engine.alerts().remove(0);
        edited.condition = Above(100.5);
        assert_eq!(host.save(edited), Ok(()));
        update(&host, "SYM", (9, &[99.0, 101.0]), 37_260);
        assert_eq!(host.fired(), [101.0]);
        // A watchlist member counts its own firing, not one of another member.
        let mut list = restored(Above(100.0), OncePerBar);
        list.symbol = String::new();
        list.watchlist = Some("tech".into());
        list.member_triggered_at.insert("SYM".into(), time(36_300));
        let host = Host::new(vec![list]);
        host.engine
            .set_watchlist("tech", vec!["SYM".into(), "OTHER".into()]);
        for symbol in ["SYM", "OTHER"] {
            update(&host, symbol, (9, &[99.0, 101.0]), 37_200);
        }
        let symbols: Vec<_> = host
            .events()
            .into_iter()
            .filter_map(|event| match event {
                AlertEvent::TriggeredForSymbol { symbol, .. } => Some(symbol),
                _ => None,
            })
            .collect();
        assert_eq!(symbols, ["OTHER"]);
    }

    #[test]
    fn restored_footprint_alerts_count_each_symbols_last_firing() {
        use crate::model::FootprintCondition::{StackedBuyImbalance, StackedSellImbalance};
        use AlertFrequency::{OncePerBar, OncePerBarClose};

        for (frequency, last, open, at, covered) in [
            (OncePerBar, 65, 60, 70, true),
            (OncePerBar, 65, 120, 125, false),
            (OncePerBarClose, 120, 60, 120, true),
            (OncePerBarClose, 120, 60, 121, false),
            (OncePerBarClose, 120, 120, 180, false),
        ] {
            for list in [false, true] {
                for changed in [false, true] {
                    let mut saved =
                        alert(AlertCondition::Footprint(StackedBuyImbalance), frequency);
                    saved.id = Some(AlertId("1".into()));
                    saved.last_triggered_at = Some(time(last));
                    if list {
                        saved.symbol.clear();
                        saved.watchlist = Some("tech".into());
                        saved.member_triggered_at.insert("SYM".into(), time(last));
                        saved.last_triggered_at = Some(time(last + 60));
                    }
                    let host = Host::new(vec![saved]);
                    host.engine
                        .set_watchlist("tech", vec!["SYM".into(), "OTHER".into()]);
                    if changed {
                        // A monitoring edit starts this session's alert with no restored guard.
                        let mut edited = host.engine.alerts().remove(0);
                        edited.condition = AlertCondition::Footprint(StackedSellImbalance);
                        assert_eq!(host.save(edited), Ok(()));
                    }
                    let symbols: &[_] = if list { &["SYM", "OTHER"] } else { &["SYM"] };
                    for symbol in symbols {
                        let mut event = FootprintAlertEvent {
                            symbol: (*symbol).into(),
                            exchange: None,
                            timeframe: Timeframe::Min1,
                            condition: if changed {
                                StackedSellImbalance
                            } else {
                                StackedBuyImbalance
                            },
                            bar: time(open),
                            closed: frequency == OncePerBarClose,
                            time: time(at),
                            price: 100.0,
                            message: String::new(),
                        };
                        host.engine.footprint_events(&[event.clone()], time(at));
                        let expected = if !covered || *symbol == "OTHER" || changed {
                            vec![100.0]
                        } else {
                            vec![]
                        };
                        assert_eq!(
                            host.fired(),
                            expected,
                            "{frequency:?}, last {last}, open {open}, at {at}, list {list}, changed {changed}, {symbol}"
                        );
                        host.engine.footprint_events(&[event.clone()], time(at));
                        assert!(host.fired().is_empty(), "same bar stays silent");
                        event.bar = time(open + 60);
                        event.time = time(at + 60);
                        host.engine.footprint_events(&[event], time(at + 60));
                        assert_eq!(host.fired(), [100.0], "the next bar fires");
                    }
                }
            }
        }
    }

    /// A crossing follows the configured ray's stable slot through drawing and alert restore,
    /// including an older bar prepended while the drawing was stored.
    #[cfg(feature = "drawings")]
    #[test]
    fn an_alert_on_a_fan_ray_fires_on_a_crossing_and_keeps_its_ray_when_restored() {
        use crate::drawings::persistence::{drawing_to_stored, stored_to_drawing};
        use crate::drawings::{ChartPoint, Drawing, DrawingToolType, GannConfig, GannLevel};
        use crate::scales::PriceScaleMode;
        let mut drawing = Drawing::new(9, DrawingToolType::GannFan);
        drawing.stored_id = Some("fan".into());
        drawing.chart_points = vec![ChartPoint::new(0.0, 100.0), ChartPoint::new(4.0, 104.0)];
        let mut config = GannConfig::for_tool(DrawingToolType::GannFan);
        config.fan_ratios = [2.0, 1.0, 0.5].map(|v| GannLevel::new(v, [0; 4])).to_vec();
        config.fan_ratios[1].visible = false;
        drawing.gann = Some(config);
        assert_eq!(
            drawing.alert_source(2),
            AlertSource::Drawing("fan#2".into())
        );
        let resolve = |drawing: &Drawing, source: &AlertSource, bar: usize| {
            let level = drawing.alert_level(source)?;
            drawing
                .alert_levels(bar as f64, PriceScaleMode::Normal)
                .into_iter()
                .find(|(index, ..)| *index == level)
                .map(|(.., price)| price)
        };
        let host = Host::new(vec![alert(
            AlertCondition::Compare {
                other: drawing.alert_source(2),
                comparison: AlertComparison::Crossing,
            },
            AlertFrequency::EveryTime,
        )]);
        let bars: Vec<_> = [99.0, 100.0, 102.0, 103.0]
            .into_iter()
            .enumerate()
            .map(|(i, close)| bar(i as i64, close, 1.0))
            .collect();
        for n in 0..bars.len() {
            host.engine.update(
                "SYM",
                None,
                Timeframe::Min1,
                &bars[..=n],
                time(1_000 + n as i64),
                |source, bar| resolve(&drawing, source, bar),
            );
        }
        assert_eq!(host.fired(), [102.0]);
        let saved = serde_json::to_value(&host.engine.alerts()[0]).unwrap();
        let stored = drawing_to_stored(&drawing, |i| (i * 60_000.0) as i64);
        let restored = stored_to_drawing(&stored, |t| t as f32 / 60_000.0 + 1.0, 3);
        assert_eq!(
            restored.alert_source(2),
            AlertSource::Drawing("fan#2".into())
        );
        let host = Host::new(vec![serde_json::from_value(saved).unwrap()]);
        let bars: Vec<_> = std::iter::once(bar(-1, 50.0, 1.0))
            .chain(bars)
            .chain(std::iter::once(bar(4, 101.0, 1.0)))
            .collect();
        for n in [5, 6] {
            host.engine.update(
                "SYM",
                None,
                Timeframe::Min1,
                &bars[..n],
                time(1_100 + n as i64),
                |source, bar| resolve(&restored, source, bar),
            );
            assert_eq!(host.fired(), if n == 5 { vec![] } else { vec![101.0] });
        }
    }

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

    /// A saved alert compares only values it observed after the save: a crossing between the bar
    /// before and the forming bar at the save is history. Each row saves the alert at 00:01:10 with
    /// the 00:00 bar closed at `before` and feeds the forming 00:01 bar tick by tick.
    #[test]
    fn a_saved_alert_crosses_only_from_values_it_observed() {
        type SavedRow = (AlertCondition, f64, Vec<f64>, Vec<usize>);
        let (lower, upper) = (100.0, 110.0);
        let rows: Vec<SavedRow> = vec![
            (
                AlertCondition::CrossingUp(100.0),
                99.0,
                vec![105.0, 99.0, 101.0],
                vec![2],
            ),
            // The bell's default: crossing at the last close, the bar before on either side.
            (
                AlertCondition::Crossing(100.0),
                99.0,
                vec![100.0, 101.0, 99.0],
                vec![2],
            ),
            (
                AlertCondition::Crossing(100.0),
                101.0,
                vec![100.0, 99.0, 101.0],
                vec![2],
            ),
            (
                AlertCondition::EnteringChannel { lower, upper },
                95.0,
                vec![105.0, 111.0, 105.0],
                vec![2],
            ),
            (
                AlertCondition::ExitingChannel { lower, upper },
                105.0,
                vec![111.0, 105.0, 99.0],
                vec![2],
            ),
        ];
        for (row, (condition, before, ticks, expected)) in rows.into_iter().enumerate() {
            let host = Host::new(Vec::new());
            host.save(alert(condition, AlertFrequency::EveryTime))
                .unwrap();
            host.events();
            let mut fired = Vec::new();
            for (tick, close) in ticks.into_iter().enumerate() {
                feed(
                    &host,
                    &[bar(0, before, 1.0), bar(1, close, 1.0)],
                    &[],
                    70 + tick as i64,
                );
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

    /// Saved trigger history survives restoration and is pushed once, including silent entries,
    /// while the retained history stays bounded.
    #[test]
    fn restored_triggers_are_pushed_on_the_first_watch_only() {
        let mut watched = alert(AlertCondition::Above(100.0), AlertFrequency::EveryTime);
        watched.id = Some(AlertId("1".into()));
        watched.name = "Breakout".into();
        let host = Host::new(vec![watched]);
        host.events();
        let bars = [bar(0, 99.0, 1.0), bar(1, 101.0, 1.0), bar(2, 103.0, 1.0)];
        feed(&host, &bars, &[], 1_000);
        let mut fired = AlertTrigger::new(AlertId("1".into()), "SYM", time(1_000));
        fired.name = "Breakout".into();
        fired.price = Some(103.0);
        assert_eq!(host.engine.triggers(), [fired]);
        // The host keeps the history as text beside the alerts.
        let mut quiet = AlertTrigger::new(AlertId("1".into()), "SYM", time(2_000));
        quiet.silent = true;
        let mut saved = host.engine.triggers();
        saved.push(quiet);
        let saved: Vec<AlertTrigger> =
            serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap();
        let restored = AlertEngine::new()
            .with_alerts(host.engine.alerts())
            .with_triggers(saved.clone());
        let inbox = Arc::new(Inbox::default());
        let (_first, events) = Listener::new(&inbox, 1);
        restored.watch(events);
        let (_second, events) = Listener::new(&inbox, 2);
        restored.watch(events);
        let answers = inbox.drain().answers;
        let pushed = |key| {
            answers
                .iter()
                .filter(|(k, _)| *k == key)
                .map(|(_, event)| event.downcast_ref::<AlertEvent>().unwrap().clone())
                .collect::<Vec<_>>()
        };
        let upsert = AlertEvent::Upsert(Box::new(restored.alerts()[0].clone()));
        assert_eq!(
            pushed(1),
            [
                upsert.clone(),
                AlertEvent::Logged(saved[0].clone()),
                AlertEvent::Logged(saved[1].clone())
            ]
        );
        assert_eq!(pushed(2), [upsert]);
        // The history keeps the newest triggers.
        let many = (0..5_001)
            .map(|n| AlertTrigger::new(AlertId(n.to_string()), "SYM", time(n)))
            .collect();
        let kept = AlertEngine::new().with_triggers(many).triggers();
        assert_eq!(
            (kept.len(), kept[0].id.0.as_str(), kept[4_999].id.0.as_str()),
            (5_000, "1", "5000")
        );
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

    /// A reload replaces the alert subscription and its history without an error, backoff or
    /// permanent stop, and drops bars received in the same drain.
    #[cfg(feature = "ui")]
    #[test]
    fn an_alert_feed_reloads_when_its_feed_asks() {
        use crate::services::AlertFeed;
        #[derive(Default)]
        struct Feed(Mutex<Vec<crate::data::Sink>>);
        impl crate::data::Datafeed for Feed {
            fn history(
                &self,
                _: crate::data::HistoryRequest,
                reply: Reply<crate::data::HistoryPage>,
            ) {
                let bars = vec![bar(0, 10.0, 1.0), bar(1, 20.0, 1.0)];
                reply.send(Ok(crate::data::HistoryPage::new(bars, true)));
            }
            fn subscribe(&self, subscription: crate::data::Subscription) {
                self.0.lock().unwrap().push(subscription.sink);
            }
        }
        let ctx = egui::Context::default();
        let host = Arc::new(Feed::default());
        let mut feed = AlertFeed::new(
            host.clone(),
            AlertFeed::history_request("SYM", Timeframe::Min1),
            None,
        );
        for _ in 0..2 {
            assert!(feed.poll(&ctx, time(1000)).is_empty());
        }
        let loaded = [bar(0, 10.0, 1.0), bar(1, 20.0, 1.0)];
        assert_eq!(feed.bars(), loaded);
        let old = host.0.lock().unwrap()[0].clone();
        old.push_bar(bar(5, 50.0, 1.0));
        old.reload_history();
        assert!(feed.poll(&ctx, time(1000)).is_empty());
        assert!(!old.is_active());
        assert_eq!(host.0.lock().unwrap().len(), 2);
        assert!(feed.bars().is_empty());
        assert!(feed.poll(&ctx, time(1000)).is_empty());
        assert_eq!(feed.bars(), loaded);
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

        // A disabled alert is ignored; saving it active again records a first observation,
        // so the crossing from before the save does not fire.
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
        assert!(
            host.events().is_empty(),
            "resumed: crossing before the save"
        );
        feed(&host, &[bars[0].clone(), bar(1, 99.5, 1.0)], &[], 1_001);
        assert!(host.events().is_empty());
        feed(&host, &bars, &[], 1_001);
        // Once reports the new status before the trigger.
        let events = host.events();
        assert!(matches!(&events[0], AlertEvent::Upsert(a) if a.status == AlertStatus::Triggered));
        assert!(
            matches!(&events[1], AlertEvent::TriggeredForSymbol { symbol, time: t, price: Some(p), .. }
            if symbol == "SYM" && *t == time(1_001) && *p == 101.0)
        );

        for host_events in [false, true] {
            // A watchlist alert fires once for each member and ends when every member has fired.
            let condition = if host_events {
                AlertCondition::CustomEvents("Turns".into())
            } else {
                AlertCondition::Above(100.0)
            };
            let mut list = alert(condition, AlertFrequency::Once);
            list.symbol = String::new();
            list.watchlist = Some("tech".into());
            let members = || vec!["SYM".to_owned(), "OTHER".to_owned()];
            let fired_on = |host: &Host, symbols: &[&str]| -> Vec<String> {
                for symbol in symbols {
                    if host_events {
                        host.engine.custom_events(
                            "Turns",
                            symbol,
                            None,
                            Timeframe::Min1,
                            &[crate::model::CustomAlertEvent::new(1, time(1_000), "Turn")],
                            time(1_000),
                        );
                    } else {
                        host.engine.update(
                            symbol,
                            None,
                            Timeframe::Min1,
                            &bars,
                            time(1_000),
                            |_, _| None,
                        );
                    }
                }
                host.events()
                    .into_iter()
                    .filter_map(|event| match event {
                        AlertEvent::TriggeredForSymbol { symbol, .. } => Some(symbol),
                        _ => None,
                    })
                    .collect()
            };
            let host = Host::new(vec![list]);
            host.engine.set_watchlist("tech", members());
            host.events();
            assert_eq!(fired_on(&host, &["SYM", "SYM"]), ["SYM"]);
            let partial = host.engine.alerts().remove(0);
            assert_eq!(partial.status, AlertStatus::Active);
            assert_eq!(
                partial.member_triggered_at.keys().collect::<Vec<_>>(),
                ["SYM"]
            );
            // A member that leaves and returns keeps its saved completion.
            host.engine.set_watchlist("tech", vec!["OTHER".into()]);
            host.engine.set_watchlist("tech", members());
            assert!(fired_on(&host, &["SYM"]).is_empty());
            assert_eq!(fired_on(&host, &["OTHER", "NOT_A_MEMBER"]), ["OTHER"]);
            let ended = host.engine.alerts().remove(0);
            assert_eq!(ended.status, AlertStatus::Triggered);
            // A restored copy fires only for the members that had not fired, none once it ended.
            for (saved, expected) in [(partial.clone(), vec!["OTHER"]), (ended, vec![])] {
                let host = Host::new(vec![saved]);
                host.engine.set_watchlist("tech", members());
                host.events();
                assert_eq!(fired_on(&host, &["SYM", "OTHER"]), expected);
            }
            // A list left with only members that fired ends the alert; an empty list does not.
            let host = Host::new(vec![partial]);
            host.engine.set_watchlist("tech", Vec::new());
            assert_eq!(host.engine.alerts()[0].status, AlertStatus::Active);
            host.engine.set_watchlist("tech", vec!["SYM".into()]);
            assert_eq!(host.engine.alerts()[0].status, AlertStatus::Triggered);
        }

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
            // A new alert on bar 1 uses bar 0's bounds, not bar 1's bounds twice.
            let host = Host::new(vec![watching]);
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

    /// A five-minute bar that trades past a level or a channel boundary within one poll fires the
    /// alert once, and not again while it stays unchanged and only one-minute bars arrive, whether
    /// the host updates both intervals at once or one after the other.
    #[test]
    fn a_longer_interval_move_fires_once_however_often_the_shorter_one_updates() {
        let (lower, upper) = (100.0, 110.0);
        let (up_and_back, down_and_back) = ([105.0, 112.0, 105.5], [115.0, 108.0, 114.5]);
        // The five-minute bar opens at the first price, trades to the second and closes at the third.
        let rows = [
            (AlertCondition::CrossingUp(upper), [105.0, 112.0, 109.0]),
            (AlertCondition::CrossingUp(upper), down_and_back),
            (AlertCondition::CrossingDown(upper), up_and_back),
            (
                AlertCondition::EnteringChannel { lower, upper },
                up_and_back,
            ),
            (AlertCondition::Above(upper), [105.0, 112.0, 109.0]),
            (AlertCondition::OutsideChannel { lower, upper }, up_and_back),
        ];
        let (five, one) = (
            BarClock::new(Timeframe::Min5),
            BarClock::new(Timeframe::Min1),
        );
        for (row, (condition, [open, extreme, close])) in rows.into_iter().enumerate() {
            for together in [true, false] {
                let mut watching = alert(condition.clone(), AlertFrequency::EveryTime);
                watching.timeframe = Some(Timeframe::Min5);
                watching
                    .additional_conditions
                    .push(crate::model::AlertRule::new(
                        AlertSource::Price,
                        AlertCondition::Above(50.0),
                    ));
                watching.additional_timeframes.push(Some(Timeframe::Min1));
                let host = Host::new(Vec::new());
                host.save(watching).unwrap();
                host.events();
                let poll = |slow: &[Bar], fast: &[Bar], now| {
                    if together {
                        host.engine.update_matching(
                            None,
                            "SYM",
                            None,
                            &[(&five, slow), (&one, fast)],
                            time(now),
                            |_, _, _| None,
                        );
                    } else {
                        for (timeframe, bars) in [(Timeframe::Min1, fast), (Timeframe::Min5, slow)]
                        {
                            host.engine
                                .update("SYM", None, timeframe, bars, time(now), |_, _| None);
                        }
                    }
                    host.fired().len()
                };
                let older = bar(-5, open, 10.0);
                let mut fired = vec![poll(
                    &[older.clone(), bar(0, open, 10.0)],
                    &[bar(1, open, 5.0), bar(2, open, 5.0)],
                    150,
                )];
                let (high, low) = (open.max(extreme), open.min(extreme));
                let slow = [older, Bar::new(time(0), open, high, low, close, 11.0)];
                let mut fast = vec![bar(1, open, 5.0), bar(2, close, 6.0)];
                fired.push(poll(&slow, &fast, 160));
                for minute in [3, 4] {
                    fast.push(bar(minute, close, 1.0));
                    fired.push(poll(&slow, &fast, minute * 60));
                }
                assert_eq!(fired, [0, 1, 0, 0], "row {row}, together {together}");
            }
        }
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

    /// The trigger symbol, time, price and message of each firing pushed since the last call.
    fn triggers(host: &Host) -> Vec<(String, DateTime<Utc>, Option<f64>, String)> {
        host.events()
            .into_iter()
            .filter_map(|event| match event {
                AlertEvent::TriggeredForSymbol {
                    symbol,
                    time,
                    price,
                    message,
                    ..
                } => Some((symbol, time, price, message)),
                _ => None,
            })
            .collect()
    }

    /// A detector alert fires once for each new formation or target of its detector on its
    /// symbol, at the signal's anchor; history, other detectors, other symbols and a second chart
    /// whose bars end earlier never fire it.
    #[test]
    fn detector_alerts_fire_once_per_new_signal_of_their_detector() {
        use crate::model::{Direction, Signal, SignalId, SignalShape};
        let detector = AlertCondition::DetectorSignals("Big trades".into());
        let host = Host::new(vec![alert(detector, AlertFrequency::EveryTime)]);
        host.events();
        let marker = |id: &str, category: &str, at, price| {
            let shape = SignalShape::Marker {
                time: time(at),
                price,
                direction: Direction::Up,
            };
            let mut signal = Signal::new(SignalId(id.into()), shape, category);
            signal.tooltip = Some(format!("Buy {id}"));
            signal
        };
        let level = |id: &str, from| {
            let shape = SignalShape::Level { price: 105.0, from };
            Signal::new(SignalId(id.into()), shape, "Big trades")
        };
        let signals = [
            marker("old", "Big trades", 50, Some(99.0)),
            marker("new", "Big trades", 120, Some(101.0)),
            marker("other", "Icebergs", 130, Some(102.0)),
            level("target", Some(time(125))),
            level("anywhere", None),
        ];
        let deliver = |symbol, from, now| {
            host.engine.detector_signals(
                symbol,
                None,
                Timeframe::Min1,
                &signals,
                time(from),
                time(now),
            )
        };
        deliver("SYM", 60, 200);
        deliver("OTHER", 60, 201);
        deliver("SYM", 60, 202);
        // A chart with a newer bar, then one whose bars end earlier, such as a chart of the
        // regular session beside one of the extended session.
        deliver("SYM", 150, 203);
        deliver("SYM", 60, 204);
        let at = |seconds, price, message: &str| {
            ("SYM".to_owned(), time(seconds), price, message.to_owned())
        };
        assert_eq!(
            triggers(&host),
            [
                at(120, Some(101.0), "Big trades: Buy new"),
                at(125, Some(105.0), "Big trades"),
            ]
        );
    }

    /// A custom event alert is saved only once the host connects its source, and fires once per
    /// event of that source with the event's time, price and message.
    #[test]
    fn custom_event_alerts_need_a_connected_source_and_fire_once_per_event() {
        use crate::model::CustomAlertEvent;
        let host = Host::new(Vec::new());
        let condition = AlertCondition::CustomEvents("Turns".into());
        let mut turns = alert(condition, AlertFrequency::EveryTime);
        assert_eq!(
            host.save(turns.clone()),
            Err(FeedError::Other(
                "No event route connects the custom event source Turns".into()
            ))
        );
        turns.status = AlertStatus::Disabled;
        assert_eq!(host.save(turns), Ok(()), "a paused alert needs no route");
        let mut turns = host.engine.alerts()[0].clone();
        turns.status = AlertStatus::Active;
        host.engine.connect_custom_events("Turns");
        assert_eq!(host.save(turns), Ok(()));
        host.events();
        let event = |sequence, at, price| {
            let mut event = CustomAlertEvent::new(sequence, time(at), format!("Turn {sequence}"));
            event.price = price;
            event
        };
        let deliver = |source, events: &[CustomAlertEvent]| {
            host.engine
                .custom_events(source, "SYM", None, Timeframe::Min1, events, time(500))
        };
        deliver("Turns", &[event(1, 100, Some(101.0)), event(2, 160, None)]);
        deliver("Turns", &[event(2, 160, None)]);
        deliver("Other", &[event(3, 220, Some(1.0))]);
        let at = |seconds, price, message: &str| {
            ("SYM".to_owned(), time(seconds), price, message.to_owned())
        };
        assert_eq!(
            triggers(&host),
            [at(100, Some(101.0), "Turn 1"), at(160, None, "Turn 2")]
        );
        let kept: Vec<_> = host
            .engine
            .triggers()
            .into_iter()
            .map(|t| (t.symbol, t.time, t.price, t.message))
            .collect();
        assert_eq!(
            kept,
            [at(100, Some(101.0), "Turn 1"), at(160, None, "Turn 2")]
        );
        // Paused while other alerts evaluate bars, then resumed, it fires no event again.
        let mut turns = host.engine.alerts()[0].clone();
        for status in [AlertStatus::Disabled, AlertStatus::Active] {
            turns.status = status;
            assert_eq!(host.save(turns.clone()), Ok(()));
            host.engine
                .update("SYM", None, Timeframe::Min1, &[], time(510), |_, _| None);
        }
        deliver("Turns", &[event(2, 160, None)]);
        assert_eq!(triggers(&host), []);
    }

    /// A price level or channel boundary reached and left between two evaluations fires from the
    /// high or low the bar recorded, but a high or low the previous evaluation saw, or one made
    /// before the alert was saved, does not. Alerts at bar close, study sources, study partners and
    /// study boundaries read the close.
    #[test]
    fn price_levels_reached_between_evaluations_fire_from_the_traded_range() {
        type RangeRow = (
            AlertCondition,
            AlertSource,
            AlertFrequency,
            bool,
            Vec<Vec<Bar>>,
            Vec<usize>,
        );
        fn wick(minute: i64, high: f64, low: f64, close: f64) -> Bar {
            Bar::new(time(minute * 60), close, high, low, close, 1.0)
        }
        // The forming bar flat at `start`, then with a wick whose close comes back to `start`
        // with the same volume, then with no new high or low and its close at `last`.
        let same_bar = |start, high, low, last| {
            [
                wick(1, start, start, start),
                wick(1, high, low, start),
                wick(1, high, low, last),
            ]
            .map(|forming| vec![bar(0, start, 1.0), forming])
            .to_vec()
        };
        let study = AlertSource::Study {
            id: "s".into(),
            output: 0,
        };
        let compare = |other, comparison| AlertCondition::Compare { other, comparison };
        let (price, every) = (AlertSource::Price, AlertFrequency::EveryTime);
        let up = AlertCondition::CrossingUp(100.0);
        let skipped = vec![
            vec![bar(0, 99.0, 1.0), bar(1, 99.0, 1.0)],
            vec![
                bar(0, 99.0, 1.0),
                wick(1, 101.0, 99.0, 99.0),
                bar(2, 99.0, 1.0),
            ],
        ];
        let mut rows: Vec<RangeRow> = vec![
            (
                up.clone(),
                price.clone(),
                every,
                false,
                same_bar(99.0, 101.0, 99.0, 99.5),
                vec![1],
            ),
            (
                AlertCondition::CrossingDown(100.0),
                price.clone(),
                every,
                false,
                same_bar(101.0, 101.0, 99.0, 100.5),
                vec![1],
            ),
            (
                AlertCondition::Crossing(100.0),
                price.clone(),
                every,
                false,
                same_bar(99.0, 101.0, 99.0, 99.5),
                vec![1],
            ),
            (
                AlertCondition::Above(100.0),
                price.clone(),
                every,
                false,
                same_bar(99.0, 101.0, 99.0, 99.5),
                vec![1],
            ),
            (
                AlertCondition::Below(100.0),
                price.clone(),
                every,
                false,
                same_bar(101.0, 101.0, 99.0, 100.5),
                vec![1],
            ),
            // A drawing's level meets the range; a study's value is known only at the close.
            (
                compare(
                    AlertSource::Drawing("line".into()),
                    AlertComparison::CrossingUp,
                ),
                price.clone(),
                every,
                false,
                same_bar(99.0, 101.0, 99.0, 99.5),
                vec![1],
            ),
            (
                compare(study.clone(), AlertComparison::CrossingUp),
                price.clone(),
                every,
                false,
                same_bar(99.0, 101.0, 99.0, 99.0),
                vec![],
            ),
            (
                up.clone(),
                study,
                every,
                false,
                same_bar(99.0, 101.0, 99.0, 99.0),
                vec![],
            ),
            // A poll gap across a bar boundary still reads the wick of the bar left behind.
            (
                up.clone(),
                price.clone(),
                every,
                false,
                skipped.clone(),
                vec![1],
            ),
            (
                up.clone(),
                price.clone(),
                AlertFrequency::OncePerBarClose,
                false,
                skipped,
                vec![],
            ),
            // The high made before the save never fires; only a later, higher one does.
            (
                up,
                price,
                every,
                true,
                [(101.0, 99.0), (101.0, 99.5), (102.0, 99.0)]
                    .map(|(high, close)| vec![bar(0, 99.0, 1.0), wick(1, high, 99.0, close)])
                    .to_vec(),
                vec![2],
            ),
        ];
        // A wick across a channel boundary with the close back where it started, on a fixed
        // channel from 100 to 110 and on one between drawings at 100 and 110.
        let drawn = |comparison| AlertCondition::Channel {
            boundaries: Box::new(["line", "upper"].map(|id| AlertSource::Drawing(id.into()))),
            comparison,
        };
        let (lower, upper) = (100.0, 110.0);
        let (out, back_in) = (
            same_bar(105.0, 112.0, 105.0, 105.5),
            same_bar(95.0, 103.0, 95.0, 95.5),
        );
        for (fixed, comparison, updates) in [
            (
                AlertCondition::EnteringChannel { lower, upper },
                AlertChannelComparison::Entering,
                &back_in,
            ),
            (
                AlertCondition::InsideChannel { lower, upper },
                AlertChannelComparison::Inside,
                &back_in,
            ),
            (
                AlertCondition::ExitingChannel { lower, upper },
                AlertChannelComparison::Exiting,
                &out,
            ),
            (
                AlertCondition::OutsideChannel { lower, upper },
                AlertChannelComparison::Outside,
                &out,
            ),
        ] {
            for condition in [fixed, drawn(comparison)] {
                rows.push((
                    condition,
                    AlertSource::Price,
                    AlertFrequency::EveryTime,
                    false,
                    updates.clone(),
                    vec![1],
                ));
            }
        }
        // A study boundary is known only at the close.
        rows.push((
            AlertCondition::Channel {
                boundaries: Box::new([
                    AlertSource::Study {
                        id: "s".into(),
                        output: 0,
                    },
                    AlertSource::Drawing("upper".into()),
                ]),
                comparison: AlertChannelComparison::Exiting,
            },
            AlertSource::Price,
            AlertFrequency::EveryTime,
            false,
            out,
            vec![],
        ));
        let value = |source: &AlertSource, _| match source {
            AlertSource::Drawing(id) if id == "upper" => Some(110.0),
            AlertSource::Drawing(_) => Some(100.0),
            AlertSource::Study { .. } => Some(99.5),
            _ => None,
        };
        for (row, (condition, source, frequency, saved, updates, expected)) in
            rows.into_iter().enumerate()
        {
            let mut alert = alert(condition, frequency);
            alert.source = source;
            let host = if saved {
                let host = Host::new(Vec::new());
                host.save(alert).unwrap();
                host
            } else {
                Host::new(vec![alert])
            };
            host.events();
            let mut fired = Vec::new();
            for (update, bars) in updates.iter().enumerate() {
                host.engine.update(
                    "SYM",
                    None,
                    Timeframe::Min1,
                    bars,
                    time(170 + update as i64),
                    value,
                );
                if !host.fired().is_empty() {
                    fired.push(update);
                }
            }
            assert_eq!(fired, expected, "row {row}");
        }
    }

    /// A move past a level or a channel boundary and back fires the transition of its way back
    /// alike whether the alert evaluates once over the move or after each trade, on a fixed
    /// level or channel, a drawing level and a channel between two drawings. So does a move
    /// through the whole channel, which enters it and exits it, while a touch of the level and
    /// back is no crossing the other way.
    #[test]
    fn a_move_and_its_way_back_fire_alike_at_any_evaluation_rate() {
        let (lower, upper) = (100.0, 110.0);
        let drawn = |comparison| AlertCondition::Channel {
            boundaries: Box::new(["line", "upper"].map(|id| AlertSource::Drawing(id.into()))),
            comparison,
        };
        let level = |comparison| AlertCondition::Compare {
            other: AlertSource::Drawing("upper".into()),
            comparison,
        };
        // The trades of the forming bar, from the one the alert first observes.
        let (out_and_back, in_and_back): (&[f64], &[f64]) =
            (&[105.0, 112.0, 105.5], &[95.0, 103.0, 95.5]);
        let down_and_back: &[f64] = &[115.0, 108.0, 114.5];
        // Each row fires this many times at either rate.
        let rows: [(AlertCondition, &[f64], usize); 12] = [
            (
                AlertCondition::EnteringChannel { lower, upper },
                out_and_back,
                1,
            ),
            (drawn(AlertChannelComparison::Entering), out_and_back, 1),
            (
                AlertCondition::ExitingChannel { lower, upper },
                in_and_back,
                1,
            ),
            (drawn(AlertChannelComparison::Exiting), in_and_back, 1),
            (AlertCondition::CrossingDown(upper), out_and_back, 1),
            (level(AlertComparison::CrossingDown), out_and_back, 1),
            (AlertCondition::CrossingUp(upper), down_and_back, 1),
            (level(AlertComparison::CrossingUp), down_and_back, 1),
            // From the level itself, only the way back crosses it.
            (AlertCondition::Crossing(upper), &[upper, 112.0, upper], 1),
            // A touch of the level from below and back is no crossing down.
            (
                AlertCondition::CrossingDown(upper),
                &[105.0, upper, 105.5],
                0,
            ),
            // From inside, out below and up through the channel; from below, up through it and
            // back in from above.
            (
                AlertCondition::EnteringChannel { lower, upper },
                &[105.0, 95.0, 103.0, 115.0],
                1,
            ),
            (
                AlertCondition::ExitingChannel { lower, upper },
                &[98.0, 95.0, 105.0, 115.0, 105.0],
                1,
            ),
        ];
        let value = |source: &AlertSource, _| match source {
            AlertSource::Drawing(id) if id == "upper" => Some(upper),
            AlertSource::Drawing(_) => Some(lower),
            _ => None,
        };
        for (row, (condition, trades, fires)) in rows.into_iter().enumerate() {
            let fired = [true, false].map(|each_trade| {
                let host = Host::new(Vec::new());
                host.save(alert(condition.clone(), AlertFrequency::EveryTime))
                    .unwrap();
                host.events();
                let mut forming =
                    Bar::new(time(60), trades[0], trades[0], trades[0], trades[0], 1.0);
                let mut fired = 0;
                for (n, &price) in trades.iter().enumerate() {
                    forming.high = forming.high.max(price);
                    forming.low = forming.low.min(price);
                    forming.close = price;
                    if n == 0 || each_trade || n == trades.len() - 1 {
                        host.engine.update(
                            "SYM",
                            None,
                            Timeframe::Min1,
                            &[bar(0, trades[0], 1.0), forming.clone()],
                            time(70 + n as i64),
                            value,
                        );
                        fired += host.fired().len();
                    }
                }
                fired
            });
            assert_eq!(fired, [fires; 2], "row {row}: after each trade, once");
        }
    }
}
