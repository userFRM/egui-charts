//! The mapped feed: a profile served to the chart through the library's `Datafeed` trait.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use chrono::{DateTime, Months, NaiveDate, NaiveTime, TimeDelta, Utc};
use egui_charts::data::{
    Datafeed, FeedCapabilities, FeedError, FeedStatus, HistoryPage, HistoryRequest, Reply, Sink,
    Subscription, SubscriptionId,
};
use egui_charts::model::{Bar, BarClock, Instrument, Timeframe, TimezoneId};
use serde_json::Value;

use super::bars::{self, Aggregator, DAY_MS};
use super::http::{self, Request};
use super::profile::{History, Profile, RangeFormat, RouteKind, fill, fill_url};
use super::stream::{Routed, Session, StreamEvent};
use super::{Failure, Secrets, conceal, endpoint, shape, time};

/// The feed of one profile. It answers history from the REST template and keeps one stream
/// session per subscription. One instance serves every chart of the own-data mode.
pub(crate) struct MappedFeed {
    shared: Arc<Shared>,
}

/// What the feed's callbacks reach. They hold it weakly: a session lives inside it.
struct Shared {
    profile: Profile,
    secrets: Secrets,
    /// Everything that changes. It is never held while the feed calls out: not across a
    /// method of a sink or a reply, a request, or the opening or the drop of a session.
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    subscriptions: HashMap<SubscriptionId, Live>,
    /// How the streams of each key join its history.
    joins: Vec<(Key, Join)>,
    /// The run of empty windows of each symbol and interval.
    runs: HashMap<(String, Timeframe), Run>,
}

/// The bars that belong together: those of one symbol and interval on one clock. The clock is
/// part of it because a bar of a day or longer is stamped for one clock.
#[derive(Clone, PartialEq)]
struct Key {
    symbol: String,
    timeframe: Timeframe,
    clock: BarClock,
}

/// What the latest pages of a key tell its subscriptions.
#[derive(Default)]
struct Join {
    /// The newest bar of the last latest page that held bars.
    newest: Option<Bar>,
    /// Latest pages being fetched. The subscriptions of the key fold no trade meanwhile.
    in_flight: usize,
}

/// A run of consecutive empty windows.
#[derive(Clone, Copy)]
struct Run {
    /// The continuation named last. A request that asks before it continues the run.
    next: DateTime<Utc>,
    /// The empty windows so far.
    windows: u32,
    /// The end of the first window.
    began: DateTime<Utc>,
}

/// One subscription and its stream.
struct Live {
    sink: Sink,
    key: Key,
    /// The address of the stream without its query, for errors and log lines.
    endpoint: String,
    aggregator: Aggregator,
    /// The slot of the newest bar pushed from a bar route.
    pushed: Option<DateTime<Utc>>,
    /// How often the socket has opened.
    opens: u64,
    /// The open count of the gap fill in flight. The subscription folds no trade meanwhile.
    filling: Option<u64>,
    /// Whether a record or a message that cannot be read has been logged.
    warned: bool,
    /// `None` until `Session::open` has returned.
    session: Option<Session>,
}

/// The bars a request asks for: those of `symbol` and `timeframe` that open in
/// `[since, before)` on the slots of `clock`, the newest `count`.
struct Ask {
    symbol: String,
    timeframe: Timeframe,
    clock: BarClock,
    before: Option<DateTime<Utc>>,
    since: Option<DateTime<Utc>>,
    count: usize,
}

/// One request to the service: what its template is filled with and where it leads.
struct Window {
    /// What `{from}`, `{to}` and `{limit}` become.
    from: String,
    to: String,
    limit: usize,
    /// The first instant after the window.
    end: DateTime<Utc>,
    /// The open of the first slot of the window. Older bars continue before it.
    start: DateTime<Utc>,
    /// Whether the template names `{from}`, so that a window without bars is one window and
    /// not everything the service has.
    lower: bool,
    /// Whether the window is whole days of an interval shorter than a day. The service may
    /// then fill its limit from the end of the day, with rows that open after the window.
    days: bool,
}

/// What a response holds for an [`Ask`].
struct Kept {
    /// The bars asked for, oldest first.
    bars: Vec<Bar>,
    /// Whether rows open before the slot that holds `since`: older bars exist.
    older: bool,
    /// Whether rows open at or after `before`.
    newer: bool,
}

/// A run of empty windows ends history with this many windows.
const LONGEST_RUN: u32 = 64;

/// A run of empty windows also ends history with the first window that starts more than this
/// many days before the end of the run's first window.
const FARTHEST_RUN_DAYS: i64 = 366;

impl MappedFeed {
    /// A feed that reads `profile` with the key values `secrets`. Nothing is sent until a chart
    /// asks.
    pub(crate) fn new(profile: Profile, secrets: Secrets) -> Self {
        Self {
            shared: Arc::new(Shared {
                profile,
                secrets,
                state: Mutex::default(),
            }),
        }
    }
}

impl Datafeed for MappedFeed {
    fn capabilities(&self, reply: Reply<FeedCapabilities>) {
        let timeframes = self.shared.profile.intervals.iter().map(|row| row.chart);
        reply.send(Ok(FeedCapabilities::new().with_timeframes(timeframes)));
    }

    fn resolve(&self, symbol: &str, reply: Reply<Instrument>) {
        // Any symbol resolves. One the service does not know fails there, with its status.
        let timeframes = self.shared.profile.intervals.iter().map(|row| row.chart);
        let instrument = Instrument::new(symbol).with_supported_timeframes(timeframes);
        // The footprint and the volume profiles group trades by the price step.
        let instrument = match self.shared.profile.tick_size {
            Some(step) => instrument.with_tick_size(step),
            None => instrument,
        };
        reply.send(Ok(instrument));
    }

    fn history(&self, request: HistoryRequest, reply: Reply<HistoryPage>) {
        let shared = &self.shared;
        shared.sweep();
        let end_of_history = || Ok(HistoryPage::new(Vec::new(), true));
        let Some(interval) = service(&shared.profile, request.timeframe) else {
            return reply.send(Err(FeedError::NotSupported));
        };
        let Some(history) = &shared.profile.history else {
            return reply.send(end_of_history());
        };
        let ask = Ask {
            symbol: request.symbol,
            timeframe: request.timeframe,
            clock: request.clock,
            before: request.before,
            since: request.since,
            count: request.count,
        };
        if ask.before.is_some() && !names(history, "{from}") && !names(history, "{to}") {
            // The service can only send its newest bars.
            return reply.send(end_of_history());
        }
        let runs = shared.state().runs.get(&ask.run()).copied();
        let run = runs.filter(|run| Some(run.next) == ask.before);
        // A stretch that has shown itself sparse is crossed in the widest windows.
        let wanted = match run {
            Some(_) => history.page_size,
            None => ask.count.min(history.page_size),
        };
        let now = Utc::now();
        let Some(window) = window(history, &shared.profile.time_zone, &ask, wanted, now) else {
            return reply.send(end_of_history());
        };
        let call = request_for(history, &window, &ask.symbol, interval, &shared.secrets);
        let url = conceal(&call.url, &shared.secrets);
        if ask.before.is_none() {
            shared.state().join(&ask.key()).in_flight += 1;
        }
        let weak = Arc::downgrade(shared);
        http::fetch(call, move |result| {
            // A feed that is gone has no chart left to answer.
            if let Some(shared) = weak.upgrade()
                && let Some(history) = &shared.profile.history
            {
                reply.send(shared.answer(history, &ask, &window, run, &url, result));
            }
        });
    }

