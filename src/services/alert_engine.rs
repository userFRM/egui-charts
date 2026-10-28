//! An alert service that evaluates alerts on the bars the host feeds it.

use std::collections::HashMap;
use std::sync::Mutex;

use chrono::{DateTime, Utc};

use super::{AlertEvent, Alerts, Events};
use crate::data::{FeedError, Reply};
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
/// equal those of the previous update does nothing, so repeated frames never fire. An update
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
    /// Open time of the bar the alert last fired on.
    fired: Option<DateTime<Utc>>,
    /// A watchlist alert with frequency once has fired on this symbol.
    done: bool,
}

/// The inputs of one evaluation: the bar, its close and volume, and for each rule the value of
/// its source and of the other output it compares with.
#[derive(Clone, PartialEq)]
struct Seen {
    time: DateTime<Utc>,
    bar: (f64, f64),
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
        }
    }

    /// Starts with `alerts` as [`alerts`](Self::alerts) returned them, giving those without an
    /// id one.
    pub fn with_alerts(self, alerts: Vec<Alert>) -> Self {
        {
            let mut state = self.lock();
            for alert in alerts {
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
        let mut state = self.lock();
        state.expire(now);
        let State {
            alerts,
            events,
            watchlists,
            runs,
            ..
        } = &mut *state;
        for alert in alerts
            .iter_mut()
            .filter(|alert| alert.status == AlertStatus::Active && alert.is_valid())
        {
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
            let Some(bar) = evaluate(alert, run, bars, &value) else {
                continue;
            };
            let price = bar.close.is_finite().then_some(bar.close);
            if alert.frequency == AlertFrequency::Once {
                run.done = true;
                if alert.watchlist.is_none() {
                    alert.status = AlertStatus::Triggered;
                }
            }
            run.fired = Some(bar.time);
            if let Some(events) = events {
                if alert.status != AlertStatus::Active {
                    events.push(AlertEvent::Upsert(alert.clone()));
                }
                events.push(AlertEvent::TriggeredForSymbol {
                    id,
                    symbol: symbol.to_owned(),
                    time: now,
                    price,
                });
            }
        }
        runs.retain(|(id, _), _| {
            alerts
                .iter()
                .any(|a| a.id.as_ref() == Some(id) && a.status == AlertStatus::Active)
        });
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

/// Evaluates `alert` on the newest of `bars`, updating what `run` remembers. Returns the bar it
/// fired on, if it did.
fn evaluate<'a>(
    alert: &Alert,
    run: &mut Run,
    bars: &'a [Bar],
    value: &impl Fn(&AlertSource, usize) -> Option<f64>,
) -> Option<&'a Bar> {
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
        bar: (bar.close, bar.volume),
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
    let holds = rules.iter().enumerate().all(|(n, (src, condition))| {
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
            AlertFrequency::OncePerBar | AlertFrequency::OncePerBarClose => {
                run.fired != Some(bar.time)
            }
        };
    fires.then_some(bar)
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
    }

    impl Host {
        fn new(alerts: Vec<Alert>) -> Self {
            let inbox = Arc::new(Inbox::default());
            let (_listener, events) = Listener::new(&inbox, 1);
            let engine = AlertEngine::new().with_alerts(alerts);
            engine.watch(events);
            Self {
                engine,
                inbox,
                _listener,
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
    fn a_trigger_follows_the_change_of_status_and_reloads_never_repeat_it() {
        let mut watching = alert(AlertCondition::Above(100.0), AlertFrequency::EveryTime);
        watching.timeframe = None;
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
    }
}