    fn subscribe(&self, subscription: Subscription) {
        let shared = &self.shared;
        shared.sweep();
        let Subscription {
            id,
            symbol,
            timeframe,
            clock,
            sink,
            ..
        } = subscription;
        let (Some(stream), Some(interval)) =
            (&shared.profile.stream, service(&shared.profile, timeframe))
        else {
            return;
        };
        sink.set_status(FeedStatus::Connecting);
        let mut values = vec![
            ("symbol".to_owned(), symbol.clone()),
            ("interval".to_owned(), interval.to_owned()),
        ];
        values.extend(shared.secrets.iter().cloned());
        let filled: Vec<(&str, &str)> = values
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
            .collect();
        let endpoint = conceal(endpoint(&fill_url(&stream.url, &filled)), &shared.secrets);
        let key = Key {
            symbol,
            timeframe,
            clock,
        };
        {
            let mut state = shared.state();
            let mut aggregator = Aggregator::new(key.clock.clone(), timeframe);
            let join = state.join(&key);
            if join.in_flight == 0 {
                aggregator.start(join.newest.clone());
            }
            let live = Live {
                sink,
                key,
                endpoint,
                aggregator,
                pushed: None,
                opens: 0,
                filling: None,
                warned: false,
                session: None,
            };
            state.subscriptions.insert(id, live);
        }
        let weak = Arc::downgrade(shared);
        let session = Session::open(stream.clone(), values, move |event| {
            if let Some(shared) = weak.upgrade() {
                shared.on_event(id, event);
            }
        });
        // The session of a subscription that ended meanwhile closes at the end of this call,
        // after the lock is released.
        let _ended = {
            let mut state = shared.state();
            match state.subscriptions.get_mut(&id) {
                Some(live) => live.session.replace(session),
                None => Some(session),
            }
        };
    }

    fn unsubscribe(&self, id: SubscriptionId) {
        // The session closes at the end of this call, after the lock is released, and the
        // close does not wait.
        let _ended = self.shared.state().subscriptions.remove(&id);
    }
}

impl Shared {
    /// The state, also when a thread panicked while it held the lock.
    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Ends the subscriptions whose owner went away without unsubscribing.
    fn sweep(&self) {
        let sinks: Vec<(SubscriptionId, Sink)> = {
            let state = self.state();
            let held = state.subscriptions.iter();
            held.map(|(id, live)| (*id, live.sink.clone())).collect()
        };
        let ended: Vec<SubscriptionId> = sinks
            .into_iter()
            .filter(|(_, sink)| !sink.is_active())
            .map(|(id, _)| id)
            .collect();
        if ended.is_empty() {
            return;
        }
        // Their sessions close at the end of this call, after the lock is released.
        let _ended: Vec<Option<Live>> = {
            let mut state = self.state();
            let ended = ended.iter();
            ended.map(|id| state.subscriptions.remove(id)).collect()
        };
    }

    /// The page that answers `ask` with the response to the request for `window`. `run` is the
    /// run of empty windows the request continues.
    fn answer(
        &self,
        history: &History,
        ask: &Ask,
        window: &Window,
        run: Option<Run>,
        url: &str,
        result: Result<String, Failure>,
    ) -> Result<HistoryPage, FeedError> {
        let kept = result.and_then(|body| self.kept(history, ask, url, &body));
        let mut state = self.state();
        if ask.before.is_none() {
            // A latest page. The streams of its key go on from its newest bar, which the chart
            // gets with the page.
            let key = ask.key();
            let newest = kept.as_ref().ok().and_then(|kept| kept.bars.last());
            let join = state.join(&key);
            join.in_flight = join.in_flight.saturating_sub(1);
            if let Some(newest) = newest {
                join.newest = Some(newest.clone());
                let joining = state.subscriptions.values_mut();
                for live in joining.filter(|live| live.key == key) {
                    live.aggregator.start(Some(newest.clone()));
                }
            }
        }
        let kept = kept.map_err(|failure| reported(&failure, url, &ask.symbol, &self.secrets))?;
        if !kept.bars.is_empty() {
            state.runs.remove(&ask.run());
            // A page shorter than asked never ends history: the empty page after it does.
            return Ok(HistoryPage::new(kept.bars, false));
        }
        if let Some(floor) = ask.floor().filter(|_| kept.older) {
            return Ok(HistoryPage::default().with_continue_before(floor));
        }
        if !(window.lower || (window.days && kept.newer)) {
            // The service was asked for the bars before the end and has none.
            return Ok(HistoryPage::new(Vec::new(), true));
        }
        // An empty window, such as a weekend. Older bars continue before it, until the run of
        // such windows is too long to have anything behind it.
        let (windows, began) = run.map_or((1, window.end), |run| (run.windows + 1, run.began));
        let reach = began.signed_duration_since(window.start);
        if windows >= LONGEST_RUN || reach > TimeDelta::days(FARTHEST_RUN_DAYS) {
            state.runs.remove(&ask.run());
            return Ok(HistoryPage::new(Vec::new(), true));
        }
        let next = window.start;
        let run = Run {
            next,
            windows,
            began,
        };
        state.runs.insert(ask.run(), run);
        Ok(HistoryPage::default().with_continue_before(next))
    }

    /// The bars of the response `body` that `ask` asks for.
    fn kept(&self, history: &History, ask: &Ask, url: &str, body: &str) -> Result<Kept, Failure> {
        let response: Value = serde_json::from_str(body)
            .map_err(|error| Failure::Unreadable(format!("not JSON: {error}")))?;
        let zone = &self.profile.time_zone;
        let reading = read(
            &response,
            history,
            zone,
            &ask.clock,
            ask.timeframe,
            Utc::now(),
        )
        .map_err(Failure::Unreadable)?;
        if reading.skipped > 0 {
            log::warn!(
                "own data: {} rows of {} were left out: their time is missing, unreadable or \
                 too far ahead",
                reading.skipped,
                endpoint(url)
            );
        }
        let slot = |bar: &Bar| ask.clock.bar_open(bar.time);
        let floor = ask.floor();
        let below = |bar: &Bar| floor.is_some_and(|floor| slot(bar) < floor);
        let above = |bar: &Bar| ask.before.is_some_and(|before| slot(bar) >= before);
        let mut bars = reading.bars;
        let older = bars.iter().any(below);
        let newer = bars.iter().any(above);
        bars.retain(|bar| !below(bar) && !above(bar));
        let more = bars.len().saturating_sub(ask.count);
        bars.drain(..more);
        Ok(Kept { bars, older, newer })
    }

    /// What a session of subscription `id` reports.
    fn on_event(self: &Arc<Self>, id: SubscriptionId, event: StreamEvent) {
        match event {
            StreamEvent::Open { again } => {
                let known = {
                    let mut state = self.state();
                    let Some(live) = state.subscriptions.get_mut(&id) else {
                        return;
                    };
                    live.opens += 1;
                    // The newest bar the subscription knows: the one being built, or the
                    // newest one a bar route pushed.
                    let building = live.aggregator.bar();
                    let building = building.map(|bar| live.key.clock.bar_open(bar.time));
                    let newest = building.max(live.pushed);
                    (live.sink.clone(), live.key.clone(), live.opens, newest)
                };
                let (sink, key, opens, newest) = known;
                sink.set_status(FeedStatus::Live);
                // Records may have been missed since the connection dropped. Without a bar to
                // start from, a latest page that is still in flight brings them.
                if again && let Some(since) = newest {
                    self.fill(id, key, opens, since);
                }
            }
            StreamEvent::Message { records, .. } => {
                let now = Utc::now();
                let mut pushes = Vec::new();
                let mut unread = None;
                let sink = {
                    let mut state = self.state();
                    let Some(live) = state.subscriptions.get(&id) else {
                        return;
                    };
                    let folding = live.filling.is_none() && !state.fetching(&live.key);
                    let Some(live) = state.subscriptions.get_mut(&id) else {
                        return;
                    };
                    let Key {
                        timeframe, clock, ..
                    } = &live.key;
                    let zone = &self.profile.time_zone;
                    let routes = self.profile.stream.as_ref().map(|stream| &stream.routes);
                    for routed in records {
                        let Routed::Record { route, record } = routed else {
                            continue;
                        };
                        let Some(route) = routes.and_then(|routes| routes.get(route)) else {
                            continue;
                        };
                        match &route.kind {
                            RouteKind::Bars(fields) => {
                                match bars::bar(&record, fields, zone, clock, *timeframe) {
                                    Some(bar) => {
                                        let slot = clock.bar_open(bar.time);
                                        // A time too far ahead is refused by the chart and
                                        // is not the newest bar known.
                                        if !bars::ahead(clock, *timeframe, slot, now) {
                                            live.pushed = live.pushed.max(Some(slot));
                                        }
                                        pushes.push(bar);
                                    }
                                    None => unread = unread.or(Some((&route.value, "time"))),
                                }
                            }
                            RouteKind::Trades(fields) => {
                                match bars::read_trade(&record, fields, zone) {
                                    Ok(trade) if folding => {
                                        pushes.extend(live.aggregator.add(&trade, now));
                                    }
                                    Ok(_) => {}
                                    Err(part) => unread = unread.or(Some((&route.value, part))),
                                }
                            }
                            RouteKind::Ignore => {}
                        }
                    }
                    if let Some((route, part)) = unread.filter(|_| !live.warned) {
                        live.warned = true;
                        // Never the record itself, which may hold a key.
                        log::warn!(
                            "own data: a record of the route \"{route}\" of {} has no readable \
                             {part}; such records are dropped",
                            live.endpoint
                        );
                    }
                    live.sink.clone()
                };
                for bar in pushes {
                    sink.push_bar(bar);
                }
            }
            StreamEvent::Unreadable { reason } => {
                let mut state = self.state();
                if let Some(live) = state.subscriptions.get_mut(&id)
                    && !live.warned
                {
                    live.warned = true;
                    log::warn!(
                        "own data: {} sent {reason}; such messages are dropped",
                        live.endpoint
                    );
                }
            }
            StreamEvent::Closed { reason, .. } => {
                let known = {
                    let state = self.state();
                    let live = state.subscriptions.get(&id);
                    live.map(|live| (live.sink.clone(), live.endpoint.clone()))
                };
                if let Some((sink, endpoint)) = known {
                    sink.set_status(FeedStatus::Disconnected);
                    let text = conceal(&format!("{endpoint}: {reason}"), &self.secrets);
                    sink.report_error(FeedError::Network(text));
                }
            }
        }
    }

    /// Loads the bars from the slot `since` on, which a subscription may have missed while its
    /// socket was down, and pushes them. `opens` is the open count of the reconnect: the answer
    /// counts only while no later reconnect has started a fill of its own.
    fn fill(self: &Arc<Self>, id: SubscriptionId, key: Key, opens: u64, since: DateTime<Utc>) {
        let (Some(history), Some(interval)) =
            (&self.profile.history, service(&self.profile, key.timeframe))
        else {
            return;
        };
        let now = Utc::now();
        let slots = now.signed_duration_since(since).num_milliseconds()
            / key.timeframe.duration_ms().max(1);
        let count = usize::try_from(slots).unwrap_or(0).saturating_add(1);
        let count = count.min(history.page_size.max(1));
        let ask = Ask {
            symbol: key.symbol,
            timeframe: key.timeframe,
            clock: key.clock,
            before: None,
            since: Some(since),
            count,
        };
        let Some(window) = window(history, &self.profile.time_zone, &ask, count, now) else {
            return;
        };
        let call = request_for(history, &window, &ask.symbol, interval, &self.secrets);
        let url = conceal(&call.url, &self.secrets);
        // From here on the subscription folds no trade, until the answer has been pushed.
        match self.state().subscriptions.get_mut(&id) {
            Some(live) if live.opens == opens => live.filling = Some(opens),
            _ => return,
        }
        let weak = Arc::downgrade(self);
        http::fetch(call, move |result| {
            let Some(shared) = weak.upgrade() else {
                return;
            };
            let Some(history) = &shared.profile.history else {
                return;
            };
            let kept = result.and_then(|body| shared.kept(history, &ask, &url, &body));
            // An answer for an older open is discarded: a newer fill is in flight.
            let filling = shared
                .state()
                .filling(id, opens)
                .map(|live| live.sink.clone());
            let Some(sink) = filling else {
                return;
            };
            // The bars go out while the subscription still folds no trade, so that no trade
            // can be pushed before the bar it continues.
            let newest = match kept {
                Ok(kept) => {
                    let newest = kept.bars.last().cloned();
                    for bar in kept.bars {
                        sink.push_bar(bar);
                    }
                    newest
                }
                Err(failure) => {
                    sink.report_error(reported(&failure, &url, &ask.symbol, &shared.secrets));
                    None
                }
            };
            if let Some(live) = shared.state().filling(id, opens) {
                live.filling = None;
                // Without a bar, the subscription folds again on the bar it has.
                if newest.is_some() {
                    live.aggregator.start(newest);
                }
            }
        });
    }
}

impl State {
    /// What is known of the latest pages of `key`.
    fn join(&mut self, key: &Key) -> &mut Join {
        let held = self.joins.iter().position(|(held, _)| held == key);
        let at = held.unwrap_or_else(|| {
            self.joins.push((key.clone(), Join::default()));
            self.joins.len() - 1
        });
        &mut self.joins[at].1
    }

    /// Whether a latest page of `key` is being fetched.
    fn fetching(&self, key: &Key) -> bool {
        let mut joins = self.joins.iter();
        joins.any(|(held, join)| held == key && join.in_flight > 0)
    }

    /// The subscription `id`, while the gap fill of its open `opens` is the one in flight.
    fn filling(&mut self, id: SubscriptionId, opens: u64) -> Option<&mut Live> {
        let live = self.subscriptions.get_mut(&id)?;
        (live.filling == Some(opens)).then_some(live)
    }
}

impl Ask {
    fn key(&self) -> Key {
        Key {
            symbol: self.symbol.clone(),
            timeframe: self.timeframe,
            clock: self.clock.clone(),
        }
    }

    /// What the runs of empty windows are counted by.
    fn run(&self) -> (String, Timeframe) {
        (self.symbol.clone(), self.timeframe)
    }

    /// The open of the slot that holds `since`. That bar is the one the chart is waiting for,
    /// so it is answered although it opens before `since`.
    fn floor(&self) -> Option<DateTime<Utc>> {
        self.since.map(|since| self.clock.bar_open(since))
    }
}

/// What `{interval}` becomes for `timeframe`, when the table has a row for it and the interval
/// has a duration.
fn service(profile: &Profile, timeframe: Timeframe) -> Option<&str> {
    profile
        .service_interval(timeframe)
        .filter(|_| !timeframe.is_trade_based())
}

/// Whether a template of `history` names `placeholder`: its URL, a header or its body.
fn names(history: &History, placeholder: &str) -> bool {
    let mut headers = history.headers.iter();
    history.url.contains(placeholder)
        || history.body.contains(placeholder)
        || headers.any(|(_, value)| value.contains(placeholder))
}

/// The window of `wanted` slots before the end `ask` names, or before `now` for the newest
/// bars. `None` when it leaves the range of time: there is nothing to ask for.
fn window(
    history: &History,
    zone: &TimezoneId,
    ask: &Ask,
    wanted: usize,
    now: DateTime<Utc>,
) -> Option<Window> {
    let (clock, timeframe) = (&ask.clock, ask.timeframe);
    let end = ask.before.unwrap_or(now);
    let floor = ask.floor();
    let limit = wanted.max(1);
    let slots = i64::try_from(limit).ok()?;
    let lower = names(history, "{from}");
    let dates = matches!(history.range, RangeFormat::Date | RangeFormat::CompactDate);
    let print = |instant| time::print(instant, &history.range, zone);
    let print_day = |day| time::print_day(day, &history.range);
    let midnight = |day: NaiveDate| day.and_time(NaiveTime::MIN).and_utc();
    let build = |from, to, limit, start, days| Window {
        from,
        to,
        limit,
        end,
        start,
        lower,
        days,
    };
    if bars::calendar(timeframe) {
        // A bar of a day or longer is dated, so its window is counted in dates. An end that
        // opens a period is the open of the chart's oldest bar or a continuation named here.
        let last = if clock.bar_open(end) == end {
            bars::period_date(end).pred_opt()?
        } else {
            end.with_timezone(zone).date_naive()
        };
        let newest = bars::period_date(bars::period_open(clock, last));
        let back = u32::try_from(slots - 1).ok()?;
        let first = match timeframe {
            Timeframe::Month1 => newest.checked_sub_months(Months::new(back))?,
            Timeframe::Months(count) => {
                newest.checked_sub_months(Months::new(count.get().checked_mul(back)?))?
            }
            _ => {
                let days = (timeframe.duration_ms() / DAY_MS).checked_mul(back.into())?;
                newest.checked_sub_signed(TimeDelta::try_days(days)?)?
            }
        };
        let first = bars::period_date(bars::period_open(clock, first));
        let first = floor.map_or(first, |floor| first.max(bars::period_date(floor)));
        let start = bars::period_open(clock, first);
        let (from, to) = if dates {
            (print_day(first), print_day(last))
        } else {
            // An instant form names 00:00 UTC on a date, and its end is not part of the range.
            (print(midnight(first)), print(midnight(last.succ_opt()?)))
        };
        return Some(build(from, to, limit, start, false));
    }
    if dates {
        // A date cannot name part of a day, so the window is whole days of the profile's zone,
        // the last one included. One day can hold more rows than the slots wanted.
        let last = end.checked_sub_signed(TimeDelta::milliseconds(1))?;
        let last = last.with_timezone(zone).date_naive();
        let days = if lower {
            (slots.checked_mul(timeframe.duration_ms())? / DAY_MS).max(1)
        } else {
            1
        };
        let first = last.checked_sub_signed(TimeDelta::try_days(days - 1)?)?;
        let first = floor.map_or(first, |floor| {
            first.max(floor.with_timezone(zone).date_naive())
        });
        let start = time::wall(first.and_time(NaiveTime::MIN), zone)?;
        let limit = history.page_size.max(1);
        let (from, to) = (print_day(first), print_day(last));
        return Some(build(from, to, limit, start, true));
    }
    let span = TimeDelta::try_milliseconds(slots.checked_mul(timeframe.duration_ms())?)?;
    let start = end.checked_sub_signed(span)?;
    let start = floor.map_or(start, |floor| start.max(floor));
    // `{from}` is printed rounded down and `{to}` rounded up to the unit of the form, so the
    // printed window loses no row. What the rounding adds is cut from the answer.
    let unit = match history.range {
        RangeFormat::Seconds => 1_000_000_000,
        RangeFormat::Milliseconds => 1_000_000,
        RangeFormat::Microseconds => 1_000,
        _ => 1,
    };
    let over = i64::from(end.timestamp_subsec_nanos()) % unit;
    let up = TimeDelta::nanoseconds((unit - over) % unit);
    let to = end.checked_add_signed(up).unwrap_or(end);
    Some(build(print(start), print(to), limit, start, false))
}

/// The request for `window` of `symbol`, every template of `history` filled.
fn request_for(
    history: &History,
    window: &Window,
    symbol: &str,
    interval: &str,
    secrets: &Secrets,
) -> Request {
    let limit = window.limit.to_string();
    let mut values = vec![
        ("symbol", symbol),
        ("interval", interval),
        ("from", window.from.as_str()),
        ("to", window.to.as_str()),
        ("limit", limit.as_str()),
    ];
    let keys = secrets.iter();
    values.extend(keys.map(|(name, value)| (name.as_str(), value.as_str())));
    let headers = history.headers.iter();
    let mut headers: Vec<(String, String)> = headers
        .map(|(name, value)| (name.clone(), fill(value, &values)))
        .collect();
    // An empty body template sends `GET`.
    let body = (!history.body.is_empty()).then(|| fill(&history.body, &values));
    let typed = |(name, _): &(String, String)| name.eq_ignore_ascii_case("content-type");
    if body.is_some() && !headers.iter().any(typed) {
        headers.push(("Content-Type".to_owned(), "application/json".to_owned()));
    }
    Request {
        url: fill_url(&history.url, &values),
        headers,
        body,
    }
}

/// The error the chart reports for `failure` of a request to `url`: the kind `feed_error`
/// picks, with the endpoint before the reason where the kind carries a text. No key value of
/// `secrets` is shown.
fn reported(failure: &Failure, url: &str, symbol: &str, secrets: &Secrets) -> FeedError {
    let text = conceal(&format!("{}: {failure}", endpoint(url)), secrets);
    match failure.feed_error(symbol) {
        FeedError::Network(_) => FeedError::Network(text),
        FeedError::InvalidData(_) => FeedError::InvalidData(text),
        other => other,
    }
}

/// What a history response holds when it is read with a mapping.
pub(crate) struct Reading {
    /// The bars with a readable time, oldest first. Bars of equal time keep the order of the
    /// response.
    pub(crate) bars: Vec<Bar>,
    /// Rows left out because their time is missing or unreadable, or lies too far after `now`,
    /// as [`bars::ahead`] judges a pushed bar.
    pub(crate) skipped: usize,
}

/// Reads a parsed history response. The feed and the dialog's Test both call it, so the preview
/// cannot disagree with the chart. The error says why nothing can be read: the rows are not
/// where the mapping says, or no row has a readable time.
pub(crate) fn read(
    response: &Value,
    history: &History,
    zone: &TimezoneId,
    clock: &BarClock,
    timeframe: Timeframe,
    now: DateTime<Utc>,
) -> Result<Reading, String> {
    let rows = shape::rows(response, &history.rows)?;
    let read = rows
        .iter()
        .filter_map(|row| bars::bar(row, &history.bar, zone, clock, timeframe));
    let mut read: Vec<Bar> = read.collect();
    if read.is_empty() && !rows.is_empty() {
        // A wrong time mapping must not look like a window without bars.
        let count = rows.len();
        let rows = if count == 1 { "row" } else { "rows" };
        return Err(format!("no row has a readable time ({count} {rows})"));
    }
    // A time in the wrong unit lands far ahead, where it would be the newest bar.
    read.retain(|bar| !bars::ahead(clock, timeframe, clock.bar_open(bar.time), now));
    // A stable sort: rows of one time stay in the order the service sent them, so the chart,
    // which keeps the last bar of a slot, takes a correction sent after the row it corrects.
    read.sort_by_key(|bar| bar.time);
    let skipped = rows.len() - read.len();
    Ok(Reading {
        bars: read,
        skipped,
    })
}

/// The request for the newest page of `symbol` at `timeframe`, exactly as the feed sends it for
/// a chart's first load. The dialog's Test sends this request. The error says why none can be
/// built: the profile has no history, or its interval table has no row for `timeframe`.
pub(crate) fn latest(
    profile: &Profile,
    secrets: &Secrets,
    symbol: &str,
    timeframe: Timeframe,
    clock: &BarClock,
    now: DateTime<Utc>,
) -> Result<Request, String> {
    let Some(history) = &profile.history else {
        return Err("the profile has no REST history".to_owned());
    };
    let Some(interval) = service(profile, timeframe) else {
        return Err(format!("the interval table has no row for {timeframe}"));
    };
    let ask = Ask {
        symbol: symbol.to_owned(),
        timeframe,
        clock: clock.clone(),
        before: None,
        since: None,
        count: history.page_size,
    };
    let Some(window) = window(history, &profile.time_zone, &ask, ask.count, now) else {
        return Err("the newest page lies outside the range of time".to_owned());
    };
    Ok(request_for(history, &window, symbol, interval, secrets))
}

#[cfg(test)]
pub(crate) mod tests {
    use std::io::{BufRead as _, BufReader, Read as _, Write as _};
    use std::net::TcpListener;
    use std::time::{Duration, Instant};

    use eframe::egui;
    use egui_charts::config::{ChartOptions, TimezoneMode};
    use egui_charts::widget::{Chart, ChartBuilder, SeriesId};

    use super::super::profile::{
        BarFields, DateFormat, Interval, Layout, Rows, Segment, TimeSpec, TimeUnit,
    };
    use super::*;

    /// One request the local server received.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub(crate) struct Seen {
        /// Method and target, such as `GET /bars?symbol=ABC&interval=1m&to=1794407580&limit=3`.
        pub(crate) line: String,
        /// The request headers, names in lower case, in the order sent.
        pub(crate) headers: Vec<(String, String)>,
        /// The request body, empty for `GET`.
        pub(crate) body: String,
    }

    /// A scripted HTTP server on a local port. It answers what `answer` returns for each
    /// request and records every request. It never filters, sorts or pages: a test writes each
    /// response by hand, so no expected value comes from logic that mirrors the code under
    /// test.
    pub(crate) struct Server {
        /// `http://127.0.0.1:PORT`, without a trailing slash.
        pub(crate) origin: String,
        seen: Arc<Mutex<Vec<Seen>>>,
    }

    impl Server {
        /// `answer` gets the number of the request, counted from zero, and the request, and
        /// returns the status and the body.
        pub(crate) fn start(
            answer: impl Fn(usize, &Seen) -> (u16, String) + Send + 'static,
        ) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let origin = format!("http://{}", listener.local_addr().unwrap());
            let seen = Arc::new(Mutex::new(Vec::new()));
            let log = seen.clone();
            std::thread::spawn(move || {
                for connection in listener.incoming() {
                    let Ok(mut connection) = connection else {
                        return;
                    };
                    let mut reader = BufReader::new(connection.try_clone().unwrap());
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    let line = line.trim_end().trim_end_matches(" HTTP/1.1").to_owned();
                    let mut headers = Vec::new();
                    loop {
                        let mut header = String::new();
                        reader.read_line(&mut header).unwrap();
                        let Some((name, value)) = header.trim_end().split_once(':') else {
                            break;
                        };
                        headers.push((name.to_ascii_lowercase(), value.trim().to_owned()));
                    }
                    let length = headers
                        .iter()
                        .find(|(name, _)| name == "content-length")
                        .map_or(0, |(_, value)| value.parse().unwrap());
                    let mut body = vec![0; length];
                    reader.read_exact(&mut body).unwrap();
                    let request = Seen {
                        line,
                        headers,
                        body: String::from_utf8(body).unwrap(),
                    };
                    let number = log.lock().unwrap().len();
                    let (status, text) = answer(number, &request);
                    log.lock().unwrap().push(request);
                    let _ = write!(
                        connection,
                        "HTTP/1.1 {status} Scripted\r\nContent-Type: application/json\r\n\
                         Content-Length: {}\r\nConnection: close\r\n\r\n{text}",
                        text.len()
                    );
                }
            });
            Self { origin, seen }
        }

        /// Every request received so far, in order.
        pub(crate) fn seen(&self) -> Vec<Seen> {
            self.seen.lock().unwrap().clone()
        }
    }

    /// The answer of a script: request `n` gets `pages[n]` with status 200. A request beyond
    /// the script gets status 400, which ends the chart's paging with an error, so a test that
    /// asks one request too many stops and shows the line it asked.
    pub(crate) fn pages(
        pages: Vec<String>,
    ) -> impl Fn(usize, &Seen) -> (u16, String) + Send + 'static {
        move |number, _| {
            let page = pages.get(number);
            page.map_or((400, String::new()), |page| (200, page.clone()))
        }
    }

    /// Runs frames of `chart` until `done` holds. Panics after thirty seconds, naming `what`.
    pub(crate) fn until(
        ctx: &egui::Context,
        chart: &mut Chart,
        what: &str,
        done: impl Fn(&Chart) -> bool,
    ) {
        let start = Instant::now();
        loop {
            let size = egui::vec2(800.0, 600.0);
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    ..Default::default()
                },
                |ui| {
                    chart.show(ui);
                },
            );
            output.drop_without_applying_deltas();
            if done(chart) {
                return;
            }
            assert!(
                start.elapsed() < Duration::from_secs(30),
                "timed out waiting for {what}"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// What a test compares of a bar: its time in Unix seconds and its open, high, low, close
    /// and volume, or `None` for a whitespace bar (whose values are NaN and equal nothing).
    pub(crate) type Shown = (i64, Option<[f64; 5]>);

    /// The bars of the chart's main series, oldest first.
    pub(crate) fn shown(chart: &Chart) -> Vec<Shown> {
        let bars = chart.bars(SeriesId::MAIN).iter();
        bars.map(|bar| {
            let values = [bar.open, bar.high, bar.low, bar.close, bar.volume];
            (
                bar.time.timestamp(),
                (!bar.is_whitespace()).then_some(values),
            )
        })
        .collect()
    }

    /// The base profile: objects at the root of the response, `{to}` and `{limit}` in the
    /// address, instants in seconds, three bars per request.
    pub(crate) fn profile(origin: &str) -> Profile {
        let key = |name: &str| vec![Segment::Key(name.to_owned())];
        let interval = |chart, service: &str| Interval {
            chart,
            service: service.to_owned(),
        };
        Profile {
            name: "Local".into(),
            symbols: vec!["ABC".into()],
            intervals: vec![
                interval(Timeframe::Min1, "1m"),
                interval(Timeframe::Day1, "1d"),
            ],
            history: Some(History {
                url: format!(
                    "{origin}/bars?symbol={{symbol}}&interval={{interval}}&to={{to}}&limit={{limit}}"
                ),
                page_size: 3,
                bar: BarFields {
                    time: TimeSpec::Instant {
                        field: key("t"),
                        unit: TimeUnit::Seconds,
                    },
                    open: key("o"),
                    high: key("h"),
                    low: key("l"),
                    close: key("c"),
                    volume: key("v"),
                },
                ..History::default()
            }),
            ..Profile::default()
        }
    }

    /// A request as the table writes it: the line, and for a request with a body also its
    /// content type and the body.
    fn asked(seen: &Seen) -> String {
        if seen.body.is_empty() {
            return seen.line.clone();
        }
        let kind = seen.headers.iter().find(|(name, _)| name == "content-type");
        let kind = kind.map_or("", |(_, value)| value);
        format!("{} {kind} {}", seen.line, seen.body)
    }

    struct Case {
        name: &'static str,
        /// What the row changes in the base profile.
        edit: fn(&mut Profile),
        /// What it changes in the chart.
        chart: fn(ChartBuilder) -> ChartBuilder,
        /// The answers, in the order of the requests.
        answers: Vec<String>,
        /// How the first request starts. Its bounds come from the system clock.
        first: &'static str,
        /// Every later request, in order.
        asked: Vec<String>,
        /// The chart's bars once history has ended.
        bars: Vec<Shown>,
        /// How many bars the chart refused.
        rejected: u64,
    }

    /// The fixture is one-minute bars of 11 November 2026 from 14:30 UTC (second 1,794,407,400,
    /// then every 60 seconds) and three of 10 November from 14:30 UTC (second 1,794,321,000).
    /// Every response is written out as a service sends it, and every request line after the
    /// first of a chart, whose bounds come from the system clock, is calculated by hand.
    #[test]
    fn history_maps_every_shape_and_pages_back() {
        // The fixture as a service sends it.
        const J0: &str = r#"{"t":1794407400,"o":100.0,"h":101.0,"l":99.5,"c":100.5,"v":1000}"#;
        const J1: &str = r#"{"t":1794407460,"o":100.5,"h":102.0,"l":100.0,"c":101.5,"v":1200}"#;
        const J2: &str = r#"{"t":1794407520,"o":101.5,"h":101.75,"l":100.25,"c":100.75,"v":900}"#;
        const J3: &str = r#"{"t":1794407580,"o":100.75,"h":103.0,"l":100.5,"c":102.5,"v":1500}"#;
        const J4: &str = r#"{"t":1794407640,"o":102.5,"h":102.75,"l":101.0,"c":101.25,"v":800}"#;
        const J5: &str = r#"{"t":1794407700,"o":101.25,"h":101.5,"l":100.0,"c":100.25,"v":700}"#;
        const K0: &str = r#"{"t":1794321000,"o":99.0,"h":99.5,"l":98.5,"c":99.25,"v":400}"#;
        const K1: &str = r#"{"t":1794321060,"o":99.25,"h":100.0,"l":99.0,"c":99.75,"v":500}"#;
        const K2: &str = r#"{"t":1794321120,"o":99.75,"h":100.25,"l":99.5,"c":100.0,"v":600}"#;
        // The same bars as the chart shows them.
        const B0: Shown = (1794407400, Some([100.0, 101.0, 99.5, 100.5, 1000.0]));
        const B1: Shown = (1794407460, Some([100.5, 102.0, 100.0, 101.5, 1200.0]));
        const B2: Shown = (1794407520, Some([101.5, 101.75, 100.25, 100.75, 900.0]));
        const B3: Shown = (1794407580, Some([100.75, 103.0, 100.5, 102.5, 1500.0]));
        const B4: Shown = (1794407640, Some([102.5, 102.75, 101.0, 101.25, 800.0]));
        const B5: Shown = (1794407700, Some([101.25, 101.5, 100.0, 100.25, 700.0]));
        const A0: Shown = (1794321000, Some([99.0, 99.5, 98.5, 99.25, 400.0]));
        const A1: Shown = (1794321060, Some([99.25, 100.0, 99.0, 99.75, 500.0]));
        const A2: Shown = (1794321120, Some([99.75, 100.25, 99.5, 100.0, 600.0]));
        // The requests most rows expect: the first, then the pages before B3 and before B0.
        const FIRST: &str = "GET /bars?symbol=ABC&interval=1m&to=";
        const BEFORE_B3: &str = "GET /bars?symbol=ABC&interval=1m&to=1794407580&limit=3";
        const BEFORE_B0: &str = "GET /bars?symbol=ABC&interval=1m&to=1794407400&limit=3";

        let list = |rows: &[&str]| format!("[{}]", rows.join(","));
        let case = |name, edit, answers: Vec<String>, first, asked: Vec<&str>, bars| Case {
            name,
            edit,
            chart: |chart| chart,
            answers,
            first,
            asked: asked.into_iter().map(str::to_owned).collect(),
            bars,
            rejected: 0,
        };
        let two_pages = || vec![list(&[J3, J4, J5]), list(&[J0, J1, J2]), "[]".to_owned()];
        let all = || vec![B0, B1, B2, B3, B4, B5];

        #[rustfmt::skip]
        let cases = [
            case("objects at the root", |_| {}, two_pages(), FIRST, vec![BEFORE_B3, BEFORE_B0], all()),
            case(
                "arrays read by column number",
                |profile| {
                    let column = |number| vec![Segment::Index(number)];
                    let history = profile.history.as_mut().unwrap();
                    history.rows.layout = Layout::Arrays { header: None };
                    history.bar = BarFields {
                        time: TimeSpec::Instant { field: column(0), unit: TimeUnit::Seconds },
                        open: column(1),
                        high: column(2),
                        low: column(3),
                        close: column(4),
                        volume: column(5),
                    };
                },
                vec![
                    "[[1794407580,100.75,103.0,100.5,102.5,1500],[1794407640,102.5,102.75,101.0,101.25,800],\
                     [1794407700,101.25,101.5,100.0,100.25,700]]".into(),
                    "[[1794407400,100.0,101.0,99.5,100.5,1000],[1794407460,100.5,102.0,100.0,101.5,1200],\
                     [1794407520,101.5,101.75,100.25,100.75,900]]".into(),
                    "[]".into(),
                ],
                FIRST,
                vec![BEFORE_B3, BEFORE_B0],
                all(),
            ),
            case(
                "arrays named by a header in another order; the empty page comes without its list",
                |profile| {
                    let key = |name: &str| vec![Segment::Key(name.to_owned())];
                    profile.history.as_mut().unwrap().rows =
                        Rows { path: key("rows"), layout: Layout::Arrays { header: Some(key("columns")) } };
                },
                vec![
                    r#"{"columns":["t","v","o","c","h","l"],"rows":[[1794407580,1500,100.75,102.5,103.0,100.5],[1794407640,800,102.5,101.25,102.75,101.0],[1794407700,700,101.25,100.25,101.5,100.0]]}"#.into(),
                    r#"{"columns":["t","v","o","c","h","l"],"rows":[[1794407400,1000,100.0,100.5,101.0,99.5],[1794407460,1200,100.5,101.5,102.0,100.0],[1794407520,900,101.5,100.75,101.75,100.25]]}"#.into(),
                    r#"{"columns":["t","v","o","c","h","l"]}"#.into(),
                ],
                FIRST,
                vec![BEFORE_B3, BEFORE_B0],
                all(),
            ),
            case(
                "columns, beside a member that is not a list",
                |profile| profile.history.as_mut().unwrap().rows.layout = Layout::Columns,
                vec![
                    r#"{"s":"ok","t":[1794407580,1794407640,1794407700],"o":[100.75,102.5,101.25],"h":[103.0,102.75,101.5],"l":[100.5,101.0,100.0],"c":[102.5,101.25,100.25],"v":[1500,800,700]}"#.into(),
                    r#"{"s":"ok","t":[1794407400,1794407460,1794407520],"o":[100.0,100.5,101.5],"h":[101.0,102.0,101.75],"l":[99.5,100.0,100.25],"c":[100.5,101.5,100.75],"v":[1000,1200,900]}"#.into(),
                    r#"{"s":"no_data","t":[],"o":[],"h":[],"l":[],"c":[],"v":[]}"#.into(),
                ],
                FIRST,
                vec![BEFORE_B3, BEFORE_B0],
                all(),
            ),
            case(
                "rows below data.candles; the empty page is null",
                |profile| {
                    profile.history.as_mut().unwrap().rows.path =
                        vec![Segment::Key("data".into()), Segment::Key("candles".into())];
                },
                vec![
                    format!(r#"{{"data":{{"candles":{}}}}}"#, list(&[J3, J4, J5])),
                    format!(r#"{{"data":{{"candles":{}}}}}"#, list(&[J0, J1, J2])),
                    r#"{"data":{"candles":null}}"#.into(),
                ],
                FIRST,
                vec![BEFORE_B3, BEFORE_B0],
                all(),
            ),
            // Five rows for a limit of three: the newest three are B3 to B5 once sorted.
            case(
                "rows out of order, and more of them than asked",
                |_| {},
                vec![list(&[J2, J5, J1, J4, J3]), list(&[J1, J0, J2]), "[]".into()],
                FIRST,
                vec![BEFORE_B3, BEFORE_B0],
                all(),
            ),
            case(
                "a slot sent twice: the later row is the correction",
                |_| {},
                vec![
                    list(&[J4, r#"{"t":1794407700,"o":101.25,"h":101.25,"l":101.25,"c":101.25,"v":1}"#, J5]),
                    "[]".into(),
                ],
                FIRST,
                vec!["GET /bars?symbol=ABC&interval=1m&to=1794407640&limit=3"],
                vec![B4, B5],
            ),
            case(
                "partial pages do not end history",
                |_| {},
                vec![list(&[J4, J5]), list(&[J1, J2, J3]), list(&[J0]), "[]".into()],
                FIRST,
                vec![
                    "GET /bars?symbol=ABC&interval=1m&to=1794407640&limit=3",
                    "GET /bars?symbol=ABC&interval=1m&to=1794407460&limit=3",
                    BEFORE_B0,
                ],
                all(),
            ),
            // Windows of three minutes. The one before B5 is empty and is crossed; the next
            // holds B0 and B1; then 64 empty windows back from B0 end history. The last of them
            // is 1794407400 - 63 * 180 = 1794396060 down to 1794395880. 67 requests in all.
            Case {
                asked: [
                    vec![
                        "GET /bars?symbol=ABC&interval=1m&from=1794407520&to=1794407700&limit=3".to_owned(),
                        "GET /bars?symbol=ABC&interval=1m&from=1794407340&to=1794407520&limit=3".to_owned(),
                    ],
                    (0..64)
                        .map(|window| {
                            let (from, to) = (1794407220 - window * 180, 1794407400 - window * 180);
                            format!("GET /bars?symbol=ABC&interval=1m&from={from}&to={to}&limit=3")
                        })
                        .collect(),
                ]
                .concat(),
                ..case(
                    "an empty window is crossed, and a run of 64 ends history",
                    |profile| {
                        let history = profile.history.as_mut().unwrap();
                        history.url = history.url.replace("&to=", "&from={from}&to=");
                    },
                    [vec![list(&[J5]), "[]".into(), list(&[J0, J1])], vec!["[]".to_owned(); 64]].concat(),
                    "GET /bars?symbol=ABC&interval=1m&from=",
                    vec![],
                    vec![B0, B1, B5],
                )
            },
            case(
                "a template without a bound is asked once",
                |profile| {
                    let history = profile.history.as_mut().unwrap();
                    history.url = history.url.replace("&to={to}&limit={limit}", "");
                },
                vec![list(&[J3, J4, J5])],
                "GET /bars?symbol=ABC&interval=1m",
                vec![],
                vec![B3, B4, B5],
            ),
            // A date names a whole day, itself included. The service fills its limit from the
            // end of the day, so the page before B3 holds B3 to B5 again, and the feed moves on
            // to the day before. The same happens once more on 10 November, and 9 November is
            // empty, which ends history for a template without `{from}`.
            case(
                "dates as bounds on minute bars",
                |profile| profile.history.as_mut().unwrap().range = RangeFormat::Date,
                vec![
                    list(&[J3, J4, J5]),
                    list(&[J3, J4, J5]),
                    list(&[K0, K1, K2]),
                    list(&[K0, K1, K2]),
                    "[]".into(),
                ],
                FIRST,
                vec![
                    "GET /bars?symbol=ABC&interval=1m&to=2026-11-11&limit=3",
                    "GET /bars?symbol=ABC&interval=1m&to=2026-11-10&limit=3",
                    "GET /bars?symbol=ABC&interval=1m&to=2026-11-10&limit=3",
                    "GET /bars?symbol=ABC&interval=1m&to=2026-11-09&limit=3",
                ],
                vec![A0, A1, A2, B3, B4, B5],
            ),
            case(
                "a body is sent with POST as JSON",
                |profile| {
                    let history = profile.history.as_mut().unwrap();
                    history.url = history.url.split('?').next().unwrap().to_owned();
                    history.body = r#"{"symbol":"{symbol}","interval":"{interval}","to":{to},"limit":{limit}}"#.into();
                },
                vec![list(&[J3, J4, J5]), "[]".into()],
                r#"POST /bars application/json {"symbol":"ABC","interval":"1m","to":"#,
                vec![r#"POST /bars application/json {"symbol":"ABC","interval":"1m","to":1794407580,"limit":3}"#],
                vec![B3, B4, B5],
            ),
            case(
                "numbers as text",
                |_| {},
                vec![
                    r#"[{"t":"1794407580","o":"100.75","h":"103.0","l":"100.5","c":"102.5","v":"1500"},{"t":"1794407640","o":" 102.5 ","h":"102.75","l":"101.0","c":"101.25","v":"800"},{"t":"1794407700","o":"101.25","h":"101.5","l":"100.0","c":"100.25","v":"700"}]"#.into(),
                    "[]".into(),
                ],
                FIRST,
                vec![BEFORE_B3],
                vec![B3, B4, B5],
            ),
            // The second page holds one row per rule for a missing or unreadable field, from
            // 14:25 (1794407400 - 300 = 1794407100) to 14:32. A scroll-back asks for 600 bars, so
            // the page is not cut. The two rows without a time are skipped. The chart refuses
            // five bars: no close, an empty open, and a volume that is "NaN", true or negative.
            // Three arrive: a bar without a volume, which is 0, an empty slot and B2.
            Case {
                rejected: 5,
                ..case(
                    "missing and unreadable fields",
                    |_| {},
                    vec![
                        list(&[J3, J4, J5]),
                        list(&[
                            r#"{"o":1,"h":1,"l":1,"c":1,"v":1}"#,
                            r#"{"t":null,"o":1,"h":1,"l":1,"c":1,"v":1}"#,
                            r#"{"t":1794407100,"o":100.0,"h":101.0,"l":99.5,"c":100.5}"#,
                            r#"{"t":1794407160,"o":100.0,"h":101.0,"l":99.5,"v":10}"#,
                            r#"{"t":1794407220,"o":"","h":101.0,"l":99.5,"c":100.5,"v":10}"#,
                            r#"{"t":1794407280,"o":null,"h":null,"l":null,"c":null,"v":5}"#,
                            r#"{"t":1794407340,"o":100.0,"h":101.0,"l":99.5,"c":100.5,"v":"NaN"}"#,
                            r#"{"t":1794407400,"o":100.0,"h":101.0,"l":99.5,"c":100.5,"v":true}"#,
                            r#"{"t":1794407460,"o":100.0,"h":101.0,"l":99.5,"c":100.5,"v":-1}"#,
                            J2,
                        ]),
                        "[]".into(),
                    ],
                    FIRST,
                    vec![BEFORE_B3, "GET /bars?symbol=ABC&interval=1m&to=1794407100&limit=3"],
                    vec![(1794407100, Some([100.0, 101.0, 99.5, 100.5, 0.0])), (1794407280, None), B2, B3, B4, B5],
                )
            },
            // Daily rows carry a date. On a chart in New York a day opens at 05:00 UTC in
            // November, and that is the stamp: 00:00 UTC on 11 November is 1794355200, so the
            // dates 5, 6, 9, 10 and 11 November are 1794355200 - n * 86400 + 18000 for n = 6, 5,
            // 2, 1 and 0. The page before 10 November ends on the 9th, and the page before
            // 5 November on the 4th.
            Case {
                chart: |chart| {
                    let mut options = ChartOptions::default();
                    options.time_scale.timezone = TimezoneMode::Timezone("America/New_York".parse().unwrap());
                    chart.with_timeframe(Timeframe::Day1).with_chart_options(options)
                },
                ..case(
                    "daily bars dated by the day, on a chart in another zone",
                    |profile| {
                        let history = profile.history.as_mut().unwrap();
                        history.range = RangeFormat::Date;
                        history.bar.time = TimeSpec::Wall {
                            date: vec![Segment::Key("d".into())],
                            format: DateFormat::Dashed,
                            clock: None,
                        };
                    },
                    vec![
                        r#"[{"d":"2026-11-10","o":100.75,"h":103.0,"l":100.5,"c":102.5,"v":1500},{"d":"2026-11-11","o":102.5,"h":102.75,"l":101.0,"c":101.25,"v":800}]"#.into(),
                        r#"[{"d":"2026-11-05","o":100.0,"h":101.0,"l":99.5,"c":100.5,"v":1000},{"d":"2026-11-06","o":100.5,"h":102.0,"l":100.0,"c":101.5,"v":1200},{"d":"2026-11-09","o":101.5,"h":101.75,"l":100.25,"c":100.75,"v":900}]"#.into(),
                        "[]".into(),
                    ],
                    "GET /bars?symbol=ABC&interval=1d&to=",
                    vec![
                        "GET /bars?symbol=ABC&interval=1d&to=2026-11-09&limit=3",
                        "GET /bars?symbol=ABC&interval=1d&to=2026-11-04&limit=3",
                    ],
                    vec![
                        (1793854800, Some([100.0, 101.0, 99.5, 100.5, 1000.0])),
                        (1793941200, Some([100.5, 102.0, 100.0, 101.5, 1200.0])),
                        (1794200400, Some([101.5, 101.75, 100.25, 100.75, 900.0])),
                        (1794286800, Some([100.75, 103.0, 100.5, 102.5, 1500.0])),
                        (1794373200, Some([102.5, 102.75, 101.0, 101.25, 800.0])),
                    ],
                )
            },
        ];

        let ctx = egui::Context::default();
        for case in cases {
            let server = Server::start(pages(case.answers.clone()));
            let mut profile = profile(&server.origin);
            (case.edit)(&mut profile);
            let builder = ChartBuilder::new()
                .with_feed(Arc::new(MappedFeed::new(profile, Secrets::new())))
                .with_symbol("ABC")
                .with_timeframe(Timeframe::Min1)
                .with_initial_bars(3);
            let mut chart = (case.chart)(builder).build();
            until(&ctx, &mut chart, case.name, |chart| {
                let feed = chart.feed_state(SeriesId::MAIN);
                feed.is_some_and(|feed| feed.history_complete || feed.last_error.is_some())
            });
            let feed = chart.feed_state(SeriesId::MAIN).unwrap();
            assert!(
                feed.last_error.is_none(),
                "{}: {:?}",
                case.name,
                feed.last_error
            );
            let requests: Vec<String> = server.seen().iter().map(asked).collect();
            assert!(
                requests[0].starts_with(case.first),
                "{}: {}",
                case.name,
                requests[0]
            );
            assert_eq!(requests[1..], case.asked, "{}", case.name);
            assert_eq!(shown(&chart), case.bars, "{}", case.name);
            assert_eq!(feed.rejected_bars, case.rejected, "{}", case.name);
        }

        // A failed request names its endpoint and its reason on the chart, and never the key.
        // The key travels in the address here; `None` is an address where nothing listens.
        #[rustfmt::skip]
        let failures = [
            (
                "unauthorised",
                Some((401, r#"{"error":"bad key"}"#)),
                "invalid data: {origin}/bars: rejected as unauthorised (HTTP 401): check the key and where it is sent",
            ),
            ("another refusal", Some((400, "")), "invalid data: {origin}/bars: HTTP 400"),
            ("an unknown symbol", Some((404, "")), "symbol not found: ABC"),
            ("too many requests", Some((429, "")), "rate limited"),
            ("a server error", Some((500, "")), "network error: {origin}/bars: HTTP 500"),
            (
                "not JSON",
                Some((200, "<html>busy</html>")),
                "invalid data: {origin}/bars: unreadable response: not JSON: ",
            ),
            (
                "no readable time",
                Some((200, r#"[{"t":"soon"},{"t":"soon"},{"t":"soon"}]"#)),
                "invalid data: {origin}/bars: unreadable response: no row has a readable time (3 rows)",
            ),
            ("nothing listens", None, "network error: {origin}/bars: no response ("),
        ];
        for (name, answer, expected) in failures {
            let server =
                answer.map(|(status, body)| Server::start(move |_, _| (status, body.to_owned())));
            let origin = match &server {
                Some(server) => server.origin.clone(),
                None => {
                    let unused = TcpListener::bind("127.0.0.1:0").unwrap();
                    format!("http://{}", unused.local_addr().unwrap())
                }
            };
            let mut profile = profile(&origin);
            profile.keys = vec!["key".to_owned()];
            profile.history.as_mut().unwrap().url.push_str("&key={key}");
            let secrets = vec![("key".to_owned(), "s3cret-7f3a".to_owned())];
            let mut chart = ChartBuilder::new()
                .with_feed(Arc::new(MappedFeed::new(profile, secrets)))
                .with_symbol("ABC")
                .with_timeframe(Timeframe::Min1)
                .with_initial_bars(3)
                .build();
            until(&ctx, &mut chart, name, |chart| {
                let feed = chart.feed_state(SeriesId::MAIN);
                feed.is_some_and(|feed| feed.last_error.is_some())
            });
            let feed = chart.feed_state(SeriesId::MAIN).unwrap();
            let text = feed.last_error.as_ref().unwrap().to_string();
            assert!(
                text.starts_with(&expected.replace("{origin}", &origin)),
                "{name}: {text}"
            );
            assert!(!text.contains("s3cret-7f3a"), "{name}: {text}");
            if let Some(server) = server {
                // The key did reach the endpoint, so its absence from the text means something.
                assert!(
                    server.seen()[0].line.ends_with("&key=s3cret-7f3a"),
                    "{name}"
                );
            }
        }
    }
}
